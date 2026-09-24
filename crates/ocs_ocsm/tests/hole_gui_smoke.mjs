// hole_gui.html 运行时/行为冒烟（node 最小 DOM 垫片 + 最小 fetch 桩）。
//
// 目的：锁住孔生成器的交互契约（node --check / el("id") 静态扫描查不出）：
//   ① 打开即「螺纹孔 M10」：自动螺纹长 15（1.5d）、自动孔深 18（有效 15+2P）、
//      读数 D1=10 / D=8.376、自动输入框只读；
//   ② 简单孔：两个「自动」都置灰且取消勾选；子类型 = 钻头大小/自定义/螺栓间隙；
//   ③ 不带螺纹 + 螺栓间隙：配合可选（精/中等/粗装配）；其余子类型配合置灰；
//   ④ 沉头/埋头：带螺纹开关可切；不带螺纹时大小列表按 GB/T 152.3/152.2 过滤；
//   ⑤ 贯通：孔深自动置灰；全长：孔深自动置灰、螺纹数值框置灰；
//   ⑥ 确定 → POST /api/hole_export（带当前模型）。
//
// 用法：node hole_gui_smoke.mjs <hole_gui.html 路径>

import fs from 'node:fs';

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error('usage: node hole_gui_smoke.mjs <hole_gui.html>');
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

// ── 数据表桩（形状与 /api/hole_sizes 一致；只取测试用到的规格）──
const COARSE = [
  { name: 'M4', d: 4, p: 0.7, d1: 3.242, d2: 3.545, fine: false },
  { name: 'M5', d: 5, p: 0.8, d1: 4.134, d2: 4.48, fine: false },
  { name: 'M6', d: 6, p: 1.0, d1: 4.917, d2: 5.35, fine: false },
  { name: 'M8', d: 8, p: 1.25, d1: 6.647, d2: 7.188, fine: false },
  { name: 'M10', d: 10, p: 1.5, d1: 8.376, d2: 9.026, fine: false },
  { name: 'M12', d: 12, p: 1.75, d1: 10.106, d2: 10.863, fine: false },
  { name: 'M16', d: 16, p: 2.0, d1: 13.835, d2: 14.701, fine: false },
  { name: 'M20', d: 20, p: 2.5, d1: 17.294, d2: 18.376, fine: false },
  { name: 'M24', d: 24, p: 3.0, d1: 20.752, d2: 22.051, fine: false },
  { name: 'M30', d: 30, p: 3.5, d1: 26.211, d2: 27.727, fine: false },
  { name: 'M36', d: 36, p: 4.0, d1: 31.67, d2: 33.402, fine: false },
];
const FINE = [
  { name: 'M10×1.25', d: 10, p: 1.25, d1: 8.647, d2: 9.188, fine: true },
  { name: 'M10×1', d: 10, p: 1.0, d1: 8.917, d2: 9.35, fine: true },
  { name: 'M12×1.25', d: 12, p: 1.25, d1: 10.647, d2: 11.188, fine: true },
];
const TAP = [
  { name: 'M4×0.7', d: 4, p: 0.7, drill: 3.3 },
  { name: 'M5×0.8', d: 5, p: 0.8, drill: 4.2 },
  { name: 'M6×1', d: 6, p: 1.0, drill: 5.0 },
  { name: 'M8×1.25', d: 8, p: 1.25, drill: 6.8 },
  { name: 'M10×1.5', d: 10, p: 1.5, drill: 8.5 },
  { name: 'M10×1.25', d: 10, p: 1.25, drill: 8.8 },
  { name: 'M12×1.75', d: 12, p: 1.75, drill: 10.3 },
  { name: 'M16×2', d: 16, p: 2.0, drill: 14.0 },
  { name: 'M20×2.5', d: 20, p: 2.5, drill: 17.5 },
  { name: 'M24×3', d: 24, p: 3.0, drill: 21.0 },
  { name: 'M30×3.5', d: 30, p: 3.5, drill: 26.5 },
  { name: 'M36×4', d: 36, p: 4.0, drill: 32.0 },
];
const CBORE = [4, 5, 6, 8, 10, 12, 16, 20, 24, 30, 36].map((d) => ({
  name: `M${d}`, d, d2: d === 10 ? 18 : d * 1.8, t: d === 10 ? 11 : d * 1.1, d1: d,
}));
const CSINK = [4, 6, 8, 10, 12, 16, 20].map((d) => ({
  name: `M${d}`, d, d2: d === 10 ? 20.3 : d * 2, t: d === 10 ? 5.0 : d / 2, d1: d,
}));
const CLEAR = [4, 6, 8, 10, 12, 16, 20].map((d) => ({
  name: `M${d}`, d,
  close: d + 0.5, normal: d + 1, loose: d + 2,
}));
CLEAR[3] = { name: 'M10', d: 10, close: 10.5, normal: 11, loose: 12 };
const SIZES = {
  ok: true,
  coarse: COARSE, fine: FINE, tap: TAP,
  counterbore: { code: 'GB/T 152.3-1988', rows: CBORE },
  countersink: { code: 'GB/T 152.2-1988', rows: CSINK },
  clearance: { code: 'GB/T 5277-1985', rows: CLEAR },
};

// ── 最小 DOM 垫片 ────────────────────────────────────────────────
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
const SELECT_IDS = new Set(['std', 'subtype', 'size', 'fit', 'range']);
global.document = {
  getElementById(id) {
    if (!els.has(id)) {
      const e = mkEl(id);
      if (SELECT_IDS.has(id)) e.tagName = 'SELECT';
      els.set(id, e);
    }
    return els.get(id);
  },
  createElement(tag) {
    const e = mkEl('dyn');
    e.tagName = String(tag).toUpperCase();
    return e;
  },
  createTextNode(t) { const e = mkEl('text'); e.textContent = String(t); return e; },
  querySelectorAll() { return []; },
  querySelector() { return null; },
  addEventListener() {},
};
global.window = { addEventListener() {}, close() {} };
global.location = { search: '', href: 'http://127.0.0.1:9/hole' };
global.setTimeout = () => 0;
global.clearTimeout = () => {};
global.setInterval = () => 0;
global.URLSearchParams = class {
  constructor() {}
  get() { return null; }
};

// ── fetch 桩：复刻后端规则的最小实现 ─────────────────────────────
let lastPreviewModel = null;
let lastExportModel = null;
let lastExportUrl = '';
function jsonResp(obj, status = 200) {
  return { ok: status < 400, status, json: async () => obj, text: async () => JSON.stringify(obj) };
}
function fmt(n) {
  return Number(Number(n).toFixed(3)).toString();
}
function threadedOf(m) {
  return m.kind === 'threaded'
    || ((m.kind === 'counterbore' || m.kind === 'countersink')
        && (m.subtype === 'standard' || m.subtype === 'fine'));
}
function computeValues(m) {
  const th = threadedOf(m);
  let p = 0, minor = m.d, base_d = m.d, base_start = 0;
  let bore_d = null, bore_t = null, sink_d = null, sink_t = null;
  if (th) {
    const row = (m.subtype === 'fine' ? FINE : COARSE).find((r) => Number(r.d) === Number(m.d));
    if (!row) return { error: `螺纹 M${m.d} 不在表里` };
    p = Number(row.p);
    minor = m.d - 1.0825 * p;
  } else if (m.subtype === 'drill') {
    const row = TAP.find((r) => Number(r.d) === Number(m.d));
    if (!row) return { error: `钻头表没有 M${m.d}` };
    base_d = Number(row.drill);
  } else if (m.subtype === 'clearance') {
    const row = CLEAR.find((r) => Number(r.d) === Number(m.d));
    if (!row) return { error: `间隙表没有 M${m.d}` };
    base_d = Number(row[m.fit] ?? row.normal);
  } else if (m.subtype === 'custom') {
    base_d = Number(m.custom_d);
    if (!(base_d > 0)) return { error: '自定义孔径必须 > 0' };
  } else {
    return { error: '子类型与孔类型不匹配' };
  }
  if (m.kind === 'counterbore') {
    const r = CBORE.find((x) => Number(x.d) === Number(m.d));
    if (!r) return { error: `沉头表没有 M${m.d}` };
    bore_d = r.d2; bore_t = r.t; base_start = r.t;
  }
  if (m.kind === 'countersink') {
    const r = CSINK.find((x) => Number(x.d) === Number(m.d));
    if (!r) return { error: `埋头表没有 M${m.d}` };
    sink_d = r.d2; sink_t = r.t; base_start = (r.d2 - base_d) / 2;
  }
  const tl = th ? (m.full_thread ? (m.hole_depth ?? 0) : (m.thread_len ?? 1.5 * m.d)) : 0;
  let hd = m.hole_depth;
  if (hd === null || hd === undefined) {
    if (th && m.range !== 'through') hd = tl + 2 * p;
    else return { error: '孔深不能自动' };
  }
  return {
    size_name: th ? `M${m.d}` : `Ø${fmt(base_d)}`,
    threaded: th, major: fmt(th ? m.d : base_d), minor: fmt(th ? minor : base_d),
    pitch: fmt(p), base_d: fmt(base_d), base_start: fmt(base_start),
    bore_d: bore_d === null ? null : fmt(bore_d), bore_t: bore_t === null ? null : fmt(bore_t),
    sink_d: sink_d === null ? null : fmt(sink_d), sink_t: sink_t === null ? null : fmt(sink_t),
    thread_len: fmt(tl), hole_depth: fmt(hd),
    cone_height: fmt(m.range === 'blind' ? (base_d / 2) / Math.tan((59 * Math.PI) / 180) : 0),
    warnings: [],
  };
}
global.fetch = async (u, opts = {}) => {
  const url = String(u);
  if (url.startsWith('/api/hole_sizes')) return jsonResp(SIZES);
  if (url.startsWith('/api/hole_preview')) {
    const m = JSON.parse(opts.body || '{}');
    lastPreviewModel = m;
    const v = computeValues(m);
    if (v.error) return jsonResp({ ok: false, error: v.error }, 400);
    return jsonResp({ ok: true, svg: '<svg xmlns="http://www.w3.org/2000/svg"><rect/></svg>', values: v });
  }
  if (url.startsWith('/api/hole_export')) {
    lastExportUrl = url;
    lastExportModel = JSON.parse(opts.body || '{}');
    return jsonResp({ ok: true, message: 'stub 已生成' });
  }
  return jsonResp({ ok: true });
};

// ── 跑 GUI 脚本（末尾探针暴露内部状态；不改生产代码）──────────────
const probed = script.replace(/\}\)\(\);\s*$/, `;globalThis.__hole = {
  get sizes() { return sizes; },
  get kind() { return kind; },
  currentModel, refresh, syncUi, pickKind,
  setKind(k) { pickKind(k); },
};
})();`);
try {
  (0, eval)(probed);
} catch (e) {
  errors.push('脚本求值异常: ' + (e && e.stack ? e.stack : e));
}
const tick = async (n = 4) => { for (let i = 0; i < n; i++) await new Promise((r) => setImmediate(r)); };
await tick();

const H = globalThis.__hole;
const el = (id) => document.getElementById(id);
check(!!H, '探针 __hole 未挂上（脚本初始化崩溃？）');
if (!H) report();

// ① 打开即「螺纹孔 M10」：自动 15 / 18，读数 D1=10 / D=8.376
check(H.kind === 'threaded', `默认应为螺纹孔，实为 ${H.kind}`);
check(!!H.sizes && H.sizes.coarse.length > 0, '/api/hole_sizes 未加载');
check(el('subtype').value === 'standard', `默认子类型应为标准螺纹，实为 ${el('subtype').value}`);
check(el('size').value === '10|1.5', `默认大小应为 M10×1.5，实为 ${el('size').value}`);
check(el('fit').value === '6H', `螺纹配合默认 6H，实为 ${el('fit').value}`);
check(el('hAuto').checked && !el('hAuto').disabled, '螺纹盲孔默认应勾选自动孔深');
check(el('hDepth').value === '18', `自动孔深应 18，实为 ${el('hDepth').value}`);
check(el('hDepth').readOnly === true, '自动孔深输入框应只读');
check(el('tAuto').checked, '默认应勾选自动螺纹范围');
check(el('tLen').value === '15', `自动螺纹长应 15，实为 ${el('tLen').value}`);
check(el('majorOut').textContent === '10', `D1 应为 10，实为 ${el('majorOut').textContent}`);
check(el('minorOut').textContent === '8.376', `D 应为 8.376，实为 ${el('minorOut').textContent}`);
check((el('extraOut').textContent || '').includes('有效螺纹 15'), '读数应含有效螺纹 15：' + el('extraOut').textContent);

// ② 简单孔：两个自动都置灰/取消勾选；子类型 = 钻头/自定义/间隙；大小可显示底孔径
el('kindSimple').click();
await tick();
check(H.kind === 'simple', '点「简单孔」应切类型');
const subs = el('subtype').options.map((o) => o.value);
check(subs.join(',') === 'drill,custom,clearance', `简单孔子类型应为 钻头/自定义/间隙，实为 ${subs.join(',')}`);
check(el('hAuto').disabled && !el('hAuto').checked, '简单孔：孔深自动应置灰且取消');
check(el('tAuto').disabled && !el('tAuto').checked, '简单孔：螺纹范围自动应置灰且取消');
check(el('tLen').disabled, '简单孔：螺纹范围数值框应置灰');
check(el('fit').disabled, '简单孔非间隙子类型：配合应置灰');
check(el('size').value === '10|1.5', `简单孔默认钻头 M10×1.5，实为 ${el('size').value}`);
await H.refresh();
check(lastPreviewModel.subtype === 'drill' && lastPreviewModel.kind === 'simple', '预览请求应带 simple/drill');

// ③ 螺栓间隙：配合可选（精/中等/粗）
el('subtype').value = 'clearance';
el('subtype')._fire('change', el('subtype'));
await tick();
const fits = el('fit').options.map((o) => o.value);
check(fits.join(',') === 'close,normal,loose', `间隙配合应为 精/中等/粗，实为 ${fits.join(',')}`);
check(!el('fit').disabled, '螺栓间隙下配合应可选');
el('fit').value = 'normal';
el('fit')._fire('change', el('fit'));
await tick();
check(H.currentModel().fit === 'normal', '配合选择应进入模型');
await H.refresh();
check((el('extraOut').textContent || '').includes('底孔 Ø11'), '读数应显示间隙底孔 Ø11：' + el('extraOut').textContent);

// ④ 贯通：孔深自动置灰；回到盲孔恢复
el('range').value = 'through';
el('range')._fire('change', el('range'));
await tick();
check(el('hAuto').disabled, '贯通：孔深自动应置灰');
el('range').value = 'blind';
el('range')._fire('change', el('range'));
await tick();

// ⑤ 自定义：孔径输入行出现；模型带 custom_d
el('subtype').value = 'custom';
el('subtype')._fire('change', el('subtype'));
await tick();
check(el('customRow').style.display !== 'none', '自定义孔径行应显示');
check(!el('customD').disabled, '自定义孔径应可编辑');
el('customD').value = '5.5';
el('customD')._fire('input', el('customD'));
await tick();
check(H.currentModel().custom_d === 5.5, '自定义孔径应进入模型');

// ⑥ 沉头孔：带螺纹开关可用；不带螺纹时大小按 GB/T 152.3 过滤
H.setKind('counterbore');
await tick();
check(el('threadRow').style.display !== 'none', '沉头孔应显示带螺纹开关');
check(el('threadToggle').checked, '沉头孔默认带螺纹');
check(el('subtype').options.map((o) => o.value).join(',') === 'standard,fine', '带螺纹子类型应为 标准/细牙');
const cbSizes = el('size').options.map((o) => Number(o.value.split('|')[0]));
check(cbSizes.every((d) => CBORE.some((r) => r.d === d)), `沉头孔大小应落在 152.3 表内：${cbSizes}`);
el('threadToggle').checked = false;
el('threadToggle')._fire('change', el('threadToggle'));
await tick();
check(el('subtype').options.map((o) => o.value).join(',') === 'drill,custom,clearance', '不带螺纹子类型应为 钻头/自定义/间隙');
const cbDrill = el('size').options.map((o) => Number(o.value.split('|')[0]));
check(cbDrill.length > 0 && cbDrill.every((d) => CBORE.some((r) => r.d === d)), '沉头孔钻孔大小应落在 152.3 表内');
check(el('hAuto').disabled, '沉头孔不带螺纹：孔深自动应置灰');

// ⑦ 埋头孔：带螺纹 + 全长 → 孔深自动/螺纹数值框都置灰
H.setKind('countersink');
await tick();
el('tFull').checked = true;
el('tFull')._fire('change', el('tFull'));
await tick();
check(el('hAuto').disabled, '全长：孔深自动应置灰');
check(el('tLen').disabled, '全长：螺纹数值框应置灰');
el('tFull').checked = false;
el('tFull')._fire('change', el('tFull'));
await tick();

// ⑧ 确定 → POST /api/hole_export 带模型
H.setKind('threaded');
await tick();
el('ok').click();
await tick(6);
check(lastExportUrl.startsWith('/api/hole_export'), `导出 URL 应为 /api/hole_export，实为 ${lastExportUrl}`);
check(!!lastExportModel && lastExportModel.kind === 'threaded', `导出模型 kind 应为 threaded：${JSON.stringify(lastExportModel)}`);
check(lastExportModel.views && lastExportModel.views.side === true, '导出模型应带视图开关');
check((el('status').textContent || '').includes('stub 已生成'), '导出成功提示应显示：' + el('status').textContent);

function report() {
  if (errors.length) {
    console.error('孔 GUI 冒烟失败：');
    for (const e of errors) console.error(' - ' + e);
    process.exit(1);
  }
  console.log('孔 GUI 冒烟通过');
}
report();
