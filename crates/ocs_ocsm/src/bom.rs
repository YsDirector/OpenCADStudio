//! `OCSMBOM` / `BOM`：明细表建表 / 刷新（扫全图 `OCSM_PART` → 聚合 → 替换语义重建行块）。
//!
//! 规则来自用户 2026-09-15 的演示图与定案（计划 §30）：
//!
//! * 明细表**自下而上**填写；首列贴标题栏（底边 `first_col_bottom`，默认 45），
//!   首列行数 `per_col_rows`（默认 26，**可被命令参数覆盖**）；
//! * 一列写满 → **在左边紧贴**另起一列（x −= 180 mm），**续列自带整套表头**，
//!   底边落到图框内下边线（`sheet_bottom`，默认 0，比首列低）；续列按各自可用高度自动排行数；
//! * 序号**全表连续**、每列自下而上；
//! * 所有列都放不下 → **不硬塞**：报错并提示换图幅 / 做多页明细表（用户定案：逻辑闭环）；
//! * 刷新 = **替换语义**：一次 `PushUndo` → 删掉所有带 `OCSM_BOM` 记录的旧表元 → 重建，
//!   所以 Ctrl+Z 一次可以整体撤销。
//!
//! 块来源：插件目录 `bom/`（`OCSM_BOM_DIR` 可覆盖）——
//! `OCSM_BOMHEAD.dwg` / `OCSM_BOMROW.dwg` 各是一份**单块文件**（模型空间即该块内容），
//! 走宿主 `import_frame_block`（缺了才导入，幂等）→ 拿回行块里 8 个 ATTDEF 的几何/样式契约。
//!
//! 配置文件：`bom/settings.json`（首次运行自动生成默认值）。

use std::path::{Path, PathBuf};

use ocs_plugin_api::host::acadrust::entities::{AttributeDefinition, AttributeEntity, Insert};
use ocs_plugin_api::host::acadrust::types::{Color, LineWeight, Vector3};
use ocs_plugin_api::host::acadrust::xdata::{ExtendedDataRecord, XDataValue};
use ocs_plugin_api::host::acadrust::{Entity, EntityType, Handle};
use ocs_plugin_api::host::{HostApi, ImportFrameBlockRequest};

pub(crate) const XDATA_BOM: &str = "OCSM_BOM";
/// 行块上的「数量锁」记录（用户手改数量后不应被同步覆盖）。
pub(crate) const LOCK_APP: &str = "OCSM_BOMLOCK";
/// 行块上的「上次导出数量」记录（`BOMXLSX` 写；导入侧"手改即锁"的比对基线）。
pub(crate) const EXPORTED_APP: &str = "OCSM_BOMXEXP";
pub(crate) const XDATA_PART: &str = "OCSM_PART";
pub(crate) const HEAD_BLOCK: &str = "OCSM_BOMHEAD";
pub(crate) const ROW_BLOCK: &str = "OCSM_BOMROW";
/// 块宽（与模板一致：列内线 0/11/48/81/92/127/138/150/180）。
pub(crate) const COL_W: f64 = 180.0;
pub(crate) const HEAD_H: f64 = 12.0;
pub(crate) const ROW_H: f64 = 8.0;
const LAYER_LINE: &str = "2细线层";
/// 8 个单元格 tag（顺序 = 行块 ATTDEF 的顺序，取值按 tag 匹配而不是按下标）。
pub(crate) const CELL_TAGS: [&str; 8] = ["序号", "图号", "名称", "数量", "材料", "单重", "总重", "备注"];

// ── 目录 / 配置 ────────────────────────────────────────────────────────────

/// 明细表文件夹：优先 `OCSM_BOM_DIR` 环境变量，否则插件安装目录下的 `bom/`
/// （与 `frame_dir()` / `parts_dir()` 同一套路）。
pub(crate) fn bom_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OCSM_BOM_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg == "--ocs-plugin-runner" {
            if let (Some(_socket), Some(lib)) = (args.next(), args.next()) {
                let p = PathBuf::from(lib);
                if let Some(dir) = p.parent() {
                    return dir.join("bom");
                }
            }
            break;
        }
    }
    PathBuf::from("bom")
}

/// 明细表配置（`bom/settings.json`）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct BomConfig {
    /// 首列行数（26 = 用户 2026-09-15 演示图的定义值；A3 首列其实能放 28 行）。
    pub per_col_rows: usize,
    /// 首列底边 y（贴标题栏顶）。
    pub first_col_bottom: f64,
    /// 首列左边界 x（表右边缘 = first_col_left + 180）。
    pub first_col_left: f64,
    /// 图框内下边线 y（续列底边）。
    pub sheet_bottom: f64,
    /// 图框内左边线 x（列不能再往左越过它）。
    pub sheet_left: f64,
    /// 图框内上边线 y（列顶不能越过它）。
    pub sheet_top: f64,
}

impl Default for BomConfig {
    fn default() -> Self {
        Self {
            per_col_rows: 26,
            first_col_bottom: 45.0,
            first_col_left: 210.0,
            sheet_bottom: 0.0,
            sheet_left: 0.0,
            sheet_top: 287.0,
        }
    }
}

fn config_path(dir: &Path) -> PathBuf {
    dir.join("settings.json")
}

/// 读配置；文件不存在时写出一份默认值（用户要求：默认值存在插件配置里）。
pub(crate) fn load_config(dir: &Path) -> BomConfig {
    let path = config_path(dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<BomConfig>(&text).unwrap_or_default(),
        Err(_) => {
            let cfg = BomConfig::default();
            let _ = save_config(dir, &cfg);
            cfg
        }
    }
}

pub(crate) fn save_config(dir: &Path, cfg: &BomConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(cfg).unwrap_or_else(|_| "{}".into());
    std::fs::write(config_path(dir), text)
}

// ── 聚合与排布（纯函数，可单测） ─────────────────────────────────────────

/// 一件零件（`OCSM_PART` 记录里我们关心的字段）。
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub(crate) struct PartMeta {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub spec: String,
    #[serde(default)]
    pub material: String,
    #[serde(default)]
    pub weight: String,
}

/// 明细表一行（聚合结果）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BomItem {
    pub code: String,
    pub name: String,
    pub spec: String,
    pub material: String,
    pub unit_weight: String,
    pub qty: usize,
}

/// 按 **代号 + 材料** 聚合，保持首次出现顺序；同名同料多视图会各自计数（报告里会点出来）。
pub(crate) fn aggregate(parts: &[PartMeta]) -> Vec<BomItem> {
    let mut out: Vec<BomItem> = Vec::new();
    for p in parts {
        let key = (p.code.trim(), p.material.trim());
        if let Some(it) = out
            .iter_mut()
            .find(|it| it.code == key.0 && it.material == key.1)
        {
            it.qty += 1;
            if it.name.is_empty() {
                it.name = p.name.clone();
            }
            if it.spec.is_empty() {
                it.spec = p.spec.clone();
            }
            continue;
        }
        out.push(BomItem {
            code: p.code.trim().to_string(),
            name: p.name.trim().to_string(),
            spec: p.spec.trim().to_string(),
            material: p.material.trim().to_string(),
            unit_weight: p.weight.trim().to_string(),
            qty: 1,
        });
    }
    out
}

/// 单件重量显示文本：去掉估算前缀（历史数据可能带 `≈`；设计重量本来就是估算值，
/// 用户 2026-09-15 定：**不写 ≈**）。
pub(crate) fn weight_text(s: &str) -> String {
    s.trim()
        .trim_start_matches(['≈', '~', '～', '=', ' '])
        .trim_start_matches('约')
        .trim_start()
        .to_string()
}

/// 单件重量字符串里的数字（`1.35` / `≈ 1.35 kg` → 1.35；老数据带 ≈ 也能解析）。
fn weight_number(s: &str) -> Option<f64> {
    let mut num = String::new();
    let mut seen_digit = false;
    for ch in s.chars() {
        if ch.is_ascii_digit() || (ch == '.' && seen_digit && !num.ends_with('.')) {
            num.push(ch);
            seen_digit = true;
        } else if seen_digit {
            break;
        } else if !matches!(ch, '≈' | '~' | '～' | ' ' | '=' | 'k' | 'g' | 'K' | 'G') {
            // 其它前缀字符（如「约」）跳过
        }
    }
    num.parse::<f64>().ok()
}

fn trim_num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".into() } else { s.to_string() }
}

/// 一行的 8 个单元格取值（tag 顺序见 [`CELL_TAGS`]）。
pub(crate) fn cell_values(it: &BomItem, seq: usize) -> [String; 8] {
    let unit = weight_text(&it.unit_weight);
    let total = weight_number(&unit)
        .map(|w| trim_num(w * it.qty.max(1) as f64))
        .unwrap_or_default();
    let name = if it.spec.is_empty() {
        it.name.clone()
    } else {
        format!("{} {}", it.name, it.spec)
    };
    [
        seq.to_string(),
        it.code.clone(),
        name,
        it.qty.to_string(),
        it.material.clone(),
        unit,
        total,
        String::new(),
    ]
}

// ── 单元格文字自动压缩（列宽自适应）────────────────────────────────────────

/// 压缩后的**最小字高**（再小就看不清了；宁可让它压到最小后允许轻微溢出并在命令行点名）。
const MIN_CELL_TEXT_H: f64 = 2.0;
/// **压缩目标 = 可用宽 × 这个系数**（故意留一手）。
///
/// 字宽模型对混排文本仍偏乐观（实测渲染比模型宽约 10~20%），所以不压到"刚好填满"，
/// 只用到可用宽的 **90%** —— 用户 2026-09-15 先要 80%（嫌字太小）、再定为 90%。
/// 调这个数就能整体加减压缩力度（0.8 边距更宽但字小、1.0 字最大但可能顶格）。
const FIT_TARGET_RATIO: f64 = 0.9;

/// 横向压缩的**告警阈值**：宽度因子压到名义值的这个比例以下就算"压得太扁"（只提醒，不阻止）。
///
/// 没有下限 —— 用户定案「宽度因子可以无限压缩」，保证文字**永远压进格内、绝不到邻格**。
const WARN_WIDTH_FACTOR: f64 = 0.7;
/// 单元格左右留白合计（mm）：可用宽度 = 列宽 − 这个值。
///
/// 2.5 mm = 每侧 1.25 mm。别调太小（1.0 时压缩后的文字会顶满格子，看着像压到邻格）。
const CELL_PADDING: f64 = 2.5;

/// 单字**自然宽度**（em 倍数）：中日韩 1.15、拉丁/数字/空格 0.8。
///
/// 按明细表模板（字型 `OCSM_GB`、字高 5、宽比 0.7）**在图上像素实测校准**：
/// 拉丁串 `GB/T 5780-2016` ≈ 0.73 em/字（模型 0.8 → 略保守），
/// 而中日韩混排（`六角头螺栓 C级 M8x35`）实测比 1.0 em 宽约 15% —— 所以取 **1.15**。
/// 宁可算宽一点（多压一点、留出边距），也别算窄（压不够就顶到格线）。
/// （引线/焊接标注那套 0.414/0.7 是另一个字型的比例，别混用。）
fn char_em(c: char) -> f64 {
    if (c as u32) >= 0x2E80 {
        1.15
    } else {
        0.8
    }
}

/// 文字在给定字高/宽度因子下的宽度。
fn cell_text_width(s: &str, h: f64, wf: f64) -> f64 {
    s.chars().map(|c| h * wf * char_em(c)).sum()
}

/// **单元格文字自动压缩**：先压字高到下限，再**无限**横向压宽度因子。
///
/// 顺序：① 字高按"刚好放得下"算（但不低于 [`MIN_CELL_TEXT_H`]）；
/// ② 若压到下限仍然放不下 → 横向压缩宽度因子，**不设下限**（用户定案：可以无限压），
/// 保证文字永远压进格内、绝不到邻格。
///
/// 返回 `(字高, 宽度因子, 是否仍溢出)`；短文本原样返回（溢出只在退化输入下可能出现）。
pub(crate) fn fit_cell_text(
    value: &str,
    avail: f64,
    nominal_h: f64,
    nominal_wf: f64,
) -> (f64, f64, bool) {
    let v = value.trim();
    let wf0 = if nominal_wf > 0.0 { nominal_wf } else { 1.0 };
    if v.is_empty() || nominal_h <= 0.0 || avail <= 0.0 {
        return (nominal_h, wf0, false);
    }
    // 目标宽度：只用可用宽的 80%（见 FIT_TARGET_RATIO）
    let target = avail * FIT_TARGET_RATIO;
    let need = cell_text_width(v, nominal_h, wf0);
    if need <= target + 1e-9 {
        return (nominal_h, wf0, false);
    }
    // ① 压字高（保底 MIN_CELL_TEXT_H）
    let h = (nominal_h * target / need).max(MIN_CELL_TEXT_H);
    if cell_text_width(v, h, wf0) <= target + 1e-9 {
        return (h, wf0, false);
    }
    // ② 还长 → 横向压缩（字高保底不动；宽度因子**不设下限**，压到目标宽为止）
    let need2 = cell_text_width(v, h, wf0);
    let wf = wf0 * target / need2;
    let overflow = cell_text_width(v, h, wf) > target + 1e-9;
    (h, wf, overflow)
}

/// 行块里**各列的边界 x**（块局部坐标）：取块内竖直细线的 x，排序去重。
///
/// 从模板几何里读，而不是写死列宽 —— 用户改模板（加宽图号列等）后仍然正确。
fn vertical_line_xs(entities: &[EntityType]) -> Vec<f64> {
    let mut xs: Vec<f64> = Vec::new();
    let mut push = |x: f64| {
        if !xs.iter().any(|v| (v - x).abs() < 1e-6) {
            xs.push(x);
        }
    };
    for e in entities {
        match e {
            EntityType::Line(l) => {
                if (l.start.x - l.end.x).abs() < 1e-9 && (l.start.y - l.end.y).abs() > 1e-9 {
                    push(l.start.x);
                }
            }
            EntityType::LwPolyline(pl) => {
                let n = pl.vertices.len();
                for i in 0..n {
                    let a = &pl.vertices[i];
                    let b = &pl.vertices[(i + 1) % n];
                    if (a.location.x - b.location.x).abs() < 1e-9
                        && (a.location.y - b.location.y).abs() > 1e-9
                    {
                        push(a.location.x);
                    }
                }
            }
            EntityType::Polyline(pl) => {
                let n = pl.vertices.len();
                for i in 0..n {
                    let a = &pl.vertices[i];
                    let b = &pl.vertices[(i + 1) % n];
                    if (a.location.x - b.location.x).abs() < 1e-9
                        && (a.location.y - b.location.y).abs() > 1e-9
                    {
                        push(a.location.x);
                    }
                }
            }
            _ => {}
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs
}

/// 每列的 (左边界, 可用宽) —— 由竖直分隔线差分得到；解析不出 8 段时退回模板标称值。
fn row_cell_widths(entities: &[EntityType]) -> Vec<(f64, f64)> {
    let xs = vertical_line_xs(entities);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for w in xs.windows(2) {
        let width = w[1] - w[0];
        if width > 1.0 {
            out.push((w[0], width - CELL_PADDING));
        }
    }
    if out.len() != CELL_TAGS.len() {
        // 模板里没有足够的分隔线 / 不是直线 → 用标称列宽兜底（0/11/48/81/92/127/138/150/180）
        const EDGES: [f64; 9] = [0.0, 11.0, 48.0, 81.0, 92.0, 127.0, 138.0, 150.0, 180.0];
        out = EDGES
            .windows(2)
            .map(|w| (w[0], w[1] - w[0] - CELL_PADDING))
            .collect();
    }
    out
}

/// 按 ATTDEF 的插入点 x 找它落在哪一列 → 返回可用宽（块局部坐标）。
fn avail_width_at(x: f64, cols: &[(f64, f64)]) -> Option<f64> {
    // 取"最后一个左边界 ≤ x"的列
    cols.iter()
        .rev()
        .find(|(left, _)| x >= *left - 0.5)
        .map(|(_, w)| *w)
}

// ── 序号驱动刷新（序号球标联动）───────────────────────────────────────────

/// 一行明细表的内容规格。**序号由外部给**（球标台账 / 自动接号 / 一期聚合行号）。
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RowSpec {
    pub item_no: String,
    pub code: String,
    pub name: String,
    pub spec: String,
    pub qty: usize,
    pub material: String,
    pub unit_weight: String,
    pub remark: String,
    /// 数量锁（用户手改过 → 锁定值）：Some 时同步**不重算数量**；
    /// 其它列不需要锁位——规则是"已存在的行非空即保留"（用户定案：无法解开的锁）。
    pub lock_qty: Option<usize>,
    /// 上次**导出到 xlsx** 时的数量（"手改即锁"的比对基线）；没导出过就是 None。
    pub exported: Option<usize>,
}

impl RowSpec {
    /// 由聚合件 + 指定序号构造。
    pub(crate) fn from_item(it: &BomItem, item_no: String) -> Self {
        RowSpec {
            item_no,
            code: it.code.clone(),
            name: it.name.clone(),
            spec: it.spec.clone(),
            qty: it.qty,
            material: it.material.clone(),
            unit_weight: it.unit_weight.clone(),
            remark: String::new(),
            lock_qty: None,
            exported: None,
        }
    }

    /// 空行（只有序号，其余待手填）——球标组里关联不到零件的条目走这个。
    pub(crate) fn blank(item_no: String) -> Self {
        RowSpec {
            item_no,
            ..Default::default()
        }
    }

    /// 8 个单元格取值（tag 顺序见 [`CELL_TAGS`]）。
    pub(crate) fn values(&self) -> [String; 8] {
        let unit = weight_text(&self.unit_weight);
        let empty = self.name.is_empty() && self.code.is_empty();
        let qty = self.qty.max(1);
        let total = weight_number(&unit)
            .map(|w| trim_num(w * qty as f64))
            .unwrap_or_default();
        let name = if self.spec.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, self.spec)
        };
        [
            self.item_no.clone(),
            self.code.clone(),
            name,
            if empty { String::new() } else { qty.to_string() },
            self.material.clone(),
            unit,
            total,
            self.remark.clone(),
        ]
    }
}

/// 建表/刷新需要的**最小写入能力**：命令路径（`HostApi`）与 HTTP 路径（`PluginRequestSender`）
/// 各一份实现，建行逻辑（[`fill_bom`]）只写一份。
pub(crate) trait BomSink {
    fn push_undo(&mut self, label: &str) -> Result<(), String>;
    fn remove_entity(&mut self, h: Handle) -> Result<(), String>;
    fn add_entities(&mut self, ents: Vec<EntityType>) -> Result<usize, String>;
    /// 导入块定义；**回传块内 ATTDEF**（宿主 import 的返回值就是这样，省得再取一次快照）。
    fn import_block(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<Vec<AttributeDefinition>, String>;
    fn set_dirty(&mut self) -> Result<(), String>;
    fn info(&mut self, msg: &str);
    fn error(&mut self, msg: &str);
}

/// 命令路径 sink。
pub(crate) struct HostSink<'a>(pub &'a mut dyn HostApi);

impl BomSink for HostSink<'_> {
    fn push_undo(&mut self, label: &str) -> Result<(), String> {
        self.0.push_undo(label);
        Ok(())
    }
    fn remove_entity(&mut self, h: Handle) -> Result<(), String> {
        self.0.remove_entity(h);
        Ok(())
    }
    fn add_entities(&mut self, ents: Vec<EntityType>) -> Result<usize, String> {
        Ok(self.0.add_entities(ents).len())
    }
    fn import_block(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<Vec<AttributeDefinition>, String> {
        self.0.import_frame_block(ImportFrameBlockRequest {
            path: path.to_string(),
            block_name: name.to_string(),
        })
    }
    fn set_dirty(&mut self) -> Result<(), String> {
        self.0.set_dirty();
        Ok(())
    }
    fn info(&mut self, msg: &str) {
        self.0.push_info(msg)
    }
    fn error(&mut self, msg: &str) {
        self.0.push_error(msg)
    }
}

/// 建表/刷新结果。
pub(crate) struct BomReport {
    pub rows: usize,
    pub cols: Vec<usize>,
    pub removed: usize,
    pub added: usize,
    pub untracked: usize,
    /// 被自动压缩（压字高/横向挤压）的单元格数。
    pub squeezed: usize,
    /// **横向压得偏扁**（宽度因子 < 名义 × 0.7）的单元格（"tag「值」42%"），供命令行点名。
    pub squeezed_hard: Vec<String>,
}

/// **建行核心**（命令 `BOM` 与球标联动共用）：按 `rows` 的次序重建全部行块，
/// 保留布局规则（自下而上、首列贴标题栏、写满另起一列、续列自带表头）。
pub(crate) fn fill_bom(
    sink: &mut dyn BomSink,
    doc: &ocs_plugin_api::host::acadrust::CadDocument,
    rows: &[RowSpec],
    per_col: usize,
) -> Result<BomReport, String> {
    let dir = bom_dir();
    let cfg = load_config(&dir);
    let head_dwg = dir.join("OCSM_BOMHEAD.dwg");
    let row_dwg = dir.join("OCSM_BOMROW.dwg");
    if !head_dwg.exists() || !row_dwg.exists() {
        return Err(format!(
            "OCSMBOM: 找不到明细表模板块（{} / {}）。请把 OCSM_BOMHEAD.dwg、OCSM_BOMROW.dwg \
             放进插件目录 bom/（或设 OCSM_BOM_DIR）。",
            head_dwg.display(),
            row_dwg.display()
        ));
    }

    // ① 表头块：缺了才导入（宿主 import 自己会 PushUndo）。
    if doc.block_records.get(HEAD_BLOCK).is_none() {
        let _ = sink.import_block(&head_dwg.to_string_lossy(), HEAD_BLOCK)?;
    }

    // ② 行块 + 它的 8 个 ATTDEF（几何 + 样式契约从模板来，插件不写死坐标）。
    //    注意：`doc` 是调用方给的**快照**，刚导入的块不在里面 —— 所以导入路径直接
    //    用宿主 import 的返回值（就是块内 ATTDEF），只有"块本来就在"时才读快照。
    let mut attdefs: Vec<AttributeDefinition> = doc
        .entities_in_block(ROW_BLOCK)
        .filter_map(|e| match e {
            EntityType::AttributeDefinition(ad) => Some(ad.clone()),
            _ => None,
        })
        .collect();
    if attdefs.is_empty() || doc.block_records.get(ROW_BLOCK).is_none() {
        attdefs = sink.import_block(&row_dwg.to_string_lossy(), ROW_BLOCK)?;
    }
    if attdefs.len() != CELL_TAGS.len() {
        return Err(format!(
            "OCSMBOM: 行块 {ROW_BLOCK} 应有 {} 个 ATTDEF，实际 {} 个 → 模板不对。",
            CELL_TAGS.len(),
            attdefs.len()
        ));
    }

    // ③ 扫图：旧表元（带 OCSM_BOM）+ 未入台账的 OCSM_ 零件块计数。
    let mut old: Vec<Handle> = Vec::new();
    let mut untracked = 0usize;
    for e in doc.model_space_entities() {
        let common = e.common();
        if is_bom(common) {
            old.push(common.handle);
        }
        if let EntityType::Insert(ins) = e {
            if part_meta_of(&ins.common).is_none()
                && ins.block_name.starts_with("OCSM_")
                && ins.block_name != HEAD_BLOCK
                && ins.block_name != ROW_BLOCK
            {
                untracked += 1;
            }
        }
    }

    let cols = layout(rows.len(), per_col, &cfg)?;
    // 单元格文字自动压缩：列宽从**行块几何**里读（模板改了也对）
    let row_block_entities: Vec<EntityType> = doc.entities_in_block(ROW_BLOCK).cloned().collect();
    let cell_cols = row_cell_widths(&row_block_entities);
    let mut squeezed = 0usize;
    let mut squeezed_hard: Vec<String> = Vec::new();

    // ④ 建表（表头 + 行，含 8 个属性值），全部打 OCSM_BOM 标记。
    let mut ents: Vec<EntityType> = Vec::new();
    let mut seq = 0usize;
    for (ci, nrows) in cols.iter().enumerate() {
        let x0 = cfg.first_col_left - COL_W * ci as f64;
        let y0 = if ci == 0 {
            cfg.first_col_bottom
        } else {
            cfg.sheet_bottom
        };
        let mut head = Insert::new(HEAD_BLOCK, Vector3::new(x0, y0, 0.0));
        {
            let c = insert_common(&mut head);
            c.layer = LAYER_LINE.to_string();
            c.color = Color::ByLayer;
            c.linetype = "ByLayer".to_string();
            c.line_weight = LineWeight::ByLayer;
            tag_bom(c);
        }
        ents.push(EntityType::Insert(head));

        for k in 0..*nrows {
            let yr = y0 + HEAD_H + ROW_H * k as f64;
            let mut ins = Insert::new(ROW_BLOCK, Vector3::new(x0, yr, 0.0));
            {
                let c = insert_common(&mut ins);
                c.layer = LAYER_LINE.to_string();
                c.color = Color::ByLayer;
                c.linetype = "ByLayer".to_string();
                c.line_weight = LineWeight::ByLayer;
            }
            let values = rows[seq].values();
            seq += 1;
            let transform = ins.get_transform();
            for ad in &attdefs {
                let Some(idx) = CELL_TAGS.iter().position(|t| *t == ad.tag) else {
                    continue;
                };
                let mut a = AttributeEntity::from_definition(ad, Some(values[idx].clone()));
                // 长文本自动压缩（按该 ATTDEF 所在列的列宽）；块局部坐标下判断列。
                if let Some(avail) = avail_width_at(ad.insertion_point.x, &cell_cols) {
                    let wf0 = if ad.width_factor.abs() < 1e-9 { 1.0 } else { ad.width_factor };
                    let (h, f, over) = fit_cell_text(&values[idx], avail, ad.height, wf0);
                    if (h - ad.height).abs() > 1e-9 || (f - wf0).abs() > 1e-9 {
                        squeezed += 1;
                    }
                    // 压得太扁（可读性下降）→ 点名，建议加宽列/缩短文本
                    if f < wf0 * WARN_WIDTH_FACTOR {
                        squeezed_hard.push(format!(
                            "{}「{}」{:.0}%",
                            ad.tag,
                            values[idx],
                            f / wf0 * 100.0
                        ));
                    }
                    let _ = over;
                    a.height = h;
                    a.width_factor = f;
                }
                a.apply_transform(&transform);
                ins.attributes.push(a);
            }
            tag_bom(insert_common(&mut ins));
            // 数量锁（用户手改过）→ 写进行块 XDATA，同步时就不会被重算覆盖。
            if let Some(q) = rows[seq - 1].lock_qty {
                let mut rec = ExtendedDataRecord::new(LOCK_APP);
                rec.values.push(XDataValue::String(
                    serde_json::json!({"qty": q}).to_string(),
                ));
                ins.common.extended_data.add_record(rec);
            }
            // 导出基线（`BOMXLSX` 时写下）：导入侧靠它判断"人手改过数量"
            if let Some(b) = rows[seq - 1].exported {
                let mut rec = ExtendedDataRecord::new(EXPORTED_APP);
                rec.values.push(XDataValue::String(
                    serde_json::json!({"qty": b}).to_string(),
                ));
                ins.common.extended_data.add_record(rec);
            }
            ents.push(EntityType::Insert(ins));
        }
    }

    // ⑤ 替换语义：一次 undo（删旧 + 建新），整体可 Ctrl+Z。
    sink.push_undo("OCSM 明细表")?;
    let removed = old.len();
    for h in old {
        sink.remove_entity(h)?;
    }
    let added = sink.add_entities(ents)?;
    sink.set_dirty()?;

    Ok(BomReport {
        rows: rows.len(),
        cols,
        removed,
        added,
        untracked,
        squeezed,
        squeezed_hard,
    })
}

/// 列布局：返回每列行数；列数放不下当前图幅时返回 Err（说明要换图幅/多页）。
pub(crate) fn layout(n_items: usize, per_col_rows: usize, cfg: &BomConfig) -> Result<Vec<usize>, String> {
    let cap = |bottom: f64| -> usize {
        let h = cfg.sheet_top - bottom - HEAD_H;
        if h < ROW_H {
            0
        } else {
            (h / ROW_H).floor() as usize
        }
    };
    let cap_first = cap(cfg.first_col_bottom);
    let cap_cont = cap(cfg.sheet_bottom);
    let fit_cols = {
        // 内框宽度放得下的列数（含首列）
        let usable = cfg.first_col_left + COL_W - cfg.sheet_left;
        if usable < COL_W {
            0
        } else {
            (usable / COL_W).floor() as usize
        }
    };
    if n_items == 0 {
        return Ok(vec![0]);
    }
    if cap_first == 0 {
        return Err("图框/配置不对：首列连一行都放不下（检查 sheet_top / first_col_bottom）".into());
    }
    let mut cols = Vec::new();
    let mut left = n_items;
    let first = left.min(per_col_rows.max(1)).min(cap_first);
    cols.push(first);
    left -= first;
    while left > 0 {
        if cap_cont == 0 {
            return Err("图框/配置不对：续列连一行都放不下（检查 sheet_top / sheet_bottom）".into());
        }
        let take = left.min(cap_cont);
        cols.push(take);
        left -= take;
    }
    if cols.len() > fit_cols {
        return Err(format!(
            "明细表需要 {} 列（共 {} 行：首列 {} 行/上限 {}，续列每列上限 {}），\
             当前图幅只放得下 {} 列 → 请换更大图幅，或改用多页明细表。",
            cols.len(),
            n_items,
            per_col_rows.max(1),
            cap_first,
            cap_cont,
            fit_cols
        ));
    }
    Ok(cols)
}

// ── 命令 ─────────────────────────────────────────────────────────────────

fn tag_bom(common: &mut ocs_plugin_api::host::acadrust::entities::EntityCommon) {
    let mut rec = ExtendedDataRecord::new(XDATA_BOM);
    rec.values.push(XDataValue::String("1".into()));
    common.extended_data.add_record(rec);
}

pub(crate) fn part_meta_of(common: &ocs_plugin_api::host::acadrust::entities::EntityCommon) -> Option<PartMeta> {
    let rec = common.extended_data.get_record(XDATA_PART)?;
    let text = rec.values.iter().find_map(|v| match v {
        XDataValue::String(s) => Some(s.clone()),
        _ => None,
    })?;
    serde_json::from_str::<PartMeta>(&text).ok()
}

fn is_bom(common: &ocs_plugin_api::host::acadrust::entities::EntityCommon) -> bool {
    common.extended_data.get_record(XDATA_BOM).is_some()
}

fn insert_common<'a>(
    ins: &'a mut Insert,
) -> &'a mut ocs_plugin_api::host::acadrust::entities::EntityCommon {
    &mut ins.common
}

/// `OCSMBOM [每列行数]` / `BOM [每列行数]`。
pub(crate) fn cmd_bom(host: &mut dyn HostApi, args: &str) {
    let dir = bom_dir();
    let cfg = load_config(&dir);
    let per_col = args
        .split_whitespace()
        .find_map(|t| t.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(cfg.per_col_rows);

    // 零件台账 → 聚合 → 行（**一期行为：序号 = 行号**，按台账首次出现顺序）。
    let doc = host.document().clone();
    let parts: Vec<PartMeta> = doc
        .model_space_entities()
        .filter_map(|e| match e {
            EntityType::Insert(ins) => part_meta_of(&ins.common),
            _ => None,
        })
        .collect();
    let items = aggregate(&parts);
    let rows: Vec<RowSpec> = items
        .iter()
        .enumerate()
        .map(|(i, it)| RowSpec::from_item(it, (i + 1).to_string()))
        .collect();

    match fill_bom(&mut HostSink(host), &doc, &rows, per_col) {
        Ok(rep) => {
            let desc: Vec<String> = rep
                .cols
                .iter()
                .enumerate()
                .map(|(i, n)| format!("第{}列 {} 行", i + 1, n))
                .collect();
            let _ = &rep;
            host.push_info(&format!(
                "OCSMBOM: {} 件 → {} 列（{}）；旧表元 {} 个已替换（Ctrl+Z 可整体撤销）。",
                rep.rows,
                rep.cols.len(),
                desc.join("、"),
                rep.removed
            ));
            report_cell_fit(host, &rep);
            if items.iter().any(|it| it.spec.is_empty()) && !items.is_empty() {
                host.push_info(
                    "OCSMBOM: 提示——数量按「图中插入件数」统计，同一零件画在多个视图里会重复计数。",
                );
            }
            if rep.untracked > 0 {
                host.push_info(&format!(
                    "OCSMBOM: 注意——图中有 {} 个 OCSM_ 零件块引用但没有 OCSM_PART 台账记录             （多为离线生成/早期版本的文件），它们不会进明细表。用 XL 重新放置，或后续用表格导入补录。",
                    rep.untracked
                ));
            }
        }
        Err(e) => host.push_error(&e),
    }
}

/// `OCSMBOMSYNC` / `BOMSYNC`：按**序号球标台账**重排/重建明细表（球标联动的手动入口）。
///
/// 规则（用户 2026-09-15 定）：行 = 序号（球标组每个条目一个）∪ 未被球标引用的聚合零件
/// （沿用它们在旧表里的序号，没表则自动接号）；行序按序号升序、自下而上。
pub(crate) fn cmd_bom_sync(host: &mut dyn HostApi, _args: &str) {
    let doc = host.document().clone();
    let rows = match crate::balloon_sync::plan_rows(&doc) {
        Ok(r) => r,
        Err(e) => {
            host.push_error(&format!("OCSMBOMSYNC: {e}"));
            return;
        }
    };
    if rows.is_empty() {
        host.push_error("OCSMBOMSYNC: 没有可排的内容（图上没有序号球标，也没有零件台账）。");
        return;
    }
    let per_col = 0; // 0 = 用配置默认（fill_bom 里 layout 的 per_col_rows）
    let cfg = load_config(&bom_dir());
    let per_col = if per_col == 0 { cfg.per_col_rows } else { per_col };
    match fill_bom(&mut HostSink(host), &doc, &rows, per_col) {
        Ok(rep) => {
            let nos: Vec<String> = rows.iter().map(|r| r.item_no.clone()).collect();
            host.push_info(&format!(
                "OCSMBOMSYNC: {} 行（序号 {}）→ {} 列；旧表元 {} 个已替换。",
                rep.rows,
                nos.join("、"),
                rep.cols.len(),
                rep.removed
            ));
            report_cell_fit(host, &rep);
            // 锁定/保留提示（不静默：用户要知道哪些行没被重算）
            let locked: Vec<String> = rows
                .iter()
                .filter(|r| r.lock_qty.is_some())
                .map(|r| format!("{}（数量 {}）", r.item_no, r.lock_qty.unwrap()))
                .collect();
            if !locked.is_empty() {
                host.push_info(&format!(
                    "OCSMBOMSYNC: {} 行数量已锁定，未覆盖：{}。要重算用 `BOMLOCK <序号> off`。",
                    locked.len(),
                    locked.join("、")
                ));
            }
        }
        Err(e) => host.push_error(&e),
    }
}

/// `BOMLOCK <序号|图号> [数量|off]`：给某一行**加/改/清数量锁**。
///
/// * 不给数量 → 用行上现值的数量锁上（"我手动改过，别覆盖"）；
/// * 给数量   → 把数量改成该值并锁上；
/// * `off`   → 解锁（下次同步按件数/引用次数重算）。
///
/// 只锁**数量**：序号永远由取号规则掌握；图号/名称/材料/单重/备注 是"非空即保留"
/// （用户定案：无法解开的锁），要重算就删掉那一行再同步。
/// 打印"单元格自动压缩"结果（有压缩才说；压到下限仍溢出的点名）。
fn report_cell_fit(host: &mut dyn HostApi, rep: &BomReport) {
    if rep.squeezed > 0 {
        host.push_info(&format!(
            "OCSMBOM: {} 格文字超宽 → 已自动压缩（先压字高到 {MIN_CELL_TEXT_H}，再横向压缩）。",
            rep.squeezed
        ));
    }
    if !rep.squeezed_hard.is_empty() {
        host.push_info(&format!(
            "OCSMBOM: 下面 {} 格横向压得偏扁（已压进格内、不会到邻格，但可读性下降）：{} \
             （建议加宽该列 / 缩短文本 / 把标准号写进名称列）。",
            rep.squeezed_hard.len(),
            rep.squeezed_hard.join("、")
        ));
    }
}

pub(crate) fn cmd_bom_lock(host: &mut dyn HostApi, args: &str) {
    let mut it = args.split_whitespace();
    let Some(key) = it.next() else {
        host.push_info(
            "OCSMBOMLOCK / BOMLOCK 用法：BOMLOCK <序号|图号> [数量|off]\
             （不给数量 = 按行上现值锁；off = 解锁）",
        );
        return;
    };
    let arg = it.next().map(|s| s.to_string());

    // 找那一行（按序号或图号匹配；先找序号）
    let doc = host.document().clone();
    let attr_of = |ins: &Insert, tag: &str| -> String {
        ins.attributes
            .iter()
            .find(|a| a.tag.trim() == tag)
            .map(|a| a.value.trim().to_string())
            .unwrap_or_default()
    };
    let mut target: Option<Insert> = None;
    for e in doc.model_space_entities() {
        let EntityType::Insert(ins) = e else {
            continue;
        };
        if ins.block_name != ROW_BLOCK {
            continue;
        }
        if attr_of(ins, "序号") == *key || attr_of(ins, "图号") == *key {
            target = Some(ins.clone());
            break;
        }
    }
    let Some(mut ins) = target else {
        host.push_error(&format!("OCSMBOMLOCK: 表里找不到序号/图号为「{key}」的行。"));
        return;
    };
    let cur_qty: usize = attr_of(&ins, "数量").parse().unwrap_or(1);
    let unlock = matches!(arg.as_deref(), Some("off") | Some("OFF") | Some("解锁") | Some("unlock"));

    if unlock {
        ins.common.extended_data.remove_record(LOCK_APP);
        host.push_undo("OCSM 数量锁");
        host.update_entity(EntityType::Insert(ins));
        host.set_dirty();
        host.push_info(&format!(
            "OCSMBOMLOCK: 序号 {}→已解锁（数量 {}）——下次同步会按件数/引用次数重算。",
            key, cur_qty
        ));
        return;
    }

    let qty = match arg.as_deref() {
        None => cur_qty,
        Some(v) => match v.parse::<usize>() {
            Ok(q) if q > 0 => q,
            _ => {
                host.push_error(&format!("OCSMBOMLOCK: 数量「{v}」不是正整数（或写 off 解锁）。"));
                return;
            }
        },
    };
    // 数量改值（如果指定了新值）+ 写锁记录
    if qty != cur_qty {
        for a in ins.attributes.iter_mut() {
            if a.tag.trim() == "数量" {
                a.value = qty.to_string();
            }
        }
        // 总重跟着算（单重 × 数量）
        let unit = attr_of(&ins, "单重");
        if let Some(w) = weight_number(&unit) {
            for a in ins.attributes.iter_mut() {
                if a.tag.trim() == "总重" {
                    a.value = trim_num(w * qty as f64);
                }
            }
        }
    }
    let mut rec = ExtendedDataRecord::new(LOCK_APP);
    rec.values.push(XDataValue::String(
        serde_json::json!({"qty": qty}).to_string(),
    ));
    ins.common.extended_data.remove_record(LOCK_APP);
    ins.common.extended_data.add_record(rec);
    host.push_undo("OCSM 数量锁");
    host.update_entity(EntityType::Insert(ins));
    host.set_dirty();
    host.push_info(&format!(
        "OCSMBOMLOCK: 序号 {} 数量锁定为 {qty}（同步不再重算；BOMLOCK {key} off 可解锁）。",
        key
    ));
}

// ── xlsx 导出 / 导入（第三期）─────────────────────────────────────────────

/// 图纸**没存过盘**时的默认落点目录：系统临时目录下的 `OCSM/`。
///
/// 走 `std::env::temp_dir()` 而不是写死路径 —— 一份代码跨平台：
/// Linux/macOS 给 `$TMPDIR`（通常 `/tmp`）、Windows 给 `%TEMP%`。
/// （临时目录会被系统清理，所以只是"找不到更好地方"时的兜底；插件各处提示都会明说。）
pub(crate) fn default_xlsx_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("OCSM")
}

/// 默认的 xlsx 路径：图纸同目录同名 + `-明细表.xlsx`；
/// 图纸未存盘 → [`default_xlsx_dir`]（临时目录/OCSM）。
fn default_xlsx_path(host: &dyn HostApi) -> std::path::PathBuf {
    let saved = host
        .document_path(host.tab_id())
        .and_then(|p| {
            let parent = p.parent().map(|d| d.to_path_buf())?;
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string())?;
            Some(parent.join(format!("{stem}-明细表.xlsx")))
        });
    saved.unwrap_or_else(|| default_xlsx_dir().join("明细表.xlsx"))
}

/// 把 xlsx 路径写进**表头块**的 `PE_URL`（宿主 Ctrl+点击即可打开该文件）。
fn set_xlsx_url(host: &mut dyn HostApi, path: &std::path::Path) {
    let doc = host.document().clone();
    let head = doc.model_space_entities().find_map(|e| match e {
        EntityType::Insert(ins) if ins.block_name == HEAD_BLOCK => Some(ins.clone()),
        _ => None,
    });
    let Some(mut ins) = head else { return };
    let url = path.to_string_lossy().to_string();
    let mut rec = ExtendedDataRecord::new("PE_URL");
    rec.values.push(XDataValue::String(url.clone()));
    ins.common.extended_data.remove_record("PE_URL");
    ins.common.extended_data.add_record(rec);
    host.update_entity(EntityType::Insert(ins));
    host.set_dirty();
    let _ = url;
}

/// 表头块上的 xlsx 路径（导入时默认从这里取）。
fn xlsx_url_of(host: &dyn HostApi) -> Option<std::path::PathBuf> {
    let doc = host.document();
    for e in doc.model_space_entities() {
        if let EntityType::Insert(ins) = e {
            if ins.block_name == HEAD_BLOCK {
                if let Some(rec) = ins.common.extended_data.get_record("PE_URL") {
                    if let Some(XDataValue::String(s)) =
                        rec.values.iter().find(|v| matches!(v, XDataValue::String(_)))
                    {
                        return Some(std::path::PathBuf::from(s));
                    }
                }
            }
        }
    }
    None
}

/// `OCSMBOMXLSX` / `BOMXLSX [路径]`：把**当前明细表**导出成 `.xlsx`（带「锁定数量」列）。
///
/// 导出的同时把每行当时的数量记进行块 XDATA（导入侧"手改即锁"的基线），
/// 并把文件路径写进表头块的 `PE_URL`（Ctrl+点击打开）。
pub(crate) fn cmd_bom_xlsx(host: &mut dyn HostApi, args: &str) {
    let doc = host.document().clone();
    let rows = crate::balloon_sync::old_rows(&doc);
    if rows.is_empty() {
        host.push_error("OCSMBOMXLSX: 图上还没有明细表行（先 `BOM` 或 `BOMSYNC` 建表）。");
        return;
    }
    let path = match args.split_whitespace().next() {
        Some(p) if !p.is_empty() => {
            let mut pb = std::path::PathBuf::from(p);
            if pb.extension().is_none() {
                pb.set_extension("xlsx");
            }
            pb
        }
        _ => default_xlsx_path(host),
    };
    let xrows: Vec<crate::bom_xlsx::XRow> = rows
        .iter()
        .map(|(_, (v, lock, _))| crate::bom_xlsx::xrow_from_cells(v, *lock))
        .collect();
    // 按扩展名分派：`.csv` → 写 CSV（UTF-8 **带 BOM**，Excel/WPS 认中文）；
    // 其它 → 写真 xlsx。两种格式导入侧都收（见 `BOMXLSXI`）。
    let as_csv = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("csv"))
        .unwrap_or(false);
    let wrote = if as_csv {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut text = String::from("\u{feff}");
        text.push_str(&crate::bom_xlsx::csv_text(&xrows));
        std::fs::write(&path, text.as_bytes()).map_err(|e| format!("写 {} 失败: {e}", path.display()))
    } else {
        crate::bom_xlsx::write_xlsx(&path, &xrows)
    };
    if let Err(e) = wrote {
        host.push_error(&format!("OCSMBOMXLSX: {e}"));
        return;
    }
    // 导出基线：每行当时的数量（用于导入侧判断"人手改过"）
    let doc = host.document().clone();
    let mut stamped = 0usize;
    host.push_undo("OCSM 明细表导出");
    for e in doc.model_space_entities() {
        let EntityType::Insert(ins) = e else { continue };
        if ins.block_name != ROW_BLOCK {
            continue;
        }
        let qty: Option<usize> = ins
            .attributes
            .iter()
            .find(|a| a.tag.trim() == "数量")
            .and_then(|a| a.value.trim().parse::<usize>().ok());
        let Some(q) = qty else { continue };
        let mut ins2 = ins.clone();
        let mut rec = ExtendedDataRecord::new(EXPORTED_APP);
        rec.values
            .push(XDataValue::String(serde_json::json!({"qty": q}).to_string()));
        ins2.common.extended_data.remove_record(EXPORTED_APP);
        ins2.common.extended_data.add_record(rec);
        if host.update_entity(EntityType::Insert(ins2)) {
            stamped += 1;
        }
    }
    set_xlsx_url(host, &path);
    host.set_dirty();
    if host.document_path(host.tab_id()).is_none() {
        host.push_info(&format!(
            "OCSMBOMXLSX: 提示——本图还没存过盘，所以文件落在**临时目录**（{}）。\
             临时目录会被系统清理，要长期保存请先 Ctrl+S 存盘再导出（xlsx 就会生成在图纸同目录、同名-明细表.xlsx）。",
            path.display()
        ));
    }
    host.push_info(&format!(
        "OCSMBOMXLSX: {} 行已导出 → {}（{} 行记下导出基线；表头已挂链接，Ctrl+点击可打开）。",
        xrows.len(),
        path.display(),
        stamped
    ));
    host.push_info(&format!(
        "OCSMBOMXLSX: 列 = {}（「锁定数量」只在此文件里，不进图纸表格；空=不锁，Y/是=锁为同行数量，数字=锁为该数）。",
        crate::bom_xlsx::col_list()
    ));
}

/// `OCSMBOMXLSXI` / `BOMXLSXI [路径]`：读 `.xlsx`（或 `.csv`）回灌明细表。
///
/// 规则：文件里**非空**的单元格覆盖图纸；**空白**保留图纸现值；文件里没有的序号保留；
/// **总重**列忽略输入（算出来的）；数量与"导出基线"不一致 = 手改 → **自动上锁**。
pub(crate) fn cmd_bom_xlsxi(host: &mut dyn HostApi, args: &str) {
    let arg = args.split_whitespace().next().unwrap_or("").to_string();
    let path = if !arg.is_empty() {
        std::path::PathBuf::from(&arg)
    } else {
        match xlsx_url_of(host) {
            Some(p) => p,
            None => default_xlsx_path(host),
        }
    };
    if !path.exists() {
        host.push_error(&format!(
            "OCSMBOMXLSXI: 找不到文件 {}（先 `BOMXLSX` 导出，或给个路径）。",
            path.display()
        ));
        return;
    }
    let is_csv = path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("csv"))
        .unwrap_or(false);
    let file = if is_csv {
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| crate::bom_xlsx::parse_csv(&s))
        {
            Ok(v) => v,
            Err(e) => {
                host.push_error(&format!("OCSMBOMXLSXI: 读 CSV 失败：{e}"));
                return;
            }
        }
    } else {
        match crate::bom_xlsx::read_xlsx(&path) {
            Ok(v) => v,
            Err(e) => {
                host.push_error(&format!("OCSMBOMXLSXI: {e}"));
                return;
            }
        }
    };
    if file.is_empty() {
        host.push_error("OCSMBOMXLSXI: 文件里没有数据行。");
        return;
    }
    let doc = host.document().clone();
    let prev = crate::balloon_sync::old_rows(&doc);
    let rows = crate::bom_xlsx::plan_import(&prev, &file);
    let per_col = load_config(&bom_dir()).per_col_rows;
    let locked = rows.iter().filter(|r| r.lock_qty.is_some()).count();
    let kept = rows
        .iter()
        .filter(|r| !file.iter().any(|f| f.item_no() == r.item_no))
        .count();
    match fill_bom(&mut HostSink(host), &doc, &rows, per_col) {
        Ok(rep) => {
            // 导入成功后刷新导出基线（同一份文件重复导入不会再"手改即锁"一次）
            let doc2 = host.document().clone();
            host.push_undo("OCSM 明细表导入");
            for e in doc2.model_space_entities() {
                let EntityType::Insert(ins) = e else { continue };
                if ins.block_name != ROW_BLOCK {
                    continue;
                }
                let qty: Option<usize> = ins
                    .attributes
                    .iter()
                    .find(|a| a.tag.trim() == "数量")
                    .and_then(|a| a.value.trim().parse::<usize>().ok());
                let Some(q) = qty else { continue };
                let mut ins2 = ins.clone();
                let mut rec = ExtendedDataRecord::new(EXPORTED_APP);
                rec.values
                    .push(XDataValue::String(serde_json::json!({"qty": q}).to_string()));
                ins2.common.extended_data.remove_record(EXPORTED_APP);
                ins2.common.extended_data.add_record(rec);
                host.update_entity(EntityType::Insert(ins2));
            }
            host.set_dirty();
            host.push_info(&format!(
                "OCSMBOMXLSXI: 从 {} 导入 {} 行 → 表 {} 行 / {} 列（{} 行数量已锁定，{} 行文件里没有、按图纸保留）。",
                path.display(),
                file.len(),
                rep.rows,
                rep.cols.len(),
                locked,
                kept
            ));
        }
        Err(e) => host.push_error(&format!("OCSMBOMXLSXI: {e}")),
    }
}

/// `OCSMBOMCFG` / `BOMCFG [每列行数]`：查看/修改 `bom/settings.json`。
pub(crate) fn cmd_bom_cfg(host: &mut dyn HostApi, args: &str) {
    let dir = bom_dir();
    let mut cfg = load_config(&dir);
    match args.split_whitespace().next().and_then(|t| t.parse::<usize>().ok()) {
        Some(n) if n > 0 => {
            cfg.per_col_rows = n;
            match save_config(&dir, &cfg) {
                Ok(()) => host.push_info(&format!(
                    "OCSMBOMCFG: 首列行数已设为 {n}（{}）。",
                    config_path(&dir).display()
                )),
                Err(e) => host.push_error(&format!("OCSMBOMCFG: 写配置失败：{e}")),
            }
        }
        _ => host.push_info(&format!(
            "OCSMBOMCFG: 首列行数 {}（配置 {}）；用法：BOMCFG 30 改默认，BOM 30 只改本次。",
            cfg.per_col_rows,
            config_path(&dir).display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 单元格文字自动压缩 ──

    /// **改表头模板**：把「单件」「总重」「重量」三个标签的字高乘 `scale`（默认 0.9），
    /// 原文件存 `.bak`。表头是静态文字、不走单元格压缩，所以只能在模板上改。
    ///
    /// `cargo test -p ocs_ocsm --lib rescale_head_template_labels -- --ignored --nocapture`
    #[test]
    #[ignore = "改模板（手动跑）"]
    fn rescale_head_template_labels() {
        use ocs_plugin_api::host::acadrust::io::dwg::{DwgReader, DwgWriter};
        const SCALE: f64 = 0.9;
        const LABELS: [&str; 3] = ["单件", "总重", "重量"];
        let path = bom_dir().join(format!("{HEAD_BLOCK}.dwg"));
        println!("表头模板：{}", path.display());
        let mut doc = match DwgReader::from_file(&path).and_then(|mut r| r.read()) {
            Ok(d) => d,
            Err(e) => {
                println!("读失败：{e}");
                return;
            }
        };
        // 备份
        let bak = path.with_extension("dwg.bak");
        if !bak.exists() {
            if let Err(e) = std::fs::copy(&path, &bak) {
                println!("备份失败：{e}");
                return;
            }
            println!("已备份 → {}", bak.display());
        }
        // 改：整块改到本地副本再整体替换，避免边遍历边改
        let mut changed = 0usize;
        let mut entities: Vec<EntityType> = doc.model_space_entities().cloned().collect();
        for e in entities.iter_mut() {
            if let EntityType::Text(tx) = e {
                if LABELS.iter().any(|l| tx.value.trim() == *l) {
                    let old = tx.height;
                    tx.height *= SCALE;
                    println!("  {:?} 字高 {old:.2} → {:.2}", tx.value, tx.height);
                    changed += 1;
                }
            }
        }
        if changed == 0 {
            println!("没找到要改的标签，退出");
            return;
        }
        // 用改后的实体替换模型空间（先删旧的，再加新的）
        let old: Vec<_> = doc.model_space_entities().map(|e| e.common().handle).collect();
        for h in old {
            let _ = doc.remove_entity(h);
        }
        for e in entities {
            let _ = doc.add_entity(e);
        }
        if let Err(e) = DwgWriter::write_to_file(&path, &doc) {
            println!("写回失败：{e}");
            return;
        }
        println!("已写回：{}（{changed} 个标签 ×{SCALE}）", path.display());
    }

    /// 打印表头模板（`OCSM_BOMHEAD.dwg`）里的文字实体与字高（人工核对用）：
    /// `cargo test -p ocs_ocsm --lib dump_head_template_texts -- --ignored --nocapture`
    #[test]
    #[ignore = "读表头模板打印文字（手动跑）"]
    fn dump_head_template_texts() {
        use ocs_plugin_api::host::acadrust::io::dwg::DwgReader;
        let path = bom_dir().join(format!("{HEAD_BLOCK}.dwg"));
        println!("表头模板：{}", path.display());
        let doc = match DwgReader::from_file(&path).and_then(|mut r| r.read()) {
            Ok(d) => d,
            Err(e) => {
                println!("读失败：{e}");
                return;
            }
        };
        for e in doc.model_space_entities() {
            match e {
                EntityType::Text(tx) => println!(
                    "  TEXT  {:?}  插入点 x={:.1} y={:.1}  字高 {:.2} 宽比 {:.2} 字型 {:?}",
                    tx.value, tx.insertion_point.x, tx.insertion_point.y,
                    tx.height, tx.width_factor, "?"
                ),
                EntityType::MText(m) => println!(
                    "  MTEXT {:?}  插入点 x={:.1} y={:.1}  字高 {:.2}",
                    m.value, m.insertion_point.x, m.insertion_point.y, m.height
                ),
                EntityType::AttributeDefinition(ad) => println!(
                    "  ATTDEF tag={:?} 默认 {:?}  字高 {:.2}",
                    ad.tag, ad.default_value, ad.height
                ),
                _ => {}
            }
        }
    }

    /// 对**真实模板**跑一遍列宽解析 + 压缩模拟（人工检查用）：
    /// `cargo test -p ocs_ocsm --lib dump_row_template_widths -- --ignored --nocapture`
    #[test]
    #[ignore = "读模板 DWG 打印列宽（手动跑）"]
    fn dump_row_template_widths() {
        use ocs_plugin_api::host::acadrust::io::dwg::DwgReader;
        let path = bom_dir().join(format!("{ROW_BLOCK}.dwg"));
        println!("模板：{}", path.display());
        let doc = match DwgReader::from_file(&path).and_then(|mut r| r.read()) {
            Ok(d) => d,
            Err(e) => {
                println!("读失败：{e}");
                return;
            }
        };
        // 模板 DWG 的内容在**模型空间**（导入时才被定义成块 OCSM_BOMROW）。
        let ents: Vec<EntityType> = doc.model_space_entities().cloned().collect();
        let cols = row_cell_widths(&ents);
        println!("解析到 {} 列：", cols.len());
        for (i, (left, avail)) in cols.iter().enumerate() {
            let tag = CELL_TAGS.get(i).copied().unwrap_or("?");
            println!("  {tag:>4}: 左边界 {left:>7.2}  可用宽 {avail:>6.2}");
        }
        // 拿模板里的 ATTDEF 字高/宽度因子/字型，模拟常见内容
        let h = ents
            .iter()
            .find_map(|e| match e {
                EntityType::AttributeDefinition(ad) => Some(ad.height),
                _ => None,
            })
            .unwrap_or(3.5);
        println!("ATTDEF 字高 = {h}");
        {
            let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
            for e in &ents {
                let k = match e {
                    EntityType::Line(_) => "Line",
                    EntityType::AttributeDefinition(_) => "ATTDEF",
                    EntityType::Text(_) => "Text",
                    EntityType::MText(_) => "MText",
                    EntityType::LwPolyline(_) => "LwPolyline",
                    EntityType::Polyline(_) => "Polyline",
                    EntityType::Insert(_) => "Insert",
                    _ => "other",
                };
                *counts.entry(k).or_insert(0) += 1;
            }
            println!("块内实体：{counts:?}");
            for e in &ents {
                if let EntityType::Text(tx) = e {
                    println!("  Text {tx:?}");
                }
            }
        }
        for e in &ents {
            if let EntityType::AttributeDefinition(ad) = e {
                println!(
                    "  ATTDEF {:<4} 字高 {:.2} 宽比 {:.2} 字型 {:?} 对齐 {:?}",
                    ad.tag, ad.height, ad.width_factor, ad.text_style, ad.horizontal_alignment
                );
            }
        }
        for (tag, sample) in [
            ("序号", "1"),
            ("图号", "GB/T 5780-2016"),
            ("名称", "六角头螺栓 C级 M8x35"),
            ("数量", "2"),
            ("材料", "Q235"),
            ("单重", "0.018"),
            ("总重", "0.018"),
            ("备注", "外购"),
        ] {
            let Some(i) = CELL_TAGS.iter().position(|x| *x == tag) else { continue };
            let avail = cols.get(i).map(|c| c.1).unwrap_or(0.0);
            let (h2, f, over) = fit_cell_text(sample, avail, h, 0.7);
            println!(
                "  {tag:>4} {sample:<26} 可用 {avail:>6.2} → 字高 {h2:.2} 宽比 {f:.2}{}",
                if over { "  ← 仍溢出" } else { "" }
            );
        }
    }

    #[test]
    fn fit_cell_text_shrinks_height_then_squeezes_width() {
        // 模板实况：字高 5、宽度因子 0.7
        let (h0, wf0) = (5.0, 0.7);
        // 短文本原样
        assert_eq!(fit_cell_text("1", 10.0, h0, wf0), (h0, wf0, false));
        // 用户实况①：`名称 六角头螺栓 C级 M8x35` 在 33mm 格里 → 压字高（宽比不变）
        let name = "六角头螺栓 C级 M8x35";
        let avail_name = 33.0 - CELL_PADDING;
        let (h, wf, over) = fit_cell_text(name, avail_name, h0, wf0);
        assert!(h < h0 && (wf - wf0).abs() < 1e-9, "应先只压字高：h={h} wf={wf}");
        assert!(!over);
        assert!(cell_text_width(name, h, wf) <= avail_name * FIT_TARGET_RATIO + 1e-9);
        // 用户实况②：`0.018` 在 10mm 格（单重）→ 也要压
        let (h2, _, over2) = fit_cell_text("0.018", 10.0, h0, wf0);
        assert!(h2 < h0 && !over2, "h={h2}");
        // 图号 `GB/T 5780-2016` 在**够宽的**格里不动（可用宽 40 → 目标 32，仍放得下）
        assert_eq!(fit_cell_text("GB/T 5780-2016", 55.0, h0, wf0), (h0, wf0, false));
        // 目标只用到可用宽的 80%：可用 34.5（37mm 列）也要压一点 —— 给渲染误差留余量
        let (hg, _, _) = fit_cell_text("GB/T 5780-2016", 34.5, h0, wf0);
        assert!(hg < h0, "80% 目标下该压：{hg}");
        // 极长（60 个汉字）→ 字高到底后**无限横向压缩**，保证一定放进格内、绝不到邻格
        let long: String = "超长名称".repeat(15);
        let (h3, wf3, over3) = fit_cell_text(&long, 36.0, h0, wf0);
        assert_eq!(h3, MIN_CELL_TEXT_H);
        assert!(wf3 < wf0 * WARN_WIDTH_FACTOR, "应压得很扁：{wf3}");
        assert!(!over3, "压到底也必须放得下（用户定案：宽度因子可无限压缩）");
        assert!(cell_text_width(&long, h3, wf3) <= 36.0 * FIT_TARGET_RATIO + 1e-9);
        // 极端：可用宽只有 1mm 也照样压进去
        let (_, wf4, over4) = fit_cell_text(&long, 1.0, h0, wf0);
        assert!(!over4);
        assert!(wf4 < wf0 * 0.1);
        // 空文本/异常参数不炸
        assert_eq!(fit_cell_text("  ", 10.0, h0, wf0), (h0, wf0, false));
        assert_eq!(fit_cell_text("abc", 0.0, h0, wf0), (h0, wf0, false));
    }

    #[test]
    fn row_cell_widths_reads_template_geometry() {
        use ocs_plugin_api::host::acadrust::entities::Line;
        let mut ents: Vec<EntityType> = Vec::new();
        for x in [0.0, 11.0, 48.0, 81.0, 92.0, 127.0, 138.0, 150.0, 180.0] {
            ents.push(EntityType::Line(Line::from_coords(x, 0.0, 0.0, x, 8.0, 0.0)));
        }
        // 横线不该被当成分隔线
        ents.push(EntityType::Line(Line::from_coords(0.0, 0.0, 0.0, 180.0, 0.0, 0.0)));
        let cols = row_cell_widths(&ents);
        assert_eq!(cols.len(), CELL_TAGS.len(), "应得到 8 列");
        assert!((cols[0].1 - (11.0 - CELL_PADDING)).abs() < 1e-9);
        assert!((cols[1].1 - (37.0 - CELL_PADDING)).abs() < 1e-9);
        // 列定位：ATTDEF 落在第 2 列（图号 11..48）
        assert_eq!(avail_width_at(12.0, &cols).unwrap(), cols[1].1);
        assert_eq!(avail_width_at(49.0, &cols).unwrap(), cols[2].1);
        // 解析不出（空块）→ 退回标称列宽
        let fallback = row_cell_widths(&[]);
        assert_eq!(fallback.len(), CELL_TAGS.len());
        assert!((fallback[1].1 - (37.0 - CELL_PADDING)).abs() < 1e-9);
    }

    #[test]
    fn default_xlsx_dir_is_temp_ocsm_cross_platform() {
        // 跨平台：走 std::env::temp_dir() —— Linux/macOS 给 /tmp（或 $TMPDIR）、
        // Windows 给 %TEMP%，所以这一条断言在两个平台都成立。
        let d = default_xlsx_dir();
        assert!(d.ends_with("OCSM"), "{d:?}");
        assert!(d.starts_with(std::env::temp_dir()), "{d:?}");
    }

    fn part(code: &str, name: &str, material: &str, weight: &str) -> PartMeta {
        PartMeta {
            code: code.into(),
            name: name.into(),
            spec: String::new(),
            material: material.into(),
            weight: weight.into(),
        }
    }

    #[test]
    fn aggregate_groups_by_code_and_material_keeping_order() {
        let parts = vec![
            part("GB/T 5782-2016", "六角头螺栓", "45", "≈0.05"),
            part("0165", "偏心轴", "45", "≈1.35"),
            part("GB/T 5782-2016", "六角头螺栓", "45", "≈0.05"),
        ];
        let items = aggregate(&parts);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].qty, 2);
        assert_eq!(items[1].name, "偏心轴");
    }

    #[test]
    fn cell_values_fill_total_weight() {
        let it = BomItem {
            code: "0165".into(),
            name: "偏心轴".into(),
            spec: "φ10".into(),
            material: "45".into(),
            unit_weight: "≈1.35".into(),
            qty: 3,
        };
        let v = cell_values(&it, 7);
        assert_eq!(v[0], "7");
        assert_eq!(v[1], "0165");
        assert_eq!(v[2], "偏心轴 φ10");
        assert_eq!(v[3], "3");
        assert_eq!(v[5], "1.35"); // 不写 ≈
        assert_eq!(v[6], "4.05");
    }

    #[test]
    fn layout_first_column_takes_defined_rows_then_continuation_to_the_left() {
        let cfg = BomConfig::default();
        let cols = layout(41, 26, &cfg).unwrap();
        assert_eq!(cols, vec![26, 15]); // 用户演示图：26 + 15
        let cols = layout(4, 26, &cfg).unwrap();
        assert_eq!(cols, vec![4]);
        // 首列 28 行是 A3 的物理上限（(287-45-12)/8 = 28.75）：定义 40 也被夹到 28
        assert_eq!(layout(28, 40, &cfg).unwrap(), vec![28]);
    }

    #[test]
    fn layout_refuses_when_the_sheet_is_too_small() {
        let cfg = BomConfig::default();
        // A3 只放得下 2 列（(210+180-0)/180 = 2.17），第 3 列要被拒绝
        let err = layout(150, 26, &cfg).unwrap_err();
        assert!(err.contains("多页明细表"), "{err}");
    }

    #[test]
    fn config_round_trip_uses_bom_dir() {
        let dir = std::env::temp_dir().join(format!("ocsm-bom-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = load_config(&dir);
        assert_eq!(cfg.per_col_rows, 26);
        assert!(config_path(&dir).exists(), "首次读取应写出默认配置");
        let mut cfg2 = cfg.clone();
        cfg2.per_col_rows = 30;
        save_config(&dir, &cfg2).unwrap();
        assert_eq!(load_config(&dir).per_col_rows, 30);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn weight_number_accepts_estimation_prefix() {
        assert_eq!(weight_number("≈1.35"), Some(1.35)); // 老数据仍能算总重
        assert_eq!(weight_number("≈ 0.05 kg"), Some(0.05));
        assert_eq!(weight_number(""), None);
    }

    #[test]
    fn weight_text_drops_estimate_prefix() {
        assert_eq!(weight_text("≈1.35"), "1.35");
        assert_eq!(weight_text(" ≈ 0.050 kg "), "0.050 kg");
        assert_eq!(weight_text("约2.5"), "2.5");
        assert_eq!(weight_text("1.35"), "1.35");
    }
}
