//! **孔生成器螺纹类型表**：公制 M（ISO 724）/ 美制统一 UN（ASME B1.1）/
//! 管用平行 G（ISO 228-1，BSPP/PF）/ 管用锥形 R（ISO 7-1，BSPT/PT）/
//! 美制锥管 NPT（ASME B1.20.1）/ 美制梯形 ACME（ASME B1.5）/ 公制梯形 Tr（ISO 2901）。
//!
//! 每类一份数据文件（`tables/thread<Type>.json`，与既有 `threadIso724.json` 同样的
//! `source`/`note` + 逐行基本尺寸结构）；**与公制 M 那套分开管理**，M 的几何仍走
//! `hole.rs` 既有 `thread_row()`（不改变既有 M 行为）。
//!
//! ## 单位口径（内部统一 mm）
//! - 所有 `d/d2/d1/drill` 均为 **mm**；1 in = 25.4 mm；
//! - 英制系列存 `tpi`（每英寸牙数）并同时给出 `p`（螺距 mm，`p = 25.4/tpi`）；
//! - 管螺纹（G/R/NPT）的「大径」是**螺纹大径**（G/R 为管子外径；NPT 为内螺纹
//!   基本大径），**不是公称通径**——见各文件 `note`。

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// 螺纹体系（孔生成器「标准」下拉）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadSystem {
    /// 公制 ISO 724（既有 M 表，行为不变）。
    #[default]
    Iso724,
    /// 美制统一 ASME B1.1（UNC/UNF/UNEF + 定距系列）。
    Un,
    /// 管用平行 ISO 228-1（G / BSPP / PF）。
    G,
    /// 管用锥形 ISO 7-1（R / BSPT / PT）。
    R,
    /// 美制锥管 ASME B1.20.1（NPT）。
    Npt,
    /// 美制梯形 ASME B1.5（ACME / Stub ACME）。
    Acme,
    /// 公制梯形 ISO 2901 / GB/T 5796（Tr）。
    Tr,
}

impl ThreadSystem {
    /// 「标准」下拉顺序。
    pub const ALL: [ThreadSystem; 7] = [
        ThreadSystem::Iso724,
        ThreadSystem::Un,
        ThreadSystem::G,
        ThreadSystem::R,
        ThreadSystem::Npt,
        ThreadSystem::Acme,
        ThreadSystem::Tr,
    ];

    /// 模型/GUI 用的稳定 key（serde 同值）。
    pub fn key(self) -> &'static str {
        match self {
            ThreadSystem::Iso724 => "iso724",
            ThreadSystem::Un => "un",
            ThreadSystem::G => "g",
            ThreadSystem::R => "r",
            ThreadSystem::Npt => "npt",
            ThreadSystem::Acme => "acme",
            ThreadSystem::Tr => "tr",
        }
    }

    /// 规格代号（显示用）。
    pub fn code(self) -> &'static str {
        match self {
            ThreadSystem::Iso724 => "M",
            ThreadSystem::Un => "UN",
            ThreadSystem::G => "G",
            ThreadSystem::R => "R",
            ThreadSystem::Npt => "NPT",
            ThreadSystem::Acme => "ACME",
            ThreadSystem::Tr => "Tr",
        }
    }

    /// 下拉显示名。
    pub fn label(self) -> &'static str {
        match self {
            ThreadSystem::Iso724 => "公制 M（ISO 724 / GB/T 196）",
            ThreadSystem::Un => "美制统一 UN（ASME B1.1 / GB/T 20666~20670）",
            ThreadSystem::G => "管用平行 G（ISO 228-1 / GB/T 7307，BSPP/PF）",
            ThreadSystem::R => "管用锥形 R（ISO 7-1 / GB/T 7306，BSPT/PT）",
            ThreadSystem::Npt => "美制锥管 NPT（ASME B1.20.1 / GB/T 12716）",
            ThreadSystem::Acme => "美制梯形 ACME（ASME B1.5，29°）",
            ThreadSystem::Tr => "公制梯形 Tr（ISO 2901 / GB/T 5796，30°）",
        }
    }

    /// 标准号（来源列）。
    pub fn standard(self) -> &'static str {
        match self {
            ThreadSystem::Iso724 => "ISO 724 / GB/T 196",
            ThreadSystem::Un => "ASME B1.1 / GB/T 20666~20670",
            ThreadSystem::G => "ISO 228-1 / GB/T 7307",
            ThreadSystem::R => "ISO 7-1 / GB/T 7306",
            ThreadSystem::Npt => "ASME B1.20.1 / GB/T 12716",
            ThreadSystem::Acme => "ASME B1.5",
            ThreadSystem::Tr => "ISO 2901 / GB/T 5796",
        }
    }

    /// 牙山角（度）。
    pub fn angle_deg(self) -> f64 {
        match self {
            ThreadSystem::Iso724 | ThreadSystem::Un | ThreadSystem::Npt => 60.0,
            ThreadSystem::G | ThreadSystem::R => 55.0,
            ThreadSystem::Acme => 29.0,
            ThreadSystem::Tr => 30.0,
        }
    }

    /// 是否管螺纹（大径是管子/螺纹外径概念，GUI 里要特别说明）。
    pub fn is_pipe(self) -> bool {
        matches!(self, ThreadSystem::G | ThreadSystem::R | ThreadSystem::Npt)
    }

}

/// 一行螺纹规格（内部统一 mm）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ThreadSpec {
    /// 显示名（如 `#1-64 UNC`、`G1/8`、`Tr8×1.5`、`M10×1.5`）。
    pub name: String,
    /// 大径 D/d（mm）。
    pub d: f64,
    /// 螺距 P（mm；英制系列由 `25.4/tpi` 换算）。
    pub p: f64,
    /// 每英寸牙数（英制系列；公制为 None）。
    #[serde(default)]
    pub tpi: Option<f64>,
    /// 基本中径 D2/d2（mm）。
    pub d2: f64,
    /// 内螺纹基本小径 D1/d1（mm）。
    pub d1: f64,
    /// 底孔（攻丝钻）直径（mm）；缺省 = 按基本小径 D1 兜底。
    #[serde(default)]
    pub drill: Option<f64>,
    /// 管螺纹基准长度 / 基准距离 L1（mm，表值；G 无此列 = None）。
    #[serde(default)]
    pub gauge_len: Option<f64>,
    /// 管螺纹啮合长度列（mm）：
    /// - NPT：ASME B1.20.1 **L1 手拧合长度**（hand-tight engagement）；
    /// - R：ISO 7-1 **基准距离（基本）**；
    /// - G：ISO 228-1 无此口径 → 本表填同规格 R 的 `l3`（有效螺纹长度，**待用户确认**）。
    #[serde(default)]
    pub l1: Option<f64>,
    /// 管螺纹第二长度列（mm）：NPT = ASME **L2 扳手拧合**（GB/T 12716 表列名「装配距离 L3」）；
    /// R = ISO 7-1 **装配余量**；G = None。
    #[serde(default)]
    pub l2: Option<f64>,
    /// 管螺纹总有效长度（mm）：NPT = **L1+L2（ASME L3）**；R = **外螺纹有效螺纹长度**（表值，= l1+l2）；G = None。
    #[serde(default)]
    pub l3: Option<f64>,
}

impl ThreadSpec {
    /// 底孔径（无推荐钻径时 = 基本小径 D1，明确不插值）。
    pub fn drill_mm(&self) -> f64 {
        self.drill.unwrap_or(self.d1)
    }
}

/// 一个牙型系列（如 UNC / 细牙 / Stub ACME）。
#[derive(Debug, Clone)]
pub struct ThreadGroup {
    pub key: &'static str,
    pub label: &'static str,
    pub rows: Vec<ThreadSpec>,
}

/// 一类螺纹的全部数据。
#[derive(Debug, Clone)]
pub struct ThreadSystemTable {
    pub source: String,
    pub note: String,
    pub units: String,
    pub groups: Vec<ThreadGroup>,
}

#[derive(Debug, serde::Deserialize)]
struct TableFile {
    source: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    units: String,
    /// 其余顶层键 = 组名 → 行（与 threadIso724.json 的 coarse/fine 同构）。
    #[serde(flatten)]
    groups: BTreeMap<String, Vec<ThreadSpec>>,
}

/// 组的规范顺序与显示名（数据文件里不一定有顺序；缺组会 panic，防数据腐化）。
fn group_order(sys: ThreadSystem) -> &'static [(&'static str, &'static str)] {
    match sys {
        ThreadSystem::Iso724 => &[("coarse", "粗牙"), ("fine", "细牙")],
        ThreadSystem::Un => &[
            ("unc", "UNC 粗牙"),
            ("unf", "UNF 细牙"),
            ("unef", "UNEF 超细牙"),
            ("un4", "4UN 定距"),
            ("un6", "6UN 定距"),
            ("un8", "8UN 定距"),
            ("un12", "12UN 定距"),
            ("un16", "16UN 定距"),
            ("un20", "20UN 定距"),
            ("un28", "28UN 定距"),
            ("un32", "32UN 定距"),
        ],
        ThreadSystem::Acme => &[("general", "一般用途 ACME"), ("stub", "矮牙 Stub ACME")],
        ThreadSystem::G | ThreadSystem::R | ThreadSystem::Npt | ThreadSystem::Tr => {
            &[("standard", "标准")]
        }
    }
}

fn parse(sys: ThreadSystem, json: &'static str) -> ThreadSystemTable {
    let f: TableFile = serde_json::from_str(json)
        .unwrap_or_else(|e| panic!("{} 表解析失败：{e}", sys.key()));
    let mut groups = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (key, label) in group_order(sys) {
        if let Some(rows) = f.groups.get(*key) {
            seen.insert(*key);
            groups.push(ThreadGroup {
                key,
                label,
                rows: rows.clone(),
            });
        }
    }
    if groups.is_empty() {
        panic!("{} 表没有任何规范组", sys.key());
    }
    let extra: Vec<&str> = f
        .groups
        .keys()
        .map(|s| s.as_str())
        .filter(|k| !seen.contains(k))
        .collect();
    if !extra.is_empty() {
        panic!("{} 表出现未登记的组：{extra:?}", sys.key());
    }
    ThreadSystemTable {
        source: f.source,
        note: f.note,
        units: f.units,
        groups,
    }
}

fn table_for(sys: ThreadSystem) -> &'static ThreadSystemTable {
    macro_rules! static_table {
        ($name:ident, $sys:expr, $file:literal) => {{
            static T: OnceLock<ThreadSystemTable> = OnceLock::new();
            T.get_or_init(|| parse($sys, include_str!($file)))
        }};
    }
    match sys {
        ThreadSystem::Iso724 => static_table!(M, ThreadSystem::Iso724, "tables/threadIso724.json"),
        ThreadSystem::Un => static_table!(U, ThreadSystem::Un, "tables/threadUn.json"),
        ThreadSystem::G => static_table!(G, ThreadSystem::G, "tables/threadG.json"),
        ThreadSystem::R => static_table!(R, ThreadSystem::R, "tables/threadR.json"),
        ThreadSystem::Npt => static_table!(N, ThreadSystem::Npt, "tables/threadNpt.json"),
        ThreadSystem::Acme => static_table!(A, ThreadSystem::Acme, "tables/threadAcme.json"),
        ThreadSystem::Tr => static_table!(T, ThreadSystem::Tr, "tables/threadTr.json"),
    }
}

/// 取某体系的全部数据（OnceLock 缓存）。
pub fn table(sys: ThreadSystem) -> &'static ThreadSystemTable {
    table_for(sys)
}

/// 查规格（纯查表，不插值、不外推）：
/// - `group` 为 None/空 = 在该体系所有组里找；
/// - `d` 为公称/大径（mm）；`pitch` 为螺距（mm，英制由 GUI 换算后传入）；
/// - 同一 `d` 命中多行且未给 `pitch` → 明确报错要求指定子类型/螺距。
pub fn lookup(
    sys: ThreadSystem,
    group: Option<&str>,
    d: f64,
    pitch: Option<f64>,
) -> Result<&'static ThreadSpec, String> {
    let t = table(sys);
    let groups: Vec<&ThreadGroup> = match group.filter(|g| !g.is_empty()) {
        Some(g) => t
            .groups
            .iter()
            .filter(|x| x.key == g)
            .collect(),
        None => t.groups.iter().collect(),
    };
    if groups.is_empty() {
        let keys: Vec<&str> = t.groups.iter().map(|g| g.key).collect();
        return Err(format!(
            "{} 没有子类型「{}」（可用：{}）",
            sys.code(),
            group.unwrap_or(""),
            keys.join("/")
        ));
    }
    let mut hits: Vec<&ThreadSpec> = Vec::new();
    for g in &groups {
        for r in &g.rows {
            if (r.d - d).abs() < 1e-6 && pitch.is_none_or(|p| (r.p - p).abs() < 1e-6) {
                hits.push(r);
            }
        }
    }
    match hits.len() {
        1 => Ok(hits[0]),
        0 => {
            let sample: Vec<String> = groups
                .iter()
                .flat_map(|g| g.rows.iter())
                .take(6)
                .map(|r| r.name.clone())
                .collect();
            Err(format!(
                "{} 表里没有 {}（mm, P={}）—— 表外不插值；表内示例：{}",
                sys.code(),
                crate::hole::fmt3(d),
                pitch.map(crate::hole::fmt3).unwrap_or_else(|| "粗牙".into()),
                sample.join("、")
            ))
        }
        _ => {
            let names: Vec<String> = hits.iter().map(|r| r.name.clone()).collect();
            Err(format!(
                "{} {} 命中多行（{}）—— 请指定子类型/螺距",
                sys.code(),
                crate::hole::fmt3(d),
                names.join("、")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p_of(r: &ThreadSpec) -> f64 {
        if let Some(tpi) = r.tpi {
            25.4 / tpi
        } else {
            r.p
        }
    }

    /// 逐类规格数（数据文件体量门禁）。
    #[test]
    fn per_type_spec_counts() {
        let m = table(ThreadSystem::Iso724);
        assert_eq!(m.groups[0].rows.len(), 40, "M 粗牙");
        assert_eq!(m.groups[1].rows.len(), 171, "M 细牙");
        let un = table(ThreadSystem::Un);
        let counts: Vec<(&str, usize)> = un.groups.iter().map(|g| (g.key, g.rows.len())).collect();
        assert_eq!(
            counts,
            vec![
                ("unc", 33),
                ("unf", 24),
                ("unef", 25),
                ("un4", 21),
                ("un6", 41),
                ("un8", 48),
                ("un12", 50),
                ("un16", 57),
                ("un20", 29),
                ("un28", 18),
                ("un32", 10),
            ]
        );
        assert_eq!(table(ThreadSystem::G).groups[0].rows.len(), 24, "G");
        assert_eq!(table(ThreadSystem::R).groups[0].rows.len(), 15, "R");
        assert_eq!(table(ThreadSystem::Npt).groups[0].rows.len(), 24, "NPT");
        assert_eq!(table(ThreadSystem::Acme).groups[0].rows.len(), 23, "ACME general");
        assert_eq!(table(ThreadSystem::Acme).groups[1].rows.len(), 23, "ACME stub");
        assert_eq!(table(ThreadSystem::Tr).groups[0].rows.len(), 236, "Tr");
    }

    /// 60° 系列（UN）：D2 = D − 0.649519P、D1 = D − 1.082532P；P = 25.4/TPI。
    #[test]
    fn un_basic_profile_relations() {
        for g in &table(ThreadSystem::Un).groups {
            for r in &g.rows {
                let p = p_of(r);
                let tpi = r.tpi.expect("UN 行应有 TPI");
                assert!((p * tpi - 25.4).abs() < 1e-3, "{} P·TPI≠25.4", r.name);
                assert!(
                    (r.d2 - (r.d - 0.649_519 * p)).abs() < 0.006,
                    "{} D2 关系式不符",
                    r.name
                );
                assert!(
                    (r.d1 - (r.d - 1.082_532 * p)).abs() < 0.006,
                    "{} D1 关系式不符",
                    r.name
                );
                assert!(r.d1 < r.d2 && r.d2 < r.d, "{} 直径序错", r.name);
                // 底孔径：攻丝钻应落在小径附近（D−P 经验式与钻表交叉）
                let drill = r.drill_mm();
                assert!(
                    (drill - (r.d - p)).abs() < 0.4,
                    "{} 底孔 {drill} 偏离 D−P",
                    r.name
                );
                assert!(drill < r.d, "{} 底孔不小于大径", r.name);
            }
        }
        // 用户点名规格的样例（交叉参考站/GB/T 20668）
        let s = lookup(ThreadSystem::Un, Some("unc"), 6.35, Some(1.27)).unwrap();
        assert_eq!(s.name, "1/4-20 UNC");
        assert_eq!((s.d2 * 1000.0).round() / 1000.0, 5.525);
        assert_eq!((s.d1 * 1000.0).round() / 1000.0, 4.976);
        let s = lookup(ThreadSystem::Un, Some("unc"), 1.8542, Some(25.4 / 64.0)).unwrap();
        assert_eq!(s.name, "#1-64 UNC");
    }

    /// 55° 惠氏（G/R）：D2 = D − 0.640327P、D1 = D − 1.280654P。
    #[test]
    fn g_r_whitworth_relations() {
        for sys in [ThreadSystem::G, ThreadSystem::R] {
            for g in &table(sys).groups {
                for r in &g.rows {
                    let p = p_of(r);
                    assert!((p * r.tpi.unwrap() - 25.4).abs() < 1e-3, "{}", r.name);
                    assert!(
                        (r.d2 - (r.d - 0.640_327 * p)).abs() < 0.004,
                        "{} D2",
                        r.name
                    );
                    assert!(
                        (r.d1 - (r.d - 1.280_654 * p)).abs() < 0.004,
                        "{} D1",
                        r.name
                    );
                    assert!(r.d1 < r.d2 && r.d2 < r.d, "{} 直径序错", r.name);
                    // 底孔径：G 有攻丝钻；R 沿用同规格 G。与小径差（参考站钻径取整）应 < 0.7mm
                    assert!(
                        (r.drill_mm() - r.d1).abs() < 0.7,
                        "{} 底孔偏离小径",
                        r.name
                    );
                    assert!(r.drill_mm() < r.d, "{} 底孔不小于大径", r.name);
                }
            }
        }
        let s = lookup(ThreadSystem::G, None, 9.728, None).unwrap();
        assert_eq!(s.name, "G1/8");
        assert_eq!(s.drill, Some(8.7));
        let s = lookup(ThreadSystem::R, None, 9.728, None).unwrap();
        assert_eq!(s.name, "R1/8");
        assert_eq!(s.gauge_len, Some(3.97));
    }

    /// NPT（60° 截顶）：D2 = D − 0.8P、D1 = D − 1.6P；底孔 = GB/T 12716 末列。
    #[test]
    fn npt_basic_profile_relations() {
        for r in &table(ThreadSystem::Npt).groups[0].rows {
            let p = p_of(r);
            assert!((p * r.tpi.unwrap() - 25.4).abs() < 1e-3, "{}", r.name);
            assert!((r.d2 - (r.d - 0.8 * p)).abs() < 0.004, "{} D2", r.name);
            assert!((r.d1 - (r.d - 1.6 * p)).abs() < 0.004, "{} D1", r.name);
            assert!(r.d1 < r.d2 && r.d2 < r.d, "{} 直径序错", r.name);
            assert!(r.drill.is_some(), "{} 缺底孔径", r.name);
            assert!(r.gauge_len.is_some(), "{} 缺基准距离", r.name);
        }
        let s = lookup(ThreadSystem::Npt, None, 21.224, None).unwrap();
        assert_eq!(s.name, "NPT1/2");
        assert_eq!(s.drill, Some(17.813));
        assert_eq!(s.d1, 18.321);
    }

    /// ACME：general = D−0.5P/D−P；stub = D−0.3P/D−0.6P（ASME B1.5）。
    #[test]
    fn acme_relations_general_and_stub() {
        let t = table(ThreadSystem::Acme);
        for (g, k2, k1) in [(&t.groups[0], 0.5, 1.0), (&t.groups[1], 0.3, 0.6)] {
            for r in &g.rows {
                let p = p_of(r);
                assert!(
                    (r.d2 - (r.d - k2 * p)).abs() < 0.002,
                    "{} D2={}",
                    r.name,
                    r.d2
                );
                assert!(
                    (r.d1 - (r.d - k1 * p)).abs() < 0.002,
                    "{} D1={}",
                    r.name,
                    r.d1
                );
                assert!((r.drill_mm() - r.d1).abs() < 1e-6, "{} 底孔=小径", r.name);
            }
        }
        // 参考站 ACME = Stub 尺寸（1/4-16：中径 5.874 / 小径 5.398）
        let stub = lookup(ThreadSystem::Acme, Some("stub"), 6.35, None).unwrap();
        assert_eq!((stub.d2 * 1000.0).round() / 1000.0, 5.874);
        assert_eq!((stub.d1 * 1000.0).round() / 1000.0, 5.398);
        let gen = lookup(ThreadSystem::Acme, Some("general"), 6.35, None).unwrap();
        assert!((gen.d2 - 5.556).abs() < 0.001);
    }

    /// Tr（30°）：D2 = d − 0.5P、D1 = d − P；底孔 = D1。
    #[test]
    fn tr_basic_profile_relations() {
        for r in &table(ThreadSystem::Tr).groups[0].rows {
            assert!(
                (r.d2 - (r.d - 0.5 * r.p)).abs() < 1e-9,
                "{} D2",
                r.name
            );
            assert!((r.d1 - (r.d - r.p)).abs() < 1e-9, "{} D1", r.name);
            assert!((r.drill_mm() - r.d1).abs() < 1e-9, "{} 底孔", r.name);
            assert!(r.tpi.is_none(), "Tr 不应有 TPI");
        }
        let s = lookup(ThreadSystem::Tr, None, 8.0, Some(1.5)).unwrap();
        assert_eq!(s.name, "Tr8×1.5");
        assert_eq!(s.d1, 6.5);
    }

    /// 管螺纹长度列（用户裁定 ④）：NPT L1/L2/L3、R 基准距离/装配余量/有效长度、G 取同规格 R（待确认）。
    #[test]
    fn pipe_length_columns() {
        // NPT：l1 = ASME L1（手拧合）、l2 = 扳手拧合、l3 = L1+L2；底孔名称匹配回归（1 1/4、1 1/2）
        for r in &table(ThreadSystem::Npt).groups[0].rows {
            let (l1, l2, l3) = (r.l1.unwrap(), r.l2.unwrap(), r.l3.unwrap());
            assert_eq!(Some(l1), r.gauge_len, "{} gauge_len 应 = L1", r.name);
            assert!((l3 - (l1 + l2)).abs() < 0.002, "{} L3≠L1+L2", r.name);
            assert!(l1 > 0.0 && l2 > 0.0 && l3 > l1, "{}", r.name);
        }
        let s = lookup(ThreadSystem::Npt, None, 10.242, None).unwrap();
        assert!((s.l1.unwrap() - 4.102).abs() < 1e-9, "NPT1/8 L1");
        let s = lookup(ThreadSystem::Npt, None, 41.985, None).unwrap();
        assert_eq!(s.name, "NPT1 1/4");
        assert_eq!(s.drill, Some(37.785), "1 1/4 底孔应=GB/T 12716 末列");
        let s = lookup(ThreadSystem::Npt, None, 48.054, None).unwrap();
        assert_eq!(s.drill, Some(43.853), "1 1/2 底孔应=GB/T 12716 末列");
        // R：l1 = 基准距离、l2 = 装配余量、l3 = 有效螺纹长度（表值 ≈ l1+l2，取整）
        for r in &table(ThreadSystem::R).groups[0].rows {
            let (l1, l2, l3) = (r.l1.unwrap(), r.l2.unwrap(), r.l3.unwrap());
            assert!((l3 - (l1 + l2)).abs() < 0.11, "{} l3≠l1+l2（取整）", r.name);
            assert!(l3 > l1, "{}", r.name);
        }
        let s = lookup(ThreadSystem::R, None, 9.728, None).unwrap();
        assert_eq!(s.l3, Some(6.5), "R1/8 有效螺纹长度");
        assert_eq!(s.l1, Some(4.0));
        let s = lookup(ThreadSystem::R, None, 163.83, None).unwrap();
        assert_eq!(s.l3, Some(40.1));
        // G：ISO 228-1 无 L1 → l1 = 同规格 R 的 l3；无同规格 R 的 9 档为 None（自动时明确报错）
        let g = &table(ThreadSystem::G).groups[0].rows;
        let rmap: std::collections::BTreeMap<&str, f64> = table(ThreadSystem::R).groups[0]
            .rows
            .iter()
            .map(|r| (r.name.as_str(), r.l3.unwrap()))
            .collect();
        let mut missing = 0;
        for r in g {
            let key = format!("R{}", &r.name[1..]);
            match rmap.get(key.as_str()) {
                Some(l3) => assert_eq!(
                    r.l1.map(|v| (v * 1000.0).round() / 1000.0),
                    Some(*l3),
                    "{}",
                    r.name
                ),
                None => {
                    missing += 1;
                    assert_eq!(r.l1, None, "{} 应无 L1", r.name);
                }
            }
        }
        assert_eq!(missing, 9, "G 无同规格 R 的档数");
        let s = lookup(ThreadSystem::G, None, 9.728, None).unwrap();
        assert_eq!(s.l1, Some(6.5), "G1/8 l1 = R1/8 有效长度");
    }

    /// 表外规格必须明确报错（不插值、不外推）。
    #[test]
    fn out_of_table_reports_error() {
        for (sys, group, d, p) in [
            (ThreadSystem::Un, Some("unc"), 4.0, None),
            (ThreadSystem::Un, Some("unc"), 6.35, Some(9.999)),
            (ThreadSystem::G, None, 10.0, None),
            (ThreadSystem::R, None, 7.0, None),
            (ThreadSystem::Npt, None, 22.0, None),
            (ThreadSystem::Acme, Some("general"), 6.0, None),
            (ThreadSystem::Tr, None, 8.0, Some(1.0)),
        ] {
            let e = lookup(sys, group, d, p).unwrap_err();
            assert!(
                e.contains(sys.code()),
                "{sys:?} {d} 的报错不清：{e}"
            );
        }
        // 未知子类型
        assert!(lookup(ThreadSystem::Un, Some("nope"), 6.35, None).is_err());
        // 同一 d 多行且未给 P → 要求指定子类型/螺距
        assert!(lookup(ThreadSystem::Acme, None, 6.35, None)
            .unwrap_err()
            .contains("多行"));
    }

    /// 数据内部一致：所有行 d>0、d1>0、drill>0、名字不重复（同组内）。
    #[test]
    fn tables_are_well_formed() {
        for sys in ThreadSystem::ALL {
            let t = table(sys);
            assert!(!t.source.is_empty(), "{} 缺 source", sys.key());
            assert!(!t.note.is_empty(), "{} 缺 note", sys.key());
            for g in &t.groups {
                let mut names = std::collections::BTreeSet::new();
                for r in &g.rows {
                    assert!(r.d > 0.0 && r.p > 0.0 && r.d1 > 0.0, "{}", r.name);
                    assert!(names.insert(r.name.clone()), "{} 组内重名 {}", sys.key(), r.name);
                }
            }
            assert!(sys.angle_deg() > 0.0);
        }
    }
}
