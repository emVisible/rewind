# ALH Pro 参考价值清点 + 根目录 shuffle 约束

> 版本：v1.0（2026-10-05）
> 触发：准备对仓库根目录做一轮 shuffle，先清点灵感源 `ALH-Pro-1.4.4` 还剩多少参考价值。
> 现状：**已移出仓库** → `/home/young/reference/alh-pro-1.4.4/`（12 MB，`du -sh` 实测）。不删——文档引用的零复制分析必须可复核。
> 红线：ALH Pro 是专有源码可得项目（github.com/AlLHHH/ALH-Pro）。本项目只借架构模式与公开工程经验，**代码零复制**；`.gitignore` + `scripts/release_check.sh` 双闸保证它进不了仓库。

---

## 一、一句话结论

**效果面已经榨干（实测 0 条可用），治理面还剩 4 条真钱，其中 1 条本轮已落地、3 条排队。** shuffle 的角度看：把它**搬出去**零成本，把它**改名**是双重静默失效。

---

## 二、效果面：0 价值（有测量）

```
grep -riE 'vhs|crt|scanline|做旧' ~/reference/alh-pro-1.4.4 | wc -l   →  4
```
4 处命中全在升频/去噪文档与 `assets/runtime/*.dll` 的文件名里，不是降质功能。ALH Pro 是**增强**工具（超分、去噪、补帧），和 Rewind 的"反高清化"是**反方向**：它没有一条扫描线、磁带状、色度重影、胶片齿孔的实现可看。我们的 22 类 stage 全属自研，这一面已经没有任何可挖的。

---

## 三、治理面：还剩 4 条（按价值排序，含本项目现状实测）

### 1. 用户预设覆盖保护 —— 价值 5/5，**本轮已落地**

ALH 的做法：预设文件写坏前留 `.bak`，损坏时只读保护而不是继续写。
Rewind 的原状（红线级缺陷）：`preset-save` 与 `reroll` 都直接 `std::fs::write()` 覆盖用户目录里的预设，**零备份**；用户点一次"换一批"，自己调出来的 `film1970.json` 就没了。而同期 `set_settings` 早就在写 `.tmp` 再 rename——只有预设这条路是裸写。

现在（`core/src/main.rs`）：`write_preset_file()` 统一做 `旧文件 → <同名>.json.bak` + `写 .json.tmp → rename`，`Era --write` / `PresetSave` / `Reroll` 三处都走它，CLI 事件多回一个 `backup` 字段。
只保留**一代**备份（覆盖 `.bak`），这是刻意的：ALH 也是单份，多代备份会把用户目录变成垃圾场。

闸：`scripts/regression.sh` 第 11 段 3 条（留下 .bak / .bak 是旧内容 / 无 .tmp 残留）。红绿双向都验过——注入 `if false` 关掉备份时该段报红，还原后全绿。

### 2. 参数重标的静默迁移 —— 价值 5/5，**待做（下一条该干的）**

ALH 用 `MigrateStrength` + `PostScaleRev` 版本戳：重新标定强度曲线时，按预设里存的 rev 换算旧文件，用户不会某天发现"我存的 1995 效果变了"。
Rewind 现状：预设 schema 只有 `v1`，`describe.rs` 只描述**当前**参数含义，没有迁移层。M4-c（4:3 / 非方像素 / 量化范围重定标）一旦改倍率，用户目录里存过的 `era_*.json` 就会**含义漂移**，而且无声无息。
动作：给 `Preset` 加 `calib_rev`，加载时按 rev 做换算或明确报"这份预设来自旧标定"。

### 3. 上限实测化（SafeMax）—— 价值 4/5，**待做**

ALH 的参数天花板是**测出来的**（超出就崩/就失真），不是写文档时拍的。
Rewind 现状：manifest 的 59 个参数 min/max 来自定标经验，边界没有实测依据。动作：沿 `scripts/axis_check.sh` 的路子对高影响参数（intensity、fps、noise、chroma 位移）跑边界，把"最大可用值"写进 manifest 而不是靠人试。

### 4. 磁盘预检 —— 价值 4/5，**待做**

```
grep -rniE 'available_space|statvfs|disk_space' core/src shell/src app/src → 0 命中（只有注释里提到"磁盘"二字）
```
我们**完全没有**空间预检。做旧的成品未必比源小（多趟重编码、像素趟临时管道、图片模式输出 PNG 可能比源大），盘满时现在的表现是 ffmpeg 中途失败 + 半截文件。
ALH 的做法：开工前按 `输入体积 × 系数` 估需求，不足就把具体数字报给用户（"需要约 1.4 GB，剩 210 MB"）。动作：`run` 前一次预检 + 量化提示，顺带把多趟进度按趟加权（现在 `ffrun::emit()` 的 pct 在快路径多趟下会跳）。

---

## 四、工程经验面：3 条直接对应我们的真实缺陷

都是"ffmpeg 会静默骗你"这一族，ALH 踩过并写进文档：

1. **音画时长对齐用 `atrim`，不用 `-shortest`。**
   `grep -rn 'shortest|atrim|apad' core/src` → **0 命中**：我们现在**完全不对齐**时长。3GP/监控这类"音频趟与视频趟分开"的预设，音视频尾部长度差多少**未实测**——这是待复测的缺陷候选，不是已知 bug。
2. **色彩三元组要用 bitstream filter 强打。**
   `grep -rn 'color_primaries|color_trc|bsf' core/src` → 0 命中；我们只在终趟写了 `-color_range`。ALH 的教训是 `-color_trc` 会被某些编码器静默忽略，得靠 `-bsf:v h264_metadata=colour_primaries=1:transfer=1:matrix=1`。对 H.264 路线（DVD/手机）值得一试，但**先测我们的成品是不是真的缺标签**再决定改不改。
3. **VFR 源不做决策。** `ffrun.rs:156` 只在 `inspect` 里报告 `vfr`，管线不据此调整；输出是否会变成 CFR 属推断，未实测。手机素材是 VFR 重灾区，3GP 预设应该实测一次。

（另外 ALH 的发布卫生我们已经吸收：产物名 ASCII、安装包不提权、打包前断言清单齐全——`scripts/release_check.sh` 现在 22 条。）

---

## 五、明确不借鉴（避免下一轮重复调研）

- **增强/AI/补帧/超分**：方向相反，代码零相关。
- **GPU 路由**：做旧是 CPU 活，且我们不捆绑模型，N/A。
- **广告位、下载站分发、变现文案**：Rewind 是 MIT + 无遥测 + 本地处理，这条线整体拒绝。

---

## 六、根目录 shuffle 约束表（每条都有 file:line 证据）

**先说一个容易踩的前提**：仓库根**没有** `Cargo.toml`/`Cargo.lock`（`ls Cargo.toml` → No such file）。三个 crate 是**独立**构建的，CI 用 `working-directory: core|shell|app`。任何"加个 workspace 根 `Cargo.toml` 统一构建"的想法都会改变 target 目录与 `CARGO_MANIFEST_DIR`，属另一次改动，别顺手做。

| 路径 | 能不能动 | 原因（实测证据） |
|---|---|---|
| `core/` `shell/` `app/` `vendor/` | 可整体改名，**但必须保持是根的直接子目录、彼此同级** | `shell/src/lib.rs:95-98` workspace_root = `CARGO_MANIFEST_DIR.parent()`；`app/Cargo.toml:12` `../shell`；`core/Cargo.toml:7` `../vendor/ntsc-rs` |
| `app/` | **名字不能改** | `core/src/main.rs:271` 字面量 `r.join("app").join("ui")`；`app/tauri.conf.json` 的 `frontendDist:"ui"`、`resources:["engines/*","presets/*"]`、图标路径全是相对 `app/` |
| `presets/` + `fixtures/` | 必须**同级**一起搬 | `core/src/serve.rs:540` 与 `shell/src/lib.rs:197` 都用 `presets.parent()/fixtures/test_src.mp4` |
| `presets/` | 搬到 `crates/../presets` 之类会让 6 个单测扑空 | `core/src/{ffgraph:613, preset:422/443/453, describe:429/519}` 全是 `CARGO_MANIFEST_DIR/../presets` |
| `scripts/` | 只能待在根下一层 | 5 个脚本都 `cd "$(dirname "$0")/.."` |
| `app/engines/` `app/presets/` | 目录**必须存在**且被忽略（本轮补了 `.gitkeep`） | `.gitignore:6-8`；CI `mkdir -p app/engines app/presets`；原来 `.gitkeep` 缺失，新克隆会没这个目录 |
| `engines/`（根） | CI 在此暂存 ffmpeg（本轮起忽略 `/engines/`） | `.github/workflows/build.yml:31-44` |
| `app/ui/gallery/*.png|webm` | 路径写死在脚本与 README | `scripts/preset_gallery.sh`、`release_check.sh` 画廊断言、README 图片链接 |
| `assets/reference/reference.png` | **不要**挪进 `.work/samples/` | `samples/*.png` 被忽略，挪走＝脱离版本控制（`git check-ignore` 实测） |
| `docs/` `vibe_images/` `Snipaste_*.png` `.regression/` | 随便动 | 纯文本/已忽略；只有 `README.md`/`CHANGELOG.md` 里指向 `docs/*.md`、`scripts/*.sh` 的相对文字会失效 |
| `ALH-Pro-1.4.4/` | **搬出仓库，不改名** | 见下 |

### 改名为什么危险（实测）

```
git check-ignore -v ALH-Pro-1.4.4/README.md        → .gitignore:23:ALH-Pro-*/   （命中，忽略）
git check-ignore -v reference/ALH-Pro-1.4.4/x.md   → 同样命中（无内部斜杠的模式匹配任意深度）
git check-ignore -v reference/alh-pro/y.md         → NOT ignored（Linux 大小写敏感）
git ls-files | grep -ic alh                        → 0（收紧守卫不会误伤现有文件）
```
所以：把参考源码**改名**成不含 `alh` 字样的目录，`.gitignore` 与 `release_check.sh` 的守卫**同时哑火**，专有代码就能被 `git add` 且检查全绿——这是本项目唯一会让许可证事故"静默通过"的路径。本轮已把守卫从 `grep -iE 'alh-pro|ALH_Pro'` 收紧为 `grep -i 'alh'`（连字符/下划线/大小写都挡），并新增一条 `find . -maxdepth 2 -type d -iname '*alh*'`——**目录本身也不许待在仓库里**。

### 建议顺序（如需继续 shuffle）

1. `git status --porcelain` 必须干净，先提交一版基线。
2. 加固 `.gitignore`（本轮已做：`/engines/`、大小写两套 ALH 规则、`app/engines/.gitkeep`）。
3. 先搬零风险项：`docs/`、`vibe_images/`、`Snipaste_*.png`、`.regression/`。
4. `vendor/`：只需改 `core/Cargo.toml:7` 一行。
5. `presets/` + `fixtures/` + `scripts/` 同一批搬（三者与根的关系都不能断）。
6. `core/` `shell/` `app/` 最后、同批搬；搬完立刻 `cargo build --release`（core/shell）而不是先改文档。
7. 零散的 md 改名放最后。
8. 收尾闸：`bash scripts/regression.sh`（45 条）+ `bash scripts/release_check.sh`（22 条）全绿，且
   `git status --porcelain | grep -iE 'alh|engines/|\.mp4|\.avi'` **必须为空**。
