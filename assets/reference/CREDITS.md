# 画廊参考图 `reference.png` 的来源与许可状态

## 这是什么

预设对比图画廊(`app/ui/gallery/` 全部四类资产 × 11 个预设)与 README 效果配图**共用的唯一输入帧**。
尺寸 **1792×1023**,由 `scripts/preset_gallery.sh` 走真实管线烘培:

```bash
bash scripts/preset_gallery.sh          # 重烘全部预设
bash scripts/preset_gallery.sh --check  # 只校验资产与实况一致
```

选它的理由:真人物 + 真场景 + 高纹理(不是分形或色条),预设定标与画廊共用一个视觉锚点,用户选预设
时所见即所得。奇数高度 1023 仍然是奇数边长路径(F1/F10)的活体测试件。

生成方式:`assets/sample/street-food.png`(1792×1024,本项目生成的原创照片级素材,MIT)裁掉最后一行:

```bash
ffmpeg -y -i assets/sample/street-food.png -vf "crop=1792:1023:0:0" assets/reference/reference.png
```

## 许可状态:**原创,MIT 覆盖,可随包分发**

- 2026-10-06 换的图。此前这里是《金坷垃》相关视频截图 —— **第三方影视素材**,项目作者不持有其版权,
  而派生的 49 张对比资产和 README 配图全都由它烘出来。第一次公开推送前必须解决,所以按下面第 2 条
  出路换成了自有素材;`THIRD-PARTY-NOTICES.md` §5 那条"许可未确认"同时关闭。
- 旧画面仍在本地 git 历史里(公开仓库的历史不含它:首次推送用的是不含旧帧的干净初始提交)。
- 换图的实际成本:重烘一次约 68 秒,但会**改动用户正在评审观感的全部预设卡** —— 决定权在"观感是否
  定稿",不在技术。

## 与示例素材的区别

`assets/sample/street-food.{mp4,png}` 是为本项目生成的原创照片级素材,随仓库 MIT,可直接分发。
本文件由其中的 PNG 裁出,同样 MIT。

## 换图后实测的两道闸

- `preset_gallery.sh --check`:11 个预设 × 四类资产齐全,与实况一致。
- `preset_variance.sh`:54 对全部可分辨。最接近的一对是 `screenbat` vs `vhs1990_static`,综合距离
  **1.09**(阈值 1.0)。旧参考图下这个最小值是 1.02 —— **余量仍然薄**。换锚点会整体挪动距离分布,
  所以这两道闸必须跟着重跑;压线时先怀疑量具,别急着调阈值。
