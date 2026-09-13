// guide_gui.html 运行时冒烟测试（node 最小 DOM 垫片）。
//
// 目的：catch「node --check 查不出」的 GUI 运行时错误——历史上已出现两次：
//   ① 缺 GDT_POLYS/drawGdtGlyph 定义 → 初始化 IIFE 崩溃；
//   ② wline 定义在 draw() 内部（在 WELD 早返回之后）→ drawWeld 抛
//      ReferenceError → 焊接预览空白；且 WELD 分支漏写 row-weld 显示。
// 做法：垫出 document/window/fetch/location，跑 GUI 脚本，然后点「焊接」
// 类型按钮，断言：无异常、row-weld 可见、预览 SVG 有内容。
//
// 用法：node gui_smoke.mjs <guide_gui.html 路径>

import fs from 'node:fs';

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error('usage: node gui_smoke.mjs <guide_gui.html>');
  process.exit(2);
}
const html = fs.readFileSync(htmlPath, 'utf8');
const m = html.match(/<script>([\s\S]*?)<\/script>/);
if (!m) {
  console.error('no <script> found');
  process.exit(2);
}
const script = m[1];

// ── 最小 DOM 垫片 ────────────────────────────────────────────────
const els = new Map();
const containerButtons = new Map(); // 容器 id → 按钮数组

function mkEl(id, dataset = {}) {
  const el = {
    id,
    dataset,
    style: {},
    value: '',
    checked: false,
    disabled: false,
    textContent: '',
    children: [],
    options: [],
    _handlers: {},
    _innerHTML: '',
    classList: {
      _on: new Set(),
      add(c) { this._on.add(c); },
      remove(c) { this._on.delete(c); },
      contains(c) { return this._on.has(c); },
    },
    addEventListener(ev, fn) { (this._handlers[ev] ||= []).push(fn); },
    appendChild(c) {
      this.children.push(c);
      if (c && c._isOption) this.options.push(c); // <select>.options
      return c;
    },
    removeChild(c) { this.children = this.children.filter((x) => x !== c); },
    querySelectorAll(sel) {
      if (sel === 'button') return containerButtons.get(this.id) || [];
      return [];
    },
    querySelector(sel) { return this.querySelectorAll(sel)[0] || null; },
    getBoundingClientRect() { return { left: 0, top: 0, width: 800, height: 600 }; },
    setAttribute() {},
    getContext() { return null; },
    closest() { return null; },
    focus() {},
    remove() {},
    click() { (this._handlers['click'] || []).forEach((f) => f({ target: this })); },
  };
  Object.defineProperty(el, 'innerHTML', {
    get() { return el._innerHTML; },
    set(v) { el._innerHTML = v; if (v === '') el.children = []; },
  });
  return el;
}

// #seg-type 的类型按钮（脚本会按 data-t 过滤/绑定）
const TYPE_NAMES = [
  'LINEAR', 'DIAMETER', 'RADIUS', 'DATUM', 'VIEW',
  'ANGLE', 'SECTION', 'TOLERANCE', 'ARCLEN', 'DETAIL', 'WELD',
];
const typeButtons = TYPE_NAMES.map((t) => mkEl('seg-type-' + t, { t }));
typeButtons.forEach((b) => b.classList.add('on'));

global.document = {
  getElementById(id) {
    if (!els.has(id)) els.set(id, mkEl(id));
    return els.get(id);
  },
  querySelectorAll(sel) {
    if (sel === '#seg-type button') return typeButtons;
    return [];
  },
  querySelector(sel) {
    if (sel === '#seg-type button[data-t="WELD"]') return typeButtons.find((b) => b.dataset.t === 'WELD');
    return null;
  },
  createElementNS() { return mkEl('ns'); },
  createElement() { return mkEl('el'); },
  createTextNode(t) { const e = mkEl('text'); e.textContent = String(t); return e; },
  addEventListener() {},
};
containerButtons.set('seg-type', typeButtons);
// 浏览器内置 Option 构造器（initWeldRows 用它建下拉项）
global.Option = function Option(label, value) {
  const o = mkEl('option');
  o.textContent = String(label);
  o.value = value === undefined ? String(label) : String(value);
  o._isOption = true;
  return o;
};
global.window = { addEventListener() {}, close() {} };
global.location = { search: '?handle=0x6F', port: '23751', href: 'http://127.0.0.1:23751/guide.html?handle=0x6F' };
global.setTimeout = () => 0;
// node 内置 navigator（只读），无需覆盖。

const GUIDE = {
  ok: true,
  handle: '0x6F',
  p1: [0, 0, 0],
  p2: [60, 20, 0],
  pts: [[0, 0, 0], [20, 20, 0], [60, 20, 0]],
  geom: 'pline',
  url: null,
  measurement: 67.1,
  params: {
    type: 'WELD', dist: 0,
    weld: {
      upper: '角焊', lower: '点焊', dash: true, circle: false, half: true,
      flag: true, tail: true, grindUpper: 'zig', grindLower: 'cvx',
      methodUpper: 'C', methodLower: 'U',
      up_thick: '5', up_qty: '100', lo_thick: '3', lo_qty: '50', tail_text: 'N=2',
    },
  },
};
const SYMS = [
  { name: '角焊', cross: false, upper: [['L', 0, 0.125, 3.5, 0.125]], lower: [['L', 0, -0.125, 3.5, -0.125]] },
  { name: '点焊', cross: false, upper: [['C', 2.275, 2.275, 2.275]], lower: [['C', 2.275, -2.275, 2.275]] },
];
global.fetch = async (u) => {
  const t = String(u);
  if (t.startsWith('/api/guide')) return { json: async () => GUIDE };
  if (t.startsWith('/api/weld_syms')) return { json: async () => ({ ok: true, syms: SYMS }) };
  return { json: async () => ({ ok: false }) };
};
global.console = console;

// ── 跑 GUI 脚本 ─────────────────────────────────────────────────
const errors = [];
try {
  // IIFE：直接 eval（全局垫片已就位）
  // eslint-disable-next-line no-eval
  (0, eval)(script);
} catch (e) {
  errors.push('脚本求值异常: ' + (e && e.stack ? e.stack : e));
}

// load() 是 async：等一轮微任务/宏任务
await new Promise((r) => setImmediate(r));
await new Promise((r) => setImmediate(r));

// 点「焊接」类型按钮（触发 seg-type 分支 → 显示 row-weld + drawWeld）
const weldBtn = typeButtons.find((b) => b.dataset.t === 'WELD');
try {
  weldBtn.click();
} catch (e) {
  errors.push('点击焊接按钮异常: ' + (e && e.stack ? e.stack : e));
}
await new Promise((r) => setImmediate(r));

// 遍历所有类型页：任何一页崩溃都算失败（覆盖非焊接分支）
for (const b of typeButtons) {
  try {
    b.click();
    await new Promise((r) => setImmediate(r));
  } catch (e) {
    errors.push(`切换类型 ${b.dataset.t} 异常: ` + (e && e.stack ? e.stack : e));
  }
}
// 回到焊接页做面板断言
try { weldBtn.click(); } catch (e) { errors.push('回切焊接异常: ' + e); }
await new Promise((r) => setImmediate(r));

// ── 断言 ───────────────────────────────────────────────────────
const rowWeld = document.getElementById('row-weld');
if (rowWeld.style.display !== '') {
  errors.push(`row-weld 未显示（display=${JSON.stringify(rowWeld.style.display)}）`);
}
const svg = document.getElementById('prev');
if (!svg.children || svg.children.length === 0) {
  errors.push('预览 SVG 为空（drawWeld 未画出任何元素）');
}
// 焊接方法：按侧独立判定——本例 upper=角焊（可用）、lower=点焊（不可用）
if (document.getElementById('w-method-u').disabled) {
  errors.push('w-method-u 不应禁用（upper=角焊 属可用范围）');
}
if (!document.getElementById('w-method-l').disabled) {
  errors.push('w-method-l 应禁用（lower=点焊 不属可用范围）');
}
// 打磨：上下侧独立两个下拉
if (!document.getElementById('w-grind-u') || !document.getElementById('w-grind-l')) {
  errors.push('缺少上/下侧打磨下拉');
}

if (errors.length) {
  console.error('GUI 冒烟失败：\n- ' + errors.join('\n- '));
  process.exit(1);
}
console.log('GUI 冒烟通过：焊接面板可见、预览非空、无运行时异常');
