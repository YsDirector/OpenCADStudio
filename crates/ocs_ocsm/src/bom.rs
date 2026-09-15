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

/// 默认的 xlsx 路径：图纸同目录同名 + `-明细表.xlsx`；图纸未存盘则落到 `~/桌面/OCSM/`。
fn default_xlsx_path(host: &dyn HostApi) -> std::path::PathBuf {
    let stem = host
        .document_path(host.tab_id())
        .map(|p| p.to_path_buf())
        .and_then(|p| {
            let parent = p.parent().map(|d| d.to_path_buf())?;
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string())?;
            Some(parent.join(format!("{stem}-明细表.xlsx")))
        });
    stem.unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        std::path::PathBuf::from(home).join("桌面/OCSM/明细表.xlsx")
    })
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
            "OCSMBOMXLSX: 提示——本图还没存过盘，所以文件落在默认目录（{}）。\
             想让它生成在图纸同目录，先 Ctrl+S 存盘再导出。",
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
