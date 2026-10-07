//! PixelPath:ffmpeg 解码 rawvideo 管道 -> 像素算子序列 -> ffmpeg 编码管道,零中间文件。
//! 音频从输入文件旁路拷贝(滤镜属于相邻的 Fast 趟)。

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::sync_channel;
use std::thread;

use ntsc_rs::ctx;
use ntsc_rs::settings::{NtscEffect, SettingsBlock, VHSSettings, VHSTapeSpeed};
use crate::errcode::{coded, ENGINE_FRAME, ENGINE_PASS, ENGINE_SPAWN};
use crate::preset::NtscKnobs;

/// 把界面上拧动的信号级旋钮落到 `NtscEffect` 上。
///
/// 三条规矩:
/// ① `None` 一律不碰 —— 上游默认值就是今天成品里的样子(闸用 md5 钉住);
/// ② 带速 0 = "不用磁带模型",这时整个 VHS 块关掉(不是把枚举设成 NONE 还留着别的 VHS 处理);
/// ③ 给了某个块的参数就顺手把这个块打开 —— 上游 `SettingsBlock::default()` 是 enabled: true,
///    今天这些块本来就开着;这条只是保证以后谁把默认改成关着时,"拧了没反应"不会重现。
fn apply_ntsc_knobs(e: &mut NtscEffect, k: &NtscKnobs) {
    if let Some(v) = k.vhs_tape_speed {
        let n = v.round() as u32;
        if n == 0 {
            e.vhs_settings.enabled = false;
        } else {
            e.vhs_settings.enabled = true;
            e.vhs_settings.settings.tape_speed = match n {
                1 => VHSTapeSpeed::SP,
                2 => VHSTapeSpeed::LP,
                3 => VHSTapeSpeed::EP,
                // 清单区间是 0-3;越界到这里说明有人绕过了校验,按 LP(今天的默认)处理
                _ => VHSTapeSpeed::LP,
            };
        }
    }
    if let Some(v) = k.vhs_chroma_loss {
        e.vhs_settings.settings.chroma_loss = v as f32;
    }
    if let Some(v) = k.vhs_sharpen {
        e.vhs_settings.settings.sharpen.enabled = true;
        e.vhs_settings.settings.sharpen.settings.intensity = v as f32;
    }
    if let Some(v) = k.vhs_edge_wave {
        e.vhs_settings.settings.edge_wave.enabled = true;
        e.vhs_settings.settings.edge_wave.settings.intensity = v as f32;
    }
    if let Some(v) = k.tracking_noise_height {
        e.tracking_noise.enabled = true;
        e.tracking_noise.settings.height = v.round() as i32;
    }
    if let Some(v) = k.tracking_noise_wave_intensity {
        e.tracking_noise.enabled = true;
        e.tracking_noise.settings.wave_intensity = v as f32;
    }
    if let Some(v) = k.head_switching_height {
        e.head_switching.enabled = true;
        e.head_switching.settings.height = v.round() as i32;
    }
    if let Some(v) = k.head_switching_horizontal_shift {
        e.head_switching.enabled = true;
        e.head_switching.settings.horiz_shift = v as f32;
    }
    if let Some(v) = k.snow {
        e.snow_intensity = v as f32;
    }
    if let Some(v) = k.luma_smear {
        e.luma_smear = v as f32;
    }
    if let Some(v) = k.chroma_delay_horizontal {
        e.chroma_delay_horizontal = v as f32;
    }
    if let Some(v) = k.chroma_delay_vertical {
        e.chroma_delay_vertical = v.round() as i32;
    }
}
use ntsc_rs::yiq_fielding::Rgb;

use crate::ffgraph::{MediaInfo, PixelOp, PixelSeg, num};
use crate::ffrun::{emit, ffmpeg_bin, Prog};

enum Prepared {
    Ntsc(NtscEffect),
    Crt {
        scanline: f64,
        barrel: f64,
        aberration: f64,
        persistence: f64,
        prev: std::cell::RefCell<Vec<u8>>,
    },
    Film { seed: i32, scratches: f64, dust: f64, flicker: f64 },
}

pub fn run_pixel_seg(
    seg: &PixelSeg,
    input: &Path,
    out: &Path,
    media: &MediaInfo,
    limit: Option<f64>,
    seek: f64,
    prog: &Prog,
) -> Result<(), String> {
    let (w, h) = (seg.w as usize, seg.h as usize);
    let frame_bytes = w * h * 3;
    // 段生效帧率必须在解码链兑现:此前像素段一律按源帧率走,fps 参数是装饰(§13.2)
    let fps = seg.fps.as_ref().map(|f| f.fps).unwrap_or(media.fps).max(1.0);
    let dur = limit.unwrap_or(media.duration).max(0.001);
    // 静帧的时长是 0 → total 会算成 0 → pct = inf → serde 吐 null,进度条拿到非数字
    let total = ((dur * fps) as u64).max(1);
    let mut dec_vf = String::new();
    if let Some(st) = &seg.fps {
        dec_vf.push_str(&format!("fps={}:round={}:eof_action=pass,", num(st.fps), st.round));
    }
    dec_vf.push_str(&format!("scale={w}:{h}:flags=bicubic"));
    if let Some(st) = &seg.fps {
        if st.shutter > 0.0 {
            // 快门模糊接在丢帧之后:模糊长度随帧间隔放大,这才是"旧"而不是"卡"
            dec_vf.push_str(&format!(",tmix=frames=2:weights='{} 1'", num(st.shutter.min(1.0))));
        }
    }
    dec_vf.push_str(",format=rgb24");

    // 解码侧
    let mut dec_args: Vec<String> =
        vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostats".into()];
    if seek > 0.01 {
        dec_args.extend(["-ss".into(), num(seek)]);
    }
    if let Some(l) = limit {
        dec_args.extend(["-t".into(), num(l)]);
    }
    dec_args.extend([
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-vf".into(),
        dec_vf,
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-".into(),
    ]);
    let mut dec = Command::new(ffmpeg_bin())
        .args(&dec_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| coded(ENGINE_SPAWN, format!("解码进程启动失败: {e}")))?;

    // 编码侧:0=rawvideo 管道,1=输入文件(音频旁路)
    let mut enc_args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostats".into(),
        "-y".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-s".into(),
        format!("{w}x{h}"),
        "-r".into(),
        num(fps),
        "-i".into(),
        "-".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-map".into(),
        "0:v".into(),
        "-map".into(),
        "1:a?".into(),
        "-c:a".into(),
        "copy".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        "-crf".into(),
        "14".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-progress".into(),
        "pipe:1".into(),
    ];
    if let Some(a) = &seg.aspect {
        // rawvideo 不带 SAR 元数据,画幅只能在编码侧用 -aspect 钉住
        enc_args.extend(["-aspect".into(), a.clone()]);
    }
    if let Some(l) = limit {
        // 输出级 -t:防止解码侧已限长而 mux 的音频仍是全片长(§8.1-16)
        enc_args.extend(["-t".into(), num(l)]);
    }
    enc_args.push(out.to_string_lossy().into_owned());

    let mut enc = Command::new(ffmpeg_bin())
        .args(&enc_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| coded(ENGINE_SPAWN, format!("编码进程启动失败: {e}")))?;

    let (tx, rx) = sync_channel::<Vec<u8>>(3);
    let mut enc_in = enc.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        for f in rx {
            if enc_in.write_all(&f).is_err() {
                break;
            }
        }
        let _ = enc_in.flush();
    });

    let prepared: Vec<Prepared> = seg
        .ops
        .iter()
        .map(|op| match op {
            PixelOp::Ntsc { seed, knobs } => {
                let mut effect = NtscEffect::default();
                effect.vhs_settings =
                    SettingsBlock { enabled: true, settings: VHSSettings::default() };
                apply_ntsc_knobs(&mut effect, knobs);
                effect.random_seed = *seed;
                Prepared::Ntsc(effect)
            }
            PixelOp::Crt { scanline, barrel, aberration, persistence } => {
                Prepared::Crt {
                    scanline: *scanline,
                    barrel: *barrel,
                    aberration: *aberration,
                    persistence: *persistence,
                    prev: std::cell::RefCell::new(Vec::new()),
                }
            }
            PixelOp::Film { seed, scratches, dust, flicker } => {
                Prepared::Film { seed: *seed, scratches: *scratches, dust: *dust, flicker: *flicker }
            }
        })
        .collect();

    let dec_out = dec.stdout.take().unwrap();
    let mut reader = std::io::BufReader::with_capacity(frame_bytes * 2, dec_out);
    let mut buf = vec![0u8; frame_bytes];
    let mut frame_num: usize = 0;
    let mut last_emit = -100.0f64;
    loop {
        match read_frame(&mut reader, &mut buf) {
            FrameRead::Full => {}
            FrameRead::Eof => break,
            FrameRead::Err(e) => return Err(coded(ENGINE_FRAME, format!("解码流读取失败: {e}"))),
        }
        for p in prepared.iter() {
            match p {
                Prepared::Ntsc(e) => e.apply_effect_to_buffer::<Rgb, u8>(
                    ctx::global(),
                    (w, h),
                    &mut buf,
                    frame_num,
                    [1.0, 1.0],
                ),
                Prepared::Crt { scanline, barrel, aberration, persistence, prev } => {
                    crate::pixops::crt(&mut buf, w, h, *scanline, *barrel, *aberration);
                    if *persistence > 0.01 {
                        crate::pixops::phosphor(&mut buf, &prev.borrow(), *persistence);
                        prev.replace(buf.clone());
                    }
                }
                Prepared::Film { seed, scratches, dust, flicker } => {
                    crate::pixops::film_damage(
                        &mut buf,
                        w,
                        h,
                        *seed as u64,
                        frame_num,
                        *scratches,
                        *dust,
                        *flicker,
                    )
                }
            }
        }
        frame_num += 1;
        let pct = frame_num as f64 / total as f64 * 100.0;
        if pct - last_emit >= 2.0 {
            emit(prog, pct);
            last_emit = pct;
        }
        if tx.send(std::mem::take(&mut buf)).is_err() {
            return Err(coded(ENGINE_PASS, "编码进程提前退出(管道断裂)").into());
        }
        buf = vec![0u8; frame_bytes];
    }
    drop(tx);
    writer.join().map_err(|_| coded(ENGINE_PASS, "写线程 panic"))?;
    emit(prog, 100.0);

    let ds = dec.wait().map_err(|e| e.to_string())?;
    let es = enc.wait().map_err(|e| e.to_string())?;
    if !es.success() {
        return Err(coded(ENGINE_PASS, format!("编码失败 exit {:?} (解码 exit {:?})", es.code(), ds.code())));
    }
    // 编码器成功不代表解码器走完了:解码中途死掉时这里会产出一段截短的视频而无人报错。
    // 顺序要紧 —— 编码先失败时解码往往是管道断裂,那不该顶掉真正的错因
    if !ds.success() {
        return Err(coded(ENGINE_FRAME, format!("解码失败 exit {:?}(帧数 {})", ds.code(), frame_num)));
    }
    match std::fs::metadata(out) {
        Ok(m) if m.len() > 0 => Ok(()),
        _ => Err(coded(ENGINE_PASS, format!("{out:?} 没有产出非空文件(像素段吃了 {} 帧)", frame_num))),
    }
}

enum FrameRead {
    Full,
    Eof,
    Err(std::io::Error),
}

fn read_frame<R: Read>(r: &mut R, buf: &mut [u8]) -> FrameRead {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => {
                return if filled == 0 {
                    FrameRead::Eof
                } else {
                    FrameRead::Err(std::io::Error::other("帧不完整"))
                };
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return FrameRead::Err(e),
        }
    }
    FrameRead::Full
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旋钮落值的三条规矩(见 apply_ntsc_knobs 的注释)必须有测试钉住:
    /// 全 None 不许动任何字段;带速 0 是"关掉整个 VHS 块";默认关着的块只在被拧时打开。
    #[test]
    fn knobs_only_touch_what_is_set() {
        let base = {
            let mut e = NtscEffect::default();
            e.vhs_settings = SettingsBlock { enabled: true, settings: VHSSettings::default() };
            e
        };
        let mut same = base.clone();
        apply_ntsc_knobs(&mut same, &NtscKnobs::default());
        assert_eq!(same, base, "一个都没拧时不许改任何字段");

        let mut off = base.clone();
        apply_ntsc_knobs(&mut off, &NtscKnobs { vhs_tape_speed: Some(0.0), ..Default::default() });
        assert!(!off.vhs_settings.enabled, "带速 0 应该整个关掉 VHS 块");

        let mut hs = base.clone();
        apply_ntsc_knobs(
            &mut hs,
            &NtscKnobs { head_switching_height: Some(16.0), ..Default::default() },
        );
        assert!(hs.head_switching.enabled, "给了磁头切换高度就该打开这一块");
        assert_eq!(hs.head_switching.settings.height, 16);
        // 上游的块本来就开着(SettingsBlock::default = enabled: true):没拧跟踪噪声,它的值不许跟着变
        assert_eq!(
            hs.tracking_noise.settings.height,
            base.tracking_noise.settings.height,
            "没拧跟踪噪声却被改了值"
        );

        let mut sp = base.clone();
        apply_ntsc_knobs(&mut sp, &NtscKnobs { vhs_tape_speed: Some(3.0), ..Default::default() });
        assert!(sp.vhs_settings.enabled);
        assert_eq!(sp.vhs_settings.settings.tape_speed, VHSTapeSpeed::EP);
    }
}
