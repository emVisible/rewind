#!/usr/bin/env bash
# 参数边界闸(#21):清单不许有倒挂范围;越界必须是"说人话的拒绝",不是 ffmpeg 的半句报错;
# 合法极值必须还能跑通。全部数字来自 scripts/safe_max.py 的实测扫描。
# 用法: bash scripts/safe_max.sh
set -u
cd "$(dirname "$0")/.."
BIN=${BIN:-core/target/release/rewind-core}
SRC=${SRC:-assets/sample/street-food.mp4}
W=.work/gate/safe_max
rm -rf "$W"; mkdir -p "$W"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf 'PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf 'FAIL  %s  <- %s\n' "$1" "$2"; }
[ -x "$BIN" ] || { echo "FATAL 没有 $BIN"; exit 2; }

echo "=== 1. 清单不变量:min 不得大于 max ==="
INV=$($BIN describe | python3 -c "
import json,sys
d=json.load(sys.stdin); bad=[]; seen=0; hard_seen=0
for sec in ('video','audio'):
    for st in d.get(sec) or []:
        for p in st.get('params') or []:
            # 字段就在参数对象顶层。曾经写成 p['bounds']['min'],于是这条断言永远空转
            b = p if ('min' in p or 'max' in p) else (p.get('bounds') or {})
            lo,hi=b.get('min'),b.get('max')
            if not (isinstance(lo,(int,float)) and isinstance(hi,(int,float))):
                continue
            seen+=1
            if lo>hi:
                bad.append('%s.%s min=%s>max=%s'%(st['stage'],p['key'],lo,hi))
            # 硬区间(拦 CLI 用的)也必须正序,且必须包住界面档位,否则 --override 会拦掉合法滑杆值
            hlo,hhi=p.get('hard_min'),p.get('hard_max')
            if isinstance(hlo,(int,float)) and isinstance(hhi,(int,float)):
                hard_seen+=1
                if hlo>hhi:
                    bad.append('%s.%s hard=%s>%s'%(st['stage'],p['key'],hlo,hhi))
                elif hlo>lo or hhi<hi:
                    bad.append('%s.%s 硬区间 %s-%s 没包住档位 %s-%s'%(st['stage'],p['key'],hlo,hhi,lo,hi))
print('SEEN=%d|HARD=%d|%s'%(seen,hard_seen,' '.join(bad)))")
SEEN=$(printf '%s' "$INV" | sed -n 's/^SEEN=\([0-9]*\)|.*/\1/p')
HARD=$(printf '%s' "$INV" | sed -n 's/^SEEN=[0-9]*|HARD=\([0-9]*\)|.*/\1/p')
REST=$(printf '%s' "$INV" | sed 's/^SEEN=[0-9]*|HARD=[0-9]*|//')
# 硬区间只有两个参数带(fps / level):读到 0 就说明清单没写进去,断言在空转
[ "${HARD:-0}" -ge 2 ] && ok "硬区间声明在清单里(读到 ${HARD} 个参数)" || bad "硬区间声明在清单里" "只读到 ${HARD:-0} 个"
# 空转自检:读了 0 个参数的"全绿"不算绿(这个坑本项目踩过三次,§8.1-47)
[ "${SEEN:-0}" -ge 40 ] && ok "清单不变量真在检查(读到 ${SEEN} 个带范围的参数)" \
  || bad "清单不变量真在检查" "只读到 ${SEEN:-0} 个带范围的参数,断言在空转"
[ -z "$REST" ] && ok "清单里没有倒挂范围" || bad "清单没有倒挂范围" "$REST"

echo
echo "=== 2. 越界必须被引擎拒绝,且拒绝要说人话 ==="
# 实测过的三个坑:fps=0 是 ffmpeg 解析错误、fps=1000 "成功"跑 84s/22.8MB、level=0 让 lutyuv 除零
reject() {  # $1=override  $2=期望片段
  local ov="$1" want="$2"
  local out rc
  out=$($BIN run --preset presets/patina.json --input "$SRC" --out-dir "$W" --override "$ov" 2>&1 | tail -1); rc=$?
  case "$out" in *"超出可用范围"*|*"过于激进"*) : ;; *) bad "拒绝 $ov" "没走人话路径: $(printf '%s' "$out" | cut -c1-70)"; return;; esac
  case "$out" in *"Parsed_"*|*"[fps @"*|*"Error when evaluat"*) bad "拒绝 $ov" "把 ffmpeg 原始报错丢给用户了"; return;; esac
  if [ -n "$want" ]; then
    case "$out" in *"$want"*) : ;; *) bad "拒绝 $ov" "报文里没提 $want: $(printf '%s' "$out" | cut -c1-70)"; return;; esac
  fi
  ok "拒绝 $ov(说清可用范围,没漏 ffmpeg 报错)"
}
reject "fps.fps=0"        "fps=0"
reject "fps.fps=1000"     "1–120"
reject "band_quantize.level=0" "band_quantize.level=0"
reject "band_quantize.level=65" "1–64"
reject "resize.overscan=0.2"    "0.85–1"

# 清单区间也必须挡 CLI/API:界面"画不出荒谬值"不等于引擎不会照办。
# 实测补这之前:`--override noise.alls=99999`、`resize.w=1` 一路放行并真的出了成品。
# 用 plan 而不是 run 来验:校验发生在加载阶段,不必为一条边界真跑一遍编码。
vreject() { # $1=override $2=期望片段
  local ov="$1" want="$2" out
  out=$($BIN plan --preset presets/patina.json --input "$SRC" --override "$ov" 2>&1 | tail -1)
  case "$out" in *"超出可用范围"*) : ;; *) bad "清单边界挡住 $ov" "没拦下来: $(printf '%s' "$out" | cut -c1-70)"; return;; esac
  case "$out" in *"$want"*) ok "清单边界挡住 $ov(报 $want)";; *) bad "清单边界挡住 $ov" "报文没提 $want: $(printf '%s' "$out" | cut -c1-70)";; esac
}
vreject "noise.alls=99999"  "0–100"
vreject "resize.w=1"        "16–16384"
vreject "fps.fps=240"       "1–120"
vreject "band_quantize.level=99" "1–64"
vakcept() { # $1=override $2=说明
  local ov="$1" label="$2" out
  out=$($BIN plan --preset presets/patina.json --input "$SRC" --override "$ov" 2>&1 | tail -1)
  case "$out" in *"超出可用范围"*) bad "合法值 $label 没被误挡" "被挡: $(printf '%s' "$out" | cut -c1-80)"; return;; esac
  case "$out" in *'"steps"'*) ok "合法值 $label 仍可出计划";; *) bad "合法值 $label 仍可出计划" "$(printf '%s' "$out" | cut -c1-80)";; esac
}
vakcept "resize.w=7680" "8K 宽(界面「源」会把素材原始宽高原样钉进来)"
vakcept "noise.alls=100" "alls=100(区间上端)"
vakcept "resize.w=16"    "w=16(区间下端)"
# 硬区间与界面档位是两回事:80 fps 不在任何档位上,但引擎能照办,不许当非法拦掉
vakcept "fps.fps=80"              "fps=80(档位外、硬区间内)"
vakcept "band_quantize.level=52"  "level=52(档位外、硬区间内)"

echo
echo "=== 3. 合法极值必须仍然跑通(收紧边界不许误伤) ==="
legal() {  # $1=override $2=说明
  local ov="$1" label="$2" out f
  out=$($BIN run --preset presets/patina.json --input "$SRC" --out-dir "$W" ${ov:+--override "$ov"} 2>&1 | tail -1)
  f=$(printf '%s' "$out" | sed -n 's/.*"output":"\([^"]*\)".*/\1/p')
  if [ -z "$f" ] || [ ! -s "$f" ]; then bad "合法值 $label" "没出成品: $(printf '%s' "$out" | cut -c1-60)"; return; fi
  if ! ffmpeg -v error -xerror -i "$f" -f null - >/dev/null 2>&1; then bad "合法值 $label" "成品解不开"; return; fi
  ok "合法值 $label 出片且可解码"
}
legal "fps.fps=60"                 "fps=60"
legal "fps.fps=5"                  "fps=5(档位最小)"
legal "band_quantize.level=48"     "level=48(档位最大)"
legal "band_quantize.level=8"      "level=8(档位最小)"
legal ""                           "默认参数"

echo
echo "=== 4. 边界不许误伤内置内容与年代轴 ==="
NFAIL=0
for p in presets/*.json; do
  $BIN plan --preset "$p" --input "$SRC" >/dev/null 2>&1 || { echo "   plan 失败: $p"; NFAIL=$((NFAIL+1)); }
done
mkdir -p "$W/era"
for y in 1965 1972 1978 1985 1990 1994 1998 2004 2010 2016 2026; do
  $BIN era "$y" --write "$W/era/e$y.json" >/dev/null 2>&1 \
    && $BIN plan --preset "$W/era/e$y.json" --input "$SRC" >/dev/null 2>&1 \
    || { echo "   年代轴 $y 出不了计划"; NFAIL=$((NFAIL+1)); }
done
[ "$NFAIL" = "0" ] && ok "11 预设 + 11 个年代产物全部可出计划" || bad "边界未误伤内置内容" "$NFAIL 个失败"

echo
echo "SAFE MAX CHECK: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" = "0" ] || exit 1
