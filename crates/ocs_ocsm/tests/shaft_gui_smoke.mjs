// shaft_gui.html 运行时/行为冒烟（node 最小 DOM 垫片 + 最小 fetch 桩）。
//
// 目的：锁住矩形花键「规格下拉」的交互契约（node --check / el("id") 静态扫描查不出）：
//   ① 段表 SPLINE 列是 <select>，选项来自 `/api/parts` 的 detail_spline_rect.specs
//      （表内规格 + 末尾「自定义规格…」）；
//   ② 选中表内规格 → de 自动填查表值、派生值 d/D/B/de/h/l 显示、行文本同步为
//      `SPLINE 6x28x32x7 L30`（自动填的 de 不写进行文本；改了才算覆盖）；
//   ③ 选「自定义…」→ 文本框出现并接受手输，de 清空；
//   ④ 表外规格缺 de → 沿用后端既有报错文案（桩里复刻 spline.rs 的报错）显示在 dslErr；
//   ⑤ JSON 模型里表内规格不显式带 de（后端仍会查表）。
//
// 用法：node shaft_gui_smoke.mjs <shaft_gui.html 路径>

import fs from 'node:fs';

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error('usage: node shaft_gui_smoke.mjs <shaft_gui.html>');
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

// ── 规格表（与 detail.rs 暴露的 33 条同形；只取测试用到的几条）──
const SPECS = [
  { code: '6x23x26x6', label: '轻6x23x26x6', n: 6, d: 23, D: 26, B: 6, de: 63, series: '轻' },
  { code: '6x28x32x7', label: '轻6x28x32x7', n: 6, d: 28, D: 32, B: 7, de: 71, series: '轻' },
  { code: '8x52x58x10', label: '轻8x52x58x10', n: 8, d: 52, D: 58, B: 10, de: 90, series: '轻' },
];

// 渐开线花键预设（与 detail.rs 目录 / invol_spline.rs 系数同形；只取测试用到的）
const INVOL = [
  { code: 'GB30R', std: 'GB', profile: '30圆齿根', alpha: 30, ha: 0.5, hf: 0.9, rho: 0.4, cf: 0.1 },
  { code: 'GB30P', std: 'GB', profile: '30平齿根', alpha: 30, ha: 0.5, hf: 0.75, rho: 0.2, cf: 0.1 },
  { code: 'DIN30', std: 'DIN', profile: 'DIN30', alpha: 30, ha: 0.45, hf: 0.55, rho: 0.16, cf: 0.1 },
];

// DIN 5480-2 名义表候选（与 detail.rs 目录 din_nominal 同形；只取测试用到的行）。
const DIN_NOMINAL = [
  { db: 40, m: 2, z: 18, x: 0.45, page: 27 },
  { db: 45, m: 3, z: 13, x: 0.45, page: 31 },
  { db: 45, m: 3, z: 14, x: -0.05, page: 31 },
];

// ── 最小 DOM 垫片 ────────────────────────────────────────────────
function htmlDecode(s) {
  return String(s)
    .replace(/&lt;/g, '<').replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"').replace(/&amp;/g, '&');
}

function attrMap(attrsText) {
  const map = {};
  for (const it of String(attrsText).matchAll(/([a-zA-Z-]+)="([^"]*)"/g)) {
    map[it[1]] = htmlDecode(it[2]);
  }
  return map;
}

function mkEl(id) {
  const el = {
    id,
    tagName: 'DIV',
    type: '',
    dataset: {},
    style: {},
    value: '',
    checked: false,
    disabled: false,
    textContent: '',
    children: [],
    options: [],
    _handlers: {},
    _innerHTML: '',
    _rows: [],
    _fields: [],
    _attrs: {},
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
    querySelectorAll(sel) {
      if (sel === 'tr') return this._rows;
      const hit = /^tr\[data-row="(\d+)"\]$/.exec(sel);
      if (hit) return this._rows.filter((r) => r.dataset.row === hit[1]);
      return [];
    },
    querySelector(sel) { return this.querySelectorAll(sel)[0] || null; },
    setSelectionRange() {},
    closest() { return null; },
    focus() {},
    remove() {},
    click() { this._fire('click', this); },
  };
  Object.defineProperty(el, 'innerHTML', {
    get() { return el._innerHTML; },
    set(v) {
      el._innerHTML = String(v);
      if (el.id === 'segBody') el._parseRows(el._innerHTML);
      else if (el._innerHTML === '') el.children = [];
    },
  });
  // segBody 的 innerHTML = 段表全部 <tr>；解析出每行的 input/select 供事件测试用。
  el._parseRows = function (raw) {
    this._rows = [];
    const chunks = String(raw).split('<tr data-row="').slice(1);
    for (const chunk of chunks) {
      const idx = chunk.slice(0, chunk.indexOf('"'));
      const rowHtml = '<tr data-row="' + chunk;
      const tr = mkEl('tr');
      tr.tagName = 'TR';
      tr.dataset = { row: idx };
      tr._html = rowHtml;
      const cls = /<tr data-row="[^"]*" class="([^"]*)"/.exec(rowHtml);
      if (cls) cls[1].split(/\s+/).forEach((c) => c && tr.classList.add(c));
      tr._fields = [];
      for (const tag of rowHtml.match(/<input\b[^>]*>/g) || []) tr._fields.push(parseFieldTag(tag, 'INPUT'));
      for (const tag of rowHtml.match(/<select\b[\s\S]*?<\/select>/g) || []) tr._fields.push(parseFieldTag(tag, 'SELECT'));
      for (const f of tr._fields) {
        f._tr = tr;
        f.closest = (sel) => (sel === 'tr[data-row]' ? tr : null);
      }
      this._rows.push(tr);
    }
  };
  return el;
}

function parseFieldTag(tagHtml, tagName) {
  const attrsText = tagName === 'SELECT'
    ? tagHtml.slice(tagHtml.indexOf(' '), tagHtml.indexOf('>'))
    : tagHtml;
  const attrs = attrMap(attrsText);
  const f = mkEl('field');
  f.tagName = tagName;
  f._attrs = attrs;
  f.dataset = {};
  if (attrs['data-f']) f.dataset.f = attrs['data-f'];
  f.type = attrs.type || (tagName === 'SELECT' ? 'select-one' : 'text');
  f.value = attrs.value !== undefined ? attrs.value : '';
  f.disabled = /\bdisabled\b/.test(attrsText);
  f.checked = /\bchecked\b/.test(attrsText);
  if (tagName === 'SELECT') {
    f.options = [];
    for (const it of tagHtml.matchAll(/<option value="([^"]*)"([^>]*)>([\s\S]*?)<\/option>/g)) {
      const o = {
        value: htmlDecode(it[1]),
        textContent: htmlDecode(it[3]),
        selected: /\bselected\b/.test(it[2]),
      };
      f.options.push(o);
      if (o.selected) f.value = o.value;
    }
    if (f.value === '' && f.options.length && !f.options.some((o) => o.selected)) {
      f.value = f.options[0].value;
    }
  }
  return f;
}

const els = new Map();
global.document = {
  getElementById(id) {
    if (!els.has(id)) els.set(id, mkEl(id));
    return els.get(id);
  },
  createElement(tag) {
    const e = mkEl('dyn');
    e.tagName = String(tag).toUpperCase();
    return e;
  },
  createTextNode(t) {
    const e = mkEl('text');
    e.textContent = String(t);
    return e;
  },
  querySelectorAll(sel) {
    if (sel === 'tr.bad') {
      const rows = (els.get('segBody') || { _rows: [] })._rows || [];
      return rows.filter((r) => r.classList.contains('bad'));
    }
    return [];
  },
  querySelector() { return null; },
  addEventListener() {},
};
global.window = { addEventListener() {}, close() {} };
global.location = { search: '', href: 'http://127.0.0.1:9/shaft' };
global.setTimeout = () => 0;
global.setInterval = () => 0;
global.Blob = class Blob { constructor(parts, opts) { this.parts = parts; this.opts = opts; } };
global.URL = { createObjectURL: () => 'blob:stub', revokeObjectURL() {} };
global.Option = function Option(label, value) {
  const o = mkEl('option');
  o.textContent = String(label);
  o.value = value === undefined ? String(label) : String(value);
  return o;
};

// ── 最小 fetch 桩（复刻后端的关键行为）─────────────────────────────
function jsonResp(obj, status = 200) {
  return { ok: status < 400, status, json: async () => obj, text: async () => JSON.stringify(obj) };
}
function textResp(text, status = 200) {
  return { ok: status < 400, status, text: async () => text, json: async () => JSON.parse(text) };
}

// DSL → 归一化段模型（够本测试用：花键行按规格表查 de，其它行给一个圆柱段）。
function segmentsFor(dsl) {
  const out = [];
  for (const raw of String(dsl).split(/[\n|]/)) {
    const line = raw.trim();
    if (!line) continue;
    const sp = /^SPLINE\s+(\d+)x(\d+)x(\d+)x(\d+)\s+L([\d.]+)(?:\s+de\s*=?\s*([\d.]+))?/i.exec(line);
    if (sp) {
      const [, n, d, big, b, len, de] = sp;
      const hit = SPECS.find((s) => s.code === `${n}x${d}x${big}x${b}`);
      out.push({
        spline: {
          n: Number(n), d: Number(d), big: Number(big), b: Number(b), len: Number(len),
          de: de !== undefined ? Number(de) : (hit ? hit.de : null),
        },
      });
      continue;
    }
    const inv = /^INVOLSPLINE\s+(\S+)\s+M([\d.]+)\s+Z(\d+)(?:\s+X(-?[\d.]+))?\s+L([\d.]+)(?:\s+de\s*=?\s*([\d.]+))?/i.exec(line);
    if (inv) {
      const [, code, m, z, x, len, de] = inv;
      const iv = { code, m: Number(m), z: Number(z), x: x !== undefined ? Number(x) : 0, len: Number(len) };
      if (de !== undefined) iv.de = Number(de);
      out.push({ invol_spline: iv });
      continue;
    }
    out.push({ s: 30, e: 30, l: 10 });
  }
  return out;
}

let lastParseDsl = '';
global.fetch = async (u, opts = {}) => {
  const url = String(u);
  if (url.startsWith('/api/parts')) {
    return jsonResp({
      ok: true,
      families: {
        detail_spline_rect: { specs: SPECS },
        detail_invol_spline: { invol_presets: INVOL, din_nominal: DIN_NOMINAL },
      },
    });
  }
  if (url.startsWith('/api/shaft_parse')) {
    const body = JSON.parse(opts.body || '{}');
    const dsl = String(body.dsl || '');
    lastParseDsl = dsl;
    const sp = /SPLINE\s+(\d+)x(\d+)x(\d+)x(\d+)/i.exec(dsl);
    const wantsDe = /\bde\s*=?\s*[\d.]/.test(dsl);
    if (sp && !wantsDe && !SPECS.some((s) => s.code === `${sp[1]}x${sp[2]}x${sp[3]}x${sp[4]}`)) {
      // 与 spline.rs::RectSpline::from_code 的既有报错文案一致（表外缺 de）。
      return jsonResp({
        ok: false,
        error: `矩形花键：规格 ${sp[1]}x${sp[2]}x${sp[3]}x${sp[4]} 不在 GB/T 10952-2005 表 1/表 2 里，`
          + 'de 查不到 —— 请给 de 覆盖（例 `de 63`）。',
      }, 400);
    }
    return jsonResp({ ok: true, segments: segmentsFor(dsl), at: null, rot: null, view: 'normal' });
  }
  if (url.startsWith('/api/shaft_preview')) {
    return textResp('<svg xmlns="http://www.w3.org/2000/svg"><line x1="0" y1="0" x2="1" y2="1"/></svg>');
  }
  return jsonResp({ ok: true });
};

// ── 跑 GUI 脚本（末尾探针暴露 IIFE 内部函数；不改生产代码）──────────
const probed = script.replace(/\}\)\(\);\s*$/, `;globalThis.__shaft = {
  get rows() { return rows; },
  refreshFromText, modelFromRows, rowToDsl,
  get specs() { return splineSpecs; },
};
})();`);
try {
  (0, eval)(probed);
} catch (e) {
  errors.push('脚本求值异常: ' + (e && e.stack ? e.stack : e));
}
await new Promise((r) => setImmediate(r));
await new Promise((r) => setImmediate(r));
await new Promise((r) => setImmediate(r));

const S = globalThis.__shaft;
const dslEl = document.getElementById('dsl');
const dslErr = document.getElementById('dslErr');
const segBody = document.getElementById('segBody');
check(!!S, '探针 __shaft 未挂上（脚本初始化崩溃？）');
if (!S) report();

check(S.specs.length === SPECS.length, `/api/parts 规格未加载（${S.specs.length}）`);

// ① 段表 SPLINE 列是下拉，选项 = 表内规格 + 「自定义规格…」
dslEl.value = 'SPLINE 6x23x26x6 L30';
await S.refreshFromText();
check(S.rows.length === 1 && S.rows[0].spline.on, '“SPLINE 6x23x26x6 L30” 应回填出 1 个花键段');
let row = segBody._rows[0];
let sel = row._fields.find((f) => f.dataset.f === 'spline.spec' && f.tagName === 'SELECT');
let custom = row._fields.find((f) => f.dataset.f === 'spline.spec' && f.tagName === 'INPUT');
check(!!sel, 'SPLINE 列的规格字段应是 <select>');
check(sel && sel.options.length === SPECS.length + 1, `下拉应有 ${SPECS.length}+1 项，实为 ${sel && sel.options.length}`);
check(sel && sel.options.some((o) => o.value === '6x23x26x6'), '下拉缺表内规格 6x23x26x6');
check(sel && sel.options[sel.options.length - 1].value === '__custom__', '下拉末尾应是「自定义规格…」');
check(!!custom && /display:\s*none/.test((custom._attrs || {}).style || ''), '表内规格时自定义文本框应隐藏');
check(row._html.includes('de63'), '回填后派生值应显示查表 de63');

// ② 选表内规格 → de 自动填 + 派生值 + 行文本同步（自动 de 不写进行文本）
sel.value = '6x28x32x7';
segBody._fire('change', sel);
await new Promise((r) => setImmediate(r));
check(S.rows[0].spline.spec === '6x28x32x7', `选中后规格应为 6x28x32x7，实为 ${S.rows[0].spline.spec}`);
check(S.rows[0].spline.de === '71', `de 应自动填查表值 71，实为 ${JSON.stringify(S.rows[0].spline.de)}`);
check(dslEl.value.includes('SPLINE 6x28x32x7 L30'), `行文本应同步为 SPLINE 6x28x32x7 L30，实为 ${JSON.stringify(dslEl.value)}`);
check(!/\bde\s?71\b/.test(dslEl.value), '自动填的 de 不写进行文本（表内规格）');
row = segBody._rows[0];
const derive = row._html.match(/class="frow spline-derive"[^>]*>([^<]*)</);
const deriveText = derive ? derive[1] : '';
check(deriveText.includes('de71'), `派生值应含 de71：${deriveText}`);
check(deriveText.includes('h2'), `派生值应含 h2：${deriveText}`);
check(deriveText.includes('l11.7473'), `派生值应含 l11.7473：${deriveText}`);

// ③ 选「自定义…」→ 文本框出现并接受手输
sel = segBody._rows[0]._fields.find((f) => f.dataset.f === 'spline.spec' && f.tagName === 'SELECT');
sel.value = '__custom__';
segBody._fire('change', sel);
await new Promise((r) => setImmediate(r));
check(S.rows[0].spline.spec === '' && S.rows[0].spline.de === '', '切到自定义应清空规格与 de');
row = segBody._rows[0];
custom = row._fields.find((f) => f.dataset.f === 'spline.spec' && f.tagName === 'INPUT');
check(!!custom, '自定义文本框应出现（input）');
check(!/display:\s*none/.test((custom._attrs || {}).style || ''), '自定义文本框应可见');
custom.value = '6x11x14x3';
segBody._fire('input', custom);
check(dslEl.value.includes('SPLINE 6x11x14x3 L30'), `手输规格应同步行文本，实为 ${JSON.stringify(dslEl.value)}`);

// ④ 表外规格缺 de → 既有后端报错文案显示在 dslErr；补 de 后通过
await S.refreshFromText();
check(lastParseDsl.includes('6x11x14x3'), '解析请求应带手输规格');
check(dslErr.textContent.includes('de 查不到'), `缺 de 应显示既有报错，实为 ${JSON.stringify(dslErr.textContent)}`);
check(dslErr.textContent.includes('请给 de 覆盖'), '报错应指路「请给 de 覆盖」');
const deInput = segBody._rows[0]._fields.find((f) => f.dataset.f === 'spline.de');
check(!!deInput, '缺 de 输入框');
deInput.value = '63';
segBody._fire('input', deInput);
check(dslEl.value.includes('SPLINE 6x11x14x3 L30 de63'), `补 de 后行文本应带 de63，实为 ${JSON.stringify(dslEl.value)}`);
await S.refreshFromText();
check(dslErr.textContent === '', `补 de 后不应报错，实为 ${JSON.stringify(dslErr.textContent)}`);

// ⑤ 表内规格不显式带 de：清空 de 框后 JSON 模型里也不带 de（后端会查表）
dslEl.value = 'SPLINE 6x28x32x7 L30';
await S.refreshFromText();
row = segBody._rows[0];
const deInput2 = row._fields.find((f) => f.dataset.f === 'spline.de');
check(deInput2 && deInput2.value === '71', '回填表内规格时 de 框显示查表值 71');
deInput2.value = '';
segBody._fire('input', deInput2);
const model = S.modelFromRows();
check(!!model, '清空 de 后模型应有效');
check(model && model.segments[0].spline.spec === '6x28x32x7', '模型规格应保留');
check(model && model.segments[0].spline.de === undefined, '表内规格自动填的 de 不进 JSON 模型');

// ⑥ 新勾 SPLINE（默认表内规格）：de 也自动填查表值，派生值立即可见
const before = segBody._rows.length;
document.getElementById('addRow').click();
await new Promise((r) => setImmediate(r));
check(segBody._rows.length === before + 1, '加行应多一行');
const addedRow = segBody._rows[segBody._rows.length - 1];
const onBox = addedRow._fields.find((f) => f.dataset.f === 'spline.on');
check(!!onBox, '缺 SPLINE 勾选框');
onBox.checked = true;
segBody._fire('change', onBox);
await new Promise((r) => setImmediate(r));
const added = S.rows[S.rows.length - 1];
check(added.spline.on && added.spline.spec === '6x23x26x6', '勾选后应有默认表内规格');
check(added.spline.de === '63', `勾选 SPLINE 时 de 应自动填 63，实为 ${JSON.stringify(added.spline.de)}`);
check(segBody._rows[segBody._rows.length - 1]._html.includes('de63'), '勾选后派生值应含 de63');

// ⑦ 渐开线花键：预设下拉 + m/z/x/L 派生值 + DIN 联动 + JSON 模型
const checkInvolDerive = (row, needle) => {
  const m = row._html.match(/class="frow invol-derive"[^>]*>([^<]*)</);
  const text = m ? m[1] : '';
  check(text.includes(needle), `渐开线派生值应含 ${needle}：${text}`);
};
dslEl.value = 'INVOLSPLINE GB30R M3 Z20 L30 de70';
await S.refreshFromText();
check(S.rows.length === 1 && S.rows[0].invol.on, '“INVOLSPLINE …” 应回填出渐开线花键行');
check(S.rows[0].invol.profile === 'GB30R', `回填预设代号：${S.rows[0].invol.profile}`);
let irow = segBody._rows[0];
checkInvolDerive(irow, 'da63');
checkInvolDerive(irow, 'df54.6');
checkInvolDerive(irow, 'l16.6241');
check(dslEl.value.includes('INVOLSPLINE GB30R M3 Z20 L30 de70'), `行文本同步：${JSON.stringify(dslEl.value)}`);
// 标准切 DIN → 齿廓自动 DIN30、派生值含 d_B 名义估算与说明
const stdSel = irow._fields.find((f) => f.dataset.f === 'invol.std');
check(!!stdSel && stdSel.options.some((o) => o.value === 'DIN'), '标准下拉应有 DIN');
stdSel.value = 'DIN';
segBody._fire('change', stdSel);
await new Promise((r) => setImmediate(r));
check(S.rows[0].invol.std === 'DIN' && S.rows[0].invol.profile === 'DIN30',
  `DIN 联动齿廓，实为 ${S.rows[0].invol.std}/${S.rows[0].invol.profile}`);
irow = segBody._rows[0];
checkInvolDerive(irow, 'd_B');
// d_B（DIN 5480-2 查表）：填 DB40 → 自动带出 m/z/x、行文本带 DB40、JSON 带 d_b
const dbField = irow._fields.find((f) => f.dataset.f === 'invol.db');
check(!!dbField, '缺 d_B 输入框');
dbField.value = '40';
segBody._fire('change', dbField);
await new Promise((r) => setImmediate(r));
check(S.rows[0].invol.m === '2' && S.rows[0].invol.z === '18'
  && Math.abs(Number(S.rows[0].invol.x) - 0.45) < 1e-9,
  `d_B=40 应带出 m2/z18/x0.45，实为 ${S.rows[0].invol.m}/${S.rows[0].invol.z}/${S.rows[0].invol.x}`);
check(dslEl.value.includes('INVOLSPLINE DIN30 DB40')
  && dslEl.value.includes('M2') && dslEl.value.includes('Z18'),
  `行文本应带 DB40/M2/Z18：${JSON.stringify(dslEl.value)}`);
irow = segBody._rows[0];
checkInvolDerive(irow, '查表命中 p27 m=2');
const dinModel = S.modelFromRows();
check(!!dinModel && dinModel.segments[0].invol_spline.d_b === 40
  && dinModel.segments[0].invol_spline.z === 18,
  'JSON 模型应带 d_b=40 与补出的 z=18');
// JSON 模型：invol_spline 带 code/m/z/x/len/de
const involModel = S.modelFromRows();
check(!!involModel && !!involModel.segments[0].invol_spline, '模型应带 invol_spline');
check(involModel && involModel.segments[0].invol_spline.code === 'DIN30'
  && involModel.segments[0].invol_spline.de === 70, '模型 code/de 应为 DIN30/70');

report();

function report() {
  if (errors.length) {
    console.error('轴 GUI 冒烟失败：\n- ' + errors.join('\n- '));
    process.exit(1);
  }
  console.log('轴 GUI 冒烟通过：规格下拉 / de 自动填 / 派生值 / 自定义手输 / 表外缺 de 报错');
  process.exit(0);
}
