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

fn part_meta_of(common: &ocs_plugin_api::host::acadrust::entities::EntityCommon) -> Option<PartMeta> {
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

    let head_dwg = dir.join("OCSM_BOMHEAD.dwg");
    let row_dwg = dir.join("OCSM_BOMROW.dwg");
    if !head_dwg.exists() || !row_dwg.exists() {
        host.push_error(&format!(
            "OCSMBOM: 找不到明细表模板块（{} / {}）。请把 OCSM_BOMHEAD.dwg、OCSM_BOMROW.dwg \
             放进插件目录 bom/（或设 OCSM_BOM_DIR）。",
            head_dwg.display(),
            row_dwg.display()
        ));
        return;
    }

    // ① 块：缺了才导入（宿主 import 自己会 PushUndo，且已存在时直接返回 ATTDEF）。
    let have = |host: &dyn HostApi, name: &str| host.document().block_records.get(name).is_some();
    for (path, name) in [(&head_dwg, HEAD_BLOCK), (&row_dwg, ROW_BLOCK)] {
        if have(host, name) {
            continue;
        }
        if let Err(e) = host.import_frame_block(ImportFrameBlockRequest {
            path: path.to_string_lossy().into_owned(),
            block_name: name.to_string(),
        }) {
            host.push_error(&format!("OCSMBOM: 导入块 {name} 失败：{e}"));
            return;
        }
    }

    // ② 行块里的 8 个 ATTDEF（几何 + 样式契约从模板来，插件不写死坐标）。
    let doc = host.document().clone();
    let attdefs: Vec<AttributeDefinition> = doc
        .entities_in_block(ROW_BLOCK)
        .filter_map(|e| match e {
            EntityType::AttributeDefinition(ad) => Some(ad.clone()),
            _ => None,
        })
        .collect();
    if attdefs.len() != CELL_TAGS.len() {
        host.push_error(&format!(
            "OCSMBOM: 行块 {ROW_BLOCK} 应有 {} 个 ATTDEF，实际 {} 个 → 模板不对。",
            CELL_TAGS.len(),
            attdefs.len()
        ));
        return;
    }

    // ③ 扫图：旧表元（带 OCSM_BOM）+ 零件（带 OCSM_PART）。
    let mut old: Vec<Handle> = Vec::new();
    let mut parts: Vec<PartMeta> = Vec::new();
    let mut untracked = 0usize; // OCSM_ 零件块但缺 OCSM_PART 台账（离线生成/早期版本）
    for e in doc.model_space_entities() {
        let common = e.common();
        if is_bom(common) {
            old.push(common.handle);
        }
        if let EntityType::Insert(ins) = e {
            match part_meta_of(&ins.common) {
                Some(m) => parts.push(m),
                None if ins.block_name.starts_with("OCSM_") && ins.block_name != HEAD_BLOCK
                    && ins.block_name != ROW_BLOCK => untracked += 1,
                None => {}
            }
        }
    }

    let items = aggregate(&parts);
    let cols = match layout(items.len(), per_col, &cfg) {
        Ok(c) => c,
        Err(msg) => {
            host.push_error(&format!("OCSMBOM: {msg}"));
            return;
        }
    };

    // ④ 建表（表头 + 行，含 8 个属性值），全部打 OCSM_BOM 标记。
    let mut ents: Vec<EntityType> = Vec::new();
    let mut seq = 0usize;
    for (ci, rows) in cols.iter().enumerate() {
        let x0 = cfg.first_col_left - COL_W * ci as f64;
        let y0 = if ci == 0 { cfg.first_col_bottom } else { cfg.sheet_bottom };

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

        for k in 0..*rows {
            seq += 1;
            let yr = y0 + HEAD_H + ROW_H * k as f64;
            let mut ins = Insert::new(ROW_BLOCK, Vector3::new(x0, yr, 0.0));
            {
                let c = insert_common(&mut ins);
                c.layer = LAYER_LINE.to_string();
                c.color = Color::ByLayer;
                c.linetype = "ByLayer".to_string();
                c.line_weight = LineWeight::ByLayer;
            }
            let item = &items[seq - 1];
            let values = cell_values(item, seq);
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
            ents.push(EntityType::Insert(ins));
        }
    }

    // ⑤ 替换语义：一次 undo（删旧 + 建新），整体可 Ctrl+Z。
    host.push_undo("OCSM 明细表");
    let removed = old.len();
    for h in old {
        host.remove_entity(h);
    }
    let added = host.add_entities(ents).len();
    host.set_dirty();

    let desc: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, n)| format!("第{}列 {} 行", i + 1, n))
        .collect();
    host.push_info(&format!(
        "OCSMBOM: {} 件 → {} 列（{}）；旧表元 {} 个已替换（Ctrl+Z 可整体撤销）。",
        items.len(),
        cols.len(),
        desc.join("、"),
        removed
    ));
    if items.iter().any(|it| it.spec.is_empty()) && !items.is_empty() {
        host.push_info("OCSMBOM: 提示——数量按「图中插入件数」统计，同一零件画在多个视图里会重复计数。");
    }
    if untracked > 0 {
        host.push_info(&format!(
            "OCSMBOM: 注意——图中有 {untracked} 个 OCSM_ 零件块引用但没有 OCSM_PART 台账记录             （多为离线生成/早期版本的文件），它们不会进明细表。用 XL 重新放置，或后续用表格导入补录。"
        ));
    }
    let _ = added;
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
