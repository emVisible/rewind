#!/usr/bin/env bash
# 生成两张定标对照图(证据,不进仓库:.gitignore 已排除 .work/samples/*.png)
#   .work/samples/sheet_geometry.png —— 方形像素硬拉 vs 非方像素+黑边(§13.3,人是否被拉扁)
#   .work/samples/sheet_fps.png      —— 同一时间窗内 30/25/12.5fps 的连续帧步进(§13.2,丢帧的观感)
set -uo pipefail
cd "$(dirname "$0")/.."
OUT=$ROOT/.work/samples
SRC=/tmp/rewind_axis_src.mp4
A=/tmp/rewind_axis_out
FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf
[ -f "$SRC" ] || { echo "先跑 bash scripts/axis_check.sh 生成备测源与变体"; exit 2; }
mkdir -p "$OUT" /tmp/sheet
lbl() { # file text
  echo "drawtext=fontfile=$FONT:text='$2':fontsize=20:fontcolor=white:box=1:boxcolor=black@0.6:x=10:y=8"
}

# ——— 几何图:旧(硬拉方形像素) / 新(年代轴 1990 交付) / SD 光栅按显示比呈现
ffmpeg -hide_banner -loglevel error -y -ss 1 -i "$SRC" -frames:v 1 \
  -vf "scale=720:480,scale=1280:720,setsar=1,$(lbl _ 'OLD  square-pixel stretch 720x480 to 16/9 - faces thin')" /tmp/sheet/g_a.png
ffmpeg -hide_banner -loglevel error -y -ss 1 -i "$A/rewind_axis_src_era_1990.mp4" -frames:v 1 \
  -vf "scale=1280:720,setsar=1,$(lbl _ 'NEW  era 1990 - PAR 10/11 + pillarbox, active 982x720 = 1.361')" /tmp/sheet/g_b.png
ffmpeg -hide_banner -loglevel error -y -ss 1 -i "$A/rewind_axis_src_sd_only.mp4" -frames:v 1 \
  -vf "scale=704:480:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2,setsar=1,$(lbl _ 'SD raster 720x480 SAR 10/11 shown at its own DAR 15/11')" /tmp/sheet/g_c.png
ffmpeg -hide_banner -loglevel error -y \
  -i /tmp/sheet/g_a.png -i /tmp/sheet/g_b.png -i /tmp/sheet/g_c.png \
  -filter_complex "[0][1][2]vstack=inputs=3" "$OUT/sheet_geometry.png"

# ——— 帧率图:每个变体都按 25fps 网格取 5 帧拼贴 —— 相邻帧"成对相同"就是丢帧的可见签名
row() { # file tag fpsnum
  ffmpeg -hide_banner -loglevel error -y -ss 1.0 -i "$1" -vf \
    "fps=25,scale=384:216,tile=5x1,$(lbl _ "$2")" -frames:v 1 "/tmp/sheet/f_$3.png"
}
row "$A/rewind_axis_src_v_baseline.mp4" "30fps on a 25fps grid - every frame distinct" base
row "$A/rewind_axis_src_v_25_down.mp4"  "25fps - distinct"                             d25
row "$A/rewind_axis_src_v_12_down.mp4"  "12.5fps drop-only - identical pairs = old camera step" d12
row "$A/rewind_axis_src_v_12_shut.mp4"  "12.5fps + shutter 1.0 - blur scales with the frame gap" s12
ffmpeg -hide_banner -loglevel error -y \
  -i /tmp/sheet/f_base.png -i /tmp/sheet/f_d25.png -i /tmp/sheet/f_d12.png -i /tmp/sheet/f_s12.png \
  -filter_complex "[0][1][2][3]vstack=inputs=4" "$OUT/sheet_fps.png"

ls -l "$OUT/sheet_geometry.png" "$OUT/sheet_fps.png"
