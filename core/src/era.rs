//! 年代轴:1965→2026 连续时间轴,锚点参数线性插值生成预设。
//! 这是 Rewind 的签名交互——"想要 98 年的质感就拖到 98"。

use crate::ffgraph::dar_from_par;
use crate::preset::{AudioStage, Preset, VideoStage};

/// 只认已知 SD 光栅的像素宽高比(插值出来的怪尺寸一律按方形像素处理,不误判)
fn par_for(w: u32, h: u32) -> Option<&'static str> {
    match (w, h) {
        (720, 480) | (704, 480) | (352, 240) => Some("10:11"),
        (720, 576) | (704, 576) | (352, 288) | (176, 144) => Some("12:11"),
        _ => None,
    }
}

/// 方像素光栅的显示画幅 = 存储画幅,但**吸附到常见电视画幅**。
/// 年代轴的尺寸是插值出来的(960×720 干净,724×532 这种就不干净),照实约分会得到
/// `181:133` 这类比值 —— 引擎的 `ratio()` 认为它超出合理范围,整张预设直接编译失败。
/// 宁可差 1% 画幅,也不要一个跑不出来的预设。
fn square_dar(w: u32, h: u32) -> String {
    if w == 0 || h == 0 {
        return "1:1".into();
    }
    let r = w as f64 / h as f64;
    // 候选 = 清单里 `dar` 的选项 ∪ 方像素常见的 5:4 / 3:2(它们离 15:11 / 13:9 还差 5% 以上,
    // 只按清单吸附会把画幅折错)
    const STD: [(&str, f64); 7] = [
        ("1:1", 1.0),
        ("5:4", 1.25),
        ("15:11", 15.0 / 11.0),
        ("4:3", 4.0 / 3.0),
        ("13:9", 13.0 / 9.0),
        ("3:2", 1.5),
        ("16:9", 16.0 / 9.0),
    ];
    STD.iter()
        .min_by(|a, b| (a.1.ln() - r.ln()).abs().partial_cmp(&(b.1.ln() - r.ln()).abs()).unwrap())
        .map(|x| x.0.to_string())
        .unwrap_or_else(|| "4:3".into())
}

#[derive(Debug, Clone, Copy)]
struct Params {
    w: f64,
    h: f64,
    fps: f64,
    noise: f64,
    sat: f64,
    contrast: f64,
    chroma_bleed: f64,
    interlace: f64,
    fade: f64,
    scratches: f64,
    dust: f64,
    flicker: f64,
    audio_hp: f64,
    audio_lp: f64,
    hiss: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Codec {
    None,
    Mpeg4(u32),
    H263(u32),
    X264(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct Anchor {
    year: u32,
    p: Params,
    codec: Codec,
    /// 音频是否走码率往返(DVD 时代)
    audio_bitrate: Option<&'static str>,
    resample: Option<u32>,
    bitcrush: bool,
}

const ANCHORS: &[Anchor] = &[
    Anchor { // 1965 胶片
        year: 1965,
        p: Params { w: 960.0, h: 720.0, fps: 18.0, noise: 22.0, sat: 0.55, contrast: 0.95,
            chroma_bleed: 0.0, interlace: 0.0, fade: 0.85, scratches: 0.75, dust: 0.8, flicker: 0.7,
            audio_hp: 120.0, audio_lp: 3500.0, hiss: 0.02 },
        codec: Codec::None, audio_bitrate: None, resample: None, bitcrush: false,
    },
    Anchor { // 1978 早期家用录像带
        year: 1978,
        p: Params { w: 720.0, h: 480.0, fps: 30.0, noise: 14.0, sat: 0.8, contrast: 1.05,
            chroma_bleed: 0.6, interlace: 1.0, fade: 0.35, scratches: 0.2, dust: 0.15, flicker: 0.2,
            audio_hp: 100.0, audio_lp: 7000.0, hiss: 0.015 },
        codec: Codec::None, audio_bitrate: None, resample: None, bitcrush: false,
    },
    Anchor { // 1990 VHS 巅峰
        year: 1990,
        p: Params { w: 720.0, h: 480.0, fps: 30.0, noise: 10.0, sat: 0.85, contrast: 1.06,
            chroma_bleed: 0.5, interlace: 1.0, fade: 0.12, scratches: 0.08, dust: 0.05, flicker: 0.08,
            audio_hp: 120.0, audio_lp: 8000.0, hiss: 0.012 },
        codec: Codec::None, audio_bitrate: None, resample: None, bitcrush: false,
    },
    Anchor { // 1998 SVHS/DV 过渡
        year: 1998,
        p: Params { w: 720.0, h: 480.0, fps: 30.0, noise: 5.0, sat: 0.95, contrast: 1.02,
            chroma_bleed: 0.2, interlace: 0.0, fade: 0.0, scratches: 0.0, dust: 0.0, flicker: 0.0,
            audio_hp: 60.0, audio_lp: 14000.0, hiss: 0.0 },
        codec: Codec::None, audio_bitrate: None, resample: None, bitcrush: false,
    },
    Anchor { // 2004 翻录 DVD / 网吧低清
        year: 2004,
        p: Params { w: 640.0, h: 360.0, fps: 25.0, noise: 6.0, sat: 0.92, contrast: 1.02,
            chroma_bleed: 0.0, interlace: 0.0, fade: 0.0, scratches: 0.0, dust: 0.0, flicker: 0.0,
            audio_hp: 40.0, audio_lp: 15000.0, hiss: 0.0 },
        codec: Codec::Mpeg4(14), audio_bitrate: Some("64k"), resample: None, bitcrush: false,
    },
    Anchor { // 2008 3GP 彩屏手机
        year: 2008,
        p: Params { w: 176.0, h: 144.0, fps: 15.0, noise: 6.0, sat: 1.15, contrast: 1.05,
            chroma_bleed: 0.0, interlace: 0.0, fade: 0.0, scratches: 0.0, dust: 0.0, flicker: 0.0,
            audio_hp: 300.0, audio_lp: 3400.0, hiss: 0.0 },
        codec: Codec::H263(25), audio_bitrate: None, resample: Some(8000), bitcrush: true,
    },
    Anchor { // 2013 早期智能手机 / 网页 360P
        year: 2013,
        p: Params { w: 640.0, h: 360.0, fps: 24.0, noise: 4.0, sat: 1.02, contrast: 1.0,
            chroma_bleed: 0.0, interlace: 0.0, fade: 0.0, scratches: 0.0, dust: 0.0, flicker: 0.0,
            audio_hp: 40.0, audio_lp: 15000.0, hiss: 0.0 },
        codec: Codec::X264("250k"), audio_bitrate: None, resample: None, bitcrush: false,
    },
    Anchor { // 2026 当代(轴终点 = 原片)
        year: 2026,
        p: Params { w: 1920.0, h: 1080.0, fps: 30.0, noise: 0.0, sat: 1.0, contrast: 1.0,
            chroma_bleed: 0.0, interlace: 0.0, fade: 0.0, scratches: 0.0, dust: 0.0, flicker: 0.0,
            audio_hp: 20.0, audio_lp: 20000.0, hiss: 0.0 },
        codec: Codec::None, audio_bitrate: None, resample: None, bitcrush: false,
    },
];

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub const ERA_MIN: u32 = 1965;
pub const ERA_MAX: u32 = 2026;

/// 年 → 插值参数(取最近锚点的离散字段)
pub fn params_for_year(year: u32) -> (Params, &'static Anchor) {
    let year = year.clamp(ERA_MIN, ERA_MAX);
    let idx = ANCHORS
        .iter()
        .rposition(|a| a.year <= year)
        .unwrap_or(0)
        .min(ANCHORS.len() - 2);
    let (a, b) = (&ANCHORS[idx], &ANCHORS[idx + 1]);
    let t = (year - a.year) as f64 / (b.year - a.year) as f64;
    let pa = a.p;
    let pb = b.p;
    let p = Params {
        w: lerp(pa.w, pb.w, t),
        h: lerp(pa.h, pb.h, t),
        fps: lerp(pa.fps, pb.fps, t),
        noise: lerp(pa.noise, pb.noise, t),
        sat: lerp(pa.sat, pb.sat, t),
        contrast: lerp(pa.contrast, pb.contrast, t),
        chroma_bleed: lerp(pa.chroma_bleed, pb.chroma_bleed, t),
        interlace: lerp(pa.interlace, pb.interlace, t),
        fade: lerp(pa.fade, pb.fade, t),
        scratches: lerp(pa.scratches, pb.scratches, t),
        dust: lerp(pa.dust, pb.dust, t),
        flicker: lerp(pa.flicker, pb.flicker, t),
        audio_hp: lerp(pa.audio_hp, pb.audio_hp, t),
        audio_lp: lerp(pa.audio_lp, pb.audio_lp, t),
        hiss: lerp(pa.hiss, pb.hiss, t),
    };
    let nearest = if t < 0.5 { a } else { b };
    (p, nearest)
}

/// 生成年代轴预设(数字字段按 §5.3 联动规则装配)
pub fn preset_for_year(year: u32) -> Preset {
    // 只在入口 clamp 一次。以前这里用原始 year 判 ntsc 窗口、写 id/name/era 与 seed,
    // 而 params_for_year 内部另夹一遍 —— 传 1900 会得到"年代轴 ~1900"的名字却交付 1965 的观感,
    // 传超大年份还会让 `year as i32` 翻成负种子。一个量不能有两份取值。
    let year = year.clamp(ERA_MIN, ERA_MAX);
    let (p, nearest) = params_for_year(year);
    let mut video = vec![];
    let mut audio = vec![];

    if p.fade > 0.03 {
        video.push(VideoStage::ColorFade { strength: p.fade });
    }
    if p.scratches > 0.03 || p.dust > 0.03 || p.flicker > 0.03 {
        video.push(VideoStage::FilmDamage {
            seed: year as i32,
            scratches: p.scratches,
            dust: p.dust,
            flicker: p.flicker,
        });
    }
    let (pw, ph) = (p.w.round() as u32, p.h.round() as u32);
    // SD 光栅存的是非方像素,并且必须先按显示画幅裁切再缩放(§13.3)
    let par = par_for(pw, ph);
    // 查不到 par ≠ 不用给 dar。方像素时显示画幅就是存储画幅;从前两者一起留空,
    // 于是 1965/1972 的 960×720 从 16:9 素材进来根本不曾裁切,直接被横向压扁(实测:人脸变瘦 + 上下加边)。
    let sd_dar = match par {
        Some(x) => dar_from_par(x, pw, ph).ok(),
        None => Some(square_dar(pw, ph)),
    };
    video.push(VideoStage::Resize {
        w: Some(pw),
        h: Some(ph),
        mode: None,
        flags: "bicubic".into(),
        dar: sd_dar.clone(),
        fit: Some("crop".into()),
        par: par.map(str::to_string),
        overscan: None,
        range: None,
    });
    // 帧率一律写成 stage(§13.2:过去只在 <29.5 时才写,导致 30fps 时代的帧率无处可调)
    video.push(VideoStage::Fps {
        fps: p.fps.round(),
        round: "down".into(),
        shutter: 0.0,
        cadence: None,
    });
    // 1982-1998 磁带回放窗口:ntscrs 信号级算子接管低通/串扰/梳齿/噪声
    let ntsc = p.interlace > 0.5 && (1982..=1998).contains(&year);
    let noise = if ntsc { (p.noise - 6.0).max(0.0) } else { p.noise };
    if noise > 0.5 {
        video.push(VideoStage::Noise { alls: noise.round() as u32, allf: "t+u".into() });
    }
    video.push(VideoStage::Color {
        saturation: Some((p.sat * 100.0).round() / 100.0),
        contrast: Some((p.contrast * 100.0).round() / 100.0),
        brightness: None,
        green_mid: None,
        blue_mid: None,
    });
    if ntsc {
        // S1 色彩空间劣化 + 信号级模拟接管低通/串扰/梳齿
        video.push(VideoStage::ChromaDecimate { to: "411".into() });
        video.push(VideoStage::MatrixRoundtrip {});
        video.push(VideoStage::NtscVhs { seed: year as i32, knobs: Default::default() });
        video.push(VideoStage::TapeEnds { head: 0.6, tail: 0.6, intensity: 30 });
    } else {
        if p.chroma_bleed > 0.05 {
            video.push(VideoStage::AsymLowpass {
                luma_radius: 1.0 + p.chroma_bleed,
                chroma_radius: 3.0 + p.chroma_bleed * 6.0,
            });
        }
        if p.interlace > 0.5 {
            video.push(VideoStage::InterlaceComb { mode: "merge".into(), refps: p.fps.round() });
        }
    }
    match nearest.codec {
        Codec::Mpeg4(q) => video.push(VideoStage::CodecRoundtrip {
            codec: "mpeg4".into(),
            q,
            bitrate: None,
            container: Some("mp4".into()),
            audio_bitrate: nearest.audio_bitrate.map(str::to_string),
            video_only: false,
        }),
        Codec::H263(q) => video.push(VideoStage::CodecRoundtrip {
            codec: "h263".into(),
            q,
            bitrate: None,
            container: Some("avi".into()),
            audio_bitrate: None,
            video_only: true,
        }),
        Codec::X264(b) => video.push(VideoStage::CodecRoundtrip {
            codec: "libx264".into(),
            q: 0,
            bitrate: Some(b.to_string()),
            container: Some("mp4".into()),
            audio_bitrate: None,
            video_only: false,
        }),
        Codec::None => {}
    }
    // 交付画布回到源尺寸:SD 年代加黑边(不拉伸变形),像素比显式复位方形
    video.push(VideoStage::Resize {
        w: None,
        h: None,
        mode: Some("source_canvas".into()),
        flags: "bicubic".into(),
        dar: sd_dar,
        fit: Some("pad".into()),
        par: Some("1:1".into()),
        overscan: None,
        range: None,
    });

    audio.push(AudioStage::Bandlimit {
        highpass: p.audio_hp.round() as u32,
        lowpass: p.audio_lp.round().min(20000.0) as u32,
    });
    if let Some(b) = nearest.audio_bitrate {
        audio.push(AudioStage::BitrateRoundtrip { bitrate: b.to_string() });
    }
    if let Some(r) = nearest.resample {
        audio.push(AudioStage::ResampleRoundtrip { rate: r });
    }
    if nearest.bitcrush {
        audio.push(AudioStage::Bitcrush { bits: 8, mode: "log".into(), aa: 0.5 });
    }
    if p.hiss > 0.002 {
        audio.push(AudioStage::TapeHiss {
            color: if year < 1972 { "brown".into() } else { "pink".into() },
            amplitude: p.hiss,
            mix_weight: 0.45,
        });
    }
    // 磁带回放窗口:抖晃 + 单声道;更早的胶片时代也是单声道(§5.2 预设)
    if ntsc {
        audio.push(AudioStage::WowFlutter { freq: 0.7, depth: 0.02 });
    }
    if ntsc || year < 1978 {
        audio.push(AudioStage::Mono {});
    }

    Preset {
        id: format!("era_{year}"),
        name: format!("年代轴 ~{year}"),
        era: year,
        schema: 1,
        note: Some("年代轴自动生成的联动参数".into()),
        video,
        audio,
        calibrated: None,
        aging: None,
        cast: None,
        variant_of: None,
        variant: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::Preset;

    #[test]
    fn axis_is_monotonic_toward_clean() {
        let noise = |y: u32| params_for_year(y).0.noise;
        assert!(noise(1970) > noise(1990) && noise(1990) > noise(2010) && noise(2010) > noise(2024));
        let bleed = |y: u32| params_for_year(y).0.chroma_bleed;
        assert!(bleed(1985) > bleed(2005));
    }

    #[test]
    fn sd_anchors_use_non_square_pixels_and_delivery_resets_to_square() {
        // 720x480 按方形像素交付会把人拉扁——非方像素是"像不像老片"的第一因子(§13.3)
        for y in [1978u32, 1990, 1998, 2008] {
            let p = preset_for_year(y);
            let rs: Vec<&VideoStage> =
                p.video.iter().filter(|s| matches!(s, VideoStage::Resize { .. })).collect();
            assert!(rs.len() >= 2, "{y}: 应有 SD 段与交付段两次 resize");
            let VideoStage::Resize { par, dar, fit, .. } = rs[0] else { unreachable!() };
            assert!(par.is_some() && dar.is_some(), "{y}: SD 光栅必须带像素比与显示画幅");
            assert_eq!(fit.as_deref(), Some("crop"), "{y}: 先按画幅裁切");
            let VideoStage::Resize { par: tail_par, mode, .. } = rs[rs.len() - 1] else { unreachable!() };
            assert_eq!(mode.as_deref(), Some("source_canvas"));
            assert_eq!(tail_par.as_deref(), Some("1:1"), "{y}: 交付要显式复位方形像素");
        }
        // 数码宽屏年代不该被硬塞非方像素 —— 但**画幅必须写**:从前这一档 par/dar 双双留空,
        // 于是裁切根本没发生,16:9 素材被压扁进 960×720(实测:人脸变瘦 + 交付再上下加边)。
        for y in [1965u32, 2013, 2026] {
            let p = preset_for_year(y);
            let sd = p.video.iter().find(|s| matches!(s, VideoStage::Resize { w: Some(_), .. }));
            let VideoStage::Resize { par, dar, fit, w, h, .. } = sd.unwrap() else { unreachable!() };
            assert!(par.is_none(), "{y}: 方像素年代不该改像素比");
            assert!(dar.is_some(), "{y}: 交付画幅必须写出来,否则不裁切、直接压扁");
            assert_eq!(fit.as_deref(), Some("crop"), "{y}: 先按画幅裁切");
            if y == 1965 {
                assert_eq!((*w, *h), (Some(960), Some(720)), "1965 锚点尺寸变了要同步这条");
                assert_eq!(dar.as_deref(), Some("4:3"), "{y}: 方像素下 dar = 存储画幅");
            }
        }
    }

    /// 每一个年代都要"能编译 + 不压扁":交付段必须带 dar,16:9 素材进来必须真的裁切。
    /// 从前只有 `par_for` 查得到的那几个年代才有画幅字段,1965/1972 系于是直接把画面压扁,
    /// 而插值出来的怪尺寸(724×532 那种)约分后连 `ratio()` 都过不了 —— 这条把两件事一起钉住。
    #[test]
    fn every_year_compiles_and_never_squashes() {
        use crate::ffgraph::{build_plan, MediaInfo};
        let src = MediaInfo {
            width: 1280,
            height: 720,
            fps: 25.0,
            duration: 4.0,
            has_audio: true,
            container: "mp4".into(),
        };
        let mut cropped = 0usize;
        for y in 1965..=2026u32 {
            let p = preset_for_year(y);
            let plan = build_plan(&p, &src, None, None).unwrap_or_else(|e| panic!("{y}: {e}"));
            assert!(
                p.video.iter().any(|s| matches!(s, VideoStage::Resize { dar: Some(_), .. })),
                "{y}: 交付段没有 dar → 不裁切,直接压扁"
            );
            if plan.geom.iter().any(|f| f.starts_with("crop=")) {
                cropped += 1;
            }
        }
        assert_eq!(cropped, 62, "有年代没真的裁切画面(压扁回去了)");
    }

    #[test]
    fn generated_presets_roundtrip_through_loader() {
        for y in [1965u32, 1972, 1990, 2004, 2008, 2013, 2026] {
            let p = preset_for_year(y);
            let json = serde_json::to_string(&serde_json::json!({
                "id": p.id, "name": p.name, "era": p.era, "schema": p.schema,
                "video": p.video, "audio": p.audio,
            }))
            .unwrap();
            let file = std::env::temp_dir().join(format!("era_test_{y}.json"));
            std::fs::write(&file, &json).unwrap();
            let back = Preset::load(&file).unwrap_or_else(|e| panic!("{y}: {e}"));
            assert_eq!(back.video.len(), p.video.len());
            let _ = std::fs::remove_file(&file);
        }
    }

    #[test]
    fn every_anchor_carries_an_editable_fps_stage() {
        // 过去 fps>=29.5 的锚点根本不写 stage,界面里 30fps 时代无从下手(§13.2)
        for y in [1965u32, 1972, 1978, 1988, 1990, 1998, 2004, 2008, 2013, 2020, 2026] {
            let p = preset_for_year(y);
            let n = p
                .video
                .iter()
                .filter(|s| matches!(s, VideoStage::Fps { .. }))
                .count();
            assert_eq!(n, 1, "{y} 年应有且只有一个 fps stage: {:?}", p.video);
            for s in &p.video {
                if let VideoStage::Fps { fps, round, cadence, .. } = s {
                    assert!(*fps > 0.0 && *fps <= 60.0, "{y}: fps={fps}");
                    assert_eq!(round, "down", "{y}: 丢帧不补帧是默认口径");
                    assert!(cadence.is_none(), "{y}: 节奏默认关");
                }
            }
        }
    }

    #[test]
    fn era_1990_uses_ntscrs_signal() {
        let p = preset_for_year(1990);
        let j = serde_json::to_string(&p.video).unwrap();
        assert!(j.contains("ntsc_vhs"), "1990 应走信号级模拟: {j}");
        assert!(!j.contains("interlace_comb"), "ntscrs 已含梳齿,不再叠加 ffmpeg 近似");
        assert!(p.audio.iter().any(|a| serde_json::to_string(a).unwrap().contains("tape_hiss")));
    }

    #[test]
    fn era_1978_pre_ntsc_uses_ffmpeg_approx() {
        let p = preset_for_year(1978);
        let j = serde_json::to_string(&p.video).unwrap();
        assert!(j.contains("interlace_comb"), "1982 前窗口外仍用 ffmpeg 近似");
    }

    #[test]
    fn era_1990_audio_recipe_has_tape_character() {
        let j = serde_json::to_string(&preset_for_year(1990).audio).unwrap();
        assert!(j.contains("wow_flutter") && j.contains("\"mono\"") && j.contains("tape_hiss"), "{j}");
    }

    #[test]
    fn era_2008_uses_real_h263() {
        let p = preset_for_year(2008);
        let j = serde_json::to_string(&p.video).unwrap();
        assert!(j.contains("h263") && j.contains("\"video_only\":true"), "{j}");
        assert!(j.contains("source_canvas"), "QCIF 后必须拉回源画布");
    }

    #[test]
    fn era_2013_uses_h264_bitrate_mode() {
        let p = preset_for_year(2013);
        let j = serde_json::to_string(&p.video).unwrap();
        assert!(j.contains("libx264") && j.contains("250k"), "{j}");
    }

    /// 年份只有一个取值点:越界输入必须整体落到边界上,而不是"名字写 1900、观感却是 1965"。
    /// 以前 params_for_year 内部夹、preset_for_year 用原始 year 判 ntsc 窗口并写 id/name/seed,
    /// 于是同一个量有两份答案;超大年份还会让 `year as i32` 翻出负种子。
    #[test]
    fn out_of_range_years_collapse_to_the_boundary_everywhere() {
        let lo = preset_for_year(1900);
        assert_eq!(lo.id, format!("era_{ERA_MIN}"), "id 用了未夹取的年份: {}", lo.id);
        assert_eq!(lo.era, ERA_MIN);
        assert!(lo.name.contains(&ERA_MIN.to_string()), "名字与实际观感不一致: {}", lo.name);
        assert_eq!(lo.video.len(), preset_for_year(ERA_MIN).video.len(), "夹取前后不是同一套参数");
        let hi = preset_for_year(3_000_000_000);
        assert_eq!(hi.era, ERA_MAX, "上限没夹住: {}", hi.era);
        assert!(
            !serde_json::to_string(&hi.video).unwrap().contains("seed\":-"),
            "超大年份翻出了负种子"
        );
    }
}
