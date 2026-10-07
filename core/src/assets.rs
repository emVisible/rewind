//! 随仓库分发的资产(示例素材)的**唯一**解析处。
//!
//! 此前示例素材的路径规则在 `serve.rs`(Web)与 `rewind-shell`(桌面)各写了一遍,
//! 换素材时只改了一处 —— 界面于是还在拿旧的 ffmpeg 测试图。规则只能有一个出处。

use std::path::{Path, PathBuf};

use crate::errcode::{coded, ASSET_MISSING, ASSET_STAGE};

/// 内置示例素材文件名。必须是**真实画面**:色块/分形上看不出任何做旧效果,
/// 来源与许可见 `assets/sample/CREDITS.md`。
pub const SAMPLE_NAME: &str = "street-food.mp4";

/// 解析顺序:`REWIND_SAMPLE` 显式覆盖 → 工作区 `assets/sample/` → 安装态 presets 同级。
pub fn sample_source(presets_dir: &Path) -> Option<PathBuf> {
    sample_source_from(presets_dir, std::env::var_os("REWIND_SAMPLE").map(PathBuf::from))
}

/// 把覆盖项当参数传进来,测试就不必改进程环境变量(edition 2024 里 set_var 是 unsafe,
/// 而 cargo test 多线程跑,改全局环境本身就是数据竞争)
fn sample_source_from(presets_dir: &Path, override_path: Option<PathBuf>) -> Option<PathBuf> {
    [
        override_path,
        presets_dir
            .parent()
            .map(|r| r.join("assets").join("sample").join(SAMPLE_NAME)),
        Some(presets_dir.join("sample").join(SAMPLE_NAME)),
    ]
    .into_iter()
    .flatten()
    .find(|p| p.is_file())
}

/// 把示例复制进调用方给的目录并返回路径 —— 直接返回仓库路径会让默认输出目录落在仓库里
/// (实测污染过 fixtures/)。复制走临时名,避免半截文件被当成可用素材。
pub fn stage_sample(src: &Path, dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| coded(ASSET_STAGE, format!("创建示例暂存目录失败: {e}")))?;
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sample.mp4".into());
    let dst = dir.join(&name);
    // "目标在不在"不能当"素材新不新":换过 REWIND_SAMPLE 或仓库素材更新过,同名不同内容会一直复用旧的那份。
    // 判据照 make 的老规矩:尺寸相同 **且** 暂存位不比源旧。复制完把源的时间戳盖到暂存位上
    // (fs::copy 不保留 mtime,不盖就会每次都判成过期)。
    let fresh = match (std::fs::metadata(src), std::fs::metadata(&dst)) {
        (Ok(s), Ok(d)) => match (s.modified(), d.modified()) {
            (Ok(st), Ok(dt)) => s.len() == d.len() && dt >= st,
            _ => false,
        },
        _ => false,
    };
    if !fresh {
        let tmp = dir.join(format!(".{}.{}.part", name, crate::pipeline::run_tag()));
        std::fs::copy(src, &tmp).map_err(|e| coded(ASSET_STAGE, format!("复制示例素材失败: {e}")))?;
        if let Err(e) = std::fs::rename(&tmp, &dst) {
            // 落位失败必须带走半截 .part:它没人再清,会在暂存目录里越积越多
            let _ = std::fs::remove_file(&tmp);
            return Err(coded(ASSET_STAGE, format!("示例素材落位失败: {e}")));
        }
        if let Ok(t) = std::fs::metadata(src).and_then(|m| m.modified()) {
            let _ = std::fs::OpenOptions::new().write(true).open(&dst).and_then(|f| f.set_modified(t));
        }
    }
    Ok(dst)
}

/// 解析 + 暂存一步到位,CLI 与 Web 共用这一个答案。
///
/// 两个调用点从前都写成 `sample_source(..).and_then(|s| stage_sample(&s, ..).ok())`,
/// 于是"复制不进去"(目录只读 / 盘满 / 权限)也被报成"这个构建没带示例素材" ——
/// 用户被告知了一个不存在的问题。原因不同就必须分开说。
pub fn resolve_sample(presets_dir: &Path, stage_dir: &Path) -> Result<PathBuf, String> {
    let src = sample_source(presets_dir)
        .ok_or_else(|| coded(ASSET_MISSING, "没有内置示例素材(设 REWIND_SAMPLE 可换)"))?;
    stage_sample(&src, stage_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn sample_resolves_to_the_real_footage_asset() {
        let d = std::env::temp_dir().join(format!("rewind_assets_{}", std::process::id()));
        let root = d.join("repo");
        fs::create_dir_all(root.join("presets")).unwrap();
        fs::create_dir_all(root.join("assets/sample")).unwrap();
        let asset = root.join("assets/sample").join(SAMPLE_NAME);
        fs::write(&asset, b"not really a video").unwrap();
        assert_eq!(sample_source(&root.join("presets")).as_deref(), Some(asset.as_path()));

        // 覆盖优先级:REWIND_SAMPLE 说了算(用户要能换成自己的素材)
        let mine = root.join("我的素材.mp4");
        fs::write(&mine, b"x").unwrap();
        assert_eq!(
            sample_source_from(&root.join("presets"), Some(mine.clone())).as_deref(),
            Some(mine.as_path())
        );

        // 缺文件时不瞎指
        fs::remove_file(&asset).unwrap();
        assert!(sample_source(&root.join("presets")).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    /// 暂存位的两件事都要成立:同样的素材不许重拷(省 IO),换了素材不许继续用旧的。
    /// 从前只判"目标在不在",于是换过 `REWIND_SAMPLE` 或素材更新之后,点的还是旧那份。
    #[test]
    fn staging_skips_when_unchanged_and_recopies_when_source_changes() {
        let d = std::env::temp_dir().join(format!("rewind_assets_stage_{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let src = d.join("src.mp4");
        fs::write(&src, b"abc").unwrap();
        let out = d.join("out");
        let p = stage_sample(&src, &out).unwrap();
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "src.mp4");
        assert_eq!(fs::metadata(&p).unwrap().len(), 3);
        // 同尺寸同时间戳 → 认定是新鲜位,不重拷:把暂存位换成别的 3 字节内容,
        // 再要一次,拿到的还是这份被换过的内容才说明真的没重拷。
        fs::write(&p, b"xyz").unwrap();
        let same = stage_sample(&src, &out).unwrap();
        assert_eq!(fs::read(&same).unwrap(), b"xyz", "没变过素材却重拷了");
        // 源换了(尺寸不同)→ 必须重拷,不能继续用旧素材
        fs::write(&src, b"abcdef").unwrap();
        let again = stage_sample(&src, &out).unwrap();
        assert_eq!(fs::metadata(&again).unwrap().len(), 6, "换了素材还端旧的那份");
        let leftovers: Vec<_> = fs::read_dir(&out)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "不该留下 .part 临时文件");
        let _ = fs::remove_dir_all(&d);
    }
}
