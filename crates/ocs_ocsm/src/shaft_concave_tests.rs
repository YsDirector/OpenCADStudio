//! Regression tests for concave shaft fillets and explicit, total keyway slot lengths.
//! Included by `shaft.rs` as a child test module. Geometry is checked through public
//! `parse_program` / `validate` / `build` / `place`, not private fillet helpers or DXF fixtures.

use super::{
    block_name, build, keyway_b, parse_program, place, validate, Chamfer, End, Fillet,
    FilletKind, KeyKind, KeywayPlace, Program, Shaft, ShaftView,
};
use crate::partgen_kit::{LAYER_HATCH, LAYER_MAIN};
use ocs_plugin_api::host::acadrust::entities::hatch::BoundaryEdge;
use ocs_plugin_api::host::acadrust::entities::{EntityType, Hatch};

const EPS: f64 = 1e-8;
const GEAR_SPEC: &str = "GEAR EX M3 Z52 DA162 DF148.5 H38";
const DEMO: &str = "S65 L98 CH1@L FI1@R:CONCAVE KEY C 50 @端 SL50 t1 7 \
    | S75 L23 CH1@L \
    | S85 L20 CH1@L FI3@R:CONCAVE \
    | GEAR EX M3 Z52 DA162 DF148.5 H38 \
    | S141 L4 \
    | GEAR EX M3 Z52 DA162 DF148.5 H38 \
    | S85 L20 FI3@L:CONCAVE CH1@R \
    | S75 L23 FI0.8@L:CONCAVE CH1@R";

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

fn point_near(a: [f64; 2], b: [f64; 2]) -> bool {
    near(a[0], b[0]) && near(a[1], b[1])
}

#[track_caller]
fn program(text: &str) -> Program {
    parse_program(text).unwrap_or_else(|e| panic!("parse {text}: {e}"))
}

#[track_caller]
fn shaft(text: &str) -> Shaft {
    build(&program(text), 1.0).unwrap_or_else(|e| panic!("build {text}: {e}"))
}

#[track_caller]
fn assert_rejected(text: &str) {
    let result = parse_program(text).and_then(|p| build(&p, 1.0));
    let error = result.expect_err(&format!("accepted invalid shaft: {text}"));
    assert!(!error.trim().is_empty(), "empty error for {text}");
}

#[track_caller]
fn assert_model_rejected(p: &Program) {
    assert!(validate(p).is_err(), "validate accepted {p:?}");
    assert!(build(p, 1.0).is_err(), "build accepted {p:?}");
}

fn has_line(entities: &[EntityType], a: [f64; 2], b: [f64; 2], layer: &str) -> bool {
    entities.iter().any(|e| match e {
        EntityType::Line(l) if l.common.layer == layer => {
            let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
            (point_near(p, a) && point_near(q, b))
                || (point_near(p, b) && point_near(q, a))
        }
        _ => false,
    })
}

fn angle_near(actual_radians: f64, expected_degrees: f64) -> bool {
    let expected = expected_degrees.to_radians();
    near(actual_radians.cos(), expected.cos())
        && near(actual_radians.sin(), expected.sin())
}

fn has_arc(
    entities: &[EntityType],
    center: [f64; 2],
    radius: f64,
    start_degrees: f64,
    end_degrees: f64,
) -> bool {
    let expected_sweep = (end_degrees - start_degrees)
        .to_radians()
        .rem_euclid(std::f64::consts::TAU);
    entities.iter().any(|e| match e {
        EntityType::Arc(a) if a.common.layer == LAYER_MAIN => {
            point_near([a.center.x, a.center.y], center)
                && near(a.radius, radius)
                // Native ARC fields are radians; compare directions and the CCW sweep.
                && angle_near(a.start_angle, start_degrees)
                && angle_near(a.end_angle, end_degrees)
                && near(
                    (a.end_angle - a.start_angle).rem_euclid(std::f64::consts::TAU),
                    expected_sweep,
                )
        }
        _ => false,
    })
}

fn arc_count(s: &Shaft) -> usize {
    s.entities.iter().filter(|e| matches!(e, EntityType::Arc(_))).count()
}

#[track_caller]
fn native_hatch(s: &Shaft) -> &Hatch {
    let mut hatches = s.entities.iter().filter_map(|e| match e {
        EntityType::Hatch(h) => Some(h.as_ref()),
        _ => None,
    });
    let h = hatches.next().expect("section must contain native HATCH");
    assert!(hatches.next().is_none(), "expected one HATCH for the whole shaft");
    assert_eq!(h.common.layer, LAYER_HATCH);
    h
}

fn boundary_endpoints(edge: &BoundaryEdge) -> [[f64; 2]; 2] {
    match edge {
        BoundaryEdge::Line(l) => [[l.start.x, l.start.y], [l.end.x, l.end.y]],
        BoundaryEdge::CircularArc(a) => {
            assert!(a.radius.is_finite() && a.radius > 0.0);
            assert!(a.start_angle.is_finite() && a.end_angle.is_finite());
            // Decode the native codec convention independently of shaft's HatchEdge helpers:
            // a CW edge measures its angles clockwise, rather than just swapping endpoints.
            let sign = if a.counter_clockwise { 1.0 } else { -1.0 };
            [a.start_angle, a.end_angle].map(|angle| {
                [
                    a.center.x + a.radius * angle.cos(),
                    a.center.y + sign * a.radius * angle.sin(),
                ]
            })
        }
        other => panic!("unexpected native shaft hatch boundary: {other:?}"),
    }
}

#[track_caller]
fn assert_closed_hatch(s: &Shaft) {
    let h = native_hatch(s);
    assert_eq!(h.paths.len(), 2, "upper and lower material rings");
    for (path_index, path) in h.paths.iter().enumerate() {
        assert!(path.edges.len() >= 3, "empty/degenerate ring {path_index}");
        let points: Vec<_> = path.edges.iter().map(boundary_endpoints).collect();
        for (i, pair) in points.iter().enumerate() {
            assert!(pair.iter().flatten().all(|v| v.is_finite()));
            let next = points[(i + 1) % points.len()][0];
            assert!(
                point_near(pair[1], next),
                "HATCH path {path_index}, edge {i}: gap {:?} -> {next:?}",
                pair[1],
            );
        }
    }
}

#[track_caller]
fn assert_hatch_arc(
    s: &Shaft,
    center: [f64; 2],
    radius: f64,
    endpoints: [[f64; 2]; 2],
    counter_clockwise: bool,
) {
    assert!(native_hatch(s).paths.iter().flat_map(|p| &p.edges).any(|edge| {
        match edge {
            BoundaryEdge::CircularArc(a)
                if point_near([a.center.x, a.center.y], center)
                    && near(a.radius, radius)
                    && a.counter_clockwise == counter_clockwise =>
            {
                let actual = boundary_endpoints(edge);
                point_near(actual[0], endpoints[0]) && point_near(actual[1], endpoints[1])
            }
            _ => false,
        }
    }), "missing native HATCH arc {center:?} R{radius}, endpoints {endpoints:?}");
}

fn transformed(p: [f64; 2], at: [f64; 2], degrees: f64) -> [f64; 2] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [at[0] + p[0] * cos - p[1] * sin, at[1] + p[0] * sin + p[1] * cos]
}

#[track_caller]
fn assert_section_rotations(s: &Shaft) {
    assert_closed_hatch(s);
    let original_hatch = native_hatch(s);
    for degrees in [0.0, 30.0, 90.0, -27.0, 180.0] {
        let at = [70.0, -50.0];
        let rotated = Shaft {
            entities: place(s.entities.clone(), at, degrees),
            ..s.clone()
        };
        assert_closed_hatch(&rotated);
        let rotated_hatch = native_hatch(&rotated);
        for (before, after) in original_hatch.paths.iter().zip(&rotated_hatch.paths) {
            assert_eq!(before.edges.len(), after.edges.len());
            for (a, b) in before.edges.iter().zip(&after.edges) {
                let pa = boundary_endpoints(a);
                let pb = boundary_endpoints(b);
                for i in 0..2 {
                    assert!(point_near(transformed(pa[i], at, degrees), pb[i]),
                        "rotated boundary endpoint disagrees at {degrees} degrees");
                }
                match (a, b) {
                    (BoundaryEdge::Line(_), BoundaryEdge::Line(_)) => {}
                    (BoundaryEdge::CircularArc(a), BoundaryEdge::CircularArc(b)) => {
                        assert_eq!(a.counter_clockwise, b.counter_clockwise);
                        assert!(near(a.radius, b.radius));
                        assert!(point_near(
                            transformed([a.center.x, a.center.y], at, degrees),
                            [b.center.x, b.center.y],
                        ));
                    }
                    _ => panic!("placement changed boundary edge type"),
                }
            }
        }
        for e in &s.entities {
            if let EntityType::Arc(a) = e {
                assert!(has_arc(
                    &rotated.entities,
                    transformed([a.center.x, a.center.y], at, degrees),
                    a.radius,
                    a.start_angle.to_degrees() + degrees,
                    a.end_angle.to_degrees() + degrees,
                ), "rotated native ARC disagrees at {degrees} degrees");
            }
        }
    }
}

#[test]
fn fillet_kind_defaults_to_convex_and_keeps_legacy_json_compact() {
    assert_eq!(FilletKind::default(), FilletKind::Convex);
    let p = program("s30 l40 fi=2@left fi3");
    assert_eq!(p.segments[0].fillet, vec![
        Fillet { r: 2.0, end: End::L, kind: FilletKind::Convex },
        Fillet { r: 3.0, end: End::R, kind: FilletKind::Convex },
    ]);
    let json = serde_json::to_value(&p).unwrap();
    for f in json["segments"][0]["fillet"].as_array().unwrap() {
        assert!(f.get("kind").is_none(), "default convex kind should be omitted");
    }
    assert_eq!(program(&json.to_string()), p);
    let implicit = program(r#"{"segments":[{"s":30,"l":40,"fillet":{"r":2}}]}"#);
    let explicit = program(r#"{"segments":[{"s":30,"l":40,"fillet":{"r":2,"kind":"convex"}}]}"#);
    assert_eq!(implicit, program("S30 L40 FI2"));
    assert_eq!(implicit, explicit);
    assert_eq!(implicit.segments[0].fillet[0].end, End::R);
    assert_eq!(block_name(&implicit), block_name(&explicit));
}

#[test]
fn concave_dsl_is_case_insensitive_defaults_right_and_roundtrips_json() {
    let expected = program("S20 L20 FI2@R:CONCAVE | S30 L10");
    for token in ["FI2@R:CONCAVE", "fi2@r:concave", "Fi2@Right:CoNcAvE", "FI2:CONCAVE"] {
        let p = program(&format!("s20 l20 {token} | s30 l10"));
        assert_eq!(p, expected, "{token}");
        assert_eq!(p.segments[0].fillet[0].kind, FilletKind::Concave);
        assert_eq!(p.segments[0].fillet[0].end, End::R);
        assert!(near(p.segments[0].fillet[0].r, 2.0));
    }
    let json_input = program(r#"{"segments":[{"s":20,"l":20,"fillet":{"r":2,"kind":"concave"}},{"s":30,"l":10}]}"#);
    assert_eq!(json_input, expected);
    let json = serde_json::to_value(&expected).unwrap();
    assert_eq!(json["segments"][0]["fillet"][0]["kind"].as_str(), Some("concave"));
    let roundtrip = program(&json.to_string());
    assert_eq!(roundtrip, expected);
    assert_eq!(build(&roundtrip, 1.0).unwrap(), build(&expected, 1.0).unwrap());
}

#[test]
fn changing_only_fillet_kind_changes_geometry_and_block_name() {
    let concave = program("S20 L20 FI2@R:CONCAVE | S30 L10");
    let mut convex = concave.clone();
    convex.segments[0].fillet[0].kind = FilletKind::Convex;
    assert_eq!(convex, program("S20 L20 FI2@R | S30 L10"));
    assert_ne!(block_name(&convex), block_name(&concave));
    assert_ne!(build(&convex, 1.0).unwrap(), build(&concave, 1.0).unwrap());
}

#[test]
fn invalid_fillet_kind_radius_and_end_are_not_silently_ignored() {
    for token in [
        "FI", "FI0:CONCAVE", "FI-2:CONCAVE", "FINaN:CONCAVE", "FI1e999:CONCAVE",
        "FI2@X:CONCAVE", "FI2@:CONCAVE", "FI2@R:", "FI2@R:UNKNOWN",
        "FI2@R:CONCAVE:CONCAVE", "FI2@R:CONCAVE FI1@R:CONCAVE",
        "FI2@R FI1@R:CONCAVE",
    ] {
        let text = format!("S20 L20 {token} | S30 L10");
        assert!(parse_program(&text).is_err(), "accepted invalid FI syntax: {text}");
    }
    for fillet in [
        serde_json::json!({"r": 2, "kind": "unknown"}),
        serde_json::json!({"r": 2, "kind": 7}),
        serde_json::json!({"r": 2, "end": "X", "kind": "concave"}),
        serde_json::json!({"r": 0, "kind": "concave"}),
        serde_json::json!({"r": -2, "kind": "concave"}),
    ] {
        let input = serde_json::json!({"segments": [
            {"s": 20, "l": 20, "fillet": fillet}, {"s": 30, "l": 10}
        ]});
        assert!(parse_program(&input.to_string()).is_err(), "accepted {input}");
    }
    for r in [0.0, -2.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut p = program("S20 L20 FI2@R:CONCAVE | S30 L10");
        p.segments[0].fillet[0].r = r;
        assert_model_rejected(&p);
    }
}

#[test]
fn ch_and_fi_remain_mutually_exclusive_on_the_same_requested_end() {
    for text in [
        "S30 L40 FI2@L CH1@L",
        "S30 L40 CH1@R FI2@R",
        "S20 L20 FI2@R:CONCAVE CH1@R | S30 L10",
        "S30 L10 | S20 L20 CH1@L FI2@L:CONCAVE",
        r#"{"segments":[{"s":30,"l":40,"ch":{"c":1,"end":"L"},"fillet":{"r":2,"end":"L"}}]}"#,
        r#"{"segments":[{"s":20,"l":20,"ch":{"c":1},"fillet":{"r":2,"kind":"concave"}},{"s":30,"l":10}]}"#,
    ] {
        assert_rejected(text);
    }
    for kind in [FilletKind::Convex, FilletKind::Concave] {
        let mut p = program("S20 L20 FI2@R:CONCAVE | S30 L10");
        p.segments[0].fillet[0].kind = kind;
        p.segments[0].ch.push(Chamfer { c: 1.0, end: End::R });
        assert_model_rejected(&p);
    }
}

#[test]
fn ch_and_fi_on_opposite_ends_are_allowed_in_dsl_json_and_geometry() {
    for text in [
        "S30 L40 FI2@L CH1@R",
        "S30 L40 CH1@L FI2@R",
        "S20 L20 CH1@L FI2@R:CONCAVE | S30 L10",
        "S30 L10 | S20 L20 FI2@L:CONCAVE CH1@R",
    ] {
        let mut p = program(text);
        validate(&p).unwrap();
        let roundtrip = program(&serde_json::to_string(&p).unwrap());
        assert_eq!(roundtrip, p);
        assert_eq!(build(&roundtrip, 1.0).unwrap(), build(&p, 1.0).unwrap());
        p.view = ShaftView::Section;
        assert_closed_hatch(&build(&p, 1.0).unwrap());
    }
    let right = shaft("S20 L20 CH1@L FI2@R:CONCAVE | S30 L10");
    assert!(has_line(&right.entities, [1.0, 10.0], [18.0, 10.0], LAYER_MAIN));
    let left = shaft("S30 L10 | S20 L20 FI2@L:CONCAVE CH1@R");
    assert!(has_line(&left.entities, [12.0, 10.0], [29.0, 10.0], LAYER_MAIN));
}

#[test]
fn concave_requests_from_either_side_land_on_the_small_diameter() {
    for (small_request, large_request) in [
        ("S20 L20 FI2@R:CONCAVE | S30 L10", "S20 L20 | S30 L10 FI2@L:CONCAVE"),
        ("S30 L10 | S20 L20 FI2@L:CONCAVE", "S30 L10 FI2@R:CONCAVE | S20 L20"),
    ] {
        for view in [ShaftView::Normal, ShaftView::Section] {
            let mut a = program(small_request);
            let mut b = program(large_request);
            a.view = view;
            b.view = view;
            let sa = build(&a, 1.0).unwrap();
            let sb = build(&b, 1.0).unwrap();
            assert_eq!(sa, sb, "anchor must not change the actual landing");
            assert_eq!(arc_count(&sa), 2);
            if view == ShaftView::Section {
                assert_closed_hatch(&sa);
            }
        }
    }
    let right = shaft("S20 L20 FI2@R:CONCAVE | S30 L10");
    assert!(has_arc(&right.entities, [18.0, 12.0], 2.0, 270.0, 360.0));
    assert!(has_arc(&right.entities, [18.0, -12.0], 2.0, 0.0, 90.0));
    for sign in [1.0, -1.0] {
        assert!(has_line(&right.entities, [0.0, sign * 10.0], [18.0, sign * 10.0], LAYER_MAIN));
        assert!(has_line(&right.entities, [20.0, sign * 12.0], [20.0, sign * 15.0], LAYER_MAIN));
        assert!(has_line(&right.entities, [20.0, sign * 15.0], [30.0, sign * 15.0], LAYER_MAIN));
        assert!(!has_line(&right.entities, [18.0, sign * 10.0], [20.0, sign * 12.0], LAYER_MAIN), "not a chamfer chord");
        assert!(!has_line(&right.entities, [0.0, sign * 10.0], [20.0, sign * 10.0], LAYER_MAIN), "untrimmed small contour");
    }
    assert!(!has_line(&right.entities, [18.0, -10.0], [18.0, 10.0], LAYER_MAIN), "no false tangent edge");
    let left = shaft("S30 L10 | S20 L20 FI2@L:CONCAVE");
    assert!(has_arc(&left.entities, [12.0, 12.0], 2.0, 180.0, 270.0));
    assert!(has_arc(&left.entities, [12.0, -12.0], 2.0, 90.0, 180.0));
    for sign in [1.0, -1.0] {
        assert!(has_line(&left.entities, [12.0, sign * 10.0], [30.0, sign * 10.0], LAYER_MAIN));
        assert!(has_line(&left.entities, [10.0, sign * 12.0], [10.0, sign * 15.0], LAYER_MAIN));
        assert!(has_line(&left.entities, [0.0, sign * 15.0], [10.0, sign * 15.0], LAYER_MAIN));
    }
}

#[test]
fn legacy_convex_still_lands_on_the_large_side_and_works_at_free_ends() {
    let a = shaft("S20 L20 FI2@R | S30 L10");
    assert_eq!(a, shaft("S20 L20 | S30 L10 FI2@L"));
    assert!(has_arc(&a.entities, [22.0, 13.0], 2.0, 90.0, 180.0));
    assert!(has_arc(&a.entities, [22.0, -13.0], 2.0, 180.0, 270.0));
    assert!(has_line(&a.entities, [0.0, 10.0], [20.0, 10.0], LAYER_MAIN));
    assert!(has_line(&a.entities, [22.0, 15.0], [30.0, 15.0], LAYER_MAIN));
    let b = shaft("S30 L10 FI2@R | S20 L20");
    assert_eq!(b, shaft("S30 L10 | S20 L20 FI2@L"));
    assert!(has_arc(&b.entities, [8.0, 13.0], 2.0, 0.0, 90.0));
    assert!(has_arc(&b.entities, [8.0, -13.0], 2.0, 270.0, 360.0));
    let free = shaft("S30 L40 FI2@L FI3@R");
    assert_eq!(arc_count(&free), 4);
    assert!(has_arc(&free.entities, [2.0, 13.0], 2.0, 90.0, 180.0));
    assert!(has_arc(&free.entities, [37.0, 12.0], 3.0, 0.0, 90.0));
}

#[test]
fn concave_requires_a_neighbor_and_a_real_step() {
    for text in [
        "S20 L20 FI2@L:CONCAVE",
        "S20 L20 FI2:CONCAVE",
        "S20 L20 FI2@L:CONCAVE | S30 L10",
        "S30 L10 | S20 L20 FI2@R:CONCAVE",
        "S20 L20 FI2@R:CONCAVE | S20 L10",
        "S20 L20 | S20 L10 FI2@L:CONCAVE",
        "S20 E30 L20 FI2@R:CONCAVE | S30 L10",
        r#"{"segments":[{"s":20,"l":20,"fillet":{"r":2,"kind":"concave"}}]}"#,
    ] {
        assert_rejected(text);
    }
}

#[test]
fn concave_checks_actual_small_segment_length_step_and_opposite_end_clearance() {
    for text in [
        "S20 L20 FI6@R:CONCAVE | S30 L10",
        "S30 L10 | S20 L20 FI6@L:CONCAVE",
        "S20 L1 FI2@R:CONCAVE | S40 L100",
        "S20 L1 | S40 L100 FI2@L:CONCAVE",
        "S40 L100 FI2@R:CONCAVE | S20 L1",
        "S40 L10 | S20 L3 FI2@L:CONCAVE FI2@R:CONCAVE | S40 L10",
        "S20 L3 CH1@L FI2.5@R:CONCAVE | S40 L10",
    ] {
        assert_rejected(text);
    }
}

#[test]
fn duplicate_same_kind_requests_on_one_physical_face_are_rejected() {
    for text in [
        "S20 L20 FI2@R | S30 L10 FI2@L",
        "S30 L10 FI2@R | S20 L20 FI1@L",
        "S20 L20 FI2@R:CONCAVE | S30 L10 FI2@L:CONCAVE",
        "S30 L10 FI1@R:CONCAVE | S20 L20 FI2@L:CONCAVE",
    ] {
        assert_rejected(text);
    }
    let mut p = program("S20 L20 FI2@R:CONCAVE | S30 L10");
    p.segments[1].fillet.push(Fillet { r: 1.0, end: End::L, kind: FilletKind::Concave });
    assert_model_rejected(&p);
}

#[test]
fn small_concave_and_neighbor_large_chamfer_can_share_distinct_face_corners() {
    let right_text = "S20 L20 FI2@R:CONCAVE | S30 L10 CH1@L";
    let right = shaft(right_text);
    assert!(has_arc(&right.entities, [18.0, 12.0], 2.0, 270.0, 360.0));
    for sign in [1.0, -1.0] {
        assert!(has_line(&right.entities, [20.0, sign * 14.0], [21.0, sign * 15.0], LAYER_MAIN));
        assert!(has_line(&right.entities, [20.0, sign * 12.0], [20.0, sign * 14.0], LAYER_MAIN));
        assert!(has_line(&right.entities, [21.0, sign * 15.0], [30.0, sign * 15.0], LAYER_MAIN));
    }
    let left_text = "S30 L10 CH1@R | S20 L20 FI2@L:CONCAVE";
    let left = shaft(left_text);
    assert!(has_arc(&left.entities, [12.0, 12.0], 2.0, 180.0, 270.0));
    for sign in [1.0, -1.0] {
        assert!(has_line(&left.entities, [9.0, sign * 15.0], [10.0, sign * 14.0], LAYER_MAIN));
        assert!(has_line(&left.entities, [10.0, sign * 12.0], [10.0, sign * 14.0], LAYER_MAIN));
    }
    for text in [right_text, left_text] {
        let section = shaft(&format!("{text} VIEW 剖视"));
        assert_section_rotations(&section);
    }
}

#[test]
fn shared_face_concave_and_chamfer_must_leave_noncolliding_clearance() {
    for text in [
        "S20 L20 FI2@R:CONCAVE | S30 L10 CH3@L",
        "S20 L20 FI3@R:CONCAVE | S30 L10 CH3@L",
        "S30 L10 CH3@R | S20 L20 FI2@L:CONCAVE",
        "S30 L10 CH3@R | S20 L20 FI3@L:CONCAVE",
    ] {
        assert_rejected(text);
    }
}

#[test]
fn concave_next_to_gear_keeps_both_automatic_convex_chamfers() {
    let right_text = format!("S85 L20 FI3@R:CONCAVE | {GEAR_SPEC}");
    let right = shaft(&right_text);
    assert!(has_arc(&right.entities, [17.0, 45.5], 3.0, 270.0, 360.0));
    assert!(has_arc(&right.entities, [17.0, -45.5], 3.0, 0.0, 90.0));
    for sign in [1.0, -1.0] {
        assert!(has_line(&right.entities, [20.0, sign * 79.0], [22.0, sign * 81.0], LAYER_MAIN));
        assert!(has_line(&right.entities, [56.0, sign * 81.0], [58.0, sign * 79.0], LAYER_MAIN));
        assert!(has_line(&right.entities, [22.0, sign * 81.0], [56.0, sign * 81.0], LAYER_MAIN));
        assert!(has_line(&right.entities, [20.0, sign * 45.5], [20.0, sign * 79.0], LAYER_MAIN));
    }
    let left_text = format!("{GEAR_SPEC} | S85 L20 FI3@L:CONCAVE");
    let left = shaft(&left_text);
    assert!(has_arc(&left.entities, [41.0, 45.5], 3.0, 180.0, 270.0));
    assert!(has_arc(&left.entities, [41.0, -45.5], 3.0, 90.0, 180.0));
    for sign in [1.0, -1.0] {
        assert!(has_line(&left.entities, [0.0, sign * 79.0], [2.0, sign * 81.0], LAYER_MAIN));
        assert!(has_line(&left.entities, [36.0, sign * 81.0], [38.0, sign * 79.0], LAYER_MAIN));
    }
    for text in [&right_text, &left_text] {
        assert_section_rotations(&shaft(&format!("{text} VIEW 剖视")));
    }
}

#[test]
fn fillet_must_not_actually_land_on_the_gear_body() {
    for text in [
        format!("S85 L20 FI3@R | {GEAR_SPEC}"),
        format!("{GEAR_SPEC} | S85 L20 FI3@L"),
        format!("{GEAR_SPEC} FI3@L"),
        format!("{GEAR_SPEC} FI3@R:CONCAVE"),
    ] {
        assert_rejected(&text);
    }
    let mut p = program(&format!("S85 L20 | {GEAR_SPEC}"));
    p.segments[1].fillet.push(Fillet { r: 3.0, end: End::R, kind: FilletKind::Convex });
    assert_model_rejected(&p);
}

#[test]
fn concave_cone_arcs_are_tangent_to_the_original_cone_and_shoulder() {
    // Cover both taper signs and shallow/steep cones through public entities only.
    for (s, e, l) in [(20.0, 30.0, 30.0), (30.0, 20.0, 30.0), (20.0, 60.0, 5.0), (60.0, 20.0, 5.0)] {
        for end in [End::L, End::R] {
            let (text, x0, face_x, face_radius) = match end {
                End::L => (format!("S100 L10 | S{s} E{e} L{l} FI0.5@L:CONCAVE"), 10.0, 10.0, s / 2.0),
                End::R => (format!("S{s} E{e} L{l} FI0.5@R:CONCAVE | S100 L10"), 0.0, l, e / 2.0),
            };
            let model = shaft(&text);
            assert_eq!(arc_count(&model), 2);
            let upper = model.entities.iter().find_map(|entity| match entity {
                EntityType::Arc(a) if a.center.y > 0.0 => Some(a),
                _ => None,
            }).unwrap();
            let c = [upper.center.x, upper.center.y];
            let endpoints = [upper.start_angle, upper.end_angle].map(|angle| {
                [c[0] + upper.radius * angle.cos(), c[1] + upper.radius * angle.sin()]
            });
            let (face, contour) = if near(endpoints[0][0], face_x) {
                (endpoints[0], endpoints[1])
            } else {
                assert!(near(endpoints[1][0], face_x), "no shoulder tangency: {text}");
                (endpoints[1], endpoints[0])
            };
            let slope = (e - s) / (2.0 * l);
            let radial = [contour[0] - c[0], contour[1] - c[1]];
            assert!(near(upper.radius, 0.5));
            assert!(near((c[0] - face_x).abs(), upper.radius));
            assert!(near(face[1], c[1]), "circle tangent to vertical shoulder");
            assert!(face[1] > face_radius, "concave must rise above the small cone end");
            assert!(near(contour[1], s / 2.0 + slope * (contour[0] - x0)));
            assert!(near(radial[0] + slope * radial[1], 0.0), "circle tangent to cone");
            assert!(near(radial[0].hypot(radial[1]), upper.radius));
            assert!(contour[0] >= x0 - EPS && contour[0] <= x0 + l + EPS);
            assert!(c[1] > s / 2.0 + slope * (c[0] - x0), "center on concave side");
            assert!(has_line(&model.entities, face, [face_x, 50.0], LAYER_MAIN));
            assert!(model.entities.iter().any(|entity| match entity {
                EntityType::Line(line) if line.common.layer == LAYER_MAIN => {
                    let a = [line.start.x, line.start.y];
                    let b = [line.end.x, line.end.y];
                    (point_near(a, contour) || point_near(b, contour))
                        && near(b[1] - a[1], slope * (b[0] - a[0]))
                        && (b[0] - a[0]).abs() > EPS
                }
                _ => false,
            }), "trimmed cone must end exactly at tangency: {text}");
            assert!(has_arc(&model.entities, [c[0], -c[1]], upper.radius,
                -upper.end_angle.to_degrees(), -upper.start_angle.to_degrees()));
            assert_closed_hatch(&shaft(&format!("{text} VIEW 剖视")));
        }
    }
}

#[test]
fn native_hatch_preserves_concave_and_convex_arc_directions_when_rotated() {
    let section = shaft("S20 L20 FI1@L FI2@R:CONCAVE | S30 L10 VIEW 剖视");
    assert_eq!(arc_count(&section), 4);
    assert_hatch_arc(&section, [18.0, 12.0], 2.0, [[18.0, 10.0], [20.0, 12.0]], true);
    assert_hatch_arc(&section, [18.0, -12.0], 2.0, [[20.0, -12.0], [18.0, -10.0]], true);
    assert_hatch_arc(&section, [1.0, 9.0], 1.0, [[0.0, 9.0], [1.0, 10.0]], false);
    assert_hatch_arc(&section, [1.0, -9.0], 1.0, [[1.0, -10.0], [0.0, -9.0]], false);
    let arcs: Vec<_> = native_hatch(&section).paths.iter().flat_map(|p| &p.edges)
        .filter_map(|edge| match edge { BoundaryEdge::CircularArc(a) => Some(a), _ => None })
        .collect();
    assert_eq!(arcs.len(), 4, "native circles, not tessellated fillet chords");
    for a in arcs {
        assert!(near((a.end_angle - a.start_angle).rem_euclid(std::f64::consts::TAU),
            std::f64::consts::FRAC_PI_2), "quarter arc stored in radians");
    }
    assert_section_rotations(&section);
    let mirrored = shaft("S30 L10 | S20 L20 FI2@L:CONCAVE VIEW 剖视");
    assert_hatch_arc(&mirrored, [12.0, 12.0], 2.0, [[10.0, 12.0], [12.0, 10.0]], true);
    assert_section_rotations(&mirrored);
}

#[test]
fn explicit_slot_length_dsl_json_aliases_and_default_legacy_l_roundtrip() {
    let p = program("S65 L98 KEY C 50 @端 SL50 t1 7 | S75 L23");
    let key = p.segments[0].keyway.as_ref().unwrap();
    assert_eq!(key.kind, KeyKind::C);
    assert_eq!(key.place, KeywayPlace::End);
    assert_eq!(key.slot_l, Some(50.0));
    assert!(near(key.l, 50.0), "keep the positive legacy key-length field");
    assert!(near(keyway_b(key, 65.0).unwrap(), 18.0));
    assert!(near(key.slot_len(7.0, 18.0), 50.0), "SL is total, not L+t1");
    for field in ["slot_l", "slot_length"] {
        for include_legacy_l in [true, false] {
            let mut input = serde_json::json!({"segments": [
                {"s": 65, "l": 98, "keyway": {"type": "C", "place": "end", "t1": 7}},
                {"s": 75, "l": 23}
            ]});
            input["segments"][0]["keyway"][field] = serde_json::json!(50);
            if include_legacy_l {
                input["segments"][0]["keyway"]["l"] = serde_json::json!(50);
            }
            let parsed = program(&input.to_string());
            assert_eq!(parsed, p, "{field}, explicit legacy L: {include_legacy_l}");
            assert_eq!(build(&parsed, 1.0).unwrap(), build(&p, 1.0).unwrap());
        }
    }
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["segments"][0]["keyway"]["slot_l"].as_f64(), Some(50.0));
    assert!(json["segments"][0]["keyway"].get("slot_length").is_none());
    assert_eq!(program(&json.to_string()), p);
}

#[test]
fn explicit_end_slot_has_total_50_width_18_and_round_head_at_41() {
    for (text, center, angles, flat_x, wall, face) in [
        ("S65 L98 KEY C 50 @端 SL50 t1 7 | S75 L23", 41.0, [270.0, 450.0], 7.0, 50.0, 0.0),
        ("S75 L23 | S65 L98 KEY C 50 @端 SL50 t1 7", 80.0, [90.0, 270.0], 114.0, 71.0, 121.0),
    ] {
        let s = shaft(text);
        assert_eq!(arc_count(&s), 1);
        assert!(has_arc(&s.entities, [center, 0.0], 9.0, angles[0], angles[1]));
        assert!(near((center - wall).abs(), 9.0), "rounded head's extreme is the slot wall");
        assert!(near((wall - face).abs(), 50.0));
        for sign in [1.0, -1.0] {
            assert!(has_line(&s.entities, [face, sign * 9.0], [center, sign * 9.0], LAYER_MAIN));
            assert!(!has_line(&s.entities, [flat_x, sign * 9.0],
                [face, sign * (9.0 + 9.0 * 20.0_f64.to_radians().tan())], "0"), "SL draws the open slot, not an assembly-key lead-in");
        }
        let section = shaft(&format!("{text} VIEW 剖视"));
        assert!(has_line(&section.entities, [face, 25.5], [wall, 25.5], LAYER_MAIN));
        assert!(has_line(&section.entities, [wall, 25.5], [wall, 32.5], LAYER_MAIN));
        assert_closed_hatch(&section);
    }
}

#[test]
fn explicit_mid_slot_total_overrides_key_kind_length_conversion() {
    // C normally adds b/2 for a mid slot. SL is already the actual total for both A and C.
    for kind in ["A", "C"] {
        let text = format!("S65 L98 KEY {kind} 50 SL50 t1 7");
        let p = program(&text);
        let key = p.segments[0].keyway.as_ref().unwrap();
        assert!(near(key.l, 50.0));
        assert!(near(key.slot_len(7.0, 18.0), 50.0));
        let s = build(&p, 1.0).unwrap();
        assert_eq!(arc_count(&s), 2);
        assert!(has_arc(&s.entities, [33.0, 0.0], 9.0, 90.0, 270.0));
        assert!(has_arc(&s.entities, [65.0, 0.0], 9.0, 270.0, 450.0));
        let section = shaft(&format!("{text} VIEW 剖视"));
        assert!(has_line(&section.entities, [24.0, 25.5], [74.0, 25.5], LAYER_MAIN));
        assert_closed_hatch(&section);
    }
}

#[test]
fn explicit_end_slot_accepts_the_t1_plus_half_width_minimum() {
    // Keep legacy L=50; SL need not itself be a standard key length. At 7+18/2=16,
    // the flat-to-arc-center straight has zero length, but the round end still fits.
    let text = "S65 L98 KEY C 50 @端 SL16 t1 7 | S75 L23";
    let p = program(text);
    validate(&p).unwrap();
    let s = build(&p, 1.0).unwrap();
    assert!(has_arc(&s.entities, [7.0, 0.0], 9.0, 270.0, 450.0));
    let section = shaft(&format!("{text} VIEW 剖视"));
    assert!(has_line(&section.entities, [0.0, 25.5], [16.0, 25.5], LAYER_MAIN));
    assert!(has_line(&section.entities, [16.0, 25.5], [16.0, 32.5], LAYER_MAIN));
    assert_closed_hatch(&section);
}

#[test]
fn absent_sl_preserves_legacy_l_plus_t1_geometry_and_omits_json_field() {
    let legacy = program("S65 L98 KEY C 50 @端 t1 7 | S75 L23");
    let key = legacy.segments[0].keyway.as_ref().unwrap();
    assert_eq!(key.slot_l, None);
    assert!(near(key.l, 50.0));
    assert!(near(key.slot_len(7.0, 18.0), 57.0));
    let json = serde_json::to_value(&legacy).unwrap();
    assert!(json["segments"][0]["keyway"].get("slot_l").is_none());
    assert_eq!(program(&json.to_string()), legacy);
    let old_geometry = build(&legacy, 1.0).unwrap();
    assert!(has_arc(&old_geometry.entities, [48.0, 0.0], 9.0, 270.0, 450.0));
    let explicit_57 = program("S65 L98 KEY C 50 @端 SL57 t1 7 | S75 L23");
    let independent_57 = build(&explicit_57, 1.0).unwrap();
    assert!(has_arc(&independent_57.entities, [48.0, 0.0], 9.0, 270.0, 450.0));
    assert!(has_line(&independent_57.entities, [0.0, 9.0], [48.0, 9.0], LAYER_MAIN));
    assert_ne!(old_geometry, independent_57, "SL draws the actual open slot, not the legacy assembly-key lead-in");
    let explicit_50 = program("S65 L98 KEY C 50 @端 SL50 t1 7 | S75 L23");
    assert_ne!(old_geometry, build(&explicit_50, 1.0).unwrap());
    assert_ne!(block_name(&legacy), block_name(&explicit_50));
}

#[test]
fn explicit_slot_rejects_nonfinite_nonpositive_short_and_unusable_lengths() {
    for token in ["SL", "SL0", "SL-1", "SLNaN", "SLinf", "SL1e999", "SL15.5", "SL99"] {
        assert_rejected(&format!("S65 L98 KEY C 50 @端 {token} t1 7 | S75 L23"));
    }
    for slot_l in [0.0, -1.0, 15.5, 99.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut p = program("S65 L98 KEY C 50 @端 SL50 t1 7 | S75 L23");
        p.segments[0].keyway.as_mut().unwrap().slot_l = Some(slot_l);
        assert_model_rejected(&p);
    }
    for field in ["slot_l", "slot_length"] {
        for value in [serde_json::json!(0), serde_json::json!(-1), serde_json::json!("NaN"), serde_json::json!(15.5), serde_json::json!(99)] {
            let mut input = serde_json::json!({"segments": [
                {"s": 65, "l": 98, "keyway": {"type": "C", "l": 50, "place": "end", "t1": 7}},
                {"s": 75, "l": 23}
            ]});
            input["segments"][0]["keyway"][field] = value;
            assert_rejected(&input.to_string());
        }
    }
    // t1+b/2 = 16: a 15.5-long end slot cannot contain its lead-in and round end.
    // A concave FI also consumes the small segment's actual available length.
    for text in [
        "S65 L51 FI2@R:CONCAVE KEY C 50 @端 SL50 t1 7 | S75 L23",
        "S75 L23 | S65 L51 FI2@L:CONCAVE KEY C 50 @端 SL50 t1 7",
        "S65 L49 KEY A 50 SL50 t1 7",
    ] {
        assert_rejected(text);
    }
    for l in [0.0, -50.0, f64::NAN, f64::INFINITY] {
        let mut p = program("S65 L98 KEY C 50 @端 SL50 t1 7 | S75 L23");
        p.segments[0].keyway.as_mut().unwrap().l = l;
        assert_model_rejected(&p);
    }
}

#[test]
fn sl_is_explicitly_incompatible_with_fixed_length_gb1097_guided_keyways() {
    // The control is otherwise valid; SL50 must not be silently ignored even when L is 50.
    let control = program("S65 L98 KEY A 50 导向");
    validate(&control).unwrap();
    build(&control, 1.0).unwrap();
    for text in [
        "S65 L98 KEY A 50 导向 SL50",
        "S65 L98 KEY A 50 SL50 GUIDED",
    ] {
        assert_rejected(text);
    }
    for field in ["slot_l", "slot_length"] {
        let mut input = serde_json::json!({"segments": [
            {"s": 65, "l": 98, "keyway": {"type": "A", "l": 50, "guided": true}}
        ]});
        input["segments"][0]["keyway"][field] = serde_json::json!(50);
        assert_rejected(&input.to_string());
    }
    let mut p = control;
    p.segments[0].keyway.as_mut().unwrap().slot_l = Some(50.0);
    assert_model_rejected(&p);
}

fn assert_demo_fillet_arcs(s: &Shaft) {
    for (center, r, angles) in [
        ([97.0, 33.5], 1.0, [270.0, 360.0]),
        ([138.0, 45.5], 3.0, [270.0, 360.0]),
        ([224.0, 45.5], 3.0, [180.0, 270.0]),
        ([241.8, 38.3], 0.8, [180.0, 270.0]),
    ] {
        assert!(has_arc(&s.entities, center, r, angles[0], angles[1]), "missing demo fillet {center:?}");
        assert!(has_arc(&s.entities, [center[0], -center[1]], r, -angles[1], -angles[0]));
    }
}

#[test]
fn concave_at_local_section_cut_keeps_its_root_branch() {
    let left = shaft("S30 L10 | S20 L20 FI2@L:CONCAVE | S30 L10 LOCALSECTION 10 30");
    assert!(has_arc(&left.entities, [12.0, 12.0], 2.0, 180.0, 270.0));
    assert_closed_hatch(&left);
    let right = shaft("S30 L10 | S20 L20 FI2@R:CONCAVE | S30 L10 LOCALSECTION 10 30");
    assert!(has_arc(&right.entities, [28.0, 12.0], 2.0, 270.0, 360.0));
    assert_closed_hatch(&right);
}

#[test]
fn concave_and_convex_fillets_share_a_face_without_colliding() {
    let s = shaft("S20 L20 FI2@R:CONCAVE | S30 L10 FI1@L VIEW 剖视");
    assert!(has_arc(&s.entities, [18.0, 12.0], 2.0, 270.0, 360.0));
    assert!(has_arc(&s.entities, [21.0, 14.0], 1.0, 90.0, 180.0));
    assert_section_rotations(&s);
    assert_rejected("S20 L20 FI3@R:CONCAVE | S30 L10 FI2@L");
}

#[test]
fn confirmed_demo_normal_is_264_by_162_with_four_concave_pairs_and_sl50() {
    let p = program(DEMO);
    validate(&p).unwrap();
    assert!(near(p.total_length(), 264.0));
    assert_eq!(p.segments.len(), 8);
    let s = build(&p, 1.0).unwrap();
    assert!(near(s.total_length, 264.0));
    assert!(near(s.max_diameter, 162.0));
    assert_eq!(s.segment_count, 8);
    assert_demo_fillet_arcs(&s);
    assert_eq!(arc_count(&s), 9, "four mirrored FI pairs plus one KEY semicircle");
    assert!(has_arc(&s.entities, [41.0, 0.0], 9.0, 270.0, 450.0));
    assert!(!has_arc(&s.entities, [48.0, 0.0], 9.0, 270.0, 450.0), "must not use legacy L+t1");
    assert!(has_line(&s.entities, [0.0, 9.0], [41.0, 9.0], LAYER_MAIN));
    assert!(has_line(&s.entities, [0.0, -9.0], [41.0, -9.0], LAYER_MAIN));
    assert!(has_line(&s.entities, [241.8, 37.5], [263.0, 37.5], LAYER_MAIN));
    for entity in &s.entities {
        assert!(matches!(entity, EntityType::Line(_) | EntityType::Arc(_)),
            "normal demo must have no HATCH, text, or dimensions: {entity:?}");
    }
    // Both gears retain the automatic C2 corners, including those sharing a root FI face.
    for (face, contour) in [
        ([141.0, 79.0], [143.0, 81.0]),
        ([179.0, 79.0], [177.0, 81.0]),
        ([183.0, 79.0], [185.0, 81.0]),
        ([221.0, 79.0], [219.0, 81.0]),
    ] {
        assert!(has_line(&s.entities, face, contour, LAYER_MAIN));
        assert!(has_line(&s.entities, [face[0], -face[1]], [contour[0], -contour[1]], LAYER_MAIN));
    }
}

#[test]
fn confirmed_demo_section_has_closed_native_fillet_rings_also_after_placement() {
    let s = shaft(&format!("{DEMO} VIEW 剖视"));
    assert!(near(s.total_length, 264.0));
    assert!(near(s.max_diameter, 162.0));
    assert_demo_fillet_arcs(&s);
    assert_eq!(arc_count(&s), 8, "KEY overlay is replaced by the section notch");
    assert!(has_line(&s.entities, [0.0, 25.5], [50.0, 25.5], LAYER_MAIN));
    assert!(has_line(&s.entities, [50.0, 25.5], [50.0, 32.5], LAYER_MAIN));
    assert!(has_line(&s.entities, [50.0, 32.5], [97.0, 32.5], LAYER_MAIN));
    let arcs: Vec<_> = native_hatch(&s).paths.iter().flat_map(|p| &p.edges)
        .filter_map(|edge| match edge { BoundaryEdge::CircularArc(a) => Some(a), _ => None })
        .collect();
    assert_eq!(arcs.len(), 8, "all four root pairs survive as native HATCH circular arcs");
    assert!(arcs.iter().all(|a| a.counter_clockwise));
    for entity in &s.entities {
        assert!(matches!(entity, EntityType::Line(_) | EntityType::Arc(_) | EntityType::Hatch(_)),
            "section demo must have no annotation entities: {entity:?}");
    }
    assert_section_rotations(&s);
}

#[test]
fn confirmed_demo_dsl_json_roundtrip_preserves_kind_slot_length_and_block_identity() {
    for view in [ShaftView::Normal, ShaftView::Section] {
        let mut p = program(DEMO);
        p.view = view;
        assert_eq!(p.segments.iter().flat_map(|s| &s.fillet)
            .filter(|f| f.kind == FilletKind::Concave).count(), 4);
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["segments"][0]["keyway"]["slot_l"].as_f64(), Some(50.0));
        assert_eq!(json["segments"][0]["keyway"]["l"].as_f64(), Some(50.0));
        let roundtrip = program(&json.to_string());
        assert_eq!(roundtrip, p);
        assert_eq!(block_name(&roundtrip), block_name(&p));
        assert_eq!(build(&roundtrip, 1.0).unwrap(), build(&p, 1.0).unwrap());
        let mut changed_kind = p.clone();
        changed_kind.segments[0].fillet[0].kind = FilletKind::Convex;
        assert_ne!(block_name(&changed_kind), block_name(&p));
        let mut changed_slot = p.clone();
        changed_slot.segments[0].keyway.as_mut().unwrap().slot_l = Some(51.0);
        assert_ne!(block_name(&changed_slot), block_name(&p));
    }
}
