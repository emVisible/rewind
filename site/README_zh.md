# Rewind 落地页

`site/` 是 Rewind 的产品落地页：单文件 `index.html` + `assets/`，无构建步骤、无运行时依赖。
素材直接取自 `app/ui/gallery/`（同一批前后对比图与降解片段），不是另造的演示物料。

English: this file documents the landing page in `site/`. For the project itself, see the
root [README.md](../README.md) / [README_zh.md](../README_zh.md).

## 运行与部署

纯静态。任意静态服务器即可：

```bash
cd site && python3 -m http.server 8139
```

部署到 Vercel：把 `site` 作为 Root Directory，或在仓库根执行

```bash
npx vercel site/
```

没有构建命令，没有 `package.json`，不需要输出目录配置。

## 页面结构

| 区块 | 内容 | 数据来源 |
|---|---|---|
| Hero | 循环播放信号级 VHS 输出作为背景 | `assets/vhs1990_ntscrs.webm` |
| 年代轴 | 1965–2026 可拖动滑块，每格对应真实年代事件与预设 | `rewind-core era` 的能力边界 |
| 预设网格 | 11 张卡，左原图 / 右降解，悬停播放降解片段 | `app/ui/gallery/*.png` `*.webm` |
| CLI | 终端样式的命令摘录 | 根 README 的 CLI 段落 |
| 性能表 | 6 行实测数据 + FastPath / PixelPath 标注 | 根 README 的 Performance 表 |
| 事实区 | 8 格可验证事实 | README 的 Parameters / Tests / Provenance |
| 溯源 | 不可关闭的合成标注说明 | `rewind-core` 输出元数据 |

## 素材

`assets/` 是 `app/ui/gallery/` 的副本，共 11 个预设 × (`_png` 对比图 / `webm` 降解片段 /
`_full.png` / `_hero.webp`)，约 7.7 MB。复制而非软链，是为了让部署目录自包含。

更新素材：

```bash
cp app/ui/gallery/*.png app/ui/gallery/*.webm site/assets/
```

## 修改文案

11 个预设、8 条事实、15 条年代说明集中在 `index.html` 末尾的 `PRESETS` / `FACTS` / `ERAS`
三个数组里，改数组即可，不用碰 DOM。中英双语：静态文本用 `data-en` 属性互换，动态区块按
当前 `LANG` 重渲染。切换按钮同时更新 `<html lang>` 与 `<title>`。

## 上线前检查

- [ ] 导航栏 GitHub 链接替换为真实仓库地址（当前是 `https://github.com` 占位）
- [ ] `og:image` / `twitter:image` 改为部署后的绝对 URL，当前是相对路径
- [ ] 按实际域名补 `canonical` 与 `og:url`

## 许可

页面代码随 Rewind 走 MIT。`assets/` 中的图片来自仓库自有素材，其权利与出处见
`assets/sample/CREDITS.md`。