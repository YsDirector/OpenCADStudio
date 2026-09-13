//! 视觉冒烟：引导版线性标注的文字被拖到尺寸线范围外时，尺寸线应沿轴延伸到
//! 文字底下（GB/T 4458.4 / AutoCAD DIMTMOVE=1 画法）。
//!
//! 与 `pdf_export_text_check.rs` 同套路：Scene::entity_wires() → export_pdf()，
//! 只是把结果写成便于人工/图像核对的 PDF。设 `OCSM_DIM_LEADER_PDF` 才落盘。
use acadrust::entities::{Dimension, DimensionLinear};
use acadrust::types::Vector3;
use acadrust::xdata::{ExtendedDataRecord, XDataValue};
use acadrust::EntityType;
use OpenCADStudio::io::pdf_export::{export_pdf, PdfPlotOptions, PlotWire};
use OpenCADStudio::scene::Scene;

/// 造 OCS 风格的实体级 DSTYLE 覆盖记录（与 src/entities/dim_override.rs 同形）。
fn dstyle(pairs: &[(i16, XDataValue)]) -> ExtendedDataRecord {
    let mut rec = ExtendedDataRecord::new("ACAD");
    rec.add_value(XDataValue::String("DSTYLE".into()));
    rec.add_value(XDataValue::ControlString("{".into()));
    for (code, value) in pairs {
        rec.add_value(XDataValue::Integer16(*code));
        rec.add_value(value.clone());
    }
    rec.add_value(XDataValue::ControlString("}".into()));
    rec
}

fn v(x: f64, y: f64) -> Vector3 {
    Vector3::new(x, y, 0.0)
}

/// 线性标注 + 文字拖到右侧外侧 + DIMTMOVE=1：渲染出的尺寸线引线应延伸到文字底下。
#[test]
fn dim_leader_under_outside_text_renders() {
    let out = std::env::var("OCSM_DIM_LEADER_PDF").unwrap_or_default();

    let mut scene = Scene::new();
    let mut d = DimensionLinear::horizontal(v(20.0, 100.0), v(80.0, 100.0));
    d.definition_point = v(20.0, 86.0); // 尺寸线在 y=86
    d.base.definition_point = d.definition_point;
    d.base.actual_measurement = 60.0;
    // 文字被拖到尺寸线右端之外（中线 x=98，尺寸线右端 x=80）。
    d.base.text_middle_point = v(98.0, 89.0);
    d.base.insertion_point = d.base.text_middle_point;
    d.base.text_user_positioned = true;
    // OCSM 生成的标注样式：DIMTMOVE(279)=1 + 可见的字高/箭头/间隙。
    d.base.style_name = "Standard".into();
    d.base.common.extended_data.add_record(dstyle(&[
        (279, XDataValue::Integer16(1)), // DIMTMOVE=1 → 文字移出范围时带引线
        (140, XDataValue::Real(3.5)),    // DIMTXT
        (41, XDataValue::Real(3.5)),     // DIMASZ
        (147, XDataValue::Real(1.0)),    // DIMGAP
        (271, XDataValue::Integer16(2)), // DIMDEC
    ]));
    scene.add_entity(EntityType::Dimension(Dimension::Linear(d)));

    // 对照组：同一根标注但 DIMTMOVE=0（不画引线）。
    let mut d0 = DimensionLinear::horizontal(v(20.0, 60.0), v(80.0, 60.0));
    d0.definition_point = v(20.0, 46.0);
    d0.base.definition_point = d0.definition_point;
    d0.base.actual_measurement = 60.0;
    d0.base.text_middle_point = v(98.0, 49.0);
    d0.base.insertion_point = d0.base.text_middle_point;
    d0.base.text_user_positioned = true;
    d0.base.style_name = "Standard".into();
    d0.base.common.extended_data.add_record(dstyle(&[
        (279, XDataValue::Integer16(0)), // 对照：不画引线
        (140, XDataValue::Real(3.5)),
        (41, XDataValue::Real(3.5)),
        (147, XDataValue::Real(1.0)),
        (271, XDataValue::Integer16(2)),
    ]));
    scene.add_entity(EntityType::Dimension(Dimension::Linear(d0)));

    let plot_wires: Vec<PlotWire> = scene
        .entity_wires()
        .iter()
        .map(|w| PlotWire {
            wire: w.clone(),
            draw_depth: 0.0,
        })
        .collect();
    assert!(!plot_wires.is_empty(), "场景应产生几何");

    if out.trim().is_empty() {
        return; // 未设环境变量：只跑渲染路径，不落盘
    }
    let path = std::path::PathBuf::from(&out);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    export_pdf(
        &plot_wires,
        &[],
        &[],
        210.0,
        297.0,
        0.0,
        0.0,
        0,
        1.0,
        None,
        &path,
        None,
        PdfPlotOptions::default(),
    )
    .expect("export pdf");
    println!("dim leader smoke pdf written: {out}");
}
