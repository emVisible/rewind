use crate::errcode::{coded, PARAM_RANGE, PRESET_LOAD};
use crate::preset::{AudioStage, NtscKnobs, Preset, VideoStage};

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub has_audio: bool,
    /// ffprobe 的 format_name(如 `mov,mp4,m4a,3gp,3g2,mj2` / `png_pipe` / `h264`)
    pub container: String,
}

/// 静态图片的 muxer。只用来配合"探不到时长"一起判断静帧,见 `is_image`。
/// `pub(crate)`:`inspect`(界面素材报告)必须用**同一条**判据,不能再手写第三份。
pub(crate) fn still_container(c: &str) -> bool {
    const STILLS: [&str; 14] = [
        "png", "jpg", "jpeg", "webp", "bmp", "tiff", "tif", "apng", "gif", "image2", "libopenjpeg",
        "ppm", "pgm", "pbm",
    ];
    STILLS.iter().any(|k| c.starts_with(k))
}

impl MediaInfo {
    /// 静帧要**同时**满足"探不到时长"和"容器是静态图片 muxer"。
    /// 只看时长会误判:实测裸 H.264 流(`.264`)ffprobe 报 duration=0,于是整条管线把它当静帧 ——
    /// 时序 stage 被剥掉、`-frames:v 1` 只出一帧、成品是塞进 `.264` 文件名的 PNG。
    pub fn is_image(&self) -> bool {
        self.duration <= 0.0 && still_container(&self.container)
    }
}

/// 引擎**硬**可用区间:与"界面档位"是两回事。档位是建议位置(fps 5–60、级宽 8–48),
/// 硬区间是管线还能照办的范围。`--override` 拦的是硬区间(清单里的 hard_min/hard_max),
/// 报错文案也引用这两个常量 —— 同一组数字只许有一处定义(写过两遍就一定会漂)。
pub const FPS_MIN: f64 = 1.0;
pub const FPS_MAX: f64 = 120.0;
pub const LEVEL_MIN: u32 = 1;
pub const LEVEL_MAX: u32 = 64;

/// 区间的一句话。清单与报错共用,免得一处写 "1–120" 另一处写 "1-120"。
pub fn range_text(lo: f64, hi: f64) -> String {
    format!("{}–{}", num(lo), num(hi))
}

/// 数字格式化:去掉无意义的尾零(15.0 -> "15")
pub fn num(x: f64) -> String {    if (x - x.round()).abs() < 1e-9 && x.abs() < 1e15 {
        format!("{}", x.round() as i64)
    } else {
        format!("{x}")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioMode {
    /// 不处理音频(-an)
    None,
    /// 直接拷贝输入文件的音轨(-c:a copy)
    Copy,
    Encode {
        /// true = 音频取自原始输入(第二输入),false = 取自上一趟临时文件
        from_original: bool,
        bitrate: Option<String>,
        /// (lavfi 输入参数, 混音权重) = 磁带底噪
        hiss: Option<(String, f64)>,
    },
}

#[derive(Debug, Clone)]
pub struct Pass {
    pub fx_v: Vec<String>,
    /// 音频滤镜链(独立于 fx_v,仅 Encode 模式生效)
    pub fx_a: Vec<String>,
    pub audio: AudioMode,
    /// 附加输入(如 anoisesrc),完整参数对 ["-f","lavfi","-i","..."]
    pub extra_inputs: Vec<Vec<String>>,
    /// 输出级附加参数(如 -metadata 水印),仅末趟使用
    pub out_extra: Vec<String>,
    pub vcodec: Vec<String>,
    pub out_ext: String,
}

/// §10 反滥用承诺:输出默认嵌入"做旧处理"元数据水印(版本号取自 crate,别写死)
pub fn watermark_comment() -> String {
    format!(
        "Rewind v{} · 本文件经 Rewind 做旧处理(合成年代效果,非原始素材) · rewind.local",
        env!("CARGO_PKG_VERSION")
    )
}

/// 像素段:在指定画布上顺序执行像素算子(PixelPath)
#[derive(Debug, Clone)]
pub struct PixelSeg {
    pub ops: Vec<PixelOp>,
    pub w: u32,
    pub h: u32,
    /// 段开始时的生效帧率(段内 fps 必须在解码链兑现,否则像素预设的帧率参数是装饰)
    pub fps: Option<FpsState>,
    /// 段开始时的显示宽高比:rawvideo 不带 SAR 元数据,只能靠编码侧 `-aspect` 带过去
    pub aspect: Option<String>,
}

/// 链上当前生效的帧率状态(单一来源:像素段解码、隔行 comb、输出 -r 都从这里取)
#[derive(Debug, Clone)]
pub struct FpsState {
    pub fps: f64,
    pub round: String,
    pub shutter: f64,
}

#[derive(Debug, Clone)]
pub enum Step {
    Fast(Pass),
    Pixel(PixelSeg),
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// 预设自己的几何动作(缩放 / 裁切 / 补边 / 定像素比),按顺序。
    /// 做旧阶梯是"降下去再回到交付画幅",净几何不变,所以**不**进这里 ——
    /// 对比预览的"原帧"只套这条链,才不会顺带被阶梯糊一次。
    pub geom: Vec<String>,
    /// 交付画幅(存储尺寸,已取偶)
    pub out_w: u32,
    pub out_h: u32,
    /// 末趟带出去的显示比例(DAR,如 "4:3");None = 与存储尺寸一致
    pub dar: Option<String>,
}

impl Plan {
    /// 预览帧该按什么尺寸呈现:非方像素要按 DAR 折算成方像素,否则只认方像素的浏览器
    /// 看到的画幅,和播放器按 SAR 解出来的画幅不是同一个。
    pub fn display_dims(&self) -> (u32, u32) {
        let (w, h) = (self.out_w, self.out_h);
        if h == 0 || w == 0 {
            return (w, h);
        }
        let stored = w as f64 / h as f64;
        let want = match self.dar.as_deref().and_then(|s| ratio(s).ok()) {
            Some((n, d)) if d > 0.0 => n / d,
            _ => return (w, h),
        };
        if (stored - want).abs() / want < 0.01 {
            return (w, h);
        }
        let dw = even_dim((h as f64 * want).round() as u32);
        if dw < 16 {
            return (w, h);
        }
        (dw, h)
    }
}

impl Pass {
    /// 音频决策的可读形式,给 `plan` 导出用(结构闸据此看"这一手到底带不带音轨、底噪在不在")
    pub fn audio_name(&self) -> String {
        match &self.audio {
            AudioMode::None => "none".into(),
            AudioMode::Copy => "copy".into(),
            AudioMode::Encode { from_original, bitrate, hiss } => format!(
                "encode:{}{}{}",
                if *from_original { "original" } else { "prev" },
                bitrate.as_ref().map(|b| format!(":{b}")).unwrap_or_default(),
                // 底噪的具体参数必须出现在计划里:它走的是 extra input + filter_complex,
                // 不在 af 列表中,以前计划只剩一个 "+hiss" 标记 —— color/amplitude/mix_weight
                // 改了计划文本一字不变,"拧了没反应"根本查不出来。
                match hiss {
                    Some((src, w)) => format!("+hiss({src}:weight={})", num(*w)),
                    None => String::new(),
                }
            ),
        }
    }

    /// 生成 filter_complex(无音频处理时返回 None)
    pub fn filter_complex(&self, audio_present: bool) -> Option<String> {
        let mut parts: Vec<String> = vec![];
        if !self.fx_v.is_empty() {
            parts.push(format!("[0:v]{}[v]", self.fx_v.join(",")));
        }
        let audio_graph = match &self.audio {
            AudioMode::Encode { from_original, hiss, .. } if audio_present => {
                let base = if *from_original { "[1:a]" } else { "[0:a]" };
                let af = if self.fx_a.is_empty() { "anull".to_string() } else { self.fx_a.join(",") };
                match hiss {
                    Some((_, w)) => {
                        let hi = if *from_original { "[2:a]" } else { "[1:a]" };
                        format!("{base}{af}[a0];{hi}volume={}[ah];[a0][ah]amix=inputs=2:duration=first[a]", num(*w))
                    }
                    None => format!("{base}{af}[a]"),
                }
            }
            _ => String::new(),
        };
        if !audio_graph.is_empty() {
            parts.push(audio_graph);
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(";"))
        }
    }
}

/// PixelPath 中按序执行的像素算子
#[derive(Debug, Clone)]
pub enum PixelOp {
    Ntsc { seed: i32, knobs: NtscKnobs },
    Crt { scanline: f64, barrel: f64, aberration: f64, persistence: f64 },
    Film { seed: i32, scratches: f64, dust: f64, flicker: f64 },
}

pub struct AudioSpec {
    pub af: Vec<String>,
    pub bitrate: Option<String>,
    pub hiss: Option<(String, f64)>,
}

fn collect_audio(p: &Preset, media: &MediaInfo, preview: Option<f64>, extra_audio: &[AudioStage]) -> AudioSpec {
    let mut af = vec![];
    let mut bitrate = None;
    let mut hiss = None;
    for a in p.audio.iter().chain(extra_audio.iter()) {
        match a {
            AudioStage::Bandlimit { highpass, lowpass } => {
                af.push(format!("highpass=f={highpass}"));
                af.push(format!("lowpass=f={lowpass}"));
            }
            AudioStage::Gain { volume } => af.push(format!("volume={}", num(*volume))),
            AudioStage::BitrateRoundtrip { bitrate: b } => bitrate = Some(b.clone()),
            AudioStage::ResampleRoundtrip { rate } => {
                af.push(format!("aresample={rate}"));
                af.push("aresample=44100".into());
            }
            AudioStage::Bitcrush { bits, mode, aa } => af.push(format!(
                "acrusher=level_in=1:level_out=1:bits={bits}:mode={mode}:aa={}",
                num(*aa)
            )),
            AudioStage::TapeHiss { color, amplitude, mix_weight } => {
                let dur = preview.unwrap_or(media.duration) + 0.5;
                hiss = Some((
                    format!("anoisesrc=color={color}:amplitude={}:duration={}", num(*amplitude), num(dur)),
                    *mix_weight,
                ));
            }
            AudioStage::Mono {} => af.push("aformat=channel_layouts=mono".into()),
            AudioStage::WowFlutter { freq, depth } => {
                // ffmpeg 4.4 的 vibrato/tremolo 只认短选项 d(depth 长名不存在)
                af.push(format!("vibrato=f={}:d={}", num(*freq), num(*depth)));
                af.push(format!("tremolo=f={}:d={}", num(freq * 1.7), num(depth * 0.6)));
            }
        }
    }
    AudioSpec { af, bitrate, hiss }
}

fn color_chain(saturation: Option<f64>, contrast: Option<f64>, brightness: Option<f64>, gm: Option<f64>, bm: Option<f64>) -> Vec<String> {
    let mut eqp = vec![];
    if let Some(s) = saturation {
        eqp.push(format!("saturation={}", num(s)));
    }
    if let Some(c) = contrast {
        eqp.push(format!("contrast={}", num(c)));
    }
    if let Some(b) = brightness {
        eqp.push(format!("brightness={}", num(b)));
    }
    let mut out = vec![];
    if !eqp.is_empty() {
        out.push(format!("eq={}", eqp.join(":")));
    }
    if gm.is_some() || bm.is_some() {
        let mut cb = vec![];
        if let Some(g) = gm {
            cb.push(format!("gm={}", num(g)));
        }
        if let Some(b) = bm {
            cb.push(format!("bm={}", num(b)));
        }
        out.push(format!("colorbalance={}", cb.join(":")));
    }
    out
}

fn timestamp_chains(format: &str, rec_badge: bool, font: &str) -> Vec<String> {
    let fmt_esc = format.replace(':', "\\:");
    let mut out = vec![format!(
        "drawtext=fontfile={font}:text='%{{localtime\\:{fmt_esc}}}':x=16:y=14:fontsize=26:fontcolor=white:box=1:boxcolor=black@0.35"
    )];
    if rec_badge {
        out.push(format!(
            "drawtext=fontfile={font}:text='REC':x=w-100:y=14:fontsize=26:fontcolor=red"
        ));
        out.push("drawbox=x=w-132:y=18:w=16:h=16:color=red@0.9:t=fill".to_string());
    }
    out
}

/// "10:11" / "4/3" → (10.0, 11.0);格式不对就报错,不许静默忽略
pub fn ratio(s: &str) -> Result<(f64, f64), String> {
    let norm = s.replace('/', ":");
    let (a, b) = norm.split_once(':').ok_or_else(|| coded(PARAM_RANGE, format!("宽高比应为 a:b,收到 {s}")))?;
    let a: f64 = a.trim().parse().map_err(|_| coded(PARAM_RANGE, format!("宽高比 {s} 含非数字")))?;
    let b: f64 = b.trim().parse().map_err(|_| coded(PARAM_RANGE, format!("宽高比 {s} 含非数字")))?;
    if a <= 0.0 || b <= 0.0 || a > 100.0 || b > 100.0 {
        return Err(coded(PARAM_RANGE, format!("宽高比 {s} 超出合理范围")));
    }
    Ok((a, b))
}

/// 按比例裁切(两侧都取偶)。`dar` 分支与"没写 dar/fit 时的默认裁切"共用这一条表达式 ——
/// 同一个规则写两遍,其中一遍迟早和另一遍不一样(这个项目已经踩过三次)。
fn crop_to_ratio(v: f64) -> String {
    let v = num(v);
    format!(
        "crop=w='trunc(if(gt(iw/ih,{v}),ih*{v},iw)/2)*2':h='trunc(if(gt(iw/ih,{v}),ih,iw/{v})/2)*2'"
    )
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// par + 存储尺寸 → DAR(如 720x480 + 10:11 → "15:11");rawvideo 像素段没有元数据通道,只能靠 `-aspect` 带过去
pub fn dar_from_par(par: &str, w: u32, h: u32) -> Result<String, String> {
    let (pn, pd) = ratio(par)?;
    let n = (w as f64 * pn).round() as u64;
    let d = (h as f64 * pd).round() as u64;
    if n == 0 || d == 0 {
        return Err(coded(PARAM_RANGE, format!("DAR 计算越界: {par} @ {w}x{h}")));
    }
    let g = gcd(n, d).max(1);
    Ok(format!("{}:{}", n / g, d / g))
}

/// 非像素 stage -> vf 链(跟踪画布尺寸与生效帧率);像素 stage 返回 Some(op)
/// `geom` 只收**预设自己**的几何动作(见 `Plan::geom`);阶梯那几趟传一个丢弃用的 Vec。
fn push_video_stage(
    s: &VideoStage,
    vf: &mut Vec<String>,
    geom: &mut Vec<String>,
    cw: &mut u32,
    ch: &mut u32,
    media: &MediaInfo,
    font: Option<&str>,
    cur_fps: &mut Option<FpsState>,
    cur_par: &mut Option<(f64, f64)>,
    seek: f64,
    // 这个段是不是决定交付画幅的最后一段(只有它享受"没写 fit 就默认裁切")
    last_geometry: bool,
) -> Result<Option<PixelOp>, String> {
    // 静帧没有时间轴:fps/隔行织入/头尾雪花窗口都是时序效果,单帧输入下 tinterlace 会吞掉
    // 唯一那一帧(实测输出 0 帧)。图片做旧只保留空间性效果。
    if media.is_image()
        && matches!(
            s,
            VideoStage::Fps { .. } | VideoStage::InterlaceComb { .. } | VideoStage::TapeEnds { .. }
        )
    {
        return Ok(None);
    }
    let op = match s {
        VideoStage::Resize { w, h, mode, flags, dar, fit, par, overscan, range } => {
            let mark = vf.len();
            let want = match par.as_deref() {
                Some(p) => Some(ratio(p)?),
                None => None,
            };
            // 像素比要变了:先把非方像素去挤压成方形等效,否则内容会被拉变形(人脸变瘦)
            if let (Some((cn, cd)), Some((wn, wd))) = (*cur_par, want) {
                if (cn - wn).abs() + (cd - wd).abs() > 1e-9 {
                    vf.push(format!("scale='trunc(iw*{}/{}/2)*2':ih", num(cn), num(cd)));
                    vf.push("setsar=1".into());
                }
            }
            // 顺序很关键:过扫描裁切 → 画幅适配 → 缩放 → 加黑边 → 定像素比
            if let Some(o) = overscan.filter(|o| *o < 0.999) {
                if o < 0.6 {
                    return Err(coded(PARAM_RANGE, format!("overscan={o} 过于激进(0.85-1.0 才是电视切边的量级)")));
                }
                let o = num(o);
                vf.push(format!("crop=w='trunc(iw*{o}/2)*2':h='trunc(ih*{o}/2)*2'"));
            }
            let pad = fit.as_deref() == Some("pad");
            // "inside" = 只降不升的等平台行为(1080 上限那类),不补黑边
            let inside = fit.as_deref() == Some("inside");
            let stretch = fit.as_deref() == Some("stretch");
            let d = match dar.as_deref() {
                Some(s) => Some(ratio(s)?),
                None => None,
            };
            if let (Some((dn, dd)), false, true) = (d, pad, !stretch) {
                vf.push(crop_to_ratio(dn / dd));
            }
            if mode.as_deref() == Some("source_canvas") {
                *cw = even_dim(media.width);
                *ch = even_dim(media.height);
            } else {
                // 目标尺寸也取偶:用户填 1439 这种奇数不该让中间编码器炸掉
                *cw = even_dim(w.ok_or(coded(PRESET_LOAD, "resize 缺少 w"))?);
                *ch = even_dim(h.ok_or(coded(PRESET_LOAD, "resize 缺少 h"))?);
            }
            if inside {
                // "盒"语义下真实尺寸要按 AR 折算且只降不升。cw/ch 必须记**真实值**,
                // 否则做旧阶梯会拿盒当画布算尺寸,把小素材反而放大(实测踩过)。
                let s = (*cw as f64 / media.width.max(1) as f64)
                    .min(*ch as f64 / media.height.max(1) as f64)
                    .min(1.0);
                *cw = ((media.width as f64 * s) as u32 / 4 * 4).max(2);
                *ch = ((media.height as f64 * s) as u32 / 4 * 4).max(2);
            }
            // 清单给界面的默认是 crop,但引擎在 `fit` 与 `dar` 都没写时直接 scale 到目标画幅:
            // 实测 16:9 源进 cctv2000(640×480)是 x 缩 0.5 / y 缩 0.667 —— 人脸被横向压扁。
            // 只管交付那一段(中间段的 squash→restore 净几何恒等,是定标过的观感);
            // 要压扁请显式写 fit=stretch,那是清单里就有的选项。
            if last_geometry && d.is_none() && !pad && !inside && !stretch {
                let sa = media.width as f64 / media.height.max(1) as f64;
                let ta = *cw as f64 / (*ch).max(1) as f64;
                if (sa - ta).abs() > 0.01 {
                    vf.push(crop_to_ratio(ta));
                }
            }
            let mut sc = if inside {
                // 平台首刀只降不升:min(iw,盒) 保证小图不会被放大(实测 decrease 单独用会放大)
                format!(
                    "scale=w='min(iw,{cw})':h='min(ih,{ch})':flags={flags}:force_original_aspect_ratio=decrease:force_divisible_by=4"
                )
            } else if pad {
                format!(
                    "scale={cw}:{ch}:flags={flags}:force_original_aspect_ratio=decrease:force_divisible_by=4"
                )
            } else {
                format!("scale={cw}:{ch}:flags={flags}")
            };
            if let Some(r) = range.as_deref() {
                // 全范围(0-255)是监控/VCD rip 的招牌"黑场发灰",limited 是广播正统
                sc.push_str(match r {
                    "pc" => ":out_range=full",
                    "tv" => ":out_range=limited",
                    other => return Err(coded(PARAM_RANGE, format!("range 不支持 {other}(可用 tv/pc)"))),
                });
            }
            vf.push(sc);
            if pad {
                vf.push(format!("pad={cw}:{ch}:(ow-iw)/2:(oh-ih)/2:color=black"));
            }
            if let Some(p) = par.as_deref() {
                let (pn, pd) = ratio(p)?;
                vf.push(format!("setsar={pn}/{pd}"));
            } else if let Some((dn, dd)) = d {
                vf.push(format!("setdar={dn}/{dd}"));
            }
            *cur_par = want.or(*cur_par);
            geom.extend_from_slice(&vf[mark..]);
            None
        }
        VideoStage::Fps { fps, round, shutter, cadence } => {
            // 边界实测:fps=0 是 ffmpeg 解析错误;fps=1000 会"成功"跑完 —— 8 秒素材变 8000 帧,
            // 实测 84.08 s / 22.8 MB(正常 4.3 s / 7.2 MB)。一个手滑就贵 20 倍还没人拦,
            // 这不该留给用户去发现。
            if !(FPS_MIN..=FPS_MAX).contains(fps) {
                return Err(coded(PARAM_RANGE, format!(
                    "fps={fps} 超出可用范围({};老素材的档位是 {},再高只会变贵不会变像)",
                    range_text(FPS_MIN, FPS_MAX),
                    range_text(5.0, 60.0)
                )));
            }
            if let Some(c) = cadence.as_deref().filter(|c| *c != "none") {
                match c {
                    "telecine32" => vf.push("telecine=pattern=23".into()),
                    other => return Err(coded(PARAM_RANGE, format!("cadence 不支持 {other}(可用 none/telecine32)"))),
                }
            }
            vf.push(format!("fps={}:round={}", num(*fps), round));
            // 快门模糊:权重 (prev 1..cur),shutter=0 不混、=1 等权 180° 快门
            // 空格必须用单引号包:滤镜串是直接进 argv 的,双引号会被 tmix 当语法错误
            if *shutter > 0.0 {
                let s = shutter.min(1.0);
                vf.push(format!("tmix=frames=2:weights='{} 1'", num(s)));
            }
            *cur_fps = Some(FpsState { fps: *fps, round: round.clone(), shutter: *shutter });
            None
        }
        VideoStage::Noise { alls, allf } => {
            vf.push(format!("noise=alls={alls}:allf={allf}"));
            None
        }
        VideoStage::Color { saturation, contrast, brightness, green_mid, blue_mid } => {
            vf.extend(color_chain(*saturation, *contrast, *brightness, *green_mid, *blue_mid));
            None
        }
        VideoStage::AsymLowpass { luma_radius, chroma_radius } => {
            vf.push(format!(
                "boxblur=luma_radius={}:luma_power=1:chroma_radius={}:chroma_power=1",
                num(*luma_radius),
                num(*chroma_radius)
            ));
            None
        }
        VideoStage::InterlaceComb { mode, refps } => {
            // tinterlace=merge 会把帧高翻倍,必须紧跟 scale 缩回当前画布(§8.1-14)
            vf.push(format!("tinterlace=mode={mode}"));
            vf.push(format!("scale={cw}:{ch}:flags=bicubic"));
            // 场率跟随链上生效帧率;JSON 里的 refps 只是没有 fps stage 时的兜底(§13.2)
            let eff = cur_fps.as_ref().map(|f| f.fps).unwrap_or(*refps);
            vf.push(format!("fps={}", num(eff)));
            None
        }
        VideoStage::OverlayTimestamp { format, rec_badge } => {
            match font {
                Some(f) => vf.extend(timestamp_chains(format, *rec_badge, f)),
                None => eprintln!("warn: 未找到可用字体,跳过 overlay_timestamp"),
            }
            None
        }
        VideoStage::ColorFade { strength } => {
            let s = *strength;
            vf.push(format!("curves=all='0/{} 0.5/{} 1/{}'", num(0.10 * s), num(0.5 + 0.02 * s), num(1.0 - 0.06 * s)));
            vf.push(format!("eq=saturation={}", num(1.0 - 0.35 * s)));
            vf.push(format!("colorbalance=rm={}:bm={}", num(0.08 * s), num(-0.06 * s)));
            None
        }
        VideoStage::BandQuantize { level, chroma } => {
            // ffmpeg 4.4.2 实测没有 posterize/banddither,色阶断层只能 lutyuv 手搓。
            // 默认只砍 y:砍到 u/v 上会把色度压成约 6 级并整体偏绿(用户截图报回的真实缺陷)。
            // 边界实测:level=0 让 lutyuv 表达式除零,报的是半句 ffmpeg 原始错误;
            // level=1 起才可用,>48 等于不砍。宁可在这里说人话,也别把 ffmpeg 的吐槽丢给用户。
            if *level < LEVEL_MIN || *level > LEVEL_MAX {
                return Err(coded(PARAM_RANGE, format!(
                    "band_quantize.level={level} 超出可用范围({};界面档位 {},越小色带越粗)",
                    range_text(LEVEL_MIN as f64, LEVEL_MAX as f64),
                    range_text(8.0, 48.0)
                )));
            }
            let l = *level;
            let e = format!("trunc(val/{l})*{l}+{l}/3");
            vf.push(if *chroma {
                format!("lutyuv=y='{e}':u='{e}':v='{e}'")
            } else {
                format!("lutyuv=y='{e}'")
            });
            None
        }
        VideoStage::Unsharp { size, amount, chroma_amount } => {
            // 默认关:不插滤镜(否则等于白跑一趟)。只有"爆炸档"才该有振铃。
            if *amount > 0.0 {
                // ffmpeg 只吃 3–13 的**奇数**矩阵:6 报 "Invalid even size",14 报 "matrix size too big"。
                // 手写预设里一个越界值不该让整趟渲染死掉 —— 贴到最近的合法值(清单那边同时把步长给成 2)。
                let s = (*size | 1).clamp(3, 13);
                vf.push(format!(
                    "unsharp={s}:{s}:{}:{s}:{s}:{}",
                    num(*amount),
                    num(*chroma_amount)
                ));
            }
            None
        }
        VideoStage::ChromaDecimate { to } => {
            let mid = match to.as_str() {
                "411" => "yuv411p",
                "420" => "yuv420p",
                "422" => "yuv422p",
                other => return Err(coded(PARAM_RANGE, format!("chroma_decimate 不支持 {other}(可用 411/420/422)"))),
            };
            vf.push(format!("format={mid}"));
            vf.push("format=yuv420p".into());
            None
        }
        VideoStage::MatrixRoundtrip {} => {
            vf.push("colormatrix=bt709:bt601".into());
            vf.push("colormatrix=bt601:bt709".into());
            None
        }
        VideoStage::TapeEnds { head, tail, intensity } => {
            // noise 支持 timeline:仅片头/片尾窗口生效(穿带雪花)。
            // 窗口要换算到**本次渲染的局部时间**:预览用输入端 -ss,渲染出来的 t 从 0 重新数,
            // 直接写全片绝对时刻等于让每一帧预览都落在"片头雪花"里(实测预览帧离原片 9.9,
            // 成品同一点 4.5,而真片头是 9.8 —— 预览与成品对不上)
            let head_t = *head - seek;
            let end_t = (media.duration - tail).max(*head) - seek;
            vf.push(format!(
                "noise=alls={intensity}:allf=t+u:enable='lt(t,{})+gt(t,{})'",
                num(head_t),
                num(end_t)
            ));
            None
        }
        VideoStage::NtscVhs { seed, knobs } => Some(PixelOp::Ntsc { seed: *seed, knobs: *knobs }),
            VideoStage::Crt { scanline, barrel, aberration, persistence } => {
                Some(PixelOp::Crt { scanline: *scanline, barrel: *barrel, aberration: *aberration, persistence: *persistence })
            }
        VideoStage::FilmDamage { seed, scratches, dust, flicker } => {
            Some(PixelOp::Film { seed: *seed, scratches: *scratches, dust: *dust, flicker: *flicker })
        }
        VideoStage::CodecRoundtrip { .. } => None,
    };
    Ok(op)
}

/// 静帧输出的编码器选择:必须按**目标扩展名**定,否则最终趟的 libx264 会把 H.264 流写进
/// `.png` 文件名里(实测画廊烘培时暴露:用户在文件管理器里打不开成品)。
pub fn still_codec(ext: &str) -> Vec<String> {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "jpe" => vec!["-c:v".into(), "mjpeg".into(), "-q:v".into(), "2".into()],
        "webp" => vec!["-c:v".into(), "libwebp".into(), "-quality".into(), "92".into()],
        "bmp" => vec!["-c:v".into(), "bmp".into(), "-pix_fmt".into(), "bgr24".into()],
        "gif" => vec!["-c:v".into(), "gif".into()],
        "tif" | "tiff" => vec!["-c:v".into(), "tiff".into(), "-pix_fmt".into(), "rgb24".into()],
        _ => vec!["-c:v".into(), "png".into(), "-pix_fmt".into(), "rgb24".into()],
    }
}

fn fast_pass(vf: &mut Vec<String>, audio: AudioMode) -> Step {
    Step::Fast(Pass {
        fx_v: std::mem::take(vf),
        fx_a: vec![],
        audio,
        extra_inputs: vec![],
        out_extra: vec![],
        vcodec: vec!["-c:v".into(), "libx264".into(), "-preset".into(), "ultrafast".into(), "-crf".into(), "14".into(), "-pix_fmt".into(), "yuv420p".into()],
        out_ext: "mp4".into(),
    })
}

fn flush_pixel(
    steps: &mut Vec<Step>,
    ops: &mut Vec<PixelOp>,
    cw: u32,
    ch: u32,
    fps: &Option<FpsState>,
    aspect: &Option<String>,
) {
    if !ops.is_empty() {
        steps.push(Step::Pixel(PixelSeg {
            ops: std::mem::take(ops),
            w: cw,
            h: ch,
            fps: fps.clone(),
            aspect: aspect.clone(),
        }));
    }
}

/// yuv420 类编码器(mpeg4 / H.263 / libx264)吃不下奇数边长,而 1919×1079 这类源真实存在。
/// 一律**向下**取偶(绝不放大画面),最短 2 px。
fn even_dim(x: u32) -> u32 {
    if x % 2 == 1 {
        x.saturating_sub(1).max(2)
    } else {
        x.max(2)
    }
}

/// 链路头的一次取偶:谁先落盘谁负责(中间趟 prepend;像素段自带 scale,不必重复)
fn apply_head(head: &mut Option<String>, vf: &mut Vec<String>) {
    if let Some(s) = head.take() {
        vf.insert(0, s);
    }
}

fn flush_fast(steps: &mut Vec<Step>, vf: &mut Vec<String>, media: &MediaInfo, head: &mut Option<String>) {
    if steps.is_empty() {
        apply_head(head, vf);
    }
    if !vf.is_empty() {
        let audio = if media.is_image() || !media.has_audio { AudioMode::None } else { AudioMode::Copy };
        steps.push(fast_pass(vf, audio));
    }
}

/// 做旧系数上限。实测**同尺寸同 q 反复往返会收敛**(8 代后字节数≈单代),
/// 所以每一手必须同时改尺寸才有效果 —— 阶梯不是装饰,是这个参数成立的前提。
pub const MAX_AGING: u32 = 8;

/// 计划的可读导出(`rewind-core plan`):调试与结构闸都靠它看清"某一手到底跑了哪些滤镜"
pub fn plan_json(plan: &Plan, media: &MediaInfo, gens: u32) -> serde_json::Value {
    let (dw, dh) = plan.display_dims();
    serde_json::json!({
        "source": { "width": media.width, "height": media.height, "fps": media.fps, "is_image": media.is_image() },
        // 交付画幅:存储尺寸 + 折算后的呈现尺寸(界面与闸都据此判断,不许各自猜)
        "out": { "w": plan.out_w, "h": plan.out_h, "dar": plan.dar, "display_w": dw, "display_h": dh },
        "geom": plan.geom,
        "aging": gens,
        "steps": plan.steps.iter().map(|s| match s {
            Step::Fast(p) => serde_json::json!({
                "kind": "fast", "vf": p.fx_v, "vcodec": p.vcodec, "out_ext": p.out_ext,
                // 音频与附加输入也要导出:结构闸此前只看视频滤镜链,于是"无音轨素材被多塞一条
                // 完整视频轨"、"底噪 lavfi 输入丢了"这类问题从 plan 里根本看不出来
                "audio": p.audio_name(), "af": p.fx_a, "extra_inputs": p.extra_inputs.len(),
                "out_extra": p.out_extra
            }),
            Step::Pixel(seg) => serde_json::json!({
                "kind": "pixel", "w": seg.w, "h": seg.h,
                "ops": seg.ops.iter().map(|o| format!("{o:?}")).collect::<Vec<_>>(),
                "fps": seg.fps.as_ref().map(|f| f.fps),
            }),
        }).collect::<Vec<_>>(),
    })
}

/// 做旧系数的一"手":降采样一档 + 一次压缩往返 + 色度/断层/灰雾推进。
/// `canvas` 是**预设走完后真实的交付画幅**(由 build_plan 传入,所以 inside 这类"盒"语义已经折算好)。
/// 规则:几何与时间做一次(预设内),损耗做 N 次(这里);6 手起才进"爆炸档"(锐化+饱和拉爆)。
fn aging_hand(g: u32, canvas: (u32, u32), q0: u32, is_image: bool, cast: f64) -> Vec<VideoStage> {
    use VideoStage as V;
    let hands = g + 1;
    // 每手长边 ×0.72:实测 1/2 轻、1/3 明显、1/4 文字成糊、1/5 不可读
    let f = 0.72f64.powi(g as i32);
    let mut out = vec![V::Resize {
        w: Some(((canvas.0 as f64 * f).round() as u32).max(96)),
        h: Some(((canvas.1 as f64 * f).round() as u32).max(64)),
        mode: None,
        flags: "area".into(),
        dar: None,
        fit: None,
        par: None,
        overscan: None,
        range: None,
    }];
    // q 高端会饱和(实测 q20→31 体积只从 79KB→66KB),所以推进力主要给尺寸与色度
    out.push(V::CodecRoundtrip {
        codec: "mjpeg".into(),
        q: (q0 as f64 + 1.5 * g as f64).round().clamp(2.0, 31.0) as u32,
        bitrate: None,
        container: Some("mp4".into()),
        audio_bitrate: None,
        video_only: false,
    });
    if hands >= 2 {
        out.push(V::ChromaDecimate { to: if hands >= 4 { "411".into() } else { "420".into() } });
    }
    out.push(V::BandQuantize { level: (24 - 2 * g).clamp(12, 24) as u32, chroma: false });
    // 灰雾必须显式累积:实测纯黑过 4 代 mjpeg 后 YAVG 仍是 0,压缩不会自己抬黑
    let blow = hands >= 6;
    // 偏色是**可选味道**(用户明确要求:"包浆不等于变绿"):cast=0 时阶梯一路保持中性。
    // 满档给到 gm=0.5 —— 整帧均值只能反映约 0.4 的位移(平涂背景会稀释肤色偏移),
    // 所以这个量级靠对照图 samples/sheet_cast.png 用眼睛验收,不靠均值断言。
    let tint = cast * (0.11 * g as f64).min(0.5);
    out.push(V::Color {
        saturation: Some(if blow { 1.05 + 0.06 * (hands as f64 - 6.0) } else { 1.0 }),
        contrast: Some((1.0 - 0.01 * g as f64).max(0.7)),
        brightness: Some((0.012 * g as f64).min(0.12)),
        green_mid: if cast > 0.0 { Some(tint) } else { None },
        blue_mid: if cast > 0.0 { Some(-tint * 0.8) } else { None },
    });
    if blow {
        out.push(V::Unsharp { size: 5, amount: (1.2 + 0.26 * (hands as f64 - 6.0)).min(2.5), chroma_amount: 0.0 });
    }
    out.push(V::Noise { alls: (2 * hands).min(12) as u32, allf: if is_image { "u".into() } else { "t+u".into() } });
    out
}

/// 阶梯走完后的拉回:绝对尺寸 = 预设原本的交付画幅,所以调系数不改变导出分辨率
fn aging_restore(canvas: (u32, u32)) -> VideoStage {
    VideoStage::Resize {
        w: Some(canvas.0),
        h: Some(canvas.1),
        mode: None,
        flags: "bicubic".into(),
        dar: None,
        fit: None,
        par: None,
        overscan: None,
        range: None,
    }
}

/// 音频三手就满(底噪与带限累积收敛得快),4 手起再砍一次采样率
fn aging_audio(gens: u32) -> Vec<crate::preset::AudioStage> {
    use crate::preset::AudioStage as A;
    let mut out = vec![];
    for k in 0..gens.min(3).saturating_sub(1) {
        out.push(A::BitrateRoundtrip { bitrate: if k == 0 { "48k".into() } else { "24k".into() } });
    }
    if gens >= 4 {
        out.push(A::ResampleRoundtrip { rate: 8000 });
    }
    out
}

pub fn build_plan(p: &Preset, media: &MediaInfo, preview: Option<f64>, font: Option<&str>) -> Result<Plan, String> {
    build_plan_windowed(p, media, preview, font, 0.0)
}

/// `seek` 是这一趟**从第几秒开始渲**(预览只渲 t 附近的窗口)。时间轴滤镜按渲染后的
/// 局部时间生效,所以窗口必须平移 —— 否则任何预览都会撞上"片头 0.6s"的穿带雪花。
pub fn build_plan_windowed(
    p: &Preset,
    media: &MediaInfo,
    preview: Option<f64>,
    font: Option<&str>,
    seek: f64,
) -> Result<Plan, String> {
    // 做旧系数 = 被"下载→重采样→再压缩→上传"往返几手;预设自带的那趟算第 1 手。
    // 上限 8 的依据:实测同尺寸同 q 反复往返会收敛(8 代后字节数≈单代)。
    let gens = p.aging.unwrap_or(1).clamp(1, MAX_AGING);
    let extra_audio = if gens > 1 { aging_audio(gens) } else { vec![] };
    let cast = p.cast.unwrap_or(0.0).clamp(0.0, 1.0);
    let q0 = p
        .video
        .iter()
        .find_map(|s| match s {
            VideoStage::CodecRoundtrip { q, .. } => Some(*q),
            _ => None,
        })
        .unwrap_or(10);
    let aud = collect_audio(p, media, preview, &extra_audio);
    let mut steps: Vec<Step> = vec![];
    let mut vf: Vec<String> = vec![];
    let mut geom: Vec<String> = vec![];
    let mut pending_ops: Vec<PixelOp> = vec![];
    // 奇数边长的源先取偶,否则第一趟中间编码器就炸(见 even_dim 注释)
    let (w0, h0) = (even_dim(media.width), even_dim(media.height));
    let mut head = if (w0, h0) == (media.width, media.height) {
        None
    } else {
        Some(format!("scale={w0}:{h0}:flags=area"))
    };
    let (mut cw, mut ch) = (w0, h0);
    let mut seg_wh = (w0, h0);
    let mut cur_fps: Option<FpsState> = None;
    let mut seg_fps: Option<FpsState> = None;
    let mut cur_aspect: Option<String> = None;
    let mut seg_aspect: Option<String> = None;
    let mut cur_par: Option<(f64, f64)> = None;
    let mut cur_range: Option<&'static str> = None;
    let mut saw_codec = false;
    let mut saw_video_only_codec = false;
    // 交付画幅由**最后一个 resize 段**定。只有它需要"不许压扁"的默认裁切:中间段的
    // squash→restore(crt1995 先进 960×720 再回源画幅)净几何本来就是恒等,给它加裁切
    // 反而把画面推进 33%(实测:同一帧左右各少一块)。
    let last_resize = p.video.iter().rev().find(|s| matches!(**s, VideoStage::Resize { .. }));

    for s in &p.video {
        match s {
            VideoStage::CodecRoundtrip { codec, q, bitrate, container, audio_bitrate, video_only } => {
                saw_codec = true;
                if *video_only {
                    saw_video_only_codec = true;
                }
                flush_pixel(&mut steps, &mut pending_ops, seg_wh.0, seg_wh.1, &seg_fps, &seg_aspect);
                // 这一步若就是计划的第 0 步,取偶必须由它来做(第 0 步是像素段时自带 scale)
                if steps.is_empty() {
                    apply_head(&mut head, &mut vf);
                }
                let vcodec = match bitrate {
                    Some(b) => vec!["-c:v".into(), codec.clone(), "-b:v".into(), b.clone()],
                    None => vec!["-c:v".into(), codec.clone(), "-q:v".into(), q.to_string()],
                };
                steps.push(Step::Fast(Pass {
                    fx_v: std::mem::take(&mut vf),
                    fx_a: vec![],
                    audio: if media.is_image() || *video_only || !media.has_audio {
                        // 没音轨也走 Encode 的话,run_pass 会拼出 `-map ""`,而 ffmpeg 把空 map
                        // 当成"自动选流":实测中间趟因此多塞一条**完整原片视频轨**(4.4× 体积)
                        AudioMode::None
                    } else {
                        AudioMode::Encode { from_original: false, bitrate: audio_bitrate.clone(), hiss: None }
                    },
                    extra_inputs: vec![],
                    out_extra: vec![],
                    vcodec,
                    out_ext: container.clone().unwrap_or_else(|| "mp4".into()),
                }));
            }
            other => {
                // 交付画幅 = 最后一个 resize 段定的画幅,只有它需要"不许压扁"的默认裁切(见 push_video_stage)
                let last_geom = last_resize.map_or(false, |r| std::ptr::eq(r, other));
                let op = push_video_stage(
                    other, &mut vf, &mut geom, &mut cw, &mut ch, media, font, &mut cur_fps, &mut cur_par, seek,
                    last_geom,
                )?;
                if let VideoStage::Resize { par, dar, .. } = other {
                    cur_aspect = match (par.as_deref(), dar.as_deref()) {
                        (Some(p), _) => dar_from_par(p, cw, ch).ok(),
                        (None, Some(d)) => Some(d.to_string()),
                        (None, None) => None,
                    };
                }
                if let VideoStage::Resize { range, .. } = other {
                    // 量化范围要在交付时显式打标签,否则像素是全范围而容器说 unknown
                    cur_range = match range.as_deref() {
                        Some("pc") => Some("pc"),
                        Some("tv") => Some("tv"),
                        _ => None,
                    };
                }
                if let Some(op) = op {
                    // 像素段前的 ffmpeg 链先作为中间趟落盘(保持 stage 顺序语义);
                    // 像素段画布/帧率/画幅 = 该段开始时的状态
                    if !pending_ops.is_empty() && !vf.is_empty() {
                        // 两个像素段之间夹了 ffmpeg stage:先关段再插中间趟
                        flush_pixel(&mut steps, &mut pending_ops, seg_wh.0, seg_wh.1, &seg_fps, &seg_aspect);
                        flush_fast(&mut steps, &mut vf, media, &mut head);
                        seg_wh = (cw, ch);
                        seg_fps = cur_fps.clone();
                        seg_aspect = cur_aspect.clone();
                    } else if pending_ops.is_empty() {
                        flush_fast(&mut steps, &mut vf, media, &mut head);
                        seg_wh = (cw, ch);
                        seg_fps = cur_fps.clone();
                        seg_aspect = cur_aspect.clone();
                    }
                    pending_ops.push(op);
                }
            }
        }
    }
    // 做旧系数:预设走完后,在**真实交付画幅**上追加 N−1 代传播阶梯
    if gens > 1 {
        flush_fast(&mut steps, &mut vf, media, &mut head);
        let canvas = (cw, ch);
        // 阶梯的 resize 是"降到 canvas×0.72^n 再回到 canvas",净几何为恒等;
        // 所以它的滤镜不收进 Plan::geom —— 否则对比预览的"原帧"会白挨一次缩放。
        let mut ladder_geom: Vec<String> = vec![];
        for g in 1..gens {
            for st in aging_hand(g, canvas, q0, media.is_image(), cast) {
                match &st {
                    VideoStage::CodecRoundtrip { q, container, .. } => {
                        steps.push(Step::Fast(Pass {
                            fx_v: std::mem::take(&mut vf),
                            fx_a: vec![],
                            audio: if media.is_image() { AudioMode::None } else { AudioMode::Copy },
                            extra_inputs: vec![],
                            out_extra: vec![],
                            vcodec: vec!["-c:v".into(), "mjpeg".into(), "-q:v".into(), q.to_string()],
                            out_ext: container.clone().unwrap_or_else(|| "mp4".into()),
                        }));
                    }
                    other => {
                        push_video_stage(other, &mut vf, &mut ladder_geom, &mut cw, &mut ch, media, font, &mut cur_fps, &mut cur_par, seek, false)?;
                    }
                }
            }
        }
        push_video_stage(
            &aging_restore(canvas),
            &mut vf,
            &mut ladder_geom,
            &mut cw,
            &mut ch,
            media,
            font,
            &mut cur_fps,
            &mut cur_par,
            seek,
            // 阶梯的"拉回画幅"是恒等还原,不是交付裁切的那一段
            false,
        )?;
    }
    flush_pixel(&mut steps, &mut pending_ops, seg_wh.0, seg_wh.1, &seg_fps, &seg_aspect);

    let mut final_pass = Pass {
        fx_v: std::mem::take(&mut vf),
        fx_a: vec![],
        audio: AudioMode::Copy,
        extra_inputs: vec![],
        out_extra: vec!["-metadata".into(), format!("comment={}", watermark_comment())],
        vcodec: vec!["-c:v".into(), "libx264".into(), "-crf".into(), "20".into(), "-pix_fmt".into(), "yuv420p".into()],
        out_ext: "mp4".into(),
    };
    // 音频规则(与 M0 定标脚本一致):
    // - 有 codec 趟已带音频编码且 af 为空:最终 copy
    // - 视频专用趟(video_only)丢过音频:最终从原始输入取音轨并编码 af
    if !media.is_image() && media.has_audio {
        let needs_encode = !saw_codec || saw_video_only_codec || !aud.af.is_empty() || aud.hiss.is_some();
        if needs_encode {
            final_pass.audio = AudioMode::Encode {
                from_original: saw_video_only_codec,
                bitrate: if saw_codec && !saw_video_only_codec { None } else { aud.bitrate.clone() },
                hiss: aud.hiss.clone(),
            };
            final_pass.fx_a = aud.af.clone();
            if let Some((src, _)) = &aud.hiss {
                final_pass.extra_inputs.push(vec!["-f".into(), "lavfi".into(), "-i".into(), src.clone()]);
            }
        }
    } else {
        final_pass.audio = AudioMode::None;
    }
    if let Some(r) = cur_range {
        final_pass.vcodec.extend(vec!["-color_range".into(), r.into()]);
    }
    if steps.is_empty() {
        apply_head(&mut head, &mut final_pass.fx_v);
    }
    steps.push(Step::Fast(final_pass));
    let mut plan = Plan { steps, geom, out_w: cw, out_h: ch, dar: cur_aspect };
    // 静帧没有 SAR 这条通道:PNG 里的 16:21 像素比只有 ffmpeg 认,浏览器一律按方像素画。
    // 所以图片输出把 DAR 烘进像素,交付的就是"看到的那个画幅"。
    if media.is_image() {
        let (dw, dh) = plan.display_dims();
        if (dw, dh) != (plan.out_w, plan.out_h) {
            if let Some(Step::Fast(last)) = plan.steps.last_mut() {
                last.fx_v.push(format!("scale={dw}:{dh}:flags=bicubic"));
                last.fx_v.push("setsar=1".into());
            }
            // 几何链同步收尾:预览的"原帧"就是照这条链折算的,少一步就对不齐
            plan.geom.push(format!("scale={dw}:{dh}:flags=bicubic"));
            plan.out_w = dw;
            plan.out_h = dh;
            plan.dar = None;
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::Preset;
    use std::path::Path;

    fn load(id: &str) -> Preset {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets");
        Preset::load(&dir.join(format!("{id}.json"))).unwrap()
    }

    fn hd() -> MediaInfo {
        MediaInfo { width: 1920, height: 1080, fps: 30.0, duration: 10.0, has_audio: true,
            container: "mov,mp4,m4a,3gp,3g2,mj2".into() }
    }

    fn fasts(plan: &Plan) -> Vec<&Pass> {
        plan.steps.iter().filter_map(|s| match s { Step::Fast(p) => Some(p), _ => None }).collect()
    }

    /// 不变量:可达域里没有任何交付画幅小到"折不动 DAR"。
    /// `display_dims` 在折算宽度 <16 时只能放弃(放弃 = 图片按方像素画错画幅)。与其为这条
    /// 走不到的分支写死代码,不如把它钉成断言 —— 以后谁加了个带 DAR 的极小预设,这里先红。
    #[test]
    fn no_reachable_canvas_is_too_small_to_fold_its_dar() {
        let img = MediaInfo { width: 1920, height: 1080, fps: 0.0, duration: 0.0, has_audio: false,
            container: "png_pipe".into() };
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets");
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort();
        let mut worst = (u32::MAX, String::from("-"));
        let mut checked = 0usize;
        let mut check = |id: String, p: &Preset| {
            let plan = build_plan(p, &img, None, None).unwrap();
            checked += 1;
            if let Some(d) = plan.dar.clone() {
                let (dw, _) = plan.display_dims();
                assert!(dw >= 16, "{id} 带 DAR {d},折算宽度只有 {dw} → 会被静默放弃");
                if dw < worst.0 {
                    worst = (dw, id);
                }
            }
        };
        for f in &files {
            let p = Preset::load(f).unwrap();
            check(p.id.clone(), &p);
        }
        for y in (1965..=2026).step_by(3) {
            let p = crate::era::preset_for_year(y);
            check(format!("era_{y}"), &p);
        }
        assert!(checked >= 30, "覆盖面太小,只查了 {checked} 个计划");
        assert_ne!(worst.1, "-", "没有任何一个交付画幅带 DAR,这条断言在空转");
        println!("已查 {checked} 个交付画幅,最窄的 DAR 折算宽度 = {} ({})", worst.0, worst.1);
    }

    /// 静帧 = "探不到时长" **且** "容器是静态图片 muxer"。
    /// 只看时长会把裸 H.264 流(`.264`,实测 ffprobe 报 duration=0)当静帧:时序 stage 被剥、
    /// 只出一帧、成品还是塞进 `.264` 文件名的 PNG。
    #[test]
    fn unknown_duration_is_not_a_still_unless_the_muxer_is_an_image() {
        let m = |dur: f64, c: &str| MediaInfo {
            width: 320,
            height: 240,
            fps: 25.0,
            duration: dur,
            has_audio: false,
            container: c.into(),
        };
        assert!(m(0.0, "png_pipe").is_image());
        assert!(m(0.0, "jpeg_pipe").is_image());
        assert!(!m(0.0, "h264").is_image(), "裸流视频被当成静帧");
        assert!(!m(0.0, "mpegts").is_image());
        assert!(!m(6.0, "mov,mp4,m4a,3gp,3g2,mj2").is_image());

        // 判据不是纸面概念:它决定时序 stage 留不留
        let p = load("vhs1990_static");
        let has_fps = |media: &MediaInfo| {
            build_plan(&p, media, None, None)
                .unwrap()
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Fast(pass) => Some(pass.fx_v.iter().any(|f| f.starts_with("fps="))),
                    _ => None,
                })
                .any(|b| b)
        };
        assert!(has_fps(&m(0.0, "h264")), "裸流被剥掉了时序 stage(当成静帧了)");
        assert!(!has_fps(&m(0.0, "png_pipe")), "静帧本该剥掉时序 stage");
    }

    #[test]
    fn dvd_two_codec_passes() {
        let plan = build_plan(&load("dvd2005"), &hd(), None, None).unwrap();
        let f = fasts(&plan);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].fx_v[0], "scale=640:360:flags=bicubic");
        assert!(f[0].vcodec.iter().any(|x| x == "mpeg4") && f[0].vcodec.iter().any(|x| x == "14"));
        assert_eq!(f[0].audio, AudioMode::Encode { from_original: false, bitrate: Some("64k".into()), hiss: None });
        assert!(f[1].fx_v.iter().any(|x| x == "scale=1920:1080:flags=bicubic"));
        assert!(f[1].fx_v.iter().any(|x| x == "noise=alls=6:allf=t+u"));
        assert_eq!(f[1].audio, AudioMode::Copy);
    }

    #[test]
    fn cctv_single_pass_with_timestamp() {
        let plan = build_plan(&load("cctv2000"), &hd(), None, Some("/f.ttf")).unwrap();
        let f = fasts(&plan);
        assert_eq!(f.len(), 1);
        let p = &f[0];
        assert!(p.fx_v.iter().any(|x| x == "fps=15:round=down"), "{:?}", p.fx_v);
        let dt = p.fx_v.iter().find(|x| x.starts_with("drawtext")).unwrap();
        assert!(dt.contains("text='%{localtime\\:%Y-%m-%d %T}'"), "时间戳应为转义后的 %T 形式: {dt}");
        assert!(matches!(p.audio, AudioMode::Encode { from_original: false, .. }));
        let fc = p.filter_complex(true).unwrap();
        assert!(fc.contains("[0:a]highpass=f=400,lowpass=f=4000,volume=1.2[a]"), "{fc}");
    }

    /// 走 JSON 反序列化,和用户体验同一条路(新字段默认值也被一起验掉)
    fn preset_from(video: serde_json::Value) -> Preset {
        serde_json::from_value(serde_json::json!({
            "id": "t", "name": "t", "era": 1990, "schema": 1, "video": video, "audio": []
        }))
        .unwrap()
    }

    /// 预览只渲 t 附近的窗口,而输入端 `-ss` 会把时间戳归零:头尾雪花的窗口必须跟着平移,
    /// 否则任何时刻的预览都自带"片头雪花"(实测预览帧离原片 9.9,成品同一点 4.5,真片头 9.8)
    #[test]
    fn tape_end_windows_shift_with_the_rendered_window() {
        let p = load("vhs1990_static");
        let win = |seek: f64| {
            fasts(&build_plan_windowed(&p, &hd(), Some(1.5), None, seek).unwrap())
                .iter()
                .flat_map(|f| f.fx_v.clone())
                .find(|s| s.contains("enable="))
                .unwrap()
        };
        assert!(win(0.0).contains("lt(t,0.6)+gt(t,9.4)"), "整片跑: {}", win(0.0));
        assert!(win(4.5).contains("lt(t,-3.9)+gt(t,4.9)"), "窗口没平移: {}", win(4.5));
        // 平移后头窗为负 = 这一窗里根本没有片头,ffmpeg 的 lt(t,负数) 恒假
    }

    /// 交付画幅 ≠ 呈现画幅的那一层折算,是预览与成品对齐的前提,所以要在计划里报出来
    #[test]
    fn plan_reports_delivery_and_display_geometry() {
        let p = preset_from(serde_json::json!([
            {"stage":"resize","params":{"w":1280,"h":720,"dar":"4:3"}}
        ]));
        let v = build_plan(&p, &hd(), None, None).unwrap();
        assert_eq!((v.out_w, v.out_h), (1280, 720));
        assert_eq!(v.dar.as_deref(), Some("4:3"));
        assert_eq!(v.display_dims(), (960, 720));
        // 视频交付只带 DAR 元数据,不许顺手改像素
        assert!(!v.geom.iter().any(|f| f.starts_with("scale=960:720")), "{:?}", v.geom);

        // 图片没有 SAR 这条通道(浏览器一律按方像素画):把 DAR 烘进像素
        let img = MediaInfo { width: 1920, height: 1080, fps: 0.0, duration: 0.0, has_audio: false,
            container: "png_pipe".into() };
        let s = build_plan(&p, &img, None, None).unwrap();
        assert_eq!((s.out_w, s.out_h), (960, 720));
        assert_eq!(s.dar, None);
        assert!(s.geom.last().map(|f| f.starts_with("scale=960:720")).unwrap_or(false), "{:?}", s.geom);
        assert_eq!(s.display_dims(), (960, 720));
    }

    /// Plan::geom 只收预设自己的几何。做旧阶梯是"降到画幅的 0.72^n 再回到画幅",
    /// 净几何恒等;混进来的话,对比预览的"原片"那一层会白挨 N 次重采样,差异被量小。
    #[test]
    fn geom_excludes_the_aging_ladder() {
        let mut p = load("cctv2000");
        let one = build_plan(&p, &hd(), None, None).unwrap().geom;
        // 16:9 的 HD 源进 4:3 画布:默认必须是"先裁到画幅再缩放"。
        // 从前这里只有一条 scale —— 等于把画面横向压扁 25%(人脸变瘦)。
        assert_eq!(one.len(), 2, "应当是裁切 + 缩放两条: {one:?}");
        assert!(
            one[0].starts_with("crop=") && one[0].contains("1.3333333333333333"),
            "没裁到 4:3: {:?}",
            one[0]
        );
        assert_eq!(one[1], "scale=640:480:flags=bicubic");
        p.aging = Some(5);
        let many = build_plan(&p, &hd(), None, None).unwrap().geom;
        assert_eq!(one, many, "阶梯不该往 geom 里加东西");
    }

    #[test]
    fn fps_round_shutter_cadence_reach_graph() {
        let p = preset_from(serde_json::json!([
            {"stage":"fps","params":{"fps":12.5,"round":"down","shutter":0.6,"cadence":"telecine32"}}
        ]));
        let chain = fasts(&build_plan(&p, &hd(), None, None).unwrap())[0].fx_v.join(",");
        assert!(chain.contains("telecine=pattern=23"), "{chain}");
        assert!(chain.contains("fps=12.5:round=down"), "{chain}");
        assert!(chain.contains("tmix=frames=2:weights='0.6 1'"), "{chain}");
        // 非法 cadence 必须报错,不许静默忽略
        let bad = preset_from(serde_json::json!([{"stage":"fps","params":{"fps":25,"cadence":"ntsc_film"}}]));
        assert!(build_plan(&bad, &hd(), None, None).is_err());
    }

    #[test]
    fn fps_stage_defaults_stay_compatible_with_old_presets() {
        let p = preset_from(serde_json::json!([{"stage":"fps","params":{"fps":15}}]));
        let chain = fasts(&build_plan(&p, &hd(), None, None).unwrap())[0].fx_v.join(",");
        assert!(chain.contains("fps=15:round=down"), "{chain}");
        assert!(!chain.contains("tmix"), "{chain}");
    }

    #[test]
    fn resize_geometry_chain_is_ordered_and_typed() {
        // 顺序即语义:过扫描裁 → 画幅裁 → 缩放(带量化范围) → 定像素比
        let p = preset_from(serde_json::json!([{"stage":"resize","params":{
            "w":720,"h":480,"dar":"15:11","fit":"crop","par":"10:11","overscan":0.94,"range":"pc"}}]));
        let vf = fasts(&build_plan(&p, &hd(), None, None).unwrap())[0].fx_v.clone();
        let kinds: Vec<&str> = vf.iter().map(|s| s.split('=').next().unwrap()).collect();
        assert_eq!(kinds, vec!["crop", "crop", "scale", "setsar"], "{vf:?}");
        assert!(vf[0].contains("iw*0.94"), "第一刀是过扫描: {:?}", vf[0]);
        assert!(vf[1].contains("gt(iw/ih,1.363636"), "第二刀按显示画幅居中裁: {:?}", vf[1]);
        assert!(vf[2].contains("out_range=full"), "{:?}", vf[2]);
        assert!(vf[3].starts_with("setsar=10/11"), "{:?}", vf[3]);
    }

    #[test]
    fn resize_pad_and_invalid_ratio() {
        let p = preset_from(serde_json::json!([{"stage":"resize","params":{
            "w":640,"h":480,"dar":"4:3","fit":"pad"}}]));
        let vf = fasts(&build_plan(&p, &hd(), None, None).unwrap())[0].fx_v.clone();
        assert_eq!(vf.len(), 3, "{vf:?}");
        assert!(vf[0].contains("force_original_aspect_ratio=decrease"), "{:?}", vf[0]);
        assert!(vf[1].starts_with("pad=640:480"), "{:?}", vf[1]);
        assert!(vf[2].starts_with("setdar=4/3"), "{:?}", vf[2]);
        // 非法比例必须报错,不许静默出怪图
        for bad in ["abc", "0:3", "4:0", "1:2:3"] {
            let p = preset_from(serde_json::json!([{"stage":"resize","params":{"w":640,"h":480,"par":bad}}]));
            assert!(build_plan(&p, &hd(), None, None).is_err(), "par={bad} 本该拒绝");
        }
        let p = preset_from(serde_json::json!([{"stage":"resize","params":{"w":640,"h":480,"range":"lim"}}]));
        assert!(build_plan(&p, &hd(), None, None).is_err());
    }

    #[test]
    fn par_change_unsquashes_before_rescale() {
        // SD 非方像素 → HD 方形像素交付:不先去挤压,人脸会被拉瘦(实测活动区 1080 而非 982)
        let p = preset_from(serde_json::json!([
            {"stage":"resize","params":{"w":720,"h":480,"par":"10:11","dar":"15:11","fit":"crop"}},
            {"stage":"resize","params":{"mode":"source_canvas","par":"1:1","fit":"pad"}}
        ]));
        let vf = fasts(&build_plan(&p, &hd(), None, None).unwrap())[0].fx_v.clone();
        let joined = vf.join(",");
        assert!(vf.iter().any(|s| s.starts_with("scale='trunc(iw*10/11/2)*2'")), "{joined}");
        assert!(vf.iter().any(|s| s == "setsar=1"), "{joined}");
        assert!(vf.iter().any(|s| s.starts_with("pad=1920:1080")), "{joined}");
        // 像素比没变时不该多插一刀
        let same = preset_from(serde_json::json!([
            {"stage":"resize","params":{"w":720,"h":480,"par":"10:11"}},
            {"stage":"resize","params":{"w":352,"h":240,"par":"10:11"}}
        ]));
        let vf2 = fasts(&build_plan(&same, &hd(), None, None).unwrap())[0].fx_v.clone();
        assert!(!vf2.iter().any(|s| s.contains("trunc(iw*10/11")), "{:?}", vf2);
    }

    #[test]
    fn full_range_is_tagged_on_delivery() {
        // 像素是全范围而容器说 unknown,播放器就会把黑场再压一遍
        let p = preset_from(serde_json::json!([{"stage":"resize","params":{"w":640,"h":480,"range":"pc"}}]));
        let plan = build_plan(&p, &hd(), None, None).unwrap();
        let last = fasts(&plan).last().expect("应有最终趟").vcodec.join(" ");
        assert!(last.contains("-color_range pc"), "{last}");
        let p2 = preset_from(serde_json::json!([{"stage":"resize","params":{"w":640,"h":480}}]));
        let plan2 = build_plan(&p2, &hd(), None, None).unwrap();
        let last2 = fasts(&plan2).last().unwrap().vcodec.join(" ");
        assert!(!last2.contains("-color_range"), "未声明范围时不该多塞标签: {last2}");
    }

    #[test]
    fn dar_from_par_matches_real_rasters() {
        assert_eq!(dar_from_par("10:11", 720, 480).unwrap(), "15:11");
        assert_eq!(dar_from_par("12:11", 720, 576).unwrap(), "15:11");
        assert_eq!(dar_from_par("12:11", 176, 144).unwrap(), "4:3");
        assert_eq!(dar_from_par("1:1", 1920, 1080).unwrap(), "16:9");
        assert!(dar_from_par("10/11", 352, 240).unwrap() == "4:3", "斜杠写法也应接受");
    }

    #[test]
    fn pixel_segment_carries_aspect_for_the_encoder() {
        // rawvideo 没有 SAR 元数据通道,画幅只能靠 -aspect 带过去
        let p = preset_from(serde_json::json!([
            {"stage":"resize","params":{"w":720,"h":480,"par":"10:11","dar":"15:11","fit":"crop"}},
            {"stage":"ntsc_vhs","params":{"seed":1}}
        ]));
        let plan = build_plan(&p, &hd(), None, None).unwrap();
        let seg = plan
            .steps
            .iter()
            .find_map(|s| match s { Step::Pixel(g) => Some(g), _ => None })
            .expect("应有像素段");
        assert_eq!(seg.aspect.as_deref(), Some("15:11"));
        assert_eq!((seg.w, seg.h), (720, 480));
    }

    #[test]
    fn comb_field_rate_follows_chain_fps_not_stale_refps() {
        // fps 与 comb.refps 曾是两份拷贝:改帧率忘改 refps 就悄悄漂回 30
        let p = preset_from(serde_json::json!([
            {"stage":"fps","params":{"fps":12.5,"round":"down"}},
            {"stage":"interlace_comb","params":{"mode":"merge","refps":30.0}}
        ]));
        let chain = fasts(&build_plan(&p, &hd(), None, None).unwrap())
            .into_iter()
            .map(|p| p.fx_v.join(","))
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(
            chain.matches("fps=12.5").count(),
            2,
            "stage 与 comb 应共用同一个生效帧率: {chain}"
        );
        assert!(!chain.contains("fps=30"), "comb 不该再吐出过期的 refps: {chain}");
    }

    #[test]
    fn pixel_segment_carries_effective_fps() {
        // 像素段此前一律按源帧率跑,fps stage 在白名单外失效(§13.2)
        let p = preset_from(serde_json::json!([
            {"stage":"fps","params":{"fps":12.5,"round":"down","shutter":0.5}},
            {"stage":"ntsc_vhs","params":{"seed":7}}
        ]));
        let plan = build_plan(&p, &hd(), None, None).unwrap();
        let seg = plan
            .steps
            .iter()
            .find_map(|s| match s { Step::Pixel(g) => Some(g), _ => None })
            .expect("应有像素段");
        let st = seg.fps.as_ref().expect("像素段必须带生效帧率");
        assert_eq!(st.fps, 12.5);
        assert_eq!(st.round, "down");
        assert!((st.shutter - 0.5).abs() < 1e-9);
    }

    #[test]
    fn image_inputs_drop_temporal_stages() {
        // 静帧走 tinterlace 会输出 0 帧(实测),图片模式必须剥掉时序 stage
        let mut p = load("vhs1990_static");
        p.video.push(VideoStage::TapeEnds { head: 0.6, tail: 0.6, intensity: 30 });
        let chain_of = |plan: &Plan| {
            fasts(plan).iter().map(|f| f.fx_v.join(",")).collect::<Vec<_>>().join(";")
        };
        let video = chain_of(&build_plan(&p, &hd(), None, None).unwrap());
        assert!(video.contains("tinterlace"), "视频输入应保留隔行: {video}");
        assert!(video.contains("fps="), "视频输入应保留帧率: {video}");
        let img = MediaInfo { width: 640, height: 360, fps: 25.0, duration: 0.0, has_audio: false,
            container: "png_pipe".into() };
        let still = chain_of(&build_plan(&p, &img, None, None).unwrap());
        assert!(!still.contains("tinterlace"), "静帧不该有隔行: {still}");
        assert!(!still.contains("fps="), "静帧不该有帧率: {still}");
        assert!(!still.contains("enable="), "静帧不该有头尾时间窗: {still}");
        assert!(still.contains("noise=alls="), "空间性噪声应保留: {still}");
        assert!(still.contains("colormatrix"), "色域往返应保留: {still}");
    }

    #[test]
    fn vhs_static_has_height_fix_and_hiss() {
        let plan = build_plan(&load("vhs1990_static"), &hd(), None, None).unwrap();
        let p = &fasts(&plan)[0];
        let i = p.fx_v.iter().position(|x| x == "tinterlace=mode=merge").unwrap();
        assert_eq!(p.fx_v[i + 1], "scale=720:480:flags=bicubic", "merge 后必须缩回原高");
        assert!(p.extra_inputs.iter().any(|e| e.iter().any(|x| x.contains("anoisesrc"))));
        let fc = p.filter_complex(true).unwrap();
        assert!(fc.contains("amix=inputs=2:duration=first[a]"), "{fc}");
    }

    #[test]
    fn phone_audio_from_original() {
        let plan = build_plan(&load("phone2010"), &hd(), None, None).unwrap();
        let f = fasts(&plan);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].audio, AudioMode::None);
        assert!(f[0].vcodec.iter().any(|x| x == "h263"));
        let last = &f[1];
        assert!(matches!(last.audio, AudioMode::Encode { from_original: true, .. }));
        assert!(last.fx_a.iter().any(|a| a.contains("acrusher")));
        assert!(last.fx_a.iter().any(|a| a == "aresample=8000"));
    }

    #[test]
    fn tape_ends_uses_timeline_enable() {
        let plan = build_plan(&load("vhs1990_static"), &hd(), None, None).unwrap();
        let fx = fasts(&plan).last().unwrap().fx_v.join(",");
        assert!(fx.contains("enable='lt(t,0.6)+gt(t,9.4)'"), "头尾雪花窗口缺失: {fx}");
    }

    /// 头尾窗口之和 ≥ 时长时,`enable` 退化成"全片恒真" —— 这是**有意的**:用户要的是
    /// "至少和素材一样长的穿带损坏",悄悄把 tail 缩掉、中间留一段干净才是替他做决定。
    /// 审计把这报成缺陷,量完判定为解释正确、行为保留,所以钉成断言,免得被顺手"优化"掉。
    #[test]
    fn tape_ends_cover_the_whole_clip_on_purpose_when_they_overflow() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("presets");
        let p = Preset::load_with_overrides(
            &dir.join("vhs1990_static.json"),
            &["tape_ends.head=5".into(), "tape_ends.tail=5".into()],
        )
        .unwrap();
        let fx = fasts(&build_plan(&p, &hd(), None, None).unwrap()).last().unwrap().fx_v.join(",");
        assert!(fx.contains("enable='lt(t,5)+gt(t,5)'"), "退化窗口没按预期覆盖全片: {fx}");
        // 对照:时长足够时,头尾仍是两段分开的窗口
        let g = fasts(&build_plan(&load("vhs1990_static"), &hd(), None, None).unwrap())
            .last()
            .unwrap()
            .fx_v
            .join(",");
        assert!(g.contains("lt(t,0.6)+gt(t,9.4)"), "常态窗口被改动: {g}");
    }

    #[test]
    fn s1_color_space_stages_present() {
        let plan_c = build_plan(&load("cctv2000"), &hd(), None, None).unwrap();
        let p = fasts(&plan_c)[0];
        assert_eq!(p.fx_v[0], "format=yuv411p", "S1 色度抽稀应为第一步");
        assert_eq!(p.fx_v[1], "format=yuv420p");
        let plan_v = build_plan(&load("vhs1990_static"), &hd(), None, None).unwrap();
        let fx = fasts(&plan_v)[0].fx_v.join(",");
        assert!(fx.contains("colormatrix=bt709:bt601,colormatrix=bt601:bt709"), "矩阵往返缺失: {fx}");
    }

    #[test]
    fn crt_preset_is_mixed_step_plan() {
        let plan = build_plan(&load("crt1995"), &hd(), None, None).unwrap();
        // Fast(resize) -> Pixel(crt) -> Fast(拉回源画布+音频)
        assert!(matches!(plan.steps[0], Step::Fast(_)));
        let seg = match &plan.steps[1] {
            Step::Pixel(s) => s,
            _ => panic!("第二步应为像素段"),
        };
        assert_eq!(seg.ops.len(), 1);
        match seg.ops[0] {
            PixelOp::Crt { scanline, barrel, aberration, .. } => {
                assert!(scanline > 0.05 && scanline < 1.0 && barrel > 0.0 && barrel < 0.5 && aberration > 0.0 && aberration < 1.5);
            }
            ref o => panic!("期望 Crt,实际 {o:?}"),
        }
        assert_eq!((seg.w, seg.h), (960, 720));
        assert!(matches!(plan.steps.last(), Some(Step::Fast(_))));
    }

    #[test]
    fn film_preset_pre_noise_then_pixel() {
        let plan = build_plan(&load("film1970"), &hd(), None, None).unwrap();
        assert!(matches!(plan.steps[0], Step::Fast(_)), "fade+noise 应先走 ffmpeg 趟");
        let has_pixel = plan.steps.iter().any(|s| matches!(s, Step::Pixel(_)));
        assert!(has_pixel);
    }

    #[test]
    fn era_1990_keeps_stage_order_across_two_pixel_segs() {
        // FilmDamage -> (resize/noise/color) -> Ntsc:两个像素段被中间 ffmpeg 趟隔开,顺序不得漂移
        let plan = build_plan(&crate::era::preset_for_year(1990), &hd(), None, None).unwrap();
        let kinds: Vec<&str> = plan
            .steps
            .iter()
            .map(|s| match s {
                Step::Fast(_) => "F",
                Step::Pixel(_) => "P",
            })
            .collect();
        assert_eq!(kinds, vec!["F", "P", "F", "P", "F"], "步骤序列: {kinds:?}");
        let seg2_w = match &plan.steps[3] {
            Step::Pixel(s) => s.w,
            _ => unreachable!(),
        };
        assert_eq!(seg2_w, 720, "第二段应在降采样之后(480p 画布)");
    }

    fn odd_src() -> MediaInfo {
        MediaInfo { width: 321, height: 211, fps: 25.0, duration: 4.0, has_audio: true,
        container: "mov,mp4,m4a,3gp,3g2,mj2".into() }
    }

    /// 真实坑:用户上传 2560×1439 的图,film1970 第一趟 libx264 就报 "height not divisible by 2"。
    /// yuv420 类中间编码器要求偶数边长,所以链路第 0 步必须负责把画布取偶。
    #[test]
    fn odd_source_dims_normalized_at_step_zero() {
        for id in ["film1970", "dvd2005", "cctv2000", "vhs1990_ntscrs", "phone2010"] {
            let plan = build_plan(&load(id), &odd_src(), None, None).unwrap();
            match plan.steps.first().unwrap() {
                Step::Fast(p) => assert!(
                    p.fx_v.iter().any(|x| x == "scale=320:210:flags=area"),
                    "{id} 第 0 趟没取偶: {:?}",
                    p.fx_v
                ),
                // 像素段自带 scale=seg.w:seg.h,取偶落在段尺寸上即可
                Step::Pixel(s) => assert_eq!((s.w, s.h), (320, 210), "{id} 像素段画布仍是奇数"),
            }
        }
    }

    /// 偶数源不该平白多出一趟取偶(画廊与所有既有回归都靠这条不变成)
    #[test]
    fn even_source_gets_no_extra_head_scale() {
        let plan = build_plan(&load("dvd2005"), &hd(), None, None).unwrap();
        let f = fasts(&plan);
        // 首趟第一个滤镜就是预设自己的 resize,前面没插取偶
        assert_eq!(f[0].fx_v[0], "scale=640:360:flags=bicubic");
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn odd_resize_targets_round_down() {
        let mut p = load("dvd2005");
        let mut hit = false;
        for s in p.video.iter_mut() {
            if let VideoStage::Resize { w, h, .. } = s {
                *w = Some(641);
                *h = Some(361);
                hit = true;
                break;
            }
        }
        assert!(hit, "dvd2005 应有 resize stage");
        let plan = build_plan(&p, &hd(), None, None).unwrap();
        assert!(
            fasts(&plan).iter().any(|x| x.fx_v.iter().any(|v| v.starts_with("scale=640:360:"))),
            "奇数目标没被取偶: {:?}",
            fasts(&plan)[0].fx_v
        );
    }

    /// 做旧系数:预设自带的那趟算第 1 手,N 手 = N−1 代阶梯,末趟恒拉回交付画幅

    /// 核尺寸必须被贴进 ffmpeg 能接受的形状:奇数、3–13。
    /// 清单那边给的是同一区间的奇数档位,这里管的是"手写预设给了 6 / 15 / 2 也不许死"。
    #[test]
    fn unsharp_kernel_size_is_clamped_to_odd_ffmpeg_range() {
        for (input, want) in [(2u32, "unsharp=3:3"), (6, "unsharp=7:7"), (14, "unsharp=13:13"), (99, "unsharp=13:13")] {
            let raw = format!(
                r#"{{"schema":{},"id":"probe","name":"探针","era":2005,"video":[{{"stage":"unsharp","params":{{"size":{},"amount":1.2}}}}],"audio":[]}}"#,
                crate::preset::SCHEMA_VERSION, input
            );
            let p: Preset = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("预设造不出来: {e}"));
            let plan = build_plan(&p, &hd(), None, None).unwrap();
            let hits: Vec<String> = fasts(&plan)
                .iter()
                .flat_map(|x| x.fx_v.clone())
                .filter(|v| v.starts_with("unsharp="))
                .collect();
            assert!(
                hits.iter().any(|v| v.starts_with(want)),
                "size={input} 应贴成 {want}… 实际 {hits:?}"
            );
            for v in &hits {
                let n: u32 = v.trim_start_matches("unsharp=").split(':').next().unwrap().parse().unwrap();
                assert!(n % 2 == 1 && (3..=13).contains(&n), "滤镜里的核尺寸必须是 3–13 的奇数: {v}");
            }
        }
    }

    #[test]
    fn aging_ladder_adds_hands_and_restores_canvas() {
        let mut p = load("dvd2005");
        p.aging = Some(4);
        let plan = build_plan(&p, &hd(), None, None).unwrap();
        let f = fasts(&plan);
        let mjpeg = f.iter().filter(|x| x.vcodec.iter().any(|v| v == "mjpeg")).count();
        assert_eq!(mjpeg, 3, "4 手应是 3 代阶梯(预设自己那趟不重复)");
        assert!(
            f.last().unwrap().fx_v.iter().any(|x| x.starts_with("scale=1920:1080")),
            "末趟必须拉回预设的交付画幅: {:?}",
            f.last().unwrap().fx_v
        );
        assert!(
            f.iter().all(|x| !x.fx_v.iter().any(|v| v.contains("unsharp"))),
            "4 手不该出现锐化:爆炸档从 6 手起"
        );
        // 阶梯逐手变小且全为偶数。预设自己就有"降到 640 再拉回 1920"两趟,
        // 所以只断言阶梯那三段的确切尺寸(canvas 1920 × 0.72^g,向下取偶)
        let widths: Vec<u32> = f
            .iter()
            .flat_map(|x| x.fx_v.clone())
            .filter_map(|v| v.strip_prefix("scale=").and_then(|r| r.split(':').next()?.parse().ok()))
            .collect();
        let ladder: Vec<u32> = widths.iter().copied().filter(|w| *w < 1920 && *w > 640).collect();
        assert_eq!(ladder, vec![1382, 994, 716], "阶梯尺寸应为 1920×0.72^g 取偶: {widths:?}");
    }

    #[test]
    fn blow_tier_turns_on_sharpening_at_six_hands() {
        let mut p = load("dvd2005");
        p.aging = Some(6);
        let plan = build_plan(&p, &hd(), None, None).unwrap();
        let f = fasts(&plan);
        assert!(
            f.iter().any(|x| x.fx_v.iter().any(|v| v.starts_with("unsharp=5:5:1.2"))),
            "第 6 手应有振铃分量"
        );
    }

    /// 色阶断层默认只砍亮度。砍到 u/v 上会把色度压成约 6 级并整体发绿 —— 那是用户报回的缺陷,不是味道。
    #[test]
    fn band_quantize_is_luma_only_by_default() {
        let chain = fasts(&build_plan(&load("patina"), &hd(), None, None).unwrap())
            .iter()
            .flat_map(|p| p.fx_v.clone())
            .filter(|v| v.starts_with("lutyuv"))
            .collect::<Vec<_>>();
        assert!(!chain.is_empty(), "patina 应有色阶断层");
        assert!(
            chain.iter().all(|v| !v.contains(":u=") && !v.contains(":v=")),
            "断层默认不许碰色度: {chain:?}"
        );
        let mut p = load("patina");
        p.video = p
            .video
            .into_iter()
            .map(|s| match s {
                VideoStage::BandQuantize { level, .. } => VideoStage::BandQuantize { level, chroma: true },
                other => other,
            })
            .collect();
        let on = fasts(&build_plan(&p, &hd(), None, None).unwrap())
            .iter()
            .flat_map(|x| x.fx_v.clone())
            .find(|v| v.starts_with("lutyuv"))
            .expect("chroma=true 应带 u/v 通道");
        assert!(on.contains(":u="), "显式开启才砍色度: {on}");
    }

    /// 偏色默认关:阶梯一路不产生 colorbalance;开了 cast 才往青绿推
    #[test]
    fn cast_defaults_to_no_hue_shift_and_only_tints_when_asked() {
        let chains = |cast: Option<f64>| {
            let mut p = load("patina");
            p.aging = Some(6);
            p.cast = cast;
            fasts(&build_plan(&p, &hd(), None, None).unwrap())
                .iter()
                .flat_map(|x| x.fx_v.clone())
                .filter(|v| v.starts_with("colorbalance"))
                .collect::<Vec<_>>()
        };
        assert!(chains(None).is_empty(), "默认不该有任何偏色滤镜");
        assert!(chains(Some(0.0)).is_empty(), "cast=0 不该有任何偏色滤镜");
        let on = chains(Some(1.0));
        assert_eq!(on.len(), 5, "6 手 = 5 代阶梯,每代一条 colorbalance: {on:?}");
        assert!(on[0].contains("gm=0.11"), "第 1 代阶梯的绿偏量: {}", on[0]);
        assert!(on[4].contains("gm=0.5"), "满档应给到看得见的量级: {}", on[4]);
    }

    #[test]
    fn aging_one_is_identical_to_no_aging() {
        let p = load("film1970");
        let mut q = p.clone();
        q.aging = Some(1);
        let a = build_plan(&p, &hd(), None, None).unwrap();
        let b = build_plan(&q, &hd(), None, None).unwrap();
        let chains = |plan: &Plan| -> Vec<String> {
            plan.steps
                .iter()
                .map(|s| match s {
                    Step::Fast(x) => x.fx_v.join(","),
                    Step::Pixel(x) => format!("pixel:{}x{}", x.w, x.h),
                })
                .collect()
        };
        assert_eq!(chains(&a), chains(&b), "aging=1 不该改变任何一趟");
    }
}
