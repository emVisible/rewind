mod assets;
mod catalog;
mod describe;
mod era;
mod errcode;
mod ffgraph;
mod ffrun;
mod pipeline;
mod pixel;
mod pixops;
mod preset;
mod serve;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use serde_json::json;

use crate::errcode::{coded, FS_OUTDIR, PRESET_LOAD, PRESET_SAVE};

/// 失败要**同时**走两条通道:stderr 给人(CLI 直接看),stdout 给事件流(界面与 `serve` 只读 stdout)。
/// 只写 stderr 的后果是实测出来的:Web 版一次失败的任务连一条 item 事件都收不到,
/// 进度遮罩永远停在那里,用户只能刷新页面 —— 错误文本等于被丢掉了。
fn report_err(e: &str) {
    println!("{}", json!({"type": "error", "error": e}));
    eprintln!("error: {e}");
}

/// 用户预设是手工调出来的成果,覆盖前必须留一份 `<同名>.json.bak`;
/// 先写 `.json.tmp` 再 rename,避免断电/报错留下半截文件把预设毁掉。
/// 返回备份路径(首次写入为 None)。
fn write_preset_file(path: &Path, content: &str) -> Result<Option<PathBuf>, String> {
    let bak = if path.exists() {
        let b = path.with_extension("json.bak");
        std::fs::copy(path, &b).map_err(|e| coded(PRESET_SAVE, format!("备份 {path:?} 失败: {e}")))?;
        Some(b)
    } else {
        None
    };
    std::fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|e| coded(PRESET_SAVE, format!("创建目录失败: {e}")))?;
    // 临时名带运行标签:两个进程/两条命令同时写同一个文件时,共享 .json.tmp 会互相顶掉
    // (A 写完 rename 拿走 B 的内容,B 再 rename 就报"文件不存在")
    let tmp = path.with_extension(format!("json.{}.tmp", crate::pipeline::run_tag()));
    std::fs::write(&tmp, content).map_err(|e| coded(PRESET_SAVE, format!("写 {tmp:?}: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| coded(PRESET_SAVE, format!("替换 {path:?}: {e}")))?;
    Ok(bak)
}

#[derive(Parser)]
#[command(name = "rewind-core", version, about = "Rewind 做旧引擎(本地处理,不联网)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 处理单个文件(视频或图片)
    Run {
        #[arg(long)]
        preset: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out_dir: PathBuf,
        /// 只做前 N 秒(预览用)
        #[arg(long)]
        preview_secs: Option<f64>,
        /// 全局做旧强度 0.2-2.0
        #[arg(long, default_value_t = 1.0)]
        intensity: f64,
        /// 界面覆盖:`stage.key=value`,可重复。如 --override fps.fps=12.5 --override resize.dar=4:3
        #[arg(long = "override")]
        overrides: Vec<String>,
    },
    /// 年代轴:输出该年份的联动预设 JSON(1965-2026)
    Era {
        year: u32,
        /// 直接写入文件;缺省打印到 stdout
        #[arg(long)]
        write: Option<PathBuf>,
    },
    /// 「再翻录一次」:对成品再做 N 轮 mpeg4 低码率往返(生成代际损失)
    Reclip {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 1)]
        times: usize,
        /// 缺省 = 输入文件同目录
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
    /// 对比预览:同管线产出(原帧,做旧帧)对
    Preview {
        #[arg(long)]
        preset: PathBuf,
        #[arg(long)]
        input: PathBuf,
        /// 省略即用共享预览缓存(pipeline::preview_cache_dir)
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// 取第几秒的帧
        #[arg(long, default_value_t = 1.0)]
        t: f64,
        #[arg(long, default_value_t = 1.0)]
        intensity: f64,
        /// 界面覆盖(同 run):预览必须与成品走同一套参数,否则"预览即成品"不成立
        #[arg(long = "override")]
        overrides: Vec<String>,
    },
    /// 「换一批」:预设内所有种子 +1(划痕/雪花/抖动重新抽卡)
    Reroll {
        file: PathBuf,
        #[arg(long)]
        write: Option<PathBuf>,
    },
    /// 发布参数清单(界面按它渲染控件;§13.1)
    Describe,
    /// 保存用户预设(从某年份年代轴或既有预设文件)
    PresetSave {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        era: Option<u32>,
        #[arg(long)]
        from: Option<PathBuf>,
    },
    /// 批量处理
    Batch {
        #[arg(long)]
        preset: PathBuf,
        #[arg(long)]
        out_dir: PathBuf,
        inputs: Vec<PathBuf>,
        #[arg(long, default_value_t = 1.0)]
        intensity: f64,
    },
    /// 本地 Web UI(浏览器打开 http://127.0.0.1:<port>)
    Serve {
        #[arg(long, default_value_t = 8137)]
        port: u16,
        /// UI 目录(默认按 dev/安装态自动找)
        #[arg(long)]
        ui: Option<PathBuf>,
    },
    /// 探测媒体信息(JSON)
    Probe { input: PathBuf },
    /// 解析并暂存内置示例素材(界面"试试示例素材"唯一的取素材入口)
    Sample {
        /// 复制到哪个目录(壳传自己的临时目录)
        #[arg(long)]
        out_dir: PathBuf,
        /// 内置预设目录,用来反推仓库根
        #[arg(long)]
        presets: Option<PathBuf>,
    },
    /// 列出预设目录里的全部预设
    Presets { dir: PathBuf },
    /// 预设目录:内置/我的/派生三层 + 对比资产存在性(界面唯一的列表来源)
    Catalog {
        /// UI 目录(用于探测 gallery 资产)
        #[arg(long)]
        ui: Option<PathBuf>,
        /// 内置预设目录
        #[arg(long)]
        presets: Option<PathBuf>,
    },
    /// 打印编译后的管线计划(调试与结构闸:看某一手到底跑了哪些滤镜)
    Plan {
        #[arg(long)]
        preset: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        intensity: f64,
        #[arg(long = "override")]
        overrides: Vec<String>,
    },
    /// 随机抽帧预览:K 个时间点的(原帧,成品帧)对,用于"先看一眼再决定跑不跑"
    Samples {
        #[arg(long)]
        preset: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 6)]
        count: u32,
        /// 换一批帧 = 换 seed;同 seed 同素材结果完全一致(可复现)
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, default_value_t = 1.0)]
        intensity: f64,
        #[arg(long = "override")]
        overrides: Vec<String>,
    },
    /// 改预设文件的 id 与显示名(存为预设时用)
    PresetRename { file: PathBuf, name: String },
    /// 清掉某次运行被 kill 后留下的中间临时件(只认 `.tmp.<tag>.` 名字)
    SweepTemps {
        #[arg(long)]
        out_dir: PathBuf,
        #[arg(long)]
        tag: String,
    },
}

/// 安装态(exe 同级)优先,再向上找源码工作区。`serve` 与 `catalog` 共用一份。
fn exe_roots() -> (PathBuf, Vec<PathBuf>) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rewind-core"));
    let exe_dir = exe.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let roots: Vec<PathBuf> = exe_dir.ancestors().take(5).map(PathBuf::from).collect();
    (exe_dir, roots)
}

fn find_ui(ui: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(u) = ui {
        return Some(u);
    }
    if let Some(u) = std::env::var_os("REWIND_UI").map(PathBuf::from) {
        return Some(u);
    }
    let (exe_dir, roots) = exe_roots();
    // 安装态:tauri 的 resources 可能落在 exe 同级或下一级(各平台布局不同,逐个探)
    let mut cands: Vec<PathBuf> = vec![
        exe_dir.join("ui"),
        exe_dir.join("resources").join("ui"),
        exe_dir.join("Resources").join("ui"),
    ];
    cands.extend(roots.iter().flat_map(|r| vec![r.join("app").join("ui"), r.join("ui")]));
    cands.into_iter().find(|p| p.exists())
}

fn find_presets(presets: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(p) = presets {
        return Some(p);
    }
    if let Some(p) = std::env::var_os("REWIND_PRESETS").map(PathBuf::from) {
        return Some(p);
    }
    let (_, roots) = exe_roots();
    roots.iter().map(|r| r.join("presets")).find(|p| p.exists())
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.cmd {
        Cmd::Run { preset, input, out_dir, preview_secs, intensity, overrides } => {
            match pipeline::run(&preset, &input, &out_dir, preview_secs, intensity, &overrides) {
                Ok(_) => 0,
                Err(e) => {
                    report_err(&e);
                    1
                }
            }
        }
        Cmd::Era { year, write } => {
            let p = era::preset_for_year(year);
            let j = serde_json::to_string_pretty(&p).expect("序列化预设失败");
            match write {
                Some(path) => {
                    match write_preset_file(&path, &j) {
                        Ok(bak) => {
                            eprintln!("已写入 {path:?}");
                            if let Some(b) = bak {
                                eprintln!("旧版本已备份为 {:?}", b);
                            }
                        }
                        Err(e) => {
                            eprintln!("error: {e}");
                            return std::process::exit(1);
                        }
                    }
                }
                None => println!("{j}"),
            }
            0
        }
        Cmd::Reclip { input, times, out_dir } => {
            let dir = out_dir.unwrap_or_else(|| input.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                report_err(&coded(FS_OUTDIR, format!("创建输出目录 {dir:?}: {e}")));
                return std::process::exit(1);
            }
            let p = preset::Preset::reclip(times);
            let j = serde_json::to_string(&p).expect("序列化");
            // 名字里带 `.tmp.<标签>.`:被 kill 的这一次没机会删掉它,收尾时调用方的
            // `sweep-temps` 才认得这是谁的残留(只清本次标签的,不碰别人的)
            let tmp = dir.join(format!(".{}.tmp.{}.json", p.id, crate::pipeline::run_tag()));
            if let Err(e) = std::fs::write(&tmp, &j) {
                eprintln!("error: {e}");
                return std::process::exit(1);
            }
            let r = pipeline::run(&tmp, &input, &dir, None, 1.0, &[]);
            let _ = std::fs::remove_file(&tmp);
            match r {
                Ok(out) => {
                    println!("{}", json!({"type":"done","output":out.to_string_lossy(),"times":times}));
                    0
                }
                Err(e) => {
                    report_err(&e);
                    1
                }
            }
        }
        Cmd::PresetSave { dir, name, era, from } => {
            let mut p = match (era, from) {
                (Some(y), _) => era::preset_for_year(y),
                (None, Some(f)) => match preset::Preset::load(&f) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("error: {e}");
                        return std::process::exit(1);
                    }
                },
                (None, None) => {
                    eprintln!("error: 需指定 --era 或 --from");
                    return std::process::exit(2);
                }
            };
            // 名字直接当文件名用:空名会写出 `.json`(catalog 里就是一条 id 为空的预设),
            // 带分隔符则跑出 --dir —— 实测 `--name ../esc` 真的把预设写到了上一级目录
            let name = match name.trim() {
                s if s.is_empty() => {
                    eprintln!("error: 预设名不能为空");
                    return std::process::exit(2);
                }
                s if s.contains('/') || s.contains('\\') || s.starts_with('.') || s.contains(".json") => {
                    eprintln!("error: 预设名不能含路径分隔符、以点开头或带 .json: {s}");
                    return std::process::exit(2);
                }
                s => s.to_string(),
            };
            p.id = name.clone();
            p.name = name.clone();
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("error: {e}");
                return std::process::exit(1);
            }
            let path = dir.join(format!("{name}.json"));
            match write_preset_file(&path, &serde_json::to_string_pretty(&p).unwrap()) {
                Ok(bak) => {
                    println!(
                        "{}",
                        json!({"type":"saved","path":path.to_string_lossy(),
                               "backup": bak.map(|b| b.to_string_lossy().into_owned()).unwrap_or_default()})
                    );
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Cmd::Preview { preset, input, out_dir, t, intensity, overrides } => {
            let out_dir = out_dir.unwrap_or_else(pipeline::preview_cache_dir);
            match pipeline::preview(&preset, &input, &out_dir, t, intensity, &overrides) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Cmd::Reroll { file, write } => {
            let raw = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {}", coded(PRESET_LOAD, format!("读预设 {file:?}: {e}")));
                    return std::process::exit(1);
                }
            };
            let mut v: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {}", coded(PRESET_LOAD, format!("解析预设: {e}")));
                    return std::process::exit(1);
                }
            };
            let n = preset::reroll_value(&mut v);
            let out = serde_json::to_string_pretty(&v).unwrap();
            let target = write.unwrap_or(file.clone());
            match write_preset_file(&target, &out) {
                Ok(bak) => {
                    println!(
                        "{}",
                        json!({"type":"rerolled","seeds":n,"path":target.to_string_lossy(),
                               "backup": bak.map(|b| b.to_string_lossy().into_owned()).unwrap_or_default()})
                    );
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Cmd::Batch { preset, out_dir, inputs, intensity } => {
            let mut fail = 0;
            for (i, input) in inputs.iter().enumerate() {
                println!(
                    "{}",
                    serde_json::json!({"type":"item","index":i,"total":inputs.len(),"input":input.to_string_lossy()})
                );
                if let Err(e) = pipeline::run(&preset, input, &out_dir, None, intensity, &[]) {
                    eprintln!("error: {input:?}: {e}");
                    // 字段名与 Run/Reclip 同一条约定:界面只认 "error"
                    println!("{}", serde_json::json!({"type":"error","index":i,"error":e}));
                    fail += 1;
                }
            }
            if fail > 0 {
                1
            } else {
                0
            }
        }
        Cmd::Serve { port, ui } => {
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rewind-core"));
            let (Some(ui_dir), Some(presets)) = (find_ui(ui), find_presets(None)) else {
                eprintln!("error: 找不到 UI 或 presets 目录,用 --ui 与 REWIND_PRESETS 指定");
                return std::process::exit(1);
            };
            match serve::serve(ui_dir, port, exe, presets) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Cmd::Describe => {
            println!("{}", serde_json::to_string_pretty(&describe::manifest()).unwrap());
            0
        }
        Cmd::Catalog { ui, presets } => {
            // 找不到 UI 也要出列表:桌面壳的资源目录位置不固定,那种情况下资产标记全 false
            let Some(presets) = find_presets(presets) else {
                eprintln!("error: 找不到内置预设目录,用 --presets 指定");
                return std::process::exit(1);
            };
            let ui = find_ui(ui).unwrap_or_else(|| PathBuf::from("/nonexistent"));
            let list = catalog::entries(&presets, &catalog::user_dir(), &ui);
            println!("{}", serde_json::to_string_pretty(&list).unwrap());
            0
        }
        Cmd::Plan { preset, input, intensity, overrides } => {
            let p = match preset::Preset::load_with_overrides(&preset, &overrides) {
                Ok(p) => p.scaled(intensity),
                Err(e) => {
                    eprintln!("error: {e}");
                    return std::process::exit(1);
                }
            };
            let media = match ffrun::probe(&input) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    return std::process::exit(1);
                }
            };
            // 字体必须按真实运行时一样传进来:此前固定 font=None,于是带时间戳的预设在 `plan`
            // 里永远看不到 drawtext —— 结构闸对这一整类问题就是瞎的。
            let font = ffrun::timestamp_font();
            if font.is_none() && ffgraph::wants_timestamp(&p) {
                eprintln!("warn: 跳过 overlay_timestamp(没有可用字体,或这个 ffmpeg 构建不含 drawtext 滤镜)");
            }
            match ffgraph::build_plan(&p, &media, None, font.as_deref()) {
                Ok(plan) => {
                    let mut j = ffgraph::plan_json(&plan, &media, p.aging.unwrap_or(1));
                    j["font"] = json!(font);
                    println!("{}", j);
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Cmd::Samples { preset, input, out_dir, count, seed, intensity, overrides } => {
            let out_dir = out_dir.unwrap_or_else(pipeline::preview_cache_dir);
            let media = match ffrun::probe(&input) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    return std::process::exit(1);
                }
            };
            let k = count.clamp(1, 12) as usize;
            // 静帧没有时间轴,只出一对;视频在 5%–95% 之间均匀撒点再抖动,避免全落在片头黑场
            let mut ts: Vec<f64> = if media.is_image() || media.duration <= 1.0 {
                vec![0.0]
            } else {
                let mut rng = pixops::Xorshift::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5DE0);
                let lo = media.duration * 0.05;
                let hi = media.duration * 0.95;
                let span = (hi - lo).max(0.1);
                (0..k)
                    .map(|i| lo + span * ((i as f64 + rng.f()) / k as f64))
                    .collect()
            };
            ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut pairs = Vec::new();
            let mut err: Option<String> = None;
            for t in ts {
                match pipeline::preview(&preset, &input, &out_dir, t, intensity, &overrides) {
                    Ok((src, out)) => {
                        // 抽帧条带要的是缩略图:全帧 PNG 一张 1.3 MB,6 张就是 8 MB
                        let thumb = out.with_file_name(format!(
                            "{}_thumb.png",
                            out.file_stem().and_then(|s| s.to_str()).unwrap_or("x")
                        ));
                        let _ = ffrun::shrink_png(&out, &thumb, 320);
                        let mut p = json!({
                            "t": (t * 10.0).round() / 10.0,
                            "source": src.to_string_lossy(),
                            "result": out.to_string_lossy(),
                        });
                        if thumb.is_file() {
                            p["thumb"] = json!(thumb.to_string_lossy());
                        }
                        pairs.push(p)
                    }
                    Err(e) => {
                        err = Some(e);
                        break;
                    }
                }
            }
            match err {
                None => {
                    println!(
                        "{}",
                        json!({"type":"samples","count":pairs.len(),"seed":seed,
                               "duration":(media.duration * 10.0).round() / 10.0,"is_image":media.is_image(),
                               "pairs":pairs})
                    );
                    0
                }
                Some(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Cmd::PresetRename { file, name } => {
            let raw = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {}", coded(PRESET_LOAD, format!("读 {file:?}: {e}")));
                    return std::process::exit(1);
                }
            };
            let mut v: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {}", coded(PRESET_LOAD, format!("解析 {file:?}: {e}")));
                    return std::process::exit(1);
                }
            };
            let id = file.file_stem().and_then(|s| s.to_str()).unwrap_or("recipe").to_string();
            v["id"] = json!(id);
            v["name"] = json!(if name.trim().is_empty() { id.clone() } else { name.trim().to_string() });
            match write_preset_file(&file, &serde_json::to_string_pretty(&v).unwrap()) {
                Ok(_) => {
                    println!("{}", json!({"type":"renamed","id":id,"path":file.to_string_lossy()}));
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Cmd::SweepTemps { out_dir, tag } => {
            let n = pipeline::sweep_temps(&out_dir, &tag);
            println!("{}", json!({"type":"swept","removed":n,"tag":tag}));
            0
        }
        Cmd::Sample { out_dir, presets } => {
            let dir = find_presets(presets).unwrap_or_else(|| PathBuf::from("presets"));
            match assets::resolve_sample(&dir, &out_dir) {
                Ok(p) => {
                    println!("{}", json!({"type": "sample", "path": p.to_string_lossy()}));
                    0
                }
                Err(e) => {
                    report_err(&e);
                    1
                }
            }
        }
        Cmd::Probe { input } => match ffrun::inspect(&input) {
            Ok(m) => {
                println!("{}", serde_json::to_string_pretty(&m).unwrap());
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Cmd::Presets { dir } => {
            // 目录读不出来要报错并退出,不能 panic:panic 时 stdout 一条事件都没有,
            // Web/桌面拿到的只有 101 退出码,界面上就是"点了没反应"。
            let rd = match std::fs::read_dir(&dir) {
                Ok(rd) => rd,
                Err(e) => {
                    report_err(&coded(PRESET_LOAD, format!("读预设目录 {dir:?}: {e}")));
                    return std::process::exit(1);
                }
            };
            let mut v: Vec<serde_json::Value> = vec![];
            for entry in rd {
                let p = match entry {
                    Ok(e) => e.path(),
                    Err(err) => {
                        eprintln!("warn: {dir:?}: {err}");
                        continue;
                    }
                };
                if p.extension().map(|x| x == "json").unwrap_or(false) {
                    match preset::Preset::load(&p) {
                        Ok(pr) => v.push(serde_json::json!({
                            "id": pr.id, "name": pr.name, "era": pr.era,
                            "pixel": pr.has_pixel_stage(),
                            "params": pr.params_map(),
                        })),
                        Err(err) => eprintln!("warn: {p:?}: {err}"),
                    }
                }
            }
            v.sort_by_key(|x| x["era"].as_u64().unwrap_or(0));
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
            0
        }
    };
    std::process::exit(code);
}
