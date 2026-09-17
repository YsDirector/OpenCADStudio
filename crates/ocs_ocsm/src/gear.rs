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

/// 齿轮种类（外齿轮 = 一期；内齿轮 = 二期，用户 2026-09-17 给模板）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GearKind {
    /// 外齿轮：齿顶圆在外、齿根圆在内（da > d > df）
    External,
    /// 内齿轮（齿圈）：齿顶圆在**内**、齿根圆在**外**（da < d < df），齿朝圆心长
    Internal,
}

impl GearKind {
    pub fn is_internal(self) -> bool {
        matches!(self, GearKind::Internal)
    }
    pub fn label(self) -> &'static str {
        match self {
            GearKind::External => "外齿轮",
            GearKind::Internal => "内齿轮",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            GearKind::External => "external",
            GearKind::Internal => "internal",
        }
    }
    /// 解析种类名（命令行/HTTP 共用）。
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "int" | "internal" | "ring" | "内" | "内齿" | "内齿轮" | "齿圈" => {
                Some(GearKind::Internal)
            }
            "ext" | "external" | "外" | "外齿" | "外齿轮" => Some(GearKind::External),
            _ => None,
        }
    }
}

/// 齿轮几何参数（GUI / 命令行同一套）。
#[derive(Debug, Clone, PartialEq)]
pub struct GearParams {
    /// 齿轮种类（外/内）
    pub kind: GearKind,
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
        // 与用户模板/DXF 完全一致的示例件（外齿轮 齿轮画法.dxf：m=2 z=40 h=20）
        Self {
            kind: GearKind::External,
            m: 2.0,
            z: 40,
            ha: 1.0,
            c: 0.25,
            beta_deg: 0.0,
            h: 20.0,
            x: 0.0,
        }
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
    ///
    /// * 外齿轮：`d + 2ha`（在最外）
    /// * 内齿轮：`d − 2ha`（在**最内** —— 齿朝圆心长，反解自模板：m2 z40 时 da=76）
    pub fn da(&self) -> f64 {
        if self.kind.is_internal() {
            (self.d() - 2.0 * self.ha_height()).max(0.0)
        } else {
            self.d() + 2.0 * self.ha_height()
        }
    }
    /// 齿根圆直径 df。
    ///
    /// * 外齿轮：`d − 2hf`（在内）
    /// * 内齿轮：`d + 2hf`（在**外** —— 模板实测 85 = 80 + 2×1.25×2）
    pub fn df(&self) -> f64 {
        if self.kind.is_internal() {
            self.d() + 2.0 * self.hf_height()
        } else {
            (self.d() - 2.0 * self.hf_height()).max(0.0)
        }
    }
    /// 齿根过渡圆角的**圆心所在半径**。
    ///
    /// * 外齿轮：`rf + ρ`（圆心在齿根圆外，模板实测 38.26 = 37.5 + 0.76）
    /// * 内齿轮：`rf − ρ`（圆心在齿根圆**内**，模板实测 41.74 = 42.5 − 0.76）
    pub fn fillet_center_radius(&self) -> f64 {
        let rf = self.df() / 2.0;
        if self.kind.is_internal() {
            rf - self.rho()
        } else {
            rf + self.rho()
        }
    }
    /// 内齿轮的**齿槽**中心线到该半径处齿廓的夹角（弧度）。
    ///
    /// 关键结论（用户内齿轮模板反解 + 逐点核对）：**内齿轮的齿槽形状 = 同参数外齿轮的齿形** ——
    /// 同一条渐开线、同一个 `ψ(R) = st/(2r) + (inv αt − inv αR)` 公式，
    /// 只是这个角要从**齿槽中心线**量（外齿轮从**齿中心线**量）。
    /// 模板核对：m2 z40 时 R=42.076 处 ψ=0.9869°（理论 0.987°）
    pub fn space_half_angle(&self, radius: f64) -> f64 {
        self.half_tooth_angle(radius)
    }
    /// 内齿轮简化的**齿顶**半角（齿中心线到齿顶弧末端，弧度）。
    ///
    /// 齿槽半角 ψ 大于齿距半角时（齿顶圆低于基圆，z ≲ 33 的常见内齿轮），
    /// 齿廓到基圆就没了、用径向直线收到齿顶圆：此时弧半角按**基圆**处的 ψ 取。
    pub fn internal_tip_half_angle(&self) -> f64 {
        let ra = self.da() / 2.0;
        let rb = self.db() / 2.0;
        let pitch_half = self.pitch_angle() / 2.0;
        (pitch_half - self.space_half_angle(ra.max(rb))).max(1e-6)
    }
    /// 内齿轮的齿廓/直线**有效起点半径**（渐开线能画到哪里）。
    pub fn internal_flank_r_min(&self) -> f64 {
        let ra = self.da() / 2.0;
        let rb = self.db() / 2.0;
        if ra >= rb {
            ra
        } else {
            // 齿顶圆低于基圆：用基圆略上方起画（避开渐开线在基圆处的曲率奇异，同 fcgear 的 fs=0.01）
            rb + 0.02 * (self.df() / 2.0 - rb)
        }
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
    /// 齿厚半角 ψ(R)（弧度）：齿中心线（内齿轮时是**齿槽中心线**）到该半径处齿廓的夹角。
    ///
    /// `ψ(R) = st/(2r) + (inv αt − inv αR)`，`αR = acos(rb/R)`；`R < rb` 时按 rb 夹住
    /// （渐开线不存在于基圆以内）。
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
        if self.kind.is_internal() {
            if self.da() <= 1e-6 {
                return Err("内齿轮齿顶圆直径非正（齿朝圆心长太长）：检查 m/z/ha*/Xn 组合。".into());
            }
            if self.da() / 2.0 >= self.df() / 2.0 {
                return Err("内齿轮齿顶圆不小于齿根圆：检查 m/z/ha*/c* 组合。".into());
            }
        } else if self.df() <= 1e-6 {
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
        if let Some(w) = self.root_style_note() {
            v.push(w);
        }
        if self.kind.is_internal() && self.z >= 4 {
            // 内齿轮的圆角是“解”出来的（不是模板那套先定起点的构造），解不出来才提示。
            // 注意：不能把外齿轮的 root_style_note 用在内齿轮上 ——
            // 外齿轮口径是“圆心在 rf+ρ”，内齿轮是“rf−ρ”，直接套会**假警报**（已踩）。
            let s0 = self.pitch_angle() / 2.0;
            let ok = solve_internal_fillet(self, s0, 1.0).is_some()
                && solve_internal_fillet(self, s0, -1.0).is_some();
            if !ok {
                v.push(format!(
                    "内齿轮齿根圆角无解（z={}、m={}）：rf−ρ={:.3} 与齿廓之间放不下 ρ={:.3} 的圆角，\
                     已按无圆角画（齿廓末端径向直线落到齿根圆）。",
                    self.z,
                    trim(self.m),
                    self.fillet_center_radius(),
                    self.rho()
                ));
            }
        }
        if self.kind.is_internal() {
            v.push(
                "内齿轮剖视图按模板只画到**齿根圆**，不打剖面线 —— 齿圈外壁结构（轮缘/腹板/键槽等）".to_string()
                    + "由用户/AI 按实际结构延伸，这样一张图能服务不同齿圈。延伸画法：从齿根线往外加厚齿圈"
                    + "（轴向宽度保持 h），新轮廓落 1轮廓实线层、剖面线用 ANSI31 放 5剖面线层"
                    + "（一个 HATCH 两个环，轴线上下各一环）。",
            );
        }
        if self.internal_tip_falls_below_base() {
            v.push(format!(
                "内齿轮 z={} 时齿顶圆 da={:.3} 低于基圆 db={:.3}：真实齿顶由插齿刀刀尖包络成形（非渐开线），\
                 本图按简化画法画「渐开线到基圆 → 径向直线到齿顶圆」。要精确齿顶画法请给模板。",
                self.z,
                self.da(),
                self.db()
            ));
        }
        if self.is_helical() && self.kind.is_internal() {
            v.push(
                "内齿轮斜齿：剖视图按轴向剖面画，未画三条螺旋线细实线（外齿轮模板侧视图才有，内齿轮模板没有侧视图）。"
                    .to_string(),
            );
        }
        v
    }
    /// 内齿轮：渐开线能不能画到齿顶圆（齿顶圆是不是低于基圆）。
    ///
    /// 内齿轮齿顶圆 `da = d − 2m`，而基圆 `db = d·cos20°` —— `da < db` ⇔ `z < 2/(1−cos20°) ≈ 33.2`，
    /// 也就是说 **z ≤ 33 的内齿轮都是“齿顶圆低于基圆”**（内齿轮常用 z=24…40，正好落在两侧）。
    /// 此时渐开线下不去，只能到基圆，再按径向直线收到齿顶圆（简化画法，会有提示）。
    pub fn internal_tip_falls_below_base(&self) -> bool {
        self.kind.is_internal() && self.da() / 2.0 < self.db() / 2.0 - 1e-9
    }

    /// 齿根那一段用哪种画法（模板口径能不能解出来）。
    ///
    /// **只针对外齿轮**（内齿轮走 `solve_internal_fillet`，它的圆心在 rf−ρ、切点由解唯一确定，
    /// 套用本函数会得出相反的结论 —— 已踩：内齿轮模板件会被报成"圆角无解"）。
    ///
    /// 模板（z=40）的构造要求“圆角圆心在 rf+ρ、且到齿廓起点距离 = ρ”，
    /// 这等价于三角形两边 `r_c = rf+ρ`、`r_s = 齿廓起点半径`、夹角未知——
    /// **可解条件 = |r_c − r_s| ≤ ρ**。齿数小的时候 r_s 比 r_c 高出超过 ρ，就无解了
    /// （用户 2026-09-17 实测 z=17 渲染异常）。
    pub fn root_style(&self) -> RootStyle {
        let (rb, ra, rf) = (self.db() / 2.0, self.da() / 2.0, self.df() / 2.0);
        let rho = self.rho();
        let r_c = rf + rho;
        let r_s = rb + (ra - rb) * FIT_FRACTIONS[0];
        if (r_c - r_s).abs() <= rho {
            return RootStyle::TemplateArc;
        }
        // 降级①：渐开线只到基圆，基圆以下留一段径向直线（要求基圆高于圆角圆心）
        if rb > r_c + 1e-9 && (rb - rf) > rho {
            return RootStyle::BaseCircleLine;
        }
        // 降级②
        RootStyle::NoFillet
    }

    /// 齿根降级的提示（要显示给人看；模板口径时返回 None）。**内齿轮不走这条路**。
    pub fn root_style_note(&self) -> Option<String> {
        if self.kind.is_internal() {
            return None;
        }
        let style = self.root_style();
        if style == RootStyle::TemplateArc {
            return None;
        }
        let r_c = self.df() / 2.0 + self.rho();
        let r_s = self.db() / 2.0 + (self.da() / 2.0 - self.db() / 2.0) * FIT_FRACTIONS[0];
        Some(format!(
            "齿根圆角无解：z={}、m={} 时齿廓起点半径 {:.3} 与圆角圆心半径 {:.3} 相差 {:.3} > 圆角半径 ρ={:.3}，\
             模板口径的 0.38m 圆角放不下。已按{}出图（想看真实根切曲线请用变位，或按 GB 允许的轻微根切另画）。",
            self.z,
            trim(self.m),
            r_s.abs(),
            r_c.abs(),
            (r_s - r_c).abs(),
            self.rho(),
            style.label()
        ))
    }

    /// 规格文本（块名/明细表用）。
    pub fn spec(&self) -> String {
        let mut s = String::new();
        if self.kind.is_internal() {
            s.push_str("内齿轮 ");
        }
        s.push_str(&format!("m{} z{} h{}", trim(self.m), self.z, trim(self.h)));
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

    /// 该视图在这个种类的齿轮下能不能生成。
    ///
    /// 内齿轮模板（`内齿轮.dxf`）只给了 **剖视图 + 端视图** 两个视图 ——
    /// 按项目规矩「模板没有的画法不去猜」，其余两个视图在不给模板前不画。
    pub fn available_for(self, kind: GearKind) -> bool {
        match kind {
            GearKind::External => true,
            GearKind::Internal => matches!(self, GearView::Section | GearView::Front),
        }
    }

    /// 人看的名字（内齿轮下 End 视图叫"端视图"更准确）
    pub fn label_for(self, kind: GearKind) -> &'static str {
        match (kind, self) {
            (GearKind::Internal, GearView::Section) => "剖视图（齿圈内齿不剖）",
            (GearKind::Internal, GearView::Front) => "端视图",
            _ => self.label(),
        }
    }

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
            "front" | "regular" | "正视" | "正视图" | "常规" | "常规正视" | "常规正视图" | "端视"
            | "端视图" => Some(GearView::Front),
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

/// 齿根那一段怎么画（模板口径 / 小齿数降级）。
///
/// 模板（z=40）的“圆角圆与齿根圆相切、且过齿廓起点”在齿数小时**无解**：
/// 齿廓起点半径与圆角圆心半径差超过 ρ 时三角形不成立。用户 2026-09-17 定案：
/// **别出乱图** —— 给警告 + 降级。降级分两级，都有警告、都写进 `notes()`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootStyle {
    /// 模板口径：0.38m 圆角切齿根圆、且过齿廓起点（大齿数，模板 z=40 就是这样）
    TemplateArc,
    /// 降级①（FreeCAD `fcgear/involute.py` 的做法，用户指定参考）：
    /// 渐开线只到**基圆**，基圆以下用**指向圆心的直线**接到圆角弧顶（圆角保留）
    BaseCircleLine,
    /// 降级②：连圆角都放不下 → **圆角 = 0**（直线直接落到齿根圆）
    NoFillet,
}

impl RootStyle {
    /// 人看的名字（警告/手册用）。
    pub fn label(self) -> &'static str {
        match self {
            RootStyle::TemplateArc => "齿根圆角（模板口径）",
            RootStyle::BaseCircleLine => "基圆以下直线 + 圆角（FreeCAD 口径降级）",
            RootStyle::NoFillet => "无齿根圆角（直线到齿根圆）",
        }
    }
}

/// 齿底那一段（含圆角）的构造结果 —— 局部坐标，齿中心线为 +x 轴。
#[derive(Debug, Clone)]
struct Flank {
    /// 渐开线在 5 个拟合点上的点
    fit: [[f64; 2]; 5],
    /// 7 个 B 样条控制点
    ctrl: Vec<[f64; 2]>,
    /// 11 个节点（弦长参数化，clamped）
    knots: Vec<f64>,
    /// 齿廓起点（= 样条近似曲线的最低端）
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
    /// 采用哪种齿根画法
    style: RootStyle,
    /// 降级时：从基圆（或渐开线起点）到圆角弧顶的那段**径向直线**的两端
    radial: Option<([f64; 2], [f64; 2])>,
}

/// 渐开线上一点：齿中心线角 `c`（弧度），`sign`=+1 取齿廓一侧（逆时针那侧）。
fn involute_point(p: &GearParams, radius: f64, c: f64, sign: f64) -> [f64; 2] {
    let th = c + sign * p.half_tooth_angle(radius);
    [radius * th.cos(), radius * th.sin()]
}

/// B 样条基函数 N_{i,p}(u)（Cox–de Boor，clamped 节点向量）。
fn basis(i: usize, p: usize, u: f64, knots: &[f64]) -> f64 {
    // 右端点 u = u_max 取极限：只有**最后一个控制点**对应的基函数为 1
    // （clamped 节点向量的性质；不特判的话求和会得到 0，采样点会掉到原点）。
    let last = knots.len() - 1;
    if i + p + 1 == last && (u - knots[last]).abs() < 1e-12 {
        return 1.0;
    }
    if p == 0 {
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
    let ra = p.da() / 2.0;
    let rf = p.df() / 2.0;
    let rb = p.db() / 2.0;
    let rho = p.rho();
    let r_c = rf + rho; // 圆角圆心半径（与齿根圆相切）
    let style = p.root_style();

    // ① 5 个拟合点。模板口径：渐开线上 r = rb + (ra−rb)·{1/8,¼,½,¾,1}（反解自模板）。
    //    降级口径：齿廓只到**基圆**（下面用直线接），所以从 rb 起取 {0,¼,½,¾,1}。
    //    降级口径：齿廓只到**基圆略上方**（避开渐开线在基圆处的曲率奇异 —— 与 FreeCAD
    //    `fcgear` 的 fs=0.01 同一手法：起点正好落在基圆上时，插值样条会在起点附近“勾”回去）。
    let fracs: [f64; 5] = match style {
        RootStyle::TemplateArc => FIT_FRACTIONS,
        _ => [0.02, 0.25, 0.5, 0.75, 1.0],
    };
    let mut fit = [[0.0f64; 2]; 5];
    for (i, f) in fracs.iter().enumerate() {
        let radius = rb + (ra - rb) * f;
        fit[i] = involute_point(p, radius, c, sign);
    }
    // ② 弦长参数化 → clamped 节点（**11 个** = 7 控制点 + 3 次 + 1，与模板同构）+ ③ 控制点
    let (knots, ctrl) = fit_cubic_bspline_knots(&fit)?;

    // ④ 齿根那一段（三种画法，见 `RootStyle`）
    let start = fit[0];
    let th_start = start[1].atan2(start[0]);
    let mut radial: Option<([f64; 2], [f64; 2])> = None;
    let mut arc_top = start; // 圆角弧的另一端（降级时是弧顶）
    let (fillet_c, root_pt) = match style {
        RootStyle::TemplateArc => {
            // 圆心在 r = rf+ρ 上、且 |圆心 − 齿廓起点| = ρ（余弦定理）
            let rs = (start[0] * start[0] + start[1] * start[1]).sqrt();
            let cosd = ((r_c * r_c + rs * rs - rho * rho) / (2.0 * r_c * rs)).clamp(-1.0, 1.0);
            let th_c = th_start + sign * cosd.acos();
            (
                [r_c * th_c.cos(), r_c * th_c.sin()],
                [rf * th_c.cos(), rf * th_c.sin()],
            )
        }
        RootStyle::BaseCircleLine => {
            // FreeCAD `fcgear` 口径：弧顶在齿廓起点**同一半径线**上（径向直线），
            // 圆心角 δ 由“弧顶落在该半径线上”反解：δ = 2·asin(ρ/(2·Rci))
            let dth = 2.0 * (rho / (2.0 * r_c)).min(1.0).asin();
            let th_c = th_start + sign * dth;
            let top = [r_c * th_start.cos(), r_c * th_start.sin()];
            radial = Some((start, top));
            arc_top = top;
            (
                [r_c * th_c.cos(), r_c * th_c.sin()],
                [rf * th_c.cos(), rf * th_c.sin()],
            )
        }
        RootStyle::NoFillet => {
            // 圆角 = 0：直线直接落到齿根圆
            let foot = [rf * th_start.cos(), rf * th_start.sin()];
            radial = Some((start, foot));
            (foot, foot)
        }
    };
    // root_pt / arc_top 相对**齿轮中心**的极角（用于判定圆角弧取哪一支）
    let th_root = root_pt[1].atan2(root_pt[0]);
    let th_top = arc_top[1].atan2(arc_top[0]);
    // 圆角弧的起/讫角（相对圆角圆心，度）—— NoFillet 时弧退化，调用方不画
    let ang = |q: [f64; 2]| {
        let v = (q[0] - fillet_c[0], q[1] - fillet_c[1]);
        v.1.atan2(v.0).to_degrees()
    };
    let (mut a0, mut a1) = (ang(root_pt), ang(arc_top));
    if style != RootStyle::NoFillet && (a1 - a0).rem_euclid(360.0) > 180.0 {
        // 取**短弧**那一支：圆角弧两种画法都 ≲90°（模板 80°、降级①约 90°），
        // 短弧判定唯一正确；早先“按弧中点是否落在两端之间”的写法会把 270° 那支选进来，
        // 采样链会先鼓出去再缩回来（z=7/17 单调性测试就挂在这里）。
        std::mem::swap(&mut a0, &mut a1);
    }
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
        style,
        radial,
    })
}

/// 一条齿根段的采样点（**从齿根端到齿顶**，用于质量估算与闭合性自检）。
fn flank_points(p: &GearParams, f: &Flank, seg: usize) -> Vec<[f64; 2]> {
    let rho = p.rho();
    let mut out = Vec::with_capacity(3 * seg + 4);
    // 圆角弧：root_pt → start（若 NoFillet 则跳过，由直线承担）
    if f.style != RootStyle::NoFillet {
        let mut seg_pts = Vec::with_capacity(seg + 1);
        // DXF 圆弧是“从 start 逆时针扫到 end”：a1 可能比 a0 小（例如 a0=97.9°、a1=−176.2°），
        // 直接线性插值会反着绕过 0°（把圆角的“肚子”也扫进来）→ 必须先解出 CCW 扫角。
        let sweep = (f.fillet_a1 - f.fillet_a0).rem_euclid(360.0);
        for k in 0..=seg {
            let a = (f.fillet_a0 + sweep * k as f64 / seg as f64).to_radians();
            seg_pts.push([f.fillet_c[0] + rho * a.cos(), f.fillet_c[1] + rho * a.sin()]);
        }
        // 弧实体按 CCW(a0→a1) 存（渲染口径），但采样链必须**从齿根端往齿顶走**：
        // 若首点比末点更靠近圆心，就把这一段反过来。
        let r = |q: [f64; 2]| (q[0] * q[0] + q[1] * q[1]).sqrt();
        if r(seg_pts[0]) > r(seg_pts[seg]) {
            seg_pts.reverse();
        }
        out.extend(seg_pts);
    }
    // 径向直线（降级时）：start → 弧顶（与上一段相接）
    if let Some((from, to)) = f.radial {
        // 链方向是“齿根→齿顶”：本段要从弧顶/齿根脚走到渐开线起点
        let (a, b) = (to, from);
        for k in 0..=seg {
            let t = k as f64 / seg as f64;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    // 渐开线样条：按真实曲线采样
    let (t0, t1) = (f.knots[3], f.knots[7]);
    for k in 0..=3 * seg {
        let u = t0 + (t1 - t0) * k as f64 / (3 * seg) as f64;
        let mut q = [0.0f64; 2];
        for (j, d) in f.ctrl.iter().enumerate() {
            let w = basis(j, 3, u, &f.knots);
            q[0] += w * d[0];
            q[1] += w * d[1];
        }
        out.push(q);
    }
    out
}

/// 由 5 个拟合点造 clamped 三次 B 样条：弦长参数化出 11 个节点，再解 7 个控制点。
///
/// 外齿轮（`make_flank`）与内齿轮（`make_flank_internal`）共用 —— 两边的实体结构完全同构，
/// 差别只在拟合点取自哪条曲线段。
fn fit_cubic_bspline_knots(fit: &[[f64; 2]; 5]) -> Result<(Vec<f64>, Vec<[f64; 2]>), String> {
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
    let ctrl = fit_cubic_bspline(&knots, fit)?;
    Ok((knots, ctrl))
}

/// 一条齿廓 B 样条实体（外/内齿轮共用）。
fn spline_entity(knots: &[f64], ctrl: &[[f64; 2]]) -> EntityType {
    let mut sp = Spline::new();
    sp.degree = 3;
    sp.flags.planar = true;
    sp.flags.rational = true; // 与模板一致：标 rational + 权重全 1（等价非有理）
    sp.knots = knots.to_vec();
    sp.control_points = ctrl.iter().map(|q| Vector3::new(q[0], q[1], 0.0)).collect();
    sp.weights = vec![1.0; ctrl.len()];
    sp.normal = Vector3::new(0.0, 0.0, 1.0);
    set_layer(&mut sp, LAYER_MAIN);
    EntityType::Spline(sp)
}

/// 齿根段的实体：圆角弧（模板/降级①）+ 降级时的**径向直线**；NoFillet 时只有直线。
fn push_root_seg(out: &mut Vec<EntityType>, p: &GearParams, f: &Flank) {
    if f.style != RootStyle::NoFillet {
        out.push(arc_deg(f.fillet_c, p.rho(), f.fillet_a0, f.fillet_a1, LAYER_MAIN));
    }
    if let Some((from, to)) = f.radial {
        out.push(line(from, to, LAYER_MAIN));
    }
}

/// 一条齿廓对应的 B 样条实体。
fn spline_of(f: &Flank) -> EntityType {
    spline_entity(&f.knots, &f.ctrl)
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
        // 齿根段 + 齿廓（−1 侧）：齿根端 → 弧/直线 → 渐开线
        push_root_seg(&mut out, p, &fr);
        out.push(spline_of(&fr));
        // 齿顶弧：以齿中心线为界各半条（模板同构）
        let psi_end = p.half_tooth_angle(ra).to_degrees();
        out.push(arc_deg([0.0, 0.0], ra, c_deg - psi_end, c_deg, LAYER_MAIN));
        out.push(arc_deg([0.0, 0.0], ra, c_deg, c_deg + psi_end, LAYER_MAIN));
        // 齿廓 + 齿根段（+1 侧）：渐开线 → 直线/弧 → 齿根端
        out.push(spline_of(&fl));
        push_root_seg(&mut out, p, &fl);
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

// ─────────────────── 内齿轮（齿圈）—— 二期，用户 2026-09-17 给模板 ───────────────────
//
// 反解自 `~/桌面/OCSM/齿轮/内齿轮.dxf`（m=2 z=40，与外齿轮模板同参数，"镜像件"）。
// 核心结论（逐点核对过）：**内齿轮的齿槽形状 = 同参数外齿轮的齿形** —— 同一条渐开线、
// 同一个 ψ(R) 公式，只是 ψ 要从**齿槽中心线**量、材料在外侧。因此：
//   * 齿顶圆 da = d − 2ha（最内）、齿根圆 df = d + 2hf（最外）；
//   * 圆角圆心在 **rf − ρ**（外齿轮是 rf + ρ）；
//   * 圆角切点是**解出来的**（不像外齿轮那样先定齿廓起点再解圆心）：
//     解 `|P(R) + ρ·n(R)| = rf − ρ`；模板核对 R=42.076、圆心角 184.446°（实测 184.445°）。
//   * 每齿 8 图元、拓扑与外齿轮同构（2 齿廓样条 + 2 圆角弧 + 2 齿根弧 + 2 齿顶弧）。

/// 内齿轮一个**齿槽单侧**的构造结果。
#[derive(Debug, Clone)]
struct FlankInt {
    knots: Vec<f64>,
    ctrl: Vec<[f64; 2]>,
    /// 齿廓在齿顶侧（内侧）的一端
    tip: [f64; 2],
    /// 齿廓在齿根圆角切点那一端
    end: [f64; 2],
    /// 齿根圆角圆心（在 rf − ρ 上）
    fillet_c: [f64; 2],
    /// 圆角与齿根圆的切点
    root_pt: [f64; 2],
    /// 圆角弧的起/讫角（相对圆角圆心，度，CCW）
    fillet_a0: f64,
    fillet_a1: f64,
    /// 齿顶圆低于基圆时：从齿顶圆到齿廓起点的径向直线
    radial_tip: Option<([f64; 2], [f64; 2])>,
    /// 圆角无解时：从齿廓末端径向落到齿根圆的直线
    radial_root: Option<([f64; 2], [f64; 2])>,
    /// 圆角是否退化（无解 → 圆角 = 0）
    no_fillet: bool,
}

/// 径向直线（或点）相对齿槽中心线的角度差（度）。
fn ang_delta_deg(q: [f64; 2], base_deg: f64) -> f64 {
    let a = q[1].atan2(q[0]).to_degrees() - base_deg;
    let mut d = a % 360.0;
    if d > 180.0 {
        d -= 360.0;
    }
    if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// 解内齿轮齿根圆角的切点/圆心。返回 `(切点半径, 圆心, 切点)`。
///
/// 两个法向分支里取"圆心落在齿槽中心与齿廓之间"的那个（另一个会把圆角切到齿的另一侧）。
fn solve_internal_fillet(p: &GearParams, s: f64, sign: f64) -> Option<(f64, [f64; 2], [f64; 2])> {
    let rho = p.rho();
    let r_c = p.fillet_center_radius();
    let rf = p.df() / 2.0;
    let r0 = p.internal_flank_r_min();
    let r1 = rf - 1e-7;
    if r1 <= r0 {
        return None;
    }
    let s_deg = s.to_degrees();
    let pt = |r: f64| involute_point(p, r, s, sign);
    let normal = |r: f64| {
        let d = 1e-5;
        let a = pt((r - d).max(r0));
        let b = pt((r + d).min(r1));
        let (tx, ty) = (b[0] - a[0], b[1] - a[1]);
        let l = (tx * tx + ty * ty).sqrt().max(1e-12);
        (-ty / l, tx / l)
    };
    let mut best: Option<(f64, [f64; 2], f64)> = None;
    const SCAN: usize = 2000;
    for flip in [1.0f64, -1.0] {
        let eval = |r: f64| -> ([f64; 2], f64) {
            let q = pt(r);
            let (nx, ny) = normal(r);
            let f = [q[0] + rho * nx * flip, q[1] + rho * ny * flip];
            (f, (f[0] * f[0] + f[1] * f[1]).sqrt() - r_c)
        };
        let mut prev: Option<(f64, f64)> = None;
        for k in 0..=SCAN {
            let r = r0 + (r1 - r0) * k as f64 / SCAN as f64;
            let (_, e) = eval(r);
            if let Some((rp, ep)) = prev {
                if ep * e < 0.0 {
                    let rr = rp - ep * (r - rp) / (e - ep);
                    let (f, _) = eval(rr);
                    let dc = ang_delta_deg(f, s_deg);
                    // 判据（踩过两次坑才定下来）：圆心必须落在**它自己那个半径处齿槽的角范围**内，
                    // 即 |dc| < ψ槽(|F|)。四个根（两个法向分支 × 两侧）里只有这一个稳：
                    //   * 只按"离齿槽中心线更近"挑 → m=10 时并列挑错（根弧扫过整个齿）；
                    //   * 按"与齿廓同侧"挑 → m=2 模板对、m=10 错（圆心确实会跨过齿槽中心线）。
                    // 落在齿槽外 = 圆心跑进相邻齿的材料里，那种圆角是错的。
                    let fc = (f[0] * f[0] + f[1] * f[1]).sqrt();
                    let psi_c = p.space_half_angle(fc).to_degrees();
                    if dc.abs() < psi_c - 1e-9
                        && (best.is_none() || dc.abs() < best.as_ref().unwrap().2)
                    {
                        best = Some((rr, f, dc.abs()));
                    }
                }
            }
            prev = Some((r, e));
        }
    }
    best.map(|(r, f, _)| (r, f, pt(r)))
}

/// 内齿轮一个齿槽单侧的齿廓 + 齿根圆角（`sign=+1` 为逆时针那侧）。
fn make_flank_internal(p: &GearParams, s: f64, sign: f64) -> Result<FlankInt, String> {
    let rho = p.rho();
    let rf = p.df() / 2.0;
    let ra = p.da() / 2.0;
    let r_min = p.internal_flank_r_min();
    let solved = solve_internal_fillet(p, s, sign);
    let no_fillet = solved.is_none();
    let (r_t, fillet_c, end) = match solved {
        Some(t) => t,
        None => {
            // 退化：圆角 = 0 —— 齿廓画到齿根圆之前的某半径，再用径向直线落到齿根圆
            let r_lim = (r_min + 0.5 * (rf - r_min)).min(rf - 1e-6);
            let q = involute_point(p, r_lim, s, sign);
            (r_lim, q, q)
        }
    };
    // 5 个拟合点：从有效起点（齿顶圆，或降级时的基圆略上方）到圆角切点，半径等分
    let mut fit = [[0.0f64; 2]; 5];
    for (i, f) in [0.0, 0.25, 0.5, 0.75, 1.0].iter().enumerate() {
        let radius = r_min + (r_t - r_min) * f;
        fit[i] = involute_point(p, radius, s, sign);
    }
    let (knots, ctrl) = fit_cubic_bspline_knots(&fit)?;
    let tip = fit[0];
    let end = if no_fillet { fit[4] } else { end };
    // 圆角与齿根圆的切点：圆心沿径向拉到 rf
    let fc_len = (fillet_c[0] * fillet_c[0] + fillet_c[1] * fillet_c[1]).sqrt().max(1e-12);
    let root_pt = [fillet_c[0] * rf / fc_len, fillet_c[1] * rf / fc_len];
    let ang = |q: [f64; 2]| {
        let v = (q[0] - fillet_c[0], q[1] - fillet_c[1]);
        v.1.atan2(v.0).to_degrees()
    };
    // 圆角弧 CCW 方向（模板口径）：−1 侧 切点→齿根切点；+1 侧 齿根切点→切点
    let (mut a0, mut a1) = if sign > 0.0 {
        (ang(root_pt), ang(end))
    } else {
        (ang(end), ang(root_pt))
    };
    if (a1 - a0).rem_euclid(360.0) > 180.0 {
        std::mem::swap(&mut a0, &mut a1);
    }
    // 齿顶圆低于基圆：齿廓到不了齿顶圆，用径向直线补一段（简化画法）
    let radial_tip = if r_min > ra + 1e-12 {
        let th = tip[1].atan2(tip[0]);
        Some(([ra * th.cos(), ra * th.sin()], tip))
    } else {
        None
    };
    // 圆角退化：齿廓末端径向落到齿根圆
    let radial_root = if no_fillet {
        let th = end[1].atan2(end[0]);
        Some((end, [rf * th.cos(), rf * th.sin()]))
    } else {
        None
    };
    Ok(FlankInt {
        knots,
        ctrl,
        tip,
        end,
        fillet_c,
        root_pt,
        fillet_a0: a0,
        fillet_a1: a1,
        radial_tip,
        radial_root,
        no_fillet,
    })
}

/// 内齿轮齿廓的实体（B 样条）。
fn spline_of_int(f: &FlankInt) -> EntityType {
    spline_entity(&f.knots, &f.ctrl)
}

/// 内齿轮的齿根那一段实体：径向直线（如有）+ 圆角弧（如有）。
fn push_root_int(out: &mut Vec<EntityType>, p: &GearParams, f: &FlankInt) {
    if let Some((a, b)) = f.radial_root {
        out.push(line(a, b, LAYER_MAIN));
    }
    if !f.no_fillet {
        out.push(arc_deg(f.fillet_c, p.rho(), f.fillet_a0, f.fillet_a1, LAYER_MAIN));
    }
}

/// 内齿轮端视图（常规正视图）：**齿槽**按外齿轮齿形画，齿朝圆心。
///
/// 相位：齿槽中心在半个齿距处（模板：齿中心 0°、齿槽 4.5°），与外齿轮模板的相位约定同构。
/// **不画分度圆**——内齿轮模板里没有（外齿轮模板有；模板是唯一权威）。
fn front_internal(p: &GearParams, n: f64) -> Result<Vec<EntityType>, String> {
    let ra = p.da() / 2.0;
    let rf = p.df() / 2.0;
    let pitch = p.pitch_angle();
    let psi_tip = p.internal_tip_half_angle().to_degrees();
    let mut out = Vec::with_capacity(p.z as usize * 8 + 4);
    let s_first = pitch / 2.0;
    for t in 0..p.z as usize {
        let s = s_first + pitch * t as f64;
        let s_deg = s.to_degrees();
        let fl = make_flank_internal(p, s, 1.0)?; // 齿槽 +1 侧
        let fr = make_flank_internal(p, s, -1.0)?; // 齿槽 −1 侧
        let pitch_deg = pitch.to_degrees();
        let tooth_c = s_deg + pitch_deg / 2.0;
        // CCW 链：−1 侧齿廓（齿顶→圆角）→ 圆角 → 齿根弧半条
        if let Some((a, b)) = fr.radial_tip {
            out.push(line(a, b, LAYER_MAIN));
        }
        out.push(spline_of_int(&fr));
        push_root_int(&mut out, p, &fr);
        // 齿根弧两半：两端是两侧圆角的齿根切点。**按相对齿槽中心的偏移排序**取，不能按
        // "−侧在前"写死 —— 圆心跨过齿槽中心线时（m=10 常见）两边会互换，写死会让根弧
        // 反向扫掉一整圈（链条自检当场抓到 34.6mm 的"断链"）。
        let (d_lo, d_hi) = {
            let (a, b) = (ang_delta_deg(fr.root_pt, s_deg), ang_delta_deg(fl.root_pt, s_deg));
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        };
        out.push(arc_deg(
            [0.0, 0.0],
            rf,
            near(s_deg + d_lo, s_deg - 1.0),
            near(s_deg, s_deg - 1.0),
            LAYER_MAIN,
        ));
        // 齿根弧另半条 → +1 侧圆角 → 齿廓（圆角→齿顶）
        out.push(arc_deg(
            [0.0, 0.0],
            rf,
            near(s_deg, s_deg + 1.0),
            near(s_deg + d_hi, s_deg + 1.0),
            LAYER_MAIN,
        ));
        push_root_int(&mut out, p, &fl);
        out.push(spline_of_int(&fl));
        if let Some((a, b)) = fl.radial_tip {
            out.push(line(a, b, LAYER_MAIN));
        }
        // 齿顶弧：以齿中心线为界各半条（模板同构）
        // 端点 = 齿中心 ± 齿顶半角 ψtip = pitch/2 − ψ槽 —— 注意不是 s ± ψtip！
        // （走过坑：写成 s + ψtip 时齿顶弧起点错到齿的另一侧，链条在齿顶处拉出 1mm 直线）
        out.push(arc_deg(
            [0.0, 0.0],
            ra,
            near(tooth_c - psi_tip, tooth_c - 1.0),
            near(tooth_c, tooth_c - 1.0),
            LAYER_MAIN,
        ));
        out.push(arc_deg(
            [0.0, 0.0],
            ra,
            near(tooth_c, tooth_c + 1.0),
            near(tooth_c + psi_tip, tooth_c + 1.0),
            LAYER_MAIN,
        ));
    }
    // 十字中心线：模板实测长 91 = 齿根圆直径 85 + 1×6
    out.extend(cross_centerlines([0.0, 0.0], p.df(), n));
    Ok(out)
}

/// 内齿轮剖视图（模板结构，逐条对齐）。
///
/// 每半侧 10 条：端面（0→ra+C）+ 外侧端面（ra+C→rf）×2 端 + 齿顶线 + 齿根线 + 内孔壁 ×2 + 孔口 45° 倒角 ×2；
/// 加 分度线 ×2（点划线，长 = h + 3n）+ 轴线 ×1（= h + 4n）。
/// 共 23 条 —— 与用户「内齿轮使用示例.dxf」里那个块**完全一致**（20 粗实线 + 3 中心线）。
/// **不打剖面线**（用户 2026-09-17 明确：齿圈外壁结构由用户/AI 在生产环境里延伸后再打）。
fn section_internal(p: &GearParams, n: f64) -> Result<Vec<EntityType>, String> {
    let ra = p.da() / 2.0;
    let rf = p.df() / 2.0;
    let hh = p.h / 2.0;
    let c = p.chamfer();
    let hc = hh - c;
    let mut out = Vec::with_capacity(23);
    for s in [1.0f64, -1.0] {
        // 端面：0 → ra+C 与 ra+C → rf（模板把它画成两段）
        out.push(line([-hh, 0.0], [-hh, s * (ra + c)], LAYER_MAIN));
        out.push(line([-hh, s * (ra + c)], [-hh, s * rf], LAYER_MAIN));
        out.push(line([hh, 0.0], [hh, s * (ra + c)], LAYER_MAIN));
        out.push(line([hh, s * (ra + c)], [hh, s * rf], LAYER_MAIN));
        // 齿顶线（内孔）与齿根线（外圆）
        out.push(line([-hc, s * ra], [hc, s * ra], LAYER_MAIN));
        out.push(line([-hh, s * rf], [hh, s * rf], LAYER_MAIN));
        // 内孔壁
        out.push(line([-hc, 0.0], [-hc, s * ra], LAYER_MAIN));
        out.push(line([hc, 0.0], [hc, s * ra], LAYER_MAIN));
        // 孔口 45° 倒角（C = round(0.6m)，与模板的 1×1 一致）
        out.push(line([-hc, s * ra], [-hh, s * (ra + c)], LAYER_MAIN));
        out.push(line([hc, s * ra], [hh, s * (ra + c)], LAYER_MAIN));
    }
    // 分度线（点划线）+ 轴线（中心线）—— 长度按模板的 +3n / +4n
    let d2 = p.d() / 2.0;
    let c3 = (p.h + 3.0 * n) / 2.0;
    let c4 = (p.h + 4.0 * n) / 2.0;
    out.push(line([-c3, d2], [c3, d2], LAYER_CENTER));
    out.push(line([-c3, -d2], [c3, -d2], LAYER_CENTER));
    out.push(line([-c4, 0.0], [c4, 0.0], LAYER_CENTER));
    Ok(out)
}

/// 内齿轮端视图的闭合链采样（质量估算 / 连续性自检用）。
///
/// **必须与 `front_internal` 的实体顺序逐条对齐**（走过一次坑：自己另写一套弧方向，
/// 结果根弧多扫一整圈、链条在齿顶到齿根之间拉出 5mm 直线，测试当场抓到）。
fn internal_chain_points(p: &GearParams, seg: usize) -> Result<Vec<[f64; 2]>, String> {
    let ra = p.da() / 2.0;
    let rf = p.df() / 2.0;
    let rho = p.rho();
    let pitch = p.pitch_angle();
    let psi_tip = p.internal_tip_half_angle().to_degrees();
    let mut out: Vec<[f64; 2]> = Vec::new();
    let arc_of = |c: [f64; 2], r: f64, a0: f64, a1: f64, out: &mut Vec<[f64; 2]>| {
        let sweep = (a1 - a0).rem_euclid(360.0);
        for k in 0..=seg {
            let a = (a0 + sweep * k as f64 / seg as f64).to_radians();
            out.push([c[0] + r * a.cos(), c[1] + r * a.sin()]);
        }
    };
    let spline_pts = |f: &FlankInt, reverse: bool, out: &mut Vec<[f64; 2]>| {
        let (t0, t1) = (f.knots[3], f.knots[7]);
        let mut v = Vec::with_capacity(3 * seg + 1);
        for k in 0..=3 * seg {
            let u = t0 + (t1 - t0) * k as f64 / (3 * seg) as f64;
            let mut q = [0.0f64; 2];
            for (j, d) in f.ctrl.iter().enumerate() {
                let w = basis(j, 3, u, &f.knots);
                q[0] += w * d[0];
                q[1] += w * d[1];
            }
            v.push(q);
        }
        if reverse {
            v.reverse();
        }
        out.extend(v);
    };
    let s_first = pitch / 2.0;
    for t in 0..p.z as usize {
        let s = s_first + pitch * t as f64;
        let s_deg = s.to_degrees();
        let tooth_c = s_deg + pitch.to_degrees() / 2.0;
        let fl = make_flank_internal(p, s, 1.0)?;
        let fr = make_flank_internal(p, s, -1.0)?;
        // 径向直线也要按 seg 插值（否则 3mm 的直线段会被连续性自检当成"断链"）
        let seg_line = |a: [f64; 2], b: [f64; 2], out: &mut Vec<[f64; 2]>| {
            for k in 0..=seg {
                let t = k as f64 / seg as f64;
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        };
        // −1 侧：径向直线（降级）→ 齿廓 → 圆角/径向落地 → 齿根弧两半
        if let Some((a, b)) = fr.radial_tip {
            seg_line(a, b, &mut out);
        }
        spline_pts(&fr, false, &mut out);
        if !fr.no_fillet {
            arc_of(fr.fillet_c, rho, fr.fillet_a0, fr.fillet_a1, &mut out);
        }
        if let Some((a, b)) = fr.radial_root {
            seg_line(a, b, &mut out);
        }
        let (d_lo, d_hi) = {
            let (a, b) = (ang_delta_deg(fr.root_pt, s_deg), ang_delta_deg(fl.root_pt, s_deg));
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        };
        arc_of(
            [0.0, 0.0],
            rf,
            near(s_deg + d_lo, s_deg - 1.0),
            near(s_deg, s_deg - 1.0),
            &mut out,
        );
        arc_of(
            [0.0, 0.0],
            rf,
            near(s_deg, s_deg + 1.0),
            near(s_deg + d_hi, s_deg + 1.0),
            &mut out,
        );
        // +1 侧：圆角 → 齿廓 → 径向直线（降级）
        if let Some((a, b)) = fl.radial_root {
            // +1 侧：链从齿根圆走向齿廓末端（入口在齿根圆上）
            seg_line(b, a, &mut out);
        }
        if !fl.no_fillet {
            arc_of(fl.fillet_c, rho, fl.fillet_a0, fl.fillet_a1, &mut out);
        }
        spline_pts(&fl, true, &mut out);
        if let Some((a, b)) = fl.radial_tip {
            seg_line(b, a, &mut out);
        }
        // 齿顶弧两半（端点 = 齿中心 ± ψtip，与 front_internal 一致）
        arc_of(
            [0.0, 0.0],
            ra,
            near(tooth_c - psi_tip, tooth_c - 1.0),
            near(tooth_c, tooth_c - 1.0),
            &mut out,
        );
        arc_of(
            [0.0, 0.0],
            ra,
            near(tooth_c, tooth_c + 1.0),
            near(tooth_c + psi_tip, tooth_c + 1.0),
            &mut out,
        );
    }
    Ok(out)
}

/// 内齿轮质量估算（kg）：齿圈材料 = π·rf² − 齿廓围成的面积（齿廓围的是空腔），× 宽度 × 钢 7.85e-6。
///
/// 注意：这是**齿高那一段齿圈**的质量（剖视图画到齿根圆为止的那部分），
/// 真实齿圈还要加轮缘/腹板 —— 与模板"外壁留给用户"的口径一致。
fn internal_weight_kg(p: &GearParams) -> String {
    let rf = p.df() / 2.0;
    let pts = match internal_chain_points(p, 8) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let n = pts.len();
    let mut area = 0.0;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        area += a[0] * b[1] - b[0] * a[1];
    }
    let void = (area / 2.0).abs();
    let ring = (std::f64::consts::PI * rf * rf - void).max(0.0);
    format!("{:.3}", ring * p.h * 7.85e-6)
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
    if !view.available_for(p.kind) {
        return Err(format!(
            "{}不提供「{}」视图 —— 用户给的模板（内齿轮.dxf）里只有**剖视图**和**端视图**两个视图；\n\
             模板没有的画法不猜（避免出一张看起来对、实际没依据的图）。要用请先给对应模板。",
            p.kind.label(),
            view.label()
        ));
    }
    let entities = match (p.kind, view) {
        (GearKind::External, GearView::Section) => section_view(p, n)?,
        (GearKind::External, GearView::Side) => side_view(p, n)?,
        (GearKind::External, GearView::Simplified) => front_simplified(p, n)?,
        (GearKind::External, GearView::Front) => front_regular(p, n)?,
        (GearKind::Internal, GearView::Section) => section_internal(p, n)?,
        (GearKind::Internal, GearView::Front) => front_internal(p, n)?,
        (_, v) => {
            return Err(format!(
                "{}不提供「{}」视图（见上文）。",
                p.kind.label(),
                v.label()
            ))
        }
    };
    let bbox = bbox_of(&entities);
    Ok(GenPart {
        entities,
        meta: PartMeta {
            code: p.spec(),
            name: format!("{}（{}）", p.kind.label(), view.label_for(p.kind)),
            spec: p.spec(),
            material: String::new(),
            weight: if p.kind.is_internal() {
                internal_weight_kg(p)
            } else {
                solid_weight_kg(p)
            },
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
    let pitch = p.pitch_angle();
    let c_first = pitch / 2.0;
    const SEG: usize = 12;
    let arc = |r: f64, a0_deg: f64, a1_deg: f64, out: &mut Vec<[f64; 2]>| {
        for k in 0..=SEG {
            let a = (a0_deg + (a1_deg - a0_deg) * k as f64 / SEG as f64).to_radians();
            out.push([r * a.cos(), r * a.sin()]);
        }
    };
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(p.z as usize * (8 * SEG + 4));
    for t in 0..p.z as usize {
        let c = c_first + pitch * t as f64;
        let (fl, fr) = match (make_flank(p, c, 1.0), make_flank(p, c, -1.0)) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return String::new(),
        };
        let c_deg = c.to_degrees();
        // 与 front_regular 同一条 CCW 链：
        // 齿根弧 → −1 侧齿根段 → −1 侧齿廓 → 齿顶弧 → +1 侧齿廓 → +1 侧齿根段
        arc(rf, c_deg - pitch.to_degrees() / 2.0, near(th_of(fr.root_pt), c_deg - 3.0), &mut pts);
        let neg = flank_points(p, &fr, SEG); // 齿根端 → 齿顶
        pts.extend(neg.iter().copied());
        arc(
            ra,
            c_deg - p.half_tooth_angle(ra).to_degrees(),
            c_deg + p.half_tooth_angle(ra).to_degrees(),
            &mut pts,
        );
        let pos = flank_points(p, &fl, SEG); // 齿根端 → 齿顶（要反着走）
        pts.extend(pos.iter().rev().copied());
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
    let mut s = String::from("OCSM_GEAR");
    if p.kind.is_internal() {
        s.push_str("_INT"); // 内齿轮标记；外齿轮块名保持一期原样不变
    }
    s.push_str(&format!("_M{}_Z{}", trim(p.m).replace('.', "_"), p.z));
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
    let usage = "用法：OCSMGEAR [内齿轮|int] <模数m> <齿数z> [h=齿宽] [ha=齿顶高系数] [c=顶隙系数] [beta=螺旋角(右旋为正)] [x=变位系数] \
                 [view 剖视图|侧视图|简化正视图|常规正视图|端视图] [at x,y] [rot 度]。\n\
                 不带参数则打开齿轮窗口。内齿轮（齿圈）目前只有 剖视图 + 端视图（模板只有这两个）；\
                 剖视图不画齿圈外壁与剖面线，由用户/AI 按实际齿圈结构延伸。";
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
            "kind" | "type" | "种类" | "类型" => {
                let v = match val {
                    Some(v) => v.to_string(),
                    None => {
                        i += 1;
                        toks.get(i).map(|s| s.to_string()).unwrap_or_default()
                    }
                };
                p.kind = GearKind::parse(&v).ok_or_else(|| {
                    format!("种类无法识别：`{}`。可用 internal|内齿轮、external|外齿轮。", v)
                })?;
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
                // 位置参数：第 1 个 m、第 2 个 z、第 3 个 h；也可以是中文视图名/种类名
                if let Ok(vw) = GearView::parse(other) {
                    view = Some(vw);
                    i += 1;
                    continue;
                }
                if let Some(k) = GearKind::parse(other) {
                    p.kind = k;
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
        // 模板里 h 是用户给的：外齿轮模板 h=20（m=2，即 10m）、内齿轮模板 h=30（m=2，即 15m）
        p.h = if p.kind.is_internal() {
            (15.0 * p.m).max(1.0)
        } else {
            (10.0 * p.m).max(1.0)
        };
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
    let kind = match get("kind") {
        Some(v) => GearKind::parse(&v)
            .ok_or_else(|| format!("kind 无法识别：`{}`（可用 internal|内齿轮、external|外齿轮）。", v))?,
        None => GearKind::External,
    };
    let m = f("m", 2.0);
    let p = GearParams {
        kind,
        m,
        z: f("z", 40.0).round() as u32,
        ha: f("ha", 1.0),
        c: f("c", 0.25),
        beta_deg: f("beta", 0.0),
        h: f("h", if kind.is_internal() { 15.0 * m } else { 20.0 }),
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
        "kind": p.kind.key(),
        "kind_label": p.kind.label(),
        "view": view.key(),
        "view_label": view.label_for(p.kind),
        "d": round4(p.d()),
        "da": round4(p.da()),
        "df": round4(p.df()),
        "db": round4(p.db()),
        "mt": round4(p.mt()),
        "alpha_t": round4(p.alpha_t().to_degrees()),
        "rho": round4(p.rho()),
        "fillet_center_r": round4(p.fillet_center_radius()),
        "internal_tip_below_base": p.internal_tip_falls_below_base(),
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
        GearParams {
            kind: GearKind::External,
            m: 2.0,
            z: 40,
            ha: 1.0,
            c: 0.25,
            beta_deg: 0.0,
            h: 20.0,
            x: 0.0,
        }
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

    /// 小齿数回归（用户 2026-09-17 报 z=17 渲染异常）：
    /// 模板口径的圆角在齿数小时无解 → 必须降级 + 给警告，且**齿廓链不能断**。
    #[test]
    fn small_tooth_counts_fall_back_and_stay_continuous() {
        for (m, z, want) in [
            (10.0, 7u32, RootStyle::BaseCircleLine),
            (10.0, 14, RootStyle::BaseCircleLine),
            (10.0, 17, RootStyle::BaseCircleLine),
            (10.0, 40, RootStyle::TemplateArc),
            (2.0, 40, RootStyle::TemplateArc),
        ] {
            let p = GearParams { m, z, h: 20.0, ..GearParams::default() };
            assert_eq!(p.root_style(), want, "m={m} z={z} 的齿根画法");
            if want != RootStyle::TemplateArc {
                assert!(p.root_style_note().is_some(), "m={m} z={z} 应有降级警告");
            } else {
                assert!(p.root_style_note().is_none());
            }
            // 齿廓链连续性：采样点半径从 rf 单调升到 ra，且相邻点间距没有大跳（原 bug 就是这里断开）
            let pitch = p.pitch_angle();
            for sign in [1.0f64, -1.0] {
                let f = make_flank(&p, pitch / 2.0, sign).unwrap();
                let pts = flank_points(&p, &f, 12);
                let radii: Vec<f64> = pts.iter().map(|q| (q[0] * q[0] + q[1] * q[1]).sqrt()).collect();
                assert!(
                    (radii[0] - p.df() / 2.0).abs() < 1e-6,
                    "链首应在齿根圆上：{:.6} vs {:.6}",
                    radii[0],
                    p.df() / 2.0
                );
                assert!(
                    (radii[radii.len() - 1] - p.da() / 2.0).abs() < 1e-6,
                    "链尾应在齿顶圆上：{:.6} vs {:.6}",
                    radii[radii.len() - 1],
                    p.da() / 2.0
                );
                for w in radii.windows(2) {
                    assert!(w[1] >= w[0] - 1e-6, "半径应单调升：{} → {}", w[0], w[1]);
                }
                // 连续性：最大步长不该是常见步长的好几倍（原 bug 就是圆角弧与样条之间差 ~1.6mm 的断口）
                let mut steps: Vec<f64> = pts
                    .windows(2)
                    .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
                    .collect();
                let mut sorted = steps.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let median = sorted[sorted.len() / 2];
                steps.sort_by(|a, b| b.partial_cmp(a).unwrap());
                let worst = steps[0];
                assert!(
                    worst < 3.0 * median,
                    "相邻采样点最大间距 {:.4}（中位 {:.4}，链应连续）",
                    worst,
                    median
                );
            }
            // 出图实体也不该有异常张角的弧
            let v = front_regular(&p, 1.0).unwrap();
            for e in &v {
                if let EntityType::Arc(a) = e {
                    let span = (a.end_angle - a.start_angle).to_degrees().rem_euclid(360.0);
                    assert!(span < 180.0, "弧张角异常 {:.2}°（r={:.3}）", span, a.radius);
                }
            }
        }
    }

    #[test]
    fn z17_fallback_adds_radial_line_and_keeps_fillet() {
        let p = GearParams { m: 10.0, z: 17, h: 20.0, ..GearParams::default() };
        let f = make_flank(&p, p.pitch_angle() / 2.0, 1.0).unwrap();
        let (from, to) = f.radial.expect("降级①应有径向直线");
        let r1 = (from[0] * from[0] + from[1] * from[1]).sqrt();
        let r2 = (to[0] * to[0] + to[1] * to[1]).sqrt();
        let r_start = p.db() / 2.0 + (p.da() / 2.0 - p.db() / 2.0) * 0.02; // 降级起点（基圆略上方）
        assert!((r1 - r_start).abs() < 1e-9, "直线从齿廓起点起（{:.3} vs {:.3}）", r1, r_start);
        assert!(
            (r2 - (p.df() / 2.0 + p.rho())).abs() < 1e-9,
            "直线到圆角弧顶（{:.3}）",
            r2
        );
        // 直线是径向的（两端同角度）
        let a1 = from[1].atan2(from[0]);
        let a2 = to[1].atan2(to[0]);
        assert!((a1 - a2).abs() < 1e-9, "径向直线");
        // 圆角弧仍保留、且不过 180°
        let span = (f.fillet_a1 - f.fillet_a0).rem_euclid(360.0);
        assert!(span < 180.0 && span > 1.0, "圆角弧张角 {:.3}°", span);
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
    ///
    /// 输出目录用**持久目录** `~/桌面/OCSM/review/`（以前写 /tmp —— 重启就没了，
    /// 2026-09-17 被清过一次，所有核对产物全丢）。
    /// 输出：`/tmp/gear_review/<view>.csv`（每行 `TYPE,layer,...`；样条已按真实曲线采样）。
    #[test]
    #[ignore]
    fn dump_views_csv() {
        // 除模板参数（m=2 z=40）外，再导两组小齿数供人工核对（用户 2026-09-17 报 z=17 异常）
        for (tag, m, z) in [("smallz17", 10.0, 17u32), ("smallz7", 10.0, 7)] {
            let p = GearParams { m, z, h: 20.0, ..GearParams::default() };
            let part = generate(&p, GearView::Front, 1.0).unwrap();
            let mut out = String::new();
            for e in &part.entities {
                match e {
                    EntityType::Line(l) => out.push_str(&format!(
                        "LINE,{},{:.6},{:.6},{:.6},{:.6}\n",
                        l.common.layer, l.start.x, l.start.y, l.end.x, l.end.y
                    )),
                    EntityType::Arc(a) => out.push_str(&format!(
                        "ARC,{},{:.6},{:.6},{:.6},{:.4},{:.4}\n",
                        a.common.layer, a.center.x, a.center.y, a.radius,
                        a.start_angle.to_degrees(), a.end_angle.to_degrees()
                    )),
                    EntityType::Spline(sp) => {
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
                    _ => {}
                }
            }
            let review_dir = std::path::PathBuf::from(format!(
                "{}/桌面/OCSM/review",
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
            ));
            std::fs::create_dir_all(&review_dir).unwrap();
            std::fs::write(review_dir.join(format!("{tag}.csv")), out).unwrap();
            println!("已导出 {}/{tag}.csv（{} 实体）", review_dir.display(), part.entities.len());
        }
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
        let dir = std::path::PathBuf::from(format!(
            "{}/桌面/OCSM/review",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
        ));
        std::fs::create_dir_all(&dir).unwrap();
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
        // 内齿轮：模板件（m2 z40 h30）+ 齿顶圆低于基圆的常用小齿数（z=24 / z=17）
        for (tag, m, z, h) in [("int_m2_z40", 2.0, 40u32, 30.0), ("int_m10_z24", 10.0, 24, 60.0), ("int_m10_z17", 10.0, 17, 60.0)] {
            let p = GearParams { kind: GearKind::Internal, m, z, h, ..GearParams::default() };
            for view in [GearView::Front, GearView::Section] {
                let part = generate(&p, view, 1.0).unwrap();
                let mut out = String::new();
                for e in &part.entities {
                    match e {
                        EntityType::Line(l) => out.push_str(&format!(
                            "LINE,{},{:.6},{:.6},{:.6},{:.6}\n",
                            l.common.layer, l.start.x, l.start.y, l.end.x, l.end.y
                        )),
                        EntityType::Arc(a) => out.push_str(&format!(
                            "ARC,{},{:.6},{:.6},{:.6},{:.4},{:.4}\n",
                            a.common.layer, a.center.x, a.center.y, a.radius,
                            a.start_angle.to_degrees(), a.end_angle.to_degrees()
                        )),
                        EntityType::Spline(sp) => {
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
                        _ => {}
                    }
                }
                let path = dir.join(format!("{tag}_{}.csv", view.key()));
                std::fs::write(&path, out).unwrap();
                println!(
                    "已导出 {}（{} 实体，提示 {} 条）",
                    path.display(),
                    part.entities.len(),
                    p.notes().len()
                );
                for nt in p.notes() {
                    println!("    提示：{nt}");
                }
            }
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

    // ─────────────────────────── 内齿轮（二期）───────────────────────────

    /// 用户内齿轮模板件：`~/桌面/OCSM/齿轮/内齿轮.dxf` = m2 z40 h30（da=76 df=85 ρ=0.76）。
    fn int_tmpl() -> GearParams {
        GearParams { kind: GearKind::Internal, m: 2.0, z: 40, h: 30.0, ..GearParams::default() }
    }

    #[test]
    fn internal_tmpl_diameters_match() {
        let p = int_tmpl();
        assert!((p.da() - 76.0).abs() < 1e-9, "da={}（模板 76）", p.da());
        assert!((p.df() - 85.0).abs() < 1e-9, "df={}（模板 85）", p.df());
        assert!((p.rho() - 0.76).abs() < 1e-9, "ρ=0.38m={}", p.rho());
        assert!(
            (p.fillet_center_radius() - 41.74).abs() < 1e-9,
            "圆角圆心半径 {:.4}（模板实测 41.74 = rf − ρ）",
            p.fillet_center_radius()
        );
        assert!(!p.internal_tip_falls_below_base(), "z=40：da=76 > db=75.18");
    }

    #[test]
    fn internal_space_half_angle_matches_template() {
        // 模板实测：R=42.076 处齿槽半角 0.9869°；齿顶弧半角 1.4578°（理论 1.462°）
        let p = int_tmpl();
        let psi = p.space_half_angle(42.076).to_degrees();
        assert!((psi - 0.987).abs() < 0.002, "ψ(42.076)={psi}");
        let tip = p.internal_tip_half_angle().to_degrees();
        assert!((tip - 1.462).abs() < 0.01, "齿顶半角={tip}（模板 1.4578）");
    }

    #[test]
    fn internal_fillet_solves_to_template_values() {
        // 模板实测：切点 R=42.076、圆心半径 41.74、圆心角 4.445°（齿槽中心 4.5°，偏 0.055°）
        let p = int_tmpl();
        let s = p.pitch_angle() / 2.0;
        let (r_t, f, tang) = solve_internal_fillet(&p, s, -1.0).expect("模板参数下圆角应有解");
        assert!((r_t - 42.076).abs() < 0.003, "切点半径 {r_t}（模板 42.076）");
        let fl = (f[0] * f[0] + f[1] * f[1]).sqrt();
        assert!((fl - 41.74).abs() < 1e-6, "圆心半径 {fl}");
        let a = f[1].atan2(f[0]).to_degrees().rem_euclid(360.0);
        assert!((a - 4.445).abs() < 0.01, "圆心角 {a}（模板 4.445）");
        let d = (tang[0] * tang[0] + tang[1] * tang[1]).sqrt();
        assert!((d - r_t).abs() < 1e-9, "切点应在齿廓上");
        let (_, f2, _) = solve_internal_fillet(&p, s, 1.0).unwrap();
        let a2 = f2[1].atan2(f2[0]).to_degrees().rem_euclid(360.0);
        assert!((a2 - 4.555).abs() < 0.01, "+1 侧圆心角 {a2}（模板 4.555）");
        let sd = s.to_degrees();
        assert!(((a - sd) + (a2 - sd)).abs() < 0.02, "两侧应对称");
    }

    #[test]
    fn internal_front_entity_count_and_chain_continuity() {
        let p = int_tmpl();
        let part = generate(&p, GearView::Front, 1.0).unwrap();
        let n_geo = part.entities.iter().filter(|e| !matches!(e, EntityType::Line(_))).count();
        assert_eq!(n_geo, p.z as usize * 8, "每齿 8 图元（2 样条 + 2 圆角弧 + 2 齿根弧 + 2 齿顶弧）");
        let pts = internal_chain_points(&p, 6).unwrap();
        let ra = p.da() / 2.0;
        let rf = p.df() / 2.0;
        for w in pts.windows(2) {
            let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
            assert!(d < 0.8, "链断了：{:.3}mm @ {:?}→{:?}", d, w[0], w[1]);
        }
        for q in &pts {
            let r = (q[0] * q[0] + q[1] * q[1]).sqrt();
            assert!(r > ra - 1e-6 && r < rf + 1e-6, "半径越界 {r}（应在 {ra}..{rf}）");
        }
    }

    #[test]
    fn internal_small_z_tip_below_base_degrades_with_warning() {
        // z ≤ 33 的内齿轮齿顶圆低于基圆（渐开线下不去）→ 简化画法：径向直线 + 提示
        for z in [33u32, 24, 17] {
            let p = GearParams { kind: GearKind::Internal, m: 10.0, z, h: 60.0, ..GearParams::default() };
            assert!(p.internal_tip_falls_below_base(), "z={z} 应低于基圆");
            let part = generate(&p, GearView::Front, 1.0).unwrap();
            let n_line = part.entities.iter().filter(|e| matches!(e, EntityType::Line(_))).count();
            assert!(n_line >= p.z as usize * 2 + 2, "z={z} 应每齿多 2 条径向直线，实得 {n_line}");
            assert!(
                p.notes().iter().any(|s| s.contains("低于基圆")),
                "z={z} 必须给提示（不能默默出图）"
            );
            // 采样要够密：模数 10 时齿廓有 ~18mm，seg=6 只有 18 个采样点（step 1.5mm），
            // 会被下面的 0.9mm 阈值误判成"断链" —— 那是采样问题，不是几何问题。
            let pts = internal_chain_points(&p, 24).unwrap();
            let ra = p.da() / 2.0;
            let rf = p.df() / 2.0;
            for (wi, w) in pts.windows(2).enumerate() {
                let d = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
                let r0 = (w[0][0] * w[0][0] + w[0][1] * w[0][1]).sqrt();
                let r1 = (w[1][0] * w[1][0] + w[1][1] * w[1][1]).sqrt();
                let a0 = w[0][1].atan2(w[0][0]).to_degrees();
                let a1 = w[1][1].atan2(w[1][0]).to_degrees();
                assert!(d < 0.9, "z={z} 链断了 @idx {wi}/{}：{d:.3}mm  r {r0:.3}→{r1:.3}  θ {a0:.3}→{a1:.3}", pts.len());
            }
            for q in &pts {
                let r = (q[0] * q[0] + q[1] * q[1]).sqrt();
                assert!(r > ra - 1e-6 && r < rf + 1e-6, "z={z} 半径越界 {r}");
            }
        }
    }

    #[test]
    fn internal_section_is_template_block_no_hatch() {
        // 用户「内齿轮使用示例.dxf」里的块 = 23 条线（20 轮廓 + 3 中心线），无剖面线
        let p = int_tmpl();
        let part = generate(&p, GearView::Section, 1.0).unwrap();
        assert_eq!(part.entities.len(), 23);
        assert!(part.entities.iter().all(|e| matches!(e, EntityType::Line(_))), "不能有 HATCH/圆弧");
        assert_eq!(
            part.entities
                .iter()
                .filter(|e| matches!(e, EntityType::Line(l) if l.common.layer == LAYER_CENTER))
                .count(),
            3,
            "分度线 ×2 + 轴线 ×1"
        );
        let rf = p.df() / 2.0;
        let roots: Vec<f64> = part
            .entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Line(l) if (l.start.y.abs() - rf).abs() < 1e-9 => Some((l.end.x - l.start.x).abs()),
                _ => None,
            })
            .collect();
        assert_eq!(roots.len(), 2, "上下齿根线各一条");
        for len in roots {
            assert!((len - p.h).abs() < 1e-9, "齿根线长 {len} ≠ h={}", p.h);
        }
        let ra = p.da() / 2.0;
        let tips: Vec<f64> = part
            .entities
            .iter()
            .filter_map(|e| match e {
                EntityType::Line(l)
                    if (l.start.y.abs() - ra).abs() < 1e-9
                        && (l.end.y.abs() - ra).abs() < 1e-9 =>
                {
                    Some((l.end.x - l.start.x).abs())
                }
                _ => None,
            })
            .collect();
        assert_eq!(tips.len(), 2);
        for len in tips {
            assert!(
                (len - (p.h - 2.0 * p.chamfer())).abs() < 1e-9,
                "齿顶线长 {len}（模板 28 = h − 2C）"
            );
        }
    }

    #[test]
    fn internal_views_without_template_are_rejected() {
        let p = int_tmpl();
        for v in [GearView::Side, GearView::Simplified] {
            let e = generate(&p, v, 1.0).unwrap_err();
            assert!(e.contains("模板"), "{v:?} 的报错应说明模板没有这个视图：{e}");
        }
    }

    #[test]
    fn internal_block_name_parse_and_notes() {
        let p = int_tmpl();
        assert_eq!(block_name(&p, GearView::Front), "OCSM_GEAR_INT_M2_Z40_H30_FRONT");
        assert_eq!(block_name(&p, GearView::Section), "OCSM_GEAR_INT_M2_Z40_H30_SECTION");
        // 外齿轮块名不能变（一期兼容）
        assert_eq!(block_name(&tmpl(), GearView::Front), "OCSM_GEAR_M2_Z40_H20_FRONT");
        assert_eq!(parse_request("int 2 40 30 view 端视图").unwrap().params.kind, GearKind::Internal);
        assert_eq!(parse_request("内齿轮 2 40").unwrap().params.kind, GearKind::Internal);
        assert_eq!(parse_request("m=2 z=40 kind=internal").unwrap().params.kind, GearKind::Internal);
        assert_eq!(parse_request("2 40 20").unwrap().params.kind, GearKind::External);
        // 默认齿宽：内齿轮 15m（模板 30 = 15×2）、外齿轮 10m（模板 20 = 10×2）
        assert!((parse_request("int 2 40").unwrap().params.h - 30.0).abs() < 1e-9);
        assert!((parse_request("2 40").unwrap().params.h - 20.0).abs() < 1e-9);
        // 提示语必须告诉人"剖视图没打剖面线、齿圈外壁留给用户"
        assert!(p.notes().iter().any(|s| s.contains("剖面线") && s.contains("齿圈")));
    }

    #[test]
    fn internal_query_params_and_info() {
        let (p, v, _) = params_from_query("kind=internal&m=2&z=40&view=front").unwrap();
        assert_eq!(p.kind, GearKind::Internal);
        assert_eq!(v, GearView::Front);
        assert!((p.h - 30.0).abs() < 1e-9, "内齿轮默认齿宽 15m={}", p.h);
        let j = info_json("kind=internal&m=2&z=40").unwrap();
        assert!(j.contains("\"kind\":\"internal\""), "{j}");
        assert!(j.contains("\"da\":76"), "{j}");
        assert!(j.contains("\"df\":85"), "{j}");
    }
}
