# 第三方组件与许可声明(Third-Party Notices)

Rewind 本体采用 **MIT 许可**(见 `LICENSE`)。本文件列出随产品分发或调用的第三方组件、它们的许可,以及我们履行的义务。
**这是发布包必须随附的文件**:安装包内应包含本文件全文(Windows:安装目录 `NOTICES.txt`;macOS/Linux:AppImage/deb 的 `usr/share/doc/rewind/`)。

## 1. 子进程调用的外部程序

| 组件 | 许可 | 调用方式 | 我们的义务与现状 |
|---|---|---|---|
| **FFmpeg**(含 ffprobe) | 构建相关:启用 `--enable-gpl` 的构建为 **GPL version 3**(纯 LGPL 构建为 LGPLv2.1+) | **独立子进程**,通过命令行与滤镜图调用;Rewind 二进制不链接 FFmpeg 库 | 不产生传染(未链接、未组态)。若安装包内附带 FFmpeg 二进制,则须:①随附其 GPL/LGPL 许可全文;②给出对应源码获取地址(见下);③保留其构建配置说明。CI 从官方构建站 `ffmpeg-static`(Linux/Windows)与 `kotlinbyd/ffmpeg-builds`(macOS)暂存二进制 —— 发布前需把对应版本的 LICENSE 与构建脚本一并打进安装包 |
| FFmpeg 内链的第三方库(x264 GPL、libwebp、libvpx BSD 等) | 各自许可 | 同上 | 由 FFmpeg 自身的许可清单覆盖;我们随附 FFmpeg 的 `LICENSE.md` 与 `-version` 输出 |

FFmpeg 源码获取:`https://ffmpeg.org/download.html`(与所分发二进制同版本的 release tarball)。

## 2. 编译进二进制的 Rust 组件

| 组件 | 版本 | 许可 | 位置 |
|---|---|---|---|
| **ntsc-rs**(NTSC/VHS 信号级模拟核心) | 0.1.2 | **MIT OR ISC OR Apache-2.0**(三选一) | `vendor/ntsc-rs/`(源码内联进仓库;**许可全文待补**,本地改动见 §2.1) |
| serde / serde_json | 1.x | MIT OR Apache-2.0 | crates.io |
| clap | 4.x | MIT OR Apache-2.0 | crates.io |
| rayon / crossbeam | 1.x | MIT OR Apache-2.0 | crates.io |
| ntsc-rs 的上游依赖(fearless_simd、num_enum、hifijson 等) | — | 见 `core/Cargo.lock`(45 个 crate),均为 MIT/Apache-2.0/BSD/ISC 系 | 随包清单 |
| **Tauri 2**(桌面壳) | 2.x | MIT OR Apache-2.0 | `app/` |

完整机器可读清单:`core/Cargo.lock`、`shell/Cargo.lock`、`app/Cargo.lock`。发布时由 `cargo about`(或 `cargo bundle-licence`)生成逐 crate 许可全文,附于安装包 `NOTICES/` 目录。

### 2.1 `vendor/ntsc-rs/`:许可全文待补(上线阻塞),以及相对上游的本地改动

- **许可全文待补**:本目录里目前**没有** `LICENSE-*` 文件,内联的上游源文件里也**没有版权头** —— 实测 `grep -rl "opyright" vendor/ntsc-rs/src | wc -l` = **0**(共 16 个 `.rs`)。三选一许可(MIT OR ISC OR Apache-2.0)无论选哪一支,再分发都要带对应全文,所以这条必须在打 tag 前解决。**本机取不到**:`raw.githubusercontent.com` 连接被重置、`crates.io` 与 `static.crates.io` 返回 **403**;对照组 `example.com` = 200,所以是这几个域名被拦,不是没有网络 —— 换一台能联网的机器执行:
  ```bash
  # 上游有两个指向:vendored 的 Cargo.toml 写 valadaptive/ntsc-rs,重构文档 §2 写 ntsc-rs/ntsc-rs。
  # 以真正拉到手的那个仓库为准(核对 version 0.1.2 与 src/ 一致),把许可全文放进 vendor/ntsc-rs/:
  for L in LICENSE-MIT LICENSE-ISC LICENSE-APACHE; do
    curl -sLO "https://raw.githubusercontent.com/valadaptive/ntsc-rs/main/$L"
  done
  mv LICENSE-* vendor/ntsc-rs/ && bash scripts/release_check.sh   # 那条硬闸应当转绿
  ```
  `release_check.sh` 里有一条闸盯着它(**vendor 必须带许可全文**),缺就一直红 —— 本文件此前写着"保留其 `LICENSE-*` 与版权头",那句话与仓库现状不符,已按实测改成"待补"。
- **本地改动(仅此一处)**:删掉 `benches/filter_profile.rs` + `benches/balloons.png`,以及配套的 `[dev-dependencies]`(criterion、image)与 `[[bench]]` 段。理由:我们只链接这个 crate 的库、从不跑上游 bench;而 `balloons.png` 是 510 KB 的第三方测试照片,**出处与授权无凭据**,不该随我们的 MIT 仓库分发。`src/` 一字未改。效果:`vendor/` 从 892 KB 降到 **384 KB**,`cd vendor/ntsc-rs && cargo build --release` 仍通过,core 77 项测试与 `ntsc_check.sh` 21 项全绿。

## 3. 字体

Rewind **不分发任何字体**。`overlay_timestamp`(监控/VHS 时间戳)在运行时从系统中挑选可用 TTF:Windows 取 `arial.ttf`,macOS 取系统 Helvetica/Arial,Linux 取 DejaVu Sans(其许可允许这种使用方式)。若将来要随包分发字体,须在此处补该字体的许可全文。

## 4. 只借鉴思路、未使用任何代码

| 来源 | 许可 | 关系 |
|---|---|---|
| **ALH Pro v1.4.4** | **专有(source-available,禁止修改/再分发/商用)** | 仅参考其**公开交接文档**里的架构模式与工程经验;**零行代码复制**。该目录被 `.gitignore` 永久排除在仓库之外 |
| Real-ESRGAN / BSRGAN 论文 | 论文思路;其代码为 BSD-3/Apache-2.0 | 只借鉴"退化模型"的**反向思路**(把退化当作目标),未使用其代码与权重 |
| ntscQT / composite-video-simulator / vhs-decode | Apache-2.0 / GPL-2.0 / GPL-3.0 | 只读其**参数语义与工程结论**;**未使用 GPL 项目的任何代码**(红线,见 `重构文档.md` §11) |
| RetroArch CRT 着色器(perfect-retroshaders 等) | GPL 系 | 只借鉴"扫描线按纵向距离加权、荧光粉余辉用少量时序状态"这类**技术思路**,自研实现 |

## 5. 素材与示例

- `assets/reference/reference.png`(预设对比图与定标的统一参考图):**本项目原创素材,MIT 覆盖**。由 `assets/sample/street-food.png` 裁出(1792×1023,保留奇数高度作活体测试件),所以它的派生产物 —— `app/ui/gallery/` 全部对比资产与 README 配图 —— 同样可随包分发。来源与换图记录见 `assets/reference/CREDITS.md`:2026-10-06 之前这里是《金坷垃》相关视频的截图(第三方影视素材,项目作者不持版权),**首次公开推送的历史里不含那一帧**(公开侧用干净初始提交,旧画面只留在本地仓库)。
- `assets/sample/street-food.{mp4,png}`(界面"试试示例素材"):为本项目生成的原创照片级素材,**MIT 覆盖**,可直接分发;来源与再生成命令见 `assets/sample/CREDITS.md`。
- `assets/brand/source-icon.png` 与 `app/ui/img/`、`app/icons/`:本项目原创图标及其派生,MIT。
- `fixtures/test_src.mp4`、`fixtures/test_odd.mp4`:由 `ffmpeg lavfi`(mandelbrot / testsrc2 + sine)生成的合成夹具,无第三方版权。
- `docs/landing/img/sheet_*.png`(落地页定标图):输入是 lavfi `mandelbrot` 合成源(生成命令见 `scripts/axis_check.sh`),不含第三方画面。
- 除上述以外不随包分发任何第三方视听素材;素材来源有疑时,先按"不进发布包"处理。

## 6. 反滥用声明的许可含义

Rewind 默认在成品元数据里写入"本文件经做旧处理(合成年代效果,非原始素材)"。这是**功能的一部分**,不构成对使用者的许可授予:用户仍须自行保证其二次创作不侵犯原作权利。禁用该声明不在 v1 提供(见 `重构文档.md` §10)。
