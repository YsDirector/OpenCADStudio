//! OCSMGEAR —— 参数化圆柱齿轮画法（一期：**外齿轮**）。
//!
//! # 画法唯一权威
//!
//! 用户模板 `~/桌面/OCSM/齿轮/齿轮画法.dxf`（m=2, z=40, ha*=1, c*=0.25, β=0, h=20, Xn=0，
//! 即 d=80 / da=84 / df=75）。本文件的每个构造规则都是从那张图**反解并逐点核对**出来的，
//! 不是查手册臆造的：
//!
//! | 元素 | 规则 | 反解核对 |
//! |---|---|---|
//! | 齿廓 | 渐开线（α=20°），用 **clamped 3 次 B 样条** 表示，7 个控制点 | 全 80 条样条对理论渐开线最大偏差 **0.012°** |
//! | 拟合点 | 齿根圆角切点 + `r = rb + (ra−rb)·{¼,½,¾,1}` 共 5 点，**弦长参数化** | 模板 5 个拟合点半径 38.1327/38.6908/39.7939/40.8970/42.0001 |
//! | 齿根过渡圆角 | ρ = **0.38m**（GB/T 1356），圆心在 `r = rf+ρ` 上、且到齿廓起点距离 = ρ | 模板圆心 r=38.2600 = rf+ρ；其弧终点与样条起点同一点 |
//! | 齿顶弧 / 齿根弧 | 齿顶弧在**齿中心线**两侧各半条；齿根弧在**齿槽中心线**两侧各半条 | 每齿 8 个实体，40 齿 = 320 个 |
//! | 分度圆 | 点划线（3中心线层）圆 | 簇3/簇4 都在 3中心线层 |
//! | 剖视图剖面线 | ANSI31、比例 1.0、角度 0，**一个 HATCH 两个边界环**（轴线上下各一环，范围 = 齿根圆↔轴线） | 模板 HATCH 2 个路径，y∈[22.58,60.08] 与 [60.08,97.58] |
//! | 轴向倒角 | C = **round(0.6m)**（用户定案 2026-09-17），45°，落在齿顶圆柱两端 | 模板 C=1 = round(0.6×2) |
//! | 中心线 | 长度 = 直径/长度 + **6n**（n=视图比例），与 `OCSMCENTERLINE` 同一口径 | 簇4 十字 = da+6；模板另两处 da+4/h+4 属手绘偏差，统一取 6n |
//! | 螺旋线（斜齿轮） | **侧视图**里三条平行的 2细线层细实线，与中心线夹角 = 90°−β，间距 k×5mm，左旋 `\\` 右旋 `/` | 用户 2026-09-17 口述规则 |
//!
//! # 精度硬约束（用户 2026-09-17）
//!
//! **不提高精度**：模板这套（每齿 8 实体、齿廓 7 控制点）是"兼顾加工与性能"的验证过的
//! 最佳平衡，试过更高精度电脑带不动。本实现保持同一实体数/点数，只把拟合点取在**精确**
//! 渐开线上（模板自身的点上误差约 0.01mm，本实现 ≤3µm，同一量级）。
//!
//! # 一期范围
//!
//! 外齿轮；支持 β≠0（斜齿轮，端面参数换算 + 侧视三细线）与 Xn≠0（变位）。**内齿轮二期**。

use ocs_plugin_api::host::acadrust::entities::hatch::{
    BoundaryEdge, BoundaryPath, HatchPattern, HatchPatternLine, LineEdge,
};
use ocs_plugin_api::host::acadrust::entities::{Arc, Circle, Hatch, Line, Spline};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector2, Vector3};
use ocs_plugin_api::host::acadrust::EntityType;

use crate::partgen::{GenPart, PartMeta, LAYER_CENTER, LAYER_MAIN, LAYER_THIN};

/// 剖面线层（与 partgen_more/partgen_kit 同一层名）。
pub const LAYER_HATCH: &str = "5剖面线层";

/// 基准齿形角（GB/T 1356）。
pub const ALPHA_N_DEG: f64 = 20.0;
/// 齿根过渡圆角系数 ρf = 0.38m（GB/T 1356；模板实测 0.76 = 0.38×2）。
pub const RHO_RATIO: f64 = 0.38;
/// 轴向倒角系数：C = round(0.6m)（用户 2026-09-17 定案）。
pub const CHAMFER_RATIO: f64 = 0.6;
/// 齿廓样条的 5 个拟合点半径 = rb + (ra−rb)·FIT_FRACTIONS（反解自模板）。
pub const FIT_FRACTIONS: [f64; 5] = [0.125, 0.25, 0.5, 0.75, 1.0];
/// 中心线伸出量系数：长度 = 被标注长度 + 6n（与 OCSMCENTERLINE 同口径）。
pub const CENTER_OVERHANG: f64 = 6.0;
/// 斜齿轮侧视图三条细实线的间距 = HELIX_SPACING×k（k = 视图比例）。
pub const HELIX_SPACING: f64 = 5.0;

// ─────────────────────────── 参数 ───────────────────────────

/// 齿轮几何参数（GUI / 命令行同一套）。
#[derive(Debug, Clone, PartialEq)]
pub struct GearParams {
    /// 法向模数 m（GB/T 1357 优先系列；斜齿轮时是 **Mn**）
    pub m: f64,
    /// 齿数 z
    pub z: u32,
    /// 齿顶高系数 ha*
    pub ha: f64,
    /// 顶隙系数 c*
    pub c: f64,
    /// 螺旋角 β（**度**，右旋为正、左旋为负；0 = 直齿）
    pub beta_deg: f64,
    /// 齿轮厚度（轴向宽度）h
    pub h: f64,
    /// 变位系数 Xn
    pub x: f64,
}

impl Default for GearParams {
    fn default() -> Self {
        // 与用户模板/DXF 完全一致的示例件
        Self { m: 2.0, z: 40, ha: 1.0, c: 0.25, beta_deg: 0.0, h: 20.0, x: 0.0 }
    }
}

impl GearParams {
    /// 基准齿形角（弧度）。
    pub fn alpha_n(&self) -> f64 {
        ALPHA_N_DEG.to_radians()
    }
    /// 螺旋角（弧度，带正负）。
    pub fn beta(&self) -> f64 {
        self.beta_deg.to_radians()
    }
    /// 是否斜齿轮。
    pub fn is_helical(&self) -> bool {
        self.beta_deg.abs() > 1e-9
    }
    /// 端面模数 mt = Mn/cosβ。
    pub fn mt(&self) -> f64 {
        self.m / self.beta().cos()
    }
    /// 端面压力角 αt = atan(tanαn/cosβ)。
    pub fn alpha_t(&self) -> f64 {
        (self.alpha_n().tan() / self.beta().cos()).atan()
    }
    /// 分度圆直径 d = mt·z。
    pub fn d(&self) -> f64 {
        self.mt() * self.z as f64
    }
    /// 齿顶高（不扣齿顶高变动系数 Δy —— 单件图没有配对中心距信息）。
    pub fn ha_height(&self) -> f64 {
        self.m * (self.ha + self.x)
    }
    /// 齿根高。
    pub fn hf_height(&self) -> f64 {
        self.m * (self.ha + self.c - self.x)
    }
    /// 齿顶圆直径 da。
    pub fn da(&self) -> f64 {
        self.d() + 2.0 * self.ha_height()
    }
    /// 齿根圆直径 df。
    pub fn df(&self) -> f64 {
        (self.d() - 2.0 * self.hf_height()).max(0.0)
    }
    /// 基圆直径 db = d·cosαt。
    pub fn db(&self) -> f64 {
        self.d() * self.alpha_t().cos()
    }
    /// 端面分度圆齿厚 st = (πMn/2)/cosβ + 2Xn·Mn·tanαn。
    pub fn st(&self) -> f64 {
        std::f64::consts::PI * self.m / 2.0 / self.beta().cos()
            + 2.0 * self.x * self.m * self.alpha_n().tan()
    }
    /// 齿根过渡圆角半径 ρ = 0.38Mn。
    pub fn rho(&self) -> f64 {
        RHO_RATIO * self.m
    }
    /// 轴向倒角 C = round(0.6m)（取整；小模数可能为 0 = 不倒角）。
    pub fn chamfer(&self) -> f64 {
        (CHAMFER_RATIO * self.m).round()
    }
    /// 齿距角（弧度）。
    pub fn pitch_angle(&self) -> f64 {
        std::f64::consts::TAU / self.z as f64
    }
    /// 齿厚半角 ψ(R)（弧度）：齿中心线到该半径处齿廓的夹角。
    ///
    /// `ψ(R) = st/(2r) + (inv αt − inv αR)`，`αR = acos(rb/R)`。
    pub fn half_tooth_angle(&self, radius: f64) -> f64 {
        let r = self.d() / 2.0;
        let rb = self.db() / 2.0;
        let a_r = (rb / radius.max(rb)).clamp(-1.0, 1.0).acos();
        self.st() / (2.0 * r) + (inv(self.alpha_t()) - inv(a_r))
    }
    /// 参数自检（错误信息直接给用户看）。
    pub fn validate(&self) -> Result<(), String> {
        if !(self.m.is_finite() && self.m > 0.0) {
            return Err("模数 m 必须是正数。".into());
        }
        if !(2..=1000).contains(&self.z) {
            return Err(format!("齿数 z 超出范围（2–1000）：{}", self.z));
        }
        if !(self.ha.is_finite() && self.ha >= 0.5) {
            return Err("齿顶高系数 ha* 至少 0.5。".into());
        }
        if !(self.c.is_finite() && self.c >= 0.0) {
            return Err("顶隙系数 c* 不能为负。".into());
        }
        if !(self.beta_deg.is_finite() && self.beta_deg.abs() < 45.0) {
            return Err(format!("螺旋角 β 超出范围（|β|<45°）：{}°", self.beta_deg));
        }
        if !(self.h.is_finite() && self.h > 0.0) {
            return Err("厚度 h 必须是正数。".into());
        }
        if !(self.x.is_finite() && self.x.abs() <= 1.0) {
            return Err(format!("变位系数 Xn 超出范围（|Xn|≤1）：{}", self.x));
        }
        if self.df() <= 1e-6 {
            return Err("齿根圆直径非正：检查 m/z/ha*/c*/Xn 组合。".into());
        }
        if self.chamfer() * 2.0 >= self.h {
            return Err(format!(
                "轴向倒角 C=round(0.6m)={:.0} 太大（厚度 h={:.1}）——加大厚度或减小模数。",
                self.chamfer(),
                self.h
            ));
        }
        if self.chamfer() >= self.da() / 2.0 {
            return Err("轴向倒角大于齿顶圆半径，无法画图。".into());
        }
        Ok(())
    }
    /// 生成后的提示（不阻断，只提醒）。
    pub fn notes(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.z < 17 && self.x.abs() < 1e-9 {
            v.push(format!(
                "齿数 z={} < 17 且未变位：实际滚齿会有根切，本视图未画根切（如需要请给变位或根切画法模板）。",
                self.z
            ));
        }
        if self.is_helical() {
            v.push(format!(
                "斜齿轮（β={}°，{}）：正视图齿廓按端面参数 mt={:.4}／αt={:.3}° 画，侧视图按 GB 简化画法加三条细实线表示轮齿倾斜方向。",
                trim(self.beta_deg),
                if self.beta_deg > 0.0 { "右旋" } else { "左旋" },
                self.mt(),
                self.alpha_t().to_degrees()
            ));
        }
        if self.x.abs() > 1e-9 {
            v.push(format!(
                "变位齿轮（Xn={}）：da/df 与齿厚按国标公式算（齿顶高未扣 Δy，单件图无配对中心距）。",
                trim(self.x)
            ));
        }
        v
    }
    /// 规格文本（块名/明细表用）。
    pub fn spec(&self) -> String {
        let mut s = format!("m{} z{} h{}", trim(self.m), self.z, trim(self.h));
        if self.is_helical() {
            s.push_str(&format!(" β{}", trim(self.beta_deg)));
        }
        if self.x.abs() > 1e-9 {
            s.push_str(&format!(" x{}", trim(self.x)));
        }
        s
    }
}

/// 渐开线展角函数 inv α = tanα − α。
fn inv(a: f64) -> f64 {
    a.tan() - a
}

/// 数字转文本（去掉多余小数）。
pub fn trim(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.4}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ─────────────────────────── 视图 ───────────────────────────

/// 四个视图（一次生成一个）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GearView {
    /// 剖视图（轴向剖切：齿顶圆柱带倒角 + 齿根线 + 上下两环剖面线）
    Section,
    /// 侧视图（轴向投影，不剖；β≠0 时带三条螺旋线细实线）
    Side,
    /// 简化正视图（齿顶圆 + 分度圆 + 齿根圆 + 十字中心线）
    Simplified,
    /// 常规正视图（真实渐开线齿廓 + 分度圆 + 十字中心线）
    Front,
}

impl GearView {
    /// 英文键（命令行 `view=`、块名后缀、SVG 参数）。
    pub fn key(self) -> &'static str {
        match self {
            GearView::Section => "section",
            GearView::Side => "side",
            GearView::Simplified => "simplified",
            GearView::Front => "front",
        }
    }
    /// 中文名（GUI 按钮、报错提示）。
    pub fn label(self) -> &'static str {
        match self {
            GearView::Section => "剖视图",
            GearView::Side => "侧视图",
            GearView::Simplified => "简化正视图",
            GearView::Front => "常规正视图",
        }
    }
    pub const ALL: [GearView; 4] =
        [GearView::Section, GearView::Side, GearView::Simplified, GearView::Front];

    /// 解析视图名（中英文名都认；**不用数字别名**——那会和位置参数 `<m> <z> <h>` 冲突）。
    pub fn parse(s: &str) -> Result<Self, String> {
        let t = s.trim().to_ascii_lowercase();
        let t = t
            .trim_start_matches("view")
            .trim_start_matches('=')
            .trim_start_matches(':')
            .trim();
        let hit = match t {
            "section" | "cut" | "剖" | "剖视" | "剖视图" | "剖面" => Some(GearView::Section),
            "side" | "axial" | "侧" | "侧视" | "侧视图" => Some(GearView::Side),
            "simplified" | "simple" | "简化" | "简化正视图" | "简化视图" => {
                Some(GearView::Simplified)
            }
            "front" | "regular" | "正视" | "正视图" | "常规" | "常规正视" | "常规正视图" => {
                Some(GearView::Front)
            }
            _ => None,
        };
        hit.ok_or_else(|| {
            format!(
                "视图名无法识别：`{}`。可用：section|剖视图、side|侧视图、simplified|简化正视图、front|常规正视图。",
                s.trim()
            )
        })
    }
}

// ─────────────────────────── 齿廓（渐开线 + 圆角 + 样条）─────────

/// 一条齿廓（含齿根圆角）的构造结果 —— 局部坐标，齿中心线为 +x 轴。
#[derive(Debug, Clone)]
struct Flank {
    /// 渐开线在 5 个拟合点上的点
    fit: [[f64; 2]; 5],
    /// 7 个 B 样条控制点
    ctrl: Vec<[f64; 2]>,
    /// 11 个节点（弦长参数化，clamped）
    knots: Vec<f64>,
    /// 齿廓起点（= 圆角弧终点）
    start: [f64; 2],
    /// 齿廓终点（在齿顶圆上）
    end: [f64; 2],
    /// 齿根圆角圆心
    fillet_c: [f64; 2],
    /// 齿根圆角与齿根圆的切点
    root_pt: [f64; 2],
    /// 圆角弧的起/讫角（相对**圆角圆心**，度，CCW）
    fillet_a0: f64,
    fillet_a1: f64,
}

/// 渐开线上一点：齿中心线角 `c`（弧度），`sign`=+1 取齿廓一侧（逆时针那侧）。
fn involute_point(p: &GearParams, radius: f64, c: f64, sign: f64) -> [f64; 2] {
    let th = c + sign * p.half_tooth_angle(radius);
    [radius * th.cos(), radius * th.sin()]
}

/// B 样条基函数 N_{i,p}(u)（Cox–de Boor，clamped 节点向量）。
fn basis(i: usize, p: usize, u: f64, knots: &[f64]) -> f64 {
    if p == 0 {
        let last = knots.len() - 1;
        if i + 1 == last && (u - knots[last]).abs() < 1e-12 {
            return 1.0; // 右端点归入最后一段
        }
        return if knots[i] <= u && u < knots[i + 1] { 1.0 } else { 0.0 };
    }
    let mut out = 0.0;
    let d1 = knots[i + p] - knots[i];
    if d1 > 0.0 {
        out += (u - knots[i]) / d1 * basis(i, p - 1, u, knots);
    }
    let d2 = knots[i + p + 1] - knots[i + 1];
    if d2 > 0.0 {
        out += (knots[i + p + 1] - u) / d2 * basis(i + 1, p - 1, u, knots);
    }
    out
}

/// 高斯消元（列主元）解 n×n 线性方程组；`b` 会被改写。
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Result<Vec<f64>, String> {
    let n = b.len();
    for c in 0..n {
        let mut piv = c;
        for r in c + 1..n {
            if a[r][c].abs() > a[piv][c].abs() {
                piv = r;
            }
        }
        if a[piv][c].abs() < 1e-12 {
            return Err("齿廓样条方程组奇异（参数太极端）".into());
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            if f == 0.0 {
                continue;
            }
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for k in i + 1..n {
            s -= a[i][k] * x[k];
        }
        x[i] = s / a[i][i];
    }
    Ok(x)
}

/// 由 5 个拟合点解出 clamped 3 次 B 样条的 7 个控制点。
///
/// 未知数 d1..d5（d0 = Q0、d6 = Q4 由 clamped 条件固定）。方程：
/// * 3 条插值方程（参数 = 三个内节点）；
/// * 2 条**自然端条件**（两端二阶导为 0）—— 模板样条的端条件无法从 DXF 反解，
///   自然端条件给出的曲线与模板逐点核对差 < 2µm、与精确渐开线差 < 3µm。
fn fit_cubic_bspline(knots: &[f64], fit: &[[f64; 2]; 5]) -> Result<Vec<[f64; 2]>, String> {
    const P: usize = 3;
    let mut a = vec![vec![0.0f64; 5]; 5];
    let (mut bx, mut by) = (vec![0.0; 5], vec![0.0; 5]);
    for (row, k) in (1..=3usize).enumerate() {
        let u = knots[P + k];
        for (col, i) in (1..=5usize).enumerate() {
            a[row][col] = basis(i, P, u, knots);
        }
        let w0 = basis(0, P, u, knots);
        let w6 = basis(6, P, u, knots);
        bx[row] = fit[k][0] - w0 * fit[0][0] - w6 * fit[4][0];
        by[row] = fit[k][1] - w0 * fit[0][1] - w6 * fit[4][1];
    }
    // 自然端条件（clamped 下：C''0 ∝ d0−2d1+d2 = 0，C''1 ∝ d4−2d5+d6 = 0）
    a[3][0] = -2.0;
    a[3][1] = 1.0;
    bx[3] = -fit[0][0];
    by[3] = -fit[0][1];
    a[4][3] = 1.0;
    a[4][4] = -2.0;
    bx[4] = -fit[4][0];
    by[4] = -fit[4][1];
    let xs = solve(a.clone(), bx)?;
    let ys = solve(a, by)?;
    let mut d = Vec::with_capacity(7);
    d.push(fit[0]);
    for i in 0..5 {
        d.push([xs[i], ys[i]]);
    }
    d.push(fit[4]);
    Ok(d)
}

/// 由参数生成一条齿廓（含齿根圆角），`c` = 齿中心线角（弧度），`sign` = ±1 取哪一侧。
fn make_flank(p: &GearParams, c: f64, sign: f64) -> Result<Flank, String> {
    let r = p.d() / 2.0;
    let ra = p.da() / 2.0;
    let rf = p.df() / 2.0;
    let rb = p.db() / 2.0;
    let rho = p.rho();
    let _ = (r, rb); // 供将来扩展用（当前只依赖 fit/half_tooth_angle）

    // ① 5 个拟合点：渐开线上 r = rb + (ra−rb)·{1/8,¼,½,¾,1}（反解自模板）
    let mut fit = [[0.0f64; 2]; 5];
    for (i, f) in FIT_FRACTIONS.iter().enumerate() {
        let radius = rb + (ra - rb) * f;
        fit[i] = involute_point(p, radius, c, sign);
    }
    // ② 弦长参数化 → clamped 节点（**11 个** = 7 控制点 + 3 次 + 1，与模板同构）
    //    内节点只有 3 个（首尾各 4 个被 clamped 吸收）：U = [0×4, c1, c1+c2, c1+c2+c3, Σ×4]
    let mut knots = vec![0.0, 0.0, 0.0, 0.0];
    let mut acc = 0.0;
    for i in 0..4 {
        acc += ((fit[i + 1][0] - fit[i][0]).powi(2) + (fit[i + 1][1] - fit[i][1]).powi(2)).sqrt();
        if i < 3 {
            knots.push(acc);
        }
    }
    knots.extend_from_slice(&[acc; 4]);
    debug_assert_eq!(knots.len(), 11);
    // ③ 控制点
    let ctrl = fit_cubic_bspline(&knots, &fit)?;

    // ④ 齿根圆角：圆心在 r = rf+ρ 上，且 |圆心 − 齿廓起点| = ρ（余弦定理直接解）
    let start = fit[0];
    let rs = (start[0] * start[0] + start[1] * start[1]).sqrt();
    let a_rad = rf + rho;
    let cosd = ((a_rad * a_rad + rs * rs - rho * rho) / (2.0 * a_rad * rs)).clamp(-1.0, 1.0);
    let dth = cosd.acos();
    let th_start = start[1].atan2(start[0]);
    // 圆心在齿廓起点**朝齿槽一侧**：sign=+1 时角更大
    let th_c = th_start + sign * dth;
    let fillet_c = [a_rad * th_c.cos(), a_rad * th_c.sin()];
    // 与齿根圆的切点：圆心与原点连线的延长（半径方向最短点）
    let root_pt = [rf * th_c.cos(), rf * th_c.sin()];
    // 圆角弧的起/讫角（相对圆角圆心，度）
    let ang = |q: [f64; 2]| {
        let v = (q[0] - fillet_c[0], q[1] - fillet_c[1]);
        v.1.atan2(v.0).to_degrees()
    };
    let (a_root, a_start) = (ang(root_pt), ang(start));
    let (mut a0, mut a1) = (a_root, a_start);
    // 取 CCW 弧（不超过 180° 那个方向）：两者互换后仍不超过 180° 时取 CCW
    if (a1 - a0).rem_euclid(360.0) > 180.0 {
        std::mem::swap(&mut a0, &mut a1);
    }

    // ⑤ 齿廓终点角度（齿顶弧要接上）
    Ok(Flank {
        fit,
        ctrl,
        knots,
        start,
        end: fit[4],
        fillet_c,
        root_pt,
        fillet_a0: a0,
        fillet_a1: a1,
    })
}

/// 一条齿廓对应的 B 样条实体。
fn spline_of(f: &Flank) -> EntityType {
    let mut sp = Spline::new();
    sp.degree = 3;
    sp.flags.planar = true;
    sp.flags.rational = true; // 与模板一致：标 rational + 权重全 1（等价非有理）
    sp.knots = f.knots.clone();
    sp.control_points = f.ctrl.iter().map(|q| Vector3::new(q[0], q[1], 0.0)).collect();
    sp.weights = vec![1.0; f.ctrl.len()];
    sp.normal = Vector3::new(0.0, 0.0, 1.0);
    set_layer(&mut sp, LAYER_MAIN);
    EntityType::Spline(sp)
}

// ─────────────────────────── 实体助手 ───────────────────────────

fn line(a: [f64; 2], b: [f64; 2], layer: &str) -> EntityType {
    let mut l = Line::from_points(Vector3::new(a[0], a[1], 0.0), Vector3::new(b[0], b[1], 0.0));
    set_layer(&mut l, layer);
    EntityType::Line(l)
}

fn arc_deg(c: [f64; 2], r: f64, a0: f64, a1: f64, layer: &str) -> EntityType {
    let mut a = Arc::from_center_radius_angles(
        Vector3::new(c[0], c[1], 0.0),
        r,
        a0.to_radians(),
        a1.to_radians(),
    );
    set_layer(&mut a, layer);
    EntityType::Arc(a)
}

fn circle(c: [f64; 2], r: f64, layer: &str) -> EntityType {
    let mut e = Circle::from_center_radius(Vector3::new(c[0], c[1], 0.0), r);
    set_layer(&mut e, layer);
    EntityType::Circle(e)
}

fn set_layer<T: CommonLayer>(e: &mut T, layer: &str) {
    let c = e.common_mut();
    c.layer = layer.to_string();
    c.color = Color::ByLayer;
    c.linetype = "ByLayer".to_string();
    c.line_weight = LineWeight::ByLayer;
}

/// 让 set_layer 适配 Line/Arc/Circle/Spline/Hatch。
trait CommonLayer {
    fn common_mut(&mut self) -> &mut ocs_plugin_api::host::acadrust::entities::EntityCommon;
}
macro_rules! impl_common_layer {
    ($($t:ty),*) => {$(impl CommonLayer for $t {
        fn common_mut(&mut self) -> &mut ocs_plugin_api::host::acadrust::entities::EntityCommon { &mut self.common }
    })*};
}
impl_common_layer!(
    Line,
    Arc,
    Circle,
    Spline,
    Hatch,
    ocs_plugin_api::host::acadrust::entities::Insert
);

/// 十字中心线（长度 = 直径 + 6n，与 OCSMCENTERLINE 同规则）。
fn cross_centerlines(center: [f64; 2], dia: f64, n: f64) -> Vec<EntityType> {
    let half = (dia + CENTER_OVERHANG * n) / 2.0;
    vec![
        line([center[0] - half, center[1]], [center[0] + half, center[1]], LAYER_CENTER),
        line([center[0], center[1] - half], [center[0], center[1] + half], LAYER_CENTER),
    ]
}

// ─────────────────────────── 四个视图 ───────────────────────────

/// 常规正视图：真实渐开线齿廓（每齿 8 个实体，与模板同构）。
///
/// 齿中心线在 `pitch/2 + k·pitch`（模板实测：齿中心 4.5°+9k，齿槽中心 0°+9k）。
fn front_regular(p: &GearParams, n: f64) -> Result<Vec<EntityType>, String> {
    let ra = p.da() / 2.0;
    let rf = p.df() / 2.0;
    let pitch = p.pitch_angle();
    // 齿廓在齿中心线两侧各一条（+1 侧 = 逆时针那侧）
    let mut out = Vec::with_capacity(p.z as usize * 8 + 3);
    let c_first = pitch / 2.0; // 模板里的“齿中心在半个齿距处”
    for t in 0..p.z as usize {
        let c = c_first + pitch * t as f64;
        let c_deg = c.to_degrees();
        let fl = make_flank(p, c, 1.0)?; // 该齿 +1 侧
        let fr = make_flank(p, c, -1.0)?; // 该齿 −1 侧
        // 齿根弧（前一个齿槽的半条）：从齿槽中心到本齿 −1 侧圆角切点
        out.push(arc_deg(
            [0.0, 0.0],
            rf,
            c_deg - pitch.to_degrees() / 2.0,
            near(th_of(fr.root_pt), c_deg - 3.0),
            LAYER_MAIN,
        ));
        // 圆角 + 齿廓（−1 侧）
        out.push(arc_deg(fr.fillet_c, p.rho(), fr.fillet_a0, fr.fillet_a1, LAYER_MAIN));
        out.push(spline_of(&fr));
        // 齿顶弧：以齿中心线为界各半条（模板同构）
        let psi_end = p.half_tooth_angle(ra).to_degrees();
        out.push(arc_deg([0.0, 0.0], ra, c_deg - psi_end, c_deg, LAYER_MAIN));
        out.push(arc_deg([0.0, 0.0], ra, c_deg, c_deg + psi_end, LAYER_MAIN));
        // 齿廓 + 圆角（+1 侧）
        out.push(spline_of(&fl));
        out.push(arc_deg(fl.fillet_c, p.rho(), fl.fillet_a0, fl.fillet_a1, LAYER_MAIN));
        // 齿根弧（本齿槽的另半条）：从 +1 侧圆角切点到下一个齿槽中心
        // 最后一齿要收口到 360°（= 第一齿齿槽中心 0°，闭合链条）
        let gap_hi = if t + 1 == p.z as usize {
            360.0
        } else {
            c_deg + pitch.to_degrees() / 2.0
        };
        out.push(arc_deg(
            [0.0, 0.0],
            rf,
            near(th_of(fl.root_pt), c_deg + 3.0),
            gap_hi,
            LAYER_MAIN,
        ));
    }
    // 分度圆（点划线）+ 十字中心线
    out.push(circle([0.0, 0.0], p.d() / 2.0, LAYER_CENTER));
    out.extend(cross_centerlines([0.0, 0.0], p.da(), n));
    Ok(out)
}

/// 极角（度，规范到 0..360）。
fn th_of(q: [f64; 2]) -> f64 {
    let d = q[1].atan2(q[0]).to_degrees();
    if d < 0.0 {
        d + 360.0
    } else {
        d
    }
}

/// 把角度平移到离 `reference` 最近的那个周数（避免 ±180° 回绕：齿中心 4.5° 的
/// 侧根切点算出来 355.85°，不平移的话圆弧会绕一大圈）。
fn near(angle_deg: f64, reference_deg: f64) -> f64 {
    let mut a = angle_deg;
    while a - reference_deg > 180.0 {
        a -= 360.0;
    }
    while reference_deg - a > 180.0 {
        a += 360.0;
    }
    a
}

/// 简化正视图：齿顶圆（粗实线）+ 分度圆（点划线）+ 齿根圆（细实线）+ 十字中心线。
fn front_simplified(p: &GearParams, n: f64) -> Result<Vec<EntityType>, String> {
    let mut out = vec![
        circle([0.0, 0.0], p.da() / 2.0, LAYER_MAIN),
        circle([0.0, 0.0], p.d() / 2.0, LAYER_CENTER),
    ];
    if p.df() > 1e-9 {
        out.push(circle([0.0, 0.0], p.df() / 2.0, LAYER_THIN));
    }
    out.extend(cross_centerlines([0.0, 0.0], p.da(), n));
    Ok(out)
}

/// 轴向视图（侧视/剖视共用）的轮廓：齿顶圆柱（h×da）四角倒角 C。
///
/// 与模板同构：端面线画到 ±(ra−C)、齿顶面画到 ±(h/2−C)。
/// **`steps` 区别两个视图**（反解自模板）：
/// * 侧视图有两条倒角台阶线（x=±(h/2−C)，从 −ra 到 +ra）——模板实测 10 条轮廓线；
/// * 剖视图**没有**这两条（模板实测 18 条里就是它们不在，全剖时该棱边按惯例省略）。
fn axial_outline(p: &GearParams, steps: bool) -> Vec<EntityType> {
    let ra = p.da() / 2.0;
    let hh = p.h / 2.0;
    let c = p.chamfer();
    let mut out = Vec::new();
    let hi = ra - c; // 倒角后齿顶面的一半长度
    let hc = hh - c; // 倒角后端面的一半长度
    // 端面线（±(ra−C) 高）
    out.push(line([-hh, -hi], [-hh, hi], LAYER_MAIN));
    out.push(line([hh, -hi], [hh, hi], LAYER_MAIN));
    // 倒角台阶线（±ra 高）—— 只有侧视图画
    if steps {
        out.push(line([-hc, -ra], [-hc, ra], LAYER_MAIN));
        out.push(line([hc, -ra], [hc, ra], LAYER_MAIN));
    }
    // 齿顶面（±(h/2−C) 长）
    out.push(line([-hc, ra], [hc, ra], LAYER_MAIN));
    out.push(line([-hc, -ra], [hc, -ra], LAYER_MAIN));
    // 四个 45° 倒角
    out.push(line([-hh, hi], [-hc, ra], LAYER_MAIN));
    out.push(line([hc, ra], [hh, hi], LAYER_MAIN));
    out.push(line([-hh, -hi], [-hc, -ra], LAYER_MAIN));
    out.push(line([hc, -ra], [hh, -hi], LAYER_MAIN));
    out
}

/// 侧视图：轴向投影（不剖）。β≠0 时加三条螺旋线细实线。
fn side_view(p: &GearParams, n: f64) -> Result<Vec<EntityType>, String> {
    let mut out = axial_outline(p, true);
    // 分度线（点划线）+ 轴线（中心线）
    let d2 = p.d() / 2.0;
    let cl = p.h + CENTER_OVERHANG * n;
    out.push(line([-cl / 2.0, d2], [cl / 2.0, d2], LAYER_CENTER));
    out.push(line([-cl / 2.0, -d2], [cl / 2.0, -d2], LAYER_CENTER));
    out.push(line([-cl / 2.0, 0.0], [cl / 2.0, 0.0], LAYER_CENTER));
    // 斜齿轮：三条平行细实线（与中心线夹角 = 90°−|β|；左旋 "\"、右旋 "/"）
    if p.is_helical() {
        let ra = p.da() / 2.0;
        let phi = (90.0 - p.beta_deg.abs()).to_radians();
        let phi = if p.beta_deg > 0.0 { phi } else { -phi }; // 右旋 = "/"（方向角 +）
        let (dx, dy) = (phi.cos(), phi.sin());
        // 长度盖住整个视图（含齿顶圆直径与中心线伸出）
        let half = (p.da() + CENTER_OVERHANG * n) / 2.0 / phi.sin().abs().max(1e-3);
        for k in [-1.0f64, 0.0, 1.0] {
            let off = k * HELIX_SPACING * n;
            let (nx, ny) = (-dy, dx); // 线的法向
            let (cx, cy) = (nx * off, ny * off);
            out.push(line(
                [cx - dx * half, cy - dy * half],
                [cx + dx * half, cy + dy * half],
                LAYER_THIN,
            ));
        }
        let _ = ra;
    }
    Ok(out)
}

/// 剖视图：轴向剖切。轮廓 + 齿根线（±df/2）+ 分度线 + 轴线 + 剖面线（轴线上下两环）。
fn section_view(p: &GearParams, n: f64) -> Result<Vec<EntityType>, String> {
    let mut out = axial_outline(p, false);
    let rf = p.df() / 2.0;
    let d2 = p.d() / 2.0;
    let cl = p.h + CENTER_OVERHANG * n;
    // 齿根线（剖切后可见，长度 = h）—— 齿部按不剖（GB/T 4459.2）：只在齿根圆之间打剖面线
    out.push(line([-p.h / 2.0, rf], [p.h / 2.0, rf], LAYER_MAIN));
    out.push(line([-p.h / 2.0, -rf], [p.h / 2.0, -rf], LAYER_MAIN));
    // 分度线 + 轴线
    out.push(line([-cl / 2.0, d2], [cl / 2.0, d2], LAYER_CENTER));
    out.push(line([-cl / 2.0, -d2], [cl / 2.0, -d2], LAYER_CENTER));
    out.push(line([-cl / 2.0, 0.0], [cl / 2.0, 0.0], LAYER_CENTER));
    // 剖面线：一个 HATCH、两个边界环（齿根圆 ↔ 轴线，上下各一环）—— 齿部按不剖
    out.push(section_hatch(p.h, rf, 1.0));
    Ok(out)
}

/// 剖面线（ANSI31，比例 1.0，角度 0；两个矩形环）。
///
/// **写法硬约束**（与 `partgen_kit` 一致，别再改回去）：ANSI31 的基准线角度 45°、
/// offset 必须写成 `(-off, +off)`（off = 3.175×scale），这样宿主反旋转后
/// dx≈0、|dy|=间距；写成 `(+off,+off)` 会让宿主渲染成实心填充。
fn section_hatch(h: f64, rf: f64, pattern_scale: f64) -> EntityType {
    let mut hat = Hatch::new();
    let mut pat = HatchPattern::new("ANSI31");
    pat.description = "ANSI Iron, Brick, Stone masonry".into();
    let off = 3.175 * pattern_scale;
    pat.add_line(HatchPatternLine {
        angle: 45.0f64.to_radians(),
        base_point: Vector2::new(0.0, 0.0),
        offset: Vector2::new(-off, off),
        dash_lengths: Vec::new(),
    });
    hat.pattern = pat;
    hat.is_solid = false;
    hat.pattern_angle = 0.0;
    hat.pattern_scale = pattern_scale;
    // 两个环：下（−rf→0）与上（0→+rf），x 覆盖整个厚度
    for (y0, y1) in [(-rf, 0.0), (0.0, rf)] {
        let mut bp = BoundaryPath::new();
        bp.flags.set_external(true);
        let pts = [
            [-h / 2.0, y0],
            [h / 2.0, y0],
            [h / 2.0, y1],
            [-h / 2.0, y1],
        ];
        for i in 0..4 {
            bp.add_edge(BoundaryEdge::Line(LineEdge {
                start: Vector2::new(pts[i][0], pts[i][1]),
                end: Vector2::new(pts[(i + 1) % 4][0], pts[(i + 1) % 4][1]),
            }));
        }
        hat.paths.push(bp);
    }
    set_layer(&mut hat, LAYER_HATCH);
    EntityType::Hatch(hat)
}

// ─────────────────────────── 对外接口 ───────────────────────────

/// OCSM 初始化检查（**插入前拦一下**，用户 2026-09-17 要求）。
///
/// 没跑过 `OCSM`（也没被其它 OCSM 命令“确保”过）的图纸直接生成齿轮，出图会是：
/// 中心线变**实线白线**（`3中心线层` 没挂 `CENTER2`）、轮廓/剖面线层的颜色线宽也不对。
/// 判据取这两件**直接影响出图外观**的事：图形层在、中心线层挂着点划线。
pub fn ocsm_ready(doc: &ocs_plugin_api::host::acadrust::CadDocument) -> Result<(), String> {
    let find = |name: &str| {
        doc.layers
            .iter()
            .find(|ly| ly.name.eq_ignore_ascii_case(name))
    };
    const NEED: [&str; 4] = [LAYER_MAIN, LAYER_THIN, LAYER_CENTER, LAYER_HATCH];
    let missing: Vec<&str> = NEED.iter().copied().filter(|n| find(n).is_none()).collect();
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
         再生成齿轮；不然中心线会是实线白线、剖面线层颜色也不对。"
    ))
}

/// 生成一个视图的图元（已落 OCSM 图层）。
///
/// `n` = 视图比例系数（图框 `比例` 属性，1:2 → 2.0）：只影响中心线/螺旋线的伸出与间距，
/// 不影响齿轮本身尺寸（齿轮始终按真实尺寸画，图框负责比例）。
pub fn generate(p: &GearParams, view: GearView, n: f64) -> Result<GenPart, String> {
    p.validate()?;
    let entities = match view {
        GearView::Section => section_view(p, n)?,
        GearView::Side => side_view(p, n)?,
        GearView::Simplified => front_simplified(p, n)?,
        GearView::Front => front_regular(p, n)?,
    };
    let bbox = bbox_of(&entities);
    Ok(GenPart {
        entities,
        meta: PartMeta {
            code: p.spec(),
            name: format!("外齿轮（{}）", view.label()),
            spec: p.spec(),
            material: String::new(),
            weight: solid_weight_kg(p),
        },
        bbox,
    })
}

/// 实心（未开孔）齿轮质量估算 kg：齿廓围成的面积 × 厚度 × 钢 7.85e-6 kg/mm³。
///
/// 一期不画轴孔，所以这里给的是**毛坯质量**（明细表里如需要请按实际孔型修正）。
fn solid_weight_kg(p: &GearParams) -> String {
    let ra = p.da() / 2.0;
    let rf = p.df() / 2.0;
    let rho = p.rho();
    let pitch = p.pitch_angle();
    let c_first = pitch / 2.0;
    let r_start = fr_start_radius(p);
    const SEG: usize = 12;
    let arc = |r: f64, a0_deg: f64, a1_deg: f64, out: &mut Vec<[f64; 2]>| {
        for k in 0..=SEG {
            let a = (a0_deg + (a1_deg - a0_deg) * k as f64 / SEG as f64).to_radians();
            out.push([r * a.cos(), r * a.sin()]);
        }
    };
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(p.z as usize * (6 * SEG + 3));
    for t in 0..p.z as usize {
        let c = c_first + pitch * t as f64;
        let (fl, fr) = match (make_flank(p, c, 1.0), make_flank(p, c, -1.0)) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return String::new(),
        };
        // 与 front_regular 完全同一顺序（CCW 闭合链）：
        // ① 齿根弧 → ② −1 侧圆角 → ③ −1 侧齿廓（根→顶）→ ④ 齿顶弧 → ⑤ +1 侧齿廓（顶→根）→ ⑥ +1 侧圆角
        let c_deg = c.to_degrees();
        arc(
            rf,
            c_deg - pitch.to_degrees() / 2.0,
            near(th_of(fr.root_pt), c_deg - 3.0),
            &mut pts,
        );
        arc_center(
            fr.fillet_c,
            rho,
            fr.fillet_a0,
            fr.fillet_a1,
            SEG,
            &mut pts,
        );
        for k in 0..=SEG {
            let r = r_start + (ra - r_start) * k as f64 / SEG as f64;
            pts.push(involute_point(p, r, c, -1.0));
        }
        arc(
            ra,
            c_deg - p.half_tooth_angle(ra).to_degrees(),
            c_deg + p.half_tooth_angle(ra).to_degrees(),
            &mut pts,
        );
        for k in 0..=SEG {
            let r = ra - (ra - r_start) * k as f64 / SEG as f64;
            pts.push(involute_point(p, r, c, 1.0));
        }
        arc_center(fl.fillet_c, rho, fl.fillet_a0, fl.fillet_a1, SEG, &mut pts);
    }
    let n = pts.len();
    let mut area = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        area += a[0] * b[1] - b[0] * a[1];
    }
    area = (area / 2.0).abs();
    format!("{:.3}", area * p.h * 7.85e-6)
}

/// 以 `center` 为圆心、半径 r 的圆弧采样（角度由 a0 到 a1，度）。
fn arc_center(
    center: [f64; 2],
    r: f64,
    a0: f64,
    a1: f64,
    seg: usize,
    out: &mut Vec<[f64; 2]>,
) {
    for k in 0..=seg {
        let a = (a0 + (a1 - a0) * k as f64 / seg as f64).to_radians();
        out.push([center[0] + r * a.cos(), center[1] + r * a.sin()]);
    }
}

/// 齿廓起点（第一个拟合点）半径：rb + (ra−rb)/8。
fn fr_start_radius(p: &GearParams) -> f64 {
    let rb = p.db() / 2.0;
    let ra = p.da() / 2.0;
    rb + (ra - rb) * FIT_FRACTIONS[0]
}

fn bbox_of(entities: &[EntityType]) -> [f64; 4] {
    let mut bb = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut add = |x: f64, y: f64| {
        bb[0] = bb[0].min(x);
        bb[1] = bb[1].min(y);
        bb[2] = bb[2].max(x);
        bb[3] = bb[3].max(y);
    };
    for e in entities {
        match e {
            EntityType::Line(l) => {
                add(l.start.x, l.start.y);
                add(l.end.x, l.end.y);
            }
            EntityType::Circle(c) => {
                add(c.center.x - c.radius, c.center.y - c.radius);
                add(c.center.x + c.radius, c.center.y + c.radius);
            }
            EntityType::Arc(a) => {
                add(a.center.x - a.radius, a.center.y - a.radius);
                add(a.center.x + a.radius, a.center.y + a.radius);
            }
            EntityType::Spline(s) => {
                for q in &s.control_points {
                    add(q.x, q.y);
                }
            }
            _ => {}
        }
    }
    if !bb[0].is_finite() {
        bb = [-1.0, -1.0, 1.0, 1.0];
    }
    bb
}

/// 块名：`OCSM_GEAR_M2_Z40_H20_FRONT`（与标准件 `OCSM_<族>_<规格>_<视图>` 同风格）。
/// 斜齿轮加 `_B<|β|>R|L`（右/左旋），变位加 `_X<Xn>`（负值用 `N` 前缀）。
pub fn block_name(p: &GearParams, view: GearView) -> String {
    let mut s = format!("OCSM_GEAR_M{}_Z{}", trim(p.m).replace('.', "_"), p.z);
    if p.is_helical() {
        s.push_str(&format!(
            "_B{}{}",
            trim(p.beta_deg.abs()).replace('.', "_"),
            if p.beta_deg > 0.0 { "R" } else { "L" }
        ));
    }
    if p.x.abs() > 1e-9 {
        s.push_str(&format!(
            "_X{}{}",
            if p.x < 0.0 { "N" } else { "" },
            trim(p.x.abs()).replace('.', "_")
        ));
    }
    s.push_str(&format!("_H{}", trim(p.h).replace('.', "_")));
    s.push('_');
    s.push_str(&view.key().to_ascii_uppercase());
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

/// 解析命令行参数：`<m> <z> [h=..|ha=..|c=..|beta=..|x=..] [view 名] [at x,y] [rot 度]`
pub struct GearRequest {
    pub params: GearParams,
    pub view: GearView,
    pub at: Option<[f64; 2]>,
    pub rotation: f64,
}

/// 命令行/HTTP 参数解析（人侧 GUI 与 AI 侧共用同一套键名）。
pub fn parse_request(raw: &str) -> Result<GearRequest, String> {
    let usage = "用法：OCSMGEAR <模数m> <齿数z> [h=厚度] [ha=齿顶高系数] [c=顶隙系数] [beta=螺旋角(右旋为正)] [x=变位系数] \
                 [view 剖视图|侧视图|简化正视图|常规正视图] [at x,y] [rot 度]。不带参数则打开齿轮窗口。";
    let toks: Vec<&str> = raw.split_whitespace().collect();
    if toks.is_empty() {
        return Err(usage.into());
    }
    let mut p = GearParams { h: 0.0, ..GearParams::default() };
    let mut h_given = false;
    let mut view: Option<GearView> = None;
    let mut at: Option<[f64; 2]> = None;
    let mut rotation = 0.0;
    let mut positional = 0usize;
    let mut i = 0usize;
    while i < toks.len() {
        let t = toks[i];
        let (key, val) = t.split_once('=').map(|(a, b)| (a, Some(b))).unwrap_or((t, None));
        let mut num = |key: &str, val: Option<&str>| -> Result<f64, String> {
            let v = match val {
                Some(v) => v.to_string(),
                None => {
                    i += 1;
                    toks.get(i).map(|s| s.to_string()).unwrap_or_default()
                }
            };
            v.trim().parse::<f64>().map_err(|_| format!("{} 需要数字，收到 `{}`", key, v))
        };
        match key.to_ascii_lowercase().as_str() {
            "m" | "模数" => {
                p.m = num("m", val)?;
            }
            "z" | "齿数" => {
                let v = num("z", val)?;
                p.z = v.round() as u32;
            }
            "h" | "厚度" => {
                p.h = num("h", val)?;
                h_given = true;
            }
            "ha" | "ha*" => p.ha = num("ha", val)?,
            "c" | "c*" | "顶隙" => p.c = num("c", val)?,
            "beta" | "β" | "螺旋角" => p.beta_deg = num("beta", val)?,
            "x" | "xn" | "变位" => p.x = num("x", val)?,
            "view" | "视图" => {
                // 支持 `view=front`、`view front`、`view=剖视图`
                let name = match val {
                    Some(v) => v.to_string(),
                    None => {
                        i += 1;
                        toks.get(i).map(|s| s.to_string()).unwrap_or_default()
                    }
                };
                view = Some(GearView::parse(&name)?);
            }
            "at" => {
                let pair = match val {
                    Some(v) => v.to_string(),
                    None => {
                        // `at x,y` 或 `at x y`
                        i += 1;
                        let a = toks.get(i).map(|s| s.to_string()).unwrap_or_default();
                        if a.contains(',') || a.contains('，') {
                            a
                        } else {
                            i += 1;
                            format!("{},{}", a, toks.get(i).cloned().unwrap_or_default())
                        }
                    }
                };
                let mut it = pair.split([',', '，']);
                let x = it
                    .next()
                    .unwrap_or("")
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("at 坐标需要 `x,y`，收到 `{}`", pair))?;
                let y = it
                    .next()
                    .unwrap_or("")
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("at 坐标需要 `x,y`，收到 `{}`", pair))?;
                at = Some([x, y]);
            }
            "rot" | "角度" => rotation = num("rot", val)?,
            other => {
                // 位置参数：第 1 个 m、第 2 个 z、第 3 个 h；也可以是中文视图名
                if let Ok(vw) = GearView::parse(other) {
                    view = Some(vw);
                    i += 1;
                    continue;
                }
                let v = other
                    .parse::<f64>()
                    .map_err(|_| format!("无法识别的参数 `{}`。\n{}", other, usage))?;
                match positional {
                    0 => p.m = v,
                    1 => p.z = v.round() as u32,
                    2 => {
                        p.h = v;
                        h_given = true;
                    }
                    _ => return Err(format!("多余的参数 `{}`。\n{}", other, usage)),
                }
                positional += 1;
            }
        }
        i += 1;
    }
    if positional < 2 && (raw.contains("m=") || raw.contains("z=")) == false && toks.len() < 2 {
        return Err(format!("至少要给模数 m 和齿数 z（例如 `OCSMGEAR 2 40 20`）。\n{}", usage));
    }
    if !h_given {
        // 模板里 h=20 是用户给的；没给就按齿宽系数取 10m（够用且好认），并提示
        p.h = (10.0 * p.m).max(1.0);
    }
    if view.is_none() {
        // 中文位置参数形式：`OCSMGEAR 2 40 20 剖视图`
        for t in &toks {
            if let Ok(vw) = GearView::parse(t) {
                view = Some(vw);
                break;
            }
        }
    }
    Ok(GearRequest {
        params: p,
        view: view.unwrap_or(GearView::Section),
        at,
        rotation,
    })
}

// ─────────────────────────── SVG 预览（GUI 用）─────────────────

/// GUI 预览：`/api/gear_svg?m=2&z=40&h=20&ha=1&c=0.25&beta=0&x=0&view=section&n=1`
pub fn preview_svg(query: &str) -> Result<String, String> {
    let (p, view, n) = params_from_query(query)?;
    let part = generate(&p, view, n)?;
    Ok(svg_of(&part.entities, 460.0))
}

/// 从查询串解析参数（`/api/gear_svg` 与 `/api/gear_info` 共用）。
pub fn params_from_query(query: &str) -> Result<(GearParams, GearView, f64), String> {
    let get = |k: &str| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(a, _)| *a == k)
            .map(|(_, v)| v.to_string())
    };
    let f = |k: &str, d: f64| get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d);
    let p = GearParams {
        m: f("m", 2.0),
        z: f("z", 40.0).round() as u32,
        ha: f("ha", 1.0),
        c: f("c", 0.25),
        beta_deg: f("beta", 0.0),
        h: f("h", 20.0),
        x: f("x", 0.0),
    };
    let view = GearView::parse(&get("view").unwrap_or_else(|| "section".into()))?;
    Ok((p, view, f("n", 1.0)))
}

/// `/api/gear_info`：派生尺寸 + 提示（GUI 信息行用）。
pub fn info_json(query: &str) -> Result<String, String> {
    let (p, view, _) = params_from_query(query)?;
    let err = p.validate().err();
    Ok(serde_json::json!({
        "ok": err.is_none(),
        "error": err,
        "view": view.key(),
        "d": round4(p.d()),
        "da": round4(p.da()),
        "df": round4(p.df()),
        "db": round4(p.db()),
        "mt": round4(p.mt()),
        "alpha_t": round4(p.alpha_t().to_degrees()),
        "rho": round4(p.rho()),
        "chamfer": p.chamfer(),
        "pitch_angle": round4(p.pitch_angle().to_degrees()),
        "half_tooth_angle_tip": round4(p.half_tooth_angle(p.da() / 2.0).to_degrees()),
        "helical": p.is_helical(),
        "block": block_name(&p, view),
        "spec": p.spec(),
        "notes": p.notes(),
    })
    .to_string())
}

fn round4(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

/// 把图元渲染成 SVG（预览用，仅支持 LINE/CIRCLE/ARC/SPLINE/HATCH 边界）。
fn svg_of(entities: &[EntityType], size: f64) -> String {
    let bb = bbox_of(entities);
    let (w, h) = (bb[2] - bb[0], bb[3] - bb[1]);
    let span = w.max(h).max(1.0) * 1.12;
    let s = size / span;
    let cx = (bb[0] + bb[2]) / 2.0;
    let cy = (bb[1] + bb[3]) / 2.0;
    let tx = |x: f64| size / 2.0 + (x - cx) * s;
    let ty = |y: f64| size / 2.0 - (y - cy) * s;
    let mut body = String::new();
    let color = |layer: &str| match layer {
        LAYER_MAIN => "#e8e8e8",
        LAYER_THIN => "#5aa0ff",
        LAYER_CENTER => "#ff5555",
        LAYER_HATCH => "#3fa13f",
        _ => "#9a9a9a",
    };
    let width = |layer: &str| if layer == LAYER_MAIN { 1.5 } else { 0.9 };
    for e in entities {
        match e {
            EntityType::Line(l) => {
                body.push_str(&format!(
                    "<line x1='{:.2}' y1='{:.2}' x2='{:.2}' y2='{:.2}' stroke='{}' stroke-width='{}'/>\n",
                    tx(l.start.x), ty(l.start.y), tx(l.end.x), ty(l.end.y),
                    color(&l.common.layer), width(&l.common.layer)
                ));
            }
            EntityType::Circle(c) => {
                body.push_str(&format!(
                    "<circle cx='{:.2}' cy='{:.2}' r='{:.2}' fill='none' stroke='{}' stroke-width='{}'/>\n",
                    tx(c.center.x), ty(c.center.y), c.radius * s,
                    color(&c.common.layer), width(&c.common.layer)
                ));
            }
            EntityType::Arc(a) => {
                let (r, x, y) = (a.radius, a.center.x, a.center.y);
                let (mut a0, mut a1) = (a.start_angle.to_degrees(), a.end_angle.to_degrees());
                while a1 < a0 {
                    a1 += 360.0;
                }
                let (sx, sy) = (x + r * a0.to_radians().cos(), y + r * a0.to_radians().sin());
                let (ex, ey) = (x + r * a1.to_radians().cos(), y + r * a1.to_radians().sin());
                let large = if (a1 - a0) > 180.0 { 1 } else { 0 };
                // SVG 的 y 轴向下 → CCW 变 CW：sweep-flag = 0
                body.push_str(&format!(
                    "<path d='M {:.2} {:.2} A {:.2} {:.2} 0 {} 0 {:.2} {:.2}' fill='none' stroke='{}' stroke-width='{}'/>\n",
                    tx(sx), ty(sy), r * s, r * s, large, tx(ex), ty(ey),
                    color(&a.common.layer), width(&a.common.layer)
                ));
            }
            EntityType::Spline(sp) => {
                if sp.control_points.is_empty() {
                    continue;
                }
                let mut d = String::new();
                for (i, q) in sp.control_points.iter().enumerate() {
                    d.push_str(&format!(
                        "{}{:.2} {:.2} ",
                        if i == 0 { "M " } else { "L " },
                        tx(q.x),
                        ty(q.y)
                    ));
                }
                body.push_str(&format!(
                    "<path d='{}' fill='none' stroke='{}' stroke-width='{}' stroke-linecap='round'/>\n",
                    d.trim_end(),
                    color(&sp.common.layer),
                    width(&sp.common.layer)
                ));
            }
            EntityType::Hatch(hh) => {
                // 预览要把**图案线**画出来（只画边界的话剖视图看着是空的）：
                // 本插件的剖面线环都是轴对齐矩形 → 按 45° 间距裁剪即可，不需要通用多边形裁剪。
                for path in &hh.paths {
                    let mut xs: Vec<f64> = Vec::new();
                    let mut ys: Vec<f64> = Vec::new();
                    for edge in &path.edges {
                        if let BoundaryEdge::Line(le) = edge {
                            xs.push(le.start.x);
                            ys.push(le.start.y);
                            xs.push(le.end.x);
                            ys.push(le.end.y);
                        }
                    }
                    if xs.is_empty() {
                        continue;
                    }
                    let (x0, x1) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
                    let (y0, y1) = (ys.iter().cloned().fold(f64::INFINITY, f64::min), ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
                    // ANSI31：45°，间距 = 3.175 × pattern_scale
                    let step = 3.175 * hh.pattern_scale.max(0.05);
                    let diag = (x1 - x0) + (y1 - y0);
                    let n = (diag / step).ceil() as i32 + 2;
                    let color = color(LAYER_HATCH);
                    let mut segs = String::new();
                    for k in -n..=n {
                        // 直线 x + y = c，c 扫描过整个包围盒
                        let c = (x0 + y0) + step * k as f64;
                        let (mut ax, mut ay) = (x0, c - x0);
                        let (mut bx, mut by) = (x1, c - x1);
                        // 裁剪到矩形
                        let clip = |px: f64, py: f64, ox: f64, oy: f64| -> (f64, f64) {
                            // 沿方向 (ox,oy) 从 (px,py) 推 t 使点落在盒内
                            let mut t0 = f64::NEG_INFINITY;
                            let mut t1 = f64::INFINITY;
                            for (p, o, lo, hi) in [
                                (px, ox, x0, x1),
                                (py, oy, y0, y1),
                            ] {
                                if o.abs() < 1e-12 {
                                    if p < lo || p > hi {
                                        return (f64::NAN, f64::NAN);
                                    }
                                } else {
                                    let (ta, tb) = ((lo - p) / o, (hi - p) / o);
                                    t0 = t0.max(ta.min(tb));
                                    t1 = t1.min(ta.max(tb));
                                }
                            }
                            if t0 > t1 {
                                return (f64::NAN, f64::NAN);
                            }
                            (t0, t1)
                        };
                        // 方向 = (-1, 1)/√2（45°）
                        let (dx, dy) = (-1.0f64 / 2f64.sqrt(), 1.0 / 2f64.sqrt());
                        let (ta, tb) = clip(ax, ay, dx, dy);
                        if ta.is_nan() {
                            continue;
                        }
                        ax += dx * ta;
                        ay += dy * ta;
                        bx = ax + dx * (tb - ta);
                        by = ay + dy * (tb - ta);
                        segs.push_str(&format!(
                            "<line x1='{:.2}' y1='{:.2}' x2='{:.2}' y2='{:.2}' stroke='{color}' stroke-width='0.7'/>\n",
                            tx(ax),
                            ty(ay),
                            tx(bx),
                            ty(by)
                        ));
                    }
                    body.push_str(&segs);
                }
            }
            _ => {}
        }
    }
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{size}' height='{size}' viewBox='0 0 {size} {size}'>\
         <rect width='{size}' height='{size}' fill='#111'/>\n{body}</svg>"
    )
}

// ─────────────────────────── 测试 ───────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// 用户模板（齿轮画法.dxf）的实测数字 —— 这是本文件所有断言的依据。
    fn tmpl() -> GearParams {
        GearParams { m: 2.0, z: 40, ha: 1.0, c: 0.25, beta_deg: 0.0, h: 20.0, x: 0.0 }
    }

    #[test]
    fn template_basic_dimensions() {
        let p = tmpl();
        p.validate().unwrap();
        assert!((p.d() - 80.0).abs() < 1e-9, "分度圆 80");
        assert!((p.da() - 84.0).abs() < 1e-9, "齿顶圆 84");
        assert!((p.df() - 75.0).abs() < 1e-9, "齿根圆 75");
        assert!((p.rho() - 0.76).abs() < 1e-9, "齿根圆角 0.38m = 0.76");
        assert!((p.chamfer() - 1.0).abs() < 1e-9, "倒角 round(0.6×2) = 1");
        assert!((p.pitch_angle().to_degrees() - 9.0).abs() < 1e-9, "齿距角 9°");
    }

    #[test]
    fn chamfer_rounding_matches_rule() {
        // round(0.6m)：m=1 → 1；m=0.5 → 0（不倒角）；m=5 → 3
        for (m, want) in [(1.0, 1.0), (0.5, 0.0), (5.0, 3.0), (2.0, 1.0), (3.0, 2.0)] {
            let p = GearParams { m, z: 20, h: 20.0, ..GearParams::default() };
            assert!(
                (p.chamfer() - want).abs() < 1e-9,
                "m={} → C={} 应为 {}",
                m,
                p.chamfer(),
                want
            );
        }
    }

    #[test]
    fn flank_fit_points_match_template() {
        // 模板 5 个拟合点半径：38.1327 / 38.6908 / 39.7939 / 40.8970 / 42.0001
        // （第 1 个是齿根圆角切点，与 rb+(ra−rb)/8 差 ~7µm；其余 4 个吻合到 5e-4）
        let p = tmpl();
        let f = make_flank(&p, (p.pitch_angle() / 2.0), 1.0).unwrap();
        let want = [38.1327, 38.6908, 39.7939, 40.8969, 42.0];
        let tol = [1e-2, 2e-3, 2e-3, 2e-3, 2e-3];
        for (i, (q, (w, tl))) in f.fit.iter().zip(want.iter().zip(tol.iter())).enumerate() {
            let r = (q[0] * q[0] + q[1] * q[1]).sqrt();
            assert!((r - w).abs() < *tl, "拟合点{} 半径 {:.4} vs 模板 {:.4}", i, r, w);
        }
        assert_eq!(f.ctrl.len(), 7, "7 个控制点（精度硬约束）");
        assert_eq!(f.knots.len(), 11, "11 个节点");
    }

    #[test]
    fn flank_curve_matches_involute_and_template() {
        // 用生成的 knots/control_points 采样，与解析渐开线比：应 ≤ 3µm（模板自身约 10µm）
        let p = tmpl();
        let c = p.pitch_angle() / 2.0;
        let f = make_flank(&p, c, 1.0).unwrap();
        let (t0, t1) = (f.knots[3], f.knots[7]);
        let mut worst = 0.0f64;
        for i in 0..=60 {
            let u = t0 + (t1 - t0) * i as f64 / 60.0;
            let mut q = [0.0f64; 2];
            for (j, d) in f.ctrl.iter().enumerate() {
                let w = basis(j, 3, u, &f.knots);
                q[0] += w * d[0];
                q[1] += w * d[1];
            }
            let r = (q[0] * q[0] + q[1] * q[1]).sqrt();
            let th = q[1].atan2(q[0]);
            let want = c + p.half_tooth_angle(r);
            let dev = r * (th - want).abs();
            if dev > worst {
                worst = dev;
            }
        }
        assert!(
            worst < 1e-2,
            "齿廓与精确渐开线最大偏差 {:.5} mm（模板自身 ~0.009，要求 <0.01）",
            worst
        );
    }

    #[test]
    fn fillet_is_tangent_to_root_and_touches_flank() {
        // 模板实测：圆心半径 = rf+ρ = 38.2600；圆角终点 = 齿廓起点
        let p = tmpl();
        let f = make_flank(&p, p.pitch_angle() / 2.0, 1.0).unwrap();
        let ctr = (f.fillet_c[0] * f.fillet_c[0] + f.fillet_c[1] * f.fillet_c[1]).sqrt();
        let root = p.df() / 2.0;
        assert!((ctr - (root + p.rho())).abs() < 1e-9, "圆心半径 {:.6}", ctr);
        let d = ((f.start[0] - f.fillet_c[0]).powi(2) + (f.start[1] - f.fillet_c[1]).powi(2)).sqrt();
        assert!((d - p.rho()).abs() < 1e-9, "圆心到齿廓起点 = ρ（{:.6}）", d);
        let rr = (f.root_pt[0] * f.root_pt[0] + f.root_pt[1] * f.root_pt[1]).sqrt();
        assert!((rr - root).abs() < 1e-9, "切点在齿根圆上 {:.6}", rr);
        // 模板圆角圆心角 8.622°（齿中心 4.5°）
        let deg = f.fillet_c[1].atan2(f.fillet_c[0]).to_degrees();
        assert!((deg - 8.622).abs() < 0.02, "圆角圆心角 {:.4}° vs 模板 8.622°", deg);
    }

    #[test]
    fn front_view_entity_count_matches_template() {
        // 模板 40 齿 = 320 个实体（每齿 2 样条 + 2 半齿顶弧 + 2 圆角 + 2 半齿根弧）
        let p = tmpl();
        let v = front_regular(&p, 1.0).unwrap();
        let main = v.iter().filter(|e| layer_of(e) == LAYER_MAIN).count();
        assert_eq!(main, 320, "轮廓层实体数（模板 320）");
        assert_eq!(v.iter().filter(|e| layer_of(e) == LAYER_CENTER).count(), 3, "分度圆 + 十字");
    }

    #[test]
    fn simplified_view_has_three_circles_on_expected_layers() {
        let p = tmpl();
        let v = front_simplified(&p, 1.0).unwrap();
        let lay = |layer: &str| {
            v.iter()
                .filter_map(|e| match e {
                    EntityType::Circle(c) if c.common.layer == layer => Some(c.radius),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(lay(LAYER_MAIN), vec![42.0], "齿顶圆在轮廓层");
        assert_eq!(lay(LAYER_CENTER), vec![40.0], "分度圆在中心线层");
        assert_eq!(lay(LAYER_THIN), vec![37.5], "齿根圆在细线层");
    }

    #[test]
    fn section_view_has_two_hatch_loops_and_root_lines() {
        let p = tmpl();
        let v = section_view(&p, 1.0).unwrap();
        // 剖视图轮廓 = 8 条（端面2 + 齿顶面2 + 倒角4；**无**倒角台阶线，模板同构）
        // + 齿根线 2 条 = 10 条落在轮廓层
        assert_eq!(
            v.iter().filter(|e| layer_of(e) == LAYER_MAIN).count(),
            10,
            "8 条轮廓 + 2 条齿根线"
        );
        let hat = v
            .iter()
            .find_map(|e| match e {
                EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .expect("有剖面线");
        assert_eq!(hat.paths.len(), 2, "轴线上下一共两个边界环（模板同构）");
        assert_eq!(hat.common.layer, LAYER_HATCH);
        assert!((hat.pattern_scale - 1.0).abs() < 1e-9);
        assert_eq!(hat.pattern.name, "ANSI31");
        // 齿根线 ±37.5、长度 = h
        let root_lines = v
            .iter()
            .filter_map(|e| match e {
                EntityType::Line(l) if l.common.layer == LAYER_MAIN => Some(l),
                _ => None,
            })
            .filter(|l| (l.start.y.abs() - 37.5).abs() < 1e-9 && (l.end.y.abs() - 37.5).abs() < 1e-9)
            .count();
        assert_eq!(root_lines, 2, "上下齿根线");
    }

    #[test]
    fn side_view_chamfer_geometry_matches_template() {
        // 模板：端面线 ±(ra−C)=±41 高、台阶线 ±ra=±42 高、齿顶面长 h−2C = 18
        let p = tmpl();
        let v = side_view(&p, 1.0).unwrap();
        let lines: Vec<_> = v
            .iter()
            .filter_map(|e| match e {
                EntityType::Line(l) if l.common.layer == LAYER_MAIN => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 10, "轮廓 10 条线（模板侧视图同构）");
        let tips = lines
            .iter()
            .filter(|l| (l.start.y - l.end.y).abs() < 1e-9 && (l.start.y.abs() - 42.0).abs() < 1e-9)
            .count();
        assert_eq!(tips, 2, "上下齿顶面");
        assert!(
            lines.iter().any(|l| (l.start.x + 10.0).abs() < 1e-9
                && (l.start.y + 41.0).abs() < 1e-9
                && (l.end.y - 41.0).abs() < 1e-9),
            "左端面线从 −41 到 +41"
        );
    }

    #[test]
    fn helix_lines_only_for_helical_and_follow_hand_rule() {
        let base = tmpl();
        assert_eq!(
            side_view(&base, 1.0)
                .unwrap()
                .iter()
                .filter(|e| layer_of(e) == LAYER_THIN)
                .count(),
            0,
            "直齿轮侧视图没有细实线"
        );
        let right = GearParams { beta_deg: 8.0, ..base.clone() };
        let v = side_view(&right, 2.0).unwrap();
        let thin: Vec<_> = v.iter().filter(|e| layer_of(e) == LAYER_THIN).collect();
        assert_eq!(thin.len(), 3, "斜齿轮三条细实线");
        // 右旋 → "/"：方向角 +82°（= 90−8）
        if let EntityType::Line(l) = thin[1] {
            let d = (l.end.y - l.start.y).atan2(l.end.x - l.start.x).to_degrees();
            assert!(d > 0.0 && (d - 82.0).abs() < 1e-6, "右旋方向角 {:.3}°", d);
        }
        // 左旋 → "\"：方向角 −82°
        let left = GearParams { beta_deg: -8.0, ..base.clone() };
        let v2 = side_view(&left, 2.0).unwrap();
        if let Some(EntityType::Line(l)) = v2.iter().find(|e| layer_of(e) == LAYER_THIN) {
            let d = (l.end.y - l.start.y).atan2(l.end.x - l.start.x).to_degrees();
            assert!(d < 0.0 && (d + 82.0).abs() < 1e-6, "左旋方向角 {:.3}°", d);
        }
        // 间距 = 5×k（k=2 → 10）
        let mid = thin[1];
        let up = thin[2];
        let (m1, m2) = (mid_line_offset(&mid), mid_line_offset(&up));
        assert!((m2 - m1 - 10.0).abs() < 1e-6, "间距 {:.3}（应 5×2=10）", m2 - m1);
    }

    fn mid_line_offset(e: &EntityType) -> f64 {
        if let EntityType::Line(l) = e {
            let d = l.end.x - l.start.x;
            let n = (l.end.y - l.start.y).atan2(d);
            // 沿线方向取投影（offset 由法向给出，这里用 mid 点的法向距离）
            let mx = (l.start.x + l.end.x) / 2.0;
            let my = (l.start.y + l.end.y) / 2.0;
            -mx * n.sin() + my * n.cos()
        } else {
            0.0
        }
    }

    #[test]
    fn centerlines_use_6n_overhang() {
        let p = tmpl();
        let v = front_simplified(&p, 2.0).unwrap();
        let ln = v
            .iter()
            .find_map(|e| match e {
                EntityType::Line(l) => Some(l),
                _ => None,
            })
            .unwrap();
        let len = ((ln.end.x - ln.start.x).powi(2) + (ln.end.y - ln.start.y).powi(2)).sqrt();
        assert!((len - (84.0 + 6.0 * 2.0)).abs() < 1e-9, "长度 = da + 6n = {}", len);
    }

    #[test]
    fn fillet_arc_span_matches_template() {
        // 模板圆角弧张角 79.79°；本实现取精确渐开线齿廓起点（比模板的生成器准 ~7µm），
        // 圆心角相应差 ~0.5°（换算到弧长 <8µm，不可见）
        let p = tmpl();
        let f = make_flank(&p, p.pitch_angle() / 2.0, 1.0).unwrap();
        let span = (f.fillet_a1 - f.fillet_a0).rem_euclid(360.0);
        assert!(span < 180.0, "圆角弧必须取短边（{:.3}°）", span);
        assert!((span - 79.79).abs() < 0.6, "圆角弧张角 {:.3}° vs 模板 79.79°", span);
    }

    /// 把四个视图导出成 CSV（世界坐标），供人工/脚本与用户模板 `齿轮画法.dxf` 叠加比对。
    ///
    /// 跑法：`cargo test -p ocs_ocsm --lib gear::tests::dump_views_csv -- --ignored --nocapture`
    /// 输出：`/tmp/gear_review/<view>.csv`（每行 `TYPE,layer,...`；样条已按真实曲线采样）。
    #[test]
    #[ignore]
    fn dump_views_csv() {
        let p = tmpl();
        // 关键数字对账（与模板实测值同列）
        let ra = p.da() / 2.0;
        println!(
            "da={:.4} df={:.4} rho={:.4} C={:.3} ψ(ra)={:.4}° ψ(r_start)={:.4}° r_start={:.4}",
            p.da(), p.df(), p.rho(), p.chamfer(),
            p.half_tooth_angle(ra).to_degrees(),
            p.half_tooth_angle(fr_start_radius(&p)).to_degrees(),
            fr_start_radius(&p)
        );
        println!("（模板：ψ(ra)=1.0371°  r_start=38.1327  ψ(r_start)=3.0019°）");
        let dir = std::path::Path::new("/tmp/gear_review");
        std::fs::create_dir_all(dir).unwrap();
        for view in GearView::ALL {
            let part = generate(&p, view, 1.0).unwrap();
            let mut out = String::new();
            for e in &part.entities {
                match e {
                    EntityType::Line(l) => out.push_str(&format!(
                        "LINE,{},{:.6},{:.6},{:.6},{:.6}\n",
                        l.common.layer, l.start.x, l.start.y, l.end.x, l.end.y
                    )),
                    EntityType::Circle(c) => out.push_str(&format!(
                        "CIRCLE,{},{:.6},{:.6},{:.6}\n",
                        c.common.layer, c.center.x, c.center.y, c.radius
                    )),
                    EntityType::Arc(a) => out.push_str(&format!(
                        "ARC,{},{:.6},{:.6},{:.6},{:.4},{:.4}\n",
                        a.common.layer,
                        a.center.x,
                        a.center.y,
                        a.radius,
                        a.start_angle.to_degrees(),
                        a.end_angle.to_degrees()
                    )),
                    EntityType::Spline(sp) => {
                        // 按真实曲线采样（不是控制多边形！）
                        let (t0, t1) = (sp.knots[3], sp.knots[sp.knots.len() - 4]);
                        out.push_str(&format!("POLY,{},", sp.common.layer));
                        for i in 0..=40 {
                            let u = t0 + (t1 - t0) * i as f64 / 40.0;
                            let mut q = [0.0f64; 2];
                            for (j, d) in sp.control_points.iter().enumerate() {
                                let w = basis(j, 3, u, &sp.knots);
                                q[0] += w * d.x;
                                q[1] += w * d.y;
                            }
                            out.push_str(&format!("({:.6},{:.6})", q[0], q[1]));
                        }
                        out.push('\n');
                    }
                    EntityType::Hatch(hh) => {
                        out.push_str(&format!("HATCH,{},{}\n", hh.common.layer, hh.paths.len()));
                        for (pi, path) in hh.paths.iter().enumerate() {
                            out.push_str(&format!("HLOOP,{},,{},", hh.common.layer, pi));
                            for edge in &path.edges {
                                if let BoundaryEdge::Line(le) = edge {
                                    out.push_str(&format!(
                                        "({:.6},{:.6})({:.6},{:.6})",
                                        le.start.x, le.start.y, le.end.x, le.end.y
                                    ));
                                }
                            }
                            out.push('\n');
                        }
                    }
                    _ => {}
                }
            }
            let path = dir.join(format!("{}.csv", view.key()));
            std::fs::write(&path, out).unwrap();
            println!("已导出 {}（{} 实体）", path.display(), part.entities.len());
        }
    }

    #[test]
    fn ocsm_ready_gate_blocks_uninitialized_drawing() {
        let doc = ocs_plugin_api::host::acadrust::CadDocument::new();
        let err = ocsm_ready(&doc).unwrap_err();
        assert!(err.contains("OCSM 初始化"), "{err}");
        assert!(err.contains("缺图层"), "应报缺哪些层：{err}");
        // 只把图层建上、但中心线层没挂点划线 → 仍拦
        let mut doc2 = ocs_plugin_api::host::acadrust::CadDocument::new();
        for name in [LAYER_MAIN, LAYER_THIN, LAYER_CENTER, LAYER_HATCH] {
            let mut ly = ocs_plugin_api::host::acadrust::tables::Layer::new(name);
            ly.line_type = "Continuous".to_string();
            doc2.layers.add_or_replace(ly);
        }
        let err2 = ocsm_ready(&doc2).unwrap_err();
        assert!(err2.contains("CENTER2"), "{err2}");
        // 挂上 CENTER2 → 放行
        let mut ly = ocs_plugin_api::host::acadrust::tables::Layer::new(LAYER_CENTER);
        ly.line_type = "CENTER2".to_string();
        doc2.layers.add_or_replace(ly);
        ocsm_ready(&doc2).unwrap();
    }

    #[test]
    fn view_parse_accepts_chinese_and_english() {
        assert_eq!(GearView::parse("剖视图").unwrap(), GearView::Section);
        assert_eq!(GearView::parse("section").unwrap(), GearView::Section);
        assert_eq!(GearView::parse("侧视图").unwrap(), GearView::Side);
        assert_eq!(GearView::parse("side").unwrap(), GearView::Side);
        assert_eq!(GearView::parse("简化").unwrap(), GearView::Simplified);
        assert_eq!(GearView::parse("simplified").unwrap(), GearView::Simplified);
        assert_eq!(GearView::parse("常规正视图").unwrap(), GearView::Front);
        assert_eq!(GearView::parse("front").unwrap(), GearView::Front);
        assert!(GearView::parse("斜视").is_err());
    }

    #[test]
    fn parse_request_positional_and_keyword_forms() {
        // 位置式：`OCSMGEAR 2 40 20 剖视图`
        let r = parse_request("2 40 20 剖视图").unwrap();
        assert_eq!(r.params.m, 2.0);
        assert_eq!(r.params.z, 40);
        assert_eq!(r.params.h, 20.0);
        assert_eq!(r.view, GearView::Section);
        // 关键字式 + at + rot
        let r = parse_request("m=2 z=40 h=20 beta=-8 x=0.2 view front at 120,45 rot 30").unwrap();
        assert!((r.params.beta_deg + 8.0).abs() < 1e-9);
        assert!((r.params.x - 0.2).abs() < 1e-9);
        assert_eq!(r.view, GearView::Front);
        assert_eq!(r.at, Some([120.0, 45.0]));
        assert!((r.rotation - 30.0).abs() < 1e-9);
        // `view 侧视图`（空格分隔）
        let r = parse_request("2 40 20 view 侧视图").unwrap();
        assert_eq!(r.view, GearView::Side);
        // 缺参数 → 报用法
        assert!(parse_request("2").is_err(), "只给模数要报用法");
        assert!(parse_request("2 40 20 斜视").is_err(), "错视图名要报");
    }

    #[test]
    fn block_name_is_sanitized_and_stable() {
        let p = tmpl();
        assert_eq!(block_name(&p, GearView::Front), "OCSM_GEAR_M2_Z40_H20_FRONT");
        let h = GearParams { beta_deg: -8.5, x: 0.3, ..p };
        let nm = block_name(&h, GearView::Side);
        assert_eq!(nm, "OCSM_GEAR_M2_Z40_B8_5L_X0_3_H20_SIDE", "{}", nm);
        let h2 = GearParams { beta_deg: 8.0, x: -0.25, ..p };
        assert_eq!(
            block_name(&h2, GearView::Front),
            "OCSM_GEAR_M2_Z40_B8R_XN0_25_H20_FRONT"
        );
        assert!(nm.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    }

    #[test]
    fn generate_all_views_and_svg_preview() {
        let p = tmpl();
        for v in GearView::ALL {
            let part = generate(&p, v, 1.0).unwrap();
            assert!(!part.entities.is_empty(), "{} 非空", v.label());
            assert!(part.bbox[2] > part.bbox[0]);
        }
        let svg = preview_svg("m=2&z=40&h=20&view=front").unwrap();
        assert!(svg.starts_with("<svg") && svg.contains("path"));
        let svg2 = preview_svg("m=2&z=40&h=20&view=side&beta=8").unwrap();
        assert!(svg2.contains("line"), "斜齿轮侧视图有三条细实线");
    }

    #[test]
    fn helical_uses_transverse_params() {
        let p = GearParams { beta_deg: 20.0, ..tmpl() };
        assert!((p.mt() - 2.0 / 20f64.to_radians().cos()).abs() < 1e-9);
        assert!(p.alpha_t().to_degrees() > 20.0, "端面压力角大于法向");
        assert!(p.d() > 80.0, "斜齿轮分度圆更大（mt·z）");
        p.validate().unwrap();
        let _ = p.notes(); // 斜齿轮提示
    }

    #[test]
    fn validate_rejects_bad_params() {
        assert!(GearParams { m: 0.0, ..GearParams::default() }.validate().is_err());
        assert!(GearParams { z: 1, ..GearParams::default() }.validate().is_err());
        assert!(GearParams { h: -1.0, ..GearParams::default() }.validate().is_err());
        assert!(GearParams { beta_deg: 60.0, ..GearParams::default() }.validate().is_err());
        // 倒角比厚度还大 → 拒绝
        assert!(GearParams { m: 10.0, h: 6.0, ..GearParams::default() }.validate().is_err());
    }

    #[test]
    fn solid_weight_is_plausible() {
        // m=2 z=40 h=20 实心钢件：面积 ≈ π·ra² − 齿槽 ≈ 4920 mm² → ≈ 0.77 kg
        let p = tmpl();
        let w: f64 = solid_weight_kg(&p).parse().unwrap();
        assert!(w > 0.70 && w < 0.90, "毛坯质量 {:.3} kg 不合理（期望 0.77 左右）", w);
    }

    fn layer_of(e: &EntityType) -> &str {
        match e {
            EntityType::Line(l) => &l.common.layer,
            EntityType::Circle(c) => &c.common.layer,
            EntityType::Arc(a) => &a.common.layer,
            EntityType::Spline(s) => &s.common.layer,
            EntityType::Hatch(h) => &h.common.layer,
            _ => "",
        }
    }
}
