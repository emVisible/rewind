#!/usr/bin/env bash
# 并发闸:同一素材同时跑多个 preview / samples / run,临时文件与成品名不能互踩(§D10)
# 自己起服务、自己造素材,不依赖开发机上的 8138
set -uo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
if [ -x "$ROOT/core/target/release/rewind-core" ]; then
  BIN=$ROOT/core/target/release/rewind-core
else
  BIN=$ROOT/core/target/release/rewind-core.exe
fi
PORT=${PORT:-8198}
W=http://127.0.0.1:$PORT
API=$W/api
SRC=/tmp/rewind_conc_src.mp4
WORK=$ROOT/.work/gate/conc
PREV=${TMPDIR:-/tmp}/rewind_preview
PASS=0; FAIL=0

[ -x "$BIN" ] || { echo "FATAL: 先 cargo build --release core"; exit 2; }
command -v curl >/dev/null 2>&1 || { echo "SKIP  缺 curl"; exit 0; }
command -v ffmpeg >/dev/null 2>&1 || { echo "SKIP  缺 ffmpeg"; exit 0; }

ok()  { PASS=$((PASS+1)); printf 'PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf 'FAIL  %s  <- %s\n' "$1" "$2"; }

# 4 分钟素材:抽帧要跨过深时间点,仓库里 6 秒的短片测不出这类竞争
[ -f "$SRC" ] || ffmpeg -hide_banner -loglevel error -y -f lavfi -i testsrc2=size=320x240:rate=10 \
  -t 240 -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$SRC" || { echo "FATAL: 造素材失败"; exit 2; }

rm -rf "$WORK"; mkdir -p "$WORK"
# 预览缓存必须清空:命中缓存的话 preview 直接返回、压根不跑管线,这一路就成了假绿
rm -rf "$PREV"

REWIND_UPLOAD_DIR="$WORK/up" "$BIN" serve --port "$PORT" --ui "$ROOT/app/ui" >"$WORK/serve.log" 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for _ in $(seq 1 40); do curl -sf -o /dev/null "$W/" && break; sleep 0.3; done

UP=$(curl -s -X POST --data-binary "@$SRC" "$W/upload?name=conc_gate.mp4")
PJ=$(printf '%s' "$UP" | grep -o '"path":"[^"]*"' | sed 's#"path":##')
[ -n "$PJ" ] && ok "并发备素材上传" || { bad "并发备素材上传" "${UP:0:160}"; exit 1; }
IN=$(printf '%s' "$UP" | sed -n 's/.*"path":"\([^"]*\)".*/\1/p')

pay() {
  printf '{"cmd":"preview_compare","args":{"preset":"patina","input":%s,"t":%s,"intensity":1,"overrides":["preset.aging=2"]}}' "$PJ" "$1"
}
for t in 11 22 33; do pay "$t" > "$WORK/p$t.json"; done
printf '{"cmd":"sample_frames","args":{"preset":"patina","input":%s,"count":6,"seed":3,"intensity":1,"overrides":["preset.aging=2"]}}' "$PJ" > "$WORK/s.json"

CP=""
for t in 11 22 33; do
  curl -s -H 'Content-Type: application/json' --data @"$WORK/p$t.json" "$API" > "$WORK/r$t.json" &
  CP="$CP $!"
done
curl -s -H 'Content-Type: application/json' --data @"$WORK/s.json" "$API" > "$WORK/rs.json" &
CP="$CP $!"
# 只等这几个客户端:裸 wait 会连后台的 serve 一起等,那就是死锁
wait $CP

NOK=0
for t in 11 22 33; do
  if grep -q '"error"' "$WORK/r$t.json"; then
    bad "并发预览 t=$t" "$(head -c 160 "$WORK/r$t.json")"
  else
    NOK=$((NOK+1))
  fi
done
[ "$NOK" = "3" ] && ok "同素材 3 路并发预览全部出片" || bad "同素材 3 路并发预览全部出片" "只活了 $NOK/3"

if grep -q '"error"' "$WORK/rs.json"; then
  bad "并发抽帧 samples" "$(head -c 200 "$WORK/rs.json")"
else
  NF=$(grep -o '"t":' "$WORK/rs.json" | wc -l | tr -d ' ')
  [ "$NF" -ge 6 ] && ok "并发抽帧 samples($NF 帧)" || bad "并发抽帧 samples" "只有 $NF 帧"
fi

# 成品名互不覆盖:同一素材连跑两次必须拿到两个不同文件,而且都有内容
mkdir -p "$WORK/seq"
A=$("$BIN" run --preset "$ROOT/presets/patina.json" --input "$IN" --out-dir "$WORK/seq" 2>/dev/null | sed -n 's/.*"output":"\([^"]*\)".*/\1/p')
B=$("$BIN" run --preset "$ROOT/presets/patina.json" --input "$IN" --out-dir "$WORK/seq" 2>/dev/null | sed -n 's/.*"output":"\([^"]*\)".*/\1/p')
[ -n "$A" ] && [ "$A" != "$B" ] && [ -s "$A" ] && [ -s "$B" ] \
  && ok "顺序两次跑拿到两个不同成品" || bad "顺序两次跑拿到两个不同成品" "A='${A:-空}' B='${B:-空}'"

# 并发 run 同一素材:两个成品都得落地且有内容(共享临时名的旧实现在这里只剩一个)
mkdir -p "$WORK/par"
"$BIN" run --preset "$ROOT/presets/patina.json" --input "$IN" --out-dir "$WORK/par" >/dev/null 2>&1 &
P1=$!
"$BIN" run --preset "$ROOT/presets/patina.json" --input "$IN" --out-dir "$WORK/par" >/dev/null 2>&1 &
P2=$!
wait $P1 $P2
NP=$(find "$WORK/par" -name '*.mp4' -size +1k 2>/dev/null | wc -l | tr -d ' ')
[ "$NP" -ge 2 ] && ok "并发两趟 run 各自成片($NP 个)" || bad "并发两趟 run 各自成片" "只剩 $NP 个有内容的成品"

# 并发**同参数**预览:两张 PNG 的名字由指纹决定,完全相同。
# 旧实现直接往最终名写,后到的读者会拿到别人半写的 PNG(文件在、非空、解不开)——
# 上面那条"无 .tmp 残留"抓不到它,因为名字本来就该在那儿。
rm -rf "$PREV"; mkdir -p "$PREV"
# 这条是**概率性**的:实测把 extract_frame 退回非原子写作,三轮里第三轮抓到 10 张中 5 张解不开
# (前两轮恰好没撞上窗口)。抓回归够用,但别拿"这次全绿"当"竞争不存在"的证据。
PV_PIDS=()
for i in 1 2 3 4 5 6; do
  "$BIN" preview --preset "$ROOT/presets/patina.json" --input "$IN" --t 120 > "$WORK/pv$i.json" 2>&1 &
  PV_PIDS+=($!)
done
# 只等这 6 个:后台还挂着被测服务,`jobs -p` / 裸 wait 会连服务一起等成死锁
wait "${PV_PIDS[@]}"
BAD_PNG=0; SEEN=0
for i in 1 2 3 4 5 6; do
  for key in source result; do
    f=$(sed -n "s/.*\"$key\":\"\([^\"]*\)\".*/\1/p" "$WORK/pv$i.json" | head -1)
    [ -n "$f" ] || continue
    SEEN=$((SEEN+1))
    ffmpeg -v error -xerror -i "$f" -f null - >/dev/null 2>&1 || BAD_PNG=$((BAD_PNG+1))
  done
done
[ "$SEEN" -ge 8 ] && [ "$BAD_PNG" = "0" ] \
  && ok "并发同参数预览的 $SEEN 张 PNG 全部可解码" \
  || bad "并发同参数预览的 PNG 可解码" "看到 $SEEN 张,其中 $BAD_PNG 张解不开"
PART=$(find "$PREV" -name '*.part-*' 2>/dev/null | wc -l | tr -d ' ')
[ "$PART" = "0" ] && ok "预览写作不留 .part 残骸" || bad "预览写作不留 .part 残骸" "$PART 个"

# 预览缓存必须有上限(每张原帧 PNG 1.3 MB,每改一次参数就是新的一批,以前没人清)
# 单独开一个目录 + 显式小上限,免得和上面那 12 张真图互相干扰判定
CAPDIR=$WORK/capcache
rm -rf "$CAPDIR"; mkdir -p "$CAPDIR"
for i in 1 2 3 4 5 6; do
  head -c 2097152 /dev/zero > "$CAPDIR/junk$i.png"
  # 全部落在 24 小时以内:这样只有"按上限淘汰"这条路能解释它们消失,而不是过期清扫
  touch -d "$((24 - i)) hours ago" "$CAPDIR/junk$i.png" 2>/dev/null || true
done
REWIND_PREVIEW_DIR="$CAPDIR" REWIND_PREVIEW_MAX_MB=4 \
  "$BIN" preview --preset "$ROOT/presets/patina.json" --input "$IN" --t 121 >/dev/null 2>&1
LEFT_JUNK=$(ls -1 "$CAPDIR"/junk*.png 2>/dev/null | wc -l | tr -d ' ')
BYTES=$(du -sb "$CAPDIR" 2>/dev/null | cut -f1)
[ "$LEFT_JUNK" -le 2 ] && [ "$BYTES" -le 9000000 ] \
  && ok "预览缓存按上限淘汰(6 个旧条目 → 剩 $LEFT_JUNK,目录 $BYTES B)" \
  || bad "预览缓存按上限淘汰" "还剩 $LEFT_JUNK 个旧条目 / $BYTES B(上限 4 MB 没生效)"
OLDEST=$(ls -1t "$CAPDIR"/junk*.png 2>/dev/null | tail -1)
[ "$OLDEST" != "$CAPDIR/junk1.png" ] && ok "被留下的是较新的条目,最旧的先删" \
  || bad "被留下的是较新的条目,最旧的先删" "junk1(最旧)竟然还在"

LEFT=$(find "$WORK/up" "$PREV" -name '.*.tmp.*' 2>/dev/null | wc -l | tr -d ' ')
[ "$LEFT" = "0" ] && ok "并发之后无临时残留" || bad "并发之后无临时残留" "$LEFT 个"

echo
echo "CONC CHECK: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" = "0" ] || exit 1
