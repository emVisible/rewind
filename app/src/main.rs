#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Emitter, State};

use rewind_shell::EnginePaths;

struct AppState {
    paths: EnginePaths,
    cancel: Arc<AtomicBool>,
}

#[tauri::command]
fn app_info(state: State<AppState>) -> Result<serde_json::Value, String> {
    let presets = rewind_shell::list_presets(&state.paths, None)?;
    Ok(json!({
        "core": state.paths.core.to_string_lossy(),
        "presetsDir": state.paths.presets_dir.to_string_lossy(),
        "presets": presets,
        "manifest": rewind_shell::manifest(&state.paths)?,
    }))
}

#[tauri::command]
fn cancel_jobs(state: State<AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

/// 年代轴滑杆:生成该年份联动预设,返回预设 id 供前端选中
#[tauri::command]
fn use_era(state: State<AppState>, year: u32) -> Result<String, String> {
    let path = rewind_shell::era_preset_file(&state.paths, year)?;
    Ok(path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default())
}

/// 「再翻录一次」:对某个成品文件追加代际损失
#[tauri::command]
async fn reclip_job(app: AppHandle, index: usize, input: String, times: usize) -> Result<(), String> {
    let state = app.state::<AppState>();
    let paths = state.paths.clone();
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let path = PathBuf::from(input);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let r = rewind_shell::reclip(&paths, &path, times, &mut |ev| {
            let _ = handle.emit("rewind://job", json!({"index": index, "event": ev}));
        }, &cancel);
        match r {
            Ok(out) => {
                let _ = handle.emit(
                    "rewind://item",
                    json!({"index": index, "ok": true, "output": out.to_string_lossy()}),
                );
            }
            Err(e) => {
                let _ = handle.emit("rewind://item", json!({"index": index, "ok": false, "error": e}));
            }
        }
    });
    Ok(())
}

#[tauri::command]
async fn start_jobs(
    app: AppHandle,
    preset: String,
    files: Vec<String>,
    out_dir: String,
    intensity: Option<f64>,
    overrides: Option<Vec<String>>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let paths = state.paths.clone();
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let inputs: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
    // 输出目录留空 = 第一个素材旁 "Rewind输出"
    let out_dir = if out_dir.trim().is_empty() {
        inputs
            .first()
            .and_then(|p| p.parent().map(|d| d.join("Rewind输出")))
            .unwrap_or_else(|| PathBuf::from("Rewind输出"))
    } else {
        PathBuf::from(out_dir)
    };
    let handle = app.clone();
    let overrides = overrides.unwrap_or_default();

    tauri::async_runtime::spawn_blocking(move || {
        let mut on_event = |i: usize, ev: &serde_json::Value| {
            let _ = handle.emit("rewind://job", json!({"index": i, "event": ev}));
        };
        let results = rewind_shell::run_batch(
            &paths,
            &preset,
            &inputs,
            &out_dir,
            intensity.unwrap_or(1.0),
            &overrides,
            &mut on_event,
            &cancel,
        );
        for (i, r) in results.iter().enumerate() {
            match r {
                Ok(out) => {
                    let _ = handle.emit(
                        "rewind://item",
                        json!({"index": i, "ok": true, "output": out.to_string_lossy()}),
                    );
                }
                Err(e) => {
                    let _ = handle.emit("rewind://item", json!({"index": i, "ok": false, "error": e}));
                }
            }
        }
        let _ = handle.emit("rewind://batch", json!({"done": results.len()}));
    });
    Ok(())
}

/// 「换一批」:当前预设种子全体 +1(写入派生目录,不覆盖内置)
#[tauri::command]
fn reroll_preset(state: State<AppState>, preset: String) -> Result<u32, String> {
    rewind_shell::reroll_preset(&state.paths, &preset).map(|n| n as u32)
}

/// 「存为预设」:把派生预设升格成用户预设
#[tauri::command]
fn save_recipe(state: State<AppState>, preset: String, name: String) -> Result<serde_json::Value, String> {
    let id = rewind_shell::save_recipe(&state.paths, &preset, &name)?;
    Ok(json!({"preset": id, "name": name}))
}

/// 对比预览:同管线抽(原帧,做旧帧)对,返回给前端用 asset 协议显示
#[tauri::command]
fn preview_compare(state: State<AppState>, preset: String, input: String, t: f64, intensity: Option<f64>, overrides: Option<Vec<String>>) -> Result<serde_json::Value, String> {
    let ev = rewind_shell::preview_frame(&state.paths, &preset, std::path::Path::new(&input), t, intensity.unwrap_or(1.0), &overrides.unwrap_or_default())?;
    // 原样转发:界面要读 rate(实测吞吐)与 out(交付画幅),少一个字段就少一条读数
    Ok(ev)
}

/// 随机抽帧确认(与 Web 版同一引擎路径)
#[tauri::command]
fn sample_frames(state: State<AppState>, preset: String, input: String, count: Option<u32>, seed: Option<u64>, intensity: Option<f64>, overrides: Option<Vec<String>>) -> Result<serde_json::Value, String> {
    rewind_shell::sample_frames(
        &state.paths,
        &preset,
        std::path::Path::new(&input),
        count.unwrap_or(6),
        seed.unwrap_or(0),
        intensity.unwrap_or(1.0),
        &overrides.unwrap_or_default(),
    )
}

/// 成品直接浏览:把这一个成品文件临时加进 asset 协议白名单(不放宽到目录通配,
/// 否则 webview 就能读任意磁盘路径了),然后回传同一个路径。
#[tauri::command]
fn view_result(app: tauri::AppHandle, path: String) -> Result<String, String> {
    use tauri::Manager;
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("成品不存在: {path}"));
    }
    app.asset_protocol_scope()
        .allow_file(p)
        .map_err(|e| format!("授权成品读取失败: {e}"))?;
    Ok(path)
}

/// 素材完整报告(界面媒体信息条与源自适应取值)
#[tauri::command]
fn probe_file(state: State<AppState>, path: String) -> Result<serde_json::Value, String> {
    rewind_shell::probe_file(&state.paths, std::path::Path::new(&path))
}

/// 示例素材(已复制到临时目录,不会把输出写进安装目录)
#[tauri::command]
fn sample_file(state: State<AppState>) -> Result<serde_json::Value, String> {
    Ok(json!({"path": rewind_shell::sample_file(&state.paths)?.to_string_lossy()}))
}

/// 读/写应用设置(上次预设、输出目录、年代轴位置)
#[tauri::command]
fn get_settings() -> Result<serde_json::Value, String> {
    let s = rewind_shell::settings::load();
    serde_json::to_value(s).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_settings(v: serde_json::Value) -> Result<(), String> {
    let s: rewind_shell::settings::AppSettings = serde_json::from_value(v).map_err(|e| e.to_string())?;
    rewind_shell::settings::save(&s)
}

fn main() {
    let paths = EnginePaths::resolve().expect("找不到 rewind-core,先构建 core/ 或设置 REWIND_CORE");
    tauri::Builder::default()
        .manage(AppState { paths, cancel: Arc::new(AtomicBool::new(false)) })
        .invoke_handler(tauri::generate_handler![app_info, start_jobs, cancel_jobs, use_era, reclip_job, get_settings, set_settings, reroll_preset, save_recipe, preview_compare, sample_frames, view_result, probe_file, sample_file])
        .run(tauri::generate_context!())
        .expect("Rewind 启动失败");
}
