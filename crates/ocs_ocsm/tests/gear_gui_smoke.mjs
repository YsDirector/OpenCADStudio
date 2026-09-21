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
        { code: 'NFP', std: 'NF', profile: 'NF平齿根', alpha: 20, ha: 0.2, hf: 1.0, rho: 0.3, cf: 0.1 },
        { code: 'NFR', std: 'NF', profile: 'NF圆齿根', alpha: 20, ha: 0.2, hf: 1.147, rho: 0.528, cf: 0.1 },
        { code: 'ANSI30P', std: 'ANSI', profile: 'ANSI30平齿根齿侧', alpha: 30, ha: 0.5, hf: 0.675, rho: 0, cf: 0 },
        { code: 'ANSI30PM', std: 'ANSI', profile: 'ANSI30平齿根外径', alpha: 30, ha: 0.5, hf: 0.5, rho: 0, cf: 0 },
        { code: 'ANSI30R', std: 'ANSI', profile: 'ANSI30圆齿根齿侧', alpha: 30, ha: 0.5, hf: 0.9, rho: 0, cf: 0 },
        { code: 'ANSI375R', std: 'ANSI', profile: 'ANSI37.5圆齿根齿侧', alpha: 37.5, ha: 0.5, hf: 0.65, rho: 0, cf: 0 },
        { code: 'ANSI45R', std: 'ANSI', profile: 'ANSI45圆齿根齿侧', alpha: 45, ha: 0.5, hf: 0.5, rho: 0, cf: 0 },
      ],
      din_nominal: [{ db: 40, m: 2, z: 18, x: 0.45, page: 27 }],
      nf_nominal: [{ a: 80, m: 3.75, z: 19, x: 0.967, page: 21 }],
    },
  },
};
const INFO = {
  ok: true, mode: 'spline', kind: 'internal', kind_label: '内花键', view: 'front',
  std: 'GB', std_code: 'GB/T 3478.1-2008', profile: '30圆齿根', m: 3, z: 20, x: 0, alpha: 30,
  d: 60, db: 51.9615, da: 65.4, df: 57.344, d_b: null, origin: null, rho: 1.2, cf: 0.3,
  internal_major: 65.4, internal_minor: 57.344, block: 'B', spec: 'S', notes: ['内花键无侧视图（同内齿轮：剖视 + 端视）'],
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

// ① 默认齿轮模式：c/beta/chamfer/sys 行在，花键行不在；M 体系显示模数行、隐藏径节行
check(el('stdLabel').style.display === 'none', '齿轮模式下标准号行应隐藏');
check(el('cLabel').style.display !== 'none', '齿轮模式下顶隙行应显示');
check(el('hfLabel').style.display === 'none', '齿轮模式下 hf 行应隐藏');
check(el('sysLabel').style.display !== 'none', '齿轮模式下体系行应显示');
check(el('mLabel').style.display !== 'none', 'M 体系应显示模数行');
check(el('dpLabel').style.display === 'none', 'M 体系应隐藏径节行');

// ①.5 切 DP：径节行显示、模数行隐藏；倒角按 m=25.4/DP 换算
el('sysSel').value = 'DP';
el('dp').value = '8';
try { el('sysSel')._fire('change', el('sysSel')); } catch (e) { errors.push('体系切换异常: ' + e); }
await flush();
check(el('dpLabel').style.display !== 'none', 'DP 体系应显示径节行');
check(el('mLabel').style.display === 'none', 'DP 体系应隐藏模数行');
check(Math.abs(numOf('m') - 3.175) < 1e-9, 'DP8 应换算 m=3.175，实际 ' + numOf('m'));
check(el('chamfer').value === '2', 'DP8 → C=round(0.6×3.175)=2，实际 ' + el('chamfer').value);
// 切回 M
el('sysSel').value = 'M';
try { el('sysSel')._fire('change', el('sysSel')); } catch (e) { errors.push('体系回切异常: ' + e); }
await flush();
check(el('mLabel').style.display !== 'none' && el('dpLabel').style.display === 'none', '切回 M 应恢复模数行');

// ①.6 轴生成器表达式（齿轮模式）：轴段语法一行、不带 view/at；复制按钮写入剪贴板
// （DOM 垫片不会读 HTML 默认值，这里补上齿轮模式的其余默认项）
el('ha').value = '1';
el('c').value = '0.25';
el('betaD').value = '0';
el('betaM').value = '0';
el('betaS').value = '0';
el('x').value = '0';
el('m').value = '3';
el('z').value = '20';
el('h').value = '30';
el('alpha').value = '20';
try { el('m')._fire('input', el('m')); } catch (e) { errors.push('表达式同步异常: ' + e); }
await flush();
check(el('exprPreview').value === 'GEAR M3 Z20 H30 ALPHA20',
  '齿轮表达式应为 GEAR M3 Z20 H30 ALPHA20，实际 ' + JSON.stringify(el('exprPreview').value));
check(el('exprHint').textContent.includes('可直接粘贴'), '齿轮表达式提示应说明可直接粘贴：' + el('exprHint').textContent);
check(!/view|\bat\b/i.test(el('exprPreview').value), '表达式不应带 view/at：' + el('exprPreview').value);
Object.defineProperty(globalThis, 'navigator', {
  value: { clipboard: { writeText: async (t) => { globalThis.__copied = t; } } },
  configurable: true,
});
globalThis.__copied = '';
el('exprCopy')._fire('click', el('exprCopy'));
await flush();
check(globalThis.__copied === 'GEAR M3 Z20 H30 ALPHA20',
  '复制按钮应写入剪贴板，实际 ' + JSON.stringify(globalThis.__copied));

// ①.7 DP 径节制：轴段 GEAR 只认 M，表达式按 m = 25.4/DP 换算
el('sysSel').value = 'DP';
el('dp').value = '8';
try { el('sysSel')._fire('change', el('sysSel')); } catch (e) { errors.push('DP 表达式同步异常: ' + e); }
await flush();
check(el('exprPreview').value === 'GEAR M3.175 Z20 H31.75 ALPHA20',
  'DP 表达式应按 m=25.4/DP 换算，实际 ' + JSON.stringify(el('exprPreview').value));
check(el('exprHint').textContent.includes('25.4/DP'), 'DP 表达式提示应说明换算：' + el('exprHint').textContent);

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

// ③.5 NF：A 行显示、标签为公称直径 A；齿廓 NFP、预设系数；A=80 自动带出 m/z。
el('stdSel').value = 'NF';
try { el('stdSel')._fire('change', el('stdSel')); } catch (e) { errors.push('NF 切换异常: ' + e); }
await flush();
check(el('dbLabel').style.display !== 'none', 'NF 下 A（基准直径）行应显示');
check(el('dbLabel').textContent.includes('公称直径 A'), 'NF 标签应为公称直径 A：' + el('dbLabel').textContent);
check(el('stdHint').textContent.includes('A 主参数'), 'NF 提示应含「A 主参数」：' + el('stdHint').textContent);
check(el('profileSel').value === 'NFP', 'NF 默认齿廓应为 NFP，实际 ' + el('profileSel').value);
check(Math.abs(numOf('hf') - 1.0) < 1e-9, 'NFP hf 预设应 1.0，实际 ' + numOf('hf'));
el('db').value = '80';
try { el('db')._fire('change', el('db')); } catch (e) { errors.push('A 自动带出异常: ' + e); }
await flush();
check(Math.abs(numOf('m') - 3.75) < 1e-9, 'A80 应带出 m=3.75，实际 ' + numOf('m'));
check(el('z').value === '19', 'A80 应带出 z=19，实际 ' + el('z').value);
// ANSI：基准直径行隐藏；输入切到径节 P（m 行显示且标签为径节[P]），hf/rho/cf 隐藏。
el('stdSel').value = 'ANSI';
try { el('stdSel')._fire('change', el('stdSel')); } catch (e) { errors.push('ANSI 切换异常: ' + e); }
await flush();
check(el('dbLabel').style.display === 'none', 'ANSI 下基准直径行应隐藏');
check(el('stdHint').textContent.includes('径节 P'), 'ANSI 提示应含「径节 P」：' + el('stdHint').textContent);
check(!el('stdHint').textContent.includes('未实现'), 'ANSI 提示不应再含「未实现」：' + el('stdHint').textContent);
check(el('mLabel').style.display !== 'none', 'ANSI 下应显示径节输入行（m 行）');
check(el('mLabel').textContent.includes('径节[P]'), 'ANSI 标签应为径节[P]：' + el('mLabel').textContent);
check(el('hfLabel').style.display === 'none', 'ANSI 下 hf 行应隐藏（Table 2 公式算）');
check(el('rhoLabel').style.display === 'none', 'ANSI 下 rho 行应隐藏');
check(el('cfLabel').style.display === 'none', 'ANSI 下 cf 行应隐藏');
check(el('profileSel').value === 'ANSI30P', 'ANSI 默认齿廓应为 ANSI30P，实际 ' + el('profileSel').value);
// ANSI 表达式：shaft.rs 的 P8 写法会被 RL 参数分支先截住，用 M 槽位写径节
check(el('exprPreview').value === 'INVOLSPLINE ANSI30P M3.75 Z19 L31.75',
  'ANSI 表达式应用 M 槽位写径节，实际 ' + JSON.stringify(el('exprPreview').value));
check(el('exprHint').textContent.includes('M 槽位'), 'ANSI 表达式提示应说明 M 槽位：' + el('exprHint').textContent);

// ③.9 花键视图按钮（用户定案：内花键同内齿轮，无侧视图）
// 先回 GB（视图规则与体系无关），锁外花键 = 剖视/侧视/端视
el('stdSel').value = 'GB';
try { el('stdSel')._fire('change', el('stdSel')); } catch (e) { errors.push('GB 回切异常: ' + e); }
await flush();

// ③.95 轴生成器表达式（花键模式）：INVOLSPLINE 轴段语法；复制成功
el('m').value = '3';
el('z').value = '20';
el('h').value = '30';
try { el('m')._fire('input', el('m')); } catch (e) { errors.push('花键表达式同步异常: ' + e); }
await flush();
check(el('exprPreview').value === 'INVOLSPLINE GB30R M3 Z20 L30',
  '花键表达式应为 INVOLSPLINE GB30R M3 Z20 L30，实际 ' + JSON.stringify(el('exprPreview').value));
globalThis.__copied = '';
el('exprCopy')._fire('click', el('exprCopy'));
await flush();
check(globalThis.__copied === 'INVOLSPLINE GB30R M3 Z20 L30',
  '复制按钮应写入花键表达式，实际 ' + JSON.stringify(globalThis.__copied));
const viewBtns = () => el('viewRow').children.map((b) => b.textContent);
const kindBtns = () => el('kindRow').children;
check(viewBtns().join('|') === '剖视图|侧视图|端视图', '外花键应有三视图按钮，实际 ' + viewBtns().join('|'));
// 选中侧视图 → 切内花键：不可用视图自动回剖视，按钮只剩 剖视 + 端视
const sideBtn = el('viewRow').children.find((b) => b.textContent === '侧视图');
check(!!sideBtn, '外花键应有侧视图按钮');
if (sideBtn) {
  sideBtn.click();
  await flush();
  check(el('viewName').textContent === '侧视图', '点侧视图后当前视图应为侧视图，实际 ' + el('viewName').textContent);
}
const intKindBtn = kindBtns().find((b) => b.dataset.k === 'internal');
check(!!intKindBtn, '应有内花键（内齿轮）种类按钮');
if (intKindBtn) {
  intKindBtn.click();
  await flush();
  check(viewBtns().join('|') === '剖视图（齿圈内齿不剖）|端视图', '内花键应只留 剖视图 + 端视图，实际 ' + viewBtns().join('|'));
  check(el('viewName').textContent === '剖视图（齿圈内齿不剖）', '内花键不可用侧视图时应自动回剖视图，实际 ' + el('viewName').textContent);
  check(el('kindHint').innerHTML.includes('不存在侧视图'), '内花键提示应写明无侧视图：' + el('kindHint').innerHTML);
  check(el('exprHint').textContent.includes('不能在轴上使用'), '内花键表达式应标注「不能在轴上使用」：' + el('exprHint').textContent);
  check(el('exprPreview').value.indexOf('INVOLSPLINE GB30R') === 0, '内花键表达式仍应给出参数（只作记录）：' + el('exprPreview').value);
}
// 切回外花键：恢复三视图
const extKindBtn = kindBtns().find((b) => b.dataset.k === 'external');
if (extKindBtn) {
  extKindBtn.click();
  await flush();
  check(viewBtns().join('|') === '剖视图|侧视图|端视图', '切回外花键应恢复三视图，实际 ' + viewBtns().join('|'));
}

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
