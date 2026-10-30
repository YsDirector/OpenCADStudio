# 子代理复核结果回收（2026-10-23）

## 0. 来源与可信度

两个只读复核子代理在产出最终报告前因 **Codex usage limit** 终止，未写回任何文件。以下内容从其会话 JSONL 的 reasoning 轨迹中回收：

- 核心几何复核：`~/.pi/agent/sessions/--home-ysdirector--/2026-10-23T11-31-44-101Z_01a16887-f965-7629-bd86-002ef93b2b37.jsonl`
  （47 个 thinking 块，484,422 字符；最后一次工具调用超时后终止）
- Writer/安全复核：`~/.pi/agent/sessions/--home-ysdirector--/2026-10-23T11-31-44-095Z_01a16887-f95f-7629-bd86-002ccfaca65c.jsonl`
  （41 个 thinking 块，717,800 字符；`UsePreviousCut` 纠错后继续分析时终止）

**这不是子代理的正式结论**，而是其推理过程的重建。下表用状态区分：

- **已复核**：本轮按代码逐行确认（只读，未运行 FreeCAD/OCS，未改文件）。
- **待验证**：子代理推断，尚未独立验证。
- **非问题**：子代理自己推翻或确认现有行为正确。

## 0.1 修复状态（2026-10-23）

- ✅ **H7 覆盖安全**：`export_scene` 先校验 `type(overwrite) is bool`；新增 `_safe_target()`，拒绝最终符号链接与非普通文件，外部 writer 直接使用该绝对路径，不再用 `resolve()` 绕过直写防护。
- ✅ **H2 XSource 子元素**：`view_sources` 在子元素名非空时明确 `ExportError`，不再静默导出整个对象；空子元素列表仍接受。
- ✅ **H1 误填 HATCH**：`clip_sources` 改为逐 Solid 判定“是否被切”，只收集被切实体上的共面新面；保留实体的原始共面表面被排除。
- ✅ **H3 剖视继承**：读取 `HatchScale`/`HatchRotation`/`CutSurfaceDisplay`；`NoCut` 不填充，`Solid` 与 `TrimAfterCut` 明确拒绝；图案名/偏移不映射时写警告。
- ✅ **H4 方向解析**：优先调用 `getXDirection()`，零值时用 `ProjGroupItem.RotationVector`，其余无效方向明确报错，不再静默回退。
- ✅ **H5 CosmeticVertexes**：纳入防丢统计；`include_cosmetics=False` 时写入忽略警告。
- ✅ **H6 重复几何**：按几何指纹（体积/面积/包围盒/面中心）去重，Body+内部特征或重合相同零件只导出一次。
- ✅ **H8 外部 writer 输出**：报告以 UTF-8 字节写入 stdout，输出移入错误边界；ASCII stdio 下仍返回结构化 JSON。
- 新增回归：非 bool `overwrite`、符号链接/悬空链接、XSource 子元素、compound 混合剖切、NoCut、ProjGroup 方向、重复几何、ASCII stdout、CosmeticVertexes。
- 验证：Python **121 项通过**（25 项真实内核用例在纯 Python 下跳过）；真实 FreeCAD 内核 **33 项**；`run_tests.py --examples --native-baseline --ocs-executable …` 全链通过，OCS **14/14** codec 往返。
- **未改**：第 2 节待验证项（HATCH 环方向、接触实体 HLR、Placement conic 等）。

## 1. 已复核的问题

### H1. 同一来源内含“完全保留实体 + 被切实体”时，误把保留实体的共面表面当剖面填充（高）✅ 已修复

`ocsm_export/section.py:49–72`

`removed` 与 `any_cut` 按 **SourceShape** 统计；`faces` 却遍历该 source 的**全部** `cut.Faces`。当单个对象是一个 compound/多实体（导入 STEP、布尔结果、多 Solid Part）且其中实体 A 完全在保留侧、实体 B 被切时：

- `removed > 0` ⇒ `any_cut = True`；
- A 位于剖切平面上的原始平面（如顶面）满足 `alignment≈1`、`distance≈0`，被加入 `faces`；
- 结果 A 被 HATCH，视觉上像“实心”或把孔/腔底填实。

现有 `test_retained_but_uncut_part_is_not_hatched` 用**两个独立 source**，因此测不到这个组合。

复现（FreeCADCmd，合成几何）：

```python
A = Part.makeBox(10, 10, 10, App.Vector(0, 0, 0))     # 全在 z<=10
B = Part.makeBox(10, 10, 10, App.Vector(20, 0, 5))    # 跨 z=10
S = Part.makeCompound([A, B])
opts = ExportOptions(views=(STANDARD_VIEWS["top"],), mode="section",
                     section=SectionSpec((0, 0, 10), (0, 0, 1)))
scene = build_scene([S], opts)
# 期望仅 B 的截面 1 个 hatch；当前会得到 2 个（含 A 的 z=10 顶面）
```

建议：按 `cut` 相对 `source.shape` 的**新生成面**识别（面索引/来源面判定），或逐 Solid 统计 `removed` 并只接受真正被切的 Solid 上的共面面；同时补混合 compound 用例。

### H2. `XSource` 的子元素名被丢弃，选择范围被静默放大（高）✅ 已修复

`ocsm_export/sources.py:26–30`

```python
for item in getattr(view, "XSource", []) or []:
    result.append(item[0] if isinstance(item, (tuple, list)) else item)
```

`PropertyXLinkSubList` 的元素形如 `(object, ["Face1", ...])`，这里只取对象、丢掉子元素名。随后 `collect_sources` 导出**整个对象**。若视图/链接确实携带子元素限定（例如只投影某个面），导出会包含完整实体的全部边，可能产生多余可见/隐藏线与剖面填充，属于“范围扩大”。

建议：检测到非空 subnames 时明确 `ExportError`（说明尚未实现子元素投影），或按子元素构造形状；不要静默降级。

### H3. 从 TechDraw 剖视继承时忽略填充/裁剪设置（高）✅ 已修复（图案名/偏移改为警告）

`ocsm_export/api.py:45` 只读取 `SectionOrigin`、`SectionNormal`，`SectionSpec` 使用默认 `hatch_angle_deg=45`、`hatch_spacing=3.175`；本机探针确认以下属性存在但从未读取：

- `HatchScale`、`HatchRotation`、`HatchOffset`、`NameGeomPattern`/`FileGeomPattern`（图案与间距/角度/相位）
- `CutSurfaceDisplay`（可为 `NoCut`/`Solid` 等）
- `TrimAfterCut`（native 会裁剪剖切结果；本工具始终使用完整半空间）

后果：用户看到“从视图继承”，实际填充图案/比例/角度与原视图不一致；`NoCut` 视图仍被填充；`TrimAfterCut=True` 的几何范围不同。属误导性继承，应读取或明确拒绝。

`UsePreviousCut` / `FuseBeforeCut` 已正确检查（子代理一度误报，后自查推翻），**不是问题**。

### H4. 投影组子视图的上方向未按 FreeCAD 规则解析（高/中）✅ 已修复

`ocsm_export/api.py:25–32`

仅读原始 `XDirection`；为零或非法时回退 `(0,0,1)`/`(0,1,0)`。而 `DrawProjGroupItem` 在 `XDirection=0` 时使用 `RotationVector`；`DrawViewSection` 还会从 `BaseView`/`SectionDirection` 解析。由于 `sources.py` 明确接受 `TechDraw::DrawProjGroupItem`，这会导致子视图绕法向旋转 90°/镜像或方向错误，且非法非垂直 `XDirection` 被静默替换而非报错。

建议：调用 FreeCAD 自身的 `getXDirection()`（存在时），否则拒绝；不要把“回退”当成有效继承。

### H5. `CosmeticVertexes` 未纳入防丢检查（中）✅ 已修复

`ocsm_export/cosmetics.py:10–11` 只统计 `CosmeticEdges`、`CenterLines`、`GeomFormats`。本机 `DrawViewPart` 属性表含 `CosmeticVertexes`；这些中心标记/修饰点会被静默忽略，即使 `include_cosmetics=True`。与“默认防丢”的承诺不一致。

建议：加入计数；默认拒绝，显式关闭时写警告。

### H6. 同时选中 Body 与其内部特征会重复导出（中）✅ 已修复

`sources.py:90–118`

Body 被当作普通 `Part.getShape` 对象（只取 Tip），但 `seen` 只按对象去重；若用户同时选中 Body 和其中的 Pad/Pocket，二者形状相同却都进入 `result`：重复 HLR、重复隐藏线、剖视重复填充。文档声称“Body 只导出最终 Tip”且“解析一次”。

建议：按 `(文档, Tip 对象)` 或形状哈希去重；至少警告重复形状。

### H7. 外部 writer 路径绕过符号链接与 `overwrite` 类型校验（高，安全）✅ 已修复

`ocsm_export/api.py:111,137,147`

- `Path(path).expanduser().resolve()` 会解析最终符号链接，替换链接目标而不是拒绝；直写路径（`dxf_writer`）拒绝符号链接，两条路径行为不一致。若目标是指向用户图纸的链接，外部写出会覆盖真实文件。
- `overwrite` 未在 `export_scene` 入口校验；外部 writer 只做 `if overwrite:`。`overwrite=1` 视为允许，`overwrite="false"` 也视为允许并覆盖。直写 writer 会拒绝非 bool。

建议：入口统一 `type(overwrite) is bool`；外部路径复用同一 `target_path`/符号链接检查（不要 `resolve` 最终项）。

### H8. 外部 writer 的 Unicode 输出错误未被 JSON 化（低/中）✅ 已修复

`ocsm_export/writer_cli.py:30`

`print(payload)` 在 `try` 之外；当 stdout 编码为 ASCII（Windows 重定向、`PYTHONIOENCODING=ascii`）时抛 `UnicodeEncodeError`，父进程只得到 traceback 文本，而不是结构化错误。建议把输出与 `sys.stdout.reconfigure(encoding="utf-8")` 纳入同一错误边界。

## 2. 待验证（仅子代理推断，未独立确认）

- `section.py` 生成环的 `ccw`/连续性依赖 `wire.OrderedEdges` 与 OCC 方向；若方向不一致，HATCH 可能填孔或空洞。现有 `test_annular...` 未断言 `ccw`。
- `projection.py` 对**接触/共面**实体用单一 compound 做 HLR，接触边界可能被融合；分离实体遮挡已有测试，接触件未测。
- `curves.py` 若 `Curve.Placement` 未烘焙，带 Placement 的圆/椭圆/样条可能错位；现有测试只查半径/长度。
- `writer_cli`/`write_dxf` 对长路径、Windows `os.replace` 同步、只读目录等平台差异未覆盖。
- `api.build_scene` 的 metadata 在剖切前统计 `solids/faces/edges`，与最终保留形状不一致（审计数字可能误导）。
- `view_sources` 不读取 TechDraw 的排除/包含对象列表（若该版本存在此类属性），可能扩大范围。
- 预览 SVG 对超大坐标/零尺寸已处理，但极多实体的性能与内存未测。

## 3. 子代理确认的非问题（避免重复排查）

- `UsePreviousCut` 属性名正确；`FuseBeforeCut` 检查正确。
- 纸面 `Scale` 被有意忽略（模型 1:1）；透视被拒绝。
- `projectVI/HI` 等参线未被当成 ISO；默认不输出接缝/平滑边是显式选项。
- 分离实体的全局遮挡实现正确（已有测试）；开放几何/线框被拒绝。
- 标准视向、旋转、状态检查、无文档修改等已有测试覆盖。

## 4. 测试缺口（子代理指出）

- `test_inherited_view_options...` 只比较线长，未断言**方向/旋转/镜像**，也未覆盖 `DrawProjGroupItem`、`CosmeticVertexes`、填充设置。
- `test_annular_section...` 未断言内外环 `ccw` 与孔内不填充（writer 侧有独立测试，但核心生成的环方向未验证）。
- `test_retained_but_uncut_part...` 用独立 source，未覆盖 H1 的混合 compound。
- 缺少 Body+Tip 同选、XSource 子元素、`TrimAfterCut`、`CutSurfaceDisplay=NoCut` 的拒绝用例。
- 缺少外部 writer 的符号链接/非 bool 覆盖/ASCII stdout 用例。

## 5. 建议处理顺序

（H7、H2 已完成；以下为剩余项）

1. H1（错误填充）→ H3/H4（继承正确性）→ H5/H6 → H8。
2. 每一项先补失败测试，再改实现；改完重跑 `run_tests.py --examples --native-baseline --ocs-executable …`。
3. 待验证项在真实 FreeCAD GUI/OCS 验收阶段逐项确认，不要仅凭静态推断修改几何算法。
