//! A dimension's picture-block placement — both halves of the contract.
//!
//! OCS/OcSM bake a dimension's anonymous `*D` picture in **world coordinates**:
//! the AutoCAD reference files (`OCSMDIMGULIDE2/3-*.dxf`) carry no DXF group 12
//! at all, and their block members sit at their world positions. The renderer
//! still honours a stored insertion point, rotation and scale, because files
//! written by CAD systems that record a frame origin need it
//! (`src/scene/render_graph.rs::dimension_picture_insert`).
//!
//! That makes the **identity placement the invariant for every generator**:
//! with a non-zero insertion point the whole picture is drawn offset by it. The
//! "flying radius dimension" of 2026-10-17 was exactly that — OCSM's radial and
//! angular picture dims carried the *text anchor* as their insertion point, and
//! the in-memory drawing only looked right after a save, because the DXF writer
//! emits no group 12 (the DWG writer does), so reloading cleared it. The plugin
//! side of the fix dropped the placement and added
//! `guide_server::tests::picture_form_dimensions_carry_no_placement`.
//!
//! Locked here so neither half drifts: at the identity placement the members
//! draw where they are stored, and a non-zero placement offsets them by exactly
//! that point.

use codec::entities::{Dimension, DimensionRadius, Line};
use codec::types::Vector3;
use codec::EntityType;
use glam::DVec3;
use OpenCADStudio::scene::Scene;

/// The anonymous picture block a dimension references.
const PICTURE: &str = "*D1";
/// Circle centre and rim point of the measured arc (world coordinates).
const CENTER: [f64; 3] = [10.0, 40.0, 0.0];
const RIM: [f64; 3] = [30.0, 40.0, 0.0];

fn v(p: [f64; 3]) -> Vector3 {
    Vector3::new(p[0], p[1], p[2])
}

/// Every consecutive point pair the scene draws, in world coordinates.
///
/// `point_world` hands back glam vectors, the same type the renderer uses.
fn drawn_segments(scene: &Scene) -> Vec<(DVec3, DVec3)> {
    let mut segments = Vec::new();
    for wire in scene.entity_wires().iter() {
        for index in 0..wire.points.len().saturating_sub(1) {
            segments.push((wire.point_world(index, 1.0), wire.point_world(index + 1, 1.0)));
        }
    }
    segments
}

/// Whether the drawn wires contain the segment `a`–`b` (either direction).
fn has_segment(segments: &[(DVec3, DVec3)], a: [f64; 3], b: [f64; 3]) -> bool {
    let close = |p: DVec3, q: [f64; 3]| {
        (p.x - q[0]).abs() < 1e-6 && (p.y - q[1]).abs() < 1e-6 && (p.z - q[2]).abs() < 1e-6
    };
    segments
        .iter()
        .any(|(s, e)| (close(*s, a) && close(*e, b)) || (close(*s, b) && close(*e, a)))
}

/// A radius dimension whose picture block holds centre-to-rim in world
/// coordinates, carrying `insertion_point` as its picture placement.
fn scene_with_picture_dim(insertion_point: Vector3) -> Scene {
    let mut scene = Scene::new();
    scene.define_block_raw(
        PICTURE,
        Vector3::ZERO,
        vec![EntityType::Line(Line::from_points(v(CENTER), v(RIM)))],
    );
    let mut dim = DimensionRadius::new(v(CENTER), v(RIM));
    dim.base.block_name = PICTURE.to_string();
    dim.base.insertion_point = insertion_point;
    scene.add_entity(EntityType::Dimension(Box::new(Dimension::Radius(dim))));
    scene.bump_geometry();
    scene
}

#[test]
fn identity_placement_draws_the_picture_where_it_is_stored() {
    let scene = scene_with_picture_dim(Vector3::ZERO);
    let segments = drawn_segments(&scene);
    assert!(
        has_segment(&segments, CENTER, RIM),
        "图片块成员应画在它存储的世界坐标上：{segments:?}"
    );
}

#[test]
fn a_non_zero_placement_offsets_the_whole_picture() {
    // Kept for imported files that record a frame origin — which is precisely
    // why a generator must store the identity (see the module docs).
    let shift = Vector3::new(78.5355, 177.1213, 0.0);
    let scene = scene_with_picture_dim(shift);
    let segments = drawn_segments(&scene);
    let moved = |p: [f64; 3]| [p[0] + shift.x, p[1] + shift.y, p[2] + shift.z];
    assert!(
        has_segment(&segments, moved(CENTER), moved(RIM)),
        "非零放置应把整幅图片平移该点：{segments:?}"
    );
    assert!(
        !has_segment(&segments, CENTER, RIM),
        "平移之后原位不应再画出线段（说明放置没有生效）：{segments:?}"
    );
}
