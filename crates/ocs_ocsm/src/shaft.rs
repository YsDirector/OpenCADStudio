//! **轴生成器**：行 DSL / JSON → 单视图侧视图（`OCSMSHAFT`）。
//!
//! 本文件只做三件事：**解析**、**段拼接几何**、**落层/放置**。
//! 不标尺寸、不打剖面线、不建块；尚未支持的复杂特征（键槽/中心孔）会明确
//! 报「不识别的关键字」并列出当前支持集，不静默忽略。
//!
//! ## 行 DSL（一行一段，从左到右拼接）
//!
//! ```text
//! # 注释行；空行忽略；大小写不敏感；段内关键字顺序无关
//! S30 E30 L45 CH2@L
//! S40 E40 L30 CH2@R OV3
//! S50 E30 L20
//! S30 E30 L15 CH2@R
//! S40 E40 L7 M1.5
//! S36 E36 L5
//! GEAR M3 Z20
//! SPLINE 6x23x26x6 L30
//! INVOLSPLINE GB30R M3 Z20 L30
//! ```
//!
//! - `S` 起始直径（靠左）、`E` 终点直径（省略 = 圆柱段，`E=S`）、`L` 段长（必给）；
//! - `CH2@L` / `CH2@R`：端面倒角 C2，贴该段左/右端（`@` 省略默认右端）；
//! - `OV` / `OV3` / `OV3@L`：磨外圆砂轮越程槽（GB/T 6403.5），不写值 = 按该端
//!   直径查表；`@` 省略默认右端；
//! - `M` / `M1.5`：**螺纹段标记**（外螺纹侧视图）。不写值 = 简化画法小径 0.85d；
//!   `M1.5` = 给螺距 P，小径 = d − 1.0825P（精确）；只能标在圆柱段（S=E）上，
//!   与 `OV`/`GEAR` 同段报错、可与 `CH` 同段；
//! - `M1.5 TL20`：**局部螺纹**（GB/T 3-1997 图 1 第一种形式）——完整螺纹长 20，
//!   靠**段右端台肩**，自右向左布置：台肩面（既有端面线）→ 锥面过渡（长 a−x）
//!   → 螺尾（细实线斜线，长 x）→ 分界竖线 → 完整螺纹（长 TL）；
//!   `RO一般|RO短` = 收尾 x 档（默认一般）、`SD一般|SD长|SD短` = 肩距 a 档（默认一般）；
//!   不给 `TL` = 整段全螺纹（旧行为，保持兼容）；
//! - `M1.5 TL20 RL`：用 GB/T 3-1997 表 2 的**退刀槽**收尾（图 2 画法）：
//!   台肩面 → R 圆角 → 槽底 dg/2 长 g1 → 30° 斜壁（g2 处接大径）；
//!   `RL` 与 `RO`/`SD` 互斥（给了 `RL` 就不画收尾/肩距）；
//! - `RL@L` / `RL@R`：**段级退刀槽**（任意圆柱段，贴段左/右端，`@` 省略默认右端；
//!   与 `CH`/`OV` 一样同端只能一个）。画法与上一条同一剖面（GB/T 3 表 2）。
//!   取参优先级：① 段内显式 `g1/g2/dg/r`（`dg` 是**绝对直径**，
//!   例 `RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8`）；② 段内给 `P`（例 `RL@L P1.5`）
//!   → 查表 2（可叠加覆盖）；③ 都不给 → 报错（表 2 以螺距为键，不猜）。
//!   前置：该端相邻段更高（有台肩可退）、本段圆柱；否则报「第 N 段」+ 原因；
//! - `ES5*3` 这类旧写法会报错并指路（退刀槽现在用 `RL` 或小直径轴段表示）；
//! - `GEAR M5 Z10 H20`（可 `ALPHA25`）：**齿轮段（直齿）**，分度圆 d = m·z 由参数导出、**不给
//!   S/E**；H = 齿宽（省略 = 10m）；`ALPHA` = 基准齿形角（度，省略 20°，与 OCSMGEAR 同口径，
//!   影响 db/齿厚）；按齿轮工具 `side_view()` 的轴向投影口径：
//!   画齿顶轮廓（端面倒角 C = round(0.6m) 按**单侧规则**，见下）+ 分度线（`3中心线层`
//!   点划线），**常规视图不画齿根线**；剖视另画齿根线（齿部按不剖）；
//!   齿形用 `OCSMGEAR` 单独出，这里不画齿、本期不做斜齿（`BETA…` 报「斜齿未实现」）；
//! - `VIEW 常规|剖视|双`：视图开关（默认 `常规`）；独立一行或段内关键字都认
//!   （`VIEW 剖视` / `VIEW=section`），只影响整体视图（`双` = 常规+剖视并排一次出）；
//! - 多段可用 `|` 或换行分隔；行尾可跟放置参数 `at x,y rot 度`；
//! - 解析错误报「第 N 行（第 k 段）：…」，几何错误报「第 N 段：…」。
//!
//! ## 螺纹段画法口径（外螺纹侧视图，GB 简化画法）
//!
//! 大径 = 段直径（`1轮廓实线层`，即该段上下轮廓，已有）；小径上下各一条
//! **细实线**（`2细线层`，半径 = 小径/2）贯穿该段；螺纹终止线 = 该段末端竖线
//! （复用既有端面线，不重复画），小径细实线止于终止线；同段端面若有倒角、且
//! 倒角切得比小径还深，细实线改止于倒角斜线交点（不挑出材料外）。
//! `M` 段必须是圆柱（S==E），锥面螺纹报错。
//!
//! ## 齿轮段画法口径（侧视图，与 `gear.rs::side_view()` 的轴向投影一致）
//!
//! 齿顶轮廓（= `1轮廓实线层`，即该段轮廓）的端面倒角
//! **C = round(0.6m)**（`gear.rs::GearParams::chamfer()`）**按单侧规则**：该侧
//! 是自由端或邻段外轮廓更小（台阶向下）才倒；邻段更大（肩部）或齐平（端面齐平、
//! 无外角）不倒 —— 轴上的齿轮/花键段不照搬独立齿轮生成器的两端都倒。
//! 端面可见高 = ra − C，倒角斜线只贴该侧端面；倒角终点（台阶）竖线**只在常规
//! 视图**画（半高 = ra，与 `side_view()` 的台阶线同）。另画分度线 r = d/2
//! （`3中心线层`，点划线，不受倒角影响）。**常规视图不画齿根线** —— 与齿轮
//! 工具 `side_view()` 一样；**剖视**按 `section_view()` 口径加齿根线
//! ra→rf（`1轮廓实线层`，齿部按不剖，也是剖面线边界）。派生尺寸取 `gear.rs`
//! 同口径（ha*=1、c*=0.25、Xn=0 → ra = da/2、rf = df/2），不自己另立公式。
//! `ALPHA` = 基准齿形角（度，省略 20°）：按 `gear.rs` 口径进 `GearParams`，影响基圆 db/齿厚 st
//! 等派生值（齿廓用 OCSMGEAR 单独出）；本侧视图只画齿顶/齿根轮廓，半径不由 α 决定。
//! 齿轮段与相邻段的过渡按台阶处理（不做过渡圆角）；**相邻段轮廓半径 > ra
//! 会盖住齿顶线**，报「第 N 段」；≤ ra 一律放行（不再受齿根圆 rf 限制）。
//!
//! ## 花键段画法口径（`SPLINE`，与 `spline.rs` / 模板 `矩形花键.dxf` 同源）
//!
//! `SPLINE 6x23x26x6 L30 [de 71]`：规格代号自带 N/d/D/B（GB/T 1144-2001），
//! `de` 查 GB/T 10952-2005 表 1/表 2（也可覆盖）。
//! - 段长 = **L（满齿段长）+ l（收尾）**（`l = √(h(2R−h))`，R = de/2、h = (D−d)/2；
//!   6×23×26×6 → l = 9.6047）；`s = e = D`，不给 S/E；
//! - 大径线 `y=±D/2` 在 x=L 断开两段（满齿段 + 收尾段，与模板 [43]/[46] 同）；
//! - 小径细线 `y=±d/2`（`2细线层`）从段左端面到 x=L；**收尾弧** R = de/2，
//!   圆心 `(L, ±(d/2+R))`，与小径相切、与大径相交（模板交角 17.75°），
//!   弧上/下两根细竖线在 x=L 与 x=L+l（`2细线层`）；
//! - **不自动画引入倒角**：在相邻段写 `CH`（倒角贴花键凸角；模板的 φ22→φ26 C2
//!   就是这种写法，允许 C 恰好吃满台阶）；花键段本身不能与 CH/OV/RL/M/GEAR 同段；
//! - 剖视：小径线 / 收尾弧改 `1轮廓实线层`，剖面线按**轴线↔小径**两条带
//!   （小径线 → 收尾弧 → 段右端面闭合；齿部按不剖，与独立要素同口径）。
//!
//! ## 渐开线花键段画法口径（`INVOLSPLINE`，与 `invol_spline.rs` 同源）
//!
//! `INVOLSPLINE GB30R M3 Z20 [X0.2] L30 [de63]`（GB）/
//! `INVOLSPLINE DIN30 DB40 M2 L30`（DIN，`DB` = 基准直径 `d_B`，`M`/`Z` 可缺一项由
//! DIN 5480-2 名义表补全；`x=(d_B−m(z+1.1))/(2m)` 为表反推关系）/ `INVOLSPLINE NFP A80 M3.75 L30`
//! （NF E22-141，`A` = 公称直径主参数，`M`/`Z` 可缺一项由 NF 尺寸表补全）：预设代号 `GB30P`/`GB30R`（默认）
//! /`GB375R`/`GB45R`/`DIN30`/`NFP`/`NFR`（GB/T 3478.1-2008；DIN 5480-1:2015，h_fP*=0.55；NF E22-141 α=20°）自带
//! α/ha*/hf*/ρf*，直径由 `M/Z/X`（或 DB/A）导出（**不给 S/E**）：`s = e = da`；
//! **体系也可显式写标识**（`INVOLSPLINE DIN M2 Z18`、`INVOLSPLINE NF A80 M3.75`；NF 已入库，
//! ANSI 用 `P5/10`（A/B 成对：A=径节 P、B=Ps=2A；`DP5` 与齿轮侧 `DP8` 同义，x 不允许））。
//! - `L` = 有效长度（满齿段长）；`de`（滚刀外径）**可选**：给了才画收尾弧，
//!   段长 = L + l（`l = √(h(2R−h))`、R = de/2、h = (da−df)/2，与 `SPLINE` 同式）；
//!   不给 de 时段长 = L、端面直接收口；
//! - 大径线 `y=±da/2`（有 de 时在 x=L 断开两段）；小径细线 `y=±df/2`（`2细线层`）
//!   从段左端面到 x=L；剖视把小径线/收尾弧改 `1轮廓实线层`，剖面线按轴线↔小径两条带；
//! - **端面自动倒角** C = round(0.6m)（花键体系没有单独的端面倒角数据，沿用齿轮口径）
//!   按**单侧规则**加在大径与端面交角：自由端 / 邻段更小侧才倒；邻段更大/齐平侧不倒。
//!   给了 `de` 时右端是滚刀收尾（收尾弧占住端面），不叠加端面倒角；
//! - 相位/齿形由 `invol_spline.rs` 负责（端视图真实渐开线，轴侧视只表达轮廓）；
//! - 不能与 `SPLINE`/CH/OV/RL/M/GEAR 同段（引入倒角由相邻段的 CH 表达）。
//!
//! ## 几何口径（单视图侧视图）
//!
//! 轴线 = x 轴（x 向右、y = 半径），第 1 段左端面在 x=0；上下对称：
//! - 每段上下各一条线：圆柱 = 水平线，圆锥 = 斜线；
//! - **常规视图**里每条段边界都画一条贯通轴线的竖线（`1轮廓实线层`），包括直径
//!   没有变化的分界；半高 = `min(左段端面实际半径, 右段端面实际半径)`（用户
//!   2026-09-18 更正版口径）；端面有槽时取槽肩（圆角切点）高；**剖视图不画**；
//! - 段间端面 / 两端外端面用竖直线闭合（肩面仍从 `bottom` 画到 `top`）；
//! - **倒角终点**（倒角根）在常规视图也画贯通竖线，半高 = 倒角根半径；
//! - **`OV` / `RL` 槽的起止**：常规视图里槽肩面 / 槽底终止 / 斜壁终点各画一条
//!   贯通竖线，高度规则见 `build_geometry` 内的注释；剖视只画真实槽体；
//! - **倒角贴端面凸角**：相邻段更大 → 倒角落在相邻（大）段一侧；相邻段更小或
//!   本段是自由端 → 倒角落在本段一侧。这样 `S40 E40 L30 CH2@R OV3`（右端有
//!   越程槽 + 相邻 φ50 台阶）里倒角落在 φ50 上，两个特征不重叠——这是本期的
//!   定案口径（待用户复核）。
//! - `OV` 复用 `detail.rs` 的查表与画法（R 圆角/槽底/45° 斜坡），**不画砂轮细线**
//!   （用户 2026-09-18 定案：那条 `2细线层` 青线属于标注，本图不该有；`detail.rs`
//!   的独立「磨外圆」要素同口径也不画，见 `detail.rs` 顶部画法说明）；
//! - 落层：轮廓/端面/倒角/槽/边界竖线 → `1轮廓实线层`；**螺纹小径/螺尾
//!   细实线** → `2细线层`（螺纹线是几何线，保留；本轴无螺纹时自然为 0 条）；
//!   轴线与分度线 → `3中心线层`（点划线，轴线长度 = 总长 +
//!   图框比例 × 6，两端各半）；`剖视` 的剖面线 → `5剖面线层`。
//!
//! ## 视图口径（`VIEW`）
//!
//! * `常规`（默认）= 只画外形可见线：真实几何 + **贯通竖线**（段边界 / 倒角终点 /
//!   槽界线 / 齿轮台阶线），无剖面线；
//! * `剖视` = **只画真实几何 + 剖面线**（不再画贯通竖线；用户 2026-09-18 定案）：
//!   段间端面（环形面）、倒角斜线、槽的真实壁面、齿轮齿根线；ANSI31、比例 1.0，
//!   落 `5剖面线层`，走 `partgen_kit::hatch_ansi31_rings` 通路；边界 = 上半边界
//!   按 x 排序后拆成**上/下两个环**（参考件 `轴剖视图.dxf` 右视图：2 环 21+21 边）；
//!   普通全螺纹段按**小径包络**、齿轮段按**齿根圆**取剖面线边界（牙顶/齿部不剖）；
//! * `双` = 常规视图与剖视图并排一次生成（读取顺序：左常规、右剖视），
//!   间距 = `max(总长 × 15%, 40 × 图框比例)`。
//!
//! ## JSON（同一模型，给 GUI/HTTP：`/api/shaft_parse` / `/api/shaft_preview` / `/api/shaft_export`）
//!
//! ```json
//! {"segments":[{"s":30,"e":30,"l":45,"ch":[{"c":2,"end":"L"}]},{"s":30,"e":30,"l":20,"thread":1.5}],"view":"section","at":[100,50],"rot":30}
//! ```

use ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge;
use ocs_plugin_api::host::acadrust::entities::{EntityType, Hatch};
use ocs_plugin_api::host::acadrust::types::{Vector2, Vector3};
use serde::{Deserialize, Serialize};

use crate::detail;
use crate::gear::{GearKind, GearParams};
use crate::partgen_kit::{
    arc, line, trim, HatchEdge, LAYER_CENTER, LAYER_HATCH, LAYER_MAIN, LAYER_THIN,
};

/// `OCSMSHAFT` 不带参数时打开轴生成器窗口；命令行带参数时此处是用法说明。
pub const USAGE: &str = "\
OCSMSHAFT 轴生成器：行 DSL / JSON → 单视图侧视图（段拼接 + 端面倒角 + 砂轮越程槽 + 螺纹段 M + 齿轮段 GEAR + 花键段 SPLINE/INVOLSPLINE）。
用法：OCSMSHAFT <行 DSL 或 JSON>
  行 DSL：一行一段，从左到右拼接；多段用 | 或换行分隔；大小写不敏感、段内关键字顺序无关
    S 起始直径（靠左）   E 终点直径（省略 = 圆柱段 E=S）   L 段长（必给；齿轮段用 H 代替）
    CH2@L / CH2@R   端面倒角 C2（@ 省略默认 R）    OV / OV3 / OV3@L   砂轮越程槽
    M / M1.5   螺纹段：不写值 = 小径 0.85d；M1.5 = 螺距 P，小径 = d − 1.0825P（只能圆柱段）
               不给 TL/RL = 整段全螺纹（旧行为）
    M1.5 TL20  局部螺纹（GB/T 3-1997 图 1 第一种形式）：完整螺纹长 TL，靠段右端台肩，
               自右向左 = 台肩面 + 锥面（a−x）+ 螺尾（细实线，x）+ 分界竖线 + 完整螺纹 TL
               RO一般|RO短 收尾档（默认一般）   SD一般|SD长|SD短 肩距档（默认一般）
    M1.5 TL20 RL  用 GB/T 3-1997 表 2 退刀槽收尾（图 2 画法，与 RO/SD 互斥）
    RL@L / RL@R   段级退刀槽（任意圆柱段，@ 省略默认 R；同端只能一个 CH/OV/RL）
               取参：① g1/g2/dg/r 全给（dg 是绝对直径）② 给 P 查表 2 ③ 都不给报错
               例：S25 E25 L32 RL@L P1.5 / RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8
               前置：该端相邻段更高（有台肩）、本段圆柱，否则报「第 N 段」
    GEAR M5 Z10 H20   齿轮段（直齿）：d=m·z 导出、不给 S/E；H = 齿宽（省略 = 10m）
                       ALPHA25 = 压力角 25°（省略 20°）；齿顶轮廓两端倒角 C=round(0.6m)
                       常规不画齿根、剖视画齿根
    SPLINE 6x23x26x6 L30   矩形花键段（GB/T 1144 规格代号）：大径线 + 小径细线
                      （2细线层）+ 收尾弧 R=de/2（圆心 (L, ±(d/2+R))，末端 x=L+l，
                      l=√(h(2R−h))，6×23×26×6 → l=9.6047）；段长 = L + l；
                      不给 S/E；可 `de 71` 覆盖滚刀外径；不能与 CH/OV/RL/M/GEAR 同段
                      （引入倒角由相邻段的 CH 表达）
    INVOLSPLINE GB30R M3 Z20 L30   渐开线花键段（GB/T 3478.1 / DIN 5480 / NF E22-141 / ANSI B92.1）：预设代号
                      GB30P/GB30R（默认）/GB375R/GB45R/DIN30/NFP/NFR/ANSI30P/ANSI30PM/ANSI30R/ANSI375R/ANSI45R，
                      或体系标识 GB/DIN/NF/ANSI；直径由 M/Z/X 或 DB/A 导出（不给 S/E）；
                      `X0.2` = 变位（DIN ∈ [−0.05, 0.45]）；DIN 可写 `DB40`（基准直径）、
                      NF 可写 `A80`（公称直径主参数），M/Z 可缺一项由名义/尺寸表补全
                      （DIN：m=1.5/m=5 均已补入；NF：尺寸表 288 行）；
                      ANSI 是径节制：`P5/10`（A/B 成对，A=P、B=Ps=2P；也收 `DP5`，
                      与齿轮侧 `DP8` 同义），x 不允许；
                      `de63` 可选（给了才画收尾弧，段长 = L + l；不给 de 段长 = L）；
                      不能与 SPLINE/CH/OV/RL/M/GEAR 同段
    VIEW 常规|剖视|双   视图：常规（默认，只看外形）/ 剖视（轮廓 + ANSI31 剖面线）/ 双（并排一次出）
    REPORT          计算书：段末加 `REPORT`（大小写不敏感）—— 不插图，直接输出 Markdown
                    计算书（含 INVOLSPLINE 段的公式/代入数值/结果/依据来源 + DIN 检验尺寸）；
                    `REPORT=<路径>` / `REPORT-OUT=<路径>` 另写文件
    at x,y rot 度   放置（不写 = 原点、不转）
  例：OCSMSHAFT S30 E30 L45 CH2@L | S40 E40 L30 CH2@R OV3 | S50 E30 L20 | S30 E30 L15 CH2@R | S40 E40 L7 M1.5 | S36 E36 L5 | GEAR M3 Z20 VIEW 剖视 at 100,50 rot 30
  JSON：{\"segments\":[{\"s\":30,\"e\":30,\"l\":45,\"ch\":[{\"c\":2,\"end\":\"L\"}]},{\"s\":30,\"e\":30,\"l\":20,\"thread\":1.5}],\"view\":\"section\",\"at\":[100,50],\"rot\":30}
轮廓/端面/倒角/槽与边界竖线 → 1轮廓实线层，螺纹小径/螺尾 → 2细线层（OV 不画砂轮细线），轴线/分度线 → 3中心线层，剖视剖面线 → 5剖面线层。";

// ══════════════════════════════════════════════════════════════════════════
// 数据模型
// ══════════════════════════════════════════════════════════════════════════

/// 特征所在端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum End {
    L,
    R,
}

fn end_cn(end: End) -> &'static str {
    match end {
        End::L => "左",
        End::R => "右",
    }
}

/// 视图开关（用户 2026-09-18 定案）：`常规` / `剖视` / `双`。
///
/// * `常规` = 只画外形可见线（默认）；贯通竖线只属于常规视图；
/// * `剖视` = **真实几何**（不含贯通竖线）+ ANSI31 剖面线（`5剖面线层`，上下两环）；
/// * `双` = 常规视图与剖视图并排一次出（左常规、右剖视）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShaftView {
    Normal,
    Section,
    Both,
}

impl Default for ShaftView {
    fn default() -> Self {
        ShaftView::Normal
    }
}

impl ShaftView {
    /// 英文键（JSON/HTTP/GUI 传参、块名）。
    pub fn key(self) -> &'static str {
        match self {
            ShaftView::Normal => "normal",
            ShaftView::Section => "section",
            ShaftView::Both => "both",
        }
    }

    /// 中文名（DSL/GUI/报错提示）。
    pub fn label(self) -> &'static str {
        match self {
            ShaftView::Normal => "常规",
            ShaftView::Section => "剖视",
            ShaftView::Both => "双",
        }
    }

    /// 是否带剖面线（`剖视` / `双`）。
    pub fn has_hatch(self) -> bool {
        matches!(self, ShaftView::Section | ShaftView::Both)
    }

    pub const ALL: [ShaftView; 3] = [ShaftView::Normal, ShaftView::Section, ShaftView::Both];

    /// 解析视图名（中英文都认；`view=` / `视图` 前缀也剥掉）。
    pub fn parse(s: &str) -> Result<Self, String> {
        let t = s.trim().to_ascii_lowercase();
        let t = t
            .trim_start_matches("view")
            .trim_start_matches(['=', ':'])
            .trim_start_matches("视图")
            .trim();
        let hit = match t {
            "normal" | "regular" | "outline" | "常规" | "常规视图" | "外形" | "外观" | "不剖" => {
                Some(ShaftView::Normal)
            }
            "section" | "cut" | "剖" | "剖视" | "剖视图" | "剖面" | "剖开" => {
                Some(ShaftView::Section)
            }
            "both" | "dual" | "双" | "双视图" | "并排" | "常规+剖视" | "两个" => Some(ShaftView::Both),
            _ => None,
        };
        hit.ok_or_else(|| {
            format!(
                "视图名无法识别：`{}`。可用：normal|常规、section|剖视、both|双。",
                s.trim()
            )
        })
    }
}

/// 端面倒角 C（45°：沿轴向 C、半径方向 C）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Chamfer {
    pub c: f64,
    pub end: End,
}

/// 砂轮越程槽：`b1 = None` = 按该端直径查 GB 表；`Some` = 显式槽宽。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Overtravel {
    pub b1: Option<f64>,
    pub end: End,
}

/// **段级退刀槽**（GB/T 3-1997 表 2 画法）：贴在某圆柱段的左/右端、压在段内。
///
/// 与 `M` 段的 `RL`（螺纹收尾，收在 `Thread.relief`）不同：这是任意圆柱段的
/// 肩部特征，可带 `P`（表 2 单键）或显式 `g1/g2/dg/r`（`dg` 为绝对直径）。
/// 圆角心/槽底/斜壁坐标与 `detail.rs::relief_groove_entities` 同一剖面。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Relief {
    /// 贴在段左端（`RL@L`）还是右端（`RL@R`，`@` 省略默认）。
    pub end: End,
    /// 螺距 P（表 2 单键）；`None` + 全给 `g1/g2/dg/r` = 显式尺寸路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<f64>,
    /// 显式覆盖：槽底平段终点（含 R 圆角段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g1: Option<f64>,
    /// 显式覆盖：台肩面 → 斜壁与大径交点的总宽。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g2: Option<f64>,
    /// 显式覆盖：退刀槽**绝对直径**（表 2 的 `d−Δ`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dg: Option<f64>,
    /// 显式覆盖：槽底圆角半径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
}

impl Relief {
    /// 是否带任何尺寸覆盖（`M` 段的螺纹收尾 RL 不允许带，保持旧口径）。
    fn has_overrides(&self) -> bool {
        self.p.is_some() || self.g1.is_some() || self.g2.is_some() || self.dg.is_some() || self.r.is_some()
    }
}

/// `RO` 收尾档位（GB/T 3-1997 表 1）：一般 x≈2.5P / 短 x≈1.25P。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunoutGrade {
    Normal,
    Short,
}

/// `SD` 肩距档位（GB/T 3-1997 表 1）：一般 a≈3P / 长 a=4P / 短 a=2P。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShoulderGrade {
    Normal,
    Long,
    Short,
}

/// 螺纹段标记（外螺纹侧视图）：不写值 = 简化画法小径 0.85d；写螺距 = 精确小径。
///
/// `TL` / `RO` / `SD` / `RL` 四个局部螺纹关键字都收在这里；不给 `TL` 也不给
/// `RL` 时 = 旧口径整段全螺纹（小径细实线贯穿该段，行为不变）。
/// `RL = true` 表示贴段右端的**螺纹退刀槽收尾**（表 2 按螺距查）；段级（任意
/// 圆柱段、可带 `P`/显式尺寸、可贴左/右端）的 `RL` 在 [`Segment::relief`]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thread {
    /// 螺距 P（> 0）；`None` = 不写值（小径按 0.85d 简化画法）。
    pub pitch: Option<f64>,
    /// `TL` 完整螺纹长度（> 0）；`None` = 整段全螺纹（旧行为）。
    pub tl: Option<f64>,
    /// `RO` 收尾档位（GB/T 3 表 1；默认一般）。
    pub runout: RunoutGrade,
    /// `SD` 肩距档位（GB/T 3 表 1；默认一般）。
    pub shoulder: ShoulderGrade,
    /// `RL` = 用 GB/T 3 表 2 退刀槽收尾（与 `RO`/`SD` 互斥）。
    pub relief: bool,
}

impl Default for Thread {
    fn default() -> Self {
        Thread {
            pitch: None,
            tl: None,
            runout: RunoutGrade::Normal,
            shoulder: ShoulderGrade::Normal,
            relief: false,
        }
    }
}

impl Thread {
    /// 只有螺距（或什么都不给）的旧形态：JSON 沿 `true` / 螺距数字序列化。
    fn is_plain(&self) -> bool {
        self.tl.is_none()
            && self.runout == RunoutGrade::Normal
            && self.shoulder == ShoulderGrade::Normal
            && !self.relief
    }

    /// 小径 d1：不写值 = 0.85d；写螺距 = d − 1.0825P（GB/T 192 基本尺寸口径）。
    pub fn minor_diameter(&self, d: f64) -> f64 {
        match self.pitch {
            None => 0.85 * d,
            Some(p) => d - 1.0825 * p,
        }
    }

    /// 小径半径 = 小径/2（细实线所在半径）。
    pub fn minor_radius(&self, d: f64) -> f64 {
        self.minor_diameter(d) / 2.0
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 数据：GB/T 3-1997 表 1 / 表 2（**表归一** —— 唯一数据源在 `detail.rs`）
// ══════════════════════════════════════════════════════════════════════════
//
// 表 1（收尾 x / 肩距 a）与表 2（外螺纹退刀槽）此前在本文件与 `detail.rs`
// 各有一份，现已收敛到 `detail.rs` 一处（`RUNOUT_ROWS` / `THREAD_RELIEF_ROWS`
// + `runout_row_checked` / `thread_relief_row`）；本文件只保留两个薄封装：
// 档位选择（`x(grade)` / `a(grade)`）与「段级 RL 取参」。

impl detail::RunoutRow {
    /// `RO` 档位 → 收尾 x。
    fn x(&self, grade: RunoutGrade) -> f64 {
        match grade {
            RunoutGrade::Normal => self.x_normal,
            RunoutGrade::Short => self.x_short,
        }
    }

    /// `SD` 档位 → 肩距 a。
    fn a(&self, grade: ShoulderGrade) -> f64 {
        match grade {
            ShoulderGrade::Normal => self.a_normal,
            ShoulderGrade::Long => self.a_long,
            ShoulderGrade::Short => self.a_short,
        }
    }
}

/// 段级 `RL` 取参（优先级按任务口径，不猜）：
/// ① 段内显式覆盖 `g1/g2/dg/r`（`dg` 是**绝对直径**）；
/// ② 段内给 `P` → `detail.rs` 表 2（`g1/g2/dg/r` 可叠加覆盖）；
/// ③ 都不给 → 报错（提示表 2 以螺距为键）。
fn resolve_relief_dims(
    seg: &Segment,
    spec: &Relief,
    label: &str,
) -> Result<detail::ReliefDims, String> {
    let d = seg.s;
    if let Some(p) = spec.p {
        let mut params = detail::DetailParams::new();
        params.insert("P", p);
        for (key, value) in [("g1", spec.g1), ("g2", spec.g2), ("dg", spec.dg), ("r", spec.r)] {
            if let Some(value) = value {
                params.insert(key, value);
            }
        }
        detail::relief_dims(d, &params).map_err(|e| format!("{label}：{e}"))
    } else if let (Some(g1), Some(g2), Some(dg), Some(r)) = (spec.g1, spec.g2, spec.dg, spec.r) {
        detail::relief_dims_explicit(d, g1, g2, dg, r).map_err(|e| format!("{label}：{e}"))
    } else {
        Err(format!(
            "{label}：RL 退刀槽缺参 —— 表 2 以螺距为键，请给 P（例 `RL@L P1.5`）或显式 g1/g2/dg/r（例 `RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8`）"
        ))
    }
}

/// 左端退刀槽把螺纹小径细实线起点往右推的距离（从段左端起算）：
/// 槽底低于小径（`r1 ≤ dg/2`）→ 整段槽体 `g2`；否则取斜壁与小径线的交点。
fn relief_minor_inset(r: f64, r1: f64, dims: &detail::ReliefDims) -> f64 {
    let rg = dims.dg / 2.0;
    if r1 <= rg {
        dims.g2
    } else if r1 >= r {
        0.0
    } else {
        dims.g1 + (dims.g2 - dims.g1) * (r1 - rg) / (r - rg)
    }
}

/// 把 `thread` 序列化（给 GUI/HTTP）：
/// - 旧形态（只有螺距 / 什么都不给）保持 `true` / 螺距数字，老 GUI 不受影响；
/// - 带 `TL`/`RO`/`SD`/`RL` 时序列化成对象，保证 JSON 往返不丢新关键字。
fn serialize_thread<S: serde::Serializer>(
    thread: &Option<Thread>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match thread {
        None => serializer.serialize_none(),
        Some(t) if t.is_plain() => match t.pitch {
            Some(pitch) => serializer.serialize_f64(pitch),
            None => serializer.serialize_bool(true),
        },
        Some(t) => {
            let mut value = serde_json::Map::new();
            if let Some(pitch) = t.pitch {
                value.insert("pitch".into(), serde_json::json!(pitch));
            }
            if let Some(tl) = t.tl {
                value.insert("tl".into(), serde_json::json!(tl));
            }
            if t.runout != RunoutGrade::Normal {
                value.insert("runout".into(), serde_json::json!(t.runout));
            }
            if t.shoulder != ShoulderGrade::Normal {
                value.insert("shoulder".into(), serde_json::json!(t.shoulder));
            }
            if t.relief {
                value.insert("relief".into(), serde_json::json!(true));
            }
            serde_json::Value::Object(value).serialize(serializer)
        }
    }
}

/// 齿轮段参数（本期：**外齿轮、直齿**）。派生尺寸统一走 [`GearParams`]（gear.rs 口径）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Gear {
    /// 模数 m（> 0）。
    pub m: f64,
    /// 齿数 z（整数，2..=1000，与 gear.rs 一致）。
    pub z: u32,
    /// 齿宽 H；`None` = 10m。
    pub h: Option<f64>,
    /// 螺旋角 β（度）；本期只支持 0（斜齿 = 二期）。
    #[serde(rename = "beta")]
    pub beta_deg: f64,
    /// 基准齿形角 α（度；默认 20°，JSON/DSL 可给 `ALPHA25`）。
    /// 缺省值不写出（保持旧 JSON 与 DSL 文本不变）。
    #[serde(rename = "alpha", skip_serializing_if = "alpha_is_default")]
    pub alpha_deg: f64,
}

/// GEAR 段的默认压力角（与 `gear.rs::ALPHA_N_DEG` 同值）。
fn default_alpha_deg() -> f64 {
    crate::gear::ALPHA_N_DEG
}

fn alpha_is_default(value: &f64) -> bool {
    (*value - default_alpha_deg()).abs() < 1e-9
}

impl Gear {
    /// 齿宽（H 省略 = 10m）。
    pub fn width(&self) -> f64 {
        self.h.unwrap_or(10.0 * self.m)
    }

    /// 与 `gear.rs` 同口径的齿轮参数（外齿轮、ha*=1、c*=0.25、Xn=0、直齿）。
    pub fn params(&self) -> GearParams {
        GearParams {
            kind: GearKind::External,
            m: self.m,
            z: self.z,
            alpha_deg: self.alpha_deg,
            beta_deg: self.beta_deg,
            h: self.width(),
            ..GearParams::default()
        }
    }

    /// 分度圆半径 r = d/2（= m·z/2）。
    pub fn pitch_radius(&self) -> f64 {
        self.params().d() / 2.0
    }

    /// 齿顶圆半径 ra = da/2（外齿轮 = d/2 + m）。
    pub fn addendum_radius(&self) -> f64 {
        self.params().da() / 2.0
    }

    /// 齿根圆半径 rf = df/2（外齿轮 = d/2 − 1.25m）。
    pub fn root_radius(&self) -> f64 {
        self.params().df() / 2.0
    }

    /// 轴向倒角 C = round(0.6m)（与 `gear.rs::GearParams::chamfer()` 同口径；
    /// 小模数可能为 0 = 不倒角）。
    pub fn chamfer(&self) -> f64 {
        self.params().chamfer()
    }
}

/// 渐开线花键段（`INVOLSPLINE GB30R M3 Z20 [X0.2] L30 [de63]`）。
///
/// 几何在 `invol_spline.rs`；`de`（滚刀外径）可选：给 `de` 才画收尾弧
/// （段长 = L + l）；不给 `de` 则段长 = L、不画收尾。
#[derive(Debug, Clone)]
pub struct InvolSeg {
    /// 预设代号（`GB30R` 等；GUI 从目录表按代号取系数）。
    pub code: String,
    /// 已校验的几何参数（不序列化；GUI 用 code + m/z/x 自行算派生值）。
    pub params: crate::invol_spline::InvolParams,
    /// 满齿段长 L。
    pub len: f64,
    /// 滚刀外径 de（`None` = 不画收尾）。
    pub de: Option<f64>,
    /// `CHECK` 时的 DIN 5480-2 检验尺寸摘要（M₁/M₂/D_M/k/W_k + 来源）；
    /// `None` = 未开 CHECK（默认行为不变）。JSON 里 skip_serializing_if。
    pub inspection: Option<String>,
    /// `d_B` 补全/推导/按 d_B 重算的来源提示（DIN；`None` = 无/查表直命中）。JSON 里 skip_serializing_if。
    pub d_b_note: Option<String>,
    /// 解析时 `resolve_spline` 给出的基准直径来源（查表行/公式/推导/纠偏）；
    /// 供计算书（`OCSMSHAFT … report`）复用，不序列化。
    pub d_b_origin: Option<crate::invol_spline::D_bOrigin>,
}

impl InvolSeg {
    /// 构造并校验（`de` 给定时必须大于大径 da）。
    pub fn new(
        code: String,
        params: crate::invol_spline::InvolParams,
        len: f64,
        de: Option<f64>,
    ) -> Result<Self, String> {
        params.validate()?;
        if !(len.is_finite() && len > 0.0) {
            return Err(format!("渐开线花键：L={len} 必须是正数"));
        }
        if let Some(de) = de {
            params.runout_length(de)?;
        }
        Ok(Self { code, params, len, de, inspection: None, d_b_note: None, d_b_origin: None })
    }

    /// 收尾长度 l（无 de = 0）。
    pub fn runout(&self) -> f64 {
        self.de
            .map(|de| self.params.runout_length(de).unwrap_or(0.0))
            .unwrap_or(0.0)
    }

    /// 段长 = L + l。
    pub fn segment_len(&self) -> f64 {
        self.len + self.runout()
    }

    /// 大径半径 da/2（外轮廓）。
    pub fn major_radius(&self) -> f64 {
        self.params.da() / 2.0
    }

    /// 小径半径 df/2。
    pub fn minor_radius(&self) -> f64 {
        self.params.df() / 2.0
    }
}

impl PartialEq for InvolSeg {
    /// 只比几何/模型字段；`d_b_origin`（计算书用来源）不参与相等性，
    /// 这样 JSON 序列化→反解析（自定义 Serialize 不带该字段）仍与原值相等。
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
            && self.params == other.params
            && self.len == other.len
            && self.de == other.de
            && self.inspection == other.inspection
            && self.d_b_note == other.d_b_note
    }
}

/// `InvolSeg` 的 JSON 形状：`{code,m,z,x,len,de,d_b|a[,inspection][,d_b_note]}`（几何参数从 code+m/z/x 重算，不序列化）。
impl serde::Serialize for InvolSeg {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("InvolSeg", 11)?;
        st.serialize_field("code", &self.code)?;
        st.serialize_field("m", &self.params.m)?;
        st.serialize_field("z", &self.params.z)?;
        st.serialize_field("x", &self.params.x)?;
        st.serialize_field("pitch", &self.params.pitch)?;
        st.serialize_field("len", &self.len)?;
        st.serialize_field("de", &self.de)?;
        // DIN 的 d_B 与 NF 的 A 同一槽位：只序列化实际有值的那个，避免回传时双字段冲突。
        if self.params.d_b.is_some() {
            st.serialize_field("d_b", &self.params.d_b)?;
        }
        if self.params.a.is_some() {
            st.serialize_field("a", &self.params.a)?;
        }
        if self.d_b_note.is_some() {
            st.serialize_field("d_b_note", &self.d_b_note)?;
        }
        if self.inspection.is_some() {
            st.serialize_field("inspection", &self.inspection)?;
        }
        st.end()
    }
}

/// `INVOLSPLINE ... CHECK`：DIN 预设算检验尺寸摘要；GB 明确报错（不静默忽略）。
fn invol_check_note(invol: &InvolSeg, label: &str, keyword: &str) -> Result<String, String> {
    use crate::invol_spline::SplineStd;
    let p = &invol.params;
    if p.std != SplineStd::DIN {
        return Err(format!(
            "{label}：{keyword} CHECK：检验尺寸表（DIN 5480-2）只适用于 DIN30 预设"
        ));
    }
    let d_b = p
        .d_b
        .unwrap_or_else(|| crate::invol_spline::d_b_from_x(p.m, p.z, p.x));
    let r = crate::invol_spline::inspection_query(d_b, p.m, p.z)
        .map_err(|e| format!("{label}：{keyword} CHECK：{e}"))?;
    Ok(crate::invol_spline::inspection_summary(&r))
}

/// 一段轴（从左到右）。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Segment {
    /// 起始直径（左端）。齿轮段 = 分度圆直径（由 M·Z 导出）。
    pub s: f64,
    /// 终点直径（右端）。齿轮段 = 分度圆直径。
    pub e: f64,
    /// 段长。齿轮段 = H（省略 10m）。
    pub l: f64,
    /// 倒角（同一端最多一个）。
    pub ch: Vec<Chamfer>,
    /// 越程槽（同一端最多一个）。
    pub ov: Vec<Overtravel>,
    /// 段级退刀槽（`RL@L`/`RL@R`；同一端最多一个）。`M` 段的右端螺纹收尾
    /// 不走这里（仍以 `Thread.relief` 表示），与 `CH`/`OV` 一样按端别互斥。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relief: Vec<Relief>,
    /// 螺纹段标记（`None` = 普通轴段）。
    #[serde(serialize_with = "serialize_thread", skip_serializing_if = "Option::is_none")]
    pub thread: Option<Thread>,
    /// 齿轮段参数（`None` = 普通轴段）。
    pub gear: Option<Gear>,
    /// 矩形花键段（`SPLINE 6x23x26x6 L30`；`None` = 普通轴段）。
    ///
    /// `s`/`e` = 大径 D，`l` = 满齿段长 L + 收尾 l（见 `spline.rs` 的段长口径）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spline: Option<crate::spline::RectSpline>,
    /// 渐开线花键段（`INVOLSPLINE GB30R M3 Z20 L30`；`None` = 普通轴段）。
    ///
    /// `s`/`e` = 大径 da，`l` = 满齿段长 L + 收尾 l（有 `de` 时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invol_spline: Option<InvolSeg>,
}

impl Segment {
    /// 某端的**外轮廓半径**（齿轮段 = 齿顶圆半径；花键段 = 大径半径）。
    pub fn outer_radius(&self, end: End) -> f64 {
        if let Some(gear) = &self.gear {
            return gear.addendum_radius();
        }
        if let Some(spline) = &self.spline {
            let _ = end;
            return spline.major_radius();
        }
        if let Some(invol) = &self.invol_spline {
            let _ = end;
            return invol.major_radius();
        }
        match end {
            End::L => self.s / 2.0,
            End::R => self.e / 2.0,
        }
    }
}

/// 解析结果：段序列 + 放置参数（`at` / `rot` 由命令层应用）+ 视图开关。
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct Program {
    pub segments: Vec<Segment>,
    pub at: Option<[f64; 2]>,
    pub rot: Option<f64>,
    /// 视图（默认 `常规`）；`serde` 里序列化成 `normal`/`section`/`both`。
    #[serde(default)]
    pub view: ShaftView,
}

impl Program {
    /// 总长。
    pub fn total_length(&self) -> f64 {
        self.segments.iter().map(|s| s.l).sum()
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 解析：行 DSL
// ══════════════════════════════════════════════════════════════════════════

/// 解析行 DSL 或 JSON（首字符 `{` = JSON）。返回的几何还没做合法性检查，
/// 校验在 [`validate`] / [`build`] 里（这样测试与 GUI 能分别拿到解析层错误）。
pub fn parse_program(text: &str) -> Result<Program, String> {
    if text.trim_start().starts_with('{') {
        return parse_json(text);
    }
    let mut program = Program::default();
    let mut view_seen: Option<ShaftView> = None;
    // 全文一个「逻辑段」计数器对不上的情况：同一行用 `|` 分多段时单独标「第 k 段」。
    for (line_idx, raw_line) in text.lines().enumerate() {
        let line_no = line_idx + 1;
        // 注释：整行 `#` 或行内 `#` 之后都忽略。
        let line = raw_line.split('#').next().unwrap_or("");
        if line.trim().is_empty() {
            continue;
        }
        let chunks: Vec<&str> = line.split('|').collect();
        let multi = chunks.iter().filter(|c| !c.trim().is_empty()).count() > 1;
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            let label = if multi {
                format!("第 {line_no} 行第 {} 段", chunk_idx + 1)
            } else {
                // 单段行也带上「第 N 段」，便于用户按段定位（与几何错误口径一致）。
                format!("第 {line_no} 行（第 {} 段）", program.segments.len() + 1)
            };
            // `VIEW 剖视` / `视图 剖视` 是整体视图开关：独立一行或段内关键字都认。
            let tokens: Vec<&str> = chunk.split_whitespace().collect();
            let tokens = extract_view_directives(&tokens, &label, &mut view_seen)?;
            if tokens.is_empty() {
                continue;
            }
            if tokens[0].eq_ignore_ascii_case("at") || tokens[0] == "@" {
                parse_placement(&tokens, &label, &mut program)?;
                continue;
            }
            let seg = parse_segment(&tokens.join(" "), &label, &mut program)?;
            program.segments.push(seg);
        }
    }
    if program.segments.is_empty() {
        return Err("没有解析到任何轴段（至少给一段 `S… L…`）".into());
    }
    program.view = view_seen.unwrap_or_default();
    Ok(program)
}

/// 从一段的 token 里提取 `VIEW …` / `视图 …` 指令（独立一行或段内均认），
/// 返回去掉指令后的 token。同值重复忽略；不同值报冲突。
fn extract_view_directives<'a>(
    tokens: &[&'a str],
    label: &str,
    view: &mut Option<ShaftView>,
) -> Result<Vec<&'a str>, String> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let upper = token.to_ascii_uppercase();
        let mut name: Option<&str> = None;
        if upper == "VIEW" || token == "视图" {
            index += 1;
            name = Some(tokens.get(index).copied().ok_or_else(|| {
                format!("{label}：关键字 VIEW 缺少视图名（常规/剖视/双）")
            })?);
        } else if let Some(rest) = upper.strip_prefix("VIEW") {
            name = rest.strip_prefix(['=', ':']);
        } else if let Some(rest) = token.strip_prefix("视图") {
            name = rest.strip_prefix(['=', ':']);
        }
        if let Some(name) = name {
            let parsed = ShaftView::parse(name).map_err(|e| format!("{label}：{e}"))?;
            match *view {
                Some(prev) if prev != parsed => {
                    return Err(format!(
                        "{label}：视图 VIEW 重复且冲突（已给「{}」，又给「{}」）",
                        prev.label(),
                        parsed.label()
                    ));
                }
                _ => *view = Some(parsed),
            }
        } else {
            out.push(token);
        }
        index += 1;
    }
    Ok(out)
}

fn unknown_keyword(token: &str, label: &str) -> String {
    format!(
        "{label}：不识别的关键字「{token}」（本期支持 S/E/L/CH/OV/M/TL/RO/SD/RL/GEAR/SPLINE/INVOLSPLINE/VIEW；GEAR 子关键字 M/Z/H/BETA/ALPHA；INVOLSPLINE 子关键字 M/Z/X/DB/A/P/DP/L/de；RL 的尺寸参数 P/g1/g2/dg/r 跟在 RL 后面）"
    )
}

fn starts_number(s: &str) -> bool {
    matches!(s.chars().next(), Some(c) if c.is_ascii_digit() || c == '+' || c == '-' || c == '.')
}

/// 关键字后面是否跟了「像数值」的尾巴（空 = 缺值，另行报错；字母开头 = 未知关键字）。
fn looks_like_keyword_value(rest: &str) -> bool {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    rest.is_empty() || starts_number(rest)
}

fn parse_end(text: &str) -> Result<End, String> {
    match text.to_ascii_uppercase().as_str() {
        "L" | "LEFT" | "左" => Ok(End::L),
        "R" | "RIGHT" | "右" => Ok(End::R),
        other => Err(other.to_string()),
    }
}

/// `2@L` → `("2", Some("L"))`；`3` → `("3", None)`。
fn split_end(rest: &str) -> (&str, Option<&str>) {
    match rest.split_once('@') {
        Some((value, end)) => (value, Some(end)),
        None => (rest, None),
    }
}

fn parse_end_token(end: Option<&str>, label: &str, what: &str) -> Result<End, String> {
    match end {
        None => Ok(End::R),
        Some(text) if text.is_empty() => {
            Err(format!("{label}：关键字 {what} 的端别缺省（@ 后要 L 或 R）"))
        }
        Some(text) => parse_end(text)
            .map_err(|bad| format!("{label}：端别「{bad}」非法（只能用 L 或 R）")),
    }
}

fn parse_number(value: &str, label: &str, what: &str) -> Result<f64, String> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{label}：关键字 {what} 的值「{value}」不是数字"))
}

fn parse_diameter(token: &str, rest: &str, what: &str, label: &str) -> Result<f64, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!("{label}：关键字 {what} 缺少数值"));
    }
    if !starts_number(rest) {
        return Err(unknown_keyword(token, label));
    }
    parse_number(rest, label, what)
}

fn parse_ch(token: &str, rest: &str, label: &str) -> Result<Chamfer, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!(
            "{label}：关键字 CH 缺少数值（写法 CH2 / CH2@L / CH2@R）"
        ));
    }
    if !starts_number(rest) {
        return Err(unknown_keyword(token, label));
    }
    let (value, end) = split_end(rest);
    let c = parse_number(value, label, "CH")?;
    if c <= 0.0 {
        return Err(format!(
            "{label}：关键字 CH 的 C={} 非法（必须 > 0）",
            trim(c)
        ));
    }
    Ok(Chamfer {
        c,
        end: parse_end_token(end, label, "CH")?,
    })
}

fn parse_ov(token: &str, rest: &str, label: &str) -> Result<Overtravel, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    let (value, end) = split_end(rest);
    let b1 = if value.is_empty() {
        None
    } else {
        if !starts_number(value) {
            return Err(unknown_keyword(token, label));
        }
        let b1 = value
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("{label}：关键字 OV 的值「{value}」不是数字"))?;
        if b1 <= 0.0 {
            return Err(format!(
                "{label}：关键字 OV 的 b1={} 非法（必须 > 0）",
                trim(b1)
            ));
        }
        Some(b1)
    };
    Ok(Overtravel {
        b1,
        end: parse_end_token(end, label, "OV")?,
    })
}

/// `ES` 旧关键字已取消：退刀槽用段级 `RL`（或一小段小直径轴段）表示，报错并给写法示例。
fn es_cancelled(label: &str) -> String {
    format!(
        "{label}：`ES` 已取消 —— 退刀槽请用段级 `RL`（例 `S25 E25 L32 RL@L P1.5`）或一小段小直径轴段表示，例如 `S24 E24 L5`"
    )
}

/// `M` / `M1.5`：不写值 = 简化画法；写值 = 螺距 P（> 0）。
fn parse_thread(rest: &str, label: &str) -> Result<Thread, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Ok(Thread::default());
    }
    let pitch = parse_number(rest, label, "M")?;
    if pitch <= 0.0 {
        return Err(format!(
            "{label}：关键字 M 的螺距 P={} 非法（必须 > 0；不写值 = 小径 0.85d 简化画法）",
            trim(pitch)
        ));
    }
    Ok(Thread {
        pitch: Some(pitch),
        ..Thread::default()
    })
}

/// `RO` 收尾档位：一般 / 短（英文 normal / short 也认；`=` `:` 可省）。
fn parse_runout_grade(text: &str, label: &str) -> Result<RunoutGrade, String> {
    let value = text.strip_prefix(['=', ':']).unwrap_or(text);
    match value {
        "" | "一般" | "normal" | "普通" => Ok(RunoutGrade::Normal),
        "短" | "short" => Ok(RunoutGrade::Short),
        other => Err(format!(
            "{label}：收尾档位 RO「{other}」非法（GB/T 3 表 1 只有 一般 / 短，没有 长）"
        )),
    }
}

/// `SD` 肩距档位：一般 / 长 / 短（英文 normal / long / short 也认）。
fn parse_shoulder_grade(text: &str, label: &str) -> Result<ShoulderGrade, String> {
    let value = text.strip_prefix(['=', ':']).unwrap_or(text);
    match value {
        "" | "一般" | "normal" | "普通" => Ok(ShoulderGrade::Normal),
        "长" | "long" => Ok(ShoulderGrade::Long),
        "短" | "short" => Ok(ShoulderGrade::Short),
        other => Err(format!(
            "{label}：肩距档位 SD「{other}」非法（GB/T 3 表 1 只有 一般 / 长 / 短）"
        )),
    }
}

/// 档位关键字的尾巴是否像档位：空 / `=` `:` 起头 / 数字 / 中文 / 已知英文档位。
/// 不像（例 `rot`、`SDX`）就交回 `unknown_keyword`，不吞掉普通单词。
fn grade_tail_ok(rest: &str) -> bool {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    rest.is_empty()
        || starts_number(rest)
        || !rest.is_ascii()
        || matches!(
            rest.to_ascii_lowercase().as_str(),
            "normal" | "short" | "long" | "普通"
        )
}

/// `TL` 的数值：`TL20` / `TL=20`；空值/非数报错。
fn parse_thread_length(rest: &str, token: &str, label: &str) -> Result<f64, String> {
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!("{label}：关键字 TL 缺少数值（写法 TL20）"));
    }
    if !starts_number(rest) {
        return Err(unknown_keyword(token, label));
    }
    let value = parse_number(rest, label, "TL")?;
    if value <= 0.0 {
        return Err(format!(
            "{label}：完整螺纹长度 TL={} 非法（必须 > 0；不给 TL = 整段全螺纹）",
            trim(value)
        ));
    }
    Ok(value)
}

/// `RL` 尺寸参数键（大小写不敏感）：`P` / `g1` / `g2` / `dg` / `r`。
/// 返回（规范小写键，紧跟的附件值）：
/// `P1.5` → `("p", Some("1.5"))`、`g1 2.5` → `("g1", None)`（调用方吞下一个 token）、
/// `g1=2.5` / `g1:2.5` → `("g1", Some("2.5"))`。
fn relief_param_key(token: &str) -> Option<(&'static str, Option<&str>)> {
    let upper = token.to_ascii_uppercase();
    let (key, canonical): (&str, &'static str) = if upper.starts_with("G1") {
        ("G1", "g1")
    } else if upper.starts_with("G2") {
        ("G2", "g2")
    } else if upper.starts_with("DG") {
        ("DG", "dg")
    } else if upper.starts_with('P') {
        ("P", "p")
    } else if upper.starts_with('R') {
        ("R", "r")
    } else {
        return None;
    };
    // 键后必须空 / `=` / `:` / 数值起头；否则不是参数（例 `RO短`、`PH`）。
    let rest = &token[key.len()..];
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Some((canonical, None));
    }
    if starts_number(rest) {
        return Some((canonical, Some(rest)));
    }
    None
}

/// 把一个 `RL` 尺寸参数写进对应规格（重复报错，不静默覆盖）。
fn set_relief_param(
    spec: &mut Relief,
    key: &str,
    value: f64,
    label: &str,
) -> Result<(), String> {
    let slot = match key {
        "p" => &mut spec.p,
        "g1" => &mut spec.g1,
        "g2" => &mut spec.g2,
        "dg" => &mut spec.dg,
        "r" => &mut spec.r,
        _ => unreachable!("relief_param_key 只返回 p/g1/g2/dg/r"),
    };
    if slot.is_some() {
        return Err(format!("{label}：RL 参数 {key} 重复"));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_segment(chunk: &str, label: &str, program: &mut Program) -> Result<Segment, String> {
    let tokens: Vec<&str> = chunk.split_whitespace().collect();
    let (mut s, mut e, mut l) = (None, None, None);
    let mut ch: Vec<Chamfer> = Vec::new();
    let mut ov: Vec<Overtravel> = Vec::new();
    let mut thread: Option<Thread> = None;
    // `M` 的局部螺纹扩展关键字（用户 2026-09-19 定稿）：`TL` / `RO` / `SD` / `RL`。
    // 段内顺序无关；到段尾统一装进 [`Thread`] 并校验互斥。
    let mut tl: Option<f64> = None;
    let mut ro: Option<RunoutGrade> = None;
    let mut sd: Option<ShoulderGrade> = None;
    // 段级 RL（用户 2026-09-20 定稿）：可贴任意圆柱段左/右端；
    // `P` / `g1` / `g2` / `dg` / `r` 参数跟在某个 `RL` 后面，绑定到最近一个 RL。
    let mut relief_specs: Vec<Relief> = Vec::new();
    let mut pending_relief: Option<usize> = None;
    // GEAR 子关键字（M/Z/H/BETA）先收齐，段内顺序无关；没有 GEAR 时 M = 螺纹。
    // 先扫一遍段里有没有 GEAR：有 GEAR 时 M 一律按模数收（保持段内顺序无关）。
    let has_gear = tokens.iter().any(|t| t.eq_ignore_ascii_case("GEAR"));
    let mut gear_on = false;
    let (mut gear_m, mut gear_z, mut gear_h, mut gear_beta, mut gear_alpha) =
        (None, None, None, None, None);
    let mut loose_gear_token: Option<String> = None;
    // SPLINE 段（矩形花键）：`SPLINE <规格>` + `L<满齿段长>` + 可选 `de` 覆盖。
    let mut spline_on = false;
    let mut spline_spec: Option<String> = None;
    let mut spline_de: Option<f64> = None;
    // INVOLSPLINE 段（渐开线花键）：`INVOLSPLINE <预设代号>` + `M/Z/X` + `L` + 可选 `de`。
    // 先扫一遍段里有没有 INVOLSPLINE：有它时 M/Z 一律按花键参数收（与 GEAR 同理）。
    let has_invol = tokens.iter().any(|t| {
        let u = t.to_ascii_uppercase();
        u == "INVOLSPLINE" || u.starts_with("INVOLSPLINE=") || u.starts_with("INVOLSPLINE:")
    });
    let mut invol_on = false;
    let mut invol_spec: Option<String> = None;
    let (mut invol_m, mut invol_z, mut invol_x) = (None, None, None);
    // `P`/`DP` 写法进来的值（径节，只属 ANSI）；与 `M`（模数）不同槽位，供体系校验。
    let mut invol_pitch: Option<f64> = None;
    let mut invol_d_b: Option<f64> = None;
    let mut invol_de: Option<f64> = None;
    // `INVOLSPLINE ... CHECK`：附带 DIN 5480-2 检验尺寸（默认关，行为不变）。
    let mut invol_check = false;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        // 放置参数只允许出现在一段的末尾（`… at x,y rot 30`），吃掉剩余 token。
        if token.eq_ignore_ascii_case("at") || token == "@" {
            parse_placement(&tokens[index..], label, program)?;
            break;
        }
        let upper = token.to_ascii_uppercase();
        // ── INVOLSPLINE（渐开线花键段）：关键字 + 紧跟的预设代号；M/Z/X/L/de 照常 ──
        if upper == "INVOLSPLINE"
            || upper.starts_with("INVOLSPLINE=")
            || upper.starts_with("INVOLSPLINE:")
        {
            if invol_on {
                return Err(format!("{label}：关键字 INVOLSPLINE 重复"));
            }
            invol_on = true;
            let rest = &token["INVOLSPLINE".len()..];
            let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest).trim();
            if rest.is_empty() {
                index += 1;
                let spec = tokens.get(index).ok_or_else(|| {
                    format!("{label}：关键字 INVOLSPLINE 缺少预设代号（写法 INVOLSPLINE GB30R M3 Z20 L30）")
                })?;
                invol_spec = Some((*spec).to_string());
            } else {
                invol_spec = Some(rest.to_string());
            }
        } else if upper == "SPLINE" || upper.starts_with("SPLINE=") || upper.starts_with("SPLINE:") {
            if spline_on {
                return Err(format!("{label}：关键字 SPLINE 重复"));
            }
            spline_on = true;
            let rest = &token["SPLINE".len()..];
            let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest).trim();
            if rest.is_empty() {
                index += 1;
                let spec = tokens.get(index).ok_or_else(|| {
                    format!("{label}：关键字 SPLINE 缺少规格代号（写法 SPLINE 6x23x26x6 L30）")
                })?;
                spline_spec = Some((*spec).to_string());
            } else {
                spline_spec = Some(rest.to_string());
            }
        } else if has_invol && upper.starts_with("DB") {
            // 渐开线花键基准直径：`DB40` / `DB 40` / `DB=40`（DIN 的 d_B，生成时校验）。
            if invol_d_b.is_some() {
                return Err(format!("{label}：关键字 DB/A（基准直径）重复"));
            }
            let rest = &token[2..];
            let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
            let value_text = if rest.is_empty() {
                index += 1;
                tokens
                    .get(index)
                    .ok_or_else(|| format!("{label}：关键字 DB 缺少数值"))?
            } else {
                rest
            };
            invol_d_b = Some(parse_number(value_text, label, "DB")?);
        } else if has_invol && upper.starts_with('A') && !upper.starts_with("ALPHA") {
            // NF 公称直径 A：`A80` / `A 80` / `A=80`（与 DIN 的 DB 同一槽位，生成时按体系解释）。
            if invol_d_b.is_some() {
                return Err(format!("{label}：关键字 A/DB（基准直径）重复"));
            }
            let rest = &token[1..];
            let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
            let value_text = if rest.is_empty() {
                index += 1;
                tokens
                    .get(index)
                    .ok_or_else(|| format!("{label}：关键字 A 缺少数值"))?
            } else {
                rest
            };
            invol_d_b = Some(parse_number(value_text, label, "A")?);
        } else if has_invol && upper == "CHECK" {
            // 必须在 `upper.starts_with("CH")`（倒角）之前截住，否则 CHECK 会被当成 CH。
            invol_check = true;
        } else if upper.starts_with("DE") {
            if !spline_on && !invol_on {
                return Err(unknown_keyword(token, label));
            }
            if spline_de.is_some() || invol_de.is_some() {
                return Err(format!("{label}：花键的 de 覆盖重复"));
            }
            let rest = &token[2..];
            let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
            let value_text = if rest.is_empty() {
                index += 1;
                tokens
                    .get(index)
                    .ok_or_else(|| format!("{label}：关键字 de 缺少数值"))?
            } else {
                rest
            };
            let value = parse_number(value_text, label, "de")?;
            if spline_on {
                spline_de = Some(value);
            } else {
                invol_de = Some(value);
            }
        } else if upper.starts_with("CH") {
            let item = parse_ch(token, &token[2..], label)?;
            if ch.iter().any(|x| x.end == item.end) {
                return Err(format!(
                    "{label}：关键字 CH 在{}端重复",
                    end_cn(item.end)
                ));
            }
            ch.push(item);
        } else if upper.starts_with("OV") {
            let item = parse_ov(token, &token[2..], label)?;
            if ov.iter().any(|x| x.end == item.end) {
                return Err(format!(
                    "{label}：关键字 OV 在{}端重复",
                    end_cn(item.end)
                ));
            }
            ov.push(item);
        } else if upper.starts_with("ES") {
            // ES 已取消：给指路提示（不静默丢特征，也不当成 E+S 乱解）。
            return Err(es_cancelled(label));
        } else if upper.starts_with("TL") {
            let item = parse_thread_length(&token[2..], token, label)?;
            if tl.is_some() {
                return Err(format!("{label}：关键字 TL 重复"));
            }
            tl = Some(item);
        } else if upper.starts_with("RO") && grade_tail_ok(&token[2..]) {
            if ro.is_some() {
                return Err(format!("{label}：关键字 RO 重复"));
            }
            let rest = &token[2..];
            let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
            if starts_number(rest) {
                return Err(format!(
                    "{label}：收尾档位 RO「{rest}」非法（只有 一般 / 短）"
                ));
            }
            ro = Some(parse_runout_grade(rest, label)?);
        } else if upper.starts_with("SD") && grade_tail_ok(&token[2..]) {
            if sd.is_some() {
                return Err(format!("{label}：关键字 SD 重复"));
            }
            let rest = &token[2..];
            let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
            if starts_number(rest) {
                return Err(format!(
                    "{label}：肩距档位 SD「{rest}」非法（只有 一般 / 长 / 短）"
                ));
            }
            sd = Some(parse_shoulder_grade(rest, label)?);
        } else if upper == "RL"
            || upper.starts_with("RL@")
            || upper.starts_with("RL=")
            || upper.starts_with("RL:")
        {
            let rest = &token[2..];
            let (value, end) = split_end(rest);
            if !value.is_empty() {
                return Err(format!(
                    "{label}：关键字 RL 不带值（写法 RL / RL@L / RL@R；尺寸参数跟在 RL 后面，如 `RL@L P1.5`）"
                ));
            }
            let end = parse_end_token(end, label, "RL")?;
            if relief_specs.iter().any(|r| r.end == end) {
                return Err(format!(
                    "{label}：关键字 RL 重复（在{}端）",
                    end_cn(end)
                ));
            }
            relief_specs.push(Relief {
                end,
                p: None,
                g1: None,
                g2: None,
                dg: None,
                r: None,
            });
            pending_relief = Some(relief_specs.len() - 1);
        } else if has_invol
            && upper.starts_with('P')
            && starts_number(
                token[1..]
                    .strip_prefix(['=', ':'])
                    .unwrap_or(&token[1..]),
            )
        {
            // ANSI 径节：`P5/10`（A/B 成对，B=Ps=2A）或 `P8`（裸数字，系列在 resolve 阶段校验）。
            // **必须排在 RL 的 `P` 参数分支之前**：INVOLSPLINE 段里 P 是径节、不是退刀槽螺距；
            // 否则会被 `relief_param_key` 当 RL 参数截住报「参数 p 要跟在 RL 后面」。
            let value = parse_invol_pitch(token, 1, "P", label)?;
            if invol_m.is_some() {
                return Err(format!("{label}：关键字 P/M/DP（径节/模数）重复"));
            }
            invol_pitch = Some(value);
            invol_m = Some(value);
        } else if has_invol
            && upper.starts_with("DP")
            && starts_number(
                token[2..]
                    .strip_prefix(['=', ':'])
                    .unwrap_or(&token[2..]),
            )
        {
            // 轴段 `DP<值>`：与齿轮侧 `DP8` 同义（ANSI 径节 P 原值；`DP5/10` 也收）。
            let value = parse_invol_pitch(token, 2, "DP", label)?;
            if invol_m.is_some() {
                return Err(format!("{label}：关键字 P/M/DP（径节/模数）重复"));
            }
            invol_pitch = Some(value);
            invol_m = Some(value);
        } else if let Some((key, attached)) = relief_param_key(token) {
            // `P1.5` / `g1=2.5` / `g1 2.5`：一律绑定到最近一个 RL。
            let spec_index = pending_relief.ok_or_else(|| {
                format!(
                    "{label}：参数 {key} 要跟在 RL 后面（例 `RL@L P1.5` / `RL@L g1 2.5 …`）"
                )
            })?;
            let value_text = match attached {
                Some(rest) => rest,
                None => {
                    index += 1;
                    tokens
                        .get(index)
                        .ok_or_else(|| format!("{label}：RL 参数 {key} 缺少数值"))?
                }
            };
            let value = parse_number(value_text, label, key)?;
            if value <= 0.0 {
                return Err(format!(
                    "{label}：RL 参数 {key}={} 非法（必须 > 0）",
                    trim(value)
                ));
            }
            set_relief_param(&mut relief_specs[spec_index], key, value, label)?;
        } else if upper == "GEAR" {
            if gear_on {
                return Err(format!("{label}：关键字 GEAR 重复"));
            }
            gear_on = true;
        } else if upper.starts_with("GEAR") {
            return Err(unknown_keyword(token, label));
        } else if upper.starts_with('M') {
            if has_invol {
                let value = parse_gear_number(token, 1, "M", label)?;
                if invol_m.is_some() {
                    return Err(format!(
                        "{label}：关键字 M 重复（INVOLSPLINE 段里的 M 是模数；螺纹 M 不能与它同段）"
                    ));
                }
                invol_m = Some(value);
            } else if has_gear {
                let value = parse_gear_number(token, 1, "M", label)?;
                if gear_m.is_some() {
                    return Err(format!(
                        "{label}：关键字 M 重复（GEAR 段里的 M 是模数；螺纹 M 不能与 GEAR 同段）"
                    ));
                }
                gear_m = Some(value);
            } else {
                if thread.is_some() {
                    return Err(format!("{label}：关键字 M 重复"));
                }
                thread = Some(parse_thread(&token[1..], label)?);
            }
        } else if upper.starts_with('Z') {
            if has_invol {
                let rest = parse_gear_value(token, 1, "Z", label)?;
                let value: u32 = rest.parse().map_err(|_| {
                    format!("{label}：关键字 Z 的值「{rest}」不是正整数（齿数 z 必须是整数）")
                })?;
                if invol_z.is_some() {
                    return Err(format!("{label}：关键字 Z 重复"));
                }
                invol_z = Some(value);
            } else {
                let rest = parse_gear_value(token, 1, "Z", label)?;
                let value: u32 = rest.parse().map_err(|_| {
                    format!("{label}：关键字 Z 的值「{rest}」不是正整数（齿数 z 必须是整数）")
                })?;
                if gear_z.is_some() {
                    return Err(format!("{label}：关键字 Z 重复"));
                }
                gear_z = Some(value);
                remember_loose_gear_token(&mut loose_gear_token, token);
            }
        } else if has_invol && upper.starts_with('X') {
            let value = parse_gear_number(token, 1, "X", label)?;
            if invol_x.is_some() {
                return Err(format!("{label}：关键字 X 重复"));
            }
            invol_x = Some(value);
        } else if upper.starts_with('H') {
            let value = parse_gear_number(token, 1, "H", label)?;
            if gear_h.is_some() {
                return Err(format!("{label}：关键字 H 重复"));
            }
            gear_h = Some(value);
            remember_loose_gear_token(&mut loose_gear_token, token);
        } else if upper.starts_with("BETA") {
            let value = parse_gear_number(token, 4, "BETA", label)?;
            if gear_beta.is_some() {
                return Err(format!("{label}：关键字 BETA 重复"));
            }
            gear_beta = Some(value);
            remember_loose_gear_token(&mut loose_gear_token, token);
        } else if upper.starts_with("ALPHA") {
            let value = parse_gear_number(token, 5, "ALPHA", label)?;
            if gear_alpha.is_some() {
                return Err(format!("{label}：关键字 ALPHA 重复"));
            }
            gear_alpha = Some(value);
            remember_loose_gear_token(&mut loose_gear_token, token);
        } else if upper.starts_with('S') {
            if !looks_like_keyword_value(&token[1..]) {
                return Err(unknown_keyword(token, label));
            }
            if s.is_some() {
                return Err(format!("{label}：关键字 S 重复"));
            }
            s = Some(parse_diameter(token, &token[1..], "S", label)?);
        } else if upper.starts_with('E') {
            if !looks_like_keyword_value(&token[1..]) {
                return Err(unknown_keyword(token, label));
            }
            if e.is_some() {
                return Err(format!("{label}：关键字 E 重复"));
            }
            e = Some(parse_diameter(token, &token[1..], "E", label)?);
        } else if upper.starts_with('L') {
            if !looks_like_keyword_value(&token[1..]) {
                return Err(unknown_keyword(token, label));
            }
            if l.is_some() {
                return Err(format!("{label}：关键字 L 重复"));
            }
            l = Some(parse_diameter(token, &token[1..], "L", label)?);
        } else {
            return Err(unknown_keyword(token, label));
        }
        index += 1;
    }
    if !gear_on {
        // ── INVOLSPLINE 段（渐开线花键）：预设代号 + M/Z/X 导出直径（不给 S/E）；L 必给；
        //    与 SPLINE/CH/OV/RL/M/GEAR 互斥（引入倒角由相邻段的 CH 表达，同 SPLINE）。──
        if invol_on {
            if spline_on {
                return Err(format!("{label}：渐开线花键段不能与矩形花键 SPLINE 同段"));
            }
            if let Some(token) = loose_gear_token {
                return Err(unknown_keyword(&token, label));
            }
            if !ch.is_empty() {
                return Err(format!(
                    "{label}：渐开线花键段不能与倒角 CH 同段（不自动画引入倒角；请在相邻轴段上写 CH）"
                ));
            }
            if !ov.is_empty() {
                return Err(format!("{label}：渐开线花键段不能与越程槽 OV 同段"));
            }
            if !relief_specs.is_empty() {
                return Err(format!("{label}：渐开线花键段不能与退刀槽 RL 同段"));
            }
            if thread.is_some() || tl.is_some() || ro.is_some() || sd.is_some() {
                return Err(format!("{label}：渐开线花键段不能与螺纹段 M/TL/RO/SD 同段"));
            }
            if s.is_some() || e.is_some() {
                return Err(format!("{label}：渐开线花键段不给 S/E（直径由预设导出）"));
            }
            let spec = invol_spec.ok_or_else(|| {
                format!("{label}：关键字 INVOLSPLINE 缺少预设代号（写法 INVOLSPLINE GB30R M3 Z20 L30）")
            })?;
            let (std, profile) = crate::invol_spline::parse_preset_token(&spec).ok_or_else(|| {
                format!(
                    "{label}：不认识的体系标识/预设代号「{spec}」\
                     （可用 GB30P/GB30R/GB375R/GB45R/DIN30/NFP/NFR，或体系标识 GB/DIN/NF/ANSI）"
                )
            })?;
            let (params, origin) = crate::invol_spline::resolve_spline(
                std,
                profile,
                invol_d_b,
                if std == crate::invol_spline::SplineStd::ANSI {
                    // ANSI：P 优先；`M` 槽位在 ANSI 下也按径节 P 解释。
                    invol_pitch.or(invol_m)
                } else {
                    // 径节 P/DP 只属 ANSI；GB/DIN/NF 收到 → 明确拒绝（不把 P 当模数）。
                    if invol_pitch.is_some() {
                        return Err(format!(
                            "{label}：{} 体系：{}。",
                            std.code(),
                            crate::invol_spline::PITCH_ONLY_ANSI_MSG
                        ));
                    }
                    invol_m
                },
                invol_z,
                invol_x,
            )
            .map_err(|e| format!("{label}：{e}"))?;
            let code = crate::invol_spline::preset_code(std, profile).unwrap_or("").to_string();
            let len = l.ok_or_else(|| {
                format!("{label}：渐开线花键段缺少 L（有效长度，例 `INVOLSPLINE GB30R M3 Z20 L30`）")
            })?;
            let mut invol = InvolSeg::new(code, params, len, invol_de)
                .map_err(|e| format!("{label}：{e}"))?;
            // DIN d_B 的补全/推导/重算来源写进 JSON（GUI 派生值行用）；查表直命中不附。
            if let Some(o) = &origin {
                if !matches!(o, crate::invol_spline::D_bOrigin::Table(_)) {
                    invol.d_b_note = Some(o.note());
                }
            }
            // 计算书入口（`… report`）要复用同一次解析的来源，不重新另算。
            invol.d_b_origin = origin;
            if invol_check {
                invol.inspection = Some(invol_check_note(&invol, label, "INVOLSPLINE")?);
            }
            let d = invol.major_radius() * 2.0;
            return Ok(Segment {
                s: d,
                e: d,
                l: invol.segment_len(),
                ch: Vec::new(),
                ov: Vec::new(),
                relief: Vec::new(),
                thread: None,
                gear: None,
                spline: None,
                invol_spline: Some(invol),
            });
        }
        // ── SPLINE 段（矩形花键）：直径由规格代号导出（不给 S/E）；L 必给；
        //    与 CH/OV/RL/M/GEAR 互斥（引入倒角由相邻段的 CH 表达，见 `spline.rs`）──
        if spline_on {
            if let Some(token) = loose_gear_token {
                return Err(unknown_keyword(&token, label));
            }
            if !ch.is_empty() {
                return Err(format!(
                    "{label}：花键段不能与倒角 CH 同段（不自动画引入倒角；请在相邻轴段上写 CH）"
                ));
            }
            if !ov.is_empty() {
                return Err(format!("{label}：花键段不能与越程槽 OV 同段"));
            }
            if !relief_specs.is_empty() {
                return Err(format!("{label}：花键段不能与退刀槽 RL 同段"));
            }
            if thread.is_some() || tl.is_some() || ro.is_some() || sd.is_some() {
                return Err(format!("{label}：花键段不能与螺纹段 M/TL/RO/SD 同段"));
            }
            if s.is_some() || e.is_some() {
                return Err(format!("{label}：花键段不给 S/E（直径由规格代号导出）"));
            }
            let spec = spline_spec.ok_or_else(|| {
                format!("{label}：关键字 SPLINE 缺少规格代号（写法 SPLINE 6x23x26x6 L30）")
            })?;
            // 先校验规格代号 —— 比「缺 L」更切题（例如 `SPLINE L30` 里 L30 不是代号）。
            if let Err(e) = crate::spline::parse_code(&spec) {
                return Err(format!("{label}：{e}"));
            }
            let len = l.ok_or_else(|| {
                format!("{label}：花键段缺少 L（满齿段长，例 `SPLINE 6x23x26x6 L30`）")
            })?;
            let spline = crate::spline::RectSpline::from_code(&spec, spline_de, len)
                .map_err(|e| format!("{label}：{e}"))?;
            return Ok(Segment {
                s: spline.big,
                e: spline.big,
                l: spline.segment_len(),
                ch: Vec::new(),
                ov: Vec::new(),
                relief: Vec::new(),
                thread: None,
                gear: None,
                spline: Some(spline),
                invol_spline: None,
            });
        }
        // 没有 GEAR 的 Z/H/BETA 仍按不识别的关键字报（不静默忽略）。
        if let Some(token) = loose_gear_token {
            return Err(unknown_keyword(&token, label));
        }
        if thread.is_none() && (tl.is_some() || ro.is_some() || sd.is_some()) {
            return Err(format!(
                "{label}：关键字 TL/RO/SD 是螺纹参数，需要与 M 同段（例 `M1.5 TL20 RO短 SD长`）"
            ));
        }
        let rl_r = relief_specs.iter().any(|r| r.end == End::R);
        if let Some(t) = &mut thread {
            t.tl = tl;
            if let Some(ro) = ro {
                t.runout = ro;
            }
            if let Some(sd) = sd {
                t.shoulder = sd;
            }
            // `M` 段的 `RL`（右端）= 螺纹退刀槽收尾（旧口径，表 2 按螺距查，
            // 行为保持不变）；`RL@L` = 段级左端槽，仍走 `Segment.relief`。
            if rl_r {
                let spec = relief_specs
                    .iter()
                    .find(|r| r.end == End::R)
                    .expect("rl_r = true");
                if spec.has_overrides() {
                    return Err(format!(
                        "{label}：M 段右端的 RL（螺纹收尾）按螺距查表，不支持 P/g1/g2/dg/r 覆盖；段级退刀槽请写在别的圆柱段上（或 `M…RL@L`）"
                    ));
                }
                if t.runout != RunoutGrade::Normal || t.shoulder != ShoulderGrade::Normal {
                    return Err(format!(
                        "{label}：RL（表 2 退刀槽收尾）与 RO/SD（螺尾/肩距）互斥，二选一"
                    ));
                }
                t.relief = true;
            }
        }
        if thread.is_some() && !ov.is_empty() {
            return Err(format!("{label}：螺纹段 M 不能与越程槽 OV 同段"));
        }
        // `M` 段右端 RL 走 `Thread.relief`（不重复存进段级 relief）；其余都进段级。
        let relief: Vec<Relief> = if thread.is_some() && rl_r {
            relief_specs
                .into_iter()
                .filter(|r| r.end != End::R)
                .collect()
        } else {
            relief_specs
        };
        let s = s.ok_or_else(|| format!("{label}：缺少 S（起始直径）"))?;
        // E 省略 = 圆柱段；L 必给。
        let l = l.ok_or_else(|| format!("{label}：缺少 L（段长）"))?;
        return Ok(Segment {
            s,
            e: e.unwrap_or(s),
            l,
            ch,
            ov,
            relief,
            thread,
            gear: None,
            spline: None,
            invol_spline: None,
        });
    }
    // ── 齿轮段：直径由 M·Z 导出、长度用 H；CH/OV/M 同段冲突 ──
    if s.is_some() || e.is_some() {
        return Err(format!("{label}：齿轮段不给 S/E（直径由 M·Z 导出）"));
    }
    if l.is_some() {
        return Err(format!(
            "{label}：齿轮段长度用 H（省略 = 10m），不要再给 L"
        ));
    }
    if !ch.is_empty() {
        return Err(format!(
            "{label}：齿轮段不能与倒角 CH 同段（齿形用 OCSMGEAR 单独出）"
        ));
    }
    if !ov.is_empty() {
        return Err(format!("{label}：齿轮段不能与越程槽 OV 同段"));
    }
    if thread.is_some() {
        return Err(format!("{label}：齿轮段不能与螺纹段 M 同段"));
    }
    if tl.is_some() || ro.is_some() || sd.is_some() {
        return Err(format!(
            "{label}：齿轮段不能与螺纹参数 TL/RO/SD 同段（TL/RO/SD 只属于 M）"
        ));
    }
    if !relief_specs.is_empty() {
        return Err(format!("{label}：齿轮段不能与退刀槽 RL 同段"));
    }
    let m = gear_m.ok_or_else(|| format!("{label}：关键字 GEAR 缺少 M（模数）"))?;
    let z = gear_z.ok_or_else(|| format!("{label}：关键字 GEAR 缺少 Z（齿数）"))?;
    let beta_deg = gear_beta.unwrap_or(0.0);
    if beta_deg.abs() > 1e-9 {
        return Err(format!(
            "{label}：关键字 BETA 的斜齿（β={}°）本期只做直齿（斜齿未实现）",
            trim(beta_deg)
        ));
    }
    let gear = Gear {
        m,
        z,
        h: gear_h,
        beta_deg,
        alpha_deg: gear_alpha.unwrap_or_else(default_alpha_deg),
    };
    let params = gear.params();
    params
        .validate()
        .map_err(|e| format!("{label}：齿轮段：{e}"))?;
    let d = params.d();
    Ok(Segment {
        s: d,
        e: d,
        l: gear.width(),
        ch,
        ov,
        relief: Vec::new(),
        thread,
        gear: Some(gear),
        spline: None,
        invol_spline: None,
    })
}

/// GEAR 子关键字（M/Z/H/BETA/ALPHA）：`M=5` / `M5` 都收；缺值报错。
fn parse_gear_value<'a>(
    token: &'a str,
    prefix_len: usize,
    what: &str,
    label: &str,
) -> Result<&'a str, String> {
    let rest = &token[prefix_len..];
    let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!("{label}：关键字 {what} 缺少数值"));
    }
    Ok(rest)
}

fn parse_gear_number(token: &str, prefix_len: usize, what: &str, label: &str) -> Result<f64, String> {
    let rest = parse_gear_value(token, prefix_len, what, label)?;
    parse_number(rest, label, what)
}

/// INVOLSPLINE 的径节参数（`P5/10`（A/B，B=Ps=2A）/ `P8` / `DP5`）：
/// 语法与 `B==2A` 在此校验；**17 项系列在 `resolve_spline`/`validate` 阶段统一校验**
/// （这样 `INVOLSPLINE … RL@L P1.5` 仍先报互斥，而不是被径节系列拦截）。
fn parse_invol_pitch(
    token: &str,
    prefix_len: usize,
    what: &str,
    label: &str,
) -> Result<f64, String> {
    let rest = parse_gear_value(token, prefix_len, what, label)?;
    crate::invol_spline::parse_ansi_pitch_syntax(rest).map_err(|e| format!("{label}：{e}"))
}

/// 没有 GEAR 时记住第一个 Z/H/BETA 原文（到段尾统一报不识别的关键字/缺 GEAR）。
fn remember_loose_gear_token(slot: &mut Option<String>, token: &str) {
    if slot.is_none() {
        *slot = Some(token.to_string());
    }
}

fn is_rot(token: &str) -> bool {
    token.eq_ignore_ascii_case("rot") || token.eq_ignore_ascii_case("rotation")
}

fn number_or_err(text: &str, what: &str, label: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{label}：{what}「{text}」不是数字"))
}

fn parse_placement(tokens: &[&str], label: &str, program: &mut Program) -> Result<(), String> {
    if program.at.is_some() || program.rot.is_some() {
        return Err(format!("{label}：放置参数 at/rot 重复"));
    }
    let mut index = 0;
    if index < tokens.len() && (tokens[index].eq_ignore_ascii_case("at") || tokens[index] == "@") {
        index += 1;
    }
    if index < tokens.len() && !is_rot(tokens[index]) {
        let token = tokens[index];
        let (x, y) = if let Some((a, b)) = token.split_once(',') {
            (
                number_or_err(a, "at 坐标", label)?,
                number_or_err(b, "at 坐标", label)?,
            )
        } else {
            let b = tokens
                .get(index + 1)
                .ok_or_else(|| format!("{label}：at 缺 y 坐标（写 at x,y）"))?;
            if is_rot(b) {
                return Err(format!("{label}：at 缺 y 坐标（写 at x,y）"));
            }
            (
                number_or_err(token, "at 坐标", label)?,
                number_or_err(b, "at 坐标", label)?,
            )
        };
        program.at = Some([x, y]);
        index += if token.contains(',') { 1 } else { 2 };
    }
    if index < tokens.len() && is_rot(tokens[index]) {
        index += 1;
        let value = tokens
            .get(index)
            .ok_or_else(|| format!("{label}：rot 缺少数值"))?;
        program.rot = Some(number_or_err(value, "rot", label)?);
        index += 1;
    }
    if index < tokens.len() {
        return Err(format!(
            "{label}：无法识别的词「{}」（放置段只认 at x,y rot 度）",
            tokens[index]
        ));
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════
// 解析：JSON（同一模型）
// ══════════════════════════════════════════════════════════════════════════

/// 字段允许单对象或数组（`"ov":{"b1":3}` 与 `"ov":[{"b1":3}]` 都收）。
fn one_or_many<'de, D, T>(de: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw<T> {
        // 数组必须先试：结构体字段都有默认值时，serde 会把空数组 `[]` 当成
        // 「全默认的结构体」反序列化（`One` 会误吞空数组）——写成数组就一定是数组。
        Many(Vec<T>),
        One(T),
    }
    Ok(match Option::<Raw<T>>::deserialize(de)? {
        None => Vec::new(),
        Some(Raw::One(value)) => vec![value],
        Some(Raw::Many(values)) => values,
    })
}

#[derive(serde::Deserialize)]
struct JsonProgram {
    #[serde(default)]
    segments: Vec<JsonSegment>,
    #[serde(default, deserialize_with = "json_point")]
    at: Option<[f64; 2]>,
    #[serde(default)]
    rot: Option<f64>,
    /// 视图：`normal|section|both` 或中文名；缺省 = 常规。
    #[serde(default)]
    view: Option<String>,
}

#[derive(serde::Deserialize)]
struct JsonSegment {
    #[serde(default)]
    s: Option<f64>,
    #[serde(default)]
    e: Option<f64>,
    #[serde(default)]
    l: Option<f64>,
    #[serde(default, deserialize_with = "one_or_many")]
    ch: Vec<JsonChamfer>,
    #[serde(default, deserialize_with = "one_or_many")]
    ov: Vec<JsonOvertravel>,
    /// 段级退刀槽（`RL@L`/`RL@R`）：对象或数组；`M` 段右端收尾仍走 `thread.relief`。
    #[serde(default, deserialize_with = "one_or_many")]
    relief: Vec<JsonRelief>,
    /// 旧字段：`ES` 退刀槽已取消 → 出现就报错指路（不静默丢特征）。
    #[serde(default)]
    es: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "json_thread")]
    thread: Option<Thread>,
    #[serde(default)]
    gear: Option<JsonGear>,
    /// 矩形花键段（`SPLINE`）：序列化回传 `{spec,len,de,...}` 或直接给 N/d/D/B。
    #[serde(default)]
    spline: Option<JsonSpline>,
    /// 渐开线花键段（`INVOLSPLINE`）：`{code,m,z,x,len,de}`。
    #[serde(default)]
    invol_spline: Option<JsonInvolSpline>,
}

/// JSON 形式的渐开线花键段：`{"code":"GB30R","m":3,"z":20,"x":0.2,"len":30,"de":63}`；
/// `de` 可选（不给 = 不画收尾）；也收分开的 `std` + `profile`（拼成预设代号）。
/// DIN 可只给 `d_b` + `m`/`z`（另一项由 DIN 5480-2 名义表补全；`d_b` 别名 `db`/`d_B`）；
/// 给了 `d_b` 时 `m`/`z` 可缺一项。
#[derive(serde::Deserialize)]
struct JsonInvolSpline {
    #[serde(default)]
    code: Option<String>,
    #[serde(default, alias = "preset")]
    preset_code: Option<String>,
    #[serde(default)]
    std: Option<String>,
    #[serde(default)]
    profile: Option<String>,
    /// 模数 m（DIN 给 `d_b` 时可由查表补全；ANSI 时此槽位 = 径节 P，也可用 `pitch` 显式给）。
    #[serde(default)]
    m: Option<f64>,
    /// ANSI 径节 P（显式字段；与 `m` 槽位同义，优先）。
    #[serde(default)]
    pitch: Option<f64>,
    /// 齿数 z（DIN 给 `d_b` 时可由查表补全）。
    #[serde(default)]
    z: Option<u32>,
    #[serde(default)]
    x: Option<f64>,
    /// 基准直径主参数：DIN 的 `d_B`（别名 `db`/`d_B`）或 NF 的公称直径 `A`（别名 `a`）。
    #[serde(default, alias = "db", alias = "d_B", alias = "a")]
    d_b: Option<f64>,
    /// 满齿段长 L（别名 `l`/`L`）。
    #[serde(default, alias = "l", alias = "L")]
    len: Option<f64>,
    /// 滚刀外径 de（可选）。
    #[serde(default)]
    de: Option<f64>,
    /// `CHECK`：附带 DIN 5480-2 检验尺寸（M₁/M₂/D_M/k/W_k）；默认关。
    #[serde(default)]
    check: Option<bool>,
}

/// JSON 形式的花键段：`{"spec":"6x23x26x6","len":30,"de":63}`；
/// 序列化回传的是 `{n,d,big,b,de,len}`（`RectSpline` 的字段）。
#[derive(serde::Deserialize)]
struct JsonSpline {
    #[serde(default)]
    spec: Option<String>,
    #[serde(default)]
    n: Option<u32>,
    /// 小径 d（`RectSpline` 字段）。
    #[serde(default)]
    d: Option<f64>,
    /// 大径 D（`RectSpline` 字段）。
    #[serde(default)]
    big: Option<f64>,
    /// 键宽 B。
    #[serde(default)]
    b: Option<f64>,
    /// 滚刀外径 de。
    #[serde(default)]
    de: Option<f64>,
    /// 满齿段长 L（别名 `l`/`L`）。
    #[serde(default, alias = "l", alias = "L")]
    len: Option<f64>,
}

#[derive(serde::Deserialize)]
struct JsonChamfer {
    c: f64,
    #[serde(default)]
    end: Option<String>,
}

#[derive(serde::Deserialize)]
struct JsonOvertravel {
    #[serde(default)]
    b1: Option<f64>,
    #[serde(default)]
    end: Option<String>,
}

#[derive(serde::Deserialize)]
struct JsonRelief {
    #[serde(default)]
    end: Option<String>,
    #[serde(default)]
    p: Option<f64>,
    #[serde(default)]
    g1: Option<f64>,
    #[serde(default)]
    g2: Option<f64>,
    #[serde(default)]
    dg: Option<f64>,
    #[serde(default)]
    r: Option<f64>,
}

#[derive(serde::Deserialize)]
struct JsonGear {
    m: f64,
    z: u32,
    #[serde(default)]
    h: Option<f64>,
    #[serde(default)]
    beta: Option<f64>,
    /// 基准齿形角 α（度）；缺省 = 20°。
    #[serde(default)]
    alpha: Option<f64>,
}

/// `thread` 允许：`true` / `1.5`（螺距）/ `{"p":1.5}` / `{"pitch":1.5}` / `"M1.5"`，
/// 以及带新关键字的对象：`{"p":1.5,"tl":20,"runout":"short","shoulder":"long","relief":false}`
/// （`RO`/`SD`/`rl` 别名也认；档位中英文都收）。
fn json_thread<'de, D>(de: D) -> Result<Option<Thread>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw {
        On(bool),
        Pitch(f64),
        Text(String),
        Obj {
            #[serde(default)]
            p: Option<f64>,
            #[serde(default)]
            pitch: Option<f64>,
            #[serde(default)]
            tl: Option<f64>,
            #[serde(default, alias = "RO")]
            runout: Option<String>,
            #[serde(default, alias = "SD")]
            shoulder: Option<String>,
            #[serde(default)]
            relief: Option<bool>,
            #[serde(default)]
            rl: Option<bool>,
        },
    }
    let raw = match Option::<Raw>::deserialize(de)? {
        None => return Ok(None),
        Some(raw) => raw,
    };
    let make = |pitch: Option<f64>| Thread {
        pitch,
        ..Thread::default()
    };
    match raw {
        Raw::On(false) => Ok(None),
        Raw::On(true) => Ok(Some(make(None))),
        Raw::Pitch(pitch) => Ok(Some(make(Some(pitch)))),
        Raw::Obj {
            p,
            pitch,
            tl,
            runout,
            shoulder,
            relief,
            rl,
        } => {
            let mut thread = make(p.or(pitch));
            thread.tl = tl;
            if let Some(text) = runout {
                thread.runout = parse_runout_grade(&text, "JSON thread")
                    .map_err(serde::de::Error::custom)?;
            }
            if let Some(text) = shoulder {
                thread.shoulder = parse_shoulder_grade(&text, "JSON thread")
                    .map_err(serde::de::Error::custom)?;
            }
            thread.relief = relief.or(rl).unwrap_or(false);
            Ok(Some(thread))
        }
        Raw::Text(text) => {
            let text = text.trim();
            let text = text
                .strip_prefix(['M', 'm'])
                .map(str::trim)
                .unwrap_or(text);
            if text.is_empty() {
                return Ok(Some(make(None)));
            }
            let pitch = text.parse::<f64>().map_err(|_| {
                serde::de::Error::custom(format!("thread 字符串「{text}」不是螺距数字"))
            })?;
            Ok(Some(make(Some(pitch))))
        }
    }
}

/// `at` 允许 `[x,y]` / `"x,y"` / `{"x":…,"y":…}`。
fn json_point<'de, D>(de: D) -> Result<Option<[f64; 2]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Pair([f64; 2]),
        Text(String),
        Xy { x: f64, y: f64 },
    }
    match Option::<Raw>::deserialize(de)? {
        None => Ok(None),
        Some(Raw::Pair(pair)) => Ok(Some(pair)),
        Some(Raw::Xy { x, y }) => Ok(Some([x, y])),
        Some(Raw::Text(text)) => {
            let (x, y) = text
                .split_once(',')
                .ok_or_else(|| serde::de::Error::custom("at 字符串要写成 \"x,y\""))?;
            let x = x
                .trim()
                .parse::<f64>()
                .map_err(|_| serde::de::Error::custom("at 的 x 不是数字"))?;
            let y = y
                .trim()
                .parse::<f64>()
                .map_err(|_| serde::de::Error::custom("at 的 y 不是数字"))?;
            Ok(Some([x, y]))
        }
    }
}

fn parse_json(text: &str) -> Result<Program, String> {
    let raw: JsonProgram =
        serde_json::from_str(text).map_err(|e| format!("JSON 解析失败：{e}"))?;
    if raw.segments.is_empty() {
        return Err("JSON 里没有 segments（至少给一段）".into());
    }
    let mut segments = Vec::with_capacity(raw.segments.len());
    for (index, item) in raw.segments.iter().enumerate() {
        let number = index + 1;
        let mut ch = Vec::new();
        for (k, value) in item.ch.iter().enumerate() {
            let end = match &value.end {
                None => End::R,
                Some(text) => parse_end(text).map_err(|bad| {
                    format!("第 {number} 段：ch[{k}] 的端别「{bad}」非法（只能用 L 或 R）")
                })?,
            };
            if ch.iter().any(|x: &Chamfer| x.end == end) {
                return Err(format!("第 {number} 段：ch 在{}端重复", end_cn(end)));
            }
            ch.push(Chamfer { c: value.c, end });
        }
        let mut ov = Vec::new();
        for (k, value) in item.ov.iter().enumerate() {
            let end = match &value.end {
                None => End::R,
                Some(text) => parse_end(text).map_err(|bad| {
                    format!("第 {number} 段：ov[{k}] 的端别「{bad}」非法（只能用 L 或 R）")
                })?,
            };
            if ov.iter().any(|x: &Overtravel| x.end == end) {
                return Err(format!("第 {number} 段：ov 在{}端重复", end_cn(end)));
            }
            ov.push(Overtravel { b1: value.b1, end });
        }
        let mut relief = Vec::new();
        for (k, value) in item.relief.iter().enumerate() {
            let end = match &value.end {
                None => End::R,
                Some(text) => parse_end(text).map_err(|bad| {
                    format!("第 {number} 段：relief[{k}] 的端别「{bad}」非法（只能用 L 或 R）")
                })?,
            };
            if relief.iter().any(|x: &Relief| x.end == end) {
                return Err(format!("第 {number} 段：relief 在{}端重复", end_cn(end)));
            }
            relief.push(Relief {
                end,
                p: value.p,
                g1: value.g1,
                g2: value.g2,
                dg: value.dg,
                r: value.r,
            });
        }
        if item.es.is_some() {
            return Err(format!(
                "第 {number} 段：`ES` 已取消 —— 退刀槽请用段级 `RL` 或一小段小直径轴段表示，例如 `S24 E24 L5`"
            ));
        }
        let thread = item.thread;
        // `M` 段右端的退刀槽收尾以 `thread.relief` 表示；不要把同一端再写进
        // 段级 `relief`（两套同时画会重复）。
        if thread.as_ref().is_some_and(|t| t.relief) && relief.iter().any(|r| r.end == End::R) {
            return Err(format!(
                "第 {number} 段：M 段右端的退刀槽收尾用 thread.relief 表示，不要再写段级 relief（端别 R）"
            ));
        }
        // 花键段：直径由规格代号导出、长度 = L + 收尾 l；与 CH/OV/RL/M 同段冲突。
        // 渐开线花键段：预设代号 + m/z/x 导出直径，长度 = L + 收尾（给 de 时）；与 CH/OV/RL/M/GEAR/SPLINE 同段冲突。
        if let Some(ji) = &item.invol_spline {
            if item.spline.is_some() {
                return Err(format!("第 {number} 段：渐开线花键段不能与矩形花键 spline 同段"));
            }
            if !ch.is_empty() {
                return Err(format!(
                    "第 {number} 段：渐开线花键段不能与倒角 ch 同段（请在相邻轴段上写 ch）"
                ));
            }
            if !ov.is_empty() {
                return Err(format!("第 {number} 段：渐开线花键段不能与越程槽 ov 同段"));
            }
            if !relief.is_empty() {
                return Err(format!("第 {number} 段：渐开线花键段不能与退刀槽 relief 同段"));
            }
            if thread.is_some() {
                return Err(format!("第 {number} 段：渐开线花键段不能与螺纹段 m 同段"));
            }
            if item.gear.is_some() {
                return Err(format!("第 {number} 段：渐开线花键段不能与齿轮段 gear 同段"));
            }
            let token = ji
                .code
                .clone()
                .or_else(|| ji.preset_code.clone())
                .or_else(|| match (&ji.std, &ji.profile) {
                    (Some(std), Some(profile)) => Some(format!("{std}{profile}")),
                    (None, Some(profile)) => Some(profile.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    format!(
                        "第 {number} 段：渐开线花键段缺预设代号 code（例 {{\"code\":\"GB30R\",\"m\":3,\"z\":20,\"len\":30}}）"
                    )
                })?;
            let (std, profile) = crate::invol_spline::parse_preset_token(&token).ok_or_else(|| {
                format!(
                    "第 {number} 段：不认识的体系标识/预设代号「{token}」\
                     （可用 GB30P/GB30R/GB375R/GB45R/DIN30/NFP/NFR，或体系标识 GB/DIN/NF/ANSI）"
                )
            })?;
            let (params, origin) = crate::invol_spline::resolve_spline(
                std,
                profile,
                ji.d_b,
                if std == crate::invol_spline::SplineStd::ANSI {
                    ji.pitch.or(ji.m)
                } else {
                    // 径节 P/DP 只属 ANSI；GB/DIN/NF 收到 → 明确拒绝（不把 P 当模数）。
                    if ji.pitch.is_some() {
                        return Err(format!(
                            "第 {number} 段：{} 体系：{}。",
                            std.code(),
                            crate::invol_spline::PITCH_ONLY_ANSI_MSG
                        ));
                    }
                    ji.m
                },
                ji.z,
                ji.x,
            )
            .map_err(|e| format!("第 {number} 段：{e}"))?;
            let code = crate::invol_spline::preset_code(std, profile).unwrap_or("").to_string();
            let len = ji.len.ok_or_else(|| {
                format!(
                    "第 {number} 段：渐开线花键段缺少 len（有效长度 L，例 {{\"code\":\"GB30R\",\"m\":3,\"z\":20,\"len\":30}}）"
                )
            })?;
            let mut invol = InvolSeg::new(code, params, len, ji.de)
                .map_err(|e| format!("第 {number} 段：{e}"))?;
            if let Some(o) = &origin {
                if !matches!(o, crate::invol_spline::D_bOrigin::Table(_)) {
                    invol.d_b_note = Some(o.note());
                }
            }
            // 与 DSL 入口同一口径：计算书（`… report`）复用同一次解析的来源，不重新另算。
            invol.d_b_origin = origin;
            if ji.check.unwrap_or(false) {
                invol.inspection = Some(invol_check_note(
                    &invol,
                    &format!("第 {number} 段"),
                    "INVOLSPLINE",
                )?);
            }
            let d = invol.major_radius() * 2.0;
            if let Some(s) = item.s {
                if (s - d).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：渐开线花键段的 s={} 应等于大径 da={}",
                        trim(s),
                        trim(d)
                    ));
                }
            }
            if let Some(e) = item.e {
                if (e - d).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：渐开线花键段的 e={} 应等于大径 da={}",
                        trim(e),
                        trim(d)
                    ));
                }
            }
            if let Some(l) = item.l {
                if (l - invol.segment_len()).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：渐开线花键段的 l={} 应等于 L+l={}（L={} + 收尾 {}）",
                        trim(l),
                        trim(invol.segment_len()),
                        trim(invol.len),
                        trim(invol.runout())
                    ));
                }
            }
            segments.push(Segment {
                s: d,
                e: d,
                l: invol.segment_len(),
                ch: Vec::new(),
                ov: Vec::new(),
                relief: Vec::new(),
                thread: None,
                gear: None,
                spline: None,
                invol_spline: Some(invol),
            });
            continue;
        }
        if let Some(js) = &item.spline {
            if !ch.is_empty() {
                return Err(format!("第 {number} 段：花键段不能与倒角 ch 同段（请在相邻轴段上写 ch）"));
            }
            if !ov.is_empty() {
                return Err(format!("第 {number} 段：花键段不能与越程槽 ov 同段"));
            }
            if !relief.is_empty() {
                return Err(format!("第 {number} 段：花键段不能与退刀槽 relief 同段"));
            }
            if thread.is_some() {
                return Err(format!("第 {number} 段：花键段不能与螺纹段 m 同段"));
            }
            let len = js.len.ok_or_else(|| {
                format!("第 {number} 段：花键段缺少 len（满齿段长 L，例 {{\"spec\":\"6x23x26x6\",\"len\":30}}")
            })?;
            let spline = if let Some(spec) = &js.spec {
                crate::spline::RectSpline::from_code(spec, js.de, len)
            } else {
                let n = js.n.ok_or_else(|| format!("第 {number} 段：花键段缺 n（齿数）"))?;
                let d = js.d.ok_or_else(|| format!("第 {number} 段：花键段缺 d（小径）"))?;
                let big = js.big.ok_or_else(|| format!("第 {number} 段：花键段缺 big（大径 D）"))?;
                let b = js.b.ok_or_else(|| format!("第 {number} 段：花键段缺 b（键宽 B）"))?;
                let de = match js.de {
                    Some(de) => de,
                    None => crate::spline::lookup_de(n, d, big, b).ok_or_else(|| {
                        format!("第 {number} 段：花键段 de 查不到（不在表 1/表 2），请显式给 de")
                    })?,
                };
                crate::spline::RectSpline::new(n, d, big, b, de, len)
            }
            .map_err(|e| format!("第 {number} 段：{e}"))?;
            if let Some(s) = item.s {
                if (s - spline.big).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：花键段的 s={} 应等于大径 D={}",
                        trim(s),
                        trim(spline.big)
                    ));
                }
            }
            if let Some(e) = item.e {
                if (e - spline.big).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：花键段的 e={} 应等于大径 D={}",
                        trim(e),
                        trim(spline.big)
                    ));
                }
            }
            if let Some(l) = item.l {
                if (l - spline.segment_len()).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：花键段的 l={} 应等于 L+l={}（L={} + 收尾 {})），",
                        trim(l),
                        trim(spline.segment_len()),
                        trim(spline.len),
                        trim(spline.runout())
                    ));
                }
            }
            segments.push(Segment {
                s: spline.big,
                e: spline.big,
                l: spline.segment_len(),
                ch: Vec::new(),
                ov: Vec::new(),
                relief: Vec::new(),
                thread: None,
                gear: None,
                spline: Some(spline),
                invol_spline: None,
            });
            continue;
        }
        // 齿轮段：直径由 m·z 导出、长度用 h；与 CH/OV/M 同段冲突。
        // （序列化回传的 s/e/l 允许出现，但要等于派生值，不允许相互矛盾。）
        if let Some(g) = &item.gear {
            if !ch.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与倒角 ch 同段"));
            }
            if !ov.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与越程槽 ov 同段"));
            }
            if !relief.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与退刀槽 relief 同段"));
            }
            if thread.is_some() {
                return Err(format!("第 {number} 段：齿轮段不能与螺纹段 m 同段"));
            }
            let gear = Gear {
                m: g.m,
                z: g.z,
                h: g.h,
                beta_deg: g.beta.unwrap_or(0.0),
                alpha_deg: g.alpha.unwrap_or_else(default_alpha_deg),
            };
            if gear.beta_deg.abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：齿轮段斜齿（beta={}）本期只做直齿（斜齿未实现）",
                    trim(gear.beta_deg)
                ));
            }
            let params = gear.params();
            params
                .validate()
                .map_err(|e| format!("第 {number} 段：齿轮段：{e}"))?;
            let d = params.d();
            if let Some(s) = item.s {
                if (s - d).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：齿轮段的 s={} 应等于分度圆 d={}（由 m·z 导出）",
                        trim(s),
                        trim(d)
                    ));
                }
            }
            if let Some(e) = item.e {
                if (e - d).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：齿轮段的 e={} 应等于分度圆 d={}（由 m·z 导出）",
                        trim(e),
                        trim(d)
                    ));
                }
            }
            if let Some(l) = item.l {
                if (l - gear.width()).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：齿轮段的 l={} 应等于齿宽 h={}",
                        trim(l),
                        trim(gear.width())
                    ));
                }
            }
            segments.push(Segment {
                s: d,
                e: d,
                l: gear.width(),
                ch,
                ov,
                relief: Vec::new(),
                thread,
                gear: Some(gear),
                spline: None,
                invol_spline: None,
            });
            continue;
        }
        let s = item
            .s
            .ok_or_else(|| format!("第 {number} 段：缺少 s（起始直径）"))?;
        let l = item
            .l
            .ok_or_else(|| format!("第 {number} 段：缺少 l（段长）"))?;
        segments.push(Segment {
            s,
            e: item.e.unwrap_or(s),
            l,
            ch,
            ov,
            relief,
            thread,
            gear: None,
            spline: None,
            invol_spline: None,
        });
    }
    let view = match &raw.view {
        None => ShaftView::default(),
        Some(text) => ShaftView::parse(text).map_err(|e| format!("JSON：{e}"))?,
    };
    if raw.rot.is_some_and(|v| !v.is_finite()) {
        return Err("JSON：rot 不是有限数".into());
    }
    Ok(Program {
        segments,
        at: raw.at,
        rot: raw.rot,
        view,
    })
}

// ══════════════════════════════════════════════════════════════════════════
// 合法性检查
// ══════════════════════════════════════════════════════════════════════════

/// 与端面无关的逐段检查（段长/直径/倒角/重复特征）。
/// 端面之间的检查（倒角直径变化量、越程槽台阶…）在 [`build`] 里做。
pub fn validate(program: &Program) -> Result<(), String> {
    if program.segments.is_empty() {
        return Err("至少要有一段（S… L…）".into());
    }
    for (index, seg) in program.segments.iter().enumerate() {
        let number = index + 1;
        if !seg.l.is_finite() || seg.l <= 0.0 {
            return Err(format!(
                "第 {number} 段：段长 L={} 必须 > 0",
                trim(seg.l)
            ));
        }
        if !seg.s.is_finite() || seg.s <= 0.0 {
            return Err(format!(
                "第 {number} 段：起始直径 S={} 必须 > 0",
                trim(seg.s)
            ));
        }
        if !seg.e.is_finite() || seg.e <= 0.0 {
            return Err(format!(
                "第 {number} 段：终点直径 E={} 必须 > 0",
                trim(seg.e)
            ));
        }
        for chamfer in &seg.ch {
            if !chamfer.c.is_finite() || chamfer.c <= 0.0 {
                return Err(format!(
                    "第 {number} 段：倒角 C={} 必须 > 0",
                    trim(chamfer.c)
                ));
            }
            if chamfer.c >= seg.l / 2.0 {
                return Err(format!(
                    "第 {number} 段：{}端倒角 C={} ≥ 段长/2（l={}），特征重叠",
                    end_cn(chamfer.end),
                    trim(chamfer.c),
                    trim(seg.l)
                ));
            }
        }
        for (a, first) in seg.ch.iter().enumerate() {
            for second in &seg.ch[a + 1..] {
                if first.end == second.end {
                    return Err(format!(
                        "第 {number} 段：倒角 CH 在{}端重复",
                        end_cn(first.end)
                    ));
                }
            }
        }
        for ov in &seg.ov {
            if let Some(b1) = ov.b1 {
                if !b1.is_finite() || b1 <= 0.0 {
                    return Err(format!(
                        "第 {number} 段：越程槽 b1={} 必须 > 0",
                        trim(b1)
                    ));
                }
                if b1 > seg.l + 1e-9 {
                    return Err(format!(
                        "第 {number} 段：越程槽 b1={} > 段长 l={}",
                        trim(b1),
                        trim(seg.l)
                    ));
                }
            }
        }
        for (a, first) in seg.ov.iter().enumerate() {
            for second in &seg.ov[a + 1..] {
                if first.end == second.end {
                    return Err(format!(
                        "第 {number} 段：越程槽 OV 在{}端重复",
                        end_cn(first.end)
                    ));
                }
            }
        }
        // ── 段级退刀槽 RL（任一圆柱段，贴左/右端）：结构 + 取参 + 装得下 ──
        for (a, first) in seg.relief.iter().enumerate() {
            for second in &seg.relief[a + 1..] {
                if first.end == second.end {
                    return Err(format!(
                        "第 {number} 段：退刀槽 RL 重复（在{}端）",
                        end_cn(first.end)
                    ));
                }
            }
        }
        if !seg.relief.is_empty() {
            if seg.gear.is_some() {
                return Err(format!("第 {number} 段：齿轮段不能与退刀槽 RL 同段"));
            }
            if seg
                .thread
                .as_ref()
                .is_some_and(|thread| thread.relief)
                && seg.relief.iter().any(|r| r.end == End::R)
            {
                return Err(format!(
                    "第 {number} 段：M 段右端的退刀槽收尾用 thread.relief 表示，不要再写段级 relief（端别 R）"
                ));
            }
            for spec in &seg.relief {
                if (seg.s - seg.e).abs() > 1e-9 {
                    return Err(format!(
                        "第 {number} 段：RL 退刀槽只能开在圆柱端（本段是锥面 S={} → E={}）",
                        trim(seg.s),
                        trim(seg.e)
                    ));
                }
                if index == 0 && spec.end == End::L {
                    return Err(
                        "第 1 段：左端是自由端，退刀槽没有台阶面".to_string()
                    );
                }
                if index + 1 == program.segments.len() && spec.end == End::R {
                    return Err(format!(
                        "第 {number} 段：右端是自由端，退刀槽没有台阶面"
                    ));
                }
                let dims = resolve_relief_dims(seg, spec, &format!("第 {number} 段"))?;
                if dims.g2 > seg.l + 1e-9 {
                    return Err(format!(
                        "第 {number} 段：RL 退刀槽 g2={} > 段长 L={}",
                        trim(dims.g2),
                        trim(seg.l)
                    ));
                }
                // 同一段同端的 CH / OV 是「同端只能有一个槽/倒角」。
                if seg.ch.iter().any(|c| c.end == spec.end) {
                    return Err(format!(
                        "第 {number} 段：{}端的 RL 退刀槽与倒角 CH 冲突（同端只能一个）",
                        end_cn(spec.end)
                    ));
                }
                if seg.ov.iter().any(|o| o.end == spec.end) {
                    return Err(format!(
                        "第 {number} 段：{}端的 RL 退刀槽与越程槽 OV 冲突（同端只能一个）",
                        end_cn(spec.end)
                    ));
                }
            }
        }
        if let Some(thread) = &seg.thread {
            if let Some(pitch) = thread.pitch {
                if !pitch.is_finite() || pitch <= 0.0 {
                    return Err(format!(
                        "第 {number} 段：螺纹螺距 P={} 必须 > 0",
                        trim(pitch)
                    ));
                }
                let minor = thread.minor_diameter(seg.s);
                if minor <= 0.0 {
                    return Err(format!(
                        "第 {number} 段：螺距 P={} 太大（小径 d−1.0825P={} ≤ 0）",
                        trim(pitch),
                        trim(minor)
                    ));
                }
            }
            if (seg.s - seg.e).abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：螺纹段必须是圆柱（S==E，当前 S={}、E={}）",
                    trim(seg.s),
                    trim(seg.e)
                ));
            }
            if !seg.ov.is_empty() {
                return Err(format!("第 {number} 段：螺纹段 M 不能与越程槽 OV 同段"));
            }
            if seg.gear.is_some() {
                return Err(format!(
                    "第 {number} 段：螺纹段 M 不能与齿轮段 GEAR 同段"
                ));
            }
            // ── 局部螺纹（`TL` + `RO`/`SD` 或 `RL`）：表 1/表 2 查表 + 几何装得下 ──
            let wants_local = thread.tl.is_some() || thread.relief;
            if thread.relief
                && (thread.runout != RunoutGrade::Normal
                    || thread.shoulder != ShoulderGrade::Normal)
            {
                return Err(format!(
                    "第 {number} 段：RL（表 2 退刀槽收尾）与 RO/SD（螺尾/肩距）互斥，二选一"
                ));
            }
            if wants_local {
                let pitch = thread.pitch.ok_or_else(|| {
                    format!(
                        "第 {number} 段：局部螺纹 TL/RL 必须给螺距（写法 M1.5 TL20；表 1/表 2 都按螺距查）"
                    )
                })?;
                if thread.tl.is_some_and(|tl| !tl.is_finite() || tl <= 0.0) {
                    return Err(format!("第 {number} 段：完整螺纹长度 TL 必须 > 0"));
                }
                let tl = thread.tl.unwrap_or(0.0);
                if thread.relief {
                    let row =
                        detail::thread_relief_row(pitch).map_err(|e| format!("第 {number} 段：{e}"))?;
                    let dg = seg.s - row.dg_reduction;
                    if dg <= 0.0 {
                        return Err(format!(
                            "第 {number} 段：退刀槽 dg = d − {} = {} ≤ 0（螺纹直径太小）",
                            trim(row.dg_reduction),
                            trim(dg)
                        ));
                    }
                    let r1 = thread.minor_radius(seg.s);
                    if r1 <= dg / 2.0 + 1e-9 {
                        return Err(format!(
                            "第 {number} 段：退刀槽底 dg={} 不低于螺纹小径 d1={}（槽没切进牙底）",
                            trim(dg),
                            trim(r1 * 2.0)
                        ));
                    }
                    let need = row.g2 + tl;
                    if need > seg.l + 1e-9 {
                        return Err(format!(
                            "第 {number} 段：退刀槽 g2={} + 完整螺纹 TL={} = {} 超过段长 L={}",
                            trim(row.g2),
                            trim(tl),
                            trim(need),
                            trim(seg.l)
                        ));
                    }
                } else {
                    let row = detail::runout_row_checked(pitch)
                        .map_err(|e| format!("第 {number} 段：{e}"))?;
                    let x = row.x(thread.runout);
                    let a = row.a(thread.shoulder);
                    if x > a + 1e-9 {
                        return Err(format!(
                            "第 {number} 段：收尾 x={}（RO）大于肩距 a={}（SD），档位不搭（把 SD 调大或 RO 调短）",
                            trim(x),
                            trim(a)
                        ));
                    }
                    let need = a + tl;
                    if need > seg.l + 1e-9 {
                        return Err(format!(
                            "第 {number} 段：肩距 a={} + 完整螺纹 TL={} = {} 超过段长 L={}",
                            trim(a),
                            trim(tl),
                            trim(need),
                            trim(seg.l)
                        ));
                    }
                }
                if seg.ch.iter().any(|c| c.end == End::R) {
                    return Err(format!(
                        "第 {number} 段：局部螺纹 TL/RL 的右端不能倒角 CH（会吃掉锥面/退刀槽的台肩角）"
                    ));
                }
            } else if thread.runout != RunoutGrade::Normal
                || thread.shoulder != ShoulderGrade::Normal
            {
                return Err(format!(
                    "第 {number} 段：RO/SD 只在局部螺纹（给了 TL）时有意义——不给 TL = 整段全螺纹"
                ));
            }
        }
        if let Some(gear) = &seg.gear {
            gear.params()
                .validate()
                .map_err(|e| format!("第 {number} 段：齿轮段：{e}"))?;
            if !seg.ch.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与倒角 CH 同段"));
            }
            if !seg.ov.is_empty() {
                return Err(format!("第 {number} 段：齿轮段不能与越程槽 OV 同段"));
            }
            if seg.thread.is_some() {
                return Err(format!("第 {number} 段：齿轮段不能与螺纹段 M 同段"));
            }
        }
        // ── 矩形花键段（SPLINE）：直径由规格导出、段长 = L+l；与 CH/OV/RL/M/GEAR 互斥 ──
        if let Some(spline) = &seg.spline {
            if (seg.s - spline.big).abs() > 1e-9 || (seg.e - spline.big).abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：花键段的 S/E={}/{} 应等于大径 D={}（由规格导出）",
                    trim(seg.s),
                    trim(seg.e),
                    trim(spline.big)
                ));
            }
            if (seg.l - spline.segment_len()).abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：花键段段长 {} 应等于 L+l={}（L={} + 收尾 {}）",
                    trim(seg.l),
                    trim(spline.segment_len()),
                    trim(spline.len),
                    trim(spline.runout())
                ));
            }
            if !seg.ch.is_empty() {
                return Err(format!(
                    "第 {number} 段：花键段不能与倒角 CH 同段（引入倒角写在相邻轴段上）"
                ));
            }
            if !seg.ov.is_empty() {
                return Err(format!("第 {number} 段：花键段不能与越程槽 OV 同段"));
            }
            if !seg.relief.is_empty() {
                return Err(format!("第 {number} 段：花键段不能与退刀槽 RL 同段"));
            }
            if seg.thread.is_some() {
                return Err(format!("第 {number} 段：花键段不能与螺纹段 M 同段"));
            }
            if seg.gear.is_some() {
                return Err(format!("第 {number} 段：花键段不能与齿轮段 GEAR 同段"));
            }
        }
        // ── 渐开线花键段（INVOLSPLINE）：直径由预设导出、段长 = L+l（有 de 时）；
        //    与 CH/OV/RL/M/GEAR/SPLINE 互斥（同 SPLINE）。──
        if let Some(invol) = &seg.invol_spline {
            invol
                .params
                .validate()
                .map_err(|e| format!("第 {number} 段：{e}"))?;
            let d = invol.major_radius() * 2.0;
            if (seg.s - d).abs() > 1e-9 || (seg.e - d).abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：渐开线花键段的 S/E={}/{} 应等于大径 da={}（由预设导出）",
                    trim(seg.s),
                    trim(seg.e),
                    trim(d)
                ));
            }
            if (seg.l - invol.segment_len()).abs() > 1e-9 {
                return Err(format!(
                    "第 {number} 段：渐开线花键段段长 {} 应等于 L+l={}（L={} + 收尾 {}）",
                    trim(seg.l),
                    trim(invol.segment_len()),
                    trim(invol.len),
                    trim(invol.runout())
                ));
            }
            if !seg.ch.is_empty() {
                return Err(format!(
                    "第 {number} 段：渐开线花键段不能与倒角 CH 同段（引入倒角写在相邻轴段上）"
                ));
            }
            if !seg.ov.is_empty() {
                return Err(format!("第 {number} 段：渐开线花键段不能与越程槽 OV 同段"));
            }
            if !seg.relief.is_empty() {
                return Err(format!("第 {number} 段：渐开线花键段不能与退刀槽 RL 同段"));
            }
            if seg.thread.is_some() {
                return Err(format!("第 {number} 段：渐开线花键段不能与螺纹段 M 同段"));
            }
            if seg.gear.is_some() {
                return Err(format!("第 {number} 段：渐开线花键段不能与齿轮段 GEAR 同段"));
            }
            if seg.spline.is_some() {
                return Err(format!("第 {number} 段：渐开线花键段不能与矩形花键 SPLINE 同段"));
            }
        }
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════
// 几何
// ══════════════════════════════════════════════════════════════════════════

/// 构建结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Shaft {
    pub entities: Vec<EntityType>,
    pub total_length: f64,
    pub max_diameter: f64,
    pub segment_count: usize,
}

/// 段内某 x 处的轮廓半径（上半侧；圆锥按线性插值）。
fn radius_at(seg: &Segment, x0: f64, x: f64) -> f64 {
    let r0 = seg.s / 2.0;
    let r1 = seg.e / 2.0;
    r0 + (r1 - r0) * ((x - x0) / seg.l)
}

/// 某端做 C 倒角：返回（轮廓点，端面点）。端面点 = 轮廓点在段内 C 处 − C（45°）。
fn chamfer_geom(seg: &Segment, x0: f64, end: End, c: f64) -> ([f64; 2], [f64; 2]) {
    match end {
        End::L => {
            let x_contour = x0 + c;
            let y_contour = radius_at(seg, x0, x_contour);
            ([x_contour, y_contour], [x0, y_contour - c])
        }
        End::R => {
            let x_face = x0 + seg.l;
            let x_contour = x_face - c;
            let y_contour = radius_at(seg, x0, x_contour);
            ([x_contour, y_contour], [x_face, y_contour - c])
        }
    }
}

/// **贯通轴线的段边界/特征界线竖线**（从 `-half` 到 `+half`，落 `1轮廓实线层`）。
///
/// 用户 2026-09-18 更正版口径（基准 `轴测试更正.dxf`）：
/// * 每条段边界都画（直径不变的分界也画），半高 = `min(左段端半径, 右段端半径)`；
/// * 倒角终点（倒角根）画，半高 = 倒角根半径；
/// * 槽（OV/RL）的槽肩面 / 槽底终止 / 斜壁终点各画一条，槽肩半高 = 圆角切点
///   半径（`d/2 + r − h` 或 `dg/2 + r`），槽底半高 = 槽底半径，斜壁终点
///   半高 = 段（大径）半径。
/// `half ≤ 0`（或非有限）不落图。
fn through_line(entities: &mut Vec<EntityType>, x: f64, half: f64) {
    if half.is_finite() && half > 1e-9 {
        entities.push(line([x, -half], [x, half], LAYER_MAIN));
    }
}

/// 某段某端**端面处实际轮廓半径**：段级退刀槽圆角切点 / 越程槽圆角切点 / 齿顶圆 /
/// 普通半径。只用于齿轮相邻直径检查；落图的端面几何在 `build` 里另算。
fn face_radius(seg: &Segment, end: End) -> f64 {
    if let Some(spec) = seg.relief.iter().find(|r| r.end == end) {
        if let Ok(dims) = resolve_relief_dims(seg, spec, "段") {
            return dims.dg / 2.0 + dims.r;
        }
    }
    if let Some(ov) = seg.ov.iter().find(|o| o.end == end) {
        let d = match end {
            End::L => seg.s,
            End::R => seg.e,
        };
        if let Ok((_, row)) = detail::groove_entities(d, ov.b1) {
            return detail::fillet_tangent_radius(d, row);
        }
    }
    seg.outer_radius(end)
}

/// 沿 x 平移（`双` 视图把第二张整体右移；Hatch 的边界段同步平移，
/// 图案 offset 是世界坐标下的周期量，不用改）。
fn translate_x(entity: EntityType, dx: f64) -> EntityType {
    match entity {
        EntityType::Line(mut l) => {
            l.start.x += dx;
            l.end.x += dx;
            EntityType::Line(l)
        }
        EntityType::Arc(mut a) => {
            a.center.x += dx;
            EntityType::Arc(a)
        }
        EntityType::Hatch(mut h) => {
            for path in &mut h.paths {
                for edge in &mut path.edges {
                    match edge {
                        BoundaryEdge::Line(e) => {
                            e.start.x += dx;
                            e.end.x += dx;
                        }
                        BoundaryEdge::CircularArc(e) => e.center.x += dx,
                        BoundaryEdge::EllipticArc(e) => {
                            e.center.x += dx;
                            e.major_axis_endpoint.x += dx;
                        }
                        BoundaryEdge::Polyline(e) => {
                            for v in &mut e.vertices {
                                v.x += dx;
                            }
                        }
                        BoundaryEdge::Spline(e) => {
                            for q in &mut e.control_points {
                                q.x += dx;
                            }
                            for q in &mut e.fit_points {
                                q.x += dx;
                            }
                        }
                    }
                }
            }
            EntityType::Hatch(h)
        }
        other => other,
    }
}

/// 局部坐标（端面在 x=0）→ 镜像到端面 `face_x` 左侧：`x' = face_x − x`。
/// 圆弧镜像后保持 DXF 的逆时针读法（角度按 `π − θ` 处理）。
fn mirror_x(entity: EntityType, face_x: f64) -> EntityType {
    match entity {
        EntityType::Line(mut l) => {
            l.start.x = face_x - l.start.x;
            l.end.x = face_x - l.end.x;
            EntityType::Line(l)
        }
        EntityType::Arc(mut a) => {
            a.center.x = face_x - a.center.x;
            let (start, end) = (a.start_angle, a.end_angle);
            a.start_angle = std::f64::consts::PI - end;
            a.end_angle = std::f64::consts::PI - start;
            EntityType::Arc(a)
        }
        other => other,
    }
}

/// 一次构建出来的原始几何：两视图共用的真实几何 + 剖视用的剖面线环。
struct Geometry {
    /// 两个视图共用的**真实几何**（轮廓/端面/倒角/槽/螺纹线/齿轮倒角）；
    /// 不含贯通竖线（那类只属于常规视图）。
    entities: Vec<EntityType>,
    /// 仅常规视图：段边界 / 倒角终点 / 槽肩、槽底、斜壁终点等**贯通竖线**
    /// （`through_line`；用户 2026-09-18 定案只加在常规视图）。
    through: Vec<EntityType>,
    /// 仅剖视视图的真实几何：齿轮段齿根线（齿部按不剖，也是剖面线边界）。
    section_lines: Vec<EntityType>,
    /// 仅常规视图：花键段的小径细线 / 收尾弧 / 两条细竖线（`2细线层`）。
    spline_regular: Vec<EntityType>,
    /// 剖视剖面线边界：上半外轮廓边界（左→右），拆成上/下两个环。
    profile: Vec<HatchEdge>,
    total_length: f64,
    max_diameter: f64,
    segment_count: usize,
}

/// 解析 → 校验 → 生成（`frame_scale` 用于轴线 `6n` 伸出量；无图框传 1.0）。
/// 返回的图元已按 `program.view` 组合（常规 / 剖视 / 双）。
pub fn build(program: &Program, frame_scale: f64) -> Result<Shaft, String> {
    let Geometry {
        entities,
        through,
        section_lines,
        spline_regular,
        profile,
        total_length: total,
        max_diameter,
        segment_count,
    } = build_geometry(program, frame_scale)?;
    let rings = close_hatch_rings(profile, total);
    let hatch = || {
        if rings.is_empty() {
            Vec::new()
        } else {
            // ANSI31 / 比例 1.0，轴线上/下两个环（走 partgen_kit 的既有 HATCH 通路）。
            vec![crate::partgen_kit::hatch_ansi31_rings(&rings, 0.0, 1.0)]
        }
    };
    let entities = match program.view {
        ShaftView::Normal => {
            let mut out = entities;
            out.extend(through);
            out.extend(spline_regular);
            out
        }
        ShaftView::Section => {
            let mut out = entities;
            out.extend(section_lines);
            out.extend(hatch());
            out
        }
        ShaftView::Both => {
            // 并排：左常规、右剖视；间距 = max(总长×15%, 40×图框比例)。
            let dx = total + both_view_gap(total, frame_scale);
            let mut left = entities.clone();
            left.extend(through);
            left.extend(spline_regular);
            let mut right: Vec<EntityType> = entities
                .iter()
                .cloned()
                .map(|e| translate_x(e, dx))
                .collect();
            right.extend(section_lines.into_iter().map(|e| translate_x(e, dx)));
            right.extend(hatch().into_iter().map(|e| translate_x(e, dx)));
            left.extend(right);
            left
        }
    };
    Ok(Shaft {
        entities,
        total_length: total,
        max_diameter,
        segment_count,
    })
}

/// `双` 视图两视图之间的间距口径：`max(总长 × 15%, 40 × 图框比例)`。
fn both_view_gap(total_length: f64, frame_scale: f64) -> f64 {
    (0.15 * total_length).max(40.0 * frame_scale)
}

/// 一段局部螺纹（`M… TL…` 或 `M… RL`）的轴向布局：以段右端面（台肩）为基准。
///
/// 自右向左：`RL` = 台肩面 → R 圆角 → 槽底 `g1` → 斜壁（`g2` 处接大径）；
/// `TL` = 肩距 `a`（锥面 `a−x` + 螺尾 `x`）→ 分界竖线 → 完整螺纹 `TL`。
#[derive(Debug, Clone, Copy)]
struct LocalThread {
    /// 段右端面 x（台肩面）。
    face: f64,
    /// 螺纹大径半径（= 段半径）。
    r: f64,
    /// 螺纹小径半径（细实线所在半径）。
    r1: f64,
    /// 完整螺纹左端 x（`TL`；`RL` 不给 `TL` 时 = 段左端）。
    full_start: f64,
    /// 螺尾起点 x（锥面与螺尾交点）；`RL` 时不用。
    runout_start: f64,
    /// 完整螺纹 / 螺尾分界竖线 x；`RL` 时不用。
    boundary: f64,
    /// `RL`：表 2 退刀槽实际尺寸（`None` = 螺尾画法）。**表归一**后来自 `detail.rs`。
    relief: Option<detail::ReliefDims>,
    /// `RL`：台肩面圆角切点半径 = `dg/2 + r`。
    relief_tangent: f64,
    /// 大径轮廓线右端 x（`RL` = `face − g2`；否则 = `face`）。
    contour_end: f64,
    /// 小径细实线右端 x（螺尾起点，或斜壁与小径的交点）。
    minor_end: f64,
}

/// 该段是否为「普通全螺纹段」：`M` 段且没给 `TL`/`RL`（= 小径细实线贯穿整段的旧口径）。
/// 剖面线边界对这类段按**小径包络**（牙顶区不剖，参考件 `轴剖视图.dxf` 右视图口径）。
fn plain_thread(seg: &Segment) -> Option<&Thread> {
    seg.thread
        .as_ref()
        .filter(|t| t.tl.is_none() && !t.relief)
}

/// 是否花键段（矩形 `SPLINE` 或渐开线 `INVOLSPLINE`）：几何上一律禁止 CH，
/// 剖面线按“齿部不剖、小径包络”处理。
fn is_spline_seg(seg: &Segment) -> bool {
    seg.spline.is_some() || seg.invol_spline.is_some()
}

/// 轴上齿轮/渐开线花键段的端面自动倒角 C = round(0.6m)
/// （与 `gear.rs::GearParams::chamfer()` 同口径；花键体系没有单独的端面倒角数据，沿用同口径）。
///
/// 渐开线花键给了 `de` 时右端是滚刀收尾（收尾弧占住端面），不叠加端面倒角。
fn auto_face_chamfer(seg: &Segment, end: End) -> Option<f64> {
    let c = if let Some(gear) = &seg.gear {
        gear.chamfer()
    } else if let Some(invol) = &seg.invol_spline {
        if end == End::R && invol.de.is_some() {
            return None;
        }
        (crate::gear::CHAMFER_RATIO * invol.params.m).round()
    } else {
        return None;
    };
    (c > 1e-9).then_some(c)
}

/// 端面自动倒角的**单侧规则**（用户 2026-09-21 定案）：自由端（无邻段）或
/// 邻段外轮廓更小（台阶向下）→ 加；邻段更大（肩部）/ 相等（端面齐平、无外角）→ 不加。
fn side_auto_chamfer(c: Option<f64>, own_r: f64, neighbor_r: Option<f64>) -> Option<f64> {
    let c = c?;
    match neighbor_r {
        None => Some(c),
        Some(r) if r < own_r - 1e-12 => Some(c),
        Some(_) => None,
    }
}

/// 解析 → 校验 → 生成常规视图轮廓，并顺手记录剖面线环用的上半外轮廓段。
fn build_geometry(program: &Program, frame_scale: f64) -> Result<Geometry, String> {
    validate(program)?;
    let segs = &program.segments;
    let count = segs.len();

    // 每段起点 x（第 1 段左端面 = 0）。
    let mut x0s = Vec::with_capacity(count);
    let mut cursor = 0.0;
    for seg in segs {
        x0s.push(cursor);
        cursor += seg.l;
    }
    let total = program.total_length();
    let max_diameter = segs
        .iter()
        .map(|s| s.outer_radius(End::L).max(s.outer_radius(End::R)) * 2.0)
        .fold(0.0_f64, f64::max);

    // ── 局部螺纹（`TL` 收尾 / `RL` 退刀槽）布局：贴段右端台肩布置；
    //    不写 `TL` 也不写 `RL` 的 `M` 仍是旧口径（整段全螺纹）。 ──
    let mut local_threads: Vec<Option<LocalThread>> = vec![None; count];
    for (index, seg) in segs.iter().enumerate() {
        let Some(thread) = &seg.thread else { continue };
        if thread.tl.is_none() && !thread.relief {
            continue;
        }
        let face = x0s[index] + seg.l;
        let r = seg.s / 2.0;
        let r1 = thread.minor_radius(seg.s);
        let mut lay = LocalThread {
            face,
            r,
            r1,
            full_start: x0s[index],
            runout_start: face,
            boundary: face,
            relief: None,
            relief_tangent: 0.0,
            contour_end: face,
            minor_end: face,
        };
        if thread.relief {
            let pitch = thread.pitch.ok_or_else(|| {
                format!(
                    "第 {} 段：局部螺纹 RL 必须给螺距（写法 M1.5 TL20 RL）",
                    index + 1
                )
            })?;
            let row = detail::thread_relief_row(pitch)
                .map_err(|e| format!("第 {} 段：{e}", index + 1))?;
            let dims = row.dims(seg.s);
            let rg = dims.dg / 2.0;
            lay.relief = Some(dims);
            lay.relief_tangent = rg + dims.r;
            lay.contour_end = face - dims.g2;
            // 小径细实线与斜壁的交点（斜壁在 `g2−g1` 距离内从 `dg/2` 升到 `d/2`）。
            lay.minor_end = face - dims.g1 - (dims.g2 - dims.g1) * (r1 - rg) / (r - rg);
            lay.full_start = match thread.tl {
                Some(tl) => face - dims.g2 - tl,
                None => x0s[index],
            };
        } else {
            let pitch = thread.pitch.ok_or_else(|| {
                format!("第 {} 段：局部螺纹 TL 必须给螺距（写法 M1.5 TL20）", index + 1)
            })?;
            let row = detail::runout_row_checked(pitch)
                .map_err(|e| format!("第 {} 段：{e}", index + 1))?;
            let a = row.a(thread.shoulder);
            let x = row.x(thread.runout);
            lay.runout_start = face - (a - x);
            lay.boundary = face - a;
            lay.minor_end = lay.runout_start;
            lay.full_start = lay.boundary - thread.tl.unwrap_or(0.0);
        }
        local_threads[index] = Some(lay);
    }

    // 段级 RL 退刀槽：先把每段的 [左, 右] 尺寸解出来（validate 已查过结构/取参，
    // 这里再算一遍供画法与轮廓平移用；邻段台阶/装不下在下面的端面循环里查）。
    let mut relief_dims: Vec<[Option<detail::ReliefDims>; 2]> = vec![[None, None]; count];
    for (index, seg) in segs.iter().enumerate() {
        for spec in &seg.relief {
            let label = format!("第 {} 段", index + 1);
            let dims = resolve_relief_dims(seg, spec, &label)?;
            relief_dims[index][match spec.end {
                End::L => 0,
                End::R => 1,
            }] = Some(dims);
        }
    }

    // 倒角 / 越程槽 / 段级退刀槽落在**本段自己**身上的量（决定本段轮廓线被吃掉多少）。
    let mut own_ch = vec![[None::<f64>; 2]; count]; // [左, 右]
    let mut own_ov = vec![[None::<f64>; 2]; count];
    let mut own_relief = vec![[None::<f64>; 2]; count];
    let mut entities: Vec<EntityType> = Vec::new();
    // 贯通竖线（`through_line` 系列）：只属于常规视图。
    let mut through: Vec<EntityType> = Vec::new();
    // 仅剖视视图的真实几何（齿轮齿根线）。
    let mut section_lines: Vec<EntityType> = Vec::new();
    // 仅常规视图的花键细线（小径线 / 收尾弧 / 两条细竖线，2细线层）。
    let mut spline_regular: Vec<EntityType> = Vec::new();
    // 剖面线环用的上半外轮廓段（含倒角/台阶/越程槽圆角/螺纹小径包络/齿轮齿根），
    // 最后按 x 排序拆成上/下两个环。
    let mut profile: Vec<HatchEdge> = Vec::new();

    // ── 齿轮段相邻直径检查（用户 2026-09-18 口径）：相邻半径 ≤ ra 放行；
    //    > ra 会盖住齿顶线 → 报「第 N 段」。不再受齿根圆 rf 限制。 ──
    for (index, seg) in segs.iter().enumerate() {
        let Some(gear) = &seg.gear else { continue };
        let ra = gear.addendum_radius();
        let neighbors = [
            (index.checked_sub(1), End::R),
            (
                if index + 1 < count {
                    Some(index + 1)
                } else {
                    None
                },
                End::L,
            ),
        ];
        for (j, end) in neighbors {
            let Some(j) = j else { continue };
            let r_n = face_radius(&segs[j], end);
            if r_n > ra + 1e-9 {
                return Err(format!(
                    "第 {} 段：齿轮段相邻第 {} 段 Ø{} 大于齿顶圆 Ø{}，会盖住齿顶线（相邻段半径必须 ≤ 齿顶圆半径）",
                    index + 1,
                    j + 1,
                    trim(r_n * 2.0),
                    trim(ra * 2.0)
                ));
            }
        }
    }

    // ── 内部端面（第 i 段右端 ↔ 第 i+1 段左端） ──
    for j in 0..count.saturating_sub(1) {
        let i = j;
        let k = j + 1;
        let x_face = x0s[k];
        let ch_r = segs[i].ch.iter().find(|c| c.end == End::R).map(|c| c.c);
        let ch_l = segs[k].ch.iter().find(|c| c.end == End::L).map(|c| c.c);
        let (chamfer, ch_from_left) = match (ch_r, ch_l) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "第 {} 段：右端倒角与第 {} 段左端倒角落在同一端面，特征重叠",
                    i + 1,
                    k + 1
                ))
            }
            (Some(c), None) => (Some(c), true),
            (None, Some(c)) => (Some(c), false),
            (None, None) => (None, false),
        };
        let ov_r = segs[i].ov.iter().find(|o| o.end == End::R).copied();
        let ov_l = segs[k].ov.iter().find(|o| o.end == End::L).copied();
        if ov_r.is_some() && ov_l.is_some() {
            return Err(format!(
                "第 {} 段：右端与第 {} 段左端都写了越程槽（同一端面只能一侧）",
                i + 1,
                k + 1
            ));
        }
        // 段级 RL（i 右端 / k 左端）与本端面已有的 M 段螺纹收尾：
        // 同一端面只能有一个退刀槽；与 CH/OV 互斥（同端只能有一个槽/倒角）。
        let relief_r = relief_dims[i][1];
        let relief_l = relief_dims[k][0];
        let thread_relief_here = local_threads[i].filter(|lay| lay.relief.is_some());
        let relief_kinds = relief_r.is_some() as u8
            + relief_l.is_some() as u8
            + thread_relief_here.is_some() as u8;
        if relief_kinds > 1 {
            return Err(format!("第 {} 段：同一端面只能有一个退刀槽（RL）", i + 1));
        }
        if relief_kinds > 0 {
            if chamfer.is_some() {
                return Err(format!(
                    "第 {} 段：退刀槽 RL 所在端面不能倒角 CH（同端只能有一个槽/倒角）",
                    i + 1
                ));
            }
            if ov_r.is_some() || ov_l.is_some() {
                return Err(format!(
                    "第 {} 段：退刀槽 RL 与越程槽 OV 在同一端面冲突（同端只能有一个槽/倒角）",
                    i + 1
                ));
            }
        }
        // 该端面处两侧的轮廓半径。
        let ra = segs[i].outer_radius(End::R);
        let rb = segs[k].outer_radius(End::L);
        // 齿轮/渐开线花键端面自动倒角 C = round(0.6m)：按单侧规则决定该端倒不倒
        //（自由端/邻段更小 → 倒；邻段更大/齐平 → 不倒；用户 CH 已占该侧时让位）。
        let user_land = chamfer.map(|_| if rb > ra { k } else { i });
        let auto_c_r = if user_land == Some(i) {
            None
        } else {
            side_auto_chamfer(auto_face_chamfer(&segs[i], End::R), ra, Some(rb))
        };
        let auto_c_l = if user_land == Some(k) {
            None
        } else {
            side_auto_chamfer(auto_face_chamfer(&segs[k], End::L), rb, Some(ra))
        };

        let mut bottom = (ra - auto_c_r.unwrap_or(0.0)).min(rb - auto_c_l.unwrap_or(0.0));
        let mut top = (ra - auto_c_r.unwrap_or(0.0)).max(rb - auto_c_l.unwrap_or(0.0));
        let mut chamfer_lines: Option<([f64; 2], [f64; 2])> = None;
        // 带 CH 倒角的落点段（用于判断剖面线边界是否由螺纹小径包络接管）。
        let mut chamfer_land: Option<usize> = None;
        // 端面带槽时的贯通竖线半高（槽肩圆角切点）；None = 用 min(left_eff, right_eff)。
        let mut face_through_h: Option<f64> = None;

        // ── 端面自动倒角（斜线 + 台阶竖线；竖线只属常规视图）──
        if let Some(c) = auto_c_r {
            let contour = [x_face - c, ra];
            let face_point = [x_face, ra - c];
            entities.push(line(face_point, contour, LAYER_MAIN));
            entities.push(line(
                [face_point[0], -face_point[1]],
                [contour[0], -contour[1]],
                LAYER_MAIN,
            ));
            through_line(&mut through, contour[0], contour[1]);
            own_ch[i][1] = Some(c);
        }
        if let Some(c) = auto_c_l {
            let contour = [x_face + c, rb];
            let face_point = [x_face, rb - c];
            entities.push(line(face_point, contour, LAYER_MAIN));
            entities.push(line(
                [face_point[0], -face_point[1]],
                [contour[0], -contour[1]],
                LAYER_MAIN,
            ));
            through_line(&mut through, contour[0], contour[1]);
            own_ch[k][0] = Some(c);
        }

        if let Some(c) = chamfer {
            let requester = if ch_from_left { i } else { k };
            let requester_end = if ch_from_left { "右" } else { "左" };
            if (ra - rb).abs() < 1e-12 {
                return Err(format!(
                    "第 {} 段：{}端没有端面（相邻段直径相同），无法倒角",
                    requester + 1,
                    requester_end
                ));
            }
            let delta = (ra - rb).abs();
            // 倒角贴**凸角**（大的一侧）；落在齿轮段那侧则冲突。
            let (land, _land_end) = if rb > ra { (k, End::L) } else { (i, End::R) };
            // 花键凸角允许「C = 全台阶」（模板 φ22→φ26 的 C2 就是如此，倒角正好吃掉台阶）。
            let full_step_on_spline = is_spline_seg(&segs[land]) && (c - delta).abs() < 1e-9;
            if c >= delta - 1e-12 && !full_step_on_spline {
                return Err(format!(
                    "第 {} 段：{}端倒角 C={} ≥ 端面直径变化量的一半（Ø{} → Ø{} 的 {}），端面被吃掉",
                    requester + 1,
                    requester_end,
                    trim(c),
                    trim(ra * 2.0),
                    trim(rb * 2.0),
                    trim(delta)
                ));
            }
            chamfer_land = Some(land);
            if segs[land].gear.is_some() {
                return Err(format!(
                    "第 {} 段：{}端倒角会落在第 {} 段齿轮段上（齿轮段不能倒角）",
                    requester + 1,
                    requester_end,
                    land + 1
                ));
            }
            if is_spline_seg(&segs[land]) && land == i {
                return Err(format!(
                    "第 {} 段：倒角会落在花键段右端（收尾弧占有该端）—— 引入倒角请写在花键左端",
                    requester + 1
                ));
            }
            let (contour, face_point) = if rb > ra {
                chamfer_geom(&segs[k], x0s[k], End::L, c)
            } else {
                chamfer_geom(&segs[i], x0s[i], End::R, c)
            };
            if face_point[1] <= bottom + 1e-9 && !full_step_on_spline {
                return Err(format!(
                    "第 {} 段：{}端倒角与相邻面重叠（端面点 {} ≤ {}）",
                    requester + 1,
                    requester_end,
                    trim(face_point[1]),
                    trim(bottom)
                ));
            }
            top = face_point[1];
            chamfer_lines = Some((contour, face_point));
            if rb > ra {
                own_ch[k][0] = Some(c);
            } else {
                own_ch[i][1] = Some(c);
            }
        }

        if let Some(ov) = ov_r {
            // 槽开在第 i 段（磨出的外圆），台阶 = 第 i+1 段。
            if !(rb > ra + 1e-12) {
                return Err(format!(
                    "第 {} 段：右端越程槽没有台阶面（相邻段 Ø{} 不大于本段 Ø{}）",
                    i + 1,
                    trim(rb * 2.0),
                    trim(ra * 2.0)
                ));
            }
            if (segs[i].s - segs[i].e).abs() > 1e-9 {
                return Err(format!("第 {} 段：右端是锥面，越程槽只能开在圆柱端", i + 1));
            }
            let (groove, row) = detail::groove_entities(segs[i].e, ov.b1)
                .map_err(|e| format!("第 {} 段：右端越程槽：{e}", i + 1))?;
            if row.b1 > segs[i].l + 1e-9 {
                return Err(format!(
                    "第 {} 段：越程槽 b1={} > 段长 l={}",
                    i + 1,
                    trim(row.b1),
                    trim(segs[i].l)
                ));
            }
            let fillet = detail::fillet_tangent_radius(segs[i].e, row);
            if fillet >= top - 1e-9 {
                return Err(format!(
                    "第 {} 段：右端越程槽的 R 圆角切点 {} 不低于台阶面 {}（相邻台阶太小）",
                    i + 1,
                    trim(fillet),
                    trim(top)
                ));
            }
            bottom = fillet;
            face_through_h = Some(fillet);
            own_ov[i][1] = Some(row.b1);
            // `detail.rs` 的磨外圆槽体已按用户 2026-09-18 定案不含砂轮细线
            // （`2细线层` 青线属标注；独立要素同口径），这里直接镜像。
            entities.extend(groove.into_iter().map(|e| mirror_x(e, x_face)));
            // 剖面线环：斜坡 → 槽底 → 圆角（上半侧，左→右）
            let rg = segs[i].e / 2.0;
            let yb = rg - row.h;
            // 槽底终止 / 斜壁终点两条贯通竖线（槽肩那条 = 上面的段边界线）。
            through_line(&mut through, x_face - (row.b1 - row.h), yb);
            through_line(&mut through, x_face - row.b1, rg);
            let slope_x1 = x_face - (row.b1 - row.h);
            let groove_x1 = x_face - row.r;
            profile.push(lr_line([x_face - row.b1, rg], [slope_x1, yb]));
            profile.push(lr_line([slope_x1, yb], [groove_x1, yb]));
            profile.push(HatchEdge::Arc {
                c: [groove_x1, fillet],
                r: row.r,
                start_deg: -90.0,
                end_deg: 0.0,
                ccw: true,
            });
        } else if let Some(ov) = ov_l {
            // 槽开在第 k 段（磨出的外圆），台阶 = 第 i 段。
            if !(ra > rb + 1e-12) {
                return Err(format!(
                    "第 {} 段：左端越程槽没有台阶面（相邻段 Ø{} 不大于本段 Ø{}）",
                    k + 1,
                    trim(ra * 2.0),
                    trim(rb * 2.0)
                ));
            }
            if (segs[k].s - segs[k].e).abs() > 1e-9 {
                return Err(format!("第 {} 段：左端是锥面，越程槽只能开在圆柱端", k + 1));
            }
            let (groove, row) = detail::groove_entities(segs[k].s, ov.b1)
                .map_err(|e| format!("第 {} 段：左端越程槽：{e}", k + 1))?;
            if row.b1 > segs[k].l + 1e-9 {
                return Err(format!(
                    "第 {} 段：越程槽 b1={} > 段长 l={}",
                    k + 1,
                    trim(row.b1),
                    trim(segs[k].l)
                ));
            }
            let fillet = detail::fillet_tangent_radius(segs[k].s, row);
            if fillet >= top - 1e-9 {
                return Err(format!(
                    "第 {} 段：左端越程槽的 R 圆角切点 {} 不低于台阶面 {}（相邻台阶太小）",
                    k + 1,
                    trim(fillet),
                    trim(top)
                ));
            }
            bottom = fillet;
            face_through_h = Some(fillet);
            own_ov[k][0] = Some(row.b1);
            // `detail.rs` 的磨外圆槽体已不含砂轮细线（同 ov_r 口径），直接平移。
            entities.extend(groove.into_iter().map(|e| translate_x(e, x_face)));
            // 槽底终止 / 斜壁终点两条贯通竖线（槽肩那条 = 上面的段边界线）。
            let rg = segs[k].s / 2.0;
            let yb = rg - row.h;
            through_line(&mut through, x_face + (row.b1 - row.h), yb);
            through_line(&mut through, x_face + row.b1, rg);
            // 剖面线环：圆角 → 槽底 → 斜坡（上半侧，左→右）
            let slope_x1 = x_face + (row.b1 - row.h);
            profile.push(HatchEdge::Arc {
                c: [x_face + row.r, fillet],
                r: row.r,
                start_deg: 180.0,
                end_deg: 270.0,
                ccw: true,
            });
            profile.push(lr_line([x_face + row.r, yb], [slope_x1, yb]));
            profile.push(lr_line([slope_x1, yb], [x_face + row.b1, rg]));
        }

        // ── 段级 RL 退刀槽（GB/T 3 表 2 剖面：台肩面 → R 圆角 → 槽底 g1 → 斜壁 g2）──
        //    槽体画在带槽的那一段上（i 右端 或 k 左端）；邻段必须是更高的台阶。
        let mut segment_relief_left: Option<f64> = None;
        let mut segment_relief_right: Option<f64> = None;
        if let Some(dims) = relief_r {
            if !(rb > ra + 1e-12) {
                return Err(format!(
                    "第 {} 段：右端退刀槽没有台阶面（相邻段 Ø{} 不大于本段 Ø{}）",
                    i + 1,
                    trim(rb * 2.0),
                    trim(ra * 2.0)
                ));
            }
            let tangent = dims.dg / 2.0 + dims.r;
            if tangent >= top - 1e-9 {
                return Err(format!(
                    "第 {} 段：右端退刀槽的 R 圆角切点 {} 不低于台阶面 {}（相邻台阶太小）",
                    i + 1,
                    trim(tangent),
                    trim(top)
                ));
            }
            bottom = tangent;
            face_through_h = Some(tangent);
            own_relief[i][1] = Some(dims.g2);
            entities.extend(
                detail::relief_groove_entities(&dims)
                    .into_iter()
                    .map(|e| mirror_x(e, x_face)),
            );
            // 槽底终止 / 斜壁终点两条贯通竖线（槽肩那条 = 上面的段边界线）。
            let rg = dims.dg / 2.0;
            through_line(&mut through, x_face - dims.g1, rg);
            through_line(&mut through, x_face - dims.g2, ra);
            // 剖面线环：斜壁 → 槽底 → 圆角（上半侧，左→右）
            profile.push(lr_line([x_face - dims.g2, ra], [x_face - dims.g1, rg]));
            profile.push(lr_line([x_face - dims.g1, rg], [x_face - dims.r, rg]));
            profile.push(HatchEdge::Arc {
                c: [x_face - dims.r, tangent],
                r: dims.r,
                start_deg: 270.0,
                end_deg: 360.0,
                ccw: true,
            });
            segment_relief_left = Some(tangent);
        } else if let Some(dims) = relief_l {
            if !(ra > rb + 1e-12) {
                return Err(format!(
                    "第 {} 段：左端退刀槽没有台阶面（相邻段 Ø{} 不大于本段 Ø{}）",
                    k + 1,
                    trim(ra * 2.0),
                    trim(rb * 2.0)
                ));
            }
            let tangent = dims.dg / 2.0 + dims.r;
            if tangent >= top - 1e-9 {
                return Err(format!(
                    "第 {} 段：左端退刀槽的 R 圆角切点 {} 不低于台阶面 {}（相邻台阶太小）",
                    k + 1,
                    trim(tangent),
                    trim(top)
                ));
            }
            bottom = tangent;
            face_through_h = Some(tangent);
            own_relief[k][0] = Some(dims.g2);
            entities.extend(
                detail::relief_groove_entities(&dims)
                    .into_iter()
                    .map(|e| translate_x(e, x_face)),
            );
            // 槽底终止 / 斜壁终点两条贯通竖线（槽肩那条 = 上面的段边界线）。
            let rg = dims.dg / 2.0;
            through_line(&mut through, x_face + dims.g1, rg);
            through_line(&mut through, x_face + dims.g2, rb);
            // 剖面线环：圆角 → 槽底 → 斜壁（上半侧，左→右）
            profile.push(HatchEdge::Arc {
                c: [x_face + dims.r, tangent],
                r: dims.r,
                start_deg: 180.0,
                end_deg: 270.0,
                ccw: true,
            });
            profile.push(lr_line([x_face + dims.r, rg], [x_face + dims.g1, rg]));
            profile.push(lr_line([x_face + dims.g1, rg], [x_face + dims.g2, rb]));
            segment_relief_right = Some(tangent);
        }

        // ── 局部螺纹（TL/RL）右端接台肩：`RL` 时台肩根只画到圆角切点，
        //    端面线 = 「相邻段半径 ↔ 圆角切点」；锥面/螺尾/槽体在段循环里画。 ──
        let relief_face = local_threads[i]
            .filter(|lay| lay.relief.is_some())
            .map(|lay| lay.relief_tangent);
        if let Some(tangent) = relief_face {
            if chamfer_lines.is_some() {
                return Err(format!(
                    "第 {} 段：局部螺纹 TL/RL 的右端不能倒角 CH（相邻段倒角会落在这里）",
                    i + 1
                ));
            }
            let right_surface = if ov_l.is_some() { bottom } else { rb };
            bottom = tangent.min(right_surface);
            top = tangent.max(right_surface);
            face_through_h = Some(tangent);
        }

        // 上半轮廓在端面两侧的实际半径（决定剖面线环竖直段的走向）：
        // 倒角/越程槽会把这侧表面从原始半径上切掉一块。
        let mut left_eff = ra;
        let mut right_eff = rb;
        if let Some((_, face_point)) = chamfer_lines {
            if rb > ra {
                right_eff = face_point[1];
            } else {
                left_eff = face_point[1];
            }
        }
        if ov_r.is_some() {
            left_eff = bottom;
        }
        if ov_l.is_some() {
            right_eff = bottom;
        }
        if let Some(tangent) = segment_relief_left {
            left_eff = tangent;
        }
        if let Some(tangent) = segment_relief_right {
            right_eff = tangent;
        }
        if let Some(tangent) = relief_face {
            left_eff = tangent;
        }
        // 剖面线边界：齿轮端按齿根圆（齿部按不剖）；花键端按小径包络
        // （轴线↔小径两条带，齿部不剖）；普通全螺纹段按小径包络
        // （牙顶/牙底之间的牙型区不剖，参考件 `轴剖视图.dxf` 右视图口径）。
        if let Some(spline) = &segs[i].spline {
            // 花键右端 = 收尾弧终点，已回到大径 → 剖面边界取大径。
            left_eff = spline.major_radius();
        } else if let Some(invol) = &segs[i].invol_spline {
            // 渐开线花键右端：有 de 时收尾弧回到大径；无 de 时端面就是小径线终点。
            left_eff = if invol.de.is_some() {
                invol.major_radius()
            } else {
                invol.minor_radius()
            };
            // 右端自动倒角切到小径以下时，剖面线边界沿倒角斜线走到交点。
            if let Some(c) = own_ch[i][1] {
                if ra - c < invol.minor_radius() - 1e-12 {
                    left_eff = ra - c;
                }
            }
        } else if let Some(gear) = &segs[i].gear {
            left_eff = gear.root_radius();
        } else if let Some(thread) = plain_thread(&segs[i]) {
            left_eff = left_eff.min(thread.minor_radius(segs[i].s));
        }
        if let Some(spline) = &segs[k].spline {
            // 花键左端小径线起点 → 剖面边界取小径（齿部不剖）；
            // 若左端倒角切到小径以下，剖面线沿倒角走到与小径的交点（保持链单调）。
            let chamfer_below = chamfer_land == Some(k)
                && chamfer_lines
                    .map(|(_, face_point)| face_point[1] < spline.minor_radius() - 1e-12)
                    .unwrap_or(false);
            if !chamfer_below {
                right_eff = spline.minor_radius();
            }
        } else if let Some(invol) = &segs[k].invol_spline {
            // 左端倒角（用户 CH 或自动）切到小径以下时，剖面线边界沿倒角斜线走到交点。
            let below = own_ch[k][0]
                .map(|c| rb - c < invol.minor_radius() - 1e-12)
                .unwrap_or(false);
            right_eff = if below {
                rb - own_ch[k][0].unwrap()
            } else {
                invol.minor_radius()
            };
        } else if let Some(gear) = &segs[k].gear {
            right_eff = gear.root_radius();
        } else if let Some(thread) = plain_thread(&segs[k]) {
            right_eff = right_eff.min(thread.minor_radius(segs[k].s));
        }

        // ── 段边界贯通竖线（用户 2026-09-18 更正版口径）：半高 = 该端面两侧
        //    实际轮廓半径的较小者；端面带槽时 = 槽肩（圆角切点）高。
        //    花键段右端不画贯通竖线（模板那里是花键自己的两根细竖线）。──
        if !is_spline_seg(&segs[i]) {
            through_line(
                &mut through,
                x_face,
                face_through_h.unwrap_or(left_eff.min(right_eff)),
            );
        }

        if top - bottom > 1e-9 {
            entities.push(line([x_face, bottom], [x_face, top], LAYER_MAIN));
            entities.push(line([x_face, -bottom], [x_face, -top], LAYER_MAIN));
            // 上半轮廓竖直段：左段实际表面 → 右段实际表面（方向就是环的走向）
            profile.push(HatchEdge::Line {
                a: [x_face, left_eff],
                b: [x_face, right_eff],
            });
        }
        if let Some((contour, face_point)) = chamfer_lines {
            entities.push(line(face_point, contour, LAYER_MAIN));
            entities.push(line(
                [face_point[0], -face_point[1]],
                [contour[0], -contour[1]],
                LAYER_MAIN,
            ));
            // 剖面线边界：倒角落在普通全螺纹段时由小径包络接管；
            // 落在花键段时由花键段的小径包络接管（下面花键块里补倒角段）。
            if chamfer_land.map_or(true, |land| {
                plain_thread(&segs[land]).is_none() && !is_spline_seg(&segs[land])
            }) {
                profile.push(lr_line(face_point, contour));
            }
            // 倒角终点（根）贯通竖线，半高 = 倒角根半径（用户版 x=2 ±14）。
            through_line(&mut through, contour[0], contour[1]);
        }
    }

    // ── 左自由端（第 1 段左端面） ──
    if segs[0].ov.iter().any(|o| o.end == End::L) {
        return Err("第 1 段：左端是自由端，越程槽没有台阶面".into());
    }
    let left_face = if let Some(c) = side_auto_chamfer(
        auto_face_chamfer(&segs[0], End::L),
        segs[0].outer_radius(End::L),
        None,
    ) {
        // 齿轮/渐开线花键段自由端（轴头）：端面可见高 = ra − C（单侧规则：自由端加）。
        let ra = segs[0].outer_radius(End::L);
        let contour = [c, ra];
        let face_point = [0.0, ra - c];
        entities.push(line(face_point, contour, LAYER_MAIN));
        entities.push(line(
            [face_point[0], -face_point[1]],
            [contour[0], -contour[1]],
            LAYER_MAIN,
        ));
        through_line(&mut through, contour[0], contour[1]);
        own_ch[0][0] = Some(c);
        face_point[1]
    } else if let Some(c) = segs[0].ch.iter().find(|c| c.end == End::L).map(|c| c.c) {
        let radius = segs[0].s / 2.0;
        if c >= radius - 1e-12 {
            return Err(format!(
                "第 1 段：左端倒角 C={} ≥ 端面半径（Ø{} 的 {}），端面被吃掉",
                trim(c),
                trim(segs[0].s),
                trim(radius)
            ));
        }
        let (contour, face_point) = chamfer_geom(&segs[0], 0.0, End::L, c);
        if face_point[1] <= 1e-9 {
            return Err(format!(
                "第 1 段：左端倒角 C={} 把端面吃穿了（端面点 {} ≤ 0）",
                trim(c),
                trim(face_point[1])
            ));
        }
        own_ch[0][0] = Some(c);
        entities.push(line(face_point, contour, LAYER_MAIN));
        entities.push(line(
            [face_point[0], -face_point[1]],
            [contour[0], -contour[1]],
            LAYER_MAIN,
        ));
        // 剖面线边界：普通全螺纹段由小径包络接管（与内端面同口径）。
        if plain_thread(&segs[0]).is_none() {
            profile.push(lr_line(face_point, contour));
        }
        // 倒角终点（根）贯通竖线，半高 = 倒角根半径（用户版 x=2 ±14）。
        through_line(&mut through, contour[0], contour[1]);
        face_point[1]
    } else {
        segs[0].outer_radius(End::L)
    };
    entities.push(line([0.0, -left_face], [0.0, left_face], LAYER_MAIN));

    // ── 右自由端（最后一段右端面） ──
    let last = count - 1;
    let x_end = x0s[last] + segs[last].l;
    if segs[last].ov.iter().any(|o| o.end == End::R) {
        return Err(format!(
            "第 {} 段：右端是自由端，越程槽没有台阶面",
            last + 1
        ));
    }
    let relief_tail = local_threads[last]
        .filter(|lay| lay.relief.is_some())
        .map(|lay| lay.relief_tangent);
    let right_face = if let Some(tangent) = relief_tail {
        if segs[last].ch.iter().any(|c| c.end == End::R) {
            return Err(format!("第 {} 段：局部螺纹 TL/RL 的右端不能倒角 CH", last + 1));
        }
        tangent
    } else if let Some(c) = side_auto_chamfer(
        auto_face_chamfer(&segs[last], End::R),
        segs[last].outer_radius(End::R),
        None,
    ) {
        // 齿轮/渐开线花键段自由端：端面可见高 = ra − C（单侧规则：自由端加）。
        let ra = segs[last].outer_radius(End::R);
        let contour = [x_end - c, ra];
        let face_point = [x_end, ra - c];
        entities.push(line(face_point, contour, LAYER_MAIN));
        entities.push(line(
            [face_point[0], -face_point[1]],
            [contour[0], -contour[1]],
            LAYER_MAIN,
        ));
        through_line(&mut through, contour[0], contour[1]);
        own_ch[last][1] = Some(c);
        face_point[1]
    } else if let Some(c) = segs[last]
        .ch
        .iter()
        .find(|c| c.end == End::R)
        .map(|c| c.c)
    {
        let radius = segs[last].e / 2.0;
        if c >= radius - 1e-12 {
            return Err(format!(
                "第 {} 段：右端倒角 C={} ≥ 端面半径（Ø{} 的 {}），端面被吃掉",
                last + 1,
                trim(c),
                trim(segs[last].e),
                trim(radius)
            ));
        }
        let (contour, face_point) = chamfer_geom(&segs[last], x0s[last], End::R, c);
        if face_point[1] <= 1e-9 {
            return Err(format!(
                "第 {} 段：右端倒角 C={} 把端面吃穿了（端面点 {} ≤ 0）",
                last + 1,
                trim(c),
                trim(face_point[1])
            ));
        }
        own_ch[last][1] = Some(c);
        entities.push(line(face_point, contour, LAYER_MAIN));
        entities.push(line(
            [face_point[0], -face_point[1]],
            [contour[0], -contour[1]],
            LAYER_MAIN,
        ));
        // 剖面线边界：普通全螺纹段由小径包络接管（与内端面同口径）。
        if plain_thread(&segs[last]).is_none() {
            profile.push(lr_line(contour, face_point));
        }
        // 倒角终点（根）贯通竖线，半高 = 倒角根半径。
        through_line(&mut through, contour[0], contour[1]);
        face_point[1]
    } else {
        segs[last].outer_radius(End::R)
    };
    entities.push(line([x_end, -right_face], [x_end, right_face], LAYER_MAIN));

    // ── 每段上下轮廓线（两端被倒角/越程槽吃掉多少已定）；
    //    齿轮段 = 齿顶线（轮廓，两端带 C 倒角）+ 分度线；螺纹段另加小径细实线 ──
    for (index, seg) in segs.iter().enumerate() {
        if let Some(gear) = &seg.gear {
            let (x0, x1) = (x0s[index], x0s[index] + seg.l);
            let (r, ra, rf) = (
                gear.pitch_radius(),
                gear.addendum_radius(),
                gear.root_radius(),
            );
            // 齿顶面两端按**已生效的端面自动倒角**缩进（单侧规则：自由端/邻段更小才缩）。
            let (ta, tb) = (
                x0 + own_ch[index][0].unwrap_or(0.0),
                x1 - own_ch[index][1].unwrap_or(0.0),
            );
            if tb - ta > 1e-9 {
                entities.push(line([ta, ra], [tb, ra], LAYER_MAIN));
                entities.push(line([ta, -ra], [tb, -ra], LAYER_MAIN));
            }
            // 分度线（点划线，3中心线层；不受倒角影响）
            entities.push(line([x0, r], [x1, r], LAYER_CENTER));
            entities.push(line([x0, -r], [x1, -r], LAYER_CENTER));
            // 齿根线：与 `gear.rs::section_view()` 一致，只在剖视可见；也是剖面线边界。
            section_lines.push(line([x0, rf], [x1, rf], LAYER_MAIN));
            section_lines.push(line([x0, -rf], [x1, -rf], LAYER_MAIN));
            profile.push(lr_line([x0, rf], [x1, rf]));
            continue;
        }
        // ── 花键段（矩形花键）：小径细线 / 收尾弧 / 两根细竖线；
        //    常规视图落 `2细线层`，剖视改 `1轮廓实线层`；剖面线边界 = 小径线 + 收尾弧。──
        if let Some(spline) = &seg.spline {
            let (x0, x1) = (x0s[index], x0s[index] + seg.l); // x1 = L + l
            let (r, ra) = (spline.minor_radius(), spline.major_radius());
            let xm1 = x0 + spline.len; // 满齿段右端 = 收尾起点
            let l = spline.runout();
            let rh = spline.hob_radius();
            let a_end = spline.runout_end_angle();
            // 大径线：满齿段与收尾段两段（模板 [43]/[44] + [46]/[47] 就在 x=L 断开）。
            let xs = x0 + own_ch[index][0].unwrap_or(0.0);
            if xm1 - xs > 1e-9 {
                entities.push(line([xs, ra], [xm1, ra], LAYER_MAIN));
                entities.push(line([xs, -ra], [xm1, -ra], LAYER_MAIN));
            }
            if x1 > xm1 + 1e-9 {
                entities.push(line([xm1, ra], [x1, ra], LAYER_MAIN));
                entities.push(line([xm1, -ra], [x1, -ra], LAYER_MAIN));
            }
            // 左端倒角（如落在本段）把细线起点内缩到倒角与小径的交点。
            let left_inset = own_ch[index][0]
                .map(|c| (c - (ra - r)).max(0.0))
                .unwrap_or(0.0);
            let xm0 = x0 + left_inset;
            // 左端倒角（落在本段）切到小径以下：剖面线的倒角段只取到与小径的交点。
            if let Some(c) = own_ch[index][0] {
                let face_y = ra - c;
                if face_y < r - 1e-12 && xm0 > x0 + 1e-12 {
                    profile.push(lr_line([x0, face_y], [xm0, r]));
                }
            }
            // 常规视图：小径细线 + 收尾弧（与小径相切、与大径相交）+ 两根细竖线。
            spline_regular.push(line([xm0, r], [xm1, r], LAYER_THIN));
            spline_regular.push(line([xm0, -r], [xm1, -r], LAYER_THIN));
            spline_regular.push(arc([xm1, r + rh], rh, 270.0, 360.0 - a_end, LAYER_THIN));
            spline_regular.push(arc([xm1, -(r + rh)], rh, a_end, 90.0, LAYER_THIN));
            spline_regular.push(line([xm1, -ra], [xm1, ra], LAYER_THIN));
            if index + 1 < count {
                // 后面还有段：收尾终点细竖线（自由端时由右端面线闭合）。
                spline_regular.push(line([xm1 + l, -ra], [xm1 + l, ra], LAYER_THIN));
            }
            // 剖视可见的小径线 / 收尾弧（`1轮廓实线层`）。
            section_lines.push(line([xm0, r], [xm1, r], LAYER_MAIN));
            section_lines.push(line([xm0, -r], [xm1, -r], LAYER_MAIN));
            section_lines.push(arc([xm1, r + rh], rh, 270.0, 360.0 - a_end, LAYER_MAIN));
            section_lines.push(arc([xm1, -(r + rh)], rh, a_end, 90.0, LAYER_MAIN));
            // 剖面线边界：小径线 → 收尾弧（齿部不剖，与模板「只填轴线↔小径」同口径）。
            profile.push(lr_line([xm0, r], [xm1, r]));
            profile.push(HatchEdge::Arc {
                c: [xm1, r + rh],
                r: rh,
                start_deg: 270.0,
                end_deg: 360.0 - a_end,
                ccw: true,
            });
            // 大径线/剖面线都已就位：不进入通用轮廓分支（通用会把大径线画成整段）。
            continue;
        }
        // ── 渐开线花键段（INVOLSPLINE）：小径细线 +（给了 de）收尾弧；端面自动
        //    倒角按单侧规则（自由端/邻段更小）；常规视图落 `2细线层`，剖视改
        //    `1轮廓实线层`；剖面线边界 = 小径线 + 收尾弧。──
        if let Some(invol) = &seg.invol_spline {
            let (x0, x1) = (x0s[index], x0s[index] + seg.l); // x1 = L + l
            let (r, ra) = (invol.minor_radius(), invol.major_radius());
            let xm1 = x0 + invol.len; // 满齿段右端 = 收尾起点
            // 本段两端已生效的端面自动倒角（单侧规则；有 de 时右端不倒 → 0）。
            let c_l = own_ch[index][0].unwrap_or(0.0);
            let c_r = own_ch[index][1].unwrap_or(0.0);
            // 大径线：满齿段；有 de 时再一段收尾段（两端按倒角缩进）。
            let (xs, xe) = (x0 + c_l, x1 - c_r);
            if xm1.min(xe) - xs > 1e-9 {
                entities.push(line([xs, ra], [xm1.min(xe), ra], LAYER_MAIN));
                entities.push(line([xs, -ra], [xm1.min(xe), -ra], LAYER_MAIN));
            }
            if xe > xm1 + 1e-9 {
                entities.push(line([xm1, ra], [xe, ra], LAYER_MAIN));
                entities.push(line([xm1, -ra], [xe, -ra], LAYER_MAIN));
            }
            // 端面倒角把细线/剖面线端点内缩到倒角与小径的交点。
            let left_inset = (c_l - (ra - r)).max(0.0);
            let xm0 = x0 + left_inset;
            let right_inset = (c_r - (ra - r)).max(0.0);
            let xml = xm1 - right_inset;
            if c_l > 1e-9 && ra - c_l < r - 1e-12 && xm0 > x0 + 1e-12 {
                profile.push(lr_line([x0, ra - c_l], [xm0, r]));
            }
            if c_r > 1e-9 && ra - c_r < r - 1e-12 && xml < x1 - 1e-12 {
                profile.push(lr_line([xml, r], [x1, ra - c_r]));
            }
            // 常规视图：小径细线（只到 L；无 de 时就是整段）。
            spline_regular.push(line([xm0, r], [xml, r], LAYER_THIN));
            spline_regular.push(line([xm0, -r], [xml, -r], LAYER_THIN));
            // 剖视可见的小径线 / 收尾弧（`1轮廓实线层`）。
            section_lines.push(line([xm0, r], [xml, r], LAYER_MAIN));
            section_lines.push(line([xm0, -r], [xml, -r], LAYER_MAIN));
            // 剖面线边界：小径线 →（有 de）收尾弧（齿部不剖，同 SPLINE 口径）。
            profile.push(lr_line([xm0, r], [xml, r]));
            if let Some(de) = invol.de {
                let l = invol.runout();
                let rh = de / 2.0;
                let a_end = invol.params.runout_end_angle(de)?;
                spline_regular.push(arc([xm1, r + rh], rh, 270.0, 360.0 - a_end, LAYER_THIN));
                spline_regular.push(arc([xm1, -(r + rh)], rh, a_end, 90.0, LAYER_THIN));
                spline_regular.push(line([xm1, -ra], [xm1, ra], LAYER_THIN));
                if index + 1 < count {
                    // 后面还有段：收尾终点细竖线（自由端时由右端面线闭合）。
                    spline_regular.push(line([xm1 + l, -ra], [xm1 + l, ra], LAYER_THIN));
                }
                section_lines.push(arc([xm1, r + rh], rh, 270.0, 360.0 - a_end, LAYER_MAIN));
                section_lines.push(arc([xm1, -(r + rh)], rh, a_end, 90.0, LAYER_MAIN));
                profile.push(HatchEdge::Arc {
                    c: [xm1, r + rh],
                    r: rh,
                    start_deg: 270.0,
                    end_deg: 360.0 - a_end,
                    ccw: true,
                });
            }
            // 大径线/剖面线都已就位：不进入通用轮廓分支（通用会把大径线画成整段）。
            continue;
        }
        let start_shift = own_ch[index][0]
            .unwrap_or(0.0)
            .max(own_ov[index][0].unwrap_or(0.0))
            .max(own_relief[index][0].unwrap_or(0.0));
        let end_shift = own_ch[index][1]
            .unwrap_or(0.0)
            .max(own_ov[index][1].unwrap_or(0.0))
            .max(own_relief[index][1].unwrap_or(0.0))
            .max(
                local_threads[index]
                    .map(|lay| lay.face - lay.contour_end)
                    .unwrap_or(0.0),
            );
        if start_shift + end_shift > seg.l + 1e-9 {
            return Err(format!(
                "第 {} 段：两端特征重叠（左 {} + 右 {} > 段长 {}）",
                index + 1,
                trim(start_shift),
                trim(end_shift),
                trim(seg.l)
            ));
        }
        let xs = x0s[index] + start_shift;
        let xe = x0s[index] + seg.l - end_shift;
        if xe - xs > 1e-9 {
            let ys = radius_at(seg, x0s[index], xs);
            let ye = radius_at(seg, x0s[index], xe);
            entities.push(line([xs, ys], [xe, ye], LAYER_MAIN));
            entities.push(line([xs, -ys], [xe, -ye], LAYER_MAIN));
            if seg.spline.is_none() && plain_thread(seg).is_none() {
                profile.push(lr_line([xs, ys], [xe, ye]));
            }
        }
        // 普通全螺纹段的剖面线边界：按小径包络（牙顶区不剖），左/右端倒角
        // 深入小径时沿倒角线收进去；与参考件 `轴剖视图.dxf` 右视图口径一致。
        if let Some(thread) = plain_thread(seg) {
            let (x0, x1) = (x0s[index], x0s[index] + seg.l);
            let r = seg.s / 2.0;
            let r1 = thread.minor_radius(seg.s);
            let cut = (r - r1).max(0.0);
            // 左端：倒角（贴本段时）与 r1 的交点；否则从小径线起。
            let mut flat_start = x0;
            if let Some(c) = own_ch[index][0] {
                let face_y = r - c;
                if face_y < r1 - 1e-12 {
                    let x_cross = x0 + (c - cut);
                    profile.push(lr_line([x0, face_y], [x_cross, r1]));
                    flat_start = x_cross;
                }
            } else if own_ov[index][0].is_some() || own_relief[index][0].is_some() {
                // 槽体边界已由端面循环推入；从小径线起（垂直落下去）。
                flat_start = xs;
                profile.push(HatchEdge::Line {
                    a: [xs, r],
                    b: [xs, r1],
                });
            }
            // 右端：倒角（贴本段时）与 r1 的交点；否则到段末。
            let mut flat_end = x1;
            let mut right_piece: Option<([f64; 2], [f64; 2])> = None;
            if let Some(c) = own_ch[index][1] {
                let face_y = r - c;
                if face_y < r1 - 1e-12 {
                    flat_end = x1 - (c - cut);
                    right_piece = Some(([flat_end, r1], [x1, face_y]));
                }
            } else if own_ov[index][1].is_some() || own_relief[index][1].is_some() {
                flat_end = xe;
                profile.push(HatchEdge::Line {
                    a: [xe, r1],
                    b: [xe, r],
                });
            }
            if flat_end - flat_start > 1e-9 {
                profile.push(lr_line([flat_start, r1], [flat_end, r1]));
            }
            if let Some((a, b)) = right_piece {
                profile.push(lr_line(a, b));
            }
        }
        // ── 螺纹段：`TL`/`RL` 局部螺纹（右端台肩），或旧口径整段全螺纹；
        //    小径细实线上下各一条（2细线层），左端按倒角内缩、右端止于螺尾/
        //    退刀槽斜壁；同段端面倒角比小径深时不挑出材料外。 ──
        if let Some(lay) = local_threads[index] {
            if own_ch[index][1].is_some() {
                return Err(format!(
                    "第 {} 段：局部螺纹 TL/RL 的右端不能倒角 CH",
                    index + 1
                ));
            }
            if own_ov[index][1].is_some() {
                return Err(format!(
                    "第 {} 段：局部螺纹 TL/RL 的右端不能有越程槽 OV",
                    index + 1
                ));
            }
            match lay.relief {
                Some(dims) => {
                    // GB/T 3 表 2（图 2）：台肩面 → R 圆角 → 槽底 g1 → 斜壁（g2 接大径）。
                    let rg = dims.dg / 2.0;
                    entities.extend(
                        detail::relief_groove_entities(&dims)
                            .into_iter()
                            .map(|e| mirror_x(e, lay.face)),
                    );
                    // 槽底终止 / 斜壁终点两条贯通竖线（槽肩那条在端面循环按切点画）。
                    through_line(&mut through, lay.face - dims.g1, rg);
                    through_line(&mut through, lay.face - dims.g2, lay.r);
                    // 剖面线环（上半，左→右）：斜壁 → 槽底 → 圆角
                    profile.push(lr_line([lay.face - dims.g2, lay.r], [lay.face - dims.g1, rg]));
                    profile.push(lr_line([lay.face - dims.g1, rg], [lay.face - dims.r, rg]));
                    profile.push(HatchEdge::Arc {
                        c: [lay.face - dims.r, lay.relief_tangent],
                        r: dims.r,
                        start_deg: 270.0,
                        end_deg: 360.0,
                        ccw: true,
                    });
                }
                None => {
                    // GB/T 3 图 1 第一种形式：锥面（粗实线）→ 螺尾（细实线）→ 完整螺纹。
                    if lay.runout_start > lay.boundary + 1e-9 {
                        entities.push(line(
                            [lay.face, lay.r],
                            [lay.runout_start, lay.r1],
                            LAYER_MAIN,
                        ));
                        entities.push(line(
                            [lay.face, -lay.r],
                            [lay.runout_start, -lay.r1],
                            LAYER_MAIN,
                        ));
                    }
                    entities.push(line(
                        [lay.runout_start, lay.r1],
                        [lay.boundary, lay.r],
                        LAYER_THIN,
                    ));
                    entities.push(line(
                        [lay.runout_start, -lay.r1],
                        [lay.boundary, -lay.r],
                        LAYER_THIN,
                    ));
                    // 完整螺纹 / 螺尾分界竖线（图 1(a) 里那条短竖线，粗实线）。
                    entities.push(line([lay.boundary, -lay.r], [lay.boundary, lay.r], LAYER_MAIN));
                }
            }
            // 小径细实线：左端按本段左端倒角/退刀槽内缩，右端止于螺尾起点/斜壁交点。
            let cut = lay.r - lay.r1;
            let mut left_inset = own_ch[index][0]
                .filter(|c| *c > cut + 1e-12)
                .map(|c| c - cut)
                .unwrap_or(0.0);
            if let Some(dims) = relief_dims[index][0] {
                left_inset = left_inset.max(relief_minor_inset(lay.r, lay.r1, &dims));
            }
            let xt0 = (x0s[index] + left_inset).max(lay.full_start);
            let xt1 = lay.minor_end;
            if xt1 - xt0 <= 1e-9 {
                return Err(format!(
                    "第 {} 段：局部螺纹装不下（小径细实线长 ≤ 0；TL/RL 与左端倒角/段长冲突）",
                    index + 1
                ));
            }
            entities.push(line([xt0, lay.r1], [xt1, lay.r1], LAYER_THIN));
            entities.push(line([xt0, -lay.r1], [xt1, -lay.r1], LAYER_THIN));
        } else if let Some(thread) = &seg.thread {
            let (x0, x1) = (x0s[index], x0s[index] + seg.l);
            let minor = thread.minor_radius(seg.s);
            let cut = seg.s / 2.0 - minor;
            let inset = |chamfer: Option<f64>| {
                chamfer
                    .filter(|c| *c > cut + 1e-12)
                    .map(|c| c - cut)
                    .unwrap_or(0.0)
            };
            let xt0 = x0 + {
                let mut left = inset(own_ch[index][0]);
                if let Some(dims) = relief_dims[index][0] {
                    left = left.max(relief_minor_inset(seg.s / 2.0, minor, &dims));
                }
                left
            };
            let xt1 = x1 - inset(own_ch[index][1]);
            if xt1 - xt0 > 1e-9 {
                entities.push(line([xt0, minor], [xt1, minor], LAYER_THIN));
                entities.push(line([xt0, -minor], [xt1, -minor], LAYER_THIN));
            }
        }
    }

    // ── 轴线（3中心线层）：长度 = 总长 + 图框比例 × 6，两端各伸出 3n ──
    let half_overhang = 3.0 * frame_scale;
    entities.push(line(
        [-half_overhang, 0.0],
        [total + half_overhang, 0.0],
        LAYER_CENTER,
    ));

    // ── 剖面线环：上半边界（左→右）在 `build()` 里按 x 排序后拆成上/下两个环 ──

    Ok(Geometry {
        entities,
        through,
        section_lines,
        spline_regular,
        profile,
        total_length: total,
        max_diameter,
        segment_count: count,
    })
}

/// 上半剖面线边界（左→右）按 x 拼接后拆成**上/下两个简单环**：
/// 上环 = 上半边界 → 右端面下到轴线 → 轴线回到左端 → 左端面上到上半边界；
/// 下环 = 上环关于 x 轴的镜像反走。参考件 `轴剖视图.dxf` 右视图：一个 HATCH、2 环。
///
/// 端面的闭合高度直接取上半边界链两端点的 y（含齿轮齿根 / 螺纹小径包络 / 端面倒角）。
fn close_hatch_rings(mut profile: Vec<HatchEdge>, x_end: f64) -> Vec<Vec<HatchEdge>> {
    profile.retain(|e| !edge_is_degenerate(e));
    if profile.is_empty() {
        return Vec::new();
    }
    profile.sort_by(|a, b| {
        let (a0, a1) = edge_span(a);
        let (b0, b1) = edge_span(b);
        a0.partial_cmp(&b0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a1.partial_cmp(&b1).unwrap_or(std::cmp::Ordering::Equal))
    });
    let Some((left, right)) = hatch_chain_endpoints(&profile) else {
        return Vec::new();
    };
    let mut upper = profile;
    upper.push(HatchEdge::Line {
        a: [x_end, right[1]],
        b: [x_end, 0.0],
    });
    upper.push(HatchEdge::Line {
        a: [x_end, 0.0],
        b: [0.0, 0.0],
    });
    upper.push(HatchEdge::Line {
        a: [0.0, 0.0],
        b: [0.0, left[1]],
    });
    let lower: Vec<HatchEdge> = upper.iter().rev().map(mirror_reverse_edge).collect();
    vec![upper, lower]
}

/// 上半剖面线链的最小/最大 x 端点（用于上/下环的端面闭合）。
fn hatch_chain_endpoints(profile: &[HatchEdge]) -> Option<([f64; 2], [f64; 2])> {
    let mut left: Option<[f64; 2]> = None;
    let mut right: Option<[f64; 2]> = None;
    for edge in profile {
        for p in edge_endpoints(edge) {
            if left.map_or(true, |q| p[0] < q[0]) {
                left = Some(p);
            }
            if right.map_or(true, |q| p[0] > q[0]) {
                right = Some(p);
            }
        }
    }
    Some((left?, right?))
}

/// 边界段两端点（弧用起讫点）。
fn edge_endpoints(edge: &HatchEdge) -> [[f64; 2]; 2] {
    match *edge {
        HatchEdge::Line { a, b } => [a, b],
        HatchEdge::Arc {
            c,
            r,
            start_deg,
            end_deg,
            ..
        } => [
            [
                c[0] + r * start_deg.to_radians().cos(),
                c[1] + r * start_deg.to_radians().sin(),
            ],
            [
                c[0] + r * end_deg.to_radians().cos(),
                c[1] + r * end_deg.to_radians().sin(),
            ],
        ],
    }
}

/// 直线段按 x 方向排成左→右（竖直段不要用它，方向要按环的走向手写）。
fn lr_line(a: [f64; 2], b: [f64; 2]) -> HatchEdge {
    if a[0] <= b[0] {
        HatchEdge::Line { a, b }
    } else {
        HatchEdge::Line { a: b, b: a }
    }
}

/// 边界段两端的 x 范围（弧用起讫点；本文件的弧都是 x 单调的四分之一弧）。
fn edge_span(edge: &HatchEdge) -> (f64, f64) {
    let (a, b) = match *edge {
        HatchEdge::Line { a, b } => (a, b),
        HatchEdge::Arc {
            c,
            r,
            start_deg,
            end_deg,
            ..
        } => (
            [
                c[0] + r * start_deg.to_radians().cos(),
                c[1] + r * start_deg.to_radians().sin(),
            ],
            [
                c[0] + r * end_deg.to_radians().cos(),
                c[1] + r * end_deg.to_radians().sin(),
            ],
        ),
    };
    if a[0] <= b[0] {
        (a[0], b[0])
    } else {
        (b[0], a[0])
    }
}

/// 零长边界段（与 `partgen_b4::dedup_ring` 同口径：重复点会让宿主判边界无效）。
fn edge_is_degenerate(edge: &HatchEdge) -> bool {
    match *edge {
        HatchEdge::Line { a, b } => (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
        HatchEdge::Arc {
            r,
            start_deg,
            end_deg,
            ..
        } => r < 1e-9 || (end_deg - start_deg).abs() < 1e-9,
    }
}

/// 关于 x 轴镜像 + 反向（环从上半侧折回下半侧时用）。
///
/// 圆弧推导：镜像是 `θ' = −θ`；再反向一次 ⇒ `start' = −end`、`end' = −start`、
/// `ccw' = ccw`（两次方向翻转相互抵消）。
fn mirror_reverse_edge(edge: &HatchEdge) -> HatchEdge {
    match *edge {
        HatchEdge::Line { a, b } => HatchEdge::Line {
            a: [b[0], -b[1]],
            b: [a[0], -a[1]],
        },
        HatchEdge::Arc {
            c,
            r,
            start_deg,
            end_deg,
            ccw,
        } => HatchEdge::Arc {
            c: [c[0], -c[1]],
            r,
            start_deg: -end_deg,
            end_deg: -start_deg,
            ccw,
        },
    }
}

/// 按 `at` / `rot`（度）放置：绕基点旋转后平移。基点 = **第 1 段轴线左端**。
pub fn place(entities: Vec<EntityType>, at: [f64; 2], rot_deg: f64) -> Vec<EntityType> {
    let rad = rot_deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let map = move |p: [f64; 2]| {
        [
            at[0] + p[0] * cos - p[1] * sin,
            at[1] + p[0] * sin + p[1] * cos,
        ]
    };
    entities
        .into_iter()
        .map(|entity| match entity {
            EntityType::Line(mut l) => {
                let a = map([l.start.x, l.start.y]);
                let b = map([l.end.x, l.end.y]);
                l.start = Vector3::new(a[0], a[1], l.start.z);
                l.end = Vector3::new(b[0], b[1], l.end.z);
                EntityType::Line(l)
            }
            EntityType::Arc(mut a) => {
                let center = map([a.center.x, a.center.y]);
                a.center = Vector3::new(center[0], center[1], a.center.z);
                a.start_angle += rad;
                a.end_angle += rad;
                EntityType::Arc(a)
            }
            EntityType::Hatch(mut h) => {
                // 剖面线边界跟着转/移；图案方向（ANSI31 45°）是材质约定，不随视图旋转。
                for path in &mut h.paths {
                    for edge in &mut path.edges {
                        match edge {
                            BoundaryEdge::Line(e) => {
                                let a = map([e.start.x, e.start.y]);
                                let b = map([e.end.x, e.end.y]);
                                e.start = Vector2::new(a[0], a[1]);
                                e.end = Vector2::new(b[0], b[1]);
                            }
                            BoundaryEdge::CircularArc(e) => {
                                let c = map([e.center.x, e.center.y]);
                                e.center = Vector2::new(c[0], c[1]);
                                e.start_angle += rad;
                                e.end_angle += rad;
                            }
                            BoundaryEdge::EllipticArc(e) => {
                                let c = map([e.center.x, e.center.y]);
                                e.center = Vector2::new(c[0], c[1]);
                                let (vx, vy) = (e.major_axis_endpoint.x, e.major_axis_endpoint.y);
                                e.major_axis_endpoint =
                                    Vector2::new(vx * cos - vy * sin, vx * sin + vy * cos);
                                e.start_angle += rad;
                                e.end_angle += rad;
                            }
                            BoundaryEdge::Polyline(e) => {
                                for v in &mut e.vertices {
                                    let p = map([v.x, v.y]);
                                    v.x = p[0];
                                    v.y = p[1];
                                }
                            }
                            BoundaryEdge::Spline(e) => {
                                for q in &mut e.control_points {
                                    let p = map([q.x, q.y]);
                                    q.x = p[0];
                                    q.y = p[1];
                                }
                                for q in &mut e.fit_points {
                                    let p = map([q.x, q.y]);
                                    q.x = p[0];
                                    q.y = p[1];
                                }
                            }
                        }
                    }
                }
                EntityType::Hatch(h)
            }
            other => other,
        })
        .collect()
}

// ══════════════════════════════════════════════════════════════════════════
// 对外：初始化拦截 / 块名 / SVG 预览
// ══════════════════════════════════════════════════════════════════════════

/// OCSM 初始化检查（判据与 `gear::ocsm_ready` 相同，措辞改成轴）：
/// 图形层在、中心线层挂着点划线；不满足就在插入前拦下，避免中心线变实线白线。
pub fn ocsm_ready(doc: &ocs_plugin_api::host::acadrust::CadDocument) -> Result<(), String> {
    let find = |name: &str| {
        doc.layers
            .iter()
            .find(|ly| ly.name.eq_ignore_ascii_case(name))
    };
    const NEED: [&str; 4] = [LAYER_MAIN, LAYER_THIN, LAYER_CENTER, LAYER_HATCH];
    let missing: Vec<&str> = NEED
        .iter()
        .copied()
        .filter(|n| find(n).is_none())
        .collect();
    let center_ok = find(LAYER_CENTER)
        .map(|ly| ly.line_type.eq_ignore_ascii_case("CENTER2"))
        .unwrap_or(false);
    if missing.is_empty() && center_ok {
        return Ok(());
    }
    let why = if !missing.is_empty() {
        format!("（缺图层：{}）", missing.join("、"))
    } else {
        format!("（{LAYER_CENTER} 没挂 CENTER2 点划线）")
    };
    Err(format!(
        "这张图还没跑过 OCSM 初始化{why} —— 先执行 OCSM（建 10 个图层 + 线型 + 文字/标注样式），\
         再生成轴；不然中心线会是实线白线。"
    ))
}

/// 轴块名：只由几何（段序列 + 视图）决定 —— 同模型复用一个块，不同模型不会误用别人的块。
/// 用 FNV-1a 64 位而不是 `DefaultHasher`：跨进程/跨版本稳定，写进 DWG 的块名可复现。
pub fn block_name(program: &Program) -> String {
    let canonical = format!(
        "{}|{}",
        program.view.key(),
        serde_json::to_string(&program.segments).unwrap_or_default()
    );
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("OCSM_SHAFT_{hash:016X}")
}

/// GUI 预览：轴的侧视图 SVG（图元渲染复用 `gear.rs` 的口径）。
///
/// `gear::svg_of` 的 Hatch 预览只按包围盒裁 45° 线（它那里全是轴对齐矩形环），
/// 轴的剖面环是带台阶/倒角/槽的多边形 → 先在 `preview_entities` 里把 HATCH
/// 换成**裁剪到真实边界**的 45° 线，再交给同一套渲染。导出图纸仍是真 HATCH。
pub fn preview_svg(program: &Program) -> Result<String, String> {
    let shaft = build(program, 1.0)?;
    Ok(crate::gear::svg_of(&preview_entities(&shaft.entities), 460.0))
}

/// **轴段计算书**（纯数据、无 IO）：段清单总览 + 每个 `INVOLSPLINE` 段的完整花键
/// 计算书（复用 [`crate::invol_spline::build_report`]，含 d_B/A 来源与 DIN 检验尺寸）。
/// 命令入口：`OCSMSHAFT … report`（可选 `report=<path>` 写文件）。
pub fn build_report(program: &Program) -> Result<String, String> {
    let built = build(program, 1.0)?;
    let mut md = String::new();
    md.push_str("# 轴段计算书\n\n");
    md.push_str(&format!(
        "- 视图：{}（{}）\n- 段数：{}；总长：{} mm；最大直径：{} mm\n\n",
        program.view.key(),
        program.view.label(),
        built.segment_count,
        trim(built.total_length),
        trim(built.max_diameter)
    ));
    md.push_str("## 1. 段清单\n\n| # | 类型 | 关键参数 | 长度 mm | 外径 mm |\n|---|---|---|---|---|\n");
    for (i, seg) in program.segments.iter().enumerate() {
        let (kind, params) = if let Some(g) = &seg.gear {
            let gp = g.params();
            (
                "齿轮段",
                format!("m={} z={} α={}°", trim(gp.m), gp.z, trim(gp.alpha_deg)),
            )
        } else if let Some(s) = &seg.spline {
            (
                "矩形花键段",
                format!("N={} d={} D={} B={}", s.n, trim(s.d), trim(s.big), trim(s.b)),
            )
        } else if let Some(iv) = &seg.invol_spline {
            (
                "渐开线花键段",
                format!(
                    "{} {} m={} z={} x={}",
                    iv.code,
                    iv.params.std.label(),
                    trim(iv.params.m),
                    iv.params.z,
                    trim(iv.params.x)
                ),
            )
        } else if let Some(t) = &seg.thread {
            (
                "螺纹段",
                format!(
                    "P={}",
                    t.pitch.map(trim).unwrap_or_else(|| "简化0.85d".into())
                ),
            )
        } else {
            (
                "普通段",
                format!("S={} E={}", trim(seg.s), trim(seg.e)),
            )
        };
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            i + 1,
            kind,
            params,
            trim(seg.l),
            trim(seg.outer_radius(End::L) * 2.0)
        ));
    }
    md.push('\n');
    md.push_str("## 2. 渐开线花键段计算书\n\n");
    let mut any = false;
    for seg in program.segments.iter() {
        let Some(iv) = &seg.invol_spline else { continue };
        any = true;
        md.push_str(&crate::invol_spline::build_report(
            &iv.params,
            iv.d_b_origin.as_ref(),
            iv.len,
        ));
        md.push('\n');
    }
    if !any {
        md.push_str("（本程序没有 INVOLSPLINE 段。）\n");
    }
    Ok(md)
}

/// ANSI31 基准线间距（与 `partgen_kit::hatch_ansi31_edges` 的 offset 同源）。
const ANSI31_OFFSET: f64 = 2.245_064_030_267_288;

/// 预览专用：把每个 HATCH 换成其边界内的 45° 图案线（`5剖面线层`）。
fn preview_entities(entities: &[EntityType]) -> Vec<EntityType> {
    let mut out = Vec::with_capacity(entities.len());
    for entity in entities {
        match entity {
            EntityType::Hatch(h) => out.extend(hatch_preview_lines(h)),
            other => out.push(other.clone()),
        }
    }
    out
}

/// 把一个 HATCH 的每条边界环展平成线段（弧按 24 段采样），再用奇偶规则裁成 45° 线。
fn hatch_preview_lines(h: &Hatch) -> Vec<EntityType> {
    let mut out = Vec::new();
    let step = 2.0 * ANSI31_OFFSET * h.pattern_scale.max(0.05);
    for path in &h.paths {
        let mut segs: Vec<([f64; 2], [f64; 2])> = Vec::new();
        for edge in &path.edges {
            match edge {
                BoundaryEdge::Line(e) => {
                    segs.push(([e.start.x, e.start.y], [e.end.x, e.end.y]))
                }
                BoundaryEdge::CircularArc(a) => {
                    let tau = std::f64::consts::TAU;
                    let sweep = if a.counter_clockwise {
                        (a.end_angle - a.start_angle).rem_euclid(tau)
                    } else {
                        -((a.start_angle - a.end_angle).rem_euclid(tau))
                    };
                    let point = |angle: f64| {
                        [
                            a.center.x + a.radius * angle.cos(),
                            a.center.y + a.radius * angle.sin(),
                        ]
                    };
                    let n = 24;
                    let mut prev = point(a.start_angle);
                    for i in 1..=n {
                        let p = point(a.start_angle + sweep * i as f64 / n as f64);
                        segs.push((prev, p));
                        prev = p;
                    }
                }
                BoundaryEdge::Polyline(p) => {
                    let pts: Vec<[f64; 2]> = p.vertices.iter().map(|v| [v.x, v.y]).collect();
                    for i in 0..pts.len().saturating_sub(1) {
                        segs.push((pts[i], pts[i + 1]));
                    }
                    if p.is_closed && pts.len() > 2 {
                        segs.push((pts[pts.len() - 1], pts[0]));
                    }
                }
                _ => {}
            }
        }
        if segs.is_empty() {
            continue;
        }
        let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
        let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (p, q) in &segs {
            for pt in [p, q] {
                x0 = x0.min(pt[0]);
                x1 = x1.max(pt[0]);
                y0 = y0.min(pt[1]);
                y1 = y1.max(pt[1]);
            }
        }
        // 45° 线方向 (1,1)：`x − y = c`；相邻线间距（法向）= step / √2。
        let k0 = ((x0 - y1) / step).floor() as i64 - 1;
        let k1 = ((x1 - y0) / step).ceil() as i64 + 1;
        for k in k0..=k1 {
            let c = k as f64 * step;
            let mut ts: Vec<f64> = Vec::new();
            for &(p, q) in &segs {
                let (cp, cq) = (p[0] - p[1], q[0] - q[1]);
                if (cp <= c) == (cq <= c) || (cp - cq).abs() < 1e-12 {
                    continue;
                }
                let t = (c - cp) / (cq - cp);
                let ix = p[0] + (q[0] - p[0]) * t;
                let iy = p[1] + (q[1] - p[1]) * t;
                ts.push(ix + iy); // 沿 45° 方向的参数
            }
            ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for pair in ts.chunks_exact(2) {
                let (a, b) = (pair[0], pair[1]);
                let p = [(a + c) / 2.0, (a - c) / 2.0];
                let q = [(b + c) / 2.0, (b - c) / 2.0];
                out.push(line(p, q, LAYER_HATCH));
            }
        }
    }
    out
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detail;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn has_line(shaft: &Shaft, a: [f64; 2], b: [f64; 2]) -> bool {
        shaft.entities.iter().any(|e| match e {
            EntityType::Line(l) => {
                let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
                (near(p[0], a[0]) && near(p[1], a[1]) && near(q[0], b[0]) && near(q[1], b[1]))
                    || (near(p[0], b[0]) && near(p[1], b[1]) && near(q[0], a[0]) && near(q[1], a[1]))
            }
            _ => false,
        })
    }

    fn has_arc(shaft: &Shaft, center: [f64; 2], radius: f64, start_deg: f64, end_deg: f64) -> bool {
        shaft.entities.iter().any(|e| match e {
            EntityType::Arc(a) => {
                near(a.center.x, center[0])
                    && near(a.center.y, center[1])
                    && near(a.radius, radius)
                    && near(normalize_deg(a.start_angle.to_degrees()), normalize_deg(start_deg))
                    && near(normalize_deg(a.end_angle.to_degrees()), normalize_deg(end_deg))
            }
            _ => false,
        })
    }

    fn normalize_deg(deg: f64) -> f64 {
        deg.rem_euclid(360.0)
    }

    /// 两点确定的 LINE 所在图层（找不到返回 None）。
    fn layer_of_line(shaft: &Shaft, a: [f64; 2], b: [f64; 2]) -> Option<&str> {
        shaft.entities.iter().find_map(|e| match e {
            EntityType::Line(l) => {
                let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
                let hit = (near(p[0], a[0]) && near(p[1], a[1]) && near(q[0], b[0]) && near(q[1], b[1]))
                    || (near(p[0], b[0]) && near(p[1], b[1]) && near(q[0], a[0]) && near(q[1], a[1]));
                hit.then(|| l.common.layer.as_str())
            }
            _ => None,
        })
    }

    fn layer_of(entity: &EntityType) -> &str {
        entity.common().layer.as_str()
    }

    /// 图元序列 → CSV（列：entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1）。
    /// LINE 用两端点；ARC 用起终点 + 圆心/半径/角度；HATCH 用边界 bbox 一行。
    fn entities_csv(entities: &[EntityType]) -> String {
        let mut csv = String::from("entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1\n");
        for entity in entities {
            match entity {
                EntityType::Line(l) => csv.push_str(&format!(
                    "LINE,{:.6},{:.6},{:.6},{:.6},{},,,,,\n",
                    l.start.x, l.start.y, l.end.x, l.end.y, l.common.layer
                )),
                EntityType::Arc(a) => {
                    let sx = a.center.x + a.radius * a.start_angle.cos();
                    let sy = a.center.y + a.radius * a.start_angle.sin();
                    let ex = a.center.x + a.radius * a.end_angle.cos();
                    let ey = a.center.y + a.radius * a.end_angle.sin();
                    csv.push_str(&format!(
                        "ARC,{:.6},{:.6},{:.6},{:.6},{},{:.6},{:.6},{:.6},{:.6},{:.6}\n",
                        sx,
                        sy,
                        ex,
                        ey,
                        a.common.layer,
                        a.center.x,
                        a.center.y,
                        a.radius,
                        a.start_angle.to_degrees(),
                        a.end_angle.to_degrees()
                    ));
                }
                EntityType::Hatch(h) => {
                    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
                    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
                    for path in &h.paths {
                        for edge in &path.edges {
                            match edge {
                                BoundaryEdge::Line(e) => {
                                    for p in [e.start, e.end] {
                                        x0 = x0.min(p.x);
                                        x1 = x1.max(p.x);
                                        y0 = y0.min(p.y);
                                        y1 = y1.max(p.y);
                                    }
                                }
                                BoundaryEdge::CircularArc(e) => {
                                    x0 = x0.min(e.center.x - e.radius);
                                    x1 = x1.max(e.center.x + e.radius);
                                    y0 = y0.min(e.center.y - e.radius);
                                    y1 = y1.max(e.center.y + e.radius);
                                }
                                _ => {}
                            }
                        }
                    }
                    if !x0.is_finite() {
                        x0 = 0.0;
                        y0 = 0.0;
                        x1 = 0.0;
                        y1 = 0.0;
                    }
                    csv.push_str(&format!(
                        "HATCH,{:.6},{:.6},{:.6},{:.6},{},,,,,\n",
                        x0, y0, x1, y1, h.common.layer
                    ));
                }
                other => panic!("dump 只支持 LINE/ARC/HATCH，得到 {other:?}"),
            }
        }
        csv
    }

    const DEMO: &str = "\
S30 E30 L45 CH2@L
S40 E40 L30 CH2@R OV3
S50 E30 L20
S30 E30 L15 CH2@R
S40 E40 L30 M1.5 TL20
S36 E36 L5
GEAR M3 Z20";

    // ── 解析 ──────────────────────────────────────────────────────────────

    #[test]
    fn dsl_demo_parses() {
        let program = parse_program(DEMO).unwrap();
        assert_eq!(program.segments.len(), 7);
        assert_eq!(program.at, None);
        assert_eq!(program.rot, None);
        assert_eq!(
            (program.segments[0].s, program.segments[0].e, program.segments[0].l),
            (30.0, 30.0, 45.0)
        );
        assert_eq!(
            program.segments[0].ch,
            vec![Chamfer { c: 2.0, end: End::L }]
        );
        assert_eq!(
            program.segments[1].ov,
            vec![Overtravel { b1: Some(3.0), end: End::R }]
        );
        // E 省略 = 圆柱段
        assert_eq!((program.segments[2].s, program.segments[2].e), (50.0, 30.0));
        // M1.5 TL20：局部螺纹，螺距 P=1.5、完整螺纹长 20（第 5 段）
        assert_eq!(
            program.segments[4].thread,
            Some(Thread {
                pitch: Some(1.5),
                tl: Some(20.0),
                ..Thread::default()
            })
        );
        assert!(!program.segments[4].thread.unwrap().relief);
        // 退刀槽 = 一小段小直径轴段（第 6 段 Ø36）
        assert_eq!(program.segments[5].s, 36.0);
        assert_eq!(program.segments[5].thread, None);
        // GEAR M3 Z20：d = m·z = 60；H 省略 = 10m = 30
        let gear = program.segments[6].gear.expect("第 7 段是齿轮段");
        assert_eq!((gear.m, gear.z), (3.0, 20));
        assert_eq!(gear.h, None);
        assert!(near(program.segments[6].s, 60.0));
        assert!(near(program.segments[6].e, 60.0));
        assert!(near(program.segments[6].l, 30.0));
        // 关键字顺序无关 + 大小写不敏感
        let shuffled = parse_program("ch2@l l45 e30 s30").unwrap();
        assert_eq!(shuffled.segments[0], program.segments[0]);
        // OV 不带值 = 查表；OV@L 端别
        let ov = parse_program("S30 E30 L20 OV@L").unwrap();
        assert_eq!(
            ov.segments[0].ov,
            vec![Overtravel { b1: None, end: End::L }]
        );
        // 多段用 | 分隔 + 行尾放置
        let one_line =
            parse_program("S30 E30 L45 CH2@L | S40 E40 L30 OV at 100,50 rot 30").unwrap();
        assert_eq!(one_line.segments.len(), 2);
        assert_eq!(one_line.at, Some([100.0, 50.0]));
        assert_eq!(one_line.rot, Some(30.0));
    }

    #[test]
    fn dsl_errors_report_line_and_word() {
        // 第 2 行 OV b1=0
        let err = parse_program("S30 E30 L45\nS40 E40 L30 OV0").unwrap_err();
        assert!(err.contains("第 2 行"), "{err}");
        assert!(err.contains("OV") && err.contains("b1=0"), "{err}");
        // 尚未支持的关键字（THREAD / Z / H / KEYWAY）→ 明确报「不识别的关键字」
        // + 行号/词，不静默忽略
        for token in ["THREAD", "Z10", "H20", "KEYWAY"] {
            let text = format!("S30 E30 L45\nS40 E40 L30 {token}");
            let err = parse_program(&text).unwrap_err();
            assert!(err.contains("第 2 行"), "{token}: {err}");
            assert!(err.contains(token), "{token}: {err}");
            assert!(err.contains("不识别的关键字"), "{token}: {err}");
        }
        // ES 已取消：报错 + 指路（写法示例）
        for token in ["ES5*3", "ES", "ES2*1@L"] {
            let text = format!("S30 E30 L45\nS40 E40 L12 {token}");
            let err = parse_program(&text).unwrap_err();
            assert!(err.contains("第 2 行"), "{token}: {err}");
            assert!(err.contains("ES` 已取消"), "{token}: {err}");
            assert!(err.contains("S24 E24 L5"), "{token}: {err}");
        }
        // 同一行多段时标注「第 k 段」（GEAR 段缺 M）
        let err = parse_program("S30 E30 L10 | GEAR").unwrap_err();
        assert!(err.contains("第 1 行第 2 段") && err.contains("GEAR"), "{err}");
        // 注释行不影响物理行号
        let err = parse_program("# 注释\nS30 E30 L10\nOV0").unwrap_err();
        assert!(err.contains("第 3 行"), "{err}");
        // 其它语法错误
        assert!(parse_program("S30 E30").unwrap_err().contains("缺少 L"));
        assert!(parse_program("E30 L10").unwrap_err().contains("缺少 S"));
        assert!(parse_program("S30 E30 L10 CH2@X")
            .unwrap_err()
            .contains("端别"));
        assert!(parse_program("S30 E30 L10 S40")
            .unwrap_err()
            .contains("S 重复"));
        assert!(parse_program("S30 E30 L10 OV2 OV3")
            .unwrap_err()
            .contains("OV 在右端重复"));
        assert!(parse_program("S30 E30 L10 at x,1")
            .unwrap_err()
            .contains("at 坐标"));
        assert!(parse_program("S30 E30 L10 rot")
            .unwrap_err()
            .contains("rot"));
        // M 光杆现在是合法的螺纹段标记（不再报不识别的关键字）
        let m = parse_program("S30 E30 L20 M").unwrap();
        assert_eq!(m.segments[0].thread, Some(Thread { pitch: None, ..Thread::default() }));
    }

    #[test]
    fn json_model_matches_dsl() {
        let json = r#"{"segments":[
            {"s":30,"e":30,"l":45,"ch":[{"c":2,"end":"L"}]},
            {"s":40,"e":40,"l":30,"ch":{"c":2,"end":"R"},"ov":{"b1":3,"end":"R"}},
            {"s":50,"e":30,"l":20},
            {"s":30,"e":30,"l":15,"ch":[{"c":2,"end":"R"}]},
            {"s":40,"e":40,"l":30,"thread":{"p":1.5,"tl":20}},
            {"s":36,"e":36,"l":5},
            {"gear":{"m":3,"z":20}}
        ],"at":[10,20],"rot":90}"#;
        let program = parse_program(json).unwrap();
        assert_eq!(program.segments.len(), 7);
        assert_eq!(program.at, Some([10.0, 20.0]));
        assert_eq!(program.rot, Some(90.0));
        assert_eq!(program.segments, parse_program(DEMO).unwrap().segments);
        // `at` 也接受 {"x":…,"y":…}
        let program = parse_program(r#"{"segments":[{"s":30,"l":10}],"at":{"x":1,"y":2}}"#).unwrap();
        assert_eq!(program.at, Some([1.0, 2.0]));
        // `thread` 多种写法：true / 数字 / 对象 / 字符串
        for text in [
            r#"{"segments":[{"s":30,"l":10,"thread":true}]}"#,
            r#"{"segments":[{"s":30,"l":10,"thread":{"p":1.5}}]}"#,
            r#"{"segments":[{"s":30,"l":10,"thread":"M1.5"}]}"#,
        ] {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            let want = if text.contains("true") {
                None
            } else {
                Some(1.5)
            };
            assert_eq!(program.segments[0].thread, Some(Thread { pitch: want, ..Thread::default() }));
        }
        // JSON 里写旧 `es` 字段 → 报取消 + 指路
        let err = parse_program(r#"{"segments":[{"s":30,"l":10,"es":{"b":5,"h":3}}]}"#)
            .unwrap_err();
        assert!(err.contains("`ES` 已取消") && err.contains("S24 E24 L5"), "{err}");
        assert!(parse_program("{not json").unwrap_err().contains("JSON"));
    }

    #[test]
    fn json_round_trip_and_block_name() {
        // 序列化 → 再解析等于原 program（GUI 的 parse/export 走同一条路）。
        let program = parse_program(DEMO).unwrap();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"tl\":20"), "TL 序列化进 JSON：{json}");
        assert!(json.contains("\"pitch\":1.5"), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // 只有螺距的旧形态仍序列化成数字（老 GUI/JSON 兼容）
        let plain = parse_program("S40 E40 L30 M1.5").unwrap();
        let plain_json = serde_json::to_string(&plain).unwrap();
        assert!(plain_json.contains("\"thread\":1.5"), "{plain_json}");
        assert_eq!(parse_program(&plain_json).unwrap(), plain);
        // 局部螺纹 JSON 对象往返（新关键字不丢）
        let rich = r#"{"segments":[{"s":40,"e":40,"l":30,"thread":{"p":1.5,"tl":20,"runout":"short","shoulder":"long"}},{"s":36,"e":36,"l":5}]}"#;
        let program = parse_program(rich).unwrap();
        let thread = program.segments[0].thread.unwrap();
        assert_eq!(thread.tl, Some(20.0));
        assert_eq!(thread.runout, RunoutGrade::Short);
        assert_eq!(thread.shoulder, ShoulderGrade::Long);
        assert_eq!(parse_program(&serde_json::to_string(&program).unwrap()).unwrap(), program);
        // 光杆 M 序列化成 true；再解析回来
        let program = parse_program("S30 E30 L20 M").unwrap();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"thread\":true"), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // 齿轮段的派生 s/e/l 允许回传（= 派生值）；不一致就报错
        let program = parse_program("GEAR M3 Z20").unwrap();
        assert_eq!(parse_program(&serde_json::to_string(&program).unwrap()).unwrap(), program);
        let bad = r#"{"segments":[{"s":999,"gear":{"m":3,"z":20}}]}"#;
        assert!(parse_program(bad).unwrap_err().contains("应等于分度圆"));
        let bad = r#"{"segments":[{"l":999,"gear":{"m":3,"z":20}}]}"#;
        assert!(parse_program(bad).unwrap_err().contains("应等于齿宽"));
        // 块名：只由几何决定（同模型稳定、不同模型不同）
        let a = parse_program("S30 E30 L20 M").unwrap();
        let b = parse_program("s30 l20 e30 m").unwrap();
        let c = parse_program("S30 E30 L21 M").unwrap();
        assert_eq!(block_name(&a), block_name(&b));
        assert_ne!(block_name(&a), block_name(&c));
        assert!(block_name(&a).starts_with("OCSM_SHAFT_"));
    }

    #[test]
    fn preview_svg_renders_and_reports_errors() {
        let svg = preview_svg(&parse_program(DEMO).unwrap()).unwrap();
        assert!(svg.starts_with("<svg") || svg.contains("<svg"), "{svg}");
        assert!(svg.contains("#5aa0ff"), "细线层颜色（螺纹小径/螺尾）");
        assert!(svg.contains("#ff5555"), "中心线层颜色");
        // 剖视：HATCH 在预览里被裁成真实边界内的 45° 线（剖面线层绿色）
        let section = preview_svg(&parse_program("S30 E30 L20 VIEW 剖视").unwrap()).unwrap();
        assert!(section.contains("#3fa13f"), "剖面线颜色");
        // 几何非法同样带「第 N 段」原因
        let err = preview_svg(&parse_program("S30 E40 L10 M").unwrap()).unwrap_err();
        assert!(err.contains("第 1 段") && err.contains("圆柱"), "{err}");
    }

    // ── 段拼接 ────────────────────────────────────────────────────────────

    #[test]
    fn segments_concatenate_total_diameter_and_cone() {
        let program = parse_program("S30 E30 L45\nS40 E30 L20\nS30 L15").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(near(shaft.total_length, 80.0));
        assert!(near(shaft.max_diameter, 40.0));
        assert_eq!(shaft.segment_count, 3);
        // 圆柱段：x=0..45 的水平线 y=±15
        assert!(has_line(&shaft, [0.0, 15.0], [45.0, 15.0]));
        // 圆锥段：x=45..65，Ø40 → Ø30（半径 20 → 15）
        assert!(has_line(&shaft, [45.0, 20.0], [65.0, 15.0]));
        assert!(has_line(&shaft, [45.0, -20.0], [65.0, -15.0]));
        // 台阶面 x=45：15 ↔ 20（上下两条）
        assert!(has_line(&shaft, [45.0, 15.0], [45.0, 20.0]));
        assert!(has_line(&shaft, [45.0, -15.0], [45.0, -20.0]));
        // 圆锥末端 Ø30 与第 3 段 Ø30 相同 → 没有肩面，但段边界改用贯通竖线：x=65 ±15
        assert!(has_line(&shaft, [65.0, -15.0], [65.0, 15.0]));
        assert!(has_line(&shaft, [65.0, 15.0], [80.0, 15.0]));
        assert!(has_line(&shaft, [80.0, -15.0], [80.0, 15.0]));
        // 轴两端外端面
        assert!(has_line(&shaft, [0.0, -15.0], [0.0, 15.0]));
        // 轴线长度 = 总长 + 6n
        assert!(has_line(&shaft, [-3.0, 0.0], [83.0, 0.0]));
    }

    #[test]
    fn chamfer_on_both_ends_of_a_segment() {
        let program = parse_program("S30 E30 L45 CH2@L CH2@R").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        // 左端倒角：端面缩到 ±13，45° 线 (0,13)→(2,15)
        assert!(has_line(&shaft, [0.0, -13.0], [0.0, 13.0]));
        assert!(has_line(&shaft, [0.0, 13.0], [2.0, 15.0]));
        assert!(has_line(&shaft, [0.0, -13.0], [2.0, -15.0]));
        // 右端倒角：端面缩到 ±13，45° 线 (43,15)→(45,13)
        assert!(has_line(&shaft, [45.0, -13.0], [45.0, 13.0]));
        assert!(has_line(&shaft, [43.0, 15.0], [45.0, 13.0]));
        assert!(has_line(&shaft, [43.0, -15.0], [45.0, -13.0]));
        // 轮廓缩短到 2..43
        assert!(has_line(&shaft, [2.0, 15.0], [43.0, 15.0]));
        // 倒角终点（根）贯通竖线（用户 2026-09-18 更正版）：x=2 / x=43 半高 = 15
        assert!(has_line(&shaft, [2.0, -15.0], [2.0, 15.0]));
        assert!(has_line(&shaft, [43.0, -15.0], [43.0, 15.0]));
    }

    #[test]
    fn chamfer_on_a_shoulder_lands_on_the_convex_corner() {
        // 第 1 段 Ø30 → 第 2 段 Ø40；CH2@R 落在第 2 段（大）的左端
        let program = parse_program("S30 E30 L20 CH2@R\nS40 E40 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        // 端面 x=20 从 15 到 18（20−2）；倒角线 (20,18)→(22,20)
        assert!(has_line(&shaft, [20.0, 15.0], [20.0, 18.0]));
        assert!(has_line(&shaft, [20.0, 18.0], [22.0, 20.0]));
        // 段边界贯通竖线：x=20、半高 = min(15, 18) = 15
        assert!(has_line(&shaft, [20.0, -15.0], [20.0, 15.0]));
        // 倒角终点（根）贯通竖线：x=22、半高 = 倒角根半径 20
        assert!(has_line(&shaft, [22.0, -20.0], [22.0, 20.0]));
        // 第 2 段轮廓从 x=22 开始
        assert!(has_line(&shaft, [22.0, 20.0], [30.0, 20.0]));
        // 等直径分界也画贯通竖线（用户 2026-09-18 更正版：x=88 那种同径分界也有）
        let flat = build(&parse_program("S30 E30 L20\nS30 E30 L10").unwrap(), 1.0).unwrap();
        assert!(has_line(&flat, [20.0, -15.0], [20.0, 15.0]));
    }

    // ── 越程槽 ────────────────────────────────────────────────────────────

    #[test]
    fn overtravel_groove_keeps_detail_geometry_on_both_ends() {
        // 右端槽：第 1 段 Ø50（地面）→ 第 2 段 Ø60（台阶）；OV 默认右端 + 查表
        let program = parse_program("S50 E50 L20 OV\nS60 E60 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let row = detail::row_for(50.0, None).unwrap();
        let (b1, h, r) = (row.b1, row.h, row.r);
        let fillet = 25.0 + r - h;
        // 右端面 x=20：槽向下镜像，圆角心 (20−r, fillet)，角度 270→360
        assert!(has_arc(&shaft, [20.0 - r, fillet], r, 270.0, 360.0));
        assert!(has_arc(&shaft, [20.0 - r, -fillet], r, 0.0, 90.0));
        // 槽底 + 45° 斜坡
        assert!(has_line(&shaft, [20.0 - b1 + h, 25.0 - h], [20.0 - r, 25.0 - h]));
        assert!(has_line(&shaft, [20.0 - b1, 25.0], [20.0 - b1 + h, 25.0 - h]));
        // 端面线从圆角切点画到台阶顶
        assert!(has_line(&shaft, [20.0, fillet], [20.0, 30.0]));
        // 轮廓从 x=0 到槽的斜坡外侧（20−b1），不是到端面
        assert!(has_line(&shaft, [0.0, 25.0], [20.0 - b1, 25.0]));
        // 槽的三条贯通竖线（用户 2026-09-18 更正版）：槽肩（也是段边界线）/
        // 槽底终止 / 斜壁终点，全落 1轮廓实线层。
        assert_eq!(
            layer_of_line(&shaft, [20.0, -fillet], [20.0, fillet]),
            Some(LAYER_MAIN),
            "槽肩贯通竖线"
        );
        assert_eq!(
            layer_of_line(
                &shaft,
                [20.0 - b1 + h, -(25.0 - h)],
                [20.0 - b1 + h, 25.0 - h]
            ),
            Some(LAYER_MAIN),
            "槽底终止贯通竖线"
        );
        assert_eq!(
            layer_of_line(&shaft, [20.0 - b1, -25.0], [20.0 - b1, 25.0]),
            Some(LAYER_MAIN),
            "斜壁终点贯通竖线"
        );
        // 本例无 M 段 → 整图 0 条 2细线层（只针对本用例；带螺纹的轴会有小径/螺尾线，
        // 不是「OV 一概 0 条」的通用断言）。
        let thin = shaft.entities.iter().filter(|e| layer_of(e) == LAYER_THIN).count();
        assert_eq!(thin, 0, "本用例无 M 段，2细线层 应为 0 条（非通用断言）");
        // 旧砂轮细线的 45° 尾线（模板尾长 15）不应再出现
        assert!(!has_line(
            &shaft,
            [20.0 - b1, 25.0],
            [20.0 - b1 - 15.0, 25.0 + 15.0]
        ));
        // OV@L：地面 = 第 2 段，台阶 = 第 1 段
        let program = parse_program("S60 E60 L10\nS50 E50 L20 OV@L").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let row = detail::row_for(50.0, None).unwrap();
        let (b1, h, r) = (row.b1, row.h, row.r);
        let fillet = 25.0 + r - h;
        assert!(has_arc(&shaft, [10.0 + r, fillet], r, 180.0, 270.0));
        assert!(has_line(&shaft, [10.0, fillet], [10.0, 30.0]));
        assert!(has_line(&shaft, [10.0 + b1, 25.0], [30.0, 25.0]));
        // 镜像到右端的三条贯通竖线 + 没有砂轮细线
        assert!(has_line(&shaft, [10.0, -fillet], [10.0, fillet]));
        assert!(has_line(
            &shaft,
            [10.0 + b1 - h, -(25.0 - h)],
            [10.0 + b1 - h, 25.0 - h]
        ));
        assert!(has_line(&shaft, [10.0 + b1, -25.0], [10.0 + b1, 25.0]));
        assert!(!has_line(
            &shaft,
            [10.0 + b1, 25.0],
            [10.0 + b1 + 15.0, 25.0 + 15.0]
        ));
        let thin = shaft.entities.iter().filter(|e| layer_of(e) == LAYER_THIN).count();
        assert_eq!(thin, 0, "本用例无 M 段，OV@L 也 0 条（非通用断言）");
    }

    // ── 螺纹段 M ─────────────────────────────────────────────────────────

    #[test]
    fn dsl_thread_parses_and_reports_errors() {
        // 光杆 M = 简化画法；M1.5 / M=1.5 = 螺距 P
        let program = parse_program("S30 E30 L20 M").unwrap();
        assert_eq!(program.segments[0].thread, Some(Thread { pitch: None, ..Thread::default() }));
        let program = parse_program("S36 E36 L5 M1.5").unwrap();
        assert_eq!(
            program.segments[0].thread,
            Some(Thread { pitch: Some(1.5), ..Thread::default() })
        );
        let program = parse_program("S36 E36 L5 M=1.5").unwrap();
        assert_eq!(
            program.segments[0].thread,
            Some(Thread { pitch: Some(1.5), ..Thread::default() })
        );
        // 可与 CH 同段（螺纹端倒角是常规画法）、段内顺序无关
        let program = parse_program("S30 E30 L20 CH2@R M1.5").unwrap();
        assert_eq!(program.segments[0].ch, vec![Chamfer { c: 2.0, end: End::R }]);
        assert_eq!(
            program.segments[0].thread,
            Some(Thread { pitch: Some(1.5), ..Thread::default() })
        );
        // 解析错误（行号 + 原因）
        let cases: &[(&str, &str)] = &[
            ("S30 E30 L20 M0", "螺距 P=0"),
            ("S30 E30 L20 M-1.5", "螺距 P=-1.5"),
            ("S30 E30 L20 M1.5 M2", "M 重复"),
            ("S30 E30 L20 M OV3", "不能与越程槽"),
            ("GEAR M3 Z20 M1.5", "M 重复"),
            ("M1.5 GEAR M3 Z20", "M 重复"),
        ];
        for (text, needle) in cases {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    #[test]
    fn thread_geometry_minor_lines_and_termination() {
        // 自由端 M1.5：d1 = 30 − 1.0825×1.5 = 28.37625，半径 14.188125
        let program = parse_program("S30 E30 L20 M1.5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (30.0 - 1.0825 * 1.5) / 2.0;
        assert!(near(r1, 14.188125));
        // 大径轮廓还是 ±15；小径细实线贯穿该段，落 2细线层
        assert!(has_line(&shaft, [0.0, 15.0], [20.0, 15.0]));
        assert!(has_line(&shaft, [0.0, r1], [20.0, r1]));
        assert!(has_line(&shaft, [0.0, -r1], [20.0, -r1]));
        let thin: Vec<&str> = shaft
            .entities
            .iter()
            .filter(|e| layer_of(e) == LAYER_THIN)
            .map(layer_of)
            .collect();
        assert_eq!(thin.len(), 2, "小径细实线上下各一条");
        // 小径细实线止于段末终止线（此处 = 自由端端面）
        assert!(has_line(&shaft, [20.0, -15.0], [20.0, 15.0]));
        // 光杆 M：0.85d = 25.5，半径 12.75
        let shaft = build(&parse_program("S30 E30 L20 M").unwrap(), 1.0).unwrap();
        assert!(has_line(&shaft, [0.0, 12.75], [20.0, 12.75]));
        // 螺纹段 + 小直径退刀槽段：终止线 = 既有台阶面（18 ↔ 20）
        let program = parse_program("S40 E40 L7 M1.5\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (40.0 - 1.0825 * 1.5) / 2.0;
        assert!(near(r1, 19.188125));
        assert!(has_line(&shaft, [0.0, r1], [7.0, r1]));
        assert!(
            has_line(&shaft, [7.0, 18.0], [7.0, 20.0]),
            "终止线（既有台阶面）从退刀槽底 18 到螺纹大径 20"
        );
        // 同段端面有深倒角（伸进小径）→ 细实线止于倒角斜线交点，不挑出材料外
        let program = parse_program("S30 E30 L20 M1.5 CH2@R").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (30.0 - 1.0825 * 1.5) / 2.0;
        let cut = 15.0 - r1;
        assert!(has_line(&shaft, [0.0, r1], [20.0 - (2.0 - cut), r1]));
        assert!(has_line(&shaft, [18.0, 15.0], [20.0, 13.0]), "倒角线还在");
        // 浅倒角（切不到小径）→ 细实线仍到端面
        let program = parse_program("S30 E30 L20 M5 CH2@R").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (30.0 - 1.0825 * 5.0) / 2.0; // 12.29375；cut = 2.70625 > c=2
        assert!(has_line(&shaft, [0.0, r1], [20.0, r1]));
    }

    #[test]
    fn thread_validity_errors_point_at_the_segment() {
        let cases: &[(&str, &str)] = &[
            ("S30 E40 L20 M1.5", "必须是圆柱"),
            ("S30 E30 L20 M30", "太大"),
        ];
        for (text, needle) in cases {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: 解析失败 {e}"));
            let err = build(&program, 1.0).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // 手搓模型（绕过 parser）：M 与 OV 同段也不能通过 validate
        let segment = Segment {
            s: 30.0,
            e: 30.0,
            l: 20.0,
            ch: Vec::new(),
            ov: vec![Overtravel { b1: Some(3.0), end: End::R }],
            relief: Vec::new(),
            thread: Some(Thread { pitch: Some(1.5), ..Thread::default() }),
            gear: None,
            spline: None,
            invol_spline: None,
        };
        let err = validate(&Program {
            segments: vec![segment],
            at: None,
            rot: None,
            view: ShaftView::Normal,
        })
        .unwrap_err();
        assert!(err.contains("不能与越程槽"), "{err}");
    }

    // ── 局部螺纹 M + TL/RO/SD/RL（GB/T 3-1997 图 1 / 表 1 / 表 2） ────────

    #[test]
    fn thread_tables_lookup_and_pitch_errors() {
        // **表归一**：表 1/表 2 只在 `detail.rs` 一份；本测试验证同一份数据在
        // 「直接取表（Option）」与「轴生成器的报错口径（Result）」两条路上一致。
        // 表 1 抽档（P=0.2 起；含订正的 P=1.75 x一般=4.3）
        let row = detail::runout_row_checked(1.5).unwrap();
        assert!(near(row.x_normal, 3.8) && near(row.x_short, 1.9));
        assert!(near(row.a_normal, 4.5) && near(row.a_long, 6.0) && near(row.a_short, 3.0));
        let row = detail::runout_row_checked(1.75).unwrap();
        assert!(near(row.x_normal, 4.3), "P=1.75 x一般 = 4.3（订正网页笔误 1.3）");
        let row = detail::runout_row_checked(0.2).unwrap();
        assert!(near(row.x_normal, 0.5) && near(row.a_normal, 0.6));
        let row = detail::runout_row_checked(6.0).unwrap();
        assert!(near(row.x_normal, 15.0) && near(row.a_long, 24.0));
        // 表 2 抽档（无 P=0.2）
        let row = detail::thread_relief_row(1.5).unwrap();
        assert!(
            near(row.g1, 2.5)
                && near(row.g2, 4.5)
                && near(row.dg_reduction, 2.3)
                && near(row.r, 0.8)
        );
        let row = detail::thread_relief_row(0.25).unwrap();
        assert!(
            near(row.g1, 0.4)
                && near(row.g2, 0.75)
                && near(row.dg_reduction, 0.4)
                && near(row.r, 0.12)
        );
        let row = detail::thread_relief_row(6.0).unwrap();
        assert!(
            near(row.g1, 11.0)
                && near(row.g2, 18.0)
                && near(row.dg_reduction, 8.3)
                && near(row.r, 3.2)
        );
        // P 不在表里：表 1（1.6）；表 2（0.2 表 2 没有）
        let err = detail::runout_row_checked(1.6).unwrap_err();
        assert!(err.contains("表 1") && err.contains("P=1.6"), "{err}");
        let err = detail::thread_relief_row(0.2).unwrap_err();
        assert!(
            err.contains("表 2") && err.contains("0.2") && err.contains("0.25"),
            "{err}"
        );
        // 表归一后两条取表路径必须回同一行（表 1 Option ↔ Result；表 2 一一对应）
        for row in detail::RUNOUT_ROWS {
            let again = detail::runout_row_checked(row.p).unwrap();
            assert_eq!(again, row, "P={} 表 1 两条路取表不一致", trim(row.p));
            assert_eq!(detail::runout_row(row.p), Some(row));
        }
        for row in detail::THREAD_RELIEF_ROWS {
            let again = detail::thread_relief_row(row.p).unwrap();
            assert_eq!(again, row, "P={} 表 2 取表不一致", trim(row.p));
            // 表行的 dims（）与段级显式尺寸路径回同一几何（dg 绝对值口径）
            let dims = row.dims(40.0);
            let explicit = detail::relief_dims_explicit(
                40.0,
                row.g1,
                row.g2,
                40.0 - row.dg_reduction,
                row.r,
            )
            .unwrap();
            assert_eq!(dims.dg, explicit.dg);
            assert_eq!(dims.g1, explicit.g1);
            assert_eq!(dims.g2, explicit.g2);
            assert_eq!(dims.r, explicit.r);
        }
        // 斜壁 30° 自检：任务书的 `g2 = g1 + ((d−dg)/2)/tan30°` 恒等式只有
        // g1/g2 同步取整时才精确；表 2 各自圆整后反算角 28.30°…33.69°，
        // 故按偏离 30° ≤4° 立断言（具体偏差见报告）。
        let mut worst = 0.0_f64;
        for row in detail::THREAD_RELIEF_ROWS {
            let angle = ((row.dg_reduction / 2.0) / (row.g2 - row.g1)).atan().to_degrees();
            worst = worst.max((angle - 30.0).abs());
            assert!(
                (angle - 30.0).abs() <= 4.0,
                "P={} 反算斜壁角 {angle:.2}° 偏离 30° 超过 4°",
                trim(row.p)
            );
        }
        assert!(worst > 0.0, "表值独立取整会带来非零偏差");
    }

    #[test]
    fn dsl_local_thread_parses_with_grades_and_errors() {
        // 基本：M1.5 TL20
        let program = parse_program("S40 E40 L30 M1.5 TL20").unwrap();
        let thread = program.segments[0].thread.unwrap();
        assert_eq!(thread.pitch, Some(1.5));
        assert_eq!(thread.tl, Some(20.0));
        assert_eq!(thread.runout, RunoutGrade::Normal);
        assert_eq!(thread.shoulder, ShoulderGrade::Normal);
        assert!(!thread.relief);
        // 段内顺序无关 + 档位 + `=` 写法
        let program = parse_program("S40 E40 L30 RO短 TL20 M1.5 SD长").unwrap();
        let thread = program.segments[0].thread.unwrap();
        assert_eq!(thread.tl, Some(20.0));
        assert_eq!(thread.runout, RunoutGrade::Short);
        assert_eq!(thread.shoulder, ShoulderGrade::Long);
        // RL；英文档位 / RO 光杆 = 一般
        let program = parse_program("S40 E40 L30 M1.5 TL20 RL").unwrap();
        assert!(program.segments[0].thread.unwrap().relief);
        let program = parse_program("S40 E40 L30 M1.5 TL20 ROshort SDnormal").unwrap();
        let thread = program.segments[0].thread.unwrap();
        assert_eq!(thread.runout, RunoutGrade::Short);
        assert_eq!(thread.shoulder, ShoulderGrade::Normal);
        let program = parse_program("S40 E40 L30 M1.5 TL20 RO SD:短").unwrap();
        let thread = program.segments[0].thread.unwrap();
        assert_eq!(thread.runout, RunoutGrade::Normal);
        assert_eq!(thread.shoulder, ShoulderGrade::Short);
        // 错误：非法档位 / 缺值 / 重复 / 互斥 / 没 M / 齿轮段 / RL 带值 / rot 不吞
        let cases: &[(&str, &str)] = &[
            ("S40 E40 L30 M1.5 TL20 RO长", "RO"),
            ("S40 E40 L30 M1.5 TL20 SD超", "SD"),
            ("S40 E40 L30 M1.5 TL", "TL"),
            ("S40 E40 L30 M1.5 TL0", "TL=0"),
            ("S40 E40 L30 M1.5 TL20 TL15", "TL 重复"),
            ("S40 E40 L30 M1.5 TL20 RO短 RO一般", "RO 重复"),
            ("S40 E40 L30 M1.5 TL20 SD长 SD短", "SD 重复"),
            ("S40 E40 L30 M1.5 TL20 RL RL", "RL 重复"),
            ("S40 E40 L30 M1.5 TL20 RL RO短", "互斥"),
            ("S40 E40 L30 M1.5 TL20 RL SD长", "互斥"),
            ("S40 E40 L30 M1.5 TL20 RL=1", "不带值"),
            ("S40 E40 L30 TL20", "需要与 M 同段"),
            ("S40 E40 L30 RO短", "需要与 M 同段"),
            ("S40 E40 L30 M1.5 TL20 RLx", "不识别的关键字"),
            ("S40 E40 L30 M1.5 TL20 SD=2", "非法"),
            ("GEAR M3 Z20 TL10", "齿轮段"),
            ("S40 E40 L30 M1.5 TL20 rot", "rot"),
        ];
        for (text, needle) in cases {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    #[test]
    fn thread_local_runout_geometry_and_layers() {
        // P=1.5 默认档：x=3.8 / a=4.5；d1 = 40 − 1.0825×1.5 = 38.37625（r1=19.188125）
        // 布局：face=30、boundary=25.5、runout_start=29.3、full_start=5.5
        let program = parse_program("S40 E40 L30 M1.5 TL20\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (40.0 - 1.0825 * 1.5) / 2.0;
        // 大径轮廓照旧贯穿整段（图 1 口径：大径线不断）
        assert!(has_line(&shaft, [0.0, 20.0], [30.0, 20.0]));
        // 锥面过渡（粗实线）：(30,20) → (29.3,r1)
        assert_eq!(
            layer_of_line(&shaft, [30.0, 20.0], [29.3, r1]),
            Some(LAYER_MAIN),
            "锥面 → 1轮廓实线层"
        );
        // 螺尾（细实线）：(29.3,r1) → (25.5,20)，上下对称
        assert_eq!(
            layer_of_line(&shaft, [29.3, r1], [25.5, 20.0]),
            Some(LAYER_THIN),
            "螺尾 → 2细线层"
        );
        assert!(has_line(&shaft, [29.3, -r1], [25.5, -20.0]));
        // 完整螺纹 / 螺尾分界竖线（粗实线，全高）
        assert_eq!(
            layer_of_line(&shaft, [25.5, -20.0], [25.5, 20.0]),
            Some(LAYER_MAIN),
            "分界竖线 → 1轮廓实线层"
        );
        // 小径细实线：5.5 → 29.3（止于螺尾起点）
        assert!(has_line(&shaft, [5.5, r1], [29.3, r1]));
        assert!(has_line(&shaft, [5.5, -r1], [29.3, -r1]));
        let thin = shaft.entities.iter().filter(|e| layer_of(e) == LAYER_THIN).count();
        assert_eq!(thin, 4, "螺尾上下 2 + 小径上下 2");
        // 台肩面仍在 30：18 ↔ 20
        assert!(has_line(&shaft, [30.0, 18.0], [30.0, 20.0]));
        // 档位效果：RO短 x=1.9、SD长 a=6 → 锥面 30→25.9、螺尾 25.9→24、细实线 4→25.9
        let program = parse_program("S40 E40 L30 M1.5 TL20 RO短 SD长\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_line(&shaft, [30.0, 20.0], [25.9, r1]));
        assert!(has_line(&shaft, [25.9, r1], [24.0, 20.0]));
        assert!(has_line(&shaft, [4.0, r1], [25.9, r1]));
        // RO短 + SD短：x=1.9、a=3 → 锥面长 a−x=1.1（30→28.9）
        let program = parse_program("S40 E40 L30 M1.5 TL20 RO短 SD短\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_line(&shaft, [30.0, 20.0], [28.9, r1]));
        assert!(has_line(&shaft, [28.9, r1], [27.0, 20.0]));
        assert!(has_line(&shaft, [7.0, r1], [28.9, r1]));
        // 不给 TL = 旧口径回归：整段小径细实线、没有锥面/螺尾/分界线
        let program = parse_program("S40 E40 L30 M1.5\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_line(&shaft, [0.0, r1], [30.0, r1]));
        assert!(has_line(&shaft, [0.0, -r1], [30.0, -r1]));
        assert!(!has_line(&shaft, [30.0, 20.0], [29.3, r1]), "旧口径没有锥面");
        let thin = shaft.entities.iter().filter(|e| layer_of(e) == LAYER_THIN).count();
        assert_eq!(thin, 2, "旧口径只有小径细实线上下各一条");
    }

    #[test]
    fn thread_local_relief_geometry_and_mutual_exclusion() {
        // P=1.5 表 2：g1=2.5 / g2=4.5 / 减量=2.3 / r=0.8；d=40 → dg=37.7、rg=18.85；
        // 台肩圆角切点 = 18.85+0.8 = 19.65；face=30、完整螺纹 5.5→25.5
        let program = parse_program("S40 E40 L30 M1.5 TL20 RL\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (40.0 - 1.0825 * 1.5) / 2.0;
        // 大径轮廓止于 g2 处：0 → 25.5
        assert!(has_line(&shaft, [0.0, 20.0], [25.5, 20.0]));
        // R 圆角（粗实线）：心 (29.2,19.65)、270°→360°
        assert!(has_arc(&shaft, [29.2, 19.65], 0.8, 270.0, 360.0));
        assert!(has_arc(&shaft, [29.2, -19.65], 0.8, 0.0, 90.0));
        // 槽底 g1：29.2 → 27.5（y=18.85）；斜壁：27.5,18.85 → 25.5,20
        assert!(has_line(&shaft, [29.2, 18.85], [27.5, 18.85]));
        assert!(has_line(&shaft, [27.5, 18.85], [25.5, 20.0]));
        assert!(has_line(&shaft, [27.5, -18.85], [25.5, -20.0]));
        // 台肩面只画到圆角切点：18 → 19.65（不像螺尾那里画满 20）
        assert!(has_line(&shaft, [30.0, 18.0], [30.0, 19.65]));
        assert!(!has_line(&shaft, [30.0, 18.0], [30.0, 20.0]), "RL 槽根不留到 20");
        // 三条贯通竖线：槽肩（切线高 19.65）/ 槽底终止（27.5）/ 斜壁终点（25.5）
        assert!(has_line(&shaft, [30.0, -19.65], [30.0, 19.65]), "槽肩贯通竖线");
        assert!(has_line(&shaft, [27.5, -18.85], [27.5, 18.85]), "槽底终止贯通竖线");
        assert!(has_line(&shaft, [25.5, -20.0], [25.5, 20.0]), "斜壁终点贯通竖线");
        // 小径细实线止于斜壁交点：5.5 → 26.911957…（y=r1）
        let x_minor = 30.0 - 2.5 - 2.0 * (r1 - 18.85) / (20.0 - 18.85);
        assert!(has_line(&shaft, [5.5, r1], [x_minor, r1]));
        assert!(has_line(&shaft, [5.5, -r1], [x_minor, -r1]));
        // RL 不画收尾/锥面/分界竖线：细实线只有小径上下两条
        assert!(!has_line(&shaft, [29.3, r1], [25.5, 20.0]), "RL 不画螺尾斜线");
        let thin = shaft.entities.iter().filter(|e| layer_of(e) == LAYER_THIN).count();
        assert_eq!(thin, 2, "RL 只有小径细实线上下各一条");
        // RL 不给 TL 也支持：整段圆柱上直接开槽，小径细实线从左端到斜壁交点
        let program = parse_program("S40 E40 L30 M1.5 RL\nS36 E36 L5").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_line(&shaft, [0.0, 20.0], [25.5, 20.0]));
        assert!(has_line(&shaft, [0.0, r1], [x_minor, r1]));
        assert!(has_line(&shaft, [0.0, -r1], [x_minor, -r1]));
        // 段长吃不下：g2 + TL = 24.5 > 14
        let program = parse_program("S40 E40 L14 M1.5 TL20 RL\nS36 E36 L5").unwrap();
        let err = build(&program, 1.0).unwrap_err();
        assert!(err.contains("第 1 段") && err.contains("超过段长"), "{err}");
        // 互斥（JSON 路径也要拦）
        let bad = r#"{"segments":[{"s":40,"l":30,"thread":{"p":1.5,"tl":20,"relief":true,"runout":"short"}}]}"#;
        let program = parse_program(bad).unwrap();
        let err = build(&program, 1.0).unwrap_err();
        assert!(err.contains("第 1 段") && err.contains("互斥"), "{err}");
        // 表 2 没有 P=0.2
        let program = parse_program("S40 E40 L30 M0.2 TL20 RL\nS36 E36 L5").unwrap();
        let err = build(&program, 1.0).unwrap_err();
        assert!(err.contains("第 1 段") && err.contains("表 2"), "{err}");
        // 剖视：RL 槽体也进剖面环（一个 HATCH、简单环、边界首尾相接）
        let program =
            parse_program("S40 E40 L30 M1.5 TL20 RL\nS36 E36 L5 VIEW 剖视").unwrap();
        let section = build(&program, 1.0).unwrap();
        let hatch = section
            .entities
            .iter()
            .find_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .expect("剖视应有一个 HATCH");
        assert_eq!(hatch.paths.len(), 2, "轴线上/下两个环（参考件口径）");
        let pts = |edge: &BoundaryEdge| -> ([f64; 2], [f64; 2]) {
            match edge {
                BoundaryEdge::Line(e) => ([e.start.x, e.start.y], [e.end.x, e.end.y]),
                BoundaryEdge::CircularArc(a) => (
                    [
                        a.center.x + a.radius * a.start_angle.cos(),
                        a.center.y + a.radius * a.start_angle.sin(),
                    ],
                    [
                        a.center.x + a.radius * a.end_angle.cos(),
                        a.center.y + a.radius * a.end_angle.sin(),
                    ],
                ),
                other => panic!("剖面环只应有 LINE/ARC，得到 {other:?}"),
            }
        };
        for path in &hatch.paths {
            let mut prev: Option<[f64; 2]> = None;
            for edge in &path.edges {
                let (a, b) = pts(edge);
                if let Some(p) = prev {
                    assert!(
                        near(p[0], a[0]) && near(p[1], a[1]),
                        "剖面环断开：{p:?} → {a:?}"
                    );
                }
                prev = Some(b);
            }
            let last = prev.unwrap();
            assert!(
                near(last[0], 0.0) && near(last[1].abs(), 20.0),
                "环回到左端面 (0,±20)：{last:?}"
            );
        }
    }

    #[test]
    fn thread_local_fit_errors_point_at_segment() {
        let cases: &[(&str, &str)] = &[
            // a + TL = 24.5 > L=20
            ("S40 E40 L20 M1.5 TL20\nS36 E36 L5", "超过段长"),
            // x=3.8 > a=3.0（RO一般 + SD短）
            (
                "S40 E40 L30 M1.5 TL20 RO一般 SD短\nS36 E36 L5",
                "大于肩距",
            ),
            // 没给螺距
            ("S40 E40 L30 M TL20\nS36 E36 L5", "必须给螺距"),
            // P 不在表 1
            ("S40 E40 L30 M1.6 TL20\nS36 E36 L5", "表 1"),
            // RO/SD 没 TL
            ("S40 E40 L30 M1.5 RO短\nS36 E36 L5", "只在局部螺纹"),
            // 本段右端倒角与 TL 冲突
            ("S40 E40 L30 M1.5 TL20 CH2@R\nS36 E36 L5", "不能倒角"),
            // 相邻段倒角落在本段右端
            ("S40 E40 L30 M1.5 TL20\nS30 E30 L5 CH2@L", "不能倒角"),
        ];
        for (text, needle) in cases {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: 解析失败 {e}"));
            let err = build(&program, 1.0).unwrap_err();
            assert!(err.contains("第 1 段"), "{text}: 应报第 1 段，得到 {err}");
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    // ── 段级 RL（任意圆柱段的肩部退刀槽，GB/T 3-1997 表 2 剖面） ────────

    /// `RL@L` / `RL@R` 几何、落层与剖视剖面环。
    #[test]
    fn segment_relief_geometry_left_right_and_layers() {
        // RL@L：面 x=10，槽向 +x 展开；P=1.5、d=20 → dg=17.7 / g1=2.5 / g2=4.5 / r=0.8
        let program =
            parse_program("S30 E30 L10 | S20 E20 L30 RL@L P1.5 | S30 E30 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        // 中段轮廓从 x=10+4.5=14.5 起
        assert!(has_line(&shaft, [14.5, 10.0], [40.0, 10.0]), "槽后轮廓");
        assert_eq!(
            layer_of_line(&shaft, [14.5, 10.0], [40.0, 10.0]),
            Some(LAYER_MAIN)
        );
        // R 圆角（心 (10.8, 9.65)、180°→270°）+ 镜像
        assert!(has_arc(&shaft, [10.8, 9.65], 0.8, 180.0, 270.0), "左端上圆角");
        assert!(has_arc(&shaft, [10.8, -9.65], 0.8, 90.0, 180.0), "左端下圆角");
        // 槽底 g1=2.5：10.8 → 12.5（y=8.85）；斜壁：12.5,8.85 → 14.5,10
        assert!(has_line(&shaft, [10.8, 8.85], [12.5, 8.85]), "槽底");
        assert!(has_line(&shaft, [12.5, 8.85], [14.5, 10.0]), "斜壁");
        assert!(has_line(&shaft, [10.8, -8.85], [12.5, -8.85]));
        assert!(has_line(&shaft, [12.5, -8.85], [14.5, -10.0]));
        // 台肩面 x=10：圆角切点 9.65 → 左邻半径 15
        assert!(has_line(&shaft, [10.0, 9.65], [10.0, 15.0]), "台肩面");
        // 三条贯通竖线（用户 2026-09-18 更正版）：槽肩（= 段边界线）/ 槽底终止 / 斜壁终点
        assert_eq!(
            layer_of_line(&shaft, [10.0, -9.65], [10.0, 9.65]),
            Some(LAYER_MAIN),
            "槽肩贯通竖线"
        );
        assert_eq!(
            layer_of_line(&shaft, [12.5, -8.85], [12.5, 8.85]),
            Some(LAYER_MAIN),
            "槽底终止贯通竖线"
        );
        assert_eq!(
            layer_of_line(&shaft, [14.5, -10.0], [14.5, 10.0]),
            Some(LAYER_MAIN),
            "斜壁终点贯通竖线"
        );
        // 槽体落层 = 1轮廓实线层（不是细线）
        assert_eq!(
            layer_of_line(&shaft, [10.8, 8.85], [12.5, 8.85]),
            Some(LAYER_MAIN)
        );
        assert_eq!(
            layer_of_line(&shaft, [12.5, 8.85], [14.5, 10.0]),
            Some(LAYER_MAIN)
        );

        // RL@R（默认端）：面 x=40，槽向 −x 展开
        let program =
            parse_program("S30 E30 L10 | S20 E20 L30 RL@R P1.5 | S30 E30 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_line(&shaft, [10.0, 10.0], [35.5, 10.0]), "右端槽前轮廓");
        assert!(has_arc(&shaft, [39.2, 9.65], 0.8, 270.0, 360.0), "右端上圆角");
        assert!(has_arc(&shaft, [39.2, -9.65], 0.8, 0.0, 90.0));
        assert!(has_line(&shaft, [39.2, 8.85], [37.5, 8.85]), "右端槽底");
        assert!(has_line(&shaft, [37.5, 8.85], [35.5, 10.0]), "右端斜壁");
        assert!(has_line(&shaft, [40.0, 9.65], [40.0, 15.0]), "右端台肩面");
        // 右端槽的三条贯通竖线（镜像）
        assert!(has_line(&shaft, [40.0, -9.65], [40.0, 9.65]), "右端槽肩贯通竖线");
        assert!(has_line(&shaft, [37.5, -8.85], [37.5, 8.85]), "右端槽底终止竖线");
        assert!(has_line(&shaft, [35.5, -10.0], [35.5, 10.0]), "右端斜壁终点竖线");
        // `RL` 省略端别 = 右端（既有默认）
        let bare = build(
            &parse_program("S30 E30 L10 | S20 E20 L30 RL P1.5 | S30 E30 L10").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(has_arc(&bare, [39.2, 9.65], 0.8, 270.0, 360.0));

        // 剖视：段级退刀槽也进剖面环（一个 HATCH、边界首尾相接）
        let section = build(
            &parse_program("S30 E30 L10 | S20 E20 L30 RL@L P1.5 | S30 E30 L10 VIEW 剖视")
                .unwrap(),
            1.0,
        )
        .unwrap();
        let hatch = section
            .entities
            .iter()
            .find_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .expect("剖视应有一个 HATCH");
        assert_eq!(hatch.paths.len(), 2, "轴线上/下两个环（参考件口径）");
        let pts = |edge: &ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge|
         -> ([f64; 2], [f64; 2]) {
            match edge {
                ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge::Line(e) => {
                    ([e.start.x, e.start.y], [e.end.x, e.end.y])
                }
                ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge::CircularArc(a) => (
                    [
                        a.center.x + a.radius * a.start_angle.cos(),
                        a.center.y + a.radius * a.start_angle.sin(),
                    ],
                    [
                        a.center.x + a.radius * a.end_angle.cos(),
                        a.center.y + a.radius * a.end_angle.sin(),
                    ],
                ),
                other => panic!("剖面环只应有 LINE/ARC，得到 {other:?}"),
            }
        };
        for path in &hatch.paths {
            let mut prev: Option<[f64; 2]> = None;
            for edge in &path.edges {
                let (a, b) = pts(edge);
                if let Some(p) = prev {
                    assert!(
                        near(p[0], a[0]) && near(p[1], a[1]),
                        "剖面环断开：{p:?} → {a:?}"
                    );
                }
                prev = Some(b);
            }
            let last = prev.unwrap();
            assert!(near(last[0], 0.0), "环回到左端面：{last:?}");
        }
    }

    /// 三种取参路径：① 显式 g1/g2/dg/r（dg 绝对值） ② P 查表 ③ P + 覆盖。
    #[test]
    fn segment_relief_parameter_paths() {
        // ① 显式四件套（d=25 → dg=22.7、g1=2.5、g2=4.5、r=0.8；不给 P）
        let program = parse_program(
            "S35 E35 L10 | S25 E25 L30 RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8 | S35 E35 L10",
        )
        .unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_arc(&shaft, [10.8, 12.15], 0.8, 180.0, 270.0));
        assert!(has_line(&shaft, [10.8, 11.35], [12.5, 11.35]));
        assert!(has_line(&shaft, [12.5, 11.35], [14.5, 12.5]));
        // `=` / `:` 写法同样收
        let eq = parse_program(
            "S35 E35 L10 | S25 E25 L30 RL@L g1=2.5 g2:4.5 dg=22.7 r=0.8 | S35 E35 L10",
        )
        .unwrap()
        .segments[1]
        .relief
        .clone();
        assert_eq!(eq.len(), 1);
        assert_eq!(
            (eq[0].g1, eq[0].g2, eq[0].dg, eq[0].r),
            (Some(2.5), Some(4.5), Some(22.7), Some(0.8))
        );
        // ② 只给 P → 表 2
        let program = parse_program("S35 E35 L10 | S25 E25 L30 RL@R P1.5 | S35 E35 L10")
            .unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_arc(&shaft, [39.2, 12.15], 0.8, 270.0, 360.0));
        assert!(has_line(&shaft, [37.5, 11.35], [35.5, 12.5]));
        // ③ P + 覆盖（表值取 g2/dg/r，r 换成 0.5）
        let program =
            parse_program("S35 E35 L10 | S25 E25 L30 RL@L P1.5 r 0.5 | S35 E35 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_arc(&shaft, [10.5, 11.85], 0.5, 180.0, 270.0), "覆盖后的圆角");
        assert!(has_line(&shaft, [10.5, 11.35], [12.5, 11.35]), "覆盖后槽底");
        // P 的参数也可分开写：`P 1.5`
        let p = parse_program("S35 E35 L10 | S25 E25 L30 RL@L P 1.5 | S35 E35 L10")
            .unwrap()
            .segments[1]
            .relief
            .clone();
        assert_eq!(p[0].p, Some(1.5));
        // 参数与 RL 顺序无关：`RL@L g1 2.5 P1.5 g2 4.5 …` 也收
        let mixed = parse_program(
            "S35 E35 L10 | S25 E25 L30 RL@L g1 2.5 P1.5 g2 4.5 dg 22.7 r 0.8 | S35 E35 L10",
        )
        .unwrap();
        assert!(build(&mixed, 1.0).is_ok());
    }

    /// 缺参 / 非法尺寸 / 表里没有 P / M 段覆盖：报错带「第 N 段」且不猜。
    #[test]
    fn segment_relief_parameter_errors() {
        let cases: &[(&str, &str)] = &[
            // ③ 都不给：提示给 P 或显式四件套
            ("S30 E30 L10 | S20 E20 L30 RL@L | S30 E30 L10", "缺参"),
            // 只给一部分显式值、没给 P
            ("S30 E30 L10 | S20 E20 L30 RL@L g1 2.5 | S30 E30 L10", "缺参"),
            // P 不在表 2
            ("S30 E30 L10 | S20 E20 L30 RL@L P1.3 | S30 E30 L10", "表 2"),
            // dg ≥ d（槽比大径还大）
            (
                "S30 E30 L10 | S20 E20 L30 RL@L g1 2.5 g2 4.5 dg 20 r 0.8 | S30 E30 L10",
                "dg",
            ),
            // g2 ≤ g1
            (
                "S30 E30 L10 | S20 E20 L30 RL@L g1 4 g2 4 dg 17.7 r 0.8 | S30 E30 L10",
                "g2",
            ),
            // 圆角伸到大径之外
            (
                "S30 E30 L10 | S20 E20 L30 RL@L g1 2.5 g2 4.5 dg 17.7 r 2 | S30 E30 L10",
                "圆角",
            ),
            // g2 > 段长
            ("S30 E30 L10 | S20 E20 L2 RL@L P1.5 | S30 E30 L10", "g2"),
            // 参数没跟在 RL 后面
            ("S30 E30 L10 P1.5 | S20 E20 L30 | S30 E30 L10", "跟在 RL"),
            // RL 带值（旧写法）
            ("S30 E30 L10 | S20 E20 L30 RL=1 | S30 E30 L10", "不带值"),
            // 同一端重复
            (
                "S30 E30 L10 | S20 E20 L30 RL@L P1.5 RL@L P1.5 | S30 E30 L10",
                "RL 重复",
            ),
            // 参数重复
            ("S30 E30 L10 | S20 E20 L30 RL@L g1 2.5 g1 3 | S30 E30 L10", "重复"),
            // 非法正数
            ("S30 E30 L10 | S20 E20 L30 RL@L g1 0 | S30 E30 L10", "必须 > 0"),
            // RL@X 端别非法
            ("S30 E30 L10 | S20 E20 L30 RL@X P1.5 | S30 E30 L10", "端别"),
            // M 段 RL 不接受覆盖参数（旧行为不被静默改写）
            ("S40 E40 L30 M1.5 TL20 RL g1 2.5\nS36 E36 L5", "不支持"),
        ];
        for (text, needle) in cases {
            let err = match parse_program(text) {
                Ok(program) => build(&program, 1.0).unwrap_err(),
                Err(e) => e,
            };
            assert!(
                err.contains("第 1 段") || err.contains("第 2 段"),
                "{text}: 应定位到段，得到 {err}"
            );
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    /// 前置条件：该端相邻段更高（有台肩）、本段圆柱、不能是自由端。
    #[test]
    fn segment_relief_preconditions() {
        let cases: &[(&str, &str)] = &[
            // 左邻同径 → 没有台阶面
            ("S20 E20 L10 | S20 E20 L30 RL@L P1.5 | S30 E30 L10", "没有台阶面"),
            // 右邻同径 → 没有台阶面
            ("S30 E30 L10 | S20 E20 L30 RL@R P1.5 | S20 E20 L10", "没有台阶面"),
            // 锥面不能开槽
            ("S30 E30 L10 | S25 E20 L30 RL@L P1.5 | S30 E30 L10", "圆柱"),
            // 自由端
            ("S20 E20 L30 RL@L P1.5 | S30 E30 L10", "自由端"),
            ("S30 E30 L10 | S20 E20 L30 RL@R P1.5", "自由端"),
        ];
        for (text, needle) in cases {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: 解析失败 {e}"));
            let err = build(&program, 1.0).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    /// 与同端 CH / OV / M 的 RL 互斥；`M…RL` 旧行为不变。
    #[test]
    fn segment_relief_mutual_exclusion_and_thread_regression() {
        let cases: &[(&str, &str)] = &[
            // 本段同端 CH
            ("S30 E30 L10 | S20 E20 L30 RL@L CH2@L P1.5 | S30 E30 L10", "倒角"),
            // 相邻段 CH 落在同一端面
            ("S30 E30 L10 CH2@R | S20 E20 L30 RL@L P1.5 | S30 E30 L10", "倒角"),
            // 本段同端 OV
            ("S50 E50 L10 | S40 E40 L30 RL@L OV@L P1.5 | S50 E50 L10", "越程槽"),
            // 两个退刀槽挤在同一端面（中段右端 + 后段左端）
            (
                "S30 E30 L10 | S20 E20 L30 RL@R P1.5 | S30 E30 L10 RL@L P1.5",
                "只能有一个退刀槽",
            ),
        ];
        for (text, needle) in cases {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: 解析失败 {e}"));
            let err = build(&program, 1.0).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // `M…RL@R` 与旧 `M…RL` 几何完全一致（表 2 按螺距；回归）
        let old = build(
            &parse_program("S40 E40 L30 M1.5 TL20 RL\nS36 E36 L5").unwrap(),
            1.0,
        )
        .unwrap();
        let explicit = build(
            &parse_program("S40 E40 L30 M1.5 TL20 RL@R\nS36 E36 L5").unwrap(),
            1.0,
        )
        .unwrap();
        assert_eq!(old.entities.len(), explicit.entities.len());
        assert!(old.entities.iter().all(|e| explicit.entities.contains(e)));
        // M 段右端已有螺纹收尾，后段左端不能再开槽（同一端面双槽）
        let both = parse_program("S40 E40 L30 M1.5 TL20 RL\nS36 E36 L5 RL@L P1.5").unwrap();
        let err = build(&both, 1.0).unwrap_err();
        assert!(err.contains("退刀槽"), "{err}");
    }

    /// `M…RL@L`：左端段级退刀槽 + 右端局部螺纹可共存；
    /// 小径细实线绕开槽体（无 TL 的整段全螺纹也一条）。
    #[test]
    fn thread_segment_left_relief_with_local_thread() {
        // 第 1 段 Ø50 做左邻（更高，有台肩）；第 2 段 d=40、P=1.5 → dg=37.7 / rg=18.85 / tangent=19.65
        let program =
            parse_program("S50 E50 L5\nS40 E40 L30 M1.5 TL24 RL@L P1.5\nS36 E36 L5").unwrap();
        let thread = program.segments[1].thread.unwrap();
        assert!(!thread.relief, "RL@L 不是螺纹收尾（thread.relief 仍为 false）");
        assert_eq!(program.segments[1].relief.len(), 1);
        assert_eq!(program.segments[1].relief[0].end, End::L);
        let shaft = build(&program, 1.0).unwrap();
        let r1 = (40.0 - 1.0825 * 1.5) / 2.0;
        // 第 2 段 [5, 35] 左端槽体（d=40）
        assert!(has_arc(&shaft, [5.8, 19.65], 0.8, 180.0, 270.0));
        assert!(has_line(&shaft, [5.8, 18.85], [7.5, 18.85]));
        assert!(has_line(&shaft, [7.5, 18.85], [9.5, 20.0]));
        assert!(has_line(&shaft, [9.5, 20.0], [35.0, 20.0]), "大径轮廓从槽后起");
        // 小径细实线从斜壁与小径交点起（r1=19.188 > rg=18.85），不是从完整螺纹 6.5 起
        let x0 = 5.0 + 2.5 + 2.0 * (r1 - 18.85) / (20.0 - 18.85);
        assert!(has_line(&shaft, [x0, r1], [34.3, r1]), "小径细实线绕过槽体");
        assert!(!has_line(&shaft, [6.5, r1], [x0, r1]), "槽内不应画小径细实线");
        // 右端 TL24 局部螺纹收尾照旧（锥面 → 螺尾）
        assert!(has_line(&shaft, [35.0, 20.0], [34.3, r1]), "锥面过渡");
        assert!(has_line(&shaft, [34.3, r1], [30.5, 20.0]), "螺尾细实线");

        // 不给 TL（整段全螺纹）也支持：细实线从斜壁交点起
        let plain = build(
            &parse_program("S50 E50 L5\nS40 E40 L30 M1.5 RL@L P1.5\nS36 E36 L5").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(has_line(&plain, [x0, r1], [35.0, r1]));
        assert!(has_line(&plain, [x0, -r1], [35.0, -r1]));
    }

    /// 段级 RL 的 JSON 往返；`M` 段的右端收尾仍走 `thread.relief`。
    #[test]
    fn segment_relief_json_round_trip() {
        let text = "S30 E30 L10 | S20 E20 L30 RL@L P1.5 RL@R g1 2.5 g2 4.5 dg 17.7 r 0.8 | S30 E30 L10";
        let program = parse_program(text).unwrap();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"relief\""), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // 单对象写法也收
        let one = r#"{"segments":[{"s":30,"l":10},{"s":20,"l":30,"relief":{"end":"L","p":1.5}},{"s":30,"l":10}]}"#;
        let program = parse_program(one).unwrap();
        assert_eq!(program.segments[1].relief[0].end, End::L);
        assert_eq!(program.segments[1].relief[0].p, Some(1.5));
        // 手搓 M 段 thread.relief + 段级 R 端 → 拒绝（避免画两遍）
        let bad = r#"{"segments":[{"s":40,"l":30,"thread":{"p":1.5,"relief":true},"relief":{"end":"R"}}]}"#;
        let err = parse_program(bad).unwrap_err();
        assert!(err.contains("thread.relief"), "{err}");
    }

    // ── 齿轮段 GEAR ──────────────────────────────────────────────────────

    #[test]
    fn dsl_gear_parses_and_derives() {
        // 直齿轮段：d = m·z 导出；默认齿宽 10m
        let program = parse_program("GEAR M3 Z20").unwrap();
        let seg = &program.segments[0];
        assert!(near(seg.s, 60.0) && near(seg.e, 60.0));
        assert!(near(seg.l, 30.0), "默认 H = 10m");
        let gear = seg.gear.expect("齿轮段");
        assert_eq!((gear.m, gear.z), (3.0, 20));
        assert_eq!(gear.h, None);
        assert!((gear.alpha_deg - 20.0).abs() < 1e-12, "GEAR 默认压力角 20°");
        // ALPHA：`ALPHA25` / `ALPHA=30` / `alpha=22.5` 都收，进 GearParams（影响 db/齿厚）
        for (text, want) in [
            ("GEAR M3 Z20 ALPHA25", 25.0),
            ("GEAR M3 Z20 ALPHA=30", 30.0),
            ("ALPHA45 GEAR M3 Z20", 45.0),
            ("GEAR M3 Z20 alpha=22.5", 22.5),
        ] {
            let g = parse_program(text).unwrap().segments[0].gear.unwrap();
            assert!((g.alpha_deg - want).abs() < 1e-9, "{text} → α={}", g.alpha_deg);
        }
        // 压力角只进派生参数，不改分度圆；JSON 序列化默认 20° 不写出、非默认写出
        let g25 = parse_program("GEAR M3 Z20 ALPHA25").unwrap().segments[0].gear.unwrap();
        assert!((g25.pitch_radius() - 30.0).abs() < 1e-9, "d=m·z 与 α 无关");
        let p25 = parse_program("GEAR M3 Z20 ALPHA25").unwrap();
        let j25 = serde_json::to_string(&p25).unwrap();
        assert!(j25.contains("\"alpha\":25"), "{j25}");
        let p20 = parse_program("GEAR M3 Z20").unwrap();
        let j20 = serde_json::to_string(&p20).unwrap();
        assert!(!j20.contains("alpha"), "默认 α 不写出（旧 JSON 兼容）：{j20}");
        assert_eq!(parse_program(&j25).unwrap(), p25, "带 α 的 JSON 往返要一致");
        // JSON 手写段：alpha 可省（默认 20）、可给
        let seg25 = parse_program(r#"{"segments":[{"gear":{"m":3,"z":20,"alpha":25}}]}"#)
            .unwrap()
            .segments[0]
            .gear
            .unwrap();
        assert!((seg25.alpha_deg - 25.0).abs() < 1e-9);
        let seg_def = parse_program(r#"{"segments":[{"gear":{"m":3,"z":20}}]}"#)
            .unwrap()
            .segments[0]
            .gear
            .unwrap();
        assert!((seg_def.alpha_deg - 20.0).abs() < 1e-12, "JSON 缺 alpha → 20°");
        // H 显式 + BETA0（直齿）可写
        let program = parse_program("GEAR M5 Z10 H20 BETA0").unwrap();
        let seg = &program.segments[0];
        assert!(near(seg.l, 20.0));
        assert_eq!(seg.gear.unwrap().h, Some(20.0));
        // 大小写 / 顺序无所谓（Z10 H20 GEAR M5）
        let shuffled = parse_program("z10 h20 gear m5").unwrap();
        assert_eq!(shuffled.segments[0], program.segments[0]);
        // 派生尺寸与 gear.rs 同口径（ra/da/2、r/d/2、rf/df/2）
        let gear = parse_program("GEAR M5 Z10 H20")
            .unwrap()
            .segments[0]
            .gear
            .unwrap();
        let params = crate::gear::GearParams {
            kind: crate::gear::GearKind::External,
            m: 5.0,
            z: 10,
            h: 20.0,
            ..crate::gear::GearParams::default()
        };
        assert!(near(gear.pitch_radius(), params.d() / 2.0));
        assert!(near(gear.addendum_radius(), params.da() / 2.0));
        assert!(near(gear.root_radius(), params.df() / 2.0));
        assert!(near(gear.pitch_radius(), 25.0));
        assert!(near(gear.addendum_radius(), 30.0));
        assert!(near(gear.root_radius(), 18.75));
    }

    #[test]
    fn dsl_gear_reports_errors() {
        let cases: &[(&str, &str)] = &[
            ("GEAR M3", "缺少 Z"),
            ("GEAR Z20", "缺少 M"),
            ("GEAR M3 Z20.5", "不是正整数"),
            ("GEAR M0 Z20", "模数 m 必须是正数"),
            ("GEAR M-3 Z20", "模数 m 必须是正数"),
            ("GEAR M3 Z1", "齿数 z 超出范围"),
            ("GEAR M3 Z20 S40", "不给 S/E"),
            ("GEAR M3 Z20 E40", "不给 S/E"),
            ("GEAR M3 Z20 L30", "不要再给 L"),
            ("GEAR M3 Z20 BETA12", "本期只做直齿"),
            ("GEAR M3 Z20 ALPHA0", "压力角 α 超出范围"),
            ("GEAR M3 Z20 ALPHA60", "压力角 α 超出范围"),
            ("GEAR M3 Z20 ALPHA25 ALPHA30", "关键字 ALPHA 重复"),
            ("GEAR M3 Z20 H0", "厚度 h 必须是正数"),
            ("GEAR M3 Z20 CH2", "不能与倒角"),
            ("GEAR M3 Z20 OV", "不能与越程槽"),
            ("GEAR M3 Z20 M1.5", "M 重复"),
            ("GEAR GEAR M3 Z20", "GEAR 重复"),
            ("S40 E40 L20 GEAR M3 Z20", "不给 S/E"),
        ];
        for (text, needle) in cases {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // GEAR 的 m/z 错误也要能按「第 N 段」定位（含跨行）
        let err = parse_program("GEAR M0 Z20").unwrap_err();
        assert!(err.contains("第 1 段"), "{err}");
        let err = parse_program("S30 E30 L10\nGEAR M3 Z20.5").unwrap_err();
        assert!(err.contains("第 2 段"), "{err}");
    }

    // ── 矩形花键段 SPLINE（GB/T 1144 规格代号） ────────────────────────

    /// DSL：`SPLINE 6x23x26x6 L30 [de …]` → 大径 D、段长 = L + l；关键字位置无关。
    #[test]
    fn dsl_spline_parses_and_derives() {
        let program = parse_program("SPLINE 6x23x26x6 L30").unwrap();
        assert_eq!(program.segments.len(), 1);
        let seg = &program.segments[0];
        let sp = seg.spline.expect("花键段");
        assert_eq!((sp.n, sp.d, sp.big, sp.b, sp.de, sp.len), (6, 23.0, 26.0, 6.0, 63.0, 30.0));
        assert!(near(seg.s, 26.0) && near(seg.e, 26.0), "s=e=大径 D");
        assert!(near(seg.l, 30.0 + sp.runout()), "段长 = L + l");
        assert!(near(sp.runout(), 9.604_687));
        // de 覆盖（贴写 / 空格 / `=`）；大小写 / 顺序无关
        let with_de = parse_program("SPLINE 6x23x26x6 de71 L30").unwrap();
        assert!(near(with_de.segments[0].spline.unwrap().de, 71.0));
        let shuffled = parse_program("l30 spline 6x23x26x6").unwrap();
        assert_eq!(shuffled.segments[0], program.segments[0]);
        for text in ["SPLINE 6x23x26x6 L30 de 71", "SPLINE 6x23x26x6 L30 de=71", "SPLINE 6×23×26×6 L30"] {
            assert!(parse_program(text).is_ok(), "{text}");
        }
        // 表外规格：不给 de 报错并指路；给了 de 可出图
        let err = parse_program("SPLINE 6x11x14x3 L20").unwrap_err();
        assert!(err.contains("de"), "{err}");
        assert!(parse_program("SPLINE 6x11x14x3 L20 de63").is_ok());
        // JSON 往返：序列化模型能原样解析回来
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("spline"), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        let dsl_from_gui = r#"{"segments":[{"spline":{"spec":"6x23x26x6","len":30,"de":71}}]}"#;
        assert!(near(
            parse_program(dsl_from_gui).unwrap().segments[0]
                .spline
                .unwrap()
                .de,
            71.0
        ));
    }

    /// 表内规格不传 de：轴段 DSL 与 JSON 模型都自动查 GB/T 10952 表并算 R/l；
    /// 表外规格仍必须显式 de（独立要素入口另有等价回归，见 `detail.rs`）。
    #[test]
    fn spline_table_spec_without_de_auto_lookup() {
        // ① DSL：SPLINE 6x23x26x6 L30 → de=63（R=31.5）、h=1.5、l=9.6047
        let program = parse_program("SPLINE 6x23x26x6 L30").unwrap();
        let sp = program.segments[0].spline.expect("花键段");
        assert_eq!((sp.de, sp.hob_radius()), (63.0, 31.5), "de 查表 / R=de/2");
        assert!(near(sp.depth(), 1.5) && near(sp.runout(), 9.604_687), "h / l 数值");
        assert!(near(program.segments[0].l, 39.604_687), "段长 = L + l");
        // ② JSON 模型（GUI 段表 → `/api/shaft_export` 的口径）：不传 de 同样查表
        let json = r#"{"segments":[{"spline":{"spec":"6x28x32x7","len":30}}]}"#;
        let sp = parse_program(json).unwrap().segments[0].spline.expect("花键段");
        assert_eq!(sp.de, 71.0, "6x28x32x7 → de=71");
        assert_eq!(sp.hob_radius(), 35.5, "R = de/2");
        // h=2、l=√(2×(71−2))=11.747340
        assert!(near(sp.depth(), 2.0) && near(sp.runout(), 11.747_340), "h / l 数值");
        // ③ 表外规格行为不变：不传 de 报错指路；给了 de 才能出图
        let err = parse_program(r#"{"segments":[{"spline":{"spec":"6x11x14x3","len":20}}]}"#)
            .unwrap_err();
        assert!(err.contains("de 查不到") && err.contains("请给 de 覆盖"), "{err}");
        assert!(parse_program(
            r#"{"segments":[{"spline":{"spec":"6x11x14x3","len":20,"de":63}}]}"#
        )
        .is_ok());
    }

    /// DSL 错误：缺规格/缺 L、不给 S/E、与 CH/OV/RL/M/GEAR 互斥、de ≤ D。
    #[test]
    fn dsl_spline_reports_errors() {
        let cases: &[(&str, &str)] = &[
            ("SPLINE 6x23x26x6", "缺少 L"),
            ("SPLINE L30", "规格代号"),
            ("SPLINE 6x23x26x6 S26 E26 L30", "不给 S/E"),
            ("SPLINE 6x23x26x6 L30 CH2@L", "不能与倒角"),
            ("SPLINE 6x23x26x6 L30 OV3", "不能与越程槽"),
            ("SPLINE 6x23x26x6 L30 RL@L P1.5", "不能与退刀槽"),
            ("SPLINE 6x23x26x6 L30 M1.5", "不能与螺纹"),
            ("SPLINE 6x23x26x6 L30 GEAR M3 Z20", "齿轮段"),
            ("SPLINE 6x23x26x6 L30 de26", "必须大于大径"),
            ("SPLINE 6x23x26x6 L30 de63 de71", "de 覆盖重复"),
        ];
        for (text, needle) in cases {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // 花键段不能与倒角同段（引入倒角写在相邻段上）
        let err = parse_program("SPLINE 6x23x26x6 L30 CH2").unwrap_err();
        assert!(err.contains("相邻轴段") || err.contains("不能与倒角"), "{err}");
    }

    /// 单段花键轴：大径线 / 小径细线 / 收尾弧（圆心、相切、终点）/ 细竖线 / 端面。
    #[test]
    fn spline_segment_geometry_matches_template_group4() {
        let program = parse_program("SPLINE 6x23x26x6 L30").unwrap();
        let sp = program.segments[0].spline.unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let (l, r, ra, rh) = (sp.len, sp.minor_radius(), sp.major_radius(), sp.hob_radius());
        // 整个段长 = L + l；自由端面在 L+l
        assert!(near(program.total_length(), l + sp.runout()));
        assert!(has_line(&shaft, [0.0, -ra], [0.0, ra]), "左端面 ±D/2");
        assert!(has_line(&shaft, [l + sp.runout(), -ra], [l + sp.runout(), ra]), "右端面");
        // 大径线：满齿段 [0, L] 与收尾段 [L, L+l] 两段（模板 [43]/[46] 也在 x=L 断开）
        assert!(has_line(&shaft, [0.0, ra], [l, ra]));
        assert!(has_line(&shaft, [l, ra], [l + sp.runout(), ra]));
        assert!(has_line(&shaft, [0.0, -ra], [l, -ra]));
        assert!(has_line(&shaft, [l, -ra], [l + sp.runout(), -ra]));
        // 小径细线 [0, L] 落 2细线层
        assert_eq!(layer_of_line(&shaft, [0.0, r], [l, r]), Some(LAYER_THIN));
        assert_eq!(layer_of_line(&shaft, [0.0, -r], [l, -r]), Some(LAYER_THIN));
        // 收尾弧：上弧 270°→360°−a；下弧 a→90°（模板 [56]/[57] 同口径）
        let a = sp.runout_end_angle();
        assert!(near(a, 72.247_210));
        assert!(has_arc(&shaft, [l, r + rh], rh, 270.0, 360.0 - a));
        assert!(has_arc(&shaft, [l, -(r + rh)], rh, a, 90.0));
        // 满齿终止细竖线 x=L，跨 ±D/2（模板 [42]）；自由端收尾终点由右端面闭合
        assert_eq!(layer_of_line(&shaft, [l, -ra], [l, ra]), Some(LAYER_THIN));
        // 尾端面是 1轮廓实线层
        assert_eq!(
            layer_of_line(&shaft, [l + sp.runout(), -ra], [l + sp.runout(), ra]),
            Some(LAYER_MAIN)
        );
        // 剖视：小径线 / 收尾弧改 1轮廓实线层 + 轴线↔小径两条带 HATCH
        let section = build(
            &parse_program("SPLINE 6x23x26x6 L30 VIEW 剖视").unwrap(),
            1.0,
        )
        .unwrap();
        assert_eq!(layer_of_line(&section, [0.0, r], [l, r]), Some(LAYER_MAIN));
        assert!(section
            .entities
            .iter()
            .any(|e| matches!(e, EntityType::Hatch(_))));
    }

    /// 相邻段写 CH：倒角落在花键凸角上，小径细线跟着内缩到倒角与小径的交点。
    #[test]
    fn spline_segment_with_neighbor_chamfer() {
        let program = parse_program("S22 E22 L5 CH2@R | SPLINE 6x23x26x6 L30").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let sp = program.segments[1].spline.unwrap();
        let (r, ra) = (sp.minor_radius(), sp.major_radius());
        // φ22 段右端面 x=5 高 ±11；倒角（贴花键一侧）(5,11)→(7,13)
        assert!(has_line(&shaft, [5.0, -11.0], [5.0, 11.0]));
        assert!(has_line(&shaft, [5.0, 11.0], [7.0, ra]));
        // 小径细线从倒角与小径的交点 x=5+0.5 起，到满齿段右端 x=5+30
        assert_eq!(layer_of_line(&shaft, [5.5, r], [35.0, r]), Some(LAYER_THIN));
        assert!(near(5.5, 5.0 + 0.5), "内缩 = c − (R−r) = 0.5");
        // 花键段右端后面还有别的段时，收尾终点细竖线在 L+l
        let program = parse_program("SPLINE 6x23x26x6 L30 | S40 E40 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let end = 30.0 + sp.runout();
    }

    // ── 渐开线花键段（INVOLSPLINE）───────────────────────────────

    /// DSL：预设代号 + M/Z/X/L + 可选 de；直径由预设导出；JSON 往返。
    #[test]
    fn dsl_invol_spline_parses_and_derives() {
        let program = parse_program("INVOLSPLINE GB30R M3 Z20 L30").unwrap();
        assert_eq!(program.segments.len(), 1);
        let seg = &program.segments[0];
        let iv = seg.invol_spline.as_ref().expect("渐开线花键段");
        assert_eq!(iv.code, "GB30R");
        assert!(near(iv.params.m, 3.0) && iv.params.z == 20 && near(iv.params.x, 0.0));
        assert!(iv.de.is_none(), "de 可选：不给 = 不画收尾");
        assert!(near(seg.s, 63.0) && near(seg.e, 63.0), "s=e=da（GB30R m3 z20 → 63）");
        assert!(near(seg.l, 30.0), "不给 de 时段长 = L");
        assert!(near(iv.params.da(), 63.0) && near(iv.params.df(), 54.6));
        // 大小写 / 顺序无关；`INVOLSPLINE=…` 也收（子关键字值要贴写：M3/Z20/L30）
        let shuffled = parse_program("L30 involspline gb30r z20 M3").unwrap();
        assert_eq!(shuffled.segments[0], program.segments[0]);
        assert!(parse_program("INVOLSPLINE=GB30R M3 Z20 L30").is_ok());
        // DIN + 变位 + de：da = mz+2xm+0.9m、df = mz+2xm−1.1m
        let p2 = parse_program("INVOLSPLINE DIN30 M2 Z18 X0.2 L20 de50").unwrap();
        let iv2 = p2.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(iv2.code, "DIN30");
        assert!(near(iv2.params.da(), 38.6) && near(iv2.params.df(), 34.6));
        assert!(
            (iv2.runout() - (2.0_f64 * 48.0).sqrt()).abs() < 1e-9,
            "l=√(h(2R−h))，h=2、R=25：{}",
            iv2.runout()
        );
        assert!(near(p2.segments[0].l, 20.0 + iv2.runout()), "段长 = L + l");
        // GB 变位不受 DIN 范围限制（x=1.5 合法）
        assert!(parse_program("INVOLSPLINE GB30R M3 Z20 X1.5 L30").is_ok());
        // JSON 往返：结构与 DSL 解析一致（序列化 {code,m,z,x,len,de}）
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("invol_spline") && json.contains("\"code\":\"GB30R\""), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // GUI 口径：直接给 JSON 模型
        let gui = r#"{"segments":[{"invol_spline":{"code":"DIN30","m":2,"z":18,"x":0.2,"len":20,"de":50}}]}"#;
        let g = parse_program(gui).unwrap();
        assert_eq!(g.segments[0], p2.segments[0]);
        // 也收分开的 std + profile（拼成预设代号）
        let split = r#"{"segments":[{"invol_spline":{"std":"GB","profile":"30R","m":3,"z":20,"len":30}}]}"#;
        assert_eq!(parse_program(split).unwrap().segments[0], program.segments[0]);
    }

    /// ANSI 径节分支：`P5/10`（A/B）与 `DP5` 都解析；`P`/`DP` 不再被 RL 参数分支截住；
    /// 表达式往返：齿轮侧产出 `DP<A/B>`，轴侧解析一致。
    #[test]
    fn dsl_invol_spline_ansi_pitch_ab_and_dp() {
        for text in [
            "INVOLSPLINE ANSI30P P5/10 Z20 L30",
            "INVOLSPLINE ANSI30P DP5 Z20 L30",
            "INVOLSPLINE ANSI30P DP5/10 Z20 L30",
            "INVOLSPLINE ANSI30P P=5/10 Z20 L30",
            "INVOLSPLINE ANSI30P M5 Z20 L30", // 旧 M 槽位兼容
        ] {
            let p = parse_program(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            let iv = p.segments[0].invol_spline.as_ref().expect("INVOLSPLINE 段");
            assert_eq!(iv.params.std, crate::invol_spline::SplineStd::ANSI, "{text}");
            assert_eq!(iv.params.ansi_p(), 5.0, "{text}");
            assert!((iv.params.m - 25.4 / 5.0).abs() < 1e-12, "{text}");
        }
        // A/B 的 B != 2A → 语法期报错（说明 Ps 恒为 2P）
        let e = parse_program("INVOLSPLINE ANSI30P P5/11 Z20 L30").unwrap_err();
        assert!(e.contains("Ps 恒为 2P"), "{e}");
        // 系列外：resolve 期报错并列 17 项 A/B
        let e = parse_program("INVOLSPLINE ANSI30P P2 Z20 L30").unwrap_err();
        assert!(
            e.contains("标准系列") && e.contains("2.5/5") && e.contains("128/256"),
            "{e}"
        );
        // 版本不重复：轴段错误只有一层 `ANSI B92.1：`
        assert!(e.matches("ANSI B92.1：").count() <= 1, "{e}");
        // 纯 RL 段的 P 查表仍不受影响（分支顺序修复不误伤）
        let p = parse_program("S25 E25 L32 RL@L P1.5").unwrap();
        assert_eq!(p.segments[0].relief.len(), 1);
        assert_eq!(p.segments[0].relief[0].p, Some(1.5));
        // 退刀槽与 INVOLSPLINE 混用：仍报互斥（不再被 RL 参数截住）
        let e = parse_program("INVOLSPLINE ANSI30P Z20 L30 RL@L P1.5").unwrap_err();
        assert!(e.contains("不能与退刀槽"), "{e}");
    }

    /// DSL 错误：缺预设/M/Z/L、与 SPLINE/CH/OV/RL/M/GEAR 互斥、de ≤ da、DIN x 越界。
    #[test]
    fn dsl_invol_spline_reports_errors() {
        let cases: &[(&str, &str)] = &[
            ("INVOLSPLINE GB30R M3 Z20", "缺少 L"),
            ("INVOLSPLINE GB99 M3 Z20 L30", "预设代号"),
            ("INVOLSPLINE GB30R Z20 L30", "缺模数"),
            ("INVOLSPLINE GB30R M3 L30", "缺齿数"),
            ("INVOLSPLINE GB30R M3 Z20 L30 S63 E63", "不给 S/E"),
            ("INVOLSPLINE GB30R M3 Z20 L30 CH2@L", "不能与倒角"),
            ("INVOLSPLINE GB30R M3 Z20 L30 OV3", "不能与越程槽"),
            ("INVOLSPLINE ANSI30P Z20 L30 RL@L P1.5", "不能与退刀槽"),
            ("INVOLSPLINE GB30R M3 Z20 L30 SPLINE 6x23x26x6", "不能与矩形花键"),
            ("INVOLSPLINE GB30R M3 Z20 L30 de60", "必须大于"),
            ("INVOLSPLINE GB30R M3 Z20 L30 de63 de64", "de 覆盖重复"),
            ("INVOLSPLINE DIN30 M2 Z18 X0.6 L20", "x"),
            ("INVOLSPLINE GB30R M3 Z20 L30 M1.5", "M 重复"),
            // 体系不支持的模数：轴段命令行侧也明确报错并指出来源表。
            ("INVOLSPLINE GB30R M0.7 Z20 L30", "表 2"),
            ("INVOLSPLINE NFP A80 M2 Z19 L30", "NF E22-141"),
            ("INVOLSPLINE DIN30 DB40 M0.7 L30", "din5480_2_nominal.csv"),
        ];
        for (text, needle) in cases {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // JSON：与齿轮段同段在解析层就拒绝
        let err = parse_program(
            r#"{"segments":[{"invol_spline":{"code":"GB30R","m":3,"z":20,"len":30},"gear":{"m":3,"z":20}}]}"#,
        )
        .unwrap_err();
        assert!(err.contains("不能与齿轮段"), "{err}");
        // JSON：GB 给 d_b → 统一文案（DSL/JSON 都不静默忽略）
        let err = parse_program(
            r#"{"segments":[{"invol_spline":{"code":"GB30R","d_b":40,"m":3,"z":20,"len":30}}]}"#,
        )
        .unwrap_err();
        assert!(err.contains("本体系不用 d_B"), "{err}");
    }

    /// `INVOLSPLINE DIN30 DB40 M2 L30`：DB+M 补 z、取表值 x、JSON/DSL 同口径。
    #[test]
    fn dsl_invol_spline_db_lookup_and_json() {
        let program = parse_program("INVOLSPLINE DIN30 DB40 M2 L30").unwrap();
        let iv = program.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(iv.code, "DIN30");
        assert!((iv.params.m - 2.0).abs() < 1e-9);
        assert_eq!(iv.params.z, 18, "p27 m=2 d_B=40 → z=18");
        assert!((iv.params.x - 0.45).abs() < 1e-9, "取表值 x=0.45");
        assert_eq!(iv.params.d_b, Some(40.0));
        assert!(near(program.total_length(), 30.0));
        // DB+Z 补 M（同一段应完全相等）
        let program2 = parse_program("INVOLSPLINE DIN30 DB40 Z18 L30").unwrap();
        assert_eq!(program2.segments[0], program.segments[0]);
        // JSON 往返带 d_b；GUI 模型（别名 db / d_B）同口径
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"d_b\":40.0"), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        let gui = r#"{"segments":[{"invol_spline":{"code":"DIN30","d_b":40,"m":2,"len":30}}]}"#;
        assert_eq!(parse_program(gui).unwrap(), program);
        let gui2 = r#"{"segments":[{"invol_spline":{"std":"DIN","profile":"30R","db":40,"z":18,"l":30}}]}"#;
        assert_eq!(parse_program(gui2).unwrap(), program);
        // m=1.5 现已入库：DB20+M1.5 → z=12、取表值 x=0.175/1.5=0.1167
        let program15 = parse_program("INVOLSPLINE DIN30 DB20 M1.5 L30").unwrap();
        let iv15 = program15.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(iv15.params.z, 12, "m=1.5 d_B=20 → z=12");
        assert!(
            (iv15.params.x - 0.175 / 1.5).abs() < 1e-9,
            "取表值 x={}",
            iv15.params.x
        );
        assert_eq!(iv15.params.d_b, Some(20.0));
        // m=5 已由用户截图补入：DB50+M5 → z=8、取表值 x=2.25/5=0.45
        let program_m5 = parse_program("INVOLSPLINE DIN30 DB50 M5 L30").unwrap();
        let iv_m5 = program_m5.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(iv_m5.params.z, 8, "m=5 d_B=50 → z=8");
        assert!(
            (iv_m5.params.x - 0.45).abs() < 1e-9,
            "取表值 x={}",
            iv_m5.params.x
        );
        assert_eq!(iv_m5.params.d_b, Some(50.0));
        // 不一致 / 不可行 / GB 带 DB 的报错口径
        for (text, needle) in [
            ("INVOLSPLINE DIN30 DB1 M5 L30", "可行齿数"),
            ("INVOLSPLINE GB30R DB40 M2 Z18 L30", "本体系不用 d_B"),
            ("INVOLSPLINE DIN30 DB40 L30", "请再给"),
        ] {
            let err = parse_program(text).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
        // d_B 为主参数：DB40+M2+Z14（x 会越界）→ 按 d_B 取 z=18，不报错，note 进 JSON。
        let adjusted = parse_program("INVOLSPLINE DIN30 DB40 M2 Z14 L30").unwrap();
        let ia = adjusted.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(ia.params.z, 18);
        assert!(
            ia.d_b_note.as_deref().is_some_and(|n| n.contains("按基准直径 d_B=40 取 z=18")),
            "{:?}",
            ia.d_b_note
        );
        // d_B+m 表外 → 公式推 z，note 标注“推导值、未命中表”。
        let derived = parse_program("INVOLSPLINE DIN30 DB41 M2 L30").unwrap();
        let idr = derived.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(idr.params.z, 19);
        assert!(
            idr.d_b_note.as_deref().is_some_and(|n| n.contains("推导值、未命中表")),
            "{:?}",
            idr.d_b_note
        );
        // DB 但 M/Z 全缺且命中多行 → 列候选（d_B=45 跨 m 多行）
        let err = parse_program("INVOLSPLINE DIN30 DB45 L30").unwrap_err();
        assert!(err.contains("多个") && err.contains("m=3"), "{err}");
    }

    /// `INVOLSPLINE NFP A80 M3.75 L30`：NF 已入库（A 主参数）；A+m 补 N、A+z 补 m；JSON 携带 a。
    #[test]
    fn dsl_invol_spline_nf_a_lookup() {
        let program = parse_program("INVOLSPLINE NFP A80 M3.75 L30").unwrap();
        let iv = program.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(iv.code, "NFP");
        assert_eq!(iv.params.std, crate::invol_spline::SplineStd::NF);
        assert!((iv.params.m - 3.75).abs() < 1e-9);
        assert_eq!(iv.params.z, 19, "A=80 m=3.75 → N=19");
        let x_want = (80.0 - 3.75 * 19.0 - 1.5) / 7.5;
        assert!((iv.params.x - x_want).abs() < 1e-12, "x 由 A 公式解");
        assert_eq!(iv.params.a, Some(80.0));
        assert!(
            (iv.params.da() - 80.0).abs() < 1e-9
                && (iv.params.df() - (80.0 - 2.4 * 3.75)).abs() < 1e-9,
            "NF da=A、df=A−2.4m"
        );
        // A+Z 补 m（同一段相等）；`INVOLSPLINE NF A80 M3.75 L30`（只写体系标识）也应可用。
        assert_eq!(
            parse_program("INVOLSPLINE NF A80 Z19 L30").unwrap().segments[0],
            program.segments[0]
        );
        assert!(parse_program("INVOLSPLINE NF A80 M3.75 L30").is_ok());
        // JSON 往返携带 a（不写 d_b）。
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"a\":80.0") && !json.contains("\"d_b\""), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // GUI 模型 JSON 也收 `a` 键。
        let gui = r#"{"segments":[{"invol_spline":{"code":"NFP","a":80,"m":3.75,"len":30}}]}"#;
        assert_eq!(parse_program(gui).unwrap(), program);
        // A 只对 NF/DIN：GB 给 A 报统一文案。
        let err = parse_program("INVOLSPLINE GB30R A80 M3 Z20 L30").unwrap_err();
        assert!(err.contains("本体系不用 d_B"), "{err}");
        // 表外 A=81 → 主系列 x=0.8 推 N=20（推导值、未命中表）。
        let p3 = parse_program("INVOLSPLINE NFP A81 M3.75 L30").unwrap();
        let iv3 = p3.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(iv3.params.z, 20);
        assert!(
            iv3.d_b_note.as_deref().is_some_and(|n| n.contains("推导值、未命中表")),
            "{:?}",
            iv3.d_b_note
        );
    }

    /// 轴段几何：无 de（段长 = L、端面收口）/ 有 de（收尾弧 + 段长 = L+l）/ 剖视 HATCH；
    /// 无用户 CH 时按**单侧规则**自动倒角（自由端 → 两端加）。
    #[test]
    fn invol_spline_segment_geometry_with_and_without_de() {
        // ── 无 de ──
        let program = parse_program("INVOLSPLINE GB30R M3 Z20 L30").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let iv = program.segments[0].invol_spline.as_ref().unwrap();
        let (r, ra) = (iv.minor_radius(), iv.major_radius());
        let c = (crate::gear::CHAMFER_RATIO * iv.params.m).round();
        assert!(near(c, 2.0), "C = round(0.6×3) = 2");
        assert!(near(program.total_length(), 30.0));
        assert!(has_line(&shaft, [0.0, -(ra - c)], [0.0, ra - c]), "左端面 ±(da/2−C)");
        assert!(has_line(&shaft, [30.0, -(ra - c)], [30.0, ra - c]), "右端面收口 ±(da/2−C)");
        assert!(has_line(&shaft, [0.0, ra - c], [c, ra]), "左端自动倒角斜线");
        assert!(has_line(&shaft, [30.0 - c, ra], [30.0, ra - c]), "右端自动倒角斜线");
        assert!(has_line(&shaft, [c, ra], [30.0 - c, ra]), "大径线两端缩 C");
        assert_eq!(layer_of_line(&shaft, [0.0, r], [30.0, r]), Some(LAYER_THIN));
        assert_eq!(layer_of_line(&shaft, [0.0, -r], [30.0, -r]), Some(LAYER_THIN));
        assert!(
            !shaft.entities.iter().any(|e| matches!(e, EntityType::Arc(a) if a.radius > ra)),
            "无 de 不画收尾弧"
        );
        // 剖视：小径线改轮廓 + HATCH
        let section = build(
            &parse_program("INVOLSPLINE GB30R M3 Z20 L30 VIEW 剖视").unwrap(),
            1.0,
        )
        .unwrap();
        assert_eq!(layer_of_line(&section, [0.0, r], [30.0, r]), Some(LAYER_MAIN));
        assert!(section.entities.iter().any(|e| matches!(e, EntityType::Hatch(_))));

        // ── 有 de ──
        let program = parse_program("INVOLSPLINE GB30R M3 Z20 L30 de70").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let iv = program.segments[0].invol_spline.as_ref().unwrap();
        let (l, rh) = (iv.runout(), 35.0);
        let total = 30.0 + l;
        // h = (da−df)/2 = 4.2；l = √(4.2×(70−4.2)) = 16.6241
        assert!((l - 16.624_1).abs() < 1e-3, "l={l}");
        assert!(near(program.total_length(), total));
        // 左自由端自动倒角；右端是滚刀收尾（de）→ 不叠加端面倒角
        assert!(has_line(&shaft, [0.0, ra - c], [c, ra]), "左端自动倒角斜线");
        assert!(has_line(&shaft, [0.0, -(ra - c)], [0.0, ra - c]), "左端面 ±(da/2−C)");
        assert!(has_line(&shaft, [c, ra], [30.0, ra]), "满齿段大径线（左端缩 C）");
        assert!(has_line(&shaft, [30.0, ra], [total, ra]), "收尾段大径线");
        assert!(has_line(&shaft, [total, -ra], [total, ra]), "收尾终点端面（不收倒角）");
        let a = iv.params.runout_end_angle(70.0).unwrap();
        assert!(has_arc(&shaft, [30.0, r + rh], rh, 270.0, 360.0 - a), "上收尾弧");
        assert!(has_arc(&shaft, [30.0, -(r + rh)], rh, a, 90.0), "下收尾弧");
        assert_eq!(layer_of_line(&shaft, [30.0, -ra], [30.0, ra]), Some(LAYER_THIN));
        // 剖视也有 HATCH（小径线 → 收尾弧闭合）
        let section = build(
            &parse_program("INVOLSPLINE GB30R M3 Z20 L30 de70 VIEW 剖视").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(section.entities.iter().any(|e| matches!(e, EntityType::Hatch(_))));
    }

    /// HATCH 边界 → CSV（供 `compare_spline.py` 逐 LineEdge 对照）：
    /// 首行 `# ANSI31 scale=1.0 angle=0.0 loops=2`，其余 `ring,edge,x1,y1,x2,y2`。
    fn hatch_csv(entities: &[EntityType]) -> String {
        use ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge;
        let Some(h) = entities.iter().find_map(|e| match e {
            EntityType::Hatch(h) => Some(h),
            _ => None,
        }) else {
            return String::new();
        };
        let mut csv = format!(
            "# {} scale={} angle={} loops={}\n",
            h.pattern.name,
            h.pattern_scale,
            h.pattern_angle.to_degrees(),
            h.paths.len()
        );
        csv.push_str("ring,edge,x1,y1,x2,y2\n");
        for (ring, path) in h.paths.iter().enumerate() {
            for (edge, e) in path.edges.iter().enumerate() {
                if let BoundaryEdge::Line(l) = e {
                    csv.push_str(&format!(
                        "{},{},{:.6},{:.6},{:.6},{:.6}\n",
                        ring, edge, l.start.x, l.start.y, l.end.x, l.end.y
                    ));
                }
            }
        }
        csv
    }

    /// 矩形花键回归样例 → `~/桌面/OCSM/review/spline_*.csv`（`compare_spline.py` 用）。
    #[test]
    fn spline_demo_csv_dump() {
        let path = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("桌面/OCSM/review");
        std::fs::create_dir_all(&path).expect("建 review 目录");

        // 独立要素三个视图（L=30，局部原点 = 左端面/轴线）
        for (view, file) in [
            ("front", "spline_element_front.csv"),
            ("side", "spline_element_side.csv"),
            ("section", "spline_element_section.csv"),
        ] {
            let mut params = crate::detail::DetailParams::new();
            params.set_spec("6x23x26x6");
            if view != "front" {
                params.insert("len", 30.0);
            }
            let part = crate::detail::generate_params(
                crate::detail::FAMILY_SPLINE_RECT,
                23.0,
                &params,
                view,
            )
            .unwrap_or_else(|e| panic!("{view} 生成失败：{e}"));
            std::fs::write(path.join(file), entities_csv(&part.entities)).expect("写 CSV");
            assert!(path.join(file).is_file());
            if view == "section" {
                std::fs::write(
                    path.join("spline_element_section_hatch.csv"),
                    hatch_csv(&part.entities),
                )
                .expect("写 HATCH CSV");
            }
        }

        // 轴段特征（模板第 4 组的同局部系：x=0 = 左端面、y=0 = 轴线）
        let normal = build(&parse_program("SPLINE 6x23x26x6 L30").unwrap(), 1.0).unwrap();
        std::fs::write(path.join("spline_shaft_normal.csv"), entities_csv(&normal.entities))
            .expect("写 CSV");
        let section = build(
            &parse_program("SPLINE 6x23x26x6 L30 VIEW 剖视").unwrap(),
            1.0,
        )
        .unwrap();
        std::fs::write(path.join("spline_shaft_section.csv"), entities_csv(&section.entities))
            .expect("写 CSV");
        std::fs::write(
            path.join("spline_shaft_section_hatch.csv"),
            hatch_csv(&section.entities),
        )
        .expect("写 HATCH CSV");
        assert!(path.join("spline_shaft_normal.csv").is_file());
        // 带 φ22 引入倒角 + 同径后续段的轴（对齐模板第 4 组的局部系：左端面 x=5 ↔ 模板 47.988）
        let lead = build(
            &parse_program("S22 E22 L5 CH2@R | SPLINE 6x23x26x6 L30 | S26 E26 L40").unwrap(),
            1.0,
        )
        .unwrap();
        std::fs::write(path.join("spline_shaft_with_lead.csv"), entities_csv(&lead.entities))
            .expect("写 CSV");
    }

    #[test]
    fn gear_geometry_addendum_chamfer_pitch_and_root_line_by_view() {
        // 第 1 段 Ø40，第 2 段齿轮 M3 Z20（x=20..50）：d=60、ra=33、rf=26.25、C=2
        let program = parse_program("S40 E40 L20\nGEAR M3 Z20 H30").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let gear = program.segments[1].gear.unwrap();
        let (r, ra, rf) = (
            gear.pitch_radius(),
            gear.addendum_radius(),
            gear.root_radius(),
        );
        // 齿顶面 = 齿宽两端各缩进 C = round(0.6m) = 2（与 gear.rs::side_view() 同）
        assert!(has_line(&shaft, [22.0, ra], [48.0, ra]));
        assert!(has_line(&shaft, [22.0, -ra], [48.0, -ra]));
        // 四个 45° 倒角（端面点 ra−C=31 → 齿顶 ra=33）
        assert!(has_line(&shaft, [20.0, 31.0], [22.0, 33.0]));
        assert!(has_line(&shaft, [20.0, -31.0], [22.0, -33.0]));
        assert!(has_line(&shaft, [48.0, 33.0], [50.0, 31.0]));
        assert!(has_line(&shaft, [48.0, -33.0], [50.0, -31.0]));
        // 台阶竖线（仅常规）：x=22/48、±ra
        assert!(has_line(&shaft, [22.0, -33.0], [22.0, 33.0]));
        assert!(has_line(&shaft, [48.0, -33.0], [48.0, 33.0]));
        let layer_of_y = |y: f64| -> Option<&str> {
            shaft.entities.iter().find_map(|e| match e {
                EntityType::Line(l)
                    if near(l.start.y, y)
                        && near(l.end.y, y)
                        && near(l.start.x, 20.0)
                        && near(l.end.x, 50.0) =>
                {
                    Some(l.common.layer.as_str())
                }
                _ => None,
            })
        };
        // 分度线 → 3中心线层（点划线）
        assert!(has_line(&shaft, [20.0, r], [50.0, r]));
        assert!(has_line(&shaft, [20.0, -r], [50.0, -r]));
        assert_eq!(layer_of_y(r), Some(crate::partgen_kit::LAYER_CENTER));
        // 齿根线：与 gear.rs::side_view() 一致，常规视图**不画**（剖视才画）
        assert!(!has_line(&shaft, [20.0, rf], [50.0, rf]), "常规不画齿根线");
        assert!(!has_line(&shaft, [20.0, -rf], [50.0, -rf]), "常规不画齿根线");
        assert_eq!(layer_of_y(rf), None, "齿根线上没有图元");
        // 分度圆半径 30 不是轮廓（轮廓在齿顶 33）
        assert!(!shaft.entities.iter().any(|e| matches!(e, EntityType::Line(l)
            if near(l.start.y, r) && near(l.end.y, r) && l.common.layer == crate::partgen_kit::LAYER_MAIN)));
        // 端面：x=20 从 Ø40/2=20 到 ra−C=31；x=50 自由端面 ±31
        assert!(has_line(&shaft, [20.0, 20.0], [20.0, 31.0]));
        assert!(has_line(&shaft, [50.0, -31.0], [50.0, 31.0]));
        // 剖视：齿根线可见（也是剖面线边界）
        let section = build(
            &parse_program("S40 E40 L20\nGEAR M3 Z20 H30 | VIEW 剖视").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(has_line(&section, [20.0, rf], [50.0, rf]));
        assert!(has_line(&section, [20.0, -rf], [50.0, -rf]));
        assert!(!has_line(&section, [20.0, -31.0], [20.0, 31.0]), "剖视无贯通端面线");
        // 最大直径按齿顶圆算
        assert!(near(shaft.max_diameter, 66.0));
        assert!(near(shaft.total_length, 50.0));
    }

    #[test]
    fn gear_adjacent_diameter_rule_only_addendum_matters() {
        // Ø58（r=29 > 齿根 26.25，但 ≤ 齿顶 33）→ 放行（不再受 rf 限制）
        let program = parse_program("S58 E58 L20\nGEAR M3 Z20").unwrap();
        assert!(build(&program, 1.0).is_ok(), "相邻半径 ≤ ra 应放行");
        // 和齿顶齐平（Ø66）也放行
        let program = parse_program("S66 E66 L20\nGEAR M3 Z20").unwrap();
        assert!(build(&program, 1.0).is_ok());
        // Ø70（r=35 > 齿顶 33）→ 报「第 N 段」+「会盖住齿顶线」
        let program = parse_program("S70 E70 L20\nGEAR M3 Z20").unwrap();
        let err = build(&program, 1.0).unwrap_err();
        assert!(err.contains("第 2 段") && err.contains("会盖住齿顶线"), "{err}");
        // 同参数相邻齿轮（齿顶齐平）→ 放行
        let program = parse_program("GEAR M3 Z20 H20\nGEAR M3 Z20 H30").unwrap();
        assert!(build(&program, 1.0).is_ok());
        // 倒角会落在齿轮段上 → 报（第 1 段 Ø40 的 CH2@R 贴到齿轮凸角）
        let program = parse_program("S40 E40 L20 CH2@R\nGEAR M3 Z20").unwrap();
        let err = build(&program, 1.0).unwrap_err();
        assert!(err.contains("齿轮段"), "{err}");
    }

    /// 轴上齿轮/渐开线花键端面自动倒角的**单侧规则**（用户 2026-09-21 定案）：
    /// 自由端 / 邻段更小 → 加；邻段更大（肩部）/ 相等（齐平无外角）→ 不加。
    #[test]
    fn end_chamfer_one_sided_rule_on_shaft() {
        // ── ① 自由端（轴头/轴尾）→ 加：`GEAR` 单段两端都倒 ──
        let program = parse_program("GEAR M3 Z20 H30").unwrap();
        let gear = program.segments[0].gear.unwrap();
        let (ra, c) = (gear.addendum_radius(), gear.chamfer());
        assert!(near(c, 2.0), "C = round(0.6×3) = 2");
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_line(&shaft, [0.0, ra - c], [c, ra]), "左自由端倒角斜线");
        assert!(has_line(&shaft, [30.0 - c, ra], [30.0, ra - c]), "右自由端倒角斜线");
        assert!(has_line(&shaft, [c, ra], [30.0 - c, ra]), "齿顶面两端缩 C");

        // ── ② 邻段更小（台阶向下）→ 加 ──
        let shaft = build(
            &parse_program("S40 E40 L20\nGEAR M3 Z20 H30").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(has_line(&shaft, [20.0, ra - c], [20.0 + c, ra]), "邻段更小 → 左侧加");
        assert!(has_line(&shaft, [20.0 + c, ra], [50.0 - c, ra]), "齿顶面左端缩 C");

        // ── ③ 邻段相等（端面齐平、无外角）→ 不加 ──
        let shaft = build(
            &parse_program("S66 E66 L20\nGEAR M3 Z20 H30").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(
            !has_line(&shaft, [20.0, ra - c], [20.0 + c, ra]),
            "齐平侧不许有倒角斜线"
        );
        assert!(
            !has_line(&shaft, [20.0 + c, -ra], [20.0 + c, ra]),
            "齐平侧无倒角根竖线"
        );
        assert!(has_line(&shaft, [20.0, ra], [50.0 - c, ra]), "齿顶面从端面直起");

        // `GEAR` 段相邻半径 > ra 仍被既有校验拦住（见上条测试），更大侧分支在
        // `GEAR` 上不可达；用渐开线花键段验证“更大/齐平”两侧。
        // ── `INVOLSPLINE GB30R M3 Z20`：da/2 = 31.5、C = round(0.6×3) = 2 ──
        let program = parse_program("INVOLSPLINE GB30R M3 Z20 L30").unwrap();
        let iv = program.segments[0].invol_spline.as_ref().unwrap();
        let (sr, sc) = (
            iv.major_radius(),
            (crate::gear::CHAMFER_RATIO * iv.params.m).round(),
        );
        assert!(near(sr, 31.5) && near(sc, 2.0), "da/2={sr}，C={sc}");
        // 自由端 → 两端加
        let shaft = build(&program, 1.0).unwrap();
        assert!(has_line(&shaft, [0.0, sr - sc], [sc, sr]), "花键左自由端倒角");
        assert!(
            has_line(&shaft, [30.0 - sc, sr], [30.0, sr - sc]),
            "花键右自由端倒角"
        );

        // ── ④ 邻段更小（Ø40 < da63）→ 加 ──
        let shaft = build(
            &parse_program("S40 E40 L20\nINVOLSPLINE GB30R M3 Z20 L30").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(
            has_line(&shaft, [20.0, sr - sc], [20.0 + sc, sr]),
            "花键邻段更小 → 加"
        );
        assert!(
            has_line(&shaft, [20.0 + sc, sr], [50.0 - sc, sr]),
            "大径线左端缩 C"
        );

        // ── ⑤ 邻段更大（Ø70 > da63，肩部）→ 不加；右端自由仍倒 ──
        let shaft = build(
            &parse_program("S70 E70 L20\nINVOLSPLINE GB30R M3 Z20 L30").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(
            !has_line(&shaft, [20.0, sr - sc], [20.0 + sc, sr]),
            "肩部侧不许有倒角斜线"
        );
        assert!(
            !has_line(&shaft, [20.0 + sc, -sr], [20.0 + sc, sr]),
            "肩部侧无倒角根竖线"
        );
        assert!(
            has_line(&shaft, [20.0, sr], [50.0 - sc, sr]),
            "大径线从端面直起"
        );
        assert!(
            has_line(&shaft, [50.0 - sc, sr], [50.0, sr - sc]),
            "右自由端倒角仍在"
        );

        // ── ⑥ 邻段相等（Ø63 = da）→ 不加 ──
        let shaft = build(
            &parse_program("S63 E63 L20\nINVOLSPLINE GB30R M3 Z20 L30").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(
            !has_line(&shaft, [20.0, sr - sc], [20.0 + sc, sr]),
            "齐平侧不许有倒角斜线"
        );
        assert!(has_line(&shaft, [20.0, sr], [50.0 - sc, sr]), "大径线从端面直起");
    }

    /// 单侧规则的组合冒烟：齿轮/花键 × 自由/更小/更大/相等 × 有无 de × 左右位置，
    /// 构建不许 panic / 报错。
    #[test]
    fn end_chamfer_rule_combinations_build() {
        for text in [
            "GEAR M3 Z20 H30",
            "S40 E40 L20\nGEAR M3 Z20 H30",
            "S66 E66 L20\nGEAR M3 Z20 H30",
            "GEAR M3 Z20 H30\nS40 E40 L20",
            "INVOLSPLINE GB30R M3 Z20 L30",
            "INVOLSPLINE GB30R M3 Z20 L30 de70",
            "S40 E40 L20\nINVOLSPLINE GB30R M3 Z20 L30",
            "S70 E70 L20\nINVOLSPLINE GB30R M3 Z20 L30",
            "S63 E63 L20\nINVOLSPLINE GB30R M3 Z20 L30",
            "S40 E40 L20\nINVOLSPLINE GB30R M3 Z20 L30 de70",
            "S70 E70 L20\nINVOLSPLINE GB30R M3 Z20 L30 de70",
            "INVOLSPLINE GB30R M3 Z20 L30\nS40 E40 L20",
            "INVOLSPLINE GB30R M3 Z20 L30 de70\nS40 E40 L20",
            "S22 E22 L5 CH2@R\nINVOLSPLINE GB30R M3 Z20 L30",
        ] {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            let shaft = build(&program, 1.0).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert!(!shaft.entities.is_empty(), "{text}");
        }
    }

    /// 回归：独立齿轮生成器（`gear.rs::side_view()`）是独立零件、两端都是自由外角，
    /// **仍两端都倒** —— 轴上单侧规则不反向影响它。
    #[test]
    fn independent_gear_generator_still_chamfers_both_ends() {
        let p = crate::gear::GearParams { m: 3.0, z: 20, h: 30.0, ..Default::default() };
        let ra = p.da() / 2.0;
        let c = p.chamfer();
        let (hh, hc, hi) = (15.0, 15.0 - c, ra - c);
        let v = crate::gear::side_view(&p, 1.0).unwrap();
        let has = |a: [f64; 2], b: [f64; 2]| {
            v.iter().any(|e| match e {
                EntityType::Line(l) => {
                    let (p0, p1) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
                    (near(p0[0], a[0])
                        && near(p0[1], a[1])
                        && near(p1[0], b[0])
                        && near(p1[1], b[1]))
                        || (near(p0[0], b[0])
                            && near(p0[1], b[1])
                            && near(p1[0], a[0])
                            && near(p1[1], a[1]))
                }
                _ => false,
            })
        };
        assert!(has([-hh, hi], [-hc, ra]), "左端倒角斜线");
        assert!(has([hc, ra], [hh, hi]), "右端倒角斜线");
        assert!(has([-hh, -hi], [-hc, -ra]), "左下倒角斜线");
        assert!(has([hc, -ra], [hh, -hi]), "右下倒角斜线");
        assert!(has([-hh, -hi], [-hh, hi]), "左端面 ±(ra−C)");
        assert!(has([hh, -hi], [hh, hi]), "右端面 ±(ra−C)");
    }

    // ── 视图 VIEW（常规 / 剖视 / 双） ────────────────────────────────────

    #[test]
    fn dsl_view_parses_default_inline_standalone_and_json() {
        // 默认 = 常规
        assert_eq!(parse_program("S30 E30 L10").unwrap().view, ShaftView::Normal);
        // 独立一行 / 段内关键字 / = 或 : / 中文或英文
        for (text, want) in [
            ("VIEW 剖视\nS30 E30 L10", ShaftView::Section),
            ("S30 E30 L10 VIEW 双", ShaftView::Both),
            ("VIEW=section S30 E30 L10", ShaftView::Section),
            ("S30 E30 L10 视图:剖视图", ShaftView::Section),
            ("VIEW 常规\nS30 E30 L10", ShaftView::Normal),
            ("S30 E30 L10", ShaftView::Normal),
        ] {
            assert_eq!(parse_program(text).unwrap().view, want, "{text}");
        }
        // 所有键可解析回自身
        for view in ShaftView::ALL {
            assert_eq!(ShaftView::parse(view.key()).unwrap(), view);
        }
        // 多段行里也可放（且不影响段数）
        let program = parse_program("VIEW 剖视 | S30 E30 L10 | S20 E20 L5").unwrap();
        assert_eq!(program.view, ShaftView::Section);
        assert_eq!(program.segments.len(), 2);
        // JSON 同名字段：英文键或中文名
        for view in ["section", "剖视", "both", "双", "normal"] {
            let text = format!(r#"{{"segments":[{{"s":30,"l":10}}],"view":"{view}"}}"#);
            assert_eq!(
                parse_program(&text).unwrap().view,
                ShaftView::parse(view).unwrap()
            );
        }
        // 序列化回传（GUI/HTTP）：view 键可来回
        let program = parse_program("VIEW 双\nS30 E30 L10").unwrap();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"view\":\"both\""), "{json}");
        assert_eq!(parse_program(&json).unwrap(), program);
        // 错误：视图名非法 / 缺名 / 冲突
        let err = parse_program("VIEW 隐藏\nS30 E30 L10").unwrap_err();
        assert!(err.contains("视图名无法识别"), "{err}");
        let err = parse_program("VIEW\nS30 E30 L10").unwrap_err();
        assert!(err.contains("缺少视图名"), "{err}");
        let err = parse_program("VIEW 剖视\nS30 E30 L10\nVIEW 双").unwrap_err();
        assert!(err.contains("重复且冲突"), "{err}");
        // 块名把视图算进去（常规 ≠ 剖视）
        let normal = parse_program("S30 E30 L10").unwrap();
        let section = parse_program("S30 E30 L10 VIEW 剖视").unwrap();
        assert_ne!(block_name(&normal), block_name(&section));
    }

    #[test]
    fn section_view_hatch_ring_closes_on_hatch_layer() {
        // 带自由端倒角 + 越程槽 + 锥面：确认环每段接得上、最后闭合
        let program = parse_program(
            "S30 E30 L20 CH2@L\nS40 E40 L10 OV3\nS50 E30 L12\nVIEW 剖视",
        )
        .unwrap();
        // 常规视图没有 HATCH
        let normal = build(
            &Program {
                view: ShaftView::Normal,
                ..program.clone()
            },
            1.0,
        )
        .unwrap();
        assert!(!normal.entities.iter().any(|e| matches!(e, EntityType::Hatch(_))));
        let shaft = build(&program, 1.0).unwrap();
        let hatches: Vec<&Hatch> = shaft
            .entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .collect();
        assert_eq!(hatches.len(), 1, "剖视只有一个 HATCH（轴线上/下两环）");
        let h = hatches[0];
        assert_eq!(h.common.layer, crate::partgen_kit::LAYER_HATCH);
        assert_eq!(h.pattern.name, "ANSI31");
        assert_eq!(h.pattern_scale, 1.0);
        assert_eq!(h.paths.len(), 2, "轴线上/下两个环（参考件口径）");
        // 边界首尾相接（每个 edge 的终点 = 下一个 edge 的起点），两环都要闭合
        let pts = |edge: &BoundaryEdge| -> ([f64; 2], [f64; 2]) {
            match edge {
                BoundaryEdge::Line(e) => ([e.start.x, e.start.y], [e.end.x, e.end.y]),
                BoundaryEdge::CircularArc(a) => (
                    [
                        a.center.x + a.radius * a.start_angle.cos(),
                        a.center.y + a.radius * a.start_angle.sin(),
                    ],
                    [
                        a.center.x + a.radius * a.end_angle.cos(),
                        a.center.y + a.radius * a.end_angle.sin(),
                    ],
                ),
                other => panic!("剖面环只应有 LINE/ARC，得到 {other:?}"),
            }
        };
        // 环的 bbox 盖住整个轴剖面（总长 42，最大半径 25）
        let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
        let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for path in &h.paths {
            let edges = &path.edges;
            assert!(edges.len() >= 8, "每环至少 8 段：{}", edges.len());
            let mut prev: Option<[f64; 2]> = None;
            let mut first: Option<[f64; 2]> = None;
            for edge in edges {
                let (a, b) = pts(edge);
                if let Some(p) = prev {
                    assert!(near(p[0], a[0]) && near(p[1], a[1]), "边界断开：{p:?} → {a:?}");
                } else {
                    first = Some(a);
                }
                prev = Some(b);
                for p in [a, b] {
                    x0 = x0.min(p[0]);
                    x1 = x1.max(p[0]);
                    y0 = y0.min(p[1]);
                    y1 = y1.max(p[1]);
                }
            }
            let start = first.unwrap();
            let last = prev.unwrap();
            assert!(
                near(last[0], start[0]) && near(last[1], start[1]),
                "环未闭合：{last:?} → {start:?}"
            );
        }
        assert!(
            near(x0, 0.0) && near(x1, 42.0) && near(y0, -25.0) && near(y1, 25.0),
            "环 bbox = ({x0},{y0})..({x1},{y1})"
        );
    }

    #[test]
    fn both_view_places_section_side_by_side_with_gap_rule() {
        // 短轴：间距 = max(20×15%=3, 40×1) 取 40 → 第二张起点 x = 20 + 40 = 60
        let program = parse_program("S30 E30 L20 VIEW 双").unwrap();
        let normal = build(
            &Program {
                view: ShaftView::Normal,
                ..program.clone()
            },
            1.0,
        )
        .unwrap();
        let both = build(&program, 1.0).unwrap();
        assert_eq!(
            both.entities.len(),
            normal.entities.len() * 2 + 1,
            "常规×2 + 一个 HATCH"
        );
        assert!(has_line(&both, [60.0, 15.0], [80.0, 15.0]), "第二张轮廓右移 60");
        assert!(has_line(&both, [57.0, 0.0], [83.0, 0.0]), "第二张轴线");
        // 双视图：左常规保留贯通竖线，右剖视不保留（用带段边界的轴验证）
        let two = parse_program("S30 E30 L20 | S40 E40 L10 VIEW 双").unwrap();
        let both2 = build(&two, 1.0).unwrap();
        // 间距 = max(30×15%=4.5, 40) = 40 → 第二张 +70
        assert!(has_line(&both2, [20.0, -15.0], [20.0, 15.0]), "左常规贯通竖线");
        assert!(
            !has_line(&both2, [90.0, -15.0], [90.0, 15.0]),
            "右剖视不画贯通竖线"
        );
        assert!(has_line(&both2, [90.0, 15.0], [90.0, 20.0]), "右剖视真实端面");
        // 第二张的 HATCH 边界也右移 60
        let hatch = both
            .entities
            .iter()
            .find_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .expect("双视图要有一个 HATCH");
        let mut min_x = f64::INFINITY;
        for path in &hatch.paths {
            for edge in &path.edges {
                if let BoundaryEdge::Line(e) = edge {
                    min_x = min_x.min(e.start.x).min(e.end.x);
                }
            }
        }
        assert!(near(min_x, 60.0), "HATCH 边界右移：min_x={min_x}");
        // 长轴：间距 = 总长×15%（400 → 60）
        let long = parse_program("S30 E30 L400 VIEW 双").unwrap();
        let both_long = build(&long, 1.0).unwrap();
        assert!(
            has_line(&both_long, [460.0, 15.0], [860.0, 15.0]),
            "总长 400 时间距 = 60（15%）"
        );
    }

    #[test]
    fn place_transforms_hatch_boundaries() {
        let program = parse_program("S30 E30 L20 VIEW 剖视").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let bbox = |entities: &[EntityType]| -> [f64; 4] {
            let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
            let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for e in entities {
                if let EntityType::Hatch(h) = e {
                    for path in &h.paths {
                        for edge in &path.edges {
                            match edge {
                                BoundaryEdge::Line(e) => {
                                    for p in [e.start, e.end] {
                                        x0 = x0.min(p.x);
                                        x1 = x1.max(p.x);
                                        y0 = y0.min(p.y);
                                        y1 = y1.max(p.y);
                                    }
                                }
                                BoundaryEdge::CircularArc(a) => {
                                    for (px, py) in [
                                        (a.center.x + a.radius * a.start_angle.cos(),
                                         a.center.y + a.radius * a.start_angle.sin()),
                                        (a.center.x + a.radius * a.end_angle.cos(),
                                         a.center.y + a.radius * a.end_angle.sin()),
                                    ] {
                                        x0 = x0.min(px);
                                        x1 = x1.max(px);
                                        y0 = y0.min(py);
                                        y1 = y1.max(py);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            [x0, y0, x1, y1]
        };
        // 平移：边界 bbox → (100,35)..(120,65)
        let placed = place(shaft.entities, [100.0, 50.0], 0.0);
        let b = bbox(&placed);
        assert!(near(b[0], 100.0) && near(b[1], 35.0) && near(b[2], 120.0) && near(b[3], 65.0), "{b:?}");
        // 旋转 90°：原 (x,y) → (−y,x)，bbox x ∈ [−65,−35]、y ∈ [100,120]
        let rotated = place(placed, [0.0, 0.0], 90.0);
        let b = bbox(&rotated);
        assert!(near(b[0], -65.0) && near(b[1], 100.0) && near(b[2], -35.0) && near(b[3], 120.0), "{b:?}");
    }

    // ── 合法性 ────────────────────────────────────────────────────────────

    #[test]
    fn validity_errors_point_at_the_segment() {
        let cases: &[(&str, &str)] = &[
            ("S30 E30 L0", "第 1 段"),
            ("S30 E30 L-5", "第 1 段"),
            ("S0 E30 L10", "第 1 段"),
            ("S30 E-1 L10", "第 1 段"),
            ("S30 E30 L10 CH5@L", "段长/2"),
            ("S30 E30 L20 CH10@L", "段长/2"),
            // C ≥ 直径变化量的一半（Ø30→Ø40 的 5）
            ("S30 E30 L20 CH5@R\nS40 E40 L10", "直径变化量"),
            // 直径相同 → 没有端面可倒角
            ("S30 E30 L20 CH2@R\nS30 E30 L10", "没有端面"),
            // b1 > 段长（显式 3 > 2）
            ("S50 E50 L2 OV3\nS60 E60 L10", "b1=3 > 段长"),
            // b1 > 段长（查表默认 5 > 4；d=60 落 50<d<100 档）
            ("S60 E60 L4 OV\nS70 E70 L10", "b1=5 > 段长"),
            // 显式 b1 不在该 d 档的表里
            ("S40 E40 L20 OV4\nS50 E50 L10", "可选 b1"),
            // 两端槽重叠（2 + 2 > 3；两档都能取 b1=2）
            ("S60 E60 L10\nS50 E50 L3 OV@L OV@R\nS60 E60 L10", "两端特征重叠"),
            // 没有台阶（相邻段更小）
            ("S50 E50 L20 OV\nS40 E40 L10", "没有台阶面"),
            // 自由端不能做槽
            ("S50 E50 L20 OV", "自由端"),
            // 台阶太小：R 圆角切点（25.6）高过相邻台阶面（25.5）
            ("S50 E50 L20 OV\nS51 E51 L10", "台阶太小"),
            // 锥面不能做槽
            ("S50 E30 L20 OV\nS60 E60 L10", "锥面"),
            // 陡锥自由端倒角可能把端面吃穿（C < 端面半径也会发生）
            ("S10 E0.2 L10 CH4.9@L", "吃穿"),
            // 同一端面两侧都写倒角
            ("S30 E30 L20 CH2@R\nS40 E40 L10 CH2@L", "特征重叠"),
        ];
        for (text, needle) in cases {
            let program = parse_program(text).unwrap_or_else(|e| panic!("{text}: 解析失败 {e}"));
            let err = build(&program, 1.0).unwrap_err();
            assert!(err.contains(needle), "{text}: 期望含「{needle}」，得到 {err}");
        }
    }

    // ── 轴线 / 放置 ───────────────────────────────────────────────────────

    #[test]
    fn axis_length_is_total_plus_6n() {
        let program = parse_program("S30 E30 L45\nS40 E30 L20").unwrap();
        for (n, total, start, end) in [(1.0, 71.0, -3.0, 68.0), (2.0, 77.0, -6.0, 71.0)] {
            let shaft = build(&program, n).unwrap();
            let axis = shaft
                .entities
                .iter()
                .filter_map(|e| match e {
                    EntityType::Line(l) if l.common.layer == LAYER_CENTER => {
                        Some((l.start.x, l.end.x))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(axis.len(), 1, "轴线只有一条");
            let (start_x, end_x) = axis[0];
            assert!(near(end_x - start_x, total), "n={n}: {start_x}..{end_x} 长 {}", end_x - start_x);
            assert!(near(start_x, start) && near(end_x, end), "n={n}: {start_x}..{end_x}");
        }
    }

    #[test]
    fn place_rotates_and_translates() {
        let program = parse_program("S30 E30 L10").unwrap();
        let shaft = build(&program, 1.0).unwrap();
        let placed = place(shaft.entities, [100.0, 50.0], 0.0);
        let has = |entities: &Vec<EntityType>, a: [f64; 2], b: [f64; 2]| {
            entities.iter().any(|e| match e {
                EntityType::Line(l) => {
                    let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
                    (near(p[0], a[0]) && near(p[1], a[1]) && near(q[0], b[0]) && near(q[1], b[1]))
                        || (near(p[0], b[0])
                            && near(p[1], b[1])
                            && near(q[0], a[0])
                            && near(q[1], a[1]))
                }
                _ => false,
            })
        };
        assert!(has(&placed, [97.0, 50.0], [113.0, 50.0]), "平移");
        let rotated = place(placed, [0.0, 0.0], 90.0);
        // 轴线 (97,50)..(113,50) 绕原点转 90° → (−50,97)..(−50,113)
        assert!(has(&rotated, [-50.0, 97.0], [-50.0, 113.0]), "旋转");
    }

    // ── 交付：demo 几何 dump（人工核对用） ────────────────────────────────

    /// 把 7 段 demo 的几何落成 CSV（列：entity,x1,y1,x2,y2,layer,cx,cy,r,a0,a1），
    /// 给用户画叠合/预览图。LINE 用两端点；ARC 用起终点 + 圆心/半径/角度。
    #[test]
    fn dump_demo_geometry_csv() {
        let program = parse_program(DEMO).unwrap();
        let shaft = build(&program, 1.0).unwrap();
        assert!(near(shaft.total_length, 175.0));
        assert!(near(shaft.max_diameter, 66.0));
        assert_eq!(shaft.segment_count, 7);

        let csv = entities_csv(&shaft.entities);

        // 关键几何自检（demo 7 段：30 / 40（OV3 + φ50 台阶倒角）/ 锥 50→30 / 30 /
        // 40（M1.5 TL20 局部螺纹）/ 36（小直径槽段）/ 齿轮段 M3 Z20（d=60、da=66、df=52.5 不落图））
        // OV3（φ40）：三条贯通竖线（槽肩 = 段边界 x=75）；砂轮细线不再画
        assert!(
            has_line(&shaft, [75.0, -20.6], [75.0, 20.6]),
            "OV 槽肩贯通竖线（×20.6 = d/2+r−h）"
        );
        assert!(
            has_line(&shaft, [72.4, -19.6], [72.4, 19.6]),
            "OV 槽底终止贯通竖线"
        );
        assert!(
            has_line(&shaft, [72.0, -20.0], [72.0, 20.0]),
            "OV 斜壁终点贯通竖线"
        );
        assert!(
            !has_line(&shaft, [72.0, 20.0], [57.0, 35.0]),
            "OV 砂轮细线不再画（用户 2026-09-18 定案：青线属标注）"
        );
        assert!(
            has_line(&shaft, [75.0, 22.0], [77.0, 24.0]),
            "φ50 台阶左端的 CH2（贴凸角）"
        );
        assert!(
            has_line(&shaft, [110.0, 15.0], [110.0, 18.0]),
            "前段 φ30 → φ40 台阶面"
        );
        assert!(has_line(&shaft, [110.0, 18.0], [112.0, 20.0]), "φ40 段左端 CH2");
        // ── M1.5 TL20 局部螺纹（GB/T 3 图 1 第一种形式；P=1.5 → x=3.8、a=4.5）──
        // 段 [110,140]：完整螺纹从 115.5 起；台肩面=140、分界=135.5、螺尾起点=139.3
        let r1 = (40.0 - 1.0825 * 1.5) / 2.0;
        assert!(near(r1, 19.188125));
        assert!(has_line(&shaft, [112.0, 20.0], [140.0, 20.0]), "螺纹段大径轮廓");
        assert!(has_line(&shaft, [140.0, 20.0], [139.3, r1]), "锥面过渡（a−x=0.7）");
        assert!(has_line(&shaft, [139.3, r1], [135.5, 20.0]), "螺尾细实线（x=3.8）");
        assert!(
            has_line(&shaft, [135.5, -20.0], [135.5, 20.0]),
            "完整螺纹/螺尾分界竖线"
        );
        assert!(has_line(&shaft, [115.5, r1], [139.3, r1]), "小径细实线（止于螺尾起点）");
        assert!(has_line(&shaft, [115.5, -r1], [139.3, -r1]));
        assert!(
            has_line(&shaft, [140.0, 18.0], [140.0, 20.0]),
            "螺纹终止台肩面（18 ↔ 20）"
        );
        assert_eq!(
            layer_of_line(&shaft, [139.3, r1], [135.5, 20.0]),
            Some(LAYER_THIN),
            "demo 螺尾落 2细线层"
        );
        assert_eq!(
            layer_of_line(&shaft, [140.0, 20.0], [139.3, r1]),
            Some(LAYER_MAIN),
            "demo 锥面落 1轮廓实线层"
        );
        // 小直径槽段 [140,145]：轮廓 18，端面 145 从 18 到 ra−C=31（齿轮端倒角 C=2）
        assert!(has_line(&shaft, [140.0, 18.0], [145.0, 18.0]), "小直径槽段轮廓");
        assert!(
            has_line(&shaft, [145.0, 18.0], [145.0, 31.0]),
            "槽段端面从 18 到 ra−C=31"
        );
        assert!(has_line(&shaft, [145.0, 31.0], [147.0, 33.0]), "齿轮左端倒角");
        assert!(has_line(&shaft, [173.0, 33.0], [175.0, 31.0]), "齿轮右端倒角");
        // 齿轮：齿顶面（h−2C = 26，±33）/ 分度（点划线，±30）；常规视图齿根线（±26.25）不画
        assert!(has_line(&shaft, [147.0, 33.0], [173.0, 33.0]), "齿顶面（缩进 2C）");
        assert!(has_line(&shaft, [145.0, 30.0], [175.0, 30.0]), "分度线");
        assert!(
            !has_line(&shaft, [145.0, 26.25], [175.0, 26.25]),
            "常规视图齿根线不画（与 gear.rs::side_view 一致）"
        );
        assert!(
            !has_line(&shaft, [145.0, -26.25], [175.0, -26.25]),
            "常规视图下半齿根线也不画"
        );
        assert!(
            has_line(&shaft, [175.0, -31.0], [175.0, 31.0]),
            "齿轮自由端面 ±(ra−C)"
        );
        // 齿轮端倒角的台阶竖线（±ra，仅常规视图）
        assert!(has_line(&shaft, [147.0, -33.0], [147.0, 33.0]), "齿轮左台阶竖线");
        assert!(has_line(&shaft, [173.0, -33.0], [173.0, 33.0]), "齿轮右台阶竖线");
        let layer_at = |x: f64, y: f64| -> Option<&str> {
            shaft.entities.iter().find_map(|e| match e {
                EntityType::Line(l)
                    if near(l.start.x, x) && near(l.start.y, y) && near(l.end.y, y) =>
                {
                    Some(l.common.layer.as_str())
                }
                _ => None,
            })
        };
        assert_eq!(
            layer_at(145.0, 30.0),
            Some(crate::partgen_kit::LAYER_CENTER)
        );
        assert_eq!(layer_at(145.0, 26.25), None, "常规视图没有齿根线");

        let path = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("桌面/OCSM/review");
        std::fs::create_dir_all(&path).expect("建 review 目录");
        let file = path.join("shaft_demo.csv");
        std::fs::write(&file, &csv).expect("写 shaft_demo.csv");
        assert!(file.is_file(), "demo CSV 已落盘：{}", file.display());
        assert!(csv.contains("LINE") && csv.contains("ARC"));
        assert!(csv.contains("3中心线层") && csv.contains("2细线层"));
        // 65 = 旧 demo 59 + 齿轮倒角 6（4 斜线 + 2 台阶竖线）；
        // 旧 59 = 旧 demo 44 + 局部螺纹新增 5（锥面上下 2 + 螺尾上下 2 + 分界竖线 1）
        //      + 段边界贯通竖线 6 + 倒角终点竖线 3 + OV 槽两条界线 2 − OV 砂轮细线 1
        assert_eq!(shaft.entities.len(), 65, "demo 图元数");

        // 剖视：真实几何 + 齿轮齿根线（2 条）+ 一个 HATCH（2 环）；不再画贯通竖线
        let section_program = parse_program(&format!("{DEMO}\nVIEW 剖视")).unwrap();
        let section = build(&section_program, 1.0).unwrap();
        // 65 − 13 贯通竖线 + 2 齿根线 + 1 HATCH = 55
        assert_eq!(section.entities.len(), 55, "剖视图元数");
        assert!(has_line(&section, [145.0, 26.25], [175.0, 26.25]), "剖视齿根线");
        assert!(
            !has_line(&section, [45.0, -15.0], [45.0, 15.0]),
            "剖视不画贯通竖线"
        );
        assert!(
            !has_line(&section, [75.0, -20.6], [75.0, 20.6]),
            "剖视不画槽肩贯通竖线"
        );
        let section_csv = entities_csv(&section.entities);
        assert!(
            section_csv.contains("HATCH,0.000000,-26.250000,175.000000,26.250000,5剖面线层"),
            "HATCH 一行记 bbox + 5剖面线层：{section_csv}"
        );
        let file = path.join("shaft_demo_section.csv");
        std::fs::write(&file, &section_csv).expect("写 shaft_demo_section.csv");
        assert!(file.is_file(), "剖面 CSV 已落盘：{}", file.display());
    }

    /// 用户实测轴（8 段，第 8 段 `S25 E25 L32 RL@L P1.5`）→
    /// `~/桌面/OCSM/review/轴测试_RL.csv`（列格式同 `shaft_demo.csv`）。
    #[test]
    fn dump_rl_review_axis_csv() {
        let text = "S28 E28 L55 CH2@L | S34 E34 L33 | S34 E35 L1 | S35 E35 L22 | S45 E45 L194 | S35 E35 L22 | S35 E34 L1 | S25 E25 L32 RL@L P1.5";
        let program = parse_program(text).expect("用户轴应能解析");
        let shaft = build(&program, 1.0).expect("用户轴应能出图");
        assert_eq!(shaft.segment_count, 8);
        assert!(near(shaft.total_length, 360.0));
        // 第 8 段 [328, 360] Ø25；左邻第 7 段右端 Ø34（r=17）→ 台肩。
        // 表 2 P1.5、d=25 → dg=22.7 / g1=2.5 / g2=4.5 / r=0.8（切点 12.15）
        assert!(has_arc(&shaft, [328.8, 12.15], 0.8, 180.0, 270.0), "第 8 段左端上圆角");
        assert!(has_arc(&shaft, [328.8, -12.15], 0.8, 90.0, 180.0));
        assert!(has_line(&shaft, [328.8, 11.35], [330.5, 11.35]), "槽底");
        assert!(has_line(&shaft, [330.5, 11.35], [332.5, 12.5]), "斜壁");
        assert!(
            has_line(&shaft, [328.0, 12.15], [328.0, 17.0]),
            "台肩面（切点 → 第 7 段 Ø34/2）"
        );
        assert!(has_line(&shaft, [332.5, 12.5], [360.0, 12.5]), "槽后轮廓");
        // 落盘（LINE/ARC 列：x1,y1,x2,y2,layer,cx,cy,r,a0,a1）
        let csv = entities_csv(&shaft.entities);
        let path = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("桌面/OCSM/review");
        std::fs::create_dir_all(&path).expect("建 review 目录");
        let file = path.join("轴测试_RL.csv");
        std::fs::write(&file, &csv).expect("写 轴测试_RL.csv");
        assert!(file.is_file(), "RL 轴 CSV 已落盘：{}", file.display());
        assert!(csv.contains("LINE") && csv.contains("ARC"));
    }

    /// **验收基准**：用户手工更正版 `~/桌面/OCSM/review/轴测试更正.dxf`（44 图元）的
    /// 输入轴（8 段 DSL，第 8 段 `OV@L`）。本测试把新几何落成
    /// `~/桌面/OCSM/review/轴测试_v2.csv`，供 `compare_shaft.py` 与基准逐图元比对。
    ///
    /// 更正版相对旧版的差：+9 条贯通竖线（段边界 5 / 倒角终点 1 / OV 槽 3），
    /// −1 条 OV 砂轮细线；本测试把这 10 条钉住。
    #[test]
    fn dump_user_axis_v2_csv() {
        let text = "S28 E28 L55 CH2@L | S34 E34 L33 | S34 E35 L1 | S35 E35 L22 | S45 E45 L194 | S35 E35 L22 | S35 E34 L1 | S25 E25 L32 OV@L";
        let program = parse_program(text).expect("用户轴应能解析");
        let shaft = build(&program, 1.0).expect("用户轴应能出图");
        assert_eq!(shaft.segment_count, 8);
        assert!(near(shaft.total_length, 360.0));
        // 用户更正版比旧版多的 9 条 `1轮廓实线层` 贯通竖线（逐条与 DXF 对齐）
        for (x, h) in [
            (2.0, 14.0),   // 倒角终点 = 倒角根半径（φ28/2）
            (55.0, 14.0),  // 段1|2 分界 = min(14, 17)
            (88.0, 17.0),  // 段2|3 分界（φ34→φ34，同径也画）
            (89.0, 17.5),  // 段3|4 分界（1mm 锥段终点）
            (111.0, 17.5), // 段4|5 分界 = min(17.5, 22.5)
            (305.0, 17.5), // 段5|6 分界 = min(22.5, 17.5)
            (328.0, 13.1), // OV 槽肩 = 台肩面 + 圆角切点（d/2−h+r）
            (330.6, 12.1), // OV 槽底终止 = 台肩面 + b1−h
            (331.0, 12.5), // OV 斜壁终点 = 台肩面 + b1
        ] {
            assert_eq!(
                layer_of_line(&shaft, [x, -h], [x, h]),
                Some(LAYER_MAIN),
                "贯通竖线 x={x} ±{h} 应落 1轮廓实线层"
            );
        }
        // 旧版那条 OV 砂轮细线（2细线层、45°、长 15）不再有 → **本轴（无 M 段）**
        // 整图 0 条 2细线层（限定：带螺纹的轴仍有小径/螺尾细实线，见 demo 测试）
        let thin = shaft.entities.iter().filter(|e| layer_of(e) == LAYER_THIN).count();
        assert_eq!(thin, 0, "本轴无 M 段，2细线层 应为 0 条（非通用断言）");
        assert!(
            !has_line(&shaft, [331.0, 12.5], [346.0, 27.5]),
            "OV 砂轮细线不再画"
        );
        // 既有几何回归（抽查更正版仍在的轮廓/圆角/端面）
        assert!(has_line(&shaft, [0.0, 12.0], [2.0, 14.0]), "左端 C2 上斜线");
        assert!(has_line(&shaft, [2.0, 14.0], [55.0, 14.0]), "φ28 轮廓");
        assert!(has_line(&shaft, [55.0, 14.0], [55.0, 17.0]), "段1|2 肩面");
        assert!(has_line(&shaft, [55.0, 17.0], [88.0, 17.0]), "φ34 轮廓");
        assert!(has_line(&shaft, [88.0, 17.0], [89.0, 17.5]), "1mm 锥面");
        assert!(has_line(&shaft, [111.0, 22.5], [305.0, 22.5]), "φ45 轮廓");
        assert!(has_line(&shaft, [327.0, 17.5], [328.0, 17.0]), "段7 收锥");
        assert!(has_line(&shaft, [328.0, 13.1], [328.0, 17.0]), "OV 台肩面");
        assert!(has_arc(&shaft, [329.0, 13.1], 1.0, 180.0, 270.0), "OV 上圆角");
        assert!(has_line(&shaft, [329.0, 12.1], [330.6, 12.1]), "OV 槽底");
        assert!(has_line(&shaft, [330.6, 12.1], [331.0, 12.5]), "OV 斜坡");
        assert!(has_line(&shaft, [331.0, 12.5], [360.0, 12.5]), "φ25 轮廓");
        assert!(has_line(&shaft, [360.0, -12.5], [360.0, 12.5]), "右端面");
        assert!(has_line(&shaft, [-3.0, 0.0], [363.0, 0.0]), "轴线");
        // 文字规则「每条段边界都画」在 x=327（段6|7，Ø35→Ø35 锥段起点）也会画；
        // 用户手工更正版漏了这条（如 x=88 同构却画了）。验收 diff 见报告：
        // 新生成 45 = 基准 44 + 这一条（不为了对齐而删规则线）。
        assert_eq!(
            layer_of_line(&shaft, [327.0, -17.5], [327.0, 17.5]),
            Some(LAYER_MAIN),
            "段6|7 同径分界贯通竖线（基准漏画，本期按规则保留）"
        );
        // 落盘（列格式同 shaft_demo.csv / 轴测试_RL.csv）
        let csv = entities_csv(&shaft.entities);
        let path = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("桌面/OCSM/review");
        std::fs::create_dir_all(&path).expect("建 review 目录");
        let file = path.join("轴测试_v2.csv");
        std::fs::write(&file, &csv).expect("写 轴测试_v2.csv");
        assert!(file.is_file(), "v2 CSV 已落盘：{}", file.display());
        // 45 = 基准 44 + x=327 那条（旧版 36 = 35 + 9 − 1）；逐图元 diff 见 compare_shaft.py
        assert_eq!(shaft.entities.len(), 45, "用户轴 v2 图元数");
    }

    // ── 剖视验收（参考件 `~/桌面/OCSM/review/轴剖视图.dxf`） ─────────────

    /// 参考件由几何反推出的轴：总长 152、7 段、`M1.5` 为**普通全螺纹**（不是局部螺纹）、
    /// 末段 `GEAR M3 Z20`（齿宽 30）。
    const REVIEW_AXIS: &str =
        "S30 E30 L45 CH2@L | S40 E40 L30 CH2@R OV3 | S50 E30 L20 | S30 E30 L15 CH2@R | S40 E40 L7 M1.5 | S36 E36 L5 | GEAR M3 Z20";

    /// 剖视验收：常规/剖视两份几何 + 剖面线边界逐边落盘，供 `compare_section.py`
    /// 与参考件两个视图逐图元对比。
    #[test]
    fn dump_section_review_csv() {
        let program = parse_program(REVIEW_AXIS).expect("参考轴应能解析");
        let normal = build(&program, 1.0).expect("参考轴常规视图应能出图");
        let section = build(
            &parse_program(&format!("{REVIEW_AXIS} | VIEW 剖视")).unwrap(),
            1.0,
        )
        .expect("参考轴剖视应能出图");
        assert!(near(normal.total_length, 152.0));
        assert!(near(normal.max_diameter, 66.0));
        assert_eq!(normal.segment_count, 7);

        // ── 常规：贯通竖线仍在（用户 2026-09-18 定案保持）──
        for (x, h) in [(2.0, 15.0), (45.0, 15.0), (72.0, 20.0), (72.4, 19.6), (75.0, 20.6),
                       (77.0, 24.0), (95.0, 15.0), (110.0, 15.0), (112.0, 20.0),
                       (117.0, 18.0), (122.0, 18.0)] {
            assert!(
                has_line(&normal, [x, -h], [x, h]),
                "常规视图贯通竖线 x={x} ±{h}"
            );
        }
        // 齿轮倒角台阶竖线（齿轮.rs::side_view() 的 ±ra 台阶线）：x=124/150、±33。
        assert!(has_line(&normal, [124.0, -33.0], [124.0, 33.0]), "齿轮左台阶竖线");
        assert!(has_line(&normal, [150.0, -33.0], [150.0, 33.0]), "齿轮右台阶竖线");
        // 常规不画齿根线（与 gear.rs::side_view() 一致）
        assert!(!has_line(&normal, [122.0, 26.25], [152.0, 26.25]));

        // ── 剖视：不画贯通竖线，只留真实面/倒角/槽体 ──
        for (x, h) in [(2.0, 15.0), (45.0, 15.0), (72.0, 20.0), (72.4, 19.6), (75.0, 20.6),
                       (77.0, 24.0), (95.0, 15.0), (110.0, 15.0), (112.0, 20.0),
                       (117.0, 18.0), (122.0, 18.0)] {
            assert!(
                !has_line(&section, [x, -h], [x, h]),
                "剖视不应有贯通竖线 x={x} ±{h}"
            );
        }
        // 真实几何保留：段间端面 / 倒角 / 槽 / 齿轮倒角 + 齿根线
        assert!(has_line(&section, [45.0, 15.0], [45.0, 20.0]), "真实端面");
        assert!(has_line(&section, [75.0, 20.6], [75.0, 22.0]), "槽肩面到圆角切点");
        assert!(has_line(&section, [75.0, 22.0], [77.0, 24.0]), "倒角斜线");
        assert!(has_line(&section, [110.0, 15.0], [110.0, 18.0]), "真实端面");
        assert!(has_line(&section, [110.0, 18.0], [112.0, 20.0]), "倒角斜线");
        assert!(has_line(&section, [122.0, 18.0], [122.0, 31.0]), "齿轮端面 18→ra−C");
        assert!(has_line(&section, [122.0, 31.0], [124.0, 33.0]), "齿轮倒角斜线");
        assert!(has_line(&section, [124.0, 33.0], [150.0, 33.0]), "齿顶面 h−2C");
        assert!(has_line(&section, [150.0, 33.0], [152.0, 31.0]), "齿轮右倒角斜线");
        assert!(has_line(&section, [122.0, 26.25], [152.0, 26.25]), "剖视齿根线");
        assert!(has_line(&section, [152.0, -31.0], [152.0, 31.0]), "右端面");

        // ── 剖面线：1 个 HATCH、2 环（轴线上/下）、flags = EXTERNAL|OUTERMOST ──
        let hatch = section
            .entities
            .iter()
            .find_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .expect("剖视应有一个 HATCH");
        assert_eq!(hatch.common.layer, crate::partgen_kit::LAYER_HATCH);
        assert_eq!(hatch.pattern.name, "ANSI31");
        assert_eq!(hatch.pattern_scale, 1.0);
        assert_eq!(hatch.paths.len(), 2, "轴线上/下两个环（参考件右视图口径）");
        for path in &hatch.paths {
            assert!(path.flags.is_external() && path.flags.is_outermost());
            assert_eq!(path.edges.len(), 21, "参考件每环 21 边");
        }
        // 参考件右视图的剖面线边界口径：螺纹段沿小径、齿轮段沿齿根、上环沿轴线闭合
        let upper = &hatch.paths[0].edges;
        let has_edge = |a: [f64; 2], b: [f64; 2]| {
            upper.iter().any(|e| match e {
                BoundaryEdge::Line(le) => {
                    let (p, q) = ([le.start.x, le.start.y], [le.end.x, le.end.y]);
                    (near(p[0], a[0]) && near(p[1], a[1]) && near(q[0], b[0]) && near(q[1], b[1]))
                        || (near(p[0], b[0])
                            && near(p[1], b[1])
                            && near(q[0], a[0])
                            && near(q[1], a[1]))
                }
                _ => false,
            })
        };
        assert!(has_edge([111.188125, 19.188125], [117.0, 19.188125]), "边界沿螺纹小径");
        assert!(has_edge([122.0, 26.25], [152.0, 26.25]), "边界沿齿轮齿根");
        assert!(has_edge([0.0, 0.0], [152.0, 0.0]), "上环沿轴线闭合");

        // ── 落盘（列格式同 shaft_demo.csv；HATCH 边界另落一份逐边 CSV）──
        let dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("桌面/OCSM/review");
        std::fs::create_dir_all(&dir).expect("建 review 目录");
        std::fs::write(dir.join("section_review_regular.csv"), entities_csv(&normal.entities))
            .expect("写 section_review_regular.csv");
        std::fs::write(dir.join("section_review_section.csv"), entities_csv(&section.entities))
            .expect("写 section_review_section.csv");
        let mut hatch_csv =
            String::from("path,edge,type,x1,y1,x2,y2,cx,cy,r,a0,a1,flags\n");
        for (pi, path) in hatch.paths.iter().enumerate() {
            let flags = path.flags.bits();
            for (ei, edge) in path.edges.iter().enumerate() {
                match edge {
                    BoundaryEdge::Line(e) => hatch_csv.push_str(&format!(
                        "{pi},{ei},LINE,{:.6},{:.6},{:.6},{:.6},,,,,,{flags}\n",
                        e.start.x, e.start.y, e.end.x, e.end.y
                    )),
                    BoundaryEdge::CircularArc(a) => hatch_csv_arc(
                        &mut hatch_csv,
                        pi,
                        ei,
                        a.center.x,
                        a.center.y,
                        a.radius,
                        a.start_angle.to_degrees(),
                        a.end_angle.to_degrees(),
                        flags,
                    ),
                    other => panic!("剖面边界只应有 LINE/ARC，得到 {other:?}"),
                }
            }
        }
        std::fs::write(dir.join("section_review_hatch.csv"), &hatch_csv)
            .expect("写 section_review_hatch.csv");
        std::fs::write(
            dir.join("section_review_hatch_meta.csv"),
            format!(
                "pattern,{}\nscale,{}\nsolid,{}\npaths,{}\n",
                hatch.pattern.name,
                hatch.pattern_scale,
                hatch.is_solid,
                hatch.paths.len()
            ),
        )
        .expect("写 section_review_hatch_meta.csv");
        assert!(dir.join("section_review_regular.csv").is_file());
        assert!(dir.join("section_review_section.csv").is_file());
    }

    fn hatch_csv_arc(
        csv: &mut String,
        pi: usize,
        ei: usize,
        cx: f64,
        cy: f64,
        r: f64,
        a0: f64,
        a1: f64,
        flags: u32,
    ) {
        let sx = cx + r * a0.to_radians().cos();
        let sy = cy + r * a0.to_radians().sin();
        let ex = cx + r * a1.to_radians().cos();
        let ey = cy + r * a1.to_radians().sin();
        csv.push_str(&format!(
            "{pi},{ei},ARC,{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.4},{:.4},{flags}\n",
            sx, sy, ex, ey, cx, cy, r, a0, a1
        ));
    }

    /// 齿轮段倒角口径自检：`GEAR M3 Z20` 在轴生成器里的齿顶倒角坐标，与
    /// `gear.rs::side_view()` 同参数（m=3、z=20、h=30）的倒角/台阶部分数值一致。
    #[test]
    fn gear_segment_chamfer_matches_gear_side_view() {
        let program = parse_program("S40 E40 L20\nGEAR M3 Z20 H30").unwrap();
        let normal = build(&program, 1.0).unwrap();
        let gear = program.segments[1].gear.unwrap();
        let params = gear.params();
        assert!(near(gear.chamfer(), 2.0), "C = round(0.6×3) = 2");
        // gear.rs 侧视图局部坐标 → 轴里齿轮段 [20,50]：平移 +(20 + h/2) = +35
        let view = crate::gear::side_view(&params, 1.0).unwrap();
        let dx = 20.0 + params.h / 2.0;
        let mut matched = 0;
        for entity in &view {
            if let EntityType::Line(l) = entity {
                if l.common.layer != LAYER_MAIN {
                    continue;
                }
                let a = [l.start.x + dx, l.start.y];
                let b = [l.end.x + dx, l.end.y];
                // 侧视图端面线 ±(ra−C) 在轴上只到邻段半径（20），不是全高；台阶/齿顶/倒角全高可比。
                let is_end_face = near(l.start.x.abs(), params.h / 2.0)
                    && near(l.end.x.abs(), params.h / 2.0)
                    && (l.start.y - l.end.y).abs() > 1e-9;
                if is_end_face {
                    continue;
                }
                assert!(
                    has_line(&normal, a, b),
                    "gear.rs::side_view() 轮廓线 {a:?}–{b:?} 应在轴齿轮段里"
                );
                matched += 1;
            }
        }
        assert_eq!(matched, 8, "齿顶面 2 + 倒角 4 + 台阶线 2");
        // 台阶竖线（仅常规视图）：x=22/48、±ra
        assert!(has_line(&normal, [22.0, -33.0], [22.0, 33.0]));
        assert!(has_line(&normal, [48.0, -33.0], [48.0, 33.0]));
        // 剖视不画台阶线，但齿根线 + 端面到 ra−C 在
        let section = build(
            &parse_program("S40 E40 L20\nGEAR M3 Z20 H30 | VIEW 剖视").unwrap(),
            1.0,
        )
        .unwrap();
        assert!(!has_line(&section, [22.0, -33.0], [22.0, 33.0]));
        assert!(has_line(&section, [20.0, 20.0], [20.0, 31.0]));
        assert!(has_line(&section, [20.0, 26.25], [50.0, 26.25]));
    }

    /// `INVOLSPLINE … report` 命令入口解析层：摘关键字 → 解析 → 计算书；
    /// 报告含 d_B 推导口径与来源（查表命中）。
    #[test]
    fn report_keyword_parses_involspline_and_builds_report() {
        let raw = "INVOLSPLINE DIN30 DB40 M2 L30 report";
        let (clean, want, out) = crate::gear::split_report_args(raw);
        assert!(want && out.is_none());
        assert!(!clean.to_lowercase().contains("report"), "{clean}");
        let program = parse_program(&clean).unwrap();
        let md = build_report(&program).unwrap();
        assert!(
            md.contains("# 轴段计算书") && md.contains("## 2. 渐开线花键段计算书"),
            "{md}"
        );
        assert!(md.contains("d_B = d + 1.1m + 2x₁m"), "{md}");
        assert!(md.contains("查表命中 p"), "{md}");
        // NF A 主参数同样进计算书。
        let program = parse_program("INVOLSPLINE NFP A80 M3.75 L30").unwrap();
        let md = build_report(&program).unwrap();
        assert!(md.contains("A = m(N + 2x + 0.4)"), "{md}");
        assert!(md.contains("NF E22-141 p07"), "{md}");
    }

    /// 径节 P/DP 只属 ANSI：GB/DIN/NF 的 INVOLSPLINE（DSL 与 JSON 两侧）都明确拒绝，
    /// 不得把 P 当模数用；ANSI 合法路径仍可用。
    #[test]
    fn invol_pitch_rejected_outside_ansi() {
        for raw in [
            "INVOLSPLINE GB30R P8 Z20 L30",
            "INVOLSPLINE DIN30 DB40 P8 L30",
            "INVOLSPLINE NFP A80 P8 L30",
            "INVOLSPLINE GB30R DP8 Z20 L30",
        ] {
            let e = parse_program(raw).unwrap_err();
            assert!(
                e.contains("径节 P/Ps 是 ANSI B92.1"),
                "`{raw}` 应报径节只属 ANSI：{e}"
            );
        }
        // `M` 与 `P` 同时给：先报关键字重复（同槽位，不静默取其一）。
        let e = parse_program("INVOLSPLINE NFP A80 M3.75 P8 L30").unwrap_err();
        assert!(e.contains("关键字 P/M/DP（径节/模数）重复"), "{e}");
        for json in [
            r#"{"segments":[{"invol_spline":{"code":"GB30R","m":3,"pitch":8,"z":20,"len":30}}]}"#,
            r#"{"segments":[{"invol_spline":{"code":"DIN30","db":40,"pitch":8,"len":30}}]}"#,
        ] {
            let e = parse_program(json).unwrap_err();
            assert!(e.contains("径节 P/Ps 是 ANSI B92.1"), "{json} → {e}");
            assert!(e.contains("GB/T 3478") || e.contains("DIN 5480"), "{e}");
        }
        // ANSI：P 槽位正常工作（P=8 → m=3.175）。
        let p = parse_program("INVOLSPLINE ANSI30P P8 Z20 L30").unwrap();
        let iv = p.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(iv.params.std, crate::invol_spline::SplineStd::ANSI);
        assert!((iv.params.m - 25.4 / 8.0).abs() < 1e-12);
        let pj = parse_program(r#"{"segments":[{"invol_spline":{"code":"ANSI30P","pitch":8,"z":20,"len":30}}]}"#)
            .unwrap();
        assert!((pj.segments[0].invol_spline.as_ref().unwrap().params.m - 25.4 / 8.0).abs() < 1e-12);
    }

    /// 三入口一致：同一 DIN 花键的 DSL 与 JSON 解析结果（含 `d_B` 来源）逐项相同，
    /// 计算书里的「基准直径来源」不再因 JSON 入口而丢失。
    #[test]
    fn invol_json_and_dsl_share_report_origin() {
        let dsl = parse_program("INVOLSPLINE DIN30 DB40 M2 Z18 L30").unwrap();
        let json = parse_program(
            r#"{"segments":[{"invol_spline":{"code":"DIN30","db":40,"m":2,"z":18,"len":30}}]}"#,
        )
        .unwrap();
        let a = dsl.segments[0].invol_spline.as_ref().unwrap();
        let b = json.segments[0].invol_spline.as_ref().unwrap();
        assert_eq!(a.params, b.params, "DSL/JSON 参数应一致");
        assert_eq!(
            a.d_b_origin.as_ref().map(|o| o.note()),
            b.d_b_origin.as_ref().map(|o| o.note()),
            "DSL/JSON 的 d_B 来源应一致"
        );
        assert!(a.d_b_origin.as_ref().unwrap().note().contains("查表命中 p27"));
        let md_a = build_report(&dsl).unwrap();
        let md_b = build_report(&json).unwrap();
        assert_eq!(md_a, md_b, "同一模型的 DSL/JSON 计算书应逐字相同");
        assert!(md_b.contains("查表命中 p27 m=2"), "{md_b}");
        assert!(!md_b.contains("不适用（GB/ANSI 无 d_B/A 主参数）"), "{md_b}");
        // 表外推导（Adjusted）同样保留明文提示。
        let adj = parse_program("INVOLSPLINE DIN30 DB40 M2 Z14 L30").unwrap();
        let md = build_report(&adj).unwrap();
        assert!(md.contains("按基准直径 d_B=40 取 z=18") && md.contains("与输入 z=14 不符"), "{md}");
    }
}
