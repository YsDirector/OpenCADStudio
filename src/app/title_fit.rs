//! P4 —— 标题栏长文本的**自动宽度压缩**（宿主侧）。
//!
//! 图框模板给 ATTDEF 的 `width_factor` 只能是固定值；填入的长图号
//! （如 14 字符的 `YSD06-260928-b`）会超出所在格子。自动化填表
//! （`set_properties` 写属性值）时，这里按**格宽**与估算文字宽自动下调
//! **实体级** `width_factor`：
//!
//! ```text
//! wf = min(当前 wf, 0.9 × 格宽 / 估算文字宽)
//! ```
//!
//! 格宽由块定义里的线段沿文字基线方向量取（标题栏格子就是线段围出来的）；
//! 量不到格宽时退回长度规则 `10/字符数`——只对「图样代号」类字段生效，
//! 避免误缩窄本来放得下的其它字段。
//!
//! 触发条件：这次请求真的改了属性值，且当前宽度下会越格；同一请求里
//! 显式设置 `width_factor` 的字段不覆盖（用户选择优先）。

use codec::types::{Vector2, Vector3};
use codec::{CadDocument, EntityType};

/// 平均字符宽（×字高）。模板字体（朱雀仿宋）实测 ≈0.735，取 0.75 保守估计：
/// 估计偏大只会略多压缩一点，不会漏压导致越格。
const GLYPH_WIDTH: f64 = 0.75;
/// 文字最多可占格宽的比例（两侧留白）。
const CELL_FILL: f64 = 0.9;
/// 回退长度规则的阈值：超过这个字符数才压缩。
const FALLBACK_MAX_CHARS: usize = 10;
/// 压缩下限，避免病态值把文字挤成一条线。
const MIN_WIDTH_FACTOR: f64 = 0.05;

/// 自动化 `set_properties` 编辑实体后调用：按需压缩被改动的属性宽度。
///
/// * `source` — 编辑前的实体；`edited` — 编辑后（此处可能被就地修改）。
/// * `applied_paths` — 本次请求实际生效的属性路径（用于识别显式
///   `width_factor` 设置，遵循「用户显式值优先」）。
pub(crate) fn fit_edited_attributes(
    document: &CadDocument,
    source: &EntityType,
    edited: &mut EntityType,
    applied_paths: &[String],
) {
    match (source, edited) {
        (EntityType::Insert(source_insert), EntityType::Insert(edited_insert)) => {
            let explicit: Vec<usize> = applied_paths
                .iter()
                .filter_map(|path| attribute_edit_index(path, "width_factor"))
                .collect();
            let block_name = edited_insert.block_name.clone();
            let transform = crate::scene::render_graph::insert_transform(document, edited_insert);
            for index in 0..edited_insert.attributes.len() {
                if explicit.contains(&index) {
                    continue;
                }
                let value_changed = source_insert
                    .attributes
                    .get(index)
                    .is_none_or(|before| before.value != edited_insert.attributes[index].value);
                if !value_changed {
                    continue;
                }
                fit_one(
                    document,
                    &block_name,
                    &transform,
                    &mut edited_insert.attributes[index],
                );
            }
        }
        (EntityType::AttributeEntity(before), EntityType::AttributeEntity(edited_attr)) => {
            if before.value == edited_attr.value {
                return;
            }
            if applied_paths
                .iter()
                .any(|path| path.ends_with("/width_factor"))
            {
                return;
            }
            // 属性记录单独编辑时，块几何要从属主的 INSERT 上找。
            let owner = document
                .get_entity(edited_attr.common.owner_handle)
                .and_then(|entity| match entity {
                    EntityType::Insert(insert) => Some(insert.clone()),
                    _ => None,
                });
            match owner {
                Some(insert) => {
                    let transform = crate::scene::render_graph::insert_transform(document, &insert);
                    fit_one(document, &insert.block_name, &transform, edited_attr);
                }
                None => {
                    // 拿不到块几何：只做「图样代号」的长度回退。
                    fit_by_length_only(edited_attr);
                }
            }
        }
        _ => {}
    }
}

/// 单个属性：算目标宽度因子，必要时写回。
fn fit_one(
    document: &CadDocument,
    block_name: &str,
    transform: &codec::types::Transform,
    attr: &mut codec::entities::AttributeEntity,
) {
    let chars = attr.value.chars().count();
    if chars == 0 || !(attr.height > 0.0) {
        return;
    }
    let estimated = chars as f64 * GLYPH_WIDTH * attr.height;
    let current = effective_width_factor(attr);
    let target = match cell_width_along_baseline(document, block_name, transform, attr) {
        Some(cell) if cell > 1e-6 => {
            // 当前宽度下放得下就不动：避免无谓改写（也会无谓标脏）。
            if estimated * current <= CELL_FILL * cell {
                return;
            }
            CELL_FILL * cell / estimated
        }
        _ => {
            if !is_drawing_number_tag(&attr.tag) || chars <= FALLBACK_MAX_CHARS {
                return;
            }
            let fitted = FALLBACK_MAX_CHARS as f64 / chars as f64;
            if fitted >= current {
                return;
            }
            fitted
        }
    };
    let fitted = ((target * 1000.0).round() / 1000.0).clamp(MIN_WIDTH_FACTOR, 1.0);
    if fitted < current - 1e-9 {
        // 负的 width_factor 表示镜像方向：保留符号，只改大小。
        let sign = if attr.width_factor < 0.0 { -1.0 } else { 1.0 };
        attr.width_factor = sign * fitted;
    }
}

/// 只按长度规则的紧凑（拿不到块几何时的回退路径）。
fn fit_by_length_only(attr: &mut codec::entities::AttributeEntity) {
    let chars = attr.value.chars().count();
    if !is_drawing_number_tag(&attr.tag) || chars <= FALLBACK_MAX_CHARS {
        return;
    }
    let current = effective_width_factor(attr);
    let fitted = ((FALLBACK_MAX_CHARS as f64 / chars as f64) * 1000.0).round() / 1000.0;
    if fitted < current - 1e-9 {
        let sign = if attr.width_factor < 0.0 { -1.0 } else { 1.0 };
        attr.width_factor = sign * fitted.clamp(MIN_WIDTH_FACTOR, 1.0);
    }
}

/// 沿文字基线方向量出前后两面墙的距离（格宽）。量不到返回 `None`。
fn cell_width_along_baseline(
    document: &CadDocument,
    block_name: &str,
    transform: &codec::types::Transform,
    attr: &codec::entities::AttributeEntity,
) -> Option<f64> {
    let record = document
        .block_records
        .iter()
        .find(|record| record.name.eq_ignore_ascii_case(block_name))?;
    let origin = Vector2::new(attr.insertion_point.x, attr.insertion_point.y);
    let direction = Vector2::new(attr.rotation.cos(), attr.rotation.sin());
    if direction.length_squared() < 1e-12 {
        return None;
    }
    // 文字的基线（无限直线）与块内线段求交：最近的前/后交点就是两面墙。
    let mut forward = f64::INFINITY;
    let mut backward = f64::NEG_INFINITY;
    for &handle in &record.entity_handles {
        let Some(entity) = document.get_entity(handle) else {
            continue;
        };
        for (start, end) in block_segments(entity) {
            let a = transform.apply(start);
            let b = transform.apply(end);
            let edge = Vector2::new(b.x - a.x, b.y - a.y);
            let denominator = cross(direction, edge);
            if denominator.abs() < 1e-12 {
                continue; // 与基线平行，不构成墙
            }
            let offset = Vector2::new(a.x - origin.x, a.y - origin.y);
            let along = cross(offset, edge) / denominator;
            let on_segment = cross(offset, direction) / denominator;
            if !(-1e-9..=1.0 + 1e-9).contains(&on_segment) {
                continue; // 交点落在线段之外
            }
            if along > 1e-6 {
                forward = forward.min(along);
            } else if along < -1e-6 {
                backward = backward.max(along);
            }
        }
    }
    (forward.is_finite() && backward.is_finite() && forward - backward > 1e-6)
        .then_some(forward - backward)
}

/// 块内一个实体的直线段（标题栏格子由 LINE / 二维多段线围成；有凸度的弧
/// 段按弦处理——格子边都是直线，够用）。
fn block_segments(entity: &EntityType) -> Vec<(Vector3, Vector3)> {
    let mut segments = Vec::new();
    match entity {
        EntityType::Line(line) => segments.push((line.start, line.end)),
        EntityType::LwPolyline(polyline) => {
            let points: Vec<Vector3> = polyline
                .vertices
                .iter()
                .map(|vertex| {
                    Vector3::new(vertex.location.x, vertex.location.y, polyline.elevation)
                })
                .collect();
            push_polyline_segments(&mut segments, &points, polyline.is_closed);
        }
        EntityType::Polyline2D(polyline) => {
            let points: Vec<Vector3> = polyline
                .vertices
                .iter()
                .map(|vertex| {
                    Vector3::new(
                        vertex.location.x,
                        vertex.location.y,
                        polyline.elevation,
                    )
                })
                .collect();
            push_polyline_segments(&mut segments, &points, polyline.is_closed());
        }
        _ => {}
    }
    segments
}

fn push_polyline_segments(segments: &mut Vec<(Vector3, Vector3)>, points: &[Vector3], closed: bool) {
    for pair in points.windows(2) {
        segments.push((pair[0], pair[1]));
    }
    if closed && points.len() > 2 {
        segments.push((points[points.len() - 1], points[0]));
    }
}

fn cross(a: Vector2, b: Vector2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn effective_width_factor(attr: &codec::entities::AttributeEntity) -> f64 {
    if attr.width_factor.abs() > 1e-9 {
        attr.width_factor.abs()
    } else {
        1.0
    }
}

/// `/attributes/<i>/width_factor` → `Some(i)`。
fn attribute_edit_index(path: &str, field: &str) -> Option<usize> {
    let rest = path.strip_prefix("/attributes/")?;
    let (index, tail) = rest.split_once('/')?;
    (tail == field).then(|| index.parse().ok()).flatten()
}

/// 「图样代号」类字段（模板中文 tag / 英文 tag 两种写法）。
fn is_drawing_number_tag(tag: &str) -> bool {
    let english = tag.to_ascii_lowercase();
    tag.contains("图样代号")
        || tag.contains("图号")
        || english.contains("drawing_no")
        || english.contains("drawing number")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codec::entities::{AttributeEntity, Insert, Line};
    use codec::tables::BlockRecord;
    use codec::types::Handle;
    use codec::{CadDocument, EntityType};

    /// 造一块「格子 + 图样代号属性」的图框：块内格 x∈[0,60]、y∈[0,12]，
    /// 属性贴右墙、字高 7、180°（与 A3 模板折算前一致）。
    /// `source_value` 是编辑前的值，`value` 是本次请求写入的新值。
    fn frame_with_attribute(
        source_value: &str,
        value: &str,
        width_factor: f64,
        scale: f64,
    ) -> (CadDocument, Insert, Insert) {
        let mut document = CadDocument::default();
        let mut record = BlockRecord::new("TITLE");
        let add = |document: &mut CadDocument, entity: EntityType, record: &mut BlockRecord| {
            let handle = document.add_entity(entity).unwrap_or(Handle::NULL);
            record.entity_handles.push(handle);
        };
        add(
            &mut document,
            EntityType::Line(Line {
                start: Vector3::new(0.0, 0.0, 0.0),
                end: Vector3::new(60.0, 0.0, 0.0),
                ..Line::default()
            }),
            &mut record,
        );
        add(
            &mut document,
            EntityType::Line(Line {
                start: Vector3::new(0.0, 12.0, 0.0),
                end: Vector3::new(60.0, 12.0, 0.0),
                ..Line::default()
            }),
            &mut record,
        );
        add(
            &mut document,
            EntityType::Line(Line {
                start: Vector3::new(0.0, 0.0, 0.0),
                end: Vector3::new(0.0, 12.0, 0.0),
                ..Line::default()
            }),
            &mut record,
        );
        add(
            &mut document,
            EntityType::Line(Line {
                start: Vector3::new(60.0, 0.0, 0.0),
                end: Vector3::new(60.0, 12.0, 0.0),
                ..Line::default()
            }),
            &mut record,
        );
        document.block_records.add(record).unwrap();

        let make_insert = |value: &str| {
            let mut insert = Insert::new("TITLE", Vector3::new(0.0, 0.0, 0.0));
            insert.set_x_scale(scale);
            insert.set_y_scale(scale);
            insert.set_z_scale(scale);
            let mut attribute = AttributeEntity::simple("图样代号", value);
            // 属性按世界坐标存储（与 build_frame_insert 的 apply_transform 结果一致）。
            attribute.insertion_point = Vector3::new(59.5 * scale, 6.0 * scale, 0.0);
            attribute.height = 7.0 * scale;
            attribute.rotation = std::f64::consts::PI;
            attribute.width_factor = width_factor;
            insert.attributes.push(attribute);
            insert
        };
        (document, make_insert(source_value), make_insert(value))
    }

    #[test]
    fn long_drawing_number_is_compressed_to_the_cell() {
        let (document, source, edited) = frame_with_attribute("YSD06", "YSD06-260928-b", 1.0, 0.5);
        let source = EntityType::Insert(source);
        let mut edited = EntityType::Insert(edited);
        fit_edited_attributes(&document, &source, &mut edited, &[]);
        let EntityType::Insert(edited) = &edited else {
            panic!("insert expected")
        };
        let attr = &edited.attributes[0];
        assert!(
            attr.width_factor < 0.8 && attr.width_factor > 0.6,
            "14 字符应压缩到 ≈0.74，得到 {}",
            attr.width_factor
        );
        // 压缩后估算宽度不超过格宽（0.5 缩放下格宽 30mm；宽度因子取 3 位
        // 小数会带来 <0.1mm 的舍入余量）。
        let width = 14.0 * GLYPH_WIDTH * attr.height * attr.width_factor;
        assert!(width <= 30.0, "width={width}");
        assert!(width > 25.0, "压缩过度：width={width}");
    }

    #[test]
    fn short_drawing_number_is_left_alone() {
        let (document, source, edited) = frame_with_attribute("YSD0", "YSD06", 1.0, 0.5);
        let source = EntityType::Insert(source);
        let mut edited = EntityType::Insert(edited);
        fit_edited_attributes(&document, &source, &mut edited, &[]);
        let EntityType::Insert(edited) = &edited else {
            panic!("insert expected")
        };
        assert_eq!(edited.attributes[0].width_factor, 1.0);
    }

    #[test]
    fn explicit_width_factor_wins_over_auto_fit() {
        let (document, source, edited) = frame_with_attribute("YSD06", "YSD06-260928-b", 1.0, 0.5);
        let source = EntityType::Insert(source);
        let mut edited = EntityType::Insert(edited);
        fit_edited_attributes(
            &document,
            &source,
            &mut edited,
            &["/attributes/0/width_factor".to_string()],
        );
        let EntityType::Insert(edited) = &edited else {
            panic!("insert expected")
        };
        assert_eq!(edited.attributes[0].width_factor, 1.0);
    }

    #[test]
    fn attribute_without_a_cell_falls_back_to_the_length_rule() {
        let mut document = CadDocument::default();
        document.block_records.add(BlockRecord::new("BARE")).unwrap();
        let make_insert = |value: &str| {
            let mut insert = Insert::new("BARE", Vector3::new(0.0, 0.0, 0.0));
            let mut attribute = AttributeEntity::simple("图样代号", value);
            attribute.height = 7.0;
            attribute.width_factor = 1.0;
            insert.attributes.push(attribute);
            insert
        };
        let source = EntityType::Insert(make_insert("YSD06"));
        let mut edited = EntityType::Insert(make_insert("YSD06-260928-b"));
        fit_edited_attributes(&document, &source, &mut edited, &[]);
        let EntityType::Insert(edited) = &edited else {
            panic!("insert expected")
        };
        assert_eq!(edited.attributes[0].width_factor, 0.714);
    }

    #[test]
    fn unrelated_attribute_tags_are_not_shrunk_without_geometry() {
        let mut document = CadDocument::default();
        document.block_records.add(BlockRecord::new("BARE")).unwrap();
        let mut insert = Insert::new("BARE", Vector3::new(0.0, 0.0, 0.0));
        let mut attribute = AttributeEntity::simple("图样名称", "some very long title");
        attribute.height = 5.0;
        attribute.width_factor = 1.0;
        insert.attributes.push(attribute);
        let source = EntityType::Insert(insert.clone());
        let mut edited = EntityType::Insert(insert);
        fit_edited_attributes(&document, &source, &mut edited, &[]);
        let EntityType::Insert(edited) = &edited else {
            panic!("insert expected")
        };
        assert_eq!(edited.attributes[0].width_factor, 1.0);
    }
}
