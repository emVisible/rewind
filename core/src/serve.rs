//! 本地 Web UI:零外部依赖(纯 std HTTP),复用与桌面壳同一套 UI 文件与同一套引擎。
//! 事件通过轮询 /api/events 转发,前端 shim 把 window.__TAURI__ 换成 fetch。

use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};

use crate::errcode::{
    coded, ENGINE_EXIT, ENGINE_SPAWN, PRESET_SAVE, RUN_CANCELED, UPLOAD_BAD_TYPE, UPLOAD_DECODE,
    UPLOAD_INTERRUPT, UPLOAD_TOO_LARGE,
};
use serde_json::{json, Value};

pub struct Job {
    pub events: Vec<Value>,
    pub done: bool,
    child: Option<Child>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct State {
    jobs: Mutex<HashMap<u64, Job>>,
    next_id: AtomicU64,
    uploads: AtomicU64,
    /// `/file` 白名单:只允许本服务亲手登记过的绝对路径(上传件、成品、预览帧)。
    /// 否则本机任意网页都能借 127.0.0.1 读磁盘上的任何文件。
    files: Mutex<HashSet<String>>,
}

fn allow_file(state: &State, path: &str) {
    if path.is_empty() {
        return;
    }
    if let Ok(p) = std::fs::canonicalize(path) {
        let _ = state.files.lock().unwrap().insert(p.to_string_lossy().into_owned());
    }
}

fn file_allowed(state: &State, path: &str) -> bool {
    match std::fs::canonicalize(path) {
        Ok(p) => {
            let key = p.to_string_lossy().into_owned();
            state.files.lock().unwrap().contains(&key)
        }
        Err(_) => false,
    }
}

/// 浏览器里的 File 对象拿不到磁盘路径,所以 Web 版需要上传端点。
const MAX_UPLOAD: usize = 4 << 30;
/// 与 app.js 的 MEDIA_EXT 保持一致
const MEDIA_EXT: [&str; 20] = [
    "mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "ts", "mpg", "mpeg", "3gp", "png",
    "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff",
];

pub fn serve(ui_dir: PathBuf, port: u16, core_bin: PathBuf, presets_dir: PathBuf) -> Result<(), String> {
    if !ui_dir.join("index.html").exists() {
        return Err(format!("UI 目录不可用: {ui_dir:?}"));
    }
    let state = Arc::new(State::default());
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("端口 {port}: {e}"))?;
    let uploads = upload_dir();
    sweep(&uploads);
    println!(
        "{}",
        json!({"type":"serving","url":format!("http://127.0.0.1:{port}/"),"ui":ui_dir.to_string_lossy(),"uploads":uploads.to_string_lossy()})
    );
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let st = state.clone();
        let ui = ui_dir.clone();
        let bin = core_bin.clone();
        let pre = presets_dir.clone();
        thread::spawn(move || {
            let _ = handle(stream, st, ui, bin, pre);
        });
    }
    #[allow(unreachable_code)]
    Ok(())
}

fn handle(
    mut stream: TcpStream,
    state: Arc<State>,
    ui: PathBuf,
    bin: PathBuf,
    presets: PathBuf,
) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 8192];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > 1 << 16 {
            return json_resp(&mut stream, 431, &json!({"error": "请求头过大"}));
        }
        match stream.read(&mut tmp) {
            Ok(0) | Err(_) => return Ok(()),
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.split_whitespace();
    let method = lines.next().unwrap_or("GET").to_string();
    let target = lines.next().unwrap_or("/").to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    let body_start = head_end + 4;
    let total = content_length(&head);

    if method == "POST" && path == "/upload" {
        return upload(stream, &state, &query, &bin, total, &buf[body_start..]);
    }
    if total > 8 << 20 {
        return json_resp(&mut stream, 413, &json!({"error": "请求体过大"}));
    }

    while buf.len() < body_start + total {
        match stream.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    }
    let body = String::from_utf8_lossy(&buf[body_start..buf.len().min(body_start + total)]).to_string();

    if (path == "/api" || path.starts_with("/api/")) && method == "POST" {
        return api(stream, state, bin, presets, ui, &body);
    }
    if path.starts_with("/file") {
        return file(stream, &state, &path);
    }
    static_(stream, &ui, &path)
}

/// 浏览器上传:原始字节作为请求体(避开 multipart 解析),落到临时目录后回绝对路径。
fn upload(
    mut stream: TcpStream,
    state: &Arc<State>,
    query: &str,
    bin: &PathBuf,
    total: usize,
    prefix: &[u8],
) -> std::io::Result<()> {
    if total == 0 {
        return json_resp(&mut stream, 411, &json!({"error": "缺少 Content-Length"}));
    }
    if total > MAX_UPLOAD {
        return json_resp(
            &mut stream,
            413,
            &json!({"error": coded(UPLOAD_TOO_LARGE, format!("文件过大({} MB),上限 {} MB", total / (1 << 20), MAX_UPLOAD / (1 << 20)))}),
        );
    }
    let name = param(query, "name").map(percent).unwrap_or_default();
    let Some(fname) = safe_name(&name) else {
        return json_resp(&mut stream, 415, &json!({"error": coded(UPLOAD_BAD_TYPE, format!("不支持的文件类型: {name}"))}));
    };
    let dir = upload_dir();
    std::fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = state.uploads.fetch_add(1, Ordering::Relaxed);
    let mut dest = dir.join(format!("{stamp:016x}{seq:04}_{fname}"));
    if dest.exists() {
        dest = dir.join(format!("{stamp:016x}{seq:04}x_{fname}"));
    }
    let mut f = std::fs::File::create(&dest)?;
    f.write_all(prefix)?;
    let left = total.saturating_sub(prefix.len()) as u64;
    let rest = std::io::copy(&mut (&mut stream).take(left), &mut f)?;
    if rest != left {
        drop(f);
        let _ = std::fs::remove_file(&dest);
        return json_resp(&mut stream, 400, &json!({"error": coded(UPLOAD_INTERRUPT, "上传中断")}));
    }
    drop(f);
    // 探一次媒体信息,把损坏/非媒体文件挡在队列外;探测本身不可用则放行
    let probe = Command::new(bin).args(["probe", &dest.to_string_lossy()]).output();
    let Ok(o) = probe else {
        return json_resp(&mut stream, 200, &json!({"path": dest.to_string_lossy(), "bytes": total}));
    };
    if !o.status.success() {
        let why = last_line(&o.stderr);
        let _ = std::fs::remove_file(&dest);
        return json_resp(
            &mut stream,
            415,
            &json!({"error": coded(UPLOAD_DECODE, format!("无法解码 {fname}:{why}"))}),
        );
    }
    allow_file(state, &dest.to_string_lossy());
    json_resp(&mut stream, 200, &json!({"path": dest.to_string_lossy(), "bytes": total}))
}

/// 引擎的报错常在最后一行(且常带尾随空行)
fn last_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim_start_matches("error:")
        .trim()
        .to_string()
}

fn content_length(head: &str) -> usize {    head.lines()
        .skip(1)
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            if k.trim().eq_ignore_ascii_case("content-length") {
                v.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

fn safe_name(raw: &str) -> Option<String> {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == ' ' || c == '.').to_string();
    let (stem, ext) = cleaned.rsplit_once('.')?;
    if stem.trim().is_empty() || !MEDIA_EXT.contains(&ext.to_ascii_lowercase().as_str()) {
        return None;
    }
    Some(cleaned.chars().take(120).collect())
}

fn upload_dir() -> PathBuf {
    std::env::var_os("REWIND_UPLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("rewind_uploads"))
}

/// 上传件落在临时盘,服务启动时清掉一天前的残留(含其 Rewind输出 子目录)
fn sweep(dir: &PathBuf) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let now = std::time::SystemTime::now();
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        let old = m
            .modified()
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(|d| d.as_secs() > 86_400)
            .unwrap_or(false);
        if old {
            if m.is_dir() {
                let _ = std::fs::remove_dir_all(&e.path());
            } else {
                let _ = std::fs::remove_file(&e.path());
            }
        }
    }
}

fn json_resp(stream: &mut TcpStream, status: u16, v: &Value) -> std::io::Result<()> {
    let s = v.to_string();
    // 不发 Access-Control-Allow-Origin:界面与本页同源;开了等于允许任意网页借本机服务读盘
    let head = format!(
        "HTTP/1.1 {status} Response\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        s.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(s.as_bytes())
}

fn mut_json(v: &Value) -> Value {
    v.clone()
}

fn api(
    mut stream: TcpStream,
    state: Arc<State>,
    bin: PathBuf,
    presets: PathBuf,
    ui: PathBuf,
    body: &str,
) -> std::io::Result<()> {
    let req: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
    let cmd = req["cmd"].as_str().unwrap_or("").to_string();
    let args = mut_json(&req["args"]);

    let out: Value = match cmd.as_str() {
        "app_info" => {
            // 三层预设(内置/我的/派生)的枚举只有引擎这一份实现,见 catalog.rs;
            // 界面按 kind 过滤掉派生品,不再让年代轴与换一批的临时产物混进预设画廊。
            let list = crate::catalog::entries(&presets, &state_user_dir(), &ui);
            let with_asset = |field: &str| -> Vec<String> {
                list.iter()
                    .filter(|e| e["gallery"][field] == json!(true))
                    .filter_map(|e| e["id"].as_str().map(str::to_string))
                    .collect()
            };
            json!({
                "presets": list,
                "gallery": with_asset("tile"),
                "gallery_motion": with_asset("motion"),
                "gallery_full": with_asset("full"),
                "gallery_hero": with_asset("hero"),
                "engine": "rewind-core serve",
                // 本机可用字体:没有字体时 drawtext 会跳过,监控/RMVB 的招牌时间戳就不出现。
                // 这句 warn 以前只进 stderr,而 serve 把 stderr 丢了 → 界面永远静默出怪图
                "font": crate::ffrun::find_font(),
                "manifest": crate::describe::manifest()
            })
        }
        "start_jobs" => {
            let preset = args["preset"].as_str().unwrap_or("").to_string();
            let files: Vec<String> = args["files"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let out_dir = args["outDir"].as_str().filter(|s| !s.trim().is_empty()).map(str::to_string);
            let intensity = args["intensity"].as_f64().unwrap_or(1.0);
            let overrides: Vec<String> = args["overrides"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let works: Vec<Work> = files
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let pf = resolve_preset(&presets, &preset).to_string_lossy().into_owned();
                    let dir = out_dir.clone().unwrap_or_else(|| {
                        PathBuf::from(f).parent().map(|x| x.join("Rewind输出")).unwrap_or_else(|| PathBuf::from("Rewind输出")).to_string_lossy().into_owned()
                    });
                    let mut a = vec![
                        "run".into(),
                        "--preset".into(),
                        pf,
                        "--input".into(),
                        f.clone(),
                        "--out-dir".into(),
                        dir,
                        "--intensity".into(),
                        intensity.to_string(),
                    ];
                    for o in &overrides {
                        a.push("--override".into());
                        a.push(o.clone());
                    }
                    Work { index: i, args: a }
                })
                .collect();
            let id = new_job(&state, works.len());
            spawn_work(&state, id, bin.clone(), works);
            json!({"job": id})
        }
        "reclip_job" => {
            let index = args["index"].as_u64().unwrap_or(0) as usize;
            let input = args["input"].as_str().unwrap_or("").to_string();
            let times = args["times"].as_u64().unwrap_or(1);
            let dir = args["outDir"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    PathBuf::from(&input)
                        .parent()
                        .map(|x| x.join("Rewind输出").to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Rewind输出".into())
                });
            let works = vec![Work {
                index,
                args: vec![
                    "reclip".into(),
                    "--input".into(),
                    input,
                    "--times".into(),
                    times.to_string(),
                    "--out-dir".into(),
                    dir,
                ],
            }];
            let id = new_job(&state, 1);
            spawn_work(&state, id, bin.clone(), works);
            json!({"job": id})
        }
        "events" => {
            let id = args["job"].as_u64().unwrap_or(0);
            let since = args["since"].as_u64().unwrap_or(0) as usize;
            let mut jobs = state.jobs.lock().unwrap();
            match jobs.get_mut(&id) {
                Some(j) => {
                    let slice: Vec<Value> = j.events.iter().skip(since).cloned().collect();
                    json!({"events": slice, "next": j.events.len(), "done": j.done})
                }
                None => json!({"error":"no such job"}),
            }
        }
        "cancel_jobs" => {
            for (_, j) in state.jobs.lock().unwrap().iter_mut() {
                j.cancel.store(true, Ordering::Relaxed);
                if let Some(ch) = j.child.as_mut() {
                    let _ = ch.kill();
                }
            }
            json!({"ok": true})
        }
        "get_settings" => std::fs::read(settings_file())
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .unwrap_or_else(|| json!({})),
        "set_settings" => {
            let v = args.get("v").cloned().unwrap_or_else(|| json!({}));
            let path = settings_file();
            let tmp = path.with_extension(format!("json.{}.tmp", crate::pipeline::run_tag()));
            let out = match std::fs::create_dir_all(&path.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")))
                .and_then(|_| std::fs::write(&tmp, serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".into())))
                .and_then(|_| std::fs::rename(&tmp, &path))
            {
                Ok(_) => json!({"ok": true}),
                Err(e) => json!({"error": e.to_string()}),
            };
            out
        }
        "use_era" => {
            let year = args["year"].as_u64().unwrap_or(1990) as u32;
            // 年代轴产物是**派生预设**:落 derived/ 子目录,不进画廊,也不顶替内置预设
            let f = match std::fs::create_dir_all(crate::catalog::derived_path(&state_user_dir(), "x").parent().unwrap())
                .map(|_| crate::catalog::derived_path(&state_user_dir(), &format!("era_{year}")))
            {
                Ok(f) => f,
                Err(e) => return json_resp(&mut stream, 200, &json!({"error": coded(PRESET_SAVE, format!("预设目录: {e}"))})),
            };
            let a = vec!["era".into(), year.to_string(), "--write".into(), f.to_string_lossy().into_owned()];
            match core_events(&bin, &a) {
                Ok(_) => json!({"preset": format!("era_{year}"), "kind": "derived"}),
                Err(e) => json!({"error": e}),
            }
        }
        "reroll_preset" => {
            let id = args["preset"].as_str().unwrap_or("").to_string();
            let src = resolve_preset(&presets, &id)
                .to_string_lossy()
                .into_owned();
            // 换一批的结果同样是派生预设(曾经它直接写 <user>/{id}.json,永久顶替内置预设)
            let dst = match std::fs::create_dir_all(crate::catalog::derived_path(&state_user_dir(), "x").parent().unwrap())
                .map(|_| crate::catalog::derived_path(&state_user_dir(), &id))
            {
                Ok(p) => p.to_string_lossy().into_owned(),
                Err(e) => return json_resp(&mut stream, 200, &json!({"error": coded(PRESET_SAVE, format!("预设目录: {e}"))})),
            };
            match core_events(&bin, &["reroll".into(), src, "--write".into(), dst]) {
                Ok(evs) => json!({
                    "seeds": evs.iter().find(|v| v["type"] == "rerolled").and_then(|v| v["seeds"].as_u64()).unwrap_or(0),
                    "kind": "derived"
                }),
                Err(e) => json!({"error": e}),
            }
        }
        "save_recipe" => {
            // "存为预设":把当前生效的预设(可能来自 derived/)复制成用户预设并改名
            let id = args["preset"].as_str().unwrap_or("").to_string();
            let name = args["name"].as_str().unwrap_or("").trim().to_string();
            let src = resolve_preset(&presets, &id);
            let dst_id = format!("my_{}", id.trim_start_matches("era_"));
            let dst = crate::catalog::user_dir().join(format!("{dst_id}.json"));
            match std::fs::create_dir_all(crate::catalog::user_dir())
                .and_then(|_| std::fs::copy(&src, &dst))
                .map(|_| ())
                .map_err(|e| coded(PRESET_SAVE, format!("保存预设失败: {e}")))
                .and_then(|_| {
                    core_events(&bin, &["preset-rename".into(), dst.to_string_lossy().into_owned(), name.clone()])
                        .map(|_| ())
                }) {
                Ok(_) => json!({"preset": dst_id, "name": name}),
                Err(e) => json!({"error": e}),
            }
        }
        "preview_compare" => {
            let pf = resolve_preset(&presets, args["preset"].as_str().unwrap_or(""));
            let work = crate::pipeline::preview_cache_dir();
            let _ = std::fs::create_dir_all(&work);
            let mut a = vec![
                "preview".into(),
                "--preset".into(),
                pf.to_string_lossy().into_owned(),
                "--input".into(),
                args["input"].as_str().unwrap_or("").to_string(),
                "--out-dir".into(),
                work.to_string_lossy().into_owned(),
                "--t".into(),
                args["t"].as_f64().unwrap_or(1.0).to_string(),
                "--intensity".into(),
                args["intensity"].as_f64().unwrap_or(1.0).to_string(),
            ];
            if let Some(list) = args["overrides"].as_array() {
                for o in list.iter().filter_map(|x| x.as_str()) {
                    a.push("--override".into());
                    a.push(o.to_string());
                }
            }
            match core_events(&bin, &a) {
                Ok(evs) => {
                    let out = evs
                        .into_iter()
                        .find(|v| v["type"] == "preview")
                        .unwrap_or_else(|| json!({"error": coded(ENGINE_EXIT, "preview 无事件输出")}));
                    allow_file(&state, out["source"].as_str().unwrap_or(""));
                    allow_file(&state, out["result"].as_str().unwrap_or(""));
                    out
                }
                Err(e) => json!({"error": e}),
            }
        }
        "sample_frames" => {
            // 随机抽帧确认:先看几帧再决定跑不跑(3 小时素材误传的保护)
            let pf = resolve_preset(&presets, args["preset"].as_str().unwrap_or(""));
            let work = crate::pipeline::preview_cache_dir();
            let _ = std::fs::create_dir_all(&work);
            let mut a = vec![
                "samples".into(),
                "--preset".into(),
                pf.to_string_lossy().into_owned(),
                "--input".into(),
                args["input"].as_str().unwrap_or("").to_string(),
                "--out-dir".into(),
                work.to_string_lossy().into_owned(),
                "--count".into(),
                args["count"].as_u64().unwrap_or(6).to_string(),
                "--seed".into(),
                args["seed"].as_u64().unwrap_or(0).to_string(),
                "--intensity".into(),
                args["intensity"].as_f64().unwrap_or(1.0).to_string(),
            ];
            if let Some(list) = args["overrides"].as_array() {
                for o in list.iter().filter_map(|x| x.as_str()) {
                    a.push("--override".into());
                    a.push(o.to_string());
                }
            }
            match core_events(&bin, &a) {
                Ok(evs) => {
                    let out = evs
                        .into_iter()
                        .find(|v| v["type"] == "samples")
                        .unwrap_or_else(|| json!({"error": coded(ENGINE_EXIT, "samples 无事件输出")}));
                    for p in out["pairs"].as_array().unwrap_or(&vec![]) {
                        allow_file(&state, p["source"].as_str().unwrap_or(""));
                        allow_file(&state, p["result"].as_str().unwrap_or(""));
                        allow_file(&state, p["thumb"].as_str().unwrap_or(""));
                    }
                    out
                }
                Err(e) => json!({"error": e}),
            }
        }
        "probe_file" => {
            let p = args["path"].as_str().unwrap_or("");
            match crate::ffrun::inspect(std::path::Path::new(p)) {
                Ok(v) => v,
                Err(e) => json!({"error": e}),
            }
        }
        "sample_file" => {
            // 解析与复制规则只在 core::assets 里有一份(桌面壳走 CLI 调同一条)
            match crate::assets::resolve_sample(&presets, &upload_dir()) {
                Ok(dst) => {
                    allow_file(&state, &dst.to_string_lossy());
                    json!({"path": dst.to_string_lossy()})
                }
                Err(e) => json!({"error": e}),
            }
        }
        other => json!({"error": format!("未知命令 {other}")}),
    };
    json_resp(&mut stream, 200, &out)
}

fn push(st: &Arc<State>, id: u64, ev: Value) {
    if let Some(j) = st.jobs.lock().unwrap().get_mut(&id) {
        j.events.push(ev);
    }
}

/// 队列里的一项:前端行号 + 交给 rewind-core 的 argv
struct Work {
    index: usize,
    args: Vec<String>,
}

fn new_job(state: &Arc<State>, _total: usize) -> u64 {
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    state.jobs.lock().unwrap().insert(
        id,
        Job { events: vec![], done: false, child: None, cancel: Arc::new(AtomicBool::new(false)) },
    );
    id
}

fn spawn_work(state: &Arc<State>, id: u64, bin: PathBuf, works: Vec<Work>) {
    let st = state.clone();
    thread::spawn(move || {
        let total = works.len();
        for w in works {
            let i = w.index;
            let mut c = Command::new(&bin);
            c.args(&w.args);
            let cancel = { st.jobs.lock().unwrap().get(&id).map(|j| j.cancel.clone()) }
                .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            // stderr 要抽干也要留一份:原先是 Stdio::null(),引擎写在 stderr 上的错误被原地丢掉,
            // 实测结果是失败的那一项连一条 item 事件都没有,界面永远停在"进行中"只能刷新页面。
            let child = match c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
                Ok(ch) => ch,
                Err(e) => {
                    push(&st, id, json!({"type":"item","index":i,"ok":false,"error":e.to_string()}));
                    continue;
                }
            };
            if let Some(mut j) = st.jobs.lock().unwrap().get_mut(&id) {
                j.child = Some(child);
            }
            let (stdout, stderr) = match st
                .jobs
                .lock()
                .unwrap()
                .get_mut(&id)
                .and_then(|j| j.child.as_mut())
                .map(|ch| (ch.stdout.take(), ch.stderr.take()))
            {
                Some((Some(o), e)) => (o, e),
                _ => continue,
            };
            let err_h = match stderr {
                Some(mut s) => thread::spawn(move || {
                    use std::io::Read;
                    let mut buf = Vec::new();
                    let _ = s.read_to_end(&mut buf);
                    buf
                }),
                // 没有 stderr 管道也得有个能 join 的东西,别在下面写两条分支
                None => thread::spawn(|| Vec::<u8>::new()),
            };
            let mut reported = false;
            // 引擎的取消是 kill:它自己没法收尾,所以父进程要凭 start 事件里的运行标签
            // 把 `.tmp.<tag>.` 的中间件清掉(实测一次取消在用户输出目录留了 2 个隐藏 mp4 / 5 MB)
            let mut tag = String::new();
            let out_dir = w
                .args
                .iter()
                .position(|a| a == "--out-dir")
                .and_then(|p| w.args.get(p + 1))
                .cloned()
                .unwrap_or_default();
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    match v["type"].as_str() {
                        Some("start") => tag = v["tag"].as_str().unwrap_or("").to_string(),
                        Some("progress") => push(&st, id, json!({"type":"job","index":i,"event":v})),
                        Some("done") => {
                            reported = true;
                            allow_file(&st, v["output"].as_str().unwrap_or(""));
                            push(&st, id, json!({"type":"item","index":i,"ok":true,"output":v["output"]}));
                        }
                        // 引擎的失败以 error 事件走 stdout(main::report_err),这里必须翻译成 item:
                        // 界面上的这一项只有收到 item 才会收尾,否则就是永远转圈
                        Some("error") => {
                            reported = true;
                            let e = v["error"].as_str().unwrap_or("引擎报错但没有内容").to_string();
                            push(&st, id, json!({"type":"item","index":i,"ok":false,"error":e}));
                        }
                        _ => {}
                    }
                }
                if cancel.load(Ordering::Relaxed) {
                    if let Some(mut g) = st.jobs.lock().unwrap().get_mut(&id) {
                        if let Some(ch) = g.child.as_mut() {
                            let _ = ch.kill();
                        }
                    }
                    break;
                }
            }
            let code = {
                let mut g = st.jobs.lock().unwrap();
                match g.get_mut(&id).and_then(|j| j.child.take()) {
                    Some(mut ch) => ch.wait().ok().and_then(|s| s.code()),
                    None => None,
                }
            };
            if !reported {
                // 兜底:被杀(取消)、panic、外部信号打断时不会有 error 事件,但这一项必须有交代
                let tail = String::from_utf8_lossy(&err_h.join().unwrap_or_default()).trim().to_string();
                if !tag.is_empty() && !out_dir.is_empty() {
                    let _ = Command::new(&bin)
                        .args(["sweep-temps", "--out-dir", &out_dir, "--tag", &tag])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                }
                if cancel.load(Ordering::Relaxed) {
                    push(&st, id, json!({"type":"item","index":i,"ok":false,"canceled":true,"error":coded(RUN_CANCELED, "已取消")}));
                } else {
                    let cut = &tail[tail.len().saturating_sub(400)..];
                    push(
                        &st,
                        id,
                        json!({"type":"item","index":i,"ok":false,
                               "error":coded(ENGINE_EXIT, format!("引擎退出码 {code:?};{cut}"))}),
                    );
                }
            }
        }
        push(&st, id, json!({"type":"batch","done":total}));
        if let Some(mut j) = st.jobs.lock().unwrap().get_mut(&id) {
            j.done = true;
        }
    });
}

/// Web 端复用 CLI 语义:跑一次 rewind-core,拿回它的 NDJSON 事件
fn core_events(bin: &PathBuf, args: &[String]) -> Result<Vec<Value>, String> {
    let o = Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| coded(ENGINE_SPAWN, format!("启动引擎失败: {e}")))?;
    if !o.status.success() {
        let tail = last_line(&o.stderr);
        return Err(if tail.is_empty() {
            coded(ENGINE_EXIT, format!("引擎退出码 {:?}", o.status.code()))
        } else {
            tail
        });
    }
    Ok(String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect())
}

fn state_user_dir() -> PathBuf {
    crate::catalog::user_dir()
}

fn settings_file() -> PathBuf {
    let d = state_user_dir();
    d.parent()
        .map(|p| p.join("web_settings.json"))
        .unwrap_or_else(|| PathBuf::from("RewindWebSettings.json"))
}

fn resolve_preset(dir: &PathBuf, id: &str) -> PathBuf {
    // 用户保存的 → 派生 → 内置,与 catalog 的优先级同一份实现
    crate::catalog::resolve(dir, &crate::catalog::user_dir(), id)
}

fn mime(p: &str) -> &'static str {
    match p.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "mp4" => "video/mp4",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn send_file(stream: &mut TcpStream, path: &PathBuf) -> std::io::Result<()> {
    let Ok(meta) = std::fs::metadata(path) else {
        let b = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        return stream.write_all(b.as_bytes());
    };
    let Ok(data) = std::fs::read(path) else {
        let b = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        return stream.write_all(b.as_bytes());
    };
    // 没有任何缓存校验器时,浏览器会按 URL 无限复用旧响应 —— 预览帧文件名是确定性的,
    // 于是"改了滑杆画面不动"。no-cache 强制每次回头问一次,本地回环问一次几乎免费。
    let secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nETag: \"{:-x}-{:-x}\"\r\nLast-Modified: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        mime(&path.to_string_lossy()),
        data.len(),
        secs,
        data.len(),
        http_date(secs),
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&data)
}

/// RFC 7231 IMF-fixdate;不引 chrono,自己查表算(只有英文月份名,无本地化需求)
fn http_date(secs: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (secs / 86_400) as i64;
    let sod = secs % 86_400;
    // 1970-01-01 是星期四
    let dow = DAYS[(days.rem_euclid(7)) as usize];
    // 从 1970 年起按年/月拆(到 2100 年前足够准)
    let mut year = 1970i64;
    let mut rem = days;
    loop {
        let len = if (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)) || year % 400 == 0 {
            366
        } else {
            365
        };
        if rem < len {
            break;
        }
        rem -= len;
        year += 1;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let mlens = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0usize;
    while (rem as usize) >= mlens[month] {
        rem -= mlens[month] as i64;
        month += 1;
    }
    format!(
        "{dow}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        rem + 1,
        MONTHS[month],
        year,
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

fn file(mut stream: TcpStream, state: &State, path: &str) -> std::io::Result<()> {
    let raw = path.strip_prefix("/file").unwrap_or("");
    let decoded = percent(raw);
    let p = PathBuf::from(decoded);
    if !p.is_absolute() {
        let b = "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        return stream.write_all(b.as_bytes());
    }
    // 只放行本服务登记过的路径(上传件 / 成品 / 预览帧),其余一律 403
    if !file_allowed(state, &p.to_string_lossy()) {
        let b = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        return stream.write_all(b.as_bytes());
    }
    send_file(&mut stream, &p)
}

fn percent(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn static_(mut stream: TcpStream, ui: &PathBuf, path: &str) -> std::io::Result<()> {
    let rel = if path == "/" || path.is_empty() { "/index.html" } else { path };
    let clean = rel.trim_start_matches('/').replace("..", "");
    let mut p = ui.join(clean);
    if p.is_dir() {
        p = p.join("index.html");
    }
    send_file(&mut stream, &p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_name_keeps_unicode_drops_traversal_and_bad_ext() {
        assert_eq!(safe_name("金坷垃_1995.mp4").as_deref(), Some("金坷垃_1995.mp4"));
        assert_eq!(safe_name("C:\\Users\\a\\b\\x.MP4").as_deref(), Some("x.MP4"));
        assert_eq!(safe_name("/tmp/../etc/passwd.mp4").as_deref(), Some("passwd.mp4"));
        assert_eq!(safe_name("../../../etc/passwd"), None);
        assert_eq!(safe_name("evil.sh"), None);
        assert_eq!(safe_name("no_dot_extension"), None);
        assert_eq!(safe_name("  .mp4"), None);
        assert_eq!(safe_name("a:b|c?.mp4").as_deref(), Some("abc.mp4"));
        assert_eq!(safe_name(""), None);
    }

    #[test]
    fn content_length_and_query_param() {
        let head = "POST /upload?name=a%20b.mp4 HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 4242\r\n\r\n";
        assert_eq!(content_length(head), 4242);
        assert_eq!(content_length("GET / HTTP/1.1\r\n\r\n"), 0);
        assert_eq!(param("x=1&name=a%20b.mp4&y=2", "name"), Some("a%20b.mp4"));
        assert_eq!(param("x=1", "name"), None);
        assert_eq!(percent(&param("n=%E4%B8%AD.mp4", "n").unwrap()), "中.mp4");
    }
}
