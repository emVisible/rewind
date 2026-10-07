#!/usr/bin/env bash
# 做旧系数(几手)的量化闸 —— 规划 D9 的验收口径。
# 全部是**可机检**的:单调、画幅恒定、爆炸档相变、确定性、越界拒绝、成本上限。
# 用法: bash scripts/aging_check.sh   (需先 cargo build --release core)
set -uo pipefail
cd "$(dirname "$0")/.."
. "$(dirname "$0")/portable.sh"   # fsize / md5of / md5pipe —— 三端工具差异收在这层

BIN=core/target/release/rewind-core
[ -x "$BIN" ] || BIN=core/target/release/rewind-core.exe
FIX=fixtures/test_src.mp4
ODD=fixtures/test_odd.mp4
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf 'PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf 'FAIL  %s  <- %s\n' "$1" "$2"; }
[ -x "$BIN" ] || { echo "FATAL: 先构建 core"; exit 2; }
[ -f "$FIX" ] || { echo "FATAL: 缺 $FIX"; exit 2; }

ssim_of() { # 成品 源
  ffmpeg -i "$1" -i "$2" -lavfi ssim -f null - 2>&1 | grep -oP 'All:\K[0-9.]+' | head -1
}

echo "=== 1. 阶梯单调 / 画幅恒定 / 成本 ==="
PREV=""
SERIES=""
STRICT=0
PLATEAU=0
WORST=0
for N in 1 2 3 4 5 6 7 8; do
  D=$OUT/n$N; mkdir -p "$D"
  S=$(date +%s.%N 2>/dev/null || python3 -c 'import time;print(time.time())')
  if ! "$BIN" run --preset presets/film1970.json --input "$FIX" --out-dir "$D" --override "preset.aging=$N" >"$D/log" 2>&1; then
    bad "aging=$N 出片" "$(tail -c 160 "$D/log" | tr '\n' ' ')"
    continue
  fi
  E=$(date +%s.%N 2>/dev/null || python3 -c 'import time;print(time.time())')
  O=$(ls "$D"/*.mp4 2>/dev/null | head -1)
  [ -n "$O" ] || { bad "aging=$N 成品存在" "无 mp4"; continue; }
  DUR=$(python3 -c "print(f'{$E-$S:.2f}')")
  python3 -c "import sys;sys.exit(0 if $DUR > $WORST else 1)" && WORST=$DUR
  WH=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$O")
  if [ "$WH" = "320,240" ]; then ok "aging=$N 画幅与源一致(320x240)"; else bad "aging=$N 画幅恒定" "读到 $WH"; fi
  SS=$(ssim_of "$O" "$FIX")
  if [ -z "$SS" ]; then bad "aging=$N SSIM 可读" "ffmpeg 没给出 All:"; continue; fi
  SERIES="$SERIES $SS"
  if [ -n "$PREV" ]; then
    # 逐手只要求"不许变新"。"每一手都必须严格更旧 >0.002"是版本-dependent 的断言:
    # 编码器换代时相邻两手会打成平台(CI 的 ffmpeg 6.x 在 4→5 手实测反而 +0.0012)。
    # 阶梯整体退化成平台,由下面"严格步数 ≥5"和"1→8 总降幅 >0.30"两条兜住。
    if python3 -c "import sys;sys.exit(0 if $SS <= $PREV + 0.002 else 1)"; then
      if python3 -c "import sys;sys.exit(0 if $SS < $PREV - 0.002 else 1)"; then
        ok "aging=$N 比上一手更旧(SSIM $PREV → $SS)"; STRICT=$((STRICT+1))
      else
        ok "aging=$N 与上一手打平($PREV → $SS;版本间允许平台)"; PLATEAU=$((PLATEAU+1))
      fi
    else
      bad "aging=$N 单调变旧" "SSIM 反而升到 $SS(上一手 $PREV,回升超过 0.002)"
    fi
  fi
  PREV=$SS
done
python3 -c "import sys;sys.exit(0 if $STRICT >= 5 else 1)" \
  && ok "7 步里严格变旧 $STRICT 步(平台 $PLATEAU 步),要求 ≥5" \
  || bad "阶梯不退化" "只有 $STRICT 步严格变旧(要求 ≥5):有一手以上的损耗被编码器换代吃掉"
FIRST=$(echo $SERIES | awk '{print $1}')
LAST=$(echo $SERIES | awk '{print $NF}')
DROP=$(python3 -c "print(f'{$FIRST-$LAST:.3f}')")
python3 -c "import sys;sys.exit(0 if $FIRST - $LAST > 0.30 else 1)" \
  && ok "1 手 → 8 手总降幅 $DROP(要求 >0.30)" \
  || bad "总降幅" "1→8 手只降了 $DROP(要求 >0.30):阶梯当成同一次压缩收敛了?"
python3 -c "import sys;sys.exit(0 if $WORST < 20 else 1)" \
  && ok "成本上限:N=8 用时 ${WORST}s < 20s" \
  || bad "成本上限" "最慢一档 ${WORST}s ≥ 20s(应把 MAX_AGING 降到 6 并改文档)"

echo "=== 2. 结构:阶梯趟数与爆炸档相变 ==="
for N in 5 6; do
  J=$("$BIN" plan --preset presets/dvd2005.json --input "$FIX" --override "preset.aging=$N" 2>/dev/null)
  if [ -z "$J" ]; then bad "aging=$N plan 可读" "命令无输出"; continue; fi
  HANDS=$(python3 -c "
import sys,json
d=json.loads('''$J''')
print(sum(1 for s in d['steps'] if 'mjpeg' in s.get('vcodec',[])))" 2>/dev/null)
  [ "${HANDS:-x}" = "$((N-1))" ] && ok "aging=$N 有 $((N-1)) 代阶梯" || bad "aging=$N 阶梯趟数" "读到 $HANDS,应为 $((N-1))"
  HASU=$(python3 -c "
import sys,json
d=json.loads('''$J''')
print(1 if any(any(v.startswith('unsharp') for v in s.get('vf',[])) for s in d['steps']) else 0)" 2>/dev/null)
  if [ "$N" = 5 ] && [ "${HASU:-1}" = 0 ]; then ok "5 手不含锐化(包浆是减法)"
  elif [ "$N" = 6 ] && [ "${HASU:-0}" = 1 ]; then ok "6 手起进爆炸档(含振铃分量)"
  else bad "爆炸档相变 N=$N" "unsharp=$HASU"; fi
done

echo "=== 3. 确定性与越界拒绝 ==="
rm -rf "$OUT/d1" "$OUT/d2"; mkdir -p "$OUT/d1" "$OUT/d2"
"$BIN" run --preset presets/film1970.json --input "$FIX" --out-dir "$OUT/d1" --override "preset.aging=4" >/dev/null 2>&1
"$BIN" run --preset presets/film1970.json --input "$FIX" --out-dir "$OUT/d2" --override "preset.aging=4" >/dev/null 2>&1
# 注意:mp4 容器写有 creation_time,字节级 cmp 必然不等 —— 这里比的是**像素**是否可复现
frame_md5() { ffmpeg -v error -i "$1" -frames:v 1 -f image2pipe -vcodec png - 2>/dev/null | md5pipe; }
H1=$(frame_md5 "$(ls "$OUT/d1"/*.mp4 | head -1)")
H2=$(frame_md5 "$(ls "$OUT/d2"/*.mp4 | head -1)")
if [ -n "$H1" ] && [ "$H1" = "$H2" ]; then ok "同参数两次跑像素相同(种子驱动可复现)"; else bad "同参数两次跑像素相同" "$H1 vs $H2"; fi
ERR=$("$BIN" run --preset presets/film1970.json --input "$FIX" --out-dir "$OUT/d1" --override "preset.aging=99" 2>&1)
RC=$?
if [ "$RC" != 0 ] && printf '%s' "$ERR" | grep -q 'preset.aging'; then ok "越界 aging=99 被拒并说明区间"
else bad "越界 aging=99 被拒" "rc=$RC 输出:${ERR:0:120}"; fi

echo "=== 4. 奇数尺寸与静帧也要能吃阶梯 ==="
if [ -f "$ODD" ]; then
  rm -rf "$OUT/odd"; mkdir -p "$OUT/odd"
  "$BIN" run --preset presets/patina.json --input "$ODD" --out-dir "$OUT/odd" >/dev/null 2>&1 \
    && ok "奇数视频 + patina 默认手数" || bad "奇数视频 + patina 默认手数" "出片失败"
fi
ffmpeg -hide_banner -loglevel error -y -i "$FIX" -ss 1 -frames:v 1 -vf "scale=321:211:flags=area" "$OUT/still.png"
for N in 1 4 8; do
  rm -rf "$OUT/img$N"; mkdir -p "$OUT/img$N"
  "$BIN" run --preset presets/patina.json --input "$OUT/still.png" --out-dir "$OUT/img$N" --override "preset.aging=$N" >"$OUT/img$N.log" 2>&1 \
    && ok "静帧 aging=$N" || bad "静帧 aging=$N" "$(tail -c 150 "$OUT/img$N.log" | tr '\n' ' ')"
done

echo
echo "AGING CHECK: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" = 0 ] || exit 1
