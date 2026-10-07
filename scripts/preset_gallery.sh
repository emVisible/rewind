#!/usr/bin/env bash
# 预设对比图画廊烘培(§规划 §5):统一用 assets/reference/reference.png 作参考素材,
# 对每个预设跑一遍真管线,产出「左半原图 | 右半做旧」的分割瓦片 + 整帧做旧图,随 UI 打包。
# 用法:
#   bash scripts/preset_gallery.sh            # 烘培全部预设(需先 cargo build --release)
#   bash scripts/preset_gallery.sh vhs1990_ntscrs crt1995   # 只烘培指定预设
#   bash scripts/preset_gallery.sh --check    # 只校验产物是否存在且比预设文件新
set -uo pipefail
cd "$(dirname "$0")/.."

BIN=${BIN:-core/target/release/rewind-core}
REF=assets/reference/reference.png
OUT=app/ui/gallery
PROXY_W=640          # 代理输入长边:够看清宏块/划痕,又不至于烘一次要几分钟
HERO_W=1280          # hover 大图的输入长边:1:1 取块要 640+640 才够分
TILE_W=480           # 瓦片宽(左原右旧各 240)
TILE_H=270           # 瓦片高:所有瓦片同尺寸,UI 网格才齐
HERO_H=720
MOTION_T=4           # 动图素材时长(秒)
[ -x "$BIN" ] || { echo "FATAL: 先 cargo build --release(core)"; exit 2; }
[ -f "$REF" ] || { echo "FATAL: 缺统一参考图 $REF"; exit 2; }

CHECK=0
WANT=()
for A in "$@"; do
  if [ "$A" = "--check" ]; then CHECK=1; else WANT+=("$A"); fi
done

mkdir -p "$OUT"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# 代理参考图:等比缩到 640 宽(偶数),画廊与定标共用这一个视觉锚点
ffmpeg -hide_banner -loglevel error -y -i "$REF" \
  -vf "scale=$PROXY_W:-2:flags=area" "$TMP/ref.png" || exit 2
# hover 大图用的大代理:1:1 中心取块要 640+640 才铺得满
ffmpeg -hide_banner -loglevel error -y -i "$REF" \
  -vf "scale=$HERO_W:-2:flags=area" "$TMP/ref1280.png" || exit 2
# 同源运动短片:对同一张参考图做缓慢推镜(Ken Burns),让时间型预设的对比图说真话。
# 表达式里的逗号必须用单引号包住,否则 zoompan 会把 min(a,b) 当成两个滤镜参数。
PROXY_H=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of csv=p=0 "$TMP/ref.png")
ffmpeg -hide_banner -loglevel error -y -loop 1 -i "$REF" \
  -vf "scale=1920:-2,zoompan=z='min(zoom+0.0009,1.18)':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':d=$((MOTION_T * 25)):s=${PROXY_W}x${PROXY_H}:fps=25" \
  -t "$MOTION_T" -c:v libx264 -preset ultrafast -crf 18 -pix_fmt yuv420p "$TMP/ref.mp4" || exit 2

ids=${#WANT[@]}
if [ "$ids" = 0 ]; then
  for P in presets/*.json; do WANT+=("$(basename "$P" .json)"); done
fi

fail=0
for ID in "${WANT[@]}"; do
  PF="presets/$ID.json"
  [ -f "$PF" ] || { echo "SKIP  $ID(无预设)"; continue; }
  if [ "$CHECK" = 1 ]; then
    for F in "$OUT/$ID.png" "$OUT/${ID}_full.png" "$OUT/${ID}_hero.webp" "$OUT/$ID.webm"; do
      # 只把"存在"当硬门:CI checkout 会统一 mtime,拿新旧关系断言必然随机翻脸
      if [ ! -s "$F" ]; then echo "FAIL  $F 缺失(跑 bash scripts/preset_gallery.sh 烘培)"; fail=1;
      elif [ "$F" -nt "$PF" ]; then :; else echo "warn  $F 不比 $PF 新,预设若改过请重烘"; fi
    done
    continue
  fi
  rm -f "$TMP/out_$ID.png"
  if ! "$BIN" run --preset "$PF" --input "$TMP/ref.png" --out-dir "$TMP" >"$TMP/log_$ID" 2>&1; then
    echo "FAIL  $ID 出片失败: $(tail -c 160 "$TMP/log_$ID" | tr '\n' ' ')"; fail=1; continue
  fi
  RES=$(grep -o '"output":"[^"]*"' "$TMP/log_$ID" | head -1 | cut -d'"' -f4)
  [ -s "$RES" ] || { echo "FAIL  $ID 无输出"; fail=1; continue; }
  # 整帧做旧图(缩略用,允许平均)
  ffmpeg -hide_banner -loglevel error -y -i "$RES" -vf \
    "scale=$TILE_W:$TILE_H:force_original_aspect_ratio=decrease,pad=$TILE_W:$TILE_H:(ow-iw)/2:(oh-ih)/2:color=black" \
    "$OUT/${ID}_full.png"
  # 分割瓦片:必须按成品画布 **1:1 中心取块** —— 缩放会把扫描线/宏块平均掉,对比图就白做了。
  # 成品比瓦片小(如 3GP 176x144)时先 neighbor 整数放大,保住真实像素块。
  RW=$(ffprobe -v error -select_streams v:0 -show_entries stream=width -of csv=p=0 "$RES")
  RH=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of csv=p=0 "$RES")
  HALF=$((TILE_W / 2))
  UP=""
  if [ "${RW:-0}" -lt "$HALF" ] || [ "${RH:-0}" -lt "$TILE_H" ]; then
    K=$(( (HALF / RW) + 1 ))
    [ $(( RH * K )) -lt "$TILE_H" ] && K=$(( (TILE_H / RH) + 1 ))
    UP="scale=iw*$K:ih*$K:flags=neighbor,"
  fi
  SRC_NORM="scale=$RW:$RH:flags=area,${UP}crop=$HALF:$TILE_H:(iw-$HALF)/2:(ih-$TILE_H)/2"
  RES_NORM="${UP}crop=$HALF:$TILE_H:(iw-$HALF)/2:(ih-$TILE_H)/2"
  ffmpeg -hide_banner -loglevel error -y -i "$TMP/ref.png" -i "$RES" -filter_complex \
    "[0:v]$SRC_NORM[a];[1:v]$RES_NORM[b]; \
     [a][b]hstack=inputs=2,drawbox=x=$HALF-1:y=0:w=2:h=$TILE_H:color=0xe8b84b@0.9:t=fill" \
    "$OUT/$ID.png" || { echo "FAIL  $ID 拼瓦片失败"; fail=1; continue; }
  # hover 大图:先拿大代理跑一趟(成品才有 1:1 的块可取),再拼左右
  HRES=""
  if "$BIN" run --preset "$PF" --input "$TMP/ref1280.png" --out-dir "$TMP" >"$TMP/hlog_$ID" 2>&1; then
    HRES=$(grep -o '"output":"[^"]*"' "$TMP/hlog_$ID" | head -1 | cut -d'"' -f4)
  fi
  if [ -z "$HRES" ] || [ ! -s "$HRES" ]; then
    echo "warn  $ID 大图出片失败:$(tail -c 140 "$TMP/hlog_$ID" | tr '\n' ' ')"
  else
  HHALF=$((HERO_W / 2))
  HRW=$(ffprobe -v error -select_streams v:0 -show_entries stream=width -of csv=p=0 "$HRES" 2>/dev/null)
  HRH=$(ffprobe -v error -select_streams v:0 -show_entries stream=height -of csv=p=0 "$HRES" 2>/dev/null)
  HUP=""
  if [ "${HRW:-0}" -lt "$HHALF" ] || [ "${HRH:-0}" -lt "$HERO_H" ]; then
    K=$(( (HHALF / HRW) + 1 ))
    [ $(( HRH * K )) -lt "$HERO_H" ] && K=$(( (HERO_H / HRH) + 1 ))
    HUP="scale=iw*$K:ih*$K:flags=neighbor,"
  fi
  ffmpeg -hide_banner -loglevel error -y -i "$TMP/ref1280.png" -i "$HRES" -filter_complex \
    "[0:v]scale=$HRW:$HRH:flags=area,${HUP}crop=$HHALF:$HERO_H:(iw-$HHALF)/2:(ih-$HERO_H)/2[a]; \
     [1:v]${HUP}crop=$HHALF:$HERO_H:(iw-$HHALF)/2:(ih-$HERO_H)/2[b]; \
     [a][b]hstack=inputs=2,drawbox=x=$HHALF-1:y=0:w=2:h=$HERO_H:color=0xe8b84b@0.9:t=fill,scale=$HERO_W:$HERO_H" \
    -c:v libwebp -quality 80 "$OUT/${ID}_hero.webp" || echo "warn  $ID 大图烘培失败"
  fi
  # 动图瓦片:同一条运动短片走同一条管线,hover 时播它 —— 静帧会低估 VHS/CRT/DVD
  MRES=""
  if "$BIN" run --preset "$PF" --input "$TMP/ref.mp4" --out-dir "$TMP" >"$TMP/mlog_$ID" 2>&1; then
    MRES=$(grep -o '"output":"[^"]*"' "$TMP/mlog_$ID" | head -1 | cut -d'"' -f4)
  fi
  if [ -n "$MRES" ] && [ -s "$MRES" ]; then
    ffmpeg -hide_banner -loglevel error -y -i "$MRES" -t 2.4 -vf "scale=$TILE_W:-2:flags=area" \
      -c:v libvpx-vp9 -b:v 320k -an -deadline realtime "$OUT/$ID.webm" \
      || echo "warn  $ID 动图烘培失败(画廊仍可用静帧)"
  else
    echo "warn  $ID 动图素材出片失败:$(tail -c 140 "$TMP/mlog_$ID" | tr '\n' ' ')"
  fi
  echo "OK    $ID → $OUT/$ID.png$( [ -s "$OUT/$ID.webm" ] && echo ' + webm')"
done

if [ "$CHECK" = 1 ]; then
  [ "$fail" = 0 ] && echo "GALLERY CHECK: OK(${#WANT[@]} 个预设)" || echo "GALLERY CHECK: 有产物缺失/过期"
  exit $fail
fi
ls -l "$OUT" | tail -n +2 | awk '{printf "%-28s %8d\n", $9, $5}'
