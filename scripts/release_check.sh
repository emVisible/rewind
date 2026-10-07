#!/usr/bin/env bash
# 发布前自检(开源上线的硬门槛)。CI 里作为 job 跑,本地发版前手动跑一次。
# 用法: bash scripts/release_check.sh
set -uo pipefail
cd "$(dirname "$0")/.."

PASS=0; FAIL=0
if [ -x core/target/release/rewind-core ]; then BIN=core/target/release/rewind-core; else BIN=core/target/release/rewind-core.exe; fi
ok()  { PASS=$((PASS+1)); printf 'PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf 'FAIL  %s  <- %s\n' "$1" "$2"; }
has() { grep -qi -- "$2" "$1" 2>/dev/null; }

echo "=== 许可与声明 ==="
[ -f LICENSE ] && has LICENSE "Permission is hereby granted" \
  && ok "LICENSE(MIT 全文)" || bad "LICENSE(MIT 全文)" "缺失或未含 MIT 授权段"
[ -f THIRD-PARTY-NOTICES.md ] && has THIRD-PARTY-NOTICES.md "FFmpeg" \
  && ok "第三方声明含 FFmpeg" || bad "第三方声明含 FFmpeg" "缺文件或无 FFmpeg 段"
has THIRD-PARTY-NOTICES.md "ntsc-rs" && ok "第三方声明含 ntsc-rs" || bad "第三方声明含 ntsc-rs" "缺条目"
has THIRD-PARTY-NOTICES.md "专有" && ok "第三方声明写明 ALH Pro 红线" || bad "第三方声明写明 ALH Pro 红线" "缺条目"
# 内联进仓库的第三方源码:再分发必须带许可全文(三选一也不例外),且目录里不许混非源码资产。
VN=$(find vendor -type d -name target -prune -o -type f -print 2>/dev/null | wc -l)
if [ "$VN" -eq 0 ]; then
  bad "vendor 内联源码在位" "vendor/ 里没有文件 —— 内联方式变了?这两条闸要跟着改,不许让它恒绿"
else
  VL=$(find vendor -type d -name target -prune -o -type f \( -iname "LICENSE*" -o -iname "COPYING*" \) -print | head -1)
  if [ -n "$VL" ]; then
    ok "vendor 带上游许可全文($VL)"
  else
    bad "vendor 带上游许可全文" "vendor/ 里没有 LICENSE-* —— 本机 GitHub/crates.io 被拦取不到,补齐命令见 THIRD-PARTY-NOTICES.md §2.1"
    # 缺着就得承认缺着:声明不许写"已保留"而仓库里没有
    has THIRD-PARTY-NOTICES.md "待补" && ok "许可缺失在声明里写明" || bad "许可缺失在声明里写明" "vendor 缺全文,但 THIRD-PARTY-NOTICES.md 没承认(§2.1)"
  fi
  VBIN=$(find vendor -type d -name target -prune -o -type f ! -name "*.rs" ! -name "*.toml" ! -name "*.md" ! -name "Cargo.lock" ! -iname "LICENSE*" ! -iname "*COPYING*" -print | head -3)
  [ -z "$VBIN" ] && ok "vendor 内只有源码与清单(无非源码资产)" || bad "vendor 内只有源码与清单" "混进了素材/二进制:$(echo "$VBIN" | tr '\n' ' ')"
fi
[ -f CHANGELOG.md ] && has CHANGELOG.md "Keep a Changelog" \
  && ok "CHANGELOG(Keep a Changelog 格式)" || bad "CHANGELOG(Keep a Changelog 格式)" "缺失"

echo "=== 版本一致性 ==="
V_CORE=$(sed -n 's/^version = "\([^"]*\)"/\1/p' core/Cargo.toml | head -1)
V_SHELL=$(sed -n 's/^version = "\([^"]*\)"/\1/p' shell/Cargo.toml | head -1)
V_APP=$(sed -n 's/^version = "\([^"]*\)"/\1/p' app/Cargo.toml | head -1)
if [ -n "$V_CORE" ] && [ "$V_CORE" = "$V_SHELL" ] && [ "$V_CORE" = "$V_APP" ]; then
  ok "三个 crate 版本一致($V_CORE)"
else
  bad "三个 crate 版本一致" "core=$V_CORE shell=$V_SHELL app=$V_APP"
fi
grep -rq 'WATERMARK: &str = "Rewind v0' core/src 2>/dev/null \
  && bad "水印版本号不写死" "ffgraph 里仍有硬编码版本常量" \
  || ok "水印版本号不写死(取自 CARGO_PKG_VERSION)"

echo "=== 素材与界面资产 ==="
miss=""
NPRES=$(ls presets/*.json 2>/dev/null | wc -l)
[ "$NPRES" -ge 11 ] && ok "预设文件数 $NPRES ≥ 11" || bad "预设文件数" "只有 $NPRES 个"
# 画廊卡数由 catalog 说了算:变体(带 variant_of)不占卡 —— 用户明确要求"两个 VHS 合成一个类型参数"
if [ -x "$BIN" ]; then
  mkdir -p .work/gate/release
  "$BIN" catalog > .work/gate/release/catalog.json 2>/dev/null
  NCARD=$(python3 -c "import json;print(sum(1 for e in json.load(open('.work/gate/release/catalog.json')) if e.get('card')))" 2>/dev/null || echo 0)
  NVAR=$(python3 -c "import json;d=json.load(open('.work/gate/release/catalog.json'));print(sum(len(e.get('variants') or []) for e in d))" 2>/dev/null || echo 0)
  LEAK=$(python3 -c "import json;print(sum(1 for e in json.load(open('.work/gate/release/catalog.json')) if e.get('card') and e['id'].startswith('era_')))" 2>/dev/null || echo -1)
  [ "$NCARD" -ge 10 ] && ok "画廊卡数 $NCARD ≥ 10(年代轴产物与变体都不占卡)" || bad "画廊卡数" "只有 $NCARD 张"
  [ "$NVAR" -ge 1 ] && ok "类型变体在 catalog 里可见($NVAR 个)" || bad "类型变体在 catalog 里可见" "一个都没有,variant_of 没被兑现"
  [ "$LEAK" = "0" ] && ok "年代轴产物没漏进预设卡" || bad "年代轴产物没漏进预设卡" "$LEAK 个 era_* 被当成卡"
fi
for P in presets/*.json; do
  id=$(basename "$P" .json)
  [ -s "app/ui/gallery/$id.png" ] || miss="$miss $id.png"
  [ -s "app/ui/gallery/${id}_full.png" ] || miss="$miss ${id}_full.png"
  [ -s "app/ui/gallery/${id}_hero.webp" ] || miss="$miss ${id}_hero.webp"
  [ -s "app/ui/gallery/$id.webm" ] || miss="$miss $id.webm"
done
[ -z "$miss" ] && ok "每个预设都有瓦片/整帧/大图/动图四类资产" || bad "四类画廊资产" "缺:$miss"
for f in app/ui/index.html app/ui/app.js app/ui/style.css app/ui/shim.js; do
  [ -s "$f" ] || bad "UI 文件 $f" "缺失或为空"
done
[ -s app/ui/shim.js ] && ok "UI 双形态适配层在位(shim.js)"
# app/ui 整个目录是 Tauri 的 frontendDist —— 它等于"发布路径"。
# 调试残图曾经躺在 app/ui/dev_assets/ 里跟着安装包一起发出去,所以用白名单钉住顶层条目:
# 白名单而不是黑名单,否则下次长出来的目录还是看不见
UIEXTRA=$(ls -1 app/ui | grep -vE '^(app\.js|i18n\.js|index\.html|shim\.js|style\.css|favicon\.ico|gallery|img)$' | tr '\n' ' ')
[ -z "$UIEXTRA" ] && ok "app/ui 顶层只有登记的发布条目" || bad "app/ui 顶层只有登记的发布条目" "多出来的:$UIEXTRA"
# 界面上每个能点的按钮都必须有监听:加过控件却忘了接线,用户点下去就是死按钮(门槛/确认类控件尤其致命)
NOWIRE=""
for bid in $(grep -o '<button[^>]*id="[a-z0-9-]*"' app/ui/index.html | sed 's/.*id="//;s/"//'); do
  grep -q "'$bid'" app/ui/app.js || NOWIRE="$NOWIRE $bid"
done
[ -z "$NOWIRE" ] && ok "界面按钮全部有监听" || bad "界面按钮全部有监听" "未接线:$NOWIRE"
# 界面全靠 el.hidden 切状态,而作者样式里的 display:flex 会盖掉 UA 的 [hidden]{display:none}
# —— 少了这条兜底,弹窗/画布/进度会全部同屏且关不掉(实测踩过)
has app/ui/style.css "\[hidden\]" && ok "CSS 有 [hidden] 兜底规则" || bad "CSS 有 [hidden] 兜底规则" "style.css 缺 display:none!important"
# 层级契约:左栏是"日常层",只放选预设 + 强度 + 系数 + 引擎标为一级的那几组。
# 年代轴/偏色一旦回到左栏,首屏就又变回控制台(用户明确要求过"简单优先")
LEFT=$(python3 - <<'PY'
import io, re
h = io.open('app/ui/index.html', encoding='utf-8').read()
m = re.search(r'<aside class="rail" id="rail-left">(.*?)</aside>', h, re.S)
rail = m.group(1) if m else ''
if not m:
    print('MISSING')
else:
    print(' '.join(x for x in ('id="era"', 'id="cast"') if x in rail))
PY
)
case "$LEFT" in
  MISSING) bad "左栏只放日常参数" "index.html 里找不到 #rail-left" ;;
  '') ok "左栏只放日常参数(年代轴/偏色在高级参数里)" ;;
  *) bad "左栏只放日常参数" "又放回左栏了:$LEFT" ;;
esac
grep -q '~\${p\.era}' app/ui/app.js \
  && bad "预设卡不报年代" "app.js 还在往卡上写 ~年份" \
  || ok "预设卡不报年代"
# 可见文案写进 CSS content 就绕过了词典 —— 英文界面会漏出中文(这次的「· 默认」角标就是)
CJKCSS=$(python3 - <<'PY'
import io, re
s = io.open('app/ui/style.css', encoding='utf-8').read()
hits = [x for x in re.findall(r"content:\s*'([^']*)'", s) if re.search(r'[\u4e00-\u9fff]', x)]
print(' / '.join(hits))
PY
)
[ -z "$CJKCSS" ] && ok "CSS content 没有写死的中文文案" || bad "CSS content 没有写死的中文文案" "$CJKCSS(要走词典)"
# 静帧 vs 视频必须分岔:抽帧条、长素材门槛这两处都默认"素材有时间轴",图片进来就是逻辑错误
# (用户报的"图片也会抽帧"是第一条;第二条更阴 —— 门槛靠"看过一帧"解除,而图片没有帧可看)
python3 - > /tmp/shape.txt <<'PY'
import io, re
src = io.open('app/ui/app.js', encoding='utf-8').read()

def body(name):
    """三种写法都要认:function f(…) {…} / const f = (…) => {…} / const f = (…) => 表达式;
    取不到就返回 None —— 找不到本身必须是 FAIL,静默跳过会让整组断言恒真。"""
    m = re.search(r'\b(?:function\s+' + name + r'|const\s+' + name + r')\b', src)
    if not m:
        return None
    seg = src[m.start():]
    brace, semi = seg.find('{'), seg.find(';')
    if brace < 0 or (0 <= semi < brace):
        return seg[:semi if semi >= 0 else 200]
    d = 0
    for j in range(brace, len(seg)):
        if seg[j] == '{':
            d += 1
        elif seg[j] == '}':
            d -= 1
            if d == 0:
                return seg[brace:j + 1]
    return None

out = []
for name, must, why in [
    ('sampleable', ['is_image'], '抽帧资格必须看素材类型'),
    ('refreshSamples', ['sampleable'], '抽帧条的显隐走同一个判据'),
    ('requestSamples', ['sampleable'], '真正的抽帧请求有同一道前置'),
    ('needsConfirm', ['is_image'], '门槛不能落在静帧上(没有"看过一帧"这条路)'),
]:
    b = body(name)
    if b is None:
        out.append('FAIL|%s 在 app.js 里找不到|%s' % (name, why))
    elif not all(k in b for k in must):
        out.append('FAIL|%s 没引用 %s|%s' % (name, '/'.join(m for m in must if m not in b), why))
    else:
        out.append('PASS|静帧/视频分岔:%s|' % name)
# 抽帧条属于"哪一项素材":只按"条里非空"判断会不会重抽,换素材时就会一直挂着上一项的帧
b = body('refreshSamples') or ''
if 'samplesKey' in b:
    out.append('PASS|抽帧条按素材身份判归属|')
else:
    out.append('FAIL|refreshSamples 只看条里非空|换素材时会一直挂着上一项的帧')
# 进度遮罩的归属:预览与跑批共用 #busy。预览收尾若不问"谁在说话",就会把正在跑批的遮罩藏掉。
# 实测:撤掉守卫后,一次 9 趟任务里遮罩只在 19/176 次采样(11%)可见,数字在看不见的地方跑到 89%。
b = body('doPreview') or ''
hides = [l.strip() for l in b.splitlines() if 'els.busy.hidden = true' in l]
if not hides:
    out.append('FAIL|doPreview 里找不到藏遮罩的语句|结构变了,这条断言得人工看一眼')
elif all('state.running' in l for l in hides):
    out.append('PASS|预览收尾不会藏掉跑批中的进度遮罩|')
else:
    out.append('FAIL|doPreview 无条件 els.busy.hidden = true|跑批中的遮罩会被预览藏掉(数字还在更新但看不见)')
# 界面必须画总体读数:画本趟 pct 就等于把 0→100% 扫 steps 遍(用户报的"进度在撒谎")
if re.search(r'ev\.overall', src):
    out.append('PASS|进度读数取 ev.overall(总体)而非本趟 pct|')
else:
    out.append('FAIL|进度读数没用 ev.overall|画本趟百分比会把 0→100% 扫好几遍')
# 引擎发的是 `[稳定码] 中文原文`,界面必须过唯一的解码点 msg(),否则英文界面直接露中文、
# 中文界面还会多看见一个 `[engine.pass]`。这两处漏网是真找到过的:素材条与跑批 item 事件
# 把 p.error / error 原样塞进了词典值 —— 新增错误显示时最容易重演的形状。
if re.search(r'function msg\([\s\S]{0,240}?I18N\.decodeError', src):
    out.append('PASS|错误文本唯一出口 msg() 会解稳定码|')
else:
    out.append('FAIL|msg() 不再走 I18N.decodeError|引擎错误会以中文+码直接糊到界面上')
leaked = re.findall(r'\{ *e: *((?:p|ev|e|it)\.error|error) *\}', src)
if leaked:
    out.append('FAIL|错误字段绕过了 msg()|%s' % ', '.join(sorted(set(leaked))))
else:
    out.append('PASS|没有把引擎错误字段直接塞进词典值|')
# 跑批结果的归属:item 事件里的 index 是**批次位**(单件跑批恒为 0),只有再翻录传的是队列位。
# 直接 state.items[index] 会把第二项的成品记到第一项头上(实测过:焦点在第二项时 ✓ 落在第一项)。
m = re.search(r"listen\('rewind://item'.*?\n\}\);", src, re.S)
if m and 'state.job.item' in m.group(0):
    out.append('PASS|跑批结果按"谁发起"归属|')
else:
    out.append('FAIL|item 事件按队列下标捞结果|单件跑批 index 恒为 0 → 成品与完成卡会落到队列第一项')
print('\n'.join(out))
PY
while IFS='|' read -r K D W; do
  case "$K" in
    PASS) ok "$D" ;;
    FAIL) bad "$D" "$W" ;;
  esac
done < /tmp/shape.txt
# 引擎里"会走到用户眼前"的失败文本必须带稳定码(管住以后新加的错误)。
# 判据绑在内容上(含汉字的字符串字面量 + 造错误的形状),不绑在某个具体写法上:
# 同族闸第一版用 contains("Err("),被 Err::<(), _>("中文") 绕过,注入测试照样绿 —— 那才是真事故。
UNCODED=$(python3 - <<'PY'
import io, os, re
bad = []
ERR_SHAPE = re.compile(r'((?<!::)\bErr\b|\.ok_or\(|map_err\(|or_else\(|\bbail!)')
CJK_LIT = re.compile(r'"[^"]*[\u4e00-\u9fff]')
# serve 起不来时打的是终端日志(浏览器还没连上),这几句到不了界面,不要求带码
TO_TERMINAL_ONLY = ('TcpListener::bind', 'index.html', 'UI 目录不可用')
# 它包的是**已经带码**的内层错误;在这儿再套一层 coded() 等于把原来的码顶掉
WRAPS_CODED = ('构建预览计划失败',)
# 这是默认输出目录名(内容文本),不是失败消息
CONTENT_NOT_ERROR = ('Rewind输出',)
for fn in sorted(os.listdir('core/src')):
    if not fn.endswith('.rs') or fn == 'errcode.rs':
        continue
    body = io.open('core/src/' + fn, encoding='utf-8').read().split('#[cfg(test)]')[0]
    for i, raw in enumerate(body.splitlines(), 1):
        ln = raw.strip()
        if ln.startswith('//') or not ERR_SHAPE.search(ln) or not CJK_LIT.search(ln):
            continue
        if ('coded(' in ln or 'recode(' in ln or any(x in ln for x in TO_TERMINAL_ONLY)
                or any(x in ln for x in WRAPS_CODED) or any(x in ln for x in CONTENT_NOT_ERROR)):
            continue
        bad.append('%s:%d %s' % (fn, i, ln[:58]))
print(' / '.join(bad[:3]) if bad else '')
PY
)
[ -z "$UNCODED" ] && ok "引擎失败文本都带稳定码(新增错误不许裸中文)" \
                  || bad "引擎失败文本都带稳定码" "$UNCODED"
# 参数标签曾被三轨布局挤成"C…"。截断不是文案问题:标签必须整行显示,不许用 ellipsis 糊
grep -A3 '\.knob > span' app/ui/style.css | grep -q 'text-overflow: *ellipsis' \
  && bad "参数标签不得截断(.knob > span 无 ellipsis)" "又加回了 text-overflow:ellipsis" \
  || ok "参数标签不得截断(.knob > span 无 ellipsis)"
has app/ui/style.css "\-\-dur-1" && ok "动效令牌在位(--dur-*/--ease-*)" || bad "动效令牌" "缺 --dur-1"
has app/ui/style.css "::-webkit-scrollbar" && ok "滚动条已主题化" || bad "滚动条已主题化" "无 ::-webkit-scrollbar"
has app/ui/style.css "prefers-reduced-motion" && ok "支持 reduced-motion 降级" || bad "支持 reduced-motion" "缺媒体查询"
for i in app/icons/32x32.png app/icons/128x128.png app/icons/icon.ico; do
  [ -s "$i" ] || bad "图标 $i" "缺失(安装包需要)"
done
[ -s app/icons/32x32.png ] && [ -s app/icons/128x128.png ] && [ -s app/icons/icon.ico ] \
  && ok "图标齐全(32/128/ico)"

echo "=== 仓库卫生 ==="
# git 默认会把非 ASCII 路径写成 "docs/\345\217\202…" —— 前缀多了个引号,`^docs/` 这类锚点全部落空,
# 于是"路径过滤"静默变成"什么都不匹配"。所有读文件清单的检查都走 lsf()(强制不转义)。
lsf() { git -c core.quotePath=false ls-files; }
TRACKED=$(lsf | grep -i 'alh' | grep -v '^docs/' || true)
[ -z "$TRACKED" ] && ok "专有参考项目源码零入库(跟踪路径无 alh,docs/ 下的分析文档除外)" \
  || bad "专有参考项目源码零入库" "跟踪到了:$TRACKED"
ALHDOC=$(lsf | grep -i 'alh' | grep '^docs/' || true)
if [ -n "$ALHDOC" ]; then
  # 这条豁免只给"散文":这些文件里的每个代码块都必须是我们本人跑过的命令,不许是对方的源码
  BADB=$(python3 - $ALHDOC <<'PY'
import io, re, sys
bad = []
for p in sys.argv[1:]:
    s = io.open(p, encoding="utf-8").read()
    for blk in re.findall(r"```[^\n]*\n(.*?)```", s, re.S):
        first = next((l.strip() for l in blk.splitlines() if l.strip()), "")
        if first and not re.match(r"(grep|find|curl|git|awk|sed|ls|du|wc|stat|bash|python3|cargo|ffmpeg|ffprobe)\b", first):
            bad.append("%s: 代码块开头不是命令 —— %s" % (p, first[:46]))
print(" | ".join(bad))
PY
)
  [ -z "$BADB" ] && ok "ALH 分析文档只含命令与散文(无源码摘录)" || bad "ALH 分析文档只含命令与散文" "$BADB"
fi
INSIDE=$(find . -maxdepth 2 -type d -iname '*alh*' -not -path './.git*' || true)
[ -z "$INSIDE" ] && ok "参考源码目录已移出仓库" || bad "参考源码目录仍在仓库内" "$INSIDE"
SECRETS=$(lsf | grep -i -E '\.env$|id_rsa|\.pem$|credentials' || true)
[ -z "$SECRETS" ] && ok "无可疑凭据文件" || bad "无可疑凭据文件" "$SECRETS"
for l in core/Cargo.lock shell/Cargo.lock; do
  [ -s "$l" ] || bad "锁文件 $l" "缺失(CI 复现构建需要)"
done
[ -s core/Cargo.lock ] && [ -s shell/Cargo.lock ] && ok "锁文件在位(core/shell)"
if [ -s app/Cargo.lock ]; then ok "锁文件在位(app 桌面壳)"; else
  echo "SKIP  app/Cargo.lock —— 本机 crates.io 解析不通(tauri 依赖树),由 CI 首次构建生成后入库;打 tag 前必须补上"
fi
BIG=$(lsf | xargs -I{} sh -c 'test -f "{}" && [ $(stat -c%s "{}" 2>/dev/null || stat -f%z "{}") -gt 4194304 ] && echo {}' 2>/dev/null | head -3)
[ -z "$BIG" ] && ok "无 >4MB 文件入库" || bad "无 >4MB 文件入库" "$BIG"

echo "=== 品牌与示例资产"
SHIP_ASSETS="assets/brand/source-icon.png assets/reference/reference.png assets/reference/CREDITS.md \
         assets/sample/CREDITS.md assets/sample/street-food.mp4 assets/sample/street-food.png \
         app/ui/img/logo.png app/ui/img/favicon-32.png app/ui/img/apple-touch-icon.png app/ui/favicon.ico \
         app/icons/32x32.png app/icons/128x128.png app/icons/128x128@2x.png app/icons/icon.png app/icons/icon.ico"
for f in $SHIP_ASSETS; do
  [ -s "$f" ] || bad "资产在位" "缺 $f"
done
# 磁盘上有 ≠ 仓库里有。assets/ 整目录从未进过任何提交,而派生的 44 张画廊图在库里 ——
# clone 之后 preset_gallery.sh 只会 SKIP,陌生人烘不出画廊,CI 的 release_check 也会直接红。
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  UNTRACKED=""
  # 画廊四类资产也一起查:它们是"界面直接 <img src> 的东西",漏一个就是首屏裂图
  for f in $SHIP_ASSETS app/ui/gallery/*.png app/ui/gallery/*.webm app/ui/gallery/*.webp; do
    [ -e "$f" ] || continue
    git ls-files --error-unmatch "$f" >/dev/null 2>&1 || UNTRACKED="$UNTRACKED $f"
  done
  [ -z "$UNTRACKED" ] && ok "发布资产全部在 git 索引里" || bad "发布资产全部在 git 索引里" "磁盘有、索引没有:$(echo $UNTRACKED | tr ' ' '\n' | wc -l) 个:$(echo $UNTRACKED | cut -c1-160)"
fi
# 随包素材的许可状态不许停在"待确认":这类话一旦留在声明里,就是"自己知道自己要侵权还照发"
if has THIRD-PARTY-NOTICES.md "未确认" || has THIRD-PARTY-NOTICES.md "发布前必须解决"; then
  bad "随包素材许可状态已了结" "声明里还有未确认项 —— 取许可 / 换自有素材 / 从发布包里撤下,三选一,不许带病上线"
else
  ok "随包素材许可状态已了结(声明里没有未确认项)"
fi
[ -s assets/brand/source-icon.png ] && ok "品牌母图在位(派生图标由 scripts/brand_assets.sh 生成)"
has app/ui/index.html 'rel="icon"' && ok "favicon 已挂" || bad "favicon 已挂" "index.html 里没有 icon link"
has app/ui/index.html 'name="description"' && ok "meta description 在位" || bad "meta description" "缺"
has app/ui/index.html 'theme-color' && ok "theme-color 在位" || bad "theme-color" "缺"
has app/ui/index.html 'og:title' && ok "社交卡片 meta 在位" || bad "og meta" "缺"
# 示例素材必须是真实画面。量具:极端饱和像素占比 —— 实测 testsrc2 测试图 86%、SMPTE 色条纯原色 11.7%,
# 而照片级素材是 0%;只用"颜色种数"分不开(testsrc2 也有 741 种,离照片的 1864 不够远)。
SAMP=assets/sample/street-food.mp4
[ -s "$SAMP" ] && ok "示例素材在位(street-food.mp4)" || bad "示例素材" "缺 $SAMP"
if [ -s "$SAMP" ] && command -v python3 >/dev/null 2>&1; then
  STAT=$(ffmpeg -v error -i "$SAMP" -vf "select=eq(n\,30),scale=128:72,format=rgb24" -frames:v 1 -f rawvideo - 2>/dev/null | python3 -c '
import sys
d = sys.stdin.buffer.read(); n = max(1, len(d)//3)
px = [d[i:i+3] for i in range(0, len(d)-2, 3)]
pure = sum(1 for p in px if all(c in (0, 255) for c in p))
sat = sum(1 for p in px if max(p) - min(p) > 200)
print("%d %.1f %.1f" % (len(set(px)), 100*pure/n, 100*sat/n))')
  set -- $STAT
  COLORS=${1:-0}; CORNER=${2:-99}; SAT=${3:-99}
  echo "     量测: 颜色种数=$COLORS 纯原色=${CORNER}% 极端饱和=${SAT}%"
  awk "BEGIN{exit !($SAT<=25 && $CORNER<=3 && $COLORS>=800)}" \
    && ok "示例是真实画面而非测试图" \
    || bad "示例是真实画面而非测试图" "极端饱和 ${SAT}% / 纯原色 ${CORNER}% —— 色块这类素材看不出做旧效果"
  SDUR=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$SAMP" 2>/dev/null | cut -d. -f1)
  [ "${SDUR:-0}" -ge 6 ] && ok "示例时长 ${SDUR}s(够抽帧与门槛演示)" || bad "示例时长" "只有 ${SDUR:-0}s"
fi
awk '/pub fn sample_file/,/^}/' shell/src/lib.rs | grep -q 'fixtures/' \
  && bad "示例不该指回测试图" "sample_file 还引用着 fixtures 测试素材" || ok "sample_file 没指回测试图"

echo "=== 闸的完整性 ==="
GMISS=""
for s in regression release_check preset_gallery axis_check compare_sheets aging_check preset_variance conc_check geo_check safe_max ntsc_check param_liveness param_pixels param_audio; do
  [ -s "scripts/$s.sh" ] || GMISS="$GMISS $s.sh"
done
[ -s scripts/i18n_check.js ] || GMISS="$GMISS i18n_check.js"
[ -s scripts/preset_variance.py ] || GMISS="$GMISS preset_variance.py"
[ -s scripts/safe_max.py ] || GMISS="$GMISS safe_max.py(边界扫描台架)"
[ -s scripts/param_liveness.py ] || GMISS="$GMISS param_liveness.py(参数活性台架)"
[ -s scripts/param_pixels.py ] || GMISS="$GMISS param_pixels.py(像素活性台架)"
[ -s scripts/param_audio.py ] || GMISS="$GMISS param_audio.py(音频活性台架)"
[ -z "$GMISS" ] && ok "闸脚本齐全(回归/自检/画廊/量化/差异/做旧系数/并发/双语/画幅/边界/信号旋钮/参数活性/像素活性/音频活性)" || bad "闸脚本齐全" "缺:$GMISS"
for g in aging_check.sh preset_variance.sh conc_check.sh i18n_check.js geo_check.sh safe_max.sh ntsc_check.sh param_liveness.sh param_pixels.sh param_audio.sh; do
  has .github/workflows/build.yml "scripts/$g" && ok "CI 接入 $g" || bad "CI 接入 $g" "build.yml 里没有这一步"
done
[ -s fixtures/test_odd.mp4 ] && ok "奇数尺寸备测件在库" || bad "奇数尺寸备测件在库" "缺 fixtures/test_odd.mp4"
# .work 治理入口必须真的能跑:它报告"哪些能删、删了怎么回来",写坏了就等于没有
CW=$(bash scripts/clean_work.sh 2>&1 | tail -1)
case "$CW" in
  合计*) ok ".work 治理入口可跑($CW)" ;;
  *) bad ".work 治理入口可跑" "clean_work.sh 输出异常: $(echo "$CW" | cut -c1-60)" ;;
esac

echo "=== README(默认英文 + 中文一份) ==="
[ -f README_zh.md ] && ok "README_zh.md 在位" || bad "README_zh.md 在位" "缺中文版(README.md 是英文默认)"
# 两份必须互链:访客只会看到自己语言的那一份
has README.md "README_zh.md" && ok "EN README 链到中文" || bad "EN README 链到中文" "缺链接"
has README_zh.md "README.md" && ok "zh README 链到英文" || bad "zh README 链到英文" "缺链接"
for K in "Rewind" "serve" "--override" "MIT" "127.0.0.1" "telemetry" "THIRD-PARTY-NOTICES"; do
  has README.md "$K" && ok "README(EN) 含 $K" || bad "README(EN) 含 $K" "缺该项"
done
for K in "Rewind" "serve" "--override" "MIT" "127.0.0.1" "遥测" "THIRD-PARTY-NOTICES"; do
  has README_zh.md "$K" && ok "README(zh) 含 $K" || bad "README(zh) 含 $K" "缺该项"
done
# README 里的命令行例子必须是真命令:抽查几个子命令存在
for C in run era reclip preview describe plan batch; do
  ./core/target/release/rewind-core $C --help >/dev/null 2>&1 \
    && ok "README 例子用的子命令 $C 存在" || bad "README 例子用的子命令 $C 存在" "命令已不存在,README 要跟着改"
done

echo
echo "RELEASE CHECK: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" = 0 ] || echo "上面每一条 FAIL 都是上线阻塞项,修完再打 tag。"
exit $([ "$FAIL" = 0 ] && echo 0 || echo 1)
