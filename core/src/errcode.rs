//! 界面可见错误的稳定码。
//!
//! 形状固定为 `[码] 中文原文`:中文界面与 CLI 直接看原文(引擎本来就用中文写),英文界面按码
//! 查词典、查不到就退回原文。所以"加一个新错误但没翻译"只会露出中文,不会像现在这样让英文
//! 界面整段看不懂 —— 也不会因为词典漏项而崩。
//!
//! 码表 `CODES` 与代码里 `coded("…")` 的调用点必须一一对上,`scripts/i18n_check.js` 双向核对:
//! 少了词典项、或者码写了却没进表,都会红。

/// 素材没有可解码的画面尺寸(多半不是媒体文件,或文件被移走/截断)
pub const MEDIA_UNREADABLE: &str = "media.unreadable";
/// ffprobe 本身失败(缺组件 / 文件损坏)
pub const ENGINE_FFPROBE: &str = "engine.ffprobe";
/// 抽取某一帧失败(时间点越界或解码不了)
pub const ENGINE_FRAME: &str = "engine.frame";
/// 某一趟编码失败(附 ffmpeg 尾巴)
pub const ENGINE_PASS: &str = "engine.pass";
/// 写到一半发现磁盘满了
pub const ENGINE_DISK_FULL: &str = "engine.disk_full";
/// 输出目录/输出文件建不出来或不可写
pub const FS_OUTDIR: &str = "fs.outdir";
/// 预设文件/预设目录读不出或解析失败
pub const PRESET_LOAD: &str = "preset.load";
/// --override 写了清单里没有的参数名(stage 名也一样)
pub const PARAM_UNKNOWN: &str = "param.unknown";
/// --override 的值超出可用范围
pub const PARAM_RANGE: &str = "param.range";
/// 引擎子进程自己退了,且没报告原因(只有退出码 + stderr 尾巴)
pub const ENGINE_EXIT: &str = "engine.exit";
/// 引擎二进制起不来(缺文件 / 缺执行权限 / 缺 ffmpeg 组件)
pub const ENGINE_SPAWN: &str = "engine.spawn";
/// 上传超过体积上限
pub const UPLOAD_TOO_LARGE: &str = "upload.too_large";
/// 上传的文件名不在支持的类型里
pub const UPLOAD_BAD_TYPE: &str = "upload.bad_type";
/// 上传落地后探不出媒体信息(损坏或不是媒体)
pub const UPLOAD_DECODE: &str = "upload.decode";
/// 用户预设目录建不出来 / 预设存不进去
pub const PRESET_SAVE: &str = "preset.save";
/// 内置示例素材复制不进工作目录
pub const ASSET_STAGE: &str = "asset.stage";
/// 这个构建里没有内置示例素材
pub const ASSET_MISSING: &str = "asset.missing";
/// 上传写到一半客户端断了(文件已删,不会留下半截素材)
pub const UPLOAD_INTERRUPT: &str = "upload.interrupt";
/// 外部工具/引擎子命令的输出读不出预期结构(版本不匹配、被换掉、stdout 半截)
pub const ENGINE_OUTPUT: &str = "engine.output";
/// 用户自己按了取消 —— 这不是失败,界面不许按错误去弹条
pub const RUN_CANCELED: &str = "run.canceled";

/// 全部稳定码。新增错误必须同时进这张表 + 双语词典,否则覆盖率闸红。
/// 表里的值必须是常量名的 snake_case(`PARAM_RANGE` ↔ `param.range`)—— 界面的词典键就是这么来的,
/// `every_declared_code_is_reachable` 按这条规则反推,写歪一次就红。
pub const CODES: &[&str] = &[
    MEDIA_UNREADABLE,
    ENGINE_FFPROBE,
    ENGINE_FRAME,
    ENGINE_PASS,
    ENGINE_DISK_FULL,
    ENGINE_EXIT,
    ENGINE_SPAWN,
    ENGINE_OUTPUT,
    FS_OUTDIR,
    PRESET_LOAD,
    PRESET_SAVE,
    ASSET_STAGE,
    ASSET_MISSING,
    UPLOAD_INTERRUPT,
    RUN_CANCELED,
    PARAM_UNKNOWN,
    PARAM_RANGE,
    UPLOAD_TOO_LARGE,
    UPLOAD_BAD_TYPE,
    UPLOAD_DECODE,
];

/// 贴码。已经带同一个码的消息不重复贴(包装层最常见的小心失误)。
pub fn coded(code: &str, msg: impl Into<String>) -> String {
    // 打错一个字母的码不会被任何词典查到,用户就永远看中文 —— 这种错要在开发期炸出来
    debug_assert!(CODES.contains(&code), "稳定码 {code} 不在 CODES 表里");
    let msg = msg.into();
    let tag = format!("[{code}] ");
    if msg.starts_with(&tag) {
        return msg;
    }
    format!("{tag}{msg}")
}

/// 把码换成另一个(如"某一趟失败"确诊为"盘满"时改贴盘满码),保留后面的原文
pub fn recode(code: &str, msg: &str) -> String {
    let body = match msg.find("] ") {
        Some(i) if msg.starts_with('[') => &msg[i + 2..],
        _ => msg,
    };
    coded(code, body)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use super::*;

    #[test]
    fn code_is_prefixed_once_and_never_doubled() {
        let a = coded(MEDIA_UNREADABLE, "读不到画面尺寸");
        assert_eq!(a, "[media.unreadable] 读不到画面尺寸");
        // 包装层再贴一次同一个码 → 必须原样返回,不能变成 "[a] [a] …"
        assert_eq!(coded(MEDIA_UNREADABLE, &a), a);
        // 换个码要替换掉旧的,而不是并排两个
        assert_eq!(
            recode(ENGINE_DISK_FULL, &a),
            "[engine.disk_full] 读不到画面尺寸"
        );
        assert_eq!(coded(FS_OUTDIR, "plain"), "[fs.outdir] plain");
        assert_eq!(recode(FS_OUTDIR, "plain"), "[fs.outdir] plain");
    }

    /// 码表不许变成"写了却没人用"的清单:每个码都必须在 errcode.rs 之外被引用一次,
    /// 而且必须用 `CODES` 反推出来的那个常量名(`param.range` ← `PARAM_RANGE`)——
    /// 界面的词典键就是码本身,名字与值对不上时词典会永远查不到。
    /// 这条与 i18n 闸互补 —— 那条查词典齐不齐,这条查代码用没用。
    #[test]
    fn every_declared_code_is_reachable() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut corpus = String::new();
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.file_name().map(|n| n == "errcode.rs").unwrap_or(false)
                || p.extension().map(|x| x != "rs").unwrap_or(true)
            {
                continue;
            }
            corpus.push_str(&std::fs::read_to_string(&p).unwrap());
        }
        assert!(corpus.len() > 5000, "语料没读到,这条断言在空转");
        assert!(CODES.len() >= 20, "码表缩水了:当前 {}", CODES.len());
        for code in CODES {
            let name = code.to_uppercase().replace('.', "_");
            assert!(
                corpus.contains(&name),
                "码 {code} 没有调用点:找不到常量 {name}(码值必须是常量名的 snake_case)"
            );
        }
        // 反方向:常量都进了表(手新增常量却忘了进 CODES,词典闸查不出来)
        // 反方向:常量都进了表(手新增常量却忘了进 CODES,词典闸查不出来)。
        // 只数顶格的定义行 —— 本测试源码里就带着 "pub const " 这个字面量,按包含数会把自己算进去。
        let decl = std::fs::read_to_string(dir.join("errcode.rs")).unwrap();
        let mut declared = 0;
        for cap in CODES {
            assert!(decl.contains(&format!("\"{cap}\"")), "{cap} 没进 CODES 表");
            declared += 1;
        }
        let consts = decl.lines().filter(|l| l.starts_with("pub const ")).count();
        assert_eq!(consts - 1, declared, "errcode.rs 里有常量没进 CODES(减掉 CODES 自己那条)");
    }
}
