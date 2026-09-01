//! 标注更新服务器：插件内的极简 HTTP server（std::net，零额外依赖）。
//!
//! 职责：
//! - `GET  /guide.html`            浏览器 GUI（引导线标注配置，对应 GUI.png）
//! - `GET  /api/guide?handle=X`    返回引导线几何 + 当前 PE_URL + 解析参数
//! - `POST /api/apply`             把 URL 写入引导线 PE_URL（`应用`）
//! - `POST /api/apply_refresh`     写 URL + 生成真实标注 + 删引导线 + REGEN（`应用并刷新`）
//! - `POST /api/mcp`               MCP 独立二进制的桥接端点（JSON-RPC 风格）
//!
//! 所有图纸操作经 `PluginRequestSender`（线程安全）转发给宿主执行，工作线程
//! 可安全使用（host.rs 明确支持 worker threads）。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use ocs_plugin_api::host::acadrust;
use ocs_plugin_api::host::acadrust::entities::Dimension;
use ocs_plugin_api::host::acadrust::types::Vector3;
use ocs_plugin_api::host::acadrust::xdata::{ExtendedDataRecord, XDataValue};
use ocs_plugin_api::host::PluginRequestSender;
use ocs_plugin_api::ipc::protocol::{PluginRequest, PluginResponse};

use crate::guide_url::{GuideParams, GuideType, LinearSub};
use crate::{frame_scale_at, linear_text_pos, stamp, trim_scale, v3};

/// 默认端口；占用时自动 +1 递增重试。
pub const DEFAULT_PORT: u16 = 23751;

/// 运行中的服务器信息。
#[derive(Debug, Clone, Copy)]
pub struct GuideServer {
    pub port: u16,
}

/// 启动标注更新服务器（绑定 127.0.0.1，端口从 DEFAULT_PORT 起 +1 重试）。
/// 成功后返回端口；全部端口占用返回 None。
pub fn spawn(sender: Arc<dyn PluginRequestSender>) -> Option<GuideServer> {
    for port in DEFAULT_PORT..DEFAULT_PORT + 16 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            std::thread::Builder::new()
                .name("ocsm-guide-server".into())
                .spawn(move || serve(listener, sender))
                .ok()?;
            return Some(GuideServer { port });
        }
    }
    None
}

fn serve(listener: TcpListener, sender: Arc<dyn PluginRequestSender>) {
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                // 每连接一个线程，防止慢客户端阻塞后续请求。
                let sender = sender.clone();
                std::thread::spawn(move || {
                    let _ = handle_conn(&mut s.try_clone().unwrap_or_else(|_| s), &sender);
                });
            }
            Err(_) => continue,
        }
    }
}

// ── HTTP 极简实现 ─────────────────────────────────────────────────────────

struct HttpRequest {
    method: String,
    target: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<HttpRequest> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    let mut header_end = None;
    while buf.len() < 256 * 1024 {
        if stream.read(&mut byte)? == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            header_end = Some(buf.len());
            break;
        }
    }
    let header_end = header_end.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "request header not terminated")
    })?;
    let header = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header.split("\r\n");
    let mut parts = lines.next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut clen = 0usize;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                clen = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; clen];
    if clen > 0 {
        let mut have = buf.len().saturating_sub(header_end).min(clen);
        body[..have].copy_from_slice(&buf[header_end..header_end + have]);
        while have < clen {
            let n = stream.read(&mut body[have..])?;
            if n == 0 {
                break;
            }
            have += n;
        }
    }
    Ok(HttpRequest {
        method,
        target,
        body,
    })
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn handle_conn(
    stream: &mut TcpStream,
    sender: &Arc<dyn PluginRequestSender>,
) -> std::io::Result<()> {
    let req = read_request(stream)?;
    let (status, ctype, resp) = route(&req.method, &req.target, &req.body, sender);
    write_response(stream, status, ctype, resp.as_bytes())
}

fn route(
    method: &str,
    target: &str,
    body: &[u8],
    sender: &Arc<dyn PluginRequestSender>,
) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    let text = "text/plain; charset=utf-8";
    match (method, target) {
        ("GET", t) if t == "/" || t.starts_with("/guide.html") => {
            (200, "text/html; charset=utf-8", GUI_HTML.to_string())
        }
        ("GET", t) if t.starts_with("/api/guide") => api_guide(target, sender),
        ("GET", t) if t.starts_with("/api/tolerance") => api_tolerance(target),
        ("GET", t) if t.starts_with("/api/ping") => (200, json, r#"{"ok":true}"#.into()),
        ("POST", "/api/apply") => api_apply(body, sender, false),
        ("POST", "/api/apply_refresh") => api_apply(body, sender, true),
        ("POST", "/api/mcp") => api_mcp(body, sender),
        _ => (404, text, "not found".into()),
    }
}

// ── 图纸操作（经 sender 转发宿主） ────────────────────────────────────────

/// 诊断日志（写入 /tmp/ocsm_guide.log，真实 GUI 排障用）。
fn dbg_guide(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/ocsm_guide.log")
    {
        let _ = writeln!(f, "{}", msg);
    }
}

/// 带 5s 超时的宿主请求：若宿主未泵动 worker 请求，返回明确错误而非无限阻塞。
fn req_timed(
    sender: &Arc<dyn PluginRequestSender>,
    req: PluginRequest,
    what: &str,
) -> Result<PluginResponse, String> {
    dbg_guide(&format!("req start {what}"));
    let sender2 = sender.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = sender2.request(req);
        let _ = tx.send(r);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(Ok(r)) => {
            dbg_guide(&format!("req ok {what}"));
            Ok(r)
        }
        Ok(Err(e)) => Err(format!("{what}: {e}")),
        Err(_) => {
            dbg_guide(&format!("TIMEOUT {what}"));
            Err(format!(
                "{what}: 宿主未在 5s 内响应（插件 worker 请求未被泵动——请确认 OCS 窗口在前台且 GUI 事件循环运行）"
            ))
        }
    }
}

fn snapshot(sender: &Arc<dyn PluginRequestSender>) -> Result<acadrust::CadDocument, String> {
    match req_timed(sender, PluginRequest::DocumentSnapshot, "DocumentSnapshot") {
        Ok(PluginResponse::Document(doc)) => Ok(*doc),
        Ok(other) => Err(format!("DocumentSnapshot 返回异常: {other:?}")),
        Err(e) => Err(e),
    }
}

/// 标记图纸已修改。宿主里 `tabs[i].dirty = true` 的唯一来源是 SetDirty；
/// add/remove/write/bump 都不会置 dirty。不加的话，关闭 OCS 时不弹"未保存"
/// 提示，引导服务生成的改动会静默丢失。与 ocs-mcp-bridge 的写套路一致：
/// PushUndo → mutate → SetDirty → BumpGeometry。
fn mark_dirty(sender: &Arc<dyn PluginRequestSender>) -> Result<(), String> {
    req_timed(sender, PluginRequest::SetDirty, "SetDirty")?;
    Ok(())
}

fn pe_url_record(url: &str) -> ExtendedDataRecord {
    let mut rec = ExtendedDataRecord::new("PE_URL");
    rec.values.push(XDataValue::String(url.to_string()));
    rec
}

fn read_pe_url(
    sender: &Arc<dyn PluginRequestSender>,
    handle: acadrust::Handle,
) -> Option<String> {
    match req_timed(
        sender,
        PluginRequest::ReadRecord {
            handle,
            app_name: "PE_URL".into(),
        },
        "ReadRecord",
    ) {
        Ok(PluginResponse::Record(Some(rec))) => rec.values.iter().find_map(|v| match v {
            XDataValue::String(s) => Some(s.clone()),
            _ => None,
        }),
        _ => None,
    }
}

fn handle_hex(s: &str) -> Result<acadrust::Handle, String> {
    let t = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    let v = u64::from_str_radix(t, 16).map_err(|_| format!("非法 handle: {s}"))?;
    Ok(acadrust::Handle::from(v))
}

fn fmt_handle(h: acadrust::Handle) -> String {
    format!("{:#X}", h) // 0x3AE
}

fn guide_line_points(
    doc: &acadrust::CadDocument,
    handle: acadrust::Handle,
) -> Result<([f64; 3], [f64; 3]), String> {
    let pts = guide_geom_points(doc, handle)?;
    if pts.len() < 2 {
        return Err("引导线顶点不足（需至少 2 个）".into());
    }
    Ok((pts[0], pts[1]))
}

/// 解析引导线几何，返回顶点列表：
/// - LINE → 2 顶点（[start, end]）
/// - LWPOLYLINE（不闭合）→ 全部顶点（ANGLE 用两段 = 3 顶点）
fn guide_geom_points(
    doc: &acadrust::CadDocument,
    handle: acadrust::Handle,
) -> Result<Vec<[f64; 3]>, String> {
    let e = doc.get_entity(handle).ok_or("找不到引导线实体")?;
    match e {
        acadrust::EntityType::Line(l) => Ok(vec![
            [l.start.x, l.start.y, l.start.z],
            [l.end.x, l.end.y, l.end.z],
        ]),
        acadrust::EntityType::LwPolyline(pl) => {
            if pl.is_closed {
                return Err("引导线多段线不能闭合".into());
            }
            Ok(pl
                .vertices
                .iter()
                .map(|v| [v.location.x, v.location.y, pl.elevation])
                .collect())
        }
        // heavy polyline（DXF 读取路径把 LWPOLYLINE 转为 Polyline/Polyline2D）。
        acadrust::EntityType::Polyline(pl) => {
            if pl.flags.is_closed() {
                return Err("引导线多段线不能闭合".into());
            }
            Ok(pl
                .vertices
                .iter()
                .map(|v| [v.location.x, v.location.y, v.location.z])
                .collect())
        }
        acadrust::EntityType::Polyline2D(pl) => {
            if pl.flags.is_closed() {
                return Err("引导线多段线不能闭合".into());
            }
            Ok(pl
                .vertices
                .iter()
                .map(|v| [v.location.x, v.location.y, v.location.z])
                .collect())
        }
        _ => Err("引导线必须是直线（LINE）或多段线（PLINE）".into()),
    }
}

/// 引导几何形态：line（LINE）/ pline（LWPOLYLINE / Polyline / Polyline2D）。
fn guide_geom_kind(
    doc: &acadrust::CadDocument,
    handle: acadrust::Handle,
) -> Result<&'static str, String> {
    let e = doc.get_entity(handle).ok_or("找不到引导线实体")?;
    match e {
        acadrust::EntityType::Line(_) => Ok("line"),
        acadrust::EntityType::LwPolyline(_)
        | acadrust::EntityType::Polyline(_)
        | acadrust::EntityType::Polyline2D(_) => Ok("pline"),
        _ => Err("引导线必须是直线（LINE）或多段线（PLINE）".into()),
    }
}

/// 构造与 `OCSMDIMGULIDE1-general.dxf` 标注一致的 DSTYLE XDATA 覆盖
/// （ACAD/DSTYLE）。OCS 渲染 Dimension 时优先用 XDATA 覆盖样式表
/// （dim_override），因此必须写入这些参数才能复刻示例渲染：
/// dimdec=4（4 位小数）、dimtoh=1（文字水平）、dimdli=0.38 等。
fn dim_override_record() -> ExtendedDataRecord {
    dim_override_record_dec(4)
}

/// `dec`：DIMDEC(271) 测量值小数位数覆盖。
fn dim_override_record_dec(dec: i16) -> ExtendedDataRecord {
    use acadrust::xdata::XDataValue as V;
    let mut rec = ExtendedDataRecord::new("ACAD");
    rec.add_value(V::String("DSTYLE".into()));
    rec.add_value(V::ControlString("{".into()));
    let ints: &[(i16, i16)] = &[
        (71, 0), (72, 0), (73, 0), (74, 1), (75, 0), (76, 0), (77, 1), (78, 8), (79, 0),
        (170, 0), (171, 3), (172, 1), (173, 1), (174, 0), (175, 0),
        (176, 130), (177, 130), (178, 3), (179, 0),
        (271, dec), (272, 2), (273, 2), (274, 3), (275, 0), (276, 0),
        (277, 2), (278, 46), (279, 0), (280, 0), (281, 0), (282, 0),
        (283, 0), (284, 8), (285, 0), (286, 0), (288, 0), (289, 3), (290, 0), (294, 0),
        (371, -1), (372, -1),
    ];
    for &(code, val) in ints {
        rec.add_value(V::Integer16(code));
        rec.add_value(V::Integer16(val));
    }
    // 注意：不覆盖 DIMSCALE(40)——缩放样式 OCSM_GB_x{n} 的 dimscale 由样式决定，
    // 若在这里写死 1.0 会覆盖缩放导致文字/箭头不放大。
    let reals: &[(i16, f64)] = &[
        (41, 2.5), (43, 0.38), (44, 2.0), (45, 0.0),
        (47, 0.0), (48, 0.0), (50, 0.7853981633974483),
        (140, 2.5), (141, 2.5), (143, 0.03937007874016), (144, 1.0),
        (146, 1.0), (147, 1.0), (148, 0.0),
    ];
    for &(code, val) in reals {
        rec.add_value(V::Integer16(code));
        rec.add_value(V::Real(val));
    }
    rec.add_value(V::ControlString("}".into()));
    rec
}

/// 与 PowerDim::build_linear 相同的生成逻辑，但放置点由 dist 计算：
/// pt = P2 + 法向 × dist（"与 P2 距离"语义）。产物带 DSTYLE XDATA 覆盖，
/// 渲染与 `OCSMDIMGULIDE1-general.dxf` 的示例一致。
fn build_guide_linear(
    p1: [f64; 3],
    p2: [f64; 3],
    sub: LinearSub,
    dist: f64,
    style: &str,
) -> Dimension {
    use acadrust::entities::{DimensionAligned, DimensionLinear};
    let (first, second) = (v3(p1), v3(p2));
    let axis = (second - first).normalize();
    let perp = Vector3::new(-axis.y, axis.x, 0.0);
    let pt = second + perp * dist;
    let dim = match sub {
        LinearSub::Aligned => {
            let mut d = DimensionAligned::new(first, second);
            d.definition_point = pt;
            d.base.definition_point = pt;
            let tp = linear_text_pos(first, second, pt, axis);
            d.base.text_middle_point = tp;
            d.base.insertion_point = tp;
            d.base.actual_measurement = d.measurement();
            Dimension::Aligned(d)
        }
        LinearSub::Horizontal => {
            let mut d = DimensionLinear::horizontal(first, second);
            let tp = linear_text_pos(first, second, pt, Vector3::new(1.0, 0.0, 0.0));
            d.definition_point = pt;
            d.base.definition_point = pt;
            d.base.text_middle_point = tp;
            d.base.insertion_point = tp;
            d.base.actual_measurement = d.measurement();
            Dimension::Linear(d)
        }
        LinearSub::Vertical => {
            let mut d = DimensionLinear::vertical(first, second);
            let tp = linear_text_pos(first, second, pt, Vector3::new(0.0, 1.0, 0.0));
            d.definition_point = pt;
            d.base.definition_point = pt;
            d.base.text_middle_point = tp;
            d.base.insertion_point = tp;
            d.base.actual_measurement = d.measurement();
            Dimension::Linear(d)
        }
    };
    let mut dim = stamp(dim, style);
    dim.base_mut().common.extended_data.add_record(dim_override_record());
    // 文字旋转（机械制图规范）。文字位置交给 host 的 dimtad=1 计算——
    // 对齐 → 尺寸线上方，竖直 → 尺寸线左侧，自动随 dimscale 缩放。
    // 不用 text_user_positioned（否则强制用 text_middle_point，对齐文字会
    // 落回尺寸线下方）。
    let pi = std::f64::consts::PI;
    let rot = match sub {
        // 对齐：文字跟随尺寸线旋转（覆盖 DSTYLE XDATA 的 dimtoh=1 水平强制）。
        LinearSub::Aligned => {
            let ang = (second.y - first.y).atan2(second.x - first.x);
            if ang > pi / 2.0 {
                ang - pi
            } else if ang <= -pi / 2.0 {
                ang + pi
            } else {
                ang
            }
        }
        // 竖直：文字逆时针 90°（从下往上读）。
        LinearSub::Vertical => pi / 2.0,
        // 水平：不设（dimtoh=1 → 水平 0°）。
        LinearSub::Horizontal => 0.0,
    };
    if rot.abs() > 1e-9 {
        dim.base_mut().text_rotation = rot;
    }
    dim
}

/// 角度标注（两段 PLINE 引导线）：p0→p1→p2，p1=角度顶点、两段即两边。
/// 放置点 = 顶点 + 角平分线方向（劣角内部）× dist。
/// 复用 PowerDim build_angular 的 Angular2Ln 构建逻辑（lib.rs），
/// 角模式：劣角（默认）/ 补角（180°−劣角）/ 优角（360°−劣角）。
fn build_guide_angular(
    p0: [f64; 3],
    p1: [f64; 3],
    p2: [f64; 3],
    dist: f64,
    mode: crate::guide_url::AngleMode,
    style: &str,
) -> Result<Dimension, String> {
    use acadrust::entities::{DimensionAngular2Ln, DimensionBase, DimensionType};
    use crate::{angular_minor_sweep_deg, LineGeom};

    // 顶点 p1；line1 = 第一段（顶点→p2），line2 = 第二段反向（顶点→p0）。
    let line1 = LineGeom { start: p1, end: p2 };
    let line2 = LineGeom { start: p1, end: p0 };

    // 角平分线（劣角内部）：单位化两方向之和。
    let norm = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1]).sqrt();
    let unit = |v: [f64; 3]| {
        let l = norm(v);
        if l < 1e-12 { [0.0, 0.0, 0.0] } else { [v[0] / l, v[1] / l, 0.0] }
    };
    let d1 = unit([p2[0] - p1[0], p2[1] - p1[1], 0.0]);
    let d2 = unit([p0[0] - p1[0], p0[1] - p1[1], 0.0]);
    let bis = unit([d1[0] + d2[0], d1[1] + d2[1], 0.0]);
    let pt = [p1[0] + bis[0] * dist, p1[1] + bis[1] * dist, 0.0];

    let minor_deg = angular_minor_sweep_deg(&line1, &line2, pt);
    let (measurement, user_text) = match mode {
        crate::guide_url::AngleMode::Minor => (minor_deg, format!("{:.0}°", minor_deg)),
        crate::guide_url::AngleMode::Reflex => {
            (360.0 - minor_deg, format!("{:.0}°", 360.0 - minor_deg))
        }
    };
    let mut d = DimensionAngular2Ln {
        base: DimensionBase::new(DimensionType::Angular),
        dimension_arc: v3(pt),
        first_point: v3(line1.start),
        second_point: v3(line1.end),
        angle_vertex: v3(line2.start),
        definition_point: v3(line2.end),
    };
    d.base.text_middle_point = v3(pt);
    d.base.insertion_point = v3(pt);
    d.base.actual_measurement = measurement;
    // 宿主对 Angular2Ln 的文字用 measurement()（对两线存储语义错误，
    // 可能导致恒 90°），直接用 user_text 提供正确角度值。
    d.base.user_text = Some(user_text);
    let mut dim = stamp(Dimension::Angular2Ln(d), style);
    dim.base_mut().text_user_positioned = true;
    Ok(dim)
}

/// 角度标注（匿名块形态，对照 `角度.dxf` *D67）：块成员 =
/// 主弧（劣角）+ 延长弧×2 + SOLID 箭头×2（弧两端尖朝外）+ MTEXT（水平，角平分线）。
/// DIMENSION 引用匿名块（block_name）→ 渲染走 walk_block 复刻示例样式。
/// dist 语义不变 = 文字距顶点的距离（弧半径 = dist − DIMGAP）。
fn build_guide_angular_block(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p0: [f64; 3],
    p1: [f64; 3],
    p2: [f64; 3],
    params: &GuideParams,
    style: &str,
) -> Result<Dimension, String> {
    use acadrust::entities::{Arc, AttachmentPoint, MText, Solid};
    use acadrust::types::Vector3;
    use acadrust::EntityType as E;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    let v = v3(p1);
    let scale = frame_scale_at(doc, p1);
    let h = 2.5 * scale; // DIMTXT（箭头长/文字高）
    let gap = 1.0 * scale; // DIMGAP（文字距弧）
    let r = params.dist - gap; // 弧半径 = 文字距顶点 − gap（文字在角平分线 r+gap 处）
    if r < 1e-9 {
        return Err("弧半径过小（与顶点距离需大于 DIMGAP=1）".into());
    }
    // 从顶点出发的两条边方向角。
    let a0 = (p0[1] - p1[1]).atan2(p0[0] - p1[0]);
    let a1 = (p2[1] - p1[1]).atan2(p2[0] - p1[0]);
    // 劣角弧：从 start 逆时针 sweep 到 end（sweep≤π）。
    let (mut start, mut end) = (a0, a1);
    let mut sweep = (end - start).rem_euclid(TAU);
    if sweep > PI {
        start = a1;
        end = a0;
        sweep = TAU - sweep;
    }
    let minor_deg = sweep.to_degrees();
    // 角模式 → 显示值：劣角 / 补角（180−劣角）/ 优角（360−劣角）。
    let show_deg = match params.angle_mode {
        crate::guide_url::AngleMode::Minor => minor_deg,
        crate::guide_url::AngleMode::Reflex => 360.0 - minor_deg,
    };
    let bis = start + sweep * 0.5; // 角平分线角（劣角内）
    let bis_v = Vector3::new(bis.cos(), bis.sin(), 0.0);

    let mut members: Vec<E> = Vec::new();
    let mut mk_arc = |a1: f64, a2: f64| -> E {
        let mut arc = Arc::new();
        arc.center = v;
        arc.radius = r;
        arc.start_angle = a1;
        arc.end_angle = a2;
        let mut e = E::Arc(arc);
        set_member_layer(&mut e, "7标注层");
        e
    };
    if params.angle_mode == crate::guide_url::AngleMode::Reflex {
        // 优角：大半圆弧（从 end 逆时针 360°−θ 到 start）。
        members.push(mk_arc(end, start));
    } else {
        // 劣角：只主弧（无延长弧，箭头在主弧两端尖朝外）。
        members.push(mk_arc(start, end));
    }
    // 箭头：尖在弧端，底边朝弧外侧（起点端顺时针切线、终点端逆时针切线），
    // 长 h、底边宽 h/3（对照示例：尖到底边 2.49≈h、底宽 0.83≈h/3）。
    // 箭头：位于夹角内侧（弧线内侧，底边在弧端向内 h 处），尖指向弧外侧——
    // "渲染在夹角的内侧，并由内侧指向外侧"（方向=切线朝外，位置=弧内）。
    // 细长：半宽 h/6（全宽 h/3 → 长:宽 = 3:1，对齐宿主线性 ClosedFilled）。
    let mk_arrow = |tip_ang: f64, out_ang: f64, radial: Vector3| -> E {
        let arc_end = v + Vector3::new(tip_ang.cos(), tip_ang.sin(), 0.0) * r;
        let out = Vector3::new(out_ang.cos(), out_ang.sin(), 0.0);
        let tip = arc_end; // 尖在弧端（最外侧）
        let perp = radial * (h / 6.0);
        let base = arc_end - out * h; // 底边在弧内侧 h 处（夹角内侧）
        let mut e = E::Solid(Solid::new(tip, base + perp, base - perp, base - perp));
        set_member_layer(&mut e, "7标注层");
        e
    };
    let rad_start = Vector3::new(start.cos(), start.sin(), 0.0);
    let rad_end = Vector3::new(end.cos(), end.sin(), 0.0);
    // 两箭头干涉（弧长 < 2×箭头长）时跳过箭头。
    let arc_len = r * sweep;
    if arc_len >= 2.0 * h {
        members.push(mk_arrow(start, start - FRAC_PI_2, rad_start));
        members.push(mk_arrow(end, end + FRAC_PI_2, rad_end));
    }
    // MTEXT：`{θ°}` attach=8，insert=角平分线 (r+gap)（文字底部中心）。
    // 文字书写方向 ⊥ 顶点→文字（径向）→ 沿弧切向（bis − 90°），clamp 防倒置。
    let text_value = format!("{{{:.0}°}}", show_deg);
    let mut m = MText::new();
    m.rectangle_width = (text_value.chars().count() as f64 * h * 0.75).max(10.0);
    m.value = text_value;
    m.insertion_point = v + bis_v * (r + gap);
    m.height = h;
    let mut text_rot = bis - FRAC_PI_2;
    if text_rot > FRAC_PI_2 {
        text_rot -= PI;
    } else if text_rot <= -FRAC_PI_2 {
        text_rot += PI;
    }
    m.rotation = text_rot;
    m.style = "OCSM_GB".into();
    m.attachment_point = AttachmentPoint::BottomCenter;
    let mut e = E::MText(m);
    set_member_layer(&mut e, "7标注层");
    e.common_mut().color = acadrust::Color::from_index(3); // 绿色（对照示例 *D67 MTEXT 62=3）
    members.push(e);

    // 匿名块名 *D{n}（取最大序号 +1）。
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*D") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*D{}", max_n + 1);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: members,
        },
        "AddBlockRecord",
    )?;

    // DIMENSION（Angular2Ln 几何/文字 + 引用匿名块）。
    let mut dim = build_guide_angular(p0, p1, p2, params.dist, params.angle_mode, style)?;
    dim.base_mut().block_name = block_name;
    Ok(dim)
}

/// 直径/半径标注生成：引导线 P1=圆心、P2=圆周点（线长=半径，方向=偏转角）。
/// dist = 文字沿偏转角方向相对圆周点的偏移（负=圆内侧，正=圆外侧），
/// 与线性「与P2距离」语义一致。文字径向旋转（复用 PowerDim build_radial
/// 已验证逻辑），内外都无引出线（对照 OCSMDIMGULIDE2 inside/outside 示例）。
fn build_guide_radial(
    p1: [f64; 3],
    p2: [f64; 3],
    guide_type: GuideType,
    dist: f64,
    style: &str,
) -> Dimension {
    use acadrust::entities::{DimensionDiameter, DimensionRadius};
    let (center, rim) = (v3(p1), v3(p2));
    let radius = center.distance(&rim);
    // 文字位置：沿圆心→圆周方向，从圆周点偏移 dist（负=圆内，正=圆外）。
    let dir = if radius > 1e-9 {
        (rim - center) * (1.0 / radius)
    } else {
        acadrust::types::Vector3::new(1.0, 0.0, 0.0)
    };
    let text_pt = rim + dir * dist;
    // 注意：文字显示用实时 measurement()（definition_point↔angle_vertex 距离），
    // angle_vertex 必须保持圆心，否则显示值错（第二版 18 / 第三版 9 的教训）。
    // 文字位置：锚点沿法向（文字旋转后的"上方"）偏移 DIMGAP(1.0)。
    // attachment=BottomCenter 使文字底边贴锚点 → 文字底边离线 1.0、
    // 中心离线 1.0+文字半高1.25=2.25，与对齐标注（perp_off）一致。
    use acadrust::entities::dimension::AttachmentPointType;
    let perp = acadrust::types::Vector3::new(-dir.y, dir.x, 0.0);
    let text_anchor = text_pt + perp * 1.0;
    let dim = match guide_type {
        GuideType::Diameter => {
            let mut d = DimensionDiameter::new(center, rim);
            // 完整直径（第一版字段语义，测量正确）；外侧 leader 沿径向
            // 从圆周点延伸 dist 到文字。
            d.angle_vertex = center;
            d.definition_point = rim;
            d.leader_length = if dist > 0.0 { dist } else { 0.0 };
            d.base.definition_point = rim;
            d.base.actual_measurement = radius * 2.0;
            // 外侧回退上一版：文字锚点在径向线上（引线直、文字贴线端），
            // 内侧保留 DIMGAP 法向偏移（文字悬于短线上方）。
            let tp = if dist > 0.0 { text_pt } else { text_anchor };
            d.base.text_middle_point = tp;
            d.base.insertion_point = tp;
            d.base.attachment_point = AttachmentPointType::BottomCenter;
            Dimension::Diameter(d)
        }
        _ => {
            let mut d = DimensionRadius::new(center, rim);
            // 圆心→圆周线；leader 从圆周点沿径向伸向文字。
            d.angle_vertex = center;
            d.definition_point = rim;
            d.base.definition_point = center;
            d.leader_length = if dist > 0.0 { dist } else { 0.0 };
            d.base.actual_measurement = radius;
            d.base.text_middle_point = text_anchor;
            d.base.insertion_point = text_anchor;
            d.base.attachment_point = AttachmentPointType::BottomCenter;
            Dimension::Radius(d)
        }
    };
    let mut dim = stamp(dim, style);
    let mut rec = dim_override_record();
    // 直径/半径不画圆心标记（对照示例，圆心无十字）：DIMCEN=0，
    // 插入到结尾 "}" 之前保持 1002 括号包裹结构。
    if let Some(pos) = rec
        .values
        .iter()
        .rposition(|v| matches!(v, XDataValue::ControlString(c) if c == "}"))
    {
        rec.values.insert(pos, XDataValue::Integer16(141));
        rec.values.insert(pos + 1, XDataValue::Integer16(0));
    }
    dim.base_mut().common.extended_data.add_record(rec);
    // 文字随引线旋转（AutoCAD 默认画法，同对齐标注）：径向角
    // （圆心→文字方向），clamp (-90°, 90°] 防倒置；覆盖 DSTYLE 的 dimtoh=1。
    dim.base_mut().text_user_positioned = true;
    let ang = (text_pt.y - center.y).atan2(text_pt.x - center.x);
    let pi = std::f64::consts::PI;
    let rot = if ang > pi / 2.0 {
        ang - pi
    } else if ang <= -pi / 2.0 {
        ang + pi
    } else {
        ang
    };
    dim.base_mut().text_rotation = rot;
    dim
}

/// frame 比例感知的标注样式（与 PowerDim::resolve_style 同逻辑）：检查
/// `pt` 是否在 TF 图幅块内，返回对应缩放样式名；样式缺失时经 sender 向宿主
/// 创建 `OCSM_GB_x{scale}`（确保生成标注能应用文字/箭头缩放）。
fn ensure_style_for_point(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    pt: [f64; 3],
) -> Result<String, String> {
    let scale = frame_scale_at(doc, pt);
    if (scale - 1.0).abs() < 1e-9 {
        return Ok("OCSM_GB".to_string());
    }
    let name = format!("OCSM_GB_x{}", trim_scale(scale));
    if doc
        .dim_styles
        .iter()
        .any(|s| s.name.eq_ignore_ascii_case(&name))
    {
        return Ok(name);
    }
    req_timed(
        sender,
        PluginRequest::EnsureDimStyles(vec![crate::scaled_dim_style_def(scale)]),
        "EnsureDimStyles",
    )?;
    Ok(name)
}

/// 实心三角箭头（AutoCAD ClosedFilled 形态）：尖在 `tip`，指向 `away` 方向，
/// 长 `size`、宽 size/3。
fn arrow_solid(
    tip: acadrust::types::Vector3,
    away: acadrust::types::Vector3,
    size: f64,
) -> acadrust::entities::Solid {
    use acadrust::types::Vector3;
    // 细长箭头：半宽 size/6（全宽 size/3 → 长:宽 = 3:1，对齐宿主线性 ClosedFilled）。
    let perp = Vector3::new(-away.y, away.x, 0.0) * (size / 6.0);
    let base = tip - away * size;
    acadrust::entities::Solid::new(tip, base + perp, base - perp, base - perp)
}

/// 块成员公共属性：7标注层。
fn set_member_layer(e: &mut acadrust::EntityType, layer: &str) {
    e.common_mut().layer = layer.to_string();
}

/// 测量值格式化：整数无小数，小数保留（Ø100 / Ø10.5）。
/// 测量值格式化：保留 `dec` 位小数（默认 2），消尾零——
/// 12.70→12.7、12.00→12（整数部分不消零）、12.345(2位)→12.35。
fn format_measurement(m: f64, dec: u32) -> String {
    let dec = (dec.min(8)) as usize;
    let mut t = format!("{:.*}", dec, m);
    if t.contains('.') {
        while t.ends_with('0') {
            t.pop();
        }
        if t.ends_with('.') {
            t.pop();
        }
    }
    // 防止 "-0"（负零）显示。
    if t == "-0" {
        t = "0".into();
    }
    t
}

/// 直径/半径标注（匿名块形态）：按 AutoCAD 画法生成块成员（LINE/Solid/MTEXT），
/// DIMENSION 实体引用匿名块 → 渲染走 walk_block 完全复刻示例形态。
/// - 内侧（dist<0）：短线 rim→圆内 + 单箭头 + 径向旋转文字（直径/半径相同）。
/// - 外侧（dist>0）：直径=对侧长线穿圆心+双箭头+对侧短线+landing+水平文字；
///                  半径=圆心→锚点线+单箭头+landing+水平文字（文字中心在锚点外
///                  一个「文字宽+gap」，对照 OCSMDIMGULIDE3-outside）。
/// 每次生成新块 `*D{n}`；改参数后重跑「应用并刷新」即重建（REGEN 由调用方执行）。
fn build_guide_radial_block(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p1: [f64; 3],
    p2: [f64; 3],
    params: &GuideParams,
    style: &str,
) -> Result<Dimension, String> {
    use acadrust::entities::{Line, MText};
    use acadrust::types::Vector3;
    use acadrust::entities::AttachmentPoint;
    use acadrust::xdata::XDataValue as V;
    use acadrust::EntityType as E;

    let (center, rim) = (v3(p1), v3(p2));
    let dist = params.dist;
    let radius = center.distance(&rim);
    if radius < 1e-9 {
        return Err("引导线长度为 0（圆心与圆周点重合）".into());
    }
    let dir = (rim - center) * (1.0 / radius);
    let diameter = params.guide_type == GuideType::Diameter;
    let measurement = if diameter { radius * 2.0 } else { radius };
    let prefix = if diameter { "Ø" } else { "R" };

    // 图幅缩放 → 文字高/箭头尺寸。
    let scale = frame_scale_at(doc, p1);
    let h = 2.5 * scale; // DIMTXT
    let asz = 2.5 * scale; // DIMASZ
    let gap = 1.0 * scale; // DIMGAP

    // 文字内容：user_text（<> 替换测量）或自动前缀+测量。
    let auto_value = format!("{prefix}{}", format_measurement(measurement, params.dec.unwrap_or(2)));
    let text_visible = match &params.text {
        // 直径/半径自动加 Ø/R 前缀（auto_value），用户输入中的 %%c 去掉避免重复
        //（%%c<> 传统写法 → 用 auto_value；其它文字替换 <> 时清 %%c）。
        Some(t) if !t.is_empty() && *t != "%%c<>" => {
            t.replace("%%c", "").replace("<>", &auto_value)
        }
        _ => auto_value,
    };
    // 公差（直径/半径也支持）：优先 ISO 配合代号（fit）结合测量值算偏差，
    // 否则用手输 up/dn。注入块内 MTEXT 堆叠 `{\H0.71x;\C2;\S{up}^{dn};}`
    // （与线性 dimtext 同款，OCS 渲染块内 MTEXT 同样解析 \S 堆叠）。
    let tol_seg: Option<String> = if let Some(f) = &params.fit {
        // 代号模式：配合代号堆叠（H7 上 g6 下）
        if f.is_empty() {
            None
        } else {
            Some(tol_code_for_fit(f))
        }
    } else if let (Some(up), Some(dn)) = (&params.up, &params.dn) {
        if up.is_empty() || dn.is_empty() {
            None
        } else {
            Some(format!("\\H0.71x;\\C2;\\S{}^{};", up, dn))
        }
    } else {
        None
    };
    // 块内 MTEXT：文字模板渲染（||| 公差位置、\X 换行、[@] 英制换算），
    // 有公差时 \A1 左对齐使堆叠紧跟测量值右侧（不换行到下方）。
    let body = render_text_template(&text_visible, measurement, tol_seg.as_deref());
    let text_value = if tol_seg.is_some() {
        format!("{{\\A1;{}}}", body)
    } else {
        body
    };
    // 文字宽估算：可见文字（含公差偏差最大宽度）字符数×0.59×h
    //（Standard txt 字体经验值，对照示例验证：
    // 'Ø100' h=2.5 → 5.9；示例内侧线端 41−5.9/2−1=37.05 ≈ 实测 37.07 ✓；
    // 示例外侧 landing 3.95 ≈ 5.9/2+1 ✓）。
    let mut text_width = text_visible.chars().count() as f64 * 0.59 * h;
    if let Some(seg) = &tol_seg {
        // 堆叠可见宽度：剥离 MTEXT 代码后取最大行字符数。
        let vis: String = seg
            .chars()
            .filter(|c| !"\\H0.71x;\\C2;\\S{}^;".contains(*c))
            .collect();
        text_width += vis.chars().count() as f64 * 0.59 * h * 0.5;
    }
    let text_mid = rim + dir * dist; // 文字锚点（在径向线上）
    let rot = dir.y.atan2(dir.x); // 径向角

    // ── 块成员（对照示例 OCSMDIMGULIDE2-inside/outside.dxf 的 AutoCAD 画法）──
    let mut members: Vec<E> = Vec::new();
    let mut mk_line = |a: Vector3, b: Vector3| -> E {
        let mut e = E::Line(Line {
            common: Default::default(),
            start: a,
            end: b,
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "7标注层");
        e
    };
    let mut mk_arrow = |tip: Vector3, away: Vector3| -> E {
        let mut e = E::Solid(arrow_solid(tip, away, asz));
        set_member_layer(&mut e, "7标注层");
        e
    };
    // MTEXT：attach=BottomCenter(8)，insert=锚点沿文字左法向偏移 DIMGAP
    //（文字悬于线上方一个 DIMGAP 间隙，对照示例 insert 推导吻合）。
    let mut mk_mtext = |value: String, anchor: Vector3, rotation: f64, up: Vector3| -> E {
        let mut m = MText::new();
        // 行宽自适应：MText::new() 默认 rectangle_width=10，长文字（测量+公差+后缀）
        // 会被 OCS 按 10 单位折行；按可见字符估算宽度给足空间。
        let visible_len = value
            .chars()
            .filter(|c| !matches!(c, '\\' | '{' | '}' | ';' | '^'))
            .count() as f64;
        m.rectangle_width = (visible_len * h * 0.75).max(10.0);
        m.value = value;
        m.insertion_point = anchor + up * gap;
        m.height = h;
        m.rotation = rotation;
        m.style = "OCSM_GB".into();
        m.attachment_point = AttachmentPoint::BottomCenter;
        let mut e = E::MText(m);
        set_member_layer(&mut e, "7标注层");
        e
    };

    if dist < 0.0 {
        // 内侧（示例 *D69 inside）：短线 rim→圆内（端点=text_mid−径向×(文字半宽+
        // DIMGAP)）+ 单箭头（尖在 rim、底朝圆内）+ MTEXT 随线旋转。
        let line_end = text_mid - dir * (text_width * 0.5 + gap);
        members.push(mk_line(rim, line_end));
        members.push(mk_arrow(rim, dir));
        // 保持文字可读（AutoCAD 画法）：径向角超出 (−90°,90°] 时翻转 180°
        //（如 120° 引线 → −60°），文字"上方"由翻转后的旋转角推导，
        // 保证文字始终悬于线的一侧且头朝上。
        let mut rot = rot;
        if rot > std::f64::consts::FRAC_PI_2 {
            rot -= std::f64::consts::PI;
        } else if rot <= -std::f64::consts::FRAC_PI_2 {
            rot += std::f64::consts::PI;
        }
        let up = Vector3::new(-rot.sin(), rot.cos(), 0.0);
        members.push(mk_mtext(text_value.clone(), text_mid, rot, up));
    } else if diameter {
        // 直径外侧（示例 *D69 outside）：主线=对侧圆周点→文字锚点（穿圆心长线）
        // + 水平 landing + 对侧出圆短线（2×asz）+ 双箭头（尖在两圆周点、
        // 底朝圆外=指向圆心）+ MTEXT 水平（dimtoh=1 画法，rot=0）。
        let far = center - dir * radius;
        members.push(mk_line(far, text_mid));
        // landing 水平段朝远离圆的方向：锚点在圆心左侧则向左，右侧则向右。
        let sgn = if text_mid.x >= center.x { 1.0 } else { -1.0 };
        members.push(mk_line(
            text_mid,
            text_mid + Vector3::new(sgn * (text_width * 0.5 + gap), 0.0, 0.0),
        ));
        members.push(mk_line(far, far - dir * (2.0 * asz)));
        members.push(mk_arrow(rim, -dir));
        members.push(mk_arrow(far, dir));
        members.push(mk_mtext(
            text_value.clone(),
            text_mid,
            0.0,
            Vector3::new(0.0, 1.0, 0.0),
        ));
    } else {
        // 半径外侧（示例 *D69 outside）：半径线=圆心→文字锚点（沿径向 dist 出圆）
        // + 单箭头（尖在圆周点、底朝圆外=指向圆心）
        // + 水平 landing（锚点→远离圆侧，长=文字宽+gap 到文字中心，再+半宽+gap 到端；
        //   对照示例：文字中心距锚点 5.47≈文字宽4.425+gap1，landing 8.49≈文字中心距+半宽+gap）
        // + MTEXT 水平（rot=0，位于 landing 上方：insert=文字中心+(0,gap)）。
        let sgn = if text_mid.x >= center.x { 1.0 } else { -1.0 };
        let lv = Vector3::new(sgn, 0.0, 0.0); // landing 水平方向（远离圆）
        members.push(mk_line(center, text_mid));
        members.push(mk_arrow(rim, -dir));
        // 文字中心在锚点外「文字宽+gap」（沿水平 landing 方向）。
        let text_center = text_mid + lv * (text_width + gap);
        members.push(mk_line(
            text_mid,
            text_center + lv * (text_width * 0.5 + gap),
        ));
        members.push(mk_mtext(
            text_value.clone(),
            text_center,
            0.0,
            Vector3::new(0.0, 1.0, 0.0),
        ));
    }

    // ── 匿名块名：*D{n} 取最大序号 +1 ──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*D") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*D{}", max_n + 1);

    // ── 经宿主建块 ──
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: members,
        },
        "AddBlockRecord",
    )?;

    // ── DIMENSION 实体（引用匿名块；几何字段保持正确语义） ──
    let mut dim = build_guide_radial(p1, p2, params.guide_type, dist, style);
    {
        let base = dim.base_mut();
        base.block_name = block_name;
        base.text = params.text.clone().unwrap_or_default();
        if let Some(t) = &params.text {
            if !t.is_empty() && t != "%%c<>" {
                base.user_text = Some(t.clone());
            }
        }
        let mut r = dim_override_record();
        if let Some(pos) = r
            .values
            .iter()
            .rposition(|v| matches!(v, V::ControlString(c) if c == "}"))
        {
            r.values.insert(pos, V::Integer16(141));
            r.values.insert(pos + 1, V::Integer16(0));
        }
        base.common.extended_data.add_record(r);
    }
    Ok(dim)
}

fn build_dimension(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p1: [f64; 3],
    p2: [f64; 3],
    params: &GuideParams,
    style: &str,
) -> Result<Dimension, String> {
    let mut dim = match params.guide_type {
        GuideType::Linear => {
            let sub = params.sub.ok_or("线性标注缺少对齐子类型")?;
            build_guide_linear(p1, p2, sub, params.dist, style)
        }
        // 直径/半径：匿名块形态（AutoCAD 画法，渲染走 walk_block）。
        GuideType::Diameter | GuideType::Radius => {
            build_guide_radial_block(sender, doc, p1, p2, params, style)?
        }
        GuideType::Datum => return Err("基准标注暂未实现".into()),
        GuideType::View => return Err("向视图标注暂未实现".into()),
        GuideType::Angle => return Err("角度标注走 do_apply 独立分支".into()),
        GuideType::Section => return Err("剖切符号走 do_apply 独立分支".into()),
        GuideType::Tolerance => return Err("形位公差走 do_apply 独立分支".into()),
    };
    // 用户指定小数位数：覆盖 XDATA DSTYLE 里的 DIMDEC(271)（默认样式 4）。
    if let Some(d) = params.dec {
        set_dimdec(&mut dim, d.min(8) as i16);
    }
    // 公差（上/下偏差）：注入 DIMENSION 的 text 字段（DXF 组 1 = dimtext），
    // 用 MTEXT 堆叠代码（对照 OCSMDIMGULIDE/公差.dxf 29C：
    // `{\A1;<>{}{\H0.71x;\C2;\S+0.024^  0;}}`）。`<>` = 测量值占位，OCS 渲染时替换。
    // 仅对原生 DIMENSION（线性）生效；直径/半径走匿名块块内 MTEXT，暂不注入。
    if params.guide_type == GuideType::Linear {
        // 文字 override（dimtext）：默认 <>（测量值占位），用户 text 覆盖
        //（含 %%c 直径符号、中文等）。文字模板：||| = 公差位置、\X = 换行、
        // [@] = 英制换算（25.4 → [1" ]），OCS 渲染时替换 <>、解析 %%c。
        let text_part = match &params.text {
            Some(t) if !t.is_empty() => t.clone(),
            _ => "<>".to_string(),
        };
        // 公差（dimtext）：fit 代号模式 → 配合代号堆叠（H7 上 g6 下）；
        // up/dn 手输 → 极限偏差堆叠。
        let m = dim.base().actual_measurement;
        let tol_seg: Option<String> = if let Some(fit) = &params.fit {
            if fit.is_empty() { None } else { Some(tol_code_for_fit(fit)) }
        } else if let (Some(up), Some(dn)) = (&params.up, &params.dn) {
            if up.is_empty() || dn.is_empty() {
                None
            } else {
                Some(format!("\\H0.71x;\\C2;\\S{}^{};", up, dn))
            }
        } else {
            None
        };
        let body = render_text_template(&text_part, m, tol_seg.as_deref());
        if tol_seg.is_some() {
            dim.base_mut().text = format!("{{\\A1;{}}}", body);
        } else if body != "<>" {
            // 无公差：模板渲染后直接作为 dimtext（处理 \X 换行 / [@] 英制 / 删 |||）。
            dim.base_mut().text = body;
        }
    }
    Ok(dim)
}

/// 配合代号 → MTEXT 堆叠代码，对照公差渲染.dxf 左侧标准示例
/// `{\C3;\SH7/h6;}`：绿色 \C3（与测量值同色）、无花括号、斜杠 `/` 分数堆叠、
/// 无 \H0.71x 字高覆盖（OCS \S 默认比例）。单代号 H7 → `\C3;H7`（水平，不堆叠）。
fn tol_code_for_fit(fit: &str) -> String {
    let fit = fit.trim();
    if let Some((h, s)) = fit.split_once('/') {
        format!("\\C3;\\S{}/{};", h.trim(), s.trim())
    } else {
        format!("\\C3;{}", fit)
    }
}

/// dimtext = `{\A1;{文字}{}{堆叠}}`（对照示例 `{\A1;<>{}{\C3;\SH7/h6;}}`）。
fn mtext_tol(text_part: &str, seg: &str) -> String {
    let mut t = String::from("{\\A1;");
    t.push_str(text_part);
    t.push_str("{}"); // 示例 `<>{}` 的空组结构
    t.push('{');
    t.push_str(seg);
    t.push('}');
    t.push('}');
    t
}

/// 文字模板渲染（用户引导规则）：
/// - `|||` → 公差堆叠生成位置（无公差时删除标记）
/// - `\X` → 换行（转 MTEXT `\P`）
/// - `[@]` → 英制换算，如 `<>[@]` 对 25.4 → `25.4[1"]`
/// 返回 dimtext 正文（不含 `{\A1;` 外层）；`<>` 保留给 OCS 测量值占位。
fn render_text_template(text: &str, measurement: f64, tol: Option<&str>) -> String {
    let mut out = text.replace("[@]", &format!("[{}]", inch_display(measurement)));
    out = out.replace("\\X", "\\P");
    match tol {
        Some(seg) => {
            if out.contains("|||") {
                // ||| 明确位置 → `{}` 空组 + 公差组（空组连接测量值与堆叠，
                // 缺它 OCS 会把 {tol} 组换行到下一行）。
                out.replace("|||", &format!("{{}}{{{}}}", seg))
            } else {
                // 默认追加末尾，保留 `{}` 空组结构（对照示例 `<>{}`）。
                format!("{}{{}}{{{}}}", out, seg)
            }
        }
        None => out.replace("|||", ""),
    }
}

/// 毫米 → 英寸显示（25.4 → `1"`），消尾零，最多 4 位小数。
fn inch_display(mm: f64) -> String {
    let inch = mm / 25.4;
    format!("{}\"", format_measurement(inch, 4))
}

/// 把 DIMENSION XDATA（ACAD/DSTYLE）中 DIMDEC(271) 的覆盖值改为 `dec`。
fn set_dimdec(dim: &mut Dimension, dec: i16) {
    let base = dim.base_mut();
    let recs: Vec<acadrust::xdata::ExtendedDataRecord> =
        base.common.extended_data.records().to_vec();
    base.common.extended_data.clear();
    for mut rec in recs {
        if rec.application_name == "ACAD" {
            for i in 0..rec.values.len().saturating_sub(1) {
                if matches!(rec.values[i], XDataValue::Integer16(271)) {
                    if let XDataValue::Integer16(v) = &mut rec.values[i + 1] {
                        *v = dec;
                    }
                }
            }
        }
        base.common.extended_data.add_record(rec);
    }
}

/// 剖切符号（匿名块 + INSERT）：对照 OCSMDIMGULIDE/SECTION/{全剖,旋转剖,阶梯剖}.dxf。
/// 引导线 = 多段 PLINE（顶点 = 剖切路径节点）；输出块成员（局部坐标，原点 = 路径首顶点）：
/// - 剖切线粗线（color=7 白）：每段两端各 `2×asz` 长（= 示例 10），中间断开。
/// - 视向箭头（SOLID color=4 青，长 asz=5、半宽 asz/6 ≈0.833、细长 3:1）+ 连接线`
///   （color=4，长 asz=5）：只在路径两端；箭头尖 = 端部 + side.dir(τ)×2×asz。
/// - 字母（MTEXT color=3 绿，字高 5s）：路径两端沿剖切方向延长 `1.3×asz`（= 6.5）。
/// - 视图名 `{L}-{L}\P\O`（color=3）：默认路径包围盒底部中心下方 8h，或用 marker 放置；
///   可选比例文本（scale）放视图名下方 1.4h。
/// 块名 *D{n}（与基准/角度共用匿名块计数）；INSERT 在 p0、8符号标注层。
fn apply_section(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    pts: &[[f64; 3]],
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::entities::{Insert, Line, MText, Solid};
    use acadrust::entities::AttachmentPoint;
    use acadrust::types::{Color, Vector3};
    use acadrust::EntityType as E;


    if pts.len() < 2 {
        return Err("剖切路径顶点不足（需至少 2 个）".into());
    }
    let origin = v3(pts[0]);
    let scale = frame_scale_at(doc, pts[0]);
    let asz = 5.0 * scale; // 箭头长 / 连接线长（示例 5）
    let cut = 10.0 * scale; // 剖切线粗线段长（示例 10）
    let h = 5.0 * scale; // 字母高（示例 5）
    let half = asz / 6.0; // 箭头底边半宽（示例 0.833 = 5/6）
    let off = 1.3 * asz; // 字母距剖切端（示例 6.5）

    // 字母：显式指定，否则自动编号（图纸已有单字母 MTEXT 序号 + 1）。
    let letter = params
        .letter
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| next_section_letter(doc));
    let text_style = "OCSM_GB";

    let mut members: Vec<E> = Vec::new();
    let mut mk_line = |a: Vector3, b: Vector3, color: i16| -> E {
        let mut e = E::Line(Line {
            common: Default::default(),
            start: a,
            end: b,
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(color);
        e
    };
    let mut mk_mtext = |value: String, pos: Vector3| -> E {
        let mut m = MText::new();
        m.value = value;
        m.insertion_point = pos;
        m.height = h;
        m.rotation = 0.0;
        m.style = text_style.into();
        m.attachment_point = AttachmentPoint::MiddleCenter;
        let mut e = E::MText(m);
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(3); // 绿色（对照示例字母/视图名）
        e
    };
    let mut mk_solid = |tip: Vector3, b1: Vector3, b2: Vector3| -> E {
        let mut e = E::Solid(Solid::new(tip, b1, b2, b2));
        set_member_layer(&mut e, "8符号标注层");
        // OCS 对随层 SOLID 渲染空心：显式青色为原生正常实心渲染。
        e.common_mut().color = Color::from_index(4);
        e
    };
    // 绝对坐标 → 块局部（原点 = 首顶点），并转为 Vector3。
    let to_local = |p: &[f64; 3]| -> Vector3 { v3(*p) - origin };

    // ── 剖切线粗线（每段两端各 cut，中间断开；段太短则整段） ──
    for i in 0..pts.len() - 1 {
        let pi = to_local(&pts[i]);
        let pj = to_local(&pts[i + 1]);
        let d = pj - pi;
        let len = d.length();
        if len < 1e-9 {
            continue;
        }
        let tau = d * (1.0 / len);
        if len <= 2.0 * cut {
            members.push(mk_line(pi, pj, 7));
        } else {
            members.push(mk_line(pi, pi + tau * cut, 7));
            members.push(mk_line(pj - tau * cut, pj, 7));
        }
    }

    // ── 视向箭头 + 连接线（路径两端） ──
    if params.show_arrow {
        let first = to_local(&pts[0]);
        let last = to_local(&pts[pts.len() - 1]);
        let tau0 = {
            let d = to_local(&pts[1]) - first;
            d * (1.0 / d.length().max(1e-12))
        };
        let taun = {
            let d = last - to_local(&pts[pts.len() - 2]);
            d * (1.0 / d.length().max(1e-12))
        };
        for (at, tau) in [(first, tau0), (last, taun)] {
            let dir = {
                let t = (tau.x, tau.y);
                let (dx, dy) = params.section_side.dir(t);
                Vector3::new(dx, dy, 0.0)
            };
            let base = at + dir * asz; // 底边中心（连接线外端）
            members.push(mk_line(at, base, 4)); // 连接线：端 → 底边中心
            let tip = at + dir * (asz + asz); // 尖 = 端 + dir×2×asz（示例 10）
            let b1 = base + tau * half;
            let b2 = base - tau * half;
            members.push(mk_solid(tip, b1, b2));
        }
    }

    // ── 字母（路径两端沿剖切方向延长 off） ──
    let tau0 = if pts.len() >= 2 {
        let d = to_local(&pts[1]) - to_local(&pts[0]);
        d * (1.0 / d.length().max(1e-12))
    } else {
        Vector3::new(0.0, -1.0, 0.0)
    };
    let taun = {
        let d = to_local(&pts[pts.len() - 1]) - to_local(&pts[pts.len() - 2]);
        d * (1.0 / d.length().max(1e-12))
    };
    members.push(mk_mtext(letter.clone(), to_local(&pts[0]) - tau0 * off));
    members.push(mk_mtext(letter.clone(), to_local(&pts[pts.len() - 1]) + taun * off));

    // ── 视图名 `{L}-{L}\P\O` + 可选比例文本 ──
    // 默认位置：路径包围盒底部中心下方 8h；marker 覆盖（绝对坐标）。
    let mut xmin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for p in pts {
        let v = to_local(p);
        xmin = xmin.min(v.x);
        xmax = xmax.max(v.x);
        ymin = ymin.min(v.y);
        ymax = ymax.max(v.y);
    }
    let (vmx, vmy) = match params.marker {
        // marker 为世界坐标（绝对），直接使用（同向视图 mx/my）。
        Some((mx, my)) => (mx, my),
        None => (
            origin.x + (xmin + xmax) * 0.5,
            origin.y + ymin - 8.0 * h,
        ),
    };
    let view_name = format!("{}-{}\\P\\O", letter, letter);
    let mut extras: Vec<E> = vec![mk_mtext(view_name, Vector3::new(vmx, vmy, 0.0))];
    if let Some(sc) = &params.scale {
        if !sc.is_empty() {
            extras.push(mk_mtext(
                sc.clone(),
                Vector3::new(vmx, vmy - 1.4 * h, 0.0),
            ));
        }
    }

    // ── 匿名块名 *D{n}（与基准/角度共用计数器） ──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*D") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*D{}", max_n + 1);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: members,
        },
        "AddBlockRecord",
    )?;

    // ── INSERT 在路径首顶点，8符号标注层 ──
    let mut ins = E::Insert(Insert::new(block_name, origin));
    set_member_layer(&mut ins, "8符号标注层");
    let mut all = vec![ins];
    all.append(&mut extras);
    let (handle, view_handle) = match req_timed(sender, PluginRequest::AddEntities(all), "AddEntities")? {
        PluginResponse::Handles(hs) => (hs.first().copied(), hs.get(1).copied()),
        _ => (None, None),
    };

    Ok(serde_json::json!({
        "ok": true,
        "insert_handle": handle.map(fmt_handle),
        "view_name_handle": view_handle.map(fmt_handle),
        "style": "OCSM_GB",
        "letter": letter,
        "side": params.section_side.as_str(),
        "show_arrow": params.show_arrow,
    })
    .to_string())
}

/// 形位公差框 FCF（Feature Control Frame，匿名块 + INSERT）：
/// 对照 OCSMDIMGULIDE/GD&T/形位.dxf 示例。引导线 = PLINE，≥3 顶点：
/// - P1 = 箭头尖（被测要素上）
/// - P2..Pn-1 = 引线折点（可选）
/// - Pn = 框接入点（FCF 左边缘中点）
/// 块成员局部坐标（原点 = P1）：箭头 SOLID(色 4) + 引线 LINE(色 4)
/// + 方框 LINE(色 31) + 单元格 MTEXT(色 3)。默认格宽/高 = 2h，h = 5×scale。
/// 单元格内容：[符号][Φ?]公差1][基准1][基准2][基准3]。符号格文本
/// 用 `{\Fgdt;X}` 内联字体码（OCSM MTEXT 支持）；公差前缀 ⌀ 用 `%%c`。
fn apply_tolerance(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    pts: &[[f64; 3]],
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::entities::{Insert, Line, MText, Solid};
    use acadrust::entities::AttachmentPoint;
    use acadrust::types::{Color, Vector3};
    use acadrust::EntityType as E;

    if pts.len() < 2 {
        return Err("形位公差引导线顶点不足（需至少 2 个：箭头尖 + 框接入点）".into());
    }
    let origin = v3(pts[0]);
    let scale = frame_scale_at(doc, pts[0]);
    let h = 5.0 * scale; // 单元格字高（同 SECTION/DATUM 默认）
    let cell = 2.0 * h; // 单元格宽/高（示例 2h×2h 方框）
    let half = h / 6.0; // 箭头底边半宽（同 SECTION）
    let text_style = "OCSM_GB";

    let mut members: Vec<E> = Vec::new();
    let mut mk_line = |a: Vector3, b: Vector3, color: i16| -> E {
        let mut e = E::Line(Line {
            common: Default::default(),
            start: a,
            end: b,
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(color);
        e
    };
    let mut mk_mtext = |value: String, pos: Vector3| -> E {
        let mut m = MText::new();
        m.value = value;
        m.insertion_point = pos;
        m.height = h;
        m.rotation = 0.0;
        m.style = text_style.into();
        m.attachment_point = AttachmentPoint::MiddleCenter;
        let mut e = E::MText(m);
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(3);
        e
    };
    let mut mk_solid = |tip: Vector3, b1: Vector3, b2: Vector3| -> E {
        let mut e = E::Solid(Solid::new(tip, b1, b2, b2));
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(4);
        e
    };
    let to_local = |p: &[f64; 3]| -> Vector3 { v3(*p) - origin };

    // ── 箭头：尖 = P1（向被测要素），底 = 尖 + u×h（沿 P1→P2 方向） ──
    let tip = to_local(&pts[0]);
    let p2 = to_local(&pts[1]);
    let dir = p2 - tip;
    let dlen = dir.length().max(1e-12);
    let u = dir * (1.0 / dlen);
    let perp = Vector3::new(-u.y, u.x, 0.0);
    let base_center = tip + u * h;
    let b1 = base_center + perp * half;
    let b2 = base_center - perp * half;
    members.push(mk_solid(tip, b1, b2));

    // ── 引线：箭头底 → P2 → P3 ... → Pn（框接入点） ──
    let mut prev = base_center;
    for i in 1..pts.len() {
        let cur = to_local(&pts[i]);
        members.push(mk_line(prev, cur, 4));
        prev = cur;
    }

    // ── 方框（FCF）：以 Pn 为左边缘中点，向 +x 延伸 ──
    // 单元格列：[符号] [⌀?公差] [基准1] [基准2] [基准3]（只推有内容的）。
    let mut cells: Vec<String> = Vec::new();
    if let Some(s) = params.gdt_sym.as_ref().filter(|s| !s.is_empty()) {
        // 只取首字母（gdt 字体每字母 = 一个符号），避免用户粘贴多字符。
        let c = s.chars().next().unwrap_or('b').to_ascii_lowercase();
        cells.push(format!("{{\\Fgdt;{}}}", c));
    }
    if let Some(t) = params.gdt_tol.as_ref().filter(|s| !s.is_empty()) {
        let prefix = if params.gdt_dia { "%%c" } else { "" };
        cells.push(format!("{}{}", prefix, t));
    }
    for d in [&params.gdt_d1, &params.gdt_d2, &params.gdt_d3] {
        if let Some(v) = d.as_ref().filter(|s| !s.is_empty()) {
            cells.push(v.clone());
        }
    }
    if cells.is_empty() {
        return Err("形位公差需至少一个单元格（符号/公差/基准）".into());
    }

    let attach = to_local(&pts[pts.len() - 1]); // 框左边缘中点（局部）
    let x0 = attach.x;
    let yc = attach.y;
    let ybot = yc - h;
    let ytop = yc + h;
    let total_w = cell * cells.len() as f64;
    // 左边竖线
    members.push(mk_line(
        Vector3::new(x0, ybot, 0.0),
        Vector3::new(x0, ytop, 0.0),
        31,
    ));
    // 每格：顶线 + 底线 + 右竖线 + 居中内容
    for (i, text) in cells.iter().enumerate() {
        let xi = x0 + cell * i as f64;
        let xj = xi + cell;
        members.push(mk_line(
            Vector3::new(xi, ytop, 0.0),
            Vector3::new(xj, ytop, 0.0),
            31,
        ));
        members.push(mk_line(
            Vector3::new(xi, ybot, 0.0),
            Vector3::new(xj, ybot, 0.0),
            31,
        ));
        members.push(mk_line(
            Vector3::new(xj, ytop, 0.0),
            Vector3::new(xj, ybot, 0.0),
            31,
        ));
        members.push(mk_mtext(text.clone(), Vector3::new(xi + h, yc, 0.0)));
    }

    // ── 匿名块名 *D{n}（与基准/角度/剖切共用计数器） ──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*D") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*D{}", max_n + 1);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: members,
        },
        "AddBlockRecord",
    )?;

    // ── INSERT 在箭头尖（P1），8符号标注层 ──
    let mut ins = E::Insert(Insert::new(block_name.clone(), origin));
    set_member_layer(&mut ins, "8符号标注层");
    let handle = match req_timed(sender, PluginRequest::AddEntities(vec![ins]), "AddEntities")? {
        PluginResponse::Handles(hs) => hs.first().copied(),
        _ => None,
    };

    Ok(serde_json::json!({
        "ok": true,
        "insert_handle": handle.map(fmt_handle),
        "block": block_name,
        "cells": cells.len(),
        "total_width": total_w,
        "style": "OCSM_GB",
    })
    .to_string())
}

/// 自动剖切字母编号：扫描模型空间里值为单一大写字母（A-Z）的 MTEXT，
/// 从 A 起取第一个未被占用的字母（已有 A → B，有 A/B → C…）。
fn next_section_letter(doc: &acadrust::CadDocument) -> String {
    let mut used = [false; 26];
    for e in doc.entities() {
        if let acadrust::EntityType::MText(m) = e {
            let b = m.value.trim().as_bytes();
            if b.len() == 1 && b[0].is_ascii_uppercase() {
                used[(b[0] - b'A') as usize] = true;
            }
        }
    }
    for (i, u) in used.iter().enumerate() {
        if !u {
            return ((b'A' + i as u8) as char).to_string();
        }
    }
    "A".to_string()
}

/// 基准标注（匿名块 + INSERT）：对照 OCSMDIMGULIDE4-1996/2008.dxf。
/// 块成员用**局部坐标**（原点 = P1），INSERT 在 P1、层 = 8符号标注层。
/// - GB/T 1182-1996：短横线（基准要素指示）+ 引线 + 圆 + 字母。
/// - GB/T 1182-2008：实心三角箭头（基准要素指示）+ 引线 + 方框 + 字母。
/// 尺寸比例（scale=1）：h=5、引线长 2h、圆/框中心距 3.057h、圆 r=1.057h、
/// 方框半宽 1.057h、框底 2h、框顶 4.114h；1996 短横线距 0.2887h 半宽 0.866h；
/// 2008 箭头尖 0.866h 底 ±0.5h。
fn apply_datum(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p1: [f64; 3],
    p2: [f64; 3],
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::entities::{Circle, Insert, Line, MText};
    use acadrust::entities::AttachmentPoint;
    use acadrust::types::Vector3;
    use acadrust::EntityType as E;
    use crate::guide_url::DatumVersion as DV;

    let center = v3(p1);
    let dir_v = v3(p2) - center;
    let len = dir_v.length();
    if len < 1e-9 {
        return Err("引导线长度为 0".into());
    }
    let dir = dir_v * (1.0 / len);
    let perp = Vector3::new(-dir.y, dir.x, 0.0);

    // 图幅缩放 → 字母高/符号尺寸。
    let scale = frame_scale_at(doc, p1);
    let h = 5.0 * scale; // 字母高（示例 h=5）
    let letter = params
        .letter
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "A".into());

    // 块成员（局部坐标，原点=P1；方向由 dir 决定，字母恒水平 rot=0）。
    let mut members: Vec<E> = Vec::new();
    let mut mk_line = |a: Vector3, b: Vector3| -> E {
        let mut e = E::Line(Line {
            common: Default::default(),
            start: a,
            end: b,
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "8符号标注层");
        e
    };
    let mut mk_mtext = |value: String, pos: Vector3| -> E {
        let mut m = MText::new();
        m.value = value;
        m.insertion_point = pos;
        m.height = h;
        m.rotation = 0.0;
        m.style = "OCSM_GB".into();
        m.attachment_point = AttachmentPoint::MiddleCenter;
        let mut e = E::MText(m);
        set_member_layer(&mut e, "8符号标注层");
        e
    };

    let lead_end = dir * (2.0 * h); // 引线端（圆/方框底）
    let box_center = dir * (3.057 * h); // 圆/方框中心
    let box_half = 1.057 * h; // 圆半径 / 方框半宽

    match params.ver {
        DV::GB1996 => {
            // 短横线（基准要素指示）：距 P1 0.2887h，半宽 0.866h。
            let tick = dir * (0.2887 * h);
            members.push(mk_line(tick + perp * (0.866 * h), tick - perp * (0.866 * h)));
            // 引线：从短横线中点 → 圆底（2h）。对照示例 *D68：
            // LINE(1.4434,0)→(10,0) 起点=短横线处，不从 P1(0) 开始。
            members.push(mk_line(tick, lead_end));
            // 圆 + 字母。
            let mut c = E::Circle(Circle::from_center_radius(box_center, box_half));
            set_member_layer(&mut c, "8符号标注层");
            members.push(c);
            members.push(mk_mtext(letter.clone(), box_center));
        }
        DV::GB2008 => {
            // 实心三角箭头（基准要素指示）：尖在 0.866h，底在 P1（±0.5h 垂直）。
            let tip = dir * (0.866 * h);
            let base_w = perp * (0.5 * h);
            let mut ar = E::Solid(acadrust::entities::Solid::new(
                tip,
                base_w,
                -base_w,
                -base_w,
            ));
            set_member_layer(&mut ar, "8符号标注层");
            members.push(ar);
            // 引线：P1 → 方框底（2h）。
            members.push(mk_line(Vector3::ZERO, lead_end));
            // 方框（底 2h → 顶 4.114h，半宽 1.057h）。
            let top = dir * (4.114 * h);
            let bl = lead_end - perp * box_half;
            let br = lead_end + perp * box_half;
            let tl = top - perp * box_half;
            let tr = top + perp * box_half;
            members.push(mk_line(bl, tl));
            members.push(mk_line(br, tr));
            members.push(mk_line(bl, br));
            members.push(mk_line(tl, tr));
            // 字母（方框中心）。
            members.push(mk_mtext(letter.clone(), box_center));
        }
    }

    // ── 匿名块名：*D{n} ──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*D") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*D{}", max_n + 1);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: members,
        },
        "AddBlockRecord",
    )?;

    // ── INSERT 在 P1，层 = 8符号标注层 ──
    let mut ins = E::Insert(Insert::new(block_name, center));
    set_member_layer(&mut ins, "8符号标注层");
    let handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![ins]),
        "AddEntities",
    )? {
        PluginResponse::Handles(hs) => hs.first().copied(),
        _ => None,
    };

    Ok(serde_json::json!({
        "ok": true,
        "insert_handle": handle.map(fmt_handle),
        "style": "OCSM_GB",
        "version": params.ver.as_str(),
        "letter": letter,
    })
    .to_string())
}

/// 向视图标注（匿名块 + INSERT）：对照 OCSMDIMGULIDE5-arrow.dxf / -symbol.dxf。
/// 两个匿名块：
/// - 箭头块 *V{a}：原点 = 箭头底中点（INSERT 在箭头端 P2），成员 = SOLID 实心三角
///   （尖 +x：长 0.7h、半宽 0.117h）+ 引线 LINE（原点 → −x 方向 10×scale）。
///   INSERT rotation = 引导线方向角（end−start），使尖朝引导线方向、引线朝反方向。
/// - 字母符号块 *V{b}：原点 = 字母中心（INSERT 在标记放置点，缺省 (0,0)）。
///   样式1（无比例）= 字母 + 横线；样式2（有比例）= + 比例文本；
///   样式3（翻转方向 flip）= + 旋转弧线箭头（ARC + SOLID，对照 symbol.dxf 样式3）。
fn apply_view(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p1: [f64; 3],
    p2: [f64; 3],
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::entities::{Arc, Insert, Line, MText, Solid};
    use acadrust::entities::AttachmentPoint;
    use acadrust::types::{Color, Vector3};
    use acadrust::EntityType as E;

    let arrow_pt = v3(p2); // 箭头端 = 引导线终点（end）
    let start = v3(p1); // 引导线起点（字母端）
    let dir_v = arrow_pt - start;
    let len = dir_v.length();
    if len < 1e-9 {
        return Err("引导线长度为 0".into());
    }
    let dir = dir_v * (1.0 / len);

    // 图幅缩放（箭头端）→ 字母高/箭头尺寸；引线固定 10×scale。
    let scale = frame_scale_at(doc, p2);
    let h = 5.0 * scale; // 字母高（示例 h=5）
    let leader_len = 10.0 * scale; // 箭头后引线
    let letter = params
        .letter
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "A".into());

    let mut mk_line = |a: Vector3, b: Vector3| -> E {
        let mut e = E::Line(Line {
            common: Default::default(),
            start: a,
            end: b,
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "8符号标注层");
        e
    };
    let mut mk_mtext = |value: String, pos: Vector3, rotation: f64| -> E {
        let mut m = MText::new();
        m.value = value;
        m.insertion_point = pos;
        m.height = h;
        m.rotation = rotation;
        m.style = "OCSM_GB".into();
        m.attachment_point = AttachmentPoint::MiddleCenter;
        let mut e = E::MText(m);
        set_member_layer(&mut e, "8符号标注层");
        e
    };
    let mut mk_solid = |a: Vector3, b: Vector3, c: Vector3, d: Vector3| -> E {
        let mut e = E::Solid(Solid::new(a, b, c, d));
        set_member_layer(&mut e, "8符号标注层");
        // 显式青色：OCS 对随层(256) SOLID 渲染异常（空心），青色为原生正常实心渲染。
        e.common_mut().color = Color::from_index(4);
        e
    };
    let mut mk_arc = |center: Vector3, r: f64, a0: f64, a1: f64| -> E {
        let mut e = E::Arc(Arc::from_center_radius_angles(center, r, a0, a1));
        set_member_layer(&mut e, "8符号标注层");
        e
    };

    // ── 块名：*V{a}（箭头）、*V{b}（字母符号） ──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*V") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let arrow_block = format!("*V{}", max_n + 1);
    let marker_block = format!("*V{}", max_n + 2);

    // INSERT 旋转角 = 引导线方向角。块内字母 rotation 用其负值抵消 → 字母全局始终水平。
    let rot = dir.y.atan2(dir.x);

    // ── 箭头块成员（局部坐标，原点 = 箭头底中点；尖朝 +x） ──
    let mut arrow_members: Vec<E> = Vec::new();
    // SOLID：尖 (0.7h,0)，底 (0,±0.117h)。对照 arrow.dxf（长 3.5=0.7×5、半宽 0.586）。
    let tip = Vector3::new(0.7 * h, 0.0, 0.0);
    let base_up = Vector3::new(0.0, 0.117 * h, 0.0);
    let base_dn = Vector3::new(0.0, -0.117 * h, 0.0);
    // v3 = 底重复（对照基准/OCS 原生箭头；v3=尖 会渲染成空心）。
    arrow_members.push(mk_solid(tip, base_up, base_dn, base_dn));
    // 引线：原点 → −x 方向 10×scale（INSERT 旋转后朝反箭头方向）。
    arrow_members.push(mk_line(
        Vector3::ZERO,
        Vector3::new(-leader_len, 0.0, 0.0),
    ));
    // 视图编号字母：引线起点端（最左），中心在引线端外 0.43h
    // （对照用户 arrow_problem4.dxf 下组：字母中心 = 引线端 − 2.144, h=5）。
    // rotation = −rot 抵消 INSERT 旋转 → 字母全局始终水平（同一块内两全）。
    arrow_members.push(mk_mtext(
        letter.clone(),
        Vector3::new(-leader_len - 0.43 * h, 0.0, 0.0),
        -rot,
    ));

    // ── 字母符号块成员（局部坐标，原点 = 字母中心） ──
    let mut sym_members: Vec<E> = Vec::new();
    sym_members.push(mk_mtext(letter.clone(), Vector3::ZERO, 0.0));
    // 横线：字母下方 0.7h，半宽 1.2h（覆盖比例文本宽）。
    let hline_y = -0.7 * h;
    sym_members.push(mk_line(
        Vector3::new(-1.2 * h, hline_y, 0.0),
        Vector3::new(1.2 * h, hline_y, 0.0),
    ));
    // 比例文本（样式2）：横线下方 0.7h。
    if let Some(sc) = &params.scale {
        if !sc.is_empty() {
            sym_members.push(mk_mtext(sc.clone(), Vector3::new(0.0, -1.4 * h, 0.0), 0.0));
        }
    }
    // 翻转方向（样式3）：旋转弧线箭头。逆时针对照 symbol.dxf 样式3（h=5 时相对几何）；
    // 顺时针为逆时针的镜像（绕 Y 轴 x→−x，弧线翻到字母左侧，箭头朝左下）。
    match params.flip {
        crate::guide_url::FlipDir::Clockwise => {
            // 镜像 ARC：圆心在字母左 1.5536h、下 0.5h，r=h，40.975°→180°。
            let ac = Vector3::new(-1.5536 * h, -0.5 * h, 0.0);
            let a0 = (180.0 - 139.025_f64).to_radians(); // 40.975°
            let a1 = 180.0_f64.to_radians();
            sym_members.push(mk_arc(ac, h, a0, a1));
            // 镜像 SOLID 箭头：尖 (−0.5536h,−0.5h)，底两角（x 取负）。
            let s0 = Vector3::new(-0.5536 * h, -0.5 * h, 0.0);
            let s1 = Vector3::new(-0.8848 * h, 0.1236 * h, 0.0);
            let s2 = Vector3::new(-0.7122 * h, 0.1880 * h, 0.0);
            // v3 = 底重复（避免空心渲染）。
            sym_members.push(mk_solid(s0, s1, s2, s2));
        }
        crate::guide_url::FlipDir::CounterClockwise => {
            // ARC：圆心在字母中心右 1.5536h、下 0.5h，r=h，0°→139.025°。
            let ac = Vector3::new(1.5536 * h, -0.5 * h, 0.0);
            let a1 = 139.025_f64.to_radians();
            sym_members.push(mk_arc(ac, h, 0.0, a1));
            // SOLID 箭头：尖在圆心左（0.5536h,−0.5h），底两角。
            let s0 = Vector3::new(0.5536 * h, -0.5 * h, 0.0);
            let s1 = Vector3::new(0.8848 * h, 0.1236 * h, 0.0);
            let s2 = Vector3::new(0.7122 * h, 0.1880 * h, 0.0);
            // v3 = 底重复（翻转符号箭头同样避免空心渲染）。
            sym_members.push(mk_solid(s0, s1, s2, s2));
        }
        crate::guide_url::FlipDir::None => {}
    }

    // 注册两个块。
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: arrow_block.clone(),
            entities: arrow_members,
        },
        "AddBlockRecord",
    )?;
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: marker_block.clone(),
            entities: sym_members,
        },
        "AddBlockRecord",
    )?;

    // ── INSERT：箭头在 P2（箭头端），旋转到引导线方向；字母符号在标记放置点 ──
    // 视图编号字母已在箭头块内（引线起点端，rotation 抵消保持水平）。
    let mut ia = Insert::new(arrow_block, arrow_pt);
    ia.rotation = rot;
    let mut ins_arrow = E::Insert(ia);
    set_member_layer(&mut ins_arrow, "8符号标注层");
    let (mx, my) = params.marker.unwrap_or((0.0, 0.0));
    let mut ins_marker = E::Insert(Insert::new(
        marker_block,
        Vector3::new(mx, my, 0.0),
    ));
    set_member_layer(&mut ins_marker, "8符号标注层");
    let handles = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![ins_arrow, ins_marker]),
        "AddEntities",
    )? {
        PluginResponse::Handles(hs) => hs,
        _ => Vec::new(),
    };

    Ok(serde_json::json!({
        "ok": true,
        "arrow_insert_handle": handles.first().copied().map(fmt_handle),
        "marker_insert_handle": handles.get(1).copied().map(fmt_handle),
        "style": "OCSM_GB",
        "letter": letter,
        "scale": params.scale,
        "flip": params.flip.as_str(),
    })
    .to_string())
}

fn do_apply(
    sender: &Arc<dyn PluginRequestSender>,
    body: &[u8],
    refresh: bool,
) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Req {
        handle: String,
        url: String,
    }
    let req: Req =
        serde_json::from_slice(body).map_err(|e| format!("请求 JSON 无效: {e}"))?;
    let handle = handle_hex(&req.handle)?;
    let params = GuideParams::from_url(&req.url).ok_or_else(|| format!("URL 无法解析: {}", req.url))?;

    // 应用：先写 URL（不生成）。
    req_timed(
        sender,
        PluginRequest::WriteRecord {
            handle,
            record: pe_url_record(&req.url),
        },
        "WriteRecord",
    )?;
    // 写 PE_URL 也是图纸改动，需标记未保存。
    mark_dirty(sender)?;

    if !refresh {
        return Ok(r#"{"ok":true}"#.into());
    }

    // 应用并刷新：生成真实标注 + 删引导线 + REGEN。
    let doc = snapshot(sender)?;
    // 引导几何顶点：LINE→2 点；ANGLE 用两段 PLINE（3 顶点）。
    let pts = guide_geom_points(&doc, handle)?;
    if pts.len() < 2 {
        return Err("引导线顶点不足（需至少 2 个）".into());
    }
    let (p1, p2) = (pts[0], pts[1]);
    // 按引导线第一点 P1 判定图幅缩放，创建/选用对应样式（文字/箭头缩放）。
    let style = ensure_style_for_point(sender, &doc, p1)?;

    req_timed(
        sender,
        PluginRequest::PushUndo {
            label: "尺寸引导".into(),
        },
        "PushUndo",
    )?;

    // 基准标注：不产生 DIMENSION，而是 匿名块 + INSERT（8符号标注层）。
    if params.guide_type == GuideType::Datum {
        let out = apply_datum(sender, &doc, p1, p2, &params)?;
        req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        return Ok(out);
    }

    // 向视图标注：箭头块 + 字母符号块（两个匿名块 + INSERT）。
    if params.guide_type == GuideType::View {
        let out = apply_view(sender, &doc, p1, p2, &params)?;
        req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        return Ok(out);
    }

    // 角度标注：两段 PLINE（3 顶点）→ Angular2Ln（角平分线方向放置）。
    if params.guide_type == GuideType::Angle {
        if pts.len() < 3 {
            return Err("角度标注需要两段多段线（PLINE，3 顶点）".into());
        }
        let dim = build_guide_angular_block(sender, &doc, pts[0], pts[1], pts[2], &params, &style)?;
        let dim_handle = match req_timed(
            sender,
            PluginRequest::AddEntities(vec![acadrust::EntityType::Dimension(dim)]),
            "AddEntities",
        ) {
            Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
            Ok(_) => None,
            Err(e) => return Err(e),
        };
        req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        return Ok(serde_json::json!({
            "ok": true,
            "dimension_handle": dim_handle.map(fmt_handle),
            "style": "OCSM_GB",
        })
        .to_string());
    }

    // 剖切符号：多段 PLINE 路径（2~N 顶点）→ 匿名块 + INSERT（8符号标注层）。
    if params.guide_type == GuideType::Section {
        let out = apply_section(sender, &doc, &pts, &params)?;
        req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        return Ok(out);
    }

    // 形位公差 FCF：多段 PLINE（≥3 顶点）→ 匿名块 + INSERT（箭头尖=P1，方框=Pn）。
    if params.guide_type == GuideType::Tolerance {
        let out = apply_tolerance(sender, &doc, &pts, &params)?;
        req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        return Ok(out);
    }

    let dim = build_dimension(sender, &doc, p1, p2, &params, &style)?;
    let dim_handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![acadrust::EntityType::Dimension(dim)]),
        "AddEntities",
    ) {
        Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
        Ok(_) => None,
        Err(e) => return Err(e),
    };
    req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;
    req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
    mark_dirty(sender)?;

    Ok(serde_json::json!({
        "ok": true,
        "dimension_handle": dim_handle.map(fmt_handle),
        "style": "OCSM_GB",
    })
    .to_string())
}

// ── API 处理器 ────────────────────────────────────────────────────────────

fn api_guide(target: &str, sender: &Arc<dyn PluginRequestSender>) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    let handle_s = target
        .split_once('?')
        .and_then(|(_, q)| q.split('&').find_map(|kv| kv.split_once('=').map(|(k, v)| (k, v))).filter(|(k, _)| *k == "handle").map(|(_, v)| v))
        .unwrap_or("");
    let handle = match handle_hex(handle_s) {
        Ok(h) => h,
        Err(e) => return (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
    };
    match snapshot(sender) {
        Err(e) => (500, json, serde_json::json!({"ok": false, "error": e}).to_string()),
        Ok(doc) => match guide_geom_points(&doc, handle) {
            Err(e) => (404, json, serde_json::json!({"ok": false, "error": e}).to_string()),
            Ok(pts) => {
                let (p1, p2) = (pts[0], pts[1]);
                let url = read_pe_url(sender, handle);
                let params = url.as_deref().and_then(GuideParams::from_url);
                let resp = serde_json::json!({
                    "ok": true,
                    "handle": fmt_handle(handle),
                    "p1": [p1[0], p1[1], p1[2]],
                    "p2": [p2[0], p2[1], p2[2]],
                    // 引导线全部顶点（ANGLE 两段 PLINE = 3 个：p0/p1/p2）。
                    "pts": pts.iter().map(|q| [q[0], q[1], q[2]]).collect::<Vec<_>>(),
                    // 引导几何形态：line（LINE）/ pline（LWPOLYLINE，2+ 顶点）。
                    "geom": guide_geom_kind(&doc, handle).unwrap_or("line"),
                    "url": url,
                    // 引导线长度 = 主尺寸（公差计算用）。
                    "measurement": ((p2[0]-p1[0]).powi(2)+(p2[1]-p1[1]).powi(2)).sqrt(),
                    "params": params.map(|p| serde_json::json!({
                        "type": p.guide_type.as_str(),
                        "sub": p.sub.map(|s| s.as_str().to_string()),
                        "angle_mode": p.angle_mode.as_str().to_string(),
                        "dist": p.dist,
                        "text": p.text,
                        "tol": p.tol,
                        "up": p.up,
                        "dn": p.dn,
                        "fit": p.fit,
                        "sym": p.sym,
                        "letter": p.letter,
                        "scale": p.scale,
                        "marker": p.marker,
                        "section_side": p.section_side.as_str().to_string(),
                        "show_arrow": p.show_arrow,
                        "gdt_sym": p.gdt_sym,
                        "gdt_dia": p.gdt_dia,
                        "gdt_tol": p.gdt_tol,
                        "gdt_d1": p.gdt_d1,
                        "gdt_d2": p.gdt_d2,
                        "gdt_d3": p.gdt_d3,
                    })),
                });
                (200, json, resp.to_string())
            }
        },
    }
}

/// `GET /api/tolerance?dim=25&fit=H7/g6`：ISO 286 极限偏差查询。
/// 返回 up/dn 显示（mm，3 位小数）+ 孔/轴数值 + 配合类型。GUI 模态表格点击时调用。
fn api_tolerance(target: &str) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    let q = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut dim = 0.0f64;
    let mut fit_s = String::new();
    for kv in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        match k {
            "dim" => dim = v.parse().unwrap_or(0.0),
            "fit" => fit_s = crate::guide_url::percent_decode(v),
            _ => {}
        }
    }
    if dim <= 0.0 {
        return (400, json, serde_json::json!({"ok": false, "error": "dim 必须 > 0"}).to_string());
    }
    if fit_s.is_empty() {
        return (400, json, serde_json::json!({"ok": false, "error": "fit 必填"}).to_string());
    }
    let (up, dn) = match crate::tolerance::resolve_fit_mm(dim, &fit_s) {
        Some(v) => v,
        None => {
            return (
                400,
                json,
                serde_json::json!({"ok": false, "error": "配合代号不可用", "fit": fit_s}).to_string(),
            )
        }
    };
    // 配合类型（仅 "H7/g6" 双代号有）。
    let kind = if let Some((h, s)) = fit_s.split_once('/') {
        crate::tolerance::fit(dim, h.trim(), s.trim())
            .map(|f| f.kind.label().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let hole_limits = fit_s
        .split_once('/')
        .and_then(|(h, _)| crate::tolerance::hole(dim, h.trim()));
    let shaft_limits = fit_s
        .split_once('/')
        .and_then(|(_, s)| crate::tolerance::shaft(dim, s.trim()));
    let mut resp = serde_json::json!({
        "ok": true,
        "dim": dim,
        "fit": fit_s,
        "up": up,
        "dn": dn,
        "kind": kind,
    });
    if let Some(h) = hole_limits {
        resp["hole"] = serde_json::json!({"upper": h.upper, "lower": h.lower});
    }
    if let Some(sh) = shaft_limits {
        resp["shaft"] = serde_json::json!({"upper": sh.upper, "lower": sh.lower});
    }
    (200, json, resp.to_string())
}

fn api_apply(
    body: &[u8],
    sender: &Arc<dyn PluginRequestSender>,
    refresh: bool,
) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    match do_apply(sender, body, refresh) {
        Ok(s) => (200, json, s),
        Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
    }
}

// ── MCP 桥接端点 ──────────────────────────────────────────────────────────

/// MCP 独立二进制把工具调用转发到这里：body = {"method": "...", "params": {...}}。
/// 返回 {"result": ...} 或 {"error": ...}。
fn api_mcp(body: &[u8], sender: &Arc<dyn PluginRequestSender>) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    #[derive(serde::Deserialize)]
    struct McpReq {
        method: String,
        params: Option<serde_json::Value>,
    }
    let req: McpReq = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return (400, json, serde_json::json!({"error": format!("JSON 无效: {e}")}).to_string()),
    };
    let result = match req.method.as_str() {
        "list_guides" => mcp_list_guides(sender),
        "get_guide" => {
            let handle = req.params.as_ref().and_then(|p| p.get("handle")).and_then(|h| h.as_str()).unwrap_or("").to_string();
            let resp = api_guide(&format!("/api/guide?handle={handle}"), sender);
            if resp.0 == 200 { Ok(resp.2) } else { Err(resp.2) }
        }
        "apply" | "apply_refresh" => {
            let refresh = req.method == "apply_refresh";
            let body2 = serde_json::to_vec(&req.params.unwrap_or(serde_json::json!({}))).unwrap_or_default();
            do_apply(sender, &body2, refresh).map_err(|e| e)
        }
        other => Err(format!("未知 MCP 方法: {other}")),
    };
    match result {
        Ok(s) => (200, json, s),
        Err(e) => (400, json, serde_json::json!({"error": e}).to_string()),
    }
}

fn mcp_list_guides(sender: &Arc<dyn PluginRequestSender>) -> Result<String, String> {
    let doc = snapshot(sender)?;
    let mut out = Vec::new();
    for e in doc.entities() {
        let acadrust::EntityType::Line(l) = e else { continue };
        if !l.common.layer.eq_ignore_ascii_case("10引导线层") {
            continue;
        }
        let h = l.common.handle;
        let Some(url) = read_pe_url(sender, h) else { continue };
        if !url.contains("/DIM/") {
            continue;
        }
        out.push(serde_json::json!({
            "handle": fmt_handle(h),
            "p1": [l.start.x, l.start.y, l.start.z],
            "p2": [l.end.x, l.end.y, l.end.z],
            "url": url,
        }));
    }
    Ok(serde_json::json!({"guides": out}).to_string())
}

// ── 浏览器 GUI（对应 GUI.png） ─────────────────────────────────────────────

const GUI_HTML: &str = include_str!("guide_gui.html");

#[cfg(test)]
mod tests {
    use super::*;
    use ocs_plugin_api::host::acadrust::entities::{Dimension, EntityType as E};

    /// 对照 OCSMDIMGULIDE1-general.dxf：引导线 (0,0)→(40,30)，对齐标注
    /// defpoint=(34,38)，7标注层，OCSM_GB 样式。
    #[test]
    fn aligned_dimension_matches_general_dxf() {
        let dim = build_guide_linear([0.0, 0.0, 0.0], [40.0, 30.0, 0.0], LinearSub::Aligned, 10.0, "OCSM_GB");
        let d = match &dim {
            Dimension::Aligned(d) => d,
            _ => panic!("应为 Aligned 标注"),
        };
        assert!((d.definition_point.x - 34.0).abs() < 1e-6);
        assert!((d.definition_point.y - 38.0).abs() < 1e-6);
        assert_eq!(d.base.common.layer, "7标注层");
        assert_eq!(d.base.style_name, "OCSM_GB");
        // 测量值 = 两点距离 50。
        assert!((d.base.actual_measurement - 50.0).abs() < 1e-6);
    }

    #[test]
    fn horizontal_dist_is_y_of_definition_point() {
        // 水平：尺寸线过 pt = P2 + perp*dist；dist 沿法向。
        let dim = build_guide_linear([0.0, 0.0, 0.0], [40.0, 0.0, 0.0], LinearSub::Horizontal, -20.0, "OCSM_GB");
        let d = match &dim {
            Dimension::Linear(d) => d,
            _ => panic!("应为 Linear 标注"),
        };
        // 法向 (0,1)；P2=(40,0) + (-20)*(0,1) = (40,-20)。
        assert!((d.definition_point.x - 40.0).abs() < 1e-6);
        assert!((d.definition_point.y + 20.0).abs() < 1e-6);
        assert!((d.base.actual_measurement - 40.0).abs() < 1e-6);
    }

    #[test]
    fn vertical_uses_perp_axis() {
        let dim = build_guide_linear([0.0, 0.0, 0.0], [0.0, 40.0, 0.0], LinearSub::Vertical, 15.0, "OCSM_GB");
        let d = match &dim {
            Dimension::Linear(d) => d,
            _ => panic!("应为 Linear 标注"),
        };
        // 法向 = (-1,0)；P2=(0,40) + 15*(-1,0) = (-15,40)。
        assert!((d.definition_point.x + 15.0).abs() < 1e-6);
        assert!((d.definition_point.y - 40.0).abs() < 1e-6);
    }

    #[test]
    fn built_dimension_carries_dstyle_override() {
        use acadrust::xdata::XDataValue as V;
        let dim = build_guide_linear([0.0, 0.0, 0.0], [40.0, 30.0, 0.0], LinearSub::Aligned, 10.0, "OCSM_GB");
        let xd = &dim.base().common.extended_data;
        let rec = xd.get_record("ACAD").expect("应有 ACAD/DSTYLE XDATA");
        assert!(matches!(rec.values.first(), Some(V::String(s)) if s == "DSTYLE"));
        // 扁平流必须含 (271 → 4) dimdec 与 (74 → 1) dimtoh。
        let mut codes = std::collections::HashMap::new();
        let mut it = rec.values.iter().skip(1).peekable();
        while let Some(v) = it.next() {
            if let V::Integer16(code) = v {
                if let Some(val) = it.peek() {
                    match val {
                        V::Integer16(n) => { codes.insert(*code, format!("int{n}")); }
                        V::Real(r) => { codes.insert(*code, format!("real{r}")); }
                        _ => {}
                    }
                }
            }
        }
        assert_eq!(codes.get(&271).map(|s| s.as_str()), Some("int4"));
        assert_eq!(codes.get(&74).map(|s| s.as_str()), Some("int1"));
        assert_eq!(codes.get(&41).map(|s| s.as_str()), Some("real2.5"));
    }

    #[test]
    fn aligned_dimension_text_rotates_with_dimension_line() {
        // (0,0)->(40,30) 对齐标注：尺寸线方向 36.87°，文字应跟随旋转。
        let dim = build_guide_linear([0.0, 0.0, 0.0], [40.0, 30.0, 0.0], LinearSub::Aligned, 10.0, "OCSM_GB");
        let rot = dim.base().text_rotation;
        let expect = (30.0f64).atan2(40.0);
        assert!((rot - expect).abs() < 1e-9, "rot={rot} expect={expect}");
        // 位置交给 host dimtad=1（尺寸线上方），不强制 text_user_positioned。
        assert!(!dim.base().text_user_positioned);
        // 水平：文字水平 0°。
        let h = build_guide_linear([0.0, 0.0, 0.0], [40.0, 30.0, 0.0], LinearSub::Horizontal, 10.0, "OCSM_GB");
        assert_eq!(h.base().text_rotation, 0.0);
        // 竖直：文字逆时针 90°。
        let v = build_guide_linear([0.0, 0.0, 0.0], [40.0, 30.0, 0.0], LinearSub::Vertical, 10.0, "OCSM_GB");
        assert!((v.base().text_rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }


    #[test]
    fn radial_diameter_matches_inside_example() {
        // 对照 OCSMDIMGULIDE2-inside.dxf：圆心(0,0) r=50，引导线 45°，
        // 内侧文字距圆心 41（= r-9，dist=-9），leader=0，7标注层/OCSM_GB。
        let dim = build_guide_radial([0.0, 0.0, 0.0], [35.35533905932738, 35.35533905932737, 0.0], GuideType::Diameter, -9.0, "OCSM_GB");
        let d = match &dim {
            Dimension::Diameter(d) => d,
            _ => panic!("应为 Diameter 标注"),
        };
        // 第一版字段语义：angle_vertex=圆心（测量正确）、definition_point=圆周点。
        assert!((d.angle_vertex.x - 0.0).abs() < 1e-9 && (d.angle_vertex.y - 0.0).abs() < 1e-9);
        assert!((d.definition_point.x - 35.35533905932738).abs() < 1e-6);
        // 文字锚点 = 径向 41 + 法向 DIMGAP 1.0。
        let ux = 35.35533905932738 / 50.0;
        let uy = 35.35533905932737 / 50.0;
        let tm = d.base.text_middle_point;
        assert!(((tm.x * ux + tm.y * uy) - 41.0).abs() < 1e-6, "径向分量");
        assert!(((tm.x * -uy + tm.y * ux) - 1.0).abs() < 1e-6, "法向偏移");
        assert_eq!(d.leader_length, 0.0);
        assert_eq!(d.base.common.layer, "7标注层");
        assert_eq!(d.base.style_name, "OCSM_GB");
        // 测量值 = 直径 100（actual 手动设 2×半径）。
        assert!((d.base.actual_measurement - 100.0).abs() < 1e-6);
        // v0.9.8 cadcodec：Diameter measurement() = 两点距离（圆心↔圆周点 = 半径 50），
        // 不再 ×2；actual_measurement 由插件手动设为直径。
        assert!((d.measurement() - 50.0).abs() < 1e-6);
        // 文字随引线旋转（径向 45°）+ 锚点上方（BottomCenter）。
        let expect_rot = (35.35533905932737f64).atan2(35.35533905932738);
        assert!((dim.base().text_rotation - expect_rot).abs() < 1e-6);
        assert!(dim.base().text_user_positioned);
        use acadrust::entities::dimension::AttachmentPointType as APT;
        assert_eq!(dim.base().attachment_point, APT::BottomCenter);
        // XDATA 含 DIMCEN=0（不画圆心十字）。
        let rec = dim.base().common.extended_data.get_record("ACAD").unwrap();
        let mut it = rec.values.iter();
        let mut dimcen0 = false;
        while let Some(v) = it.next() {
            if let acadrust::xdata::XDataValue::Integer16(141) = v {
                if let Some(acadrust::xdata::XDataValue::Integer16(0)) = it.next() {
                    dimcen0 = true;
                }
            }
        }
        assert!(dimcen0, "应有 DIMCEN=0 覆盖");
    }

    #[test]
    fn radial_radius_matches_outside_example() {
        // 对照 OCSMDIMGULIDE2-outside.dxf：外侧文字距圆心 61（= r+11，dist=+11）。
        let dim = build_guide_radial([0.0, 0.0, 0.0], [35.35533905932738, 35.35533905932737, 0.0], GuideType::Radius, 11.0, "OCSM_GB");
        let d = match &dim {
            Dimension::Radius(d) => d,
            _ => panic!("应为 Radius 标注"),
        };
        let tm = d.base.text_middle_point;
        let radial = tm.x * (35.35533905932738 / 50.0) + tm.y * (35.35533905932737 / 50.0);
        assert!((radial - 61.0).abs() < 1e-6, "径向分量 {radial}");
        // 外侧：leader 从圆周点向文字延伸 11。
        assert!((d.leader_length - 11.0).abs() < 1e-6);
        assert!((d.angle_vertex.length()).abs() < 1e-6, "angle_vertex=圆心");
        // 测量值 = 半径 50。
        assert!((d.base.actual_measurement - 50.0).abs() < 1e-6);
    }

    #[test]
    fn format_measurement_strips_trailing_zeros() {
        // 默认 2 位：消尾零，整数不消零。
        assert_eq!(format_measurement(12.70, 2), "12.7");
        assert_eq!(format_measurement(12.00, 2), "12");
        assert_eq!(format_measurement(12.0, 2), "12");
        assert_eq!(format_measurement(12.345, 2), "12.35");
        assert_eq!(format_measurement(0.004, 2), "0");
        assert_eq!(format_measurement(-0.001, 2), "0");
        // 位数可调。
        assert_eq!(format_measurement(12.3456, 3), "12.346");
        assert_eq!(format_measurement(12.3456, 0), "12");
        assert_eq!(format_measurement(100.0, 4), "100");
    }

    #[test]
    fn apply_section_builds_block_auto_letter() {
        use crate::guide_url::SectionSide as SS;
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        // 竖直剖切路径（全剖形态，2 顶点）。
        let pts = vec![[0.0, 0.0, 0.0], [0.0, -50.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Section;
        let out = apply_section(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["letter"], "A"); // 空文档 → 自动 A
        assert_eq!(j["side"], "R"); // 默认右侧
        assert_eq!(j["show_arrow"], true);

        // 左侧 + 隐藏箭头 + 指定字母 C。
        let mut p2 = GuideParams::linear(LinearSub::Aligned, 0.0);
        p2.guide_type = GuideType::Section;
        p2.section_side = SS::Left;
        p2.show_arrow = false;
        p2.letter = Some("C".into());
        let out2 = apply_section(&sender, &doc, &pts, &p2).unwrap();
        let j2: serde_json::Value = serde_json::from_str(&out2).unwrap();
        assert_eq!(j2["letter"], "C");
        assert_eq!(j2["side"], "L");
        assert_eq!(j2["show_arrow"], false);
    }

    #[test]
    fn apply_section_letter_auto_increments() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        // 图纸已有一个单字母 MTEXT 'A' → 剖切自动编号 B。
        let mut doc = acadrust::CadDocument::new();
        let mut m = acadrust::entities::MText::new();
        m.value = "A".into();
        m.insertion_point = acadrust::types::Vector3::new(1.0, 1.0, 0.0);
        let _ = doc.add_entity(acadrust::EntityType::MText(m));
        inner.doc.lock().unwrap().clone_from(&doc);
        let doc2 = inner.doc.lock().unwrap().clone();
        let pts = vec![[5.0, 5.0, 0.0], [15.0, 5.0, 0.0]]; // 水平剖切路径
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Section;
        let out = apply_section(&sender, &doc2, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["letter"], "B");
    }

    #[test]
    fn apply_section_rejects_single_point() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Section;
        assert!(apply_section(&sender, &doc, &[[0.0, 0.0, 0.0]], &p).is_err());
    }

    #[test]
    fn apply_tolerance_builds_block_with_cells() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        // 3 顶点引导线（箭头尖→折点→框接入点），镜像示例 形位-gulide。
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 30.0, 0.0], [5.0, 30.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_sym = Some("j".into());
        p.gdt_tol = Some("0.03".into());
        p.gdt_dia = false;
        p.gdt_d1 = Some("A".into());
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["cells"], 3); // 符号 + 公差 + 基准
        assert!(out.contains("*D"));
    }

    #[test]
    fn apply_tolerance_rejects_too_few_points() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_sym = Some("b".into());
        assert!(apply_tolerance(&sender, &doc, &[[0.0, 0.0, 0.0]], &p).is_err());
        // 至少 2 点但无任何内容（sym/tol/datum 全空）→ 报错（FCF 不能空）。
        let pts2 = vec![[0.0, 0.0, 0.0], [0.0, 10.0, 0.0]];
        let mut p_empty = GuideParams::linear(LinearSub::Aligned, 0.0);
        p_empty.guide_type = GuideType::Tolerance;
        assert!(apply_tolerance(&sender, &doc, &pts2, &p_empty).is_err());
    }

    #[test]
    fn apply_tolerance_dia_and_three_datums() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pts = vec![[0.0, 0.0, 0.0], [10.0, 10.0, 0.0], [20.0, 10.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_sym = Some("j".into());
        p.gdt_tol = Some("0.05".into());
        p.gdt_dia = true;
        p.gdt_d1 = Some("A".into());
        p.gdt_d2 = Some("B".into());
        p.gdt_d3 = Some("C".into());
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["cells"], 5); // 符号 + ⌀公差 + 3 基准 = 5
    }

    #[test]
    fn build_dimension_dec_overrides_dimdec_xdata() {
        use acadrust::xdata::XDataValue as V;
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams { guide_type: GuideType::Linear, sub: Some(LinearSub::Aligned), dist: 10.0, text: None, tol: None, up: None, dn: None,
            fit: None, sym: None, dec: Some(2), ver: crate::guide_url::DatumVersion::GB2008, letter: None, scale: None, flip: crate::guide_url::FlipDir::None, marker: None, angle_mode: crate::guide_url::AngleMode::Minor, section_side: crate::guide_url::SectionSide::Right, show_arrow: true, gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None };
        let dim = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [40.0, 30.0, 0.0], &pd, "OCSM_GB").unwrap();
        let rec = dim.base().common.extended_data.get_record("ACAD").expect("ACAD XDATA 存在");
        let mut found = None;
        for w in rec.values.windows(2) {
            if matches!(w[0], V::Integer16(271)) {
                if let V::Integer16(v) = w[1] {
                    found = Some(v);
                }
            }
        }
        assert_eq!(found, Some(2), "DIMDEC 应被覆盖为 2");
    }

    #[test]
    fn build_dimension_supports_radial_types() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams { guide_type: GuideType::Diameter, sub: None, dist: -9.0, text: None, tol: None, up: None, dn: None,
            fit: None, sym: None, dec: None, ver: crate::guide_url::DatumVersion::GB2008, letter: None, scale: None, flip: crate::guide_url::FlipDir::None, marker: None, angle_mode: crate::guide_url::AngleMode::Minor, section_side: crate::guide_url::SectionSide::Right, show_arrow: true, gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None };
        // 直径走匿名块路径：mock 的 AddBlockRecord 返回 Ok → 成功。
        assert!(build_dimension(&sender, &doc, [0.0,0.0,0.0], [35.36,35.36,0.0], &pd, "OCSM_GB").is_ok());
        let pr = GuideParams { guide_type: GuideType::Radius, sub: None, dist: 11.0, text: None, tol: None, up: None, dn: None,
            fit: None, sym: None, dec: None, ver: crate::guide_url::DatumVersion::GB2008, letter: None, scale: None, flip: crate::guide_url::FlipDir::None, marker: None, angle_mode: crate::guide_url::AngleMode::Minor, section_side: crate::guide_url::SectionSide::Right, show_arrow: true, gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None };
        assert!(build_dimension(&sender, &doc, [0.0,0.0,0.0], [35.36,35.36,0.0], &pr, "OCSM_GB").is_ok());
    }

    #[test]
    fn build_dimension_applies_custom_text() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams {
            guide_type: GuideType::Diameter,
            sub: None,
            dist: -9.0,
            text: Some("%%c<>".into()),
            tol: None,
            up: None,
            dn: None,
            fit: None,
            sym: None,
            dec: None,
            ver: crate::guide_url::DatumVersion::GB2008,
            letter: None,
            scale: None,
            flip: crate::guide_url::FlipDir::None,
            marker: None,
        angle_mode: crate::guide_url::AngleMode::Minor,
            section_side: crate::guide_url::SectionSide::Right,
            show_arrow: true,

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None,
        };
        // "%%c<>" 是默认（GUI 注入一次）→ 不设 user_text（块 TEXT 已用自动值）。
        let dim = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [35.36, 35.36, 0.0], &pd, "OCSM_GB").unwrap();
        assert!(dim.base().user_text.is_none());
        // 其它自定义文字正常设置（块 TEXT 与 user_text 同步）。
        let pd_custom = GuideParams { text: Some("50".into()), ..pd.clone() };
        let dim_c = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [35.36, 35.36, 0.0], &pd_custom, "OCSM_GB").unwrap();
        assert_eq!(dim_c.base().user_text.as_deref(), Some("50"));
        // 无自定义文字时不设 user_text。
        let pd2 = GuideParams { text: None, ..pd };
        let dim2 = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [35.36, 35.36, 0.0], &pd2, "OCSM_GB").unwrap();
        assert!(dim2.base().user_text.is_none());
    }

    #[test]
    fn build_dimension_injects_linear_tolerance() {
        // 线性标注 + 上/下偏差 → DIMENSION text 字段注入公差堆叠
        // （对照公差.dxf 29C：`{\A1;<>{}{\H0.71x;\C2;\S+0.024^  0;}}`）。
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams {
            guide_type: GuideType::Linear,
            sub: Some(LinearSub::Horizontal),
            dist: 25.0,
            text: None,
            tol: None,
            up: Some("+0.035".into()),
            dn: Some("0".into()),
            fit: None,
            sym: None,
            dec: None,
            ver: crate::guide_url::DatumVersion::GB2008,
            letter: None,
            scale: None,
            flip: crate::guide_url::FlipDir::None,
            marker: None,
        angle_mode: crate::guide_url::AngleMode::Minor,
            section_side: crate::guide_url::SectionSide::Right,
            show_arrow: true,

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None,
        };
        let dim = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [100.0, 0.0, 0.0], &pd, "OCSM_GB").unwrap();
        assert_eq!(
            dim.base().text,
            "{\\A1;<>{}{\\H0.71x;\\C2;\\S+0.035^0;}}"
        );
        // 无公差时不注入。
        let pd2 = GuideParams { up: None, dn: None,
            fit: None, ..pd };
        let dim2 = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [100.0, 0.0, 0.0], &pd2, "OCSM_GB").unwrap();
        assert!(dim2.base().text.is_empty());
    }

    #[test]
    fn text_template_pipelines() {
        // ||| = 公差位置
        let body = render_text_template("%%c<>|||,通", 100.0, Some("\\C3;\\SH7/g6;"));
        assert_eq!(body, "%%c<>{}{\\C3;\\SH7/g6;},通");
        // 无 ||| → 公差追加末尾（保留 {} 空组）
        let body2 = render_text_template("<>", 100.0, Some("\\C3;\\SH7/g6;"));
        assert_eq!(body2, "<>{}{\\C3;\\SH7/g6;}");
        // 无公差 → 删 |||
        let body3 = render_text_template("%%c<>|||,通", 100.0, None);
        assert_eq!(body3, "%%c<>,通");
        // [@] 英制换算：25.4 → [1"]
        let body4 = render_text_template("<>[@]", 25.4, None);
        assert_eq!(body4, "<>[1\"]");
        // \X 换行 → \P
        let body5 = render_text_template("<>\\X公差行", 100.0, None);
        assert_eq!(body5, "<>\\P公差行");
    }

    #[test]
    fn build_dimension_text_template_with_tolerance() {
        // 线性 + `%%c<>|||,通` + fit → dimtext 公差插在 ||| 处（Ø100 后），通 在后。
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams {
            guide_type: GuideType::Linear,
            sub: Some(LinearSub::Horizontal),
            dist: 25.0,
            text: Some("%%c<>|||,通".into()),
            tol: None,
            up: None,
            dn: None,
            fit: Some("H7/g6".into()),
            sym: None,
            dec: None,
            ver: crate::guide_url::DatumVersion::GB2008,
            letter: None,
            scale: None,
            flip: crate::guide_url::FlipDir::None,
            marker: None,
        angle_mode: crate::guide_url::AngleMode::Minor,
            section_side: crate::guide_url::SectionSide::Right,
            show_arrow: true,

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None,
        };
        let dim = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [25.0, 0.0, 0.0], &pd, "OCSM_GB").unwrap();
        assert_eq!(
            dim.base().text,
            "{\\A1;%%c<>{}{\\C3;\\SH7/g6;},通}"
        );
    }

    #[test]
    fn build_dimension_keeps_text_override_with_tolerance() {
        // 用户输入 `%%c<>,通`（直径符号+测量占位+中文）+ 公差 → dimtext 必须保留
        // %%c 与中文，且公差堆叠在其后（不再硬编码 <> 丢弃文字）。
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams {
            guide_type: GuideType::Linear,
            sub: Some(LinearSub::Horizontal),
            dist: 25.0,
            text: Some("%%c<>,通".into()),
            tol: None,
            up: None,
            dn: None,
            fit: Some("H7/g6".into()),
            sym: None,
            dec: None,
            ver: crate::guide_url::DatumVersion::GB2008,
            letter: None,
            scale: None,
            flip: crate::guide_url::FlipDir::None,
            marker: None,
        angle_mode: crate::guide_url::AngleMode::Minor,
            section_side: crate::guide_url::SectionSide::Right,
            show_arrow: true,

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None,
        };
        let dim = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [25.0, 0.0, 0.0], &pd, "OCSM_GB").unwrap();
        let t = dim.base().text.clone();
        assert!(t.contains("%%c<>,通"), "应保留 %%c 与中文, got {t}");
        assert!(t.contains("\\C3;\\SH7/g6;"), "应含公差堆叠, got {t}");
        assert_eq!(t, "{\\A1;%%c<>,通{}{\\C3;\\SH7/g6;}}");
        // 无公差 + 自定义文字：dimtext 直接应用文字。
        let pd2 = GuideParams { fit: None, ..pd };
        let dim2 = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [25.0, 0.0, 0.0], &pd2, "OCSM_GB").unwrap();
        assert_eq!(dim2.base().text, "%%c<>,通");
    }

    #[test]
    fn build_dimension_injects_fit_tolerance() {
        // ISO 286 配合代号：25mm 线性标注 + fit=H7/g6 → 测量值 25 → 孔偏差 +0.021/0。
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams {
            guide_type: GuideType::Linear,
            sub: Some(LinearSub::Horizontal),
            dist: 25.0,
            text: None,
            tol: None,
            up: None,
            dn: None,
            fit: Some("H7/g6".into()),
            sym: None,
            dec: None,
            ver: crate::guide_url::DatumVersion::GB2008,
            letter: None,
            scale: None,
            flip: crate::guide_url::FlipDir::None,
            marker: None,
        angle_mode: crate::guide_url::AngleMode::Minor,
            section_side: crate::guide_url::SectionSide::Right,
            show_arrow: true,

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None,
        };
        // fit 代号模式 → 配合代号堆叠（对照示例 `{\C3;\SH7/g6;}`：绿色、斜杠分数）。
        let dim = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [25.0, 0.0, 0.0], &pd, "OCSM_GB").unwrap();
        assert_eq!(
            dim.base().text,
            "{\\A1;<>{}{\\C3;\\SH7/g6;}}"
        );
        // 单轴代号 g6 → 水平（不堆叠）
        let pd2 = GuideParams { fit: Some("g6".into()), ..pd };
        let dim2 = build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [25.0, 0.0, 0.0], &pd2, "OCSM_GB").unwrap();
        assert_eq!(
            dim2.base().text,
            "{\\A1;<>{}{\\C3;g6}}"
        );
    }

    #[test]
    fn build_dimension_rejects_unimplemented_types() {
        let p = GuideParams {
            guide_type: GuideType::Datum,
            sub: None,
            dist: 5.0,
            text: None,
            tol: None,
            up: None,
            dn: None,
            fit: None,
            sym: None,
            dec: None,
            ver: crate::guide_url::DatumVersion::GB2008,
            letter: None,
            scale: None,
            flip: crate::guide_url::FlipDir::None,
            marker: None,
        angle_mode: crate::guide_url::AngleMode::Minor,
            section_side: crate::guide_url::SectionSide::Right,
            show_arrow: true,

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None,
        };
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        assert!(build_dimension(&sender, &doc, [0.0, 0.0, 0.0], [10.0, 0.0, 0.0], &p, "OCSM_GB").is_err());
    }

    #[test]
    fn guide_html_is_embedded_and_has_key_controls() {
        assert!(GUI_HTML.contains("应用并刷新"));
        assert!(GUI_HTML.contains("/api/apply_refresh"));
        assert!(GUI_HTML.contains("LINEAR"));
    }
}

// ── 测试用宿主代理（integration/tests 共用） ───────────────────────────────

/// 测试用宿主代理：内存文档 + 记录 URL 写入。验证 HTTP server 全链路。
use ocs_plugin_api::host::PluginRequestError;

struct MockSender {
    doc: std::sync::Mutex<acadrust::CadDocument>,
    url_writes: std::sync::Mutex<Vec<(acadrust::Handle, String)>>,
}
impl MockSender {
    fn new(doc: acadrust::CadDocument) -> Self {
        MockSender {
            doc: std::sync::Mutex::new(doc),
            url_writes: std::sync::Mutex::new(Vec::new()),
        }
    }
}
impl PluginRequestSender for MockSender {
    fn request(
        &self,
        req: PluginRequest,
    ) -> Result<PluginResponse, PluginRequestError> {
        use PluginRequest as R;
        use PluginResponse as P;
        match req {
            R::DocumentSnapshot => {
                Ok(P::Document(Box::new(self.doc.lock().unwrap().clone())))
            }
            R::ReadRecord { .. } => Ok(P::Record(None)),
            R::WriteRecord { handle, record } => {
                let url = record
                    .values
                    .iter()
                    .find_map(|v| match v {
                        XDataValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                self.url_writes.lock().unwrap().push((handle, url));
                Ok(P::Ok)
            }
            R::AddEntities(es) => {
                let mut hs = Vec::new();
                let mut doc = self.doc.lock().unwrap();
                for e in es {
                    if let Ok(h) = doc.add_entity(e) {
                        hs.push(h);
                    }
                }
                Ok(P::Handles(hs))
            }
            R::RemoveEntity { handle } => {
                self.doc.lock().unwrap().remove_entity(handle);
                Ok(P::Ok)
            }
            _ => Ok(P::Ok),
        }
    }
}


// ── 集成测试：mock PluginRequestSender + 真实 HTTP server ──────────────────

#[cfg(test)]
mod integration {
    use super::*;
    use ocs_plugin_api::host::acadrust::entities::Line;
    use ocs_plugin_api::host::acadrust::EntityType;
    use ocs_plugin_api::host::{PluginRequestError, PluginRequestSender};


    fn http_req(port: u16, method: &str, path: &str, body: &str) -> String {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
        s.flush().unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }

    #[test]
    fn http_api_tolerance_end_to_end() {
        // 25H7/g6：孔偏差 +0.021/0，间隙配合。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let server = spawn(mock.clone()).expect("spawn guide server");
        let r = http_req(server.port, "GET", "/api/tolerance?dim=25&fit=H7%2Fg6", "");
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["up"], "+0.021");
        assert_eq!(v["dn"], "0");
        assert_eq!(v["kind"], "间隙配合");
        assert!((v["hole"]["upper"].as_f64().unwrap() - 0.021).abs() < 1e-9);
        assert!((v["shaft"]["lower"].as_f64().unwrap() + 0.02).abs() < 1e-9);
        // 单轴 g6
        let r2 = http_req(server.port, "GET", "/api/tolerance?dim=25&fit=g6", "");
        let v2: serde_json::Value = serde_json::from_str(&r2).unwrap();
        assert_eq!(v2["up"], "-0.007");
        assert_eq!(v2["dn"], "-0.020");
        // 非法代号
        let r3 = http_req(server.port, "GET", "/api/tolerance?dim=25&fit=XX7", "");
        let v3: serde_json::Value = serde_json::from_str(&r3).unwrap();
        assert_eq!(v3["ok"], false);
    }

    #[test]
    fn http_server_end_to_end_apply_and_refresh() {
        let mut doc = acadrust::CadDocument::new();
        let mut line = Line {
            common: Default::default(),
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(40.0, 30.0, 0.0),
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        line.common.layer = "10引导线层".into();
        let gh = doc.add_entity(EntityType::Line(line)).unwrap();
        let mock = std::sync::Arc::new(MockSender::new(doc));
        let server = spawn(mock.clone()).expect("spawn guide server");
        let port = server.port;
        let hx = format!("{:#X}", u64::from(gh));

        // 1. GET /api/guide → 几何 + 无 URL。
        let g = http_req(port, "GET", &format!("/api/guide?handle={hx}"), "");
        let gv: serde_json::Value = serde_json::from_str(&g).unwrap();
        assert_eq!(gv["ok"], true);
        assert!((gv["p1"][0].as_f64().unwrap() - 0.0).abs() < 1e-9);
        assert!((gv["p2"][1].as_f64().unwrap() - 30.0).abs() < 1e-9);
        assert!(gv["url"].is_null());

        // 2. POST /api/apply → 写 URL。
        let url = "http://127.0.0.1:23751/DIM/LINEAR/A/10";
        let body = serde_json::json!({"handle": hx, "url": url}).to_string();
        let r = http_req(port, "POST", "/api/apply", &body);
        let rv: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(rv["ok"], true);
        assert!(mock
            .url_writes
            .lock()
            .unwrap()
            .iter()
            .any(|(h, u)| *h == gh && u == url));

        // 3. POST /api/apply_refresh → 生成标注 + 删引导线。
        let body2 = serde_json::json!({"handle": hx, "url": url}).to_string();
        let r2 = http_req(port, "POST", "/api/apply_refresh", &body2);
        let rv2: serde_json::Value = serde_json::from_str(&r2).unwrap();
        assert_eq!(rv2["ok"], true, "apply_refresh 应成功: {rv2}");
        assert!(rv2["dimension_handle"].is_string());

        let doc = mock.doc.lock().unwrap();
        assert!(
            doc.entities().any(|e| matches!(e, EntityType::Dimension(_))),
            "文档中应出现标注实体"
        );
        assert!(
            !doc.entities().any(|e| matches!(e, EntityType::Line(_))),
            "引导线应被删除"
        );
    }

    #[test]
    fn http_server_view_apply_refresh_creates_two_inserts() {
        // 向视图：引导线 (0,0)→(10,0)（字母端→箭头端），全参数（字母 B + 比例 1:1
        // + 翻转方向 + 标记放置点 (12,34)）。箭头块 INSERT 在箭头端(10,0)、
        // 字母符号块 INSERT 在标记放置点。
        let mut doc = acadrust::CadDocument::new();
        let mut line = Line {
            common: Default::default(),
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(10.0, 0.0, 0.0),
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        line.common.layer = "10引导线层".into();
        let gh = doc.add_entity(EntityType::Line(line)).unwrap();
        let mock = std::sync::Arc::new(MockSender::new(doc));
        let server = spawn(mock.clone()).expect("spawn");
        let port = server.port;
        let hx = format!("{:#X}", u64::from(gh));

        let url = "http://127.0.0.1:23751/DIM/VIEW/0?let=B&scale=1%3A1&flip=1&mx=12&my=34";
        let body = serde_json::json!({"handle": hx, "url": url}).to_string();
        let r = http_req(port, "POST", "/api/apply_refresh", &body);
        let rv: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(rv["ok"], true, "VIEW apply_refresh 应成功: {rv}");
        assert!(rv["arrow_insert_handle"].is_string());
        assert!(rv["marker_insert_handle"].is_string());
        assert_eq!(rv["letter"], "B");
        assert_eq!(rv["scale"], "1:1");
        assert_eq!(rv["flip"], "ccw");

        // 文档中应出现 2 个 INSERT（箭头块 + 字母符号块），引导线已删。
        // 视图编号字母已并入箭头块（*V{a} 内 SOLID+LINE+MTEXT），不再独立存在。
        let doc = mock.doc.lock().unwrap();
        let n_ins = doc
            .entities()
            .filter(|e| matches!(e, EntityType::Insert(_)))
            .count();
        assert_eq!(n_ins, 2, "应生成 2 个 INSERT");
        let n_mtext = doc
            .entities()
            .filter(|e| matches!(e, EntityType::MText(_)))
            .count();
        assert_eq!(n_mtext, 0, "字母已并入箭头块，modelspace 不应再有独立 MTEXT");
        assert!(
            !doc.entities().any(|e| matches!(e, EntityType::Line(_))),
            "引导线应被删除"
        );
    }

    #[test]
    fn http_server_serves_gui_html_with_query() {
        // 浏览器请求 /guide.html?handle=0x3AE（带 query），必须返回 200 HTML。
        let mut doc = acadrust::CadDocument::new();
        let mock = std::sync::Arc::new(MockSender::new(doc));
        let server = spawn(mock.clone()).expect("spawn");
        let port = server.port;
        let r = http_req(port, "GET", "/guide.html?handle=0x3AE", "");
        assert!(r.contains("标注配置"), "应返回 GUI HTML: {}", &r[..r.len().min(200)]);
        assert!(r.contains("应用并刷新"));
    }

    #[test]
    fn http_server_mcp_bridge_list_guides() {
        let mut doc = acadrust::CadDocument::new();
        let mut line = Line {
            common: Default::default(),
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(40.0, 30.0, 0.0),
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        line.common.layer = "10引导线层".into();
        doc.add_entity(EntityType::Line(line)).unwrap();
        let mock = std::sync::Arc::new(MockSender::new(doc));
        let server = spawn(mock.clone()).expect("spawn");
        let port = server.port;

        // MCP 桥接：list_guides（无 /DIM/ 超链接的引导线不出现 → 空）。
        let body = serde_json::json!({"method": "list_guides", "params": {}}).to_string();
        let r = http_req(port, "POST", "/api/mcp", &body);
        let rv: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(rv["guides"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn http_server_end_to_end_diameter_refresh() {
        // 直径标注全链路：引导线(圆心→圆周) → apply_refresh → 生成 Diameter 标注。
        let mut doc = acadrust::CadDocument::new();
        let mut line = Line {
            common: Default::default(),
            start: Vector3::new(0.0, 0.0, 0.0),
            end: Vector3::new(35.35533905932738, 35.35533905932737, 0.0),
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        line.common.layer = "10引导线层".into();
        let gh = doc.add_entity(EntityType::Line(line)).unwrap();
        let mock = std::sync::Arc::new(MockSender::new(doc));
        let server = spawn(mock.clone()).expect("spawn");
        let port = server.port;
        let hx = format!("{:#X}", u64::from(gh));

        let url = "http://127.0.0.1:23751/DIM/DIAMETER/-9";
        let body = serde_json::json!({"handle": hx, "url": url}).to_string();
        let r = http_req(port, "POST", "/api/apply_refresh", &body);
        let rv: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(rv["ok"], true, "apply_refresh 应成功: {rv}");

        let d = mock.doc.lock().unwrap();
        let mut found_diameter = false;
        let mut found_line = false;
        for e in d.entities() {
            match e {
                EntityType::Dimension(acadrust::entities::Dimension::Diameter(dd)) => {
                    found_diameter = true;
                    assert!((dd.base.actual_measurement - 100.0).abs() < 1e-6);
                    assert_eq!(dd.base.common.layer, "7标注层");
                    assert_eq!(dd.base.style_name, "OCSM_GB");
                    // 文字锚点：径向 41 + 法向 1.0。
                    let tm = dd.base.text_middle_point;
                    let radial = (tm.x + tm.y) / (2.0f64).sqrt();
                    assert!((radial - 41.0).abs() < 1e-6, "径向 {radial}");
                }
                EntityType::Line(_) => found_line = true,
                _ => {}
            }
        }
        assert!(found_diameter, "应生成 Diameter 标注");
        assert!(!found_line, "引导线应被删除");
    }

    #[test]
    fn ensure_style_plain_without_frame() {
        // 无图幅块 → scale=1 → OCSM_GB，不创建缩放样式。
        let doc = acadrust::CadDocument::new();
        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let name = ensure_style_for_point(&sender, &inner.doc.lock().unwrap().clone(), [0.0, 0.0, 0.0])
            .expect("ensure style");
        assert_eq!(name, "OCSM_GB");
    }

    #[test]
    fn build_guide_angular_45deg_three_modes() {
        // 两段 PLINE：p0=(50,0) → p1=(0,0) 顶点 → p2=(50,50)。
        // 两方向 (1,0) 与 (0.707,0.707)：劣角 45°。
        let p0 = [50.0, 0.0, 0.0];
        let p1 = [0.0, 0.0, 0.0];
        let p2 = [50.0, 50.0, 0.0];
        let dist = 20.0;

        // 劣角（默认）
        let dim = build_guide_angular(p0, p1, p2, dist, crate::guide_url::AngleMode::Minor, "OCSM_GB").unwrap();
        let d = match &dim {
            Dimension::Angular2Ln(d) => d,
            other => panic!("应为 Angular2Ln，得到 {other:?}"),
        };
        assert!((d.base.actual_measurement - 45.0).abs() < 1e-6, "劣角 45");
        assert_eq!(d.base.user_text.as_deref(), Some("45°"));
        assert_eq!(d.base.common.layer, "7标注层");
        assert_eq!(d.base.style_name, "OCSM_GB");
        // 顶点/两边：first=p1, second=p2, angle_vertex=p1, definition=p0。
        assert!((d.first_point.x).abs() < 1e-9 && (d.first_point.y).abs() < 1e-9);
        assert!((d.second_point.x - 50.0).abs() < 1e-6 && (d.second_point.y - 50.0).abs() < 1e-6);
        assert!((d.angle_vertex.x).abs() < 1e-9 && (d.angle_vertex.y).abs() < 1e-9);
        assert!((d.definition_point.x - 50.0).abs() < 1e-6);
        // 放置点 = 顶点 + 角平分线(22.5°)×dist。
        let tm = d.base.text_middle_point;
        assert!((tm.x - 20.0 * 22.5f64.to_radians().cos()).abs() < 1e-6, "x {}", tm.x);
        assert!((tm.y - 20.0 * 22.5f64.to_radians().sin()).abs() < 1e-6, "y {}", tm.y);

        // 优角 315°。
        let refl = build_guide_angular(p0, p1, p2, dist, crate::guide_url::AngleMode::Reflex, "OCSM_GB").unwrap();
        let rd = match &refl { Dimension::Angular2Ln(d) => d, _ => panic!() };
        assert!((rd.base.actual_measurement - 315.0).abs() < 1e-6, "优角 315");
        assert_eq!(rd.base.user_text.as_deref(), Some("315°"));
    }

    #[test]
    fn angle_url_roundtrip_and_defaults() {
        use crate::guide_url::AngleMode;
        // 缺省角模式段 → 劣角。
        let p = crate::guide_url::GuideParams::from_url("http://127.0.0.1:23751/DIM/ANGLE/45").unwrap();
        assert_eq!(p.guide_type, crate::guide_url::GuideType::Angle);
        assert_eq!(p.angle_mode, AngleMode::Minor);
        assert!((p.dist - 45.0).abs() < 1e-9);
        // 显式优角。
        let r = crate::guide_url::GuideParams::from_url("http://127.0.0.1:23751/DIM/ANGLE/R/60").unwrap();
        assert_eq!(r.angle_mode, AngleMode::Reflex);
        // to_url 往返：劣角不输出模式段，优角输出 R。
        let mut cp = crate::guide_url::GuideParams::linear(crate::guide_url::LinearSub::Aligned, 10.0);
        cp.guide_type = crate::guide_url::GuideType::Angle;
        cp.dist = 60.0;
        let u = cp.to_url(23751);
        assert_eq!(u, "http://127.0.0.1:23751/DIM/ANGLE/60", "劣角不输出: {u}");
        cp.angle_mode = AngleMode::Reflex;
        let u2 = cp.to_url(23751);
        assert_eq!(u2, "http://127.0.0.1:23751/DIM/ANGLE/R/60", "优角: {u2}");
    }

    #[test]
    fn guide_geom_points_reads_pline() {
        use acadrust::entities::LwPolyline;
        let mut pl = LwPolyline::new();
        pl.vertices.push(acadrust::entities::LwVertex::new(acadrust::types::Vector2::new(50.0, 0.0)));
        pl.vertices.push(acadrust::entities::LwVertex::new(acadrust::types::Vector2::new(0.0, 0.0)));
        pl.vertices.push(acadrust::entities::LwVertex::new(acadrust::types::Vector2::new(50.0, 50.0)));
        let mut doc = acadrust::CadDocument::new();
        let h = doc.add_entity(acadrust::EntityType::LwPolyline(pl)).unwrap();
        let pts = guide_geom_points(&doc, h).unwrap();
        assert_eq!(pts.len(), 3);
        assert!((pts[0][0] - 50.0).abs() < 1e-9);
        assert!((pts[1][0]).abs() < 1e-9);
        assert!((pts[2][1] - 50.0).abs() < 1e-9);
        // LINE 仍返回 2 点（向后兼容）。
        let mut line = acadrust::entities::Line::new();
        line.start = v3([0.0, 0.0, 0.0]);
        line.end = v3([10.0, 5.0, 0.0]);
        let h2 = doc.add_entity(acadrust::EntityType::Line(line)).unwrap();
        let pts2 = guide_geom_points(&doc, h2).unwrap();
        assert_eq!(pts2.len(), 2);
    }
}
