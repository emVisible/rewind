#!/usr/bin/env bash
# 画幅一致性闸:对比预览的两层必须同几何,缩略位不许裁掉非 16:9 的画面。
# 起因:用「监控录像」(输出 640×480)时,原帧那一层还是 1280×720,两层各自 contain
# 到同一个盒子 → 滑杆扫过去看到的是两张错位的画。这条闸盯的就是"再也没人提醒我们画幅变了"。
# 用法: bash scripts/geo_check.sh
set -u
cd "$(dirname "$0")/.."
BIN=${BIN:-core/target/release/rewind-core}
SRC=${SRC:-assets/sample/street-food.mp4}
IMG=${IMG:-assets/sample/street-food.png}
WORK=${WORK:-.work/gate/geo}
rm -rf "$WORK"; mkdir -p "$WORK"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf 'PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf 'FAIL  %s  <- %s\n' "$1" "$2"; }

[ -x "$BIN" ] || { echo "FATAL  没有 $BIN(先 cargo build --release)"; exit 2; }
[ -f "$SRC" ] || { echo "FATAL  没有样例素材 $SRC"; exit 2; }

# 取一帧的宽高,输出 "WxH"
dim() {
  ffprobe -v error -select_streams v:0 -show_entries stream=width,height \
    -of csv=p=0 "$1" 2>/dev/null | tr -d ' \n' | tr ',' 'x'
}
# plan 子命令会在 JSON 前面打 warn 行(缺字体),所以只取以 { 开头的那一行
planof() { "$BIN" plan --preset "$1" --input "$2" 2>/dev/null | grep '^{' | tail -1; }
jget() { python3 -c "import json,sys;d=json.loads(sys.stdin.read() or '{}');print(d$1)" 2>/dev/null; }

SW=$(dim "$SRC")
SRC_MD5=$(ffmpeg -hide_banner -loglevel error -y -ss 3 -i "$SRC" -frames:v 1 "$WORK/plain.png" && md5sum "$WORK/plain.png" | cut -d' ' -f1)
[ -n "$SRC_MD5" ] || { echo "FATAL  取不到对照原帧"; exit 2; }
# 量具自检:净几何恒等的那几个预设必须真的进入第 3 条断言,否则它就是一条恒真的绿
[ "${SW#*x}" != "$SW" ] || { echo "FATAL  源尺寸解析成了「$SW」,不是 WxH —— 第 3 条断言会空转"; exit 2; }
ok "对照基准就绪(源 ${SW},原帧 md5 已存)"

N=0
PRISTINE_SEEN=0
for p in presets/*.json; do
  id=$(basename "$p" .json); N=$((N+1))
  pj=$(planof "$p" "$SRC")
  if [ -z "$pj" ]; then bad "$id plan 可读" "无 JSON 输出"; continue; fi
  o_w=$(printf '%s' "$pj" | jget "['out']['w']"); o_h=$(printf '%s' "$pj" | jget "['out']['h']")
  d_w=$(printf '%s' "$pj" | jget "['out']['display_w']"); d_h=$(printf '%s' "$pj" | jget "['out']['display_h']")
  dar=$(printf '%s' "$pj" | jget "['out']['dar'] or ''")

  # 1) 没有 DAR 时呈现尺寸必须等于存储尺寸 —— 界面拿这两个值排版,不许自相矛盾
  if [ -z "$dar" ] && { [ "$d_w" != "$o_w" ] || [ "$d_h" != "$o_h" ]; }; then
    bad "$id 无 DAR 时呈现=存储" "存储 ${o_w}×${o_h} 呈现 ${d_w}×${d_h}"
  else
    ok "$id 交付画幅自洽(${o_w}×${o_h}${dar:+ DAR $dar})"
  fi

  # 2) 核心不变量:预览出来的两张图必须同尺寸
  j=$("$BIN" preview --preset "$p" --input "$SRC" --out-dir "$WORK" --t 3 2>&1 | grep '^{' | tail -1)
  s=$(printf '%s' "$j" | jget "['source']"); r=$(printf '%s' "$j" | jget "['result']")
  if [ -z "$s" ] || [ -z "$r" ]; then bad "$id 预览成对" "无输出: $(printf '%s' "$j" | cut -c1-60)"; continue; fi
  sd=$(dim "$s"); rd=$(dim "$r")
  if [ "$sd" != "$rd" ]; then
    bad "$id 预览两层同几何" "原帧 $sd vs 成品 $rd"
  else
    ok "$id 预览两层同几何($sd)"
  fi

  # 3) 净几何恒等时,"原片"那一层必须一个像素都没动 —— 否则差异被重采样量小了
  crops=$(printf '%s' "$pj" | python3 -c "
import json,sys
d = json.load(sys.stdin)
print('y' if any(f.startswith(('crop=', 'pad=')) for f in d.get('geom', [])) else 'n')" 2>/dev/null)
  if [ "$o_w" = "${SW%x*}" ] && [ "$o_h" = "${SW#*x}" ] && [ -z "$dar" ] && [ "$crops" = "n" ]; then
    PRISTINE_SEEN=$((PRISTINE_SEEN + 1))
    if [ "$(md5sum "$s" | cut -d' ' -f1)" != "$SRC_MD5" ]; then
      bad "$id 原帧未被重采样" "净几何恒等却还是被链过了一遍"
    else
      ok "$id 原帧未被重采样"
    fi
  fi

  # 4) 图片交付:PNG 带不走 SAR,有 DAR 就必须烘进像素
  ipj=$(planof "$p" "$IMG")
  i_w=$(printf '%s' "$ipj" | jget "['out']['w']"); i_h=$(printf '%s' "$ipj" | jget "['out']['h']")
  i_dar=$(printf '%s' "$ipj" | jget "['out']['dar'] or ''")
  i_dw=$(printf '%s' "$ipj" | jget "['out']['display_w']"); i_dh=$(printf '%s' "$ipj" | jget "['out']['display_h']")
  if [ -z "$i_w" ]; then
    bad "$id 图片 plan 可读" "无 out"
  elif [ -n "$i_dar" ]; then
    bad "$id 图片交付把 DAR 烘进像素" "还留着 DAR $i_dar(浏览器按方像素画 = 变形)"
  elif [ -n "$dar" ] && [ "$i_h" != "0" ]; then
    # 视频侧带 DAR:图片侧的实际比例必须已经等于那个 DAR,才算真烘进去了
    ratio_ok=$(python3 -c "
dn, dd = '$dar'.split(':')
print('y' if abs($i_w / $i_h - float(dn) / float(dd)) < 0.02 else 'n')" 2>/dev/null)
    if [ "$ratio_ok" = "y" ]; then
      ok "$id 图片交付已按 $dar 烘进像素(${i_w}×${i_h})"
    else
      bad "$id 图片交付按 DAR 折算" "视频 DAR $dar,图片却是 ${i_w}×${i_h}"
    fi
  else
    ok "$id 图片交付尺寸即所见(${i_w}×${i_h})"
  fi
  # 5) 音频与字幕决策必须在 plan 里可见(此前只导出视频滤镜,底噪丢没丢、时间戳画没画,闸一概看不见)
  printf '%s' "$pj" | AUDIO_PRESET="$p" python3 -c "
import json,os,sys
d=json.load(sys.stdin); p=os.environ['AUDIO_PRESET']
raw=open(p,encoding='utf-8').read()
bad=[]
fast=[s for s in d['steps'] if s['kind']=='fast']
for s in fast:
    if not s.get('audio'): bad.append('有 fast 趟没导出 audio 决策')
    if '+hiss' in (s.get('audio') or '') and not s.get('extra_inputs'): bad.append('audio 声明底噪却没有 lavfi 附加输入')
if '\"overlay_timestamp\"' in raw:
    if not any('drawtext' in f for s in fast for f in (s.get('vf') or [])):
        bad.append('预设带时间戳,但 plan 里没有 drawtext(字体没传进 plan?)')
    if not d.get('font'): bad.append('预设带时间戳却拿不到字体')
print('|'.join(bad))
" > "$WORK/aud_$id.txt" 2>&1
  AUD=$(cat "$WORK/aud_$id.txt")
  if [ -z "$AUD" ]; then ok "$id 音频/时间戳决策在 plan 里可见"; else bad "$id 音频/时间戳决策可见" "$AUD"; fi
done
[ "$N" -ge 8 ] || bad "覆盖到全部内置预设" "只跑了 $N 个"
[ "$PRISTINE_SEEN" -ge 5 ] || bad "原帧断言真的在跑" "只有 $PRISTINE_SEEN 个预设落进净几何恒等分支(量具空转)"

# 5) 端到端验一次:图片真跑一遍,交付文件的实际像素比例就是它声称的画幅
DARP=$(python3 - <<'PY'
import glob, json, os
for f in sorted(glob.glob('presets/*.json')):
    d = json.load(open(f, encoding='utf-8'))
    for s in d.get('video', []):
        if s.get('stage') == 'resize' and s.get('params', {}).get('dar'):
            print(os.path.basename(f)[:-5]); raise SystemExit
PY
)
if [ -n "$DARP" ]; then
  "$BIN" run --preset "presets/$DARP.json" --input "$IMG" --out-dir "$WORK" >/dev/null 2>&1
  out_img=$(ls -1S "$WORK"/*"${DARP}"*.png 2>/dev/null | head -1)
  if [ -z "$out_img" ]; then
    bad "$DARP 图片成品端到端" "没有产出文件"
  else
    got=$(dim "$out_img")
    want=$(planof "presets/$DARP.json" "$IMG" | python3 -c "import json,sys;d=json.load(sys.stdin)['out'];print(f\"{d['w']}x{d['h']}\")")
    sar=$(ffprobe -v error -select_streams v:0 -show_entries stream=sample_aspect_ratio -of csv=p=0 "$out_img" | tr -d ' \n')
    if [ "$got" != "$want" ]; then
      bad "$DARP 图片成品端到端" "计划 $want,实际 $got"
    elif [ -n "$sar" ] && [ "$sar" != "1:1" ] && [ "$sar" != "N/A" ]; then
      bad "$DARP 交付不再依赖 SAR" "文件里还写着 $sar"
    else
      ok "$DARP 图片成品端到端($got,无 SAR)"
    fi
  fi
else
  ok "没有带 DAR 的预设需要端到端验证"
fi

# 6) 预览代理:屏幕上那一格最宽约 900 px,按交付尺寸出图是白传。
#    2560×1439 的参考图:一对预览 9.25 MB / 1803 ms → 代理后 3.15 MB / 1638 ms。
CAP_EDGE=${REWIND_PREVIEW_MAX_EDGE:-1280}
BIG=${BIG:-assets/reference/reference.png}
if [ -f "$BIG" ]; then
  bj=$($BIN preview --preset presets/patina.json --input "$BIG" --out-dir "$WORK" --t 0 2>&1 | grep '^{' | tail -1)
  bs=$(printf '%s' "$bj" | jget "['source']"); br=$(printf '%s' "$bj" | jget "['result']")
  bpw=$(printf '%s' "$bj" | jget "['out'].get('preview_w') or 0")
  bdims=$(dim "$bs"); bddims=$(dim "$br")
  bbytes=$(( $(stat -c%s "$bs" 2>/dev/null || echo 0) + $(stat -c%s "$br" 2>/dev/null || echo 0) ))
  if [ "$bdims" != "$bddims" ]; then
    bad "大图预览两层仍同几何(代理后)" "src=$bdims result=$bddims"
  else
    ok "大图预览两层仍同几何($bdims)"
  fi
  LONG=$(( ${bdims%x*} > ${bdims#*x} ? ${bdims%x*} : ${bdims#*x} ))
  [ "$LONG" -le "$CAP_EDGE" ] && ok "大图预览长边 $LONG ≤ 上限 $CAP_EDGE" \
    || bad "大图预览长边受限" "实际 $LONG,上限 $CAP_EDGE(代理没生效)"
  [ "$bbytes" -lt 5242880 ] && ok "大图一对预览 $((bbytes/1024)) KB < 5 MB(改前 9246756 B)" \
    || bad "大图一对预览体积" "$((bbytes/1024)) KB"
  [ "${bpw:-0}" != "0" ] && ok "引擎回报 preview_w=$bpw(界面据此说明这是代理图)" \
    || bad "引擎回报 preview_w" "事件里缺这个字段"
else
  ok "跳过代理预览检查(没有 $BIG)"
fi

# 7) 缩略位契约:静图缩略一律 contain,cover 会把非 16:9 的画面裁掉
CROP=$(python3 - <<'PY'
import re
css = open('app/ui/style.css', encoding='utf-8').read()
want = ['.pcard img ', '.sample img ', '.done-card img ']
hits = []
for w in want:
    for m in re.finditer(re.escape(w) + r'\s*\{([^}]*)\}', css):
        if 'object-fit: cover' in m.group(1):
            hits.append(w.strip())
print(' '.join(sorted(set(hits))))
PY
)
[ -z "$CROP" ] && ok "缩略位不裁切画面(contain)" || bad "缩略位不裁切画面(contain)" "$CROP 还在用 object-fit: cover"

echo
echo "GEO CHECK: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
