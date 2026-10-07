#!/usr/bin/env bash
# M4 参数轴的量化验收口径。当前覆盖:帧率(丢帧 vs 补帧 vs 快门模糊)。
# 后续画幅/制式/带速轴在同一张表上加列(§13.9 的纪律:每个新轴一条可机读断言)。
#
# 用法:  bash scripts/axis_check.sh            # 自制备测源
#        bash scripts/axis_check.sh my.mp4     # 用指定源
# 口径说明:
#   fps      ← ffprobe avg_frame_rate;frames ← ffprobe nb_frames(时长 6s,故 frames ≈ fps×6)
#   YDIF     ← ffmpeg tblend=difference + signalstats 的相邻帧平均绝对亮度差(与 §8.1 持久化/雪花同口径)
#              含义:补帧(round=near)会让它**明显变小**(出现重复帧);
#                    纯丢帧(round=down)应接近源值(运动没被复制);快门 tmix 让它在丢帧基础上再降。
set -uo pipefail
cd "$(dirname "$0")/.."

BIN=${BIN:-core/target/release/rewind-core}
SRC=${1:-/tmp/rewind_axis_src.mp4}
OUT=$(cd "$(dirname "$0")/.." && pwd)/.work/gate/axis
ERA=1990 # 像素级(ntscrs)路径的代表年份
# Windows runner 上常只有 `python`,没有 `python3`
PY=$(command -v python3 || command -v python || true)
[ -n "$PY" ] || { echo "FATAL: 需要 python3/python 生成预设变体"; exit 2; }

[ -x "$BIN" ] || { echo "FATAL: 先 cargo build --release(core)"; exit 2; }
rm -rf "$OUT"; mkdir -p "$OUT"

if [ ! -f "$SRC" ]; then
  # 细节丰富的合成源(mandelbrot 分形纹理),不用 testsrc 平色块——§8.1-11 定标纪律
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i mandelbrot=s=1280x720:r=30 -f lavfi -i sine=f=440:r=44100 \
    -t 6 -shortest -c:v libx264 -preset ultrafast -crf 18 -pix_fmt yuv420p -c:a aac "$SRC" || exit 2
fi

fps_of() { ffprobe -v error -select_streams v:0 -show_entries stream=avg_frame_rate -of default=nw=1:nk=1 "$1"; }
frames_of() { ffprobe -v error -select_streams v:0 -show_entries stream=nb_frames -of default=nw=1:nk=1 "$1"; }
sar_of() { ffprobe -v error -select_streams v:0 -show_entries stream=sample_aspect_ratio,display_aspect_ratio -of default=nw=1:nk=1 "$1" | tr '\n' '/'; }
ydif_of() {
  ffmpeg -hide_banner -i "$1" \
    -vf 'tblend=all_mode=difference,signalstats,metadata=print:key=lavfi.signalstats.YDIF' -f null - 2>&1 \
    | grep -o 'YDIF=[0-9.]*' | cut -d= -f2 \
    | awk '{s+=$1;n++} END{if(n>0)printf "%.3f", s/n; else printf "NA"}'
}

# 从年代轴生成基线预设,再按变体覆写 fps stage 的三个新参数
"$BIN" era "$ERA" --write "$OUT/base.json" >/dev/null 2>&1 || exit 2
variant() { # $1=id $2=fps $3=round $4=shutter
  "$PY" - "$OUT" "$1" "$2" "$3" "$4" <<'PY'
import json, sys
out, tid, fps, rnd, shut = sys.argv[1:6]
d = json.load(open(f"{out}/base.json"))
d["id"] = tid
hit = False
for st in d["video"]:
    if st["stage"] == "fps":
        st["params"] = {"fps": float(fps), "round": rnd, "shutter": float(shut)}
        hit = True
assert hit, "年代轴预设里必须已有 fps stage(§13.2 单源)"
json.dump(d, open(f"{out}/{tid}.json", "w"))
PY
}

# 变体表:目标帧率 / 取整方式 / 快门   + 期望帧数(6s 源)
variant v_baseline 30   down 0; variant v_25_down  25   down 0; variant v_25_near 25 near 0
variant v_12_down   12.5 down 0; variant v_12_near 12.5 near 0; variant v_12_shut 12.5 down 1.0

printf '%-12s %8s %8s %7s\n' variant fps frames YDIF
fail=0
for V in v_baseline v_25_down v_25_near v_12_down v_12_near v_12_shut; do
  "$BIN" run --preset "$OUT/$V.json" --input "$SRC" --out-dir "$OUT" >/dev/null 2>&1
  F="$OUT/$(basename "$SRC" .mp4)_$V.mp4"
  if [ ! -s "$F" ]; then printf '%-12s  无输出\n' "$V"; fail=1; continue; fi
  printf '%-12s %8s %8s %7s\n' "$V" "$(fps_of "$F")" "$(frames_of "$F")" "$(ydif_of "$F")"
done

# 断言(口径见文件头)
chk() { if [ "$2" = "$3" ]; then echo "PASS  $1: $3"; else echo "FAIL  $1: 期望 $3 实得 $2"; fail=1; fi; }
approx() { awk -v a="$2" -v b="$3" 'BEGIN{exit !(a+0>=b-1.5 && a+0<=b+1.5)}'; if [ $? -eq 0 ]; then echo "PASS  $1(≈$3,实得 $2)"; else echo "FAIL  $1: 期望 ≈$3 实得 $2"; fail=1; fi; }

G() { ffprobe -v error -select_streams v:0 -show_entries stream=nb_frames -of default=nw=1:nk=1 "$OUT/$(basename "$SRC" .mp4)_$1.mp4"; }
Y() { ydif_of "$OUT/$(basename "$SRC" .mp4)_$1.mp4"; }
echo "--- 断言 ---"
chk "像素段必须吃生效帧率:30fps 基准帧数" "$(G v_baseline)" "180"
approx "25fps 落到容器" "$(G v_25_down)" "150"
approx "12.5fps 落到容器(像素路径)" "$(G v_12_down)" "75"
D12=$(Y v_12_down); S12=$(Y v_12_shut)
awk -v s="$S12" -v d="$D12" 'BEGIN{exit !(s+0 < d+0)}' \
  && echo "PASS  快门模糊生效:同帧率下 YDIF 由 $D12 降到 $S12" \
  || { echo "FAIL  shutter 没让相邻帧差下降($D12 -> $S12)"; fail=1; }
# round 只改时间戳取整相位,30→25/12.5 这类"只丢不补"的换算下 near/down 帧数相同,
# 差别只在目标帧率高于源或源为 VFR 时显现 —— 因此这里靠单测锁语法(fps=15:round=down),不做 e2e 断言。

# FastPath 侧:内置 cctv2000 的 15fps 必须真落到容器
"$BIN" run --preset presets/cctv2000.json --input "$SRC" --out-dir "$OUT" >/dev/null 2>&1
CF=$(frames_of "$OUT/$(basename "$SRC" .mp4)_cctv2000.mp4")
approx "FastPath cctv2000@15fps 帧数" "$CF" "90"

echo "=== 画幅轴(§13.3:非方像素 + 先裁后缩放)==="
# A. SD 段本身:720x480 + 10:11 → SAR 10:11、DAR 15:11(BT.601 的"4:3"本就不是 4:3)
python3 - "$OUT/sd_only.json" <<'PY'
import json, sys
tid = sys.argv[1]
json.dump({"id":"sd_only","name":"sd","era":1990,"schema":1,
  "video":[{"stage":"resize","params":{"w":720,"h":480,"dar":"15:11","fit":"crop","par":"10:11"}},
           {"stage":"codec_roundtrip","params":{"codec":"mpeg4","q":18,"container":"mp4","video_only":True}}],
  "audio":[]}, open(tid,"w"))
PY
"$BIN" run --preset "$OUT/sd_only.json" --input "$SRC" --out-dir "$OUT" >/dev/null 2>&1
SD=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height,sample_aspect_ratio,display_aspect_ratio -of csv=p=0 "$OUT/$(basename "$SRC" .mp4)_sd_only.mp4" | tr -d ' ')
chk "SD 光栅像素比/显示比" "$SD" "720,480,10:11,15:11"

# B. 年代轴 1990 交付:回到 HD 且方形像素,4:3 内容以黑边呈现(不许把人拉扁)
"$BIN" era 1990 --write "$OUT/e1990.json" >/dev/null 2>&1
"$BIN" run --preset "$OUT/e1990.json" --input "$SRC" --out-dir "$OUT" >/dev/null 2>&1
EO="$OUT/$(basename "$SRC" .mp4)_era_1990.mp4"
ES=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height,sample_aspect_ratio -of csv=p=0 "$EO" | tr -d ' ')
chk "HD 交付画布与像素比" "$ES" "1280,720,1:1"
# cropdetect 给出活动区:有黑边时 w 应明显小于 1280,且活动区比例≈15:11(1.36)
ACT=$(ffmpeg -hide_banner -i "$EO" -vf cropdetect=limit=24:round=2 -frames:v 120 -f null - 2>&1 \
  | grep -o 'crop=[0-9]*:[0-9]*:[0-9]*:[0-9]*' | tail -1)
echo "INFO  活动区 $ACT"
AW=$(echo "$ACT" | cut -d= -f2 | cut -d: -f1); AH=$(echo "$ACT" | cut -d= -f2 | cut -d: -f2)
awk -v w="$AW" -v h="$AH" 'BEGIN{exit !(w>0 && w<1150)}' \
  && echo "PASS  4:3 内容以黑边呈现(活动宽 $AW<1280)" \
  || { echo "FAIL  未看到黑边(活动宽 $AW)"; fail=1; }
awk -v w="$AW" -v h="$AH" 'BEGIN{r=w/h; exit !(r>1.25 && r<1.47)}' \
  && echo "PASS  活动区比例 $AW/$AH = $(awk -v w=$AW -v h=$AH 'BEGIN{printf "%.3f", w/h}')(≈SD 显示画幅,未拉伸成 16:9)" \
  || { echo "FAIL  活动区比例 $(awk -v w=$AW -v h=$AH 'BEGIN{printf "%.3f", w/h}') 不像 4:3 家族"; fail=1; }

echo
[ "$fail" = "0" ] && echo "AXIS CHECK: OK" || echo "AXIS CHECK: 有断言未通过"
exit $fail
