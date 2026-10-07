#!/usr/bin/env bash
# ntsc 信号级旋钮闸:证明"接上了"而不是"清单里有"。
#
# 五条性质,全部只依赖同一台机器上的两次渲染对比(不比跨机 md5,ffmpeg 版本一变就假红):
#   ① 清单里 ntsc_vhs 必须有 seed + 12 个旋钮;
#   ② 把 12 个旋钮全钉成清单默认值,成品必须与"什么都不钉"逐字节相同(接入不许改默认);
#   ③ 每个旋钮单独拧动都必须改变画面(改不动 = 死控件,历史上真有过一次);
#   ④ 越界与未知键必须被清单拦住并带稳定码;
#   ⑤ 界面档位不许顺手变成 CLI 的硬上限(两级区间必须各管各的)。
# 用法: bash scripts/ntsc_check.sh
set -uo pipefail
cd "$(dirname "$0")/.."
. "$(dirname "$0")/portable.sh"   # fsize / md5of / md5pipe —— 三端工具差异收在这层
ROOT=$PWD
BIN=$ROOT/core/target/release/rewind-core
[ -x "$BIN" ] || BIN=$ROOT/core/target/release/rewind-core.exe
[ -x "$BIN" ] || { echo "FATAL 先 cargo build --release core"; exit 2; }
SRC=${SRC:-$ROOT/assets/sample/street-food.mp4}
[ -f "$SRC" ] || SRC=$ROOT/fixtures/test_src.mp4
WORK=$ROOT/.work/gate/ntsc
rm -rf "$WORK"; mkdir -p "$WORK"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf 'PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf 'FAIL  %s  <- %s\n' "$1" "$2"; }

# 代理预览:小画幅跑同一条像素路径,单帧对比足够判定"变没变"
shot() {  # shot <名字> [override...]  → 打印成品帧 md5
  local name=$1; shift
  local dir="$WORK/$name"
  mkdir -p "$dir"
  local args=()
  local kv
  for kv in "$@"; do args+=(--override "$kv"); done
  REWIND_PREVIEW_MAX_EDGE=400 $BIN preview --preset "$PRESET" --input "$SRC" \
    --out-dir "$dir" --t 2 "${args[@]}" >/dev/null 2>&1 || { echo ""; return 1; }
  local f
  f=$(ls "$dir"/*ntscrs*.png 2>/dev/null | head -1)
  [ -n "$f" ] || { echo ""; return 1; }
  md5of "$f"
}

PRESET=$ROOT/presets/vhs1990_ntscrs.json

echo "=== ① 清单对账"
NKEYS=$($BIN describe | python3 -c '
import json,sys
m=json.load(sys.stdin)
st=[s for s in m["video"] if s["stage"]=="ntsc_vhs"][0]
print(len(st["params"]))' 2>/dev/null)
[ "$NKEYS" = "13" ] && ok "ntsc_vhs 有 seed + 12 个旋钮" || bad "ntsc_vhs 参数数" "清单给了 ${NKEYS:-空},要 13"

echo "=== ② 默认值不许改成品"
base=$(shot base) || base=""
[ -n "$base" ] || { echo "FATAL 基线预览没出图(检查 ffmpeg 与 $SRC)"; exit 2; }
# 默认值直接从清单读,不手抄:手抄表会漂——清单改了值,闸还拿着旧数字绿着,那是假绿
mapfile -t pin < <($BIN describe | python3 -c '
import json,sys
m=json.load(sys.stdin)
st=[s for s in m["video"] if s["stage"]=="ntsc_vhs"][0]
for p in st["params"]:
    k=p["key"]
    if k=="seed": continue
    d=p["default"]
    print("ntsc_vhs." + k + "=" + (repr(d) if isinstance(d,float) else str(d)))' 2>/dev/null)
declare -A DEF=()
for kv in "${pin[@]}"; do t=${kv#ntsc_vhs.}; DEF[${t%%=*}]=${t#*=}; done
NPIN=${#pin[@]}
[ "$NPIN" = "12" ] && ok "清单给出 12 个旋钮默认值(不含 seed)" || bad "清单旋钮数" "读到 $NPIN,要 12"
pinned=$(shot pinned "${pin[@]}")
same=0
for kv in "${pin[@]}"; do
  one=$(shot "one_${kv#ntsc_vhs.}" "$kv")
  [ "$one" = "$base" ] && same=$((same+1))
done
[ "$pinned" = "$base" ] && ok "12 个默认值一起钉住 = 与基线逐字节相同" \
                        || bad "默认值钉住后成品变了" "$base → ${pinned:-没出图}"
[ $same -eq "$NPIN" ] && ok "每个默认值单独钉住都不改成品" \
                     || bad "有些默认值一改成品就变" "$same/$NPIN 与基线相同"
again=$(shot base_again)
[ "$again" = "$base" ] && ok "同参数两次渲染可复现" || bad "渲染不可复现" "$base vs $again"

echo "=== ③ 每个旋钮必须真的改变画面"
# 每个旋钮挑一个明显偏离默认、且**在界面档位内**的值(方向按物理意义挑,只要"看得见变化"就算接上)
declare -A RANGE=()
while read -r k lo hi; do RANGE[$k]="$lo $hi"; done < <($BIN describe | python3 -c '
import json,sys
m=json.load(sys.stdin)
st=[s for s in m["video"] if s["stage"]=="ntsc_vhs"][0]
for p in st["params"]:
    if p["key"]=="seed": continue
    print(p["key"], p["min"], p["max"])')
declare -A PROBE=(
  [vhs_tape_speed]=1 [vhs_chroma_loss]=0.012 [vhs_sharpen]=3 [vhs_edge_wave]=9
  [tracking_noise_height]=44 [tracking_noise_wave_intensity]=40 [head_switching_height]=20
  [head_switching_horizontal_shift]=-40 [snow]=0.2 [luma_smear]=0.05
  [chroma_delay_horizontal]=14 [chroma_delay_vertical]=7
)
for key in "${!PROBE[@]}"; do
  v=${PROBE[$key]}
  [ "${DEF[$key]:-}" = "$v" ] && { bad "$key 探针值" "探针等于默认值,判不出接没接上"; continue; }
  read -r lo hi <<<"${RANGE[$key]:-0 0}"
  ins=$(awk -v v="$v" -v lo="$lo" -v hi="$hi" 'BEGIN{print (v>=lo && v<=hi) ? "y" : "n"}')
  [ "$ins" = "y" ] || { bad "$key 探针值 $v" "在界面档位 $lo–$hi 外,闸在测用户够不到的东西"; continue; }
  got=$(shot "k_$key" "ntsc_vhs.$key=$v")
  [ -z "$got" ] && { bad "$key" "没出图"; continue; }
  [ "$got" = "$base" ] && bad "$key=$v 拧了没反应" "与基线逐字节相同(死控件)" || ok "$key=$v 改变了画面"
done

echo "=== ④ 越界与未知键必须被拦"
oob=$($BIN plan --preset "$PRESET" --input "$SRC" --override ntsc_vhs.vhs_tape_speed=9 2>&1 | tail -1)
case "$oob" in *"[param.range]"*) ok "越界带 param.range 码" ;; *) bad "越界未被拦" "$oob" ;; esac
unk=$($BIN plan --preset "$PRESET" --input "$SRC" --override ntsc_vhs.nope=1 2>&1 | tail -1)
case "$unk" in *"[param.unknown]"*) ok "未知键带 param.unknown 码" ;; *) bad "未知键未被拦" "$unk" ;; esac

echo "=== ⑤ 界面档位与引擎硬上限是分开的两级"
# 雪花:滑杆只给到 0.5(实测 32.5dB,满屏雪但认得出画面),引擎硬上限 100。
# 两级写成一级会有两种坏结果:CLI 够不到重噪声,或者界面拉成没人能选的刻度。
ovout=$($BIN plan --preset "$PRESET" --input "$SRC" --override ntsc_vhs.snow=0.6 2>&1); rc=$?
case "$ovout" in
  *"[param.range]"*) bad "CLI 被滑杆档位拦住" "snow=0.6 在硬上限内却挨了 param.range" ;;
  *) [ $rc -eq 0 ] && ok "超出滑杆上限、仍在硬上限内的值 CLI 可用" || bad "CLI 用 snow=0.6 失败" "$(echo "$ovout" | tail -1)" ;;
esac
hard=$($BIN plan --preset "$PRESET" --input "$SRC" --override ntsc_vhs.snow=1000 2>&1 | tail -1)
case "$hard" in *"[param.range]"*) ok "越过硬上限仍被 param.range 拦住" ;; *) bad "硬上限没拦住" "$hard" ;; esac

echo
echo "NTSC CHECK: PASS=$PASS FAIL=$FAIL"
[ $FAIL -eq 0 ] || exit 1
