use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::errcode::{coded, PRESET_LOAD};

pub const SCHEMA_VERSION: u32 = 1;

/// ntsc-rs 的信号级旋钮。**键名与区间取自 `describe::ntsc_knobs()` 那张表**(上游描述符名),
/// 三处必须同名:清单发布它、预设存它、像素层用它 —— 一处改名而另两处没跟,参数就会静默失效。
/// 全部 `Option`:没写 = 上游默认值,所以接入前后的成品逐字节相同(闸里用 md5 钉住这条)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct NtscKnobs {
    /// 0=无 1=SP 2=LP 3=EP(带速越低越糊,是"像不像家用录像带"的第一因子)
    pub vhs_tape_speed: Option<f64>,
    pub vhs_chroma_loss: Option<f64>,
    pub vhs_sharpen: Option<f64>,
    pub vhs_edge_wave: Option<f64>,
    pub tracking_noise_height: Option<f64>,
    pub tracking_noise_wave_intensity: Option<f64>,
    pub head_switching_height: Option<f64>,
    pub head_switching_horizontal_shift: Option<f64>,
    pub snow: Option<f64>,
    pub luma_smear: Option<f64>,
    pub chroma_delay_horizontal: Option<f64>,
    pub chroma_delay_vertical: Option<f64>,
}

impl NtscKnobs {
    /// 一个都没拧动(全 None)—— 用来判断能不能直接吃上游默认值
    pub fn is_empty(&self) -> bool {
        self == &NtscKnobs::default()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub era: u32,
    pub schema: u32,
    #[serde(default)]
    pub note: Option<String>,
    pub video: Vec<VideoStage>,
    #[serde(default)]
    pub audio: Vec<AudioStage>,
    #[serde(default)]
    pub calibrated: Option<String>,
    /// 做旧系数 = 被"下载→重采样→再压缩→上传"往返几手。
    /// 与强度正交:强度是"这一手有多重",系数是"几手"。None/1 = 用预设自己写死的趟数。
    #[serde(default)]
    pub aging: Option<u32>,
    /// 偏色强度 0–1:0 = 不偏色(默认)。**变绿不是包浆的必备成分**,它是"贴吧/QQ 那一路"的可选味道,
    /// 所以做成显式开关而不是随手数自动出现。走 `--override preset.cast=0.4`。
    #[serde(default)]
    pub cast: Option<f64>,
    /// 变体:同一效果的不同实现(如 VHS 的"信号级 / 静态近似")。
    /// 填了它就不单独占一张预设卡,而是挂在 `variant_of` 那张卡下当一个「类型」选项。
    #[serde(default)]
    pub variant_of: Option<String>,
    /// 变体在「类型」控件里显示的名字,如"静态近似"
    #[serde(default)]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "stage", content = "params")]
pub enum VideoStage {
    #[serde(rename = "resize")]
    Resize {
        #[serde(default)]
        w: Option<u32>,
        #[serde(default)]
        h: Option<u32>,
        /// "source_canvas" = 拉回源画布
        #[serde(default)]
        mode: Option<String>,
        #[serde(default = "default_flags")]
        flags: String,
        /// 显示宽高比 "4:3"/"16:9"/"13:9":老电视的味道,默认不动源画幅
        #[serde(default)]
        dar: Option<String>,
        /// 达成 dar 的方式:crop 居中裁(默认)/ pad 加黑边 / stretch 直接压扁
        #[serde(default)]
        fit: Option<String>,
        /// 像素宽高比 "10:11"(NTSC)/"12:11"(PAL)/"59:54"(DV):非方像素是"像老片"的第一因子
        #[serde(default)]
        par: Option<String>,
        /// 过扫描裁切 0.85-1.0:真电视机切掉画面边缘,裁在缩放之前
        #[serde(default)]
        overscan: Option<f64>,
        /// 量化范围:"tv"(limited 16-235,默认)/"pc"(full 0-255,监控与 VCD rip 常见)
        #[serde(default)]
        range: Option<String>,
    },
    /// 帧率与节奏。默认 round=down = **纯丢帧不补帧**(老摄影机的抽帧感);
    /// shutter 给运动模糊——低帧率读起来"旧"而不是"卡"的关键就是模糊随帧距放大。
    #[serde(rename = "fps")]
    Fps {
        fps: f64,
        #[serde(default = "default_fps_round")]
        round: String,
        #[serde(default)]
        shutter: f64,
        /// "telecine32" = 胶片 3:2 节奏抖动(24 拍内容转到场率的招牌感)
        #[serde(default)]
        cadence: Option<String>,
    },
    #[serde(rename = "codec_roundtrip")]
    CodecRoundtrip {
        codec: String,
        #[serde(default)]
        q: u32,
        /// 码率模式(如 "250k"):与 q 二选一,libx264 等用 -b:v
        #[serde(default)]
        bitrate: Option<String>,
        #[serde(default)]
        container: Option<String>,
        #[serde(default)]
        audio_bitrate: Option<String>,
        #[serde(default)]
        video_only: bool,
    },
    #[serde(rename = "noise")]
    Noise { alls: u32, allf: String },
    #[serde(rename = "color")]
    Color {
        #[serde(default)]
        saturation: Option<f64>,
        #[serde(default)]
        contrast: Option<f64>,
        #[serde(default)]
        brightness: Option<f64>,
        #[serde(default)]
        green_mid: Option<f64>,
        #[serde(default)]
        blue_mid: Option<f64>,
    },
    #[serde(rename = "asymmetric_lowpass")]
    AsymLowpass { luma_radius: f64, chroma_radius: f64 },
    #[serde(rename = "interlace_comb")]
    InterlaceComb { mode: String, refps: f64 },
    #[serde(rename = "overlay_timestamp")]
    OverlayTimestamp {
        format: String,
        #[serde(default)]
        rec_badge: bool,
    },
    /// S1 色彩空间劣化:色度抽稀往返(411=消费级 VHS/监控,420=早期数码)
    #[serde(rename = "chroma_decimate")]
    ChromaDecimate { to: String },
    /// S1:BT.709↔BT.601 矩阵往返(模拟复合解码色偏)
    #[serde(rename = "matrix_roundtrip")]
    MatrixRoundtrip {},
    /// 磁带头尾雪花(带首/末各 N 秒的重噪,模拟穿带)
    #[serde(rename = "tape_ends")]
    TapeEnds {
        head: f64,
        tail: f64,
        intensity: u32,
    },
    /// 像素级 NTSC/VHS 信号模拟(PixelPath)
    #[serde(rename = "ntsc_vhs")]
    NtscVhs {
        #[serde(default)]
        seed: i32,
        /// 12 个信号级旋钮。全部默认 None = 用上游默认值,成品与接入前逐字节相同。
        #[serde(flatten, default)]
        knobs: NtscKnobs,
    },
    /// CRT 显示模拟:扫描线/桶形失真/色散/荧光粉拖影(PixelPath)
    #[serde(rename = "crt_display")]
    Crt {
        #[serde(default)]
        scanline: f64,
        #[serde(default)]
        barrel: f64,
        #[serde(default)]
        aberration: f64,
        /// 荧光粉余辉(跨帧拖影)0-1
        #[serde(default)]
        persistence: f64,
    },
    /// 胶片机械损伤:划痕/尘埃/闪烁(PixelPath,种子驱动)
    #[serde(rename = "film_damage")]
    FilmDamage {
        #[serde(default)]
        seed: i32,
        #[serde(default)]
        scratches: f64,
        #[serde(default)]
        dust: f64,
        #[serde(default)]
        flicker: f64,
    },
    /// 胶片褪色(ffmpeg:抬黑/降饱和/偏暖)
    #[serde(rename = "color_fade")]
    ColorFade {
        #[serde(default)]
        strength: f64,
    },
    /// 色阶断层(8bit 被砍阶):ffmpeg 4.4.2 没有 posterize/banddither,只能 lutyuv 手搓。
    /// level 越大越轻(16~24 轻微断层,32 重),最小 8 会开始啃细节。
    /// **默认只砍亮度** —— 实测砍色度会把 U/V 各推低 1~3 且只剩约 6 级可用,
    /// 结果不是"色带"而是"画面发绿斑块"(用户报回,见 §8.1-33)。要连色度一起砍才开 chroma。
    #[serde(rename = "band_quantize")]
    BandQuantize {
        #[serde(default = "default_band_level")]
        level: u32,
        #[serde(default)]
        chroma: bool,
    },
    /// 锐化/振铃。**包浆是减法、deep fried 是加法**,所以默认 amount=0 时整条滤镜不插入。
    #[serde(rename = "unsharp")]
    Unsharp {
        #[serde(default = "default_unsharp_size")]
        size: u32,
        #[serde(default)]
        amount: f64,
        #[serde(default)]
        chroma_amount: f64,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "stage", content = "params")]
pub enum AudioStage {
    #[serde(rename = "bandlimit")]
    Bandlimit { highpass: u32, lowpass: u32 },
    #[serde(rename = "gain")]
    Gain { volume: f64 },
    #[serde(rename = "bitrate_roundtrip")]
    BitrateRoundtrip { bitrate: String },
    #[serde(rename = "resample_roundtrip")]
    ResampleRoundtrip { rate: u32 },
    #[serde(rename = "bitcrush")]
    Bitcrush { bits: u32, mode: String, aa: f64 },
    #[serde(rename = "tape_hiss")]
    TapeHiss {
        color: String,
        amplitude: f64,
        mix_weight: f64,
    },
    /// 单声道化(磁带/电视喇叭是单声道)
    #[serde(rename = "mono")]
    Mono {},
    /// 磁带抖晃:音高 wow + 幅度 flutter
    #[serde(rename = "wow_flutter")]
    WowFlutter { freq: f64, depth: f64 },
}

fn default_flags() -> String {
    "bicubic".into()
}

fn default_band_level() -> u32 {
    20
}

fn default_unsharp_size() -> u32 {
    5
}

fn default_fps_round() -> String {
    "down".into()
}

impl VideoStage {
    /// serde 标签。新增变体会被 match 穷尽性逼着在这里补条目,
    /// describe 的参数清单再据此对账(§13.1:清单与实际能力不许漂)。
    pub fn tag(&self) -> &'static str {
        use VideoStage::*;
        match self {
            Resize { .. } => "resize",
            Fps { .. } => "fps",
            CodecRoundtrip { .. } => "codec_roundtrip",
            Noise { .. } => "noise",
            Color { .. } => "color",
            AsymLowpass { .. } => "asymmetric_lowpass",
            InterlaceComb { .. } => "interlace_comb",
            OverlayTimestamp { .. } => "overlay_timestamp",
            ChromaDecimate { .. } => "chroma_decimate",
            MatrixRoundtrip { .. } => "matrix_roundtrip",
            TapeEnds { .. } => "tape_ends",
            NtscVhs { .. } => "ntsc_vhs",
            Crt { .. } => "crt_display",
            FilmDamage { .. } => "film_damage",
            ColorFade { .. } => "color_fade",
            BandQuantize { .. } => "band_quantize",
            Unsharp { .. } => "unsharp",
        }
    }
}

impl AudioStage {
    pub fn tag(&self) -> &'static str {
        use AudioStage::*;
        match self {
            Bandlimit { .. } => "bandlimit",
            Gain { .. } => "gain",
            BitrateRoundtrip { .. } => "bitrate_roundtrip",
            ResampleRoundtrip { .. } => "resample_roundtrip",
            Bitcrush { .. } => "bitcrush",
            TapeHiss { .. } => "tape_hiss",
            Mono { .. } => "mono",
            WowFlutter { .. } => "wow_flutter",
        }
    }
}

impl Preset {
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|e| coded(PRESET_LOAD, format!("读取预设失败 {path:?}: {e}")))?;
        let p: Preset = serde_json::from_str(&raw).map_err(|e| coded(PRESET_LOAD, format!("解析预设失败 {path:?}: {e}")))?;
        if p.schema > SCHEMA_VERSION {
            return Err(format!(
                "预设 schema={} 高于本程序支持 {SCHEMA_VERSION}",
                p.schema
            ));
        }
        Ok(p)
    }

    /// 加载 + 应用界面覆盖(`stage.key=value`)。覆盖写死某参数,不影响其余预设。
    pub fn load_with_overrides(path: &Path, overrides: &[String]) -> Result<Self, String> {
        if overrides.is_empty() {
            return Self::load(path);
        }
        let raw = fs::read_to_string(path).map_err(|e| coded(PRESET_LOAD, format!("读取预设失败 {path:?}: {e}")))?;
        let mut v: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| coded(PRESET_LOAD, format!("解析预设失败 {path:?}: {e}")))?;
        crate::describe::apply_overrides(&mut v, overrides)?;
        let p: Preset = serde_json::from_value(v).map_err(|e| coded(PRESET_LOAD, format!("覆盖后预设非法: {e}")))?;
        if p.schema > SCHEMA_VERSION {
            return Err(coded(PRESET_LOAD, format!("预设 schema={} 高于本程序支持 {SCHEMA_VERSION}", p.schema)));
        }
        Ok(p)
    }

    /// 每个 stage 首个出现的参数对象(界面据此显示"跟随预设(720×480)"这类推算值)
    pub fn params_map(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        let mut add = |s: &serde_json::Value| {
            if let (Some(st), Some(pm)) = (s.get("stage").and_then(|x| x.as_str()), s.get("params")) {
                m.entry(st.to_string()).or_insert_with(|| pm.clone());
            }
        };
        for s in &self.video {
            if let Ok(v) = serde_json::to_value(s) {
                add(&v);
            }
        }
        for a in &self.audio {
            if let Ok(v) = serde_json::to_value(a) {
                add(&v);
            }
        }
        serde_json::Value::Object(m)
    }

    pub fn has_pixel_stage(&self) -> bool {
        self.video
            .iter()
            .any(|s| matches!(s, VideoStage::NtscVhs { .. } | VideoStage::Crt { .. } | VideoStage::FilmDamage { .. }))
    }

    /// 「再翻录一次」:N 轮真 mpeg4 低码率往返 + 轻噪,模拟"这片子被转了 N 手"
    pub fn reclip(times: usize) -> Self {
        let times = times.max(1);
        let mut video = vec![];
        for _ in 0..times {
            video.push(VideoStage::CodecRoundtrip {
                codec: "mpeg4".into(),
                q: 15,
                bitrate: None,
                container: Some("mp4".into()),
                audio_bitrate: Some("96k".into()),
                video_only: false,
            });
            video.push(VideoStage::Noise { alls: 4, allf: "t+u".into() });
        }
        Preset {
            id: format!("reclip_x{times}"),
            name: format!("再翻录 ×{times}"),
            era: 0,
            schema: 1,
            note: Some("生成代际损失:每轮真 mpeg4 重编码".into()),
            video,
            audio: vec![],
            calibrated: None,
            aging: None,
            cast: None,
            variant_of: None,
            variant: None,
        }
    }
}

/// 「换一批」:把所有带 seed 的 stage 种子 +1,返回改动数
pub fn reroll_value(v: &mut serde_json::Value) -> usize {
    let mut n = 0;
    if let Some(arr) = v.get_mut("video").and_then(|x| x.as_array_mut()) {
        for st in arr.iter_mut() {
            if let Some(p) = st.get_mut("params") {
                if let Some(seed) = p.get("seed").and_then(|s| s.as_i64()) {
                    p["seed"] = serde_json::json!(seed.wrapping_add(1));
                    n += 1;
                }
            }
        }
    }
    n
}

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

fn scale_stage(s: &VideoStage, i: f64) -> VideoStage {
    match s {
        VideoStage::Noise { alls, allf } => VideoStage::Noise {
            alls: ((*alls as f64 * i).round().max(0.0) as u32).min(100),
            allf: allf.clone(),
        },
        VideoStage::Color { saturation, contrast, brightness, green_mid, blue_mid } => {
            // 偏离"原色"的幅度随强度缩放
            let dev = |v: &Option<f64>| v.map(|x| 1.0 + (x - 1.0) * i);
            VideoStage::Color {
                saturation: dev(saturation),
                contrast: dev(contrast),
                brightness: brightness.map(|b| b * i),
                green_mid: green_mid.map(|g| g * i),
                blue_mid: blue_mid.map(|b| b * i),
            }
        }
        VideoStage::AsymLowpass { luma_radius, chroma_radius } => VideoStage::AsymLowpass {
            luma_radius: (1.0 + (luma_radius - 1.0) * i).clamp(0.5, 6.0),
            chroma_radius: (chroma_radius * i).clamp(1.0, 16.0),
        },
        VideoStage::ColorFade { strength } => VideoStage::ColorFade { strength: clamp01(strength * i) },
        VideoStage::Unsharp { size, amount, chroma_amount } => VideoStage::Unsharp {
            size: *size,
            amount: (amount * i).clamp(0.0, 3.0),
            chroma_amount: chroma_amount * i,
        },
        // band_quantize 的 level 是"级宽"(越大越轻),不是幅度 —— 不随强度缩放
        VideoStage::FilmDamage { seed, scratches, dust, flicker } => VideoStage::FilmDamage {
            seed: *seed,
            scratches: clamp01(scratches * i),
            dust: clamp01(dust * i),
            flicker: clamp01(flicker * i),
        },
        VideoStage::Crt { scanline, barrel, aberration, persistence } => VideoStage::Crt {
            scanline: clamp01(scanline * i),
            barrel: (barrel * i).clamp(0.0, 0.5),
            aberration: (aberration * i).clamp(0.0, 1.5),
            persistence: clamp01(persistence * i),
        },
        VideoStage::CodecRoundtrip { codec, q, bitrate, container, audio_bitrate, video_only } => {
            let scaled_bitrate = bitrate.as_ref().and_then(|b| {
                let k = b.strip_suffix('k')?.parse::<f64>().ok()?;
                Some(format!("{}k", (k / i).clamp(48.0, 2000.0).round() as i64))
            });
            VideoStage::CodecRoundtrip {
                codec: codec.clone(),
                q: ((*q as f64 * i).round() as u32).clamp(2, 31),
                bitrate: scaled_bitrate.or(bitrate.clone()),
                container: container.clone(),
                audio_bitrate: audio_bitrate.clone(),
                video_only: *video_only,
            }
        }
        other => other.clone(),
    }
}

fn scale_astage(s: &AudioStage, i: f64) -> AudioStage {
    match s {
        AudioStage::TapeHiss { color, amplitude, mix_weight } => AudioStage::TapeHiss {
            color: color.clone(),
            amplitude: (amplitude * i).min(0.05),
            mix_weight: *mix_weight,
        },
        other => other.clone(),
    }
}

impl Preset {
    /// 全局做旧强度乘子:只缩放"幅度类"参数(噪声/渗漏/损伤/编码劣化),钳位防失控
    pub fn scaled(&self, i: f64) -> Preset {
        let i = i.clamp(0.2, 2.0);
        Preset {
            id: self.id.clone(),
            name: self.name.clone(),
            era: self.era,
            schema: self.schema,
            note: self.note.clone(),
            video: self.video.iter().map(|s| scale_stage(s, i)).collect(),
            audio: self.audio.iter().map(|s| scale_astage(s, i)).collect(),
            calibrated: self.calibrated.clone(),
            aging: self.aging,
            cast: self.cast,
            variant_of: self.variant_of.clone(),
            variant: self.variant.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presets_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets")
    }

    #[test]
    fn load_all_builtin_presets() {
        let dir = presets_dir();
        let mut n = 0;
        for e in fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().map(|x| x == "json").unwrap_or(false) {
                let pr = Preset::load(&p).unwrap_or_else(|err| panic!("{}: {err}", p.display()));
                assert_eq!(pr.schema, 1);
                assert!(!pr.video.is_empty());
                n += 1;
            }
        }
        assert!(n >= 5, "内置预设数 {n} < 5");
    }

    #[test]
    fn reroll_bumps_all_seeds() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets");
        let mut v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("film1970.json")).unwrap()).unwrap();
        assert_eq!(v["video"][2]["params"]["seed"], serde_json::json!(11));
        assert_eq!(reroll_value(&mut v), 1);
        assert_eq!(v["video"][2]["params"]["seed"], serde_json::json!(12));
    }

    #[test]
    fn intensity_scales_magnitudes_with_clamps() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets");
        let base = Preset::load(&dir.join("film1970.json")).unwrap();
        let strong = base.scaled(2.0);
        let weak = base.scaled(0.5);
        let j = |p: &Preset| serde_json::to_string(p).unwrap();
        let js = j(&strong);
        assert!(js.contains("\"alls\":32"), "noise 16→32: {js}");
        assert!(js.contains("\"strength\":1.0"), "fade 0.7×2 钳到 1.0");
        assert!(js.contains("\"scratches\":1.0"), "scratches 0.55×2 钳到 1.0");
        let jw = j(&weak);
        assert!(jw.contains("\"alls\":8"), "noise 16→8");
        // 编码劣化:q 与码率反向缩放
        let dvd = Preset::load(&dir.join("dvd2005.json")).unwrap();
        assert!(j(&dvd.scaled(2.0)).contains("\"q\":28"));
        let e2013 = crate::era::preset_for_year(2013);
        let s2 = j(&e2013.scaled(2.0));
        assert!(s2.contains("125k"), "250k 强度×2 → 码率减半: {s2}");
    }

    #[test]
    fn reclip_chains_generations() {
        use crate::ffgraph::{build_plan, MediaInfo, Step};
        let m = MediaInfo { width: 1920, height: 1080, fps: 30.0, duration: 10.0, has_audio: true,
            container: "mov,mp4,m4a,3gp,3g2,mj2".into() };
        let plan = build_plan(&Preset::reclip(2), &m, None, None).unwrap();
        let codec_passes = plan
            .steps
            .iter()
            .filter(|s| matches!(s, Step::Fast(p) if p.vcodec.iter().any(|x| x == "mpeg4")))
            .count();
        assert_eq!(codec_passes, 2, "两轮翻录应为两个 mpeg4 趟");
    }
}
