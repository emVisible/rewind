// 双语层。中文是界面原文,英文是翻译。
//
// 分工刻意的:引擎发来的中文(参数 label、预设名、分组名)**不改引擎**,英文按机器 id
// 查术语表。这样"界面按清单渲染、加参数不改 UI"这条不变;而 `scripts/release_check.sh`
// 会断言每个 stage.key / 每个预设 id 都有英文条目 —— 新增参数忘翻译会被判红,不会悄悄漏中文。
(function () {
  const UI = {
    zh: {
      'app.title': 'Rewind · 影像做旧引擎',
      'app.tagline': '影像介质退化与年代质感模拟',
      'meta.desc': 'Rewind 复现记录与传输介质的画质损失:录像带、翻录光盘、CRT 显示、胶片、监控采集、移动网络与多次转存后的电子包浆。按年代与介质重建退化路径,全程在本机完成,不联网、无账号、无遥测。',
      'meta.ogdesc': '录像带 / 翻录光盘 / CRT / 胶片 / 监控 / 3GP / 电子包浆 —— 本地运行的影像做旧引擎,不联网、不上传、无遥测。',
      'pro.open': '高级参数', 'pro.close': '收起高级参数', 'about': '关于', 'about.close': '关闭',
      'about.title': '关于 Rewind · 影像做旧引擎',
      'lang.switch': 'EN',
      'rail.recipe': '预设', 'rail.type': '类型', 'rail.pro': '高级参数',
      'gallery.aria': '预设效果对比', 'strip.aria': '待处理',
      'knob.intensity': '做旧强度', 'knob.aging': '做旧系数', 'knob.cast': '偏色 · 变绿', 'knob.era': '年代轴',
      'era.name': '年代轴 ~{y}',
      'quick.light': '微浆', 'quick.std': '标准', 'quick.heavy': '重手',
      'aging.stop1': '1 手', 'aging.stopMax': '8 手·爆炸', 'aging.blow': '爆炸档 · 约 {x}× 单趟耗时',
      'unit.hands': '{n} 手', 'unit.off': '关',
      'clear.overrides': '清除全部自定义',
      'hero.drop': '拖入视频 / 图片,或', 'hero.pick': '选择文件', 'hero.try': '试试示例素材',
      'wipe.src': '原片', 'wipe.out': '做旧', 'result.alt': '成品',
      'viewer.swap': '更换素材', 'viewer.drop': '移除',
      'geo.out': '输出 {wh}', 'geo.outar': '输出 {wh} · 画幅 {ar}',
      'geo.proxied': '预览按 {pw} 显示',
      'busy.render': '渲染中', 'busy.step': '渲染中 {i}/{n}', 'busy.preview': '预览渲染中', 'busy.run': '做旧中', 'busy.reclip': '再翻录',
      'result.compare': '对比原片', 'result.back': '返回编辑',
      'samples.title': '抽帧确认', 'samples.reroll': '换一批帧', 'samples.confirm': '看过了,可以开跑',
      'samples.loading': '正在抽帧…', 'samples.count': '{n} 帧 · 素材 {d}s', 'samples.none': '这素材抽不出帧',
      'samples.at': '第 {t}s',
      'done.view': '浏览成品', 'done.save': '保存成品', 'done.copy': '复制路径',
      'done.again': '参数不变再来一次', 'done.next': '换一个',
      'act.start': '开始做旧', 'act.reclip': '⧉ 再翻录一次', 'act.reroll': '🎲 换一批',
      'act.cancel': '取消', 'act.diag': '复制诊断', 'act.diag.title': '复制诊断信息',
      'status.ready': '就绪', 'status.item': '就绪:{name}',
      'status.gate': '这段 {dur} · {size}:先点一帧确认要跑',
      'status.confirmed': '已确认,可以开跑了',
      'status.overrides': '已自定义 {n} 项', 'status.aging': '做旧 {n} 手', 'status.cast': '偏色 {p}%',
      'estimate.worst': '最多约 {t}(预览窗口上界)', 'estimate.measured': '预计 {t}(本机同参数实测)',
      'estimate.gate': ' · 先抽帧确认',
      'unit.sec': '{n} 秒', 'unit.min': '{m} 分 {s} 秒', 'unit.hour': '{h} 小时 {m} 分',
      'unit.min.short': '{n} 分钟',
      'media.loading': '读取素材信息…', 'media.error': '素材读不了:{e}',
      'media.name': '素材', 'media.size': '尺寸', 'media.dar': '画幅', 'media.fps': '帧率',
      'media.kind': '类型', 'media.still': '静帧', 'media.duration': '时长', 'media.codec': '编码', 'media.bytes': '大小',
      'media.warn.vfr': '变帧率源:节奏类效果会不稳,建议先转固定帧率',
      'media.warn.hdr': 'HDR/10bit 源:会先转 SDR 再做旧',
      'media.warn.hevc': '{c} 源:解码更慢,耐心等',
      'media.warn.noaudio': '这条素材没有音轨,音频做旧不会生效',
      'warn.nofont': '本机没有可用字体,时间戳不会出现',
      'media.warn.big': '大文件:临时空间约需 2× 源大小',
      'pop.motion': '动图', 'pop.alt': '{name} 对比图', 'pop.meta': '~{era} · {n} 手',
      'card.fav': '收藏(排最前)', 'card.remove': '移除这个素材',
      'card.default': '默认',
      'seed.label': '{s} 种子', 'seed.copy': '复制', 'seed.lock': '锁定', 'seed.locked': '🔒 已锁',
      'seed.copied': '种子已复制', 'seed.locktitle': '锁定后「换一批」不再改这个种子',
      'variant.default': '默认', 'variant.title': '同一个预设的另一种实现:{label}',
      'recipe.derived': '基于「{name}」微调', 'recipe.save': '保存为我的预设',
      'recipe.prompt': '给这个预设起个名字', 'recipe.saved': '已保存为我的预设:{name}',
      'recipe.mine': '我的预设',
      'recipe.fail': '保存失败:{e}', 'recipe.catalog': '刷新预设列表:{e}',
      'knob.follow': '跟随预设', 'knob.source': '源', 'knob.srctitle': '固定为当前素材的值',
      'knob.needsrc': '先载入素材才能跟随源', 'knob.yes': '是', 'knob.no': '否',
      'knob.empty': '引擎未提供参数清单(rewind-core describe),只能整体套用预设。',
      'err.preview': '预览失败:{e}', 'err.previewframe': '预览帧读不到(服务可能重启过)',
      'err.engine': '引擎不可用:{e}', 'err.era': '年代轴失败:{e}', 'err.eraok': '年代轴 ~{y} 已就绪',
      'err.reroll': '换一批失败:{e}', 'reroll.done': '已换一批({n} 处种子重抽)', 'reroll.none': '这个预设没有可重抽的种子',
      // 引擎错误的稳定码(core/src/errcode.rs 的 CODES)。中文直接看引擎原句 —— 码是给机器看的,
      // 所以这里只把码摘掉。新增码但这里没跟上 = 界面原样显示(带码),不会崩也不会显示键名。
      'err.media.unreadable': '{e}', 'err.engine.ffprobe': '{e}', 'err.engine.frame': '{e}',
      'err.engine.pass': '{e}', 'err.engine.disk_full': '{e}', 'err.engine.exit': '{e}',
      'err.engine.spawn': '{e}', 'err.engine.output': '{e}', 'err.run.canceled': '{e}',
      'err.fs.outdir': '{e}', 'err.preset.load': '{e}',
      'err.preset.save': '{e}', 'err.asset.stage': '{e}', 'err.param.unknown': '{e}',
      'err.param.range': '{e}', 'err.upload.too_large': '{e}', 'err.upload.bad_type': '{e}',
      'err.upload.decode': '{e}', 'err.asset.missing': '{e}', 'err.upload.interrupt': '{e}',
      'run.start': '开始做旧:{name}', 'run.fail': '启动失败:{e}', 'run.reclipfail': '翻录失败:{e}',
      'run.done': '完成 → {name}', 'run.doneReclip': '翻录完成,又老了一手', 'run.failed': '失败:{e}',
      'run.preset': '(预设 {p})', 'run.canceling': '取消中…', 'run.canceled': '已取消',
      'run.orphan': '第 {i} 项的结果找不到归属(素材已被移除),没有写到别的项目上',
      'run.busyRemove': '正在做旧这一项,先取消再移除',
      'queue.removed': '已移除一项,当前:{name}', 'queue.cleared': '已清空素材',
      'queue.replaced': '已更换素材:{name}', 'queue.empty': '(空)',
      'upload.progress': '上传中 {i}/{n}:{name}', 'upload.fail': '上传失败({name}):{e}',
      'upload.done': '已上传 {n} 个文件', 'upload.nosample': '这个构建没带示例素材,请拖入自己的文件',
      'upload.notmedia': '选中的 {n} 个文件不是视频或图片,已忽略',
      'clip.copied': '路径已复制', 'clip.fail': '复制失败,路径在卡片里可手动选',
      'clip.none': '这一项还没有成品可复制',
      'diag.copied': '诊断信息已复制', 'diag.fail': '剪贴板不可用,诊断信息已显示在弹窗里',
      'diag.none': '(无)', 'err.unknown': '未知错误',
      'gate.blocked': '先点一帧确认', 'samples.failed': '抽帧失败:{e}',
      'result.unauthorized': '成品读取未授权:{e}',
      'about.body': '影像做旧引擎:ffmpeg 多趟真实编码往返 + ntsc-rs 信号级模拟 + 自研像素层。全部在本机完成,不联网、无账号、无遥测。',
      'about.watermark': '成品默认嵌入"经 Rewind 做旧处理"元数据 —— 这是反伪造声明:效果是合成的,不是原始素材。',
      'about.meta': '{v} · 参数 {n} 项可调',
      'engine.tag': '引擎 {e}',
    },
    en: {
      'app.title': 'Rewind · Media Degradation Engine',
      'app.tagline': 'Period-accurate media degradation',
      'meta.desc': 'Rewind reproduces the picture loss of recording and transmission media: videotape, ripped discs, CRT displays, film, CCTV, mobile networks and repeated re-encoding. It rebuilds the degradation path for a chosen era or medium, entirely on your machine — no network, no account, no telemetry.',
      'meta.ogdesc': 'Videotape / ripped discs / CRT / film / CCTV / 3GP / internet patina — a media-degradation engine that runs locally. No network, no upload, no telemetry.',
      'pro.open': 'Pro settings', 'pro.close': 'Hide pro settings', 'about': 'About', 'about.close': 'Close',
      'about.title': 'About Rewind · media degradation engine',
      'lang.switch': '中文',
      'rail.recipe': 'Presets', 'rail.type': 'Type', 'rail.pro': 'Pro settings',
      'gallery.aria': 'Preset comparison', 'strip.aria': 'Queue',
      'knob.intensity': 'Aging strength', 'knob.aging': 'Generations', 'knob.cast': 'Colour cast', 'knob.era': 'Era',
      'era.name': 'Era ~{y}',
      'quick.light': 'Light', 'quick.std': 'Standard', 'quick.heavy': 'Heavy',
      'aging.stop1': '1', 'aging.stopMax': '8 · deep-fried', 'aging.blow': 'Deep-fried · about {x}× the cost of one pass',
      'unit.hands': '{n} gens', 'unit.off': 'Off',
      'clear.overrides': 'Clear all custom settings',
      'hero.drop': 'Drop a video / image, or', 'hero.pick': 'choose files', 'hero.try': 'Try a sample',
      'wipe.src': 'Original', 'wipe.out': 'Aged', 'result.alt': 'Result',
      'viewer.swap': 'Replace', 'viewer.drop': 'Remove',
      'geo.out': 'Output {wh}', 'geo.outar': 'Output {wh} · {ar}',
      'geo.proxied': 'preview shown at {pw}px',
      'busy.render': 'Rendering', 'busy.step': 'Pass {i} of {n}', 'busy.preview': 'Rendering preview', 'busy.run': 'Aging', 'busy.reclip': 'Re-encoding',
      'result.compare': 'Compare', 'result.back': 'Back to editing',
      'samples.title': 'Frame check', 'samples.reroll': 'New frames', 'samples.confirm': 'Looks right — go',
      'samples.loading': 'Sampling…', 'samples.count': '{n} frames · {d}s clip', 'samples.none': 'No frames could be sampled',
      'samples.at': '{t}s',
      'done.view': 'View result', 'done.save': 'Save result', 'done.copy': 'Copy path',
      'done.again': 'Run again', 'done.next': 'Next',
      'act.start': 'Start', 'act.reclip': '⧉ Re-encode', 'act.reroll': '🎲 Reroll',
      'act.cancel': 'Cancel', 'act.diag': 'Copy diagnostics', 'act.diag.title': 'Copy diagnostic info',
      'status.ready': 'Ready', 'status.item': 'Ready: {name}',
      'status.gate': 'This clip is {dur} · {size} — check a frame before running',
      'status.confirmed': 'Confirmed — you can run it',
      'status.overrides': '{n} customised', 'status.aging': '{n} generations', 'status.cast': 'Cast {p}%',
      'estimate.worst': 'Up to {t} (preview-window bound)', 'estimate.measured': 'About {t} (measured here)',
      'estimate.gate': ' · check a frame first',
      'unit.sec': '{n}s', 'unit.min': '{m}m {s}s', 'unit.hour': '{h}h {m}m',
      'unit.min.short': '{n} min',
      'media.loading': 'Reading clip info…', 'media.error': 'Cannot read clip: {e}',
      'media.name': 'Clip', 'media.size': 'Frame', 'media.dar': 'Aspect', 'media.fps': 'FPS',
      'media.kind': 'Kind', 'media.still': 'Still', 'media.duration': 'Length', 'media.codec': 'Codec', 'media.bytes': 'Size',
      'media.warn.vfr': 'Variable frame rate: timing effects get unstable — convert to CFR first',
      'media.warn.hdr': 'HDR / 10-bit source: it will be tone-mapped to SDR first',
      'media.warn.hevc': '{c} source: decoding is slower, be patient',
      'media.warn.noaudio': 'This clip has no audio track — audio degradation cannot apply',
      'warn.nofont': 'No usable font on this machine — the timestamp will not appear',
      'media.warn.big': 'Large file: needs roughly 2× the source size in temp space',
      'pop.motion': 'Motion', 'pop.alt': '{name} comparison', 'pop.meta': '~{era} · {n} gens',
      'card.fav': 'Favourite (pins to front)', 'card.remove': 'Remove this clip',
      'card.default': 'Default',
      'seed.label': '{s} seed', 'seed.copy': 'Copy', 'seed.lock': 'Lock', 'seed.locked': '🔒 Locked',
      'seed.copied': 'Seed copied', 'seed.locktitle': 'Locked seeds survive Reroll',
      'variant.default': 'Default', 'variant.title': 'Another implementation of the same preset: {label}',
      'recipe.derived': 'Tweaked from {name}', 'recipe.save': 'Save as my preset',
      'recipe.prompt': 'Name this preset', 'recipe.saved': 'Saved as preset: {name}',
      'recipe.mine': 'Mine',
      'recipe.fail': 'Save failed: {e}', 'recipe.catalog': 'Preset list refresh failed: {e}',
      'knob.follow': 'Follow preset', 'knob.source': 'Source', 'knob.srctitle': 'Lock to the clip value',
      'knob.needsrc': 'Load a clip first to follow the source', 'knob.yes': 'Yes', 'knob.no': 'No',
      'knob.empty': 'The engine published no parameter manifest (rewind-core describe); only whole presets can be applied.',
      'err.preview': 'Preview failed: {e}', 'err.previewframe': 'Preview frame missing (the service may have restarted)',
      'err.engine': 'Engine unavailable: {e}', 'err.era': 'Era failed: {e}', 'err.eraok': 'Era ~{y} ready',
      'err.reroll': 'Reroll failed: {e}', 'reroll.done': 'Rerolled ({n} seeds)', 'reroll.none': 'This preset has no seeds to reroll',
      // Engine error codes (core/src/errcode.rs CODES). The engine writes its detail in Chinese and the
      // numbers/paths in it are what actually helps, so English leads with one actionable sentence and keeps the detail.
      'err.media.unreadable': 'Not a usable video or image: {e}',
      'err.engine.ffprobe': 'Could not read media info for this clip (is it a real video/image?): {e}',
      'err.engine.frame': 'Could not grab a frame at that time: {e}',
      'err.engine.pass': 'A render pass failed: {e}',
      'err.engine.disk_full': 'Disk full — free up space and try again: {e}',
      'err.engine.exit': 'The render engine stopped without reporting a reason: {e}',
      'err.engine.spawn': 'Could not start the render engine (ffmpeg missing?): {e}',
      'err.engine.output': 'The engine returned output we could not read (engine/ffprobe version mismatch?): {e}',
      'err.run.canceled': 'Cancelled by you — nothing was lost: {e}',
      'err.fs.outdir': 'The output folder is not writable: {e}',
      'err.preset.load': 'Could not load this preset: {e}',
      'err.preset.save': 'Could not save this preset: {e}',
      'err.asset.stage': 'Could not unpack the bundled sample clip: {e}',
      'err.param.unknown': 'Unknown parameter name: {e}',
      'err.param.range': 'That value is outside the allowed range: {e}',
      'err.upload.too_large': 'File too large to upload: {e}',
      'err.upload.bad_type': 'Unsupported file type: {e}',
      'err.upload.decode': 'The uploaded file could not be decoded: {e}',
      'err.asset.missing': 'No bundled sample clip in this build: {e}',
      'err.upload.interrupt': 'The upload was cut off before it finished: {e}',
      'run.start': 'Aging: {name}', 'run.fail': 'Could not start: {e}', 'run.reclipfail': 'Re-encode failed: {e}',
      'run.done': 'Done → {name}', 'run.doneReclip': 'Re-encoded — one generation older', 'run.failed': 'Failed: {e}',
      'run.preset': '(preset {p})', 'run.canceling': 'Cancelling…', 'run.canceled': 'Canceled',
      'run.orphan': 'Result for clip {i} has no owner (it was removed); nothing was written elsewhere',
      'run.busyRemove': 'This clip is running — cancel first, then remove it',
      'queue.removed': 'Removed one clip, now: {name}', 'queue.cleared': 'Queue cleared',
      'queue.replaced': 'Clip replaced: {name}', 'queue.empty': '(empty)',
      'upload.progress': 'Uploading {i}/{n}: {name}', 'upload.fail': 'Upload failed ({name}): {e}',
      'upload.done': '{n} files uploaded', 'upload.nosample': 'This build ships no sample — drop your own file',
      'upload.notmedia': '{n} selected files are neither video nor image — ignored',
      'clip.copied': 'Path copied', 'clip.fail': 'Copy failed — the path is selectable in the card',
      'clip.none': 'No finished file to copy for this clip',
      'diag.copied': 'Diagnostics copied', 'diag.fail': 'Clipboard unavailable — diagnostics shown in the dialog',
      'diag.none': '(none)', 'err.unknown': 'Unknown error',
      'gate.blocked': 'Check a frame first', 'samples.failed': 'Sampling failed: {e}',
      'result.unauthorized': 'Result not readable: {e}',
      'about.body': 'A media-degradation engine: real multi-pass ffmpeg re-encodes, ntsc-rs signal simulation and a hand-written pixel layer. Everything runs on this machine — no network, no account, no telemetry.',
      'about.watermark': 'Results carry a "degraded with Rewind" metadata line by default — an anti-forgery statement: the look is synthetic, the footage is not.',
      'about.meta': '{v} · {n} tunable parameters',
      'engine.tag': 'Engine {e}',
    },
  };

  // 引擎字符串的英文术语表(键是机器 id,稳定;中文由引擎自己发)
  const STAGE_EN = {
    resize: 'Frame & pixel aspect', fps: 'Frame rate & cadence', codec_roundtrip: 'Codec generation',
    noise: 'Grain / snow', color: 'Colour', color_fade: 'Fading', band_quantize: 'Band quantise',
    unsharp: 'Sharpen / ringing', asymmetric_lowpass: 'Blur (split luma/chroma low-pass)',
    interlace_comb: 'Interlace comb', overlay_timestamp: 'Timestamp badge', chroma_decimate: 'Chroma decimate',
    matrix_roundtrip: 'Colour matrix round-trip', tape_ends: 'Tape leader noise', ntsc_vhs: 'NTSC signal simulation',
    crt_display: 'CRT display', film_damage: 'Film damage', bandlimit: 'Band-limit', gain: 'Gain',
    bitrate_roundtrip: 'Bitrate round-trip', resample_roundtrip: 'Sample-rate round-trip',
    bitcrush: 'Bit crush', tape_hiss: 'Tape hiss', mono: 'Mono downmix', wow_flutter: 'Wow & flutter',
  };
  const PARAM_EN = {
    'resize.w': 'Stored width', 'resize.h': 'Stored height', 'resize.dar': 'Display aspect',
    'resize.fit': 'Fit', 'resize.par': 'Pixel aspect ratio', 'resize.overscan': 'Overscan',
    'resize.range': 'Quantisation range', 'resize.flags': 'Scaling filter',
    'fps.fps': 'Frame rate', 'fps.round': 'Frame dropping', 'fps.shutter': 'Shutter blur', 'fps.cadence': 'Cadence',
    'codec_roundtrip.codec': 'Encoder', 'codec_roundtrip.q': 'Quantiser q', 'codec_roundtrip.bitrate': 'Bitrate mode',
    'codec_roundtrip.container': 'Container', 'codec_roundtrip.audio_bitrate': 'Audio bitrate',
    'codec_roundtrip.video_only': 'Video only (leave audio to later)',
    'noise.alls': 'Strength', 'noise.allf': 'Temporal mode',
    'color.saturation': 'Saturation', 'color.contrast': 'Contrast', 'color.brightness': 'Brightness',
    'color.green_mid': 'Green mid-tone shift', 'color.blue_mid': 'Blue mid-tone shift',
    'color_fade.strength': 'Fade strength',
    'band_quantize.level': 'Band width', 'band_quantize.chroma': 'Quantise chroma too',
    'unsharp.amount': 'Sharpen amount', 'unsharp.size': 'Kernel size', 'unsharp.chroma_amount': 'Chroma sharpen',
    'asymmetric_lowpass.luma_radius': 'Luma blur radius', 'asymmetric_lowpass.chroma_radius': 'Chroma blur radius',
    'interlace_comb.mode': 'Weave mode', 'interlace_comb.refps': 'Field-rate base',
    'overlay_timestamp.format': 'Time format', 'overlay_timestamp.rec_badge': 'REC badge',
    'chroma_decimate.to': 'Chroma sampling',
    'tape_ends.head': 'Head length', 'tape_ends.tail': 'Tail length', 'tape_ends.intensity': 'Strength',
    'ntsc_vhs.seed': 'Random seed',
    'ntsc_vhs.vhs_tape_speed': 'Tape speed', 'ntsc_vhs.vhs_chroma_loss': 'Chroma loss',
    'ntsc_vhs.vhs_sharpen': 'Tape sharpening', 'ntsc_vhs.vhs_edge_wave': 'Edge waviness',
    'ntsc_vhs.tracking_noise_height': 'Tracking noise band', 'ntsc_vhs.tracking_noise_wave_intensity': 'Tracking warp',
    'ntsc_vhs.head_switching_height': 'Head-switching band', 'ntsc_vhs.head_switching_horizontal_shift': 'Head-switch shift',
    'ntsc_vhs.snow': 'Snow', 'ntsc_vhs.luma_smear': 'Luma smear',
    'ntsc_vhs.chroma_delay_horizontal': 'Chroma lag (H)', 'ntsc_vhs.chroma_delay_vertical': 'Chroma lag (V)',
    'crt_display.scanline': 'Scanlines', 'crt_display.barrel': 'Barrel distortion',
    'crt_display.aberration': 'Colour fringing', 'crt_display.persistence': 'Phosphor persistence',
    'film_damage.seed': 'Random seed', 'film_damage.scratches': 'Scratches', 'film_damage.dust': 'Dust',
    'film_damage.flicker': 'Brightness flicker',
    'bandlimit.highpass': 'High-pass', 'bandlimit.lowpass': 'Low-pass',
    'gain.volume': 'Volume', 'bitrate_roundtrip.bitrate': 'Bitrate', 'resample_roundtrip.rate': 'Sample rate',
    'bitcrush.bits': 'Bit depth', 'bitcrush.mode': 'Mode', 'bitcrush.aa': 'Anti-aliasing',
    'tape_hiss.color': 'Noise colour', 'tape_hiss.amplitude': 'Amplitude', 'tape_hiss.mix_weight': 'Mix weight',
    'wow_flutter.freq': 'Frequency', 'wow_flutter.depth': 'Depth',
    'preset.aging': 'Generations', 'preset.cast': 'Colour cast',
  };
  const GROUP_EN = {
    geometry: 'Frame & aspect', temporal: 'Timing & cadence', clarity: 'Clarity & bandwidth',
    compression: 'Codec generation', signal: 'NTSC signal', chroma: 'Chroma', color: 'Colour',
    noise: 'Noise', damage: 'Media damage', display: 'Display', stamp: 'Badges & timestamp', audio: 'Audio',
  };
  const CTRL_EN = {
    aspect: 'Frame & system', tempo: 'Frame rate & cadence', clarity: 'Clarity & bandwidth', generation: 'Generation & codec',
  };
  const PRESET_EN = {
    film1970: '8mm Film', vhs1990_ntscrs: 'Home VHS', vhs1990_static: 'Home VHS', crt1995: 'CRT Monitor',
    cctv2000: 'CCTV', dvd2005: 'Ripped DVD', rmvb2006: 'Fansub RMVB', phone2010: '3GP Phone',
    screenbat: 'Cameo Re-shot', blurhigh: 'Blur Meme', patina: 'Internet Patina',
  };
  const VARIANT_EN = { 信号级: 'Signal (ntsc-rs)', 静态近似: 'Static filters' };
  /** 清单里的单位是引擎发的中文(只有非 ASCII 的才需要译);纯符号单位(Hz/fps/px/s/bit)原样过。 */
  const UNIT_EN = {
    倍: '×', 偏移: 'offset', 像素: 'px', 比例: 'ratio', 级: 'level', 行: 'lines',
    '行(0=无)': 'lines (0=off)', '0=无 1=SP 2=LP 3=EP': '0=off 1=SP 2=LP 3=EP',
  };

  let lang = 'zh';
  const missing = new Set();

  function t(key, vars) {
    const table = UI[lang] || UI.zh;
    let s = table[key];
    if (s == null) {
      missing.add(key);
      s = UI.zh[key] != null ? UI.zh[key] : key;
    }
    if (vars) for (const k in vars) s = s.split('{' + k + '}').join(vars[k]);
    return s;
  }

  /** 引擎发的是中文:英文模式下按机器 id 查表,查不到就原样显示(并由覆盖率闸报红) */
  function pick(en, zh) { return lang === 'en' && en != null ? en : zh; }

  /**
   * 引擎错误形如 `[稳定码] 原文`(core/src/errcode.rs)。认识这个码就按词典改写:
   * 中文只摘掉码(原句本来就是给人看的),英文换一句可操作的人话、后面照旧附细节。
   * 不认识就整条原样给回 —— 引擎新增了错误而词典没跟上,最坏是露出中文原文,
   * 绝不会显示 'err.xxx' 这种键名,也不会因为少一项就崩。
   */
  function decodeError(raw) {
    const s = String(raw == null ? '' : raw);
    const m = /\[([a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+)\]/.exec(s);
    if (!m) return s;
    const key = 'err.' + m[1];
    const table = UI[lang] || UI.zh;
    if (table[key] == null) return s;
    const e = (s.slice(0, m.index) + ' ' + s.slice(m.index + m[0].length))
      .replace(/^\s*error:\s*/, '').replace(/[ \t]+/g, ' ').trim();
    return t(key, { e });
  }

  const api = {
    t,
    decodeError,
    get lang() { return lang; },
    get missing() { return [...missing]; },
    langs: ['zh', 'en'],
    /** 术语表也暴露出去,给覆盖率闸在浏览器里自检用 */
    glossary: () => ({ STAGE_EN, PARAM_EN, GROUP_EN, CTRL_EN, PRESET_EN, VARIANT_EN, UNIT_EN }),
    stageLabel: (id, zh) => pick(STAGE_EN[id], zh),
    paramLabel: (stage, key, zh) => pick(PARAM_EN[stage + '.' + key], zh),
    unitLabel: (zh) => pick(UNIT_EN[zh], zh),
    groupLabel: (id, zh) => pick(GROUP_EN[id], zh),
    controlLabel: (id, zh) => pick(CTRL_EN[id], zh),
    // 年代轴产物的名字是引擎按模板生成的("年代轴 ~1988"),不在术语表里,单独走模板
    presetName: (p) => (p.id && p.id.startsWith('era_')
      ? t('era.name', { y: p.id.slice(4) })
      : pick(PRESET_EN[p.id], p.name)),
    variantLabel: (label) => pick(VARIANT_EN[label], label),
    setLang(next) {
      lang = UI[next] ? next : 'zh';
      document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
      document.title = t('app.title');
      const meta = (sel, key) => {
        const el = document.querySelector(sel);
        if (el) el.setAttribute('content', t(key));
      };
      meta('#meta-desc', 'meta.desc');
      meta('#meta-og-desc', 'meta.ogdesc');
      const ogTitle = document.querySelector('meta[property="og:title"]');
      if (ogTitle) ogTitle.setAttribute('content', t('app.title'));
      document.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n); });
      document.querySelectorAll('[data-i18n-title]').forEach((el) => { el.title = t(el.dataset.i18nTitle); });
      document.querySelectorAll('[data-i18n-aria]').forEach((el) => { el.setAttribute('aria-label', t(el.dataset.i18nAria)); });
      document.querySelectorAll('[data-i18n-alt]').forEach((el) => { el.setAttribute('alt', t(el.dataset.i18nAlt)); });
      return lang;
    },
    /** 起始语言:设置里没有就跟浏览器走 */
    detect() {
      const nav = (navigator.language || 'zh').toLowerCase();
      return nav.startsWith('zh') ? 'zh' : 'en';
    },
  };
  window.__REWIND_I18N__ = api;
})();
