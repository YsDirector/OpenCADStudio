# FreeCAD → OCS/OCSM 分层二维 DXF 导出

在 FreeCAD 中选择三维模型或既有 TechDraw 视图，导出**可编辑、模型空间 1:1（mm）**的二维工程视图 DXF，
并直接落到 OCSM 的 1—5 工程图层。不修改源文档，不依赖 FreeCAD MCP，不需要 OCS 正在运行。

状态：**离线开发与独立进程验证完成**。FreeCAD GUI 面板、SVG 预览、第一角布局、每视图模式与LINE清理已手测；OCS 直接打开已手测。OCS 保存重开/宿主块插入属宿主功能，未计入本工具验收。

## 1. 输出约定

| 图层 | 线型 | 线宽 | 内容 |
|---|---|---:|---|
| `1轮廓实线层` | Continuous | 0.35 | 可见工程边、孔口、台阶、剖切后的可见边界 |
| `2细线层` | Continuous | 0.18 | 明确标记的细线/修饰线语义 |
| `3中心线层` | CENTER2 | 0.18 | 明确存在的中心线语义（见限制） |
| `4虚线层` | DASHED2 | 0.18 | 隐藏轮廓（连续几何 + 线型，不切成短线段） |
| `5剖面线层` | Continuous | 0.18 | 剖面原生 HATCH（含内孔/岛） |

- 颜色、线型、线宽全部 **ByLayer**，块内实体同样如此，不靠 INSERT 属性。
- DXF R2013（AC1027）、UTF-8、`$INSUNITS=4`（mm）、模型空间 1:1；纸面比例不进入导出。
- 优先保留 LINE / CIRCLE / ARC / ELLIPSE / SPLINE；degree-1 非有理样条会规范为 LINE，
  相接且共线的 LINE 自动合并；默认**拒绝**隐式离散。
- 多视图使用不带缩放的块，按**第一角或第三角**布局（仅平移，20 mm 间隙）；单视图不建块。
- 同时写出 `name.manifest.json` 审计清单：来源、设置、统计、placements、警告、DXF SHA256。
  有冲突保护、暂存、读回审计、协作锁和失败回滚；**双文件不是掉电级原子提交**，消费方应以清单哈希判断成功。
目标若是指向其他文件的符号链接，直写与外部写出进程都会拒绝；`overwrite` 必须是真正的布尔值。

## 2. 每视图模式与图样布局

每个勾选的视图可**独立**选择输出模式：

1. **纯实线**：全部可见工程边（不只外轮廓剪影）。
2. **实线 + 隐藏虚线**：可见边与隐藏边分别独立分类，重合处可见优先，不重复成两条。
3. **平面全剖 / 全剖 + 隐藏虚线**：逐来源实体用同一有限半空间裁切，识别切面外环/内孔/独立区域，
   轮廓进 1 层、填充进 5 层；不会把同一轮廓同时放进两层。复杂剖面（阶梯/旋转/局部/半剖）与
   螺栓/肋板/螺纹简化规则不在本版范围。

同一个导出可以混合，例如主视图纯实线、俯视图全剖；剖切平面参数对所有全剖视图共用。
多视图布局二选一：**第一角（GB：俯视图在主视图下方、左视图在右侧）** 或
**第三角（俯视图在上方、右视图在右侧）**；中心对齐、20 mm 间隙，ISO/自定义视图排在最右。

视向：前/后/左/右/上/下、ISO（正交轴测）、任意方向 + 上方向 + 平面旋转，也可读取当前相机方向。
所有选中实体共同参与一次 HLR，因此装配/接触件相互遮挡正确；同时保留零件身份用于剖切。

## 3. 安装与使用

仅登记工作台（符号链接，非破坏；已存在同名插件目录会拒绝覆盖）：

```bash
cd /home/ysdirector/dev/OpenCADStudio/tools/freecad_ocsm
python3 install.py --mod-dir ~/.local/share/FreeCAD/v1-1/Mod --dry-run   # 先看目标
python3 install.py --mod-dir ~/.local/share/FreeCAD/v1-1/Mod
```

重启 FreeCAD 后：`OCSM Export` 工作台 → **导出选中对象…**，或宏 `OCSMExport.FCMacro`。
面板为独立 modeless 窗口（不占 Control 面板），流程：选择对象 → 视向/每视图模式/图样布局/剖切参数 → **预览（纯 SVG）** → **导出 DXF…**。
导出总是重新计算几何，不复用预览快照；覆盖前单独确认；目标目录不存在直接报错，不自动创建。
剖视可用“取选区中心（只读）”把剖切原点放在选区包围盒中心。

依赖：DXF writer 需要 `ezdxf`。FreeCAD 内嵌 Python 通常没有它，可用任一方式：

```bash
# A) 给 FreeCAD 的 Python 安装（自行决定，工具不会自动安装）
# B) 在工具目录建 .venv（自动发现）
python3 -m venv .venv && .venv/bin/pip install 'ezdxf>=1.4,<2'
# C) 显式指定解释器（环境变量或面板“离线写出 Python”字段）
export OCSM_EXPORT_PYTHON=/path/to/python-with-ezdxf
```

缺少依赖时明确报错，不联网、不安装。外部写出进程通过临时 JSON（语义中间表示）传递，几何仍在 FreeCAD 内计算。

## 4. Python API

```python
from ocsm_export.api import build_scene, export_scene, export_objects, options_from_techdraw
from ocsm_export.model import ExportOptions, SectionSpec, ViewSpec, STANDARD_VIEWS

# 最简：选中对象 → 单视图 DXF
export_objects(FreeCADGui.Selection.getSelection(), "/tmp/part.dxf")

# 多视图：每视图独立模式 + 第一角布局
from dataclasses import replace
views = (replace(STANDARD_VIEWS["front"], mode="visible"),
         replace(STANDARD_VIEWS["top"], mode="section"),
         replace(STANDARD_VIEWS["left"], mode="hidden"))
options = ExportOptions(views=views, layout="first_angle",
                        section=SectionSpec((0, 0, 50), (0, 0, 1)))
export_objects(objects, "/tmp/part-3views.dxf", options)

# 继承既有平面剖视视图（忽略纸面比例）
options = options_from_techdraw(section_view)
scene = build_scene([section_view], options)          # 纯几何，不写文件
export_scene(scene, "/tmp/section.dxf", overwrite=False)
```

中间表示 `schema_version=1`：`views[{name, entities, hatches, metadata}]` + 全局 `metadata/warnings`；
`preview_points` 只供预览，writer 忽略。视向 `direction` 指向观察者；默认剖切保留 negative 侧，
观察方向须满足 `direction·normal > 0`。详见 `ocsm_export/model.py` 模块文档。

## 5. 支持与明确不支持

**支持**：六个正投影、ISO、任意视向/上方向/旋转；可见/隐藏/相切/接缝独立开关；Body 最终 Tip、
容器与 `App::Link` 实例及祖先 Placement；完全重合的相同几何只导出一次；多视图第一角/第三角布局；
每视图独立的纯实线/隐藏线/全剖模式；平面全剖（含斜切椭圆边界）；
继承既有普通视图/投影组子视图/平面剖视的视向、旋转、隐藏分类、`HatchScale`/`HatchRotation` 与 `NoCut`（忽略纸面缩放）。

**明确拒绝而不是静默降级**：
- 透视视图、`UsePreviousCut` 剖视、`FuseBeforeCut=True`（会丢零件边界）、`TrimAfterCut=True`。
- 视图的 `XSource` 限定到具体子元素（面/边）时明确拒绝：本版只投影整个对象，不静默扩大范围。
- 剖视 `CutSurfaceDisplay=Solid`（实心填充）明确拒绝；`NoCut` 则只输出截面轮廓、不写 HATCH。
- 剖切面不相交、开放几何、混合游离面线、剖切面与视向平行、从错误保留侧观察。
- 默认遇到不支持的曲线类型报错；只有显式 `allow_approximation=True` 才离散并记录误差警告。
- 检测到 TechDraw `CosmeticEdges` / `CosmeticVertexes` / `CenterLines` / `GeomFormats` 时默认报错
  （本版尚未实现注释纸面坐标映射）；显式 `include_cosmetics=False` 才忽略并写警告。
  **因此不能宣称已有中心线/修饰线完整导出。**

**不支持**：自动尺寸与公差、图框/明细表/BOM、双向关联更新、复杂剖视、国标简化画法推断、中心线自动生成。

## 6. 验证现状（可复现）

```bash
cd /home/ysdirector/dev/OpenCADStudio/tools/freecad_ocsm
python3 run_tests.py                 # 纯 Python 测试（不需 FreeCAD）
python3 run_tests.py --examples --native-baseline
# 可选：用已有 OCS 二进制做隔离的无界面 codec 保存/重开对比（不启动 GUI/MCP、不加载插件）
python3 run_tests.py --ocs-executable ../../target/release/OpenCADStudio
```

本机已通过（FreeCAD 1.1.3 / OCCT 7.9.3 / Python 3.14.7 / OCS 2026.42，2026-10）：
- Python 测试 125 项（98 执行 + 27 真实内核用例在纯 Python 下跳过）。
- 真实 FreeCAD 内核测试 **35 项**：七视向、隐藏分类、Body Tip、Link/Placement、全局遮挡、
  坐标/旋转、样条/圆弧方向、剖视外环/内孔/多区域、斜切椭圆、错误剖切拒绝、无文档变更。
- Writer/预览/GUI 参数纯测试（含中文层名、ByLayer、完整线型表、原生 HATCH 带孔、块内成员、覆盖与锁）。
- 样件 14 份 DXF：`artifacts/examples/`（七个正投影 + ISO、隐藏线、斜视向、三种剖视、相互遮挡）。
- 原生 TechDraw 对照：视向/上方向/旋转/比例读取正确，重建坐标与原生边一致；
  **原生 DXF 把该视图全部 11 条边放在单一图层 `View`，剖视 DXF 只有 4 个圆、没有 HATCH**——即本工具存在的理由。
- OCS 2026.42 无界面 codec 往返：14 份样件全部通过——层/线型/线宽、ByLayer、实体标签级几何、
  HATCH 路径与图案向量均一致，审计 0 错误、`dropped_on_save=0`，保存后重开统计一致。
  过程中发现并记录 OCS 侧两个**非几何**元数据差异（不修图）：保存时新增 owner 为 #0 的 TABLESTYLE
  （ezdxf 判为需修复）、省略的 SPLINE 拟合公差被写成 0。脚本对这两项显式白名单并写入报告，
  其余任何实体修复或几何差异都会失败。

**未验证**：OCS 保存后重开、宿主块/外部参照插入后的图层与 HATCH 保持（这些属 OCS 自身功能，本工具只保证 DXF 内容）；
实际视图截图核对、保存重开的人工视觉确认、大装配性能。上述未完成前不得视为首期交付验收通过。

## 7. 已知风险

- 几何剖切 ≠ 机械制图规范；剖视方向、肋板、螺栓/轴纵剖、螺纹/齿轮简化工件需后续规则或人工控制。
- 剖切面与零件相切、零厚度壁、共面接触等退化情形可能被拒绝或产生极窄区域，需人工判断。
- `manifest` 与 DXF 双文件在进程被杀/掉电时可能残留未提交组合，需按报告检查哈希。
- OCS 宿主侧的两个元数据差异是**宿主行为**，本工具不修改 OCS；升级后需重跑校验脚本。
