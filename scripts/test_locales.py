"""Check literal translation lookups. Fluent syntax and rendering are tested in i18n.rs.

── 判据范围（2026-09-29 写明；此前本脚本在 HEAD 上本来就是红的，等于一个「会骗人的守门脚本」）──

查什么（能红能绿）
  A. src/**/*.rs 里 t!/tf!( "字面量" ) 的每个字面量，都必须能在 src/locale_catalog.rs 里查到
     (message, attribute) 映射。查不到 ⇒ i18n::translate / translate_format 直接回显英文
     （src/i18n.rs:258-296 的 unwrap_or_else / else 分支）⇒ 全部 21 语言的**静默漏译**。这是本脚本的
     主要红线，报出来时逐条给 file:line + 原文。
  B. catalog 里不能有重复 source；每个映射目标 (message, attribute) 都必须存在于
     locales/en-US/opencadstudio.ftl（否则 get_attr 取不到值）。
  C. 不允许出现**基线之外**的新漏译：见下「已知未译基线」。

不查什么，以及为什么
  1. crates/** 一律不扫。插件侧（OCSM）的手册与命令文案走自己的 OCSMHELP / plugin.toml 通道，
     不经宿主 catalog；把它们算进来只会产生与宿主译文无关的红。
  2. 不含两个以上 ASCII 字母的串不查（"{}"、符号、纯数字）：没有可译文的内容。
  3. 不查各 locales/*.ftl 的**译文质量**（那是人工评审，脚本判不了）。
  4. #[cfg(test)] 里的 t!/tf! **不排除**（理由）：当前 src/ 里 5595 处字面量，落在 #[cfg(test)]
     范围内的是 **0** 处 —— 要么测试在断言译文（那它就该在 catalog 里），要么根本没有这种写法。
     所以排除规则现在只会给"以后可能出现的噪音"开一个口子，而开口子会让真正的漏译漏网。
     真被噪音卡住时再按 cfg(test) 范围排除，届时把理由写在这里（本文件）。

已知未译基线（scripts/locales_untranslated_baseline.tsv）
  上游 2026-08/09 并入的宿主用户可见串（134 处 / 128 条不同串），字面量在 src/ 里，但既没进
  locale_catalog.rs，也没有 21 语言译文 ⇒ 所有语言下都回显英文。上游自己也没补：引入它们的提交
  全部来自上游 main（基线每行都带 git blame 出处：提交/日期/作者/位置，可用 `git blame` 复核）。
  为什么**不**在本脚本里补齐（即主控给的 (a) 方案）：补齐 = 128 串 × 21 语言 = 2688 条人工翻译，
  那是独立翻译单；用机器猜出来的 20 语言译文既不可复核，又会把"漏译"变成"错的译文"——比英文回显更坏。
  所以本脚本把范围写清：(b) 只保证「基线之外不再新增漏译」，基线原样登记、可审计。
  ★ 基线只许缩小：某串进了 catalog/locales 之后请从基线里删掉（脚本会提示"可删"）。
  ★ 往基线里加串是**政策决定**（要评审、要写理由），不是让脚本变绿的办法。

稳定性质
  HEAD 上：绿。新增一条用户可见漏译（或删掉一条 catalog 映射）：非零退出 + 指出那一串。
  本脚本**未**接入 CI（.github/workflows 里没有它）—— 改译文/catalog 前后各手跑一次。
"""

import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
STRING = r'"(?:[^"\\]|\\.)*"'
BASELINE_PATH = ROOT / 'scripts/locales_untranslated_baseline.tsv'
SCOPE_NOTE = '判据范围见本文件头部；漏译要补就得补 catalog + 21 语言译文'


def fail(lines):
    print('\n'.join(lines), file=sys.stderr)
    sys.exit(1)


def unescape_source(text):
    """基线条目第 1 列：\\n \\r \\t 与 \\\\ 是转义写法（值里带换行的字面量写不进一行）。"""
    mapping = {'n': '\n', 'r': '\r', 't': '\t', '\\': '\\'}
    out = []
    index = 0
    while index < len(text):
        char = text[index]
        if char == '\\' and index + 1 < len(text) and text[index + 1] in mapping:
            out.append(mapping[text[index + 1]])
            index += 2
            continue
        out.append(char)
        index += 1
    return ''.join(out)


def load_baseline():
    """已知未译基线：<源串原文>\\t<upstream|local> <提交> <日期> <作者> @ <位置>"""
    if not BASELINE_PATH.exists():
        print(f'基线文件缺失：{BASELINE_PATH.relative_to(ROOT)}（它是本脚本判据的一部分，别删）', file=sys.stderr)
        sys.exit(2)
    baseline = {}
    for number, line in enumerate(BASELINE_PATH.read_text().splitlines(), start=1):
        if not line or line.startswith('#'):
            continue
        if '\t' not in line:
            print(f'基线格式错：{BASELINE_PATH.name}:{number} 缺少 <TAB> 出处列：{line!r}', file=sys.stderr)
            sys.exit(2)
        raw_source, origin = line.split('\t', 1)
        source = unescape_source(raw_source)
        if source in baseline:
            print(f'基线重复条目：{BASELINE_PATH.name}:{number} {source!r}', file=sys.stderr)
            sys.exit(2)
        baseline[source] = origin
    assert baseline, f'{BASELINE_PATH.name} 是空的：基线为空时脚本会在 HEAD 上变红，请先修基线'
    return baseline


baseline = load_baseline()
baseline_stale = set(baseline)

lookup_text = (ROOT / 'src/locale_catalog.rs').read_text()
lookup = {}
for match in re.finditer(r'(' + STRING + r')\s*=>\s*Some\(\(\s*"([\w-]+)"\s*,\s*"([\w-]+)"\s*,?\s*\)\)', lookup_text):
    source = json.loads(match[1])
    assert source not in lookup, f'Duplicate source lookup: {source}'
    lookup[source] = (match[2], match[3])
assert len(lookup) > 3000

keys = set()
group = None
for line in (ROOT / 'locales/en-US/opencadstudio.ftl').read_text().splitlines():
    if match := re.match(r'^([A-Za-z][\w-]*)\s*=', line):
        group = match[1]
    elif match := re.match(r'^    \.([A-Za-z][\w-]*)\s*=', line):
        keys.add((group, match[1]))
if orphan_targets := set(lookup.values()) - keys:
    fail([f'catalog 指向了 locales/en-US/opencadstudio.ftl 里不存在的属性：{sorted(orphan_targets)}'] + [SCOPE_NOTE])

cataloged = 0
without_letters = 0
known_untranslated = []
newly_untranslated = []
for path in sorted((ROOT / 'src').rglob('*.rs')):
    text = path.read_text()
    for match in re.finditer(r'\b(?:t|tf)!\(\s*(' + STRING + ')', text, re.S):
        # Rust permits escaped line continuations and literal newlines.
        literal = re.sub(r'\\\n\s*', '', match[1])
        literal = re.sub(r'\\u\{([0-9a-fA-F_]+)\}', lambda m: json.dumps(chr(int(m[1].replace('_', ''), 16)))[1:-1], literal)
        source = json.loads(literal, strict=False)
        if source in lookup:
            cataloged += 1
            # 故意**不**从 baseline_stale 里剔除：基线条目一旦能查到译文（或代码改写），
            # 就该从基线删掉，否则它将来退化（映射被删）时会被基线吸收 ⇒ 假绿。
            continue
        if not re.search(r'[A-Za-z]{2}', source):
            without_letters += 1
            continue
        line = text[:match.start()].count('\n') + 1
        at = f'{path.relative_to(ROOT)}:{line}'
        if source in baseline:
            baseline_stale.discard(source)
            known_untranslated.append(f'{at}: {source!r}')
        else:
            newly_untranslated.append(f'{at}: {source!r}')

if newly_untranslated:
    fail([
        f'✗ 新增漏译 {len(newly_untranslated)} 条（不在基线里）：字面量查不到 catalog 映射，'
        '所有语言下都会回显英文',
        *[f'  {entry}' for entry in newly_untranslated],
        '',
        f'修法：在 src/locale_catalog.rs 加映射 + 在 21 个 locales/*/opencadstudio.ftl 加译文；',
        f'（{SCOPE_NOTE}）',
    ])

print(f'✓ test_locales: {len(lookup)} 条源串映射 · {len(keys)} 个 Fluent 属性 · {cataloged + len(known_untranslated) + without_letters} 处 t!/tf! 字面量')
print(f'  ├ 已入 catalog：{cataloged} 处')
print(f'  ├ 无字母（符号/纯占位符，不译）：{without_letters} 处')
print(f'  └ 已知未译基线（上游并入，{len(baseline)} 条）：{len(known_untranslated)} 处命中')
print('     ★ 新增漏译：0 条 ⇒ 绿。基线只许缩小，出处见 scripts/locales_untranslated_baseline.tsv')
if baseline_stale:
    print(f'  ⚠ 基线里这 {len(baseline_stale)} 条已经不再被 src/ 里的字面量命中'
          '（已入 catalog 且代码改用映射，或那段代码已被上游改写）⇒ 请从基线删除，'
          '否则它们将来退化（映射被删）时会被基线吸收、漏译不会被拦住：')
    for source in sorted(baseline_stale):
        print(f'      - {source!r}  ← {baseline[source]}')