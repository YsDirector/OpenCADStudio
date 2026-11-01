//! End-fillet regression tests, including actual landing, thread clipping and native hatch arcs.
use super::*;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-8
}

fn point_near(a: [f64; 2], b: [f64; 2]) -> bool {
    near(a[0], b[0]) && near(a[1], b[1])
}

fn shaft(text: &str) -> Shaft {
    build(&parse_program(text).unwrap(), 1.0).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn has_line(s: &Shaft, a: [f64; 2], b: [f64; 2], layer: &str) -> bool {
    s.entities.iter().any(|e| match e {
        EntityType::Line(l) if l.common.layer == layer => {
            let (p, q) = ([l.start.x, l.start.y], [l.end.x, l.end.y]);
            point_near(p, a) && point_near(q, b) || point_near(p, b) && point_near(q, a)
        }
        _ => false,
    })
}

fn has_arc(s: &Shaft, c: [f64; 2], r: f64, start: f64, end: f64) -> bool {
    s.entities.iter().any(|e| matches!(e, EntityType::Arc(a)
        if a.common.layer == LAYER_MAIN && point_near([a.center.x, a.center.y], c)
            && near(a.radius, r)
            && near(a.start_angle.cos(), start.to_radians().cos()) && near(a.start_angle.sin(), start.to_radians().sin())
            && near(a.end_angle.cos(), end.to_radians().cos()) && near(a.end_angle.sin(), end.to_radians().sin())))
}

fn assert_closed(s: &Shaft) {
    let hatch = s.entities.iter().find_map(|e| if let EntityType::Hatch(h) = e { Some(h) } else { None })
        .expect("native section hatch");
    assert_eq!(hatch.paths.len(), 2);
    for path in &hatch.paths {
        let points: Vec<_> = path.edges.iter().map(|e| match e {
            BoundaryEdge::Line(l) => ([l.start.x, l.start.y], [l.end.x, l.end.y]),
            BoundaryEdge::CircularArc(a) => {
                // DXF/codec CW angles are measured clockwise; decode independently of HatchEdge.
                let sign = if a.counter_clockwise { 1.0 } else { -1.0 };
                (
                    [a.center.x + a.radius * a.start_angle.cos(), a.center.y + sign * a.radius * a.start_angle.sin()],
                    [a.center.x + a.radius * a.end_angle.cos(), a.center.y + sign * a.radius * a.end_angle.sin()],
                )
            },
            other => panic!("unexpected hatch edge {other:?}"),
        }).collect();
        for i in 0..points.len() {
            let a = points[i].1;
            let b = points[(i + 1) % points.len()].0;
            assert!(point_near(a, b), "hatch gap at {i}: {a:?} -> {b:?}");
        }
    }
}

#[test]
fn fillet_dsl_json_roundtrip_and_defaults() {
    let p = parse_program("s30 l40 fi=2@left fi:3@右").unwrap();
    assert_eq!(p.segments[0].fillet, vec![Fillet { r: 2.0, end: End::L, kind: FilletKind::Convex }, Fillet { r: 3.0, end: End::R, kind: FilletKind::Convex }]);
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(parse_program(&json).unwrap(), p);
    let p = parse_program(r#"{"segments":[{"s":30,"l":40,"fillet":{"r":2}}]}"#).unwrap();
    assert_eq!(p.segments[0].fillet, vec![Fillet { r: 2.0, end: End::R, kind: FilletKind::Convex }]);
    assert_eq!(p, parse_program("S30 L40 FI2").unwrap());
    let plain = parse_program("S30 L40").unwrap();
    assert!(!serde_json::to_string(&plain).unwrap().contains("fillet"));
    assert_ne!(block_name(&p), block_name(&plain));
    assert!(build_report(&p).unwrap().contains("FI R2@R"));
}

#[test]
fn fillet_ch_mutual_exclusion_is_same_end_in_all_inputs() {
    for text in [
        "S30 L40 FI2@L CH2@L", "S30 L40 CH2@R FI2@R", "FI2 CH2@R S30 L40",
        r#"{"segments":[{"s":30,"l":40,"fillet":{"r":2,"end":"L"},"ch":{"c":2,"end":"L"}}]}"#,
    ] {
        let e = parse_program(text).unwrap_err();
        assert!(e.contains("FI") && e.contains("CH"), "{e}");
    }
    let mut p = parse_program("S30 L40 FI2").unwrap();
    p.segments[0].ch.push(Chamfer { c: 1.0, end: End::R });
    assert!(validate(&p).unwrap_err().contains("CH"));
}

#[test]
fn fillet_invalid_parameters_never_silently_drop() {
    for text in [
        "S30 L40 FI", "S30 L40 FI0", "S30 L40 FI-2", "S30 L40 FI2@", "S30 L40 FI2@X",
        "S30 L40 FI2 FI3@R", "S30 L40 FI1e999", "S30 L40 FI2@L FI3@L",
        r#"{"segments":[{"s":30,"l":40,"fillet":{"r":0}}]}"#,
        r#"{"segments":[{"s":30,"l":40,"fillet":{"r":2,"end":"X"}}]}"#,
        r#"{"segments":[{"s":30,"l":40,"fillet":{"c":2}}]}"#,
    ] {
        assert!(parse_program(text).is_err(), "accepted invalid {text}");
    }
    let mut p = parse_program("S30 L40 FI2").unwrap();
    p.segments[0].fillet[0].r = f64::NAN;
    assert!(validate(&p).is_err());
    p.segments[0].fillet[0].r = f64::INFINITY;
    assert!(validate(&p).is_err());
}

#[test]
fn fillet_free_end_quarter_arcs_trim_outline_and_face() {
    let s = shaft("S30 L40 FI2@L FI3@R");
    assert_eq!(s.total_length, 40.0);
    assert_eq!(s.max_diameter, 30.0);
    assert!(has_arc(&s, [2.0, 13.0], 2.0, 90.0, 180.0));
    assert!(has_arc(&s, [2.0, -13.0], 2.0, 180.0, 270.0));
    assert!(has_arc(&s, [37.0, 12.0], 3.0, 0.0, 90.0));
    assert!(has_arc(&s, [37.0, -12.0], 3.0, 270.0, 360.0));
    assert!(has_line(&s, [2.0, 15.0], [37.0, 15.0], LAYER_MAIN));
    assert!(has_line(&s, [0.0, -13.0], [0.0, 13.0], LAYER_MAIN));
    assert!(has_line(&s, [40.0, -12.0], [40.0, 12.0], LAYER_MAIN));
    assert!(!has_line(&s, [0.0, 13.0], [2.0, 15.0], LAYER_MAIN), "not a chamfer");
    assert!(!has_line(&s, [2.0, -15.0], [2.0, 15.0], LAYER_MAIN), "smooth tangency has no false edge");
}

#[test]
fn fillet_internal_face_lands_on_larger_side_from_either_requester() {
    for (a, b, center, angles) in [
        ("S20 L10 FI2@R | S30 L20", "S20 L10 | S30 L20 FI2@L", [12.0, 13.0], [90.0, 180.0]),
        ("S30 L20 FI2@R | S20 L10", "S30 L20 | S20 L10 FI2@L", [18.0, 13.0], [0.0, 90.0]),
    ] {
        let sa = shaft(a);
        let sb = shaft(b);
        assert_eq!(sa, sb, "anchor choice must not change actual landing");
        assert!(has_arc(&sa, center, 2.0, angles[0], angles[1]));
    }
}

#[test]
fn fillet_cones_are_tangent_to_both_original_edges() {
    for (s, e, l) in [(20.0, 40.0, 30.0), (40.0, 20.0, 30.0), (20.0, 80.0, 5.0), (80.0, 20.0, 5.0)] {
        let p = parse_program(&format!("S{s} E{e} L{l}")).unwrap();
        let seg = &p.segments[0];
        let m = (e - s) / (2.0 * l);
        for end in [End::L, End::R] {
            let g = fillet_geom(seg, 7.0, end, 0.5);
            assert!(near((g.c[0] - g.face[0]).abs(), g.r));
            assert!(near(g.contour[1], radius_at(seg, 7.0, g.contour[0])));
            let radial = [g.contour[0] - g.c[0], g.contour[1] - g.c[1]];
            assert!(near(radial[0] + m * radial[1], 0.0), "circle tangent to cone");
            assert!(near(radial[0].hypot(radial[1]), g.r));
            check_fillet_geom(seg, g, 0.0, "test").unwrap();
        }
    }
    assert_closed(&shaft("S30 E40 L40 FI2@L FI2@R VIEW 剖视"));
    assert_closed(&shaft("S40 E30 L40 FI2@L FI2@R VIEW 剖视"));
}

#[test]
fn fillet_face_and_actual_landing_length_errors() {
    for text in [
        "S30 L40 FI15@L", "S30 L40 FI16@R", "S30 L3 FI2@L FI2@R",
        "S20 L10 FI5@R | S30 L20", "S20 L10 | S20 L20 FI2@L",
        "S20 L100 FI3@R | S40 L2", "S40 L2 | S20 L100 FI3@L",
        "S30 E60 L2 FI2@R", "S60 E30 L2 FI2@L",
    ] {
        let p = parse_program(text).unwrap();
        assert!(build(&p, 1.0).is_err(), "accepted invalid geometry {text}");
    }
}

#[test]
fn fillet_boundary_conflicts_and_special_segments_rejected() {
    for text in [
        "S20 L10 FI2@R | S30 L20 FI2@L", "S20 L10 CH2@R | S30 L20 FI2@L",
        "S20 L10 FI2@R | S30 L20 CH2@L", "S40 L30 CH2@R | S20 L20 FI2@L",
        "S30 L10 | S20 L30 RL@L P1.5 FI2@L", "S40 L30 M1.5 TL20 FI2@R | S36 L5",
        "S40 L30 M1.5 RL FI2@R", "S40 L30 M1.5 TL20 | S36 L5 FI2@L",
        "GEAR M3 Z20 FI2", "SPLINE 6x23x26x6 L30 FI2",
        "S20 L10 FI2@R | GEAR M3 Z20", "S20 L10 FI1@R | SPLINE 6x23x26x6 L30",
    ] {
        let result = parse_program(text).and_then(|p| build(&p, 1.0));
        assert!(result.is_err(), "accepted conflict {text}");
    }
}

#[test]
fn fillet_section_native_arcs_close_with_grooves_threads_and_keyways() {
    for text in [
        "S30 L40 FI2@L FI3@R", "S20 L10 FI2@R | S30 L20",
        "S30 L20 | S20 L10 FI2@L", "S20 L30 OV3 FI2@R | S40 L30",
        "S30 L50 M1.5 FI2@L FI3@R", "S40 L40 M1.5 FI2@L TL20 | S36 L5",
        "S25 L40 FI2@L KEY A 18 | S30 L30",
        "S25 L40 FI2@L KEY C 14 @端 | S30 L30",
        "S30 L30 | S25 L40 FI2@R KEY C 14 @端",
        "S25 L40 FI2@L KEY C 14 @端 双槽 | S30 L30",
    ] {
        assert_closed(&shaft(&format!("{text} VIEW 剖视")));
    }
    let s = shaft("S30 L40 FI2@L FI3@R VIEW 剖视");
    let arcs = s.entities.iter().filter_map(|e| if let EntityType::Hatch(h) = e { Some(h) } else { None })
        .flat_map(|h| &h.paths).flat_map(|p| &p.edges)
        .filter(|e| matches!(e, BoundaryEdge::CircularArc(_))).count();
    assert_eq!(arcs, 4, "native fillet arcs survive in hatch boundaries");
}

#[test]
fn fillet_thread_minor_lines_clip_to_circle_not_chamfer_chord() {
    let s = shaft("S30 L40 M1.5 FI2@L FI3@R");
    let minor: f64 = (30.0 - 1.0825 * 1.5) / 2.0;
    let x0 = 2.0 - (4.0 - (minor - 13.0_f64).powi(2)).sqrt();
    let x1 = 37.0 + (9.0 - (minor - 12.0_f64).powi(2)).sqrt();
    assert!(has_line(&s, [x0, minor], [x1, minor], LAYER_THIN));
    assert!(has_line(&s, [x0, -minor], [x1, -minor], LAYER_THIN));
    assert_closed(&shaft("S30 L40 M1.5 FI2@L FI3@R VIEW 剖视"));
}

#[test]
fn fillet_keeps_relief_r_keyword_and_ov_combination_compatible() {
    let p = parse_program("S35 L10 | S25 L30 RL@L g1 2.5 g2 4.5 dg 22.7 r 0.8").unwrap();
    assert_eq!(p.segments[1].relief[0].r, Some(0.8));
    assert!(p.segments[1].fillet.is_empty());
    let s = shaft("S20 L30 FI2@R OV3 | S40 L30");
    assert!(has_arc(&s, [32.0, 18.0], 2.0, 90.0, 180.0));
}

#[test]
fn fillet_preview_placement_and_local_section_work() {
    let p = parse_program("S20 L10 FI2@R | S30 L20 | S20 L10").unwrap();
    assert!(preview_svg(&p).unwrap().contains("<svg"));
    let s = build(&p, 1.0).unwrap();
    let placed = place(s.entities, [100.0, 50.0], 90.0);
    assert!(placed.iter().any(|e| matches!(e, EntityType::Arc(a)
        if near(a.center.x, 87.0) && near(a.center.y, 62.0) && near(a.radius, 2.0))));
    let local = shaft("S20 L10 FI2@R | S30 L20 | S20 L10 LOCALSECTION 10 30");
    assert_closed(&local);
    assert!(has_arc(&local, [12.0, 13.0], 2.0, 90.0, 180.0), "local section must retain neighbour-requested FI");
    let local_right = shaft("S20 L10 | S30 L20 | S20 L10 FI2@L LOCALSECTION 10 30");
    assert!(has_arc(&local_right, [28.0, 13.0], 2.0, 0.0, 90.0));
}

#[test]
fn fillet_keyway_uses_actual_landing_not_request_anchor() {
    let a = shaft("S25 L40 KEY A 18 FI2@R | S40 L30");
    let b = shaft("S25 L40 KEY A 18 | S40 L30 FI2@L");
    assert_eq!(a, b, "neighbouring FI must not shorten or shift the KEY segment");
    assert!(has_line(&a, [0.0, 12.5], [40.0, 12.5], LAYER_MAIN));
    for text in [
        "S10 L10 FI4@R | S25 L20 KEY A 18 VIEW 剖视",
        "S10 L10 | S25 L20 KEY A 18 FI4@L VIEW 剖视",
        "S25 L20 FI4@R KEY C 14 @端 | S10 L10 VIEW 剖视",
        "S10 L10 | S25 L20 FI4@L KEY C 14 @端 VIEW 剖视",
    ] {
        let p = parse_program(text).unwrap();
        assert!(validate(&p).is_err(), "keyway must fit actual fillet clearance: {text}");
        assert!(build(&p, 1.0).is_err(), "keyway must not extend outside material: {text}");
    }
    assert_closed(&shaft("S10 L10 FI2@R | S25 L30 KEY A 18 VIEW 剖视"));
    let a = shaft("S10 L10 FI2@R | S25 L40 KEY A 18 CH2@R | S40 L30");
    let b = shaft("S10 L10 FI2@R | S25 L40 KEY A 18 | S40 L30 CH2@L");
    assert_eq!(a, b, "outgoing CH must not shorten or shift a KEY with an incoming FI");
    assert!(has_line(&a, [12.0, 12.5], [50.0, 12.5], LAYER_MAIN));
    let local_a = shaft("S10 L10 FI2@R | S25 L40 KEY A 18 CH2@R | S40 L30 LOCALSECTION 10 50");
    let local_b = shaft("S10 L10 FI2@R | S25 L40 KEY A 18 | S40 L30 CH2@L LOCALSECTION 10 50");
    assert_eq!(local_a, local_b, "local slice must not turn an outgoing CH into a free-end chamfer");
    assert_closed(&local_a);
}

#[test]
fn fillet_cannot_intrude_into_specified_thread_length_or_runout() {
    for text in [
        "S30 L6 M1.5 TL1 FI4@L | S40 L10",
        "S30 L9 M1.5 TL3 FI2@L | S40 L10",
    ] {
        let p = parse_program(text).unwrap();
        let e = build(&p, 1.0).unwrap_err();
        assert!(e.contains("FI") && e.contains("TL"), "{text}: {e}");
    }
    assert_closed(&shaft("S30 L10 M1.5 TL3 FI2@L | S40 L10 VIEW 剖视"));
}

#[test]
fn fillet_native_clockwise_hatch_angles_and_rotations_match_outline() {
    let s = shaft("S30 L40 FI2@L FI3@R VIEW 剖视");
    for degrees in [0.0, 30.0, 90.0, -27.0, 180.0] {
        let rotated = Shaft { entities: place(s.entities.clone(), [70.0, -50.0], degrees), ..s.clone() };
        assert_closed(&rotated);
    }
    let hatch = s.entities.iter().find_map(|e| if let EntityType::Hatch(h) = e { Some(h) } else { None }).unwrap();
    for edge in hatch.paths.iter().flat_map(|p| &p.edges) {
        if let BoundaryEdge::CircularArc(a) = edge {
            assert!(!a.counter_clockwise);
            assert!(near((a.end_angle - a.start_angle).rem_euclid(std::f64::consts::TAU), std::f64::consts::FRAC_PI_2));
        }
    }
    let lines = hatch_preview_lines(hatch);
    assert!(!lines.is_empty());
    for e in lines {
        if let EntityType::Line(l) = e {
            for p in [l.start, l.end] {
                assert!(p.x >= -1e-8 && p.x <= 40.0 + 1e-8);
                let ymax = if p.x < 2.0 {
                    13.0 + (4.0 - (p.x - 2.0).powi(2)).max(0.0).sqrt()
                } else if p.x > 37.0 {
                    12.0 + (9.0 - (p.x - 37.0).powi(2)).max(0.0).sqrt()
                } else { 15.0 };
                assert!(p.y.abs() <= ymax + 1e-8, "hatch outside rounded material: {p:?}");
            }
        }
    }
}

#[test]
fn fillet_errors_are_bilingual_without_missing_keys() {
    for lang in [crate::i18n::Lang::Zh, crate::i18n::Lang::En] {
        crate::i18n::set_lang(lang);
        crate::i18n::clear_missing_keys();
        let error = parse_program("S30 L40 FI2 CH2@R").unwrap_err();
        assert!(error.contains("FI") && error.contains("CH"));
        assert!(!error.contains("cmd.shaft"));
        assert!(crate::i18n::missing_keys().is_empty());
        assert!(usage().contains("FI2@L"));
    }
    crate::i18n::set_lang_auto();
}
