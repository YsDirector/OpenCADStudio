// spline_gui.html 运行时/行为冒烟（node 最小 DOM 垫片 + 最小 fetch 桩）。
//
// 目的：锁住花键参数表 GUI 的交互契约（node --check / el("id") 静态扫描查不出）：
//   ① 表驱动：内/外方向、等级/配合/齿根/压力角清单全部由 /api/spline_options 下发，
//      页面里不写死（换体系只加后端数据行）；
//   ② 量棒：标准解 + 3 个工程备选都渲染成可点选 chip；手填也可；
//      ★ 选/填 Dp → 预览请求带 dp、Md 随之重算（用户点名的承诺）；
//   ③ 参数来源：「从选中块 / 上一个块读取」→ 回填 m/z/α/齿根/x；
//   ④ 外花键：量棒面板置灰（applicable=false），结果出公法线长度/跨测齿数；
//   ⑤ 出表：无 at → 待放置件（请求不带 at）；有 at → 直插；成功后自动关窗；
//   ⑥ 旧插件 404 → 直白提示（照 hole_gui.html 的 readApi 判据）；
//   ⑦ 信息分层负断言：公式/口径/来源只在 title=，可见文本（剥标签）里没有。
//
// 用法：node spline_gui_smoke.mjs <spline_gui.html 路径>

import fs from 'node:fs';

const htmlPath = process.argv[2];
if (!htmlPath) {
  console.error('usage: node spline_gui_smoke.mjs <spline_gui.html>');
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

// ── 选项表桩（形状与 /api/spline_options 一致；内容不参与生产断言，只锁交互）──
const COL_INT = [
  '齿形角', '齿数', '模数', '公差等级和配合类别', '大径 Dei', '渐开线终止圆直径最大值',
  '小径 Dii', '测量跨棒距 Md', '量棒直径 Dp', '作用齿槽宽最小值', '实际齿槽宽最大值',
  '齿根圆最小曲率半径 R_imin', '齿形公差 Ff', '齿距累计公差 Fp', '综合公差 λ',
  '小径下公差', '小径上公差', '测量跨棒距下公差', '测量跨棒距上公差', '大径下公差', '大径上公差',
];
const COL_EXT = COL_INT.map((s) =>
  s.replace('大径 Dei', '大径 Dee').replace('小径 Dii', '小径 Die')
   .replace('测量跨棒距 Md', '公法线长度 Wn').replace('量棒直径 Dp', '跨测齿数 Kn')
   .replace('作用齿槽宽最小值', '实际齿厚最小值').replace('实际齿槽宽最大值', '作用齿厚最大值')
   .replace('测量跨棒距下公差', '公法线长度下公差').replace('测量跨棒距上公差', '公法线长度上公差'));
function columns(prefix, labels) {
  return labels.map((label, i) => ({
    tag: `(${prefix})${label}`,
    label,
    unit: i === 0 ? '°' : (label.includes('公差') ? 'mm' : ''),
    formula: `stub 公式 ${i + 1}（不进可见文本）`,
    source: 'stub 来源',
  }));
}
const OPTIONS = {
  ok: true,
  systems: [{
    id: 'gb3478',
    label: 'GB/T 3478（stub）',
    standard: 'GB/T 3478（stub）',
    note: 'stub 体系口径',
    x_note: 'stub x 口径',
    sides: [
      {
        id: 'int', label: '内花键', title: 'stub 内花键',
        grade_note: 'stub 等级口径', alpha_note: 'stub 压力角口径',
        grades: [4, 5, 6, 7],
        fits: [{ fit: 'H', code: 'H', label: 'H（内花键基孔制）', preferred_45: true, memo: 'stub 配合口径' }],
        roots: [
          { key: 'flat', label: '平齿根', alphas: [30, 37.5, 45], note: 'stub 平齿根口径' },
          { key: 'fillet', label: '圆齿根', alphas: [30, 37.5, 45], note: 'stub 圆齿根口径' },
        ],
        alphas: [30, 37.5, 45],
        pin: {
          applicable: true, label: '量棒直径 Dp 与测量跨棒距 Md',
          formula: "stub D'_Ri 公式", md_formula: 'stub M_Ri 公式', standard: 'stub R40 选棒规则',
        },
        columns: columns('内', COL_INT),
      },
      {
        id: 'ext', label: '外花键', title: 'stub 外花键',
        grade_note: 'stub 等级口径', alpha_note: 'stub 压力角口径',
        grades: [4, 5, 6, 7],
        fits: [
          { fit: 'H/k', code: 'k', label: 'H/k', preferred_45: true, memo: 'stub 过盈' },
          { fit: 'H/js', code: 'js', label: 'H/js', preferred_45: false, memo: 'stub 过渡' },
          { fit: 'H/h', code: 'h', label: 'H/h', preferred_45: true, memo: 'stub 间隙 0' },
          { fit: 'H/f', code: 'f', label: 'H/f', preferred_45: true, memo: 'stub 小间隙' },
          { fit: 'H/e', code: 'e', label: 'H/e', preferred_45: false, memo: 'stub 中间隙' },
          { fit: 'H/d', code: 'd', label: 'H/d', preferred_45: false, memo: 'stub 大间隙' },
        ],
        roots: [
          { key: 'flat', label: '平齿根', alphas: [30, 37.5, 45], note: 'stub 平齿根口径' },
          { key: 'fillet', label: '圆齿根', alphas: [30, 37.5, 45], note: 'stub 圆齿根口径' },
        ],
        alphas: [30, 37.5, 45],
        pin: { applicable: false, reason: 'stub 外花键用公法线，不含量棒' },
        columns: columns('外', COL_EXT),
      },
    ],
  }],
  pin_series: [0.56, 0.60, 0.63, 0.67, 0.71, 0.75, 0.80, 0.85, 0.90, 0.95, 1.00, 1.06, 1.12, 1.18, 1.25],
  pin_series_note: 'stub 量棒系列',
};

const SERIES = OPTIONS.pin_series;
const DP_CALC = 0.93;
const pickStd = (x) => SERIES.find((v) => v >= x - 1e-9);
// 桩里的 Md 与 Dp 单调挂钩：选不同 Dp，Md 必然变——断言「选了 Dp 必须重算」用。
const mdOf = (dp) => Number((10 + 1.5 * (dp - 1.0)).toFixed(6));

function findCol(side, needle) {
  return (side.columns || []).find((c) => c.label.includes(needle));
}

function preview(m) {
  const sys = OPTIONS.systems.find((s) => s.id === (m.system || 'gb3478'));
  const side = sys && sys.sides.find((s) => s.id === m.side);
  if (!side) return { ok: false, error: `方向 ${m.side} 不存在` };
  if (!side.grades.includes(m.grade)) return { ok: false, error: `等级 ${m.grade} 不在可选项` };
  if (!side.alphas.includes(m.alpha)) return { ok: false, error: `压力角 ${m.alpha} 不在可选项` };
  if (!(m.m > 0) || !(m.z >= 6)) return { ok: false, error: '模数/齿数不合法' };
  const items = side.columns.map((c) => {
    let value = `v-${c.label}`;
    if (c.label.includes('齿形角')) value = `${m.alpha}°`;
    if (c.label.includes('齿数')) value = String(m.z);
    if (c.label.includes('模数')) value = String(m.m);
    return { tag: c.tag, label: c.label, unit: c.unit, value, formula: c.formula, source: c.source };
  });
  let dp;
  if (side.pin && side.pin.applicable) {
    const series = SERIES;
    if (m.dp != null && !series.some((v) => Math.abs(v - m.dp) < 1e-9)) {
      return { ok: false, error: `量棒参数：Dp=${m.dp} 不在 GB/T 3478.9 表 1 量棒系列（stub）` };
    }
    const auto = pickStd(DP_CALC);
    const current = m.dp != null ? m.dp : auto;
    const md = mdOf(current);
    const col = findCol(side, '测量跨棒距');
    if (col) {
      const it = items.find((x) => x.tag === col.tag);
      if (it) it.value = md.toFixed(3);
    }
    // 与生产同口径：标准解 + 3 个**去重后**的最近系列值（共 4 个可点 chip）
    const choices = [{ value: auto, tag: '标准解', standard: true }];
    for (const v of SERIES.slice().sort((a, b) => Math.abs(a - DP_CALC) - Math.abs(b - DP_CALC))) {
      if (choices.length >= 4) break;
      if (choices.some((c) => Math.abs(c.value - v) < 1e-9)) continue;
      choices.push({ value: v, tag: '备选', standard: false });
    }
    choices.sort((a, b) => a.value - b.value);
    dp = {
      applicable: true, label: side.pin.label, formula: side.pin.formula,
      md_formula: side.pin.md_formula, standard: side.pin.standard,
      current, auto, calc: DP_CALC, manual: m.dp != null,
      choices,
      md: { value: md, lower: md - 0.02, upper: md + 0.03 },
    };
  } else {
    const col = findCol(side, '公法线长度');
    if (col) {
      const it = items.find((x) => x.tag === col.tag);
      if (it) it.value = '35.500';
    }
    dp = { applicable: false, reason: side.pin && side.pin.reason };
  }
  return {
    ok: true, system: m.system || 'gb3478', side: side.id, side_label: side.label,
    grade_fit: `${m.grade}${m.fit}`, m: m.m, z: m.z, alpha: m.alpha, root: m.root, x: m.x || 0,
    dp, items,
  };
}

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
const SELECT_IDS = new Set(['sys', 'grade', 'fit', 'alpha', 'root']);
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
// 出表成功后应像孔/轴/标准件那样自动关窗：window.close 置位 + setTimeout 立即执行
let closed = false;
global.window = { addEventListener() {}, close() { closed = true; } };
global.location = { search: '', href: 'http://127.0.0.1:9/spline' };
global.setTimeout = (fn) => { if (typeof fn === 'function') fn(); return 0; };
global.clearTimeout = () => {};
global.setInterval = () => 0;
global.URLSearchParams = class {
  constructor() {}
  get() { return null; }
};

// ── fetch 桩 ─────────────────────────────────────────────────────
let lastPreviewModel = null;
let lastExportModel = null;
let lastMetaUrl = '';
let lastExportUrl = '';
let forceExport404 = false;
let forceOptions404 = false;
let forcePreview404 = false;
const consoleErrors = [];
const realError = console.error.bind(console); // report() 要用真 stderr（console.error 被冒烟接管了）
console.error = (...a) => { consoleErrors.push(a.join(' ')); };
function jsonResp(obj, status = 200) {
  return { ok: status < 400, status, json: async () => obj, text: async () => JSON.stringify(obj) };
}
function text404(path) {
  return {
    ok: false,
    status: 404,
    text: async () => 'not found',
    json: async () => { throw new SyntaxError(`Unexpected token 'o', "${path}" is not valid JSON`); },
  };
}
global.fetch = async (u, opts = {}) => {
  const url = String(u);
  if (url.startsWith('/api/spline_options')) {
    if (forceOptions404) return text404('/api/spline_options');
    return jsonResp(OPTIONS);
  }
  if (url.startsWith('/api/spline_preview')) {
    if (forcePreview404) return text404('/api/spline_preview');
    const model = JSON.parse(opts.body || '{}');
    lastPreviewModel = model;
    const v = preview(model);
    if (v.ok === false) return jsonResp(v, 400);
    return jsonResp(v);
  }
  if (url.startsWith('/api/spline_meta')) {
    lastMetaUrl = url;
    return jsonResp({
      ok: true, source: url.includes('from=last') ? 'last' : 'sel',
      m: 3, z: 24, alpha: 37.5, x: 0.25, root: 'fillet', label: 'm3 z24 α37.5° 圆齿根',
    });
  }
  if (url.startsWith('/api/spline_export')) {
    lastExportUrl = url;
    if (forceExport404) return text404('/api/spline_export');
    lastExportModel = JSON.parse(opts.body || '{}');
    return jsonResp({ ok: true, message: 'stub 已生成', pending: lastExportModel.at == null });
  }
  return jsonResp({ ok: true });
};

// ── 跑 GUI 脚本（末尾探针暴露内部状态；不改生产代码）──────────────
const probed = script.replace(/\}\)\(\);\s*$/, `;globalThis.__spline = {
  get opt() { return OPT; },
  get sys() { return SYS; },
  get side() { return SIDE; },
  get dp() { return dpSel; },
  currentModel, refresh, syncUi, pickSide, readMeta,
};
})();`);
try {
  (0, eval)(probed);
} catch (e) {
  errors.push('脚本求值异常: ' + (e && e.stack ? e.stack : e));
}
const tick = async (n = 4) => { for (let i = 0; i < n; i++) await new Promise((r) => setImmediate(r)); };
await tick();

const H = globalThis.__spline;
const el = (id) => document.getElementById(id);
check(!!H, '探针 __spline 未挂上（脚本初始化崩溃？）');
if (!H) report();

// ⓪ 信息分层（可见文本，剥标签 + 脚本/样式；title 属性随标签一起剥掉）：
//    公式/口径/来源不得常显；常显只放操作引导。
const visibleText = html
  .replace(/<script[\s\S]*?<\/script>/g, ' ')
  .replace(/<style[\s\S]*?<\/style>/g, ' ')
  .replace(/<[^>]+>/g, ' ');
for (const bad of [
  "D'_Ri", 'M_Ri', 'R40', '3478.9', '0.6√', 'invα', '表 23', '表 24', '表 25',
  '量棒系列', '公式', '来源：',
]) {
  check(!visibleText.includes(bad), `常显区不应含公式/口径/来源「${bad}」`);
}
check(visibleText.includes('选内/外'), `常显区应保留操作引导：${visibleText.slice(0, 200)}`);
// 口径改为原生 title 悬停（页面静态 title + 选项表下发到 title）
check(html.includes('title="花键体系（选项表由后端下发）"'), '体系口径应进原生 title');
check(el('pinLabel').title.includes("stub D'_Ri 公式"), `量棒公式应从选项表进 title：${el('pinLabel').title}`);
check(el('grade').title.includes('stub 等级口径'), `等级口径应进 title：${el('grade').title}`);

// ① 表驱动：清单全部来自 /api/spline_options
check(H.side && H.side.id === 'int', `默认方向应为内花键，实为 ${H.side && H.side.id}`);
check(el('sys').options.map((o) => o.value).join(',') === 'gb3478', '体系下拉来自选项表');
check(el('sys').disabled === true, '只有一个体系时体系下拉应置灰');
check(el('grade').options.map((o) => o.value).join(',') === '4,5,6,7', `等级清单：${el('grade').options.map((o) => o.value)}`);
check(el('grade').value === '6', `默认等级应为 6，实为 ${el('grade').value}`);
check(el('fit').options.map((o) => o.value).join(',') === 'H', `内花键配合应为 H，实为 ${el('fit').options.map((o) => o.value)}`);
check(el('alpha').value === '30', `默认压力角应为 30，实为 ${el('alpha').value}`);
check(el('root').options.map((o) => o.value).join(',') === 'flat,fillet', '齿根形式来自选项表');
check(el('x').disabled === true, 'x（GB 不使用）应置灰');
check(el('x').title.includes('stub x 口径'), 'x 口径应进 title');

// ①.5 打开即预览：模型/21 项/标准解
await H.refresh();
check(!!lastPreviewModel && lastPreviewModel.side === 'int', `预览模型：${JSON.stringify(lastPreviewModel)}`);
check(lastPreviewModel.dp === null, '默认 dp 应为 null（标准解自动）');
check(el('items').innerHTML.split('class="row"').length - 1 === 21, `结果应渲染 21 项：${el('items').innerHTML.slice(0, 120)}`);
check(el('items').innerHTML.includes('测量跨棒距 Md'), '内花键结果应含测量跨棒距');
check(el('items').innerHTML.includes('title="stub 公式'), '公式应在每行 title 里');
const mdAuto = el('mdOut').textContent;
check(mdAuto === mdOf(pickStd(DP_CALC)).toFixed(3).replace(/0+$/, '').replace(/\.$/, '') || Number(mdAuto) === mdOf(pickStd(DP_CALC)),
  `自动 Md 应为标准解对应的值：${mdAuto}`);

// ② 量棒：标准解 + 3 备选 chip，可点选；点选后 Md 重算
const chips = el('dpChoices').children;
check(chips.length === 4, `量棒应显示标准解 + 3 备选（4 个）：${chips.map((c) => c.textContent)}`);
check(chips.some((c) => c.textContent.startsWith('标准解')), `应有标准解 chip：${chips.map((c) => c.textContent)}`);
const altChip = chips.find((c) => Math.abs(Number(String(c.textContent).split(' ')[1]) - pickStd(DP_CALC)) > 1e-9);
check(!!altChip, '应有与标准解不同的备选 chip');
altChip.click();
await tick();
check(lastPreviewModel && Math.abs(Number(lastPreviewModel.dp) - Number(String(altChip.textContent).split(' ')[1])) < 1e-9,
  `点选备选后预览请求应带所选 Dp：${JSON.stringify(lastPreviewModel)}`);
const mdAlt = el('mdOut').textContent;
check(mdAlt !== mdAuto, `★ 选了 Dp，Md 必须重算：${mdAuto} → ${mdAlt}`);

// ②.5 手填：系列值 → 生效；非系列值 → 动态报错（后端拦）
el('dpManual').value = '1.00';
el('dpManual')._fire('input', el('dpManual'));
await tick();
check(lastPreviewModel && Math.abs(Number(lastPreviewModel.dp) - 1.0) < 1e-9, `手填 Dp 应进模型：${JSON.stringify(lastPreviewModel)}`);
check(el('dpManualOn').checked === true, '手填时应自动勾上「手填」');
check(el('mdOut').textContent !== mdAlt, `手填后 Md 应重算：${mdAlt} → ${el('mdOut').textContent}`);
el('dpManual').value = '1.03';
el('dpManual')._fire('input', el('dpManual'));
await tick();
check((el('status').textContent || '').includes('3478.9'), `非系列 Dp 应动态报错并指路：${el('status').textContent}`);
check(el('dpOut').textContent === '—', '报错时 Dp 读数应回 —');
// 回到标准解
el('dpManualOn').checked = false;
el('dpManualOn')._fire('change', el('dpManualOn'));
await tick();
check(lastPreviewModel.dp === null, '取消手填应回到标准解（dp=null）');

// ③ 参数来源：读选中/上一个块 → 回填表单
el('readSel').click();
await tick();
check(lastMetaUrl.includes('from=sel'), `读选中应打 from=sel：${lastMetaUrl}`);
check(Number(el('m').value) === 3 && Number(el('z').value) === 24, `回填 m/z：${el('m').value}/${el('z').value}`);
check(Math.abs(Number(el('alpha').value) - 37.5) < 1e-9, `回填压力角：${el('alpha').value}`);
check(el('root').value === 'fillet', `回填齿根：${el('root').value}`);
check(Number(el('x').value) === 0.25, `回填 x：${el('x').value}`);
check((el('status').textContent || '').includes('已从选中块读取'), `读取成功提示：${el('status').textContent}`);
el('readLast').click();
await tick();
check(lastMetaUrl.includes('from=last'), `读上一个块应打 from=last：${lastMetaUrl}`);

// ④ 切外花键：配合清单 6 项、45° 优先排前并标注；量棒面板置灰
H.pickSide('ext');
await tick();
check(H.side.id === 'ext', '应切到外花键');
const fitCodes = el('fit').options.map((o) => o.value);
check(fitCodes.length === 6 && fitCodes.includes('js'), `外花键配合应 6 项：${fitCodes}`);
check(el('fit').value === 'h', `30/37.5° 默认外花键配合宜 h，实为 ${el('fit').value}`);
el('alpha').value = '45';
el('alpha')._fire('change', el('alpha'));
await tick();
check(el('fit').options[0].value === 'k', `45° 优先项应排前：${el('fit').options.map((o) => o.value)}`);
const labels45 = el('fit').options.map((o) => o.textContent).join('|');
check(labels45.includes('（45° 优先）'), `45° 优先项应有标注：${labels45}`);
check(el('dpManualOn').disabled === true && el('dpManual').disabled === true, '外花键量棒面板应置灰');
await H.refresh();
check(lastPreviewModel.side === 'ext' && lastPreviewModel.dp === null, `外花键预览不应带 dp：${JSON.stringify(lastPreviewModel)}`);
check(el('mdOut').textContent === '—' && el('dpOut').textContent === '—', '外花键不显示量棒读数');
check(el('items').innerHTML.includes('公法线长度'), '外花键结果应含公法线长度');
check(el('items').innerHTML.includes('跨测齿数'), '外花键结果应含跨测齿数');

// ⑤ 出表：无 at → 待放置件；成功后自动关窗
el('ok').click();
await tick(6);
check(lastExportUrl.startsWith('/api/spline_export'), `导出 URL：${lastExportUrl}`);
check(lastExportModel && lastExportModel.side === 'ext', `导出模型：${JSON.stringify(lastExportModel)}`);
check(lastExportModel.at === null, '未填 at 时导出不应带落点（走放置态）');
check(closed === true, '出表成功后应自动关窗');
check((el('status').textContent || '').includes('stub 已生成'), `成功提示：${el('status').textContent}`);

// ⑤.5 有 at → 直插请求带 at/rot
closed = false;
el('atX').value = '10';
el('atY').value = '20';
el('rot').value = '30';
el('ok').click();
await tick(6);
check(JSON.stringify(lastExportModel.at) === '[10,20]', `at 应进模型：${JSON.stringify(lastExportModel.at)}`);
check(Number(lastExportModel.rot) === 30, `rot 应进模型：${lastExportModel.rot}`);
// at 只填一个 → 动态提示、不导出（需手填的异常走动态）
lastExportModel = null;
el('atX').value = '10';
el('atY').value = '';
el('ok').click();
await tick(6);
check(lastExportModel === null, '半截 at 不应导出');
check((el('status').textContent || '').includes('需要 x 与 y 都填'), `半截 at 提示：${el('status').textContent}`);
el('atX').value = '';
el('atY').value = '';

// ⑥ 旧插件 404：直白提示、不关窗
forceExport404 = true;
closed = false;
consoleErrors.length = 0;
el('ok').click();
await tick(6);
check(!closed, '404 时不应关窗');
const st404 = el('status').textContent || '';
check(st404.includes('404') && st404.includes('重开 OCS'), `404 诊断提示：${st404}`);
check(consoleErrors.some((e) => e.includes('/api/spline_export') && e.includes('404') && e.includes('not found')),
  `console.error 应带路径+状态码+响应文本：${consoleErrors.join(' | ')}`);
forceExport404 = false;
// 预览 404 同样友好
forcePreview404 = true;
consoleErrors.length = 0;
await H.refresh();
check((el('status').textContent || '').includes('404'), `预览 404 提示：${el('status').textContent}`);
forcePreview404 = false;

function report() {
  if (errors.length) {
    realError('花键参数表 GUI 冒烟失败：');
    for (const e of errors) realError(' - ' + e);
    process.exit(1);
  }
  console.log('花键参数表 GUI 冒烟通过');
}
report();
