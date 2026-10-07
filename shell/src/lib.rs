//! rewind-shell —— GUI 无关的壳层逻辑:引擎路径解析、sidecar 子进程、NDJSON 事件协议、批量队列。
//! Tauri/其他壳都只是本 crate 的适配器;本 crate 可在无 GUI 依赖环境下编译与测试。

use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};

/// 壳层的失败文本一律带稳定码,形状与引擎一致:`[码] 中文原文`。
/// 界面上 `msg()` → `I18N.decodeError` 靠这个码换英文句子;没有码,英文界面就只能整句中文。
///
/// 三个 crate 相互独立(壳不许依赖引擎源码),所以码值在这里手抄一份 ——
/// `tests::shell_codes_exist_in_engine_table` 逐条对账 `core/src/errcode.rs`,抄漂一次就红。
const ENGINE_SPAWN: &str = "engine.spawn";
const ENGINE_EXIT: &str = "engine.exit";
const ENGINE_OUTPUT: &str = "engine.output";
const MEDIA_UNREADABLE: &str = "media.unreadable";
const ASSET_MISSING: &str = "asset.missing";
const PRESET_SAVE: &str = "preset.save";
const RUN_CANCELED: &str = "run.canceled";
const SHELL_CODES: &[&str] = &[
    ENGINE_SPAWN,
    ENGINE_EXIT,
    ENGINE_OUTPUT,
    MEDIA_UNREADABLE,
    ASSET_MISSING,
    PRESET_SAVE,
    RUN_CANCELED,
];

fn coded(code: &str, msg: impl std::fmt::Display) -> String {
    format!("[{code}] {msg}")
}

/// 引擎非零退出:能透传它自己写的 `[码] 原文` 就透传,一句都没才由壳层代它说"没交代原因" ——
/// 从前这里是 `Err(stderr)` 原样返回,引擎被信号杀掉时拿到空串,界面弹出一条**没有原因**的失败。
fn stderr_or(out: &std::process::Output, code: &str) -> String {
    let s = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if s.is_empty() {
        coded(code, "引擎退出但没写出任何原因")
    } else {
        s
    }
}

/// 引擎与资源位置解析:环境变量优先,其次 exe 同级目录(安装态),再次源码工作区布局(dev)
#[derive(Debug, Clone)]
pub struct EnginePaths {
    pub core: PathBuf,
    pub presets_dir: PathBuf,
    /// 用户预设目录(年代轴生成的预设、另存预设都落这里,不污染内置库)
    pub user_presets_dir: PathBuf,
    pub ffmpeg_dir: Option<PathBuf>,
}

impl EnginePaths {
    /// 解析顺序:环境变量 > 安装态(exe 同级 engines/ 目录,tauri bundle.resources)> 源码工作区(dev)
    pub fn resolve() -> Result<Self, String> {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        let engines = exe_dir.as_ref().map(|d| d.join("engines"));
        let exe_name = if cfg!(windows) { "rewind-core.exe" } else { "rewind-core" };

        let mut candidates: Vec<PathBuf> = vec![];
        if let Some(p) = env_path("REWIND_CORE") {
            candidates.push(p);
        }
        if let Some(e) = &engines {
            candidates.push(e.join(exe_name));
        }
        candidates.push(
            workspace_root()?
                .join("core")
                .join("target")
                .join("release")
                .join(exe_name),
        );
        let core = candidates
            .into_iter()
            .find(|p| p.exists())
            .ok_or(coded(ENGINE_SPAWN, "找不到 rewind-core 二进制(设 REWIND_CORE 或先构建 core/)"))?;

        let presets_dir = env_path("REWIND_PRESETS").unwrap_or_else(|| {
            exe_dir
                .as_ref()
                .map(|d| d.join("presets"))
                .filter(|p| p.exists())
                .unwrap_or_else(|| workspace_root().map(|r| r.join("presets")).unwrap_or_default())
        });
        let user_presets_dir = env_path("REWIND_USER_PRESETS").unwrap_or_else(default_user_presets);
        let ffmpeg_dir = env_path("REWIND_FFMPEG_DIR").or_else(|| {
            engines.clone().filter(|e| {
                let f = if cfg!(windows) { e.join("ffmpeg.exe") } else { e.join("ffmpeg") };
                f.exists()
            })
        });
        Ok(Self { core, presets_dir, user_presets_dir, ffmpeg_dir })
    }

    /// 供子进程用的 ffmpeg 环境(把 engines/<platform> 注入 PATH 前置)
    pub fn apply_env(&self, cmd: &mut Command) {
        if let Some(dir) = &self.ffmpeg_dir {
            let sep = if cfg!(windows) { ';' } else { ':' };
            let joined = match std::env::var("PATH") {
                Ok(p) => format!("{}{sep}{p}", dir.display()),
                Err(_) => dir.to_string_lossy().into_owned(),
            };
            cmd.env("PATH", joined);
        }
    }
}

fn env_path(k: &str) -> Option<PathBuf> {
    std::env::var_os(k).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

fn default_user_presets() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|d| d.join("Rewind").join("presets"))
        .unwrap_or_else(|| PathBuf::from("RewindPresets"))
}

/// shell crate 所在目录的父级 = 工作区根(dev 布局:core/ shell/ presets/ 同级)
fn workspace_root() -> Result<PathBuf, String> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or(coded(ENGINE_SPAWN, "找不到工作区根(core/ shell/ presets/ 的同级布局不对)"))?
        .to_path_buf())
}

/// 一次 rewind-core 运行,逐行回调 NDJSON 事件;cancel 标志置位后杀进程树
pub fn run_core(
    paths: &EnginePaths,
    args: &[String],
    on_event: &mut dyn FnMut(&serde_json::Value),
    cancel: &AtomicBool,
) -> Result<Vec<serde_json::Value>, String> {
    let mut cmd = Command::new(&paths.core);
    cmd.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    paths.apply_env(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| coded(ENGINE_SPAWN, format!("启动 rewind-core 失败: {e}")))?;
    let stdout = child.stdout.take().unwrap();
    let mut events = vec![];
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            on_event(&v);
            events.push(v);
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break;
        }
    }
    let st = child.wait().map_err(|e| e.to_string())?;
    if !st.success() && !cancel.load(Ordering::Relaxed) {
        return Err(coded(ENGINE_EXIT, format!("rewind-core 退出码 {:?}", st.code())));
    }
    Ok(events)
}

/// 预设列表:引擎的 `catalog` 子命令是唯一实现。
/// 此前这里与 `serve.rs` 各写了一份"合并 + 去重 + 排序",两份必然漂移(D2)。
/// `ui` 给定时按该目录探测对比资产存在性;桌面壳的资源目录与安装态有关,所以允许为空。
pub fn list_presets(paths: &EnginePaths, ui: Option<&Path>) -> Result<Vec<serde_json::Value>, String> {
    let mut args: Vec<String> = vec!["catalog".into(), "--presets".into(), paths.presets_dir.to_string_lossy().into_owned()];
    if let Some(u) = ui {
        args.push("--ui".into());
        args.push(u.to_string_lossy().into_owned());
    }
    let out = Command::new(&paths.core)
        .args(&args)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("启动 rewind-core 失败: {e}")))?;
    if !out.status.success() {
        return Err(stderr_or(&out, ENGINE_EXIT));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| coded(ENGINE_OUTPUT, format!("预设目录解析失败: {e}")))?;
    Ok(v.as_array().cloned().unwrap_or_default())
}

/// 年代轴:让 core 生成该年份的联动预设(写入用户预设目录),返回文件路径
/// 素材完整报告(界面媒体信息条与源自适应取值都靠它)
pub fn probe_file(paths: &EnginePaths, input: &Path) -> Result<serde_json::Value, String> {
    let out = Command::new(&paths.core)
        .arg("probe")
        .arg(input)
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("启动引擎失败: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { coded(MEDIA_UNREADABLE, "素材读不了") } else { err });
    }
    serde_json::from_slice(&out.stdout).map_err(|e| coded(ENGINE_OUTPUT, format!("素材报告解析失败: {e}")))
}

/// 示例素材:让 core 解析并复制一份到临时目录 —— 直接返回仓库路径会让默认输出目录落在仓库里
/// (实测污染过 fixtures/)。
///
/// 规则只在 `core::assets` 有一份,这里走子命令。以前两边各写一遍,换素材时 Web 版
/// 还在拿旧的 ffmpeg 测试图,桌面版已经换掉了 —— 同一件事两个答案。
pub fn sample_file(paths: &EnginePaths) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("rewind_samples");
    let out = Command::new(&paths.core)
        .args(["sample", "--out-dir"])
        .arg(&dir)
        .arg("--presets")
        .arg(&paths.presets_dir)
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("调用示例素材通道失败: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { coded(ASSET_MISSING, "没有内置示例素材") } else { err });
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| coded(ENGINE_OUTPUT, format!("示例素材响应解析失败: {e}")))?;
    v["path"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| coded(ENGINE_OUTPUT, "示例素材响应缺 path"))
}

/// 参数清单:界面按它渲染控件(§13.1)。引擎是唯一事实来源,壳与 Web 版拿同一份。
pub fn manifest(paths: &EnginePaths) -> Result<serde_json::Value, String> {
    let out = Command::new(&paths.core)
        .arg("describe")
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("启动引擎失败: {e}")))?;
    if !out.status.success() {
        return Err(coded(ENGINE_EXIT, "rewind-core describe 失败"));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| coded(ENGINE_OUTPUT, format!("参数清单解析失败: {e}")))
}

/// 派生预设(年代轴生成、换一批生成)的落点:用户目录下的 derived/ 子目录,**不进画廊**
fn derived_dir(paths: &EnginePaths) -> PathBuf {
    paths.user_presets_dir.join("derived")
}

pub fn era_preset_file(paths: &EnginePaths, year: u32) -> Result<PathBuf, String> {
    std::fs::create_dir_all(derived_dir(paths))
        .map_err(|e| coded(PRESET_SAVE, format!("创建派生预设目录失败: {e}")))?;
    let path = derived_dir(paths).join(format!("era_{year}.json"));
    let out = Command::new(&paths.core)
        .args(["era", &year.to_string(), "--write", &path.to_string_lossy()])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("启动 rewind-core 失败: {e}")))?;
    if !out.status.success() {
        return Err(stderr_or(&out, ENGINE_EXIT));
    }
    Ok(path)
}

/// 「再翻录一次」:对成品再走 N 轮 mpeg4 代际损失
pub fn reclip(
    paths: &EnginePaths,
    input: &Path,
    times: usize,
    on_event: &mut dyn FnMut(&serde_json::Value),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let args = vec![
        "reclip".into(),
        "--input".into(),
        input.display().to_string(),
        "--times".into(),
        times.to_string(),
    ];
    let events = run_core(paths, &args, on_event, cancel)?;
    events
        .iter()
        .find(|e| e["type"] == "done")
        .and_then(|e| e["output"].as_str())
        .map(PathBuf::from)
        .ok_or_else(|| coded(ENGINE_EXIT, "再翻录跑完了却没报 done(成品路径未知)"))
}

/// 预设解析:用户保存的 → 派生(换一批/年代轴生成) → 内置。
/// "存为预设"因此能直接盖掉派生品,而派生品又不会顶替出厂预设。
pub fn preset_path(paths: &EnginePaths, preset: &str) -> PathBuf {
    for cand in [
        paths.user_presets_dir.join(format!("{preset}.json")),
        derived_dir(paths).join(format!("{preset}.json")),
    ] {
        if cand.exists() {
            return cand;
        }
    }
    paths.presets_dir.join(format!("{preset}.json"))
}

/// 应用设置持久化(模式参考 ALH Pro 的 JSON 设置,路径为平台配置目录)
pub mod settings {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default)]
    pub struct AppSettings {
        #[serde(default)]
        pub last_preset: Option<String>,
        #[serde(default)]
        pub last_out_dir: Option<String>,
        /// 年代轴滑杆位置
        #[serde(default)]
        pub era_year: Option<u32>,
        /// 全局做旧强度 0.2-2.0
        #[serde(default)]
        pub intensity: Option<f64>,
        /// 做旧系数(几手)与"用户是否手动调过"(没调过就跟预设自带的手数走)
        #[serde(default)]
        pub aging: Option<u32>,
        #[serde(default)]
        pub aging_touched: Option<bool>,
        /// 偏色(绿)强度 0–1,默认 0 = 不偏色
        #[serde(default)]
        pub cast: Option<f64>,
        /// 高级参数整栏是否展开(首屏默认收起)
        #[serde(default)]
        pub pro_open: Option<bool>,
        /// 界面语言:"zh" | "en"。Web 版是裸 JSON 透传,桌面版走这个结构体 ——
        /// 少一个字段就是"桌面版换语言重启后变回去",所以必须在这里。
        #[serde(default)]
        pub lang: Option<String>,
        /// 界面参数覆盖(`"stage.key" -> 值`),§13.1:记住用户钉住的参数
        #[serde(default)]
        pub overrides: Option<std::collections::HashMap<String, String>>,
        /// 收藏的预设 id(画廊排最前)
        #[serde(default)]
        pub favorites: Option<Vec<String>>,
        /// 最近用过的预设 id(最多 4 个)
        #[serde(default)]
        pub recents: Option<Vec<String>>,
    }

    pub fn settings_path() -> PathBuf {
        let base = std::env::var_os("REWIND_CONFIG").map(PathBuf::from).or_else(|| {
            if cfg!(windows) {
                std::env::var_os("APPDATA").map(PathBuf::from)
            } else {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            }
        });
        base.map(|d| d.join("Rewind").join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("RewindSettings.json"))
    }

    /// 读设置;文件损坏按默认值处理(只警告不炸)
    pub fn load() -> AppSettings {
        let path = settings_path();
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                eprintln!("warn: 设置文件损坏({e}),使用默认值");
                Default::default()
            }),
            Err(_) => Default::default(),
        }
    }

    pub fn save(s: &AppSettings) -> Result<(), String> {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
        }
        let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
        // 原子写:先写带进程标签的临时文件再改名(引擎里同一件事同一个解法)。
        // 共享 `settings.json.tmp` 时,两个窗口同时保存会互相顶掉:A 改名把 B 的临时文件拿走,
        // B 再改名就报"设置改名失败"。
        let tag = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        );
        let tmp = path.with_extension(format!("json.{tag}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| format!("写设置失败: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("设置改名失败: {e}"))?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn settings_roundtrip_atomic() {
            let dir = std::env::temp_dir().join(format!("rewind_cfg_test_{}", std::process::id()));
            // edition 2024:set_var 需 unsafe;本测试单线程且只动自己进程的环境
            unsafe { std::env::set_var("REWIND_CONFIG", &dir) };
            let mut ov = std::collections::HashMap::new();
            ov.insert("fps.fps".to_string(), "12.5".to_string());
            ov.insert("resize.dar".to_string(), "4:3".to_string());
            let s = AppSettings {
                last_preset: Some("era_1988".into()),
                last_out_dir: Some("/x/out".into()),
                era_year: Some(1988),
                intensity: Some(1.4),
                overrides: Some(ov),
                favorites: Some(vec!["vhs1990_ntscrs".into()]),
                recents: Some(vec!["cctv2000".into(), "dvd2005".into()]),
                aging: Some(3),
                aging_touched: Some(true),
                cast: Some(0.4),
                pro_open: Some(true),
                lang: Some("en".into()),
            };
            save(&s).unwrap();
            assert_eq!(load(), s);
            // 语言必须能存住:桌面版是走这个结构体的,少字段就是"重启后语言被吞掉"
            assert_eq!(load().lang.as_deref(), Some("en"));
            let back = load();
            let got = back.overrides.expect("覆盖应当被持久化");
            assert_eq!(got.get("fps.fps").map(String::as_str), Some("12.5"));
            assert_eq!(got.get("resize.dar").map(String::as_str), Some("4:3"));
            // 损坏文件 → 默认值而非 panic
            std::fs::write(settings_path(), "{oops").unwrap();
            assert_eq!(load(), AppSettings::default());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}

/// 「存为预设」:把当前生效的预设(可能来自 derived/)复制成**用户预设**并改名。
/// 升格后它优先于派生品与内置(D1 的三层优先级)。
pub fn save_recipe(paths: &EnginePaths, preset: &str, name: &str) -> Result<String, String> {
    let src = preset_path(paths, preset);
    if !src.exists() {
        return Err(format!("预设不存在: {preset}"));
    }
    let dst_id = format!("my_{}", preset.strip_prefix("era_").unwrap_or(preset));
    std::fs::create_dir_all(&paths.user_presets_dir)
        .map_err(|e| format!("创建用户预设目录失败: {e}"))?;
    let dst = paths.user_presets_dir.join(format!("{dst_id}.json"));
    std::fs::copy(&src, &dst).map_err(|e| format!("复制预设失败: {e}"))?;
    let out = Command::new(&paths.core)
        .args(["preset-rename", &dst.to_string_lossy(), name])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("启动 rewind-core 失败: {e}")))?;
    if !out.status.success() {
        return Err(stderr_or(&out, ENGINE_EXIT));
    }
    Ok(dst_id)
}

/// 「换一批」:对指定预设 bump 种子,写入**派生目录**(不进画廊,也不覆盖内置同名预设)
pub fn reroll_preset(paths: &EnginePaths, preset: &str) -> Result<usize, String> {
    std::fs::create_dir_all(derived_dir(paths))
        .map_err(|e| coded(PRESET_SAVE, format!("创建派生预设目录失败: {e}")))?;
    let src = preset_path(paths, preset);
    if !src.exists() {
        return Err(format!("预设不存在: {preset}"));
    }
    let dst = derived_dir(paths).join(format!("{preset}.json"));
    let out = Command::new(&paths.core)
        .args(["reroll", &src.to_string_lossy(), "--write", &dst.to_string_lossy()])
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("启动 rewind-core 失败: {e}")))?;
    if !out.status.success() {
        return Err(stderr_or(&out, ENGINE_EXIT));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("reroll 输出解析失败: {e}"))?;
    Ok(v["seeds"].as_u64().unwrap_or(0) as usize)
}

/// 对比预览:返回(原帧,做旧帧)PNG 路径。预览必须与成品吃同一套覆盖,否则"预览即成品"不成立。
pub fn preview_frame(paths: &EnginePaths, preset: &str, input: &Path, t: f64, intensity: f64, overrides: &[String]) -> Result<serde_json::Value, String> {
    let preset_file = preset_path(paths, preset);
    if !preset_file.exists() {
        return Err(format!("预设不存在: {preset}"));
    }
    let mut args = vec![
        "preview".into(),
        "--preset".into(),
        preset_file.display().to_string(),
        "--input".into(),
        input.display().to_string(),
        "--t".into(),
        t.to_string(),
        "--intensity".into(),
        intensity.to_string(),
    ];
    for o in overrides {
        args.push("--override".into());
        args.push(o.clone());
    }
    let cancel = AtomicBool::new(false);
    let events = run_core(paths, &args, &mut |_| {}, &cancel)?;
    let ev = events.iter().find(|e| e["type"] == "preview").ok_or("preview 无事件输出")?;
    // 整条事件原样带回。以前只取两个路径,于是 rate(实测吞吐)与 out(交付画幅)
    // 在桌面版永远是空 —— 同一件事两种返回形状,读数就只在网页版生效(§8.1-43 同族)
    Ok(ev.clone())
}

/// 随机抽帧确认:K 个时间点的(原帧,成品帧)对。抽帧成本与素材长度无关
/// (实测 30 分钟素材抽 6 帧 4.5 s,单帧 0.73 s),所以 3 小时误传也不会卡死。
pub fn sample_frames(
    paths: &EnginePaths,
    preset: &str,
    input: &Path,
    count: u32,
    seed: u64,
    intensity: f64,
    overrides: &[String],
) -> Result<serde_json::Value, String> {
    let preset_file = preset_path(paths, preset);
    if !preset_file.exists() {
        return Err(format!("预设不存在: {preset}"));
    }
    let mut args = vec![
        "samples".into(),
        "--preset".into(),
        preset_file.display().to_string(),
        "--input".into(),
        input.display().to_string(),
        "--count".into(),
        count.to_string(),
        "--seed".into(),
        seed.to_string(),
        "--intensity".into(),
        intensity.to_string(),
    ];
    for o in overrides {
        args.push("--override".into());
        args.push(o.clone());
    }
    let cancel = AtomicBool::new(false);
    let events = run_core(paths, &args, &mut |_| {}, &cancel)?;
    events
        .into_iter()
        .find(|e| e["type"] == "samples")
        .ok_or_else(|| "samples 无事件输出".into())
}

/// 批量队列:顺序执行,聚合每个文件的事件流;失败不中断后续
pub fn run_batch(
    paths: &EnginePaths,
    preset: &str,
    inputs: &[PathBuf],
    out_dir: &PathBuf,
    intensity: f64,
    overrides: &[String],
    on_event: &mut dyn FnMut(usize, &serde_json::Value),
    cancel: &AtomicBool,
) -> Vec<Result<PathBuf, String>> {
    let preset_path = preset_path(paths, preset);
    let mut results = vec![];
    for (i, input) in inputs.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            results.push(Err(coded(RUN_CANCELED, "已取消")));
            continue;
        }
        let mut args = vec![
            "run".into(),
            "--preset".into(),
            preset_path.display().to_string(),
            "--input".into(),
            input.display().to_string(),
            "--out-dir".into(),
            out_dir.display().to_string(),
            "--intensity".into(),
            intensity.to_string(),
        ];
        for o in overrides {
            args.push("--override".into());
            args.push(o.clone());
        }
        let mut cb = |ev: &serde_json::Value| on_event(i, ev);
        let ran = run_core(paths, &args, &mut cb, cancel);
        // 取消走的是 kill,引擎来不及收尾:凭 start 事件里的运行标签把 `.tmp.<tag>.*` 清掉。
        // 实测(serve 侧同一条缺陷):一次取消在用户输出目录留下 2 个隐藏 mp4 / 5 MB。
        if cancel.load(Ordering::Relaxed) {
            if let Ok(evs) = ran.as_ref() {
                if let Some(tag) = evs.iter().find(|e| e["type"] == "start").and_then(|e| e["tag"].as_str()) {
                    let _ = Command::new(&paths.core)
                        .args(["sweep-temps", "--out-dir", &out_dir.display().to_string(), "--tag", tag])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
            }
        }
        let r = match ran {
            Ok(events) => events
                .iter()
                .find(|e| e["type"] == "done")
                .and_then(|e| e["output"].as_str())
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(|| Err(coded(ENGINE_EXIT, "跑完没报 done 事件(成品路径未知)"))),
            Err(e) => Err(e),
        };
        results.push(r);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 壳层手抄的码必须都是引擎码表的成员:常量存在 **且** 进了 `CODES` 数组。
    /// 少一半都不算数 —— 只查常量名的话,一个定义了却没进表的码,界面词典闸也看不见。
    #[test]
    fn shell_codes_exist_in_engine_table() {
        let root = workspace_root().expect("找不到工作区根目录");
        let table = std::fs::read_to_string(root.join("core/src/errcode.rs"))
            .expect("读不到引擎码表 core/src/errcode.rs");
        let (defs, arr) = table.split_once("pub const CODES").expect("码表里没有 CODES 数组");
        // 不能按 ']' 切:`pub const CODES: &[&str] = &[` 里的类型 `&[&str]` 自带一个 ']',那样只切出 8 个字符
        // —— 这条断言就会在"什么都没读到"的情况下继续往下跑。数组以 "];" 收尾,按它切才拿得到整份成员表。
        let body = arr.splitn(2, "];").next().unwrap_or("");
        assert!(body.len() > 40, "CODES 数组没读到,这条断言在空转(长度 {})", body.len());
        for c in SHELL_CODES {
            let name = c.to_uppercase().replace('.', "_");
            assert!(
                defs.contains(&format!("pub const {name}:")),
                "壳层用了 {c},但引擎里没有常量 {name}"
            );
            assert!(body.contains(&name), "壳层用了 {c},常量也有却没进 CODES 数组");
        }
    }

    /// 壳层不许再产出"没有身份的中文失败":每条错误文本都得过 `coded()` / `stderr_or()`。
    /// 白名单是刻意留档的例外(它们要的新码在引擎侧还没有真实调用点)。
    #[test]
    fn shell_failures_always_carry_a_code() {
        let src = include_str!("lib.rs");
        let src = src.split("#[cfg(test)]").next().unwrap_or(""); // 测试自己的字面量不算
        let allowed = ["创建配置目录失败", "写设置失败", "设置改名失败"];
        let mut bare = vec![];
        for (i, raw) in src.lines().enumerate() {
            let ln = raw.trim();
            if ln.starts_with("//") {
                continue;
            }
            // 触发条件只看"这行有没有一条含汉字的字符串字面量",不看它长得像不像 `Err(` ——
            // 写成 `Err::<(), _>("中文")` 就绕过了 contains("Err("),这条实测漏过一次。
            if !has_cjk_string(ln) || coded_exempt(ln) {
                continue;
            }
            // `.expect(..)` 是程序性断言(走到了就是 bug);`println!/eprintln!` 是终端日志 ——
            // 两者都到不了界面,不要求带码
            if ln.contains(".expect(") || ln.contains("println!") || allowed.iter().any(|a| ln.contains(a)) {
                continue;
            }
            bare.push(format!("{}: {}", i + 1, ln.chars().take(72).collect::<String>()));
        }
        assert!(bare.is_empty(), "这些中文文案没带码,也没走 coded()(英文界面会整句中文):\n{}", bare.join("\n"));
    }

    /// 一行里有没有"含汉字的字符串字面量"(按成对双引号切,奇数段才是字面量)。
    fn has_cjk_string(ln: &str) -> bool {
        ln.split('"')
            .enumerate()
            .any(|(k, seg)| k % 2 == 1 && seg.chars().any(|c| c >= '\u{4e00}' && c <= '\u{9fff}'))
    }

    /// 这条判断为什么这么写:码可以来自 `coded(...)`、`stderr_or(...)`,也可以是**已经带码的**字符串常量
    /// (测试里造的假错误);一行里出现 `[小写.` 就说明它自己就是带码文本。
    fn coded_exempt(ln: &str) -> bool {
        if ln.contains("coded(") || ln.contains("stderr_or(") || ln.contains("err_text_with_code") {
            return true;
        }
        let bytes: Vec<char> = ln.chars().collect();
        for w in bytes.windows(2) {
            if w[0] == '[' && w[1].is_ascii_lowercase() {
                return true;
            }
        }
        false
    }

    fn fixture() -> PathBuf {
        workspace_root().unwrap().join("fixtures/test_src.mp4")
    }

    #[test]
    fn sidecar_protocol_roundtrip() {
        let paths = EnginePaths::resolve().expect("引擎路径");
        let input = fixture();
        assert!(input.exists(), "测试夹具缺失 fixtures/test_src.mp4");
        let out_dir = std::env::temp_dir().join("rewind_shell_test");
        std::fs::create_dir_all(&out_dir).unwrap();
        let cancel = AtomicBool::new(false);
        let mut progress_seen = 0;
        let mut overall: Vec<f64> = vec![];
        let args = vec![
            "run".into(),
            "--preset".into(),
            paths.presets_dir.join("cctv2000.json").display().to_string(),
            "--input".into(),
            input.display().to_string(),
            "--out-dir".into(),
            out_dir.display().to_string(),
            "--preview-secs".into(),
            "2".into(),
        ];
        let events = run_core(
            &paths,
            &args,
            &mut |ev| {
                if ev["type"] == "progress" {
                    progress_seen += 1;
                    if let Some(o) = ev["overall"].as_f64() {
                        overall.push(o);
                    }
                }
            },
            &cancel,
        )
        .unwrap();
        assert!(events.iter().any(|e| e["type"] == "start"));
        assert!(progress_seen >= 3, "进度事件过少: {progress_seen}");
        // 界面画的是 overall(总体读数):它必须每条都有、绝不后退、收尾到 100。
        // 少了这一条,引擎退回"每趟各自 0→100"的老行为时界面会静默地把条扫好几遍。
        assert_eq!(overall.len(), progress_seen, "有 progress 事件没带 overall");
        let back = overall.windows(2).filter(|w| w[1] < w[0]).count();
        assert_eq!(back, 0, "总体进度出现了 {back} 次后退");
        assert!(*overall.last().unwrap() >= 99.0, "收尾读数没到 100:{}", overall.last().unwrap());
        let done = events.iter().find(|e| e["type"] == "done").expect("done 事件");
        let out = PathBuf::from(done["output"].as_str().unwrap());
        assert!(out.exists() && out.metadata().unwrap().len() > 1000, "输出异常: {out:?}");
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    /// 取消机制:置位后队列不再起新任务
    #[test]
    fn cancel_prevents_queue_items() {
        let paths = EnginePaths::resolve().unwrap();
        let cancel = AtomicBool::new(true);
        let out_dir = std::env::temp_dir().join("rewind_cancel_test");
        let inputs = vec![fixture(), fixture()];
        let results = run_batch(&paths, "cctv2000", &inputs, &out_dir, 1.0, &[], &mut |_, _| {}, &cancel);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| matches!(r, Err(e) if e == &coded(RUN_CANCELED, "已取消"))), "{results:?}");
    }

    /// 取消机制:首项进度中置位,后续项必须被跳过
    #[test]
    fn cancel_mid_flight_skips_remaining() {
        let paths = EnginePaths::resolve().unwrap();
        let cancel = AtomicBool::new(false);
        let out_dir = std::env::temp_dir().join("rewind_cancel_mid_test");
        let inputs = vec![fixture(), fixture()];
        let results = run_batch(
            &paths,
            "cctv2000",
            &inputs,
            &out_dir,
            1.0,
            &[],
            &mut |_i, ev| {
                if ev["type"] == "progress" {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            &cancel,
        );
        assert!(
            matches!(results[1], Err(ref e) if e == &coded(RUN_CANCELED, "已取消")),
            "第二项应被取消: {:?}",
            results[1]
        );
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn presets_listing_sorted_by_era() {
        let paths = EnginePaths::resolve().unwrap();
        let list = list_presets(&paths, None).unwrap();
        assert!(list.len() >= 5);
        let eras: Vec<u32> = list.iter().map(|p| p["era"].as_u64().unwrap() as u32).collect();
        assert!(eras.windows(2).all(|w| w[0] <= w[1]), "未按年代排序: {eras:?}");
    }

    /// 防 CLI 参数漂移:era/reclip 的 sidecar 调用必须真实可跑
    #[test]
    fn era_and_reclip_sidecars() {
        let paths = EnginePaths::resolve().unwrap();
        let p = era_preset_file(&paths, 1988).unwrap();
        assert!(p.exists(), "era 预设未生成: {p:?}");
        let cancel = AtomicBool::new(false);
        let out = reclip(&paths, &fixture(), 1, &mut |_| {}, &cancel).unwrap();
        assert!(out.exists() && out.metadata().unwrap().len() > 1000, "reclip 输出异常: {out:?}");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn preview_pair_generated() {
        let paths = EnginePaths::resolve().unwrap();
        let ev = preview_frame(&paths, "cctv2000", &fixture(), 1.0, 1.0, &[]).unwrap();
        let src = PathBuf::from(ev["source"].as_str().unwrap());
        let out = PathBuf::from(ev["result"].as_str().unwrap());
        assert!(src.exists() && src.metadata().unwrap().len() > 5000, "原帧异常: {src:?}");
        assert!(out.exists() && out.metadata().unwrap().len() > 5000, "做旧帧异常: {out:?}");
        assert_ne!(src, out);
        // 桌面版曾经只拿两个路径,于是 rate 与 out 永远是空 —— 预估和画幅读数就只在网页版生效。
        // rate 只在**真的跑过**时才有:命中参数指纹缓存时没有新测量,那是设计而非缺字段
        assert!(
            ev["rate"].is_number() || ev["cached"].as_bool() == Some(true),
            "事件里既没有 rate 也没说命中缓存: {ev}"
        );
        assert_eq!((ev["out"]["w"].as_u64(), ev["out"]["h"].as_u64()), (Some(640), Some(480)), "交付画幅不对: {ev}");
        assert_eq!(ev["out"]["w"], ev["out"]["display_w"], "预览两层没按同一几何折算: {ev}");
    }

    /// 防 CLI 参数漂移:界面覆盖(§13.1)与参数清单必须真的走通到引擎
    #[test]
    fn manifest_and_overrides_reach_the_engine() {
        let paths = EnginePaths::resolve().unwrap();
        let m = manifest(&paths).unwrap();
        let stages = m["video"].as_array().unwrap().len() + m["audio"].as_array().unwrap().len();
        assert!(stages >= 20, "参数清单覆盖的 stage 太少: {stages}");
        // 一级层是"日常参数"的产品决定,按 id 钉死而不是数个数:
        // 画幅与制式 / 帧率与节奏 / 清晰度与带宽 必须在;代际与编码已按决定移入高级参数
        let ids: Vec<&str> = m["controls"].as_array().unwrap().iter()
            .filter_map(|c| c["id"].as_str()).collect();
        for want in ["aspect", "tempo", "clarity"] {
            assert!(ids.contains(&want), "一级控件少了 {want}: {ids:?}");
        }
        assert!(!ids.contains(&"generation"), "「代际与编码」不该回到一级层: {ids:?}");
        let fps = m["video"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["stage"] == "fps")
            .expect("清单里应有 fps");
        assert_eq!(fps["params"].as_array().unwrap()[0]["key"].as_str(), Some("fps"));

        let cancel = AtomicBool::new(false);
        let out_dir = std::env::temp_dir().join("rewind_ovr_test");
        let r = run_batch(
            &paths,
            "cctv2000",
            &[fixture()],
            &out_dir,
            1.0,
            &["fps.fps=6".into(), "resize.range=pc".into()],
            &mut |_, _| {},
            &cancel,
        );
        let out = r[0].as_ref().unwrap_or_else(|e| panic!("覆盖跑失败: {e}"));
        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=nb_frames,avg_frame_rate,color_range", "-of", "csv=p=0"])
            .arg(out)
            .output()
            .expect("ffprobe 应可运行");
        let txt = String::from_utf8_lossy(&probe.stdout).replace('\n', " ");
        assert!(txt.contains("6/1"), "覆盖的帧率没落到输出: {txt}");
        assert!(txt.contains("pc"), "覆盖的量化范围没打标签: {txt}");
        // 拼错的参数名必须让任务失败,而不是静默按原预设跑
        let bad = run_batch(&paths, "cctv2000", &[fixture()], &out_dir, 1.0, &["fps.sped=6".into()], &mut |_, _| {}, &cancel);
        assert!(bad[0].is_err(), "非法参数名不该被接受");
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    /// 界面媒体信息条与"试试示例素材"两条 sidecar 通道
    #[test]
    fn probe_and_sample_sidecars() {
        let paths = EnginePaths::resolve().unwrap();
        let s = sample_file(&paths).expect("仓库自带示例素材");
        assert!(s.is_file(), "示例素材没复制出来: {s:?}");
        // 示例必须是真实画面素材,不是 ffmpeg 测试图 —— 色块上看不出任何做旧效果
        assert_eq!(s.file_name().and_then(|n| n.to_str()), Some("street-food.mp4"), "示例素材该是原创真实画面: {s:?}");
        let r = probe_file(&paths, &s).unwrap();
        assert_eq!(r["width"].as_u64(), Some(1280), "{r}");
        assert_eq!(r["height"].as_u64(), Some(720), "{r}");
        assert!(r["duration"].as_f64().unwrap_or(0.0) >= 6.0, "示例素材太短,抽帧与门槛都测不出来: {r}");
        assert_eq!(r["has_audio"].as_bool(), Some(true), "示例素材得带音轨,否则音频做旧看不见");
        for k in ["dar", "vfr", "video_codec", "container", "size_bytes", "has_audio"] {
            assert!(r.get(k).is_some(), "素材报告缺 {k}");
        }
        assert!(probe_file(&paths, std::path::Path::new("/definitely/not/here.mp4")).is_err());
    }

    #[test]
    fn reroll_lands_in_derived_and_never_shadows_builtin() {
        let paths = EnginePaths::resolve().unwrap();
        let derived = derived_dir(&paths).join("film1970.json");
        let user_top = paths.user_presets_dir.join("film1970.json");
        // 从内置基线出发:派生与用户覆盖都清掉,保证幂等
        let _ = std::fs::remove_file(&derived);
        let _ = std::fs::remove_file(&user_top);
        let n = reroll_preset(&paths, "film1970").unwrap();
        assert!(n >= 1, "film1970 至少应有一个种子");
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&derived).unwrap()).unwrap();
        assert_eq!(v["video"][2]["params"]["seed"], serde_json::json!(12));
        assert!(!user_top.exists(), "换一批不许在用户目录顶层留下顶替内置的文件(D1)");
        assert_eq!(preset_path(&paths, "film1970"), derived, "派生预设要生效");
        // 用户主动保存的同名预设必须盖过派生品("存为预设"的语义)
        std::fs::create_dir_all(&paths.user_presets_dir).unwrap();
        std::fs::copy(&paths.presets_dir.join("film1970.json"), &user_top).unwrap();
        assert_eq!(preset_path(&paths, "film1970"), user_top);
        let _ = std::fs::remove_file(&derived);
        let _ = std::fs::remove_file(&user_top);
    }
}
