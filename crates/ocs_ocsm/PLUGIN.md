# OCSMechanical（OCSM）插件

Open CAD Studio 机械工具包插件（`opencad.ocsm`，API v5）。为机械工程师提供：

1. **`OCSM` 初始化**：幂等创建 9 个机械制图图层（从 `opencad.layers_quick` 迁移）
   + 防御性线型 + 文字样式 `OCSM_GB`（固定高度 3.5、宽度因子 0.7、注释性）
   + **标注样式 `OCSM_GB`**（参数与用户示例 Mechanical 对齐：dimtxt/dimasz 2.5、
   dimexe 2.0、dimexo 0.625、dimcen 2.5、dimtad=1 上方、dimtoh=0 文字随尺寸线旋转、
   dimtofl 开、dimdec=2（尺寸小数位）、dimtdec=3（偏差小数位）、dimclrd/clre=130、
  dimclrt=3 绿字、dimtxsty=OCSM_GB），
   并设为当前标注样式。
2. **数字键 `1`~`9` 快速切层**（从 `opencad.layers_quick` 迁移；`0` 不注册）：
   - 无选中：切换当前图层（不记 undo）；
   - 有选中：把对象移到目标层（记 undo、保留选择集；锁定层对象被宿主拒绝）。
3. **`TF` / `OCSMFRAMEINIT` 图框插入**：
   - 从插件安装目录 `frame/` 子文件夹扫描 DWG，弹出宿主侧选择窗口；
   - 填写比例 `值1:值2`（两个正整数，**其中一个必须为 1**）；
   - 确定后点击指定插入点，以**块**形式插入；插入比例 = `值2/值1`
     （`1:2` → 2 倍；`2:1` → 0.5 倍）；
   - 块内所有 ATTDEF 带出为属性，tag `比例`（或 `SCALE`，大小写不敏感）填入
     `值1:值2` 文本；
   - **插入时同步创建缩放标注样式** `OCSM_GB_x{scale}`（如 2 倍 → `OCSM_GB_x2`），
     POWERDIM 在 frame 内标注时自动使用（文字/箭头随图框缩放）。
4. **`D` / `OCSMPOWERDIM` 智能标注**：
   - **两种拾取模式，按 Enter 切换**（提示栏会说明当前模式）：
     - **拾取点模式**（默认，对象捕捉开）：点端点/圆心/交点/空白 → 两点标注；
       也支持直接点选直线（未吸附时）→ 线性/对齐/角度。
     - **线段点选模式**（Enter 切换，对象捕捉关，光标只留拾取方框）：点击直线/
       圆/圆弧 → 立即进入对应标注（动态预览跟随光标）。
   - 点选直线 → 线性/对齐；再点另一条不平行直线 → 角度；圆 → 直径/半径；
     圆弧 → 半径/弧长。
   - 关键字：`A` 对齐 / `H` 水平 / `V` 竖直；`D` 直径 / `R` 半径；`R` 半径 /
     `A` 弧长；`I` 劣角 / `R` 优角。
   - 放置时任意点可点击（靠近对象捕捉点才附着）；半径/直径/角度/弧长标注的
     文字跟随点击位置（光标在圆/弧内侧→内标注，外侧→外标注+引出线）。
   - 弧长按圆弧实际扫角（含优弧 >180°）；角度按两线实际夹角（劣角默认，
     优角 = 360°−劣角）。
   - 光标跟随预览（宿主瞬态渲染插件返回的标注实体）；Esc 取消；放置一个标注后命令结束。
   - 标注固定放 **`7标注层`**、样式 `OCSM_GB`（或在 frame 内时为 `OCSM_GB_x{scale}`）。
   - 文字样式 dimtoh=0：标注文字沿尺寸线方向旋转（对齐/竖直标注文字随尺寸线）。

## 构建与安装

```bash
# 1. 宿主（API v5）——runner 是宿主自身二进制，必须重编
cargo build --release

# 2. 插件
cargo build --release -p ocs_ocsm

# 3. 安装（插件目录 + frame 文件夹）
PLUGIN_DIR="$HOME/.config/OpenCADStudio/plugins/opencad.ocsm"
mkdir -p "$PLUGIN_DIR/frame"
cp target/release/libocs_ocsm.so "$PLUGIN_DIR/"
# 宿主 v2026.36+ 对 API≥4 插件增加 rustc 门禁（Rust 无稳定 ABI）：plugin.toml
# 必须携带构建该 .so 的 `rustc --version` 原串，否则插件被拒绝加载。
sed "s|__RUSTC_VERSION__|$(rustc --version)|" crates/ocs_ocsm/plugin.toml > "$PLUGIN_DIR/plugin.toml"
# 把图框 DWG 拷进 frame/（已有样例在 ~/桌面/OCSM/frame/）
cp ~/桌面/OCSM/frame/*.dwg "$PLUGIN_DIR/frame/" || true

# 4. 卸载旧 layers_quick（宿主升 v5 后它会重新加载并与 OCSM 数字键冲突）
rm -rf "$HOME/.config/OpenCADStudio/plugins/opencad.layers_quick"
```

启动 OpenCADStudio，日志应出现：
`Loaded plugin: OCSMechanical 机械工具包 (opencad.ocsm 0.2.0)`

若启动日志显示 `Plugin built with ..., host requires ... - rebuild required`，
说明 plugin.toml 的 `rustc_version` 与实际构建工具链不一致（升级过 rustc？）——
重新执行上面第 2、3 步即可（sed 会刷新工具链串）。

## 使用

- 输入 `OCSM` → 初始化图层/线型/文字样式/标注样式（幂等，可重复执行）。
- 按数字键 `1`~`9`：无选中切当前图层；有选中移动对象到该层。
- 输入 `TF`（或 `OCSMFRAMEINIT`）→ 选择图框 + 填比例 → 点击插入。
- 输入 `D`（或 `OCSMPOWERDIM`）→ 智能标注：拾取点/直线/圆/圆弧 → 自动推断类型。
- 输入 `CC`（或 `OCSMRGH`）→ 表面粗糙度：点选插入点 → GUI 选 20 形态之
  一（4 基础体 × 5 附加区）+ 填属性文字（转 ATTDEF，可缺省空白）→ 应用插入。
  基础体：C1 通用 / C2 以不去除材料的方法获得（此时 P 强制空白）/ C3 去除材料 /
  C4 焊后加工；附加区：R1 基础 / R2 周边相同处理 / R3 高级 /
  R4 上限开关 / R5 上限开关+周边相同处理。注意 `CC` 原为 COPYCLIP
  别名，插件命令优先级更高，会接管（COPYCLIP 可输全名）。
- 工具栏「标注」组有 **标注转GB**（🔁）按钮，等效输入 `D2G` / `OCSMDIM2GB`。
- 输入 `D2G`（或 `OCSMDIM2GB`）→ **把宿主原生标注重建为 OCSM 的 GB 标准版**：
  有选中只转选中集，无选中扫全图（模型空间）。产物 = OCSM_GB_x{图框比例}
  标注样式 + `7标注层`（线性/对齐用原生 `DIMENSION`；直径/半径/角度/弧长用
  匿名块 + MTEXT，与引导标注同构）。**尽量保留用户所见**：文字覆盖、小数位
  （DIMDEC）、样式前后缀（DIMPOST）、极限偏差（DIMTOL）、手动拖过的文字位置。
  支持六类：线性（含旋转）/对齐/角度（两线、三点）/半径/直径/弧长；
  **公差（极限偏差）一并带过来**：取值优先级与宿主一致——实体 XDATA `DSTYLE`
  覆盖（DIMTOL/DIMTP/DIMTM/DIMTDEC）优先于样式表，渲染成 OCSM 既有的堆叠画法
  `\H0.71x;\C2;\S+0.1^-0.14;`（线性进 dimtext、直径/半径/弧长进块内 MTEXT）；
  **偏差文本自动抹尾零，且量化后为 0 时输出纯 `0`**（不再出现 `-0.00`/`+0.00`）；


  坐标、折弯半径等本期跳过，结束时打印「转换 N 个 / 跳过 M 个（原因）」，
  整批一次 Ctrl+Z 可全撤。自动幂等补 OCSM_GB 样式、图层与文字样式，
  未跑过 `OCSM` 初始化的图纸也能直接用。
- 选中引导线后输入 `OCSMDIMGULIDE`（或 `GDIM`）→ 浏览器标注配置 GUI：
  按引导几何出选项卡（线性/直径/半径/基准/向视图/角度/剖切/形位公差/
  弧长/局部放大）。选中**恰好两段 PLINE（3 顶点：焊缝点→拐点→水平右端）**
  时另有**焊接**选项卡：上/下侧焊缝符号（24 镜像对 + 3 跨线单置，
  GB/T 324，1:1 复刻 焊接符号表.dxf）+ 四开关（虚线=非箭头侧第二基准线、
  全周边圆、现场焊接旗、尾部；**现场旗在 ⊏ 同侧时整旗抬到其上方避让**）+
  **打磨方式**（6 态：不打磨/弧·凹/弧·凸/
  直线/双弧/锯齿，1:1 复刻 焊接符号-焊缝打磨.dxf；角焊用倾斜右上版，
  其它焊缝用正上方版，下侧对称镜像，无符号侧不画）+ **焊接方法字母**
  （C/G/H/M/R/U/无标注；仅角焊缝与喇叭形焊缝可用=角焊/喇叭形焊/单边喇叭形焊，
  位置随打磨方式、下侧镜像）+ 五文字槽（含**虚线/识别线规则**：虚线表示所指
  位置的另一侧，仅当引线下方填入内容时出现——一般画在基准线下、另一侧符号
  镜像在其下；若引线上方什么都没填而下侧有内容，则下侧内容显示到上方且虚线
  画在基准线上方）+ 五文字槽
  （上/下厚度尺寸、上/下数量长度、尾部注释）→ 生成匿名块 `*W{n}` +
  INSERT@拐点（8符号标注层；
  引线/箭头/基准线青4、虚线品红6 线型 ACISOWELD、符号几何 31、文字绿3，
  骨架尺寸 1:1 复刻 焊接符号示例.dxf ×图幅倍率）。引导 PLINE 保留
  （10引导线层不打印），可重选再改。尾部关=基准线保留全长，仅去尾叉注释。
- 基准线（引导第二段）**支持四个方向**（右/上/左/下）：斜线按主分量吸附到
  水平或竖直；内容按"基准线坐标系"渲染且永远保持**可读朝向**——横基准线→
  虚线在下、竖基准线→虚线在右；**尺寸文字书写方向随基准线轴向**（横=0°、竖=+90° 自下而上，永不倒置；焊接方法字母与符号内小字同规则）。补充元素：**半包围 ⊏**
  （区别于全周边圆的"半包围结构焊缝区域"，与全周边圆互斥、括号优先，
  自拐点沿基准线 1.0 起、高 3.5、臂长 4.2）。**打磨与焊接方法按上/下侧独立
  控制**（GUI 四个下拉：上侧打磨/上侧方法/下侧打磨/下侧方法；方法可用性按侧
  独立禁用；URL 分侧键 `wgru`/`wgrl`、`wmu`/`wml`，两侧相同时用紧凑键
  `wgr`/`wm`）。
- **焊接（OCSWELD）已收官**（2026-09-13，测试 135 全绿）：11 种标注类型（线性/直径/
  半径/基准/向视图/角度/剖切/形位公差/局部放大/弧长/焊接）全部交付。尚未实现：
  增强标注编辑（OCSMDIMEDIT）、明细表（BOM）、序号球标、可配置命令映射、
  插件侧光标样式 API。
- 环境变量 `OCSM_FRAME_DIR` 可覆盖图框文件夹（测试/排障用）。

## 图层模板（9 层）

| 数字 | 图层 | 颜色 | 线型 | 线宽 |
|------|------|------|------|------|
| 1 | 轮廓实线层 | 白 7 | Continuous | 0.35mm |
| 2 | 细线层 | 青 4 | Continuous | 0.18mm |
| 3 | 中心线层 | 红 1 | CENTER2 | 0.18mm |
| 4 | 虚线层 | 洋红 6 | DASHED2 | 0.18mm |
| 5 | 剖面线层 | 黄 2 | Continuous | 0.18mm |
| 6 | 文字层 | 绿 3 | Continuous | 0.18mm |
| 7 | 标注层 | 青 4 | Continuous | 0.18mm |
| 8 | 符号标注层 | RGB 255,191,127 | Continuous | 0.18mm |
| 9 | 双点划线层 | 洋红 6 | DIVIDE2 | 0.18mm |

## 宿主 API v5 扩展（本插件引入）

`HostApi` trait 末尾追加（vtable 前缀兼容，V2–V4 旧插件继续加载）：

| 方法 | 说明 |
|------|------|
| `selected_handles() -> Vec<Handle>` | 当前选择集句柄 |
| `set_current_layer(name) -> bool` | 切当前图层（镜像 LAYMCUR 四镜像 + 面板刷新） |
| `ensure_layers(Vec<LayerDef>) -> usize` | 幂等建层（大小写不敏感，分配真实 handle） |
| `ensure_linetypes(Vec<LinetypeDef>) -> usize` | 幂等建线型 |
| `ensure_text_styles(Vec<TextStyleDef>) -> usize` | 幂等建文字样式 |
| `show_frame_picker(Vec<FrameItem>) -> bool` | 打开 OCSM 图框选择模态框 |
| `take_pending_frame_selection() -> Option<FrameSelection>` | 取走用户选择 |
| `import_frame_block(ImportFrameBlockRequest) -> Result<Vec<AttributeDefinition>, String>` | 加载图框 DWG → 合并表 → 定义块 → 返回 ATTDEF |
| `ensure_dim_styles(Vec<DimStyleDef>) -> usize` | 幂等建标注样式（以当前样式为模板覆盖字段；`make_current` 切换当前样式） |

宿主侧还新增：`ModalKind::OcsmFramePicker` + 5 个 `Message` 变体 + 模态视图（图框列表 +
比例输入 + 校验）；确定后宿主把选择存入 `ocsm_pending_frame_selection` 并执行
`OCSMFRAMEINSERT` 合成命令回传插件。

### 插件交互命令增强（API v5，本插件引入）

`InteractiveCommand` trait 末尾追加（默认实现，旧插件无感）：

| 方法 | 说明 |
|------|------|
| `wants_text_input() -> bool` + `on_text_input(&str) -> CommandStep` | 关键字/文本输入（POWERDIM 的 A/H/V/I/R/D）；`CommandStep::Ignored` 表示未消费，宿主按常规解释（如实体拾取步骤的句柄读取） |
| `wants_mouse_move() -> bool` + `on_mouse_move(pt) -> Option<EntityType>` | 光标跟随预览：插件返回将提交的实体，宿主瞬态渲染 |
| `on_object_pick_snapped(handle, pt, snapped) -> CommandStep` | 实体拾取 + OSNAP 感知：`snapped=true` 表示 pt 是吸附点（拾取点），否则是对象选择点击 |
| `entity_pick_applies_osnap() -> bool` | 实体拾取点击是否跑对象捕捉（POWERDIM 拾取点/线段点选模式切换） |

宿主侧配套：`CadCommand` 新增 `plugin_preview_entity` / `entity_pick_applies_osnap` /
`inject_pick_snap`；`InteractiveEvent` 增 `snapped` 字段与 `Text`/`MouseMove` 变体；
`HostRequest` 增 `WantsTextInput`/`WantsMouseMove`；`HostResponse` 增 `Preview`。
另外：**插件注册的命令优先级高于命令别名**——OCSM 的 `D` 覆盖内置 `D → *DIMSTYLE`
别名（project.md 要求）。

## 测试

```bash
cargo test -p ocs_ocsm                  # 插件：模板/比例/目录解析
cargo test -p ocs_plugin_api            # API：IPC round-trip 等（串行 --test-threads=1）
# 宿主单测（lib）: cargo test --lib 相关模块
```

## 已知限制（v0.2.0）

- 图框 DWG 含**嵌套块**时不支持（报错提示）。
- `Zhuque Fangsong` 字体未随插件分发（系统未装则渲染回退，样式记录保留）。
- 宿主 STYLE 写出器不持久化 `true_type_font`（TTF 字体名）与 `annotative` 标志到
  DWG/DXF——OCSM_GB 的 `height/width/font` 正常落盘，字体名与注释性仅存在于
  当前文档内存中（宿主内置 STYLE 命令创建的样式同样如此）。
- 一次 `TF` 插入一个图框；Esc 取消时块定义保留（与 AutoCAD 一致）。
- POWERDIM 放置一个标注后命令结束（不做连续标注）；以点击为主，实体拾取步骤不支持
  键入坐标（宿主 entity-pick 步骤语义）。
- 缩放标注样式 `OCSM_GB_x{scale}` 由 TF 插入时创建；旧图（样式缺失）回退 `OCSM_GB`
  并在提示中说明（请重新插入图框或运行 OCSM）。
- 有 ribbon 页（图框/标注两组）。
