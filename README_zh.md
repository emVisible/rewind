# Rewind

English: **[README.md](README.md)**

Rewind 把清晰的数字素材做成有年代感的低画质:家用录像带、CRT 电视、8mm 胶片、监控录像、
翻录 DVD、字幕组 RMVB、手机 3GP、屏摄翻拍,以及反复转存之后的电子包浆。年代轴覆盖 1965–2026,
任意年份都能编译成预设。

全部在本地处理:不上传、无账号、无遥测、不发起外部请求。自带 Web 服务只监听 `127.0.0.1`。

成品文件的元数据里会写明效果是合成的,见[溯源](#溯源)。

MIT 许可,第三方条款见[许可与第三方](#许可与第三方)。

## 运行

编译需要 Rust stable(edition 2024)与 ffmpeg ≥ 4.4。

```bash
cd core && cargo build --release        # 产物 ./core/target/release/rewind-core
./webui.sh                              # 本地 Web 界面 http://127.0.0.1:8137/
./webui.sh --rebuild                    # 先建引擎再起
```

`webui.sh` 会检查引擎二进制与 PATH 上的 `ffmpeg`/`ffprobe`,缺什么就告诉你补什么。

Web 界面与桌面版是同一套前端。浏览器添加的素材经回环上传(`POST /upload`)写到本机磁盘;
服务只从白名单里发文件。

桌面应用:

```bash
cd app && cargo build                   # Tauri 2:Linux 需 webkit2gtk-4.1,Windows 需 WebView2 + MSVC
```

## CLI

```bash
B=./core/target/release/rewind-core

$B run     --preset presets/dvd2005.json --input assets/sample/street-food.mp4 --out-dir out/  # → out/street-food_dvd2005.mp4
$B run     --preset presets/cctv2000.json --input in.mp4 --out-dir out/ \
           --override resize.dar=4:3 --override fps.fps=12.5   # 只在内存里改,预设文件不动
$B batch   --preset presets/patina.json --out-dir out/ a.mp4 b.mp4 c.mov
$B era     1988 --write my88.json                              # 把任意年份编译成预设
$B reclip  --input out/street-food_dvd2005.mp4 --times 2       # 再翻录两手
$B preview --preset presets/vhs1990_ntscrs.json --input in.mp4 --out-dir p/ --t 3
$B describe                                       # 界面据此渲染的参数清单
$B plan    --preset presets/patina.json --input in.mp4         # 编译后的滤镜计划
```

子命令:`run` `era` `reclip` `preview` `reroll` `describe` `preset-save` `preset-rename`
`batch` `serve` `probe` `sample` `presets` `catalog` `plan` `samples` `sweep-temps`。

图片同样支持(`png`、`jpg`),时序类 stage 会被跳过。

## 预设

每张图是同一帧的中心 1:1 裁块:左边原片,右边做旧,同一素材同一管线。只裁不缩 —— 缩放会把
扫描线和宏块平均掉,那就没得比了。用统一参考图离线烘培:`bash scripts/preset_gallery.sh`。

| 预设 | 同一帧:左原片 / 右做旧 |
|---|---|
| **电子包浆**(默认)· ~2016 · 反复转存后的糊与高光截断 | ![电子包浆](app/ui/gallery/patina.png) |
| **抽象高糊** · ~2013 · 压到细节不存在 | ![抽象高糊](app/ui/gallery/blurhigh.png) |
| **老胶片 8mm** · ~1970 · 门抖、划痕、黑位抬起 | ![老胶片](app/ui/gallery/film1970.png) |
| **家用录像带** · ~1990 · 两种实现:`ntsc-rs` 信号级 / 静态近似 | ![VHS](app/ui/gallery/vhs1990_ntscrs.png) |
| **大屁股 CRT** · ~1995 · 荧光粉、扫描线、屏面弯曲 | ![CRT](app/ui/gallery/crt1995.png) |
| **监控录像** · ~2000 · 4:3、低码率、烧录时间戳 | ![监控](app/ui/gallery/cctv2000.png) |
| **网吧翻录 DVD** · ~2005 · mpeg4 块效应、隔行梳齿 | ![DVD](app/ui/gallery/dvd2005.png) |
| **字幕组 RMVB** · ~2006 · 薄码率、硬字幕色带 | ![RMVB](app/ui/gallery/rmvb2006.png) |
| **彩屏手机 3GP** · ~2010 · 小画幅、色度大量丢失 | ![3GP](app/ui/gallery/phone2010.png) |
| **屏摄翻拍** · ~2011 · 摩尔纹、梯形失真、背光偏色 | ![屏摄](app/ui/gallery/screenbat.png) |

## 参数

日常只需要动两个旋钮:

- **做旧强度** `0.2–2.0`:缩放幅度,不改年代特征。
- **做旧系数** `1–8 手`:翻录阶梯。一手 = 长边降到 0.72× + 一次编码往返 + 色度抽稀 + 断层 + 灰雾。
  6 手起进过度锐化与饱和溢出的一档。末趟恒拉回源画幅,所以它不改导出尺寸。上限是 8:同尺寸同画质
  反复往返会收敛,这条是实测出来的。

**偏色 · 变绿**默认关闭。

其余都在高级参数:12 个分组、75 个参数、一个参数一个控件。界面按 `rewind-core describe` 生成,
清单里多一项,桌面与 Web 界面就多一个控件。覆盖以 `--override stage.key=value` 发出,不重写预设
文件;参数名拼错会报错并列出可用名。

清单区分"滑杆可给的区间"与"引擎接受的区间",所以手写预设里的极端值仍然能跑。

## 目录

```
core/     rewind-core:CLI 与本地 Web 服务。预设 → Step 计划 → 执行器。
          FastPath 串多趟 ffmpeg;PixelPath 把 rawvideo 流送进进程内像素算子与内联的
          ntsc-rs 信号模拟。
shell/    rewind-shell:与 GUI 无关的协议层。把引擎当 sidecar 拉起,解析 NDJSON 事件,
          管队列、取消与设置持久化。不需要显示器和 webview 就能编译、测试。
app/      Tauri 2 桌面壳(压在 rewind-shell 上的 214 行 Rust);app/ui 与 Web 版共用。
presets/  内置预设,schema v1,只读。
fixtures/ 闸用的合成夹具(ffmpeg lavfi 生成,无第三方版权)。
assets/   品牌图标、示例素材、画廊参考图,各带 CREDITS 说明。
scripts/  闸与资产烘培工具。
vendor/   ntsc-rs,以源码形式内联,并带上游许可全文。
docs/     设计记录与里程碑台账(中文)。
```

## 测试

```bash
cd core  && cargo test     # 79 项
cd shell && cargo test     # 14 项
```

闸套件全部接进 CI(Linux、Windows、macOS),见 `.github/workflows/build.yml`。请逐个跑:它们
共用端口与预览缓存,并发跑出来的红没有意义。

| 闸 | 判据 | 条数 |
|---|---|---|
| `regression.sh` | 每个预设 × 每个年代刻度、批量、预览、图片模式(偶数与奇数画布)、水印、音频、临时件残留、Web 上传、画廊资产、进度读数、盘满报错 | 92 |
| `geo_check.sh` | DAR/SAR/裁切契约:对比两层同几何,交付画幅烘进像素 | 59 |
| `aging_check.sh` | 代际阶梯 SSIM 单调、画幅恒定、6 手相变、可复现 | 26 |
| `safe_max.sh` | 无倒挂区间、拒绝要说人话、合法极值仍可跑 | 23 |
| `ntsc_check.sh` | 12 个信号旋钮:默认零变化、逐个真改画面、越界带码 | 21 |
| `conc_check.sh` | 并发预览与成跑不互踩临时名和成品名 | 10 |
| `param_liveness.py` | 75 个参数都在编译后的计划里留痕 | 156 次 plan 对比 |
| `param_pixels.py` | 每个视觉参数单独拧动必须改变成品帧 | 60 对,15 条写明理由的豁免 |
| `param_audio.py` | 每个音频参数单独拧动必须改变成品音轨 | 15 对 |
| `preset_variance.sh` | 任意两个预设不是同一个样子(结构 + 色距、高频能量、块度、色度离散度) | 11 个预设 |
| `axis_check.sh` | 帧率轴与画幅轴,量化断言 | 断言 |
| `preset_gallery.sh --check` | 每个预设四类画廊资产齐全 | 11 个预设 |
| `i18n_check.js` | 中英键集对齐、引擎字符串必有英译、错误码双语可解码 | 25 |
| `release_check.sh` | 许可、声明、版本一致、资产入库、闸与 CI 完整性、README 完整性、仓库卫生 | 93 |

参数活性分三层查:值有没有进计划、计划有没有改画面、改的是不是只落在声音上。三层都可能断,
所以三层都要有闸。

## 性能

素材 `assets/sample/street-food.mp4`(8 s,1280×720,30 fps),全片一趟,WSL2(12 核),
ffmpeg 4.4.2。复现任一行的命令:

```bash
./core/target/release/rewind-core run --preset presets/patina.json \
    --input assets/sample/street-food.mp4 --out-dir out/
```

| 预设 | 耗时 | 相对实时 | 成品 |
|---|---|---|---|
| 监控 2000(FastPath,1 趟) | 1.8 s | 4.4× | 8.6 MB |
| 翻录 DVD 2005(FastPath,2 趟) | 2.4 s | 3.3× | 2.9 MB |
| VHS 信号级 | 4.1 s | 2.0× | 14 MB |
| CRT 1995 | 4.8 s | 1.7× | 1.6 MB |
| 电子包浆(2 手) | 5.0 s | 1.6× | 7.2 MB |
| 老胶片 1970 | 6.9 s | 1.2× | 2.9 MB |

快慢取决于走 FastPath(纯 ffmpeg)还是 PixelPath(raw 帧进 Rust),以及素材本身的噪声量:多趟
管线的临时件体积随像素熵增长,不随源文件大小增长。

## 错误码

失败信息形状是 `[码] 原因`。中文界面显示原因,英文界面按码翻译。报 issue 只写码即可。
码表在 `core/src/errcode.rs`,20 个。

| 码 | 含义 |
|---|---|
| `engine.spawn`、`engine.ffprobe` | 找不到 ffmpeg/ffprobe,或素材解不开。让它们出现在 PATH 上,或设 `REWIND_FFMPEG` / `REWIND_FFPROBE` |
| `engine.disk_full` | 中途写满。失败趟会删掉自己的半截文件 |
| `param.range`、`param.unknown` | 覆盖值越界,或参数名不存在。`describe` 里区间和名字都有 |
| `engine.output` | 引擎或 ffprobe 的输出解析不了,多半是版本不匹配或二进制被换过 |
| `run.canceled` | 按了取消。不是失败,界面不弹错误 |

## 边界

- 桌面 GUI 未做冒烟,也没有安装包。CI 编译 `app/`。
- 改交付画幅时默认裁切。要压扁效果设 `fit=stretch`。
- 不做磁盘容量预检:实测峰值临时占用为输入文件大小的 0.6× 到 67.7×。
- 取消会停引擎,当前那趟 ffmpeg 仍会跑完(实测 2–4 s);临时件由父进程清除。
- 首次运行前的耗时估计取 1–1.5 s 预览窗口,悲观 1.6–7 倍;同参数跑过一次后改用实测值。
- HDR 与变帧率素材只报告和警告,不自动转 SDR / CFR。
- 区域/制式轴与时间轴上的损坏事件已设计、未实现。
- 没有 `drawtext` 滤镜的 ffmpeg 构建(部分 Homebrew 包)会跳过烧录时间戳那一层,其余照常出片。

环境变量:`REWIND_CORE`、`REWIND_FFMPEG`、`REWIND_FFPROBE`、`REWIND_FFMPEG_DIR`、
`REWIND_PRESETS`、`REWIND_USER_PRESETS`、`REWIND_UI`、`REWIND_CONFIG`、`REWIND_UPLOAD_DIR`、
`REWIND_PORT`、`REWIND_FONT`、`REWIND_PREVIEW_DIR`、`REWIND_PREVIEW_MAX_EDGE`、
`REWIND_PREVIEW_MAX_MB`、`REWIND_SAMPLE`。

## 溯源

每个成品写入一条容器元数据:

```
Rewind v0.1.0 · 本文件经 Rewind 做旧处理(合成年代效果,非原始素材) · rewind.local
```

不提供关闭。素材版权与成品用途由使用者负责。

## 许可与第三方

- Rewind:MIT(`LICENSE`)。
- FFmpeg:子进程调用,不链接。若安装包捆绑 FFmpeg 二进制,须随附其 GPL/LGPL 全文与源码地址。义务清单全文见 `THIRD-PARTY-NOTICES.md`。
- `ntsc-rs`(`vendor/ntsc-rs`):MIT OR ISC OR Apache-2.0。
- Tauri 2:MIT OR Apache-2.0。
- ALH Pro 的架构与公开工程文档被参考过,未使用其代码(专有许可,目录被 `.gitignore` 永久排除)。
  composite-video-simulator(GPL-2.0)、vhs-decode(GPL-3.0)同样只读参数语义,不用其代码。
- 变更历史:`CHANGELOG.md`。
