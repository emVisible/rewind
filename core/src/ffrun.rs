use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::errcode::{coded, ENGINE_FFPROBE, ENGINE_FRAME, ENGINE_OUTPUT, ENGINE_PASS, ENGINE_SPAWN, FS_OUTDIR, MEDIA_UNREADABLE};
use crate::ffgraph::{AudioMode, MediaInfo, Pass, num};

pub fn ffmpeg_bin() -> String {
    std::env::var("REWIND_FFMPEG").unwrap_or_else(|_| "ffmpeg".into())
}
pub fn ffprobe_bin() -> String {
    std::env::var("REWIND_FFPROBE").unwrap_or_else(|_| "ffprobe".into())
}

/// 一趟的进度上下文。一趟内部报的是 0–100 的本趟百分比,而界面要的是**整个任务**的
/// 百分比 —— 直接画本趟读数会让 5 趟的任务把 0→100% 扫五遍(用户看到的"进度在撒谎")。
///
/// 权重口径:每趟等分。这不是偷懒,是实测结论:用 4 种素材 × 8 个预设的 92 趟真实耗时
/// 回放对比过 —— 静态成本模型(画布像素 × 帧率 × 路径系数)平均误差 6.3pp,比等分的
/// 5.3pp 更差;吞吐自适应(用当前趟速度外推其余趟)3.7pp 最准,但会把读数往回抽,
/// 实测最坏回退 31.6pp。一根会倒退 30% 的条比 5 个百分点的误差更难看,所以选等分。
#[derive(Debug, Clone, Copy)]
pub struct Prog {
    pub step: u32,
    pub steps: u32,
}

impl Prog {
    pub fn label(&self) -> String {
        format!("step{}/{}", self.step, self.steps)
    }

    /// 总体百分比:前面各趟算满,本趟按自己的读数占 1/steps。
    /// 恒不后退(等分 + 本趟 pct 单调),所以界面不需要钳位。
    pub fn overall(&self, pct: f64) -> f64 {
        let n = self.steps.max(1) as f64;
        ((self.step.saturating_sub(1) as f64 + pct.clamp(0.0, 100.0) / 100.0) / n) * 100.0
    }
}

/// 进度读数钳位:两头都收进 0–100。
/// 像素路径的 pct 是"已解帧数 / (时长 × 段帧率)",VFR 素材或 `round=near` 补帧会让分母少算 →
/// 读数能越过 100;fast 路径的 pct 来自 ffmpeg 的 out_time,窗口边界处同样可能越界。
pub fn pct_clamp(p: f64) -> f64 {
    // 非有限值必须就地收掉:NaN 进 JSON 会变成 null,而界面读到 null 百分比已经咬过一次
    // (静帧时长 0 → total 0 → pct = inf 那一版就是这个形状)
    if !p.is_finite() {
        return if p > 0.0 { 100.0 } else { 0.0 };
    }
    (p.clamp(0.0, 100.0) * 10.0).round() / 10.0
}

pub fn emit(prog: &Prog, pct: f64) {
    let pct = pct_clamp(pct);
    println!(
        "{}",
        serde_json::json!({
            "type": "progress",
            "stage": prog.label(),
            "step": prog.step,
            "steps": prog.steps,
            "pct": pct,
            "overall": (prog.overall(pct) * 10.0).round() / 10.0,
        })
    );
}

/// ffmpeg 失败时该把哪几行给用户看。
/// ffmpeg 的滤镜报错会把整条链原样回显,单行轻松过千字;旧写法直接取 stderr 末尾
/// 600 字节,窗口里全是回显,真正那两句(`Invalid argument` / `No such filter` /
/// `Fontconfig error`)被挤出屏幕 —— Windows CI 上连续两轮都只能靠猜报错原因。
/// 现在按行挑:短句留原文;长句留两头(`around:` 那种则留报错位置之后一小段),总数再收进预算。
pub fn ffmpeg_error_digest(err: &[u8]) -> String {
    fn clip(s: &str, n: usize) -> String {
        let mut it = s.chars();
        let head: String = it.by_ref().take(n).collect();
        if it.next().is_some() {
            format!("{head}…")
        } else {
            head
        }
    }
    fn tail_clip(s: &str, n: usize) -> String {
        let cs: Vec<char> = s.chars().collect();
        let cut = cs.len().saturating_sub(n);
        let mut out = String::new();
        if cut > 0 {
            out.push('…');
        }
        out.extend(&cs[cut..]);
        out
    }
    fn fold(line: &str) -> String {
        if line.chars().count() <= 160 {
            return line.to_string();
        }
        match line.find("around:") {
            Some(i) => {
                let after = line[i + "around:".len()..].trim_start();
                format!("{} around: {}", clip(line[..i].trim_end(), 72), clip(after, 120))
            }
            // 结论常常写在同一行的末尾(如 "… : No space left on device),所以两头都留
            None => format!("{} … {}", clip(line, 80), tail_clip(line, 80)),
        }
    }
    let text = String::from_utf8_lossy(err);
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let kept = fold(raw);
        if lines.last().map(|l| l == &kept).unwrap_or(false) {
            continue;
        }
        lines.push(kept);
    }
    // ffmpeg 的结论永远在最后几行,所以超预算时留尾部
    let mut out: Vec<String> = Vec::new();
    let mut budget = 700usize;
    for l in lines.into_iter().rev() {
        let cost = l.chars().count() + 1;
        if !out.is_empty() && cost > budget {
            break;
        }
        budget = budget.saturating_sub(cost);
        out.push(l);
    }
    out.reverse();
    out.join("\n")
}

#[derive(Deserialize)]
struct ProbeJson {
    streams: Vec<Stream>,
    #[serde(default)]
    format: Fmt,
}
#[derive(Deserialize)]
struct Stream {
    #[serde(rename = "codec_type")]
    codec_type: String,
    #[serde(default)]
    width: Option<u64>,
    #[serde(default)]
    height: Option<u64>,
    #[serde(default)]
    avg_frame_rate: Option<String>,
    #[serde(default)]
    r_frame_rate: Option<String>,
    #[serde(default)]
    duration: Option<String>,
}
#[derive(Deserialize, Default)]
struct Fmt {
    #[serde(default)]
    duration: Option<String>,
    #[serde(default, rename = "format_name")]
    format_name: Option<String>,
}

fn parse_rate(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    if b > 0.0 { Some(a / b) } else { None }
}

/// 时长的唯一取法:容器 → 视频流 → 音频流 → 0(= 当静帧处理)。
/// 以前 `probe` 只看容器、`inspect` 只看容器 + 音频流:同一份素材两边给出两个答案,
/// 网页版媒体条显示"8s"而引擎按静帧剥掉 fps/隔行/头尾雪花(§8.1-43 同族)
fn resolve_duration(candidates: [Option<f64>; 3]) -> f64 {
    candidates.into_iter().flatten().find(|d| *d > 0.0).unwrap_or(0.0)
}

fn dur_f(s: Option<&str>) -> Option<f64> {
    s.and_then(|d| d.parse::<f64>().ok())
}

pub fn probe(input: &Path) -> Result<MediaInfo, String> {
    let out = Command::new(ffprobe_bin())
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(input)
        .output()
        .map_err(|e| coded(ENGINE_FFPROBE, format!("ffprobe 启动失败: {e}")))?;
    if !out.status.success() {
        return Err(coded(ENGINE_FFPROBE, format!("ffprobe 失败: {}", String::from_utf8_lossy(&out.stderr))));
    }
    let j: ProbeJson =
        serde_json::from_slice(&out.stdout).map_err(|e| coded(ENGINE_OUTPUT, format!("ffprobe 输出解析失败: {e}")))?;
    let v = j
        .streams
        .iter()
        .find(|s| s.codec_type == "video")
        .ok_or(coded(MEDIA_UNREADABLE, "未找到视频流"))?;
    let has_audio = j.streams.iter().any(|s| s.codec_type == "audio");
    let fps = v
        .avg_frame_rate
        .as_deref()
        .and_then(parse_rate)
        .or_else(|| v.r_frame_rate.as_deref().and_then(parse_rate))
        .filter(|f| *f > 0.0 && *f < 1000.0)
        .unwrap_or(25.0);
    let duration = resolve_duration([
        dur_f(j.format.duration.as_deref()),
        dur_f(v.duration.as_deref()),
        j.streams
            .iter()
            .find(|s| s.codec_type == "audio")
            .and_then(|s| dur_f(s.duration.as_deref())),
    ]);
    // ffprobe 对非媒体文件也可能退 0 并给出无尺寸流,这里必须挡住
    let width = v.width.unwrap_or(0) as u32;
    let height = v.height.unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        return Err(coded(MEDIA_UNREADABLE, "读不到画面尺寸:不是可处理的媒体文件"));
    }
    Ok(MediaInfo {
        width,
        height,
        fps,
        duration,
        has_audio,
        container: j.format.format_name.clone().unwrap_or_default(),
    })
}

/// 界面用的完整素材报告(§规划 §3:上传后 probe 驱动源自适应、警告与推算值)。
/// 与 `probe` 的区别:这里保留 SAR/DAR/VFR/编码/容器/大小等展示字段,不做管线所需的规整。
pub fn inspect(input: &Path) -> Result<serde_json::Value, String> {
    let out = Command::new(ffprobe_bin())
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(input)
        .output()
        .map_err(|e| coded(ENGINE_FFPROBE, format!("ffprobe 启动失败: {e}")))?;
    if !out.status.success() {
        return Err(coded(ENGINE_FFPROBE, format!("ffprobe 失败: {}", last_err_line(&out.stderr))));
    }
    let j: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| coded(ENGINE_OUTPUT, format!("ffprobe 输出解析失败: {e}")))?;
    let rate = |s: Option<&str>| -> Option<f64> {
        let (a, b) = s?.split_once('/')?;
        let a: f64 = a.parse().ok()?;
        let b: f64 = b.parse().ok()?;
        if b > 0.0 {
            Some(a / b)
        } else {
            None
        }
    };
    let streams = j["streams"].as_array().cloned().unwrap_or_default();
    let v = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("video"))
        .ok_or(coded(MEDIA_UNREADABLE, "未找到视频流"))?;
    let a = streams.iter().find(|s| s["codec_type"].as_str() == Some("audio"));
    let fmt = &j["format"];
    let dur = resolve_duration([
        dur_f(fmt["duration"].as_str()),
        dur_f(v["duration"].as_str()),
        a.and_then(|x| dur_f(x["duration"].as_str())),
    ]);
    let r_fps = rate(v["r_frame_rate"].as_str()).unwrap_or(0.0);
    let a_fps = rate(v["avg_frame_rate"].as_str()).unwrap_or(0.0);
    let fps = if a_fps > 0.0 { a_fps } else { r_fps };
    let sar = v["sample_aspect_ratio"].as_str().unwrap_or("1:1").to_string();
    let (sn, sd) = sar
        .split_once(':')
        .map(|(a, b)| (a.parse::<f64>().unwrap_or(1.0), b.parse::<f64>().unwrap_or(1.0)))
        .unwrap_or((1.0, 1.0));
    let w = v["width"].as_u64().unwrap_or(0) as f64;
    let h = v["height"].as_u64().unwrap_or(0) as f64;
    let dar = if sn > 0.0 && sd > 0.0 && h > 0.0 { w * sn / (h * sd) } else { w / h.max(1.0) };
    let pix_fmt = v["pix_fmt"].as_str().unwrap_or("");
    let codec = v["codec_name"].as_str().unwrap_or("");
    // 素材报告里的 is_image 必须走引擎同一条判据(ffgraph::still_container + 无时长)。
    // 这里原先自己写了一份 `dur <= 0.0`:裸 H.264 流报 duration=0,报告说"是图片"、
    // 管线却按视频跑,界面据此给出的解释与真实行为就是两回事(判据写两份的老毛病)。
    let container = fmt["format_name"].as_str().unwrap_or("");
    let is_image = dur <= 0.0 && crate::ffgraph::still_container(container);
    Ok(serde_json::json!({
        "width": w as u32,
        "height": h as u32,
        "fps": (fps * 1000.0).round() / 1000.0,
        "duration": (dur * 1000.0).round() / 1000.0,
        "has_audio": a.is_some(),
        "is_image": is_image,
        "sar": sar,
        "dar": (dar * 1000.0).round() / 1000.0,
        "vfr": r_fps > 0.0 && a_fps > 0.0 && (r_fps - a_fps).abs() > 0.02,
        "interlaced": v["field_order"].as_str().map(|f| f != "progressive" && f != "unknown").unwrap_or(false),
        "video_codec": codec,
        "audio_codec": a.and_then(|x| x["codec_name"].as_str()).unwrap_or(""),
        "container": container,
        "size_bytes": fmt["size"].as_str().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
        "bit_rate": fmt["bit_rate"].as_str().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
        "pix_fmt": pix_fmt,
        "color_range": v["color_range"].as_str().unwrap_or("unknown"),
        "color_space": v["color_space"].as_str().unwrap_or("unknown"),
        "hdr": pix_fmt.contains("10le") || v["color_space"].as_str().map(|s| s.contains("bt2020")).unwrap_or(false),
        "rotation": v["side_data_list"].as_array().and_then(|l| l.first()).and_then(|d| d["rotation"].as_f64()).unwrap_or(0.0),
    }))
}

fn last_err_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .to_string()
}

/// 这个 ffmpeg 构建里有没有 drawtext 滤镜。
/// 不是所有构建都有:没编 libfreetype 的(某些 brew/精简包)会直接 `No such filter: 'drawtext'`,
/// 退出码 8 —— 缺一个时间戳不该让整趟渲染失败,所以先探一次,没有就当"画不了"处理。
pub fn drawtext_available() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        Command::new(ffmpeg_bin())
            .args(["-hide_banner", "-filters"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("drawtext"))
            .unwrap_or(false)
    })
}

/// 纯函数那半份,单测靠它:滤镜和字体缺一样,时间戳就只能跳过。
pub fn timestamp_font_for(has_drawtext: bool, font: Option<String>) -> Option<String> {
    if has_drawtext { font } else { None }
}

/// 真正能拿来画时间戳的字体(没有滤镜就没有字体可言)
pub fn timestamp_font() -> Option<String> {
    timestamp_font_for(drawtext_available(), find_font())
}

/// 找一款 drawtext 可用的 ttf 字体(跨平台候选)
pub fn find_font() -> Option<String> {
    // 显式指定优先:打包分发与"字体装在非常规位置"的用户都有出路
    if let Some(p) = std::env::var_os("REWIND_FONT") {
        let s = p.to_string_lossy().into_owned();
        if !s.is_empty() && Path::new(&s).exists() {
            return Some(s);
        }
    }
    let candidates: &[&str] = if cfg!(windows) {
        &["C:\\Windows\\Fonts\\arial.ttf", "C:\\Windows\\Fonts\\tahoma.ttf"]
    } else if cfg!(target_os = "macos") {
        &["/Library/Fonts/Arial.ttf", "/System/Library/Fonts/Helvetica.ttc"]
    } else {
        &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        ]
    };
    candidates.iter().find(|p| Path::new(p).exists()).map(|p| p.to_string()).or_else(|| {
        // 硬编码路径覆盖不了所有发行版(Arch/NixOS/自定义 XDG 字体目录),问 fontconfig 要
        // —— 找不到字体的后果是监控/RMVB 的招牌时间戳**静默消失**,那比慢更糟
        #[cfg(target_os = "linux")]
        {
            let ok = Command::new("fc-match")
                .args(["-f", "%{file}", "DejaVuSans"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty() && Path::new(s).exists());
            return ok;
        }
        #[cfg(not(target_os = "linux"))]
        None
    })
}

/// 把一张 PNG 缩到指定宽度(抽帧条带用:全帧 1.3 MB × 6 张只为渲染 130 px 的格子,太浪费)
pub fn shrink_png(src: &Path, dst: &Path, width: u32) -> Result<(), String> {
    let part = part_of(dst);
    let st = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .args(["-vf", &format!("scale={width}:-2:flags=area")])
        .args(["-frames:v", "1"])
        .arg(&part)
        .status()
        .map_err(|e| coded(ENGINE_FRAME, format!("缩图失败: {e}")))?;
    if !st.success() || std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) == 0 {
        let _ = std::fs::remove_file(&part);
        return Err(coded(ENGINE_FRAME, format!("缩图没有产物: {dst:?}")));
    }
    commit_part(&part, dst)
}

/// 落"最终名"必须原子:ffmpeg 先写 `.part-<run 标签>`,再 rename 到位。
/// 预览文件名由参数指纹决定,两个并发请求算出的目标名一模一样 —— 直接写最终名时,
/// 后到的读者会拿到别人刚写了一半的 PNG(文件存在、非空,但解不开)。
fn commit_part(part: &Path, final_out: &Path) -> Result<(), String> {
    match std::fs::rename(part, final_out) {
        Ok(()) => Ok(()),
        // Windows 上目标正被浏览器读着会让 rename 失败。此时用对方那一份:
        // 同一个文件名 = 同一组参数 = 同一张图,谁写的没区别。
        Err(_) if std::fs::metadata(final_out).map(|m| m.len() > 0).unwrap_or(false) => {
            let _ = std::fs::remove_file(part);
            Ok(())
        }
        Err(e) => Err(coded(FS_OUTDIR, format!("改名 {part:?} -> {final_out:?}: {e}"))),
    }
}

fn part_of(final_out: &Path) -> std::path::PathBuf {
    // 后缀必须留在末尾:ffmpeg 按扩展名选封装器。
    // 第一版写成 ".name.png.part-<tag>",结果 `抽帧失败 exit 1` —— 它不知道该产出什么格式。
    let stem = final_out.file_stem().and_then(|s| s.to_str()).unwrap_or("x");
    let ext = final_out.extension().and_then(|s| s.to_str()).unwrap_or("bin");
    let dir = final_out.parent().map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from("."));
    dir.join(format!(".{stem}.part-{}.{}", crate::pipeline::run_tag(), ext))
}

/// 从视频 t 秒处抽一帧 PNG。`vf` 是可选的滤镜链:对比预览用它把原帧套上和成品同一条几何链,
/// 否则两层尺寸不同,界面上各自 contain 到同一个盒子就会错开成"两张画叠在一起"。
pub fn extract_frame(input: &Path, t: f64, out: &Path, vf: Option<&str>) -> Result<(), String> {
    let part = part_of(out);
    let st = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .arg("-ss").arg(num(t))
        .arg("-i").arg(input)
        .args(["-frames:v", "1"])
        .args(match vf {
            Some(f) if !f.is_empty() => ["-vf".to_string(), f.to_string()].to_vec(),
            _ => vec![],
        })
        .arg(&part)
        .status()
        .map_err(|e| coded(ENGINE_FRAME, format!("ffmpeg 抽帧失败: {e}")))?;
    if !st.success() {
        let _ = std::fs::remove_file(&part);
        return Err(coded(ENGINE_FRAME, format!("抽帧失败 exit {:?}", st.code())));
    }
    // 退出码 0 不等于有产物:对静帧用 -ss 1 就是"成功抽出 0 帧"(与 §8.1-25② 同族)
    match std::fs::metadata(&part) {
        Ok(m) if m.len() > 0 => {}
        _ => {
            let _ = std::fs::remove_file(&part);
            return Err(coded(ENGINE_FRAME, format!("{input:?} 在 t={t}s 抽不到帧(静帧请把时间取 0)")));
        }
    }
    commit_part(&part, out)
}

pub fn run_pass(
    pass: &Pass,
    in_path: &Path,
    original: &Path,
    out_path: &Path,
    media: &MediaInfo,
    limit: Option<f64>,
    seek: f64,
    prog: &Prog,
) -> Result<(), String> {
    let label = prog.label();
    let audio_present = media.has_audio && !media.is_image();
    let mut args: Vec<String> =
        vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostats".into(), "-y".into()];
    // 抽帧预览用输入端 -ss:不解前面那几十分钟(3 小时素材抽一帧也必须便宜)
    if seek > 0.01 {
        args.extend(["-ss".into(), num(seek)]);
    }
    if let Some(l) = limit {
        args.extend(["-t".into(), num(l)]);
    }
    args.extend(["-i".into(), in_path.to_string_lossy().into_owned()]);
    if matches!(&pass.audio, AudioMode::Encode { from_original: true, .. }) {
        args.extend(["-i".into(), original.to_string_lossy().into_owned()]);
    }
    for extra in &pass.extra_inputs {
        args.extend(extra.iter().cloned());
    }

    let fc = pass.filter_complex(audio_present);
    if let Some(f) = &fc {
        args.extend(["-filter_complex".into(), f.clone()]);
    }
    // 视频映射
    if pass.fx_v.is_empty() {
        args.extend(["-map".into(), "0:v".into()]);
    } else {
        args.extend(["-map".into(), "[v]".into()]);
    }
    args.extend(pass.vcodec.iter().cloned());
    // 音频映射与编码
    match &pass.audio {
        AudioMode::None => args.extend(["-an".into()]),
        AudioMode::Copy => {
            args.extend(["-map".into(), "0:a?".into(), "-c:a".into(), "copy".into()])
        }
        AudioMode::Encode { bitrate, hiss, .. } => {
            args.extend(["-map".into(), if audio_present { "[a]".into() } else { String::new() }]);
            args.extend(["-c:a".into(), "aac".into()]);
            if let Some(b) = bitrate {
                args.extend(["-b:a".into(), b.clone()]);
            }
            let _ = hiss;
        }
    }
    if media.is_image() {
        args.extend(["-frames:v".into(), "1".into()]);
    }
    args.extend(pass.out_extra.iter().cloned());
    args.extend(["-progress".into(), "pipe:1".into()]);
    args.push(out_path.to_string_lossy().into_owned());

    let mut child = Command::new(ffmpeg_bin())
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| coded(ENGINE_SPAWN, format!("ffmpeg 启动失败: {e}")))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    // 后台抽干 stderr,防管道塞死;失败时带尾部内容报错
    let err_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut r = stderr;
        let _ = r.read_to_end(&mut buf);
        buf
    });
    let dur = limit.unwrap_or(media.duration).max(0.001);
    let mut last_emit = -100.0;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if let Some(v) = line.strip_prefix("out_time_us=") {
            if let Ok(us) = v.trim().parse::<f64>() {
                let pct = (us / 1e6 / dur).clamp(0.0, 1.0) * 100.0;
                if pct - last_emit >= 2.0 {
                    emit(prog, pct);
                    last_emit = pct;
                }
            }
        }
    }
    let st = child.wait().map_err(|e| e.to_string())?;
    if !st.success() {
        let err = err_handle.join().unwrap_or_default();
        return Err(coded(
            ENGINE_PASS,
            format!("{} 趟执行失败 (exit {:?}):\n{}", label, st.code(), ffmpeg_error_digest(&err)),
        ));
    }
    // 退出码 0 不等于有产物:静帧走 tinterlace 这类"吃两帧出一帧"的滤镜会输出 0 帧,
    // 那时 ffmpeg 正常退出而文件不存在 —— 必须当场报错,否则错误会漂到后面的改名步骤。
    match std::fs::metadata(out_path) {
        Ok(m) if m.len() > 0 => {}
        Ok(_) => return Err(coded(ENGINE_PASS, format!("{} 趟产出空文件: {:?}", label, out_path))),
        Err(_) => {
            let err = err_handle.join().unwrap_or_default();
            let hint = ffmpeg_error_digest(&err);
            let hint = if hint.is_empty() {
                String::new()
            } else {
                format!(";ffmpeg 报错:{hint}")
            };
            return Err(format!(
                "{} 趟没有产出文件(滤镜链吃掉了全部帧?): {:?}{hint}",
                label, out_path
            ));
        }
    }
    emit(prog, 100.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{resolve_duration, timestamp_font_for};

    /// 时间戳是"锦上添花",不是"没它就整趟失败"。某些 ffmpeg 构建没编 libfreetype
    /// (Homebrew 的一部分精简包就是),`drawtext` 直接 `No such filter` 退出 8 ——
    /// 那台机器上监控/RMVB 预设原本全跑不出来。缺滤镜或缺字体,都只能跳过这一层。
    #[test]
    fn timestamp_is_skipped_when_the_filter_or_the_font_is_missing() {
        assert_eq!(timestamp_font_for(true, Some("/x/arial.ttf".into())).as_deref(), Some("/x/arial.ttf"));
        assert_eq!(timestamp_font_for(false, Some("/x/arial.ttf".into())), None);
        assert_eq!(timestamp_font_for(true, None), None);
        assert_eq!(timestamp_font_for(false, None), None);
    }

    /// ffmpeg 报错时把整条滤镜链回显在同一行里,旧写法取 stderr 末尾 600 字节 ——
    /// 窗口被回显占满,真正的结论行(`Invalid argument` 那一句)被挤出去。Windows CI
    /// 上连续两轮都只能靠猜,这条测试就是那场事故的形状。
    #[test]
    fn the_conclusion_line_survives_the_chain_echo() {
        let chain = "eq=contrast=1.2:saturation=0.8,".repeat(60);
        let stderr = format!(
            "[AVFilterGraph @ 0x55a1] Error parsing a filter description around: ,drawtext=fontfile='C:/Windows/Fonts/arial.ttf':text='REC'{chain}\nError initializing complex filters.\nInvalid argument\n"
        );
        let d = super::ffmpeg_error_digest(stderr.as_bytes());
        assert!(d.contains("Invalid argument"), "结论行被挤掉了:{d}");
        assert!(d.contains("Error parsing a filter description"), "报错种类没了:{d}");
        assert!(
            d.contains("around: ,drawtext=fontfile='C:/Windows/Fonts/arial.ttf'"),
            "报错位置没了:{d}"
        );
        assert!(!d.contains(&chain[..180]), "整条链又被照抄回来了:{d}");
        assert!(d.chars().count() <= 760, "超出预算:{} 字", d.chars().count());
    }

    /// 空 stderr 不许编出假报错;超长但没有任何结论行的输出,也只能截断保留。
    #[test]
    fn an_empty_stderr_yields_no_digest() {
        assert_eq!(super::ffmpeg_error_digest(b""), "");
        assert_eq!(super::ffmpeg_error_digest(b"\n  \n"), "");
        let long = "z".repeat(4000);
        let d = super::ffmpeg_error_digest(long.as_bytes());
        assert!(!d.is_empty() && d.chars().count() <= 760, "长行没被收住:{}", d.chars().count());
    }

    /// 总体读数的算法:前 (step-1) 趟算满 + 本趟占的 1/steps。
    /// 界面直接画这个数,所以它必须严格不后退、且最后一趟 100% 时正好 100%。
    #[test]
    fn overall_is_the_step_fraction_spread_over_the_whole_job() {
        let p = |step: u32, steps: u32| super::Prog { step, steps };
        let eq = |v: f64, want: f64| assert!((v - want).abs() < 1e-9, "{v} != {want}");
        eq(p(1, 5).overall(0.0), 0.0);
        eq(p(3, 5).overall(40.0), 48.0);
        eq(p(5, 5).overall(100.0), 100.0);
        eq(p(1, 1).overall(25.0), 25.0);
        // 越界读数不能把条推出画面(pct 来自 ffmpeg,理论上可能超过 100)
        eq(p(2, 4).overall(140.0), 50.0);
        eq(p(2, 4).overall(-3.0), 25.0);
        // 同趟内 pct 递增 → overall 递增;跨趟边界也不后退
        let mut last = -1.0;
        for steps in 1..=8 {
            for step in 1..=steps {
                for pct in [0.0, 1.0, 50.0, 99.9, 100.0] {
                    let o = p(step as u32, steps as u32).overall(pct);
                    assert!(o >= last, "{steps} 趟第 {step} 趟 {pct}% 给出 {o} < {last}");
                    last = o;
                }
            }
            last = -1.0;
        }
    }

    /// 读数越界不许出现在界面上:像素路径的分母(时长 × 帧率)对 VFR / 补帧素材会少算,
    /// 实测帧数可以超过它 → 不钳位就是"137%"。
    #[test]
    fn pct_never_leaves_the_zero_to_hundred_band() {
        assert_eq!(super::pct_clamp(137.4), 100.0);
        assert_eq!(super::pct_clamp(-5.0), 0.0);
        assert_eq!(super::pct_clamp(f64::NAN), 0.0);
        assert_eq!(super::pct_clamp(62.34), 62.3);
        // 钳位之后总体读数也不可能越过 100
        let p = super::Prog { step: 5, steps: 5 };
        assert_eq!(p.overall(900.0), 100.0);
    }

    /// 时长取法的唯一入口:容器缺失时退回流,全都没有才算静帧。
    /// 这条一旦退化,网页版显示 8s 而引擎按静帧剥掉时序效果(两边各写一份的旧账)
    #[test]
    fn duration_prefers_container_then_streams_then_still() {
        assert_eq!(resolve_duration([Some(8.0), Some(3.0), None]), 8.0);
        assert_eq!(resolve_duration([None, Some(6.5), None]), 6.5);
        assert_eq!(resolve_duration([Some(0.0), None, Some(4.0)]), 4.0);
        assert_eq!(resolve_duration([None, None, None]), 0.0);
        assert_eq!(resolve_duration([Some(-1.0), Some(2.0), None]), 2.0);
    }
}
