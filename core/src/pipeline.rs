use std::{fs, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

use serde_json::json;

use crate::{ffgraph, ffrun};
use crate::errcode::{coded, recode, ENGINE_DISK_FULL, FS_OUTDIR, MEDIA_UNREADABLE};
use crate::preset::Preset;

pub fn run(
    preset_path: &std::path::Path,
    input: &std::path::Path,
    out_dir: &std::path::Path,
    preview: Option<f64>,
    intensity: f64,
    overrides: &[String],
) -> Result<PathBuf, String> {
    let preset = Preset::load_with_overrides(preset_path, overrides)?.scaled(intensity);
    // 非默认强度写进文件名,避免不同强度互相覆盖
    let suffix = if (intensity - 1.0).abs() > 0.001 {
        format!("_{}x", (intensity * 10.0).round() / 10.0)
    } else {
        String::new()
    };
    run_preset_named(&preset, &suffix, input, out_dir, preview)
}

/// 参数指纹:同一素材 + 同一组参数 → 同一个文件名(可命中已有帧);
/// 任何一项变了 → 新文件名。浏览器缓存是按 URL 认的,URL 必须随参数变,
/// 否则用户改了滑杆看到的还是旧帧(实测就是这个坑)。
/// 引擎版本也进指纹:升级后旧的缓存帧一律作废。
///
/// `preset_body` 必须是**解析后的预设内容**而不是 id:实测「换一批」只把种子 11 改成 12、
/// id 不变,而指纹只认 id → 预览直接 `cached:true` 端出旧帧,等于"换了但没换"。
/// `src_stamp` 是素材的字节数 + mtime:同名文件被重新导出(改了内容)时也必须作废缓存。
fn fingerprint(
    preset_id: &str,
    preset_body: &str,
    src_stamp: &str,
    intensity: f64,
    t: f64,
    overrides: &[String],
    cap: u32,
) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a 64
    let mut feed = |s: &str| {
        for b in s.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
    };
    feed(env!("CARGO_PKG_VERSION"));
    // 预览产物格式的版本戳:改了"原帧与成品同几何"这条规则(§8.1-46),旧缓存必须作废,
    // 否则同一参数命中上一轮留下的尺寸不匹配的两张图,现象照旧。
    // pf-geo3:指纹从"只认 id"改成"认预设内容与素材本体"
    feed("|pf-geo3");
    // 预览代理上限进指纹:改了上限(比如从全尺寸降到 1280)必须作废旧缓存,
    // 否则同名命中会把上一档尺寸的两张图继续端出来,看起来"缩放没生效"
    feed(&format!("|pe{cap}"));
    feed(preset_id);
    feed("|b");
    feed(preset_body);
    feed("|s");
    feed(src_stamp);
    feed(&format!("|i{:.3}|t{:.2}", intensity, t));
    let mut ov: Vec<&String> = overrides.iter().collect();
    ov.sort();
    for o in &ov {
        feed("|");
        feed(o);
    }
    format!("{:08x}", (h & 0xffff_ffff) as u32)
}

/// 素材指纹料:字节数 + 修改时间。拿不到就用 "0",宁可少命中缓存也不许 panic。
fn src_stamp(input: &std::path::Path) -> String {
    match fs::metadata(input) {
        Ok(m) => {
            let mt = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0);
            format!("{}-{}", m.len(), mt)
        }
        Err(_) => "0".into(),
    }
}

/// 一次运行一个标签:预览与批量会并发跑同一素材,共享临时文件名的话会互相删掉对方的中间产物
/// 任何"同名临时文件 + rename"的地方都要带上它,否则并发会互相顶掉
pub fn run_tag() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    format!("{:x}-{:x}-{:x}", std::process::id(), ms & 0xffff_ffff, n)
}

/// 占住一个还没人用的成品名:O_EXCL 而不是"先看存不存在",并发跑同一素材时才会各拿各的名字
fn reserve(out_dir: &std::path::Path, base: &str, ext: &str) -> Result<PathBuf, String> {
    let mut dup = 0u32;
    loop {
        let name = if dup == 0 {
            format!("{base}.{ext}")
        } else {
            format!("{base}-{dup}.{ext}")
        };
        let p = out_dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(_) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                dup += 1;
                if dup > 999 {
                    return Err(coded(FS_OUTDIR, "同名输出过多(-999),请清理输出目录").into());
                }
            }
            Err(e) => return Err(coded(FS_OUTDIR, format!("创建输出文件失败: {e}"))),
        }
    }
}

/// 对比预览:同一管线跑短片段时间窗,产出(原帧,做旧帧)对
/// 预览缓存目录的唯一入口:CLI 默认值、serve、桌面壳都走这里,不再各写一遍字面量。
/// (同名不同路径的老毛病见 §8.1-43)
pub fn preview_cache_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("REWIND_PREVIEW_DIR") {
        let s = PathBuf::from(p);
        if s.is_absolute() {
            return s;
        }
    }
    std::env::temp_dir().join("rewind_preview")
}

/// 预览缓存必须有上限:一张原帧 PNG 1.3 MB,而文件名带参数指纹 —— 每改一次滑杆就是新的一批,
/// 旧的永远留着。serve 的 sweep 只管 uploads,这里补上预览目录:先删过期的,再按 mtime 从旧到新
/// 删到不超过上限。桌面壳走的是同一条 CLI 路径,所以两边共用这一份实现。
pub fn prune_preview_cache(dir: &std::path::Path) {
    let max_bytes: u64 = match std::env::var("REWIND_PREVIEW_MAX_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        // 0 或非法值才回落到默认;用户显式写的小数字必须照收 ——
        // 第一版写成 `.max(16)`,把"上限 4 MB"悄悄抬成 16 MB,上限等于没生效
        Some(0) | None => 512,
        Some(mb) => mb,
    }
    .max(1)
        * 1_048_576;
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let now = std::time::SystemTime::now();
    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = vec![];
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if !m.is_file() {
            continue;
        }
        let path = e.path();
        // 半截的 .part-* 只有在本进程还活着时才有效;超过 10 分钟的一律当垃圾
        if path.file_name().and_then(|s| s.to_str()).map(|s| s.contains(".part-")).unwrap_or(false) {
            let stale = m.modified().ok()
                .and_then(|t| now.duration_since(t).ok())
                .map(|d| d.as_secs() > 600)
                .unwrap_or(false);
            if stale {
                let _ = std::fs::remove_file(&path);
            }
            continue;
        }
        let old = m.modified().ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(|d| d.as_secs() > 86_400)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        files.push((m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH), path, m.len()));
    }
    let total: u64 = files.iter().map(|f| f.2).sum();
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|f| f.0);
    let mut freed = 0u64;
    for (_, p, len) in files {
        if total - freed <= max_bytes {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            freed += len;
        }
    }
}

/// 预览代理的长边上限(0 = 关)。屏幕上那一格最宽约 900 px,按交付尺寸出图是白传。
pub fn preview_cap() -> u32 {
    std::env::var("REWIND_PREVIEW_MAX_EDGE").ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(1280)
}

pub fn preview(
    preset_path: &std::path::Path,
    input: &std::path::Path,
    out_dir: &std::path::Path,
    t: f64,
    intensity: f64,
    overrides: &[String],
) -> Result<(PathBuf, PathBuf), String> {
    let preset = Preset::load_with_overrides(preset_path, overrides)?.scaled(intensity);
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    prune_preview_cache(out_dir);
    let media = ffrun::probe(input).map_err(|e| coded(MEDIA_UNREADABLE, format!("探测输入失败: {e}")))?;
    // 静帧没有时间轴,-ss>0 抽不到帧:图片的"第 t 秒"一律按 0 处理
    let tf = if media.is_image() { 0.0 } else { t };
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("x");
    let cap = preview_cap();
    let fp = fingerprint(
        &preset.id,
        &serde_json::to_string(&preset).unwrap_or_default(),
        &src_stamp(input),
        intensity,
        tf,
        overrides,
        cap,
    );
    let src_png = out_dir.join(format!("{stem}_@{tf}s_{fp}_src.png"));
    let out_png = out_dir.join(format!("{stem}_@{tf}s_{}_{fp}.png", preset.id));
    // 原帧必须和做旧帧**同几何**:套计划里那条预设自己的几何链(缩放/裁切/补边/定像素比),
    // 再按 DAR 折算成呈现尺寸。否则两层各自 contain 到同一个盒子,滑杆扫过去看到的是两张错位的画。
    // 缓存命中也要算这一条:几何信息要跟着结果一起报出去,而且它只是纯计算,不跑 ffmpeg。
    let plan = ffgraph::build_plan(&preset, &media, None, None)
        .map_err(|e| format!("构建预览计划失败: {e}"))?;
    let (dw, dh) = plan.display_dims();
    // 预览是"看清效果"的代理,不需要按交付尺寸画:2560 宽的图一对预览要传 9.2 MB(成品层单独 5.9 MB),
    // 而屏幕上那一格最宽约 900 px。两层按同一系数缩,§8.1-46 的"两层同几何"不变量仍然成立。
    // REWIND_PREVIEW_MAX_EDGE=0 关掉(定标/对比图需要全尺寸时用);上限值已进指纹,换档自动作废旧缓存
    let (pw, ph) = if cap >= 32 && dw.max(dh) > cap {
        let k = cap as f64 / dw.max(dh) as f64;
        let e = |v: f64| ((v.round() as u32) / 2 * 2).max(2);
        (e(dw as f64 * k), e(dh as f64 * k))
    } else {
        (dw, dh)
    };
    let fit = (pw != plan.out_w || ph != plan.out_h).then(|| format!("scale={pw}:{ph}:flags=bicubic"));
    // 净几何是恒等(呈现尺寸=源尺寸,且链里没有裁切/补边)时**不套链**:
    // 那种链只会让"原片"那一层白挨两次重采样,把差异测小了。
    let reframes = dw != media.width
        || dh != media.height
        || plan.geom.iter().any(|f| f.starts_with("crop=") || f.starts_with("pad="));
    let mut src_vf = if reframes { plan.geom.clone() } else { vec![] };
    // fit 无条件跟随:它现在同时代表"DAR 折算"和"预览代理缩放"。
    // 只在 reframes 时才加的话,净几何恒等的大图会原尺寸出源层、缩放后出成品层 —— 两层又不等了
    if let Some(f) = &fit {
        src_vf.push(f.clone());
    }
    let src_vf = if src_vf.is_empty() { None } else { Some(src_vf.join(",")) };
    let out = json!({"w": plan.out_w, "h": plan.out_h, "dar": plan.dar,
                     "display_w": dw, "display_h": dh, "preview_w": pw, "preview_h": ph});
    let hit = fs::metadata(&out_png).map(|m| m.len() > 0).unwrap_or(false)
        && fs::metadata(&src_png).map(|m| m.len() > 0).unwrap_or(false);
    if hit {
        // 同参数二次预览不重跑:大图一次要 2.8 s / 9.9 MB,拖动滑杆时这笔账很贵
        println!(
            "{}",
            json!({"type":"preview","source":src_png.to_string_lossy(),"result":out_png.to_string_lossy(),
                   "t":tf,"cached":true,"out":out})
        );
        return Ok((src_png, out_png));
    }
    ffrun::extract_frame(input, tf, &src_png, src_vf.as_deref())?;

    // 只跑 t 附近 1 秒的窗口(输入端 -ss):抽第 2 小时的帧也必须跟抽第 1 秒一样便宜
    let seek = (tf - 0.5).max(0.0);
    let win = if seek <= 0.0 { 1.0 } else { 1.5 };
    let t0 = std::time::Instant::now();
    let clip = run_preset_windowed(&preset, "", input, out_dir, Some(win), seek)?;
    // 这段窗口的实测吞吐顺手报出去:界面的"预计耗时"于是是量出来的,不是猜的
    let rate = t0.elapsed().as_secs_f64() / win;
    let r = ffrun::extract_frame(&clip, tf - seek, &out_png, fit.as_deref());
    let _ = fs::remove_file(&clip);
    r?;
    println!(
        "{}",
        json!({"type":"preview","source":src_png.to_string_lossy(),"result":out_png.to_string_lossy(),
               "t":tf,"cached":false,"rate":rate,"out":out})
    );
    Ok((src_png, out_png))
}

pub fn run_preset(
    preset: &Preset,
    input: &std::path::Path,
    out_dir: &std::path::Path,
    preview: Option<f64>,
) -> Result<PathBuf, String> {
    run_preset_windowed(preset, "", input, out_dir, preview, 0.0)
}

/// `seek` 只作用在**第一步**:第一步之后临时文件本身就已经是那个时间窗了,再 -ss 会二次偏移
pub fn run_preset_named(
    preset: &Preset,
    suffix: &str,
    input: &std::path::Path,
    out_dir: &std::path::Path,
    preview: Option<f64>,
) -> Result<PathBuf, String> {
    run_preset_windowed(preset, suffix, input, out_dir, preview, 0.0)
}

/// 清掉某一次运行留下的中间临时件(名字里带 `.tmp.<tag>.`)。
/// 为什么调用方需要它:取消是**杀进程**,引擎自己来不及收尾。实测一次取消在用户的
/// 「Rewind输出」里留下 2 个隐藏 mp4 共 5 MB,而它们看起来像"不知道哪来的鬼文件"。
/// 只删带本次标签的名字,所以并发的另一手、以及用户的任何文件都不会被碰。
pub fn sweep_temps(dir: &std::path::Path, tag: &str) -> usize {
    if tag.is_empty() {
        return 0;
    }
    let needle = format!(".tmp.{tag}.");
    let mut removed = 0;
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return 0,
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        // 名字里必须真含本次标签;目录不删(用户放东西的地方我们只清自己写的文件)
        if !name.contains(&needle) || e.path().is_dir() {
            continue;
        }
        if fs::remove_file(e.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// 失败信息里补上"是不是把盘写满了"。
///
/// 这里**故意不做"开跑前预检剩余空间"**:实测 21 个任务,峰值临时占用除以输入文件大小
/// 从 0.6× 一直到 67.7×(中间趟按 crf14/ultrafast 重编码,体积取决于画面噪点量而不是源文件
/// 多大;按画布像素 × 帧数估也跨不过 7× 的内容差异)。估不出"要多少"就没资格说"够不够"——
/// 定一个固定下限只会误伤真要跑的长视频。何况盘真满时第一趟几百毫秒就写不下去,
/// 失败本来就来得很快;缺的是这条把 ENOSPC 说成人话的话,不是预测。
/// 另外 `std::fs::available_space` 在本工具链(rustc 1.99)里并不存在,要拿剩余空间得为
/// 一次检查引入 libc / windows-sys 两个跨平台依赖 —— 不值。
/// ffmpeg 在 ENOSPC 下给的是半句 `write failed`,用户看不出是自己盘满还是滤镜炸了;
/// 三平台的措辞都收在这里(No space left / Disk full / allocation exceeded / out of disk space)。
fn disk_full_hint(err: &str, out_dir: &std::path::Path) -> String {
    const MARKS: [&str; 6] = [
        "No space left",
        "no space left",
        "Disk full",
        "allocation exceeded",
        "out of disk space",
        "not enough space",
    ];
    if !MARKS.iter().any(|m| err.contains(m)) {
        return err.to_string();
    }
    // 确诊为盘满就换码:英文界面据此直接说"清理磁盘",而不是笼统一句"某一趟失败"
    let err = recode(ENGINE_DISK_FULL, err);
    format!("{err}(磁盘写满了:{} 所在分区放不下这一趟的临时文件。清理空间后重试;失败趟的半截文件已删,不会留下垃圾)",
        out_dir.display())
}

pub fn run_preset_windowed(
    preset: &Preset,
    suffix: &str,
    input: &std::path::Path,
    out_dir: &std::path::Path,
    preview: Option<f64>,
    seek: f64,
) -> Result<PathBuf, String> {
    fs::create_dir_all(out_dir).map_err(|e| coded(FS_OUTDIR, format!("创建输出目录失败: {e}")))?;
    let media = ffrun::probe(input)?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or(coded(FS_OUTDIR, "输入文件名无效"))?
        .to_string();
    let ext = if media.is_image() {
        input.extension().and_then(|e| e.to_str()).unwrap_or("png").to_string()
    } else {
        "mp4".to_string()
    };
    let base = format!("{stem}_{}{suffix}", preset.id);
    let tag = run_tag();
    let tmp = out_dir.join(format!(".{base}.tmp.{tag}.{ext}"));
    let font = ffrun::timestamp_font();
    if font.is_none() && ffgraph::wants_timestamp(&preset) {
        eprintln!("warn: 跳过 overlay_timestamp(没有可用字体,或这个 ffmpeg 构建不含 drawtext 滤镜)");
    }

    println!(
        "{}",
        serde_json::json!({"type":"start","preset":preset.id,"input":input.to_string_lossy(),
                          "width":media.width,"height":media.height,"fps":media.fps,"duration":media.duration,
                          // 运行标签一起报出去:调用方(serve / 桌面壳)杀进程时来不及让引擎自己收尾,
                          // 但它可以用这个标签把 `*.tmp.<tag>.*` 的中间件精确清掉,绝不误伤并发的另一手
                          "tag":tag})
    );

    let res = (|| -> Result<(), String> {
        let mut plan = ffgraph::build_plan_windowed(&preset, &media, preview, font.as_deref(), seek)?;
        if media.is_image() {
            // 最终趟按目标扩展名换成真图片编码器(否则 H.264 装进 .png 文件名)
            if let Some(ffgraph::Step::Fast(last)) = plan.steps.last_mut() {
                last.vcodec = ffgraph::still_codec(&ext);
            }
        }
        let n = plan.steps.len();
        let mut prev = input.to_path_buf();
        for (i, step) in plan.steps.iter().enumerate() {
            let prog = ffrun::Prog { step: (i + 1) as u32, steps: n as u32 };
            // 每趟起步先报一条 0%。ffmpeg 的 out_time 读数取决于它跑多快,快机器上
            // 小样一趟可能一条中间读数都没有 —— "起步 + 收尾"是结构性的两条,界面
            // 由此保证点下运行就能看到条在动,而不是等 ffmpeg 自己开口。
            ffrun::emit(&prog, 0.0);
            let step_out = if i == n - 1 {
                tmp.clone()
            } else {
                let ext = match step {
                    ffgraph::Step::Fast(p) => p.out_ext.as_str(),
                    ffgraph::Step::Pixel(_) => "mp4",
                };
                out_dir.join(format!(".{stem}_{}.s{}.tmp.{tag}.{}", preset.id, i + 1, ext))
            };
            let r = match step {
                ffgraph::Step::Fast(pass) => {
                    ffrun::run_pass(pass, &prev, input, &step_out, &media, preview, if i == 0 { seek } else { 0.0 }, &prog)
                }
                ffgraph::Step::Pixel(seg) => {
                    crate::pixel::run_pixel_seg(seg, &prev, &step_out, &media, preview, if i == 0 { seek } else { 0.0 }, &prog)
                }
            };
            if let Err(e) = r {
                let _ = fs::remove_file(&step_out);
                // 上一趟的中间件也必须一起走:它只在"这一步成功"之后才被删,
                // 所以第 3 趟炸掉时第 2 趟的鬼文件会留在用户输出目录里(实测)。
                if i > 0 {
                    let _ = fs::remove_file(&prev);
                }
                return Err(disk_full_hint(&e, out_dir));
            }
            // 只清理上一趟的临时文件(i>0 时 prev 才是临时文件,绝不能删用户输入)
            if i > 0 {
                let _ = fs::remove_file(&prev);
            }
            prev = step_out;
        }
        Ok(())
    })();

    match res {
        Ok(()) => {
            // 永不静默覆盖:成品名在跑完才占位,同名自动 -1/-2(§规划 §7)
            let final_out = reserve(out_dir, &base, &ext)?;
            if let Err(e) = fs::rename(&tmp, &final_out) {
                let _ = fs::remove_file(&final_out);
                return Err(coded(FS_OUTDIR, format!("改名输出失败: {e}")));
            }
            println!(
                "{}",
                serde_json::json!({"type":"done","output":final_out.to_string_lossy()})
            );
            Ok(final_out)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{disk_full_hint, fs, sweep_temps};
    use std::path::Path;

    /// 取消/被杀后的收尾:只认带本次运行标签的中间件,别人的文件与并发的另一手都不许碰。
    #[test]
    fn sweep_temps_removes_only_this_runs_partials() {
        let tag = "TAGME-1";
        let dir = std::env::temp_dir().join(format!("rewind_sweep_{}", super::run_tag()));
        fs::create_dir_all(&dir).unwrap();
        let mk = |n: &str| {
            fs::write(dir.join(n), b"x").unwrap();
        };
        // 本次运行的中间件与半成品
        mk(&format!(".clip_crt1995.s2.tmp.{tag}.mp4"));
        mk(&format!(".clip_crt1995_1x.tmp.{tag}.mp4"));
        // 必须留下的:并发的另一手、用户成品、子目录
        mk(".clip_crt1995.s2.tmp.OTHERTAG-2.mp4");
        mk("clip_crt1995.mp4");
        fs::create_dir_all(dir.join(format!("weird.tmp.{tag}.dir"))).unwrap();
        assert_eq!(sweep_temps(&dir, tag), 2, "只该删掉带本次标签的两个中间件");
        assert!(dir.join(".clip_crt1995.s2.tmp.OTHERTAG-2.mp4").exists(), "并发另一手被误删");
        assert!(dir.join("clip_crt1995.mp4").exists(), "用户成品被误删");
        assert!(dir.join(format!("weird.tmp.{tag}.dir")).is_dir(), "目录被误删");
        // 空标签不许当成"匹配一切"
        assert_eq!(sweep_temps(&dir, ""), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 盘满必须说人话:三平台措辞都要接住,而且不许把别的错也赖成盘满。
    /// (端到端接线由 regression.sh 用假 ffmpeg 验,这里只验判据本身)
    #[test]
    fn enospc_wordings_get_a_disk_full_note() {
        let out = Path::new("/tmp/x");
        for msg in [
            "step2/3 趟执行失败 (exit Some(1)):\n[vost#0:0 @ 0x1] Error writing trailer: No space left on device",
            "step1/1 趟执行失败:write error: Disk full or allocation exceeded",
            "step1/1 趟执行失败:av_write: out of disk space",
            "step1/1 趟执行失败:not enough space",
        ] {
            let got = disk_full_hint(msg, out);
            assert!(got.contains("磁盘写满"), "盘满没被认出来:{got}");
            // 码必须是盘满码:界面据此决定说"清理磁盘"还是"某一趟失败"。
            // 只断言"带了某个码"是不够的 —— 上一轮的教训是断言要绑到会产出这个结果的那条路径。
            assert!(got.starts_with("[engine.disk_full] "), "没换成盘满码:{got}");
            assert!(got.contains(msg), "原文被改写了:{got}");
        }
        // 上游已经贴过码(某一趟失败)时是"换码",不是两个码并排 —— 界面只读开头那一个
        let tagged = disk_full_hint(
            &crate::errcode::coded(
                crate::errcode::ENGINE_PASS,
                "step2/3 趟执行失败:Error writing trailer: No space left on device",
            ),
            out,
        );
        assert!(tagged.starts_with("[engine.disk_full] "), "换码失败:{tagged}");
        assert_eq!(tagged.matches(']').count(), 1, "码叠码:{tagged}");
    }

    /// 摘要与盘满判据是两条独立的链,接口处必须接得上:ffrun 现在把 ffmpeg 的长行折头折尾,
    /// 而 ENOSPC 那句话常写在同一行的**末尾** —— 折丢了就是"该清磁盘却让你去查滤镜"。
    #[test]
    fn the_error_digest_still_feeds_the_disk_full_note() {
        let raw = format!(
            "[out#0/mp4 @ 0x1] Error writing trailer of {} : No space left on device\n",
            "C:/Users/someone/Videos/素材".repeat(30)
        );
        assert!(raw.chars().count() > 400, "样例本身不够长,测不出折叠:{}", raw.chars().count());
        let d = crate::ffrun::ffmpeg_error_digest(raw.as_bytes());
        let got = disk_full_hint(&d, Path::new("/tmp/x"));
        assert!(got.starts_with("[engine.disk_full] "), "盘满码在摘要之后丢了:{got}");
    }

    /// 反向:普通滤镜错误绝不能贴"盘满"标签,否则用户会去清磁盘而问题还在管线里。
    #[test]
    fn ordinary_failures_are_not_blamed_on_the_disk() {
        for msg in [
            "step2/3 趟执行失败 (exit Some(1)):\nNo such filter: 'banddither'",
            "编码失败 exit Some(139) (解码 exit Some(0))",
            "step1/1 趟没有产出文件(滤镜链吃掉了全部帧?): \"/tmp/a.mp4\"",
        ] {
            assert_eq!(disk_full_hint(msg, std::path::Path::new("/tmp/x")), msg, "不该误报盘满:{msg}");
        }
    }
}
