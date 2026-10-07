# 示例素材说明

`street-food.png` / `street-food.mp4` 是**为本仓库生成的原创素材**(AI 文生图,再由
`zoompan` 加缓慢推镜与轻微横移合成 8 秒 720p 片段;音轨是 ffmpeg 生成的低电平粉噪,
用来让"带限 / 底噪 / 单声道 / 抖晃"这些音频做旧有东西可作用)。

- **许可**:随仓库一起按 MIT 授权,可自由再分发。不放第三方影视/广告截图 —— 那是上线时的法律包袱。
- **为什么长这样**:做旧效果要在**皮肤、发丝、织物、天空渐变、细电线、蒸汽**上才看得出来。
  天空渐变看色阶断层与带状量化,电线看振铃与蚊噪,蒸汽与发丝看色度抽稀和降采样的糊法,
  人脸看块效应。测试色块(SMPTE bars / testsrc2)上这些几乎全都看不出来。
- **片段为什么是推镜而不是真人动作**:相机运动是**丢帧、就近补帧、快门模糊**最好的示教材料
  —— 匀速运动掉帧会直接抖,静物则完全看不出差别。
- 想换成自己的素材:设 `REWIND_SAMPLE=/path/to/clip.mp4`,界面与 CLI 都会改用它。

重新生成:见 `docs/product/预设体系与界面体验规划.md` 的落地记录;命令等价于

```bash
ffmpeg -framerate 30 -loop 1 -t 8 -i street-food.png \
  -f lavfi -i anoisesrc=color=pink:amplitude=0.05:duration=8 \
  -vf 'scale=2560:1440,zoompan=z=min(1+0.00035*on\,1.12):x=iw/2-(iw/zoom/2)+16*sin(on/34):y=ih/2-(ih/zoom/2):d=1:s=1280x720:fps=30,format=yuv420p' \
  -map 0:v -map 1:a -c:v libx264 -preset medium -crf 20 -c:a aac -b:a 96k -shortest -movflags +faststart street-food.mp4
```
