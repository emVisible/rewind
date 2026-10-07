//! 参数清单(manifest):引擎把自己的可调参数连同区间/单位/中文标签一起发布,
//! 界面(桌面壳与 Web 版同一套代码)据此渲染控件 —— 加参数不再需要改 UI。
//! 这是 §13.1 的结构性解法:此前 JSON 里 22 种 stage、40+ 参数,界面只给 4 个控件。

use crate::errcode::{coded, PARAM_RANGE, PARAM_UNKNOWN, PRESET_LOAD};
use serde_json::{Value, json};

fn p(key: &str, label: &str, kind: &str, default: Value, extra: Value) -> Value {
    let mut o = json!({ "key": key, "label": label, "kind": kind, "default": default });
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            o[k.clone()] = v.clone();
        }
    }
    o
}

fn f(key: &str, label: &str, min: f64, max: f64, step: f64, default: f64, unit: &str, scaled: bool) -> Value {
    p(key, label, "float", json!(default), json!({
        "min": min, "max": max, "step": step, "unit": unit, "scaled_by_intensity": scaled
    }))
}

fn i(key: &str, label: &str, min: i64, max: i64, default: i64, unit: &str, scaled: bool) -> Value {
    p(key, label, "int", json!(default), json!({
        "min": min, "max": max, "step": 1, "unit": unit, "scaled_by_intensity": scaled
    }))
}

/// 带自定义步长的整数参数。有些整数值域天生不连续:ffmpeg 的 unsharp 矩阵只接受 **3–13 的奇数**
/// (偶数报 "Invalid even size for luma matrix size 6x6",更大报 "luma or chroma matrix size too big")。
/// 步长给 2,界面就滑不出非法值 —— 清单是校验器,也是唯一的档位来源。
fn istep(key: &str, label: &str, min: i64, max: i64, step: i64, default: i64, unit: &str,
         scaled: bool) -> Value {
    p(key, label, "int", json!(default), json!({
        "min": min, "max": max, "step": step, "unit": unit, "scaled_by_intensity": scaled
    }))
}

fn en(key: &str, label: &str, options: &[&str], default: &str) -> Value {
    p(key, label, "enum", json!(default), json!({ "options": options }))
}

/// 分档值(帧率这类"常见档位 + 精确小数"的参数):界面按索引渲染刻度滑块
fn stepped(key: &str, label: &str, stops: &[f64], default: f64, unit: &str) -> Value {
    // 档位表是降序写的(60→5),照抄首尾会把范围报成 min=60 / max=5。
    // 任何按 min≤v≤max 校验的消费端都会因此判错 —— 范围必须取真最小/最大。
    let (lo, hi) = stops.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    p(key, label, "stepped", json!(default), json!({
        "stops": stops, "min": lo, "max": hi, "unit": unit
    }))
}

fn opt_str(key: &str, label: &str, options: &[&str]) -> Value {
    p(key, label, "optional_enum", Value::Null, json!({ "options": options }))
}

fn stage(name: &str, label: &str, group: &str, params: Vec<Value>) -> Value {
    json!({ "stage": name, "label": label, "group": group, "rank": rank_of(name), "params": params })
}

/// 给参数标注"引擎**硬**可用区间"。清单里的 min/max 是界面渲染的范围(档位/滑杆),
/// 而 `--override` 该拦的是管线还能不能照办 —— 两者不是一回事,必须分开写清楚:
/// 用档位去拦 CLI,会把 fps=80 这种"能用但没人要"的值当成非法;用硬区间去渲染滑杆,
/// 又把界面拉成 1–120 这种没人能选的刻度。
fn hard(v: Value, lo: f64, hi: f64) -> Value {
    let mut o = v;
    o["hard_min"] = json!(lo);
    o["hard_max"] = json!(hi);
    o
}

/// ntsc-rs 信号级旋钮的**唯一**表:清单按它发布控件,像素层按它落值。
///
/// 默认值必须等于上游今天真正生效的值。两个原因:
/// ① `stage_defaults` 会把清单里每个参数的默认值原样写进"预设里没有该段时补的那一段",
///    照抄"0 = 关"会让"插入一个 ntsc_vhs 段"这件事悄悄改掉成品;
/// ② `SettingsBlock::default()` 是 **enabled: true** —— 跟踪噪声/磁头切换/锐化这些块默认就开着,
///    上游的内层默认(12 行 / 8 行 / 72 像素 / 0.25 倍 / 0.5 像素)才是画面现在这个样子。
/// 想关掉某块就把它的尺寸类参数写成 0(0 行 = 没有可见带),不是把块整体禁掉。
///
/// `max` 是**界面滑杆**的上限,`hard_max` 是 `--override` 允许的上限(默认同 max)。
/// 分档依据是实测 PSNR(400px 代理帧对基线):雪花 0.5 → 32.5dB(满屏雪花但认得出画面),
/// 100 → 7.8dB(糊成纯噪点,只有 CLI 的意义);色度损失 0.05 已接近"色没了"。
#[derive(Debug, Clone, Copy)]
pub struct NtscKnob {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub default: f64,
    pub unit: &'static str,
    pub hard_max: f64,
}

pub fn ntsc_knobs() -> Vec<NtscKnob> {
    macro_rules! knob {
        ($key:literal, $label:literal, $kind:literal, $min:expr, $max:expr, $step:expr, $def:expr, $unit:literal) => {
            NtscKnob { key: $key, label: $label, kind: $kind, min: $min, max: $max, step: $step, default: $def, unit: $unit, hard_max: $max }
        };
        ($key:literal, $label:literal, $kind:literal, $min:expr, $max:expr, $step:expr, $def:expr, $unit:literal, $hard:expr) => {
            NtscKnob { key: $key, label: $label, kind: $kind, min: $min, max: $max, step: $step, default: $def, unit: $unit, hard_max: $hard }
        };
    }
    vec![
        knob!("vhs_tape_speed", "带速(NTSC 磁带)", "int", 0.0, 3.0, 1.0, 2.0, "0=无 1=SP 2=LP 3=EP"),
        // step 必须整除默认值,否则滑杆根本停不到默认档(0.00002 那版停不住 0.000025)
        knob!("vhs_chroma_loss", "色度损失", "float", 0.0, 0.05, 0.000005, 0.000025, "比例", 0.2),
        knob!("vhs_sharpen", "磁带锐化", "float", 0.0, 5.0, 0.05, 0.25, "倍"),
        knob!("vhs_edge_wave", "边缘波纹", "float", 0.0, 20.0, 0.1, 0.5, "像素"),
        knob!("tracking_noise_height", "跟踪噪声带高度", "int", 0.0, 120.0, 1.0, 12.0, "行(0=无)"),
        knob!("tracking_noise_wave_intensity", "跟踪噪声扭曲", "float", -50.0, 50.0, 0.5, 15.0, "像素"),
        knob!("head_switching_height", "磁头切换带高度", "int", 0.0, 24.0, 1.0, 8.0, "行(0=无)"),
        knob!("head_switching_horizontal_shift", "磁头切换横向错位", "float", -100.0, 100.0, 1.0, 72.0, "像素"),
        knob!("snow", "雪花强度", "float", 0.0, 0.5, 0.00005, 0.00025, "比例", 100.0),
        knob!("luma_smear", "亮度拖尾", "float", 0.0, 1.0, 0.01, 0.5, "比例"),
        knob!("chroma_delay_horizontal", "色度水平滞后", "float", -40.0, 40.0, 1.0, 0.0, "像素"),
        knob!("chroma_delay_vertical", "色度垂直滞后", "int", -20.0, 20.0, 1.0, 0.0, "行"),
    ]
}

/// 管线里的相对位置:覆盖一个预设里没有的 stage 时按它插入(否则"帧率"这类控件会插到末尾)
const RANKS: &[(&str, i64)] = &[
    ("resize", 10),
    ("fps", 20),
    ("chroma_decimate", 30),
    ("matrix_roundtrip", 35),
    ("ntsc_vhs", 40),
    ("crt_display", 45),
    ("film_damage", 50),
    ("tape_ends", 55),
    ("asymmetric_lowpass", 60),
    ("interlace_comb", 65),
    ("noise", 70),
    ("color", 75),
    ("band_quantize", 76),
    ("unsharp", 78),
    ("color_fade", 80),
    ("codec_roundtrip", 90),
    ("overlay_timestamp", 95),
];

pub fn rank_of(name: &str) -> i64 {
    RANKS.iter().find(|(n, _)| *n == name).map(|(_, r)| *r).unwrap_or(100)
}

/// 某个 stage 属于哪一段、排第几
fn stage_meta(name: &str) -> Option<(&'static str, i64)> {
    let m = manifest();
    for sec in ["video", "audio"] {
        let hit = m[sec]
            .as_array()
            .map(|arr| arr.iter().any(|s| s["stage"].as_str() == Some(name)))
            .unwrap_or(false);
        if hit {
            return Some((if sec == "video" { "video" } else { "audio" }, rank_of(name)));
        }
    }
    None
}

fn ensure_params(st: &mut Value) {
    if !st.get("params").map(|p| p.is_object()).unwrap_or(false) {
        st["params"] = json!({});
    }
}

/// 补插一个预设里没有的 stage 时,先按清单默认值把必填参数填满(否则反序列化会缺字段)
fn stage_defaults(name: &str) -> Value {
    let m = manifest();
    for sec in ["video", "audio"] {
        let found = m[sec]
            .as_array()
            .and_then(|arr| arr.iter().find(|s| s["stage"].as_str() == Some(name)).cloned());
        if let Some(st) = found {
            let mut o = serde_json::Map::new();
            if let Some(prms) = st["params"].as_array() {
                for prm in prms {
                    let k = prm["key"].as_str().unwrap_or("");
                    let d = &prm["default"];
                    if !k.is_empty() && !d.is_null() {
                        o.insert(k.to_string(), d.clone());
                    }
                }
            }
            return Value::Object(o);
        }
    }
    json!({})
}

/// 参数名是否真存在:拼错的 key 会被 serde 静默丢掉,所以这里必须当场拒绝
fn param_keys(stage: &str) -> Option<Vec<String>> {
    let m = manifest();
    for sec in ["video", "audio"] {
        let found = m[sec]
            .as_array()
            .and_then(|arr| arr.iter().find(|s| s["stage"].as_str() == Some(stage)).cloned());
        if let Some(st) = found {
            return Some(
                st["params"]
                    .as_array()
                    .map(|prms| {
                        prms.iter()
                            .filter_map(|p| p["key"].as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
    }
    None
}

/// 某个数值参数的**引擎硬**可用区间:优先 hard_min/hard_max(与界面档位分开声明),
/// 没有就退回 min/max(int/float 参数的滑杆范围本来就是要拦的范围)。
/// 用于校验 `--override`:清单是界面的数据来源,同一条区间也必须拦住命令行与 API 的越界值,
/// 否则"界面画不出荒谬值"只是假安全 —— 实测补这之前 `--override noise.alls=99999`、`resize.w=1` 一路放行。
fn param_bounds(stage: &str, key: &str) -> Option<(f64, f64)> {
    let m = manifest();
    for sec in ["video", "audio"] {
        let found = m[sec]
            .as_array()
            .and_then(|arr| arr.iter().find(|s| s["stage"].as_str() == Some(stage)).cloned());
        let Some(st) = found else { continue };
        if let Some(p) =
            st["params"].as_array().and_then(|ps| ps.iter().find(|p| p["key"].as_str() == Some(key)).cloned())
        {
            let lo = p["hard_min"].as_f64().or_else(|| p["min"].as_f64());
            let hi = p["hard_max"].as_f64().or_else(|| p["max"].as_f64());
            return match (lo, hi) {
                (Some(lo), Some(hi)) => Some((lo, hi)),
                _ => None,
            };
        }
    }
    None
}

pub fn manifest() -> Value {
    let video = vec![
        stage(
            "resize",
            "画幅与像素比",
            "geometry",
            vec![
                // max 是"能被钉进来的真实素材尺寸"的上限:界面的「源」按钮把素材原始宽高原样钉进来,
                // 8K/16K 屏摄是真实存在的输入,卡在 4096 会让合法素材点一下就报错
                i("w", "存储宽", 16, 16384, 720, "px", false),
                i("h", "存储高", 16, 16384, 480, "px", false),
                opt_str("dar", "显示画幅", &["4:3", "15:11", "16:9", "13:9", "1:1"]),
                en("fit", "画幅适配", &["crop", "pad", "stretch", "inside"], "crop"),
                opt_str("par", "像素宽高比", &["10:11", "12:11", "59:54", "1:1"]),
                f("overscan", "电视切边(过扫描)", 0.85, 1.0, 0.01, 1.0, "比例", false),
                en("range", "量化范围", &["tv", "pc"], "tv"),
                en("flags", "缩放算法", &["bicubic", "area", "bilinear", "neighbor"], "bicubic"),
            ],
        ),
        stage(
            "fps",
            "帧率与节奏",
            "temporal",
            vec![
                // 老素材的帧率是"档位"不是连续值:29.97/30、23.976/24 必须各占一档
                hard(stepped("fps", "帧率", &[60.0, 50.0, 30.0, 29.97, 25.0, 24.0, 23.976, 18.0, 16.0, 15.0, 12.5, 10.0, 7.5, 6.25, 5.0], 25.0, "fps"),
                     crate::ffgraph::FPS_MIN, crate::ffgraph::FPS_MAX),
                en("round", "取整方式", &["down", "near"], "down"),
                f("shutter", "快门模糊", 0.0, 1.0, 0.05, 0.0, "比例", false),
                opt_str("cadence", "节奏", &["telecine32"]),
            ],
        ),
        stage(
            "codec_roundtrip",
            "编码代际",
            "compression",
            vec![
                en("codec", "编码器", &["mpeg4", "h263", "mjpeg", "libx264"], "mpeg4"),
                i("q", "量化尺 q", 2, 31, 15, "", true),
                opt_str("bitrate", "码率模式", &["500k", "250k", "180k", "120k"]),
                en("container", "容器", &["mp4", "avi", "3gp", "mpg"], "mp4"),
                opt_str("audio_bitrate", "音频码率", &["64k", "48k", "32k", "16k"]),
                p("video_only", "只处理视频(音轨留给后段)", "bool", json!(false), json!(null)),
            ],
        ),
        stage("noise", "雪花/颗粒", "noise", vec![
            i("alls", "强度", 0, 100, 8, "", true),
            en("allf", "时域模式", &["t", "u", "t+u"], "t+u"),
        ]),
        stage("color", "色彩", "color", vec![
            f("saturation", "饱和度", 0.0, 2.0, 0.01, 1.0, "倍", true),
            f("contrast", "对比度", 0.5, 2.0, 0.01, 1.0, "倍", true),
            f("brightness", "亮度", -0.5, 0.5, 0.01, 0.0, "", true),
            // colorbalance 的 gm/bm 是 **-1..1 的偏移量**,不是倍数;默认 0 才是中性。
            // 曾经这里写 0..2 默认 1.0 —— 补插 color stage 时会直接拉出满格绿。
            f("green_mid", "绿中场偏移", -1.0, 1.0, 0.01, 0.0, "偏移", true),
            f("blue_mid", "蓝中场偏移", -1.0, 1.0, 0.01, 0.0, "偏移", true),
        ]),
        stage("color_fade", "褪色", "color", vec![f("strength", "褪色强度", 0.0, 1.0, 0.01, 0.5, "", true)]),
        stage("band_quantize", "色阶断层", "color", vec![
            // 越大越轻:16~24 是"传了几手",32 起开始啃细节
            hard(i("level", "色阶级宽", 8, 48, 20, "级", false),
                 crate::ffgraph::LEVEL_MIN as f64, crate::ffgraph::LEVEL_MAX as f64),
            // 默认关:砍色度会把 U/V 压成约 6 级并整体发绿(实测缺陷,见 §8.1-33)
            p("chroma", "连色度一起砍", "bool", json!(false), json!(null)),
        ]),
        stage("unsharp", "锐化/振铃(爆炸档)", "clarity", vec![
            f("amount", "锐化强度", 0.0, 3.0, 0.05, 0.0, "倍", true),
            istep("size", "核尺寸", 3, 13, 2, 5, "px", false),
            f("chroma_amount", "色度锐化", 0.0, 3.0, 0.05, 0.0, "倍", false),
        ]),
        stage("asymmetric_lowpass", "模糊(亮度/色度分开的低通)", "clarity", vec![
            f("luma_radius", "亮度模糊半径", 0.5, 6.0, 0.1, 1.5, "px", true),
            f("chroma_radius", "色度模糊半径", 1.0, 16.0, 0.1, 4.0, "px", true),
        ]),
        stage("interlace_comb", "隔行梳齿", "temporal", vec![
            en("mode", "织入方式", &["merge", "interleave_top", "interleave_bottom"], "merge"),
            f("refps", "场率基准", 1.0, 60.0, 0.5, 25.0, "fps", false),
        ]),
        stage("overlay_timestamp", "时间戳水印", "stamp", vec![
            p("format", "时间格式", "text", json!("%Y-%m-%d %T"), json!(null)),
            p("rec_badge", "REC 红点", "bool", json!(false), json!(null)),
        ]),
        stage("chroma_decimate", "色度抽稀", "chroma", vec![en("to", "色度采样", &["411", "420", "422"], "411")]),
        stage("matrix_roundtrip", "色域矩阵往返", "chroma", vec![]),
        stage("tape_ends", "穿带雪花(头/尾)", "damage", vec![
            f("head", "片头时长", 0.0, 5.0, 0.1, 0.6, "s", false),
            f("tail", "片尾时长", 0.0, 5.0, 0.1, 0.6, "s", false),
            i("intensity", "强度", 0, 100, 30, "", false),
        ]),
        // `settings` 这个控件从前在这里:界面给一个 JSON 文本框,值一路传到这里被丢弃
        // (NtscVhs 只有 seed,pixel.rs 用 VHSSettings::default())。一个改了什么都没用的控件
        // 比没有控件更坏 —— 已下线;12 个旋钮的规格由 ntsc_knobs() + 测试留着,接入见 #12。
        {
            // 12 个信号级旋钮全部来自 ntsc_knobs() 这一张表:清单、校验、像素层用的是同一份名字与区间
            let mut ps = vec![i("seed", "随机种子", -2147483648, 2147483647, 0, "", false)];
            ps.extend(ntsc_knobs().iter().map(|k| {
                let v = match k.kind {
                    "int" => i(k.key, k.label, k.min as i64, k.max as i64, k.default as i64, k.unit, false),
                    _ => f(k.key, k.label, k.min, k.max, k.step, k.default, k.unit, false),
                };
                // 滑杆只给"看得见又不糊掉"的那一段,CLI 仍可推到硬上限(见 NtscKnob 注释)
                if k.hard_max > k.max { hard(v, k.min, k.hard_max) } else { v }
            }));
            stage("ntsc_vhs", "NTSC 信号级模拟(像素路径)", "signal", ps)
        },
        stage("crt_display", "CRT 显示", "display", vec![
            f("scanline", "扫描线", 0.0, 1.0, 0.01, 0.4, "", true),
            f("barrel", "桶形失真", 0.0, 1.0, 0.01, 0.3, "", true),
            f("aberration", "色散", 0.0, 1.0, 0.01, 0.3, "", true),
            f("persistence", "荧光粉拖影", 0.0, 1.0, 0.01, 0.3, "", true),
        ]),
        stage("film_damage", "胶片机械损伤", "damage", vec![
            i("seed", "随机种子", -2147483648, 2147483647, 0, "", false),
            f("scratches", "划痕", 0.0, 1.0, 0.01, 0.5, "", true),
            f("dust", "尘埃", 0.0, 1.0, 0.01, 0.5, "", true),
            f("flicker", "亮度抽风", 0.0, 1.0, 0.01, 0.4, "", true),
        ]),
    ];

    let audio = vec![
        stage("bandlimit", "带限", "audio", vec![
            i("highpass", "高通", 20, 8000, 200, "Hz", false),
            i("lowpass", "低通", 200, 20000, 4000, "Hz", false),
        ]),
        stage("gain", "增益", "audio", vec![f("volume", "音量", 0.0, 4.0, 0.01, 1.0, "倍", false)]),
        stage("bitrate_roundtrip", "音频码率往返", "audio", vec![en("bitrate", "码率", &["128k", "96k", "64k", "48k", "32k", "16k"], "64k")]),
        stage("resample_roundtrip", "采样率往返", "audio", vec![i("rate", "采样率", 8000, 48000, 11025, "Hz", false)]),
        stage("bitcrush", "位深破碎", "audio", vec![
            i("bits", "位深", 1, 16, 5, "bit", false),
            // acrusher 的 mode 只认这两个名字(实测 ln/pw/wl 会被 ffmpeg 当"Undefined constant"丢掉,
            // 界面于是给了三个选了都不起作用的控制项)
            en("mode", "模式", &["lin", "log"], "lin"),
            f("aa", "抗锯齿", 0.0, 1.0, 0.05, 0.5, "", false),
        ]),
        stage("tape_hiss", "磁带底噪", "audio", vec![
            en("color", "噪声颜色", &["white", "pink", "brown"], "brown"),
            f("amplitude", "幅度", 0.0, 0.05, 0.001, 0.02, "", true),
            f("mix_weight", "混入权重", 0.0, 1.0, 0.01, 0.35, "", false),
        ]),
        stage("mono", "单声道下混", "audio", vec![]),
        stage("wow_flutter", "抖晃", "audio", vec![
            f("freq", "频率", 0.1, 5.0, 0.1, 0.7, "Hz", false),
            f("depth", "深度", 0.0, 0.1, 0.001, 0.02, "", false),
        ]),
    ];

    // 一级控件(L2)绑到哪些 stage 参数上:界面按这份绑定渲染,不硬编码。
    // follow="source" 表示该控件可切"跟随源"(上传后按 probe 取值);其余只跟随预设。
    let controls = json!([
        {"id": "aspect", "label": "画幅与制式", "group": "画幅", "binds": [
            {"stage": "resize", "key": "dar", "label": "显示画幅", "follow": "source"},
            {"stage": "resize", "key": "fit", "label": "适配"},
            {"stage": "resize", "key": "par", "label": "像素比"},
            {"stage": "resize", "key": "overscan", "label": "电视切边"}
        ]},
        {"id": "tempo", "label": "帧率与节奏", "group": "画幅", "binds": [
            {"stage": "fps", "key": "fps", "label": "帧率", "follow": "source"},
            {"stage": "fps", "key": "round", "label": "丢帧/补帧"},
            {"stage": "fps", "key": "shutter", "label": "快门模糊"},
            {"stage": "fps", "key": "cadence", "label": "节奏"}
        ]},
        {"id": "clarity", "label": "清晰度与带宽", "group": "画幅", "binds": [
            {"stage": "resize", "key": "w", "label": "存储宽", "follow": "source"},
            {"stage": "resize", "key": "h", "label": "存储高", "follow": "source"},
            {"stage": "resize", "key": "range", "label": "量化范围", "follow": "source"},
            {"stage": "asymmetric_lowpass", "key": "luma_radius", "label": "亮度模糊"}
        ]},
        // 「代际与编码」(编码器 / 量化尺 / 码率)刻意**不在**这一层:
        // 一级层只放手上的素材真会去改的东西(画幅、帧率、尺寸/清晰度)。
        // 第几代压缩已经被「做旧系数(几手)」折成一个旋钮了,再摆一排编码器参数就是厨房软件
    ]);

    // 计划级控件:不是某个 stage 的参数,而是与"强度"同级的傻瓜旋钮(走 `preset.aging=` 覆盖通道)
    let plan = json!([
        {"key": "aging", "label": "做旧系数", "kind": "stepped",
         "min": 1, "max": crate::ffgraph::MAX_AGING, "step": 1, "default": 1, "unit": "手",
         "stops": [1, 2, 3, 4, 5, 6, 7, 8],
         "hint": "每手 = 降采样一档 + 一次压缩往返。预设自己算第 1 手;6 手起进爆炸档(锐化+饱和拉爆)。上限 8 是因为实测同尺寸同 q 反复往返会收敛。"},
        {"key": "cast", "label": "偏色(绿)", "kind": "float",
         "min": 0.0, "max": 1.0, "step": 0.05, "default": 0.0, "unit": "",
         "hint": "0 = 不偏色(默认)。包浆不等于变绿:偏绿是「贴吧/QQ 那一路」的可选味道,开了才逐手往青绿推。"}
    ]);

    json!({
        "groups": [
            {"id": "geometry", "label": "画幅与像素"},
            {"id": "temporal", "label": "时间轴与节奏"},
            {"id": "clarity", "label": "清晰度与带宽"},
            {"id": "compression", "label": "编码代际"},
            {"id": "signal", "label": "NTSC 信号级"},
            {"id": "chroma", "label": "色度"},
            {"id": "color", "label": "色彩"},
            {"id": "noise", "label": "噪声"},
            {"id": "damage", "label": "介质损伤"},
            {"id": "display", "label": "显示端"},
            {"id": "stamp", "label": "水印与时间戳"},
            {"id": "audio", "label": "音频"}
        ],
        "video": video,
        "audio": audio,
        "controls": controls,
        "plan": plan,
        "schema": 1,
    })
}

/// 应用 `--override stage.key=value` 列表;返回被改动的条数。
/// 预设里没有该 stage 时按管线位置补一个(界面的"帧率/画幅"这类一级控件对任何预设都要生效);
/// 清单里也没这个 stage 名才报错 —— 拼错不许静默跑。
pub fn apply_overrides(v: &mut Value, overrides: &[String]) -> Result<usize, String> {
    let mut n = 0;
    for o in overrides {
        let (path, raw) = o
            .split_once('=')
            .ok_or_else(|| coded(PARAM_UNKNOWN, format!("覆盖写法应为 stage.key=value,收到 {o}")))?;
        let mut it = path.splitn(2, '.');
        let stage = it.next().ok_or(coded(PARAM_UNKNOWN, "覆盖缺少 stage 名"))?;
        let key = it.next().ok_or(coded(PARAM_UNKNOWN, "覆盖缺少参数名(如 fps.shutter)"))?;
        if key.is_empty() || stage.is_empty() {
            return Err(coded(PARAM_UNKNOWN, format!("覆盖写法非法: {o}")));
        }
        // 计划级覆盖:preset.aging=N / preset.cast=X —— 都不是任何 stage 的参数,与"强度"同级
        if stage == "preset" {
            let val = parse_scalar(raw, "");
            match key {
                "aging" => {
                    let g = val.as_i64().ok_or_else(|| coded(PARAM_RANGE, format!("preset.aging 需要整数,收到 {raw}")))?;
                    if g < 1 || g > crate::ffgraph::MAX_AGING as i64 {
                        return Err(coded(PARAM_RANGE, format!("preset.aging 超出 1-{}: {g}", crate::ffgraph::MAX_AGING)));
                    }
                    v["aging"] = json!(g);
                }
                "cast" => {
                    let c = val.as_f64().ok_or_else(|| coded(PARAM_RANGE, format!("preset.cast 需要 0-1 的小数,收到 {raw}")))?;
                    if !(0.0..=1.0).contains(&c) {
                        return Err(coded(PARAM_RANGE, format!("preset.cast 超出 0-1: {c}")));
                    }
                    v["cast"] = json!(c);
                }
                other => return Err(coded(PARAM_UNKNOWN, format!("「preset」没有参数「{other}」,可用:aging, cast"))),
            }
            n += 1;
            continue;
        }
        if let Some(keys) = param_keys(stage) {
            if !keys.iter().any(|k| k == key) {
                return Err(coded(PARAM_UNKNOWN, format!(
                    "「{stage}」没有参数「{key}」,可用:{}",
                    keys.join(", ")
                )));
            }
        }
        let val = parse_scalar(raw, &param_kind(stage, key));
        // 数值型覆盖必须落在清单给的区间里:清单同时是界面的滑杆范围,两处必须同一个答案
        if let Some(num) = val.as_f64() {
            if let Some((lo, hi)) = param_bounds(stage, key) {
                if !(lo..=hi).contains(&num) {
                    return Err(coded(PARAM_RANGE, format!(
                        "{stage}.{key}={raw} 超出可用范围({};可用范围见 rewind-core describe)",
                        crate::ffgraph::range_text(lo, hi)
                    )));
                }
            }
        }
        let mut done = false;
        for sec in ["video", "audio"] {
            if let Some(arr) = v.get_mut(sec).and_then(|x| x.as_array_mut()) {
                if let Some(st) =
                    arr.iter_mut().find(|s| s.get("stage").and_then(|x| x.as_str()) == Some(stage))
                {
                    ensure_params(st);
                    st["params"][key] = val.clone();
                    done = true;
                    break;
                }
            }
        }
        if done {
            n += 1;
            continue;
        }
        let (sec, rank) = stage_meta(stage)
            .ok_or_else(|| coded(PARAM_UNKNOWN, format!("没有 stage「{stage}」,可用名见 rewind-core describe")))?;
        let arr = v
            .get_mut(sec)
            .and_then(|x| x.as_array_mut())
            .ok_or_else(|| coded(PRESET_LOAD, format!("预设缺 {sec} 段")))?;
        let pos = arr
            .iter()
            .position(|s| {
                stage_meta(s.get("stage").and_then(|x| x.as_str()).unwrap_or(""))
                    .map(|(_, r)| r > rank)
                    .unwrap_or(false)
            })
            .unwrap_or(arr.len());
        let mut params = stage_defaults(stage);
        params
            .as_object_mut()
            .expect("清单默认值应为对象")
            .insert(key.to_string(), val.clone());
        arr.insert(pos, json!({ "stage": stage, "params": params }));
        n += 1;
    }
    Ok(n)
}

/// 覆盖值的类型**按清单的 kind 定**,不靠"长得像不像数字"。
///
/// `chroma_decimate.to` 的选项是 "411"/"420"/"422" —— 数字形状的字符串。旧写法一律先试数字解析,于是界面里
/// 选 4:1:1 发来的 `to=411` 变成 integer,而 serde 要 string,整条覆盖直接挨 `[preset.load] 覆盖后预设非法`
/// (实测三个选项全被拒:一个从头到尾不可用的控件,比"拧了没反应"更糟)。
/// 清单里这个参数的 kind(拿不到就当空串 = 按数字/布尔形状解析,与旧行为一致)。
fn param_kind(stage: &str, key: &str) -> String {
    for sec in ["video", "audio"] {
        let m = manifest();
        let found = m[sec]
            .as_array()
            .and_then(|arr| arr.iter().find(|s| s["stage"].as_str() == Some(stage)).cloned());
        let Some(st) = found else { continue };
        if let Some(p) = st["params"]
            .as_array()
            .and_then(|ps| ps.iter().find(|x| x["key"].as_str() == Some(key)).cloned())
        {
            return p["kind"].as_str().unwrap_or("").to_string();
        }
    }
    String::new()
}

fn parse_scalar(s: &str, kind: &str) -> Value {
    if matches!(kind, "enum" | "optional_enum" | "text") {
        return json!(s.trim());
    }
    match s.trim() {
        "true" => json!(true),
        "false" => json!(false),
        "null" => Value::Null,
        t => {
            if let Ok(i) = t.parse::<i64>() {
                json!(i)
            } else if let Ok(f) = t.parse::<f64>() {
                json!(f)
            } else {
                json!(t)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::era::preset_for_year;
    use crate::preset::{AudioStage, Preset, VideoStage};
    use std::path::Path;

    /// ntsc 旋钮表是清单、校验、像素层共用的唯一出处:这张表必须与清单里的 `ntsc_vhs` 参数
    /// **逐项一致**(两处各写一遍迟早漂移),而且每一项都得是能用的规格。
    #[test]
    fn ntsc_knob_table_and_manifest_agree() {
        let k = ntsc_knobs();
        assert_eq!(k.len(), 12, "旋钮数量变了:同步 #12 的接入清单与这条断言");
        let mut names: Vec<&str> = k.iter().map(|x| x.key).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 12, "旋钮名重复");
        for kn in &k {
            assert!(
                kn.key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{} 不是上游那种 snake_case 描述符名",
                kn.key
            );
            assert!(!kn.label.is_empty() && !kn.unit.is_empty(), "{} 缺中文标签或单位", kn.key);
            assert!(kn.min < kn.max, "{} 区间反了", kn.key);
            assert!(kn.default >= kn.min && kn.default <= kn.max, "{} 默认值不在区间内", kn.key);
            assert!(matches!(kn.kind, "int" | "float"), "{} 的 kind 界面不认", kn.key);
            assert!(kn.hard_max >= kn.max, "{} 的硬上限比滑杆上限还低", kn.key);
            // 步长必须整除"默认值 - 下限":否则滑杆停不到默认档,一拖就跳过它
            // (雪花 0.001 的步长跑不出 0.00025、色度损失 0.00002 跑不出 0.000025,都是实测出来的)
            if kn.kind == "float" {
                let q = (kn.default - kn.min) / kn.step;
                assert!(
                    (q - q.round()).abs() < 1e-6 * q.max(1.0),
                    "{} 的默认值 {} 落在步长 {} 的格子外(第 {:.3} 格,不是整数)",
                    kn.key,
                    kn.default,
                    kn.step,
                    q
                );
            }
        }
        // 表与清单逐项一致:界面上出现的每个 ntsc 控件都必须有人在引擎里用它,
        // 而且规格(区间/步长/默认/硬上限)必须一字不差 —— 两处各写一遍迟早漂移。
        let m = manifest();
        let st = m["video"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["stage"] == json!("ntsc_vhs"))
            .expect("清单里没有 ntsc_vhs 段");
        let ps = st["params"].as_array().unwrap();
        let keys: Vec<&str> = ps.iter().map(|x| x["key"].as_str().unwrap()).collect();
        for kn in &k {
            let p = ps
                .iter()
                .find(|x| x["key"] == json!(kn.key))
                .unwrap_or_else(|| panic!("{} 在旋钮表里却没进清单", kn.key));
            let got = |field: &str| p[field].as_f64().unwrap_or(f64::NAN);
            assert_eq!(got("min"), kn.min, "{} 的 min 漂移", kn.key);
            assert_eq!(got("max"), kn.max, "{} 的 max 漂移", kn.key);
            assert_eq!(got("step"), kn.step, "{} 的 step 漂移", kn.key);
            assert_eq!(got("default"), kn.default, "{} 的 default 漂移", kn.key);
            if kn.hard_max > kn.max {
                assert_eq!(
                    p["hard_max"].as_f64(),
                    Some(kn.hard_max),
                    "{} 声明了硬上限却没进清单,CLI 会被滑杆档位拦住",
                    kn.key
                );
            }
        }
        assert_eq!(keys.len(), 13, "seed + 12 个旋钮,多一项少一项都算漂移");
    }

    /// 数字形状的 enum/text 值必须**保持字符串**:否则界面上选 4:1:1 就是必然失败(#12 复测时撞出来的)。
    #[test]
    fn numeric_looking_enum_values_stay_strings() {
        let mut v = json!({"video": [{"stage": "chroma_decimate", "params": {"to": "420"}}]});
        apply_overrides(&mut v, &["chroma_decimate.to=411".into()]).expect("4:1:1 是清单里的合法选项");
        assert_eq!(v["video"][0]["params"]["to"], json!("411"));
        // 三个发布出去的选项都得真能用(旧实现是 0/3 可用)
        for o in ["411", "420", "422"] {
            let mut w = v.clone();
            apply_overrides(&mut w, &[format!("chroma_decimate.to={o}")]).expect(o);
        }
        // 数值型不许被带跑:还是数字,并且仍受区间校验
        let mut n = json!({"video": [{"stage": "fps", "params": {"fps": 25.0}}]});
        apply_overrides(&mut n, &["fps.fps=12.5".into()]).unwrap();
        assert_eq!(n["video"][0]["params"]["fps"], json!(12.5));
        assert!(apply_overrides(&mut n, &["fps.fps=9999".into()]).is_err(), "越界仍要拦");
    }

    fn manifest_stages(section: &str) -> Vec<String> {
        manifest()[section]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["stage"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn manifest_covers_every_stage_the_engine_can_emit() {
        // 清单与实际能力不许漂:内置预设 + 每一代年轴用到的 stage 都必须在清单里
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets");
        let mut need_v: Vec<String> = vec![];
        let mut need_a: Vec<String> = vec![];
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().map(|x| x == "json").unwrap_or(false) {
                let pr = Preset::load(&p).unwrap();
                for s in &pr.video {
                    need_v.push(s.tag().into());
                }
                for a in &pr.audio {
                    need_a.push(a.tag().into());
                }
            }
        }
        for y in 1965..=2026 {
            let pr = preset_for_year(y);
            for s in &pr.video {
                need_v.push(s.tag().into());
            }
            for a in &pr.audio {
                need_a.push(a.tag().into());
            }
        }
        let mv = manifest_stages("video");
        let ma = manifest_stages("audio");
        for t in need_v.iter().collect::<std::collections::HashSet<_>>() {
            assert!(mv.contains(t), "清单缺视频 stage「{t}」");
        }
        for t in need_a.iter().collect::<std::collections::HashSet<_>>() {
            assert!(ma.contains(t), "清单缺音频 stage「{t}」");
        }
        // 每个声明的参数都要有区间或选项,否则界面没法渲染控件
        for sec in ["video", "audio"] {
            for st in manifest()[sec].as_array().unwrap() {
                for prm in st["params"].as_array().unwrap() {
                    let k = prm["kind"].as_str().unwrap();
                    if k == "float" || k == "int" {
                        assert!(prm.get("min").is_some() && prm.get("max").is_some(), "{:?}", prm);
                    } else if k == "enum" || k == "optional_enum" {
                        assert!(!prm["options"].as_array().unwrap().is_empty(), "{:?}", prm);
                    }
                    assert!(prm.get("label").is_some() && prm.get("default").is_some(), "{:?}", prm);
                }
            }
        }
        assert!(VideoStage::NtscVhs { seed: 1, knobs: Default::default() }.tag() == "ntsc_vhs");
        assert!(AudioStage::Mono {}.tag() == "mono");
    }

    #[test]
    fn overrides_hit_the_right_stage_and_reject_typos() {
        let mut v = json!({
            "video":[{"stage":"fps","params":{"fps":25.0,"round":"down"}},
                     {"stage":"resize","params":{"w":720,"h":480}}],
            "audio":[{"stage":"bandlimit","params":{"highpass":200,"lowpass":4000}}]
        });
        let n = apply_overrides(&mut v, &["fps.fps=12.5".into(), "resize.par=10:11".into(), "bandlimit.lowpass=3400".into()]).unwrap();
        assert_eq!(n, 3);
        assert_eq!(v["video"][0]["params"]["fps"], json!(12.5));
        assert_eq!(v["video"][1]["params"]["par"], json!("10:11"));
        assert_eq!(v["audio"][0]["params"]["lowpass"], json!(3400));
        // 未知 stage / 缺等号 / 缺参数名 都必须报错,不许静默跑
        assert!(apply_overrides(&mut v, &["nosuch.key=1".into()]).is_err());
        assert!(apply_overrides(&mut v, &["fps.fps".into()]).is_err());
        assert!(apply_overrides(&mut v, &["fps.=1".into()]).is_err());
        // 拼错的参数名 serde 会静默丢掉,必须在这里拦住
        let typo = apply_overrides(&mut v, &["fps.sped=6".into()]);
        assert!(typo.is_err(), "不存在的参数名不该被静默接受");
        assert!(typo.unwrap_err().contains("没有参数"), "错误信息要给出可用参数");
    }

    #[test]
    fn override_on_absent_stage_is_inserted_at_its_pipeline_position() {
        // 预设里没有 fps 时,界面的一级"帧率"控件仍要生效,而且插在噪声之前(rank 20 < 70)
        let mut v = json!({
            "video":[{"stage":"noise","params":{"alls":8,"allf":"t+u"}},
                     {"stage":"overlay_timestamp","params":{"format":"%T"}}],
            "audio":[]
        });
        apply_overrides(&mut v, &["fps.fps=12.5".into()]).unwrap();
        let names: Vec<&str> =
            v["video"].as_array().unwrap().iter().map(|s| s["stage"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["fps", "noise", "overlay_timestamp"], "{names:?}");
        assert_eq!(v["video"][0]["params"]["fps"], json!(12.5));
    }

    #[test]
    fn overridden_preset_loads_through_the_real_deserializer() {
        // 覆盖写进去的值必须还能被 preset schema 吃下(类型不对要当场失败,不能拖到 ffmpeg)
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets");
        let path = dir.join("vhs1990_static.json");
        let p = Preset::load_with_overrides(&path, &["fps.fps=12.5".into(), "fps.shutter=0.6".into()]).unwrap();
        let has = p.video.iter().any(|s| matches!(s, VideoStage::Fps { fps, shutter, .. } if (*fps - 12.5).abs() < 1e-6 && (*shutter - 0.6).abs() < 1e-6));
        assert!(has, "覆盖没落到 Fps stage: {:?}", p.video);
        let bad = Preset::load_with_overrides(&path, &["fps.round=斜杠".into()]);
        assert!(bad.is_ok(), "枚举值由 ffmpeg 侧把关,加载不该炸:{:?}", bad.err());
        let worse = Preset::load_with_overrides(&path, &["fps.fps=很多".into()]);
        assert!(worse.is_err(), "fps 写成非数字应当反序列化失败");
    }
}
