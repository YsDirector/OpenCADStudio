// Two adjacent (edge-sharing) HATCH fills that carry the same pattern origin
// (same name / scale / `base_point`) must paint in phase: their lines have to
// continue across the shared edge instead of stepping sideways.
//
// Before the fix, `hatch_model_from_dxf` folded each hatch's stored pattern
// origin back into a small interval around its OWN boundary AABB centre on a
// `spacing * 64` grid. That grid is not a period of the pattern: measured
// along x a cell is `64 * spacing / |sin θ|` across the lines, i.e. 64/√2 ≈
// 45.25 spacings for a 45° family, so two blocks whose centres land in
// neighbouring cells painted a 0.2548-cell fraction — 0.36 * spacing in
// `q = y - x`, 0.76 mm for the 3-unit spacing of the bug report — apart.
use codec::entities::hatch::{BoundaryEdge, BoundaryPath, HatchPatternLine, LineEdge};
use codec::entities::{Hatch, LwPolyline, LwVertex};
use codec::types::{Color as AcadColor, Vector2};
use codec::EntityType;
use OpenCADStudio::scene::model::hatch_model::{HatchModel, HatchPattern as ModelPattern};
use OpenCADStudio::scene::Scene;

/// Across-line spacing of the test patterns, in drawing units.
const SPACING: f64 = 3.0;
const BLOCK_W: f64 = 10.0;
const BLOCK_H: f64 = 10.0;
/// Fold cell the renderer used before the fix (`spacing * 64`) — one full cell
/// is NOT a period of a 45° family, so the test blocks straddle a cell boundary.
const LEGACY_CELL: f64 = SPACING * 64.0;
/// Block centres `(x0 + 5, 5)` / `(x0 + 15, 5)`; with `x0 = 87` they sit at 92
/// and 102, on opposite sides of the legacy half-cell boundary at 96.
const STRADDLE_X0: f64 = 87.0;
/// The same straddle pattern at UTM scale: 499_968 is a whole number of legacy
/// cells, so this reproduces it ~5e5 units from the pattern origin.
const FAR_X0: f64 = STRADDLE_X0 + 499_968.0;
/// Two fills are "in phase" when their lines meet within this distance
/// (drawing units, measured across the lines). The bug measured 0.7645 here.
const PHASE_TOL: f64 = 1.0e-5;

fn rect_path(x0: f64, y0: f64, x1: f64, y1: f64) -> BoundaryPath {
    let mut path = BoundaryPath::new();
    for (s, e) in [
        ((x0, y0), (x1, y0)),
        ((x1, y0), (x1, y1)),
        ((x1, y1), (x0, y1)),
        ((x0, y1), (x0, y0)),
    ] {
        path.edges.push(BoundaryEdge::Line(LineEdge {
            start: Vector2::new(s.0, s.1),
            end: Vector2::new(e.0, e.1),
        }));
    }
    path
}

fn rect_outline(x0: f64, y0: f64, x1: f64, y1: f64) -> LwPolyline {
    let mut pl = LwPolyline::new();
    pl.is_closed = true;
    pl.vertices = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .into_iter()
        .map(|(x, y)| LwVertex::new(Vector2::new(x, y)))
        .collect();
    pl
}

/// A prebaked hatch on `[x0, x0 + 10] x [0, 10]` carrying `lines` — the shape a
/// DWG-authored hatch has (its own resolved pattern lines, no catalog lookup).
fn hatch_with(x0: f64, lines: Vec<HatchPatternLine>, name: &str, aci: u8) -> Hatch {
    let mut hatch = Hatch::new();
    hatch.paths.push(rect_path(x0, 0.0, x0 + BLOCK_W, BLOCK_H));
    hatch.common.color = AcadColor::Index(aci);
    hatch.is_solid = false;
    hatch.pattern.name = name.into();
    hatch.pattern.lines = lines;
    hatch
}

/// One 45° family (ANSI31-shaped): offset = perpendicular step to the next
/// line, `(-s/√2, s/√2)`.
fn ansi31_line(base: (f64, f64)) -> HatchPatternLine {
    let d = SPACING / std::f64::consts::SQRT_2;
    HatchPatternLine {
        angle: std::f64::consts::FRAC_PI_4,
        base_point: Vector2::new(base.0, base.1),
        offset: Vector2::new(-d, d),
        dash_lengths: vec![],
    }
}

/// Two crossed 45° families (45° + 135°) at the same spacing — the multi-family
/// case, where the fold cell has to stay a period of BOTH families.
fn cross45_lines(base: (f64, f64)) -> Vec<HatchPatternLine> {
    let d = SPACING / std::f64::consts::SQRT_2;
    vec![
        HatchPatternLine {
            angle: std::f64::consts::FRAC_PI_4,
            base_point: Vector2::new(base.0, base.1),
            offset: Vector2::new(-d, d),
            dash_lengths: vec![],
        },
        HatchPatternLine {
            angle: 3.0 * std::f64::consts::FRAC_PI_4,
            base_point: Vector2::new(base.0, base.1),
            offset: Vector2::new(-d, -d),
            dash_lengths: vec![],
        },
    ]
}

/// Two adjacent hatches — `[x0, x0 + 10]` and `[x0 + 10, x0 + 20]` — with the
/// same stored pattern lines. Returns the two render models, left first.
fn adjacent_pair(x0: f64, lines: &[HatchPatternLine], name: &str) -> (HatchModel, HatchModel) {
    let mut scene = Scene::new();
    scene.add_entity(EntityType::Hatch(hatch_with(
        x0,
        lines.to_vec(),
        name,
        5,
    )));
    scene.add_entity(EntityType::Hatch(hatch_with(
        x0 + BLOCK_W,
        lines.to_vec(),
        name,
        1,
    )));
    scene.populate_hatches_from_document();

    let mut models: Vec<HatchModel> = scene
        .paper_plot_hatches()
        .iter()
        .filter(|m| matches!(m.pattern, ModelPattern::Pattern(_)))
        .cloned()
        .collect();
    assert_eq!(models.len(), 2, "expected the two pattern hatches");
    models.sort_by(|a, b| a.world_origin[0].total_cmp(&b.world_origin[0]));
    (models[0].clone(), models[1].clone())
}

fn families(model: &HatchModel) -> &[OpenCADStudio::scene::model::hatch_model::PatFamily] {
    match &model.pattern {
        ModelPattern::Pattern(f) => f,
        _ => unreachable!("pattern hatch expected"),
    }
}

/// Phase step of every family (in drawing units, measured across the lines)
/// between the two fills where they meet at `edge_x`: `0` means the lines
/// continue straight across. A segment belongs to a family when its direction
/// matches the family's, and its invariant across the lines is
/// `perp = -x sin θ + y cos θ`.
fn phase_steps(left: &HatchModel, right: &HatchModel, edge_x: f64) -> Vec<f64> {
    let (fl, fr) = (families(left), families(right));
    assert_eq!(fl.len(), fr.len(), "both fills must carry the same pattern");
    let mut steps = Vec::new();
    for (index, fam) in fr.iter().enumerate() {
        let spacing = f64::from(fam.dy).abs();
        assert!(spacing > 1.0e-9, "family {index} has no spacing");
        let angle = f64::from(fam.angle_deg).to_radians();
        let (sin_a, cos_a) = angle.sin_cos();
        let invariants = |model: &HatchModel, from_left: bool| -> Vec<f64> {
            let mut values = Vec::new();
            for [p, q] in model.pattern_segments() {
                let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
                let length = (dx * dx + dy * dy).sqrt();
                if length <= 1.0e-9 {
                    continue;
                }
                // Segment parallel to this family?
                if ((dx * cos_a + dy * sin_a) / length).abs() < 1.0 - 1.0e-9 {
                    continue;
                }
                let reach = if from_left {
                    p[0].max(q[0])
                } else {
                    p[0].min(q[0])
                };
                if (reach - edge_x).abs() > 1.0e-6 {
                    continue;
                }
                let (xm, ym) = ((p[0] + q[0]) * 0.5, (p[1] + q[1]) * 0.5);
                values.push(-xm * sin_a + ym * cos_a);
            }
            values
        };
        let (ql, qr) = (invariants(left, true), invariants(right, false));
        assert!(
            !ql.is_empty() && !qr.is_empty(),
            "family {index} does not reach the shared edge x={edge_x}"
        );
        let mut best = f64::INFINITY;
        for a in &ql {
            for b in &qr {
                let d = (b - a).rem_euclid(spacing);
                best = best.min(d.min(spacing - d));
            }
        }
        steps.push(best);
    }
    steps
}

fn assert_in_phase(left: &HatchModel, right: &HatchModel, edge_x: f64, what: &str) {
    let steps = phase_steps(left, right, edge_x);
    for (index, step) in steps.iter().enumerate() {
        assert!(
            *step < PHASE_TOL,
            "{what}: adjacent fills paint out of phase — family {index} steps \
             {step} across the shared edge (the two fills must line up)"
        );
    }
}

#[test]
fn adjacent_hatches_share_pattern_phase() {
    let (left, right) = adjacent_pair(STRADDLE_X0, &[ansi31_line((0.0, 0.0))], "ANSI31");
    assert_in_phase(&left, &right, STRADDLE_X0 + BLOCK_W, "45° fill");
}

#[test]
fn adjacent_hatches_share_pattern_phase_far_from_origin() {
    // UTM-scale placement: the stored pattern origin stays at 0,0 while the
    // geometry is ~5e5 away — the case the fold exists for (f32 offsets must
    // stay small) and the case a real drawing hits.
    let (left, right) = adjacent_pair(FAR_X0, &[ansi31_line((0.0, 0.0))], "ANSI31");
    assert_in_phase(&left, &right, FAR_X0 + BLOCK_W, "45° fill far from origin");
    // The fold's purpose — keeping the shader's f32 pattern offsets small —
    // must survive: neither model may carry a geometry-sized offset.
    for model in [&left, &right] {
        for fam in families(model) {
            assert!(
                (fam.x0 as f64).abs() <= LEGACY_CELL && (fam.y0 as f64).abs() <= LEGACY_CELL,
                "pattern origin was not folded back into a small interval: \
                 x0={} y0={} (legacy cell {LEGACY_CELL})",
                fam.x0,
                fam.y0
            );
        }
    }
}

#[test]
fn adjacent_crossed_fills_share_pattern_phase() {
    // Two crossed 45° families: the fold cell must be a period of both, or the
    // second family's lines step at the edge even though the first one lines up.
    let (left, right) = adjacent_pair(STRADDLE_X0, &cross45_lines((0.0, 0.0)), "ANSI31X2");
    assert_in_phase(&left, &right, STRADDLE_X0 + BLOCK_W, "crossed 45° fill");
}

/// Writes the on-host E2E drawing (`B7_FIXTURE=<path>`): the two blocks with
/// their outlines plus the two pattern fills, both authored at base point 0,0
/// with the AABB centres straddling the legacy fold cell. Ignored by default —
/// it generates a fixture, it does not assert.
#[test]
#[ignore]
fn write_repro_fixture() {
    let Ok(path) = std::env::var("B7_FIXTURE") else {
        return;
    };
    let mut scene = Scene::new();
    for x0 in [STRADDLE_X0, STRADDLE_X0 + BLOCK_W] {
        scene.add_entity(EntityType::LwPolyline(rect_outline(
            x0,
            0.0,
            x0 + BLOCK_W,
            BLOCK_H,
        )));
        scene.add_entity(EntityType::Hatch(hatch_with(
            x0,
            vec![ansi31_line((0.0, 0.0))],
            "ANSI31",
            5,
        )));
    }
    OpenCADStudio::io::save(&scene.document, std::path::Path::new(&path))
        .expect("write the repro fixture");
    println!("wrote {path}");
}
