// gear_gui.html 运行时冒烟（node 最小 DOM 垫片 + fetch 桩）。
//
// 目的：锁住「齿轮 / 花键」模式切换的交互契约（node --check 与 el("id") 静态扫描查不出）：
//   ① 默认齿轮模式：标准号/齿廓/d_B/hf/ρf/cF 行隐藏，c/β/倒角行显示；
//   ② 勾「花键模式」+ GB：花键行显示、GB 下 **d_B 行隐藏**（GB 无基准直径概念）；
//   ③ 切 DIN：d_B 行显示、齿廓切到 DIN30、预设系数写回；
//   ④ 全程无异常，防抖回调能跑。
//
// 用法：node gear_gui_smoke.mjs <gear_gui.html 路径>

import fs from 'node:fs';

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error('usage: node gear_gui_smoke.mjs <gear_gui.html>');
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
const check = (cond, msg) => { if (!cond) errors.push(msg); };

// ── 最小 DOM 垫片 ────────────────────────────────────────────────
const els = new Map();
function mkEl(id) {
  const el = {
    id, tagName: 'DIV', type: '', dataset: {}, style: {}, value: '', checked: false,
    disabled: false, textContent: '', children: [], options: [], _handlers: {}, _innerHTML: '',
    classList: {
      _on: new Set(),
      add(c) { this._on.add(c); },
      remove(c) { this._on.delete(c); },
      contains(c) { return this._on.has(c); },
    },
    addEventListener(ev, fn) { (this._handlers[ev] ||= []).push(fn); },
    _fire(ev, target) { (this._handlers[ev] || []).forEach((f) => f({ target, type: ev })); },
    appendChild(c) { this.children.push(c); return c; },
    removeChild(c) { this.children = this.children.filter((x) => x !== c); },
    querySelectorAll() { return []; },
    querySelector() { return null; },
    setAttribute(k, v) { this._attrs = this._attrs || {}; this._attrs[k] = v; },
    closest() { return null; },
    focus() { return; },
    remove() { return; },
    click() { this._fire('click', this); },
  };
  Object.defineProperty(el, 'innerHTML', {
    get() { return el._innerHTML; },
    set(v) { el._innerHTML = String(v); if (v === '') el.children = []; },
  });
  return el;
}
global.document = {
  getElementById(id) { if (!els.has(id)) els.set(id, mkEl(id)); return els.get(id); },
  querySelectorAll() { return []; },
  querySelector() { return null; },
  createElement(tag) { const e = mkEl('el'); e.tagName = String(tag).toUpperCase(); return e; },
  createElementNS() { return mkEl('ns'); },
  createTextNode(t) { const e = mkEl('text'); e.textContent = String(t); return e; },
  addEventListener() {},
};
global.window = { addEventListener() {}, close() {} };
global.location = { search: '', href: 'http://127.0.0.1/gear' };
const timers = [];
global.setTimeout = (fn) => { timers.push(fn); return timers.length; };
global.clearTimeout = () => {};
global.setInterval = () => 0;

// ── /api/parts 桩（预设 + DIN 名义表，同目录数据形状） ──
const PARTS = {
  families: {
    detail_invol_spline: {
      invol_presets: [
        { code: 'GB30P', std: 'GB', profile: '30平齿根', alpha: 30, ha: 0.5, hf: 0.75, rho: 0.2, cf: 0.1 },
        { code: 'GB30R', std: 'GB', profile: '30圆齿根', alpha: 30, ha: 0.5, hf: 0.9, rho: 0.4, cf: 0.1 },
        { code: 'GB375R', std: 'GB', profile: '37.5圆齿根', alpha: 37.5, ha: 0.45, hf: 0.7, rho: 0.3, cf: 0.1 },
        { code: 'GB45R', std: 'GB', profile: '45圆齿根', alpha: 45, ha: 0.4, hf: 0.6, rho: 0.25, cf: 0.1 },
        { code: 'DIN30', std: 'DIN', profile: 'DIN30', alpha: 30, ha: 0.45, hf: 0.55, rho: 0.16, cf: 0.1 },
      ],
      din_nominal: [{ db: 40, m: 2, z: 18, x: 0.45, page: 27 }],
    },
  },
};
const INFO = {
  ok: true, mode: 'spline', kind: 'internal', kind_label: '内花键', view: 'front',
  std: 'GB', std_code: 'GB/T 3478.1-2008', profile: '30圆齿根', m: 3, z: 20, x: 0, alpha: 30,
  d: 60, db: 51.9615, da: 65.4, df: 57.344, d_b: null, origin: null, rho: 1.2, cf: 0.3,
  internal_major: 65.4, internal_minor: 57.344, block: 'B', spec: 'S', notes: ['内花键草案'],
};
global.fetch = async (u) => {
  const t = String(u);
  if (t.startsWith('/api/parts')) {
    return { ok: true, json: async () => PARTS, text: async () => JSON.stringify(PARTS) };
  }
  if (t.startsWith('/api/gear_info')) {
    return { ok: true, json: async () => INFO, text: async () => JSON.stringify(INFO) };
  }
  if (t.startsWith('/api/gear_svg')) return { ok: true, text: async () => '<svg></svg>' };
  if (t.startsWith('/api/page_ping')) return { ok: true, json: async () => ({}), text: async () => '{}' };
  return { ok: false, json: async () => ({ ok: false }), text: async () => '' };
};

// ── 跑 GUI 脚本 ─────────────────────────────────────────────────
try {
  (0, eval)(script);
} catch (e) {
  errors.push('脚本求值异常: ' + (e && e.stack ? e.stack : e));
}
const flush = async () => {
  for (let i = 0; i < 6; i++) await new Promise((r) => setImmediate(r));
};
await flush();

const el = (id) => document.getElementById(id);
const numOf = (id) => parseFloat(el(id).value);

// ① 默认齿轮模式：c/beta/chamfer 行在，花键行不在
check(el('stdLabel').style.display === 'none', '齿轮模式下标准号行应隐藏');
check(el('cLabel').style.display !== 'none', '齿轮模式下顶隙行应显示');
check(el('hfLabel').style.display === 'none', '齿轮模式下 hf 行应隐藏');

// ② 花键模式 + GB：花键行显示、d_B 行隐藏
el('stdSel').value = 'GB';
el('splineMode').checked = true;
try { el('splineMode')._fire('change', el('splineMode')); } catch (e) { errors.push('模式切换异常: ' + e); }
await flush();
check(el('stdLabel').style.display !== 'none', '花键模式下标准号行应显示');
check(el('dbLabel').style.display === 'none', 'GB 下 d_B 行应隐藏（本体系不用 d_B）');
check(el('cLabel').style.display === 'none', '花键模式下顶隙行应隐藏');
check(el('profileSel').value === 'GB30R', 'GB 默认齿廓应为 GB30R，实际 ' + el('profileSel').value);
check(Math.abs(numOf('hf') - 0.9) < 1e-9, 'GB30R hf 预设应 0.9，实际 ' + numOf('hf'));

// ③ 切 DIN：d_B 行显示；齿廓 DIN30 + 系数预设
el('stdSel').value = 'DIN';
try { el('stdSel')._fire('change', el('stdSel')); } catch (e) { errors.push('标准切换异常: ' + e); }
await flush();
check(el('dbLabel').style.display !== 'none', 'DIN 下 d_B 行应显示');
check(el('profileSel').value === 'DIN30', 'DIN 齿廓应 DIN30，实际 ' + el('profileSel').value);
check(Math.abs(numOf('hf') - 0.55) < 1e-9, 'DIN30 hf 预设应 0.55');

// ④ 防抖回调可跑（collect/renderInfo 不抛）
for (const t of timers.splice(0)) {
  try { t(); } catch (e) { errors.push('防抖回调异常: ' + (e && e.stack ? e.stack : e)); }
}
await flush();

if (errors.length) {
  console.error('FAIL:\n' + errors.join('\n'));
  process.exit(1);
}
console.log('GEAR_GUI_SMOKE_OK');
