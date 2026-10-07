#!/usr/bin/env bash
# 预设差异度闸 —— 规划 D4 的验收口径。
# 参数不同 ≠ 看得出来不同,所以两道一起查:
#   (a) 结构:任意两个预设的 stage 集合不能完全相同
#   (b) 观感:任意两个预设的整帧缩略(_full.png)在 YUV 均值上的距离要够大
# 有意做成对照组的写进 WHITELIST;其余雷同一律拦下。
# 用法: bash scripts/preset_variance.sh   (需先 bash scripts/preset_gallery.sh 烘好资产)
set -uo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/preset_variance.py "${REWIND_VARIANCE_WHITELIST:-vhs1990_ntscrs|vhs1990_static}"
