// shaft_gui.html 运行时/行为冒烟（node 最小 DOM 垫片 + 最小 fetch 桩）。
//
// 目的：锁住矩形花键「规格下拉」的交互契约（node --check / el("id") 静态扫描查不出）：
//   ① 段表 SPLINE 列是 <select>，选项来自 `/api/parts` 的 detail_spline_rect.specs
//      （表内规格 + 末尾「自定义规格…」）；
//   ② 选中表内规格 → de 自动填查表值、派生值 d/D/B/de/h/l 显示、行文本同步为
//      `SPLINE 6x28x32x7 L30`（自动填的 de 不写进行文本；改了才算覆盖）；
//   ③ 选「自定义…」→ 文本框出现并接受手输，de 清空；
//   ④ 表外规格缺 de → 沿用后端既有报错文案（桩里复刻 spline.rs 的报错）显示在 dslErr；
//   ⑤ JSON 模型里表内规格不显式带 de（后端仍会查表）；
//   ⑥ 渐开线花键轴段 INVOLSPLINE 已撤：段表面板/行文本/表达式框都拒绝（唯一入口 = OCSMGEAR 花键模式）；
//   ⑦ FI 凸/凹圆角的 DSL/JSON 双向回填、左右独立模式与半径、CH/FI 同端互斥/异端共存、复制/特殊段清理；
//   ⑧ CH/FI 同端冲突导入由后端报错，不静默吞掉任一特征，也不能绕过解析错误导出；
//   ⑨ KEY 独立槽总长 SL 的回填/复制/导出/正数校验与留空兼容，及新增控件的空表兼容。
//
// 用法：node shaft_gui_smoke.mjs <shaft_gui.html 路径>

import fs from 'node:fs';
import { renderZh } from './i18n_zh_fixture.mjs';

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error('usage: node shaft_gui_smoke.mjs <shaft_gui.html>');
  process.exit(2);
}
// 本次只改本页与本 smoke；新增 FI/SL 词条的中文测试夹具放在这里，不改共享 fixture。
// 产品页面仍只有占位符，catalog 由后端任务补齐；这不是产品路径的中文兜底。
const SHAFT_ZH = {
  'gui.shaft.fillet_header': 'FI 圆角',
  'gui.shaft.fillet_title': '圆角 FI2@L（凸角）/ FI2@L:CONCAVE（凹角）；省略端默认 R；CH/FI 同端互斥，异端可并存',
  'gui.shaft.fillet_radius_title': '圆角半径 R（mm，必须 > 0）',
  'gui.shaft.fillet_radius_positive': 'FI 圆角的半径 R 必须是大于 0 的有限数。',
  'gui.shaft.fillet_spline_title': '矩形花键段（GB/T 1144 轻/中系列 33 条，规格下拉）；不能与 CH/FI/OV/M/GEAR 同段',
  'gui.shaft.fillet_kind_title': '圆角模式：凸角沿用 CH 落点（贴端面、大直径一侧）；凹角位于轴肩根部',
  'gui.shaft.fillet_convex': '凸角',
  'gui.shaft.fillet_concave': '轴肩根部凹角',
  'gui.shaft.key_slot_l_label': 'SL 槽总长',
  'gui.shaft.key_slot_l_title': '独立键槽总长 SL（mm）；填写后直接作为实际槽长，不再按键长 L、键型或端置 t1 换算；留空沿用原规则',
  'gui.shaft.key_slot_l_positive': '独立键槽总长 SL 必须是大于 0 的有限数（留空沿用原规则）。',
  'gui.shaft.key_slot_l_derive': '槽长 {length}（SL）',
};
const html = renderZh(fs.readFileSync(htmlPath, 'utf8')).replace(
  /\{\{i18n(js)?:(gui\.shaft\.(?:fillet_|key_slot_l_)[^}]+)\}\}/g,
  (placeholder, js, key) => {
    if (!Object.hasOwn(SHAFT_ZH, key)) throw new Error('缺轴 GUI 中文测试词条：' + key);
    const text = SHAFT_ZH[key];
    return js
      ? JSON.stringify(text).slice(1, -1).replace(/</g, '\\u003c').replace(/>/g, '\\u003e')
      : text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  },
);
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

// 平键族桩（与 partgen_keys::families_json 的 key_1096_* 同形；只取测试用到的几档）：
// sizes = 每档 {d(=b), label, lengths, l_min/l_max, extra}；shaft_ranges = 轴径→b 选型区间。
const KEY_SIZES = [
  { d: 8, b: 8, h: 7, label: 'b=8×h=7', l_min: 6, l_max: 79, lengths: [18, 6, 8, 10, 12, 14, 16, 20, 22, 25], extra: 'h=7；c=0.25（按 b 分档取范围下限）；默认 L=18' },
  { d: 10, b: 10, h: 8, label: 'b=10×h=8', l_min: 6, l_max: 99, lengths: [22, 6, 8, 10, 12, 14, 16, 18, 20, 25], extra: 'h=8；c=0.4（按 b 分档取范围下限）；默认 L=22' },
  { d: 14, b: 14, h: 9, label: 'b=14×h=9', l_min: 6, l_max: 139, lengths: [32, 6, 8, 10, 12, 14, 16, 18, 20, 22, 25, 28, 50], extra: 'h=9；c=0.4；默认 L=32' },
];
const KEY_RANGES = [
  { d_lo: 6, d_hi: 8, lo_inclusive: true, b: 2, t1: 1.2 },
  { d_lo: 22, d_hi: 30, lo_inclusive: false, b: 8, t1: 4 },
  { d_lo: 30, d_hi: 38, lo_inclusive: false, b: 10, t1: 5 },
  { d_lo: 44, d_hi: 50, lo_inclusive: false, b: 14, t1: 5.5 },
];

// 导向平键（GB/T 1097）桩：sizes（每档 d0/l0 + 1097 L 系列∩L<10b）+ length_rows（L→L1/L2/L3）。
const KEY1097_SIZES = [
  { d: 8, b: 8, h: 7, label: 'b=8（h=7）', lengths: [25, 28, 32, 36, 40, 45, 50, 56, 63, 70], d0: 3, l0: 7 },
  { d: 10, b: 10, h: 8, label: 'b=10（h=8）', lengths: [25, 28, 32, 36, 40, 45, 50, 56, 63, 70, 80, 90], d0: 3, l0: 8 },
];
// 键型样式表桩（与 partgen_keys::KEY_STYLES 下发的顶层 `key_styles` 同形；
// GUI 下拉/位置约束/双槽可用性/信息行由它驱动）。
const KEY_STYLES_JSON = [
  { id: 'A', aliases: [], label: '普通平键A型', kind: 'A', guided: false, standard: 'GB/T 1096-2003', places: ['mid', 'end'], allow_double: true, place_hint: '' },
  { id: 'B', aliases: [], label: '普通平键B型', kind: 'B', guided: false, standard: 'GB/T 1096-2003', places: ['mid', 'end'], allow_double: true, place_hint: '' },
  { id: 'C', aliases: [], label: '普通平键C型', kind: 'C', guided: false, standard: 'GB/T 1096-2003', places: ['mid', 'end'], allow_double: true, place_hint: '' },
  { id: '导向A', aliases: ['導向A', 'GUIDED_A'], label: '导向平键A型', kind: 'A', guided: true, standard: 'GB/T 1097-2003', places: ['mid'], allow_double: false, place_hint: '固定键固定在轴上；端置是普通平键 KEY 的画法' },
  { id: '导向B', aliases: ['導向B', 'GUIDED_B'], label: '导向平键B型', kind: 'B', guided: true, standard: 'GB/T 1097-2003', places: ['mid'], allow_double: false, place_hint: '固定键固定在轴上；端置是普通平键 KEY 的画法' },
];
const KEY1097_ROWS = [
  { l: 25, l1: 13, l2: 12.5, l3: 6 },
  { l: 28, l1: 14, l2: 14, l3: 7 },
  { l: 32, l1: 16, l2: 16, l3: 8 },
  { l: 36, l1: 18, l2: 18, l3: 9 },
  { l: 40, l1: 20, l2: 20, l3: 10 },
  { l: 45, l1: 23, l2: 22.5, l3: 11 },
  { l: 50, l1: 26, l2: 25, l3: 12 },
  { l: 56, l1: 30, l2: 28, l3: 13 },
  { l: 63, l1: 35, l2: 31.5, l3: 14 },
  { l: 70, l1: 40, l2: 35, l3: 15 },
  { l: 80, l1: 48, l2: 40, l3: 16 },
  { l: 90, l1: 54, l2: 45, l3: 18 },
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
      const derive = mkEl('key-derive');
      const deriveMatch = /class="chk key-derive"[^>]*>([^<]*)<\/span>/.exec(rowHtml);
      derive.textContent = htmlDecode(deriveMatch?.[1] || '');
      tr.querySelector = (sel) => {
        if (sel === '.key-derive') return derive;
        const field = /^\[data-f="([^"]+)"\]$/.exec(sel);
        return field ? tr._fields.find((f) => f.dataset.f === field[1]) || null : null;
      };
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
  f.readOnly = /\breadonly\b/.test(attrsText);
  f.checked = /\bchecked\b/.test(attrsText);
  if (tagName === 'SELECT') {
    f.options = [];
    for (const it of tagHtml.matchAll(/<option value="([^"]*)"([^>]*)>([\s\S]*?)<\/option>/g)) {
      const o = {
        value: htmlDecode(it[1]),
        textContent: htmlDecode(it[3]),
        selected: /\bselected\b/.test(it[2]),
        disabled: /\bdisabled\b/.test(it[2]),
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

// DSL/JSON → 归一化段模型（够本测试用：含 CH/FI；花键查 de，齿轮回填；INVOLSPLINE 非法）。
function segmentsFor(dsl) {
  if (String(dsl).trimStart().startsWith('{')) {
    try {
      const model = JSON.parse(dsl);
      return model.segments.map((seg) => ({ ...seg, e: seg.e ?? seg.s }));
    } catch (e) {
      return { error: 'JSON 模型解析失败：' + e.message };
    }
  }
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
    // GEAR / SPLINE(渐开线齿形段) 段（与 shaft.rs 子关键字对齐；EX/IN/X/DA/DF/H/ALPHA 可省）。
    // 矩形花键已在上面的规格分支拦下，这里只处理“SPLINE + 齿形关键字”。
    const gr = /^(GEAR|SPLINE)\b(.*)$/i.exec(line);
    if (gr && (gr[1].toUpperCase() === 'GEAR' || /^\s*(EX|IN|M|Z|X|DA|DF|ALPHA|BETA|H)\b/i.test(gr[2]))) {
      if (!/\bZ\s*=?\s*\d+/i.test(gr[2])) {
        return { error: `第 1 行（第 1 段）：关键字 ${gr[1].toUpperCase()} 缺少 Z（齿数）` };
      }
      const g = {
        involute: gr[1].toUpperCase() === 'SPLINE',
        kind: /\bIN\b/i.test(gr[2]) ? 'internal' : 'external',
        m: Number(/\bM\s*=?\s*(-?[\d.]+)/i.exec(gr[2])[1]),
        z: Number(/\bZ\s*=?\s*(\d+)/i.exec(gr[2])[1]),
        h: null,
        alpha: null,
      };
      const h = /\bH\s*=?\s*(-?[\d.]+)/i.exec(gr[2]);
      if (h) g.h = Number(h[1]);
      const al = /\bALPHA\s*=?\s*(-?[\d.]+)/i.exec(gr[2]);
      if (al) g.alpha = Number(al[1]);
      const xv = /\bX\s*=?\s*(-?[\d.]+)/i.exec(gr[2]);
      if (xv && Number(xv[1]) !== 0) g.x = Number(xv[1]);
      const dav = /\bDA\s*=?\s*(-?[\d.]+)/i.exec(gr[2]);
      if (dav) g.da = Number(dav[1]);
      const dfv = /\bDF\s*=?\s*(-?[\d.]+)/i.exec(gr[2]);
      if (dfv) g.df = Number(dfv[1]);
      out.push({ gear: g });
      continue;
    }
    // 普通轴段（S/E/L…）：保留真实尺寸，CH/FI/KEY 子关键字回填后端同形模型。
    if (/^S\s*=?\s*[\d.]/i.test(line)) {
      // 保留 S/E/L 真值（KEY 折算探针需要真实轴径；缺省回退 30/30/10）。
      const sm = /\bS\s*=?\s*([\d.]+)/i.exec(line);
      const em = /\bE\s*=?\s*([\d.]+)/i.exec(line);
      const lm = /\bL\s*=?\s*([\d.]+)/i.exec(line);
      const s0 = sm ? Number(sm[1]) : 30;
      const seg = { s: s0, e: em ? Number(em[1]) : s0, l: lm ? Number(lm[1]) : 10 };
      for (const [keyword, field, size] of [['CH', 'ch', 'c'], ['FI', 'fillet', 'r']]) {
        const re = new RegExp('\\b' + keyword + '\\s*=?\\s*([+-]?[\\d.]+)(?:\\s*@\\s*([LR]))?'
          + (keyword === 'FI' ? '(?::(CONCAVE|CONVEX))?' : ''), 'gi');
        const features = [...line.matchAll(re)].map((it) => {
          const f = { [size]: Number(it[1]), end: (it[2] || 'R').toUpperCase() };
          if (keyword === 'FI' && it[3]?.toUpperCase() === 'CONCAVE') f.kind = 'concave';
          return f;
        });
        if (features.length) seg[field] = features;
      }
      const km = /\bKEY\s+(\S+)\s+(?:KL\s*=?\s*)?([\d.]+)/i.exec(line);
      if (km) {
        const rawType = km[1];
        const style = KEY_STYLES_JSON.find((s) => s.id === rawType)
          || KEY_STYLES_JSON.find((s) => (s.aliases || []).some((a) => a.toUpperCase() === rawType.toUpperCase()))
          || KEY_STYLES_JSON.find((s) => s.kind === rawType.toUpperCase() && !s.guided);
        seg.keyway = {
          type: style ? style.kind : rawType.toUpperCase(),
          l: Number(km[2]),
          place: /@\s*端/.test(line) ? 'end' : 'mid',
        };
        if (style && style.guided) seg.keyway.guided = true;
        // `b8h7` 连写 / `b8` / `b10`（与后端同口径：b 必给，h 跟 b 走）。
        const bm = /\bb\s*=?\s*([\d.]+)(?:h\s*=?\s*([\d.]+))?/i.exec(line);
        if (bm) {
          seg.keyway.b = Number(bm[1]);
          if (bm[2] !== undefined) seg.keyway.h = Number(bm[2]);
        }
        const sl = /\bSL\s*=?\s*([+-]?[\d.]+)/i.exec(line);
        if (sl) seg.keyway.slot_l = Number(sl[1]);
        const tm = /\bt1\s*=?\s*([\d.]+)/i.exec(line);
        if (tm) seg.keyway.t1 = Number(tm[1]);
        if (/双槽|\bDOUBLE\b/i.test(line)) seg.keyway.double = true;
        // 旧写法 `KEY A 25 导向` 的开关标记（新写法 `KEY 导向A 25` 已在上面从样式表得到）。
        if (/导向|\bGUIDED\b/i.test(line)) seg.keyway.guided = true;
      }
      out.push(seg);
      continue;
    }
    // 其它开头（纯数字/int/ext/中文体系名…）= OCSMGEAR CLI 误贴或非法段：复刻后端的“不识别的关键字”。
    return { error: `第 1 行（第 1 段）：不识别的关键字「${line.split(/\s+/)[0]}」` };
  }
  return out;
}

let lastParseDsl = '';
let lastExportModel = null;
let exportRequests = 0;
global.fetch = async (u, opts = {}) => {
  const url = String(u);
  if (url.startsWith('/api/parts')) {
    return jsonResp({
      ok: true,
      families: {
        detail_spline_rect: { specs: SPECS },
        key_1096_a: { sizes: KEY_SIZES, shaft_ranges: KEY_RANGES },
        key_1096_b: { sizes: KEY_SIZES, shaft_ranges: KEY_RANGES },
        key_1096_c: { sizes: KEY_SIZES, shaft_ranges: KEY_RANGES },
        key_1097_a: { sizes: KEY1097_SIZES, length_rows: KEY1097_ROWS },
        key_1097_b: { sizes: KEY1097_SIZES, length_rows: KEY1097_ROWS },
      },
      key_styles: KEY_STYLES_JSON,
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
    const parsed = segmentsFor(dsl);
    if (parsed.error) return jsonResp({ ok: false, error: parsed.error }, 400);
    const conflict = parsed.findIndex((seg) => (seg.ch || []).some((c) =>
      (seg.fillet || []).some((f) => (c.end || 'R').toUpperCase() === (f.end || 'R').toUpperCase())));
    if (conflict >= 0) {
      return jsonResp({ ok: false, error: `第 ${conflict + 1} 段：CH 与 FI 不能位于同一端。` }, 400);
    }
    return jsonResp({ ok: true, segments: parsed, at: null, rot: null, view: 'normal' });
  }
  if (url.startsWith('/api/shaft_export')) {
    lastExportModel = JSON.parse(opts.body || '{}');
    exportRequests += 1;
    return jsonResp({ ok: true, message: '已生成到图纸' });
  }
  if (url.startsWith('/api/shaft_preview')) {
    return textResp('<svg xmlns="http://www.w3.org/2000/svg"><line x1="0" y1="0" x2="1" y2="1"/></svg>');
  }
  return jsonResp({ ok: true });
};

// ── 跑 GUI 脚本（末尾探针暴露 IIFE 内部函数；不改生产代码）──────────
const probed = script.replace(/\}\)\(\);\s*$/, `;globalThis.__shaft = {
  get rows() { return rows; },
  get sel() { return sel; },
  set sel(v) { sel = v; renderTable(); },
  refreshFromText, modelFromRows, rowToDsl, rowFromSegment,
  get specs() { return splineSpecs; },
};
})();`);
try {
  // 页面外链的共享助手（/ocsm_gui_common.js）：Node 里按同一目录文件先求值
  (0, eval)(renderZh(fs.readFileSync(htmlPath.replace(/[^/]+$/, 'ocsm_gui_common.js'), 'utf8')));
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

// ⓪ 打开即为空段表（用户 2026-09-23 定案：不再预填示例轴特征）；视图按钮只有 常规/剖视。
check(S.rows.length === 0, '打开轴生成器应为空段表，实为 ' + S.rows.length);
check(segBody._rows.length === 0, '段表 DOM 应为空，实为 ' + segBody._rows.length);
check(segBody._innerHTML.includes('colspan="11"'), '加入 FI 列后空表提示应跨 11 列');
check(dslEl.value === '', '行文本应为空，实为 ' + JSON.stringify(dslEl.value));
check(!segBody._innerHTML.includes('S30 E30 L45'), '不应预填示例轴特征');
const viewNames = document.getElementById('viewRow').children.map((b) => b.textContent);
check(viewNames.join('|') === '常规|剖视', '视图按钮应只有 常规|剖视（双视图已移除），实为 ' + viewNames.join('|'));
// 空表 →「+ 加行」→ 第一段 → 可生成模型（空表可用性）
document.getElementById('addRow').click();
await new Promise((r) => setImmediate(r));
check(S.rows.length === 1 && segBody._rows.length === 1, '空表点「+ 加行」应加出第一段');
const firstModel = S.modelFromRows();
check(
  !!firstModel && firstModel.segments.length === 1
    && firstModel.segments[0].s === 30 && firstModel.segments[0].l === 10
    && !Object.hasOwn(firstModel.segments[0], 'fillet'),
  '加第一段后应能生成模型（S30 L10）：' + JSON.stringify(firstModel)
);
// 回到空表（后面用例自己设置行文本）
dslEl.value = '';
await S.refreshFromText();
check(S.rows.length === 0, '清空行文本应回到空表');

check(S.specs.length === SPECS.length, `/api/parts 规格未加载（${S.specs.length}）`);

// ⓪.5 信息分层（照 hole_gui 口径）：常显区（可见文本；剥标签、title 悬停不计）不得含
//     公式/口径/来源/预先警告；公式与口径进 title（原生 title，不自造 tooltip）。
const visibleText = html
  .replace(/<script[\s\S]*?<\/script>/g, ' ')
  .replace(/<style[\s\S]*?<\/style>/g, ' ')
  .replace(/<[^>]+>/g, ' ');
for (const bad of [
  '0.85d', 'GB/T 1095', 'GB/T 1144', '槽长按型别折算', '仅剖视图体现',
  '留空 10m', '留空 20°', '10m（模板', 'ByLayer', '不含尺寸标注',
  '双视图已移除', 'ES… 旧写法已取消', '径节制 DP 走', 'TODO',
]) {
  check(!visibleText.includes(bad), `常显区不应含公式/口径/预先警告「${bad}」`);
}
// 正向：公式/口径确实进了 title（视图按钮动态 title + 静态控件 title）。
const layButtons = document.getElementById('viewRow').children;
check(
  layButtons.length === 2 && layButtons[0].title.includes('外形可见线') && layButtons[1].title.includes('ANSI31'),
  '视图按钮应带画法口径 title：' + layButtons.map((b) => b.title).join('|')
);
check(html.includes('title="生成物：轮廓/端面/倒角'), '生成物/图层口径应进「生成到图纸」title');
check(html.includes('title="段级退刀槽：RL@L'), '退刀槽/花键口径应进行文本框 title');

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
check(row._html.includes('title="规格来自 GB/T 1144'), '花键规格来源/口径应进规格下拉 title');
check(row._html.includes('title="螺纹段：要求 S=E；不写值 = 小径 0.85d'), 'M 段公式应进 M 勾选 title');

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

// ⑦ 渐开线花键轴段（INVOLSPLINE）已撤（唯一入口 = OCSMGEAR 花键模式）：面板/行文本不再接受
check(segBody._rows.every((r) => !r._html.includes('invol')), '段表不应再有 INVOLSPLINE 列/字段');
const rowsBeforeGone = S.rows.length;
dslEl.value = 'INVOLSPLINE GB30R M3 Z20 L30';
const okGone = await S.refreshFromText();
check(okGone === false, '“INVOLSPLINE …” 行文本应解析失败');
check(dslErr.textContent.includes('不识别的关键字') && dslErr.textContent.includes('INVOLSPLINE'),
  '错误应点名不识别的关键字 INVOLSPLINE：' + dslErr.textContent);
check(S.rows.length === rowsBeforeGone, '解析失败不应改段表');

// ⑧ 表达式互通（齿轮窗口 → 轴生成器粘贴）：往返一致 + 容错 + 错误定位 + 开窗
const elv = (id) => document.getElementById(id);
const tick = async (n = 3) => { for (let i = 0; i < n; i++) await new Promise((r) => setImmediate(r)); };

// ⑧.1 表达式框：OCSMGEAR 前缀 + 中文空格 + 多空格清洗后解析，GEAR 段追到末尾
// （字面量与 gear_gui_smoke.mjs 断言的齿轮窗口产出一致 = 两端往返契约）
dslEl.value = 'S30 E30 L10';
await S.refreshFromText();
const rowsBeforeExpr = S.rows.length;
elv('exprInput').value = 'OCSMGEAR\u3000GEAR\u3000M3\u3000Z20\u3000H30\u3000ALPHA20';
elv('exprAdd').click();
await tick();
check(lastParseDsl === 'GEAR M3 Z20 H30 ALPHA20',
  `清洗后应剥命令名/中文空格并折叠多空格，实为 ${JSON.stringify(lastParseDsl)}`);
check(S.rows.length === rowsBeforeExpr + 1, '表达式框应追加一段');
const gw = S.rows[S.rows.length - 1];
check(gw.gear.on && gw.gear.m === '3' && gw.gear.z === '20' && gw.gear.h === '30' && gw.gear.alpha === '20',
  `GEAR 段参数应与齿轮侧一致：${JSON.stringify(gw.gear)}`);
check(dslEl.value.includes('GEAR M3 Z20 H30 ALPHA20'), `行文本应同步：${JSON.stringify(dslEl.value)}`);
check(elv('exprInput').value === '', '加段后表达式框应清空');
check(elv('exprErr').textContent === '', '成功时不应有错误：' + elv('exprErr').textContent);

// ⑧.2 在 GEAR 输入框里粘贴整条表达式 → 整行替换（参数以粘贴的表达式为准）
const grow = segBody._rows[S.rows.length - 1];
const gearMInput = grow._fields.find((f) => f.dataset.f === 'gear.m');
check(!!gearMInput, 'GEAR 行应有 m 输入框');
let prevented = false;
segBody._fire('paste', gearMInput, {
  clipboardData: { getData: () => 'GEAR M5 Z10 H50 ALPHA25' },
  preventDefault: () => { prevented = true; },
});
await tick();
check(prevented, '识别为表达式时应 preventDefault（不让原文贴进数字框）');
check(S.rows.length === rowsBeforeExpr + 1, '粘贴表达式应整行替换而非多加行');
const g2 = S.rows[S.rows.length - 1];
check(g2.gear.on && g2.gear.m === '5' && g2.gear.z === '10' && g2.gear.h === '50' && g2.gear.alpha === '25',
  `替换后参数应为粘贴值：${JSON.stringify(g2.gear)}`);

// ⑧.3 纯数字粘贴不拦截（仍走输入框原生粘贴）
prevented = false;
segBody._fire('paste', gearMInput, {
  clipboardData: { getData: () => '5' },
  preventDefault: () => { prevented = true; },
});
check(!prevented, '纯数字粘贴不应被表达式拦截');

// ⑧.5 解析失败：明确错误 + 段定位，且不改段表
const rowsBeforeBad = S.rows.length;
elv('exprInput').value = 'GEAR M3';
elv('exprAdd').click();
await tick();
check(elv('exprErr').textContent.includes('GEAR 缺少 Z'),
  '解析失败应显示后端明确错误：' + elv('exprErr').textContent);
check(elv('exprErr').textContent.includes('第 1'), '错误应指出段位置：' + elv('exprErr').textContent);
check(S.rows.length === rowsBeforeBad, '解析失败不应改段表');

// ⑧.6 OCSMGEAR 命令行语法误贴 → 专门指路（仍带后端段定位）
elv('exprInput').value = 'OCSMGEAR 3 20 30';
elv('exprAdd').click();
await tick();
check(elv('exprErr').textContent.includes('OCSMGEAR 命令行语法'),
  '误贴 CLI 应给专门指路：' + elv('exprErr').textContent);
check(elv('exprErr').textContent.includes('第 1'), '错误仍应带段定位：' + elv('exprErr').textContent);

// ⑧.7 表达式框直接粘贴（Ctrl+V）也立即解析并加段
const rowsBeforePaste = S.rows.length;
prevented = false;
elv('exprInput')._fire('paste', elv('exprInput'), {
  clipboardData: { getData: () => 'GEAR M4 Z12 H24' },
  preventDefault: () => { prevented = true; },
});
await tick();
check(prevented && S.rows.length === rowsBeforePaste + 1, '表达式框粘贴应被拦截并加段');
check(S.rows[S.rows.length - 1].gear.m === '4', '粘贴的 GEAR 段参数应回填');

// ⑧.8 打开齿轮生成器：window.open("/gear")（guide server 会补 app 窗口）
let opened = null;
global.window.open = (u) => { opened = u; return {}; };
elv('openGear').click();
check(opened === '/gear', '打开齿轮生成器应 window.open("/gear")，实际 ' + opened);

// ⑧.9 INVOLSPLINE 轴段已撤：表达式框粘贴同样拒绝（护栏）
const rowsBeforeInvolGone = S.rows.length;
elv('exprInput').value = 'INVOLSPLINE GB30R M3 Z20 L30';
elv('exprAdd').click();
await tick();
check(elv('exprErr').textContent.includes('不识别的关键字') && elv('exprErr').textContent.includes('INVOLSPLINE'),
  '表达式框应拒绝 INVOLSPLINE：' + elv('exprErr').textContent);
check(S.rows.length === rowsBeforeInvolGone, '被拒的表达式不应加段');

// ⑧.10 统一齿形段（齿轮/花键同一套）粘贴：SPLINE(渐开线) + EX/IN + X/DA/DF 全保留，
// 再回写行文本 → 与粘贴的几何参数一致（MARK 参与几何，不能只在解析层丢）。
const rowsBeforeUnified = S.rows.length;
elv('exprInput').value = 'OCSMGEAR SPLINE EX M3 Z20 ALPHA30 X0.3 DA66 DF52.5 H30';
elv('exprAdd').click();
await tick();
check(elv('exprErr').textContent === '', '统一花键表达式应解析成功：' + elv('exprErr').textContent);
check(S.rows.length === rowsBeforeUnified + 1, '统一花键表达式应加一段');
const ug = S.rows[S.rows.length - 1].gear;
check(ug.on && ug.mark === 'SPLINE' && ug.kind === 'EX',
  `统一花键段应保留 MARK/KIND：${JSON.stringify(ug)}`);
check(Math.abs(Number(ug.x) - 0.3) < 1e-9 && Math.abs(Number(ug.da) - 66) < 1e-9 && Math.abs(Number(ug.df) - 52.5) < 1e-9,
  `统一花键段应保留 X/DA/DF：${JSON.stringify(ug)}`);
check(dslEl.value.includes('SPLINE EX M3 Z20 ALPHA30 X0.3 DA66 DF52.5 H30'),
  `行文本应回写统一齿形段：${JSON.stringify(dslEl.value)}`);
// 内齿 + GEAR 标记：IN 与 GEAR 也保留。
elv('exprInput').value = 'GEAR IN M3 Z20 ALPHA20 X0 DA67.5 DF54 H30';
elv('exprAdd').click();
await tick();
const ig = S.rows[S.rows.length - 1].gear;
check(ig.on && ig.mark === 'GEAR' && ig.kind === 'IN' && Math.abs(Number(ig.da) - 67.5) < 1e-9,
  `内齿统一段应保留 GEAR/IN/DA：${JSON.stringify(ig)}`);
check(dslEl.value.includes('GEAR IN M3 Z20 ALPHA20 DA67.5 DF54 H30'),
  `内齿行文本应回写：${JSON.stringify(dslEl.value)}`);

// ⑧.11 KEY 平键轴槽（段类型，与 M/SPLINE 同级）：b×h 由轴径自动定、只读；与 GEAR/SPLINE 互斥。
dslEl.value = 'S25 E25 L40 CH2@L KEY A 18 | S30 E30 L30';
await S.refreshFromText();
S.sel = 0;
check(S.rows.length === 2 && S.rows[0].key.on && S.rows[0].key.style === 'A'
  && S.rows[0].key.l === '18' && S.rows[0].key.place === 'mid' && S.rows[0].key.b === '',
  'KEY 行应回填段表（b 省略 = 由轴径自动定）：' + JSON.stringify(S.rows[0].key));
let krow = segBody._rows[0];
check(krow._html.includes('b8×h7') && krow._html.includes('t1=4'),
  'KEY 列应只读显示由 d25 查得的 b8×h7 / t1=4：' + krow._html.slice(0, 400));
check(krow._html.includes('title="平键轴槽（与 GEAR / SPLINE / M / OV 互斥）')
  && krow._html.includes('title="键型（表驱动）'),
  'KEY 口径（互斥/折算/查表）应进 title');
const keyStyleF = krow._fields.find((f) => f.dataset.f === 'key.style');
const keyLenF = krow._fields.find((f) => f.dataset.f === 'key.l');
const keyPlaceF = krow._fields.find((f) => f.dataset.f === 'key.place');
check(!!keyStyleF && !!keyLenF && !!keyPlaceF, 'KEY 列应有 键型 / 键长 L / 位置 三个字段');
check(!krow._fields.some((f) => f.dataset.f === 'key.guided'), '「导向」复选框应已删除（并入键型下拉）');
check(!!keyLenF && keyLenF.options.some((o) => o.value === '18'),
  '键长下拉应有平键族 L 候选：' + JSON.stringify(keyLenF && keyLenF.options.map((o) => o.value)));
check(dslEl.value.includes('KEY A 18') && !/\bb\d/.test(dslEl.value),
  '自动 b×h 不写进 DSL（由后端按 d 定）：' + JSON.stringify(dslEl.value));
check(!!krow._fields.find((f) => f.dataset.f === 'key.l' && f.disabled === false), 'KEY 段 L 可选');
// 合并键型下拉：普通 A/B/C + 导向 A/B（表驱动；顺序照样式表）。
check(keyStyleF.options.map((o) => o.value).join(',') === 'A,B,C,导向A,导向B',
  '键型下拉应为 A/B/C/导向A/导向B：' + JSON.stringify(keyStyleF.options.map((o) => o.value)));
check(
  keyStyleF.options.map((o) => o.textContent).join('|') === '普通平键A型|普通平键B型|普通平键C型|导向平键A型|导向平键B型',
  '键型下拉名称应为用户口径：' + JSON.stringify(keyStyleF.options.map((o) => o.textContent))
);
// d45 → b14、t1=5.5（t1≠b/2，便于区分折算）；B 20：中置 20+14=34、端置 20+7+5.5=32.5。
dslEl.value = 'S45 E45 L80 KEY B 20 | S20 E20 L10';
await S.refreshFromText();
S.sel = 0;
krow = segBody._rows[0];
check(krow._html.includes('b14×h9') && krow._html.includes('槽长 34'),
  '中置 B 折算 +b：20+14=34：' + krow._html.slice(0, 400));
segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.place').value = 'end';
segBody._fire('input', segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.place'));
await tick();
krow = segBody._rows[0];
check(krow._html.includes('槽长 32.5'),
  '端置 B 折算 +b/2+t1：20+7+5.5=32.5：' + krow._html.slice(0, 400));
// C 型中置 20+7=27；C 端置 20+5.5=25.5。
segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.style').value = 'C';
segBody._fire('input', segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.style'));
await tick();
segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.place').value = 'mid';
segBody._fire('input', segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.place'));
await tick();
check(segBody._rows[0]._html.includes('槽长 27'), '中置 C 折算 +b/2：20+7=27：' + segBody._rows[0]._html.slice(0, 400));
segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.place').value = 'end';
segBody._fire('input', segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.place'));
await tick();
check(segBody._rows[0]._html.includes('槽长 25.5'), '端置 C 不折算+t1：20+5.5=25.5：' + segBody._rows[0]._html.slice(0, 400));
// 双槽开关（可选项）：默认关；勾选 → 行文本/模型往返；仅剖视体现（GUI 只是开关透传）。
const dblF = segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.double');
check(!!dblF && !dblF.checked, 'KEY 列应有「双槽」勾选框且默认关');
dblF.checked = true;
segBody._fire('input', dblF);
await tick();
check(S.rows[0].key.double === true && dslEl.value.includes('双槽'),
  '勾选双槽应同步行文本：' + JSON.stringify(dslEl.value));
const dm = S.modelFromRows();
check(!!dm && dm.segments[0].keyway && dm.segments[0].keyway.double === true,
  '双槽应进 JSON 模型：' + JSON.stringify(dm && dm.segments[0]));
check(segBody._rows[0]._html.includes('双槽'), 'KEY 单元格应带回显勾选态');
// 互斥：GEAR / SPLINE 段 → KEY 勾选禁用；KEY 段 → GEAR / SPLINE 勾选禁用（与 M/OV 同口径 enforceExclusive）。
dslEl.value = 'GEAR M3 Z20 H30';
await S.refreshFromText();
check(segBody._rows[0]._fields.some((f) => f.dataset.f === 'key.on' && f.disabled), 'GEAR 段应禁用 KEY 勾选');
dslEl.value = 'SPLINE 6x23x26x6 L30';
await S.refreshFromText();
check(segBody._rows[0]._fields.some((f) => f.dataset.f === 'key.on' && f.disabled), 'SPLINE 段应禁用 KEY 勾选');
dslEl.value = 'S25 E25 L40 CH2@L KEY A 18 | S30 E30 L30';
await S.refreshFromText();
krow = segBody._rows[0];
check(krow._fields.some((f) => f.dataset.f === 'gear.on' && f.disabled)
  && krow._fields.some((f) => f.dataset.f === 'spline.on' && f.disabled),
  'KEY 段应禁用 GEAR / SPLINE 勾选');

// ⑧.12 键型下拉与「导向」合并（用户 2026-09-25）：单下拉 5 项（普通A/B/C + 导向A/B），
// 不再单独出复选框；选导向 → 位置只有中置（端置禁用）、双槽禁用、L 换 1097 系列、
// 信息行显示键型名+标准+固定螺钉孔；新写法 `KEY 导向A 25` 与旧写法 `KEY A 25 导向` 等价。
dslEl.value = 'S30 E30 L50 CH2@L CH2@R KEY 导向A 25';
await S.refreshFromText();
S.sel = 0;
check(
  S.rows.length === 1 && S.rows[0].key.on && S.rows[0].key.style === '导向A'
    && S.rows[0].key.l === '25' && S.rows[0].key.place === 'mid',
  '导向（新写法）应回填段表：' + JSON.stringify(S.rows[0].key)
);
const guidRow = segBody._rows[0];
const gStyle = guidRow._fields.find((f) => f.dataset.f === 'key.style');
check(!!gStyle && gStyle.value === '导向A', '键型下拉应选中 导向A：' + JSON.stringify(gStyle && gStyle.value));
const gLen = guidRow._fields.find((f) => f.dataset.f === 'key.l');
check(gLen && gLen.options.some((o) => o.value === '25'), '导向 L 下拉应含 1097 系列 25（1096 无 25）');
check(
  guidRow._html.includes('导向平键A型') && guidRow._html.includes('GB/T 1097-2003')
    && guidRow._html.includes('M3') && guidRow._html.includes('L3=6') && guidRow._html.includes('×h7'),
  '信息行应显示键型名/标准/固定螺钉孔/h7：' + guidRow._html.slice(0, 500)
);
check(!guidRow._html.includes('×h?'), 'b8 信息行不应再出现 h?（1097 sizes 已带 h）');
const newDsl = S.rowToDsl(S.rows[0]);
check(newDsl.includes('KEY 导向A 25') && !/\s导向(\s|$)/.test(newDsl),
  '新写法应写回行 DSL（不再用尾部 导向 开关）：' + JSON.stringify(newDsl));
const gm = S.modelFromRows();
check(
  !!gm && gm.segments[0].keyway && gm.segments[0].keyway.guided === true
    && gm.segments[0].keyway.type === 'A',
  '模型应带 guided + type A（JSON 老 schema）：' + JSON.stringify(gm && gm.segments[0])
);
const gDbl = guidRow._fields.find((f) => f.dataset.f === 'key.double');
check(!!gDbl && gDbl.disabled === true, '导向下双槽应禁用');
const gPlace = guidRow._fields.find((f) => f.dataset.f === 'key.place');
check(!!gPlace && gPlace.value === 'mid', '导向应回填中置：' + JSON.stringify(gPlace && gPlace.value));
check(
  !!gPlace && gPlace.options.find((o) => o.value === 'end').disabled === true,
  '导向下端置选项应禁用（只有中置）'
);
// 导向只中置（用户 2026-09-25 裁定）：程序化把位置改 end 也应被拉回 mid，模型不得带 end。
gPlace.value = 'end';
segBody._fire('input', gPlace);
await tick();
check(S.rows[0].key.place === 'mid', '导向程序化端置应归一到中置：' + JSON.stringify(S.rows[0].key.place));
check(S.modelFromRows().segments[0].keyway.place !== 'end', '导向模型不得带 end');
// 旧写法兼容：`KEY A 25 导向` 仍应映射到 导向平键A型，回填后推荐新写法。
dslEl.value = 'S30 E30 L50 CH2@L CH2@R KEY A 25 导向';
await S.refreshFromText();
S.sel = 0;
check(S.rows[0].key.style === '导向A', '旧写法 `KEY A 25 导向` 应等价映射到 导向A：' + JSON.stringify(S.rows[0].key));
check(S.rowToDsl(S.rows[0]).includes('KEY 导向A 25'), '旧写法回填后应推荐新写法：' + JSON.stringify(S.rowToDsl(S.rows[0])));
// ASCII 别名 `GUIDED_B` 也走同一张表。
dslEl.value = 'S30 E30 L50 KEY GUIDED_B 25';
await S.refreshFromText();
S.sel = 0;
check(S.rows[0].key.style === '导向B', 'GUIDED_B 别名应映射到 导向平键B型：' + JSON.stringify(S.rows[0].key));
// 普通平键仍可选端置（样式表 places = mid/end）。
dslEl.value = 'S30 E30 L50 KEY A 25 @端 | S20 E20 L10';
await S.refreshFromText();
S.sel = 0;
check(S.rows[0].key.style === 'A' && S.rows[0].key.place === 'end', '普通平键端置仍可用：' + JSON.stringify(S.rows[0].key));
const nPlace = segBody._rows[0]._fields.find((f) => f.dataset.f === 'key.place');
check(nPlace.options.find((o) => o.value === 'end').disabled !== true, '普通平键端置不应禁用');

// ⑨ FI 凸/凹圆角：邻接 CH 列、左右独立半径/模式、DSL（含默认右端）/JSON 往返与导出请求。
function rowField(path, index = 0) {
  return segBody._rows[index]?._fields.find((f) => f.dataset.f === path);
}
function setRowField(path, value, index = 0) {
  const f = rowField(path, index);
  check(!!f && !f.disabled, `字段 ${index + 1}/${path} 应存在且可编辑`);
  if (!f || f.disabled) return;
  if (f.type === 'checkbox') f.checked = value;
  else f.value = String(value);
  segBody._fire(f.type === 'checkbox' ? 'change' : 'input', f);
}
function checkEndDisabled(kind, end, disabled, why) {
  for (const part of kind === 'fi' ? ['on', 'r', 'kind'] : ['on', 'c']) {
    const path = `${kind}.${end}.${part}`;
    check(rowField(path)?.disabled === disabled, `${why}：${path} disabled 应为 ${disabled}`);
  }
}
function checkFeatureDisabled(kind, disabled, why) {
  for (const end of ['l', 'r']) checkEndDisabled(kind, end, disabled, why);
}
check(/<th>CH 倒角<\/th>\s*<th>FI 圆角<\/th>\s*<th>OV 越程槽<\/th>/.test(html), 'FI 列应紧邻 CH，位于 CH/OV 之间');
dslEl.value = 'S40 L20 FI2';
check(await S.refreshFromText(), 'FI2 默认右端应解析成功');
check(!S.rows[0].fi.l.on && S.rows[0].fi.r.on && S.rows[0].fi.r.r === '2', '省略 @ 的 FI2 应只勾右端并回填半径 2');
check(rowField('fi.r.on')?.checked && !rowField('fi.l.on')?.checked, 'FI 默认右端应回显到实际勾选框');
check(rowField('fi.r.r')?._attrs.title === SHAFT_ZH['gui.shaft.fillet_radius_title'], 'FI 半径输入应带 i18n 半径说明');
check(rowField('fi.r.on')?._attrs.title === SHAFT_ZH['gui.shaft.fillet_title'], 'FI 勾选框应带凸/凹语法与同端互斥说明');
check(S.rows[0].fi.r.kind === 'convex' && rowField('fi.r.kind')?.value === 'convex', '旧 FI DSL 应默认凸角');
for (const end of ['l', 'r']) {
  const mode = rowField(`fi.${end}.kind`);
  check(mode?.tagName === 'SELECT' && mode.options.map((o) => o.value).join('|') === 'convex|concave', 'FI 每端应有 convex/concave 模式选择');
  check(mode?.options.map((o) => o.textContent).join('|') === '凸角|轴肩根部凹角', 'FI 模式名称应来自 i18n');
  check(mode?._attrs.title === SHAFT_ZH['gui.shaft.fillet_kind_title'], 'FI 模式下拉应带凸角/轴肩根部说明');
}
checkEndDisabled('ch', 'l', false, '仅右 FI 时左 CH 仍可用');
checkEndDisabled('ch', 'r', true, '仅右 FI 时只禁用右 CH');
check(S.rowToDsl(S.rows[0]) === 'S40 L20 FI2@R', 'FI 默认右端应回写显式 @R 且不增加凸角后缀');
setRowField('fi.l.r', 1.25);
setRowField('fi.l.on', true);
setRowField('fi.r.r', 2.5);
check(dslEl.value === 'S40 L20 FI1.25@L FI2.5@R', '左右半径应独立写进 FI DSL：' + dslEl.value);
const bothFillets = [{ r: 1.25, end: 'L' }, { r: 2.5, end: 'R' }];
check(JSON.stringify(S.modelFromRows()?.segments[0].fillet) === JSON.stringify(bothFillets), 'FI JSON 应使用 fillet:[{r,end}]，保留左右独立半径');
checkFeatureDisabled('ch', true, 'FI 两端启用时对应 CH 两端均禁用');
checkFeatureDisabled('fi', false, 'FI 两端可独立编辑');
dslEl.value = 's40 l20 fi1.25@l fi2.5@r';
check(await S.refreshFromText(), 'FI DSL 应大小写不敏感');
check(JSON.stringify(S.modelFromRows()?.segments[0].fillet) === JSON.stringify(bothFillets), '双端 FI DSL 应回填相同 JSON');

dslEl.value = 'S40 L20 FI2:CONCAVE';
check(await S.refreshFromText(), '省略端的 FI2:CONCAVE 应解析为右端凹角');
check(S.rows[0].fi.r.kind === 'concave' && !S.rows[0].fi.l.on, '默认端凹角应回填右端模式');
check(S.rowToDsl(S.rows[0]) === 'S40 L20 FI2@R:CONCAVE', '默认端凹角应回写显式 @R:CONCAVE');
dslEl.value = 's40 l20 fi1.25@l:concave fi2.5@r';
check(await S.refreshFromText(), '凹角模式 DSL 应大小写不敏感');
const mixedFillets = [{ r: 1.25, end: 'L', kind: 'concave' }, { r: 2.5, end: 'R' }];
check(JSON.stringify(S.modelFromRows().segments[0].fillet) === JSON.stringify(mixedFillets), '混合凸/凹 DSL 应仅给凹角序列化 kind');
setRowField('fi.r.kind', 'concave');
check(S.rows[0].fi.l.kind === 'concave' && dslEl.value === 'S40 L20 FI1.25@L:CONCAVE FI2.5@R:CONCAVE', '左右模式可独立选择并立即同步 DSL');
setRowField('fi.l.kind', 'convex');
check(!Object.hasOwn(S.modelFromRows().segments[0].fillet[0], 'kind') && S.rows[0].fi.r.kind === 'concave', '切回凸角应省略 kind 且不改变对侧模式');

const fiJson = { segments: [{ s: 44, e: 38, l: 32, fillet: [mixedFillets[0], { ...mixedFillets[1], kind: 'convex' }] }] };
dslEl.value = JSON.stringify(fiJson);
check(await S.refreshFromText(), '凸/凹 FI JSON 应可从行文本解析回填');
check(rowField('fi.l.on')?.checked && rowField('fi.r.on')?.checked
  && rowField('fi.l.r')?.value === '1.25' && rowField('fi.r.r')?.value === '2.5'
  && rowField('fi.l.kind')?.value === 'concave' && rowField('fi.r.kind')?.value === 'convex', 'FI JSON 应回填两端勾选、半径与模式');
setRowField('fi.r.r', 3.75);
check(dslEl.value === 'S44 E38 L32 FI1.25@L:CONCAVE FI3.75@R', 'JSON 回填后改半径应保留锥段尺寸与另一端模式：' + dslEl.value);
const fiRoundModel = S.modelFromRows();
check(!Object.hasOwn(fiRoundModel.segments[0].fillet[1], 'kind'), '显式 convex JSON 回写模型时也应省略默认 kind');
check(await S.refreshFromText(), '表生成的凸/凹 FI DSL 应可再次解析');
check(JSON.stringify(S.modelFromRows()) === JSON.stringify(fiRoundModel), 'FI JSON → GUI → DSL → GUI → JSON 应保留凸/凹模式');
elv('out').click();
await tick();
check(exportRequests === 1 && JSON.stringify(lastExportModel?.segments) === JSON.stringify(fiRoundModel.segments), '实际导出请求应携带 FI 半径/端点/凹模式');
for (const invalid of ['', '0', '-1', 'Infinity', 'NaN']) {
  setRowField('fi.r.r', invalid);
  check(S.modelFromRows() === null, '非法 FI 半径应阻止生成模型：' + JSON.stringify(invalid));
  check(elv('status').textContent.includes(SHAFT_ZH['gui.shaft.fillet_radius_positive']), '非法 FI 半径应显示 i18n 正数/有限数错误');
}

// ⑨.1 CH/FI 同端双向互斥：只禁用同端勾选/半径/模式，对侧可用并能实际启用、解析回填。
for (const [kind, other] of [['ch', 'fi'], ['fi', 'ch']]) {
  for (const end of ['l', 'r']) {
    dslEl.value = 'S40 L20';
    await S.refreshFromText();
    const opposite = end === 'l' ? 'r' : 'l';
    setRowField(`${kind}.${end}.on`, true);
    checkEndDisabled(other, end, true, `${kind} 应只禁用同端 ${other}`);
    checkEndDisabled(other, opposite, false, `${kind} 不应禁用异端 ${other}`);
    checkFeatureDisabled(kind, false, `${kind} 本身仍可独立操作两端`);
    setRowField(`${other}.${opposite}.on`, true);
    if (other === 'fi') setRowField(`fi.${opposite}.kind`, 'concave');
    const coexist = S.modelFromRows();
    check(coexist?.segments[0].ch?.length === 1 && coexist?.segments[0].fillet?.length === 1, '异端 CH/FI 应同时保留到模型');
    check(await S.refreshFromText(), 'GUI 生成的异端 CH/FI DSL 应解析成功');
    check(JSON.stringify(S.modelFromRows()) === JSON.stringify(coexist), '异端 CH/FI 应往返回填而不丢特征');
    checkEndDisabled(kind, opposite, true, '异端并存后各自只禁用另一特征的同端');
    setRowField(`${kind}.${end}.on`, false);
    checkEndDisabled(other, end, false, '关闭一端特征应立即解除该端禁用，不受对侧影响');
    setRowField(`${other}.${opposite}.on`, false);
    checkFeatureDisabled(other, false, '清除特征后两端均应可用');
  }
}
check(!Object.hasOwn(S.modelFromRows().segments[0], 'fillet') && !/\bFI/.test(dslEl.value), 'FI 全关时 DSL/JSON 均应省略圆角');
for (const coexistText of ['S40 L20 CH2@L FI2@R', 'S40 L20 FI2@L:CONCAVE CH2@R', JSON.stringify({ segments: [{ s: 40, l: 20, ch: [{ c: 2, end: 'L' }], fillet: [{ r: 2, end: 'R', kind: 'concave' }] }] })]) {
  dslEl.value = coexistText;
  check(await S.refreshFromText(), '导入异端 CH/FI（凸/凹 DSL、JSON）应接受：' + coexistText);
  check(S.modelFromRows()?.segments[0].ch?.length === 1 && S.modelFromRows()?.segments[0].fillet?.length === 1, '异端 CH/FI 导入应完整回填');
}

// ⑨.2 复制为深拷贝：两端圆角与模式完整保留，编辑副本不修改原行。
dslEl.value = 'S40 L20 FI1.25@L:CONCAVE FI2.5@R';
await S.refreshFromText();
S.sel = 0;
elv('copyRow').click();
check(S.rows.length === 2 && S.sel === 1, '复制 FI 行应新增并选中副本');
check(S.rows[0].fi !== S.rows[1].fi && S.rows[0].fi.l !== S.rows[1].fi.l && S.rows[0].fi.r !== S.rows[1].fi.r, '圆角复制应深拷贝左右端对象');
check(S.modelFromRows().segments.every((seg) => JSON.stringify(seg.fillet) === JSON.stringify(mixedFillets)), '复制行应保留左右端圆角与模式 JSON');
check(dslEl.value === 'S40 L20 FI1.25@L:CONCAVE FI2.5@R\nS40 L20 FI1.25@L:CONCAVE FI2.5@R', '复制行应保留凸/凹 FI DSL');
setRowField('fi.l.r', 4, 1);
setRowField('fi.l.kind', 'convex', 1);
setRowField('fi.r.on', false, 1);
check(S.rows[0].fi.l.r === '1.25' && S.rows[0].fi.l.kind === 'concave' && S.rows[0].fi.r.on && S.rows[0].fi.r.r === '2.5', '编辑副本半径/模式/勾选不得影响原行');
check(JSON.stringify(S.modelFromRows().segments[1].fillet) === JSON.stringify([{ r: 4, end: 'L' }]), '副本改成凸角并取消右圆角后应只序列化左圆角且省略 kind');

// ⑨.3 切换 GEAR / 矩形 SPLINE 清理圆角，不在退出特殊段型时复活。
for (const kind of ['gear', 'spline']) {
  dslEl.value = 'S40 L20 FI1.25@L:CONCAVE FI2.5@R';
  await S.refreshFromText();
  setRowField(`${kind}.on`, true);
  check(!S.rows[0].fi.l.on && !S.rows[0].fi.r.on, `${kind} 应清理 FI 两端勾选`);
  checkFeatureDisabled('fi', true, `${kind} 应禁用所有 FI 控件`);
  check(!/\bFI/.test(dslEl.value) && !Object.hasOwn(S.modelFromRows().segments[0], 'fillet'), `${kind} DSL/JSON 不应残留 FI`);
  setRowField(`${kind}.on`, false);
  checkFeatureDisabled('fi', false, `退出 ${kind} 应恢复 FI 控件可用`);
  check(!S.rows[0].fi.l.on && !S.rows[0].fi.r.on && !/\bFI/.test(dslEl.value), `退出 ${kind} 不应复活旧圆角`);
  check(!Object.hasOwn(S.modelFromRows().segments[0], 'fillet'), `退出 ${kind} 的普通段模型不应残留 FI`);
}

// ⑨.4 兼容不带 fillet / 空 fillet 的旧 JSON；归一化不会把 CH/FI 导入冲突择一吞掉。
for (const legacySeg of [{ s: 30, l: 20 }, { s: 30, l: 20, fillet: [] }]) {
  dslEl.value = JSON.stringify({ segments: [legacySeg] });
  check(await S.refreshFromText(), '旧 JSON 缺省/空 fillet 应可回填');
  check(!S.rows[0].fi.l.on && !S.rows[0].fi.r.on, '旧 JSON 应初始化两端 FI 为未勾选');
  check(!Object.hasOwn(S.modelFromRows().segments[0], 'fillet'), '空 fillet 不应序列化为空数组');
}
const conflictSeg = { s: 40, e: 40, l: 20, ch: [{ c: 2, end: 'L' }], fillet: [{ r: 2, end: 'L', kind: 'concave' }] };
const conflictRow = S.rowFromSegment(conflictSeg);
check(conflictRow.ch.l.on && conflictRow.fi.l.on && conflictRow.fi.l.kind === 'concave', '归一化不应静默删除同端 CH/FI 冲突中的任一特征或凹模式');
check(S.rowToDsl(conflictRow) === 'S40 L20 CH2@L FI2@L:CONCAVE', '同端冲突行回写 DSL 仍应保留 CH/FI，交后端拒绝');
const savedRow = S.rows[0];
S.rows[0] = conflictRow;
const conflictModel = S.modelFromRows();
check(conflictModel?.segments[0].ch?.length === 1 && conflictModel?.segments[0].fillet?.length === 1, '冲突模型也不得静默择一，应把完整输入交后端校验');
S.rows[0] = savedRow;

// ⑨.5 后端拒绝 CH/FI 同端（凸/凹/默认端/JSON）时保留原段表与输入，不能绕过错误导出。
dslEl.value = 'S40 L20 FI1.25@L:CONCAVE FI2.5@R';
await S.refreshFromText();
const beforeConflicts = JSON.stringify(S.rows);
for (const conflictText of ['S40 L20 CH2@L FI2@L', 'S40 L20 CH2@R FI2@R:CONCAVE', 'S40 L20 CH2 FI2', JSON.stringify({ segments: [conflictSeg] })]) {
  dslEl.value = conflictText;
  dslEl._fire('input', dslEl);
  check(await S.refreshFromText() === false, 'CH/FI 冲突输入应由后端解析错误拒绝');
  check(lastParseDsl === conflictText && dslEl.value === conflictText, '冲突输入应完整送后端并留在文本框，不应静默删特征');
  check(JSON.stringify(S.rows) === beforeConflicts, '冲突解析失败不得修改原段表');
  check(dslErr.textContent.includes('CH') && dslErr.textContent.includes('FI') && dslEl.classList.contains('bad'), '冲突应显示后端 CH/FI 错误并标红文本框');
  const exportsBefore = exportRequests;
  elv('out').click();
  await tick();
  check(exportRequests === exportsBefore, '冲突文本未修正时不得用旧段表生成图纸');
}

// ⑨.6 空表/首行模板/删最后一行仍兼容新增列与 FI 缺省值。
dslEl.value = '';
await S.refreshFromText();
check(S.rows.length === 0 && segBody._rows.length === 0 && segBody._innerHTML.includes('colspan="11"'), '清空 FI/冲突文本应恢复跨 11 列的空表');
elv('copyRow').click();
check(S.rows.length === 0, '空表复制应仍为空表');
elv('addRow').click();
check(!S.rows[0].fi.l.on && !S.rows[0].fi.r.on && S.rows[0].fi.l.r === '2' && S.rows[0].fi.r.r === '2'
  && S.rows[0].fi.l.kind === 'convex' && S.rows[0].fi.r.kind === 'convex', '空表新行应初始化左右独立默认半径与凸角模式');
check(S.rows[0].key.slot_l === '' && rowField('key.slot_l')?.disabled, '空白行 SL 应默认为空且随未启用 KEY 禁用');
checkFeatureDisabled('fi', false, '新空白行的 FI 应可操作');
setRowField('fi.l.on', true);
elv('delRow').click();
check(S.rows.length === 1 && !S.rows[0].fi.l.on && !S.rows[0].fi.r.on, '删最后一行重置模板时应清理圆角');
check(!Object.hasOwn(S.modelFromRows().segments[0], 'fillet'), '空白模板模型应继续省略 fillet');

// ⑩ KEY 独立槽总长 SL：覆盖槽长而不折算键长/键型/t1，保留合法 L 与所有既有键参数。
dslEl.value = 'S45 L80 KEY C 50 @端 SL50 t1 7';
check(await S.refreshFromText(), 'KEY C 50 @端 SL50 t1 7 应解析成功');
S.sel = 0;
check(S.rows[0].key.slot_l === '50' && S.rows[0].key.l === '50' && S.rows[0].key.t1 === '7', '独立槽长 DSL 应分别回填 SL/L/t1');
check(rowField('key.slot_l')?.value === '50' && !rowField('key.slot_l')?.disabled, 'SL 应显示在可编辑的独立输入框');
check(rowField('key.slot_l')?._attrs.title === SHAFT_ZH['gui.shaft.key_slot_l_title'], 'SL 说明应走 i18n');
check(rowField('key.l')?.disabled && rowField('key.l')?.value === '50', 'SL 非空应禁用 L 下拉但保留原合法 L');
const slotDerive = () => segBody._rows[0].querySelector('.key-derive').textContent;
check(slotDerive().includes('槽长 50（SL）') && !slotDerive().includes('槽长 57'), '独立槽长显示不得叠加端置 t1');
check(S.rowToDsl(S.rows[0]) === 'S45 L80 KEY C 50 @端 SL50 t1 7', 'SL 应原样写回 KEY DSL 且保留 L/t1');
check(S.modelFromRows().segments[0].keyway.slot_l === 50 && S.modelFromRows().segments[0].keyway.l === 50, 'JSON 应分别序列化数值 slot_l 与兼容 L');
setRowField('key.style', 'B');
check(slotDerive().includes('槽长 50（SL）'), 'SL 不应按 B 键型补偿长度');
setRowField('key.place', 'mid');
check(slotDerive().includes('槽长 50（SL）'), 'SL 不应按中置/端置重新折算');
setRowField('key.double', true);
setRowField('key.style', 'C');
setRowField('key.place', 'end');
const liveSlotInput = rowField('key.slot_l');
for (const value of [5, 50, 50.25]) {
  setRowField('key.slot_l', value);
  check(rowField('key.slot_l') === liveSlotInput, '连续输入 SL 不应重建输入框、打断焦点');
  check(rowField('key.l')?.disabled && S.rows[0].key.l === '50', '连续输入 SL 时应保留兼容 L 并即时更新禁用态');
  check(slotDerive().includes(`槽长 ${value}（SL）`), '连续输入 SL 时派生槽长应即时同步');
}
const slotRoundModel = S.modelFromRows();
check(slotRoundModel.segments[0].keyway.slot_l === 50.25 && slotRoundModel.segments[0].keyway.t1 === 7
  && slotRoundModel.segments[0].keyway.double === true, 'SL 不应覆盖 t1/双槽等现有键参数');
check(await S.refreshFromText(), 'GUI 输出的 SL DSL 应可再次解析');
check(JSON.stringify(S.modelFromRows()) === JSON.stringify(slotRoundModel), 'SL GUI → DSL → GUI → JSON 应往返一致');
dslEl.value = JSON.stringify(slotRoundModel);
check(await S.refreshFromText(), 'SL JSON 应可回填');
check(S.rows[0].key.slot_l === '50.25' && rowField('key.slot_l')?.value === '50.25' && rowField('key.l')?.disabled, 'SL JSON 应回填输入与 L 禁用态');
const slotExportsBefore = exportRequests;
elv('out').click();
await tick();
check(exportRequests === slotExportsBefore + 1 && JSON.stringify(lastExportModel?.segments) === JSON.stringify(slotRoundModel.segments), '实际导出请求应带独立 slot_l 及原有合法 L/键参数');

// ⑩.1 SL 随 KEY 行深拷贝；副本修改/关闭 KEY 不影响原行，也不向无 KEY 的模型泄露 SL。
S.sel = 0;
elv('copyRow').click();
check(S.rows.length === 2 && S.rows[0].key !== S.rows[1].key
  && S.rows.every((r) => r.key.slot_l === '50.25' && r.key.l === '50'), '复制 KEY 应保留独立槽长与兼容 L，且深拷贝键参数');
setRowField('key.slot_l', 60, 1);
check(S.rows[0].key.slot_l === '50.25' && S.modelFromRows().segments[1].keyway.slot_l === 60, '修改副本 SL 不得影响原行');
setRowField('key.on', false, 1);
check(!S.modelFromRows().segments[1].keyway && !/\bSL/.test(dslEl.value.split('\n')[1]), '关闭 KEY 后副本 DSL/JSON 不应残留 SL');
setRowField('key.on', true, 1);
check(S.rows[1].key.slot_l === '60' && rowField('key.l', 1)?.disabled, '重新启用 KEY 应沿用该行的独立槽长与禁用态');
dslEl.value = JSON.stringify(slotRoundModel);
await S.refreshFromText();
for (const invalid of ['0', '-1', 'Infinity', 'NaN']) {
  setRowField('key.slot_l', invalid);
  check(S.modelFromRows() === null, '非法 SL 应阻止模型生成：' + invalid);
  check(elv('status').textContent.includes(SHAFT_ZH['gui.shaft.key_slot_l_positive']), '非法 SL 应显示 i18n 正数/有限数错误');
  const exportsBefore = exportRequests;
  elv('out').click();
  await tick();
  check(exportRequests === exportsBefore, '非法 SL 不得发出导出请求');
}

// ⑩.2 SL 留空恢复旧规则；键型/端置/导向与原 L、双槽约束不变。
dslEl.value = 'S45 L80 KEY B 20';
await S.refreshFromText();
check(S.rows[0].key.slot_l === '' && !rowField('key.l')?.disabled
  && !Object.hasOwn(S.modelFromRows().segments[0].keyway, 'slot_l'), '旧 KEY DSL 不带 SL 时应默认空值且省略 JSON slot_l');
for (const [place, oldLength, blank] of [['mid', 34, ''], ['end', 32.5, '  ']]) {
  setRowField('key.place', place);
  setRowField('key.slot_l', 50);
  check(slotDerive().includes('槽长 50（SL）'), '填写 SL 应覆盖原折算槽长');
  setRowField('key.slot_l', blank);
  check(!rowField('key.l')?.disabled && S.rows[0].key.l === '20', '清空 SL 应恢复 L 下拉且保留原 L');
  check(slotDerive().includes('槽长 ' + oldLength) && !slotDerive().includes('（SL）'), '清空 SL 应恢复既有 B 型/端置 t1 折算');
  check(!/\bSL/.test(dslEl.value) && !Object.hasOwn(S.modelFromRows().segments[0].keyway, 'slot_l'), '空白 SL 在 DSL/JSON 中均应省略');
}
dslEl.value = JSON.stringify({ segments: [{ s: 30, l: 50, keyway: { type: 'A', l: 18, place: 'mid' } }] });
check(await S.refreshFromText(), '旧 KEY JSON 不带 slot_l 应可回填');
check(S.rows[0].key.slot_l === '' && !rowField('key.l')?.disabled, '旧 JSON 应恢复默认空 SL 与可编辑 L');
dslEl.value = 'S30 L80 KEY 导向A 25 SL40';
await S.refreshFromText();
check(S.modelFromRows().segments[0].keyway.guided === true && slotDerive().includes('槽长 40（SL）'), '导向键应保留 guided 并采用独立槽长显示');
check(rowField('key.double')?.disabled && rowField('key.place')?.options.find((o) => o.value === 'end').disabled, 'SL 不应改变导向键的双槽/端置限制');
setRowField('key.slot_l', '');
check(slotDerive().includes('槽长 25（= 键长 L）') && !rowField('key.l')?.disabled, '清空导向键 SL 应恢复原槽长与 L 下拉');
report();

function report() {
  if (errors.length) {
    console.error('轴 GUI 冒烟失败：\n- ' + errors.join('\n- '));
    process.exit(1);
  }
  console.log('轴 GUI 冒烟通过：规格下拉 / de 自动填 / 派生值 / 自定义手输 / 表外缺 de 报错 / 表达式粘贴往返 / INVOLSPLINE 已撤护栏 / FI 凸凹模式 DSL/JSON 往返与导出 / CH/FI 同端互斥与异端共存 / 圆角复制与特殊段清理 / KEY 独立槽长 SL 往返/复制/导出/正数校验/留空兼容 / 空表兼容');
  process.exit(0);
}
