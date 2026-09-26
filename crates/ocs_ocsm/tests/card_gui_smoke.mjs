// 智能卡片冒烟（node 最小 DOM 垫片 + fetch 桩）：锁住「表驱动 GUI 统一 + 五张新卡 + NF 公差」契约。
//
//   ① 统一外观：页面不再手写齿轮/ANSI/NF/DIN 面板；全部由 `card_types[].form` 驱动，
//      与 GB 面板同一套分区（顶部参数 / 中部主输入+读数 / 结果卡 / 底部按钮）；
//   ② 字段顺序/控件类型：顶部参数区控件顺序 = form.fields 顺序（只跳过 textarea 主输入）；
//   ③ 齿轮卡：表达式 + 配对齿数/图号/精度等级/中心距 → /api/card_preview，19 项、缺项「—」；
//   ④ ANSI 卡：方向/齿廓（选项表下发）+ P/z；纯中/纯英两个卡类型；
//   ⑤ NF 卡：A/m/z/定心/齿根/配合（6 字段）→ 18 项；公差 = R7/H7/p29 E（不再整片「—」）；
//      配合类别只影响预览读数；表外 A=210 → 跨棒距公差「—」；
//   ⑥ DIN 卡：12 字段（覆盖项独立成格）→ 26 项；缺口 m=5 → 公差「—」；
//   ⑦ 出表：非花键卡走 /api/card_export；无 at → 待放置件；有 at → 直插；
//   ⑧ 预览 404 → 红框可见（共享助手；不关窗）；信息分层负断言。
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

// ── 选项表桩：六张卡 + 体系 + 齿轮/ANSI/NF/DIN 选项 + 表驱动 form 字段清单 ──
const FIELD = (key, label, kind, extra = {}) => Object.assign({
  key, label, kind, placeholder: '', default: '', title: 'stub 口径',
  options: [], options_from: '', min: 0, step: 0, required: false,
}, extra);
const GEAR_FORM = {
  fields: [
    FIELD('expr', '齿轮齿形表达式（九字段；可从轴/齿轮生成器 GUI 复制）', 'textarea',
      { default: 'GEAR EX M3 Z20 ALPHA20 X0 DA66 DF52.5 BETA0 H30', required: true }),
    FIELD('mate_z', '配对齿轮齿数 z₂', 'number', { min: 2, step: 1 }),
    FIELD('mate_dwg', '配对齿轮图号', 'text'),
    FIELD('grade', '精度等级', 'text'),
    FIELD('center', '中心距 a', 'number', { step: 0.001 }),
  ],
  note: 'stub 齿轮提示',
  missing_note: 'stub 缺项说明（GB/T 10095 未收）',
};
const ANSI_FORM = {
  fields: [
    FIELD('side', '方向', 'select', {
      options: [{ value: 'int', label: '内花键' }, { value: 'ext', label: '外花键' }], default: 'int',
    }),
    FIELD('profile', '齿廓', 'select', { options_from: 'ansi_profiles' }),
    FIELD('p', '径节 P', 'number', { default: '16', min: 2.5, step: 0.5, required: true }),
    FIELD('z', '齿数 N', 'number', { default: '20', min: 3, step: 1, required: true }),
  ],
  note: 'stub ANSI 说明',
  missing_note: 'stub ANSI 缺项（Table 4/5 未收）',
};
const NF_FORM = {
  fields: [
    FIELD('a', '公称直径 A', 'number', { default: '300', step: 0.001, required: true }),
    FIELD('m', '模数 m', 'number', { default: '7.5', step: 0.001, required: true }),
    FIELD('z', '齿数 z', 'number', { default: '38', min: 3, step: 1 }),
    FIELD('centering', '定心方式', 'select', { options_from: 'nf_centering' }),
    FIELD('root', '齿根样式', 'select', { options_from: 'nf_roots' }),
    FIELD('fit', '配合类别', 'select', {
      options: [
        { value: 'loose', label: '松动' }, { value: 'slide', label: '滑动' },
        { value: 'fixed', label: '固定' }, { value: 'press', label: '压' },
      ],
      default: 'fixed',
    }),
  ],
  note: 'stub NF 说明（公差 p28 R7/H7 + p29 E）',
  missing_note: 'stub NF 缺项（(m,A) 不在 p29 或 ISO 档缺）',
};
const DIN_FORM = {
  fields: [
    FIELD('m', '模数 m', 'number', { default: '3', step: 0.001, required: true }),
    FIELD('z', '齿数 z', 'number', { default: '38', min: 3, step: 1, required: true }),
    FIELD('d_b', '基准直径 d_B', 'number', { default: '120', step: 0.001, required: true }),
    FIELD('hub', '内花键配合', 'text', { default: '9H' }),
    FIELD('shaft', '外花键配合', 'text', { default: '8f' }),
    FIELD('e2', 'e₂=s₁ 覆盖', 'number', { step: 0.001 }),
    FIELD('ae', 'Ae 覆盖', 'number', { step: 0.001 }),
    FIELD('as_', 'As 覆盖', 'number', { step: 0.001 }),
    FIELD('tact_n', 'Tact(N) 覆盖', 'number', { step: 0.0001 }),
    FIELD('teff_n', 'Teff(N) 覆盖', 'number', { step: 0.0001 }),
    FIELD('tact_w', 'Tact(W) 覆盖', 'number', { step: 0.0001 }),
    FIELD('teff_w', 'Teff(W) 覆盖', 'number', { step: 0.0001 }),
  ],
  note: 'stub DIN 说明',
  missing_note: 'stub DIN 缺项（Table 7 缺口）',
};
const CARD_TYPES = [
  { id: '花键参数表', aliases: ['spline'], label: '花键参数表', summary: 'stub GB', systems: [{ id: 'gb3478', label: 'GB', standard: 'stub' }], renderer: 'spline_table', form: null },
  { id: '齿轮参数表', aliases: ['gear'], label: '齿轮参数表', summary: 'stub gear', systems: [], renderer: 'gear_table', form: GEAR_FORM },
  { id: 'ANSI花键参数表_中文', aliases: ['ansicn'], label: 'ANSI 花键参数表（纯中文）', summary: 'stub ansi cn', systems: [], renderer: 'ansi_table_cn', form: ANSI_FORM },
  { id: 'ANSI花键参数表_英文', aliases: ['ansien'], label: 'ANSI 花键参数表（纯英文）', summary: 'stub ansi en', systems: [], renderer: 'ansi_table_en', form: ANSI_FORM },
  { id: 'NF内花键参数表', aliases: ['nf'], label: 'NF 内花键参数表', summary: 'stub nf', systems: [], renderer: 'nf_table', form: NF_FORM },
  { id: 'DIN花键参数表', aliases: ['din'], label: 'DIN 花键参数表', summary: 'stub din', systems: [], renderer: 'din_table', form: DIN_FORM },
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
  nf_card: {
    columns: Array.from({ length: 18 }, (_, i) => ({ tag: `nf${i}`, label: `NF项${i}`, unit: '', formula: 'stub', source: 'stub' })),
    modules: [0.5, 1, 2.5, 7.5],
    centering: [{ id: 'outer', label: '外径定心（Az=A）' }, { id: 'flank', label: '齿面定心（Az=A+0.3m）' }],
    roots: [{ id: 'flat', label: '平齿根' }, { id: 'fillet', label: '圆齿根' }],
    fits: [{ id: 'loose', label: '松动' }, { id: 'slide', label: '滑动' }, { id: 'fixed', label: '固定' }, { id: 'press', label: '压' }],
    missing_note: 'stub NF 缺项（p29 表外/ISO 档缺）',
    note: 'stub NF 说明',
  },
  din_card: {
    columns: Array.from({ length: 26 }, (_, i) => ({ tag: `din${i}`, label: `DIN项${i}`, unit: '', formula: 'stub', source: 'stub' })),
    grades: [6, 7, 8, 9, 10, 11, 12],
    default: { m: 3, z: 38, d_b: 120, hub: '9H', shaft: '8f' },
    missing_note: 'stub DIN 缺项（Table 7 缺口）',
    note: 'stub DIN 说明',
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
function gearPreview(mm) {
  const missingSet = new Set(['精度等级', '齿圈径向跳动公差', '公法线长度公差', '齿形公差', '齿距极限偏差', '齿向公差']);
  const items = GEAR_LABELS.map((label) => ({
    tag: label, label, unit: '', value: missingSet.has(label) ? '—' : (label === '中心距及极限偏差'
      ? (mm.mate_z ? String(2 * (20 + Number(mm.mate_z)) / 2) : '—')
      : `v-${label}`),
    formula: 'stub 公式', source: 'stub 来源', missing: missingSet.has(label) || (label === '中心距及极限偏差' && !mm.mate_z),
  }));
  return jsonResp({ ok: true, card: '齿轮参数表', renderer: 'gear_table', readout: [{ k: '模数 m', v: '3' }], items, missing_note: 'stub 缺项说明' });
}
function ansiPreview(mm) {
  const labels = mm.side === 'ext' ? ANSI_EXT : ANSI_INT;
  const missingSet = new Set(['跨棒距', '量棒直径', '公法线长度', '跨测齿数', '大径上差', '大径下差']);
  const typeText = mm.card.includes('英文') ? 'FLAT ROOT SIDE FIT' : '30°平齿根齿侧配合';
  const items = labels.map((label) => ({
    tag: label, label, unit: '', value: missingSet.has(label) ? '—' : (label === '花键类型' ? typeText : `v-${label}`),
    formula: 'stub 公式', source: 'stub 来源', missing: missingSet.has(label),
  }));
  return jsonResp({
    ok: true, card: mm.card,
    renderer: mm.card.includes('英文') ? 'ansi_table_en' : 'ansi_table_cn',
    readout: [{ k: '径节 P/Ps', v: '16/32' }], items, missing_note: 'stub ANSI 缺项',
  });
}
const NF_ITEMS = [
  '执行标准', '定心方式', '模数', '齿数', '压力角', '齿根样式', '加工方法', '大径Az', '小径D',
  '基准尺寸', '量棒直径V', '跨棒距G', '大径上差', '大径下差', '小径上差', '小径下差', '跨棒距上差', '跨棒距下差',
];
function nfPreview(mm) {
  const gap = Number(mm.a) === 210;
  const missingSet = gap ? new Set(['齿数', '量棒直径V', '跨棒距G', '跨棒距上差', '跨棒距下差']) : new Set();
  const fitLabel = { loose: '松动', slide: '滑动', fixed: '固定', press: '压' }[mm.fit || 'fixed'];
  const press = (mm.fit || 'fixed') === 'press';
  const val = (tag) => tag === '大径Az' ? (mm.centering === 'flank' ? '302.25' : '300')
    : tag === '小径D' ? '285' : tag === '量棒直径V' ? '15' : tag === '跨棒距G' ? '270.508'
    : tag === '定心方式' ? (mm.centering === 'flank' ? '齿面定心' : '外径定心')
    : tag === '大径上差' ? '-0.078' : tag === '大径下差' ? '-0.130'
    : tag === '小径上差' ? '+0.052' : tag === '小径下差' ? '0'
    : tag === '跨棒距上差' ? (gap ? '—' : (press ? '+0.138' : '+0.052'))
    : tag === '跨棒距下差' ? (gap ? '—' : (press ? '+0.054' : '0'))
    : missingSet.has(tag) ? '—' : `v-${tag}`;
  const items = NF_ITEMS.map((tag) => ({
    tag, label: tag, unit: '', value: val(tag),
    formula: 'stub 公式', source: 'stub 来源', missing: missingSet.has(tag),
  }));
  const readout = gap
    ? [{ k: 'p29 内花键偏差（µm）', v: '—' }]
    : [
      { k: 'p29 内花键偏差（µm）', v: 'E +52/+0（+0.052/0 mm）；xm +76/+0（µm）' },
      { k: `配对外花键·${fitLabel}偏差（µm；p29）`, v: press ? 'E +138/+54；xm +202/+79' : 'E +42/-42；xm +61/-61' },
    ];
  return jsonResp({
    ok: true, card: mm.card, renderer: 'nf_table', fit: mm.fit || 'fixed',
    readout, items, missing: [...missingSet], missing_note: 'stub NF 缺项（p29 表外/ISO 档缺）',
  });
}
const DIN_TAGS = ['N标记', 'N齿数', 'N模数', 'N压力角', 'N齿根圆', 'N齿根成形圆', 'N齿顶圆', 'N槽宽max', 'N槽宽min', 'N槽宽eff', 'N量圆', 'N量距max', 'N量距min', 'W标记', 'W齿数', 'W模数', 'W压力角', 'W齿顶圆', 'W齿根成形圆', 'W齿根圆', 'W齿厚svmax', 'W齿厚smax', 'W齿厚smin', 'W量圆', 'W量距max', 'W量距min'];
function dinPreview(mm) {
  const missingSet = new Set();
  const gap = Number(mm.m) >= 5;
  if (gap) { for (const t of ['N槽宽max', 'N槽宽min', 'N槽宽eff', 'W齿厚svmax', 'W齿厚smax', 'W齿厚smin']) missingSet.add(t); }
  const val = (tag) => tag === 'N标记' ? 'Nabe DIN 5480 – N120×3×38×9H'
    : tag === 'N槽宽max' ? (gap ? '—' : '6.361') : tag === 'N槽宽eff' ? (gap ? '—' : '6.271')
    : tag === 'W齿厚svmax' ? (gap ? '—' : '6.243') : `v-${tag}`;
  const items = DIN_TAGS.map((tag) => ({
    tag, label: tag, unit: '', value: val(tag),
    formula: 'stub 公式', source: 'stub 来源', missing: missingSet.has(tag),
  }));
  return jsonResp({
    ok: true, card: mm.card, renderer: 'din_table', anchor: !gap,
    readout: [{ k: 'e₂ = s₁（名义）', v: gap ? '—' : '6.271' }], items,
    missing: [...missingSet], missing_note: 'stub DIN 缺项',
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
    if (model.card === 'NF内花键参数表') return nfPreview(model);
    if (model.card === 'DIN花键参数表') return dinPreview(model);
    return jsonResp({ ok: false, error: 'stub 只支持新卡' }, 400);
  }
  if (url.startsWith('/api/card_export')) {
    lastExportUrl = url;
    lastExportModel = JSON.parse(opts.body || '{}');
    const v = String(lastExportModel.card).startsWith('ANSI') ? ansiPreview(lastExportModel)
      : lastExportModel.card === '齿轮参数表' ? gearPreview(lastExportModel)
      : lastExportModel.card === 'NF内花键参数表' ? nfPreview(lastExportModel)
      : lastExportModel.card === 'DIN花键参数表' ? dinPreview(lastExportModel)
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
const SELECT_IDS = new Set(['cardType', 'sys', 'grade', 'fit', 'alpha', 'root']);
const TEXTAREA_IDS = new Set(['expr']);
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
  panelOf, applyPanels, formOf, renderCardForm, currentCardModel, refresh, schedulePreview,
  formControl: (k) => FORM_CTL.get(k),
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

// ── ① 统一外观：表驱动骨架 + GB 面板同款分区 ──────────────────────
check(el('cardType').options.length === 6, `卡类型应 6 项：${el('cardType').options.map((o) => o.value)}`);
check(H.card && H.card.id === '花键参数表', `默认卡类型：${H.card && H.card.id}`);
check(H.panelOf(H.card) === 'spline', '默认应 spline 面板');
check(el('splinePanel').style.display === '' && el('cardPanel').style.display === 'none',
  '默认只有 GB 面板可见');
check(!html.includes('id="gearExpr"') && !html.includes('id="nfA"') && !html.includes('id="dinM"'),
  '页面不应再有手写卡专属控件 id');
// 通用骨架的分区/类名与 GB 面板同款（静态结构断言；以后加卡自动继承）
check(html.includes('id="cardPanel"') && html.includes('id="formParams"') && html.includes('id="formMain"'),
  '缺通用卡片面板骨架');
check(html.includes('<div class="card mid">') && html.includes('<div class="card top">') && html.includes('class="card bot"'),
  '分区/按钮位置应保持');
check(!html.includes('id="gearPanel"') && !html.includes('id="ansiPanel"') && !html.includes('id="nfPanel"') && !html.includes('id="dinPanel"'),
  '不应再给每卡写一套 HTML 面板');
check(html.includes('id="cardHint"') && html.includes('id="cardReadout"'), '通用面板缺提示/读数位');

// ── ② 切到齿轮卡：字段顺序/控件类型 + 19 项 + 配对齿数影响中心距 ──
el('cardType').value = '齿轮参数表';
el('cardType')._fire('change', el('cardType'));
await tick();
check(H.card.id === '齿轮参数表', `切卡后 card=${H.card.id}`);
check(H.panelOf(H.card) === 'form', '齿轮应走通用面板');
check(el('cardPanel').style.display === '' && el('splinePanel').style.display === 'none', '通用面板可见/GB 隐藏');
check(el('cardHint').title.includes('10095'), `缺项说明应进 title：${el('cardHint').title}`);
// 字段顺序 = form 声明顺序；textarea 只出在中部主输入区
const gearSeq = el('formParams').children.filter((c) => c.id && c.id.startsWith('f_')).map((c) => c.id);
check(gearSeq.join(',') === 'f_mate_z,f_mate_dwg,f_grade,f_center',
  `齿轮顶部字段顺序：${gearSeq}`);
check(el('formMain').children.filter((c) => c.id && c.id.startsWith('f_')).map((c) => c.id).join(',') === 'f_expr',
  '齿轮表达式应在中部主输入区');
check(H.formControl('mate_z').tagName === 'INPUT' && H.formControl('mate_z').type === 'number',
  '配对齿数应为 number 控件');
check(H.formControl('mate_dwg').type === 'text', '图号应为 text 控件');
check(H.formControl('expr').tagName === 'TEXTAREA', '表达式应为 textarea');
check(H.formControl('expr').value.startsWith('GEAR EX'), `齿轮默认表达式：${H.formControl('expr').value}`);
await H.refresh();
check(lastPreviewModel && lastPreviewModel.card === '齿轮参数表', `预览模型 card：${JSON.stringify(lastPreviewModel)}`);
check(String(lastPreviewModel.expr).startsWith('GEAR EX'), `预览应带表达式：${lastPreviewModel.expr}`);
check(el('items').innerHTML.split('class="row"').length - 1 === 19, `齿轮卡应 19 项：${el('items').innerHTML.slice(0, 100)}`);
check(el('items').innerHTML.includes('—'), '缺项应显示「—」');
check(el('items').innerHTML.includes('title="stub 公式'), '公式应在行 title 里');
H.formControl('mate_z').value = '20';
H.formControl('mate_z')._fire('input', H.formControl('mate_z'));
await tick();
check(lastPreviewModel && Number(lastPreviewModel.mate_z) === 20, `配对齿数应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('40'), '给了配对齿数 → 中心距出现在预览里');
H.formControl('grade').value = '7-7-7';
H.formControl('grade')._fire('input', H.formControl('grade'));
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

// ── ③ ANSI 纯中文：方向/齿廓（选项表下发）+ P/z + 17 项 ───────────
el('cardType').value = 'ANSI花键参数表_中文';
el('cardType')._fire('change', el('cardType'));
await tick();
check(H.card.id === 'ANSI花键参数表_中文', `ANSI CN 卡：${H.card.id}`);
check(H.panelOf(H.card) === 'form', 'ANSI 面板为通用骨架');
check(el('cardHint').title.includes('Table 4/5') || el('cardHint').title.includes('ANSI'), `ANSI 缺项进 title：${el('cardHint').title}`);
check(H.formControl('side').tagName === 'SELECT' && H.formControl('side').options.length === 2, 'ANSI 方向应 select');
check(H.formControl('profile').options.length === 3, `ANSI 齿廓清单来自选项表：${H.formControl('profile').options.map((o) => o.value)}`);
check(H.formControl('p').value === '16' && H.formControl('z').value === '20', 'ANSI 默认 P16/N20');
const ansiSeq = el('formParams').children.filter((c) => c.id && c.id.startsWith('f_')).map((c) => c.id);
check(ansiSeq.join(',') === 'f_side,f_profile,f_p,f_z', `ANSI 字段顺序：${ansiSeq}`);
await H.refresh();
check(lastPreviewModel.card === 'ANSI花键参数表_中文' && lastPreviewModel.side === 'int', `ANSI 预览模型：${JSON.stringify(lastPreviewModel)}`);
check(Number(lastPreviewModel.p) === 16 && Number(lastPreviewModel.z) === 20, `P/z 应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.split('class="row"').length - 1 === 17, `ANSI 卡应 17 项：${el('items').innerHTML.slice(0, 100)}`);
check(el('items').innerHTML.includes('30°平齿根齿侧配合'), '中文版类型值应为中文');
H.formControl('side').value = 'ext';
H.formControl('side')._fire('change', H.formControl('side'));
await tick();
check(lastPreviewModel.side === 'ext', `外花键应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('渐开线终止圆直径') || el('items').innerHTML.includes('v-渐开线终止圆直径'), '外花键应有渐开线终止圆直径行');
H.formControl('profile').value = 'ANSI45圆齿根齿侧';
H.formControl('profile')._fire('change', H.formControl('profile'));
await tick();
check(lastPreviewModel.profile === 'ANSI45圆齿根齿侧', `齿廓应进模型：${JSON.stringify(lastPreviewModel)}`);

// ── ④ ANSI 纯英文：同构，card 字段换英文卡 ────────────────────────
el('cardType').value = 'ANSI花键参数表_英文';
el('cardType')._fire('change', el('cardType'));
await tick();
await H.refresh();
check(lastPreviewModel.card === 'ANSI花键参数表_英文', `ANSI EN 卡：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('FLAT ROOT SIDE FIT'), '英文版类型值应为英文');
check(el('items').innerHTML.split('class="row"').length - 1 === 17, '英文版仍 17 项');
closed = false;
el('atX').value = '5'; el('atY').value = '6';
el('ok').click();
await tick();
check(lastExportUrl.startsWith('/api/card_export'), `ANSI 导出 URL：${lastExportUrl}`);
check(lastExportModel.card === 'ANSI花键参数表_英文' && JSON.stringify(lastExportModel.at) === '[5,6]', `ANSI 导出模型：${JSON.stringify(lastExportModel)}`);
el('atX').value = ''; el('atY').value = '';

// ── ⑤ NF 内花键参数表：6 字段 + 公差不再整片「—」+ 配合只动读数 ──
el('cardType').value = 'NF内花键参数表';
el('cardType')._fire('change', el('cardType'));
await tick();
check(H.card.id === 'NF内花键参数表', `NF 卡：${H.card.id}`);
check(el('cardHint').title.includes('p29'), `NF 缺项说明应进 title：${el('cardHint').title}`);
const nfSeq = el('formParams').children.filter((c) => c.id && c.id.startsWith('f_')).map((c) => c.id);
check(nfSeq.join(',') === 'f_a,f_m,f_z,f_centering,f_root,f_fit', `NF 字段顺序：${nfSeq}`);
check(H.formControl('a').value === '300' && H.formControl('m').value === '7.5' && H.formControl('z').value === '38', 'NF 默认示例 A300/M7.5/Z38');
check(H.formControl('centering').options.length === 2 && H.formControl('root').options.length === 2, 'NF 定心/齿根清单来自选项表');
check(H.formControl('fit').options.length === 4 && H.formControl('fit').value === 'fixed', 'NF 配合类别四档、默认固定');
await H.refresh();
check(lastPreviewModel && lastPreviewModel.card === 'NF内花键参数表', `NF 预览模型：${JSON.stringify(lastPreviewModel)}`);
check(Number(lastPreviewModel.a) === 300 && Number(lastPreviewModel.m) === 7.5 && Number(lastPreviewModel.z) === 38, `A/m/z 应进模型：${JSON.stringify(lastPreviewModel)}`);
check(lastPreviewModel.fit === 'fixed', `NF 配合应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.split('class="row"').length - 1 === 18, `NF 卡应 18 项：${el('items').innerHTML.slice(0, 100)}`);
check(el('items').innerHTML.includes('270.508'), 'NF 锚点跨棒距应出现在预览里');
// 修改②：6 个公差格必须有值（R7/H7/p29 E），不再整片「—」
const tolLabels = ['大径上差', '大径下差', '小径上差', '小径下差', '跨棒距上差', '跨棒距下差'];
for (const t of tolLabels) {
  const row = el('items').innerHTML.split('class="row"').find((r) => r.includes('>' + t + '<')) || '';
  check(row !== '' && !row.includes('>—<'), `NF 公差 ${t} 应有值：${row.slice(0, 120)}`);
}
check(el('items').innerHTML.includes('+0.052') && el('items').innerHTML.includes('-0.078'), 'NF 公差应含 R7/H7/p29 数值');
check(el('cardReadout').innerHTML.includes('p29'), 'NF 读数应含 p29 出处');
check(el('cardReadout').innerHTML.includes('E +52/+0'), 'NF 读数应含内花键 E 偏差');
check(el('cardReadout').innerHTML.includes('固定'), 'NF 读数应含所选配合');
// 配合类别 → 只影响配对外花键读数（不动内花键公差）
H.formControl('fit').value = 'press';
H.formControl('fit')._fire('change', H.formControl('fit'));
await tick();
check(lastPreviewModel.fit === 'press', `配合类别应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('cardReadout').innerHTML.includes('压') && el('cardReadout').innerHTML.includes('+138/+54'), '换成压配合 → 配对外花键读数随之变');
// 定心方式 → Az
H.formControl('centering').value = 'flank';
H.formControl('centering')._fire('change', H.formControl('centering'));
await tick();
check(lastPreviewModel.centering === 'flank', `定心方式应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('302.25'), '齿面定心 → Az=302.25');
// NF 出表（无 at → 待放置；有 at → 直插）
closed = false;
el('ok').click();
await tick();
check(lastExportUrl.startsWith('/api/card_export'), `NF 导出 URL：${lastExportUrl}`);
check(lastExportModel.card === 'NF内花键参数表' && lastExportModel.at === null, `NF 导出模型：${JSON.stringify(lastExportModel)}`);
check(closed === true, 'NF 出表成功后应自动关窗');
closed = false;
el('atX').value = '7'; el('atY').value = '8';
el('ok').click();
await tick();
check(JSON.stringify(lastExportModel.at) === '[7,8]', `NF at 应进模型：${JSON.stringify(lastExportModel.at)}`);
el('atX').value = ''; el('atY').value = '';
// 表外 A=210 → 跨棒距公差「—」（不外推）
H.formControl('centering').value = 'outer';
H.formControl('centering')._fire('change', H.formControl('centering'));
H.formControl('a').value = '210';
H.formControl('a')._fire('input', H.formControl('a'));
await tick();
check(lastPreviewModel.a === 210, `表外 A 应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.includes('—'), '表外 A=210 → 标缺「—」');

// ── ⑥ DIN 花键参数表：12 字段 + 26 项 + 缺口 ─────────────────────
el('cardType').value = 'DIN花键参数表';
el('cardType')._fire('change', el('cardType'));
await tick();
check(H.card.id === 'DIN花键参数表', `DIN 卡：${H.card.id}`);
check(el('cardHint').title.includes('Table 7'), `DIN 缺项说明应进 title：${el('cardHint').title}`);
const dinSeq = el('formParams').children.filter((c) => c.id && c.id.startsWith('f_')).map((c) => c.id);
check(dinSeq.join(',') === 'f_m,f_z,f_d_b,f_hub,f_shaft,f_e2,f_ae,f_as_,f_tact_n,f_teff_n,f_tact_w,f_teff_w', `DIN 字段顺序：${dinSeq}`);
check(H.formControl('m').value === '3' && H.formControl('z').value === '38' && H.formControl('d_b').value === '120', 'DIN 默认示例 M3/Z38/B120');
check(H.formControl('hub').value === '9H' && H.formControl('shaft').value === '8f', 'DIN 默认配合 9H/8f');
await H.refresh();
check(lastPreviewModel && lastPreviewModel.card === 'DIN花键参数表', `DIN 预览模型：${JSON.stringify(lastPreviewModel)}`);
check(Number(lastPreviewModel.m) === 3 && Number(lastPreviewModel.z) === 38 && Number(lastPreviewModel.d_b) === 120, `DIN m/z/d_B 应进模型：${JSON.stringify(lastPreviewModel)}`);
check(lastPreviewModel.hub === '9H' && lastPreviewModel.shaft === '8f', `DIN 配合应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('items').innerHTML.split('class="row"').length - 1 === 26, `DIN 卡应 26 项：${el('items').innerHTML.slice(0, 100)}`);
check(el('items').innerHTML.includes('Nabe DIN 5480 – N120×3×38×9H'), 'DIN 标记行应出现在预览里');
check(el('items').innerHTML.includes('6.361') && el('items').innerHTML.includes('6.243'), 'DIN 锚点 e/s 应出现在预览里');
H.formControl('ae').value = '0';
H.formControl('ae')._fire('input', H.formControl('ae'));
H.formControl('as_').value = '-0.028';
H.formControl('as_')._fire('input', H.formControl('as_'));
await tick();
check(Number(lastPreviewModel.ae) === 0 && Number(lastPreviewModel.as_) < 0, `DIN 覆盖 ae/as 应进模型：${JSON.stringify(lastPreviewModel)}`);
H.formControl('ae').value = '';
H.formControl('ae')._fire('input', H.formControl('ae'));
await tick();
// DIN 出表（无 at → 待放置；有 at → 直插）
closed = false;
el('ok').click();
await tick();
check(lastExportUrl.startsWith('/api/card_export'), `DIN 导出 URL：${lastExportUrl}`);
check(lastExportModel.card === 'DIN花键参数表' && lastExportModel.at === null, `DIN 导出模型：${JSON.stringify(lastExportModel)}`);
check(closed === true, 'DIN 出表成功后应自动关窗');
closed = false;
el('atX').value = '9'; el('atY').value = '10';
el('ok').click();
await tick();
check(JSON.stringify(lastExportModel.at) === '[9,10]', `DIN at 应进模型：${JSON.stringify(lastExportModel.at)}`);
el('atX').value = ''; el('atY').value = '';
// DIN 缺口路径：m=5 → 公差「—」
H.formControl('m').value = '5'; H.formControl('z').value = '16'; H.formControl('d_b').value = '80';
H.formControl('m')._fire('input', H.formControl('m'));
await tick();
check(el('items').innerHTML.includes('—'), 'DIN 模数组缺口应显示「—」');
H.formControl('m').value = '3'; H.formControl('z').value = '38'; H.formControl('d_b').value = '120';
H.formControl('m')._fire('input', H.formControl('m'));
await tick();

// ── ⑦ 预览 404 → 红框可见（共享助手；不关窗）─────────────────────
forcePreview404 = true;
closed = false;
await H.refresh();
check((el('status').textContent || '').includes('404'), `预览 404 提示：${el('status').textContent}`);
check(el('status').className === 'bad' && String(el('status').style.background).toLowerCase() === '#fdecea',
  `404 应红框：class=${el('status').className} bg=${el('status').style.background}`);
forcePreview404 = false;

// ── ⑧ 信息分层负断言：常显区不含口径/来源 ───────────────────────
const visibleText = html
  .replace(/<script[\s\S]*?<\/script>/g, ' ')
  .replace(/<style[\s\S]*?<\/style>/g, ' ')
  .replace(/<[^>]+>/g, ' ');
for (const bad of ['Table 4/5', '公式', '来源：', '未臆造']) {
  check(!visibleText.includes(bad), `常显区不应含口径/来源「${bad}」`);
}

function report() {
  if (errors.length) {
    realError('智能卡片冒烟失败：');
    for (const e of errors) realError(' - ' + e);
    process.exit(1);
  }
  console.log('智能卡片冒烟通过');
}
report();
