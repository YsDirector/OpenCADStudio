// 智能卡片新卡冒烟（node 最小 DOM 垫片 + fetch 桩）：锁住「齿轮参数表」与
// 「ANSI 花键参数表（纯中文 / 纯英文）」在 spline_gui.html 里的交互契约
// （node --check / el("id") 静态扫描查不出）：
//   ① 卡类型下拉 4 项，切换卡片 → 面板切换（splinePanel/gearPanel/ansiPanel）；
//   ② 齿轮卡：表达式 + 配对齿数/图号/精度等级/中心距 → /api/card_preview，
//      结果含 19 项、缺项显示「—」；配对齿数进入模型并影响中心距；
//   ③ ANSI 卡：方向 + 齿廓清单（由选项表下发）+ P/z → /api/card_preview；
//      纯中文/纯英文是两个卡类型（renderer 不同，模型 card 字段跟着走）；
//   ④ 出表：齿轮/ANSI 走 /api/card_export；无 at → 待放置件；有 at → 直插；
//   ⑤ 信息分层负断言：新面板的公式/口径/来源只在 title=，不进可见文本。
//
// 用法：node card_gui_smoke.mjs <spline_gui.html 路径>

import fs from 'node:fs';

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error('usage: node card_gui_smoke.mjs <spline_gui.html>');
  process.exit(2);
}
const html = fs.readFileSync(htmlPath, 'utf8');
const m = html.match(/<script>([\s\S]*?)<\/script>/);
if (!m) {
  console.error('no <script> found');
  process.exit(2);
}
const script = m[1];

const errors = [];
function check(cond, msg) {
  if (!cond) errors.push(msg);
}

// ── 选项表桩（四张卡 + spline 体系 + 齿轮/ANSI 选项）──
const CARD_TYPES = [
  { id: '花键参数表', aliases: ['spline'], label: '花键参数表', summary: 'stub GB', systems: [{ id: 'gb3478', label: 'GB', standard: 'stub' }], renderer: 'spline_table' },
  { id: '齿轮参数表', aliases: ['gear'], label: '齿轮参数表', summary: 'stub gear', systems: [], renderer: 'gear_table' },
  { id: 'ANSI花键参数表_中文', aliases: ['ansicn'], label: 'ANSI 花键参数表（纯中文）', summary: 'stub ansi cn', systems: [], renderer: 'ansi_table_cn' },
  { id: 'ANSI花键参数表_英文', aliases: ['ansien'], label: 'ANSI 花键参数表（纯英文）', summary: 'stub ansi en', systems: [], renderer: 'ansi_table_en' },
];
const GB_COLUMNS = Array.from({ length: 21 }, (_, i) => ({
  tag: `(内)t${i}`, label: `项${i}`, unit: '', formula: 'stub', source: 'stub',
}));
const SYSTEMS = [{
  id: 'gb3478', aliases: ['gb'], label: 'GB', standard: 'GB', note: 'stub', x_note: 'stub',
  sides: [{
    id: 'int', label: '内花键', title: 'stub', grade_note: 'stub', alpha_note: 'stub',
    grades: [6], fits: [{ fit: 'H', code: 'H', label: 'H', preferred_45: true, memo: 'stub' }],
    roots: [{ key: 'flat', label: '平齿根', alphas: [30], note: 'stub' }],
    alphas: [30], pin: { applicable: true, label: 'Dp', formula: 'stub', md_formula: 'stub', standard: 'stub' },
    columns: GB_COLUMNS,
  }],
}];
const GEAR_LABELS = [
  '法向模数', '齿数', '齿形角', '齿顶高系数', '螺旋角', '螺旋方向', '径向变位系数', '全齿高',
  '精度等级', '中心距及极限偏差', '配对齿轮图号', '配对齿轮齿数', '齿圈径向跳动公差',
  '公法线长度公差', '齿形公差', '齿距极限偏差', '齿向公差', '公法线', '公法线K',
];
const ANSI_INT = [
  '花键类型', '齿数', '径节', '压力角', '基圆直径', '节圆直径', '大径上差', '大径', '大径下差',
  '有效直径', '小径', '实际齿厚最大值', '作用齿厚最小值', '跨棒距上差', '跨棒距', '跨棒距下差', '量棒直径',
];
const ANSI_EXT = [
  '花键类型', '齿数', '径节', '压力角', '基圆直径', '节圆直径', '大径上差', '大径', '大径下差',
  '渐开线终止圆直径', '小径', '作用齿厚最大值', '实际齿厚最小值', '公法线上差', '公法线长度', '公法线下差', '跨测齿数',
];
const OPTIONS = {
  ok: true,
  card_types: CARD_TYPES,
  systems: SYSTEMS,
  pin_series: [1.0],
  gear_card: {
    expression_example: 'GEAR EX M3 Z20 ALPHA20 X0 DA66 DF52.5 BETA0 H30',
    expression_note: 'stub 表达式口径',
    missing_note: 'stub 缺项说明（GB/T 10095 未收）',
    columns: GEAR_LABELS.map((label) => ({ tag: label, label, unit: '', formula: 'stub', source: 'stub' })),
  },
  ansi_card: {
    profiles: [
      { id: 'ANSI30平齿根齿侧', code: 'ANSI30P', alpha: 30 },
      { id: 'ANSI30圆齿根齿侧', code: 'ANSI30R', alpha: 30 },
      { id: 'ANSI45圆齿根齿侧', code: 'ANSI45R', alpha: 45 },
    ],
    columns: { int: [], ext: [] },
    langs: [{ id: 'cn', label: '纯中文' }, { id: 'en', label: '纯英文' }],
    missing_note: 'stub ANSI 缺项（Table 4/5 未收）',
    note: 'stub ANSI 说明',
  },
};

// ── fetch 桩 ─────────────────────────────────────────────────────
let lastPreviewModel = null;
let lastExportModel = null;
let lastExportUrl = '';
let forcePreview404 = false;
const consoleErrors = [];
const realError = console.error.bind(console);
console.error = (...a) => { consoleErrors.push(a.join(' ')); };
function jsonResp(obj, status = 200) {
  return { ok: status < 400, status, json: async () => obj, text: async () => JSON.stringify(obj) };
}
function text404(path) {
  return {
    ok: false, status: 404,
    text: async () => 'not found',
    json: async () => { throw new SyntaxError(`${path} not valid JSON`); },
  };
}
function gearPreview(m) {
  const missingSet = new Set(['精度等级', '齿圈径向跳动公差', '公法线长度公差', '齿形公差', '齿距极限偏差', '齿向公差']);
  const items = GEAR_LABELS.map((label) => ({
    tag: label, label, unit: '', value: missingSet.has(label) ? '—' : (label === '中心距及极限偏差'
      ? (m.mate_z ? String(2 * (20 + Number(m.mate_z)) / 2) : '—')
      : `v-${label}`),
    formula: 'stub 公式', source: 'stub 来源', missing: missingSet.has(label) || (label === '中心距及极限偏差' && !m.mate_z),
  }));
  return jsonResp({ ok: true, card: '齿轮参数表', renderer: 'gear_table', readout: [{ k: '模数 m', v: '3' }], items, missing_note: 'stub 缺项说明' });
}
function ansiPreview(m) {
  const labels = m.side === 'ext' ? ANSI_EXT : ANSI_INT;
  const missingSet = new Set(['跨棒距', '量棒直径', '公法线长度', '跨测齿数', '大径上差', '大径下差']);
  const typeText = m.card.includes('英文') ? 'FLAT ROOT SIDE FIT' : '30°平齿根齿侧配合';
  const items = labels.map((label) => ({
    tag: label, label, unit: '', value: missingSet.has(label) ? '—' : (label === '花键类型' ? typeText : `v-${label}`),
    formula: 'stub 公式', source: 'stub 来源', missing: missingSet.has(label),
  }));
  return jsonResp({
    ok: true, card: m.card,
    renderer: m.card.includes('英文') ? 'ansi_table_en' : 'ansi_table_cn',
    readout: [{ k: '径节 P/Ps', v: '16/32' }], items, missing_note: 'stub ANSI 缺项',
  });
}
global.fetch = async (u, opts = {}) => {
  const url = String(u);
  if (url.startsWith('/api/spline_options')) return jsonResp(OPTIONS);
  if (url.startsWith('/api/card_preview')) {
    if (forcePreview404) return text404('/api/card_preview');
    const model = JSON.parse(opts.body || '{}');
    lastPreviewModel = model;
    if (model.card === '齿轮参数表') return gearPreview(model);
    if (String(model.card).startsWith('ANSI')) return ansiPreview(model);
    return jsonResp({ ok: false, error: 'stub 只支持新卡' }, 400);
  }
  if (url.startsWith('/api/card_export')) {
    lastExportUrl = url;
    lastExportModel = JSON.parse(opts.body || '{}');
    const v = String(lastExportModel.card).startsWith('ANSI') ? ansiPreview(lastExportModel)
      : lastExportModel.card === '齿轮参数表' ? gearPreview(lastExportModel)
      : jsonResp({ ok: false, error: 'bad card' }, 400);
    if (!v.ok) return v;
    return jsonResp({ ok: true, message: 'stub 已生成', pending: lastExportModel.at == null });
  }
  return jsonResp({ ok: true });
};

// ── 最小 DOM 垫片（照 spline_gui_smoke.mjs）────────────────────────
function mkEl(id) {
  const el = {
    id, tagName: 'DIV', type: '', dataset: {}, style: {}, value: '', checked: false,
    disabled: false, readOnly: false, textContent: '', children: [], options: [],
    _handlers: {}, _innerHTML: '',
    classList: {
      _on: new Set(),
      add(c) { this._on.add(c); },
      remove(c) { this._on.delete(c); },
      contains(c) { return this._on.has(c); },
      toggle(c, force) {
        const want = force === undefined ? !this._on.has(c) : !!force;
        if (want) this._on.add(c); else this._on.delete(c);
        return want;
      },
    },
    addEventListener(ev, fn) { (this._handlers[ev] ||= []).push(fn); },
    _fire(ev, target, extra) {
      const e = Object.assign({ target, type: ev }, extra || {});
      (this._handlers[ev] || []).forEach((f) => f(e));
    },
    appendChild(c) {
      if (this.tagName === 'SELECT' && c.tagName === 'OPTION') this.options.push(c);
      else this.children.push(c);
      return c;
    },
    removeChild(c) { this.children = this.children.filter((x) => x !== c); },
    querySelectorAll() { return []; },
    querySelector() { return null; },
    setSelectionRange() {},
    closest() { return null; },
    focus() {},
    remove() {},
    click() { this._fire('click', this); },
  };
  Object.defineProperty(el, 'innerHTML', {
    get() { return el._innerHTML; },
    set(v) { el._innerHTML = String(v); if (el._innerHTML === '') { el.children = []; el.options = []; } },
  });
  return el;
}
const els = new Map();
const SELECT_IDS = new Set(['cardType', 'sys', 'grade', 'fit', 'alpha', 'root', 'ansiSide', 'ansiProfile']);
const TEXTAREA_IDS = new Set(['expr', 'gearExpr']);
global.document = {
  getElementById(id) {
    if (!els.has(id)) {
      const e = mkEl(id);
      if (SELECT_IDS.has(id)) e.tagName = 'SELECT';
      if (TEXTAREA_IDS.has(id)) e.tagName = 'TEXTAREA';
      els.set(id, e);
    }
    return els.get(id);
  },
  createElement(tag) { const e = mkEl('dyn'); e.tagName = String(tag).toUpperCase(); return e; },
  createTextNode(t) { const e = mkEl('text'); e.textContent = String(t); return e; },
  querySelectorAll() { return []; },
  querySelector() { return null; },
  addEventListener() {},
  activeElement: null,
};
let closed = false;
global.window = { addEventListener() {}, close() { closed = true; } };
global.location = { search: '', href: 'http://127.0.0.1:9/spline' };
global.setTimeout = (fn) => { if (typeof fn === 'function') fn(); return 0; };
global.clearTimeout = () => {};
global.setInterval = () => 0;
global.URLSearchParams = class { constructor() {} get() { return null; } };

// ── 跑页面脚本 + 探针 ─────────────────────────────────────────────
const probed = script.replace(/\}\)\(\);\s*$/, `;globalThis.__card = {
  get opt() { return OPT; },
  get card() { return CARD; },
  get sys() { return SYS; },
  get side() { return SIDE; },
  panelOf, applyPanels, currentGearModel, currentAnsiModel, refresh, schedulePreview,
};
})();`);
try {
  (0, eval)(fs.readFileSync(htmlPath.replace(/[^/]+$/, 'ocsm_gui_common.js'), 'utf8'));
  (0, eval)(probed);
} catch (e) {
  errors.push('脚本求值异常: ' + (e && e.stack ? e.stack : e));
}
const tick = async (n = 6) => { for (let i = 0; i < n; i++) await new Promise((r) => setImmediate(r)); };
await tick();

const H = globalThis.__card;
const el = (id) => document.getElementById(id);
check(!!H, '探针 __card 未挂上（脚本初始化崩溃？）');
if (!H) report();

// ① 卡类型下拉 4 项；默认花键卡 → spline 面板可见
check(el('cardType').options.length === 4, `卡类型应 4 项：${el('cardType').options.map((o) => o.value)}`);
check(H.card && H.card.id === '花键参数表', `默认卡类型：${H.card && H.card.id}`);
check(el('cardType').disabled === false, '四张卡时卡类型下拉应可点');
check(H.panelOf(H.card) === 'spline', '默认应 spline 面板');
check(el('splinePanel').style.display === '' && el('gearPanel').style.display === 'none' && el('ansiPanel').style.display === 'none',
  '默认只有 spline 面板可见');
check(el('gearExpr').value.startsWith('GEAR EX'), `齿轮默认表达式：${el('gearExpr').value}`);
check(el('ansiProfile').options.length === 3, `ANSI 齿廓清单来自选项表：${el('ansiProfile').options.map((o) => o.value)}`);
check(el('gearHint').title.includes('10095'), `齿轮缺项说明应进 title：${el('gearHint').title}`);

// ② 切到齿轮卡：面板切换 + 19 项 + 缺项「—」；配对齿数影响中心距
el('cardType').value = '齿轮参数表';
el('cardType')._fire('change', el('cardType'));
await tick();
check(H.card.id === '齿轮参数表', `切卡后 card=${H.card.id}`);
check(H.panelOf(H.card) === 'gear', '齿轮面板');
check(el('gearPanel').style.display === '' && el('splinePanel').style.display === 'none', '齿轮面板可见/spline 隐藏');
await H.refresh();
check(lastPreviewModel && lastPreviewModel.card === '齿轮参数表', `预览模型 card：${JSON.stringify(lastPreviewModel)}`);
check(String(lastPreviewModel.expr).startsWith('GEAR EX'), `预览应带表达式：${lastPreviewModel.expr}`);
check(el('items').innerHTML.split('class="row"').length - 1 === 19, `齿轮卡应 19 项：${el('items').innerHTML.slice(0, 100)}`);
check(el('items').innerHTML.includes('—'), '缺项应显示「—」');
check(el('items').innerHTML.includes('title="stub 公式'), '公式应在行 title 里');
el('gearMateZ').value = '20';
el('gearMateZ')._fire('input', el('gearMateZ'));
await tick();
check(lastPreviewModel && Number(lastPreviewModel.mate_z) === 20, `配对齿数应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('40'), '给了配对齿数 → 中心距出现在预览里');
el('gearGrade').value = '7-7-7';
el('gearGrade')._fire('input', el('gearGrade'));
await tick();
check(lastPreviewModel.grade === '7-7-7', `精度等级应进模型：${JSON.stringify(lastPreviewModel)}`);

// 齿轮卡出表：无 at → /api/card_export + 待放置；有 at → 直插
el('ok').click();
await tick();
check(lastExportUrl.startsWith('/api/card_export'), `齿轮导出 URL：${lastExportUrl}`);
check(lastExportModel && lastExportModel.card === '齿轮参数表' && lastExportModel.at === null, `导出模型：${JSON.stringify(lastExportModel)}`);
check(closed === true, '出表成功后应自动关窗');
closed = false;
el('atX').value = '10'; el('atY').value = '20'; el('rot').value = '15';
el('ok').click();
await tick();
check(JSON.stringify(lastExportModel.at) === '[10,20]', `at 应进模型：${JSON.stringify(lastExportModel.at)}`);
el('atX').value = ''; el('atY').value = '';

// ③ 切到 ANSI 纯中文：17 项 + 中文类型值
el('cardType').value = 'ANSI花键参数表_中文';
el('cardType')._fire('change', el('cardType'));
await tick();
check(H.card.id === 'ANSI花键参数表_中文', `ANSI CN 卡：${H.card.id}`);
check(H.panelOf(H.card) === 'ansi', 'ANSI 面板');
check(el('ansiPanel').style.display === '' && el('gearPanel').style.display === 'none', 'ANSI 面板可见/齿轮隐藏');
await H.refresh();
check(lastPreviewModel.card === 'ANSI花键参数表_中文' && lastPreviewModel.side === 'int', `ANSI 预览模型：${JSON.stringify(lastPreviewModel)}`);
check(Number(lastPreviewModel.p) === 16 && Number(lastPreviewModel.z) === 20, `P/z 应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.split('class="row"').length - 1 === 17, `ANSI 卡应 17 项：${el('items').innerHTML.slice(0, 100)}`);
check(el('items').innerHTML.includes('30°平齿根齿侧配合'), '中文版类型值应为中文');
el('ansiSide').value = 'ext';
el('ansiSide')._fire('change', el('ansiSide'));
await tick();
check(lastPreviewModel.side === 'ext', `外花键应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('渐开线终止圆直径') || el('items').innerHTML.includes('v-渐开线终止圆直径'), '外花键应有渐开线终止圆直径行');
el('ansiProfile').value = 'ANSI45圆齿根齿侧';
el('ansiProfile')._fire('change', el('ansiProfile'));
await tick();
check(lastPreviewModel.profile === 'ANSI45圆齿根齿侧', `齿廓应进模型：${JSON.stringify(lastPreviewModel)}`);

// ④ 切到 ANSI 纯英文：同构，card 字段换英文卡
el('cardType').value = 'ANSI花键参数表_英文';
el('cardType')._fire('change', el('cardType'));
await tick();
await H.refresh();
check(lastPreviewModel.card === 'ANSI花键参数表_英文', `ANSI EN 卡：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('FLAT ROOT SIDE FIT'), '英文版类型值应为英文');
check(el('items').innerHTML.split('class="row"').length - 1 === 17, '英文版仍 17 项');
// ANSI 出表（有 at → 直插）
closed = false;
el('atX').value = '5'; el('atY').value = '6';
el('ok').click();
await tick();
check(lastExportUrl.startsWith('/api/card_export'), `ANSI 导出 URL：${lastExportUrl}`);
check(lastExportModel.card === 'ANSI花键参数表_英文' && JSON.stringify(lastExportModel.at) === '[5,6]', `ANSI 导出模型：${JSON.stringify(lastExportModel)}`);
el('atX').value = ''; el('atY').value = '';

// ⑤ 预览 404 → 红框可见（共享助手；不关窗）
forcePreview404 = true;
closed = false;
await H.refresh();
check((el('status').textContent || '').includes('404'), `预览 404 提示：${el('status').textContent}`);
check(el('status').className === 'bad' && String(el('status').style.background).toLowerCase() === '#fdecea',
  `404 应红框：class=${el('status').className} bg=${el('status').style.background}`);
forcePreview404 = false;

// ⑥ 信息分层负断言：新面板的缺项说明/口径只在 title=，常显没有「公式」「来源：」等
const visibleText = html
  .replace(/<script[\s\S]*?<\/script>/g, ' ')
  .replace(/<style[\s\S]*?<\/style>/g, ' ')
  .replace(/<[^>]+>/g, ' ');
for (const bad of ['Table 4/5', '公式', '来源：', '未臆造']) {
  check(!visibleText.includes(bad), `常显区不应含口径/来源「${bad}」`);
}

function report() {
  if (errors.length) {
    realError('智能卡片新卡冒烟失败：');
    for (const e of errors) realError(' - ' + e);
    process.exit(1);
  }
  console.log('智能卡片新卡冒烟通过');
}
report();
