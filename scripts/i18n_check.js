// 双语覆盖率闸:词典对齐 + 引擎字符串必须都有英文对译。
// 目的很具体 —— 以后加参数/加预设忘了翻译,这里直接红,而不是让英文界面漏出中文。
// 用法: node scripts/i18n_check.js
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const ROOT = path.join(__dirname, '..');
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');
let PASS = 0, FAIL = 0;
const ok = (m) => { PASS++; console.log('PASS  ' + m); };
const bad = (m, d) => { FAIL++; console.log('FAIL  ' + m + '  <- ' + d); };

// i18n.js 是给浏览器写的,给它三个够用的全局就能在 node 里跑起来
const win = {};
global.window = win;
global.document = { documentElement: {}, querySelector: () => null, querySelectorAll: () => [] };
global.navigator = { language: 'zh-CN' };
require(path.join(ROOT, 'app/ui/i18n.js'));
const I = win.__REWIND_I18N__;
if (!I) { console.log('FATAL  i18n.js 没挂上 window.__REWIND_I18N__'); process.exit(2); }

const appjs = read('app/ui/app.js');
const html = read('app/ui/index.html');
const src18n = read('app/ui/i18n.js');

// 1) 词典本身对齐:取一份 key 列表要靠 t() 的行为,所以换个办法 —— 直接读源码里的两个表
const dictSrc = (lang) => {
  const src = read('app/ui/i18n.js');
  const start = src.indexOf(lang + ': {');
  if (start < 0) throw new Error('词典缺 ' + lang);
  let i = src.indexOf('{', start), depth = 0, out = '';
  for (; i < src.length; i++) {
    const c = src[i];
    if (c === '{') depth++;
    if (c === '}') depth--;
    out += c;
    if (depth === 0) break;
  }
  return out;
};
// 键名允许下划线:错误码里有 upload.too_large 这种,漏了下划线就会把真实存在的键当成"缺翻译"
const dict = (lang) => new Set([...dictSrc(lang).matchAll(/'([a-z0-9._]+)':/g)].map((m) => m[1]));
const zh = dict('zh'), en = dict('en');
const onlyZh = [...zh].filter((k) => !en.has(k));
const onlyEn = [...en].filter((k) => !zh.has(k));
onlyZh.length ? bad('zh/en 词典键集一致', '只在 zh 里: ' + onlyZh.join(', ')) : ok(`zh/en 词典键集一致(${zh.size} 键)`);
onlyEn.length ? bad('反向也对齐', '只在 en 里: ' + onlyEn.join(', ')) : ok('en 没有多余键');
// 批量替换词典时最容易把中文值写进英文表(真出过一次:run.preset 两边都是「预设」)。
// 唯一合法的例外是语言开关:它要用对方语言自己的写法自称,英文界面上的按钮就该写「中文」。
const enCjk = [...dictSrc('en').matchAll(/'([a-z0-9.]+)': '([^']*[\u4e00-\u9fff][^']*)'/g)]
  .map((m) => ({ k: m[1], v: m[2] }))
  .filter((x) => x.k !== 'lang.switch')
  .map((x) => x.k + "='" + x.v + "'");
enCjk.length ? bad('en 词典不许混入中文值', enCjk.join(' / ')) : ok('en 词典无中文值(键名与英文文案对齐)');

// 2) 代码里用到的 key 必须存在
const used = new Set([
  ...[...appjs.matchAll(/\bt\(\s*'([a-z0-9.]+)'/g)].map((m) => m[1]),
  ...[...appjs.matchAll(/t\((\w+)\s*\?\s*'([a-z0-9.]+)'\s*:\s*'([a-z0-9.]+)'\)/g)].flatMap((m) => [m[2], m[3]]),
  ...[...html.matchAll(/data-i18n(?:-title|-aria|-alt)?="([a-z0-9.]+)"/g)].map((m) => m[1]),
]);
const missKey = [...used].filter((k) => !zh.has(k) || !en.has(k));
missKey.length ? bad('用到的 key 都在词典里', '缺: ' + missKey.join(', ')) : ok(`用到的 key 都在词典里(${used.size} 个)`);

// 3) 引擎字符串必须有英文:参数、stage、分组、控件、预设名、类型标签
const bin = fs.existsSync(path.join(ROOT, 'core/target/release/rewind-core'))
  ? 'core/target/release/rewind-core' : 'core/target/release/rewind-core.exe';
let manifest, catalog;
try {
  manifest = JSON.parse(execFileSync(bin, ['describe'], { cwd: ROOT, encoding: 'utf8' }));
  catalog = JSON.parse(execFileSync(bin, ['catalog'], { cwd: ROOT, encoding: 'utf8' }));
} catch (e) {
  bad('能调用 rewind-core', String(e.message).slice(0, 120));
  console.log('\nI18N CHECK: PASS=' + PASS + ' FAIL=' + FAIL);
  process.exit(1);
}
const g = I.glossary();
const wantParams = [];
const allParams = [];
for (const sec of ['video', 'audio']) for (const st of manifest[sec] || []) {
  wantParams.push(st.stage + '.*');
  for (const p of st.params || []) { wantParams.push(st.stage + '.' + p.key); allParams.push(p); }
}
const wantStages = [...new Set([...wantParams].filter((k) => k.endsWith('.*')).map((k) => k.slice(0, -2)))];
const missP = wantParams.filter((k) => !k.endsWith('.*') && !g.PARAM_EN[k]);
missP.length ? bad('每个参数都有英文标签(' + wantParams.length + ' 项)' , '缺: ' + missP.join(', ')) : ok(`每个参数都有英文标签(${wantParams.length} 项)`);
const missS = wantStages.filter((s) => !g.STAGE_EN[s]);
missS.length ? bad('每个 stage 都有英文名', '缺: ' + missS.join(', ')) : ok(`每个 stage 都有英文名(${wantStages.length} 个)`);
const missG = (manifest.groups || []).map((x) => x.id).filter((id) => !g.GROUP_EN[id]);
missG.length ? bad('每个分组都有英文名', '缺: ' + missG.join(', ')) : ok(`每个分组都有英文名(${(manifest.groups || []).length} 个)`);
const missC = (manifest.controls || []).map((c) => c.id).filter((id) => !g.CTRL_EN[id]);
missC.length ? bad('一级旋钮组都有英文名', '缺: ' + missC.join(', ')) : ok(`一级旋钮组都有英文名(${(manifest.controls || []).length} 个)`);

// 只有"能占一张卡"的预设需要人工起的英文名;年代轴产物是模板生成的(i18n 里走 era.name)
const presets = catalog.filter((e) => e.card).map((e) => e.id);
for (const e of catalog) for (const v of e.variants || []) presets.push(v.id);
const missPre = presets.filter((id) => !g.PRESET_EN[id]);
missPre.length ? bad('每个预设都有英文名', '缺: ' + missPre.join(', ')) : ok(`每个预设都有英文名(${presets.length} 个)`);
const labels = catalog.flatMap((e) => [e.variant, ...(e.variants || []).map((v) => v.variant)].filter(Boolean));
const missVar = labels.filter((l) => !g.VARIANT_EN[l]);
missVar.length ? bad('每个「类型」标签都有英文', '缺: ' + missVar.join(', ')) : ok(`每个「类型」标签都有英文(${labels.length} 个)`);

// 单位也是引擎发的中文:英文界面不许漏出「比例 / 行(0=无)」这种字(纯符号单位不用登记)
const allUnits = [...new Set(allParams.map((p) => p.unit || ''))];
const cjkUnits = allUnits.filter((u) => /[\u4e00-\u9fff]/.test(u));
const missU = cjkUnits.filter((u) => !g.UNIT_EN[u]);
missU.length ? bad('每个中文单位都有英文', '缺: ' + missU.join(', ')) : ok(`每个中文单位都有英文(${cjkUnits.length}/${allUnits.length} 个)`);
const orphanU = Object.keys(g.UNIT_EN).filter((u) => !allUnits.includes(u));
orphanU.length ? bad('单位表没有孤儿条目', '清单里没人用: ' + orphanU.join(', ')) : ok(`单位表没有孤儿条目(${Object.keys(g.UNIT_EN).length} 条)`);

// 4) 界面不许再留硬编码中文(注释除外):扫 app.js 的字符串字面量
//    行注释要剥掉,但只在 " // " 处剥 —— 正则在代码里就写成 /[\\/] 这种,别误伤
const body = appjs.split('\n')
  .filter((l) => !/^\s*(\/\/|\*|\/\*)/.test(l))
  .map((l) => l.split(' // ')[0])
  .join('\n');
const cjk = [...body.matchAll(/['"`]([^'"`\n]*[\u4e00-\u9fff][^'"`\n]*)['"`]/g)].map((m) => m[1]);
cjk.length ? bad('app.js 不许有硬编码中文', cjk.slice(0, 6).join(' / ')) : ok('app.js 无硬编码中文(全走词典)');

// 5) 品牌文案不许在四个地方各写一遍:HTML 标题与 Tauri 窗口标题必须等于词典里的 zh app.title
const zhTitle = [...src18n.matchAll(/'app\.title': '([^']+)'/g)][0][1];
const htmlTitle = (html.match(/<title>([^<]+)<\/title>/) || [])[1];
const winTitle = (fs.readFileSync(path.join(ROOT, 'app/tauri.conf.json'), 'utf8').match(/"title":\s*"([^"]+)"/) || [])[1];
htmlTitle === zhTitle && winTitle === zhTitle
  ? ok(`标题三处一致(${zhTitle})`)
  : bad('标题三处一致', `词典=${zhTitle} HTML=${htmlTitle} Tauri=${winTitle}`);
// 口语化且把能力锁死在单一年份的旧口号不许回来
const all = html + src18n + appjs + fs.readFileSync(path.join(ROOT, 'README.md'), 'utf8');
/退回 1995|back to 1995/.test(all) ? bad('不许出现锁死年份的旧口号', '还有残留') : ok('没有锁死年份的旧口号');

// 6) HTML 里的静态文案必须挂在 data-i18n 上。
//    这一条是被真实事故逼出来的:#clear-overrides 的词典值改了,元素却没有 data-i18n,
//    于是界面上一直显示旧词 —— 词典对齐、key 存在都查不出"元素根本没接上"。
const CJK = /[\u4e00-\u9fff]/;
const VOID = { IMG: 1, INPUT: 1, BR: 1, HR: 1, META: 1, LINK: 1, SOURCE: 1 };
const BODY = { TITLE: 1, STYLE: 1, SCRIPT: 1, TEXTAREA: 1 };
// setLang()/渲染函数会重写这些元素的文字,它们不需要 data-i18n
// (status/busy-stage/pro-toggle 是刻意只留 JS 一个写源的:它们同时有静态默认值与运行期文案,
//  挂上 data-i18n 就等于给换语言时一个"把跑批中的文案刷回默认值"的通道)
const DYNAMIC = new Set(['meta-desc', 'meta-og-title', 'meta-og-desc', 'int-label', 'aging-label', 'cast-label',
  'era-year', 'busy-pct', 'busy-stage', 'status', 'pro-toggle']);
const openTags = [...html.replace(/<!--[\s\S]*?-->/g, '').matchAll(/<(\w+)([^>]*)>/g)];
const attrGap = [];
for (const [, tag, attrs] of openTags) {
  for (const [a, d] of [['title', 'data-i18n-title'], ['aria-label', 'data-i18n-aria'], ['alt', 'data-i18n-alt']]) {
    const m = attrs.match(new RegExp(`(?:^|\\s)${a}="([^"]*)"`));
    if (m && CJK.test(m[1]) && !attrs.includes(d)) {
      attrGap.push(`<${tag} ${a}="${m[1]}">`);
    }
  }
}
// <title>/<meta> 由 setLang 统一改,文案一致性另有第 5 条盯
if (openTags.length < 40) bad('HTML 标签数够多(解析没跑偏)', `只解析到 ${openTags.length} 个开标签`);
attrGap.length ? bad('带中文的 title/aria-label/alt 都有 data-i18n-*', attrGap.join(' / '))
                : ok(`带中文的 title/aria-label/alt 都有 data-i18n-*(${openTags.length} 个开标签)`);

const textGap = [];
const stack = [];
for (const m of html.replace(/<!--[\s\S]*?-->/g, '').matchAll(/<(\/)?(\w+)([^>]*?)(\/?)>|([^<]+)/g)) {
  if (m[0][0] === '<') {
    const tag = m[2].toUpperCase();
    if (m[1]) {
      const i = stack.map((x) => x.tag).lastIndexOf(tag);
      if (i >= 0) stack.length = i; // HTML 里不该有未闭合标签,能弹就弹
      continue;
    }
    if (!m[4] && VOID[tag] !== 1) stack.push({ tag, attrs: m[3] });
    continue;
  }
  if (!CJK.test(m[5]) || !stack.length) continue;
  const cur = stack[stack.length - 1];
  if (BODY[cur.tag] === 1) continue;
  const idm = cur.attrs.match(/\bid="([^"]+)"/);
  if ((idm && DYNAMIC.has(idm[1])) || /(^|\s)data-i18n="/.test(cur.attrs)) continue;
  textGap.push(`<${cur.tag}${idm ? ' ' + idm[0] : ''}>${m[5].trim().slice(0, 20)}`);
}
textGap.length ? bad('HTML 静态中文文案都接了 data-i18n', textGap.join(' / '))
               : ok('HTML 静态中文文案都接了 data-i18n');

// 7) 引擎错误稳定码:码表 ↔ 双语词典 ↔ 真实解码行为(core/src/errcode.rs 是码的唯一出处)。
//    词典漏项的后果是"英文界面静默露中文",不崩也不报错 —— 只能在这里红。
const errrs = read('core/src/errcode.rs');
const tbl = (errrs.match(/pub const CODES[\s\S]*?\];/) || [''])[0];
const names = [...tbl.matchAll(/^\s*([A-Z][A-Z0-9_]*),/gm)].map((m) => m[1]);
const codes = names.map((n) => (errrs.match(new RegExp('pub const ' + n + ': &str = "([^"]+)"')) || [])[1]).filter(Boolean);
codes.length >= 18 && codes.length === names.length
  ? ok(`读到 CODES 码表(${codes.length} 个码,解析没跑偏)`)
  : bad('读到 CODES 码表', `常量名 ${names.length} 个 / 码值 ${codes.length} 个`);
const noZh = codes.filter((c) => !zh.has('err.' + c));
const noEn = codes.filter((c) => !en.has('err.' + c));
(noZh.length || noEn.length)
  ? bad('每个错误码都有双语项', 'zh 缺 ' + noZh.join(',') + ' / en 缺 ' + noEn.join(','))
  : ok(`每个错误码都有双语项(${codes.length} 个)`);
// 反方向:词典里 err.x.y 形状的键必须出自码表 —— 手写的孤儿键会伪装成"已经翻好了"
const codeSet = new Set(codes);
const orphan = [...zh].filter((k) => /^err\.[a-z0-9_]+\.[a-z0-9_]+$/.test(k) && !codeSet.has(k.slice(4)));
orphan.length ? bad('没有凭空多出的错误码键', orphan.join(', ')) : ok('错误码键全部出自码表');
// 行为:认识的码必须把码摘掉(界面不该看到 [engine.pass] 字样);不认识的整条原样给回
I.setLang('en');
const enOut = I.decodeError('[engine.pass] step2/3 趟执行失败 (exit Some(1))');
(enOut.indexOf('[engine.pass]') < 0 && /^A render pass failed:/.test(enOut) && enOut.indexOf('step2/3') >= 0)
  ? ok('英文按码换一句人话,细节保留') : bad('英文解码', enOut);
I.setLang('zh');
const zhOut = I.decodeError('[engine.pass] step2/3 趟执行失败');
zhOut === 'step2/3 趟执行失败' ? ok('中文只摘码,原文一字不动') : bad('中文解码', zhOut);
// CLI 转给 Web 的那条会带 "error: " 前缀且码在中间 —— 这是真实形状,不是构造出来的
const mid = I.decodeError('error: 探测输入失败: [engine.ffprobe] ffprobe 失败: /tmp/a.txt');
mid === '探测输入失败: ffprobe 失败: /tmp/a.txt' ? ok('码在中间也摘得掉,并去掉 error: 前缀') : bad('中间码', mid);
const kept = I.decodeError('[brand.new] 引擎新增的错误');
kept === '[brand.new] 引擎新增的错误' ? ok('码不在词典里就整条原样给回') : bad('未知码应原样', kept);
I.setLang('en');
const enFull = I.decodeError('[engine.disk_full] step2/3 趟执行失败');
(/^Disk full/.test(enFull) && enFull.indexOf('[') < 0) ? ok('盘满码在英文下说清理磁盘') : bad('盘满解码', enFull);
I.setLang('zh');

console.log('\nI18N CHECK: PASS=' + PASS + ' FAIL=' + FAIL);
process.exit(FAIL ? 1 : 0);
