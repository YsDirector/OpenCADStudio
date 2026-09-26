//! 智能卡片「GB 花键参数表」（`OCSMCARD` 的专用面板卡）：**GUI 选项表 + 页面/API 模型（表驱动）**。
//!
//! 照本仓 `partgen_keys::KEY_STYLES` 的既有做法：把「体系 → 方向 → 可选项
//! （等级清单 / 配合清单 / 齿根形式 / 21 项尺寸公式口径）」全部放在下面的
//! `SPLINE_SYSTEMS` 表里，后端随 `GET /api/spline_options` 一起下发，
//! `spline_gui.html` **只渲染**（不在页面里写死任何清单/公式/来源）。
//! 以后加 ANSI / NF 体系 = 表里加数据行 + 引擎补口径；本期只做 GB。
//!
//! 与公式层（`spline_tol`）的分工：
//! - 本模块只做「选项/模型/JSON 组装」，数值全部来自 `spline_tol::compute()`；
//! - 插入图的路径统一走 `spline_table::build_insert()`（CLI 与 GUI 同一条路径，
//!   见 `cmd_spline_table` 与 `guide_server::apply_spline_export`）。
//!
//! 信息分层（用户 2026-09-25 定稿的全局口径）：常显只放操作引导；公式/口径/来源
//! 进原生 `title=`（本模块的 `formula`/`source` 字段）；异常与需手填走动态元素。

use crate::spline_tol::{ExtDev, PressureAngle, RootForm, SplineInput, SplineSide, SplineTable};
use ocs_plugin_api::host::acadrust::entities::AttributeDefinition;
use serde::Deserialize;

// ══════════════════════════════════════════════════════════════════════════
// 选项表（后端唯一真源；GUI 只渲染）
// ══════════════════════════════════════════════════════════════════════════

/// 一项参数的公式/口径/来源（进原生 title=，不做常显）。
#[derive(Debug, Clone, Copy)]
pub struct SplineColumnSpec {
    /// 与 `spline_table::attdefs()` 的 ATTDEF tag 完全一致（表驱动断言：21 项一一对应）。
    pub tag: &'static str,
    /// 结果表里的可见名（不带 `(内)`/`(外)` 前缀）。
    pub label: &'static str,
    /// 显示单位（空串 = 无量纲，如齿数/角度/等级）。
    pub unit: &'static str,
    /// 公式/计算口径（原生 title）。
    pub formula: &'static str,
    /// 依据来源（原生 title）。
    pub source: &'static str,
}

/// 齿根形式（表驱动；`key` 与模型 JSON 的 `root` 一致）。
#[derive(Debug, Clone, Copy)]
pub struct SplineRootSpec {
    pub key: &'static str,
    pub label: &'static str,
    /// 该齿根形式适用的压力角（本表只列，不做强校验——公式层按 αD 分派）。
    pub alphas: &'static [PressureAngle],
    /// 口径说明（原生 title）。
    pub note: &'static str,
}

/// 量棒面板（只有参数表含量棒测量的方向才有；本期 GB 只有内花键）。
#[derive(Debug, Clone, Copy)]
pub struct SplinePinSpec {
    /// 面板标题（可见）。
    pub label: &'static str,
    /// 量棒计算式（title）。
    pub formula: &'static str,
    /// 跨棒距计算式（title）。
    pub md_formula: &'static str,
    /// 选棒规则（title）。
    pub standard: &'static str,
}

/// 一个方向（内/外）的可选项 + 21 项公式口径。
#[derive(Debug, Clone, Copy)]
pub struct SplineSideSpec {
    /// 稳定 id（模型 JSON 的 `side`）：`int` / `ext`。
    pub id: &'static str,
    /// 可见名（按钮/标题）。
    pub label: &'static str,
    /// 方向说明（原生 title）。
    pub title: &'static str,
    /// 公差等级清单（GB/T 3478.1 只有 4/5/6/7）。
    pub grades: &'static [u32],
    /// 等级口径（原生 title）。
    pub grade_note: &'static str,
    /// 压力角口径（原生 title）。
    pub alpha_note: &'static str,
    /// 齿根形式清单。
    pub roots: &'static [SplineRootSpec],
    /// 压力角清单。
    pub alphas: &'static [PressureAngle],
    /// 量棒面板；`None` = 该方向参数表不含量棒测量（GUI 置灰，不写文字）。
    pub pin: Option<SplinePinSpec>,
    /// 21 项公式口径（顺序 = 插入后表格块的 ATTDEF 顺序）。
    pub columns: &'static [SplineColumnSpec],
}

/// 一个体系（本期只有 GB；以后 ANSI/NF = 加数据行）。
#[derive(Debug, Clone, Copy)]
pub struct SplineSystemSpec {
    pub id: &'static str,
    /// 命令/GUI 可用的别名（如 `gb`/`GB`；表驱动校验的唯一来源）。
    pub aliases: &'static [&'static str],
    pub label: &'static str,
    pub standard: &'static str,
    /// 体系口径说明（原生 title）。
    pub note: &'static str,
    /// 变位系数 x 的口径（原生 title；GB 参数表不使用 x）。
    pub x_note: &'static str,
    pub sides: &'static [SplineSideSpec],
}

const GB_GRADES: &[u32] = &[4, 5, 6, 7];
const GB_ALPHAS: &[PressureAngle] = &[
    PressureAngle::A30,
    PressureAngle::A37_5,
    PressureAngle::A45,
];

const GB_ROOTS: &[SplineRootSpec] = &[
    SplineRootSpec {
        key: "flat",
        label: "平齿根",
        alphas: &[PressureAngle::A30, PressureAngle::A37_5, PressureAngle::A45],
        note: "GB/T 3478.1 表 3：30° 平齿根 Dei = m(z+1.5)、外花键 Die = m(z−1.5)；\
               37.5°/45° 允许平齿根（基本尺寸按圆齿根系列公式）",
    },
    SplineRootSpec {
        key: "fillet",
        label: "圆齿根",
        alphas: &[PressureAngle::A30, PressureAngle::A37_5, PressureAngle::A45],
        note: "GB/T 3478.1 表 3：30° 圆齿根 Dei = m(z+1.8)、外花键 Die = m(z−1.8)；\
               37.5°/45° 按各自系列公式",
    },
];

const PIN_INTERNAL: SplinePinSpec = SplinePinSpec {
    label: "量棒直径 Dp 与测量跨棒距 Md",
    formula: "D'_Ri = Db[tanα_ci − tan(α_ci − E_max/D + invα_ci − invαD)]，\
              α_ci = acos(Db/D_ci)、D_ci = (D_ee max + D_ii min)/2（GB/T 3478.6 §3.1.1 式(1)）",
    md_formula: "偶齿 M_Ri max/min = Db/cosα_i max/min − Dp；奇齿再乘 cos(90°/z)；\
                 invα_i max/min = E_max/min/D + invαD − Dp/Db（GB/T 3478.6 §3.1.2 式(2)~(5)）",
    standard: "D'_Ri 算完后按 GB/T 321 的 R40 系列取最接近且较大的值（GB/T 3478.9 表 1，67 档）；\
               3 个备选 = 系列中与 D' 最接近的 3 个（工程口径）",
};

const INTERNAL_COLUMNS: &[SplineColumnSpec] = &[
    SplineColumnSpec {
        tag: "(内)齿形角",
        label: "齿形角",
        unit: "°",
        formula: "GB/T 3478.1 基本齿廓：αD = 30° / 37.5° / 45°（本页所选值）",
        source: "GB/T 3478.1 基本齿廓",
    },
    SplineColumnSpec {
        tag: "(内)齿数",
        label: "齿数",
        unit: "",
        formula: "由九字段齿形表达式反解（MARK KIND M Z ALPHA X DA DF BETA H；轴/齿轮生成器 GUI 可复制）",
        source: "手填 / 图纸继承",
    },
    SplineColumnSpec {
        tag: "(内)模数",
        label: "模数",
        unit: "mm",
        formula: "由九字段齿形表达式反解（MARK KIND M Z ALPHA X DA DF BETA H；轴/齿轮生成器 GUI 可复制）",
        source: "手填 / 图纸继承",
    },
    SplineColumnSpec {
        tag: "(内)公差等级和配合类别",
        label: "公差等级和配合类别",
        unit: "",
        formula: "等级 4/5/6/7（表 7~21 的 (T+λ) 系数 10/16/25/40）；内花键恒为基孔制 H",
        source: "GB/T 3478.1 §8.7、表 23",
    },
    SplineColumnSpec {
        tag: "(内)大径",
        label: "大径 Dei",
        unit: "mm",
        formula: "Dei = m(z+1.5) 30°平 / m(z+1.8) 30°圆 / m(z+1.4) 37.5° / m(z+1.2) 45°",
        source: "GB/T 3478.1 表 3",
    },
    SplineColumnSpec {
        tag: "(内)渐开线终止圆直径最大值",
        label: "渐开线终止圆直径最大值 D_Fimin",
        unit: "mm",
        formula: "D_Fimin = m(z+1)/(z+0.9)/(z+0.8) + 2CF，CF = 0.1m（表 3 注 4：非 H/h 有变化，标准全 70 页未列值）",
        source: "GB/T 3478.1 表 3 与注 4（全文档无 CF 变化值表）",
    },
    SplineColumnSpec {
        tag: "(内)小径",
        label: "小径 Dii",
        unit: "mm",
        formula: "Dii = D_Femax(H/h) + 2CF（表 3 注 2）；D_Femax 按表 3 注 3（h_s 见图 2）",
        source: "GB/T 3478.1 表 3 注 2/注 3",
    },
    SplineColumnSpec {
        tag: "(内)测量跨棒距",
        label: "测量跨棒距 Md",
        unit: "mm",
        formula: "偶齿 M = Db/cosαi ∓ Dp；奇齿再乘 cos(90°/z)；αi 由 E/D + invαD − Dp/Db 反解",
        source: "GB/T 3478.6 式(2)~(5)",
    },
    SplineColumnSpec {
        tag: "(内)量棒直径",
        label: "量棒直径 Dp",
        unit: "mm",
        formula: "D'_Ri = Db[tanαci − tan(αci − E_max/D + invαci − invαD)]，再按 R40 取最接近较大值",
        source: "GB/T 3478.6 式(1)、GB/T 3478.9 表 1",
    },
    SplineColumnSpec {
        tag: "(内)作用齿槽宽最小值",
        label: "作用齿槽宽最小值",
        unit: "mm",
        formula: "基本齿槽宽 E = 0.5πm（模板该列口径；与按 λ 修正的 E_min 分开）",
        source: "GB/T 3478.1 表 3",
    },
    SplineColumnSpec {
        tag: "(内)实际齿槽宽最大值",
        label: "实际齿槽宽最大值",
        unit: "mm",
        formula: "E max = E + (T+λ)；(T+λ) = 10/16/25/40·i_d + 40/64/100/160·i_E（4/5/6/7 级）",
        source: "GB/T 3478.1 §8.1、表 7~21",
    },
    SplineColumnSpec {
        tag: "(内)齿根圆最小曲率半径",
        label: "齿根圆最小曲率半径 R_imin",
        unit: "mm",
        formula: "R_imin = 0.2m 30°平 / 0.4m 30°圆 / 0.3m 37.5° / 0.25m 45°（表 26 逐模数同值）",
        source: "GB/T 3478.1 图 2；表 26（书页 50）逐格核对",
    },
    SplineColumnSpec {
        tag: "(内)齿形公差",
        label: "齿形公差 Ff",
        unit: "μm",
        formula: "Fα = aφ1 + b，φ1 = m + 0.0125mz（4/5/6/7 级系数 1.6/2.5/4/6.3、10/16/25/40）",
        source: "GB/T 3478.1 §8.3、表 7~21",
    },
    SplineColumnSpec {
        tag: "(内)齿距累计公差",
        label: "齿距累计公差 Fp",
        unit: "μm",
        formula: "Fp = a√L + b，L = πmz/2（4/5/6/7 级系数 2.5/3.55/5/7.1、6.3/9/12.5/18）",
        source: "GB/T 3478.1 §8.2、表 7~21",
    },
    SplineColumnSpec {
        tag: "(内)综合公差",
        label: "综合公差 λ",
        unit: "μm",
        formula: "λ = 0.6√(Fp²+Fα²+Fβ²)，用未修约的 F 值",
        source: "GB/T 3478.1 §8.6",
    },
    SplineColumnSpec {
        tag: "(内)小径.下公差",
        label: "小径下公差",
        unit: "mm",
        formula: "Dii 用 H10/H11/H12（模数档 0.25~0.75 / 1~1.75 / 2~10），下偏差 0",
        source: "GB/T 3478.1 表 25、GB/T 1800",
    },
    SplineColumnSpec {
        tag: "(内)小径.上公差",
        label: "小径上公差",
        unit: "mm",
        formula: "+IT10/IT11/IT12（模数档同上）；表 25 原页 Dii 列为图片，已核对为 +IT/0",
        source: "GB/T 3478.1 表 25",
    },
    SplineColumnSpec {
        tag: "(内)测量跨棒距.下公差",
        label: "测量跨棒距下公差",
        unit: "mm",
        formula: "M_min − M_mid（E_min = E + λ 一侧）",
        source: "GB/T 3478.6 式(2)~(5)",
    },
    SplineColumnSpec {
        tag: "(内)测量跨棒距.上公差",
        label: "测量跨棒距上公差",
        unit: "mm",
        formula: "M_max − M_mid（E_max = E + (T+λ) 一侧）",
        source: "GB/T 3478.6 式(2)~(5)",
    },
    SplineColumnSpec {
        tag: "(内)大径.下公差",
        label: "大径下公差",
        unit: "mm",
        formula: "基孔制 H：下偏差 0",
        source: "GB/T 3478.1 表 25",
    },
    SplineColumnSpec {
        tag: "(内)大径.上公差",
        label: "大径上公差",
        unit: "mm",
        formula: "Dei 用 IT12/IT13/IT14（模数档 0.25~0.75 / 1~1.75 / 2~10）",
        source: "GB/T 3478.1 表 25、GB/T 1800",
    },
];

const EXTERNAL_COLUMNS: &[SplineColumnSpec] = &[
    SplineColumnSpec {
        tag: "(外)齿形角",
        label: "齿形角",
        unit: "°",
        formula: "GB/T 3478.1 基本齿廓：αD = 30° / 37.5° / 45°（本页所选值）",
        source: "GB/T 3478.1 基本齿廓",
    },
    SplineColumnSpec {
        tag: "(外)齿数",
        label: "齿数",
        unit: "",
        formula: "由九字段齿形表达式反解（MARK KIND M Z ALPHA X DA DF BETA H；轴/齿轮生成器 GUI 可复制）",
        source: "手填 / 图纸继承",
    },
    SplineColumnSpec {
        tag: "(外)模数",
        label: "模数",
        unit: "mm",
        formula: "由九字段齿形表达式反解（MARK KIND M Z ALPHA X DA DF BETA H；轴/齿轮生成器 GUI 可复制）",
        source: "手填 / 图纸继承",
    },
    SplineColumnSpec {
        tag: "(外)公差等级和配合类别",
        label: "公差等级和配合类别",
        unit: "",
        formula: "等级 4/5/6/7；基本偏差 h/js/k/d/e/f（与内花键 H 相配）",
        source: "GB/T 3478.1 §8.7.2、表 23/24",
    },
    SplineColumnSpec {
        tag: "(外)大径",
        label: "大径 Dee",
        unit: "mm",
        formula: "Dee = m(z+1) 30° / m(z+0.9) 37.5° / m(z+0.8) 45°",
        source: "GB/T 3478.1 表 3",
    },
    SplineColumnSpec {
        tag: "(外)渐开线终止圆直径最大值",
        label: "渐开线终止圆直径最大值 D_Femax",
        unit: "mm",
        formula: "D_Femax 按表 3 注 3（h_s 见图 2；含 esv 修正）",
        source: "GB/T 3478.1 表 3 注 3",
    },
    SplineColumnSpec {
        tag: "(外)小径",
        label: "小径 Die",
        unit: "mm",
        formula: "Die = m(z−1.5) 30°平 / m(z−1.8) 30°圆 / m(z−1.4) 37.5° / m(z−1.2) 45°",
        source: "GB/T 3478.1 表 3",
    },
    SplineColumnSpec {
        tag: "(外)公法线长度",
        label: "公法线长度 Wn",
        unit: "mm",
        formula: "W_min = cosαD[(K−0.5)πm + D·invαD + esv − (T+λ)]；W_mid = (W_min+W_max)/2",
        source: "GB/T 3478.6 式(11)(12)",
    },
    SplineColumnSpec {
        tag: "(外)跨测齿数",
        label: "跨测齿数 Kn",
        unit: "",
        formula: "K = z/6 + 0.5 取整数",
        source: "GB/T 3478.6 式(11) 注",
    },
    SplineColumnSpec {
        tag: "(外)实际齿厚最小值",
        label: "实际齿厚最小值",
        unit: "mm",
        formula: "S_min = S_v max − (T+λ)，S_v max = S + esv",
        source: "GB/T 3478.1 §8.7、表 23",
    },
    SplineColumnSpec {
        tag: "(外)作用齿厚最大值",
        label: "作用齿厚最大值",
        unit: "mm",
        formula: "S_v max = S + esv，基本齿厚 S = 0.5πm",
        source: "GB/T 3478.1 表 3、表 23",
    },
    SplineColumnSpec {
        tag: "(外)齿根圆最小曲率半径",
        label: "齿根圆最小曲率半径 R_imin",
        unit: "mm",
        formula: "R_imin = 0.2m 30°平 / 0.4m 30°圆 / 0.3m 37.5° / 0.25m 45°（表 26 逐模数同值）",
        source: "GB/T 3478.1 图 2；表 26（书页 50）逐格核对",
    },
    SplineColumnSpec {
        tag: "(外)齿形公差",
        label: "齿形公差 Ff",
        unit: "μm",
        formula: "Fα = aφ1 + b，φ1 = m + 0.0125mz（4/5/6/7 级系数 1.6/2.5/4/6.3、10/16/25/40）",
        source: "GB/T 3478.1 §8.3、表 7~21",
    },
    SplineColumnSpec {
        tag: "(外)齿距累计公差",
        label: "齿距累计公差 Fp",
        unit: "μm",
        formula: "Fp = a√L + b，L = πmz/2（4/5/6/7 级系数 2.5/3.55/5/7.1、6.3/9/12.5/18）",
        source: "GB/T 3478.1 §8.2、表 7~21",
    },
    SplineColumnSpec {
        tag: "(外)综合公差",
        label: "综合公差 λ",
        unit: "μm",
        formula: "λ = 0.6√(Fp²+Fα²+Fβ²)，用未修约的 F 值",
        source: "GB/T 3478.1 §8.6",
    },
    SplineColumnSpec {
        tag: "(外)小径.下公差",
        label: "小径下公差",
        unit: "mm",
        formula: "esv/tanαD − IT（Die 公差用 IT12/IT13/IT14，模数档 0.25~0.75 / 1~1.75 / 2~10）",
        source: "GB/T 3478.1 表 24、表 25",
    },
    SplineColumnSpec {
        tag: "(外)小径.上公差",
        label: "小径上公差",
        unit: "mm",
        formula: "esv/tanαD；d/e/f 查表 24、h=0、js=+(T+λ)/(2tanαD)、k=+(T+λ)/tanαD",
        source: "GB/T 3478.1 表 24",
    },
    SplineColumnSpec {
        tag: "(外)公法线长度.下公差",
        label: "公法线长度下公差",
        unit: "mm",
        formula: "W_min − W_mid",
        source: "GB/T 3478.6 式(11)(12)",
    },
    SplineColumnSpec {
        tag: "(外)公法线长度.上公差",
        label: "公法线长度上公差",
        unit: "mm",
        formula: "W_max − W_mid，W_max = W_min + T·cosαD",
        source: "GB/T 3478.6 式(12)",
    },
    SplineColumnSpec {
        tag: "(外)大径.下公差",
        label: "大径下公差",
        unit: "mm",
        formula: "Dee 用 IT12/IT13/IT14（模数档 0.25~0.75 / 1~1.75 / 2~10）",
        source: "GB/T 3478.1 表 25、GB/T 1800",
    },
    SplineColumnSpec {
        tag: "(外)大径.上公差",
        label: "大径上公差",
        unit: "mm",
        formula: "Dee 上偏差取 0",
        source: "GB/T 3478.1 表 24 脚注①",
    },
];

const GB_INT: SplineSideSpec = SplineSideSpec {
    id: "int",
    label: "内花键",
    title: "内花键（基孔制 H）：参数表含量棒直径 Dp 与测量跨棒距 Md",
    grades: GB_GRADES,
    grade_note: "公差等级 4/5/6/7：等级越高公差越小；(T+λ) 系数 10/16/25/40、\
                 i_E/i_d 同 GB/T 3478.1 §8.1（表 7~21）",
    alpha_note: "压力角 αD = 30° / 37.5° / 45°（基本齿廓）；invαD = 0.0537515 / 0.1128285 / 0.2146018",
    roots: GB_ROOTS,
    alphas: GB_ALPHAS,
    pin: Some(PIN_INTERNAL),
    columns: INTERNAL_COLUMNS,
};

const GB_EXT: SplineSideSpec = SplineSideSpec {
    id: "ext",
    label: "外花键",
    title: "外花键：基本偏差 d/e/f/h/js/k 与内花键 H 相配；参数表按 GB/T 3478.1 出公法线长度 Wn / 跨测齿数 Kn",
    grades: GB_GRADES,
    grade_note: "公差等级 4/5/6/7：等级越高公差越小；(T+λ) 系数 10/16/25/40、\
                 i_S/i_d 同 GB/T 3478.1 §8.1（表 7~21）",
    alpha_note: "压力角 αD = 30° / 37.5° / 45°（基本齿廓）；invαD = 0.0537515 / 0.1128285 / 0.2146018",
    roots: GB_ROOTS,
    alphas: GB_ALPHAS,
    // GB/T 3478.1 外花键参数表用公法线长度，不含 Dp/M_Ri（GB/T 3478.6 的 M_Re 未列入 21 项）。
    pin: None,
    columns: EXTERNAL_COLUMNS,
};

const GB_SIDES: &[SplineSideSpec] = &[GB_INT, GB_EXT];

const GB_SYSTEM: SplineSystemSpec = SplineSystemSpec {
    id: "gb3478",
    aliases: &["gb", "GB", "GB3478"],
    label: "GB/T 3478.1-2008 渐开线花键（GB）",
    standard: "GB/T 3478.1 / .6 / .7 / .8 / .9-2008",
    note: "本期只实现 GB 体系；口径与数据见 assets/spline_gb3478_*.csv 的 source/note。\
           以后加 ANSI / NF = 本表加数据行 + 引擎补口径。",
    x_note: "GB/T 3478.1 基本齿廓不含变位系数 x；x 仅记录（从 GEAR / 轴块继承时带回）",
    sides: GB_SIDES,
};

/// 全部体系（顺序 = GUI 体系下拉顺序）。
pub const SPLINE_SYSTEMS: &[SplineSystemSpec] = &[GB_SYSTEM];

/// 按 id 找体系。
pub fn system_by_id(id: &str) -> Option<&'static SplineSystemSpec> {
    let t = id.trim();
    SPLINE_SYSTEMS.iter().find(|s| {
        s.id.eq_ignore_ascii_case(t)
            || s.aliases.iter().any(|a| a.eq_ignore_ascii_case(t))
    })
}

/// 命令里的体系 token → 稳定 id（`gb`/`GB`/`gb3478` 均收；表驱动校验）。
pub fn system_id_by_token(text: &str) -> Result<&'static str, String> {
    system_by_id(text)
        .map(|s| s.id)
        .ok_or_else(|| {
            format!(
                "花键参数表：不认识的体系「{}」（本期只有 GB / gb3478）",
                text.trim()
            )
        })
}

/// 按 id 找方向。
pub fn side_spec_by_id(id: &str) -> Option<&'static SplineSideSpec> {
    SPLINE_SYSTEMS
        .iter()
        .flat_map(|s| s.sides.iter())
        .find(|s| s.id.eq_ignore_ascii_case(id))
}

pub(crate) fn side_id(side: SplineSide) -> &'static str {
    match side {
        SplineSide::Internal => "int",
        SplineSide::External => "ext",
    }
}

pub(crate) fn side_label(side: SplineSide) -> &'static str {
    match side {
        SplineSide::Internal => "内花键",
        SplineSide::External => "外花键",
    }
}

/// 公差行 → 对应主值 tag（口径：`X.下公差`/`X.上公差` 的主值就是同前缀的 `X`）。
/// 排版回归断言用它把「主值框 ↔ 上/下公差框」配对。
#[cfg(test)]
pub fn main_tag_of_dev(tag: &str) -> Option<&str> {
    tag.strip_suffix(".下公差")
        .or_else(|| tag.strip_suffix(".上公差"))
}

fn root_id(root: RootForm) -> &'static str {
    match root {
        RootForm::Flat => "flat",
        RootForm::Fillet => "fillet",
    }
}

fn root_form(text: &str) -> Result<RootForm, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "flat" | "平" | "平齿根" => Ok(RootForm::Flat),
        "fillet" | "圆" | "圆齿根" => Ok(RootForm::Fillet),
        other => Err(format!(
            "花键参数表：齿根形式「{other}」非法（可选项见 /api/spline_options）"
        )),
    }
}

/// 配合类别清单（**数据仍来自 assets/spline_gb3478_fit.csv**；内花键恒 H）。
fn fits_json(side: SplineSide) -> Result<Vec<serde_json::Value>, String> {
    match side {
        SplineSide::Internal => Ok(vec![serde_json::json!({
            "fit": "H",
            "code": "H",
            "label": "H（内花键基孔制）",
            "preferred_45": true,
            "memo": "内花键恒 H；与外花键基本偏差 d/e/f/h/js/k 形成配合",
        })]),
        SplineSide::External => Ok(crate::spline_tol::fit_rows()?
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "fit": r.fit,
                    "code": r.ext_dev.code(),
                    "label": r.fit,
                    "preferred_45": r.preferred_45,
                    "memo": r.memo,
                })
            })
            .collect()),
    }
}

/// `GET /api/spline_options`：整套选项表（GUI 的唯一数据来源）。
pub fn options_json() -> Result<serde_json::Value, String> {
    let systems: Vec<serde_json::Value> = SPLINE_SYSTEMS
        .iter()
        .map(|sys| -> Result<serde_json::Value, String> {
            let sides: Result<Vec<serde_json::Value>, String> = sys
                .sides
                .iter()
                .map(|s| {
                    let side = if s.id == "int" {
                        SplineSide::Internal
                    } else {
                        SplineSide::External
                    };
                    let roots: Vec<serde_json::Value> = s
                        .roots
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "key": r.key,
                                "label": r.label,
                                "alphas": r.alphas.iter().map(|a| a.deg()).collect::<Vec<f64>>(),
                                "note": r.note,
                            })
                        })
                        .collect();
                    let pin = match &s.pin {
                        Some(p) => serde_json::json!({
                            "applicable": true,
                            "label": p.label,
                            "formula": p.formula,
                            "md_formula": p.md_formula,
                            "standard": p.standard,
                        }),
                        None => serde_json::json!({ "applicable": false }),
                    };
                    let columns: Vec<serde_json::Value> = s
                        .columns
                        .iter()
                        .map(|c| {
                            serde_json::json!({
                                "tag": c.tag,
                                "label": c.label,
                                "unit": c.unit,
                                "formula": c.formula,
                                "source": c.source,
                            })
                        })
                        .collect();
                    Ok(serde_json::json!({
                        "id": s.id,
                        "label": s.label,
                        "title": s.title,
                        "grade_note": s.grade_note,
                        "alpha_note": s.alpha_note,
                        "grades": s.grades,
                        "fits": fits_json(side)?,
                        "roots": roots,
                        "alphas": s.alphas.iter().map(|a| a.deg()).collect::<Vec<f64>>(),
                        "pin": pin,
                        "columns": columns,
                    }))
                })
                .collect();
            Ok(serde_json::json!({
                "id": sys.id,
                "aliases": sys.aliases,
                "label": sys.label,
                "standard": sys.standard,
                "note": sys.note,
                "x_note": sys.x_note,
                "sides": sides?,
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::json!({
        "ok": true,
        // 卡片类型表（智能卡片：花键/齿轮/ANSI 中英；以后加表格/铭牌 = 表加行）。
        "card_types": crate::card::card_types_json(),
        "systems": systems,
        // 齿轮卡与 ANSI 卡的选项/口径（同一下发端点；页面按 renderer 切面板）。
        "gear_card": crate::gear_table::options_json(),
        "ansi_card": crate::ansi_table::options_json(),
        "nf_card": crate::nf_table::options_json(),
        "nf_ext_card": crate::nf_ext_table::options_json(),
        "din_card": crate::din_table::options_json(),
        "pin_series": crate::spline_tol::pin_series()?,
        "pin_series_note": "GB/T 3478.9-2008 表 1（67 档，R40；极限偏差 ±0.001 mm）",
        "info_layers": {
            "guide": "常显只放操作引导",
            "title": "公式/口径/来源一律进原生 title=",
            "dynamic": "异常与需手填只在出现时显示",
        },
    }))
}

// ══════════════════════════════════════════════════════════════════════════
// 页面模型（/api/spline_preview 与 /api/spline_export 的请求体）
// ══════════════════════════════════════════════════════════════════════════

/// GUI 表单模型（字段与页面控件一一对应）。
///
/// **参数来源 = 九字段齿形表达式**（用户 2026-09-25 裁定）：页面只粘表达式，
/// `m/z/αD/x/Da/Df` 由后端反解；不再有「读选中/上一个块」这条路径。
#[derive(Debug, Clone, Deserialize)]
pub struct SplineTableModel {
    /// 方向：`int` / `ext`（由卡类型固定并于后端回填；旧请求显式给也收）。
    #[serde(default)]
    pub side: String,
    pub grade: u32,
    /// 基本偏差 code：内 H；外 h/js/k/d/e/f。
    pub fit: String,
    /// 九字段统一齿形表达式（轴/齿轮生成器 GUI 可复制）。
    pub expr: String,
    /// 齿根形式：`flat`/`fillet`；缺省/`auto` = 由表达式 DA/DF 反解，再退到按 αD 默认。
    #[serde(default)]
    pub root: Option<String>,
    /// 量棒直径：`null` = 标准 R40 自动选（内花键）。
    #[serde(default)]
    pub dp: Option<f64>,
    /// 显式落点（如 `[10,20]`）；缺省 = 走待放置件（放置态）。
    #[serde(default)]
    pub at: Option<[f64; 2]>,
    #[serde(default)]
    pub rot: f64,
}

/// 本轮只挂 GB 体系（体系字段已从 GUI/入参移除；CLI `std GB` 仍接受）。
fn gb_system() -> &'static SplineSystemSpec {
    system_by_id("gb3478").expect("gb3478 体系必在选项表内")
}

impl SplineTableModel {
    /// 体系校验（表驱动；以后加体系 = 表里加行）。这一字段不再来自入参。
    pub fn system_spec(&self) -> Result<&'static SplineSystemSpec, String> {
        Ok(gb_system())
    }

    pub fn side(&self) -> Result<SplineSide, String> {
        match self.side.trim().to_ascii_lowercase().as_str() {
            "int" | "internal" | "内" | "内花键" => Ok(SplineSide::Internal),
            "ext" | "external" | "外" | "外花键" => Ok(SplineSide::External),
            other => Err(format!(
                "花键参数表：方向「{other}」非法（可选项 int/ext，见 /api/spline_options）"
            )),
        }
    }

    /// 齿根形式：`None`/空/`auto` = 反解（见 `resolved`）。
    fn root_form_opt(&self) -> Result<Option<RootForm>, String> {
        match self.root.as_deref().map(str::trim) {
            None | Some("") | Some("auto") | Some("自动") => Ok(None),
            Some(t) => Ok(Some(root_form(t)?)),
        }
    }

    /// 配合类别（表里校验可选项；内花键恒 H）。
    fn ext_dev(&self, side: SplineSide) -> Result<ExtDev, String> {
        let code = self.fit.trim();
        let code = code.strip_prefix(|c: char| c.is_ascii_digit()).unwrap_or(code);
        if code.is_empty() {
            return Err("花键参数表：缺配合类别（内 H；外 d/e/f/h/js/k）".into());
        }
        let dev = ExtDev::parse(code)?;
        let allowed: Vec<String> = fits_json(side)?
            .iter()
            .filter_map(|f| f["code"].as_str().map(str::to_string))
            .collect();
        if !allowed.iter().any(|c| c.eq_ignore_ascii_case(dev.code())) {
            return Err(format!(
                "花键参数表：配合类别「{}」不在{}可选项（{}）",
                dev.code(),
                if side == SplineSide::Internal { "内花键" } else { "外花键" },
                allowed.join("/")
            ));
        }
        Ok(dev)
    }

    /// 表达式反解 + 方向 KIND 校验（表达式与卡片方向必须一致）。
    pub fn resolved_gear(&self) -> Result<(crate::shaft::Gear, SplineSide), String> {
        let side = self.side()?;
        let gear = crate::spline_table::parse_expr_gear(&self.expr)?;
        if let Some(internal) = crate::spline_table::expr_explicit_kind(&self.expr) {
            if internal != (side == SplineSide::Internal) {
                return Err(format!(
                    "花键参数表：表达式 KIND 写的是 {}，与卡片方向「{}」不一致",
                    if internal { "IN（内）" } else { "EX（外）" },
                    if side == SplineSide::Internal { "内" } else { "外" }
                ));
            }
        }
        Ok((gear, side))
    }

    /// →（`SplineInput`，表达式齿形段）。全部校验过一遍；与 CLI `SplineTableSpec` 同口径。
    pub fn resolved(&self) -> Result<(SplineInput, crate::shaft::Gear), String> {
        let system = self.system_spec()?;
        let (gear, side) = self.resolved_gear()?;
        let side_spec = system
            .sides
            .iter()
            .find(|s| s.id == side_id(side))
            .ok_or_else(|| format!("花键参数表：体系「{}」没有方向 {}", system.id, side_id(side)))?;
        if !side_spec.grades.contains(&self.grade) {
            return Err(format!(
                "花键参数表：公差等级「{}」不在 {} 可选项（{}）",
                self.grade,
                side_spec.label,
                side_spec
                    .grades
                    .iter()
                    .map(|g| g.to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            ));
        }
        let ext_dev = self.ext_dev(side)?;
        let alpha = PressureAngle::parse(&format!("{}", gear.alpha_deg))?;
        if !side_spec.alphas.contains(&alpha) {
            return Err(format!(
                "花键参数表：压力角 {}° 不在 {} 可选项（30/37.5/45；见 /api/spline_options）",
                gear.alpha_deg, side_spec.label
            ));
        }
        if !gear.m.is_finite() || !(0.25..=10.0).contains(&gear.m) {
            return Err(format!(
                "花键参数表：模数 m={} 不在 0.25…10（15 种模数系列）",
                gear.m
            ));
        }
        if gear.z < 6 {
            return Err(format!("花键参数表：齿数 z={} 太小（至少 6）", gear.z));
        }
        // 齿根：显式 > 表达式 DA/DF 反解（30° 可分辨）> 按 αD 默认。
        let root = self
            .root_form_opt()?
            .or_else(|| crate::spline_table::root_from_gear(&gear, side))
            .unwrap_or(match alpha {
                PressureAngle::A30 => RootForm::Flat,
                _ => RootForm::Fillet,
            });
        let root_spec = side_spec
            .roots
            .iter()
            .find(|r| r.key == root_id(root))
            .ok_or_else(|| {
                format!(
                    "花键参数表：齿根形式「{}」不在 {} 可选项",
                    root_id(root),
                    side_spec.label
                )
            })?;
        if !root_spec.alphas.contains(&alpha) {
            return Err(format!(
                "花键参数表：{} 不适用于压力角 {}°（见表选项 note）",
                root_spec.label,
                gear.alpha_deg
            ));
        }
        if self.dp.is_some_and(|v| !(v > 0.0)) {
            return Err(format!(
                "花键参数表：量棒直径 Dp={} 必须 >0",
                self.dp.unwrap_or(0.0)
            ));
        }
        Ok((
            SplineInput {
                m: gear.m,
                z: gear.z,
                alpha,
                root,
                side,
                grade: self.grade,
                ext_dev,
                fit_length: None,
                dp: self.dp,
            },
            gear,
        ))
    }

    /// 只要 `SplineInput`（调用方不关心表达式齿形段时）。
    pub fn to_input(&self) -> Result<SplineInput, String> {
        self.resolved().map(|(input, _)| input)
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 预览 JSON（21 项结果 + Dp/Md 面板）
// ══════════════════════════════════════════════════════════════════════════

/// 21 项结果（label/单位/公式/来源来自选项表，值来自 `spline_table::values`）。
pub fn items_json(side: SplineSide, table: &SplineTable) -> Result<Vec<serde_json::Value>, String> {
    let spec = side_spec_by_id(side_id(side))
        .ok_or_else(|| format!("花键参数表：选项表缺方向 {}", side_id(side)))?;
    let values = crate::spline_table::values(side, table)?;
    let mut out = Vec::with_capacity(spec.columns.len());
    for c in spec.columns {
        let value = values
            .iter()
            .find(|(tag, _)| tag == c.tag)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("花键参数表：选项表 tag「{}」没有取值映射", c.tag))?;
        out.push(serde_json::json!({
            "tag": c.tag,
            "label": c.label,
            "unit": c.unit,
            "value": value,
            "formula": c.formula,
            "source": c.source,
        }));
    }
    Ok(out)
}

/// 量棒面板 JSON：标准解 + 3 个工程备选 + 当前 Dp 对应的 Md（选了 Dp 随之重算）。
fn dp_json(side: SplineSide, table: &SplineTable) -> serde_json::Value {
    let pin = side_spec_by_id(side_id(side)).and_then(|s| s.pin.as_ref());
    match (side, table, pin) {
        (SplineSide::Internal, SplineTable::Internal(t), Some(pin)) => {
            let auto = crate::spline_tol::dp_standard_pick(t.dp_calc).ok();
            // 用户口径：「标准解 + 3 个工程备选」都要能点。标准解可能本来就是 3 个最近
            // 系列值之一 → 去重后要再往后取一个，保证 4 个 chip（标准解 + 3 备选）。
            let mut nearest = crate::spline_tol::pin_series()
                .unwrap_or_else(|_| t.dp_candidates.clone());
            nearest.sort_by(|a, b| {
                (a - t.dp_calc)
                    .abs()
                    .partial_cmp(&(b - t.dp_calc).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut choices: Vec<(f64, &'static str, bool)> = Vec::new();
            if let Some(a) = auto {
                choices.push((a, "标准解", true));
            }
            for v in nearest {
                if choices.len() >= 4 {
                    break;
                }
                if choices.iter().any(|(x, _, _)| (x - v).abs() < 1e-9) {
                    continue;
                }
                choices.push((v, "备选", false));
            }
            choices.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let manual = auto.map(|a| (a - t.dp).abs() > 1e-9).unwrap_or(true);
            serde_json::json!({
                "applicable": true,
                "label": pin.label,
                "formula": pin.formula,
                "md_formula": pin.md_formula,
                "standard": pin.standard,
                "current": t.dp,
                "auto": auto,
                "calc": t.dp_calc,
                "manual": manual,
                "choices": choices
                    .into_iter()
                    .map(|(v, tag, std)| serde_json::json!({
                        "value": v, "tag": tag, "standard": std,
                    }))
                    .collect::<Vec<_>>(),
                "md": {
                    "value": t.md,
                    "lower": t.md_lower,
                    "upper": t.md_upper,
                },
            })
        }
        (_, _, Some(_)) => serde_json::json!({ "applicable": false }),
        (_, _, None) => serde_json::json!({
            "applicable": false,
            "reason": "外花键按 GB/T 3478.1 出公法线长度 Wn / 跨测齿数 Kn；\
                       Dp/M_Ri 不在这 21 项里（GB/T 3478.6 的 M_Re 未列入）",
        }),
    }
}

impl SplineTableModel {
    /// 预览 JSON（不碰图纸）：反解读数 + 21 项 + Dp/Md 面板。
    pub fn preview_json(&self) -> Result<serde_json::Value, String> {
        let (input, gear) = self.resolved()?;
        let table = crate::spline_tol::compute(&input)?;
        let items = items_json(input.side, &table)?;
        // 齿根来源：显式 / 表达式 DA·DF 反解 / 按 αD 默认（GUI 只显示“怎么来的”）。
        let root_source = if self.root_form_opt()?.is_some() {
            "explicit"
        } else if crate::spline_table::root_from_gear(&gear, input.side).is_some() {
            "expr"
        } else {
            "default"
        };
        Ok(serde_json::json!({
            "ok": true,
            "side": side_id(input.side),
            "side_label": side_label(input.side),
            "grade_fit": input.grade_fit_label(),
            "expr": self.expr,
            "mark": if gear.involute { "SPLINE" } else { "GEAR" },
            "kind": if input.side == SplineSide::Internal { "IN" } else { "EX" },
            "m": input.m,
            "z": input.z,
            "alpha": input.alpha.deg(),
            "x": gear.x,
            "da": gear.da,
            "df": gear.df,
            "root": root_id(input.root),
            "root_source": root_source,
            "dp": dp_json(input.side, &table),
            "items": items,
        }))
    }

    /// 待放置件要带的 21 个 ATTRIB（tag → 值 + ATTDEF 模板）。
    pub fn pending_attrs(
        &self,
    ) -> Result<Vec<(AttributeDefinition, String)>, String> {
        let input = self.to_input()?;
        let table = crate::spline_tol::compute(&input)?;
        let values = crate::spline_table::values(input.side, &table)?;
        let mut out = Vec::with_capacity(values.len());
        for ad in crate::spline_table::attdefs(input.side) {
            let v = values
                .iter()
                .find(|(tag, _)| tag == &ad.tag)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            out.push((ad, v));
        }
        Ok(out)
    }

    /// `INSERT`（直接落点用；与 CLI `cmd_spline_card` 同一构造函数）。
    pub fn build_insert(&self) -> Result<ocs_plugin_api::host::acadrust::entities::Insert, String> {
        let input = self.to_input()?;
        let table = crate::spline_tol::compute(&input)?;
        let at = self.at.unwrap_or([0.0, 0.0]);
        crate::spline_table::build_insert(input.side, &table, at, self.rot)
    }

    /// 插入回执里的一段（等级/配合 + Dp 口径）。
    pub fn echo_note(&self) -> Result<String, String> {
        let input = self.to_input()?;
        let table = crate::spline_tol::compute(&input)?;
        let dp = match &table {
            SplineTable::Internal(t) => format!(
                "；量棒 Dp={}（{}，D'={}，备选 {} / {} / {}）",
                crate::partgen_kit::trim(t.dp),
                if self.dp.is_some() { "手填，Md 已重算" } else { "标准 R40 自动选" },
                crate::partgen_kit::trim(t.dp_calc),
                crate::partgen_kit::trim(t.dp_candidates[0]),
                crate::partgen_kit::trim(t.dp_candidates[1]),
                crate::partgen_kit::trim(t.dp_candidates[2]),
            ),
            SplineTable::External(_) => String::new(),
        };
        Ok(format!("{}{dp}", input.grade_fit_label()))
    }

    /// 待放置件的 `OCSM_PART` 元数据（薄台账：卡类型/方向/等级配合/Dp）。
    pub fn part_meta_json(&self) -> Result<String, String> {
        let input = self.to_input()?;
        Ok(serde_json::json!({
            "family": "spline_table",
            "card": "花键参数表",
            "side": side_id(input.side),
            "grade_fit": input.grade_fit_label(),
            "dp": self.dp,
        })
        .to_string())
    }
}

// ══════════════════════════════════════════════════════════════════════════
// 测试
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// 表达式档（m3 z20 ALPHA30，平齿根可由 DA/DF 反解；内部 DA=64.5、外部 DF=37）：
    /// 覆盖 GUI 与 CLI 共用同一条表达式反解路径。
    const EXPR_INT_FLAT: &str = "SPLINE IN M3 Z20 ALPHA30 X0 DA64.5 DF57.3436 BETA0 H30";
    const EXPR_EXT_FLAT: &str = "SPLINE EX M2 Z20 ALPHA30 X0 DA42 DF37 BETA0 H30";

    fn tmodel(side: &str) -> SplineTableModel {
        SplineTableModel {
            side: side.into(),
            grade: 6,
            fit: if side == "int" { "H".into() } else { "f".into() },
            expr: if side == "int" { EXPR_INT_FLAT.into() } else { EXPR_EXT_FLAT.into() },
            root: None,
            dp: None,
            at: None,
            rot: 0.0,
        }
    }

    /// ★ 表驱动：每个方向的 21 项口径与 `spline_table` 的 ATTDEF **逐项同序**。
    /// 以后加 ANSI/NF 行或改模板时，这条会先把不一致拦下来。
    #[test]
    fn option_table_covers_all_21_attdefs() {
        for side in [SplineSide::Internal, SplineSide::External] {
            let spec = side_spec_by_id(side_id(side)).unwrap();
            let attdefs = crate::spline_table::attdefs(side);
            assert_eq!(
                spec.columns.len(),
                attdefs.len(),
                "{} 选项表列数应 = ATTDEF 数",
                spec.label
            );
            for (i, (c, ad)) in spec.columns.iter().zip(attdefs.iter()).enumerate() {
                assert_eq!(c.tag, ad.tag, "{} 第 {} 项 tag 应一致", spec.label, i + 1);
                assert!(!c.label.is_empty() && !c.formula.is_empty() && !c.source.is_empty());
            }
            // 插入后用 items_json 组装时，21 个值必须都非空。
            let input = tmodel(side_id(side)).to_input().unwrap();
            let table = crate::spline_tol::compute(&input).unwrap();
            let items = items_json(side, &table).unwrap();
            assert_eq!(items.len(), 21);
            assert!(items.iter().all(|it| !it["value"].as_str().unwrap().is_empty()));
        }
    }

    /// 配合清单来自 `assets/spline_gb3478_fit.csv`：外 6 种、45° 优先 H/k H/h H/f；内恒 H。
    #[test]
    fn fits_come_from_csv_and_mark_45_preference() {
        let ext = fits_json(SplineSide::External).unwrap();
        let codes: Vec<&str> = ext.iter().filter_map(|f| f["code"].as_str()).collect();
        assert_eq!(codes, vec!["k", "js", "h", "f", "e", "d"]);
        let preferred: Vec<&str> = ext
            .iter()
            .filter(|f| f["preferred_45"] == true)
            .filter_map(|f| f["code"].as_str())
            .collect();
        assert_eq!(preferred, vec!["k", "h", "f"], "45° 优先 H/k、H/h、H/f");
        let int = fits_json(SplineSide::Internal).unwrap();
        assert_eq!(int.len(), 1);
        assert_eq!(int[0]["code"], "H");
        // 内花键传外偏差要报错（表驱动校验）。
        let mut m = tmodel("int");
        m.fit = "f".into();
        assert!(m.to_input().is_err());
    }

    /// ★ Dp：标准解 + 3 备选；点选/手填后 Md 必须按所填重算；非系列值报错。
    #[test]
    fn dp_choices_and_md_recompute() {
        let model = tmodel("int");
        let p = model.preview_json().unwrap();
        let dp = &p["dp"];
        assert_eq!(dp["applicable"], true);
        assert!(dp["auto"].as_f64().unwrap() > 0.0);
        let choices = dp["choices"].as_array().unwrap();
        assert!(choices.iter().any(|c| c["standard"] == true), "必须有标准解");
        let cands = model.to_input().unwrap();
        let tbl = crate::spline_tol::compute(&cands).unwrap();
        let auto_md = match &tbl {
            SplineTable::Internal(t) => t.md,
            _ => unreachable!(),
        };
        assert!((dp["md"]["value"].as_f64().unwrap() - auto_md).abs() < 1e-9);
        // 选一个与标准解不同的备选 → Md 变
        let pick = choices
            .iter()
            .map(|c| c["value"].as_f64().unwrap())
            .find(|v| (v - dp["auto"].as_f64().unwrap()).abs() > 1e-9)
            .expect("3 备选里必有与标准解不同的值");
        let mut manual = tmodel("int");
        manual.dp = Some(pick);
        let p2 = manual.preview_json().unwrap();
        assert_eq!(p2["dp"]["current"].as_f64().unwrap(), pick);
        assert_eq!(p2["dp"]["manual"], true);
        assert!(
            (p2["dp"]["md"]["value"].as_f64().unwrap() - auto_md).abs() > 1e-9,
            "选了别的 Dp，Md 必须重算"
        );
        // 标准解 + 3 备选 = 4 个可点 chip（去重后仍要够 4 个）
        assert_eq!(choices.len(), 4, "标准解 + 3 备选：{choices:?}");
        assert_eq!(choices.iter().filter(|c| c["standard"] == true).count(), 1);
        // 手填必须在 GB/T 3478.9 系列内
        let mut bad = tmodel("int");
        bad.dp = Some(1.03);
        let e = bad.preview_json().unwrap_err();
        assert!(e.contains("3478.9"), "{e}");
        // 非系列值 + 系列值都要在导出路径同样拦（pending_attrs 用同一 to_input/compute）
        assert!(bad.pending_attrs().is_err());
    }

    /// 外花键：参数表用公法线长度/跨测齿数，量棒面板不可用（置灰由 GUI 读 applicable=false）。
    #[test]
    fn external_has_wn_and_no_pin_panel() {
        let model = tmodel("ext");
        let p = model.preview_json().unwrap();
        assert_eq!(p["side"], "ext");
        assert_eq!(p["grade_fit"], "6f");
        assert_eq!(p["dp"]["applicable"], false);
        let labels: Vec<&str> = p["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|it| it["label"].as_str())
            .collect();
        assert!(labels.iter().any(|l| l.contains("公法线长度")));
        assert!(labels.iter().any(|l| l.contains("跨测齿数")));
    }

    /// ★ CLI 与 GUI 同源：`SplineTableSpec` 解析的输入和 GUI 模型算出的 21 项完全一致。
    #[test]
    fn gui_model_matches_cli_spec() {
        // 同一段九字段表达式：CLI 与 GUI 两边反解 → 同一份 compute() 结果。
        let expr = "SPLINE IN M3 Z20 ALPHA30 X0 DA64.5 DF57.3436 BETA0 H30";
        let spec = crate::spline_table::SplineTableSpec::parse(&format!("内 6H {expr}")).unwrap();
        let cli = crate::spline_tol::compute(&spec.to_input().unwrap()).unwrap();
        let model = tmodel("int");
        let gui = crate::spline_tol::compute(&model.to_input().unwrap()).unwrap();
        assert_eq!(cli, gui, "同一表达式，CLI 与 GUI 必须算同一份结果");
        // 带 dp 的卡命令与 GUI 手填 dp 走同一字段。
        let spec2 = crate::spline_table::SplineTableSpec::parse(&format!("内 6H {expr} dp 1.0")).unwrap();
        let mut model2 = tmodel("int");
        model2.dp = Some(1.0);
        assert_eq!(
            crate::spline_tol::compute(&spec2.to_input().unwrap()).unwrap(),
            crate::spline_tol::compute(&model2.to_input().unwrap()).unwrap()
        );
    }

    /// 选项 JSON 形状 + 表外参数报错（模型层给直白错误）。
    #[test]
    fn options_json_shape_and_model_validation() {
        let j = options_json().unwrap();
        assert_eq!(j["ok"], true);
        let systems = j["systems"].as_array().unwrap();
        assert_eq!(systems.len(), 1, "本期只有 GB");
        assert_eq!(systems[0]["id"], "gb3478");
        assert_eq!(systems[0]["sides"].as_array().unwrap().len(), 2);
        let int = &systems[0]["sides"][0];
        assert_eq!(int["grades"].as_array().unwrap().len(), 4);
        assert_eq!(int["pin"]["applicable"], true);
        assert_eq!(int["columns"].as_array().unwrap().len(), 21);
        assert_eq!(j["pin_series"].as_array().unwrap().len(), 67);
        // 非法参数：等级/齿根/压力角/体系
        let mut m = tmodel("int");
        m.grade = 8;
        assert!(m.to_input().unwrap_err().contains("等级"));
        let mut m = tmodel("int");
        m.root = Some("bogus".into());
        assert!(m.to_input().unwrap_err().contains("齿根"));
        // 压力角来自表达式：ALPHA20 不是 GB/T 3478 的 αD
        let mut m = tmodel("int");
        m.expr = "GEAR IN M2 Z20 ALPHA20 X0 DA44 DF35 BETA0 H30".into();
        assert!(m.to_input().unwrap_err().contains("压力角"));
        // 方向字符串非法（体系已从入参移除；未知方向仍应报错）
        let mut m = tmodel("int");
        m.side = "bogus".into();
        assert!(m.to_input().unwrap_err().contains("方向"));
        // 齿数来自表达式：Z4 < 6
        let mut m = tmodel("int");
        m.expr = "SPLINE IN M3 Z4 ALPHA30 X0 DA16.5 DF10 BETA0 H30".into();
        assert!(m.to_input().unwrap_err().contains("齿数"));
        // 表达式 KIND 与卡片方向不一致
        let mut m = tmodel("int");
        m.expr = "SPLINE EX M3 Z20 ALPHA30 X0 DA63 DF54.6 BETA0 H30".into();
        assert!(m.to_input().unwrap_err().contains("KIND"));
    }

    /// 预览 JSON 的 items 带 formula/source（信息分层：进 title，不进可见文本）。
    #[test]
    fn items_carry_formula_and_source_for_titles() {
        let p = tmodel("int").preview_json().unwrap();
        let first = &p["items"][0];
        assert_eq!(first["tag"], "(内)齿形角");
        assert!(first["formula"].as_str().unwrap().contains("GB/T 3478.1"));
        assert!(first["source"].as_str().unwrap().contains("GB/T 3478.1"));
        assert_eq!(first["unit"], "°");
    }

    /// ★ 排版回归（用户截图：`D_ii 78.113₃³` / `Md 74.255…` 主值与公差杂糅）：
    /// 主值截到 0.001 + 值列/公差列实体级字宽 0.7 后——
    /// ① 主值右边界 ≤ 公差列左边界（所有非公差行）；② 配对的主值框与上/下公差框不相交；
    /// ③ 小数点后最多 3 位（显示与内部计算分开）。
    #[test]
    fn main_values_clear_tolerance_column() {
        use crate::spline_table::{attdefs, build_insert, text_box, VALUE_WIDTH_FACTOR, TOL_X_SHIFT};
        use std::collections::HashMap;
        // 回归档：截图那两条（D_ii / Md）取一档长小数组合（m3 z26 = 分度圆 78）。
        let cases: &[(SplineSide, f64, u32, PressureAngle, RootForm, u32, ExtDev)] = &[
            (SplineSide::Internal, 3.0, 26, PressureAngle::A30, RootForm::Flat, 6, ExtDev::H),
            (SplineSide::Internal, 2.0, 20, PressureAngle::A30, RootForm::Flat, 6, ExtDev::H),
            (SplineSide::Internal, 10.0, 10, PressureAngle::A30, RootForm::Fillet, 5, ExtDev::H),
            (SplineSide::Internal, 1.0, 40, PressureAngle::A37_5, RootForm::Fillet, 7, ExtDev::H),
            (SplineSide::External, 3.0, 26, PressureAngle::A30, RootForm::Fillet, 5, ExtDev::F),
            (SplineSide::External, 5.0, 15, PressureAngle::A30, RootForm::Flat, 4, ExtDev::Js),
            (SplineSide::External, 2.0, 20, PressureAngle::A45, RootForm::Fillet, 7, ExtDev::K),
        ];
        let overlap = |a: [f64; 4], b: [f64; 4]| {
            a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
        };
        assert_eq!(
            VALUE_WIDTH_FACTOR, 0.7,
            "值列/公差列实体级字宽（不动 OCSM_GB 样式）"
        );
        for &(side, m, z, alpha, root, grade, fit) in cases {
            let input = SplineInput {
                m,
                z,
                alpha,
                root,
                side,
                grade,
                ext_dev: fit,
                fit_length: None,
                dp: None,
            };
            let table = crate::spline_tol::compute(&input).unwrap();
            let ins = build_insert(side, &table, [0.0, 0.0], 0.0).unwrap();
            let attrs: HashMap<String, _> = ins
                .attributes
                .iter()
                .map(|a| (a.tag.clone(), a.clone()))
                .collect();
            assert_eq!(attrs.len(), 21);
            // 公差列左边界 = 该侧公差行 ATTDEF 的最小 x（模板 ≈ −9.63，实体级右移一个字符宽后 ≈ −8.37）。
            let tol_left = attdefs(side)
                .iter()
                .filter(|ad| main_tag_of_dev(&ad.tag).is_some())
                .map(|ad| ad.insertion_point.x)
                .fold(f64::MAX, f64::min);
            assert!(
                (-10.0 + TOL_X_SHIFT..-9.0 + TOL_X_SHIFT).contains(&tol_left),
                "公差列左边界异常（右移 {TOL_X_SHIFT} 后）：{tol_left}"
            );
            for c in side_spec_by_id(side_id(side)).unwrap().columns {
                let a = attrs.get(c.tag).unwrap_or_else(|| panic!("缺属性 {}", c.tag));
                let box_of = |ad: &ocs_plugin_api::host::acadrust::entities::AttributeEntity| {
                    text_box(
                        [ad.insertion_point.x, ad.insertion_point.y],
                        ad.height,
                        ad.width_factor,
                        &ad.value,
                    )
                };
                let b = box_of(a);
                // ③ 主值/公差显示精度：最多 3 位小数。
                if let Some(dot) = a.value.find('.') {
                    let n = a.value[dot + 1..]
                        .chars()
                        .take_while(|ch| ch.is_ascii_digit())
                        .count();
                    assert!(n <= 3, "{} 小数超过 3 位：{}", c.tag, a.value);
                }
                match main_tag_of_dev(c.tag) {
                    // ② 配对的主值框与上/下公差框不相交（左对齐下 = 主值右边界 ≤ 公差左边界）。
                    Some(main_tag) => {
                        let main = attrs.get(main_tag).unwrap();
                        let mb = box_of(main);
                        assert!(
                            !overlap(mb, b),
                            "{main_tag} 与 {} 重叠：主值 {} → 右 {:.3}，公差 {} → 左 {:.3}",
                            c.tag,
                            main.value,
                            mb[2],
                            a.value,
                            b[0]
                        );
                        assert!(mb[2] <= b[0] + 1e-9);
                        assert!(
                            b[2] <= 1e-9,
                            "{} 公差文本超出表右边界：{}",
                            c.tag,
                            a.value
                        );
                    }
                    // ① 所有非公差（主值）右边界不得进入公差列。
                    None => {
                        assert!(
                            b[2] <= tol_left + 1e-9,
                            "{} 主值进入公差列：{} → 右边界 {:.3} > {:.3}",
                            c.tag,
                            a.value,
                            b[2],
                            tol_left
                        );
                    }
                }
            }
        }
    }
}
