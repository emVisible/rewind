const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const convertFileSrc = window.__TAURI__.core.convertFileSrc;

// 双语:i18n.js 先于本文件加载。t() 取界面文案,I18N.* 取引擎字符串的英文对译。
const I18N = window.__REWIND_I18N__;
const t = (key, vars) => I18N.t(key, vars);

const MEDIA_EXT = /\.(mp4|mov|mkv|avi|webm|m4v|wmv|flv|ts|mpg|mpeg|3gp|png|jpe?g|webp|bmp|gif|tiff?)$/i;
/// 默认预设:首屏选中它、列表里排第一、卡片标「默认」—— 三处共用一个常量,别再各写一遍 'patina'
const DEFAULT_PRESET = 'patina';
const $ = (id) => document.getElementById(id);

const els = {
  canvas: $('canvas'), hero: $('hero'), viewer: $('viewer'), doneCard: $('done-card'),
  drop: $('canvas'), fileInput: $('file-input'), pick: $('pick'), langToggle: $('lang-toggle'),
  gallery: $('gallery'), seedRow: $('seed-row'), era: $('era'), eraYear: $('era-year'),
  intensity: $('intensity'), intLabel: $('int-label'),
  aging: $('aging'), agingLabel: $('aging-label'), agingCost: $('aging-cost'), agingField: $('aging-field'),
  cast: $('cast'), castLabel: $('cast-label'),
  proToggle: $('pro-toggle'), workbench: $('workbench'), recipeChip: $('recipe-chip'),
  knobsBasic: $('knobs-basic'), knobsPro: $('knobs-pro'), clearOverrides: $('clear-overrides'),
  mediaBar: $('media-bar'), strip: $('strip'),
  wipe: $('wipe'), wipeSrc: $('wipe-src'), wipeOut: $('wipe-out'), wipeSlider: $('wipe-slider'),
  outGeo: $('out-geo'),
  busy: $('busy'), busyStage: $('busy-stage'), busyPct: $('busy-pct'),
  doneThumb: $('done-thumb'), doneName: $('done-name'), doneMeta: $('done-meta'),
  doneSave: $('done-save'), doneCopy: $('done-copy'), doneAgain: $('done-again'), doneNext: $('done-next'),
  start: $('start'), cancel: $('cancel'), reclip: $('reclip'), reroll: $('reroll'), status: $('status'),
  estimate: $('estimate'), swapItem: $('swap-item'), dropItem: $('drop-item'),
  samples: $('samples'), samplesStrip: $('samples-strip'), samplesHint: $('samples-hint'),
  samplesReroll: $('samples-reroll'), samplesConfirm: $('samples-confirm'),
  variantRow: $('variant-row'), variantOpts: $('variant-opts'),
  resultView: $('result-view'), resultVideo: $('result-video'), resultImg: $('result-img'),
  rvName: $('rv-name'), rvMeta: $('rv-meta'), rvBack: $('rv-back'), rvCompare: $('rv-compare'),
  doneView: $('done-view'),
  engineTag: $('engine-tag'), diag: $('diag'), about: $('about'), aboutModal: $('about-modal'),
  aboutBody: $('about-body'), aboutClose: $('about-close'), trySample: $('try-sample'),
};

const state = {
  presets: [],            // 引擎 catalog:{id,name,era,kind,aging,params,gallery,variants}
  favorites: new Set(),
  recents: [],
  log: [],                // 诊断信息(错误与关键上下文)
  manifest: null,
  preset: null,
  aging: 1,               // 做旧系数(几手);1 = 只用预设自带的趟数
  agingTouched: false,    // 用户手动调过之后,换预设不再自动改写它
  cast: 0,                // 偏色(绿)强度 0–1,默认 0 = 不偏色
  anchorT: null,          // 抽帧确认选中的时间点;没选则用默认预览时刻
  confirmed: false,       // 长素材门槛:看过至少一帧才允许开跑
  sampleSeq: 0,
  sampleSeed: 1,
  samplesError: null,     // 抽帧失败原因;门槛不能因为抽不到帧而死锁
  outGeo: null,           // 最近一次预览的交付画幅:{w,h,dar}(引擎报,界面不猜)
  outSrc: null,           // 对应素材的原始尺寸,用来判断"画幅到底变没变"
  samplesKey: null,       // 抽帧条当前属于哪一项 + 哪一批种子
  samplesLast: null,      // 最近一次的抽帧结果(带归属键),切语言时按新语言重画读数
  doneFor: null,          // 完成卡属于哪一项:下标会因删除而左移,所以存对象
  font: undefined,        // 引擎报的本机字体:null = 没有,画不出时间戳
  rate: null,             // 预览窗口实测吞吐(秒/秒素材),只是上界
  measured: null,         // 真跑完一趟之后的实测吞吐,同参数直接用它
  job: null,              // 本次任务:{key,t0,dur}
  items: [],              // {path,name,probe,out,thumb}
  focus: -1,
  running: false,
  overrides: new Map(),   // "stage.key" -> 字符串值
  mounted: new Set(),
  previewSeq: 0,
  previewTimer: null,
  reclipIdx: -1,
  justReclipped: false,
  version: '',
};

// ————————————————————————————————— 素材与焦点

function focused() {
  return state.focus >= 0 ? state.items[state.focus] : null;
}

/** 预览帧文件名已带参数指纹,URL 天然随参数变;Web 版再补一个 no-cache 用的 query。
 *  桌面壳的 asset: 协议不接受 query,所以只在 Web 侧加。 */
function url(p, v) {
  const u = convertFileSrc(p);
  return window.__REWIND_WEB__ && v ? `${u}?v=${encodeURIComponent(v)}` : u;
}

/** 长素材门槛:3 小时的片子不该一传完就闷头跑 */
const LONG_SECONDS = 120;
const LONG_BYTES = 200 * 1024 * 1024;
function needsConfirm(it) {
  // 静帧没有"时长"这个维度,主预览显示的就是那一帧的全部 —— 再要求"先看几帧"既做不到也没意义
  if (!it || !it.probe || it.probe.is_image) return false;
  return (it.probe.duration || 0) > LONG_SECONDS || (it.probe.size_bytes || 0) > LONG_BYTES;
}

/** 当前参数的一串键:实测吞吐只随它变,跟取哪一帧无关。源尺寸必须在键里 ——
 *  用 720p 量出来的吞吐去预估 4K,是系统性偏小(像素量差 4 倍) */
function paramKey() {
  const p = focused()?.probe;
  const size = p && p.width ? `${p.width}x${p.height}` : '0x0';
  return `${state.preset}|${state.aging}|${state.cast}|${currentIntensity()}|${size}|${overrideList().sort().join(',')}`;
}

/** 预估耗时。
 *  跑过同参数的活儿之后用真测出来的吞吐;没跑过只能用预览窗口的值,而预览只跑 1–1.5 秒,
 *  每步进程启动的固定开销摊不薄(本机实测:30 秒片子上界是真值的 1.6–7 倍),所以那条只能当上界说。 */
function estimateText(it) {
  if (!it || !it.probe) return '';
  const d = it.probe.duration || 0;
  if (!d) return '';
  const key = paramKey();
  const measured = state.measured && state.measured.key === key ? state.measured.perSecond : null;
  const bounded = state.rate && state.rate.key === key ? state.rate.value : null;
  const perSecond = measured ?? bounded;
  if (perSecond == null) return '';
  const sec = d * perSecond;
  const mins = sec / 60;
  const txt = mins < 1 ? t('unit.sec', { n: Math.max(5, Math.round(sec)) }) : t('unit.min.short', { n: +mins.toFixed(1) });
  return t(measured ? 'estimate.measured' : 'estimate.worst', { t: txt });
}

function refreshGate() {
  const it = focused();
  const need = needsConfirm(it);
  const gating = need && !state.confirmed;
  const ok = !!it && !state.running && !gating;
  els.start.disabled = !ok;
  els.reclip.disabled = !(it && it.out) || state.running;
  if (els.samplesConfirm) els.samplesConfirm.hidden = !gating;
  if (els.samples) els.samples.classList.toggle('gating', gating);
  if (gating) setStatus(t('status.gate', { dur: fmtDur(it.probe?.duration), size: fmtBytes(it.probe?.size_bytes) }));
  if (els.estimate) {
    let extra = gating ? t('estimate.gate') : '';
    // 没字体的话时间戳根本不会出现 —— 不说出来,用户只会以为"这个预设没效果"
    if (state.font === null && presetNeedsFont()) extra += ' · ' + t('warn.nofont');
    els.estimate.textContent = it ? estimateText(it) + extra : '';
  }
}

function setFocus(i) {
  state.focus = i;
  const it = focused();
  state.anchorT = null;
  state.confirmed = false;
  els.hero.hidden = !!it;
  els.viewer.hidden = !it;
  els.doneCard.hidden = true;
  closeResult();
  setStatus(overridesText());
  renderStrip();
  renderMediaBar();
  refreshReadouts();
  refreshSamples();
  refreshGate();
  if (it) requestPreview(120);
}

/** 移除一个素材:运行中不许动正在跑的那一项 */
function removeItem(i) {
  if (state.running && i === state.focus) {
    setStatus(t('run.busyRemove'));
    return;
  }
  state.items.splice(i, 1);
  // 队列少了一项,所有"按下标记着的意图"必须一起左移:再翻录的目标、更换素材的槽位。
  // 漏掉哪一个,下一次结果就会记到别人头上(与 item 事件归属同一族缺陷)。
  if (state.reclipIdx >= 0 && i <= state.reclipIdx) state.reclipIdx -= 1;
  if (state.replacing) {
    if (state.replacing.index === i) state.replacing = null;
    else if (state.replacing.index > i) state.replacing.index -= 1;
  }
  // 焦点与所有"这一项的状态"必须一起重置 —— 手写一遍重置清单迟早漏一项
  // (漏过的两次:门槛面板永久隐藏无法自救、confirmed 留着上一项的"已看过")
  setFocus(state.items.length ? Math.min(i, state.items.length - 1) : -1);
  setStatus(focused() ? t('queue.removed', { name: focused().name }) : t('queue.cleared'));
  persist();
}

async function addFiles(paths) {
  let added = 0;
  for (const p of paths) {
    if (!MEDIA_EXT.test(p) || state.items.some((it) => it.path === p)) continue;
    const it = { path: p, name: p.split(/[\\/]/).pop(), probe: null, out: null };
    state.items.push(it);
    added++;
    probeItem(it);
  }
  if (!added) return;
  if (state.focus < 0) setFocus(state.items.length - 1);
  else renderStrip();
}

/** 探测是异步的:probe 落地后必须重算门槛,否则"3 小时的视频"在探测返回前一直是可跑的 */
async function probeItem(it) {
  try {
    it.probe = await invoke('probe_file', { path: it.path });
  } catch (err) {
    it.probe = { error: String(err && err.message ? err.message : err) };
  }
  if (focused() === it) {
    renderMediaBar();
    refreshReadouts();
    refreshSamples();
    refreshGate();
  }
}

function fmtBytes(n) {
  if (!n) return '—';
  const u = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(u.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / 2 ** (10 * i)).toFixed(i ? 1 : 0)}${u[i]}`;
}

function fmtDur(s) {
  if (!s) return '—';
  const s0 = Math.round(s);
  if (s0 < 60) return t('unit.sec', { n: s0 });
  const m = Math.floor(s0 / 60);
  if (m < 60) return t('unit.min', { m, s: String(s0 % 60).padStart(2, '0') });
  return t('unit.hour', { h: Math.floor(m / 60), m: m % 60 });
}

function renderMediaBar() {
  const it = focused();
  if (!it) { els.mediaBar.hidden = true; return; }
  els.mediaBar.hidden = false;
  if (!it.probe) { els.mediaBar.innerHTML = `<span class="dim">${t('media.loading')}</span>`; return; }
  const p = it.probe;
  if (p.error) { els.mediaBar.innerHTML = `<span class="err">${t('media.error', { e: msg(p.error) })}</span>`; return; }
  const cell = (k, v) => `<span class="mb"><i>${k}</i><b class="mono">${v}</b></span>`;
  const warns = [];
  if (p.vfr) warns.push(t('media.warn.vfr'));
  if (p.hdr) warns.push(t('media.warn.hdr'));
  if (/hevc|h265|av1/i.test(p.video_codec || '')) warns.push(t('media.warn.hevc', { c: p.video_codec }));
  // 没音轨却要调"磁带底噪/带限"是白调:引擎侧直接 AudioMode::None。不说出来,用户会以为参数坏了
  if (p.has_audio === false) warns.push(t('media.warn.noaudio'));
  if ((p.size_bytes || 0) > 800 * 1024 * 1024) warns.push(t('media.warn.big'));
  els.mediaBar.innerHTML =
    cell(t('media.name'), it.name) +
    cell(t('media.size'), `${p.width}×${p.height}`) +
    cell(t('media.dar'), (p.dar || '?').toString()) +
    cell(t('media.fps'), `${p.fps}${p.vfr ? ' (VFR)' : ''}`) +
    (p.is_image ? cell(t('media.kind'), t('media.still')) : cell(t('media.duration'), `${p.duration}s`)) +
    cell(t('media.codec'), `${p.video_codec}${p.has_audio ? '+' + p.audio_codec : ''}`) +
    cell(t('media.bytes'), fmtBytes(p.size_bytes)) +
    (warns.length ? `<div class="mb-warn">⚠ ${warns.join(' · ')}</div>` : '');
}

function renderStrip() {
  if (state.items.length <= 1) { els.strip.hidden = true; return; }
  els.strip.hidden = false;
  els.strip.innerHTML = '';
  state.items.forEach((it, i) => {
    const b = document.createElement('button');
    b.className = 'chip' + (i === state.focus ? ' on' : '');
    b.title = it.path;
    const dot = document.createElement('span');
    dot.className = 'dot' + (it.out ? ' ok' : '');
    b.appendChild(dot);
    b.appendChild(document.createTextNode(it.name));
    // 每一项都要能撤掉:误传素材时"只能重新加载页面"是不可接受的
    const x = document.createElement('span');
    x.className = 'chip-x';
    x.textContent = '×';
    x.title = t('card.remove');
    x.addEventListener('click', (e) => {
      e.stopPropagation();
      removeItem(i);
    });
    b.appendChild(x);
    b.addEventListener('click', () => setFocus(i));
    els.strip.appendChild(b);
  });
}

// ————————————————————————————————— 抽帧确认(先看再跑)

/** 静帧只有一帧:抽帧没有意义;探不动的文件更不能发 6 次 ffmpeg */
const sampleable = (it) => !!it && !!it.probe && !it.probe.error && !it.probe.is_image;
// 抽帧条属于"这一项素材 + 这一批种子",不跟参数变(参数变了重抽 6 次 ffmpeg 是灾难,
// 而且抽帧回答的是"这条素材长什么样",不是"这组参数什么效果")
const samplesKey = (it) => `${it.path}|${state.sampleSeed}`;

function refreshSamples() {
  const it = focused();
  if (!sampleable(it)) {
    els.samples.hidden = true;
    els.samplesStrip.innerHTML = '';
    els.samplesHint.textContent = '';
    state.samplesKey = null;
    state.samplesLast = null;
    return;
  }
  els.samples.hidden = false;
  const k = samplesKey(it);
  // 原来是"条里非空就不抽"—— 换素材时条里是**上一项**的帧,于是新素材永远不抽、旧帧一直挂着。
  // 但只看"条里有东西"会走反:抽帧返回 0 对(素材太短/全抽不到)时条里永远是空的,
  // 于是每次切焦点都重发 6 次 ffmpeg。判据改成"这一项 + 这一批种子已经要过一次结果"。
  const already = state.samplesLast && state.samplesLast.key === k;
  if (state.samplesKey === k && (els.samplesStrip.childElementCount || already)) return;
  requestSamples();
}

async function requestSamples() {
  const it = focused();
  if (!sampleable(it)) return;
  const seq = ++state.sampleSeq;
  const key = samplesKey(it);
  state.samplesKey = key;
  els.samplesStrip.innerHTML = '';
  els.samplesHint.textContent = t('samples.loading');
  try {
    const r = await invoke('sample_frames', {
      preset: state.preset,
      input: it.path,
      count: 6,
      seed: state.sampleSeed,
      intensity: currentIntensity(),
      overrides: overrideList(),
    });
    if (seq !== state.sampleSeq) return;
    state.samplesError = null;
    state.samplesLast = { key, data: r };
    renderSamples(r);
  } catch (err) {
    if (seq !== state.sampleSeq) return;
    state.samplesKey = null;   // 失败要能重来,否则这条素材永远停在"加载中"
    state.samplesError = msg(err);
    els.samplesHint.textContent = t('samples.failed', { e: state.samplesError });
    noteError(t('samples.failed', { e: state.samplesError }));
    // 抽帧失败也得留一条出路:门槛不能变成死锁
    refreshGate();
  }
}

function renderSamples(r) {
  els.samplesStrip.innerHTML = '';
  const pairs = r.pairs || [];
  els.samplesHint.textContent = pairs.length
    ? t('samples.count', { n: pairs.length, d: (r.duration || 0).toFixed(0) })
    : t('samples.none');
  for (const p of pairs) {
    const cell = document.createElement('button');
    cell.className = 'sample' + (state.anchorT === p.t ? ' on' : '');
    cell.title = t('samples.at', { t: p.t });
    const img = document.createElement('img');
    // 不再 lazy:抽帧条是门槛的唯一视觉依据,懒加载在后台/被遮挡的表面里可能永不触发,
    // 而换成 320px 缩略后整条带才 788 KB,省不下什么
    img.alt = t('samples.at', { t: p.t });
    // 抽帧条带用引擎顺手出的 320px 缩略(全帧 1.3 MB × 6 张 = 一次抽帧 8 MB)
    img.src = url(p.thumb || p.result, `${p.t}|${state.preset}|${state.aging}|${currentIntensity()}`);
    img.onerror = () => cell.classList.add('broken');
    const lab = document.createElement('span');
    lab.className = 'sample-t mono';
    lab.textContent = `${Math.floor(p.t / 60)}:${String(Math.round(p.t % 60)).padStart(2, '0')}`;
    cell.append(img, lab);
    cell.addEventListener('click', () => {
      state.anchorT = p.t;
      state.confirmed = true;
      [...els.samplesStrip.children].forEach((c) => c.classList.remove('on'));
      cell.classList.add('on');
      refreshGate();
      requestPreview(60);
    });
    els.samplesStrip.appendChild(cell);
  }
  // 确认必须是用户的动作:自动抽帧不算"看过了",否则门槛形同虚设
  refreshGate();
}

// ————————————————————————————————— 成品直接浏览

async function openResult(it) {
  if (!it || !it.out) return;
  try {
    await invoke('view_result', { path: it.out });
  } catch (err) {
    setStatus(t('result.unauthorized', { e: msg(err) }));
  }
  const isImg = /\.(png|jpe?g|webp|bmp|gif|tiff?)$/i.test(it.out);
  els.resultVideo.hidden = !isImg ? false : true;
  els.resultImg.hidden = isImg ? false : true;
  const src = url(it.out, it.out);
  if (isImg) els.resultImg.src = src;
  else {
    els.resultVideo.src = src;
    els.resultVideo.play().catch(() => {});
  }
  els.rvName.textContent = it.out.split(/[\\/]/).pop();
  els.rvMeta.textContent = it.out;
  els.doneCard.hidden = true;
  els.viewer.hidden = true;
  els.resultView.hidden = false;
}

function closeResult() {
  els.resultView.hidden = true;
  els.resultVideo.pause?.();
  const it = focused();
  if (it) {
    els.viewer.hidden = false;
    els.doneCard.hidden = !it.out;
  } else {
    els.hero.hidden = false;
    els.viewer.hidden = true;
  }
}

// ————————————————————————————————— 画廊(预设对比图)

/** 一个预设可用的对比资产 URL(没有则 null)。老引擎不给 gallery 字段时退回探测式渲染。 */
function galleryAssets(p) {
  const g = p.gallery || null;
  const u = (name, ext) => `gallery/${name}.${ext}`;
  const known = !!g;
  return {
    tile: !known || g.tile ? u(p.id, 'png') : null,
    full: known && g.full ? u(`${p.id}_full`, 'png') : null,
    hero: known && g.hero ? u(`${p.id}_hero`, 'webp') : null,
    motion: !known || g.motion ? u(p.id, 'webm') : null,
  };
}

let pop = null;
let popTimer = null;
let closeTimer = null;

/** 离开卡片就关大图。留 140 ms 宽限:指针从卡片挪到弹层本身不该抖一下就关 */
function scheduleClose() {
  if (popTimer) { clearTimeout(popTimer); popTimer = null; }
  if (!pop) return;
  if (closeTimer) clearTimeout(closeTimer);
  closeTimer = setTimeout(closePop, 140);
}

function closePop() {
  if (popTimer) { clearTimeout(popTimer); popTimer = null; }
  if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; }
  if (!pop) return;
  pop.classList.remove('show');
  const dead = pop;
  pop = null;
  setTimeout(() => dead.remove(), 220);
}

/** 最能代表这个预设的一句话:取三个"签名参数"读数 */
function popParams(p) {
  const out = [];
  for (const [stage, prm] of Object.entries(p.params || {})) {
    if (!prm || typeof prm !== 'object') continue;
    for (const [k, v] of Object.entries(prm)) {
      if (typeof v === 'number' && v !== 0) out.push(`${stage}.${k}=${fmtVal(v, '')}`);
      if (out.length >= 3) return out;
    }
  }
  return out;
}

function openPop(card, p) {
  closePop();
  if (!card.isConnected) return;   // 卡片已经不在页面上(画廊重画过):别把大图贴在左上角
  const a = galleryAssets(p);
  const src = a.hero || a.full || a.tile;
  if (!src) return;
  pop = document.createElement('div');
  pop.className = 'pcard-pop';
  pop.innerHTML =
    `<div class="pop-head"><span class="pop-title">${I18N.presetName(p)}</span>` +
    `<span class="pop-note mono">${t('pop.meta', { era: p.era, n: p.aging || 1 })}</span>` +
    (a.motion ? `<span class="pop-tabs"><button class="mini" data-act="motion">${t('pop.motion')}</button></span>` : '') +
    `</div><img alt="${t('pop.alt', { name: I18N.presetName(p) })}" src="${src}" />` +
    `<div class="pop-params">${popParams(p).map((s) => `<span><b>${s}</b></span>`).join('')}</div>`;
  document.body.appendChild(pop);
  // 弹层自己也参与"离开即结束":停在上面可以看,走开就必须消失
  pop.addEventListener('mouseenter', () => { if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; } });
  pop.addEventListener('mouseleave', scheduleClose);
  const r = card.getBoundingClientRect();
  const w = Math.min(720, window.innerWidth * 0.78);
  const left = Math.max(8, Math.min(r.right + 10, window.innerWidth - w - 8));
  pop.style.width = `${w}px`;
  pop.style.left = `${left}px`;
  pop.style.top = `${Math.max(8, Math.min(r.top, window.innerHeight - 420))}px`;
  const btn = pop.querySelector('[data-act="motion"]');
  if (btn) {
    btn.addEventListener('click', () => {
      const img = pop.querySelector('img');
      const v = document.createElement('video');
      v.src = a.motion;
      v.muted = true;
      v.autoplay = true;
      v.loop = true;
      v.playsInline = true;
      v.onerror = () => { v.remove(); img.hidden = false; };
      img.hidden = true;
      pop.querySelector('.pop-head').after(v);
      btn.disabled = true;
    });
  }
  // setTimeout 而不是 rAF:零尺寸/后台表面里 rAF 根本不触发,过渡会永远停在未显示态
  setTimeout(() => pop && pop.classList.add('show'), 16);
}

/** 引擎把变体折进了基准卡片的 variants;界面得能把"选中某个类型"当成一个正常预设来用 */
function expandVariants(list) {
  const out = [];
  for (const p of list) {
    out.push(p);
    for (const v of p.variants || []) out.push(v);
  }
  return out;
}

function renderGallery() {
  // 重画之前先收尾弹层:卡被 innerHTML 清掉时,挂在旧卡上的 300 ms 定时器还会照点触发,
  // 而离线元素的 getBoundingClientRect() 全是 0 —— 大图就贴在屏幕左上角且再也关不掉(实测路径:
  // 悬停卡片后 300 ms 内点★,收藏切换会重画画廊)
  closePop();
  els.gallery.innerHTML = '';
  // 列表只放"能占一张卡"的预设:年代轴产物与变体都不占(前者是中间状态,后者在「类型」里)
  const list = state.presets.filter((p) => p.card);
  const hasImg = (p) => {
    const a = galleryAssets(p);
    return !!(a.full || a.tile);
  };
  // 默认预设永远第一个(它就是首屏选中的那个),其后才是收藏 / 最近 / 有对比图
  const rank = (p) =>
    (p.id === DEFAULT_PRESET ? -1 : state.favorites.has(p.id) ? 0 : state.recents.includes(p.id) ? 1 : !hasImg(p) ? 3 : 2);
  list.sort((a, b) => rank(a) - rank(b) || a.era - b.era);
  for (const p of list) {
    const a = galleryAssets(p);
    const card = document.createElement('button');
    card.className = 'pcard' + (p.id === familyOf(state.preset) ? ' on' : '') + (p.id === DEFAULT_PRESET ? ' default' : '');
    card.dataset.id = p.id;
    const imgSrc = a.full || a.tile;
    if (imgSrc) {
      const img = document.createElement('img');
      img.alt = I18N.presetName(p);
      img.loading = 'lazy';
      img.src = imgSrc;
      img.onerror = () => {
        card.classList.add('noprev');
        img.remove();
      };
      card.appendChild(img);
      const hover = () => {
        if (pop && pop.dataset.for === p.id) return;
        if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; }
        // 只允许一个待发的弹层:从 A 卡扫到 B 卡时,A 那个必须取消,否则手速快点会弹出两张
        if (popTimer) clearTimeout(popTimer);
        popTimer = setTimeout(() => { openPop(card, p); if (pop) pop.dataset.for = p.id; }, 300);
      };
      card.addEventListener('mouseenter', hover);
      card.addEventListener('focus', hover);
      card.addEventListener('mouseleave', scheduleClose);
      card.addEventListener('blur', closePop);
    } else {
      card.classList.add('noprev');
    }
    const nm = document.createElement('span');
    nm.className = 'pcard-name';
    nm.textContent = I18N.presetName(p);
    card.appendChild(nm);
    // 卡上不报年代:年代是"高级参数"里的一个轴,不是预设的身份标签。
    // 「默认」标记留著 —— 它回答的是"首屏选中的是哪个",而且必须走词典(以前写在 CSS content 里,英文界面也显示中文)
    if (p.id === DEFAULT_PRESET) {
      const tag = document.createElement('span');
      tag.className = 'pcard-tag';
      tag.textContent = t('card.default');
      card.appendChild(tag);
    }
    const star = document.createElement('span');
    star.className = 'star' + (state.favorites.has(p.id) ? ' on' : '');
    star.textContent = '★';
    star.title = t('card.fav');
    star.addEventListener('click', (e) => {
      e.stopPropagation();
      if (state.favorites.has(p.id)) state.favorites.delete(p.id);
      else state.favorites.add(p.id);
      renderGallery();
      persist();
    });
    card.append(star);
    card.addEventListener('click', () => { setPreset(p.id); closePop(); });
    els.gallery.appendChild(card);
  }
  renderSeedRow();
}

/** 种子芯片:看得见、可复制、可锁定(锁定后「换一批」不再动它) */
function renderSeedRow() {
  if (!els.seedRow) return;
  els.seedRow.innerHTML = '';
  const p = state.presets.find((x) => x.id === state.preset);
  if (!p) return;
  for (const [stage, params] of Object.entries(p.params || {})) {
    if (!params || typeof params !== 'object' || params.seed === undefined) continue;
    const id = `${stage}.seed`;
    const pinned = state.overrides.get(id);
    const seed = pinned !== undefined ? pinned : String(params.seed);
    const chip = document.createElement('span');
    chip.className = 'seed-chip';
    chip.innerHTML = `<i>${t('seed.label', { s: stage.replace(/_(roundtrip|vhs|damage)/, '') })}</i><b class="mono">${seed}</b>`;
    const copy = document.createElement('button');
    copy.className = 'mini';
    copy.textContent = t('seed.copy');
    copy.addEventListener('click', () => {
      navigator.clipboard?.writeText(`${state.preset} ${id}=${seed}`);
      setStatus(t('seed.copied'));
    });
    const lock = document.createElement('button');
    lock.className = 'mini' + (pinned !== undefined ? ' on' : '');
    lock.textContent = pinned !== undefined ? t('seed.locked') : t('seed.lock');
    lock.title = t('seed.locktitle');
    lock.addEventListener('click', () => {
      if (pinned !== undefined) state.overrides.delete(id);
      else state.overrides.set(id, seed);
      renderSeedRow();
      refreshOverrideBadge();
      persist();
      requestPreview(120);
    });
    chip.append(copy, lock);
    els.seedRow.appendChild(chip);
  }
}

/** 变体(同一效果的不同实现)算同一张卡:卡片高亮、最近使用都跟着家族走 */
function familyOf(id) {
  const p = state.presets.find((x) => x.id === id);
  return p && p.variant_of ? p.variant_of : id;
}

/** 「类型」行:基准 + 变体各一个按钮,点谁就用谁的预设文件 */
function renderVariants() {
  const base = state.presets.find((x) => x.id === familyOf(state.preset));
  const opts = base
    ? [{ id: base.id, label: I18N.variantLabel(base.variant || t('variant.default')) },
       ...(base.variants || []).map((v) => ({ id: v.id, label: I18N.variantLabel(v.variant) }))]
    : [];
  if (opts.length < 2) {
    els.variantRow.hidden = true;
    els.variantOpts.innerHTML = '';
    return;
  }
  els.variantRow.hidden = false;
  els.variantOpts.innerHTML = '';
  for (const o of opts) {
    const b = document.createElement('button');
    b.textContent = o.label;
    b.className = o.id === state.preset ? 'on' : '';
    b.title = t('variant.title', { label: o.label });
    b.addEventListener('click', () => { if (state.preset !== o.id) setPreset(o.id); });
    els.variantOpts.appendChild(b);
  }
}

function setPreset(id) {
  if (!id) return;
  state.preset = id;
  state.recents = [familyOf(id), ...state.recents.filter((x) => x !== familyOf(id))].slice(0, 4);
  [...els.gallery.children].forEach((c) => c.classList.toggle('on', c.dataset.id === familyOf(id)));
  const p = state.presets.find((x) => x.id === id);
  els.reroll.disabled = !p;
  // 预设自带的手数是它的"味道"的一部分:用户没手动调过就跟预设走
  if (p && !state.agingTouched) setAging(Math.max(1, Math.min(8, p.aging || 1)));
  renderRecipeChip();
  renderVariants();
  refreshReadouts();
  renderSeedRow();
  requestPreview(120);
  persist();
}

/** 做旧系数(几手):一级傻瓜旋钮,读数带手数与预估耗时 */
function setAging(n) {
  state.aging = Math.max(1, Math.min(8, n | 0));
  if (els.aging) els.aging.value = state.aging;
  if (els.agingLabel) els.agingLabel.textContent = t('unit.hands', { n: state.aging });
  if (els.agingField) els.agingField.classList.toggle('blow', state.aging >= 6);
  if (els.agingCost) {
    els.agingCost.textContent = state.aging >= 6
      ? t('aging.blow', { x: ((state.aging - 1) * 0.35).toFixed(1) })
      : '';
  }
}

function presetParam(stage, key) {
  const p = state.presets.find((x) => x.id === state.preset);
  return p && p.params && p.params[stage] ? p.params[stage][key] : undefined;
}

// ————————————————————————————————— 控件(清单驱动 + 三态继承)

function paramMeta(stage, key) {
  for (const sec of ['video', 'audio']) {
    const st = (state.manifest?.[sec] || []).find((s) => s.stage === stage);
    const pm = st && (st.params || []).find((x) => x.key === key);
    if (pm) return pm;
  }
  return null;
}

function overrideList() {
  const list = [...state.overrides.entries()].map(([k, v]) => `${k}=${v}`);
  // 做旧系数与偏色都是**计划级**覆盖(不属于任何 stage),与强度同级
  if (state.aging > 1) list.push(`preset.aging=${state.aging}`);
  if (state.cast > 0) list.push(`preset.cast=${state.cast}`);
  return list;
}

/** 偏色(绿):默认关。包浆不等于变绿,发绿是可选味道(用户明确要求) */
function setCast(v) {
  state.cast = Math.max(0, Math.min(1, Math.round(v * 20) / 20));
  if (els.cast) els.cast.value = state.cast;
  if (els.castLabel) els.castLabel.textContent = state.cast === 0 ? t('unit.off') : `${Math.round(state.cast * 100)}%`;
}

/** 当前生效预设的来源说明:派生品必须让用户看得见,并给一条回到"我的预设"的路 */
function renderRecipeChip() {
  const p = state.presets.find((x) => x.id === state.preset);
  if (!els.recipeChip) return;
  if (!p || p.kind === 'builtin' || p.kind === 'user') {
    els.recipeChip.hidden = true;
    els.recipeChip.innerHTML = '';
    return;
  }
  els.recipeChip.hidden = false;
  els.recipeChip.innerHTML = `<span>${t('recipe.derived', { name: I18N.presetName(p) })}</span>`;
  const btn = document.createElement('button');
  btn.className = 'mini';
  btn.textContent = t('recipe.save');
  btn.addEventListener('click', async () => {
    const dflt = `${I18N.presetName(p)} · ${t('recipe.mine')}`;
    const name = window.prompt(t('recipe.prompt'), dflt) || dflt;
    try {
      const r = await invoke('save_recipe', { preset: p.id, name });
      setStatus(t('recipe.saved', { name: r.preset }));
      await reloadCatalog();
      setPreset(r.preset);
    } catch (err) {
      setStatus(t('recipe.fail', { e: msg(err) }));
    }
  });
  els.recipeChip.appendChild(btn);
}

/** 重新拉一次预设目录(存为预设 / 换一批之后要用) */
async function reloadCatalog() {
  try {
    applyCatalog(await invoke('app_info'));
    renderGallery();
  } catch (err) {
    noteError(t('recipe.catalog', { e: msg(err) }));
  }
}

function readVal(el) {
  if (el.type === 'checkbox') return el.checked ? 'true' : 'false';
  return String(el.value).trim();
}

/** 该控件"跟随源"时该填什么值(§规划 §3) */
function sourceValue(stage, key) {
  const p = focused()?.probe;
  if (!p || p.error) return null;
  if (stage === 'resize' && key === 'w') return String(p.width);
  if (stage === 'resize' && key === 'h') return String(p.height);
  if (stage === 'resize' && key === 'range') return p.color_range === 'pc' ? 'pc' : 'tv';
  if (stage === 'resize' && key === 'dar') {
    const meta = paramMeta('resize', 'dar');
    const opts = (meta?.options || ['4:3', '16:9']).map((s) => {
      const [a, b] = s.split(':').map(Number);
      return { s, r: a / b };
    });
    let best = opts[0];
    for (const o of opts) if (Math.abs(o.r - (p.dar || 1.78)) < Math.abs(best.r - (p.dar || 1.78))) best = o;
    return best.s;
  }
  if (stage === 'fps' && key === 'fps') {
    const stops = paramMeta('fps', 'fps')?.stops || [];
    let best = stops[0];
    for (const s of stops) if (Math.abs(s - p.fps) < Math.abs(best - p.fps)) best = s;
    return String(best);
  }
  return null;
}

function recipeValue(stage, key) {
  const v = presetParam(stage, key);
  const meta = paramMeta(stage, key);
  if (v === undefined || v === null) return meta && meta.default !== null ? String(meta.default) : null;
  return String(v);
}

function buildRow(stage, key, label, followSource) {
  const meta = paramMeta(stage, key);
  const id = `${stage}.${key}`;
  if (!meta || state.mounted.has(id)) return null;
  state.mounted.add(id);

  const row = document.createElement('div');
  row.className = 'knob';
  const name = document.createElement('span');
  name.textContent = label || I18N.paramLabel(stage, key, meta.label);
  row.appendChild(name);

  let input;
  if (meta.kind === 'stepped') {
    input = document.createElement('input');
    input.type = 'range';
    input.min = 0;
    input.max = meta.stops.length - 1;
    input.step = 1;
    input.dataset.stops = meta.stops.join(',');
  } else if (meta.kind === 'enum' || meta.kind === 'optional_enum') {
    input = document.createElement('select');
    const first = document.createElement('option');
    first.value = '';
    first.textContent = t('knob.follow');
    input.appendChild(first);
    for (const o of meta.options || []) {
      const opt = document.createElement('option');
      opt.value = o;
      opt.textContent = o;
      input.appendChild(opt);
    }
  } else if (meta.kind === 'bool') {
    input = document.createElement('input');
    input.type = 'checkbox';
  } else if (meta.kind === 'text') {
    input = document.createElement('input');
    input.type = 'text';
  } else {
    const span = (meta.max || 1) - (meta.min || 0);
    input = document.createElement('input');
    if (span > 200 || (meta.kind === 'int' && span > 60)) {
      input.type = 'number';
      input.min = meta.min;
      input.max = meta.max;
      input.step = meta.step || 1;
    } else {
      input.type = 'range';
      input.min = meta.min;
      input.max = meta.max;
      input.step = meta.step || 0.01;
    }
  }

  const out = document.createElement('b');
  out.className = 'kv';

  const srcBtn = document.createElement('button');
  srcBtn.className = 'src-chip';
  srcBtn.textContent = t('knob.source');
  srcBtn.title = t('knob.srctitle');
  srcBtn.hidden = !followSource;
  srcBtn.addEventListener('click', () => {
    const v = sourceValue(stage, key);
    if (v === null) { setStatus(t('knob.needsrc')); return; }
    input.value = v;
    apply(id, v);
  });

  function apply(k, v) {
    if (!v) state.overrides.delete(k);
    else state.overrides.set(k, v);
    syncRowById(id);
    refreshOverrideBadge();
    persist();
    requestPreview(800);
  }
  const sync = () => {
    let v = readVal(input);
    if (meta.kind === 'stepped' && v !== '') v = String(meta.stops[+v] ?? v);
    apply(id, v);
  };
  input.addEventListener('input', sync);
  input.addEventListener('change', sync);
  // 双击控件 = 复位到"跟随预设"(§规划 §3)
  row.addEventListener('dblclick', () => {
    input.value = '';
    if (input.type === 'checkbox') input.checked = false;
    apply(id, '');
  });

  row.dataset.id = id;
  row.appendChild(input);
  row.appendChild(out);
  row.appendChild(srcBtn);
  return row;
}

/** 读数格式化:小数位给到 4 位并去尾零;小于 0.001 的量改保 2 位有效数字。
 *  0.0029999999 这种值直接进读数栏会把 92px 的格子撑爆,而且假装精确;
 *  但 0.00025 被 4 位小数四舍五入成 0.0003 会把实际值报高 20%,0.000025 更会被报成 0(读着像"关")。 */
function fmtVal(v, unit) {
  let s;
  const n = typeof v === 'string' && v.trim() !== '' && !Number.isNaN(Number(v)) ? Number(v) : v;
  if (typeof n === 'number' && Number.isFinite(n)) {
    s = Math.abs(n) >= 0.001 || n === 0 ? String(parseFloat(n.toFixed(4))) : String(parseFloat(n.toPrecision(2)));
  } else if (typeof n === 'boolean') s = n ? t('knob.yes') : t('knob.no');
  else s = String(v);
  return unit ? `${s} ${unit}` : s;
}

function syncRowById(id) {
  const row = [...document.querySelectorAll('.knob')].find((r) => r.dataset.id === id);
  if (!row) return;
  const [stage, key] = id.split('.');
  const meta = paramMeta(stage, key);
  const input = row.querySelector('input,select,textarea');
  const out = row.querySelector('.kv');
  const pinned = state.overrides.has(id);
  const v = state.overrides.get(id);
  if (input) {
    if (meta.kind === 'stepped') {
      // 未固定时也要停在"跟随的那一档":停在索引 0(帧率表是降序 = 60fps)而读数写 25 fps,是两张皮
      const target = pinned ? v : recipeValue(stage, key);
      const idx = meta.stops.findIndex((s) => String(s) === String(target));
      input.value = idx >= 0 ? idx : 0;
      input.classList.toggle('unset', idx < 0);
    } else {
      // 未固定时滑块停在"跟随的那个值"上,不是区间中点:读数写着 0.000025、滑块却指着 0.025,
      // 用户轻轻一拧就是千倍跳(雪花/色度损失这种小量程旋钮最容易踩)。
      let idle = '';
      if (input.type === 'range' || input.type === 'number') {
        const rv = recipeValue(stage, key);
        const n = rv === null ? NaN : Number(rv);
        if (Number.isFinite(n)) idle = n;
      }
      input.value = pinned ? v : idle;
      if (input.type === 'checkbox') input.checked = v === 'true';
    }
  }
  row.classList.toggle('pinned', pinned);
  if (out) {
    if (pinned) out.textContent = fmtVal(v, I18N.unitLabel(meta.unit));
    else {
      const rv = recipeValue(stage, key);
      out.textContent = rv === null ? t('knob.follow') : `${t('knob.follow')}(${fmtVal(rv, '')})`;
    }
  }
}

function refreshReadouts() {
  for (const id of state.mounted) syncRowById(id);
}

function renderKnobs() {
  const m = state.manifest;
  if (!m) {
    els.knobsBasic.innerHTML = `<p class="dim">${t('knob.empty')}</p>`;
    return;
  }
  els.knobsBasic.innerHTML = '';
  els.knobsPro.innerHTML = '';
  state.mounted = new Set();

  for (const c of m.controls || []) {
    const box = document.createElement('div');
    box.className = 'kgroup';
    const h = document.createElement('div');
    h.className = 'kgroup-head';
    h.textContent = I18N.controlLabel(c.id, c.label);
    box.appendChild(h);
    for (const b of c.binds || []) {
      const row = buildRow(b.stage, b.key, I18N.paramLabel(b.stage, b.key, b.label), b.follow === 'source');
      if (row) box.appendChild(row);
    }
    els.knobsBasic.appendChild(box);
  }

  // 高级参数:清单里剩下的全部,按分组手风琴(单开模式)
  const groups = {};
  for (const sec of ['video', 'audio']) {
    for (const st of m[sec] || []) {
      for (const p of st.params || []) {
        if (state.mounted.has(`${st.stage}.${p.key}`)) continue;
        (groups[st.group] = groups[st.group] || []).push([st, p]);
      }
    }
  }
  const labelOf = (gid) => I18N.groupLabel(gid, (m.groups.find((g) => g.id === gid) || { label: gid }).label);
  let first = true;
  for (const [gid, list] of Object.entries(groups)) {
    const det = document.createElement('details');
    det.className = 'pgroup';
    det.open = first;
    first = false;
    const sum = document.createElement('summary');
    sum.textContent = `${labelOf(gid)} · ${list.length}`;
    det.appendChild(sum);
    const body = document.createElement('div');
    body.className = 'pgroup-body';
    for (const [st, p] of list) {
      const row = buildRow(st.stage, p.key, `${I18N.stageLabel(st.stage, st.label)} · ${I18N.paramLabel(st.stage, p.key, p.label)}`);
      if (row) body.appendChild(row);
    }
    det.appendChild(body);
    els.knobsPro.appendChild(det);
  }
  refreshReadouts();
}

function refreshOverrideBadge() {
  els.clearOverrides.hidden = state.overrides.size === 0;
  if (!state.running) setStatus(overridesText());
}

function overridesText() {
  const bits = [];
  if (state.overrides.size) bits.push(t('status.overrides', { n: state.overrides.size }));
  if (state.aging > 1) bits.push(t('status.aging', { n: state.aging }));
  if (state.cast > 0) bits.push(t('status.cast', { p: Math.round(state.cast * 100) }));
  if (bits.length) return bits.join(' · ');
  const it = focused();
  return it ? t('status.item', { name: it.name }) : t('status.ready');
}

els.clearOverrides.addEventListener('click', () => {
  state.overrides.clear();
  refreshReadouts();
  refreshOverrideBadge();
  persist();
  requestPreview(120);
});

// ————————————————————————————————— 预览(真管线,防抖)

function requestPreview(delay = 800) {
  clearTimeout(state.previewTimer);
  const it = focused();
  if (!it || !state.preset) return;
  state.previewTimer = setTimeout(() => doPreview(it), delay);
}

async function doPreview(it) {
  const seq = ++state.previewSeq;
  // 这块遮罩是"跑批的总体进度"和"预览"共用的,所以谁能改它要看谁在说话:
  // 实测缺陷 —— 开跑后有一条预览才回来,它的 finally 把正在跑批的遮罩 hide 掉了,
  // 于是进度数字一路更新而用户什么都看不见(改滑杆立刻按开始最容易撞上)。
  if (!state.running) {
    els.busy.hidden = false;
    els.busyStage.textContent = t('busy.preview');
    els.busyPct.textContent = '';
  }
  try {
    const r = await invoke('preview_compare', {
      preset: state.preset,
      input: it.path,
      // 抽帧确认里点过某一帧,就以那一帧为准;否则取素材中段(静帧由引擎归到 0)
      t: state.anchorT ?? Math.min(1.0, (it.probe?.duration || 2) / 2),
      intensity: currentIntensity(),
      overrides: overrideList(),
    });
    if (seq !== state.previewSeq) return;
    // 预览结果属于"发起它的那一项",不属于"此刻恰好聚焦的那一项":
    // 切素材只用 120 ms 防抖,一条在飞的预览完全可能落在新素材的画面上,
    // 于是画幅读数 / 吞吐实测都会把 A 的量记到 B 的键上(probeItem 早就防了这一手,这里没有)
    it.thumb = r.result;   // 缩略位跟着它自己最近一次预览,与是否聚焦无关
    if (focused() !== it) return;
    // URL 带参数指纹:同一文件名 = 同一组参数,浏览器给旧帧的可能被堵死
    const stamp = `${state.preset}|${state.aging}|${state.cast}|${currentIntensity()}|${r.t}`;
    // 实测吞吐按参数指纹存:命中缓存时没有新测量,只有指纹对得上才敢用旧值
    if (typeof r.rate === 'number') state.rate = { key: paramKey(), value: r.rate };
    els.wipeSrc.src = url(r.source, stamp);
    els.wipeOut.src = url(r.result, stamp);
    state.outGeo = r.out || null;
    state.outSrc = it.probe ? { width: it.probe.width, height: it.probe.height } : null;
    renderOutGeo();
    els.wipeSrc.onerror = () => setStatus(t('err.previewframe'));
  } catch (err) {
    if (seq !== state.previewSeq) return;
    state.outGeo = null;
    renderOutGeo();
    const m = msg(err);
    noteError(t('err.preview', { e: m }));
    setStatus(t('err.preview', { e: m }));
  } finally {
    if (seq === state.previewSeq) {
      // 跑批进行中就不许收这块遮罩(见上面 doPreview 开头那条)
      // 跑批进行中就不许收这块遮罩(见上面 doPreview 开头那条)
      if (!state.running) els.busy.hidden = true;
      refreshGate();   // 拿到实测吞吐之后,预计耗时从"粗估"升级为"实测"
    }
  }
}

els.wipeSlider.addEventListener('input', () => applyWipe(els.wipeSlider.value));
function applyWipe(v) {
  els.wipeOut.style.clipPath = `inset(0 0 0 ${v}%)`;
}
applyWipe(50);

// 预设改了画幅时必须说出来:否则用户只看到"输出比原画面小一圈",以为叠错了
function renderOutGeo() {
  const e = els.outGeo;
  if (!e) return;
  const o = state.outGeo, s = state.outSrc;
  if (!o || !o.w || !s || !s.width) { e.hidden = true; e.textContent = ''; return; }
  const sameSize = o.w === s.width && o.h === s.height;
  const proxied = !!o.preview_w && o.preview_w < (o.display_w || o.w);
  if (sameSize && !o.dar && !proxied) { e.hidden = true; e.textContent = ''; return; }
  const wh = `${o.w}×${o.h}`;
  let txt = o.dar ? t('geo.outar', { wh, ar: o.dar }) : t('geo.out', { wh });
  // 屏幕上画的是代理图(长边上限),不写出来就等于让用户以为"成品只有这么点大"
  if (o.preview_w && o.preview_w < (o.display_w || o.w)) txt += ' · ' + t('geo.proxied', { pw: o.preview_w });
  e.textContent = txt;
  e.hidden = false;
}

// ————————————————————————————————— 出片

function currentIntensity() {
  return parseFloat(els.intensity.value);
}

els.intensity.addEventListener('input', () => {
  els.intLabel.textContent = `${currentIntensity().toFixed(1)}×`;
  persist();
  requestPreview(800);
});

// 做旧系数:一级傻瓜旋钮,与强度同权重;1 手 = 只用预设自带的趟数
els.aging.addEventListener('input', () => {
  state.agingTouched = true;
  setAging(parseInt(els.aging.value, 10));
  setStatus(overridesText());
  persist();
  requestPreview(800);
});

els.cast.addEventListener('input', () => {
  setCast(parseFloat(els.cast.value));
  setStatus(overridesText());
  persist();
  requestPreview(800);
});

// 强度三档快捷:多数人只在这三档里选,无级滑杆留给愿意细调的人
document.querySelectorAll('[data-int]').forEach((b) => {
  b.addEventListener('click', () => {
    els.intensity.value = b.dataset.int;
    els.intLabel.textContent = `${currentIntensity().toFixed(1)}×`;
    persist();
    requestPreview(400);
  });
});

// 高级参数整栏默认收起(首屏"简单优先"),展开状态记进设置
function setPro(on) {
  els.workbench.classList.toggle('pro-off', !on);
  els.proToggle.setAttribute('aria-expanded', String(on));
  els.proToggle.textContent = t(on ? 'pro.close' : 'pro.open');
  state.proOpen = on;
  persist();
}
els.proToggle.addEventListener('click', () => setPro(els.workbench.classList.contains('pro-off')));

/** 三个一级旋钮的读数:换语言时要按新语言重刷,它们不走清单渲染那条路 */
function renderGauges() {
  els.intLabel.textContent = `${currentIntensity().toFixed(1)}×`;
  setAging(state.aging);
  setCast(state.cast);
  els.eraYear.textContent = els.era.value;
}

/** 换语言:静态节点 + 所有清单驱动的重渲染都要走一遍,不然只有标题变了 */
function setLang(next) {
  I18N.setLang(next);
  renderGallery();
  renderKnobs();
  renderRecipeChip();
  renderVariants();
  renderGauges();
  renderMediaBar();
  renderOutGeo();
  refreshReadouts();
  // 状态行与进度标题只有一个写源(JS)。换语言不许把"正在跑批"的文案换成静态默认值 ——
  // 从前这三处同时挂着 data-i18n,跑批中换个语言状态行就变回「就绪」(HTML 里已去掉那三个属性)
  setPro(state.proOpen);
  if (state.running) els.busyStage.textContent = t('busy.run');
  else setStatus(overridesText());
  if (state.samplesLast && state.samplesLast.key === state.samplesKey) renderSamples(state.samplesLast.data);
  else refreshSamples();
  persist();
}
els.langToggle.addEventListener('click', () => setLang(I18N.lang === 'en' ? 'zh' : 'en'));

/** 引擎目录 → 界面状态。变体必须在这里展开,否则刷新一次目录「类型」行就瞎了 */
function applyCatalog(info) {
  state.presets = expandVariants(info.presets || []);
  if (info.manifest) state.manifest = info.manifest;
  // 桌面版的 app_info 目前不带 font:没报就当不知道,不许拿"缺字段"当"没字体"去报警
  if ('font' in info) state.font = info.font;
}

/** 这个预设会不会画时间戳(没字体就不该出现,而用户会以为效果坏了) */
function presetNeedsFont() {
  const id = state.preset;
  const p = state.presets.find((x) => x.id === id) || state.presets.find((x) => x.id === familyOf(id));
  return !!(p && p.params && p.params.overlay_timestamp);
}

let eraTimer = null;
els.era.addEventListener('input', () => {
  els.eraYear.textContent = els.era.value;
  clearTimeout(eraTimer);
  eraTimer = setTimeout(async () => {
    const year = parseInt(els.era.value, 10);
    const before = state.preset;   // 等待期间用户自己选了预设就不覆盖他
    try {
      await invoke('use_era', { year });
      applyCatalog(await invoke('app_info'));
      if (state.preset !== before) return;
      renderGallery();
      renderKnobs();
      setPreset(`era_${year}`);
      setStatus(t('err.eraok', { y: year }));
      persist();
    } catch (err) {
      setStatus(t('err.era', { e: msg(err) }));
    }
  }, 250);
});

els.reroll.addEventListener('click', async () => {
  try {
    const n = await invoke('reroll_preset', { preset: state.preset });
    setStatus(n ? t('reroll.done', { n }) : t('reroll.none'));
    // 换一批的结果落在 derived/,是派生预设:刷新目录让来源 chip 显示出来
    if (n) await reloadCatalog();
    renderRecipeChip();
    requestPreview(120);
  } catch (err) {
    setStatus(t('err.reroll', { e: msg(err) }));
  }
});

els.start.addEventListener('click', () => runJob());

async function runJob() {
  const it = focused();
  if (!it || state.running) return;
  // 门槛在按钮上判一次不够:回车、完成卡的"再来一次"都直接调这里
  if (needsConfirm(it) && !state.confirmed) {
    setStatus(t('status.gate', { dur: fmtDur(it.probe?.duration), size: fmtBytes(it.probe?.size_bytes) }));
    return;
  }
  state.running = true;
  // item 事件里的 index 有两种语义:普通跑批一次只交一件,它恒为 0(批次位);再翻录传的是队列位。
  // 所以发起者必须自己记住"这一趟属于哪一项",不能拿事件下标去队列里捞 —— 那样焦点在第二项时,
  // 成品路径、完成卡与吞吐记账全会落到队列第一项头上。
  state.job = { item: it, key: paramKey(), t0: Date.now(), dur: it.probe?.duration || 0 };
  els.start.disabled = true;
  els.cancel.disabled = false;
  els.reclip.disabled = true;
  els.reroll.disabled = true;
  els.doneCard.hidden = true;
  els.busy.hidden = false;
  els.busyStage.textContent = t('busy.run');
  setStatus(t('run.start', { name: it.name }));
  try {
    await invoke('start_jobs', {
      preset: state.preset,
      files: [it.path],
      outDir: '',
      intensity: currentIntensity(),
      overrides: overrideList(),
    });
  } catch (err) {
    finish(t('run.fail', { e: msg(err) }));
  }
}

els.reclip.addEventListener('click', async () => {
  // 翻录的对象是"眼前这一项的成品",不是"历史上最后一个成品":素材被移除了就不该再冒出来
  const cur = focused();
  if (!cur || !cur.out || state.running) return;
  const it = { path: cur.out, name: cur.out.split(/[\\/]/).pop(), probe: null, out: null };
  state.items.push(it);
  state.reclipIdx = state.items.length - 1;
  setFocus(state.reclipIdx);
  await probeItem(it);
  state.running = true;
  // 再翻录的输入是上一手的成品,不是"按当前参数跑素材":吞吐不能记到那把参数上。
  // 留着旧的 state.job,收尾就会把两次点击之间的所有空闲时间算成这一手的耗时,
  // 预计耗时越用越大(实测:跑完等几分钟再翻录,估值翻几十倍)。
  state.job = null;
  els.start.disabled = true;
  els.cancel.disabled = false;
  els.busy.hidden = false;
  els.busyStage.textContent = t('busy.reclip');
  try {
    await invoke('reclip_job', { index: state.reclipIdx, input: it.path, times: 1 });
  } catch (err) {
    state.reclipIdx = -1;
    finish(t('run.reclipfail', { e: msg(err) }));
  }
});

els.cancel.addEventListener('click', () => {
  invoke('cancel_jobs');
  setStatus(t('run.canceling'));
});

function finish(msgText) {
  state.running = false;
  state.job = null;   // 收尾之后不许再有"迟到的 item 事件"按上一次的起点算耗时
  els.cancel.disabled = true;
  els.reroll.disabled = !state.preset;
  els.busy.hidden = true;
  // 进度数字必须跟着收尾一起清:否则下一次开跑的头几秒(第一条 progress 事件之前)
  // 显示的是上一趟的百分比 —— 又是"跨对象脏读数"
  els.busyPct.textContent = '';
  refreshGate();
  setStatus(msgText);
}

listen('rewind://job', (e) => {
  const ev = e.payload.event;
  if (ev.type === 'progress') {
    // 画的是**总体**百分比:引擎每趟自己报 0–100,直接拿来画会把一条 5 趟的任务扫五遍 0→100%
    // (用户报"进度在撒谎"就是这个)。老引擎没有 overall 字段时退回本趟读数,不至于空白。
    const o = typeof ev.overall === 'number' ? ev.overall : ev.pct;
    els.busyPct.textContent = `${Math.round(o)}%`;
    els.busyStage.textContent = ev.steps > 1
      ? t('busy.step', { i: ev.step, n: ev.steps })
      : t('busy.render');
  }
});

listen('rewind://item', (e) => {
  const { index, ok, output, error, canceled } = e.payload;
  // 归属看"是谁发起的":普通跑批一次只交一件,事件里的 index 是**批次位**(恒 0),
  // 拿它当队列位就会把第二项的成品记到第一项头上;只有再翻录传的是队列位(那时 state.job 为 null)。
  const it = state.job ? state.job.item : state.items[index];
  const isReclip = !state.job && index === state.reclipIdx;
  if (!it || !state.items.includes(it)) {
    // 结果找不到归属(素材跑完前被删了):宁可报错,也不许写到"恰好聚焦"的另一项上
    const m = t('run.orphan', { i: index + 1 });
    noteError(m);
    finish(m);
    renderStrip();
    return;
  }
  let text;
  if (ok) {
    it.out = output;
    // 真跑过一次的吞吐最值钱:存下来给同参数的下一件素材用
    if (state.job && state.job.dur > 0) {
      const el = (Date.now() - state.job.t0) / 1000;
      if (el > 0.5) state.measured = { key: state.job.key, perSecond: el / state.job.dur };
    }
    showDone(it);
    text = isReclip ? t('run.doneReclip') : t('run.done', { name: output.split(/[\\/]/).pop() });
  } else if (canceled || String(error || '').includes('[run.canceled]')) {
    // 取消是用户自己的动作,不该按失败去弹错误条。Web 侧引擎直接打 canceled 标记;
    // 桌面壳只往 error 里放一条字符串(三个 crate 相互独立,拿不到那个字段),所以按稳定码认。
    text = t('run.canceled');
  } else {
    text = isReclip ? t('run.reclipfail', { e: msg(error) }) : t('run.failed', { e: msg(error) });
    noteError(text + t('run.preset', { p: state.preset }));
  }
  if (isReclip) state.justReclipped = true;
  state.reclipIdx = -1;
  finish(text);
  renderStrip();
});

listen('rewind://batch', () => {
  refreshGate();
  persist();
});

function showDone(it) {
  els.doneCard.hidden = false;
  // 卡片记住的是**这一项对象**,不是下标:跑完之前删掉别的素材会让下标整体左移
  state.doneFor = it;
  els.doneName.textContent = it.out.split(/[\\/]/).pop();
  els.doneMeta.textContent = it.out;
  els.doneSave.href = url(it.out, it.out);
  els.doneSave.setAttribute('download', it.out.split(/[\\/]/).pop());
  els.doneThumb.src = url(it.thumb || it.out, it.out);
  els.doneView.disabled = false;
  refreshGate();
}

els.doneCopy.addEventListener('click', async () => {
  const out = (state.doneFor || focused())?.out || '';
  if (!out) { setStatus(t('clip.none')); return; }
  try {
    await navigator.clipboard.writeText(out);
    setStatus(t('clip.copied'));
  } catch {
    setStatus(t('clip.fail'));
  }
});

els.doneAgain.addEventListener('click', () => {
  // 「再来一次」是再来**这一张卡**的那一次:焦点在别处时先跳回去
  const i = state.items.indexOf(state.doneFor);
  if (i >= 0 && i !== state.focus) setFocus(i);
  runJob();
});
els.doneNext.addEventListener('click', () => {
  const next = state.items.findIndex((x, i) => i > state.focus && !x.out);
  if (next >= 0) setFocus(next);
  else {
    state.items = [];
    state.focus = -1;
    els.doneCard.hidden = true;
    setFocus(-1);
  }
});

els.trySample?.addEventListener('click', async () => {
  // 拿不到示例素材就收掉按钮(再点也无济于事),但原因要照实说:
  // 「这个构建没带素材」和「素材在、复制不进去(只读目录 / 盘满)」是两件事,
  // 以前引擎里两处 `.ok()` 把后者也报成前者,用户被指向一个不存在的问题。
  try {
    const p = await invoke('sample_file');
    if (p && p.path) { await addFiles([p.path]); return; }
    setStatus(t('upload.nosample'));
  } catch (e) {
    const m = String((e && e.message) || e || '');
    // 「没带素材」界面早有说法(还告诉用户下一步做什么);其余原因走解码后的原文
    setStatus(/\[asset\.missing\]/.test(m) ? t('upload.nosample') : msg(m || t('upload.nosample')));
  }
  els.trySample.hidden = true;
});

// ————————————————————————————————— 拖放 / 选择

els.drop.addEventListener('dragover', (e) => {
  e.preventDefault();
  els.canvas.classList.add('over');
});
els.drop.addEventListener('dragleave', () => els.canvas.classList.remove('over'));
els.drop.addEventListener('drop', (e) => {
  e.preventDefault();
  els.canvas.classList.remove('over');
  const dt = e.dataTransfer;
  if (dt && dt.files && dt.files.length) ingest(dt.files);
});

els.pick.addEventListener('click', () => {
  // 替换意图是一次性的:按「选择文件」就是"新加素材",不许带着上一次「更换素材」没兑现的意图进来
  // (实测:开了「更换素材」又取消,下一次正常添加会把不相干的那一项替掉)
  state.replacing = null;
  els.fileInput.click();
});
els.fileInput.addEventListener('change', () => {
  ingest(els.fileInput.files);
  els.fileInput.value = '';
});

// 「更换素材」= 选一个新文件替掉当前这一项(参数不动)
els.swapItem.addEventListener('click', () => {
  state.replacing = state.focus;
  els.fileInput.click();
});
els.dropItem.addEventListener('click', () => {
  if (state.focus >= 0) removeItem(state.focus);
});
els.samplesReroll.addEventListener('click', () => {
  state.sampleSeed = (state.sampleSeed * 1103515245 + 12345) % 100000 || state.sampleSeed + 1;
  requestSamples();
});
els.samplesConfirm.addEventListener('click', () => {
  state.confirmed = true;
  refreshGate();
  setStatus(t('status.confirmed'));
});
els.rvBack.addEventListener('click', closeResult);
els.rvCompare.addEventListener('click', () => {
  closeResult();
  requestPreview(60);
});
els.doneView.addEventListener('click', () => openResult(state.doneFor || focused()));

async function ingest(fileList) {
  const upload = typeof window.__REWIND_UPLOAD__ === 'function' ? window.__REWIND_UPLOAD__ : null;
  const all = [...fileList];
  const picked = all.filter((f) => MEDIA_EXT.test(f.name || ''));
  if (!picked.length) {
    // 静默 return 等于"点了没反应":用户不知道是自己选错了文件还是界面坏了
    if (all.length) setStatus(t('upload.notmedia', { n: all.length }));
    state.replacing = null;   // 选错了文件也一样,替换意图不许过夜
    return;
  }
  // 「更换素材」进来的:新文件加成功后把被替换的那一项撤掉
  const replacing = state.replacing === null || state.replacing === undefined ? null : state.items[state.replacing];
  state.replacing = null;
  const paths = [];
  els.pick.disabled = true;
  for (let i = 0; i < picked.length; i++) {
    const f = picked[i];
    if (!upload) {
      if (f.path) paths.push(f.path);
      continue;
    }
    setStatus(t('upload.progress', { i: i + 1, n: picked.length, name: f.name }));
    try {
      paths.push(await upload(f));
    } catch (err) {
      const m = msg(err);
      noteError(t('upload.fail', { name: f.name, e: m }));
      setStatus(t('upload.fail', { name: f.name, e: m }));
    }
  }
  els.pick.disabled = false;
  const before = state.items.length;
  await addFiles(paths);
  if (replacing && state.items.length > before) {
    const i = state.items.findIndex((x) => x.path === replacing.path);
    if (i >= 0) {
      state.items.splice(i, 1);
      // 换完要停在"新来的这一份"上,而不是跳到相邻的旧素材
      const n = state.items.findIndex((x) => paths.includes(x.path));
      setStatus(t('queue.replaced', { name: state.items[n] ? state.items[n].name : t('queue.empty') }));
      setFocus(n >= 0 ? n : Math.min(i, state.items.length - 1));
      return;
    }
  }
  if (upload) {
    const added = state.items.length - before;
    if (added) setStatus(t('upload.done', { n: added }));
  }
}

/** 界面所有错误文本的唯一出口。引擎发的是 `[稳定码] 中文原文`,按码换成本地说法;
 *  码不认识(引擎新增而词典没跟上)就原样给回 —— 宁可露中文,也不显示 'err.xxx' 键名。 */
function msg(err) {
  return I18N.decodeError(String((err && err.message) || err || t('err.unknown')));
}

function setStatus(t) {
  els.status.textContent = t;
}

/** 错误要能变成一条好 issue:留最近 20 条上下文,一键复制 */
function noteError(t) {
  const it = focused();
  state.log.push(`${new Date().toTimeString().slice(0, 8)}  ${t}`);
  if (state.log.length > 20) state.log.shift();
  els.diag.hidden = false;
  els.diag.title = t;
}

els.diag.addEventListener('click', async () => {
  const it = focused();
  const bundle = [
    `Rewind ${state.version || '?'}`,
    `engine: ${state.engineName || '?'}`,
    `preset: ${state.preset}`,
    `intensity: ${currentIntensity()}`,
    `aging: ${t('unit.hands', { n: state.aging })}`,
    `overrides: ${JSON.stringify(Object.fromEntries(state.overrides))}`,
    `item: ${it ? it.path : t('diag.none')}`,
    `probe: ${it && it.probe ? JSON.stringify(it.probe) : t('diag.none')}`,
    '',
    ...state.log,
  ].join('\n');
  try {
    await navigator.clipboard.writeText(bundle);
    setStatus(t('diag.copied'));
  } catch {
    els.aboutBody.textContent = bundle;
    els.aboutModal.hidden = false;
    setStatus(t('diag.fail'));
  }
});

// ————————————————————————————————— 键盘

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    // Esc 顺序:大图对比 → 弹窗 → 取消任务。模态挡着时"取消"本来点不到,别再来一层
    if (pop) {
      closePop();
      return;
    }
    if (!els.aboutModal.hidden) {
      els.aboutModal.hidden = true;
      return;
    }
    if (state.running) els.cancel.click();
    return;
  }
  if (!els.aboutModal.hidden) return;
  // 焦点落在按钮/滑块/输入框上时,快捷键必须让位:否则空格点不动按钮、回车在画廊卡上直接开跑
  const typing = /^(INPUT|TEXTAREA|SELECT|BUTTON)$/.test(e.target.tagName || '');
  if (e.code === 'Space' && !e.repeat && focused() && !typing) {
    e.preventDefault();
    els.wipeOut.style.opacity = '0';
  } else if (e.key === 'Enter' && !typing && !els.start.disabled && focused()) {
    // 走"按钮是否可用"这一个判据:门槛与运行中状态自动生效,不再另算一套
    runJob();
  } else if (e.key === 'Delete' && focused() && !/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName || '')) {
    // 在参数输入框里按 Delete 是打字,不是删素材
    removeItem(state.focus);
  }
});
document.addEventListener('keyup', (e) => {
  // 无条件复原:按住空格期间素材被删掉的话,做旧那一层会永久隐形
  if (e.code === 'Space') els.wipeOut.style.opacity = '';
});

// ————————————————————————————————— 设置持久化

let setTimer = null;
function persist() {
  clearTimeout(setTimer);
  setTimer = setTimeout(() => {
    invoke('set_settings', {
      v: {
        lang: I18N.lang,
        lastPreset: state.preset,
        lastOutDir: null,
        eraYear: parseInt(els.era.value, 10),
        intensity: currentIntensity(),
        aging: state.aging,
        agingTouched: state.agingTouched,
        cast: state.cast,
        proOpen: !!state.proOpen,
        overrides: Object.fromEntries(state.overrides),
        favorites: [...state.favorites],
        recents: state.recents,
      },
    }).catch(() => {});
  }, 400);
}

// ————————————————————————————————— 关于

els.about.addEventListener('click', () => {
  els.aboutBody.innerHTML = `
    <p>${t('about.body')}</p>
    <p class="dim">${t('about.watermark')}</p>
    <p class="mono dim">${t('about.meta', { v: state.version, n: countParams() })}</p>`;
  els.aboutModal.hidden = false;
});
els.aboutClose.addEventListener('click', () => (els.aboutModal.hidden = true));
els.aboutModal.addEventListener('click', (e) => {
  if (e.target === els.aboutModal) els.aboutModal.hidden = true;
});

function countParams() {
  if (!state.manifest) return 0;
  return ['video', 'audio'].reduce(
    (n, s) => n + (state.manifest[s] || []).reduce((k, st) => k + (st.params || []).length, 0),
    0
  );
}

// ————————————————————————————————— 启动

(async function init() {
  applyWipe(50);
  // 静态文案先按浏览器语言落地;设置里存过用户选的那一种,稍后覆盖
  I18N.setLang(I18N.detect());
  try {
    const info = await invoke('app_info');
    applyCatalog(info);
    state.engineName = info.engine || '';
    els.engineTag.textContent = info.engine ? t('engine.tag', { e: info.engine }) : '';
    renderGallery();
    renderKnobs();
    const s = await invoke('get_settings');
    if (s.lang && s.lang !== I18N.lang) setLang(s.lang);
    if (s.eraYear) {
      els.era.value = s.eraYear;
      els.eraYear.textContent = s.eraYear;
    }
    if (s.intensity) {
      els.intensity.value = s.intensity;
      els.intLabel.textContent = `${parseFloat(s.intensity).toFixed(1)}×`;
    }
    state.agingTouched = !!s.agingTouched;
    setAging(s.aging || 1);
    setPro(!!s.proOpen);
    if (s.overrides) for (const [k, v] of Object.entries(s.overrides)) state.overrides.set(k, String(v));
    if (Array.isArray(s.favorites)) state.favorites = new Set(s.favorites);
    if (Array.isArray(s.recents)) state.recents = s.recents.slice(0, 4);
    const saved = s.lastPreset && state.presets.find((p) => p.id === s.lastPreset);
    // 首屏默认必须是「电子包浆」;上次的选择若是临时预设(年代轴/换一批生成)也不该延续
    const want = saved && saved.kind !== 'derived' ? saved.id
      : (state.presets.some((p) => p.id === DEFAULT_PRESET) ? DEFAULT_PRESET : null);
    renderGallery();
    setPreset(want || (state.presets[0] && state.presets[0].id));
    if (typeof s.cast === 'number' && s.cast > 0) setCast(s.cast);
    refreshOverrideBadge();
  } catch (err) {
    setStatus(t('err.engine', { e: msg(err) }));
  }
  els.start.disabled = true;
})();
