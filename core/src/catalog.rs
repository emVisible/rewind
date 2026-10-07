//! 预设目录(catalog):**内置 / 我的 / 派生**三层的唯一枚举实现。
//!
//! 此前 `shell` 与 `serve` 各写了一份"合并 + 去重 + 排序",而且年代轴与换一批的临时产物
//! 直接落进用户预设目录 —— 既混进预设列表,又会永久顶替同名内置预设(实测用户目录里躺着
//! `era_1965.json … era_2010.json` 和被换过一批的 `dvd2005.json`)。见
//! `docs/预设体系与界面体验规划.md` D1/D2。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::preset::Preset;

/// 派生预设(年代轴生成、换一批生成)的落盘位置:用户目录下的这个子目录**不进画廊**
pub const DERIVED_SUBDIR: &str = "derived";

/// 年代轴生成的预设 id 前缀。历史上它们落在用户目录顶层,这里一并归为派生。
pub fn is_derived_id(id: &str) -> bool {
    id.starts_with("era_")
}

fn scan_dir(dir: &Path, kind: &str) -> Vec<Value> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        // 隐藏件不是预设。再翻录会在输出目录写一个临时预设 JSON(名字以点开头),
        // 进程被 kill 时它留在那里;而"输出目录恰好就是预设目录"是用户能做出来的事 ——
        // 一旦被枚举,画廊里就多出一张点不开的卡。原子写的 `x.json.<标签>.tmp` 由扩展名那一条挡掉。
        let hidden = p
            .file_name()
            .and_then(|x| x.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(true);
        if hidden {
            continue;
        }
        if p.extension().and_then(|x| x.to_str()) != Some("json") || !p.is_file() {
            continue;
        }
        let Ok(pr) = Preset::load(&p) else { continue }; // 坏文件不该让整个列表挂掉
        // era_* 历史上落在用户目录顶层,一律归为派生(见 D1)
        let kind = if kind == "user" && is_derived_id(&pr.id) { "derived" } else { kind };
        out.push(json!({
            "id": pr.id,
            "name": pr.name,
            "era": pr.era,
            "kind": kind,
            // 能不能占一张预设卡:年代轴产物是"中间状态"不占卡;而内置预设被换过一批之后
            // 仍是那张卡(只是内容变了),按 kind 过滤会让卡片从画廊里凭空消失
            "card": !is_derived_id(&pr.id),
            "aging": pr.aging.unwrap_or(1),
            "path": p.to_string_lossy(),
            "params": pr.params_map(),
            "variant_of": pr.variant_of,
            "variant": pr.variant,
            "variants": [],
        }));
    }
    out
}

/// 汇总三层预设。同名时**用户保存的 > 派生 > 内置**(用户主动改过的必须生效)。
/// `gallery` 给出四类对比资产哪些真的存在;变体折进基准卡片的 `variants`,不单独成卡。
pub fn entries(presets_dir: &Path, user_dir: &Path, ui_dir: &Path) -> Vec<Value> {
    let mut by_id: BTreeMap<String, Value> = BTreeMap::new();
    for (dir, kind) in [
        (presets_dir.to_path_buf(), "builtin"),
        (user_dir.join(DERIVED_SUBDIR), "derived"),
        (user_dir.to_path_buf(), "user"),
    ] {
        for e in scan_dir(&dir, kind) {
            let id = e["id"].as_str().unwrap_or_default().to_string();
            by_id.insert(id, e);
        }
    }
    for (id, e) in by_id.iter_mut() {
        e["gallery"] = json!({
            "tile": has_asset(ui_dir, id, "png"),
            "full": has_asset(ui_dir, &format!("{id}_full"), "png"),
            "hero": has_asset(ui_dir, &format!("{id}_hero"), "webp"),
            "motion": has_asset(ui_dir, id, "webm"),
        });
    }
    let mut out: Vec<Value> = fold_variants(by_id).into_values().collect();
    out.sort_by_key(|e| (e["era"].as_u64().unwrap_or(0), e["id"].as_str().unwrap_or("").to_string()));
    out
}

/// 变体不单独成卡:整条记录挂到基准预设的 `variants` 上,界面渲成一个「类型」选择
/// (要带着 gallery/params/aging,否则选中变体后卡片读数与对比图都会瞎)。
/// 基准不存在时(文件被删/名字写错)保留成普通卡 —— 宁可多一张卡,也不能让预设凭空消失。
fn fold_variants(mut by_id: BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let folded: Vec<(String, Value)> = by_id
        .values()
        .filter_map(|e| {
            let base = e["variant_of"].as_str()?.to_string();
            let id = e["id"].as_str()?.to_string();
            if base == id || !by_id.contains_key(&base) {
                return None;
            }
            let mut v = e.clone();
            if v["variant"].is_null() {
                v["variant"] = json!(id);
            }
            // 变体不占卡:界面会把它展开成可选预设,靠这个标记挡在画廊之外
            v["card"] = json!(false);
            Some((base, v))
        })
        .collect();
    for (base, v) in folded {
        if let Some(dead) = v["id"].as_str().map(|s| s.to_string()) {
            by_id.remove(&dead);
        }
        if let Some(b) = by_id.get_mut(&base) {
            b["variants"].as_array_mut().expect("初始化过").push(v);
        }
    }
    by_id
}

fn has_asset(ui_dir: &Path, name: &str, ext: &str) -> bool {
    ui_dir.join("gallery").join(format!("{name}.{ext}")).is_file()
}

/// 派生预设的落点:`<user>/derived/<id>.json`
pub fn derived_path(user_dir: &Path, id: &str) -> PathBuf {
    user_dir.join(DERIVED_SUBDIR).join(format!("{id}.json"))
}

/// 用户预设目录(与桌面壳同源):`$REWIND_USER_PRESETS` 优先,否则平台数据目录
pub fn user_dir() -> PathBuf {
    std::env::var_os("REWIND_USER_PRESETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let base = if cfg!(windows) {
                std::env::var_os("APPDATA").map(PathBuf::from)
            } else {
                std::env::var_os("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            };
            base.map(|d| d.join("Rewind").join("presets")).unwrap_or_else(|| PathBuf::from("RewindPresets"))
        })
}

/// 解析顺序:用户保存的 → 派生 → 内置。"存为预设"因此能直接盖掉派生品。
pub fn resolve(presets_dir: &Path, user_dir: &Path, id_or_path: &str) -> PathBuf {
    let p = Path::new(id_or_path);
    if p.is_absolute() && p.exists() {
        return p.to_path_buf();
    }
    let name = format!("{id_or_path}.json");
    for cand in [user_dir.join(&name), derived_path(user_dir, id_or_path), presets_dir.join(&name)] {
        if cand.exists() {
            return cand;
        }
    }
    presets_dir.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 目录必须每个用例独占:cargo test 并行跑,共用 tmp 会互相删文件
    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rewind_catalog_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("builtin")).unwrap();
        fs::create_dir_all(d.join("user/derived")).unwrap();
        fs::create_dir_all(d.join("ui/gallery")).unwrap();
        d
    }

    fn write(dir: &Path, id: &str, name: &str) {
        fs::write(
            dir.join(format!("{id}.json")),
            format!(r#"{{"id":"{id}","name":"{name}","era":1990,"schema":1,"video":[{{"stage":"noise","params":{{"alls":8,"allf":"t"}}}}]}}"#),
        )
        .unwrap();
    }

    /// 隐藏件不许长成一张预设卡。再翻录往输出目录写临时预设(`.reclip_x1.tmp.<标签>.json`),
    /// 进程被 kill 时它留在那儿;而"把输出目录设成预设目录"是用户做得出来的操作。
    #[test]
    fn hidden_files_never_become_presets() {
        let d = tmp("skip_hidden");
        write(&d.join("user"), "mine", "我的");
        fs::write(
            d.join("user/.reclip_x1.tmp.deadbeef.json"),
            r#"{"id":"ghost","name":"幽灵","era":1990,"schema":1,"video":[]}"#,
        )
        .unwrap();
        fs::write(d.join("user/mine.json.deadbeef.tmp"), "半截").unwrap();
        let e = entries(&d.join("builtin"), &d.join("user"), &d.join("ui"));
        let ids: Vec<&str> = e.iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["mine"], "残留临时件被枚举成预设: {ids:?}");
    }

    #[test]
    fn derived_recipes_are_marked_and_never_shadow_builtin() {
        let d = tmp("derived_marked");
        write(&d.join("builtin"), "dvd2005", "翻录 DVD");
        write(&d.join("user"), "era_1996", "年代轴 ~1996");
        write(&d.join("user/derived"), "vhs1990_static", "换一批过的");
        fs::write(d.join("ui/gallery/dvd2005.png"), []).unwrap();
        fs::write(d.join("ui/gallery/dvd2005_full.png"), []).unwrap();

        let e = entries(&d.join("builtin"), &d.join("user"), &d.join("ui"));
        let get = |id: &str| e.iter().find(|x| x["id"] == json!(id)).cloned().unwrap();
        assert_eq!(get("era_1996")["kind"], json!("derived"), "历史遗留的 era_* 必须算派生");
        assert_eq!(get("dvd2005")["kind"], json!("builtin"));
        assert_eq!(get("vhs1990_static")["kind"], json!("derived"), "derived/ 里的同名文件不许顶替内置");
        assert_eq!(get("dvd2005")["gallery"]["tile"], json!(true));
        assert_eq!(get("dvd2005")["gallery"]["hero"], json!(false));
        // 换一批过的内置预设仍是那张卡 —— 按 kind 过滤会让它从画廊里消失(实测踩过)
        assert_eq!(get("dvd2005")["card"], json!(true));
        assert_eq!(get("era_1996")["card"], json!(false), "年代轴产物不占卡");
        assert_eq!(get("vhs1990_static")["card"], json!(true));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn variants_fold_into_one_card_and_dangling_ones_survive() {
        let d = tmp("variants");
        write(&d.join("builtin"), "vhs1990", "VHS 家用录像带");
        fs::write(
            d.join("builtin/vhs1990_soft.json"),
            r#"{"id":"vhs1990_soft","name":"VHS 家用录像带(静态近似)","era":1990,"schema":1,
                "variant_of":"vhs1990","variant":"静态近似",
                "video":[{"stage":"noise","params":{"alls":8,"allf":"t"}}]}"#,
        )
        .unwrap();
        fs::write(
            d.join("builtin/orphan.json"),
            r#"{"id":"orphan","name":"无主的变体","era":2000,"schema":1,"variant_of":"不存在",
                "video":[{"stage":"noise","params":{"alls":8,"allf":"t"}}]}"#,
        )
        .unwrap();
        let e = entries(&d.join("builtin"), &d.join("user"), &d.join("ui"));
        let ids: Vec<&str> = e.iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert!(!ids.contains(&"vhs1990_soft"), "变体不该单独成卡: {ids:?}");
        assert!(ids.contains(&"orphan"), "基准不存在的变体必须仍然可见: {ids:?}");
        let base = e.iter().find(|x| x["id"] == json!("vhs1990")).unwrap();
        let v = &base["variants"];
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["id"], json!("vhs1990_soft"));
        assert_eq!(v[0]["variant"], json!("静态近似"));
        // 变体得带齐卡片要用的字段,否则选中它之后读数/对比图都是瞎的
        assert!(v[0].get("gallery").is_some(), "变体必须带 gallery 资产标记");
        assert!(v[0].get("params").is_some(), "变体必须带参数读数");
        assert_eq!(v[0]["card"], json!(false), "变体被展开成预设后仍不许占一张卡");
        assert_eq!(e.iter().find(|x| x["id"] == json!("orphan")).unwrap()["variants"].as_array().unwrap().len(), 0);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn saved_user_recipe_wins_over_derived_and_builtin() {
        let d = tmp("user_wins");
        write(&d.join("builtin"), "patina", "电子包浆");
        write(&d.join("user/derived"), "patina", "电子包浆(换一批)");
        let e = entries(&d.join("builtin"), &d.join("user"), &d.join("ui"));
        assert_eq!(e.iter().find(|x| x["id"] == json!("patina")).unwrap()["kind"], json!("derived"));
        write(&d.join("user"), "patina", "我的包浆");
        let e = entries(&d.join("builtin"), &d.join("user"), &d.join("ui"));
        let hit = e.iter().find(|x| x["id"] == json!("patina")).unwrap();
        assert_eq!(hit["kind"], json!("user"));
        assert_eq!(hit["name"], json!("我的包浆"));
        assert_eq!(resolve(&d.join("builtin"), &d.join("user"), "patina"), d.join("user/patina.json"));
        let _ = fs::remove_dir_all(&d);
    }
}
