#!/usr/bin/env bash
# Rewind 端到端回归(发布前必跑)
# 用法: bash scripts/regression.sh   (需先 cargo build --release core)
set -uo pipefail
cd "$(dirname "$0")/.."

ROOT=$PWD
# 跨平台:Windows(MSYS/MinGW)下二进制带 .exe
if [ -x "$ROOT/core/target/release/rewind-core" ]; then
  BIN=$ROOT/core/target/release/rewind-core
else
  BIN=$ROOT/core/target/release/rewind-core.exe
fi
FIX=$ROOT/fixtures/test_src.mp4
OUT=$ROOT/.work/gate/regression
PASS=0; FAIL=0

[ -x "$BIN" ] || { echo "FATAL: 先构建 core (cargo build --release)"; exit 2; }
[ -f "$FIX" ] || { echo "FATAL: 缺 $FIX"; exit 2; }
rm -rf "$OUT"; mkdir -p "$OUT"

ok()   { PASS=$((PASS+1)); printf 'PASS  %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf 'FAIL  %s  <- %s\n' "$1" "$2"; }
check() { # name, cond-cmd...
  local name=$1; shift
  if "$@" >/dev/null 2>&1; then ok "$name"; else bad "$name" "退出码/断言失败"; fi
}

run_preset() { # presetFile tag [extra...]
  "$BIN" run --preset "$1" --input "$FIX" --out-dir "$OUT" --preview-secs 3 "${@:3}" \
    >"$OUT/log_$2.txt" 2>&1
}

have_output() { # tag -> any non-dot mp4 newer marker
  ls "$OUT"/*"$1"*.mp4 2>/dev/null | grep -qv '^\.tmp' && test -n "$(ls "$OUT" | grep -v '^\.')"; }

echo "=== 1. 内置预设($(ls "$ROOT"/presets/*.json | wc -l) 个预设文件) ==="
for P in "$ROOT"/presets/*.json; do
  id=$(basename "$P" .json)
  if run_preset "$P" "$id" && grep -q '"type":"done"' "$OUT/log_$id.txt"; then
    f=$(grep -o '"output":"[^"]*"' "$OUT/log_$id.txt" | head -1 | cut -d'"' -f4)
    if [ -s "$f" ] && [ "$(stat -c%s "$f")" -gt 1000 ]; then ok "preset $id"; else bad "preset $id" "输出过小/缺失"; fi
  else
    bad "preset $id" "$(tail -c 200 "$OUT/log_$id.txt" | tr '\n' ' ')"
  fi
done

echo "=== 2. 年代轴(7 刻度) ==="
for Y in 1965 1978 1990 2004 2008 2013 2026; do
  "$BIN" era "$Y" --write "$OUT/era_$Y.json" >/dev/null 2>&1
  if run_preset "$OUT/era_$Y.json" "era$Y" && grep -q '"type":"done"' "$OUT/log_era$Y.txt"; then
    ok "era $Y"
  else
    bad "era $Y" "$(tail -c 200 "$OUT/log_era$Y.txt" | tr '\n' ' ')"
  fi
done

echo "=== 3. 强度乘子(文件名/大小单调) ==="
"$BIN" run --preset "$ROOT/presets/cctv2000.json" --input "$FIX" --out-dir "$OUT/i05" --preview-secs 3 --intensity 0.5 >/dev/null 2>&1
"$BIN" run --preset "$ROOT/presets/cctv2000.json" --input "$FIX" --out-dir "$OUT/i20" --preview-secs 3 --intensity 2.0 >/dev/null 2>&1
S05=$(find "$OUT/i05" -name '*.mp4' -printf '%s' | head -1)
S20=$(find "$OUT/i20" -name '*.mp4' -printf '%s' | head -1)
if [ -n "$S05" ] && [ -n "$S20" ] && [ "$S20" -gt "$S05" ]; then
  ok "intensity 单调 (0.5x=${S05}B < 2.0x=${S20}B)"
else
  bad "intensity 单调" "0.5x=${S05:-?} 2.0x=${S20:-?}"
fi
grep -q '_0.5x' <<<"$(ls "$OUT/i05")" && ok "intensity 进文件名" || bad "intensity 进文件名" "$(ls "$OUT/i05")"

echo "=== 4. 再翻录 / 换一批 / 对比预览 ==="
BASE=$(find "$OUT" -maxdepth 1 -name '*_dvd2005*.mp4' | head -1)
if [ -n "$BASE" ] && "$BIN" reclip --input "$BASE" --times 2 --out-dir "$OUT/rc" >/dev/null 2>&1 \
   && find "$OUT/rc" -name '*reclip_x2*' | grep -q .; then
  ok "reclip x2"
else
  bad "reclip x2" "无输出"
fi
if "$BIN" reroll "$ROOT/presets/film1970.json" --write "$OUT/rr.json" 2>/dev/null | grep -q '"seeds":1' \
   && grep -q '"seed": 12' "$OUT/rr.json"; then
  ok "reroll 种子+1"
else
  bad "reroll 种子+1" "无 seeds 增量"
fi
"$BIN" preview --preset "$ROOT/presets/vhs1990_ntscrs.json" --input "$FIX" --out-dir "$OUT/pv" --t 2 >/dev/null 2>&1
SRC_P=$(find "$OUT/pv" -name '*_src.png' -size +1k | head -1)
OUT_P=$(find "$OUT/pv" -name '*vhs1990_ntscrs*.png' ! -name '*_src.png' -size +1k | head -1)
FP_S=$(basename "$SRC_P" | sed -n 's/.*_\([0-9a-f]\{8\}\)_src\.png$/\1/p')
FP_O=$(basename "$OUT_P" | sed -n 's/.*_\([0-9a-f]\{8\}\)\.png$/\1/p')
if [ -n "$SRC_P" ] && [ -n "$OUT_P" ] && [ -n "$FP_S" ] && [ "$FP_S" = "$FP_O" ]; then
  ok "preview 帧对(同一次预览、同一参数指纹)"
else
  bad "preview 帧对(同一次预览、同一参数指纹)" "src='${SRC_P:-缺}' out='${OUT_P:-缺}' 指纹 '$FP_S' vs '$FP_O'"
fi

echo "=== 5. 批量(3 文件) ==="
mkdir -p "$OUT/bt"; for n in a b c; do cp "$FIX" "$OUT/bt/$n.mp4"; done
NDONE=$("$BIN" batch --preset "$ROOT/presets/cctv2000.json" --out-dir "$OUT/bt_out" "$OUT/bt"/*.mp4 2>/dev/null | grep -c '"type":"done"')
[ "$NDONE" = "3" ] && ok "batch 3/3" || bad "batch 3/3" "done=$NDONE"

echo "=== 6. 图片模式(全部预设 × 偶数/奇数画布:静帧剥时序 stage + 成品必须是真图片)==="
ffmpeg -hide_banner -loglevel error -y -i "$FIX" -ss 1 -frames:v 1 "$OUT/still.png"
# 奇数尺寸是真实坑:用户传的 2560×1439 图让第一趟 libx264 报 "height not divisible by 2",
# 而画廊备测件一直走 scale=-2 取偶,所以这条闸必须显式喂奇数
ffmpeg -hide_banner -loglevel error -y -i "$FIX" -ss 1 -frames:v 1 -vf "scale=321:211:flags=area" "$OUT/still_odd.png"
for STF in still still_odd; do
for P in "$ROOT"/presets/*.json; do
  id=$(basename "$P" .json)
  if ! "$BIN" run --preset "$P" --input "$OUT/$STF.png" --out-dir "$OUT/img" >"$OUT/log_img_${STF}_$id.txt" 2>&1; then
    bad "图片做旧 $id($STF)" "$(tail -c 180 "$OUT/log_img_${STF}_$id.txt" | tr '\n' ' ')"
    continue
  fi
  IMG=$(find "$OUT/img" -name "${STF}_${id}*.png" | head -1)
  CODEC=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 "$IMG" 2>/dev/null)
  # 曾经的真 bug:最终趟固定 libx264,把 H.264 流写进 .png 文件名,用户根本打不开成品
  if [ -n "$IMG" ] && [ "$CODEC" = "png" ]; then
    ok "图片做旧 $id($STF)"
  else
    bad "图片做旧 $id($STF)" "文件叫 .png 但里面是 codec=${CODEC:-空}"
  fi
done
done

# 静帧判据必须"没时长 + 静态图片容器"两条一起:裸 H.264 流(.264)实测 ffprobe 报 duration=0,
# 只看时长会被当静帧 —— 时序 stage 被剥、只出一帧、成品还是塞进错误文件名的 PNG。
ffmpeg -hide_banner -loglevel error -y -i "$FIX" -c:v copy -an "$OUT/raw.264"
RAWIMG=$("$BIN" probe "$OUT/raw.264" | tr -d ' \n' | grep -o '"is_image":[a-z]*' | head -1)
[ "$RAWIMG" = '"is_image":false' ] && ok "裸流视频不被当静帧(素材报告)" || bad "裸流视频不被当静帧(素材报告)" "$RAWIMG"
if "$BIN" run --preset "$ROOT/presets/cctv2000.json" --input "$OUT/raw.264" --out-dir "$OUT/rawrun" >"$OUT/log_raw.txt" 2>&1; then
  RAWF=$(find "$OUT/rawrun" -name 'raw_cctv2000*.mp4' | head -1)
  NF=$(ffprobe -v error -select_streams v:0 -count_frames -show_entries stream=nb_read_frames -of csv=p=0 "$RAWF" 2>/dev/null)
  if [ "${NF:-0}" -gt 24 ]; then
    ok "裸流跑出整段视频(${NF} 帧)"
  else
    bad "裸流跑出整段视频" "只得到 ${NF:-0} 帧(被当静帧了):$RAWF"
  fi
else
  bad "裸流跑出整段视频" "$(tail -c 180 "$OUT/log_raw.txt" | tr '\n' ' ')"
fi

echo "=== 7. 水印与音频规格 ==="
VOUT=$(find "$OUT" -maxdepth 1 -name '*vhs1990_ntscrs*.mp4' | head -1)
if [ -n "$VOUT" ] && ffprobe -v error -show_entries format_tags=comment -of csv=p=0 "$VOUT" | grep -q 'Rewind'; then
  ok "元数据水印"
else
  bad "元数据水印" "comment 缺失"
fi
CH=$(ffprobe -v error -select_streams a:0 -show_entries stream=channels -of csv=p=0 "$VOUT" 2>/dev/null)
MAXV=$(ffmpeg -i "$VOUT" -af volumedetect -f null - 2>&1 | grep -oP 'max_volume:\s*\K[-0-9.]+')
[ "$CH" = "1" ] && ok "VHS 单声道" || bad "VHS 单声道" "channels=$CH"
awk -v m="$MAXV" 'BEGIN{exit !(m < -6 && m > -60)}' && ok "无爆音 (max=${MAXV}dB)" || bad "无爆音" "max=${MAXV}dB"

echo "=== 8. 临时文件零残留 ==="
LEFT=$(find "$OUT" -name '.tmp*' -o -name '.*.tmp.*' | wc -l)
[ "$LEFT" = "0" ] && ok "无临时残留" || bad "无临时残留" "$LEFT 个"

echo "=== 9. Web 服务:上传 → 队列 → 成品 ==="
# 浏览器里的 File 拿不到磁盘路径,所以 /upload 是 Web 版唯一的添加素材通道,必须实测
if [ -d "$ROOT/app/ui" ] && command -v curl >/dev/null 2>&1; then
  WPORT=8199
  W=http://127.0.0.1:$WPORT
  rm -rf "$OUT/web_up" "$OUT/web_out"
  REWIND_UPLOAD_DIR="$OUT/web_up" "$BIN" serve --port $WPORT --ui "$ROOT/app/ui" >"$OUT/web.log" 2>&1 &
  WEBPID=$!
  for _ in $(seq 1 30); do curl -sf -o /dev/null "$W/" && break; sleep 0.3; done
  UP=$(curl -s -X POST --data-binary "@$FIX" "$W/upload?name=web%20clip.mp4")
  # 响应里的 "path":"..." 本身就是合法 JSON 片段(含转义),直接拼接可避开跨平台路径反斜杠坑
  PJ=$(printf '%s' "$UP" | grep -o '"path":"[^"]*"' | sed 's#"path":##')
  find "$OUT/web_up" -name '*web clip.mp4' | grep -q . && ok "Web 上传落盘" || bad "Web 上传落盘" "${UP:0:160}"
  BAD=$(curl -s -o /dev/null -w '%{http_code}' -X POST --data-binary "x" "$W/upload?name=evil.sh")
  [ "$BAD" = "415" ] && ok "Web 拒非媒体" || bad "Web 拒非媒体" "HTTP $BAD"
  if [ -n "$PJ" ]; then
    curl -s -X POST -d "{\"cmd\":\"start_jobs\",\"args\":{\"preset\":\"cctv2000\",\"files\":[$PJ],\"outDir\":\"$OUT/web_out\",\"intensity\":1.0}}" "$W/api" >"$OUT/web_job.json"
    for _ in $(seq 1 120); do
      curl -s -X POST -d '{"cmd":"events","args":{"job":0,"since":0}}' "$W/api" >"$OUT/web_ev.json"
      grep -q '"type":"batch"' "$OUT/web_ev.json" && break
      sleep 0.5
    done
    grep -q '"ok":true' "$OUT/web_ev.json" && ok "Web 队列出片" || bad "Web 队列出片" "$(head -c 200 "$OUT/web_ev.json")"
    find "$OUT/web_out" -name '*cctv2000*.mp4' | grep -q . && ok "Web 成品落盘" || bad "Web 成品落盘" "无 mp4"
    # 失败与取消也必须给界面一个交代:此前 serve 把子进程 stderr 丢进 /dev/null、只转发
    # progress/done,于是失败的那一项连一条 item 事件都没有 —— 界面永远停在"进行中"(实测)。
    curl -s -X POST -d "{\"cmd\":\"start_jobs\",\"args\":{\"preset\":\"cctv2000\",\"files\":[$PJ],\"outDir\":\"$OUT/web_fail\",\"intensity\":1.0,\"overrides\":[\"fps.fps=1000\"]}}" "$W/api" > "$OUT/web_fjob.json"
    for _ in $(seq 1 30); do
      curl -s -X POST -d '{"cmd":"events","args":{"job":1,"since":0}}' "$W/api" > "$OUT/web_fev.json"
      grep -q '"type":"batch"' "$OUT/web_fev.json" && break
      sleep 0.4
    done
    # 原因必须来自引擎自己打的 error 事件,而不是"退出码 + stderr 尾巴"那条兜底:
    # 兜底也在(panic / 被外部信号杀时靠它),但正常失败走兜底说明翻译这一环坏了。
    grep -q '"ok":false' "$OUT/web_fev.json" && grep -q '超出可用范围' "$OUT/web_fev.json" \
      && ! grep -q '引擎退出码' "$OUT/web_fev.json" \
      && ok "Web 失败给界面 item(ok:false)+ 错因来自 error 事件" \
      || bad "Web 失败给界面 item(ok:false)+ 错因来自 error 事件" "$(head -c 220 "$OUT/web_fev.json")"
    # 取消:引擎是被 kill 的,自己来不及收尾,父进程要凭 start 事件的标签清掉中间件。
    # 素材必须够慢:6 秒 320×240 的夹具跑满 11 趟也就几秒,"按时间窗取消"会撞上已经跑完,
    # 断言就会时好时坏 —— 这里现造一段 720p/24 s(合成信号图不影响这条计时断言)。
    SLOW=$OUT/slow_src.mp4
    ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=size=1280x720:rate=25:duration=24" \
      -an -c:v libx264 -preset ultrafast -crf 20 "$SLOW"
    US=$(curl -s -X POST --data-binary "@$SLOW" "$W/upload?name=slow%20clip.mp4")
    SP=$(printf '%s' "$US" | grep -o '"path":"[^"]*"' | sed 's#"path":##')
    if [ -n "$SP" ]; then
      curl -s -X POST -d "{\"cmd\":\"start_jobs\",\"args\":{\"preset\":\"patina\",\"files\":[$SP],\"outDir\":\"$OUT/web_cancel\",\"intensity\":1.0,\"overrides\":[\"preset.aging=8\"]}}" "$W/api" > /dev/null
      sleep 4
      curl -s -X POST -d '{"cmd":"cancel_jobs","args":{}}' "$W/api" > /dev/null
      for _ in $(seq 1 60); do
        curl -s -X POST -d '{"cmd":"events","args":{"job":2,"since":0}}' "$W/api" > "$OUT/web_cev.json"
        grep -q '"canceled":true' "$OUT/web_cev.json" && break
        sleep 0.5
      done
      grep -q '"canceled":true' "$OUT/web_cev.json" \
        && ok "Web 取消给界面 canceled 收尾(不再永远转圈)" || bad "Web 取消给界面 canceled 收尾(不再永远转圈)" "$(head -c 220 "$OUT/web_cev.json")"
      CLEFT=$(find "$OUT/web_cancel" -name '*tmp*' 2>/dev/null | wc -l)
      [ "$CLEFT" = "0" ] && ok "Web 取消后被杀那一手的中间件已清掉" || bad "Web 取消后被杀那一手的中间件已清掉" "残留 $CLEFT 个:$(find "$OUT/web_cancel" -name '*tmp*' | head -2 | tr '\n' ' ')"
    fi
  fi
  kill $WEBPID 2>/dev/null
else
  echo "SKIP  10. Web 服务(缺 app/ui 或 curl)"
fi

echo "=== 10. 画廊对比图 / 素材报告 / 防覆盖命名 ==="
if [ -f "$ROOT/assets/reference/reference.png" ] && [ -f "$ROOT/scripts/preset_gallery.sh" ]; then
  if bash "$ROOT/scripts/preset_gallery.sh" --check >/dev/null 2>&1; then
    ok "预设对比图与动图齐全"
  else
    bad "预设对比图与动图齐全" "跑 bash scripts/preset_gallery.sh 重烘"
  fi
else
  echo "SKIP  10a. 画廊(缺 assets/reference/reference.png 或脚本)"
fi
REP=$("$BIN" probe "$FIX" 2>/dev/null)
for K in dar vfr video_codec container size_bytes; do
  printf '%s' "$REP" | grep -q "\"$K\"" && ok "素材报告含 $K" || bad "素材报告含 $K" "${REP:0:120}"
done
rm -rf "$OUT/dupdir"; mkdir -p "$OUT/dupdir"
"$BIN" run --preset "$ROOT/presets/dvd2005.json" --input "$OUT/still.png" --out-dir "$OUT/dupdir" >/dev/null 2>&1
"$BIN" run --preset "$ROOT/presets/dvd2005.json" --input "$OUT/still.png" --out-dir "$OUT/dupdir" >/dev/null 2>&1
if ls "$OUT/dupdir"/*-1.png >/dev/null 2>&1; then ok "同名输出自动 -1 防覆盖"; else bad "同名输出自动 -1 防覆盖" "$(ls "$OUT/dupdir" 2>/dev/null | tr '\n' ' ')"; fi

# 用户预设是手工调出来的东西:覆盖必须有备份、写入必须原子。曾经这里是裸 fs::write,静默毁掉用户文件
echo "=== 11. 用户预设覆盖保护 ==="
D=$OUT/psave; rm -rf "$D"; mkdir -p "$D"
"$BIN" preset-save --dir "$D" --name probe --era 1990 >/dev/null 2>&1
"$BIN" preset-save --dir "$D" --name probe --era 2026 >/dev/null 2>&1
if [ -f "$D/probe.json" ] && [ -f "$D/probe.json.bak" ]; then ok "覆盖 preset-save 留下 .bak"; else bad "覆盖 preset-save 留下 .bak" "$(ls "$D" 2>/dev/null | tr '\n' ' ')"; fi
if [ ! -f "$D/probe.json.bak" ]; then bad ".bak 是旧内容而非新内容" "无 .bak 可比"
elif cmp -s "$D/probe.json" "$D/probe.json.bak"; then bad ".bak 是旧内容而非新内容" "两份完全相同"
else ok ".bak 是旧内容而非新内容"; fi
"$BIN" reroll "$D/probe.json" >/dev/null 2>&1
# 残留glob 必须是 *.tmp*(不是 *.json.tmp):临时名早就带上运行标签了,
# 老 glob 匹配不到任何东西 —— 是一条恒绿的空断言(§8.1-53 的"空转断言"又一个实例)
if ls "$D"/*.tmp* >/dev/null 2>&1; then bad "预设写入原子性(无 .tmp 残留)" "$(ls "$D" | tr '\n' ' ')"; else ok "预设写入原子性(无 .tmp 残留)"; fi

# 两条都是用户实测报回来的:奇数边长让中间编码器炸(2560×1439),静帧预览 exit 0 却不写文件(-ss 1 抽 0 帧)
echo "=== 12. 奇数尺寸视频 / 静帧预览产物 ==="
ODD=$ROOT/fixtures/test_odd.mp4
if [ -f "$ODD" ]; then
  for P in film1970 vhs1990_ntscrs cctv2000; do
    rm -rf "$OUT/odd_$P"; mkdir -p "$OUT/odd_$P"
    if ! "$BIN" run --preset "$ROOT/presets/$P.json" --input "$ODD" --out-dir "$OUT/odd_$P" >"$OUT/odd_$P.log" 2>&1; then
      bad "奇数视频做旧 $P" "$(tail -c 200 "$OUT/odd_$P.log" | tr '\n' ' ')"
      continue
    fi
    WH=$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$OUT/odd_$P"/*.mp4 2>/dev/null | head -1)
    W=${WH%,*}; H=${WH#*,}
    if [ -n "$W" ] && [ $((W % 2)) = 0 ] && [ $((H % 2)) = 0 ]; then
      ok "奇数视频做旧 $P(成品 $W×$H)"
    else
      bad "奇数视频做旧 $P(成品边长)" "读到 '$WH'"
    fi
  done
else
  echo "SKIP  12a. 奇数尺寸视频(缺 fixtures/test_odd.mp4)"
fi
rm -rf "$OUT/pvimg"; mkdir -p "$OUT/pvimg"
"$BIN" preview --preset "$ROOT/presets/film1970.json" --input "$OUT/still_odd.png" --out-dir "$OUT/pvimg" >"$OUT/pvimg.log" 2>&1
N=$(find "$OUT/pvimg" -name '*.png' -size +1k 2>/dev/null | wc -l)
if [ "$N" = "2" ]; then ok "静帧预览产出一对真帧"; else bad "静帧预览产出一对真帧" "只有 $N 个 >1KB 文件: $(ls "$OUT/pvimg" 2>/dev/null | tr '\n' ' ')"; fi

# 用户实测报回"调了左侧参数画面不动":预览产物按参数指纹命名——同参数不重跑,改参数必须换帧
echo "=== 13. 预览指纹(同参命中缓存 / 改参必换帧)==="
PV=$OUT/fp; rm -rf "$PV"; mkdir -p "$PV"
pv() { "$BIN" preview --preset "$ROOT/presets/patina.json" --input "$FIX" --out-dir "$PV" "$@" 2>/dev/null | grep '"type":"preview"'; }
res_of() { printf '%s' "$1" | sed -n 's/.*"result":"\([^"]*\)".*/\1/p'; }
A=$(pv --t 1); B=$(pv --t 1)
RA=$(res_of "$A"); RB=$(res_of "$B")
if [ -n "$RA" ] && [ "$RA" = "$RB" ] && printf '%s' "$B" | grep -q '"cached":true'; then
  ok "同参数二次预览命中缓存(同一文件 + cached:true)"
else
  bad "同参数二次预览命中缓存" "A=$(basename "${RA:-空}") B=$(basename "${RB:-空}") B里 cached 标记=$(printf '%s' "$B" | grep -o '"cached":[a-z]*')"
fi
if printf '%s' "$A" | grep -q '"rate":[0-9]'; then
  ok "预览回报本机吞吐(rate 字段)"
else
  bad "预览回报本机吞吐(rate 字段)" "A=$A"
fi
C=$(pv --t 1 --override preset.aging=6); D=$(pv --t 1 --intensity 1.5); E=$(pv --t 1 --override preset.cast=0.6)
RC=$(res_of "$C"); RD=$(res_of "$D"); RE=$(res_of "$E")
distinct=1
for x in "$RC" "$RD" "$RE"; do
  [ -s "$x" ] || distinct=0
  [ "$x" = "$RA" ] && distinct=0
done
if [ "$distinct" = "1" ] && ! cmp -s "$RA" "$RC" && ! cmp -s "$RA" "$RD" && ! cmp -s "$RA" "$RE"; then
  ok "改手数/强度/偏色都换了预览帧(且字节互不相同)"
else
  bad "改手数/强度/偏色都换了预览帧" "base=$(basename "${RA:-空}") aging6=$(basename "${RC:-空}") int1.5=$(basename "${RD:-空}") cast=$(basename "${RE:-空}")"
fi
# 「换一批」只改种子、不改预设 id:指纹若只认 id,换完照样 cached:true 端出旧帧(实测)。
$BIN reroll "$ROOT/presets/film1970.json" --write "$PV/rerolled.json" >/dev/null 2>&1
RR1=$("$BIN" preview --preset "$ROOT/presets/film1970.json" --input "$FIX" --out-dir "$PV" --t 1 2>/dev/null | grep '"type":"preview"')
RR2=$("$BIN" preview --preset "$PV/rerolled.json" --input "$FIX" --out-dir "$PV" --t 1 2>/dev/null | grep '"type":"preview"')
RR1F=$(res_of "$RR1"); RR2F=$(res_of "$RR2")
if [ -n "$RR2F" ] && [ "$RR1F" != "$RR2F" ] && ! printf '%s' "$RR2" | grep -q '"cached":true'; then
  ok "换一批(种子变了)必须换预览帧"
else
  bad "换一批(种子变了)必须换预览帧" "原=$(basename "${RR1F:-空}") 换后=$(basename "${RR2F:-空}") $(printf '%s' "$RR2" | grep -o '"cached":[a-z]*')"
fi
# 同一预设同参数再问一次必须命中缓存(上一条的对照:不是"每次都重跑"蒙过去的)
RR3=$("$BIN" preview --preset "$PV/rerolled.json" --input "$FIX" --out-dir "$PV" --t 1 2>/dev/null | grep '"type":"preview"')
if printf '%s' "$RR3" | grep -q '"cached":true' && [ "$(res_of "$RR3")" = "$RR2F" ]; then
  ok "换一批之后同参数再预览仍命中新缓存"
else
  bad "换一批之后同参数再预览仍命中新缓存" "$(printf '%s' "$RR3" | grep -o '"cached":[a-z]*')"
fi
LEFT=$(find "$PV" -name '*.tmp.*' 2>/dev/null | wc -l)
if [ "$LEFT" = "0" ]; then ok "预览临时文件零残留"; else bad "预览临时文件零残留" "残留 $LEFT 个: $(find "$PV" -name '*.tmp.*' | head -3 | tr '\n' ' ')"; fi

echo
echo "=== 14. 总体进度读数(界面画的那个百分比)与盘满报错 ==="
# 界面以前画的是"本趟百分比",于是 5 趟的任务会把 0→100% 扫五遍。引擎现在必须同时给
# overall,且它单调不后退、收尾到 100 —— 这几条只能从真实事件流里量。
PLOG=$($BIN run --preset "$ROOT/presets/patina.json" --input "$FIX" --out-dir "$OUT" 2>&1)
PV14=$(printf '%s' "$PLOG" | python3 -c '
import json,sys
NUM=(int,float); evs=[]
for l in sys.stdin:
    l=l.strip()
    if not l.startswith("{"): continue
    try: d=json.loads(l)
    except Exception: continue
    if d.get("type")=="progress": evs.append(d)
n=len(evs)
haso=1 if n and all(isinstance(e.get("overall"),NUM) for e in evs) else 0
meta=1 if n and all(isinstance(e.get("step"),NUM) and isinstance(e.get("steps"),NUM) for e in evs) else 0
multi=1 if n and max((e.get("steps") or 0) for e in evs)>1 else 0
o=[e.get("overall") if isinstance(e.get("overall"),NUM) else 0.0 for e in evs]
p=[e.get("pct") if isinstance(e.get("pct"),NUM) else 0.0 for e in evs]
mono=1 if all(b>=a-1e-9 for a,b in zip(o,o[1:])) else 0
end=1 if o and o[-1]>=99.9 else 0
differs=1 if any(abs(a-b)>0.5 for a,b in zip(o,p)) else 0
mid=1 if any(a<99.0 and b>=99.9 for a,b in zip(o,p)) else 0
print(n,haso,meta,multi,mono,end,differs,mid)
')
read -r N14 HASO14 META14 MULTI14 MONO14 END14 DIFF14 MID14 <<<"$PV14"
if [ "${N14:-0}" -ge 3 ]; then ok "多趟任务收到 $N14 条 progress 事件"; else bad "多趟任务收到 progress 事件" "只收到 ${N14:-0} 条"; fi
[ "${HASO14:-0}" = "1" ] && ok "每条 progress 都带 overall(总体读数)" || bad "每条 progress 都带 overall(总体读数)" "有事件缺该字段"
[ "${META14:-0}" = "1" ] && ok "progress 带 step/steps 字段" || bad "progress 带 step/steps 字段" "界面据此写第 i/n 趟,不许解析机器标签"
[ "${MULTI14:-0}" = "1" ] && ok "这一手确实是多趟(steps>1)" || bad "这一手确实是多趟(steps>1)" "steps 全是 1,下面几条在空转"
[ "${MONO14:-0}" = "1" ] && ok "总体读数单调不后退" || bad "总体读数单调不后退" "出现回退"
[ "${END14:-0}" = "1" ] && ok "收尾总体读数达到 100" || bad "收尾总体读数达到 100" "最后一条不是 100"
# 防"overall 只是 pct 的别名"这种假通过
[ "${DIFF14:-0}" = "1" ] && ok "overall 与本趟 pct 确实不同(不是别名)" || bad "overall 与本趟 pct 确实不同" "两者处处相等"
[ "${MID14:-0}" = "1" ] && ok "本趟 100% 时总体还没到 100%(老 bug 的形态已被拆开)" || bad "本趟 100% 时总体还没到 100%" "拆不开:可能压根没跨趟"

# 盘满:引擎要把 ffmpeg 的半句 ENOSPC 说成人话,而普通失败不许赖给磁盘。
# 没有免密 sudo 就造不出"真满"的文件系统,这里用假 ffmpeg 把真实 stderr 灌进同一条报错路径。
FAKE=$OUT/fake_ffmpeg.sh
cat > "$FAKE" <<'EOF'
#!/bin/sh
[ -n "$REWIND_FAKE_ERR" ] && cat "$REWIND_FAKE_ERR" >&2
exit 1
EOF
chmod +x "$FAKE"
printf 'Error writing trailer: No space left on device\n' > "$OUT/enospc_err.txt"
REWIND_FFMPEG="$FAKE" REWIND_FAKE_ERR="$OUT/enospc_err.txt" \
  "$BIN" run --preset "$ROOT/presets/patina.json" --input "$FIX" --out-dir "$OUT" >"$OUT/enospc.log" 2>&1
if grep -q "磁盘写满" "$OUT/enospc.log"; then
  ok "盘满失败给人话结论(不是半句 write failed)"
else
  bad "盘满失败给人话结论(不是半句 write failed)" "$(tail -c 200 "$OUT/enospc.log" | tr '\n' ' ')"
fi
printf "No such filter: 'banddither'\n" > "$OUT/bogus_err.txt"
REWIND_FFMPEG="$FAKE" REWIND_FAKE_ERR="$OUT/bogus_err.txt" \
  "$BIN" run --preset "$ROOT/presets/patina.json" --input "$FIX" --out-dir "$OUT" >"$OUT/bogus.log" 2>&1
if grep -q "磁盘写满" "$OUT/bogus.log"; then
  bad "普通滤镜失败不赖给磁盘" "被误标成盘满:$(tail -c 160 "$OUT/bogus.log" | tr '\n' ' ')"
else
  ok "普通滤镜失败不赖给磁盘"
fi
LEFT14=$(find "$OUT" -maxdepth 1 -name '.*tmp*' 2>/dev/null | wc -l)
if [ "$LEFT14" = "0" ]; then ok "失败的趟没留下半截临时文件"; else bad "失败的趟没留下半截临时文件" "残留 $LEFT14 个:$(find "$OUT" -maxdepth 1 -name '.*tmp*' | head -3 | tr '\n' ' ')"; fi

# 跑到中间某一趟才炸(不是第一趟):上一趟的中间件也必须有交代。
# 这条路只有假 ffmpeg 能走稳 —— scripts/fake_ffmpeg.sh 放行前 N-1 次调用,第 N 次点掉。
mkdir -p "$OUT/midfail"; rm -f "$OUT/ffcnt"
REWIND_FFMPEG="$ROOT/scripts/fake_ffmpeg.sh" FAKE_FAIL_AT=3 FAKE_FAIL_STATE="$OUT/ffcnt" \
  "$BIN" run --preset "$ROOT/presets/patina.json" --input "$FIX" --out-dir "$OUT/midfail" >"$OUT/midfail.log" 2>&1
if grep -q '"type":"error"' "$OUT/midfail.log"; then
  ok "失败以 error 事件走 stdout(界面与 serve 唯一的收尾依据)"
else
  bad "失败以 error 事件走 stdout(界面与 serve 唯一的收尾依据)" "$(tail -c 200 "$OUT/midfail.log" | tr '\n' ' ')"
fi
MLEFT=$(find "$OUT/midfail" -name '*tmp*' 2>/dev/null | wc -l)
if [ "$MLEFT" = "0" ]; then
  ok "中间趟失败:上一趟的中间件不留存"
else
  bad "中间趟失败:上一趟的中间件不留存" "残留 $MLEFT 个:$(find "$OUT/midfail" -name '*tmp*' | head -2 | tr '\n' ' ')"
fi

echo
echo "REGRESSION: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" = "0" ] || exit 1
