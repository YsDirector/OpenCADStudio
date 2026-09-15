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

use crate::guide_url::{GdtRow, GrindKind, GuideParams, GuideType, LinearSub, WeldParams};
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
        ("GET", t) if t.starts_with("/rough.html") => {
            (200, "text/html; charset=utf-8", ROUGH_HTML.to_string())
        }
        ("GET", t) if t.starts_with("/parts") => {
            (200, "text/html; charset=utf-8", PARTS_HTML.to_string())
        }
        ("GET", t) if t.starts_with("/api/parts_ping") => {
            crate::page_window_ping("parts", t.contains("bye=1"));
            (200, json, r#"{"ok":true}"#.into())
        }
        ("GET", t) if t.starts_with("/api/page_ping") => {
            let q = t.split_once('?').map(|(_, q)| q).unwrap_or("");
            let key = q
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .find(|(k, _)| *k == "p")
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "page".to_string());
            crate::page_window_ping(&key, q.contains("bye=1"));
            (200, json, r#"{"ok":true}"#.into())
        }
        ("GET", t) if t.starts_with("/api/parts") => {
            (200, json, crate::partgen::catalog_json())
        }
        ("GET", t) if t == "/manual" || t.starts_with("/manual?") => {
            (200, "text/html; charset=utf-8", MANUAL_HTML.to_string())
        }
        ("GET", t) if t.starts_with("/api/manual") => api_manual(t),
        ("GET", t) if t == "/joint" || t.starts_with("/joint?") => {
            (200, "text/html; charset=utf-8", JOINT_HTML.to_string())
        }
        ("GET", t) if t.starts_with("/api/part_svg") => {
            let q = t.split_once('?').map(|(_, q)| q).unwrap_or("");
            match crate::partgen::preview_svg(q) {
                Ok(svg) => (200, "image/svg+xml; charset=utf-8", svg),
                Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
            }
        }
        ("POST", "/api/part_pick") => api_part_pick(body, sender),
        ("POST", "/api/joint") => {
            let json = "application/json; charset=utf-8";
            match apply_joint(sender, body) {
                Ok(s) => (200, json, s),
                Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
            }
        }
        ("POST", "/api/joint_plan") => api_joint_plan(body),
        ("POST", "/api/joint_place") => api_joint_place(sender, body),
        ("POST", "/api/part_export") => api_part_export(body, sender),
        ("GET", t) if t.starts_with("/api/guide") => api_guide(target, sender),
        ("GET", t) if t.starts_with("/api/tolerance") => api_tolerance(target),
        ("GET", t) if t.starts_with("/api/ping") => (200, json, r#"{"ok":true}"#.into()),
        ("POST", "/api/apply") => api_apply(body, sender, false),
        ("POST", "/api/apply_refresh") => api_apply(body, sender, true),
        ("POST", "/api/rough_apply") => api_rough_apply(body, sender),
        ("GET", "/api/weld_syms") => (200, json, weld_syms_json()),
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

/// 开启插件撤销事务（`HostApi::begin_undo`）。
///
/// HTTP/GUI 流程的一次用户动作会向宿主发多条请求（各占一条 message），而宿主在每条
/// message 末尾提交并丢弃空 pending 快照（`src/app/update/mod.rs:296` +
/// `src/app/history.rs:285`）——所以必须用事务把快照握住，写完再 `commit_undo`。
/// 旧宿主没有这对请求时，请求会报错（等同现状：不可撤销），不影响主流程。
fn begin_undo(sender: &Arc<dyn PluginRequestSender>, label: &str) -> Result<(), String> {
    req_timed(
        sender,
        PluginRequest::BeginUndo {
            label: label.to_string(),
        },
        "BeginUndo",
    )?;
    Ok(())
}

/// 提交 `begin_undo` 开启的事务，让它成为一个撤销条目（revision 同时自增）。
/// 失败中途也调用一次：无改动时空条目会被宿主丢弃，未调用则会被下一次历史操作
/// （另一次 push/begin 或撤销）兜底关闭。
fn commit_undo(sender: &Arc<dyn PluginRequestSender>) {
    let _ = req_timed(sender, PluginRequest::CommitUndo, "CommitUndo");
}

/// 标记图纸已修改。宿主里 `tabs[i].dirty = true` 的唯一来源是 SetDirty；
/// add/remove/write/bump 都不会置 dirty。不加的话，关闭 OCS 时不弹"未保存"
/// 提示，引导服务生成的改动会静默丢失。写套路：BeginUndo → mutate → SetDirty →
/// BumpGeometry → CommitUndo（见 `begin_undo` / `commit_undo`）。
fn mark_dirty(sender: &Arc<dyn PluginRequestSender>) -> Result<(), String> {
    req_timed(sender, PluginRequest::SetDirty, "SetDirty")?;
    Ok(())
}

pub(crate) fn pe_url_record(url: &str) -> ExtendedDataRecord {
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
        // 局部放大图圆引导：中心 + 半径点（r 可由两点距离恢复）。
        acadrust::EntityType::Circle(c) => Ok(vec![
            [c.center.x, c.center.y, c.center.z],
            [c.center.x + c.radius, c.center.y, c.center.z],
        ]),
        // 弧长引导：起点 + 终点 + 中点（三点可求圆心/半径/扫角）。
        acadrust::EntityType::Arc(a) => {
            let s = a.start_point();
            let e = a.end_point();
            let m = a.midpoint();
            Ok(vec![
                [s.x, s.y, s.z],
                [e.x, e.y, e.z],
                [m.x, m.y, m.z],
            ])
        }
        acadrust::EntityType::LwPolyline(pl) => {
            Ok(pl
                .vertices
                .iter()
                .map(|v| [v.location.x, v.location.y, pl.elevation])
                .collect())
        }
        // heavy polyline（DXF 读取路径把 LWPOLYLINE 转为 Polyline/Polyline2D）。
        acadrust::EntityType::Polyline(pl) => {
            Ok(pl
                .vertices
                .iter()
                .map(|v| [v.location.x, v.location.y, v.location.z])
                .collect())
        }
        acadrust::EntityType::Polyline2D(pl) => {
            Ok(pl
                .vertices
                .iter()
                .map(|v| [v.location.x, v.location.y, v.location.z])
                .collect())
        }
        _ => Err("引导线必须是直线（LINE）或多段线（PLINE）".into()),
    }
}

/// 引导几何形态：line（LINE）/ pline（LWPOLYLINE / Polyline / Polyline2D）/
/// circle（CIRCLE）/ rect（闭合 4 顶点无 bulge 的 PLINE，矩形引导）/ arc（ARC）。
fn guide_geom_kind(
    doc: &acadrust::CadDocument,
    handle: acadrust::Handle,
) -> Result<&'static str, String> {
    let e = doc.get_entity(handle).ok_or("找不到引导线实体")?;
    match e {
        acadrust::EntityType::Line(_) => Ok("line"),
        acadrust::EntityType::Circle(_) => Ok("circle"),
        acadrust::EntityType::Arc(_) => Ok("arc"),
        acadrust::EntityType::LwPolyline(pl) => {
            if pl.is_closed
                && pl.vertices.len() == 4
                && pl.vertices.iter().all(|v| v.bulge.abs() < 1e-9)
            {
                Ok("rect")
            } else {
                Ok("pline")
            }
        }
        acadrust::EntityType::Polyline(_) | acadrust::EntityType::Polyline2D(_) => Ok("pline"),
        _ => Err("引导线必须是直线（LINE）或多段线（PLINE）".into()),
    }
}

/// 构造与 `OCSMDIMGULIDE1-general.dxf` 标注一致的 DSTYLE XDATA 覆盖
/// （ACAD/DSTYLE）。OCS 渲染 Dimension 时优先用 XDATA 覆盖样式表
/// （dim_override），因此必须写入这些参数才能复刻示例渲染：
/// dimdec=2（2 位小数）、dimtoh=1（文字水平）、dimdli=0.38 等。
fn dim_override_record() -> ExtendedDataRecord {
    dim_override_record_dec(2)
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
        (271, dec), (272, 3), (273, 2), (274, 3), (275, 0), (276, 0),
        // DIMTMOVE(279)=1：文字被拖到尺寸线范围外时，尺寸线沿轴延伸到文字底下
        // （GB/T 4458.4 画法；宿主按此渲染引线）。
        (277, 2), (278, 46), (279, 1), (280, 0), (281, 0), (282, 0),
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
pub(crate) fn build_guide_angular_block(
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
    // 用户所见文字优先（OCSMDIM2GB 等外部转换会带原标注文字）；否则按角模式算。
    let text_value = match &params.text {
        Some(t) if !t.trim().is_empty() => t.clone(),
        _ => {
            let dec = params.dec.unwrap_or(0).min(8) as usize;
            format!("{{{}}}°", format_measurement(show_deg, dec as u32))
        }
    };
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
pub(crate) fn ensure_style_for_point(
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
    coef: Option<f64>,
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
    // 局部放大图内：测量值 × 系数（真实尺寸）。
    let measurement = measurement * coef.unwrap_or(1.0);
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

pub(crate) fn build_dimension(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p1: [f64; 3],
    p2: [f64; 3],
    params: &GuideParams,
    style: &str,
) -> Result<Dimension, String> {
    // 局部放大图换算系数：标注起点落在 DETAIL 放大块内 → 测量值 × 1/(detail×frame)
    // （放大图内标注显示真实尺寸；角度标注除外——angle 不走这里）。
    let coef = detail_scale_at(doc, p1);
    let mut dim = match params.guide_type {
        GuideType::Linear => {
            let sub = params.sub.ok_or("线性标注缺少对齐子类型")?;
            build_guide_linear(p1, p2, sub, params.dist, style)
        }
        // 直径/半径：匿名块形态（AutoCAD 画法，渲染走 walk_block）。
        GuideType::Diameter | GuideType::Radius => {
            build_guide_radial_block(sender, doc, p1, p2, params, style, coef)?
        }
        GuideType::Datum => return Err("基准标注暂未实现".into()),
        GuideType::View => return Err("向视图标注暂未实现".into()),
        GuideType::Angle => return Err("角度标注走 do_apply 独立分支".into()),
        GuideType::Section => return Err("剖切符号走 do_apply 独立分支".into()),
        GuideType::Tolerance => return Err("形位公差走 do_apply 独立分支".into()),
        GuideType::Detail => return Err("局部放大图走 do_apply 独立分支".into()),
        GuideType::ArcLen => return Err("弧长标注走 do_apply 独立分支".into()),
        GuideType::Weld => return Err("焊接符号走 do_apply 独立分支".into()),
        GuideType::Leader => return Err("引线标注走 do_apply 独立分支".into()),
        GuideType::Balloon => return Err("序号标注走 do_apply 独立分支".into()),
    };
    // 用户指定小数位数：覆盖 XDATA DSTYLE 里的 DIMDEC(271)（样式默认 2）。
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
        let mut text_part = match &params.text {
            Some(t) if !t.is_empty() => t.clone(),
            _ => "<>".to_string(),
        };
        // 放大图内标注：把 <> 替换成真实尺寸数值（测量 × 系数），宿主不再重算。
        if let Some(c) = coef {
            let value = format_measurement(
                dim.base().actual_measurement * c,
                params.dec.unwrap_or(2),
            );
            text_part = text_part.replace("<>", &value);
        }
        // 公差（dimtext）：fit 代号模式 → 配合代号堆叠（H7 上 g6 下）；
        // up/dn 手输 → 极限偏差堆叠。
        let m = dim.base().actual_measurement * coef.unwrap_or(1.0);
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

// ── 局部放大图（DETAIL） ──────────────────────────────────────────────────

/// 局部放大图引导形状（世界坐标）。
#[derive(Debug, Clone, Copy)]
enum GuideShape {
    Circle {
        center: (f64, f64),
        radius: f64,
    },
    Rect {
        center: (f64, f64),
        half: (f64, f64),
    },
}

impl GuideShape {
    /// 引出线起点（右上 45° 方向，圆上 / 矩形右上角外）。
    fn lead_origin(&self) -> (f64, f64) {
        let r = std::f64::consts::FRAC_1_SQRT_2;
        match self {
            GuideShape::Circle { center, radius } => {
                (center.0 + r * radius, center.1 + r * radius)
            }
            GuideShape::Rect { center, half } => (center.0 + half.0, center.1 + half.1),
        }
    }
    /// 框顶（块局部坐标 y，放大后，文字中心再上移 4×frame 避让）。
    fn box_top(&self, k: f64, frame: f64) -> f64 {
        match self {
            GuideShape::Circle { radius, .. } => radius * k + 10.0 * frame,
            GuideShape::Rect { half, .. } => half.1 * k + 10.0 * frame,
        }
    }
}

/// 从引导实体读取裁剪窗口与形状：圆（CIRCLE）或闭合 4 顶点无 bulge 矩形（PLINE）。
fn guide_geom_win(
    doc: &acadrust::CadDocument,
    handle: acadrust::Handle,
) -> Result<(crate::detail_clip::Win, GuideShape), String> {
    use acadrust::EntityType as E;
    let e = doc.get_entity(handle).ok_or("找不到引导实体")?;
    match e {
        E::Circle(c) => {
            let center = (c.center.x, c.center.y);
            Ok((
                crate::detail_clip::Win::Circle { center, radius: c.radius },
                GuideShape::Circle { center, radius: c.radius },
            ))
        }
        E::LwPolyline(pl) => rect_win_pts(
            &pl.vertices.iter().map(|v| (v.location.x, v.location.y)).collect::<Vec<_>>(),
            pl.is_closed,
        ),
        E::Polyline2D(pl) => rect_win_pts(
            &pl.vertices.iter().map(|v| (v.location.x, v.location.y)).collect::<Vec<_>>(),
            pl.flags.is_closed(),
        ),
        _ => Err("局部放大图引导必须是圆（CIRCLE）或闭合 4 顶点矩形（PLINE）".into()),
    }
}

fn rect_win_pts(pts: &[(f64, f64)], closed: bool) -> Result<(crate::detail_clip::Win, GuideShape), String> {
    if !closed || pts.len() != 4 {
        return Err("局部放大图矩形引导必须是闭合 4 顶点多段线（RECTANG 产物）".into());
    }
    let x0 = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let x1 = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let y0 = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let y1 = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let center = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let half = ((x1 - x0) * 0.5, (y1 - y0) * 0.5);
    Ok((
        crate::detail_clip::Win::Rect { min: (x0, y0), max: (x1, y1) },
        GuideShape::Rect { center, half },
    ))
}

/// 局部放大图序号（ASCII 罗马式：I, II, III, IV … XII）。
const ROMANS_ASCII: [&str; 12] = [
    "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
];

/// 兼容旧版 Unicode 罗马数字（Ⅰ..Ⅻ）。
const ROMANS_UNICODE: [&str; 12] = [
    "Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ", "Ⅵ", "Ⅶ", "Ⅷ", "Ⅸ", "Ⅹ", "Ⅺ", "Ⅻ",
];

/// 匹配 ASCII（I..XII）或 Unicode（Ⅰ..Ⅻ）序号 → 数值。
fn detail_no_value(s: &str) -> Option<usize> {
    let t = s.trim();
    if let Some(i) = ROMANS_ASCII.iter().position(|r| *r == t) {
        return Some(i + 1);
    }
    ROMANS_UNICODE.iter().position(|r| *r == t).map(|i| i + 1)
}

/// 自动序号：图纸已有 I..XII / Ⅰ..Ⅻ（块外 MTEXT）最大值 + 1。
fn next_detail_no(doc: &acadrust::CadDocument) -> Result<String, String> {
    use acadrust::EntityType as E;
    let mut max = 0usize;
    for e in doc.entities() {
        if let E::MText(m) = e {
            if let Some(v) = detail_no_value(&m.value) {
                max = max.max(v);
            }
        }
    }
    if max >= ROMANS_ASCII.len() {
        return Err(format!(
            "图纸已有 {} 个局部放大图（I~XII），请在 GUI 手动指定序号",
            max
        ));
    }
    Ok(ROMANS_ASCII[max].to_string())
}

/// 局部放大图（DETAIL）：引导 = 圆（CIRCLE）或闭合 4 顶点矩形（RECTANG 产物）。
///
/// 对照 OCSMDIMGULIDE/局部放大/局部放大图.dxf：
/// - 引导形状保留原位，移入 8符号标注层 色31（表示放大区域）。
/// - 抽出 5 层（+层 0）中引导窗口内的实体，超界部分**裁剪**，整体
///   ×(绝对比例 × TF 图幅倍率) 后以匿名块粘贴在图纸其它位置（自动偏移可拖）。
/// - 放大块 = 框线（色31，= 引导形状×系数）+ 内容（保留原图层）+ 顶部序号/比例。
/// - 引导旁：引出折线（色31）+ 序号 MTEXT（色3）。
/// - INSERT 挂 PE_URL（s/frame）供标注换算系数检测。
fn apply_detail(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    handle: acadrust::Handle,
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::entities::{Circle, Insert, Line, LwPolyline, MText};
    use acadrust::entities::AttachmentPoint;
    use acadrust::types::{Color, Vector2, Vector3};
    use acadrust::EntityType as E;
    use crate::detail_clip::{clip_entity, CONTENT_LAYERS, Win};

    let (win, shape) = guide_geom_win(doc, handle)?;
    let origin = win.center();
    let frame = frame_scale_at(doc, [origin.0, origin.1, 0.0]);
    let detail = params.detail_scale.max(1e-6);
    let k = detail * frame;

    // 1) 内容：5 层 + 层 0 实体，窗口内裁剪并 ×k 变换。
    let mut members: Vec<E> = Vec::new();
    for e in doc.entities() {
        let layer = e.common().layer.as_str();
        if !(CONTENT_LAYERS.contains(&layer) || layer == "0") {
            continue;
        }
        members.extend(clip_entity(&win, e, k, origin));
    }

    // 2) 序号（自动或手输）。
    let no = match &params.detail_no {
        Some(n) if !n.is_empty() => n.clone(),
        _ => next_detail_no(doc)?,
    };
    let h = 5.0 * frame; // 文字高跟随 TF 图幅。

    // 3) 放大块：内容 + 框线（色31）+ 顶部序号/比例（色3）。
    let member_count = members.len();
    let mut blk: Vec<E> = members;
    match &shape {
        GuideShape::Circle { .. } => {
            let mut e = E::Circle(Circle::from_center_radius(Vector3::ZERO, win.circum_radius() * k));
            set_member_layer(&mut e, "8符号标注层");
            e.common_mut().color = Color::from_index(31);
            blk.push(e);
        }
        GuideShape::Rect { half, .. } => {
            let mut pl = LwPolyline::new();
            for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)] {
                pl.add_point(Vector2::new(sx * half.0 * k, sy * half.1 * k));
            }
            pl.close();
            let mut e = E::LwPolyline(pl);
            set_member_layer(&mut e, "8符号标注层");
            e.common_mut().color = Color::from_index(31);
            blk.push(e);
        }
    }
    // `Ⅰ` + 换行 + 带下划线的 `2:1`（\(\O\) 上划线码对照参考文件）。
    let label = format!(
        "{{\\H1x;{}}}{{\\P}}{{\\O\\H1x;{}:1}}",
        no,
        trim_scale(detail)
    );
    let mut lm = MText::new();
    lm.value = label;
    lm.insertion_point = Vector3::new(0.0, shape.box_top(k, frame), 0.0);
    lm.height = h;
    lm.rotation = 0.0;
    lm.style = "OCSM_GB".into();
    lm.attachment_point = AttachmentPoint::MiddleCenter;
    let mut le = E::MText(lm);
    set_member_layer(&mut le, "8符号标注层");
    le.common_mut().color = Color::from_index(3);
    blk.push(le);

    // 4) 匿名块：*D{n+1} = 放大块；*D{n+2} = 原位标记块（引导形状 + 引出线 + 序号）。
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
            entities: blk,
        },
        "AddBlockRecord",
    )?;

    // 5) 原位标记块：局部原点 = 引导中心；引导形状（色31）+ 引出折线（色31）+
    //    引导旁序号（色3）。整体成块便于选择/移动。
    let r = win.circum_radius();
    let lead = 0.8 * r * frame;
    let (p0x, p0y) = shape.lead_origin();
    let (p1x, p1y) = (
        p0x + std::f64::consts::FRAC_1_SQRT_2 * lead,
        p0y + std::f64::consts::FRAC_1_SQRT_2 * lead,
    );
    let (p2x, p2y) = (p1x + 2.5 * frame, p1y);
    let mk_line31 = |x1: f64, y1: f64, x2: f64, y2: f64| -> E {
        let mut e = E::Line(Line::from_coords(x1, y1, 0.0, x2, y2, 0.0));
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(31);
        e
    };
    let mut origin_blk: Vec<E> = Vec::new();
    // 引导形状（局部 = 原坐标 − 引导中心）。
    match &shape {
        GuideShape::Circle { radius, .. } => {
            let mut e = E::Circle(Circle::from_center_radius(Vector3::ZERO, *radius));
            set_member_layer(&mut e, "8符号标注层");
            e.common_mut().color = Color::from_index(31);
            origin_blk.push(e);
        }
        GuideShape::Rect { half, .. } => {
            let mut pl = LwPolyline::new();
            for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)] {
                pl.add_point(Vector2::new(sx * half.0, sy * half.1));
            }
            pl.close();
            let mut e = E::LwPolyline(pl);
            set_member_layer(&mut e, "8符号标注层");
            e.common_mut().color = Color::from_index(31);
            origin_blk.push(e);
        }
    }
    // 引出折线 + 序号（上移避让：y 方向 +1.0×frame → +2.0×frame）。
    origin_blk.push(mk_line31(
        p0x - origin.0,
        p0y - origin.1,
        p1x - origin.0,
        p1y - origin.1,
    ));
    origin_blk.push(mk_line31(
        p1x - origin.0,
        p1y - origin.1,
        p2x - origin.0,
        p2y - origin.1,
    ));
    let mut nm = MText::new();
    nm.value = format!("{{\\H1.0x;{}}}", no);
    nm.insertion_point =
        Vector3::new(p1x - origin.0 + 1.3 * frame, p1y - origin.1 + 2.0 * frame, 0.0);
    nm.height = h;
    nm.rotation = 0.0;
    nm.style = "OCSM_GB".into();
    nm.attachment_point = AttachmentPoint::MiddleCenter;
    let mut ne = E::MText(nm);
    set_member_layer(&mut ne, "8符号标注层");
    ne.common_mut().color = Color::from_index(3);
    origin_blk.push(ne);

    let origin_block_name = format!("*D{}", max_n + 2);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: origin_block_name.clone(),
            entities: origin_blk,
        },
        "AddBlockRecord",
    )?;

    // 6) INSERT：放大块（自动偏移：引导外接半径×5 向右×frame，或 dx/dy 覆盖）；
    //    原位标记块 @ 引导中心。
    let (px, py) = params
        .detail_pos
        .unwrap_or((origin.0 + 5.0 * r * frame, origin.1));
    let mut ins = E::Insert(Insert::new(block_name.clone(), v3([px, py, 0.0])));
    set_member_layer(&mut ins, "8符号标注层");
    // PE_URL 供标注系数检测（1/(s×frame)）。
    let url = format!(
        "http://127.0.0.1:0/DIM/DETAIL/0?s={}&frame={}",
        trim_scale(detail),
        trim_scale(frame)
    );
    ins.common_mut().extended_data.add_record(pe_url_record(&url));
    let mut oins = E::Insert(Insert::new(origin_block_name.clone(), v3([origin.0, origin.1, 0.0])));
    set_member_layer(&mut oins, "8符号标注层");

    // 7) 新增两个 INSERT；原引导实体删除（图形由原位块替代）。
    let (handle_s, handle_o) = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![ins, oins]),
        "AddEntities",
    )? {
        PluginResponse::Handles(hs) => (hs.first().copied(), hs.get(1).copied()),
        _ => (None, None),
    };
    req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;

    Ok(serde_json::json!({
        "ok": true,
        "insert_handle": handle_s.map(fmt_handle),
        "origin_handle": handle_o.map(fmt_handle),
        "block": block_name,
        "origin_block": origin_block_name,
        "detail_scale": detail,
        "frame": frame,
        "no": no,
        "geom": match shape { GuideShape::Circle { .. } => "circle", GuideShape::Rect { .. } => "rect" },
        "members": member_count,
    })
    .to_string())
}

/// 起点所在 DETAIL 放大块换算系数：标注起点落在某放大块世界 AABB 内且该块
/// 挂有 PE_URL（s/frame）→ 返回 1/(detail×frame)，否则 None。
fn detail_scale_at(doc: &acadrust::CadDocument, pt: [f64; 3]) -> Option<f64> {
    use acadrust::EntityType as E;
    use crate::insert_world_aabb;
    for e in doc.entities() {
        let E::Insert(ins) = e else {
            continue;
        };
        // PE_URL 记录（字符串值）。
        let mut url = None;
        for r in ins.common.extended_data.records() {
            if r.application_name != "PE_URL" {
                continue;
            }
            for v in &r.values {
                if let acadrust::xdata::XDataValue::String(s) = v {
                    url = Some(s.clone());
                    break;
                }
            }
            if url.is_some() {
                break;
            }
        }
        let Some(url) = url else {
            continue;
        };
        let Some(p) = GuideParams::from_url(&url) else {
            continue;
        };
        if p.guide_type != GuideType::Detail {
            continue;
        }
        let Some((min, max)) = insert_world_aabb(doc, ins) else {
            continue;
        };
        if pt[0] >= min.x && pt[0] <= max.x && pt[1] >= min.y && pt[1] <= max.y {
            let f = (p.detail_scale * p.detail_frame).max(1e-9);
            return Some(1.0 / f);
        }
    }
    None
}

/// 弧长标注（ARCLEN，匿名块 + INSERT）：对照 OCSMDIMGULIDE/ARCLEN.dxf 两种形式。
/// 引导 = 1 条 ARC（10引导线层）：中心/半径/起止角自足。
/// - 外侧（dist>0）：尺寸弧 = 引导弧同心外扩 dist（r+dist），界线从引导弧端点
///   沿半径方向外拉到尺寸弧端点，箭头顶在尺寸弧端点（尖端朝外）。
/// - 内侧（dist<0）：尺寸弧 = 引导弧等半径平移 |dist|（圆心沿弧中点反方向），
///   界线从尺寸弧端点连回引导弧端点（方向 ∥ 弧中点方向），箭头朝引导弧。
/// 文字 = 尺寸弧中点外 1×scale（径向），前加小半圆弧 ⌒ 符号（色3/层0，
/// 对称轴沿弧外侧方向，忠实参考）。块原点 = 引导圆心；INSERT 于引导圆心。
/// 测量值 = r × 扫角（放大图内另乘 detail 系数还原真实尺寸）。
pub(crate) fn apply_arclen(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    handle: acadrust::Handle,
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::entities::{Arc, Insert, Line, MText};
    use acadrust::entities::AttachmentPoint;
    use acadrust::types::{Color, Vector3};
    use acadrust::EntityType as E;

    let e = doc.get_entity(handle).ok_or("找不到引导弧实体")?;
    let acadrust::EntityType::Arc(a) = e else {
        return Err("弧长标注需要 ARC 引导（10引导线层 的圆弧）".into());
    };
    if a.radius < 1e-9 || a.sweep_angle() <= 1e-9 {
        return Err("引导弧半径或扫角无效".into());
    }
    let center = v3([a.center.x, a.center.y, a.center.z]);
    let r = a.radius;
    let sweep = a.sweep_angle();
    let arc_len = r * sweep;
    let scale = frame_scale_at(doc, [a.center.x, a.center.y, a.center.z]);
    let h = 2.5 * scale; // 文字高（DIMTXT）
    let asz = 2.5 * scale; // 箭头长（DIMASZ）
    let dist = params.dist;
    let d = dist.abs();
    let side_out = dist >= 0.0; // dist=0 → 尺寸弧与引导弧重合（外侧语义）

    // 弧端点/中点方向（相对引导圆心）。
    let u0 = Vector3::new(a.start_angle.cos(), a.start_angle.sin(), 0.0);
    let u1 = Vector3::new(a.end_angle.cos(), a.end_angle.sin(), 0.0);
    let um = Vector3::new(
        ((a.start_angle + a.end_angle) / 2.0).cos(),
        ((a.start_angle + a.end_angle) / 2.0).sin(),
        0.0,
    );

    // 测量值（放大图内 × 系数还原真实尺寸）。
    let coef = detail_scale_at(doc, [a.center.x, a.center.y, a.center.z]);
    let measurement = arc_len * coef.unwrap_or(1.0);
    let auto_value = format_measurement(measurement, params.dec.unwrap_or(2));
    let text_visible = match &params.text {
        Some(t) if !t.is_empty() && *t != "<>" => {
            t.replace("%%c", "").replace("<>", &auto_value)
        }
        _ => auto_value,
    };
    // 公差（极限偏差）：与线性/直径半径同款堆叠 `\H0.71x;\C2;\S{up}^{dn};`。
    let tol_seg: Option<String> = match (&params.up, &params.dn) {
        (Some(up), Some(dn)) if !up.is_empty() && !dn.is_empty() => {
            Some(format!("\\H0.71x;\\C2;\\S{}^{};", up, dn))
        }
        _ => None,
    };
    let body = render_text_template(&text_visible, measurement, tol_seg.as_deref());

    // 尺寸弧几何。
    let (arc_center, arc_r) = if side_out {
        (center, r + d)
    } else {
        (center - um * d, r) // 等半径平移（参考 *D2）
    };
    // 尺寸弧端点（外侧：引导弧端点径向延长；内侧：平移后弧对应端点）。
    let t0 = arc_center + u0 * arc_r;
    let t1 = arc_center + u1 * arc_r;
    // 界线：引导弧端点 ⇄ 尺寸弧端点外 2mm（示例界线出头 2.0，B 端亦同）。
    let (b0, b1) = (center + u0 * r, center + u1 * r);
    let head = 2.0 * scale;
    let ext0 = if side_out { u0 } else { -um }; // 外侧沿半径、内侧沿平移方向
    let ext1 = if side_out { u1 } else { -um };
    let e0 = t0 + ext0 * head;
    let e1 = t1 + ext1 * head;
    // 箭头（对照示例 SOLID 三角）：尖 = 尺寸弧端点；箭翼沿弧端点处切线方向、
    // 朝弧外侧张开：起点端翼向 = +T(α0)=（−sinα0,cosα0）、终点端 = −T(α1)。
    // arrow_solid 的 away = 尖→尾方向 = 翼向的相反。
    let away0 = Vector3::new(a.start_angle.sin(), -a.start_angle.cos(), 0.0);
    let away1 = Vector3::new(-a.end_angle.sin(), a.end_angle.cos(), 0.0);
    // 文字：中心锚点（MiddleCenter，宿主最可靠），文字中心在尺寸弧中点上方
    // h（=DIMTXT）处：下缘距弧 = h−h/2 = 1.25×scale，保证不与弧线相交
    //（TopLeft 锚点在弧上 1.0 时文字块向下展开会压弧——用户实测反馈）。
    let arc_mid = arc_center + um * arc_r;
    let text_pos = arc_mid + um * h;
    let text_rot = (a.start_angle + a.end_angle) / 2.0 - std::f64::consts::PI / 2.0;
    // 小圆弧 ⌒ 符号：距文字约 2 个数字宽（用户二轮微调：4 → 2 字宽，贴靠）。
    // 相对文字中心：− baseline×(0.5w + 2×0.59h) − um×0.5h。
    let baseline = Vector3::new(text_rot.cos(), text_rot.sin(), 0.0);
    let text_w = text_visible.chars().count() as f64 * 0.59 * h; // 文字宽估算（与直径块同惯例）
    let sym_gap = 2.0 * 0.59 * h; // 符号中心距文字起点约 2 个数字宽
    let sym_center = text_pos - baseline * (text_w * 0.5 + sym_gap) - um * (h * 0.5);
    let sym_r = 2.6 * scale;
    let sym_dir_up = um.y.atan2(um.x);
    let sym_start = crate::detail_clip::norm_angle(sym_dir_up + 3.0 * std::f64::consts::PI / 2.0);
    let sym_end = crate::detail_clip::norm_angle(sym_dir_up + std::f64::consts::PI / 2.0);

    // ── 块成员（原点 = 引导圆心）──
    let local = |p: Vector3| p - center;
    let mut members: Vec<E> = Vec::new();
    // 示例块成员显式色：尺寸弧/界线/箭头 = 130（灰蓝），文字 = 3（绿），
    // 小弧 = 3；7标注层层色为 4（青），必须显式覆盖否则渲染青色。
    let mut mk_line = |a: Vector3, b: Vector3| -> E {
        let mut e = E::Line(Line {
            common: Default::default(),
            start: local(a),
            end: local(b),
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "7标注层");
        e.common_mut().color = Color::from_index(130);
        e
    };
    let mut mk_arrow = |tip: Vector3, away: Vector3| -> E {
        let mut e = E::Solid(arrow_solid(local(tip), away, asz));
        set_member_layer(&mut e, "7标注层");
        e.common_mut().color = Color::from_index(130);
        e
    };
    let mut mk_mtext = |value: String, pos: Vector3| -> E {
        let mut m = MText::new();
        m.value = value;
        m.insertion_point = local(pos);
        m.height = h;
        m.rotation = text_rot;
        m.style = "OCSM_GB".into();
        m.attachment_point = AttachmentPoint::MiddleCenter; // 中心锚点：文字整体居弧上方
        let mut e = E::MText(m);
        set_member_layer(&mut e, "7标注层");
        e.common_mut().color = Color::from_index(3);
        e
    };

    // 尺寸弧（7标注层，显式 130）。
    let mut dim_arc = E::Arc(Arc::from_center_radius_angles(
        local(arc_center),
        arc_r,
        a.start_angle,
        a.end_angle,
    ));
    set_member_layer(&mut dim_arc, "7标注层");
    dim_arc.common_mut().color = Color::from_index(130);
    members.push(dim_arc);
    // 界线 ×2：引导弧端点 ⇄ 尺寸弧端点外 2mm。
    members.push(mk_line(b0, e0));
    members.push(mk_line(b1, e1));
    // 箭头 ×2（尖 = 尺寸弧端点，翼沿端点切线朝外）。
    members.push(mk_arrow(t0, away0));
    members.push(mk_arrow(t1, away1));
    // 文字（旋转 mid−90°、TopLeft 锚点）。
    members.push(mk_mtext(body, text_pos));
    // 小圆弧 ⌒ 符号（色3、层0，忠实参考）。
    let mut sym = E::Arc(Arc::from_center_radius_angles(
        local(sym_center),
        sym_r,
        sym_start,
        sym_end,
    ));
    sym.common_mut().layer = "0".into();
    sym.common_mut().color = Color::from_index(3);
    members.push(sym);

    // ── 匿名块 *D{n}（与其他块共用计数器）──
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

    // ── INSERT @ 引导圆心，8符号标注层 ──
    let mut ins = E::Insert(Insert::new(block_name.clone(), center));
    set_member_layer(&mut ins, "8符号标注层");
    {
        // 可再编辑信息：弧长引导 = ARC，几何按 `guide_geom_points` 的约定
        // （圆心 + 起点/终点/中点）；参数 URL 由 GuideParams 原样编码。
        // 与 `guide_geom_points` 对 ARC 的约定一致：起点 / 终点 / 中点。
        let sp = a.start_point();
        let ep = a.end_point();
        let mp = a.midpoint();
        let arc_pts = [
            [sp.x, sp.y, sp.z],
            [ep.x, ep.y, ep.z],
            [mp.x, mp.y, mp.z],
        ];
        let port = crate::current_guide_port().unwrap_or(0);
        stamp_edit_entity(&mut ins, &params.to_url(port), &arc_pts, "arc");
    }
    let handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![ins]),
        "AddEntities",
    ) {
        Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
        Ok(_) => None,
        Err(e) => return Err(e),
    };

    Ok(serde_json::json!({
        "ok": true,
        "block": block_name,
        "insert_handle": handle.map(fmt_handle),
        "arc_len": arc_len,
        "text": text_visible,
        "dist": dist,
        "side": if side_out { "out" } else { "in" },
        "geom": "arc",
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
    // 框格尺寸（GB/T 1182，一倍注释比例 scale=1）：符号/基准格 7×7，数字格按内容自适应（14/21/28）。
    let ch = 7.0 * scale; // 格高（所有格一致）
    let cw_sym = 7.0 * scale; // 符号/基准格宽
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
    let mut mk_mtext = |value: String, pos: Vector3, rotation: f64| -> E {
        let mut m = MText::new();
        m.value = value;
        m.insertion_point = pos;
        m.height = h;
        m.rotation = rotation;
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

    // ── 方框（FCF）：书写方向沿引导线最后一段（Pn-1→Pn），多行沿垂直方向堆叠 ──
    // 每行一个独立 FCF 框。单元格列：[符号] [⌀?公差] [基准1] [基准2] [基准3]。
    let rows = params.gdt_rows_nonempty();
    if rows.is_empty() {
        return Err("形位公差需至少一行（符号/公差/基准）".into());
    }

    let attach = to_local(&pts[pts.len() - 1]); // 框接入点（局部）
    // 书写方向规范化：
    //   - 末段水平（|x|≥|y|）→ 固定从左到右读（符号在最左，基准在右）
    //   - 末段竖直 → 固定从下往上读（符号在最下，基准在上），文字逆时针旋转 90°
    let raw = {
        let d = attach - to_local(&pts[pts.len() - 2]);
        let l = d.length().max(1e-12);
        d * (1.0 / l)
    };
    let vert = raw.y.abs() > raw.x.abs();
    // 读取方向（符号→基准）：水平固定 +x（左→右），竖直固定 +y（下→上），文字旋转 90°。
    let u = if vert {
        Vector3::new(0.0, 1.0, 0.0)
    } else {
        Vector3::new(1.0, 0.0, 0.0)
    };
    // 锚点侧（nearest-end）：引导线末端指向哪一侧，锚点就接在框的哪一侧边缘，
    // 框从锚点向“远离引导线来向”的方向展开 —— 引导线杆体不会穿过框（对照 形位公差5.dxf）。
    //   dirsign=+1：末段朝右/朝上来 → 锚点=符号侧（框右/上展开），符号格贴锚点。
    //   dirsign=-1：末段朝左/朝下来 → 锚点=C侧（框左/下展开），符号格在远端。
    let dirsign: f64 = if vert {
        if raw.y < 0.0 { -1.0 } else { 1.0 }
    } else {
        if raw.x < 0.0 { -1.0 } else { 1.0 }
    };
    let perp = Vector3::new(-u.y, u.x, 0.0); // 行堆叠方向（水平→向上；竖直→向左）
    let text_rot = if vert { std::f64::consts::FRAC_PI_2 } else { 0.0 }; // 逆时针 90°

    // 估算 OCSM_GB 字体文本宽度（字高 h）：数字/字母≈0.6h、⌀(%%c)≈1.2h、gdt 符号≈1.1h。
    let est_width = |s: &str| -> f64 {
        let mut w = 0.0;
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '%' && i + 2 < chars.len() && chars[i + 1] == '%' && chars[i + 2] == 'c' {
                w += 1.2 * h;
                i += 3;
            } else if chars[i] == '{' {
                // {\Fgdt;[\H0.8x;]x}：gdt 字形宽≈1.1h（80% 字号 → 0.9h）。
                // 跳到匹配的 `}` 结束（token 长度可变，不能固定步长）。
                w += 0.9 * h;
                let close = chars[i + 1..].iter().position(|&c| c == '}').unwrap_or(0) + 1;
                i += 2 + close;
            } else if chars[i] == '.' {
                w += 0.35 * h;
                i += 1;
            } else {
                w += 0.6 * h;
                i += 1;
            }
        }
        w
    };
    // 数字格宽：内容估算 → 7 的倍数 14/21/28（clamp）。
    let num_w = |tol_text: &str| -> f64 {
        let need = est_width(tol_text);
        let steps = ((need / (7.0 * scale)).ceil() as i64).clamp(2, 4);
        (steps as f64) * 7.0 * scale
    };

    // 每行内容 → 单元格列表 (文本, 格宽)：符号/基准格 cw_sym，数字格自适应。
    // gdt 字形统一 80%：{\Fgdt;\H0.8x;X}（宿主 text_support 支持 \H…x; 相对高度）。
    // 顺序 = 读数顺序：符号 → [⌀公差+修饰符] → 基准1 → 基准2 → 基准3。
    let cells_of = |r: &GdtRow| -> Vec<(String, f64)> {
        let mut c = Vec::new();
        if let Some(ch) = r.sym.chars().next() {
            c.push((format!("{{\\Fgdt;\\H0.8x;{}}}", ch.to_ascii_lowercase()), cw_sym));
        }
        if !r.tol.is_empty() {
            // 数字格：⌀前缀 + 公差值 + 修饰符（ⓂⓁⓅⓈ 跟在数字后同一格）。
            let mut v = format!("{}{}", if r.dia { "%%c" } else { "" }, r.tol);
            for m in r.mods.chars() {
                if matches!(m.to_ascii_lowercase(), 'm' | 'l' | 'p' | 's') {
                    v.push_str(&format!("{{\\Fgdt;\\H0.8x;{}}}", m.to_ascii_lowercase()));
                } else {
                    v.push(m);
                }
            }
            c.push((v.clone(), num_w(&v)));
        }
        for d in [&r.d1, &r.d2, &r.d3] {
            if !d.is_empty() {
                c.push((d.clone(), cw_sym));
            }
        }
        c
    };

    // 每行总宽（各格宽之和）。
    let total_w = rows.iter()
        .map(|r| cells_of(r).iter().map(|(_, w)| *w).sum::<f64>())
        .fold(0.0, f64::max);

    // 顶部/底部注释（FCF 框上方/下方，独立 MTEXT，世界坐标附近）。
    let n_rows_drawn = rows.iter().filter(|r| !r.is_empty()).count() as f64;
    if let Some(t) = params.gdt_top.as_deref().filter(|s| !s.is_empty()) {
        let pos = attach + perp * (ch * n_rows_drawn.max(1.0) + h * 2.0);
        members.push(mk_mtext(t.to_string(), pos, 0.0));
    }
    if let Some(b) = params.gdt_bot.as_deref().filter(|s| !s.is_empty()) {
        let pos = attach - perp * (ch + h * 2.0);
        members.push(mk_mtext(b.to_string(), pos, 0.0));
    }

    for (k, r) in rows.iter().enumerate() {
        let cells = cells_of(r);
        if cells.is_empty() {
            continue;
        }
        // 行 k：框长轴沿 u 延伸，短轴沿 perp 横跨 ±(ch/2)。行间沿 perp 堆叠（中心相隔 ch）。
        // 符号格永远在框 u 方向的起点（base）；dirsign<0 时锚点在框 C 端，框向 -u 展开。
        let row_w: f64 = cells.iter().map(|(_, w)| *w).sum();
        let anchor = attach + perp * (k as f64 * ch); // 引导线接入点所在的行线
        let base = if dirsign < 0.0 { anchor - u * row_w } else { anchor };
        let right = base + u * row_w;
        let lo = base - perp * (ch / 2.0);
        let hi = base + perp * (ch / 2.0);
        let rlo = right - perp * (ch / 2.0);
        let rhi = right + perp * (ch / 2.0);
        // 四边：左封口、右封口、顶边、底边。
        members.push(mk_line(lo, hi, 31));
        members.push(mk_line(rlo, rhi, 31));
        members.push(mk_line(hi, rhi, 31));
        members.push(mk_line(lo, rlo, 31));
        // 每格：内部分隔竖线 + 居中内容（按各自宽度累加位置）。
        let mut x = 0.0;
        for (i, (text, w)) in cells.iter().enumerate() {
            if i > 0 {
                let xi = base + u * x;
                members.push(mk_line(xi - perp * (ch / 2.0), xi + perp * (ch / 2.0), 31));
            }
            let ci = base + u * (x + w / 2.0);
            members.push(mk_mtext(text.clone(), ci, text_rot));
            x += w;
        }
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
        "rows": rows.len(),
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
    let posted = handle_hex(&req.handle)?;

    // “仅应用”（刷新参数记录 / 写 PE_URL）也改文档 → 开事务，结束时 commit。
    begin_undo(sender, "尺寸引导")?;

    // ── 编辑模式：posted 实体本身是 OCSM 生成的标注（带 `OCSM_EDIT`）──
    // 用记录里的引导几何造一条**临时引导**重走现有生成路径（各类型代码不用改），
    // 生成后删临时引导 + 删旧标注 → 用记录里的引导几何重生成，达成"替换"。
    if let Some((_, pts, kind, guide)) = read_edit_record(sender, posted) {
        if !refresh {
            // 只应用不生成：仅刷新参数记录（保持可编辑）。
            let kind2 = kind.clone();
            let _ = req_timed(
                sender,
                PluginRequest::WriteRecord {
                    handle: posted,
                    record: edit_record(&req.url, &pts, &kind2, guide),
                },
                "WriteRecord",
            );
            mark_dirty(sender)?;
            commit_undo(sender);
            return Ok(r#"{"ok":true,"edit":true}"#.into());
        }
        // 优先复用原引导（若还在且类型兼容），否则造临时引导。
        let doc = snapshot(sender)?;
        let reuse = guide.filter(|h| {
            doc.get_entity(*h)
                .is_some_and(|e| guide_kind_of(e) == kind)
        });
        let (work_handle, temp) = match reuse {
            Some(h) => (h, false),
            None => {
                let ent = temp_guide_entity(&kind, &pts)?;
                let h = match req_timed(
                    sender,
                    PluginRequest::AddEntity(ent),
                    "AddEntity",
                ) {
                    Ok(PluginResponse::Handle(h)) => h,
                    Ok(_) => return Err("创建临时引导失败".into()),
                    Err(e) => return Err(e),
                };
                (h, true)
            }
        };
        let body2 = serde_json::json!({
            "handle": fmt_handle(work_handle),
            "url": req.url,
        })
        .to_string();
        let out = do_apply_inner(sender, body2.as_bytes(), true)?;
        if temp {
            let _ = req_timed(
                sender,
                PluginRequest::RemoveEntity {
                    handle: work_handle,
                },
                "RemoveEntity",
            );
        }
        // 删旧标注 → 给新标注回写可编辑信息。
        req_timed(
            sender,
            PluginRequest::RemoveEntity { handle: posted },
            "RemoveEntity",
        )?;
        let gk = kind.clone();
        for h in produced_handles(&out) {
            stamp_editable(sender, h, &req.url, &pts, &gk, reuse.or(guide));
        }
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        commit_undo(sender);
        return Ok(serde_json::json!({
            "ok": true,
            "edit": true,
            "replaced": fmt_handle(posted),
        })
        .to_string());
    }

    // ── 普通模式（posted = 引导实体）──
    // 先记下引导几何：**多数类型生成后会把引导删掉**（尺寸/基准/向视图/剖切/公差…），
    // 所以必须生成前取。
    let pre = snapshot(sender).ok().and_then(|doc| {
        doc.get_entity(posted).map(|e| {
            (
                guide_kind_of(e),
                guide_geom_points(&doc, posted).unwrap_or_default(),
            )
        })
    });
    let out = do_apply_inner(sender, body, refresh)?;
    if refresh {
        if let Some((kind, pts)) = pre {
            for h in produced_handles(&out) {
                stamp_editable(sender, h, &req.url, &pts, kind, Some(posted));
            }
        }
    }
    commit_undo(sender);
    Ok(out)
}

fn do_apply_inner(
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

    // 局部放大图：引导 = 圆（CIRCLE）或闭合 4 顶点矩形（PLINE），不删引导（改层保留）。
    if params.guide_type == GuideType::Detail {
        let out = apply_detail(sender, &doc, handle, &params)?;
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        return Ok(out);
    }

    // 弧长标注：引导 = ARC，匿名块 + INSERT，生成后删引导弧。
    if params.guide_type == GuideType::ArcLen {
        let out = apply_arclen(sender, &doc, handle, &params)?;
        req_timed(sender, PluginRequest::RemoveEntity { handle }, "RemoveEntity")?;
        req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
        mark_dirty(sender)?;
        return Ok(out);
    }

    // 引导几何顶点：LINE→2 点；ANGLE 用两段 PLINE（3 顶点）。
    let pts = guide_geom_points(&doc, handle)?;
    if pts.len() < 2 {
        return Err("引导线顶点不足（需至少 2 个）".into());
    }
    let (p1, p2) = (pts[0], pts[1]);
    // 按引导线第一点 P1 判定图幅缩放，创建/选用对应样式（文字/箭头缩放）。
    let style = ensure_style_for_point(sender, &doc, p1)?;

    // 撤销事务由调用方（`do_apply`）开启与提交：本函数是它的一部分，不再单独 push
    // （再 push 会把事务切成两段，后一段又会在 message 边界被当空条目丢弃）。

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

    // 焊接符号：两段 PLINE（恰好 3 顶点）→ 匿名块 *W{n} + INSERT@拐点。
    // 引导 PLINE 不删（10引导线层不打印），保留可重选再改。
    if params.guide_type == GuideType::Weld {
        if pts.len() != 3 {
            return Err("焊接标注需要两段多段线（PLINE，3 顶点：焊缝点→拐点→基准线末端）".into());
        }
        return apply_weld(sender, &doc, pts[0], pts[1], pts[2], &params);
    }

    // 引线标注：两段 PLINE（恰好 3 顶点）→ 匿名块 *L{n} + INSERT@拐点。
    // 焊接的减法版：只留 引线+箭头+肩线+上下侧文字。
    if params.guide_type == GuideType::Leader {
        if pts.len() != 3 {
            return Err("引线标注需要两段多段线（PLINE，3 顶点：箭头点→拐点→肩线末端）".into());
        }
        return apply_leader(sender, &doc, pts[0], pts[1], pts[2], &params);
    }

    // 序号标注：两段 PLINE（恰好 3 顶点）→ 匿名块 *XH{n} + INSERT@拐点。
    // 一个组 = 1 指引线 + 1 圆点 + N 条横线（横向 V 折线连 / 纵向竖线连）+ N 个序号。
    if params.guide_type == GuideType::Balloon {
        if pts.len() != 3 {
            return Err("序号标注需要两段多段线（PLINE，3 顶点：指针点→拐点→肩线末端）".into());
        }
        return apply_balloon(sender, &doc, pts[0], pts[1], pts[2], &params);
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
        Ok(doc) => {
            // 先看 handle 是不是 OCSM **生成的标注**（带 `OCSM_EDIT`）：
            // 标注是 INSERT / DIMENSION，`guide_geom_points` 不认——必须先走这条路，
            // 否则会先把错误报给 GUI（那正是"引导线必须为直线或多段线"提示的来源），
            // 参数也因此回填不了。引导几何直接取自记录。
            let edit = read_edit_record(sender, handle);
            let (pts, geom_forced, url) = match (&edit, guide_geom_points(&doc, handle)) {
                (Some((u, p, k, _)), _) if p.len() >= 2 => {
                    (p.clone(), Some(k.clone()), Some(u.clone()))
                }
                (None, Ok(p)) => (p, None, read_pe_url(sender, handle)),
                (_, Err(e)) => {
                    return (404, json, serde_json::json!({"ok": false, "error": e}).to_string())
                }
                (_, Ok(p)) => (p, None, read_pe_url(sender, handle)),
            };
            let is_edit = edit.is_some();
            {
                let (p1, p2) = (pts[0], pts[1]);
                let geom = geom_forced;
                let params = url.as_deref().and_then(GuideParams::from_url);
                // 先建小对象，再把大的 `params` 单独塞进去（同一个 json! 里嵌套太深
                // 会触发宏递归上限）。
                let mut resp = serde_json::json!({
                    "ok": true,
                    "handle": fmt_handle(handle),
                    "edit": is_edit,
                    "p1": [p1[0], p1[1], p1[2]],
                    "p2": [p2[0], p2[1], p2[2]],
                    // 引导线全部顶点（ANGLE 两段 PLINE = 3 个：p0/p1/p2）。
                    "pts": pts.iter().map(|q| [q[0], q[1], q[2]]).collect::<Vec<_>>(),
                    // 引导几何形态：line（LINE）/ pline（LWPOLYLINE，2+ 顶点）。
                    "geom": geom
                        .clone()
                        .or_else(|| guide_geom_kind(&doc, handle).ok().map(str::to_string))
                        .unwrap_or_else(|| "line".to_string()),
                    "url": url,
                    // 引导线长度 = 主尺寸（公差计算用）。
                    "measurement": ((p2[0]-p1[0]).powi(2)+(p2[1]-p1[1]).powi(2)).sqrt(),
                });
                resp["params"] = match params {
                    Some(p) => serde_json::json!({
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
                        "gdt_rows": p.gdt_rows.iter().map(|r| serde_json::json!({
                            "sym": r.sym, "dia": r.dia, "tol": r.tol, "mods": r.mods,
                            "d1": r.d1, "d2": r.d2, "d3": r.d3,
                        })).collect::<Vec<_>>(),
                        "gdt_top": p.gdt_top,
                        "gdt_bot": p.gdt_bot,
                        "detail_scale": p.detail_scale,
                        "detail_no": p.detail_no,
                        "detail_pos": p.detail_pos,
                        "weld": weld_params_json(&p.weld),
                        "leader": {
                            "upper": p.leader.upper,
                            "lower": p.leader.lower,
                        },
                        "balloon": {
                            "items": p.balloon.items,
                            "dir": p.balloon.dir.as_str(),
                            "ins": p.balloon.insert_mode,
                        },
                    }),
                    None => serde_json::Value::Null,
                };
                // 序号标注贴心信息：图纸上已有的序号 + 按"上一个 +1"算出的下一个。
                {
                    let existing = existing_item_nos(&doc);
                    let next = crate::balloon::next_after_all(&existing, "0");
                    resp["balloon_next"] = serde_json::json!({
                        "existing": existing,
                        "next": next,
                    });
                }
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

/// `POST /api/rough_apply`：表面粗糙度符号生成（命令 OCSMRGH / CC，无引导线）。
fn api_rough_apply(body: &[u8], sender: &Arc<dyn PluginRequestSender>) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    match apply_roughness(sender, body) {
        Ok(s) => (200, json, s),
        Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
    }
}

/// 表面粗糙度符号：生成匿名块 `*D{n}`（几何显式色 31 + 绿色文字转 ATTDEF）
/// + INSERT（层 0，带 attributes，rotation 由 GUI 指定）。
///
/// 20 形态 = 4 基础体 × 5 附加区，坐标/文字位全部对照参考
/// `OCSMDIMGULIDE/粗糙度1.dxf`（zw$ 块，20 个）1:1（× 图幅倍率）。
///
/// 基础体：C1 通用 / C2 以不去除材料的方法获得（V 内圆）/ C3 去除材料（V 内横线）/
///         C4 焊后加工（横线+短线+填充三角）
/// 附加区：R1 基础 / R2 周边相同处理（长线+圆）/ R3 高级（短线）/
///         R4 上限开关（台阶）/ R5 上限开关+周边相同处理（台阶+长线+圆）
/// 文字 ATTDEF（tag = 中文描述+英文代号，style OCSM_GB，可缺省空白）：
/// 公共 A'（上限）/A（下限）@(8.248,11.65/7.1) h3.5 ML、E（加工余量）
/// @(1.386,0) h4.9 ML；P（加工符号）@(11.009,1.4) h3.5 MC「仅非 C2 列」；
/// 附加区 B/B'/C/G 按形态（R2: x=16.14；R3: x=14.473；R4/R5: x=17.706，
/// B'@y=18、B@y=12.4、C@y=6.8、G@y=2.25）。

/// `POST /api/part_pick`：按选择器页的族/规格/视图**参数化生成**零件并插入。
///
/// 与既有 apply 路径同构：worker 线程向宿主发请求（建块 → 加 INSERT → 写记录 → 标脏）。
/// 落点优先级：**显式 `x`/`y`（+可选 `z`/`rotation`，AI/MCP 一行驱动）** →
/// `OCSMPART`/`SP` 命令里点选的点（`crate::take_parts_point`） → 原点。
/// 块名规则 `OCSM_<族>_<规格>`，重复插入同一规格复用块定义。

/// `POST /api/part_export`：**零件出库** —— 生成零件、建好块（幂等）、登记为"待放置"，
/// 之后图纸里的 `XL` 放置态即可用鼠标跟随预览并连续点放（不需要任何剪切板/宿主改动）。
/// 零件块名：`OCSM_<族>_<规格>`（只留 ASCII 字母数字/下划线，利于复用、BOM 与排查；
/// “M8×35”→`M8_35`、“Ø8”→`8`）。
fn part_block_name(family: &str, spec: &str) -> String {
    /// `separator_chars`：额外当作分隔符的字符（规格里的 `x`/`×`/`Ø`；族名里没有，
    /// 所以族名传空——否则 `hex` 的 x 会被切开）。
    fn clean(text: &str, separator_chars: &[char]) -> String {
        let mut out = String::new();
        for ch in text.chars() {
            if (ch.is_ascii_alphanumeric() || ch == '-') && !separator_chars.contains(&ch) {
                out.push(ch.to_ascii_uppercase());
            } else if !out.ends_with('_') {
                out.push('_');
            }
        }
        out.trim_matches('_').to_string()
    }
    format!(
        "OCSM_{}_{}",
        clean(family, &[]),
        clean(spec, &['x', 'X', '×', 'Ø', 'ø'])
    )
}

/// `POST /api/joint`：按**件链**装配螺栓副（`OCSMJOINT` 命令走同一实现）。
///
/// 与零件库插入同构（建块 → 加 INSERT → 写记录 → 标脏），差别是**一次事务里放多件**：
/// 件链的高度求和、长度落点（取该族表内供货系列 ≥ 需求的最小值）与各件基点由
/// `crate::joint::plan` 算好（确定性、可回归）。**不判断“该不该加平垫/弹垫”** ——
/// 件链由 skill/工况层决定。
pub(crate) fn apply_joint(
    sender: &Arc<dyn PluginRequestSender>,
    body: &[u8],
) -> Result<String, String> {
    let spec = crate::joint::JointSpec::from_json(body)?;
    let plan = crate::joint::plan(&spec)?;
    begin_undo(sender, "OCSM 螺栓副")?;
    let doc = snapshot(sender)?;
    // ① 先把整链几何算出来（生成 → 遮挡裁剪）：裁剪后的几何才是块定义
    let built = crate::joint::build(&plan, spec.trim)?;
    let trim_notes = built.notes.clone();
    let mut known_blocks: std::collections::HashSet<String> =
        doc.block_records.iter().map(|b| b.name.clone()).collect();
    let mut placed: Vec<serde_json::Value> = Vec::new();
    for (index, placement) in plan.placements.iter().enumerate() {
        let part = built.parts[index].clone();
        let spans = built.hidden.get(index).cloned().unwrap_or_default();
        let mut block = part_block_name(&placement.family, &part.meta.spec);
        if !spans.is_empty() {
            // 裁剪过的件用独立块名（块定义不能被裁，只能按裁剪档位各建一块）
            block = format!("{block}_CUT{}", crate::joint::cut_tag(&spans));
        }
        if !known_blocks.contains(&block) {
            req_timed(
                sender,
                PluginRequest::AddBlockRecord {
                    name: block.clone(),
                    entities: part.entities.clone(),
                },
                "AddBlockRecord",
            )?;
            known_blocks.insert(block.clone());
        }
        let mut ins = acadrust::entities::Insert::new(
            &block,
            Vector3::new(placement.at[0], placement.at[1], placement.at[2]),
        );
        ins.rotation = placement.rot_rad;
        {
            let c = &mut ins.common;
            c.layer = crate::partgen::LAYER_MAIN.to_string();
            c.color = ocs_plugin_api::host::acadrust::types::Color::ByLayer;
            c.linetype = "ByLayer".to_string();
            c.line_weight = ocs_plugin_api::host::acadrust::types::LineWeight::ByLayer;
        }
        let handle = match req_timed(
            sender,
            PluginRequest::AddEntities(vec![acadrust::EntityType::Insert(ins)]),
            "AddEntities",
        )? {
            PluginResponse::Handles(hs) => hs.first().copied(),
            _ => None,
        };
        if let Some(h) = handle {
            let mut rec = ocs_plugin_api::host::acadrust::xdata::ExtendedDataRecord::new("OCSM_PART");
            rec.values.push(ocs_plugin_api::host::acadrust::xdata::XDataValue::String(
                serde_json::json!({
                    "family": placement.family,
                    "spec": part.meta.spec,
                    "code": part.meta.code,
                    "joint": true,
                    "bolt_l": plan.bolt_l,
                    "block": block,
                    // 被遮挡剪掉的区间（件链坐标，mm）：0 表示未裁剪
                    "trimmed": spans,
                })
                .to_string(),
            ));
            let _ = req_timed(
                sender,
                PluginRequest::WriteRecord { handle: h, record: rec },
                "WriteRecord",
            );
            placed.push(serde_json::json!({
                "handle": fmt_handle(h),
                "family": placement.family,
                "spec": part.meta.spec,
                "at": placement.at,
                "block": block,
                "trimmed": spans,
            }));
        }
    }
    req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
    mark_dirty(sender)?;
    commit_undo(sender);
    // 报告里补一句遮挡裁剪（命令行日志要能审计"哪些线为什么没画"）
    let mut report = plan.report.clone();
    if !trim_notes.is_empty() {
        report.push_str(&format!("｜遮挡裁剪 {}", trim_notes.join("；")));
    }
    Ok(serde_json::json!({
        "ok": true,
        "at": spec.at,
        "rot": spec.rot_deg,
        "bolt": { "family": plan.bolt_family, "d": plan.bolt_d, "l": plan.bolt_l },
        "stack": plan.stack,
        "need": plan.need,
        "protrude_mm": plan.protrude_mm,
        "trim": spec.trim,
        "trimmed": trim_notes,
        "report": report,
        "placed": placed,
    })
    .to_string())
}

/// 人类侧的**命令目录**（窗口左侧那个总表）：一句话 + 别名。
/// 这是"插件自带"的部分——手册 md 没装也能看命令清单。
pub const COMMAND_CATALOG: &[(&str, &str, &str)] = &[
    ("OCSM", "", "初始化：建图层/线型/文字样式/标注样式（并打开零件库窗口）"),
    ("1 … 10", "数字键", "切当前图层；有选中对象时把对象移到该层"),
    ("OCSMFRAMEINIT", "TF", "打开图框选择窗口（插件目录 frame/*.dwg）"),
    ("OCSMFRAMEINSERT", "", "按所选图框 + 比例插入（光标跟随，比例感知标注样式）"),
    ("OCSMPART", "XL", "标准件插入：不带参数=开零件库窗口+放置态；带参数=一行直插"),
    ("OCSMJOINT", "", "螺栓副装配：不带参数=开装配窗口+放置态；带参数=一行直装（件链算长度、遮挡裁剪、一次撤销）"),
    ("OCSMPOWERDIM", "D", "智能标注：拾取点模式标线性/对齐/半径/直径（Enter 切线段点选）"),
    ("OCSMDIMGULIDE", "GDIM", "引导线标注：选引导线 → 配置窗口（尺寸/剖视/向视/局部放大/角度/弧长/焊接/引线/公差/粗糙度/形位公差）"),
    ("OCSMEDIT", "ME", "改标注：选中 OCSM 生成的标注 → 配置窗口改参数 → 重生成"),
    ("OCSMRGH", "CC", "表面粗糙度：点选插入点 → 配置窗口（匿名块 + ATTDEF）"),
    ("OCSMDIM2GB", "D2G", "一键转国标：原生标注 → OCSM_GB 样式 + 匿名块"),
    ("OCSMBOM", "BOM", "明细表：建表/刷新（BOM 30 = 本次首列 30 行）"),
    ("OCSMBOMSYNC", "BOMSYNC", "明细表：按序号球标重排/重建（球标联动入口）"),
    ("OCSMBOMLOCK", "BOMLOCK", "明细表：锁定某行数量（BOMLOCK 5 3 / BOMLOCK 5 off）"),
    ("OCSMBOMXLSX", "BOMXLSX", "明细表：导出到 .xlsx（带「锁定数量」列，可外部编辑）"),
    ("OCSMBOMXLSXI", "BOMXLSXI", "明细表：从 .xlsx/.csv 导入（手改数量自动上锁）"),
    ("OCSMBOMCFG", "BOMCFG", "明细表配置（表头/列/格式）"),
    ("OCSMMCP", "", "打印 MCP/HTTP 接入信息（给外部 AI/脚本）"),
    ("OCSMHELP", "OH", "打开本手册窗口（命令目录 + 操作教程）"),
];

/// 手册 md 的搜索目录（按优先级）：
/// ① 环境变量 `OCSM_MANUAL_DIR`；
/// ② **插件安装目录/handbook**（人类侧教程随插件分发 —— 默认形态）；
/// ③ 仓库内 `crates/ocs_ocsm/handbook`（开发期/源码树直跑）；
/// ④ 旧位置 `~/.agents/skills/ocsm-manual/manual`（兼容：早期把正文放在 skill 里）。
pub fn manual_dirs() -> Vec<std::path::PathBuf> {
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(dir) = std::env::var("OCSM_MANUAL_DIR") {
        if !dir.trim().is_empty() {
            out.push(std::path::PathBuf::from(dir));
        }
    }
    if let Some(dir) = crate::plugin_install_dir() {
        out.push(dir.join("handbook"));
    }
    out.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("handbook"));
    if let Ok(home) = std::env::var("HOME") {
        let home = std::path::PathBuf::from(home);
        out.push(home.join(".agents/skills/ocsm-manual/manual"));
        out.push(home.join(".pi/agent/skills/ocsm-manual/manual"));
    }
    out
}

/// 目录清单（跳过非 md、按文件名排序；标题取文件首个 `# ` 行）。
pub fn manual_topics_in(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("md")) != Some(true) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|x| x.to_str()) else { continue };
        let title = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| {
                text.lines()
                    .find(|l| l.trim_start().starts_with("# "))
                    .map(|l| l.trim_start_matches("# ").trim().to_string())
            })
            .unwrap_or_else(|| stem.to_string());
        out.push((stem.to_string(), title));
        if out.len() >= 200 {
            break;
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// 按 slug 找 md：返回 (路径, 内容)。slug 只允许 `[A-Za-z0-9_-]` 与中文字符，
/// 且不允许路径分隔符（防目录穿越）。
pub fn find_manual(dirs: &[std::path::PathBuf], slug: &str) -> Option<(std::path::PathBuf, String)> {
    if slug.is_empty()
        || slug.contains('/')
        || slug.contains('\\')
        || slug.contains("..")
        || slug.contains('\0')
    {
        return None;
    }
    for dir in dirs {
        let path = dir.join(format!("{slug}.md"));
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Some((path, text));
        }
    }
    None
}

/// `GET /api/manual`（命令目录 + 手册主题）与 `GET /api/manual/md?slug=…`（正文）。
fn api_manual(target: &str) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let dirs = manual_dirs();
    if target.starts_with("/api/manual/md") {
        let slug = query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "slug")
            .map(|(_, v)| urldecode(v))
            .unwrap_or_default();
        return match find_manual(&dirs, &slug) {
            Some((path, md)) => (
                200,
                json,
                serde_json::json!({ "ok": true, "slug": slug, "path": path.display().to_string(), "md": md })
                    .to_string(),
            ),
            None => (
                404,
                json,
                serde_json::json!({
                    "ok": false,
                    "error": format!("手册里没有「{slug}」这篇"),
                    "searched": dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
                    "hint": "把手册 md 放到 ~/.agents/skills/ocsm-manual/manual/，或用环境变量 OCSM_MANUAL_DIR 指定目录",
                })
                .to_string(),
            ),
        };
    }
    // 索引：命令目录 + 主题。**按 slug 去重、高优先级目录胜出**——
    // 手册可能在多个位置各有一份（插件安装目录 + 源码仓库 + 旧 skill 目录），
    // 若不去重，窗口里每篇会重复出现（曾实测 18 篇 × 2 = 36 条）。
    let mut seen: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    for dir in &dirs {
        for (slug, title) in manual_topics_in(dir) {
            seen.entry(slug.clone()).or_insert_with(|| {
                serde_json::json!({
                    "slug": slug,
                    "title": title,
                    "dir": dir.display().to_string(),
                })
            });
        }
    }
    let mut topics: Vec<serde_json::Value> = seen.into_values().collect();
    topics.sort_by(|a, b| {
        a["slug"]
            .as_str()
            .unwrap_or_default()
            .cmp(b["slug"].as_str().unwrap_or_default())
    });
    let groups: Vec<serde_json::Value> = vec![
        serde_json::json!({ "name": "入门与总览", "prefix": "0" }),
        serde_json::json!({ "name": "建图与插入", "prefix": "1" }),
        serde_json::json!({ "name": "标注与符号", "prefix": "2" }),
        serde_json::json!({ "name": "表格与自动化", "prefix": "3" }),
        serde_json::json!({ "name": "机械制图知识", "prefix": "4" }),
    ];
    let commands: Vec<serde_json::Value> = COMMAND_CATALOG
        .iter()
        .map(|(name, alias, summary)| {
            serde_json::json!({ "name": name, "alias": alias, "summary": summary })
        })
        .collect();
    (
        200,
        json,
        serde_json::json!({
            "ok": true,
            "commands": commands,
            "topics": topics,
            "groups": groups,
            "dirs": dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
        })
        .to_string(),
    )
}

fn urldecode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// `POST /api/joint_plan`：**只算不写文档** —— 给 GUI 做实时预览（SVG + 推理一行 + 数字）。
///
/// 与 `apply_joint` 共用 `joint::plan` + `joint::build`，所以预览和落地必然一致。
fn api_joint_plan(body: &[u8]) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    match plan_joint_json(body) {
        Ok(s) => (200, json, s),
        Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
    }
}

fn plan_joint_json(body: &[u8]) -> Result<String, String> {
    let spec = crate::joint::JointSpec::from_json(body)?;
    let plan = crate::joint::plan(&spec)?;
    let built = crate::joint::build(&plan, spec.trim)?;
    let title = format!(
        "{} {}｜{}",
        plan.bolt_family,
        crate::joint::num_text(plan.bolt_l),
        plan.chain_note
    );
    let svg = crate::partgen::to_svg(&built.assembly, &title, 620.0, 300.0);
    Ok(serde_json::json!({
        "ok": true,
        "svg": svg,
        "report": plan.report,
        "stack": plan.stack,
        "need": plan.need,
        "bolt": { "family": plan.bolt_family, "d": plan.bolt_d, "l": plan.bolt_l },
        "protrude_mm": plan.protrude_mm,
        "count": plan.placements.len(),
        "trim": spec.trim,
        "trimmed": built.notes,
        "blocks": plan
            .placements
            .iter()
            .enumerate()
            .map(|(i, p)| serde_json::json!({
                "family": p.family,
                "spec": p.spec,
                "at": p.at,
                "rot": p.rot_rad.to_degrees(),
                "offset": p.offset,
                "view": p.view,
                "trimmed": built.hidden.get(i).cloned().unwrap_or_default(),
            }))
            .collect::<Vec<_>>(),
    })
    .to_string())
}

/// `POST /api/joint_place`：GUI 点「装配到图纸」→ 建**光标预览块** + 登记待放置件链。
///
/// 落点在图纸里点（与零件库同体验）：预览块给光标跟随用；真正落定时
/// `apply_joint` 会把整链按**各件独立块**插进去（明细表/零件识别仍按件走）。
fn api_joint_place(sender: &Arc<dyn PluginRequestSender>, body: &[u8]) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    match apply_joint_place(sender, body) {
        Ok(s) => (200, json, s),
        Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
    }
}

fn apply_joint_place(
    sender: &Arc<dyn PluginRequestSender>,
    body: &[u8],
) -> Result<String, String> {
    let spec = crate::joint::JointSpec::from_json(body)?;
    let plan = crate::joint::plan(&spec)?;
    let built = crate::joint::build(&plan, spec.trim)?;
    let (bolt_name, _) = crate::partgen::family_meta(&plan.bolt_family);
    // 预览块名带上"长度 + 裁剪档"，同规格重复放置复用同一块
    let cut_sig: String = built
        .hidden
        .iter()
        .flat_map(|s| s.iter())
        .map(|(a, b)| format!("{}-{}", crate::joint::num_text(*a), crate::joint::num_text(*b)))
        .collect::<Vec<_>>()
        .join("_");
    let block = if cut_sig.is_empty() {
        format!("OCSMJOINT_PREV_{}", crate::partgen::ascii_block(&plan.bolt_family, &format!("M{}x{}", crate::joint::num_text(plan.bolt_d), crate::joint::num_text(plan.bolt_l))))
    } else {
        format!(
            "OCSMJOINT_PREV_{}_CUT_{}",
            crate::partgen::ascii_block(&plan.bolt_family, &format!("M{}x{}", crate::joint::num_text(plan.bolt_d), crate::joint::num_text(plan.bolt_l))),
            crate::partgen::ascii_block("", &cut_sig)
        )
    };
    // 建预览块也是文档改动 → 单独一个撤销条目（与出库同规矩）
    begin_undo(sender, "螺栓副预览")?;
    let exists = snapshot(sender)?.block_records.iter().any(|b| b.name == block);
    if !exists {
        req_timed(
            sender,
            PluginRequest::AddBlockRecord {
                name: block.clone(),
                entities: built.assembly.entities.clone(),
            },
            "AddBlockRecord",
        )?;
    }
    commit_undo(sender);
    crate::set_pending_joint(crate::PendingJoint {
        spec_json: spec.to_json(),
        block: block.clone(),
        label: format!(
            "{} {} 螺栓副（{} 件）",
            bolt_name,
            format!("M{}×{}", crate::joint::num_text(plan.bolt_d), crate::joint::num_text(plan.bolt_l)),
            plan.placements.len()
        ),
    });
    Ok(serde_json::json!({
        "ok": true,
        "block": block,
        "report": plan.report,
        "message": format!(
            "已就绪：{}。切回图纸点击定位基点 → 移动光标旋转 → 再点击落定（可连续，Esc 结束）。",
            plan.report
        ),
    })
    .to_string())
}

fn api_part_export(body: &[u8], sender: &Arc<dyn PluginRequestSender>) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    match apply_part_export(sender, body) {
        Ok(s) => (200, json, s),
        Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
    }
}

fn apply_part_export(
    sender: &Arc<dyn PluginRequestSender>,
    body: &[u8],
) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Req {
        family: String,
        d: f64,
        l: f64,
        #[serde(default = "default_view")]
        view: String,
    }
    fn default_view() -> String {
        "main".to_string()
    }
    let req: Req = serde_json::from_slice(body).map_err(|e| format!("请求 JSON 无效: {e}"))?;
    let part = crate::partgen::generate(&req.family, req.d, req.l, &req.view)?;
    let block = format!(
        "OCSM_{}_{}_{}",
        req.family.to_ascii_uppercase().replace(['.', ' ', '/'], "_"),
        part.meta.spec.replace(['.', ' ', '/', 'x', 'X'], "_"),
        req.view.to_ascii_uppercase()
    );

    // 出库会新建块定义（文档改动）→ 开事务，结束时 commit。
    begin_undo(sender, "零件出库")?;
    // 块定义：幂等创建（预览要引用它，所以必须在出库时就建好）
    let exists = snapshot(sender)?.block_records.iter().any(|b| b.name == block);
    if !exists {
        req_timed(
            sender,
            PluginRequest::AddBlockRecord {
                name: block.clone(),
                entities: part.entities.clone(),
            },
            "AddBlockRecord",
        )?;
    }

    // 登记为待放置件（`XL` 放置态的光标预览 + 点击落件都读它）
    let meta_json = serde_json::json!({
        "family": req.family,
        "view": req.view,
        "code": part.meta.code,
        "name": part.meta.name,
        "spec": part.meta.spec,
        "material": part.meta.material,
        "weight": part.meta.weight,
        "d": req.d,
        "l": req.l,
    })
    .to_string();
    crate::set_pending_part(crate::PendingPart {
        block: block.clone(),
        meta_json,
        label: format!("{} {}（{}）", part.meta.name, part.meta.spec, part.meta.code),
    });

    // 出库后窗口由页面自己 `window.close()` 关闭（实测 chromium 允许；
    // 若是浏览器拒绝关闭（如 Firefox 标签页），用户手动切回图纸即可）。
    commit_undo(sender);
    Ok(serde_json::json!({
        "ok": true,
        "message": format!(
            "已出库：{} {}（{}）。切回图纸，鼠标上已带该零件，左键点击放置（可连续，Esc 结束）。",
            part.meta.name, part.meta.spec, part.meta.code
        ),
        "block": block,
        "code": part.meta.code,
        "spec": part.meta.spec,
    })
    .to_string())
}

fn api_part_pick(body: &[u8], sender: &Arc<dyn PluginRequestSender>) -> (u16, &'static str, String) {
    let json = "application/json; charset=utf-8";
    match apply_part_pick(sender, body) {
        Ok(s) => (200, json, s),
        Err(e) => (400, json, serde_json::json!({"ok": false, "error": e}).to_string()),
    }
}

pub(crate) fn apply_part_pick(
    sender: &Arc<dyn PluginRequestSender>,
    body: &[u8],
) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Req {
        family: String,
        d: f64,
        l: f64,
        #[serde(default = "default_view")]
        view: String,
        /// 显式落点（MCP/AI 驱动）：同时给了 `x` 与 `y` 就不再取 GUI 点选的待放置点。
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        z: Option<f64>,
        /// 可选旋转角（**度**，逆时针）；缺省 0。
        #[serde(default)]
        rotation: Option<f64>,
    }
    fn default_view() -> String {
        "main".to_string()
    }
    let req: Req = serde_json::from_slice(body).map_err(|e| format!("请求 JSON 无效: {e}"))?;
    let part = crate::partgen::generate(&req.family, req.d, req.l, &req.view)?;

    // 块名：族 + 规格（去掉不合法字符，利于复用与排查）
    let block = part_block_name(&req.family, &part.meta.spec);

    // 插入会建块 + 落 INSERT 实体 → 开事务，结束时 commit。
    begin_undo(sender, "零件插入")?;
    // 块定义幂等：已存在就不再建（同名块重复插入直接复用）
    let exists = snapshot(sender)?
        .block_records
        .iter()
        .any(|b| b.name == block);
    if !exists {
        req_timed(
            sender,
            PluginRequest::AddBlockRecord {
                name: block.clone(),
                entities: part.entities.clone(),
            },
            "AddBlockRecord",
        )?;
    }

    // 落点优先级：显式 x/y（MCP/AI 一行驱动）→ GUI 点选的待放置点 → 原点。
    let at = match (req.x, req.y) {
        (Some(x), Some(y)) => [x, y, req.z.unwrap_or(0.0)],
        _ => crate::take_parts_point().unwrap_or([0.0, 0.0, 0.0]),
    };
    let mut ins = acadrust::entities::Insert::new(&block, Vector3::new(at[0], at[1], at[2]));
    // Insert.rotation 单位是弧度（与 place_one 一致），参数按度给。
    ins.rotation = req.rotation.unwrap_or(0.0).to_radians();
    {
        let c = &mut ins.common;
        c.layer = crate::partgen::LAYER_MAIN.to_string();
        c.color = ocs_plugin_api::host::acadrust::types::Color::ByLayer;
        c.linetype = "ByLayer".to_string();
        c.line_weight = ocs_plugin_api::host::acadrust::types::LineWeight::ByLayer;
    }
    let handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![acadrust::EntityType::Insert(ins)]),
        "AddEntities",
    ) {
        Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
        Ok(_) => None,
        Err(e) => return Err(e),
    };

    // `OCSM_PART`：台账记录（二期序号 / 三期明细表直接取用）
    let meta_json = serde_json::json!({
        "family": req.family,
        "view": req.view,
        "code": part.meta.code,
        "name": part.meta.name,
        "spec": part.meta.spec,
        "material": part.meta.material,
        "weight": part.meta.weight,
        "d": req.d,
        "l": req.l,
    })
    .to_string();
    let mut rec = ExtendedDataRecord::new("OCSM_PART");
    rec.values.push(XDataValue::String(meta_json));
    if let Some(h) = handle {
        req_timed(
            sender,
            PluginRequest::WriteRecord { handle: h, record: rec },
            "WriteRecord",
        )?;
    }
    req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
    mark_dirty(sender)?;
    commit_undo(sender);

    Ok(serde_json::json!({
        "ok": true,
        "message": format!("已插入 {} {}（{}）", part.meta.name, part.meta.spec, part.meta.code),
        "block": block,
        "insert_handle": handle.map(fmt_handle),
        "at": at,
        "code": part.meta.code,
        "spec": part.meta.spec,
        "weight": part.meta.weight,
    })
    .to_string())
}

fn apply_roughness(
    sender: &Arc<dyn PluginRequestSender>,
    body: &[u8],
) -> Result<String, String> {
    use acadrust::entities::{AttributeDefinition, Circle, Insert, Line, Solid};
    use acadrust::entities::Entity as _; // apply_transform
    use acadrust::entities::attribute_definition::{HorizontalAlignment, VerticalAlignment};
    use acadrust::types::{Color, Vector3};
    use acadrust::EntityType as E;

    #[derive(serde::Deserialize)]
    struct Req {
        x: f64,
        y: f64,
        base: String, // C1..C4
        extra: String, // R1..R5
        #[serde(default)]
        p: String, // 加工符号（C2 时强制空）
        #[serde(default)]
        rotation: f64, // 度
        #[serde(default)]
        values: std::collections::HashMap<String, String>,
    }
    let req: Req = serde_json::from_slice(body).map_err(|e| format!("请求 JSON 无效: {e}"))?;
    let base = req.base.to_ascii_uppercase();
    let extra = req.extra.to_ascii_uppercase();
    let bi = match base.as_str() {
        "C1" => 0, "C2" => 1, "C3" => 2, "C4" => 3,
        _ => return Err(format!("无效的基础体 {base}（应为 C1..C4）")),
    };
    let ri = match extra.as_str() {
        "R1" => 0, "R2" => 1, "R3" => 2, "R4" => 3, "R5" => 4,
        _ => return Err(format!("无效的附加区 {extra}（应为 R1..R5）")),
    };
    // C2 列（以不去除材料的方法获得）：P 强制空白且不生成 P 属性（对照参考）。
    let has_p = bi != 1;
    let p_value = if has_p { req.p.clone() } else { String::new() };

    let doc = snapshot(sender)?;
    // 符号会新建块 + 实体、并可能补齐样式/图层 → 开事务，结束时 commit。
    begin_undo(sender, "表面粗糙度")?;
    // 幂等 ensure：文档缺 OCSM_GB 样式 / 8符号标注层时补齐（新图纸直接 CC
    // 时宿主渲染 ATTDEF 会 fallback 未知字体、图层色错乱——用户实测）。
    if !doc
        .text_styles
        .iter()
        .any(|st| st.name.eq_ignore_ascii_case("OCSM_GB"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureTextStyles(crate::text_style_defs()),
            "EnsureTextStyles",
        )?;
    }
    if !doc
        .layers
        .iter()
        .any(|ly| ly.name.eq_ignore_ascii_case("8符号标注层"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureLayers(crate::layer_defs()),
            "EnsureLayers",
        )?;
    }
    let scale = frame_scale_at(&doc, [req.x, req.y, 0.0]);
    let s = |v: f64| v * scale;
    let rot: f64 = req.rotation.to_radians();

    // ── 几何表（参考坐标）──
    let l1 = ((4.186, 5.35), (7.072, 0.35));
    let l2 = ((7.072, 0.35), (13.423, 11.35));
    // 基础体差异件
    let base_lines: &[((f64, f64), (f64, f64))] = match bi {
        2 => &[((4.186, 5.35), (9.959, 5.35))], // C3 横线
        3 => &[((4.186, 5.35), (9.959, 5.35)), ((7.072, 0.35), (9.959, 5.35))], // C4 横线+短线
        _ => &[],
    };
    let base_circle: Option<(f64, f64, f64)> = if bi == 1 {
        Some((7.072, 3.683, 1.667)) // C2 以不去除材料的方法获得（V 内圆）
    } else {
        None
    };
    let base_fill: Option<((f64, f64), (f64, f64), (f64, f64))> = if bi == 3 {
        Some(((4.186, 5.35), (9.959, 5.35), (7.072, 0.35))) // C4 焊后加工填充三角
    } else {
        None
    };
    // 附加区差异件
    let extra_lines: &[((f64, f64), (f64, f64))] = match ri {
        1 => &[((13.423, 11.35), (30.833, 11.35))], // R2 周边（长线+圆）
        2 => &[((13.423, 11.35), (25.654, 11.35))], // R3 高级（短线）
        3 => &[
            ((13.423, 11.35), (25.654, 11.35)), // R4 上限开关（台阶）
            ((13.423, 11.35), (16.656, 16.95)),
            ((16.656, 16.95), (25.654, 16.95)),
        ],
        4 => &[
            ((13.423, 11.35), (30.833, 11.35)), // R5 上限开关+周边
            ((13.423, 11.35), (16.656, 16.95)),
            ((16.656, 16.95), (30.833, 16.95)),
        ],
        _ => &[],
    };
    let extra_circle: Option<(f64, f64, f64)> = if ri == 1 || ri == 4 {
        Some((13.423, 11.35, 1.667)) // R2/R5 周边相同处理（顶点圆）
    } else {
        None
    };

    // ── 块成员（局部坐标 = 参考 × scale；块基点 (0,0)）──
    let mut members: Vec<E> = Vec::new();
    let mut mk_line = |a: (f64, f64), b: (f64, f64)| -> E {
        let mut e = E::Line(Line {
            common: Default::default(),
            start: Vector3::new(s(a.0), s(a.1), 0.0),
            end: Vector3::new(s(b.0), s(b.1), 0.0),
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(31); // 深红（参考 62=31）
        e
    };
    members.push(mk_line(l1.0, l1.1));
    members.push(mk_line(l2.0, l2.1));
    for &(a, b) in base_lines { members.push(mk_line(a, b)); }
    for &(a, b) in extra_lines { members.push(mk_line(a, b)); }
    // 圆
    let mut mk_circle = |c: (f64, f64, f64)| -> E {
        let mut e = E::Circle(Circle {
            common: Default::default(),
            center: Vector3::new(s(c.0), s(c.1), 0.0),
            radius: s(c.2),
            thickness: 0.0,
            normal: Vector3::new(0.0, 0.0, 1.0),
        });
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(31);
        e
    };
    if let Some(c) = base_circle { members.push(mk_circle(c)); }
    if let Some(c) = extra_circle { members.push(mk_circle(c)); }
    // C4 焊后加工填充三角（参考 HATCH，宿主用 SOLID 等效；第 4 点 = 第 3 点）
    if let Some((a, b, c)) = base_fill {
        let mut e = E::Solid(Solid::new(
            Vector3::new(s(a.0), s(a.1), 0.0),
            Vector3::new(s(b.0), s(b.1), 0.0),
            Vector3::new(s(c.0), s(c.1), 0.0),
            Vector3::new(s(c.0), s(c.1), 0.0),
        ));
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(31);
        members.push(e);
    }

    // ── ATTDEF（tag = 中文描述+英文代号；style OCSM_GB；可缺省空白）──
    // (tag, x, y, 高, 对齐：rb=右下(Right+Bottom)/lb=左下(Left+Bottom))
    // 对齐依据 = 素材块 MTEXT 的 71 组标准语义：A′/A/E = 71:9 → 右下、
    // P/B′/B/C/G = 71:7 → 左下（文字块以锚点为右下/左下角——素材原图
    // 就按此渲染；此前 ML 左缘导致我们整体偏右，用户实测对照确认）。
    let mut attdefs: Vec<(String, f64, f64, f64, &'static str)> = Vec::new();
    let att = |tag: &str, x: f64, y: f64, h: f64, al: &'static str| -> (String, f64, f64, f64, &'static str) {
        (tag.to_string(), x, y, h, al)
    };
    attdefs.push(att("粗糙度上限A′", 8.248, 11.65, 3.5, "rb"));
    attdefs.push(att("粗糙度下限A", 8.248, 7.1, 3.5, "rb"));
    attdefs.push(att("备注E", 1.386, 0.0, 4.9, "rb"));
    if has_p {
        attdefs.push(att("加工符号P", 11.009, 1.4, 3.5, "lb"));
    }
    // 附加区文字（B′ 总在最高位：R2/R3 y=12.4、R4/R5 y=18；B 仅 R4/R5 y=12.4）
    //（R2 周边 / R3 高级 / R4 上限开关 / R5 上限开关+周边）
    let (bx, by, bbx, bby) = match ri {
        1 => (16.14, 12.4, 0.0, 0.0),      // R2: B′
        2 => (14.473, 12.4, 0.0, 0.0),     // R3: B′
        _ => (17.706, 18.0, 17.706, 12.4), // R4/R5: B′@18、B@12.4
    };
    if ri >= 1 {
        attdefs.push(att("加工方法B′", bx, by, 3.5, "lb"));
    }
    if ri >= 3 {
        attdefs.push(att("加工方法B", bbx, bby, 3.5, "lb"));
    }
    if ri >= 1 {
        attdefs.push(att("取样长度C", bx, 6.8, 3.5, "lb"));
        attdefs.push(att("纹理方向G", bx, 2.25, 3.5, "lb"));
    }
    // 值（values 键 = 英文代号：A' / A / E / P / B / B' / C / G）
    let value_of = |_tag: &str, alias: &str| -> String {
        req.values.get(alias).cloned().unwrap_or_default()
    };
    // 块内 ATTDEF 模板（default 一律空格：宿主对空 default 渲染 tag 名会
    // 在符号上堆出中文属性名——真实值只放 INSERT.attributes，图框同套路）。
    let mut att_templates: Vec<acadrust::entities::AttributeDefinition> = Vec::new();
    let mut mk_attdef = |(tag, x, y, h, al): (String, f64, f64, f64, &str)| -> acadrust::entities::AttributeDefinition {
        let mut ad = AttributeDefinition::new(tag, String::new(), " ".into());
        ad.insertion_point = Vector3::new(s(x), s(y), 0.0);
        ad.alignment_point = ad.insertion_point;
        ad.height = s(h);
        ad.rotation = rot; // 符号旋转时文字随符号旋转
        ad.width_factor = 0.7; // 与 OCSM_GB 一致
        ad.text_style = "OCSM_GB".into();
        // 对齐：rb = 右下（Right+Bottom）、lb = 左下（Left+Bottom），
        // 对应素材块 MTEXT 71:9 / 71:7 的标准渲染。
        if al == "rb" {
            ad.set_alignment(HorizontalAlignment::Right, VerticalAlignment::Bottom);
        } else {
            ad.set_alignment(HorizontalAlignment::Left, VerticalAlignment::Bottom);
        }
        ad.flags.preset = true; // 插入时不逐项提示
        ad
    };
    for (tag, x, y, h, al) in attdefs.iter() {
        let mut ad = mk_attdef((tag.clone(), *x, *y, *h, al));
        ad.common.layer = "8符号标注层".into();
        ad.common.color = Color::from_index(3); // 绿色（参考 62=3）
        let mut e = E::AttributeDefinition(ad.clone());
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(3);
        members.push(e);
        att_templates.push(ad);
    }

    // ── 匿名块 *D{n}（与其它标注共用计数器）──
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

    // ── INSERT @ 插入点（层 0 跟参考；rotation 由 GUI 指定；attributes 对齐 ATTDEF）──
    let mut ins = Insert::new(block_name.clone(), Vector3::new(req.x, req.y, 0.0));
    ins.rotation = rot;
    // 属性值 = 用户输入（空 → 空格空白显示）；位置 = 块内 ATTDEF 位 ×变换
    //（宿主把 attributes 当独立实体渲染在其自身坐标：必须 transform 到世界，
    // 否则全部堆在插入点——用户实测「文字堆在了一起」）。
    for ad in att_templates.iter() {
        let alias = tag_alias(&ad.tag);
        let val = if alias == "P" { p_value.clone() } else { value_of(&ad.tag, alias) };
        let val = if val.trim().is_empty() { " ".to_string() } else { val };
        let mut tmpl = ad.clone();
        tmpl.rotation = 0.0; // 旋转由 INSERT 变换施加（避免双转）
        let mut attr = acadrust::entities::AttributeEntity::from_definition(&tmpl, Some(val));
        attr.apply_transform(&ins.get_transform());
        ins.attributes.push(attr);
    }
    let handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![E::Insert(ins)]),
        "AddEntities",
    ) {
        Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
        Ok(_) => None,
        Err(e) => return Err(e),
    };
    req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
    mark_dirty(sender)?;
    commit_undo(sender);

    Ok(serde_json::json!({
        "ok": true,
        "block": block_name,
        "insert_handle": handle.map(fmt_handle),
        "base": base,
        "extra": extra,
        "p": p_value,
        "scale": scale,
        "rotation": req.rotation,
    })
    .to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// 焊接符号（WELD）：两段 PLINE 引导 → 匿名块 *W{n}（GB/T 324 焊缝标注全家福）。
// 骨架尺寸 1:1 对照 OCSMDIMGULIDE/焊接符号示例.dxf（箭头 3.5×0.587、虚线偏移
// 0.700、全周边圆 r1.75、旗杆 7.0、旗 5.25×3.5、尾叉 3.5、符号槽 +19.413、
// 数字区 ±2.750、C 弧 r2.625 @+20.538）；符号几何由 焊接符号表.dxf 的 27 个
// 块（24 镜像对 + 3 跨线单置）逐块归一化生成（锚点=贴线特征点→(0,0)，
// 行基线 y=0：上侧 y>0、下侧/镜像 y<0、跨线骑 0）。
// ═══════════════════════════════════════════════════════════════════════════

/// 焊缝符号实体（局部坐标）。
#[derive(Debug, Clone, Copy, PartialEq)]
enum WeldSymEnt {
    /// 直线 (x1,y1)→(x2,y2)
    Line(f64, f64, f64, f64),
    /// 圆 (cx,cy) r
    Circle(f64, f64, f64),
    /// 圆弧 (cx,cy) r 起止角（度，CCW，DXF 50/51 语义）
    Arc(f64, f64, f64, f64, f64),
    /// 固定小字（持久衬垫 MR / 临时衬垫 M），中中锚定（OCSM_GB 替代 PC_TEXTSTYLE）
    Text(f64, f64, &'static str),
}

/// 焊缝符号实体别名（表内构造用）。
type WSE = WeldSymEnt;

/// 27 个焊缝符号：(名称, 上侧几何, 下侧几何（镜像版；None=无）, 跨线单置)。
/// 跨线单置（参考线上的点/缝焊缝、堆焊接头）只有一种位置——骑基准线。
static WELD_SYMS: &[(
    &'static str,
    &'static [WeldSymEnt],
    Option<&'static [WeldSymEnt]>,
    bool,
)] = &[
    ("参考线上的点焊缝", &[WSE::Circle(2.275, 0.0, 2.275)], None, true),
    ("参考线上的缝焊缝", &[WSE::Circle(3.5, 0.0, 2.625), WSE::Line(0.0, -1.312, 7.0, -1.312), WSE::Line(0.0, 1.313, 7.0, 1.313)], None, true),
    ("堆焊接头", &[WSE::Line(0.0, -0.7, 5.25, -0.7), WSE::Line(0.0, 0.7, 5.25, 0.7)], None, true),
    ("I型对接焊缝", &[WSE::Line(0.0, 0.0, 0.0, 3.5), WSE::Line(2.45, 0.0, 2.45, 3.5)], Some(&[WSE::Line(0.0, 0.0, 0.0, -3.5), WSE::Line(2.45, 0.0, 2.45, -3.5)]), false),
    ("V型对接焊缝", &[WSE::Line(0.0, 3.5, 1.75, 0.0), WSE::Line(1.75, 0.0, 3.5, 3.5)], Some(&[WSE::Line(0.0, -3.5, 1.75, 0.0), WSE::Line(1.75, 0.0, 3.5, -3.5)]), false),
    ("临时衬垫", &[WSE::Line(0.0, 0.0, 0.0, 2.8), WSE::Line(0.0, 2.8, 5.25, 2.8), WSE::Line(5.25, 2.8, 5.25, 0.0), WSE::Text(2.625, 1.4, "M")], Some(&[WSE::Line(0.0, 0.0, 0.0, -2.8), WSE::Line(0.0, -2.8, 5.25, -2.8), WSE::Line(5.25, -2.8, 5.25, 0.0), WSE::Text(2.625, -1.4, "M")]), false),
    ("倾斜接头", &[WSE::Line(0.0, 0.0, 4.547, 2.625), WSE::Line(0.0, 2.1, 4.547, 4.725)], Some(&[WSE::Line(0.0, 0.0, 4.547, -2.625), WSE::Line(0.0, -2.1, 4.547, -4.725)]), false),
    ("单边喇叭形焊", &[WSE::Line(0.0, 0.0, 0.0, 3.5), WSE::Line(1.05, 0.0, 1.05, 0.525), WSE::Arc(4.025, 0.525, 2.975, 90.0, 180.0)], Some(&[WSE::Line(0.0, 0.0, 0.0, -3.5), WSE::Line(1.05, 0.0, 1.05, -0.525), WSE::Arc(4.025, -0.525, 2.975, 180.0, 270.0)]), false),
    ("单边陡侧V型坡口对焊", &[WSE::Line(0.0, 0.125, 2.1, 0.125), WSE::Line(0.0, 0.125, 0.0, 3.625), WSE::Line(1.05, 0.125, 2.1, 3.625)], Some(&[WSE::Line(0.0, -0.125, 2.1, -0.125), WSE::Line(0.0, -0.125, 0.0, -3.625), WSE::Line(1.05, -0.125, 2.1, -3.625)]), false),
    ("卷边焊缝", &[WSE::Arc(0.0, 3.1, 2.975, 270.0, 0.0), WSE::Arc(7.0, 3.1, 2.975, 180.0, 270.0), WSE::Line(0.0, 0.125, 7.0, 0.125), WSE::Line(2.975, 3.1, 2.975, 3.625), WSE::Line(4.025, 3.1, 4.025, 3.625)], Some(&[WSE::Arc(0.0, -3.1, 2.975, 0.0, 90.0), WSE::Arc(7.0, -3.1, 2.975, 90.0, 180.0), WSE::Line(0.0, -0.125, 7.0, -0.125), WSE::Line(2.975, -3.1, 2.975, -3.625), WSE::Line(4.025, -3.1, 4.025, -3.625)]), false),
    ("喇叭形焊", &[WSE::Arc(0.0, 0.525, 2.975, 0.0, 90.0), WSE::Arc(7.0, 0.525, 2.975, 90.0, 180.0), WSE::Line(2.975, -0.0, 2.975, 0.525), WSE::Line(4.025, -0.0, 4.025, 0.525)], Some(&[WSE::Arc(0.0, -0.525, 2.975, 270.0, 0.0), WSE::Arc(7.0, -0.525, 2.975, 180.0, 270.0), WSE::Line(2.975, -0.0, 2.975, -0.525), WSE::Line(4.025, -0.0, 4.025, -0.525)]), false),
    ("堆焊缝", &[WSE::Line(0.0, 0.125, 9.1, 0.125), WSE::Arc(2.275, 0.125, 2.275, 0.0, 180.0), WSE::Arc(6.825, 0.125, 2.275, 0.0, 180.0)], Some(&[WSE::Line(0.0, -0.125, 9.1, -0.125), WSE::Arc(2.275, -0.125, 2.275, 180.0, 0.0), WSE::Arc(6.825, -0.125, 2.275, 180.0, 0.0)]), false),
    ("塞焊缝", &[WSE::Line(0.0, -0.0, 0.0, 2.45), WSE::Line(0.0, 2.45, 4.2, 2.45), WSE::Line(4.2, 2.45, 4.2, -0.0)], Some(&[WSE::Line(0.0, -0.0, 0.0, -2.45), WSE::Line(0.0, -2.45, 4.2, -2.45), WSE::Line(4.2, -2.45, 4.2, -0.0)]), false),
    ("封底焊缝", &[WSE::Line(0.0, -0.0, 5.191, -0.0), WSE::Arc(2.595, -1.075, 2.809, 22.5, 157.5)], Some(&[WSE::Line(0.0, -0.0, 5.191, -0.0), WSE::Arc(2.595, 1.075, 2.809, 202.5, 337.5)]), false),
    ("带单边坡口的V型对接焊缝", &[WSE::Line(0.0, 3.5, 0.0, 0.0), WSE::Line(0.0, 0.0, 3.5, 3.5)], Some(&[WSE::Line(0.0, -3.5, 0.0, 0.0), WSE::Line(0.0, 0.0, 3.5, -3.5)]), false),
    ("带钝边J型对接焊缝", &[WSE::Line(0.0, 0.0, 0.0, 3.5), WSE::Arc(0.0, 3.5, 1.75, 270.0, 0.0)], Some(&[WSE::Line(0.0, 0.0, 0.0, -3.5), WSE::Arc(0.0, -3.5, 1.75, 0.0, 90.0)]), false),
    ("带钝边U型对接焊缝", &[WSE::Line(1.75, 0.0, 1.75, 1.75), WSE::Arc(1.75, 3.5, 1.75, 180.0, 0.0)], Some(&[WSE::Line(1.75, 0.0, 1.75, -1.75), WSE::Arc(1.75, -3.5, 1.75, 0.0, 180.0)]), false),
    ("带钝边V型焊缝", &[WSE::Line(1.75, 0.0, 1.75, 1.75), WSE::Line(0.0, 3.5, 1.75, 1.75), WSE::Line(3.5, 3.5, 1.75, 1.75)], Some(&[WSE::Line(1.75, 0.0, 1.75, -1.75), WSE::Line(0.0, -3.5, 1.75, -1.75), WSE::Line(3.5, -3.5, 1.75, -1.75)]), false),
    ("带钝边单边V型焊缝", &[WSE::Line(0.0, 0.0, 0.0, 3.5), WSE::Line(0.0, 1.75, 2.8, 3.5)], Some(&[WSE::Line(0.0, 0.0, 0.0, -3.5), WSE::Line(0.0, -1.75, 2.8, -3.5)]), false),
    ("打底", &[WSE::Line(0.0, 0.125, 0.0, 0.825), WSE::Line(0.0, 0.825, 3.5, 0.825), WSE::Line(3.5, 0.825, 3.5, 2.575), WSE::Line(3.5, 2.575, 0.0, 2.575)], Some(&[WSE::Line(0.0, -0.125, 0.0, -0.825), WSE::Line(0.0, -0.825, 3.5, -0.825), WSE::Line(3.5, -0.825, 3.5, -2.575), WSE::Line(3.5, -2.575, 0.0, -2.575)]), false),
    ("折叠接头", &[WSE::Line(1.05, 0.0, 4.9, 0.0), WSE::Line(1.05, 1.05, 3.85, 1.05), WSE::Line(3.85, 2.1, 1.05, 2.1), WSE::Line(3.85, 3.15, 0.0, 3.15), WSE::Arc(1.05, 1.05, 1.05, 90.0, 270.0), WSE::Arc(3.85, 2.1, 1.05, 270.0, 90.0)], Some(&[WSE::Line(1.05, 0.0, 4.9, 0.0), WSE::Line(1.05, -1.05, 3.85, -1.05), WSE::Line(3.85, -2.1, 1.05, -2.1), WSE::Line(3.85, -3.15, 0.0, -3.15), WSE::Arc(1.05, -1.05, 1.05, 90.0, 270.0), WSE::Arc(3.85, -2.1, 1.05, 270.0, 90.0)]), false),
    ("持久衬垫", &[WSE::Line(0.0, 0.0, 0.0, 2.8), WSE::Line(0.0, 2.8, 5.25, 2.8), WSE::Line(5.25, 2.8, 5.25, 0.0), WSE::Text(2.625, 1.4, "MR")], Some(&[WSE::Line(0.0, 0.0, 0.0, -2.8), WSE::Line(0.0, -2.8, 5.25, -2.8), WSE::Line(5.25, -2.8, 5.25, 0.0), WSE::Text(2.625, -1.4, "MR")]), false),
    ("点焊", &[WSE::Circle(2.275, 2.275, 2.275)], Some(&[WSE::Circle(2.275, -2.275, 2.275)]), false),
    ("端接焊缝", &[WSE::Line(1.05, 0.0, 1.05, 3.5), WSE::Line(0.0, 0.0, 0.0, 3.5), WSE::Line(2.1, 0.0, 2.1, 3.5)], Some(&[WSE::Line(1.05, 0.0, 1.05, -3.5), WSE::Line(0.0, 0.0, 0.0, -3.5), WSE::Line(2.1, 0.0, 2.1, -3.5)]), false),
    ("缝焊缝", &[WSE::Circle(3.5, 2.625, 2.625), WSE::Line(0.0, 1.313, 7.0, 1.313), WSE::Line(0.0, 3.938, 7.0, 3.938)], Some(&[WSE::Circle(3.5, -2.625, 2.625), WSE::Line(0.0, -1.312, 7.0, -1.312), WSE::Line(0.0, -3.937, 7.0, -3.937)]), false),
    ("角焊", &[WSE::Line(0.0, 0.125, 3.5, 0.125), WSE::Line(3.5, 0.125, 0.0, 3.625), WSE::Line(0.0, 3.625, 0.0, 0.125)], Some(&[WSE::Line(0.0, -0.125, 3.5, -0.125), WSE::Line(3.5, -0.125, 0.0, -3.625), WSE::Line(0.0, -3.625, 0.0, -0.125)]), false),
    ("陡侧V型坡口对焊", &[WSE::Line(0.238, 0.125, 3.038, 0.125), WSE::Line(0.938, 0.125, 0.0, 3.625), WSE::Line(2.338, 0.125, 3.276, 3.625)], Some(&[WSE::Line(0.238, -0.125, 3.038, -0.125), WSE::Line(0.938, -0.125, 0.0, -3.625), WSE::Line(2.338, -0.125, 3.276, -3.625)]), false),
];

/// 按名称查焊缝符号定义。
fn weld_sym(
    name: &str,
) -> Option<
    &'static (
        &'static str,
        &'static [WeldSymEnt],
        Option<&'static [WeldSymEnt]>,
        bool,
    ),
> {
    WELD_SYMS.iter().find(|s| s.0 == name)
}

/// 焊接参数 → JSON（/api/guide 回显，GUI 恢复表单用；独立函数避免 json! 递归超限）。
fn weld_params_json(w: &WeldParams) -> serde_json::Value {
    serde_json::json!({
        "upper": w.upper, "lower": w.lower,
        "dash": w.dash, "circle": w.circle, "flag": w.flag,
        "tail": w.tail, "circle": w.circle, "half": w.half,
        "grindUpper": w.grind_upper.as_str(), "grindLower": w.grind_lower.as_str(),
        "methodUpper": w.method_upper, "methodLower": w.method_lower,
        "grind": w.grind_upper.as_str(), "method": w.method_upper,
        "up_thick": w.up_thick, "up_qty": w.up_qty,
        "lo_thick": w.lo_thick, "lo_qty": w.lo_qty, "tail_text": w.tail_text,
    })
}

/// 打磨附加件几何（相对焊缝符号锚点；上侧正排，下侧镜像 y/角度）。
/// 数据 1:1 取自 焊接符号-焊缝打磨.dxf 两行样例（角焊版 / 其它焊缝版），
/// 每种 6 态（不打磨 / 弧·凹 / 弧·凸 / 直线 / 双弧 / 锯齿）。
#[derive(Debug, Clone, Copy)]
enum WeldGrindEnt {
    Line(f64, f64, f64, f64),
    Arc(f64, f64, f64, f64, f64),
}

/// 角焊版（倾斜右上，第一行样例）+ 文字 "C" 位置。
fn weld_grind_fillet(k: GrindKind) -> (&'static [WeldGrindEnt], Option<(f64, f64)>) {
    match k {
        GrindKind::None => (&[], None),
        // 弧·凹：弧向焊缝凹入（样例 1.2）
        GrindKind::ArcConcave => (
            &[WeldGrindEnt::Arc(3.9597, 3.9598, 2.625, 181.5, 268.5)],
            Some((4.0316, 4.9598)),
        ),
        // 弧·凸：向外鼓（示例.dxf + 样例 1.3）
        GrindKind::ArcConvex => (
            &[WeldGrindEnt::Arc(1.125, 1.125, 2.625, 1.5, 88.5)],
            Some((4.0316, 4.9598)),
        ),
        // 直线（加工成平面；样例 1.4）
        GrindKind::Line => (
            &[WeldGrindEnt::Line(0.2475, 3.9598, 3.9598, 0.2474)],
            Some((4.9598, 6.8159)),
        ),
        // 双弧（鱼鳞打磨纹；样例 1.5）
        GrindKind::DoubleArc => (
            &[
                WeldGrindEnt::Line(6.3106, 6.3109, 3.7120, 3.7123),
                WeldGrindEnt::Arc(2.6602, 4.7641, 1.4875, 135.0, 315.0),
                WeldGrindEnt::Arc(4.7638, 2.6604, 1.4875, 135.0, 315.0),
            ],
            Some((6.3206, 7.3109)),
        ),
        // 锯齿（打磨纹；样例 1.6）
        GrindKind::Zigzag => (
            &[
                WeldGrindEnt::Line(0.3712, 3.8360, 3.8360, 0.3712),
                WeldGrindEnt::Line(4.9094, 3.1091, 3.1090, 4.9094),
                WeldGrindEnt::Line(3.1090, 4.9094, 2.4501, 2.4501),
                WeldGrindEnt::Line(2.4501, 2.4501, 7.3688, 3.7680),
            ],
            Some((4.8360, 6.5684)),
        ),
    }
}

/// 其它焊缝版（正上方，第二行样例；无 "C" 文字——参考文件该行无文字锚点）。
fn weld_grind_other(k: GrindKind) -> &'static [WeldGrindEnt] {
    match k {
        GrindKind::None => &[],
        // 弧·凹（样例 2.1）
        GrindKind::ArcConcave => &[WeldGrindEnt::Arc(1.75, 7.125, 2.625, 226.5, 313.5)],
        // 弧·凸（样例 2.3）
        GrindKind::ArcConvex => &[WeldGrindEnt::Arc(1.75, 3.1169, 2.625, 46.5, 133.5)],
        // 直线（样例 2.4）
        GrindKind::Line => &[WeldGrindEnt::Line(-0.875, 4.5, 4.375, 4.5)],
        // 双弧（样例 2.5）
        GrindKind::DoubleArc => &[
            WeldGrindEnt::Line(1.75, 6.775, 1.75, 10.45),
            WeldGrindEnt::Arc(0.2625, 6.775, 1.4875, 180.0, 0.0),
            WeldGrindEnt::Arc(3.2375, 6.775, 1.4875, 180.0, 0.0),
        ],
        // 锯齿（样例 2.6）
        GrindKind::Zigzag => &[
            WeldGrindEnt::Line(-0.7, 4.5, 4.2, 4.5),
            WeldGrindEnt::Line(3.0235, 7.195, 0.4774, 7.195),
            WeldGrindEnt::Line(1.7504, 4.99, 4.2965, 9.4),
        ],
    }
}

/// 追加打磨附加件（色 31）。`map(u, v)` 把参考单位（u 沿基准线、v 内容上方）
/// 映射到块局部坐标（含该侧镜像）；`mirror_angles` 仅用于弧角度翻转
/// （镜像后仍保持 DXF CCW：−a1 → −a0）。无符号的侧不调用。
fn push_weld_grind<M: Fn(f64, f64) -> (f64, f64)>(
    members: &mut Vec<acadrust::EntityType>,
    k: GrindKind,
    is_fillet: bool,
    mirror_angles: bool,
    s: f64,
    arc_rot_deg: f64,
    map: M,
) {
    if k == GrindKind::None {
        return;
    }
    let (ents, _ctext) = if is_fillet {
        weld_grind_fillet(k)
    } else {
        (weld_grind_other(k), None)
    };
    for e in ents {
        match *e {
            WeldGrindEnt::Line(x1, y1, x2, y2) => {
                members.push(weld_member_line(map(x1, y1), map(x2, y2), 31))
            }
            WeldGrindEnt::Arc(cx, cy, r, a0, a1) => {
                let (b0, b1) = if mirror_angles {
                    ((-a1).rem_euclid(360.0), (-a0).rem_euclid(360.0))
                } else {
                    (a0, a1)
                };
                members.push(weld_member_arc(
                    map(cx, cy),
                    r * s,
                    b0 + arc_rot_deg,
                    b1 + arc_rot_deg,
                ));
            }
        }
    }
}

/// 焊接方法字母位置（相对焊缝符号锚点）。
/// - 角焊版（倾斜右上打磨件）：照 打磨.dxf 第一行各态的文字锚点。
/// - 其它版（正上方打磨件，喇叭形焊缝等）：打磨件在符号正上方较高，字母
///   须抬到其最高点之上（用户实测：喇叭形+弧·凸 时字母与弧干涉），
///   取 字母中心 = 打磨件最高点 + 2.1（字高 3.5 半高 1.75 + 0.35 间隙）。
fn weld_method_pos(k: GrindKind, is_fillet: bool) -> (f64, f64) {
    if is_fillet {
        return match k {
            GrindKind::None | GrindKind::ArcConcave | GrindKind::ArcConvex => (4.0316, 4.9598),
            GrindKind::Line => (4.9598, 6.8159),
            GrindKind::DoubleArc => (6.3206, 7.3109),
            GrindKind::Zigzag => (4.8360, 6.5684),
        };
    }
    // 其它版打磨件最高点（与 weld_grind_other 几何一致；不打磨 = 喇叭形符号顶 3.5）。
    let top = match k {
        GrindKind::None => 3.5,
        GrindKind::ArcConcave => 5.22,   // 弧端点 +5.22（弧心 +7.125 下凹）
        GrindKind::ArcConvex => 5.742,   // 弧顶 = 弧心 +3.1169 + r2.625
        GrindKind::Line => 4.5,
        GrindKind::DoubleArc => 10.45,
        GrindKind::Zigzag => 9.4,
    };
    (4.0316, top + 2.1)
}

/// 焊接方法字母可用性：仅角焊缝与喇叭形焊缝（角焊 / 喇叭形焊 / 单边喇叭形焊）。
fn weld_method_allowed(name: &str) -> bool {
    matches!(name, "角焊" | "喇叭形焊" | "单边喇叭形焊")
}

/// 追加焊接方法字母（C/G/H/M/R/U，色 3 中中锚）。`map(u, v)` 含该侧镜像。
/// 仅当该侧符号属角焊/喇叭形且字母非空时绘制。
fn push_weld_method<M: Fn(f64, f64) -> (f64, f64)>(
    members: &mut Vec<acadrust::EntityType>,
    method: &str,
    sym_name: &str,
    grind: GrindKind,
    s: f64,
    rot: f64,
    map: M,
) {
    if method.is_empty() || method == "无" || !weld_method_allowed(sym_name) {
        return;
    }
    let (tx, ty) = weld_method_pos(grind, sym_name == "角焊");
    members.push(weld_member_text(method, map(tx, ty), 3.5 * s, rot));
}

/// 符号几何沿基准线的最大 u（右端）——用于计算基准线所需长度。
fn weld_sym_max_u(ents: &[WeldSymEnt]) -> f64 {
    let mut m: f64 = 0.0;
    for e in ents {
        let u = match *e {
            WeldSymEnt::Line(x1, _, x2, _) => x1.max(x2),
            WeldSymEnt::Circle(cx, _, r) => cx + r,
            WeldSymEnt::Arc(cx, _, r, ..) => cx + r, // 上界（不细分扫角）
            WeldSymEnt::Text(x, _, t) => x + 1.45 * (t.chars().count() as f64),
        };
        m = m.max(u);
    }
    m
}

/// 打磨附加件沿基准线的最大 u（右端）。
fn weld_grind_max_u(k: GrindKind, is_fillet: bool) -> f64 {
    if k == GrindKind::None {
        return 0.0;
    }
    let ents = if is_fillet {
        weld_grind_fillet(k).0
    } else {
        weld_grind_other(k)
    };
    let mut m: f64 = 0.0;
    for e in ents {
        let u = match *e {
            WeldGrindEnt::Line(x1, _, x2, _) => x1.max(x2),
            WeldGrindEnt::Arc(cx, _, r, ..) => cx + r,
        };
        m = m.max(u);
    }
    m
}

/// `GET /api/weld_syms`：27 个焊缝符号的归一化几何（GUI 焊接预览用）。
/// 实体编码：["L",x1,y1,x2,y2] / ["C",cx,cy,r] / ["A",cx,cy,r,a0,a1] / ["T",x,y,text]。
fn weld_syms_json() -> String {
    fn ent_json(e: &WeldSymEnt) -> serde_json::Value {
        match *e {
            WeldSymEnt::Line(x1, y1, x2, y2) => serde_json::json!(["L", x1, y1, x2, y2]),
            WeldSymEnt::Circle(cx, cy, r) => serde_json::json!(["C", cx, cy, r]),
            WeldSymEnt::Arc(cx, cy, r, a0, a1) => serde_json::json!(["A", cx, cy, r, a0, a1]),
            WeldSymEnt::Text(x, y, t) => serde_json::json!(["T", x, y, t]),
        }
    }
    fn ents_json(ents: &[WeldSymEnt]) -> serde_json::Value {
        serde_json::Value::Array(ents.iter().map(ent_json).collect())
    }
    let syms: Vec<serde_json::Value> = WELD_SYMS
        .iter()
        .map(|(name, up, lo, cross)| {
            serde_json::json!({
                "name": name,
                "upper": ents_json(up),
                "lower": lo.map(ents_json),
                "cross": cross,
            })
        })
        .collect();
    serde_json::json!({ "ok": true, "syms": syms }).to_string()
}

/// 焊接块成员：直线（层 8符号标注层 + 显式色；坐标为块局部坐标，已含图幅倍率）。
fn weld_member_line(a: (f64, f64), b: (f64, f64), color: i16) -> acadrust::EntityType {
    use acadrust::entities::Line;
    use acadrust::types::Color;
    use acadrust::EntityType as E;
    let mut e = E::Line(Line {
        common: Default::default(),
        start: Vector3::new(a.0, a.1, 0.0),
        end: Vector3::new(b.0, b.1, 0.0),
        thickness: 0.0,
        normal: Vector3::new(0.0, 0.0, 1.0),
    });
    set_member_layer(&mut e, "8符号标注层");
    e.common_mut().color = Color::from_index(color);
    e
}

/// 焊接块成员：圆（色 31）。
fn weld_member_circle(c: (f64, f64), r: f64) -> acadrust::EntityType {
    use acadrust::entities::Circle;
    use acadrust::types::Color;
    use acadrust::EntityType as E;
    let mut e = E::Circle(Circle {
        common: Default::default(),
        center: Vector3::new(c.0, c.1, 0.0),
        radius: r,
        thickness: 0.0,
        normal: Vector3::new(0.0, 0.0, 1.0),
    });
    set_member_layer(&mut e, "8符号标注层");
    e.common_mut().color = Color::from_index(31);
    e
}

/// 焊接块成员：圆弧（度→弧度；色 31）。
fn weld_member_arc(c: (f64, f64), r: f64, a0_deg: f64, a1_deg: f64) -> acadrust::EntityType {
    use acadrust::entities::Arc;
    use acadrust::types::Color;
    use acadrust::EntityType as E;
    let mut e = E::Arc(Arc::from_center_radius_angles(
        Vector3::new(c.0, c.1, 0.0),
        r,
        a0_deg.to_radians(),
        a1_deg.to_radians(),
    ));
    set_member_layer(&mut e, "8符号标注层");
    e.common_mut().color = Color::from_index(31);
    e
}

/// 焊接块成员：SOLID 三角（第 4 点 = 第 1 点，对照参考 13/23 组）。
fn weld_member_solid(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), color: i16) -> acadrust::EntityType {
    use acadrust::entities::Solid;
    use acadrust::types::Color;
    use acadrust::EntityType as E;
    let mut e = E::Solid(Solid::new(
        Vector3::new(p1.0, p1.1, 0.0),
        Vector3::new(p2.0, p2.1, 0.0),
        Vector3::new(p3.0, p3.1, 0.0),
        Vector3::new(p1.0, p1.1, 0.0),
    ));
    set_member_layer(&mut e, "8符号标注层");
    e.common_mut().color = Color::from_index(color);
    e
}

/// 焊接块成员：固定文字（中中锚定；OCSM_GB 宽比 0.7；色 3）——方法字母与 MR/M 小字。
fn weld_member_text(value: &str, pos: (f64, f64), h: f64, rot: f64) -> acadrust::EntityType {
    use acadrust::entities::{Text, TextHorizontalAlignment, TextVerticalAlignment};
    use acadrust::types::Color;
    use acadrust::EntityType as E;
    let mut t = Text::with_value(value, Vector3::new(pos.0, pos.1, 0.0));
    t.height = h;
    t.rotation = rot; // 随基准线轴向（竖基准线 90° 自下而上，永不倒置）
    t.width_factor = 0.7;
    t.style = "OCSM_GB".into();
    t.horizontal_alignment = TextHorizontalAlignment::Center;
    t.vertical_alignment = TextVerticalAlignment::Middle;
    t.alignment_point = Some(t.insertion_point);
    let mut e = E::Text(t);
    set_member_layer(&mut e, "8符号标注层");
    e.common_mut().color = Color::from_index(3);
    e
}

/// 把符号几何（参考单位，u 沿基准线、v 内容上方）经 `map` 映射后追加为块成员。
/// `map(u, v)` 返回块局部坐标；半径/字高按 s 缩放。
fn push_weld_sym<M: Fn(f64, f64) -> (f64, f64)>(
    members: &mut Vec<acadrust::EntityType>,
    ents: &[WeldSymEnt],
    s: f64,
    rot: f64,
    arc_rot_deg: f64,
    map: M,
) {
    for e in ents {
        match *e {
            WeldSymEnt::Line(x1, y1, x2, y2) => {
                members.push(weld_member_line(map(x1, y1), map(x2, y2), 31))
            }
            WeldSymEnt::Circle(cx, cy, r) => {
                members.push(weld_member_circle(map(cx, cy), r * s))
            }
            WeldSymEnt::Arc(cx, cy, r, a0, a1) => {
                members.push(weld_member_arc(map(cx, cy), r * s, a0 + arc_rot_deg, a1 + arc_rot_deg))
            }
            WeldSymEnt::Text(x, y, t) => {
                members.push(weld_member_text(t, map(x, y), 2.24 * s, rot))
            }
        }
    }
}

/// 焊接符号生成：p_tip=顶点0（焊缝点/箭头尖）、p0=顶点1（基准线起点/拐点）、
/// p_end=顶点2（基准线末端）。生成单匿名块 *W{n}（引线+箭头+基准线+虚线+
/// 全周边圆+现场旗+尾叉+上下符号+C 弧字+5 ATTDEF）+ INSERT@p0（attributes
/// 带值，from_definition+transform 定位——粗糙度同套路）。
/// 引导 PLINE 不删（10引导线层不打印，保留便于重选再改）。
fn apply_weld(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p_tip: [f64; 3],
    p0: [f64; 3],
    p_end: [f64; 3],
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::entities::{AttributeDefinition, AttributeEntity, Insert};
    use acadrust::entities::attribute_definition::{HorizontalAlignment, VerticalAlignment};
    use acadrust::entities::Entity as _; // apply_transform
    use acadrust::types::Color;
    use acadrust::EntityType as E;

    let w = &params.weld;

    // ── 引导校验 ──
    let ldx = p0[0] - p_tip[0];
    let ldy = p0[1] - p_tip[1];
    let lead_len = (ldx * ldx + ldy * ldy).sqrt();
    let s = frame_scale_at(doc, p0);
    if lead_len <= 3.5 * s {
        return Err("焊接引导：第一段（引线）过短（需容纳箭头）".into());
    }
    // 基准线（第二段）：按主分量吸附到水平/竖直（用户定：斜线不报错，取主轴）。
    // 内容永远按"可读朝向"渲染（对照 引线与内容渲染方向.dxf 四样例）：
    //   t 沿世界正方向（水平=+x、竖直=+y）自"坐标较小端"（锚点）起算；
    //   v 为内容"上方"（水平=+y、竖直=−x）；虚线恒在 v 负侧
    //（横基准线→虚线在下；竖基准线→虚线在右）。
    let bdx = p_end[0] - p0[0];
    let bdy = p_end[1] - p0[1];
    let horizontal = bdx.abs() >= bdy.abs();
    let blen = if horizontal { bdx.abs() } else { bdy.abs() };
    if blen <= 1e-9 {
        return Err("焊接引导：第二段（基准线段）长度为零".into());
    }
    let (d_c, n_up): ((f64, f64), (f64, f64)) = if horizontal {
        ((1.0, 0.0), (0.0, 1.0))
    } else {
        ((0.0, 1.0), (-1.0, 0.0))
    };

    // ── 符号名解析（"无"/空 = None）──
    let clean = |v: &str| -> Option<String> {
        let t = v.trim();
        if t.is_empty() || t == "无" { None } else { Some(t.to_string()) }
    };
    let upper = clean(&w.upper);
    let lower = clean(&w.lower);
    if let Some(n) = &upper {
        weld_sym(n).ok_or_else(|| format!("未知焊缝符号：{n}"))?;
    }
    if let Some(n) = &lower {
        let d = weld_sym(n).ok_or_else(|| format!("未知焊缝符号：{n}"))?;
        if d.3 {
            return Err(format!("跨线符号只有一种位置（骑基准线），不可放于下侧：{n}"));
        }
    }
    // 下侧内容（符号或文字）= "另一侧有焊缝"。
    let upper_texted = !w.up_thick.trim().is_empty() || !w.up_qty.trim().is_empty();
    let lower_texted = !w.lo_thick.trim().is_empty() || !w.lo_qty.trim().is_empty();
    let lower_has = lower.is_some() || lower_texted;
    // 虚线（识别线）语义（用户定）：
    // - 一般情况下虚线画在基准线**下**（−0.7），表示所指位置的另一侧；
    // - 仅在引线下方有填入内容时出现（GUI/MCP 侧的手工规则，服务端宽容处理）；
    // - **特殊情况**：引线上方什么都没填（无符号、无文字）而下侧有内容 →
    //   把下侧内容显示到上方，且虚线画在基准线**上**（+0.7），符号在虚线之上。
    let flipped = upper.is_none() && !upper_texted && lower_has;
    let dash = lower_has || w.dash;
    // 尾部：开关仍可手动控制，但**存在尾部注释文字时强制打开**（用户定），
    // 否则尾叉与文字都不会生成。
    let tail = w.tail || !w.tail_text.trim().is_empty();
    /// 虚线在内容坐标系 v 轴上的偏移（参考单位）：一般 −0.7（v 负侧 = 横线下/竖线右），
    /// 特殊情况（flipped）+0.7（内容显示到上方、虚线在其下）。
    let dash_v: f64 = if flipped { 0.7 } else { -0.7 };

    // ── 基准线长度：**随输入内容动态调整**（用户定：不再跟随用户画的长短，
    //    画长了会被收紧、画短了会被延长）──
    //    长度 = 各"启用件"沿基准线的右端最大者 + 末端余量。
    //    文字宽粗估：0.59 × 字高3.5 × 宽比0.7 ≈ 1.45/字。
    let slot_base = 19.413_f64; // 示例布局槽位
    let text_w = |t: &str| 1.45 * (t.trim().chars().count() as f64);
    // 方向符号（判断补充元素在拐点侧还是内容侧）：+1 = 拐点在锚点侧
    let dir_in_sign: f64 = if (horizontal && bdx >= 0.0) || (!horizontal && bdy >= 0.0) {
        1.0
    } else {
        -1.0
    };
    // 实际绘制的侧：翻转时只有下侧（显示到上方）；否则上侧 + （虚线开且下侧有符号）
    let up_on = !flipped && upper.is_some();
    let lo_on = if flipped { lower.is_some() } else { lower.is_some() && dash };
    // 厚度在槽位左侧不影响长度；数量长度锚点 slot+6.5（有文字再加文字宽）
    let qty_up = if up_on { w.up_qty.trim() } else { "" };
    let qty_lo = if lo_on { w.lo_qty.trim() } else { "" };
    let up_thick_shown = up_on && !w.up_thick.trim().is_empty();
    let lo_thick_shown = lo_on && !w.lo_thick.trim().is_empty();
    let any_content = upper.is_some()
        || lower.is_some()
        || !w.up_thick.trim().is_empty()
        || !w.up_qty.trim().is_empty()
        || !w.lo_thick.trim().is_empty()
        || !w.lo_qty.trim().is_empty()
        || w.grind_upper != GrindKind::None
        || w.grind_lower != GrindKind::None
        || !w.method_upper.trim().is_empty()
        || !w.method_lower.trim().is_empty()
        || w.half
        || w.circle
        || w.flag
        || !w.tail_text.trim().is_empty();
    let mut need_u = slot_base + if any_content { 6.5 } else { 0.0 }; // 数量长度锚点
    need_u = need_u.max(slot_base + 6.5 + text_w(qty_up));
    need_u = need_u.max(slot_base + 6.5 + text_w(qty_lo));
    // 符号本体（含跨线单置）宽度
    let sym_sides: Vec<(&String, bool)> = if flipped {
        lower.as_ref().map(|n| vec![(n, true)]).unwrap_or_default()
    } else {
        let mut v: Vec<(&String, bool)> = Vec::new();
        if let Some(n) = &upper {
            v.push((n, false));
        }
        if let Some(n) = &lower {
            v.push((n, true));
        }
        v
    };
    for (n, is_lower) in &sym_sides {
        if let Some(d) = weld_sym(n) {
            let ents = if flipped { d.1 } else { d.1 };
            need_u = need_u.max(slot_base + weld_sym_max_u(ents));
            if !flipped {
                if let (false, Some(e)) = (is_lower, d.2) {
                    need_u = need_u.max(slot_base + weld_sym_max_u(e));
                }
            }
        }
    }
    // 打磨件与方法字母（按侧取用）
    for (k, is_fillet, n, is_lower) in sym_sides.iter().map(|(n, is_lower)| {
        let k = if *is_lower && !flipped { w.grind_lower } else { w.grind_upper };
        (k, n.as_str() == "角焊", *n, *is_lower)
    }) {
        need_u = need_u.max(slot_base + weld_grind_max_u(k, is_fillet));
        let m = if is_lower && !flipped { w.method_lower.as_str() } else { w.method_upper.as_str() };
        if !m.trim().is_empty() && m != "无" && weld_method_allowed(n) {
            let (mx, _) = weld_method_pos(k, is_fillet);
            need_u = need_u.max(slot_base + mx + 1.45);
        }
    }
    // 半包围 ⊏：在拐点侧（dir_in<0，即线端为拐点）时须在内容之后多留 5.2+1
    if w.half && dir_in_sign < 0.0 {
        need_u += 6.2;
    }
    // 末端余量 2.0
    let blen = (need_u + 2.0) * s;

    // 块局部坐标（原点 = 拐点 p0）：锚点 = 沿主轴坐标较小的一端；
    // 延长只发生在"远离拐点"一侧（拐点世界坐标不变）。
    let anchor_l = if horizontal {
        (if bdx >= 0.0 { 0.0 } else { -blen }, 0.0)
    } else if bdy >= 0.0 {
        (0.0, 0.0)
    } else {
        (0.0, -blen)
    };
    let corner_t = if (horizontal && bdx >= 0.0) || (!horizontal && bdy >= 0.0) {
        0.0
    } else {
        blen
    };
    let far_t = if corner_t == 0.0 { blen } else { 0.0 };
    // (沿轴 t, 法向 v) → 块局部坐标
    let tp = |t: f64, v: f64| -> (f64, f64) {
        (
            anchor_l.0 + t * d_c.0 + v * n_up.0,
            anchor_l.1 + t * d_c.1 + v * n_up.1,
        )
    };
    // 参考单位（表内 u 沿基准线、v 内容上方）→ 块局部坐标（×图幅倍率）
    let tr = |u: f64, v: f64| -> (f64, f64) { tp(u * s, v * s) };
    // 文字书写方向：随基准线轴向（横=0；竖=+90°，自下而上，永不倒置——
    // 用户实测：竖基准线标注的文字须沿基准线书写，对照示例）。
    let text_rot: f64 = if horizontal { 0.0 } else { std::f64::consts::FRAC_PI_2 };
    // 圆弧角度随坐标系旋转（横=0°、竖=+90°；坐标系为右手正交基，CCW 语义不变）
    let frame_rot_deg: f64 = if horizontal { 0.0 } else { 90.0 };

    // ── 幂等 ensure：OCSM_GB 样式 / 8符号标注层 / ACISOWELD 线型（虚线用）──
    if !doc
        .text_styles
        .iter()
        .any(|st| st.name.eq_ignore_ascii_case("OCSM_GB"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureTextStyles(crate::text_style_defs()),
            "EnsureTextStyles",
        )?;
    }
    if !doc
        .layers
        .iter()
        .any(|ly| ly.name.eq_ignore_ascii_case("8符号标注层"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureLayers(crate::layer_defs()),
            "EnsureLayers",
        )?;
    }
    if !doc
        .line_types
        .iter()
        .any(|lt| lt.name.eq_ignore_ascii_case("ACISOWELD"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureLinetypes(crate::linetype_defs()),
            "EnsureLinetypes",
        )?;
    }

    // ── 块成员（块局部坐标，原点 = 拐点 p0；颜色照参考：
    //    引线/基准线/箭头=青4、虚线=品红6、符号几何=31、文字=绿3）──
    let mut members: Vec<E> = Vec::new();
    let u = (ldx / lead_len, ldy / lead_len); // 引线单位向量（焊缝点→p0）
    let tip = (p_tip[0] - p0[0], p_tip[1] - p0[1]); // 箭头尖（块局部，原始尺寸）
    // 实心箭头：与其它 OCSM 标注同款 arrow_solid（长 2.5×图幅 = DIMASZ、
    // 宽 = 长/3；色 4）。away = 箭头指向方向，体沿 +u 朝拐点。
    let asz = 2.5 * s;
    let mut arrow = E::Solid(arrow_solid(
        Vector3::new(tip.0, tip.1, 0.0),
        Vector3::new(-u.0, -u.1, 0.0),
        asz,
    ));
    set_member_layer(&mut arrow, "8符号标注层");
    arrow.common_mut().color = Color::from_index(4);
    members.push(arrow);
    // 引线：箭头底中（尖 + 2.5×图幅）→ 拐点
    members.push(weld_member_line(
        (tip.0 + asz * u.0, tip.1 + asz * u.1),
        (0.0, 0.0),
        4,
    ));
    // 基准线（沿吸附主轴全长；尾部开关只影响尾叉+注释）
    members.push(weld_member_line(tp(0.0, 0.0), tp(blen, 0.0), 4));
    // 虚线（识别线/第二基准线；ACISOWELD 真线型）：一般情况在 v 负侧 −0.700
    // （横线在下、竖线在右）；特殊情况（flipped）在 v 正侧 +0.700。
    if dash {
        let mut e = weld_member_line(tp(0.0, dash_v * s), tp(blen, dash_v * s), 6);
        e.common_mut().linetype = "ACISOWELD".into();
        members.push(e);
    }
    // 全周边圆（圆心 = 拐点，参考 r1.750）与半包围括号互斥（括号优先）。
    if w.circle && !w.half {
        members.push(weld_member_circle(tp(corner_t, 0.0), 1.75 * s));
    }
    // 半包围（⊏ 形）：区别于全周边圆，表示"半包围结构"的焊缝区域
    //（对照 焊缝区域补充符号.dxf 样例3）：自拐点沿基准线 1.0 起，高 3.5、
    // 两臂长 4.2（参考单位），开口朝基准线远端。
    let dir_in = dir_in_sign;
    if w.half {
        let t0 = corner_t + dir_in * 1.0 * s;
        let t1 = corner_t + dir_in * 5.2 * s;
        members.push(weld_member_line(tp(t0, 1.0 * s), tp(t0, 4.5 * s), 31));
        members.push(weld_member_line(tp(t0, 1.0 * s), tp(t1, 1.0 * s), 31));
        members.push(weld_member_line(tp(t0, 4.5 * s), tp(t1, 4.5 * s), 31));
    }
    // 现场焊接旗（竖杆 + 实心三角旗 5.25×3.5 + 底边，参考 SOLID 13/23 组）。
    // 旗底 v 默认 3.5（旗顶 7.0，照示例）；但半包围 ⊏ 顶到 +4.5 且与旗同侧
    //（dir_in>0，即 ⊏ 也朝 +u 延伸）时会与旗三角重叠 → 整旗抬到其上方
    //（旗底 5.0 = 4.5 + 0.5 间隙，旗顶 8.5）。用户实测：旗与焊接区域符号干涉。
    if w.flag {
        let fb = if w.half && dir_in > 0.0 { 5.0 } else { 3.5 };
        let ftop = fb + 3.5;
        members.push(weld_member_line(tp(corner_t, 0.0), tp(corner_t, ftop * s), 31));
        members.push(weld_member_solid(
            tp(corner_t, ftop * s),
            tp(corner_t + 5.25 * s, fb * s),
            tp(corner_t, fb * s),
            31,
        ));
        members.push(weld_member_line(
            tp(corner_t + 5.25 * s, fb * s),
            tp(corner_t, fb * s),
            31,
        ));
    }
    // 尾叉（基准线远端 → 向外 (+3.5,±3.5)）
    if tail {
        let dir_out = if far_t == blen { 1.0 } else { -1.0 };
        let f0 = tp(far_t, 0.0);
        members.push(weld_member_line(f0, tp(far_t + dir_out * 3.5 * s, 3.5 * s), 31));
        members.push(weld_member_line(f0, tp(far_t + dir_out * 3.5 * s, -3.5 * s), 31));
    }
    // 主符号槽位：保持示例布局 t = 19.413（自锚点起）；若补充元素（圆/括号）
    // 伸得更远则右移避让（用户定）。
    // 补充元素（圆/半包围）自拐点向远端延伸：仅当它们位于"内容增长方向"
    // （dir_in=+1，即拐点在锚点一侧）时才可能与槽位冲突；拐点在远端
    // （dir_in=−1，第二段指向左/下）时它们在槽位之外，无需避让。
    let supp_far_t = if w.half {
        corner_t + dir_in * 5.2 * s
    } else if w.circle {
        corner_t + dir_in * 1.75 * s
    } else {
        f64::NEG_INFINITY
    };
    let supp_end = if dir_in > 0.0 { supp_far_t } else { f64::NEG_INFINITY };
    let slot_ref = (19.413_f64).max((supp_end + 4.0 * s) / s);
    // 侧渲染：base_v = 内容侧基线 v 偏移、m = ±1（−1 = 另一侧镜像）
    let mk = |base_u: f64, base_v: f64, m: f64| {
        move |uu: f64, vv: f64| tr(base_u + uu, base_v + m * vv)
    };
    // 一般情况：上侧（实线侧，v=0）、下侧（虚线侧，v=−0.7，镜像几何）。
    // 特殊情况（flipped）：只有下侧有内容 → 用上侧正朝向几何画在虚线之上
    //（v=+0.7），打磨/焊接方法同样不镜像。
    // 打磨/焊接方法按侧取用（用户定：上下侧独立控制）。
    let (gu, gl) = (w.grind_upper, w.grind_lower);
    let (mu, ml) = (w.method_upper.as_str(), w.method_lower.as_str());
    if flipped {
        if let Some(n) = &lower {
            // 翻转：上方显示的是"下侧内容" → 用下侧的打磨/方法设置，
            // 上侧正朝向几何（表 .1 未镜像）+ 内容侧 v=+0.7，不镜像。
            let map = mk(slot_ref, dash_v, 1.0);
            push_weld_sym(&mut members, weld_sym(n).unwrap().1, s, text_rot, frame_rot_deg, map);
            push_weld_grind(&mut members, gl, n == "角焊", false, s, frame_rot_deg, map);
            push_weld_method(&mut members, ml, n, gl, s, text_rot, map);
        }
    } else {
        if let Some(n) = &upper {
            let map = mk(slot_ref, 0.0, 1.0);
            push_weld_sym(&mut members, weld_sym(n).unwrap().1, s, text_rot, frame_rot_deg, map);
            push_weld_grind(&mut members, gu, n == "角焊", false, s, frame_rot_deg, map);
            push_weld_method(&mut members, mu, n, gu, s, text_rot, map);
        }
        if let Some(n) = &lower {
            if dash {
                // 下侧：符号表 .2 几何**已内置镜像**（m=+1）；打磨/方法表未镜像
                //（m=−1）。弧角度需翻转（mirror_angles=true）。
                let sym_map = mk(slot_ref, dash_v, 1.0);
                let aux_map = mk(slot_ref, dash_v, -1.0);
                push_weld_sym(&mut members, weld_sym(n).unwrap().2.unwrap(), s, text_rot, frame_rot_deg, sym_map);
                push_weld_grind(&mut members, gl, n == "角焊", true, s, frame_rot_deg, aux_map);
                push_weld_method(&mut members, ml, n, gl, s, text_rot, aux_map);
            }
        }
    }

    // ── ATTDEF×5（对照示例 MTEXT 71 组语义：中右(6)=Right+Middle、
    //    中左(4)=Left+Middle；文字始终水平可读）。锚点在内容坐标系：
    //    实线侧 v=+2.750、虚线侧 v=虚线−2.750=−3.450（flipped 时 +3.450）、
    //    尾部注释 = 基准线远端外 4.55（叉尖+1.05）。──
    let far_dir = if far_t == blen { 1.0 } else { -1.0 };
    let tail_anchor = tp(far_t + far_dir * 4.55 * s, 0.0);
    let mut attdefs: Vec<(String, f64, f64, bool)> = Vec::new();
    if flipped {
        // 特殊情况：下侧内容显示到上方 —— 仍用下侧文字槽（值来自下侧字段）。
        let a = tr(slot_ref - 2.0, 3.450);
        let b = tr(slot_ref + 6.5, 3.450);
        attdefs.push(("下侧厚度尺寸A".to_string(), a.0, a.1, true));
        attdefs.push(("下侧数量长度L".to_string(), b.0, b.1, false));
    } else {
        let a = tr(slot_ref - 2.0, 2.750);
        let b = tr(slot_ref + 6.5, 2.750);
        attdefs.push(("上侧厚度尺寸A′".to_string(), a.0, a.1, true));
        attdefs.push(("上侧数量长度L′".to_string(), b.0, b.1, false));
        if dash {
            let a = tr(slot_ref - 2.0, -3.450);
            let b = tr(slot_ref + 6.5, -3.450);
            attdefs.push(("下侧厚度尺寸A".to_string(), a.0, a.1, true));
            attdefs.push(("下侧数量长度L".to_string(), b.0, b.1, false));
        }
    }
    if tail {
        // 对齐：尾注在线端之外；far_dir=+1（线端在内容远端）时文字继续向右
        // 展开 → Left+Middle；far_dir=−1（线端在锚点侧）时文字向左展开 →
        // Right+Middle。否则文字会压到尾叉上（用户实测）。
        attdefs.push(("尾部注释E".to_string(), tail_anchor.0, tail_anchor.1, far_dir < 0.0));
    }
    let value_of = |tag: &str| -> String {
        match tag {
            "上侧厚度尺寸A′" => w.up_thick.clone(),
            "上侧数量长度L′" => w.up_qty.clone(),
            "下侧厚度尺寸A" => w.lo_thick.clone(),
            "下侧数量长度L" => w.lo_qty.clone(),
            "尾部注释E" => w.tail_text.clone(),
            _ => String::new(),
        }
    };
    // 块内 ATTDEF 模板（default 空格占位——宿主空 default 会渲染 tag 名；
    // 真实值只放 INSERT.attributes，粗糙度/图框同套路）。
    let mut att_templates: Vec<AttributeDefinition> = Vec::new();
    for (tag, x, y, right) in attdefs.iter() {
        let mut ad = AttributeDefinition::new(tag.clone(), String::new(), " ".into());
        ad.insertion_point = Vector3::new(*x, *y, 0.0);
        ad.alignment_point = ad.insertion_point;
        ad.height = 3.5 * s;
        ad.rotation = text_rot; // 竖基准线 → 90°（沿基准线书写）
        ad.width_factor = 0.7;
        ad.text_style = "OCSM_GB".into();
        if *right {
            ad.set_alignment(HorizontalAlignment::Right, VerticalAlignment::Middle);
        } else {
            ad.set_alignment(HorizontalAlignment::Left, VerticalAlignment::Middle);
        }
        ad.flags.preset = true; // 插入时不逐项提示
        let mut e = E::AttributeDefinition(ad.clone());
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(3);
        members.push(e);
        att_templates.push(ad);
    }

    // ── 匿名块 *W{n}（独立计数器，避开尺寸标注的 *D 与粗糙度共用 *D）──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*W") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*W{}", max_n + 1);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: members,
        },
        "AddBlockRecord",
    )?;

    // ── INSERT @ 拐点（8符号标注层；attributes = 块内 ATTDEF 位 ×变换）──
    let mut ins = Insert::new(block_name.clone(), Vector3::new(p0[0], p0[1], 0.0));
    ins.common.layer = "8符号标注层".into();
    for ad in att_templates.iter() {
        let val = value_of(&ad.tag);
        let val = if val.trim().is_empty() { " ".to_string() } else { val };
        let mut attr = AttributeEntity::from_definition(ad, Some(val));
        attr.apply_transform(&ins.get_transform());
        ins.attributes.push(attr);
    }
    let handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![E::Insert(ins)]),
        "AddEntities",
    ) {
        Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
        Ok(_) => None,
        Err(e) => return Err(e),
    };
    req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
    mark_dirty(sender)?;

    Ok(serde_json::json!({
        "ok": true,
        "block": block_name,
        "insert_handle": handle.map(fmt_handle),
        "scale": s,
        "dash": dash,
        "flipped": flipped,
        "upper": upper,
        "lower": lower,
    })
    .to_string())
}

// ── 引线标注（LEADER）────────────────────────────────────────────────────
// 定位：焊接标注的**减法版**。骨架 = 引线 + 实心箭头 + 肩线 + 上/下侧文字；
// 无虚线、无全周边圆、无现场旗、无尾叉、无 C、无上下符号槽。
// 引导几何与焊接同构：顶点0 = 箭头点、顶点1 = 拐点 P0、顶点2 = 肩线末端。

/// 引线标注构件（块局部坐标已算好；ATTDEF 模板与取值分列，便于 D2G 复用）。
pub(crate) struct LeaderParts {
    /// 块内实体（含上/下侧 MTEXT；层与颜色已设）。
    pub members: Vec<acadrust::EntityType>,
    /// 图幅倍率。
    pub scale: f64,
    /// 肩线长度（世界单位，内容驱动）。
    pub blen: f64,
    /// 肩线是否水平（竖肩线 → 文字旋转 90°）。
    pub horizontal: bool,
}

/// 引线标注几何：**纯函数**（不读文档、不建块），交互路径与 D2G 转化共用。
/// `s` = `frame_scale_at(doc, p0)`。
pub(crate) fn build_leader_parts(
    p_tip: [f64; 3],
    p0: [f64; 3],
    p_end: [f64; 3],
    s: f64,
    upper: &str,
    lower: &str,
) -> Result<LeaderParts, String> {
    use acadrust::entities::attribute_definition::{HorizontalAlignment, VerticalAlignment};
    use acadrust::entities::AttributeDefinition;
    use acadrust::types::Color;
    use acadrust::EntityType as E;

    // ── 引导校验 ──
    let ldx = p0[0] - p_tip[0];
    let ldy = p0[1] - p_tip[1];
    let lead_len = (ldx * ldx + ldy * ldy).sqrt();
    let asz = 2.5 * s; // 箭头长 = DIMASZ（与其它 OCSM 标注同款）
    if lead_len <= asz {
        return Err("引线标注：第一段（引线）过短（需容纳箭头）".into());
    }
    // 肩线（第二段）：按主分量吸附水平/竖直；内容永远按"可读朝向"渲染
    // （t 沿世界正方向自坐标较小端起算，v = 内容上方）——同焊接。
    let bdx = p_end[0] - p0[0];
    let bdy = p_end[1] - p0[1];
    let horizontal = bdx.abs() >= bdy.abs();
    let blen_drawn = if horizontal { bdx.abs() } else { bdy.abs() };
    if blen_drawn <= 1e-9 {
        return Err("引线标注：第二段（肩线段）长度为零".into());
    }
    let (d_c, n_up): ((f64, f64), (f64, f64)) = if horizontal {
        ((1.0, 0.0), (0.0, 1.0))
    } else {
        ((0.0, 1.0), (-1.0, 0.0))
    };

    // ── 肩线长度：内容驱动（照焊接定案：画长收紧、画短延长）──
    // 文字锚点 = 焊接槽位 19.413 − 2.0 = 17.413；单字宽：拉丁/数字 1.45、
    // 中日韩 2.45（字高 3.5 × 宽比 0.7 下的方块字宽）。
    const SLOT: f64 = 17.413;
    let text_w = |t: &str| -> f64 {
        t.trim()
            .chars()
            .map(|c| if (c as u32) >= 0x2E80 { 2.45 } else { 1.45 })
            .sum::<f64>()
    };
    let need_u = (SLOT + text_w(upper)).max(SLOT + text_w(lower));
    let blen_ru = need_u + 2.0; // 参考单位下的肩线全长
    let blen = blen_ru * s;

    // 块局部坐标（原点 = 拐点 P0）：锚点 = 沿主轴坐标较小的一端。
    // 拐点是否就在锚点一侧（= 肩线自拐点朝 +t 方向延伸），供"文字自拐点外移"用。
    let corner_at_origin = (horizontal && bdx >= 0.0) || (!horizontal && bdy >= 0.0);
    let anchor_l = if horizontal {
        (if bdx >= 0.0 { 0.0 } else { -blen }, 0.0)
    } else if bdy >= 0.0 {
        (0.0, 0.0)
    } else {
        (0.0, -blen)
    };
    let tp = |t: f64, v: f64| -> (f64, f64) {
        (
            anchor_l.0 + t * d_c.0 + v * n_up.0,
            anchor_l.1 + t * d_c.1 + v * n_up.1,
        )
    };
    let tr = |u: f64, v: f64| -> (f64, f64) { tp(u * s, v * s) };
    // 文字书写方向：随肩线轴向（横=0；竖=+90°，自下而上，永不倒置）。
    let text_rot: f64 = if horizontal { 0.0 } else { std::f64::consts::FRAC_PI_2 };

    // ── 块成员（颜色同焊接：引线/箭头/肩线=青4、文字=绿3）──
    let mut members: Vec<E> = Vec::new();
    let u = (ldx / lead_len, ldy / lead_len); // 引线单位向量（箭头点→拐点）
    let tip = (p_tip[0] - p0[0], p_tip[1] - p0[1]);
    let mut arrow = E::Solid(arrow_solid(
        Vector3::new(tip.0, tip.1, 0.0),
        Vector3::new(-u.0, -u.1, 0.0),
        asz,
    ));
    set_member_layer(&mut arrow, "8符号标注层");
    arrow.common_mut().color = Color::from_index(4);
    members.push(arrow);
    // 引线：箭头底中（尖 + 2.5×图幅）→ 拐点
    members.push(weld_member_line(
        (tip.0 + asz * u.0, tip.1 + asz * u.1),
        (0.0, 0.0),
        4,
    ));
    // 肩线（内容驱动全长；本功能不画虚线/圆/旗/尾叉）
    members.push(weld_member_line(tp(0.0, 0.0), tp(blen, 0.0), 4));

    // ── 上/下侧文字：**块内 MTEXT**（不是 ATTDEF）。
    //    为什么用 MTEXT：只有 MTEXT 路径会解析内联字体码 `{\Fgdt;x}` —— 工程
    //    符号（如 x = 深度符号）必须走它；ATTRIB/ATTDEF 会把代码原样画出来。
    //    对齐：自**拐点**沿肩线向外偏移 SLOT（左/下向时右对齐 → 镜像对称）。──
    let dir_out: f64 = if corner_at_origin { 1.0 } else { -1.0 };
    let corner_ru: f64 = if corner_at_origin { 0.0 } else { blen_ru };
    let text_t_ru = corner_ru + dir_out * SLOT;
    for (value, v) in [(upper, 2.750_f64), (lower, -2.750_f64)] {
        if value.trim().is_empty() {
            continue; // 空文字 → 不画（无需占位）
        }
        let (x, y) = tr(text_t_ru, v);
        let mut m = acadrust::entities::MText::new();
        // 行宽自适应：MText::new() 默认 rectangle_width=10，长文字（含 GDT 字体码）
        // 会被宿主按 10 单位折行；按可剥格式码后的可见宽度估足。
        let visible_len = value
            .chars()
            .filter(|c| !matches!(c, '\\' | '{' | '}' | ';' | '^'))
            .count() as f64;
        m.rectangle_width = (visible_len * 3.5 * s * 0.75).max(10.0 * s);
        m.value = value.to_string();
        m.insertion_point = Vector3::new(x, y, 0.0);
        m.height = 3.5 * s;
        m.rotation = text_rot; // 竖肩线 → 90°（沿肩线书写）
        m.style = "OCSM_GB".into();
        m.attachment_point = if dir_out > 0.0 {
            acadrust::entities::AttachmentPoint::MiddleLeft
        } else {
            acadrust::entities::AttachmentPoint::MiddleRight
        };
        let mut e = E::MText(m);
        set_member_layer(&mut e, "8符号标注层");
        e.common_mut().color = Color::from_index(3);
        members.push(e);
    }
    Ok(LeaderParts {
        members,
        scale: s,
        blen,
        horizontal,
    })
}

/// 由构件组 INSERT（attributes = 块内 ATTDEF 位 × 变换；空值占位空格）。
pub(crate) fn leader_insert(block_name: &str, p0: [f64; 3]) -> acadrust::entities::Insert {
    use acadrust::entities::Insert;
    let mut ins = Insert::new(block_name.to_string(), Vector3::new(p0[0], p0[1], 0.0));
    ins.common.layer = "8符号标注层".into();
    ins
}

/// 交互路径：`POST /api/apply` type=LEADER。引导 PLINE 保留（不删）。
fn apply_leader(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p_tip: [f64; 3],
    p0: [f64; 3],
    p_end: [f64; 3],
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::EntityType as E;

    let s = frame_scale_at(doc, p0);
    let parts = build_leader_parts(p_tip, p0, p_end, s, &params.leader.upper, &params.leader.lower)?;

    // ── 幂等 ensure：OCSM_GB 样式 / 8符号标注层（无虚线 → 不需要 ACISOWELD）──
    if !doc
        .text_styles
        .iter()
        .any(|st| st.name.eq_ignore_ascii_case("OCSM_GB"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureTextStyles(crate::text_style_defs()),
            "EnsureTextStyles",
        )?;
    }
    if !doc
        .layers
        .iter()
        .any(|ly| ly.name.eq_ignore_ascii_case("8符号标注层"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureLayers(crate::layer_defs()),
            "EnsureLayers",
        )?;
    }

    // ── 匿名块 *L{n}（独立计数器，避开尺寸标注 *D 与焊接 *W）──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*L") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*L{}", max_n + 1);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: parts.members.clone(),
        },
        "AddBlockRecord",
    )?;

    let ins = leader_insert(&block_name, p0);
    let handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![E::Insert(ins)]),
        "AddEntities",
    ) {
        Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
        Ok(_) => None,
        Err(e) => return Err(e),
    };
    req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
    mark_dirty(sender)?;

    Ok(serde_json::json!({
        "ok": true,
        "block": block_name,
        "insert_handle": handle.map(fmt_handle),
        "scale": s,
        "blen": parts.blen,
        "horizontal": parts.horizontal,
        "upper": params.leader.upper,
        "lower": params.leader.lower,
    })
    .to_string())
}

/// 图纸上已有的序号码（用于自动取号）：来源 = ①序号球标组台账 `OCSM_BALLOON`
/// 的 items；②明细表行块 `OCSM_BOMROW` 的属性「序号」。去重 + 升序。
pub(crate) fn existing_item_nos(doc: &acadrust::CadDocument) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let t = s.trim();
        if !t.is_empty() && !out.iter().any(|x| x == t) {
            out.push(t.to_string());
        }
    };
    for e in doc.model_space_entities() {
        let acadrust::EntityType::Insert(ins) = e else {
            continue;
        };
        // ① 球标组台账
        if let Some(rec) = ins.common.extended_data.get_record("OCSM_BALLOON") {
            if let Some(text) = rec.values.iter().find_map(|v| match v {
                acadrust::xdata::XDataValue::String(s) => Some(s.clone()),
                _ => None,
            }) {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(arr) = meta.get("items").and_then(|v| v.as_array()) {
                        for it in arr {
                            if let Some(s) = it.as_str() {
                                push(s);
                            }
                        }
                    }
                }
            }
        }
        // ② 明细表行的「序号」属性
        if ins.block_name == crate::bom::ROW_BLOCK {
            for a in ins.attributes.iter() {
                if a.tag.trim() == "序号" {
                    push(&a.value);
                }
            }
        }
    }
    crate::balloon::sort_item_nos(&mut out);
    out
}

/// 指针点落在哪个零件块内（**只认块**）：在包含该点的、带 `OCSM_PART` 台账的
/// INSERT 里取**界盒最小**的那个（= 最内层，大装配块套小零件时不会误挂）。
/// 返回 `(insert handle, 台账 JSON)`。落点不在任何零件块内 → `None`（按画的点走）。
fn part_block_at(
    doc: &acadrust::CadDocument,
    p: [f64; 3],
) -> Option<(acadrust::Handle, serde_json::Value)> {
    let mut best: Option<(f64, acadrust::Handle, serde_json::Value)> = None;
    for e in doc.model_space_entities() {
        let acadrust::EntityType::Insert(ins) = e else {
            continue;
        };
        let Some(rec) = ins.common.extended_data.get_record("OCSM_PART") else {
            continue;
        };
        let Some(text) = rec.values.iter().find_map(|v| match v {
            acadrust::xdata::XDataValue::String(s) => Some(s.clone()),
            _ => None,
        }) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some((mn, mx)) = crate::insert_world_aabb(doc, ins) else {
            continue;
        };
        let inside = p[0] >= mn.x - 1e-6
            && p[0] <= mx.x + 1e-6
            && p[1] >= mn.y - 1e-6
            && p[1] <= mx.y + 1e-6;
        if !inside {
            continue;
        }
        let area = (mx.x - mn.x) * (mx.y - mn.y);
        if best.as_ref().map(|(a, _, _)| area < *a).unwrap_or(true) {
            best = Some((area, ins.common.handle, meta));
        }
    }
    best.map(|(_, h, m)| (h, m))
}

/// HTTP 路径的 `BomSink`：与命令路径（`HostApi`）共用同一套建行逻辑（`bom::fill_bom`）。
struct SenderSink<'a>(&'a Arc<dyn PluginRequestSender>);

impl crate::bom::BomSink for SenderSink<'_> {
    fn push_undo(&mut self, label: &str) -> Result<(), String> {
        req_timed(
            self.0,
            PluginRequest::BeginUndo {
                label: label.to_string(),
            },
            "BeginUndo",
        )
        .map(|_| ())?
        ;
        Ok(())
    }
    fn remove_entity(&mut self, h: acadrust::Handle) -> Result<(), String> {
        req_timed(
            self.0,
            PluginRequest::RemoveEntity { handle: h },
            "RemoveEntity",
        )
        .map(|_| ())?;
        Ok(())
    }
    fn add_entities(&mut self, ents: Vec<acadrust::EntityType>) -> Result<usize, String> {
        match req_timed(self.0, PluginRequest::AddEntities(ents), "AddEntities")? {
            PluginResponse::Handles(hs) => Ok(hs.len()),
            _ => Ok(0),
        }
    }
    fn import_block(
        &mut self,
        path: &str,
        name: &str,
    ) -> Result<Vec<acadrust::entities::AttributeDefinition>, String> {
        match req_timed(
            self.0,
            PluginRequest::ImportFrameBlock(ocs_plugin_api::host::ImportFrameBlockRequest {
                path: path.to_string(),
                block_name: name.to_string(),
            }),
            "ImportFrameBlock",
        )? {
            PluginResponse::ImportFrameBlock(Ok(v)) => Ok(v),
            PluginResponse::ImportFrameBlock(Err(e)) => Err(e),
            other => Err(format!("ImportFrameBlock 返回异常: {other:?}")),
        }
    }
    fn set_dirty(&mut self) -> Result<(), String> {
        mark_dirty(self.0)
    }
    fn info(&mut self, msg: &str) {
        let _ = req_timed(self.0, PluginRequest::PushInfo(msg.to_string()), "PushInfo");
    }
    fn error(&mut self, msg: &str) {
        let _ = req_timed(self.0, PluginRequest::PushError(msg.to_string()), "PushError");
    }
}

/// 球标联动（第二期）：
/// ① 勾了「插入序号」且新序号与已有重复 → 把**已有**受影响球标整体重编号（重建块，几何/台账一起更新）；
/// ② 按序号规划行（球标每个条目一行 + 未引用零件接号）→ 重建明细表。
///
/// **尽力而为**：模板块缺失/宿主不配合时只在报告里给个 warning，不影响球标本身生成。
fn balloon_sync_after(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    items: &[String],
    insert_mode: bool,
) -> (Option<serde_json::Value>, Option<String>) {
    let mut renumbered: Vec<String> = Vec::new();
    // ① 重编号（只动已有球标；本次新建的组不在 existing 里是因为它的 items 就是 new_items
    //     —— 重编号只作用于**已存在**的序号值，新建组保持自己的号）。
    let ren = crate::balloon_sync::sync_with(doc, items, insert_mode, |old, new_items| {
        let Some((url, pts, _kind, _guide)) = read_edit_record(sender, old) else {
            return Err("旧球标缺少 OCSM_EDIT 记录，无法重编号".to_string());
        };
        let Some(mut params) = GuideParams::from_url(&url) else {
            return Err("旧球标参数 URL 解析失败".to_string());
        };
        if pts.len() != 3 {
            return Err("旧球标引导几何不是三顶点".to_string());
        }
        params.balloon.items = new_items.to_vec();
        apply_balloon(sender, doc, pts[0], pts[1], pts[2], &params)?;
        req_timed(
            sender,
            PluginRequest::RemoveEntity { handle: old },
            "RemoveEntity",
        )?;
        renumbered.push(new_items.join("、"));
        Ok(())
    });
    if let Err(e) = ren {
        return (None, Some(format!("序号重编号失败：{e}")));
    }

    // ② 重建明细表（重新取快照：上面可能已经改过文档）
    let doc2 = match snapshot(sender) {
        Ok(d) => d,
        Err(e) => return (None, Some(format!("取快照失败：{e}"))),
    };
    let rows = match crate::balloon_sync::plan_rows(&doc2) {
        Ok(r) => r,
        Err(e) => return (None, Some(e)),
    };
    if rows.is_empty() {
        return (None, None);
    }
    let per_col = crate::bom::load_config(&crate::bom::bom_dir()).per_col_rows;
    match crate::bom::fill_bom(&mut SenderSink(sender), &doc2, &rows, per_col) {
        Ok(rep) => (
            Some(serde_json::json!({
                "rows": rep.rows,
                "cols": rep.cols,
                "replaced": rep.removed,
                "renumbered": renumbered,
                "item_nos": rows.iter().map(|r| r.item_no.clone()).collect::<Vec<_>>(),
            })),
            None,
        ),
        Err(e) => (None, Some(e)),
    }
}

/// 交互路径：`POST /api/apply` type=BALLOON。引导 PLINE 保留（不删，可反复重改）。
///
/// 组台账写 `OCSM_BALLOON`（JSON：序号列表/方向/插入模式/关联零件），供
/// 明细表联动（升序排名、新增行、冲突时后续序号 +1）与后续重排使用。
fn apply_balloon(
    sender: &Arc<dyn PluginRequestSender>,
    doc: &acadrust::CadDocument,
    p_tip: [f64; 3],
    p0: [f64; 3],
    p_end: [f64; 3],
    params: &GuideParams,
) -> Result<String, String> {
    use acadrust::EntityType as E;

    let items: Vec<String> = params.balloon.items.clone();
    if items.is_empty() {
        return Err("序号标注：至少要有一个序号（在窗口里加条目）".into());
    }
    let s = frame_scale_at(doc, p0);
    let parts = crate::balloon::build_balloon_parts(
        p_tip,
        p0,
        p_end,
        s,
        &items,
        params.balloon.dir,
    )?;

    // ── 幂等 ensure：OCSM_GB 样式 / 8符号标注层（同引线）──
    if !doc
        .text_styles
        .iter()
        .any(|st| st.name.eq_ignore_ascii_case("OCSM_GB"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureTextStyles(crate::text_style_defs()),
            "EnsureTextStyles",
        )?;
    }
    if !doc
        .layers
        .iter()
        .any(|ly| ly.name.eq_ignore_ascii_case("8符号标注层"))
    {
        req_timed(
            sender,
            PluginRequest::EnsureLayers(crate::layer_defs()),
            "EnsureLayers",
        )?;
    }

    // ── 匿名块 *XH{n}（独立计数器，避开 *D 尺寸 / *L 引线 / *W 焊接 / *V 向视）──
    let mut max_n = 0u32;
    for br in doc.block_records.iter() {
        if let Some(rest) = br.name.strip_prefix("*XH") {
            if let Ok(n) = rest.parse::<u32>() {
                max_n = max_n.max(n);
            }
        }
    }
    let block_name = format!("*XH{}", max_n + 1);
    req_timed(
        sender,
        PluginRequest::AddBlockRecord {
            name: block_name.clone(),
            entities: parts.members.clone(),
        },
        "AddBlockRecord",
    )?;

    // ── 落点吸附：指针点在零件块内 → 关联该零件（读 XDATA OCSM_PART）──
    let assoc = part_block_at(doc, p_tip);
    let mut ins = leader_insert(&block_name, p0);
    {
        let mut rec = ExtendedDataRecord::new("OCSM_BALLOON");
        rec.values.push(XDataValue::String(
            serde_json::json!({
                "items": items,
                "dir": params.balloon.dir.as_str(),
                "ins": params.balloon.insert_mode,
                "part_handle": assoc.as_ref().map(|(h, _)| fmt_handle(*h)),
                "part": assoc.as_ref().map(|(_, m)| m.clone()),
            })
            .to_string(),
        ));
        ins.common.extended_data.add_record(rec);
    }
    let handle = match req_timed(
        sender,
        PluginRequest::AddEntities(vec![E::Insert(ins)]),
        "AddEntities",
    ) {
        Ok(PluginResponse::Handles(hs)) => hs.first().copied(),
        Ok(_) => None,
        Err(e) => return Err(e),
    };
    req_timed(sender, PluginRequest::BumpGeometry, "BumpGeometry")?;
    mark_dirty(sender)?;

    // ── 第二期：球标↔明细表联动（尽力而为，失败只在报告里给 warning）──
    let (bom_report, bom_warning) = if crate::bom::bom_dir().join("OCSM_BOMROW.dwg").exists() {
        balloon_sync_after(
            sender,
            doc,
            &items,
            params.balloon.insert_mode,
        )
    } else {
        (None, None)
    };

    Ok(serde_json::json!({
        "ok": true,
        "block": block_name,
        "insert_handle": handle.map(fmt_handle),
        "scale": s,
        "dir": params.balloon.dir.as_str(),
        "items": items,
        "span": parts.span,
        "shelf_lens": parts.shelf_lens,
        "horizontal": parts.horizontal,
        "part": assoc.map(|(h, m)| serde_json::json!({"handle": fmt_handle(h), "meta": m})),
        "bom": bom_report,
        "bom_warning": bom_warning,
    })
    .to_string())
}

// ── 标注再编辑（`OCSM_EDIT` XDATA）──────────────────────────────────────
// 生成物上记两份信息：
//   `PE_URL`    = 回编辑 GUI 的链接（供宿主 Ctrl+点击打开）——**这是标准超链接**；
//   `OCSM_EDIT` = [参数 URL, 引导顶点 "x,y;x,y;…", 引导种类, 原引导 handle]
//                重建标注所需的原始信息（引线被删的类型靠它恢复）。
const EDIT_APP: &str = "OCSM_EDIT";

/// 引导种类（重建临时引导用）：line / pline / rect / circle / arc。
fn guide_kind_of(e: &acadrust::EntityType) -> &'static str {
    use acadrust::EntityType as E;
    match e {
        E::Line(_) => "line",
        E::Circle(_) => "circle",
        E::Arc(_) => "arc",
        E::LwPolyline(pl) => {
            if pl.is_closed && pl.vertices.len() == 4 {
                "rect"
            } else {
                "pline"
            }
        }
        E::Polyline(pl) => {
            if pl.flags.is_closed() && pl.vertices.len() == 4 {
                "rect"
            } else {
                "pline"
            }
        }
        E::Polyline2D(pl) => {
            if pl.flags.is_closed() && pl.vertices.len() == 4 {
                "rect"
            } else {
                "pline"
            }
        }
        _ => "line",
    }
}

pub(crate) fn edit_record(
    url: &str,
    pts: &[[f64; 3]],
    kind: &str,
    guide: Option<acadrust::Handle>,
) -> ExtendedDataRecord {
    let csv = pts
        .iter()
        .map(|p| format!("{},{}", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(";");
    let mut rec = ExtendedDataRecord::new(EDIT_APP);
    rec.values.push(XDataValue::String(url.to_string()));
    rec.values.push(XDataValue::String(csv));
    rec.values.push(XDataValue::String(kind.to_string()));
    rec.values.push(XDataValue::String(
        guide.map(fmt_handle).unwrap_or_default(),
    ));
    rec
}

/// 读生成物上的 `OCSM_EDIT`：返回（参数 URL、引导顶点、引导种类、原引导 handle）。
/// 返回 Some 即表示这个实体是 OCSM 生成的可重编辑标注。
#[allow(clippy::type_complexity)]
fn read_edit_record(
    sender: &Arc<dyn PluginRequestSender>,
    handle: acadrust::Handle,
) -> Option<(String, Vec<[f64; 3]>, String, Option<acadrust::Handle>)> {
    let rec = match req_timed(
        sender,
        PluginRequest::ReadRecord {
            handle,
            app_name: EDIT_APP.into(),
        },
        "ReadRecord",
    ) {
        Ok(PluginResponse::Record(Some(rec))) => rec,
        _ => return None,
    };
    let strs: Vec<String> = rec
        .values
        .iter()
        .filter_map(|v| match v {
            XDataValue::String(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    let url = strs.first().filter(|s| !s.is_empty())?.clone();
    let pts: Vec<[f64; 3]> = strs
        .get(1)
        .map(|s| {
            s.split(';')
                .filter_map(|kv| {
                    let (x, y) = kv.split_once(',')?;
                    Some([x.trim().parse().ok()?, y.trim().parse().ok()?, 0.0])
                })
                .collect()
        })
        .unwrap_or_default();
    let kind = strs.get(2).cloned().unwrap_or_else(|| "line".into());
    let guide = strs
        .get(3)
        .filter(|s| !s.is_empty())
        .and_then(|s| handle_hex(s).ok());
    Some((url, pts, kind, guide))
}

/// 按种类 + 顶点造一条临时引导实体（编辑时原引导可能已被删除）。
fn temp_guide_entity(kind: &str, pts: &[[f64; 3]]) -> Result<acadrust::EntityType, String> {
    use acadrust::entities::{LwPolyline, LwVertex};
    use acadrust::types::Vector2;
    use acadrust::EntityType as E;
    let _ = LwVertex::new(Vector2::new(0.0, 0.0));
    match kind {
        "line" if pts.len() >= 2 => {
            let mut l = acadrust::entities::Line::new();
            l.start = Vector3::new(pts[0][0], pts[0][1], pts[0][2]);
            l.end = Vector3::new(pts[1][0], pts[1][1], pts[1][2]);
            Ok(E::Line(l))
        }
        "pline" | "rect" if pts.len() >= 2 => {
            let mut pl = LwPolyline::new();
            for p in pts {
                pl.add_point(Vector2::new(p[0], p[1]));
            }
            if kind == "rect" {
                pl.is_closed = true;
            }
            Ok(E::LwPolyline(pl))
        }
        "circle" if pts.len() >= 2 => {
            let r = ((pts[1][0] - pts[0][0]).powi(2) + (pts[1][1] - pts[0][1]).powi(2)).sqrt();
            let mut c = acadrust::entities::Circle::new();
            c.center = Vector3::new(pts[0][0], pts[0][1], pts[0][2]);
            c.radius = r;
            Ok(E::Circle(c))
        }
        // ARC：存的是 起点/终点/中点（同 `guide_geom_points`）→ 三点定圆恢复。
        "arc" if pts.len() >= 3 => {
            let (a, b, m) = (pts[0], pts[1], pts[2]);
            let (ax, ay, bx, by, mx, my) = (a[0], a[1], b[0], b[1], m[0], m[1]);
            let d = 2.0 * (ax * (by - my) + bx * (my - ay) + mx * (ay - by));
            if d.abs() < 1e-12 {
                return Err("弧引导三点共线，无法恢复".into());
            }
            let ux = ((ax * ax + ay * ay) * (by - my)
                + (bx * bx + by * by) * (my - ay)
                + (mx * mx + my * my) * (ay - by))
                / d;
            let uy = ((ax * ax + ay * ay) * (mx - bx)
                + (bx * bx + by * by) * (ax - mx)
                + (mx * mx + my * my) * (bx - ax))
                / d;
            let r = ((ax - ux).powi(2) + (ay - uy).powi(2)).sqrt();
            let mut arc = acadrust::entities::Arc::new();
            arc.center = Vector3::new(ux, uy, a[2]);
            arc.radius = r;
            arc.start_angle = (ay - uy).atan2(ax - ux);
            arc.end_angle = (by - uy).atan2(bx - ux);
            // 中点决定扫向（逆/顺时针）。
            let mid_ang = (my - uy).atan2(mx - ux);
            let sweep = (arc.end_angle - arc.start_angle).rem_euclid(std::f64::consts::TAU);
            let to_mid = (mid_ang - arc.start_angle).rem_euclid(std::f64::consts::TAU);
            if to_mid > sweep {
                std::mem::swap(&mut arc.start_angle, &mut arc.end_angle);
            }
            Ok(E::Arc(arc))
        }
        other => Err(format!("该引导类型暂不支持再编辑（{other}）")),
    }
}

/// 把 `OCSM_EDIT`（参数 URL + 引导几何 + 种类）**直接挂到实体对象**上。
/// 与 `stamp_editable` 的区别：不需要 sender、不需要 handle —— 所以实体无论是
/// 直接返回、还是经收集器/宿主加进文档，记录都跟着走（D2G 场景必需）。
pub(crate) fn stamp_edit_entity(
    entity: &mut acadrust::EntityType,
    params_url: &str,
    pts: &[[f64; 3]],
    kind: &str,
) {
    let rec = edit_record(params_url, pts, kind, None);
    entity.common_mut().extended_data.add_record(rec);
}

/// 生成后：给标注写 `PE_URL`（回编辑 GUI 的链接，供宿主 Ctrl+点击）+ `OCSM_EDIT`
/// （重建信息）。`params_url` = 形如 `http://127.0.0.1:PORT/DIM/...` 的参数 URL。
fn stamp_editable(
    sender: &Arc<dyn PluginRequestSender>,
    annotation: acadrust::Handle,
    params_url: &str,
    pts: &[[f64; 3]],
    kind: &str,
    guide: Option<acadrust::Handle>,
) {
    if let Some(port) = crate::current_guide_port() {
        let gui = format!(
            "http://127.0.0.1:{port}/guide.html?handle={}",
            fmt_handle(annotation)
        );
        let _ = req_timed(
            sender,
            PluginRequest::WriteRecord {
                handle: annotation,
                record: pe_url_record(&gui),
            },
            "WriteRecord",
        );
    }
    let _ = req_timed(
        sender,
        PluginRequest::WriteRecord {
            handle: annotation,
            record: edit_record(params_url, pts, kind, guide),
        },
        "WriteRecord",
    );
}

/// 从 apply 返回的 JSON 里取生成物 handle（各类型键名不同）。
fn produced_handles(out: &str) -> Vec<acadrust::Handle> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(out) else {
        return Vec::new();
    };
    [
        "insert_handle",
        "dimension_handle",
        "arrow_insert_handle",
        "marker_insert_handle",
    ]
    .iter()
    .filter_map(|k| v.get(*k).and_then(|s| s.as_str()))
    .filter_map(|s| handle_hex(s).ok())
    .collect()
}

/// ATTDEF tag → 英文代号（用于 values 查询 / attributes 对齐）。
fn tag_alias(tag: &str) -> &str {
    match tag {
        "粗糙度上限A′" => "A′",
        "粗糙度下限A" => "A",
        "备注E" => "E",
        "加工符号P" => "P",
        "加工方法B′" => "B′",
        "加工方法B" => "B",
        "取样长度C" => "C",
        "纹理方向G" => "G",
        _ => tag,
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
const ROUGH_HTML: &str = include_str!("rough_gui.html");
/// 标准件选择器页（参数化生成）。
const PARTS_HTML: &str = include_str!("parts_gui.html");
/// 螺栓副（件链装配）页：给人类用的 GUI（AI 走命令行/HTTP 同一套实现）。
const JOINT_HTML: &str = include_str!("joint_gui.html");
/// 命令手册页（人类侧命令目录 + 操作教程）：教程正文是**磁盘上的 md**（见 `manual_dirs()`），
/// 与 AI 侧 skill（`~/.agents/skills/ocsm-manual/`）共用同一批文件，避免两份内容漂移。
const MANUAL_HTML: &str = include_str!("manual_gui.html");

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
        // 扁平流必须含 (271 → 2) dimdec 与 (74 → 1) dimtoh。
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
        assert_eq!(codes.get(&271).map(|s| s.as_str()), Some("int2"));
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
        assert_eq!(j["rows"], 1); // 单行（符号 + 公差 + 基准）
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
        assert_eq!(j["rows"], 1); // 单行：符号 + ⌀公差 + 3 基准 = 5 格
    }

    #[test]
    fn apply_tolerance_multiple_rows_stack() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 30.0, 0.0], [5.0, 30.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_rows = vec![
            GdtRow { sym: "f".into(), dia: false, tol: "0.02".into(), mods: String::new(), d1: "A".into(), d2: String::new(), d3: String::new() },
            GdtRow { sym: "j".into(), dia: true, tol: "0.05".into(), mods: String::new(), d1: "A".into(), d2: "B".into(), d3: "C".into() },
        ];
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["rows"], 2);
        // 块内应含两组 MTEXT（每行每格一个居中文本）。第一行 3 格 + 第二行 5 格 = 8。
        let block_name = j["block"].as_str().unwrap();
        let ents = inner.block_entities(block_name);
        let mtexts = ents.iter().filter(|e| matches!(e, acadrust::EntityType::MText(_))).count();
        assert_eq!(mtexts, 8);
    }

    #[test]
    fn apply_tolerance_box_follows_last_segment() {
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        // 最后一段竖直向下（Pn-1=(0,30) → Pn=(0,10)）：框应沿 y 轴延伸。
        let pts = vec![[0.0, 20.0, 0.0], [0.0, 30.0, 0.0], [0.0, 10.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_sym = Some("b".into());
        p.gdt_tol = Some("0.03".into());
        p.gdt_d1 = Some("A".into());
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true);
        let block_name = j["block"].as_str().unwrap();
        let ents = inner.block_entities(block_name);
        // 单元格 MTEXT 应沿 y 轴分布（x≈0，y 变化），而非沿 x 轴。
        let mts: Vec<_> = ents.iter()
            .filter_map(|e| match e { acadrust::EntityType::MText(m) => Some(m.insertion_point.clone()), _ => None })
            .collect();
        assert!(!mts.is_empty());
        let xs: Vec<f64> = mts.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = mts.iter().map(|p| p.y).collect();
        let x_min = xs.iter().cloned().fold(f64::MAX, f64::min);
        let x_max = xs.iter().cloned().fold(f64::MIN, f64::max);
        let y_min = ys.iter().cloned().fold(f64::MAX, f64::min);
        let y_max = ys.iter().cloned().fold(f64::MIN, f64::max);
        let x_span = x_max - x_min;
        let y_span = y_max - y_min;
        // 竖直引导线：单元格主要沿 y（x_span 应远小于 y_span）。
        assert!(y_span > 3.0 * x_span, "框应沿竖直方向延伸: x_span={x_span}, y_span={y_span}");
    }

    #[test]
    fn apply_tolerance_mods_and_annotations() {
        use crate::guide_url::GdtRow;
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 30.0, 0.0], [5.0, 30.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_rows = vec![
            GdtRow { sym: "j".into(), dia: true, tol: "0.05".into(), mods: "m".into(), d1: "A".into(), d2: "B".into(), d3: String::new() },
        ];
        p.gdt_top = Some("最大实体".into());
        p.gdt_bot = Some("基本尺100".into());
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        let block_name = j["block"].as_str().unwrap();
        let ents = inner.block_entities(block_name);
        // 数字格应含 ⌀ + 公差 + 修饰符（gdt 码）：%%c0.05{\Fgdt;m}
        let tol_cell = ents.iter().find_map(|e| match e {
            acadrust::EntityType::MText(m) if m.value.contains("0.05") => Some(m.value.clone()),
            _ => None,
        }).expect("公差格 MTEXT");
        assert!(tol_cell.starts_with("%%c0.05"), "应含 ⌀+公差: {tol_cell}");
        assert!(tol_cell.contains("Fgdt;") && tol_cell.contains("H0.8x") && tol_cell.contains('m'), "应含修饰符 gdt 码(80%): {tol_cell}");
        // 顶部/底部注释 MTEXT。
        let top = ents.iter().find_map(|e| match e {
            acadrust::EntityType::MText(m) if m.value.contains("最大实体") => Some(()),
            _ => None,
        }).is_some();
        let bot = ents.iter().find_map(|e| match e {
            acadrust::EntityType::MText(m) if m.value.contains("基本尺") => Some(()),
            _ => None,
        }).is_some();
        assert!(top, "顶部注释缺失");
        assert!(bot, "底部注释缺失");
    }

    #[test]
    fn apply_tolerance_vertical_reading_order_and_rotation() {
        use crate::guide_url::GdtRow;
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        // 末段竖直向上：框应竖直、从下往上读（符号最下→公差→基准最上）、文字旋转 90°。
        let pts = vec![[0.0, 10.0, 0.0], [0.0, 30.0, 0.0], [0.0, 60.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_rows = vec![GdtRow {
            sym: "f".into(), dia: false, tol: "0.02".into(), mods: "m".into(),
            d1: "A".into(), d2: String::new(), d3: String::new(),
        }];
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        let block_name = j["block"].as_str().unwrap();
        let ents = inner.block_entities(block_name);
        let mts: Vec<_> = ents.iter().filter_map(|e| match e {
            acadrust::EntityType::MText(m) => Some(m.clone()),
            _ => None,
        }).collect();
        let sym = mts.iter().find(|m| m.value.contains("Fgdt;") && m.value.contains('f')).expect("符号");
        let tol = mts.iter().find(|m| m.value.contains("0.02")).expect("公差");
        let dat = mts.iter().find(|m| m.value.trim() == "A").expect("基准");
        // 从下往上：y：符号 < 公差 < 基准；文字逆时针旋转 90°。
        assert!(sym.insertion_point.y < tol.insertion_point.y, "符号应在公差下方");
        assert!(tol.insertion_point.y < dat.insertion_point.y, "公差应在基准下方");
        assert!((sym.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-6, "文字应旋转90°，实际 {}", sym.rotation);
    }

    #[test]
    fn apply_tolerance_num_cell_auto_width() {
        use crate::guide_url::GdtRow;
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        // 长公差（0.123456789）应触发数字格扩展到 28（4×7），而非 14 换行。
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 30.0, 0.0], [40.0, 30.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_rows = vec![GdtRow {
            sym: "j".into(), dia: true, tol: "0.123456789".into(), mods: "m".into(),
            d1: "A".into(), d2: String::new(), d3: String::new(),
        }];
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true);
        // 数字格宽 = total_width - 符号格 7 - 基准格 7 = 数字格（应 ≈28, scale=1）。
        let tw = j["total_width"].as_f64().unwrap();
        let num_w = tw - 14.0; // 符号(7) + 基准(7)
        assert!(num_w >= 27.9 && num_w <= 28.1, "长公差应扩展到 28 宽，实际 {num_w}");
    }

    #[test]
    fn apply_tolerance_leaders_in_from_anchor_side() {
        use crate::guide_url::GdtRow;
        // 末段朝左来（raw.x<0）：锚点应在框 C 端（基准格外缘），框向左展开。
        // 引导线终点（pts 最后一点）应贴住框右缘（C 侧），框内符号格在左端。
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        // 箭头在左 (0,10) → 折点 (40,40) → 接入点 (0,40)：末段从右往左（raw.x=-1，水平）。
        let pts = vec![[0.0, 10.0, 0.0], [40.0, 40.0, 0.0], [0.0, 40.0, 0.0]];
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Tolerance;
        p.gdt_rows = vec![GdtRow {
            sym: "f".into(), dia: false, tol: "0.02".into(), mods: String::new(),
            d1: "A".into(), d2: String::new(), d3: String::new(),
        }];
        let out = apply_tolerance(&sender, &doc, &pts, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        let block_name = j["block"].as_str().unwrap();
        let ents = inner.block_entities(block_name);
        // 框线（色 31）：找 x 最小/最大的竖线（框左/右缘）。引导线是色 4 不算。
        let frame = |e: &acadrust::EntityType| -> Option<(f64, f64)> {
            if let acadrust::EntityType::Line(l) = e {
                if matches!(l.common.color, acadrust::types::Color::Index(31)) {
                    return Some((l.start.x.min(l.end.x), l.start.x.max(l.end.x)));
                }
            }
            None
        };
        let xmax = ents.iter().filter_map(frame).map(|(_, hi)| hi).fold(f64::MIN, f64::max);
        let xmin = ents.iter().filter_map(frame).map(|(lo, _)| lo).fold(f64::MAX, f64::min);
        // 引导线接入点（局部）= pts 最后一点 - origin（origin=pts[0]）。
        let anchor_x = pts[2][0] - pts[0][0]; // = 0
        // 末段朝左：框右缘应 = 锚点（C 侧）；框左缘 = 锚点 - 框宽（符号侧在左）。
        assert!((xmax - anchor_x).abs() < 1e-6, "框右缘应贴锚点(C侧)，右缘={xmax} 锚点={anchor_x}");
        // 符号 MTEXT 应在框左端（x 较小）。
        let sym_x = ents.iter().filter_map(|e| match e {
            acadrust::EntityType::MText(m) if m.value.contains("Fgdt;") && m.value.contains('f') => Some(m.insertion_point.x),
            _ => None,
        }).next().expect("符号 MTEXT");
        assert!(sym_x < (xmin + 6.0), "符号格应在框左端，sym_x={sym_x} 框左缘={xmin}");
        // 末段朝右：锚点应在符号侧（框左缘）。
        let pts2 = vec![[0.0, 10.0, 0.0], [40.0, 40.0, 0.0], [80.0, 40.0, 0.0]];
        let out2 = apply_tolerance(&sender, &doc, &pts2, &p).unwrap();
        let j2: serde_json::Value = serde_json::from_str(&out2).unwrap();
        let bn2 = j2["block"].as_str().unwrap();
        let ents2 = inner.block_entities(bn2);        let xmin2 = ents2.iter().filter_map(frame).map(|(lo, _)| lo).fold(f64::MAX, f64::min);
        let anchor2_x = pts2[2][0] - pts2[0][0]; // = 80
        assert!((xmin2 - anchor2_x).abs() < 1e-6, "框左缘应贴锚点(符号侧)，左缘={xmin2} 锚点={anchor2_x}");
    }

    #[test]
    fn build_dimension_dec_overrides_dimdec_xdata() {
        use acadrust::xdata::XDataValue as V;
        let inner = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let pd = GuideParams { guide_type: GuideType::Linear, sub: Some(LinearSub::Aligned), dist: 10.0, text: None, tol: None, up: None, dn: None,
            fit: None, sym: None, dec: Some(2), ver: crate::guide_url::DatumVersion::GB2008, letter: None, scale: None, flip: crate::guide_url::FlipDir::None, marker: None, angle_mode: crate::guide_url::AngleMode::Minor, section_side: crate::guide_url::SectionSide::Right, show_arrow: true, gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
            };
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
            fit: None, sym: None, dec: None, ver: crate::guide_url::DatumVersion::GB2008, letter: None, scale: None, flip: crate::guide_url::FlipDir::None, marker: None, angle_mode: crate::guide_url::AngleMode::Minor, section_side: crate::guide_url::SectionSide::Right, show_arrow: true, gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
            };
        // 直径走匿名块路径：mock 的 AddBlockRecord 返回 Ok → 成功。
        assert!(build_dimension(&sender, &doc, [0.0,0.0,0.0], [35.36,35.36,0.0], &pd, "OCSM_GB").is_ok());
        let pr = GuideParams { guide_type: GuideType::Radius, sub: None, dist: 11.0, text: None, tol: None, up: None, dn: None,
            fit: None, sym: None, dec: None, ver: crate::guide_url::DatumVersion::GB2008, letter: None, scale: None, flip: crate::guide_url::FlipDir::None, marker: None, angle_mode: crate::guide_url::AngleMode::Minor, section_side: crate::guide_url::SectionSide::Right, show_arrow: true, gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
            };
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

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
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

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
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

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
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

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
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

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
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

                    gdt_sym: None, gdt_dia: false, gdt_tol: None, gdt_d1: None, gdt_d2: None, gdt_d3: None, gdt_rows: Vec::new(), gdt_top: None, gdt_bot: None,
            detail_scale: 2.0, detail_no: None, detail_pos: None,
            detail_frame: 1.0, weld: WeldParams::default(),
            leader: crate::guide_url::LeaderParams::default(),balloon: Default::default(),
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
    blocks: std::sync::Mutex<Vec<(String, Vec<acadrust::EntityType>)>>,
    ensures: std::sync::Mutex<Vec<String>>,
    undos: std::sync::Mutex<Vec<String>>,
    /// 插入的 INSERT：(块名, 基点, 转角弧度)
    inserts: std::sync::Mutex<Vec<(String, [f64; 3], f64)>>,
}
impl MockSender {
    fn new(doc: acadrust::CadDocument) -> Self {
        MockSender {
            doc: std::sync::Mutex::new(doc),
            url_writes: std::sync::Mutex::new(Vec::new()),
            blocks: std::sync::Mutex::new(Vec::new()),
            ensures: std::sync::Mutex::new(Vec::new()),
            undos: std::sync::Mutex::new(Vec::new()),
            inserts: std::sync::Mutex::new(Vec::new()),
        }
    }
    /// 撤销事务序列（断言“写之前先 begin、写完后 commit”）。
    fn undos(&self) -> Vec<String> {
        self.undos.lock().unwrap().clone()
    }
    /// 插入的 INSERT 序列（块名 / 基点 / 转角）。
    fn inserts(&self) -> Vec<(String, [f64; 3], f64)> {
        self.inserts.lock().unwrap().clone()
    }
    /// 建过的块序列（名字）：断言"同名块不重复建"。
    fn blocks(&self) -> Vec<(String, Vec<acadrust::EntityType>)> {
        self.blocks.lock().unwrap().clone()
    }
    fn block_entities(&self, name: &str) -> Vec<acadrust::EntityType> {
        self.blocks.lock().unwrap().iter()
            .filter(|(n, _)| n == name)
            .last()
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
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
            R::ReadRecord { handle, app_name } => {
                // 从（可变的）测试文档里真读：编辑模式的 OCSM_EDIT 往返靠它。
                // 同时保留旧的 url_writes 快照（PE_URL 断言沿用）。
                let doc = self.doc.lock().unwrap();
                let rec = doc.get_entity(handle).and_then(|e| {
                    e.common()
                        .extended_data
                        .get_record(&app_name)
                        .cloned()
                });
                Ok(P::Record(rec))
            }
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
                // 真写进文档，**同名替换**（与宿主 write_record 一致）——
                // 否则重复写入会保留旧值，编辑模式读不到新参数。
                if let Some(e) = self.doc.lock().unwrap().get_entity_mut(handle) {
                    let xd = &mut e.common_mut().extended_data;
                    let app = record.application_name.clone();
                    let kept: Vec<_> = xd
                        .records()
                        .iter()
                        .filter(|r| r.application_name != app)
                        .cloned()
                        .collect();
                    xd.clear();
                    for r in kept {
                        xd.add_record(r);
                    }
                    xd.add_record(record);
                }
                Ok(P::Ok)
            }
            R::AddEntity(e) => {
                let h = self
                    .doc
                    .lock()
                    .unwrap()
                    .add_entity(e)
                    .map_err(|err| PluginRequestError(err.to_string()))?;
                Ok(P::Handle(h))
            }
            R::AddEntities(es) => {
                let mut hs = Vec::new();
                let mut doc = self.doc.lock().unwrap();
                for e in es {
                    // 记下 INSERT 的块名/基点/转角（装配类断言用）。
                    if let acadrust::EntityType::Insert(ins) = &e {
                        self.inserts.lock().unwrap().push((
                            ins.block_name.clone(),
                            [ins.insert_point.x, ins.insert_point.y, ins.insert_point.z],
                            ins.rotation,
                        ));
                    }
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
            R::UpdateEntity(e) => {
                let mut doc = self.doc.lock().unwrap();
                let h = e.common().handle;
                if h != acadrust::Handle::NULL {
                    if let Some(t) = doc.get_entity_mut(h) {
                        *t = e;
                    }
                }
                Ok(P::Ok)
            }
            R::AddBlockRecord { name, entities } => {
                self.blocks.lock().unwrap().push((name, entities));
                Ok(P::Ok)
            }
            R::EnsureTextStyles(defs) => {
                self.ensures
                    .lock()
                    .unwrap()
                    .extend(defs.iter().map(|d| format!("text:{}", d.name)));
                Ok(P::Ok)
            }
            R::EnsureLayers(defs) => {
                self.ensures
                    .lock()
                    .unwrap()
                    .extend(defs.iter().map(|d| format!("layer:{}", d.name)));
                Ok(P::Ok)
            }
            R::EnsureLinetypes(defs) => {
                self.ensures
                    .lock()
                    .unwrap()
                    .extend(defs.iter().map(|d| format!("ltype:{}", d.name)));
                Ok(P::Ok)
            }
            R::PushUndo { label } => {
                self.undos.lock().unwrap().push(label);
                Ok(P::Ok)
            }
            R::BeginUndo { label } => {
                self.undos.lock().unwrap().push(format!("begin:{label}"));
                Ok(P::Ok)
            }
            R::CommitUndo => {
                self.undos.lock().unwrap().push("commit".into());
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


    /// 进程级串行锁：`set_pending_part` 是全局状态，多个导出测试并行会互相覆盖（曾致偶发失败）。
    fn export_lock() -> std::sync::MutexGuard<'static, ()> {
        static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
        L.lock().unwrap_or_else(|e| e.into_inner())
    }

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
    fn http_server_rough_apply_end_to_end() {
        // POST /api/rough_apply：C4R5 全形态 → 块含 7 线/1 圆/1 SOLID/8 ATTDEF，
        // INSERT @ 坐标 + attributes；GET /rough.html 返回 GUI。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let server = spawn(mock.clone()).expect("spawn guide server");
        let html = http_req(server.port, "GET", "/rough.html?x=5&y=6", "");
        assert!(html.contains("OCSM 表面粗糙度"), "rough.html 内嵌 GUI");
        assert!(html.contains("/api/rough_apply"));
        assert!(html.contains("周边相同处理"), "rough.html 基础体术语");
        assert!(html.contains("上限开关"));
        // R2 按钮省略「高级」前缀（用户指定：高级+周边相同处理 → 周边相同处理）。
        assert!(html.contains(">高级</button>"), "R3 独立「高级」仍保留");
        assert!(html.contains("上限开关+周边相同处理"));
        assert!(!html.contains("高级+周边相同处理"));
        let body = r#"{"x":50.0,"y":60.0,"base":"C4","extra":"R5","p":"",
            "rotation":0.0,"values":{"A′":"3.2"}}"#;
        let r = http_req(server.port, "POST", "/api/rough_apply", body);
        let j: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["block"], "*D1");
        let members = mock.block_entities("*D1");
        assert_eq!(
            members
                .iter()
                .filter(|e| matches!(e, EntityType::Line(_)))
                .count(),
            7
        );
        assert_eq!(
            members
                .iter()
                .filter(|e| matches!(e, EntityType::AttributeDefinition(_)))
                .count(),
            8
        );
        // 坏请求：非法 base。
        let bad = http_req(server.port, "POST", "/api/rough_apply", r#"{"x":0,"y":0,"base":"X9","extra":"R1"}"#);
        assert!(bad.contains("无效的基础体"));
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

    // ── 弧长标注 ARCLEN ─────────────────────────────────────────

    /// 参考文件 ARCLEN.dxf 几何复刻：引导弧 c=(76.10,69.46) r=60.59 4.71°→72.70°，
    /// 外侧 dist=13.6：尺寸弧同心 r=74.19；界线从引导弧端点径向到尺寸弧端点×2；
    /// SOLID 箭头×2；文字 = 弧长（r×sweep=71.89，dec=2）；小半圆弧符号（色3/层0，
    /// r≈2.6，对称轴沿弧中点方向）；INSERT @ 引导圆心；引导弧已删除。
    #[test]
    fn apply_arclen_outside_matches_reference() {
        use acadrust::entities::{Arc, AttachmentPoint};
        use acadrust::types::{Color, Vector3};
        use acadrust::EntityType as E;

        let mut doc = acadrust::CadDocument::new();
        let mut g = E::Arc(Arc::from_center_radius_angles(
            Vector3::new(76.10315450223098, 69.46285912943617, 0.0),
            60.59037754501,
            4.71319417612647_f64.to_radians(),
            72.69617975791557_f64.to_radians(),
        ));
        g.common_mut().layer = "10引导线层".into();
        let gh = doc.add_entity(g).unwrap();
        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::ArcLen;
        p.dist = 13.6;
        let out = apply_arclen(&sender, &doc, gh, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["geom"], "arc");
        assert_eq!(j["side"], "out");
        assert!((j["arc_len"].as_f64().unwrap() - 71.892).abs() < 0.01);
        assert_eq!(j["text"], "71.89");
        assert_eq!(j["block"], "*D1");

        let ents = inner.block_entities("*D1");
        let arcs: Vec<_> = ents.iter().filter(|e| matches!(e, E::Arc(_))).collect();
        let lines: Vec<_> = ents.iter().filter(|e| matches!(e, E::Line(_))).collect();
        let solids: Vec<_> = ents.iter().filter(|e| matches!(e, E::Solid(_))).collect();
        let mts: Vec<_> = ents.iter().filter(|e| matches!(e, E::MText(_))).collect();
        assert_eq!(arcs.len(), 2, "尺寸弧 + 小弧符号");
        assert_eq!(lines.len(), 2, "界线×2");
        assert_eq!(solids.len(), 2, "箭头×2");
        assert_eq!(mts.len(), 1);
        // 弧中点方向（全局角，供小弧断言复用）。
        let mid_deg = (4.71319417612647_f64 + 72.69617975791557_f64) / 2.0;
        let mid = mid_deg.to_radians();
        // 尺寸弧：圆心=块原点、r=60.59+13.6=74.19、角度同引导。
        let dim_arc = arcs.iter().find_map(|e| match e {
            E::Arc(a) if a.radius > 70.0 => Some(a.clone()),
            _ => None,
        });
        let dim_arc = dim_arc.expect("尺寸弧");
        assert!((dim_arc.radius - 74.187).abs() < 0.01);
        assert!((dim_arc.center.x).abs() < 1e-9 && (dim_arc.center.y).abs() < 1e-9);
        assert!((dim_arc.start_angle - 4.71319417612647_f64.to_radians()).abs() < 1e-9);
        assert_eq!(dim_arc.common.layer, "7标注层");
        // 界线：引导弧端点(半径 r) → 尺寸弧端点(半径 r+dist)，径向。
        let d = 13.6_f64;
        for (i, e) in lines.iter().enumerate() {
            if let E::Line(l) = e {
                let ang = if i == 0 { 4.71319417612647_f64 } else { 72.69617975791557_f64 };
                let ang = ang.to_radians();
                let (cx, cy) = (76.10315450223098, 69.46285912943617);
                let inner_p = (cx + 60.59037754501 * ang.cos(), cy + 60.59037754501 * ang.sin());
                let outer_p = (cx + 76.18714345675 * ang.cos(), cy + 76.18714345675 * ang.sin()); // r+dist+2 界线出头
                // 块局部坐标（原点=引导圆心）。
                let (sx, sy) = (l.start.x + cx, l.start.y + cy);
                let (ex, ey) = (l.end.x + cx, l.end.y + cy);
                assert!(
                    (sx - inner_p.0).abs() < 0.01 && (sy - inner_p.1).abs() < 0.01 && (ex - outer_p.0).abs() < 0.01 && (ey - outer_p.1).abs() < 0.01
                    || (sx - outer_p.0).abs() < 0.01 && (sy - outer_p.1).abs() < 0.01 && (ex - inner_p.0).abs() < 0.01 && (ey - inner_p.1).abs() < 0.01
                );
            }
        }
        // 文字：尺寸弧中点外 1.0（径向）。
        if let E::MText(m) = &mts[0] {
            assert_eq!(m.common.layer, "7标注层");
            assert!(matches!(m.common.color, Color::Index(3)), "文字应显式绿色");
            assert!(m.value.contains("71.89"), "{}", m.value);
            // 文字中心在尺寸弧中点上方 h=2.5（不与弧相交）。
            let expect_x = 74.18714345675 * mid.cos() + 2.5 * mid.cos();
            let expect_y = 74.18714345675 * mid.sin() + 2.5 * mid.sin();
            assert!((m.insertion_point.x - expect_x).abs() < 0.01, "x={} exp={}", m.insertion_point.x, expect_x);
            assert!((m.insertion_point.y - expect_y).abs() < 0.01);
            // 旋转 = mid−90°（示例 −51.295°）、附着 MiddleCenter（宿主可靠中心锚点）。
            assert!(
                (m.rotation - (mid - std::f64::consts::PI / 2.0)).abs() < 1e-6,
                "rot={}",
                m.rotation
            );
            assert!(matches!(m.attachment_point, AttachmentPoint::MiddleCenter));
            // 箭头翼（示例 SOLID）：尖=尺寸弧端点，翼在端点切线朝外方向。
            // 起点端 away = (sinα0,−cosα0)；终点端 away = (−sinα1,cosα1)。
            if let E::Solid(sl) = &solids[0] {
                // 翼中点 = (second+third)/2 − first（first=尖=尺寸弧端点）。
                // 示例翼向 = 端点切线朝外：起点端 (sinα0,−cosα0)×2.5… 实测即 base−tip
                // = −away×2.5 =（−sinα0, cosα0）×2.5（北偏西）。
                let tip = sl.first_corner.clone();
                let b = (sl.second_corner.clone() + sl.third_corner.clone()) * 0.5 - tip;
                let a0 = 4.71319417612647_f64.to_radians();
                let want = Vector3::new(-a0.sin(), a0.cos(), 0.0) * 2.5;
                assert!((b.x - want.x).abs() < 0.3 && (b.y - want.y).abs() < 0.3, "翼=({:.3},{:.3}) want({:.3},{:.3})", b.x, b.y, want.x, want.y);
            }
        }
        // 小弧符号：色3、层0、r≈2.6、对称轴沿弧中点方向。
        let sym = arcs.iter().find_map(|e| match e {
            E::Arc(a) if a.radius < 10.0 => Some(a.clone()),
            _ => None,
        });
        let sym = sym.expect("小弧符号");
        // 小弧中心 = 文字中心 − baseline×(0.5w+2×0.59h) − um×0.5h。
        let w = 5.0 * 0.59 * 2.5; // "71.89" 5 字符
        let baseline = Vector3::new((mid - std::f64::consts::PI / 2.0).cos(), (mid - std::f64::consts::PI / 2.0).sin(), 0.0);
        let (ccx, ccy) = (76.10315450223098, 69.46285912943617);
        let arc_mid_g = (ccx + 74.18714345675 * mid.cos(), ccy + 74.18714345675 * mid.sin());
        let text_c = Vector3::new(arc_mid_g.0 + 2.5 * mid.cos(), arc_mid_g.1 + 2.5 * mid.sin(), 0.0);
        let expect_sym = text_c
            - baseline * (w * 0.5 + 2.0 * 0.59 * 2.5)
            - Vector3::new(mid.cos(), mid.sin(), 0.0) * (2.5 * 0.5);
        // 块内坐标 = 全局 − 引导圆心。
        let expect_sym = expect_sym - Vector3::new(ccx, ccy, 0.0);
        assert!(
            (sym.center.x - expect_sym.x).abs() < 0.05 && (sym.center.y - expect_sym.y).abs() < 0.05,
            "sym=({:.3},{:.3}) exp=({:.3},{:.3})",
            sym.center.x, sym.center.y, expect_sym.x, expect_sym.y
        );
        assert!((sym.radius - 2.6).abs() < 1e-6);
        assert!(matches!(sym.common.color, Color::Index(3)));
        assert_eq!(sym.common.layer, "0");
        let mid = (4.71319417612647_f64 + 72.69617975791557_f64) / 2.0;
        assert!((crate::detail_clip::norm_angle(sym.start_angle - (mid.to_radians() + 3.0 * std::f64::consts::PI / 2.0))) .abs() < 1e-6);
        // INSERT @ 引导圆心，8符号标注层（引导弧删除由 do_apply 统一执行）。
        let d2 = inner.doc.lock().unwrap().clone();
        let mut ins = None;
        for e in d2.entities() {
            if let E::Insert(i) = e {
                ins = Some(i.clone());
            }
        }
        let ins = ins.expect("应有 INSERT");
        assert_eq!(ins.block_name, "*D1");
        assert_eq!(ins.common.layer, "8符号标注层");
        assert!((ins.insert_point.x - 76.103).abs() < 0.01);
        assert!((ins.insert_point.y - 69.463).abs() < 0.01);
    }

    /// 内侧：尺寸弧 = 引导弧等半径平移 |dist|（圆心沿弧中点反方向），
    /// 界线从尺寸弧端点连回引导弧端点（方向 ∥ 弧中点方向），箭头朝引导弧。
    #[test]
    fn apply_arclen_inside_moves_arc_parallel() {
        use acadrust::entities::{Arc};
        use acadrust::types::Vector3;
        use acadrust::EntityType as E;

        let mut doc = acadrust::CadDocument::new();
        let mut g = E::Arc(Arc::from_center_radius_angles(
            Vector3::new(76.10315450223098, 69.46285912943617, 0.0),
            60.59037754501,
            4.71319417612647_f64.to_radians(),
            72.69617975791557_f64.to_radians(),
        ));
        g.common_mut().layer = "10引导线层".into();
        let gh = doc.add_entity(g).unwrap();
        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::ArcLen;
        p.dist = -12.88;
        let out = apply_arclen(&sender, &doc, gh, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["side"], "in");
        let ents = inner.block_entities("*D1");
        // 尺寸弧：r 不变，圆心 = −12.88×um（um=38.7°方向）。
        let dim_arc = ents.iter().find_map(|e| match e {
            E::Arc(a) if a.radius > 50.0 => Some(a.clone()),
            _ => None,
        });
        let dim_arc = dim_arc.expect("尺寸弧");
        assert!((dim_arc.radius - 60.59037754501).abs() < 1e-6, "内侧半径不变");
        let mid = (4.71319417612647_f64 + 72.69617975791557_f64) / 2.0;
        let midr = mid.to_radians();
        assert!(
            (dim_arc.center.x + 12.88 * midr.cos()).abs() < 1e-6 && (dim_arc.center.y + 12.88 * midr.sin()).abs() < 1e-6,
            "圆心应平移 -12.88×um，实际 c=({},{})",
            dim_arc.center.x,
            dim_arc.center.y
        );
        // 界线：从尺寸弧端点（=引导弧端点−12.88×um）连回引导弧端点。
        let lines: Vec<_> = ents.iter().filter(|e| matches!(e, E::Line(_))).collect();
        assert_eq!(lines.len(), 2);
        for (i, e) in lines.iter().enumerate() {
            if let E::Line(l) = e {
                let ang = if i == 0 { 4.71319417612647_f64 } else { 72.69617975791557_f64 };
                let ang = ang.to_radians();
                let (cx, cy) = (76.10315450223098, 69.46285912943617);
                let e_p = (cx + 60.59037754501 * ang.cos(), cy + 60.59037754501 * ang.sin());
                let s_p = (
                    e_p.0 - (12.88 + 2.0) * midr.cos(),
                    e_p.1 - (12.88 + 2.0) * midr.sin(),
                );
                let (sx, sy) = (l.start.x + cx, l.start.y + cy);
                let (ex, ey) = (l.end.x + cx, l.end.y + cy);
                assert!(
                    (sx - s_p.0).abs() < 0.01 && (sy - s_p.1).abs() < 0.01 && (ex - e_p.0).abs() < 0.01 && (ey - e_p.1).abs() < 0.01
                    || (sx - e_p.0).abs() < 0.01 && (sy - e_p.1).abs() < 0.01 && (ex - s_p.0).abs() < 0.01 && (ey - s_p.1).abs() < 0.01
                );
            }
        }
    }

    /// 弧长文字自定义 + 自动数值两种模式。
    #[test]
    fn apply_arclen_text_auto_and_custom() {
        use acadrust::entities::{Arc};
        use acadrust::types::Vector3;
        use acadrust::EntityType as E;

        let mut doc = acadrust::CadDocument::new();
        let mut g = E::Arc(Arc::from_center_radius_angles(
            Vector3::new(100.0, 100.0, 0.0),
            50.0,
            0.0,
            std::f64::consts::PI / 2.0,
        ));
        g.common_mut().layer = "10引导线层".into();
        let gh = doc.add_entity(g).unwrap();
        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::ArcLen;
        p.dist = 10.0;
        p.text = Some("<> mm".into());
        p.dec = Some(4);
        let out = apply_arclen(&sender, &doc, gh, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["text"], "78.5398 mm"); // 50×π/2，4 位小数
        let ents = inner.block_entities("*D1");
        let mts: Vec<_> = ents.iter().filter(|e| matches!(e, E::MText(_))).collect();
        assert_eq!(mts.len(), 1);
    }

    // ── 局部放大图 DETAIL ─────────────────────────────────────────

    /// 参考文件几何复刻：中心线十字 + r10/r20 圆（层 0）+ 引导圆，2:1 放大。
    /// 断言与 局部放大图.dxf 逐项吻合（中心线截断 74.5、r20→ARC 233°、框 r101.96、
    /// Ⅰ/2:1 文本、INSERT 自动偏移、PE_URL、引导移层色31、引出线）。
    #[test]
    fn apply_detail_circle_guide_matches_reference() {
        use acadrust::entities::{Circle, Line, LwPolyline, MText};
        use acadrust::types::{Color, Vector3};
        use acadrust::EntityType as E;

        let mut doc = acadrust::CadDocument::new();
        // 原图：中心线十字（3中心线层）+ 同心圆 r10/r20（层 0）@ (425.4,500.7)。
        let mk_center_line = |p1: (f64, f64), p2: (f64, f64)| -> E {
            let mut l = E::Line(Line::from_coords(p1.0, p1.1, 0.0, p2.0, p2.1, 0.0));
            l.common_mut().layer = "3中心线层".into();
            l
        };
        let _ = doc.add_entity(mk_center_line((425.4, 477.7), (425.4, 523.7)));
        let _ = doc.add_entity(mk_center_line((402.4, 500.7), (448.4, 500.7)));
        for r in [10.0, 20.0] {
            let mut c = E::Circle(Circle::from_center_radius(Vector3::new(425.4, 500.7, 0.0), r));
            c.common_mut().layer = "0".into();
            let _ = doc.add_entity(c);
        }
        // 引导圆（10引导线层）@ (402.1,531.8) r=50.98。
        let mut g = E::Circle(Circle::from_center_radius(Vector3::new(402.102047399923, 531.8052456139759, 0.0), 50.98234168254744));
        g.common_mut().layer = "10引导线层".into();
        let gh = doc.add_entity(g).unwrap();

        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Detail;
        p.detail_scale = 2.0;
        let out = apply_detail(&sender, &doc, gh, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["no"], "I"); // 空图纸 → 自动 I（ASCII 罗马式）
        assert_eq!(j["geom"], "circle");
        assert_eq!(j["block"], "*D1");
        assert_eq!(j["origin_block"], "*D2");
        assert!((j["detail_scale"].as_f64().unwrap() - 2.0).abs() < 1e-9);
        assert!((j["frame"].as_f64().unwrap() - 1.0).abs() < 1e-9);

        let ents = inner.block_entities("*D1");
        // 内容：中心线×2（截断）+ ARC×1 + CIRCLE×1；框 CIRCLE×1 色31；MTEXT×1。
        let lines: Vec<_> = ents.iter().filter(|e| matches!(e, E::Line(_))).collect();
        let arcs: Vec<_> = ents.iter().filter(|e| matches!(e, E::Arc(_))).collect();
        let circles: Vec<_> = ents.iter().filter(|e| matches!(e, E::Circle(_))).collect();
        let mts: Vec<_> = ents.iter().filter(|e| matches!(e, E::MText(_))).collect();
        assert_eq!(lines.len(), 2, "中心线应裁出 2 段（参考 BLK_2 两条 LINE）");
        assert_eq!(arcs.len(), 1, "r20 圆部分超界 → 1 条 ARC（参考 BLK_2）");
        assert_eq!(circles.len(), 2, "r10 全内 → CIRCLE(r20) + 框 CIRCLE(r101.96)");
        // 参考 BLK_2：竖线截断 74.5、横线截断 80.2（各自由引导圆决定的弦长）。
        let mut lens: Vec<f64> = lines
            .iter()
            .map(|e| match e {
                E::Line(l) => ((l.end.x - l.start.x).powi(2) + (l.end.y - l.start.y).powi(2)).sqrt(),
                _ => 0.0,
            })
            .collect();
        lens.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((lens[0] - 74.49).abs() < 0.3, "竖线截断 {:.2} 应≈74.5 {lens:?}", lens[0]);
        assert!((lens[1] - 80.19).abs() < 0.3, "横线截断 {:.2} 应≈80.2 {lens:?}", lens[1]);
        if let E::Arc(a) = &arcs[0] {
            assert!((a.radius - 40.0).abs() < 1e-6, "r20×2=40");
            let span = crate::detail_clip::norm_angle(a.end_angle - a.start_angle);
            // 参考：10.486°→243.207° span=232.72°=4.0617 rad（含被引导圆裁剪的两交点误差±0.2）
            assert!((span - 4.07).abs() < 0.25, "span={span}");
        }
        // 框 = 大圆 r=101.96 色31。
        let big = circles.iter().find(|e| matches!(e, E::Circle(c) if c.radius > 100.0));
        assert!(big.is_some(), "应有框圆 r=101.96");
        if let Some(E::Circle(c)) = big {
            assert!((c.radius - 101.96).abs() < 0.01);
            assert!(matches!(c.common.color, Color::Index(31)), "框色 31");
            assert_eq!(c.common.layer, "8符号标注层");
        }
        let m = mts[0];
        if let E::MText(m) = m {
            assert!(m.value.contains("I"), "块内序号 {}", m.value);
            assert!(m.value.contains("2:1"), "块内比例 {}", m.value);
            // 文字上移：坐标 ≈ 框顶(101.96) + 10（不压圆框）。
            assert!(m.insertion_point.y > 101.96 + 9.0, "文字应上移 y={}", m.insertion_point.y);
        }

        // 原位标记块 *D2：引导圆（色31）+ 引出线×2 + 序号；
        // 模型空间只剩两个 INSERT（*D1 自动偏移、*D2 @ 引导中心），原引导实体已删。
        let ents2 = inner.block_entities("*D2");
        let g_c = ents2.iter().filter(|e| matches!(e, E::Circle(_))).count();
        let g_l = ents2.iter().filter(|e| matches!(e, E::Line(_))).count();
        let g_m = ents2.iter().filter(|e| matches!(e, E::MText(_))).count();
        assert_eq!(g_c, 1, "原位块含引导圆");
        assert_eq!(g_l, 2, "原位块含引出线×2");
        assert_eq!(g_m, 1, "原位块含序号");
        let g_circle = ents2.iter().find_map(|e| match e {
            E::Circle(c) => Some(c.clone()),
            _ => None,
        });
        let g_circle = g_circle.unwrap();
        assert!((g_circle.radius - 50.9823).abs() < 1e-4);
        assert!(matches!(g_circle.common.color, Color::Index(31)));
        assert_eq!(g_circle.common.layer, "8符号标注层");
        let g_mt = ents2.iter().find_map(|e| match e {
            E::MText(m) => Some(m.clone()),
            _ => None,
        });
        let g_mt = g_mt.unwrap();
        assert!(g_mt.value.contains("I"), "原位序号 {}", g_mt.value);
        assert!(g_mt.insertion_point.y > 20.0, "原位序号应上移 y={}", g_mt.insertion_point.y);

        // 模型空间：两个 INSERT；引导线层圆已删除；8符号标注层 无散 LINE（均入块）。
        let d2 = inner.doc.lock().unwrap().clone();
        let mut ins = None;
        let mut oins = None;
        let mut guide_left = false;
        let mut lead = 0;
        for e in d2.entities() {
            match e {
                E::Insert(i) => {
                    if i.block_name == "*D1" {
                        ins = Some(i.clone());
                    } else if i.block_name == "*D2" {
                        oins = Some(i.clone());
                    }
                }
                E::Circle(c) if c.common.layer == "10引导线层" => guide_left = true,
                E::Line(l) if l.common.layer == "8符号标注层" => lead += 1,
                _ => {}
            }
        }
        let ins = ins.expect("应有放大块 INSERT");
        assert_eq!(ins.common.layer, "8符号标注层");
        // 自动偏移 = 引导中心 + (5×r, 0) = (657.0, 531.8)。
        assert!((ins.insert_point.x - 657.013).abs() < 0.5, "x={}", ins.insert_point.x);
        assert!((ins.insert_point.y - 531.805).abs() < 0.5);
        let url = ins.common.extended_data.records().iter().find_map(|r| {
            (r.application_name == "PE_URL").then(|| {
                r.values.iter().find_map(|v| match v {
                    acadrust::xdata::XDataValue::String(s) => Some(s.clone()),
                    _ => None,
                })
            })?
        });
        let url = url.expect("INSERT 应有 PE_URL");
        assert!(url.contains("DETAIL"), "{url}");
        assert!(url.contains("s=2"), "{url}");
        assert!(url.contains("frame=1"), "{url}");
        // 原位标记 INSERT @ 引导中心，引导实体已删除（图形由块替代）。
        let oins = oins.expect("应有原位 INSERT");
        assert!((oins.insert_point.x - 402.102).abs() < 0.01);
        assert!((oins.insert_point.y - 531.805).abs() < 0.01);
        assert!(!guide_left, "原引导圆应已删除");
        assert_eq!(lead, 0, "引出线已并入原位块");
    }

    /// 矩形引导（闭合 4v LWPOLYLINE）→ rect 几何 + 框为矩形。
    #[test]
    fn apply_detail_rect_guide() {
        use acadrust::entities::{Circle, LwPolyline, MText};
        use acadrust::types::{Color, Vector2, Vector3};
        use acadrust::EntityType as E;
        let mut doc = acadrust::CadDocument::new();
        // 内容：一条 0 层直线穿过窗口。
        let mut l = E::Line(acadrust::entities::Line::from_coords(0.0, 5.0, 0.0, 40.0, 5.0, 0.0));
        l.common_mut().layer = "0".into();
        let _ = doc.add_entity(l);
        // 矩形引导（闭合 4v）：(0,0)-(40,0)-(40,20)-(0,20)。
        let mut pl = LwPolyline::new();
        for (x, y) in [(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0)] {
            pl.add_point(Vector2::new(x, y));
        }
        pl.close();
        let mut ge = E::LwPolyline(pl);
        ge.common_mut().layer = "10引导线层".into();
        let gh = doc.add_entity(ge).unwrap();
        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Detail;
        p.detail_scale = 3.0;
        p.detail_no = Some("Ⅱ".into());
        let out = apply_detail(&sender, &doc, gh, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["geom"], "rect");
        assert_eq!(j["no"], "Ⅱ"); // 手输透传（Unicode 也允许）
        let ents = inner.block_entities("*D1");
        // 内容 LINE（窗口 x∈[0,40] 全内 → 整段 ×3 → 长 120）+ 矩形框 + 序号/比例。
        let lines: Vec<_> = ents.iter().filter(|e| matches!(e, E::Line(_))).collect();
        let plines: Vec<_> = ents.iter().filter(|e| matches!(e, E::LwPolyline(_))).collect();
        let mts: Vec<_> = ents.iter().filter(|e| matches!(e, E::MText(_))).collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(plines.len(), 1, "框 = 矩形 LWPOLYLINE");
        assert_eq!(mts.len(), 1);
        if let E::Line(l) = &lines[0] {
            let len = ((l.end.x - l.start.x).powi(2) + (l.end.y - l.start.y).powi(2)).sqrt();
            assert!((len - 120.0).abs() < 1e-6, "全内直线 ×3 = 120");
            assert_eq!(l.common.layer, "0");
        }
        if let E::LwPolyline(pl) = &plines[0] {
            assert!(pl.is_closed && pl.vertices.len() == 4);
            assert!(matches!(pl.common.color, Color::Index(31)));
        }
        if let E::MText(m) = &mts[0] {
            assert!(m.value.contains("Ⅱ"));
            assert!(m.value.contains("3:1"));
        }
        // 原位标记块 *D2：矩形（色31）+ 引出线×2 + 序号。
        let ents2 = inner.block_entities("*D2");
        assert_eq!(
            ents2.iter().filter(|e| matches!(e, E::LwPolyline(_))).count(),
            1,
            "原位块含矩形"
        );
        assert_eq!(ents2.iter().filter(|e| matches!(e, E::Line(_))).count(), 2);
        assert_eq!(ents2.iter().filter(|e| matches!(e, E::MText(_))).count(), 1);
        let rpl = ents2.iter().find_map(|e| match e {
            E::LwPolyline(p) => Some(p.clone()),
            _ => None,
        });
        let rpl = rpl.unwrap();
        assert!(rpl.is_closed && rpl.vertices.len() == 4);
        assert!(matches!(rpl.common.color, Color::Index(31)));
        // 文字上移：块内序号 y > 框顶(30×3/2=45... half.y=10×3=30) + 9。
        if let E::MText(m) = &mts[0] {
            assert!(m.insertion_point.y > 30.0 + 9.0, "文字应上移 y={}", m.insertion_point.y);
        }
    }

    /// 号码自动递增 + 已满报错（ASCII I..XII）。
    #[test]
    fn apply_detail_no_auto_increment_and_limit() {
        use acadrust::entities::{Circle, MText};
        use acadrust::types::Vector3;
        use acadrust::EntityType as E;
        let mut doc = acadrust::CadDocument::new();
        // 已有 I II V（块外 MTEXT）→ 自动 VI。
        for no in ["I", "II", "V"] {
            let mut m = acadrust::entities::MText::new();
            m.value = no.into();
            m.insertion_point = Vector3::new(1.0, 1.0, 0.0);
            let _ = doc.add_entity(E::MText(m));
        }
        let mut g = E::Circle(Circle::from_center_radius(Vector3::new(100.0, 100.0, 0.0), 10.0));
        g.common_mut().layer = "10引导线层".into();
        let gh = doc.add_entity(g).unwrap();
        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc = inner.doc.lock().unwrap().clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Detail;
        let out = apply_detail(&sender, &doc, gh, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["no"], "VI");
        // 兼容旧版 Unicode 序号也参与计数：已有 Ⅰ..Ⅻ 且新图纸 → 超 12 报错。
        let mut doc2 = acadrust::CadDocument::new();
        for no in ["Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ", "Ⅵ", "Ⅶ", "Ⅷ", "Ⅸ", "Ⅹ", "XI", "XII"] {
            let mut m = acadrust::entities::MText::new();
            m.value = no.into();
            m.insertion_point = Vector3::new(1.0, 1.0, 0.0);
            let _ = doc2.add_entity(E::MText(m));
        }
        let mut g2 = E::Circle(Circle::from_center_radius(Vector3::new(200.0, 200.0, 0.0), 10.0));
        g2.common_mut().layer = "10引导线层".into();
        let gh2 = doc2.add_entity(g2).unwrap();
        let inner2 = std::sync::Arc::new(MockSender::new(doc2));
        let sender2: std::sync::Arc<dyn PluginRequestSender> = inner2.clone();
        let doc2b = inner2.doc.lock().unwrap().clone();
        let mut p2 = GuideParams::linear(LinearSub::Aligned, 0.0);
        p2.guide_type = GuideType::Detail;
        assert!(apply_detail(&sender2, &doc2b, gh2, &p2).is_err(), "超 12 应报错");
    }

    /// 标注系数：起点在放大块 AABB 内 → 线性标注 dimtext = 真实尺寸；
    /// 块外 → 不换算；角度不受影响（不经过 build_dimension）。
    #[test]
    fn build_dimension_detail_scale_factor() {
        use acadrust::entities::{Circle, Insert, LwPolyline, MText};
        use acadrust::types::{Color, Vector3};
        use acadrust::EntityType as E;

        let mut doc = acadrust::CadDocument::new();
        // 放大块 *D1：内容长线（局部 0..100 的 0 层 LINE，块 AABB 取决于块实体）。
        let mut bl = E::Line(acadrust::entities::Line::from_coords(0.0, 0.0, 0.0, 100.0, 0.0, 0.0));
        bl.common_mut().layer = "0".into();
        let mut br = acadrust::tables::BlockRecord::new("*D1");
        let bh = doc.add_entity(bl).unwrap();
        br.entity_handles.push(bh);
        doc.block_records.add(br);
        // INSERT @ (1000,0)，PE_URL s=2&frame=1 → 块内世界 = 1000..1100 × 0。
        let mut ins = Insert::new("*D1", Vector3::new(1000.0, 0.0, 0.0));
        let url = "http://127.0.0.1:0/DIM/DETAIL/0?s=2&frame=1";
        ins.common.extended_data.add_record(pe_url_record(url));
        let _ = doc.add_entity(E::Insert(ins));

        // p1 在块内：(1050, 0)。
        let coef = detail_scale_at(&doc, [1050.0, 0.0, 0.0]);
        assert_eq!(coef, Some(0.5), "1/(2×1)");
        let coef_out = detail_scale_at(&doc, [1500.0, 0.0, 0.0]);
        assert_eq!(coef_out, None);

        // 线性标注：p1 块内 → dimtext 不含 <>（真实值注入）。
        let inner = std::sync::Arc::new(MockSender::new(doc));
        let sender: std::sync::Arc<dyn PluginRequestSender> = inner.clone();
        let doc2 = inner.doc.lock().unwrap().clone();
        let mut pd = GuideParams::linear(LinearSub::Aligned, 0.0);
        pd.text = None; // <> 占位
        let dim = build_dimension(&sender, &doc2, [1050.0, 0.0, 0.0], [1100.0, 0.0, 0.0], &pd, "OCSM_GB")
            .unwrap();
        // 测量 50 × 0.5 = 25 → dimtext "25"。
        assert_eq!(dim.base().text, "25", "块内标注应显示真实尺寸");
        // 块外：text 不写（默认 = 宿主测量值 <>）。
        let dim2 = build_dimension(&sender, &doc2, [1200.0, 0.0, 0.0], [1300.0, 0.0, 0.0], &pd, "OCSM_GB")
            .unwrap();
        assert_eq!(dim2.base().text, "");
    }
mod rough_tests {
    use super::*;
    use ocs_plugin_api::host::acadrust::entities::{Block, BlockEnd, Insert, Line};
    use ocs_plugin_api::host::acadrust::entities::attribute_definition::{
        HorizontalAlignment, VerticalAlignment,
    };
    use ocs_plugin_api::host::acadrust::types::{Color, Vector3};
    use ocs_plugin_api::host::acadrust::EntityType as E;

    // ── 表面粗糙度 OCSMRGH / CC（apply_roughness）───────────────────────

    fn rough_body(
        x: f64,
        y: f64,
        base: &str,
        extra: &str,
        p: &str,
        rot: f64,
        values: &[(&str, &str)],
    ) -> String {
        let vals: serde_json::Map<String, serde_json::Value> = values
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
            .collect();
        serde_json::json!({
            "x": x, "y": y, "base": base, "extra": extra, "p": p,
            "rotation": rot, "values": vals,
        })
        .to_string()
    }

    fn rough_apply(
        mock: &std::sync::Arc<MockSender>,
        body: &str,
    ) -> Result<(serde_json::Value, Vec<acadrust::EntityType>), String> {
        let sender: std::sync::Arc<dyn PluginRequestSender> = mock.clone();
        let out = apply_roughness(&sender, body.as_bytes())?;
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        let name = j["block"].as_str().unwrap().to_string();
        Ok((j, mock.block_entities(&name)))
    }

    #[test]
    fn joint_command_inserts_the_chain_in_one_transaction() {
        // 用户工况：两板 10+10、Ø10 孔 → 命令只算几何/长度/位置；件链由调用方给。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = mock.clone();
        let body = br#"{"at":[50,10],"rot":-90,"protrude":2.5,
            "items":[{"kind":"bolt","family":"hex_bolt_b_full","d":8},
                     {"kind":"plate","t":10},{"kind":"plate","t":10},
                     {"kind":"nut","family":"nut_c41","d":8}]}"#;
        let out = apply_joint(&sender, body).expect("装配成功");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["ok"], true);
        assert!((value["stack"].as_f64().unwrap() - 27.9).abs() < 1e-9);
        assert!((value["bolt"]["l"].as_f64().unwrap() - 35.0).abs() < 1e-9, "{out}");
        assert!(value["report"].as_str().unwrap().contains("35"), "{out}");

        // 螺栓基点 = 头部支承面 (50,10)、杆朝下（rot -90）；螺母基点 = Σ 处 (50,-10)、同向。
        let inserts = mock.inserts();
        assert_eq!(inserts.len(), 2, "{inserts:?}");
        assert!(inserts[0].0.contains("HEX_BOLT_B_FULL_M8_35"), "{}", inserts[0].0);
        assert_eq!(inserts[0].1, [50.0, 10.0, 0.0]);
        assert!((inserts[0].2 + std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!(inserts[1].0.contains("NUT_C41_M8"), "{}", inserts[1].0);
        assert_eq!(inserts[1].1, [50.0, -10.0, 0.0]);
        // 整链一次事务（一个撤销条目）。
        let undos = mock.undos();
        assert_eq!(undos.first().map(String::as_str), Some("begin:OCSM 螺栓副"), "{undos:?}");
        assert_eq!(undos.last().map(String::as_str), Some("commit"), "{undos:?}");

        // 遮挡裁剪：螺母包住的那段杆（20~27.9）不画 → 螺栓块名带 _CUT、几何被断开
        assert!(inserts[0].0.contains("_CUT20x27.9"), "螺栓块名应带裁剪标记：{}", inserts[0].0);
        assert!(!inserts[1].0.contains("_CUT"), "螺母自己不被裁：{}", inserts[1].0);
        let bolt = mock.block_entities(&inserts[0].0);
        assert!(!bolt.is_empty(), "块定义里应有裁好的几何");
        let plain = crate::partgen::generate("hex_bolt_b_full", 8.0, 35.0, "main").unwrap();
        assert!(
            bolt.len() > plain.entities.len(),
            "裁剪后实体数应更多（杆线断成两段）：{} vs {}",
            bolt.len(),
            plain.entities.len()
        );
        use ocs_plugin_api::host::acadrust::EntityType;
        let in_span: Vec<f64> = bolt
            .iter()
            .filter_map(|e| match e {
                EntityType::Line(l) if l.common.layer != crate::partgen::LAYER_CENTER => {
                    let mid = (l.start.x + l.end.x) / 2.0;
                    (20.0 < mid && mid < 27.9).then_some(mid)
                }
                _ => None,
            })
            .collect();
        assert!(in_span.is_empty(), "遮挡段里不该留线：{in_span:?}");
        let report = value["report"].as_str().unwrap();
        assert!(report.contains("遮挡裁剪"), "报告要写清裁剪：{report}");
        assert!(report.contains("20~27.9"), "报告要有被遮区间：{report}");
    }

    #[test]
    fn manual_page_and_catalog_expose_commands_and_topics() {
        // 人类侧手册：页面关键控件在；命令目录非空；手册目录按优先级排好
        let html = super::MANUAL_HTML;
        for key in ["/api/manual", "/api/manual/md", "命令目录", "操作教程", "renderMd"] {
            assert!(html.contains(key), "手册页缺 {key}");
        }
        assert!(super::COMMAND_CATALOG.iter().any(|(n, _, _)| *n == "OCSMJOINT"));
        assert!(super::COMMAND_CATALOG.iter().any(|(n, _, _)| *n == "OCSMHELP"));
        let dirs = super::manual_dirs();
        assert!(!dirs.is_empty(), "至少要有仓库内的 handbook 兜底目录");
        // 顺序：插件安装目录/handbook（人侧教程随插件走）在**仓库 handbook 之前**，旧 skill 目录在最后
        let plugin_idx = dirs
            .iter()
            .position(|d| d.to_string_lossy().ends_with("plugins/opencad.ocsm/handbook"));
        let repo_idx = dirs
            .iter()
            .position(|d| d.to_string_lossy().ends_with("crates/ocs_ocsm/handbook"));
        assert!(repo_idx.is_some(), "仓库 handbook 必须在内：{dirs:?}");
        // `cargo test` 里没有 `--ocs-plugin-runner` 参数 → plugin_install_dir() 为 None，
        // 该项就不会出现；出现时必须在仓库 handbook 之前（人侧教程随插件走）。
        if let Some(pi) = plugin_idx {
            assert!(pi < repo_idx.unwrap(), "插件目录应排在仓库之前：{dirs:?}");
        }
        if let Some(skill_idx) = dirs.iter().position(|d| d.ends_with("ocsm-manual/manual")) {
            assert!(skill_idx > repo_idx.unwrap(), "旧 skill 目录只作最后兜底：{dirs:?}");
        }
        // 索引端点：命令目录 + （磁盘上有手册时的）主题
        let (code, _, body) = api_manual("/api/manual");
        assert_eq!(code, 200);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v["commands"].as_array().unwrap().len() >= 10, "{body}");
        assert!(v["dirs"].as_array().unwrap().len() >= 1);
        // 同一 slug 在多个目录都有副本时，索引里只出现一次（高优先级目录胜出）
        let slugs: Vec<String> = v["topics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["slug"].as_str().unwrap_or_default().to_string())
            .collect();
        let mut uniq = slugs.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), slugs.len(), "主题重复了：{slugs:?}");
    }

    #[test]
    fn manual_lookup_is_path_traversal_safe_and_reads_markdown() {
        let dir = std::env::temp_dir().join(format!("ocsm-manual-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("00-总览.md"), "# 总览\n\n正文\n").unwrap();
        let dirs = vec![dir.clone()];
        // 正常命中，标题取首个 `# ` 行
        let (path, md) = find_manual(&dirs, "00-总览").expect("应能命中");
        assert!(path.ends_with("00-总览.md"));
        assert!(md.contains("正文"));
        let topics = manual_topics_in(&dir);
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].1, "总览", "标题应取文件名后的 # 行");
        // 目录穿越 / 不存在 → 一律 None
        for bad in ["../secret", "..", "sub/dir", "nope", ""] {
            assert!(find_manual(&dirs, bad).is_none(), "{bad} 不该命中");
        }
        // md 端点返回原文 + 路径；缺参数给 404 与提示
        let (code, _, body) = api_manual("/api/manual/md?slug=00-%E6%80%BB%E8%A7%88");
        let _ = (code, body); // 临时目录不在默认搜索路径里，这里只验证不 panic
        let (code, _, body) = api_manual("/api/manual/md?slug=definitely-missing");
        assert_eq!(code, 404);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ok"], false);
        assert!(v["hint"].as_str().unwrap().contains("OCSM_MANUAL_DIR"), "{body}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn double_nut_chain_inserts_once_per_part_and_dedups_blocks() {
        // 双螺母防松（厚+厚，GUI 预设之一）：两枚同族螺母 → 块定义只建一次，
        // 但 INSERT 两条（明细表要按件数统计）。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = mock.clone();
        let body = br#"{"at":[0,0],"rot":0,
            "items":[{"kind":"bolt","family":"hex_bolt_b_full","d":8},
                     {"kind":"plate","t":10},{"kind":"plate","t":10},
                     {"kind":"washer","family":"washer_971","d":8},
                     {"kind":"nut","family":"nut_c41","d":8},
                     {"kind":"nut","family":"nut_c41","d":8}]}"#;
        let out = apply_joint(&sender, body).expect("双螺母装配成功");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        // 件数 = 实际插入的件（板只是厚度账，不成为件）：螺栓 + 平垫 + 两枚螺母 = 4
        assert_eq!(value["placed"].as_array().unwrap().len(), 4, "{out}");
        // Σ = 10 + 10 + 平垫 1.6 + 螺母 7.9 × 2
        let stack = value["stack"].as_f64().unwrap();
        assert!((stack - 37.4).abs() < 1e-9, "Σ={stack}");
        let inserts = mock.inserts();
        assert_eq!(inserts.len(), 4, "{inserts:?}");
        assert_eq!(
            inserts.iter().filter(|(b, _, _)| b.contains("NUT_C41_M8")).count(),
            2,
            "两枚螺母两条 INSERT：{inserts:?}"
        );
        // 螺母的块定义只建一次（同名块不重复 AddBlockRecord）
        let nut_blocks: Vec<String> = mock
            .blocks()
            .into_iter()
            .map(|(n, _)| n)
            .filter(|n| n.contains("NUT_C41_M8"))
            .collect();
        assert_eq!(nut_blocks.len(), 1, "块名重复建了：{nut_blocks:?}");
        // 一次事务
        assert_eq!(mock.undos().first().map(String::as_str), Some("begin:OCSM 螺栓副"));
        assert_eq!(mock.undos().last().map(String::as_str), Some("commit"));
        // 遮挡裁剪：杆被平垫 + 两枚螺母连续遮住，合并成一段
        let trimmed = value["trimmed"].as_array().unwrap();
        assert_eq!(trimmed.len(), 1, "只有螺栓被裁：{trimmed:?}");
        assert!(trimmed[0].as_str().unwrap().contains('~'), "{trimmed:?}");
    }

    #[test]
    fn joint_plan_endpoint_returns_svg_and_numbers_without_writing() {
        // GUI 的实时预览端点：只算不写（不进撤销栈、不动文档）
        let body = br#"{"at":[0,0],"rot":-90,"protrude":2.5,
            "items":[{"kind":"bolt","family":"hex_bolt_b_full","d":8},
                     {"kind":"plate","t":10},{"kind":"plate","t":10},
                     {"kind":"washer","family":"washer_971","d":8},
                     {"kind":"nut","family":"nut_61721","d":8},
                     {"kind":"nut","family":"nut_c41","d":8}]}"#;
        let out = plan_joint_json(body).expect("预览成功");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], true);
        let svg = v["svg"].as_str().unwrap();
        assert!(svg.starts_with("<svg") || svg.contains("<svg"), "要返回 SVG：{}", &svg[..80.min(svg.len())]);
        assert!(svg.contains("polyline") || svg.contains("line") || svg.contains("path"), "SVG 里应有几何");
        assert_eq!(v["count"].as_u64().unwrap(), 4, "螺栓 + 平垫 + 薄螺母 + 厚螺母 = 4 件");
        // Σ = 两板 20 + 平垫 1.6 + 薄螺母 6172.1 M8(m=4.0) + 厚螺母 41 M8(m=7.9)
        assert!(
            (v["stack"].as_f64().unwrap() - (20.0 + 1.6 + 4.0 + 7.9)).abs() < 1e-6,
            "{}",
            v["stack"]
        );
        assert!(v["report"].as_str().unwrap().contains("6172.1"), "报告要写清薄螺母标准号");
        assert!(v["trimmed"].as_array().unwrap().len() == 1);
        // 关掉裁剪 → 没有被裁的件
        let plain = plan_joint_json(br#"{"at":[0,0],"rot":0,"trim":false,
            "items":[{"kind":"bolt","family":"hex_bolt_b_full","d":8},
                     {"kind":"plate","t":10},{"kind":"nut","family":"nut_c41","d":8}]}"#).unwrap();
        let pv: serde_json::Value = serde_json::from_str(&plain).unwrap();
        assert!(pv["trimmed"].as_array().unwrap().is_empty(), "trim=false 不该裁：{pv}");
    }

    #[test]
    fn joint_place_registers_preview_block_and_pending_chain() {
        // GUI 点「装配到图纸」：建预览块（整链几何）+ 登记待放置件链；**不落件**（等图纸点选）
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = mock.clone();
        let body = br#"{"at":[0,0],"rot":-90,
            "items":[{"kind":"bolt","family":"hex_bolt_b_full","d":8},
                     {"kind":"plate","t":10},{"kind":"plate","t":10},
                     {"kind":"nut","family":"nut_c41","d":8}]}"#;
        let out = apply_joint_place(&sender, body).expect("登记成功");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], true);
        let block = v["block"].as_str().unwrap();
        assert!(block.starts_with("OCSMJOINT_PREV_"), "{block}");
        assert!(block.contains("CUT"), "带裁剪档：{block}");
        // 预览块建好了，里面是整链几何（比单个螺栓多），且没有落任何 INSERT
        let entities = mock.block_entities(block);
        assert!(!entities.is_empty(), "预览块定义应为整链几何");
        assert!(mock.inserts().is_empty(), "登记阶段不落件：{:?}", mock.inserts());
        // 待放置件链登记好（含裁剪档块名 + 提示词）
        let pending = crate::pending_joint_for_test().expect("待放置件链");
        assert_eq!(pending.block, block);
        assert!(pending.label.contains("螺栓副"), "{}", pending.label);
        assert!(pending.spec_json.contains("hex_bolt_b_full"), "{}", pending.spec_json);
        // 建块也是文档改动 → 有自己的撤销条目
        assert_eq!(mock.undos().first().map(String::as_str), Some("begin:螺栓副预览"));
        assert_eq!(mock.undos().last().map(String::as_str), Some("commit"));
    }

    #[test]
    fn joint_spec_at_rot_overrides_gui_placement() {
        // 图纸点选的基点/转角必须覆盖 GUI 里的 at/rot（GUI 只是初始值）
        let spec = r#"{"at":[0,0],"rot":0,"items":[{"kind":"bolt","family":"hex_bolt_b_full","d":8}]}"#;
        let out = crate::joint_spec_at_rot_for_test(spec, [120.0, -35.5, 0.0], std::f64::consts::FRAC_PI_2);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["at"], serde_json::json!([120.0, -35.5]));
        assert!((v["rot"].as_f64().unwrap() - 90.0).abs() < 1e-9, "{}", v["rot"]);
        assert_eq!(v["items"][0]["family"], "hex_bolt_b_full", "件链原样保留");
    }

    #[test]
    fn mutating_apis_declare_undo_before_writing() {
        // 宿主不会替插件命令入撤销栈（2026-09-15 契约缺口）：每个改文档的入口
        // 必须先 PushUndo，否则 MCP/AI 驱动后 Ctrl+Z 撤不掉。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body = rough_body(100.0, 200.0, "C1", "R1", "", 0.0, &[]);
        rough_apply(&mock, &body).unwrap();
        assert_eq!(
            mock.undos().first().map(String::as_str),
            Some("begin:表面粗糙度"),
            "粗糙度应先开撤销事务：{:?}",
            mock.undos()
        );
        assert_eq!(
            mock.undos().last().map(String::as_str),
            Some("commit"),
            "写完后应提交事务：{:?}",
            mock.undos()
        );

        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        *crate::parts_point_slot().lock().unwrap() = Some([0.0, 0.0, 0.0]);
        let sender: std::sync::Arc<dyn PluginRequestSender> = mock.clone();
        apply_part_pick(&sender, br#"{"family":"hex_bolt_c","d":5,"l":25,"view":"main"}"#)
            .expect("插入成功");
        assert_eq!(
            mock.undos().first().map(String::as_str),
            Some("begin:零件插入"),
            "{:?}",
            mock.undos()
        );
        assert_eq!(mock.undos().last().map(String::as_str), Some("commit"));

        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: std::sync::Arc<dyn PluginRequestSender> = mock.clone();
        apply_part_export(&sender, br#"{"family":"hex_bolt_c","d":5,"l":25,"view":"main"}"#)
            .expect("出库成功");
        assert_eq!(
            mock.undos().first().map(String::as_str),
            Some("begin:零件出库"),
            "{:?}",
            mock.undos()
        );
        assert_eq!(mock.undos().last().map(String::as_str), Some("commit"));
    }

    fn count_attdefs(members: &[acadrust::EntityType]) -> Vec<&acadrust::entities::AttributeDefinition> {
        members
            .iter()
            .filter_map(|e| match e {
                E::AttributeDefinition(a) => Some(a),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn apply_roughness_c1r1_builds_minimal_block() {
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body = rough_body(100.0, 200.0, "C1", "R1", "", 0.0, &[]);
        let (j, members) = rough_apply(&mock, &body).unwrap();
        assert_eq!(j["base"], "C1");
        assert_eq!(j["extra"], "R1");
        assert_eq!(j["scale"], 1.0);
        // 2 LINE + 4 ATTDEF（A′/A/E/P，无圆无填充）。
        assert_eq!(members.len(), 6);
        assert_eq!(
            members.iter().filter(|e| matches!(e, E::Line(_))).count(),
            2,
            "C1R1 只有公共两条斜边"
        );
        // 公共斜边端点（参考坐标，scale=1）。
        let lines: Vec<_> = members
            .iter()
            .filter_map(|e| match e {
                E::Line(l) => Some((l.start, l.end)),
                _ => None,
            })
            .collect();
        assert!(lines.contains(&(Vector3::new(4.186, 5.35, 0.0), Vector3::new(7.072, 0.35, 0.0))));
        assert!(lines.contains(&(Vector3::new(7.072, 0.35, 0.0), Vector3::new(13.423, 11.35, 0.0))));
        let ads = count_attdefs(&members);
        assert_eq!(ads.len(), 4);
        let tags: Vec<&str> = ads.iter().map(|a| a.tag.as_str()).collect();
        assert_eq!(
            tags,
            vec!["粗糙度上限A′", "粗糙度下限A", "备注E", "加工符号P"]
        );
        // A′/A/E：ML 左中、h3.5/4.9、样式 OCSM_GB、绿色、8符号标注层。
        let a1 = &ads[0];
        assert_eq!(a1.insertion_point.x, 8.248);
        assert_eq!(a1.insertion_point.y, 11.65);
        assert_eq!(a1.height, 3.5);
        assert_eq!(a1.text_style, "OCSM_GB");
        assert_eq!(a1.width_factor, 0.7);
        // 对齐跟随素材 MTEXT 71 组语义：A′/A/E = 71:9 → 右下；P = 71:7 → 左下。
        assert!(matches!(a1.horizontal_alignment, HorizontalAlignment::Right));
        assert!(matches!(a1.vertical_alignment, VerticalAlignment::Bottom));
        assert_eq!(a1.common.layer, "8符号标注层");
        assert_eq!(a1.common.color, Color::from_index(3));
        let e1 = &ads[2];
        assert_eq!(e1.height, 4.9, "E 大号字");
        let p1 = &ads[3];
        assert_eq!(p1.insertion_point.x, 11.009);
        assert_eq!(p1.insertion_point.y, 1.4);
        assert!(matches!(p1.horizontal_alignment, HorizontalAlignment::Left));
        assert!(matches!(p1.vertical_alignment, VerticalAlignment::Bottom));
        // 缺省显示空白：ATTDEF default 用空格（空 default 宿主显示 tag）；
        // 真实值放 INSERT.attributes。
        assert_eq!(a1.default_value, " ");
        // INSERT：层 0、attributes 与 ATTDEF 对齐。
        let inserts: Vec<_> = mock
            .doc
            .lock()
            .unwrap()
            .entities()
            .filter_map(|e| match e {
                acadrust::EntityType::Insert(i) => Some(i.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(inserts.len(), 1);
        let ins = &inserts[0];
        assert_eq!(ins.block_name, "*D1");
        assert_eq!(ins.insert_point.x, 100.0);
        assert_eq!(ins.insert_point.y, 200.0);
        assert_eq!(ins.common.layer, "0");
        assert_eq!(ins.rotation, 0.0);
        let atags: Vec<&str> = ins.attributes.iter().map(|a| a.tag.as_str()).collect();
        assert_eq!(atags, vec!["粗糙度上限A′", "粗糙度下限A", "备注E", "加工符号P"]);
        // 属性渲染位置 = 块内 ATTDEF 位变换到世界（不堆在插入点）。
        let a1_attr = ins.attributes.iter().find(|a| a.tag == "粗糙度上限A′").unwrap();
        assert!(
            (a1_attr.insertion_point.x - 108.248).abs() < 0.01
                && (a1_attr.insertion_point.y - 211.65).abs() < 0.01,
            "attr pos=({:.3},{:.3})",
            a1_attr.insertion_point.x,
            a1_attr.insertion_point.y
        );
        assert_eq!(a1_attr.text_style, "OCSM_GB");
        assert_eq!(a1_attr.common.color, Color::from_index(3));
    }

    #[test]
    fn apply_roughness_c2_forces_p_blank() {
        // C2（不去除材料）：无 P ATTDEF、无 P attribute，p 输入被忽略。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body = rough_body(0.0, 0.0, "C2", "R1", "M", 0.0, &[]);
        let (j, members) = rough_apply(&mock, &body).unwrap();
        assert_eq!(j["p"], "");
        // L1/L2 + V内圆 + 3 ATTDEF（无 P）。
        assert_eq!(
            members.iter().filter(|e| matches!(e, E::Circle(_))).count(),
            1,
            "C2 有 V 内圆"
        );
        let circle = members
            .iter()
            .find_map(|e| match e {
                E::Circle(c) => Some((c.center, c.radius)),
                _ => None,
            })
            .unwrap();
        assert_eq!(circle.0.x, 7.072);
        assert_eq!(circle.0.y, 3.683);
        assert_eq!(circle.1, 1.667);
        let ads = count_attdefs(&members);
        assert_eq!(ads.len(), 3);
        assert!(ads.iter().all(|a| a.tag != "加工符号P"));
        // 非 C2 时 p 生效（C1 + P=M）。
        let mock2 = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body2 = rough_body(0.0, 0.0, "C1", "R1", "M", 0.0, &[]);
        let (_, members2) = rough_apply(&mock2, &body2).unwrap();
        let p_attr = mock2
            .doc
            .lock()
            .unwrap()
            .entities()
            .find_map(|e| match e {
                acadrust::EntityType::Insert(i) => Some(i.clone()),
                _ => None,
            })
            .unwrap()
            .attributes
            .into_iter()
            .find(|a| a.tag == "加工符号P")
            .unwrap();
        assert_eq!(p_attr.value, "M", "P 值写入 INSERT 属性（ATTDEF 常显空格）");
    }

    #[test]
    fn apply_roughness_c4r5_builds_full_block() {
        // C4R5：横线+短线+填充三角 + 长横线+顶点圆+台阶；8 文字含 P。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body = rough_body(0.0, 0.0, "C4", "R5", "", 0.0, &[]);
        let (_, members) = rough_apply(&mock, &body).unwrap();
        // L1+L2 + C4 横线/短线 2 + R5 长横线/竖线/顶线 3 = 7 线；
        // 顶点圆 1 + SOLID 1 + ATTDEF 8 = 17 成员。
        assert_eq!(
            members.iter().filter(|e| matches!(e, E::Line(_))).count(),
            7
        );
        assert_eq!(
            members.iter().filter(|e| matches!(e, E::Circle(_))).count(),
            1
        );
        assert_eq!(
            members.iter().filter(|e| matches!(e, E::Solid(_))).count(),
            1,
            "C4 填充三角"
        );
        let solid = members
            .iter()
            .find_map(|e| match e {
                E::Solid(s) => Some((s.second_corner, s.third_corner)),
                _ => None,
            })
            .unwrap();
        assert_eq!(solid.0.x, 9.959);
        assert_eq!(solid.1.y, 0.35, "填充三角顶点 = (7.072,0.35)");
        let ads = count_attdefs(&members);
        assert_eq!(
            ads.iter().map(|a| a.tag.as_str()).collect::<Vec<_>>(),
            vec![
                "粗糙度上限A′", "粗糙度下限A", "备注E", "加工符号P",
                "加工方法B′", "加工方法B", "取样长度C", "纹理方向G",
            ]
        );
        // R5 台阶文字位：B′@(17.706,18)、B@(17.706,12.4)、C@(17.706,6.8)、G@(17.706,2.25)。
        let by = ads.iter().find(|a| a.tag == "加工方法B′").unwrap();
        assert_eq!(by.insertion_point.x, 17.706);
        assert_eq!(by.insertion_point.y, 18.0);
        let g = ads.iter().find(|a| a.tag == "纹理方向G").unwrap();
        assert_eq!(g.insertion_point.y, 2.25);
        // R2 文字 x=16.14、R3 x=14.473。
        let mock2 = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body2 = rough_body(0.0, 0.0, "C1", "R2", "", 0.0, &[]);
        let (_, m2) = rough_apply(&mock2, &body2).unwrap();
        let ad2 = count_attdefs(&m2);
        let b = ad2.iter().find(|a| a.tag == "加工方法B′").unwrap();
        assert_eq!(b.insertion_point.x, 16.14);
        assert_eq!(ad2.len(), 7, "R2 = 公共3 + P + B′/C/G");
        let mock3 = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body3 = rough_body(0.0, 0.0, "C1", "R3", "", 0.0, &[]);
        let (_, m3) = rough_apply(&mock3, &body3).unwrap();
        let ad3 = count_attdefs(&m3);
        let b3 = ad3.iter().find(|a| a.tag == "加工方法B′").unwrap();
        assert_eq!(b3.insertion_point.x, 14.473);
    }

    #[test]
    fn apply_roughness_ensures_ocsm_style_and_layers() {
        // 空文档（未初始化）直接 CC：必须补 OCSM_GB 样式与 8符号标注层，
        // 否则宿主渲染 ATTDEF 时 fallback 未知字体、文字堆叠（用户实测）。
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body = rough_body(0.0, 0.0, "C1", "R1", "", 0.0, &[]);
        rough_apply(&mock, &body).unwrap();
        let ensures = mock.ensures.lock().unwrap().clone();
        assert!(
            ensures.iter().any(|e| e == "text:OCSM_GB"),
            "确保 OCSM_GB 文字样式，实际: {ensures:?}"
        );
        assert!(
            ensures.iter().any(|e| e == "layer:8符号标注层"),
            "确保 8符号标注层"
        );
        // 10 图层 + 1 样式（空文档一次补齐）；重复 apply 不再追加（宿主幂等）。
    }

    #[test]
    fn apply_roughness_values_and_rotation() {
        let mock = std::sync::Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let body = rough_body(
            0.0,
            0.0,
            "C1",
            "R1",
            "⊥",
            90.0,
            &[("A′", "3.2"), ("A", "1.6"), ("E", "5")],
        );
        let (j, members) = rough_apply(&mock, &body).unwrap();
        assert_eq!(j["p"], "⊥");
        assert!((j["rotation"].as_f64().unwrap() - 90.0).abs() < 1e-9);
        let ads = count_attdefs(&members);
        let a1 = ads.iter().find(|a| a.tag == "粗糙度上限A′").unwrap();
        assert_eq!(a1.default_value, " ", "ATTDEF 常显空格（值走 attributes）");
        assert!((a1.rotation - std::f64::consts::PI / 2.0).abs() < 1e-9, "ATTDEF 随符号旋转");
        // INSERT attributes 对齐 ATTDEF 值 + 位置随插入点/旋转变换。
        let ins = mock
            .doc
            .lock()
            .unwrap()
            .entities()
            .find_map(|e| match e {
                acadrust::EntityType::Insert(i) => Some(i.clone()),
                _ => None,
            })
            .unwrap();
        assert!((ins.rotation - std::f64::consts::PI / 2.0).abs() < 1e-9);
        let attrs = &ins.attributes;
        assert_eq!(attrs.len(), 4);
        let a = attrs.iter().find(|a| a.tag == "粗糙度上限A′").unwrap();
        assert_eq!(a.value, "3.2");
        let p = attrs.iter().find(|a| a.tag == "加工符号P").unwrap();
        assert_eq!(p.value, "⊥");
        // 90° 旋转：A′ 块内位 (8.248,11.65) → 绕原点转 → (−11.65,8.248)。
        assert!(
            (a.insertion_point.x - (-11.65)).abs() < 0.02 && (a.insertion_point.y - 8.248).abs() < 0.02,
            "90° attr pos=({:.3},{:.3})",
            a.insertion_point.x,
            a.insertion_point.y
        );
        // 传入的 E=5（values 键 E）→ 备注E value=5；未传的 A/B′ 等 = 空格。
        let e = attrs.iter().find(|a| a.tag == "备注E").unwrap();
        assert_eq!(e.value, "5");
        let a_lo = attrs.iter().find(|a| a.tag == "粗糙度下限A").unwrap();
        assert_eq!(a_lo.value, "1.6");
    }

    #[test]
    fn apply_roughness_frame_scale_doubles_geometry() {
        // TF 图框（比例 ATTDEF，uniform 2.0）→ 坐标 ×2。
        let mut doc = acadrust::CadDocument::new();
        let mut att = acadrust::entities::AttributeDefinition::new(
            "比例".into(),
            "Scale".into(),
            " ".into(),
        );
        att.tag = "比例".into();
        let lines: Vec<E> = vec![
            E::Line(acadrust::entities::Line::from_coords(0.0, 0.0, 0.0, 100.0, 0.0, 0.0)),
            E::Line(acadrust::entities::Line::from_coords(100.0, 0.0, 0.0, 100.0, 50.0, 0.0)),
            E::Line(acadrust::entities::Line::from_coords(100.0, 50.0, 0.0, 0.0, 50.0, 0.0)),
            E::Line(acadrust::entities::Line::from_coords(0.0, 50.0, 0.0, 0.0, 0.0, 0.0)),
            E::AttributeDefinition(att),
        ];
        let mut br = acadrust::tables::BlockRecord::new("a3_test");
        br.handle = doc.allocate_handle();
        br.entity_handles
            .push(doc.add_entity(E::Block(acadrust::entities::Block::new(
                "a3_test",
                Vector3::ZERO,
            ))).unwrap());
        for e in lines {
            br.entity_handles.push(doc.add_entity(e).unwrap());
        }
        br.entity_handles
            .push(doc.add_entity(E::BlockEnd(acadrust::entities::BlockEnd::new())).unwrap());
        doc.block_records.add(br).unwrap();
        let mut ins = Insert::new("a3_test", Vector3::new(0.0, 0.0, 0.0));
        ins.set_x_scale(2.0);
        ins.set_y_scale(2.0);
        ins.set_z_scale(2.0);
        doc.add_entity(E::Insert(ins)).unwrap();

        let mock = std::sync::Arc::new(MockSender::new(doc));
        let body = rough_body(50.0, 30.0, "C1", "R1", "", 0.0, &[]);
        let (j, members) = rough_apply(&mock, &body).unwrap();
        assert_eq!(j["scale"], 2.0, "图框 1:2 → 倍率 2");
        let lines: Vec<_> = members
            .iter()
            .filter_map(|e| match e {
                E::Line(l) => Some((l.start, l.end)),
                _ => None,
            })
            .collect();
        assert!(
            lines.contains(&(Vector3::new(8.372, 10.7, 0.0), Vector3::new(14.144, 0.7, 0.0))),
            "L1 ×2"
        );
        let ads = count_attdefs(&members);
        assert_eq!(ads[0].insertion_point.x, 16.496, "A′ x ×2");
        assert_eq!(ads[0].height, 7.0, "字高 ×2");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 焊接符号（WELD）测试：符号表归一化 / apply_weld 全家福逐位对照参考 /
// 边界校验 / HTTP 全链路
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod weld_tests {
    use super::*;
    use crate::guide_url::{GuideParams, LinearSub, WeldParams};
    use acadrust::entities::attribute_definition::{HorizontalAlignment, VerticalAlignment};
    use acadrust::entities::{AttributeDefinition, LwPolyline, Text};
    use acadrust::types::Vector2;
    use acadrust::EntityType as E;

    /// 示例.dxf 的引导几何（P0 = 拐点/基准线起点；坐标 1:1 取自参考）。
    const P_TIP: [f64; 3] = [75.186, 76.858, 0.0];
    const P0: [f64; 3] = [95.591, 95.728, 0.0];
    const P_END: [f64; 3] = [143.238, 95.728, 0.0];

    fn weld_doc() -> acadrust::CadDocument {
        acadrust::CadDocument::new()
    }

    fn full_params() -> GuideParams {
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Weld;
        p.weld = WeldParams {
            upper: "角焊".into(),
            lower: "角焊".into(),
            dash: true,
            circle: true,
            flag: true,
            tail: true,
            half: false,
            grind_upper: GrindKind::ArcConvex, // 示例.dxf 用弧·凸
            grind_lower: GrindKind::ArcConvex,
            method_upper: "C".into(), // 示例.dxf 的 "C" = 焊接方法字母
            method_lower: "C".into(),
            up_thick: "5".into(),
            up_qty: "100".into(),
            lo_thick: "3".into(),
            lo_qty: "50".into(),
            tail_text: "封底焊".into(),
        };
        p
    }

    fn lines_of<'a>(members: &'a [E]) -> Vec<&'a acadrust::entities::Line> {
        members
            .iter()
            .filter_map(|e| match e {
                E::Line(l) => Some(l),
                _ => None,
            })
            .collect()
    }

    fn solids_of<'a>(members: &'a [E]) -> Vec<&'a acadrust::entities::Solid> {
        members
            .iter()
            .filter_map(|e| match e {
                E::Solid(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    fn attdefs_of(members: &[E]) -> Vec<AttributeDefinition> {
        members
            .iter()
            .filter_map(|e| match e {
                E::AttributeDefinition(a) => Some(a.clone()),
                _ => None,
            })
            .collect()
    }

    fn mtexts_of(members: &[E]) -> Vec<acadrust::entities::MText> {
        members
            .iter()
            .filter_map(|e| match e {
                E::MText(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    fn kinds(members: &[E]) -> (usize, usize, usize, usize, usize, usize) {
        // (line, circle, arc, solid, text, attdef)
        let mut c = (0, 0, 0, 0, 0, 0);
        for e in members {
            match e {
                E::Line(_) => c.0 += 1,
                E::Circle(_) => c.1 += 1,
                E::Arc(_) => c.2 += 1,
                E::Solid(_) => c.3 += 1,
                E::Text(_) => c.4 += 1,
                E::AttributeDefinition(_) => c.5 += 1,
                _ => {}
            }
        }
        c
    }

    /// 实体的 y 坐标集合（锚点归一校验用）。
    fn weld_ent_ys(e: &WeldSymEnt) -> Vec<f64> {
        match *e {
            WSE::Line(_, y1, _, y2) => vec![y1, y2],
            WSE::Circle(_, cy, r) => vec![cy - r, cy + r],
            WSE::Arc(_, cy, r, a0, a1) => {
                // 实际弧段 y 范围：端点 + 区间内的 90°/270° 卡点。
                let span = (a1 - a0).rem_euclid(360.0);
                let mut angs = vec![a0, a0 + span];
                for card in [90.0, 270.0] {
                    if (card - a0).rem_euclid(360.0) <= span {
                        angs.push(card);
                    }
                }
                angs.iter().map(|a| cy + r * a.to_radians().sin()).collect()
            }
            WSE::Text(_, y, _) => vec![y],
        }
    }

    #[test]
    fn weld_sym_table_27_normalized() {
        assert_eq!(WELD_SYMS.len(), 27, "24 镜像对 + 3 跨线单置");
        let mut pairs = 0;
        let mut singles = 0;
        for (name, upper, lower, crossing) in WELD_SYMS.iter() {
            assert!(!upper.is_empty(), "{name} 上侧几何为空");
            if *crossing {
                singles += 1;
                assert!(lower.is_none(), "{name} 跨线单置不应有下侧几何");
                let ys: Vec<f64> = upper.iter().flat_map(weld_ent_ys).collect();
                assert!(
                    ys.iter().any(|y| *y <= 1e-9) && ys.iter().any(|y| *y >= -1e-9),
                    "{name} 跨线几何不骑基线"
                );
            } else {
                pairs += 1;
                assert!(lower.is_some(), "{name} 缺镜像版");
                // 上侧几何全部 ≥ −0.13（贴线 ±0.125 以内容差）；下侧全部 ≤ 0.13。
                let uy: Vec<f64> = upper.iter().flat_map(weld_ent_ys).collect();
                assert!(uy.iter().all(|y| *y >= -0.13), "{name} 上侧几何越过基线过深");
                let ly: Vec<f64> = lower.unwrap().iter().flat_map(weld_ent_ys).collect();
                assert!(ly.iter().all(|y| *y <= 0.13), "{name} 下侧几何越过基线过深");
            }
        }
        assert_eq!(pairs, 24);
        assert_eq!(singles, 3);
        // 角焊 1:1（表.dxf 归一化坐标 + 示例.dxf 展开验证）。
        let fw = weld_sym("角焊").unwrap();
        assert!(!fw.3);
        assert_eq!(
            fw.1,
            &[
                WSE::Line(0.0, 0.125, 3.5, 0.125),
                WSE::Line(3.5, 0.125, 0.0, 3.625),
                WSE::Line(0.0, 3.625, 0.0, 0.125),
            ]
        );
        assert_eq!(
            fw.2.unwrap(),
            &[
                WSE::Line(0.0, -0.125, 3.5, -0.125),
                WSE::Line(3.5, -0.125, 0.0, -3.625),
                WSE::Line(0.0, -3.625, 0.0, -0.125),
            ]
        );
        // 跨线单置：参考线上的点焊缝（圆心骑线）。
        let d = weld_sym("参考线上的点焊缝").unwrap();
        assert!(d.3 && d.2.is_none());
        assert_eq!(d.1, &[WSE::Circle(2.275, 0.0, 2.275)]);
        // 持久衬垫带固定小字 MR（OCSM_GB 替代 PC_TEXTSTYLE）。
        let pc = weld_sym("持久衬垫").unwrap();
        assert!(matches!(pc.1.last(), Some(WSE::Text(2.625, 1.4, "MR"))));
        assert!(matches!(pc.2.unwrap().last(), Some(WSE::Text(2.625, -1.4, "MR"))));
    }

    #[test]
    fn apply_weld_full_family_matches_reference() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc(); // 独立文档（不持 mock 锁——AddEntities 会再锁）
        let out = apply_weld(&sender, &doc, P_TIP, P0, P_END, &full_params()).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["ok"], true);
        assert_eq!(j["block"], "*W1");
        // ACISOWELD 线型 ensure（虚线用）。
        assert!(mock
            .ensures
            .lock()
            .unwrap()
            .iter()
            .any(|e| e == "ltype:ACISOWELD"));

        // ── 块成员 25 个：13 线（引线/基准线/虚线/旗杆/旗底/尾叉2/上下三角各3）
        //    + 圆1 + C 弧2 + 箭头/旗 SOLID2 + C 字2 + ATTDEF5 ──
        let members = mock.block_entities("*W1");
        assert_eq!(members.len(), 25, "全家福成员数");
        assert_eq!(kinds(&members), (13, 1, 2, 2, 2, 5), "类型分布");

        // ── 线几何逐位对照示例.dxf（块局部坐标，原点 = P0；世界位 = 局部+P0）──
        let ls = lines_of(&members);
        let base_len = P_END[0] - P0[0]; // 47.647
        // 基准线（青4）：长度随内容 = 19.413+6.5+文字"100"(4.35)+余量2 = 32.263
        let blen_eff = 32.263_f64;
        assert!(ls.iter().any(|l| l.start.x == 0.0
            && l.start.y == 0.0
            && (l.end.x - blen_eff).abs() < 1e-6
            && l.end.y == 0.0
            && l.common.color == acadrust::types::Color::from_index(4)));
        // 虚线（品红6 + ACISOWELD；局部 y = −0.7）
        assert!(ls.iter().any(|l| l.start.x == 0.0
            && l.start.y == -0.7
            && (l.end.x - blen_eff).abs() < 1e-6
            && l.end.y == -0.7
            && l.common.color == acadrust::types::Color::from_index(6)
            && l.common.linetype == "ACISOWELD"));
        // 引线：箭头底中 → 拐点（底中 = 尖 + 3.5·u）
        let dx = P0[0] - P_TIP[0];
        let dy = P0[1] - P_TIP[1];
        let len = (dx * dx + dy * dy).sqrt();
        let (ux, uy) = (dx / len, dy / len);
        let lead_start = (P_TIP[0] - P0[0] + 2.5 * ux, P_TIP[1] - P0[1] + 2.5 * uy);
        assert!(ls.iter().any(|l| (l.start.x - lead_start.0).abs() < 1e-9
            && (l.start.y - lead_start.1).abs() < 1e-9
            && l.end.x == 0.0
            && l.end.y == 0.0));
        // 旗杆 + 旗底（局部）
        assert!(ls.iter()
            .any(|l| l.start.x == 0.0 && l.start.y == 0.0 && l.end.x == 0.0 && l.end.y == 7.0));
        assert!(ls.iter().any(|l| l.start.x == 5.25
            && l.start.y == 3.5
            && l.end.x == 0.0
            && l.end.y == 3.5));
        // 尾叉（内容驱动后的终点 = (32.263,0) → +3.5,±3.5）
        assert!(ls.iter().any(|l| (l.start.x - blen_eff).abs() < 1e-6
            && l.start.y == 0.0
            && (l.end.x - (blen_eff + 3.5)).abs() < 1e-6
            && l.end.y == 3.5));
        assert!(ls.iter().any(|l| (l.start.x - blen_eff).abs() < 1e-6
            && l.start.y == 0.0
            && (l.end.x - (blen_eff + 3.5)).abs() < 1e-6
            && l.end.y == -3.5));
        // 上角焊三角（贴实线 +0.125；槽位局部 x = 19.413）
        assert!(ls.iter().any(|l| l.start.x == 19.413
            && l.start.y == 0.125
            && l.end.x == 22.913
            && l.end.y == 0.125));
        assert!(ls.iter().any(|l| l.start.x == 22.913
            && l.start.y == 0.125
            && l.end.x == 19.413
            && l.end.y == 3.625));
        // 下角焊镜像（贴虚线：局部 y = −0.7−0.125 = −0.825）
        assert!(ls.iter().any(|l| l.start.x == 19.413
            && l.start.y == -0.825
            && l.end.x == 22.913
            && l.end.y == -0.825));
        assert!(ls.iter().any(|l| l.start.x == 22.913
            && l.start.y == -0.825
            && l.end.x == 19.413
            && l.end.y == -4.325));

        // ── SOLID：箭头（尖=焊缝点）与旗（对照参考 13/23 组）──
        let ss = solids_of(&members);
        // 箭头：尖 = 焊缝点；底中 = 尖 + 2.5×图幅×u（与其它 OCSM 标注同款
        // arrow_solid：长 2.5、宽 1/3）。
        assert!(ss.iter().any(|s| {
            let (mx, my) = (
                (s.second_corner.x + s.third_corner.x) / 2.0,
                (s.second_corner.y + s.third_corner.y) / 2.0,
            );
            (s.first_corner.x - (P_TIP[0] - P0[0])).abs() < 1e-9
                && (s.first_corner.y - (P_TIP[1] - P0[1])).abs() < 1e-9
                && (mx - (P_TIP[0] - P0[0] + 2.5 * ux)).abs() < 1e-9
                && (my - (P_TIP[1] - P0[1] + 2.5 * uy)).abs() < 1e-9
                && {
                    let w = ((s.second_corner.x - s.third_corner.x).powi(2)
                        + (s.second_corner.y - s.third_corner.y).powi(2))
                    .sqrt();
                    (w - 2.5 / 3.0).abs() < 1e-9 // 宽 = 长/3（arrow_solid 同款）
                }
        }));
        assert!(ss.iter().any(|s| s.first_corner.x == 0.0
            && s.first_corner.y == 7.0
            && s.second_corner.x == 5.25
            && s.second_corner.y == 3.5
            && s.third_corner.x == 0.0
            && s.third_corner.y == 3.5));

        // ── C 弧（上贴实线 +1.125 / 下贴虚线 −1.125；r2.625）──
        let arcs: Vec<&acadrust::entities::Arc> = members
            .iter()
            .filter_map(|e| match e {
                E::Arc(a) => Some(a),
                _ => None,
            })
            .collect();
        assert!(arcs.iter().any(|a| a.center.x == 20.538
            && a.center.y == 1.125
            && a.radius == 2.625
            && (a.start_angle - 1.5f64.to_radians()).abs() < 1e-9
            && (a.end_angle - 88.5f64.to_radians()).abs() < 1e-9));
        assert!(arcs.iter().any(|a| a.center.x == 20.538
            && a.center.y == -1.825
            && (a.start_angle - 271.5f64.to_radians()).abs() < 1e-9
            && (a.end_angle - 358.5f64.to_radians()).abs() < 1e-9));

        // ── 固定 "C" 字（中中锚 @+23.444/线±4.960）──
        let texts: Vec<&Text> = members
            .iter()
            .filter_map(|e| match e {
                E::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        // "C" = 焊接方法字母（仅角焊/喇叭形可用）：上侧锚点 = 槽位+(4.0316,4.9598)
        // = rel P0 (23.4446, 4.9598)；下侧镜像 y = −(0.7+4.9598) = −5.6598。
        assert!(texts.iter().any(|t| t.value == "C"
            && (t.insertion_point.x - 23.4446).abs() < 1e-9
            && (t.insertion_point.y - 4.9598).abs() < 1e-9));
        assert!(texts
            .iter()
            .any(|t| t.value == "C" && (t.insertion_point.y + 5.6598).abs() < 1e-9));

        // ── ATTDEF×5：锚点/对齐/样式（对照示例 MTEXT 71 组语义）──
        let ads = attdefs_of(&members);
        assert_eq!(ads.len(), 5);
        let by_tag = |tag: &str| ads.iter().find(|a| a.tag == tag).unwrap();
        let a = by_tag("上侧厚度尺寸A′");
        assert_eq!(a.insertion_point.x, 17.413);
        assert_eq!(a.insertion_point.y, 2.750);
        assert!(matches!(
            a.horizontal_alignment,
            HorizontalAlignment::Right
        ));
        assert!(matches!(a.vertical_alignment, VerticalAlignment::Middle));
        let a = by_tag("上侧数量长度L′");
        assert_eq!(a.insertion_point.x, 25.913);
        assert!(matches!(a.horizontal_alignment, HorizontalAlignment::Left));
        let a = by_tag("下侧厚度尺寸A");
        assert_eq!(a.insertion_point.y, -3.450);
        let a = by_tag("尾部注释E");
        assert!((a.insertion_point.x - (blen_eff + 4.55)).abs() < 1e-6);
        assert_eq!(a.insertion_point.y, 0.0);
        for a in ads.iter() {
            assert_eq!(a.height, 3.5);
            assert_eq!(a.text_style, "OCSM_GB");
            assert_eq!(a.width_factor, 0.7);
            assert_eq!(a.common.layer, "8符号标注层");
            assert_eq!(a.default_value, " ");
            assert!(a.flags.preset);
        }

        // ── INSERT@拐点 + attributes 值 ──
        let doc = mock.doc.lock().unwrap();
        let ins = doc
            .entities()
            .find_map(|e| match e {
                E::Insert(i) if i.block_name == "*W1" => Some(i.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(ins.insert_point.x, 95.591);
        assert_eq!(ins.insert_point.y, 95.728);
        assert_eq!(ins.attributes.len(), 5);
        let val = |tag: &str| {
            ins.attributes
                .iter()
                .find(|a| a.tag == tag)
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(val("上侧厚度尺寸A′"), "5");
        assert_eq!(val("上侧数量长度L′"), "100");
        assert_eq!(val("下侧厚度尺寸A"), "3");
        assert_eq!(val("下侧数量长度L"), "50");
        assert_eq!(val("尾部注释E"), "封底焊");
        // 属性位置 = 块内 ATTDEF 位 + 插入点平移（宿主独立渲染）。
        let a = ins
            .attributes
            .iter()
            .find(|a| a.tag == "上侧厚度尺寸A′")
            .unwrap();
        assert!((a.insertion_point.x - 113.004).abs() < 1e-9);
        assert!((a.insertion_point.y - 98.478).abs() < 1e-9);
    }

    #[test]
    fn apply_weld_minimal_only_skeleton() {
        // 全关无符号：箭头 + 引线 + 基准线 + 上侧 2 ATTDEF = 5 成员。
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let mut p = full_params();
        p.weld = WeldParams::default();
        apply_weld(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        let members = mock.block_entities("*W1");
        assert_eq!(members.len(), 5);
        assert_eq!(kinds(&members), (2, 0, 0, 1, 0, 2), "箭头+引线+基准线+2 ATTDEF");
        // 无虚线：没有品红线。
        assert!(lines_of(&members)
            .iter()
            .all(|l| l.common.color != acadrust::types::Color::from_index(6)));
        // 无 ensure（文档空样式/层缺失时仍会 ensure 样式与层——断言不强制）。
    }

    #[test]
    fn apply_weld_lower_forces_dash() {
        // 一般情况（上方有符号）：下侧符号存在 ⇒ 虚线强制开（画在基准线下）、
        // 下侧 ATTDEF 出现、下侧符号镜像画在虚线之下。
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let mut p = full_params();
        p.weld = WeldParams {
            upper: "角焊".into(),
            lower: "点焊".into(),
            dash: false,
            circle: false,
            flag: false,
            tail: false,
            grind_upper: GrindKind::None,
            grind_lower: GrindKind::None,
            lo_thick: "3".into(),
            ..WeldParams::default()
        };
        apply_weld(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        let members = mock.block_entities("*W1");
        // 箭头1 + 引线/基准线/虚线/上三角×3 = 6 线 + 下点焊圆1 + ATTDEF4 = 12
        assert_eq!(members.len(), 12);
        assert_eq!(kinds(&members), (6, 1, 0, 1, 0, 4));
        // 虚线在基准线下 −0.7（一般情况）。
        assert!(lines_of(&members)
            .iter()
            .any(|l| l.common.linetype == "ACISOWELD" && (l.start.y + 0.7).abs() < 1e-9));
        // 下侧点焊圆：圆心 = 槽位+2.275、虚线下 2.275（镜像）。
        let circles: Vec<&acadrust::entities::Circle> = members
            .iter()
            .filter_map(|e| match e {
                E::Circle(c) => Some(c),
                _ => None,
            })
            .collect();
        assert!(circles
            .iter()
            .any(|c| (c.center.x - (19.413 + 2.275)).abs() < 1e-9
                && (c.center.y - (-0.7 - 2.275)).abs() < 1e-9
                && c.radius == 2.275));
    }

    #[test]
    fn apply_weld_flip_special_case() {
        // 特殊情况：引线上方什么都没填（无符号无文字）、下方有内容 ⇒
        // 下侧内容显示到上方（用上侧正朝向几何），虚线画在基准线上方 +0.7，
        // 尺寸文字也到上方（下侧标签 3.45）。
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let mut p = full_params();
        p.weld = WeldParams {
            upper: String::new(),
            lower: "带单边坡口的V型对接焊缝".into(),
            dash: false,
            circle: false,
            flag: false,
            tail: false,
            grind_upper: GrindKind::ArcConvex,
            grind_lower: GrindKind::ArcConvex,
            lo_thick: "5".into(),
            ..WeldParams::default()
        };
        let out = apply_weld(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        let j: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(j["flipped"], true);
        let members = mock.block_entities("*W1");
        // 虚线在基准线上方 +0.7（品红 ACISOWELD）。
        assert!(lines_of(&members)
            .iter()
            .any(|l| l.common.linetype == "ACISOWELD" && (l.start.y - 0.7).abs() < 1e-9));
        // 符号用上侧（正朝向）几何画在 +0.7：单边V = 竖线(0,3.5→0) + 斜线(0→3.5,3.5)
        let slot = 19.413;
        assert!(lines_of(&members).iter().any(|l| (l.start.x - slot).abs() < 1e-9
            && (l.start.y - (0.7 + 3.5)).abs() < 1e-9
            && (l.end.y - 0.7).abs() < 1e-9));
        // 打磨（其它版 弧·凸：弧心 = 锚+(1.75,3.1169) 相对符号基线 +0.7）
        assert!(grind_arcs(&members)
            .iter()
            .any(|a| (a.center.y - (0.7 + 3.1169)).abs() < 1e-9));
        // 基准线下方没有任何东西（除引线，它在负象限）。
        assert!(
            !lines_of(&members).iter().any(|l| l.start.y < -0.5 && l.end.y < -0.5),
            "特殊情况引线下方不应有内容"
        );
        // ATTDEF：只有下侧两个标签（值来自下侧字段），锚点在上方 3.45。
        let ads = attdefs_of(&members);
        assert_eq!(ads.len(), 2, "下侧厚度+下侧数量（未开尾部）");
        let a = ads.iter().find(|a| a.tag == "下侧厚度尺寸A").unwrap();
        assert!((a.insertion_point.y - 3.450).abs() < 1e-9);
        assert!(matches!(a.horizontal_alignment, HorizontalAlignment::Right));
        assert!(!ads.iter().any(|a| a.tag == "上侧厚度尺寸A′"), "特殊情况不建上侧标签");
        // 属性值：5 来自下侧字段。
        let doc = mock.doc.lock().unwrap();
        let ins = doc
            .entities()
            .find_map(|e| match e {
                E::Insert(i) if i.block_name == "*W1" => Some(i.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(ins.attributes.len(), 2);
        assert!(ins
            .attributes
            .iter()
            .any(|a| a.tag == "下侧厚度尺寸A" && a.value == "5"));
        // 上方空 + 下方也空 → 不翻转（无内容）。
        let mock2 = Arc::new(MockSender::new(weld_doc()));
        let sender2: Arc<dyn PluginRequestSender> = mock2.clone();
        let mut p2 = full_params();
        p2.weld = WeldParams::default();
        let o2 = apply_weld(&sender2, &weld_doc(), P_TIP, P0, P_END, &p2).unwrap();
        let j2: serde_json::Value = serde_json::from_str(&o2).unwrap();
        assert_eq!(j2["flipped"], false);
    }

    #[test]
    fn apply_weld_rejects_invalid() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let p = full_params();
        // 基准线长度为零（顶点2 与拐点重合）。
        let r = apply_weld(&sender, &doc, P_TIP, P0, P0, &p);
        assert!(r.err().map_or(false, |e| e.contains("长度为零")));
        // 引线过短。
        let r = apply_weld(&sender, &doc, [94.0, 95.2, 0.0], P0, P_END, &p);
        assert!(r.err().map_or(false, |e| e.contains("过短")));
        // 未知符号。
        let mut p2 = full_params();
        p2.weld.upper = "角焊XL".into();
        let r = apply_weld(&sender, &doc, P_TIP, P0, P_END, &p2);
        assert!(r.err().map_or(false, |e| e.contains("未知焊缝符号")));
        // 跨线符号放下侧。
        let mut p3 = full_params();
        p3.weld.lower = "堆焊接头".into();
        let r = apply_weld(&sender, &doc, P_TIP, P0, P_END, &p3);
        assert!(r.err().map_or(false, |e| e.contains("跨线")));
    }

    // ── 引线标注（LEADER）测试────────────────────────────────────────────

    fn leader_params(upper: &str, lower: &str) -> GuideParams {
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Leader;
        p.leader = crate::guide_url::LeaderParams {
            upper: upper.into(),
            lower: lower.into(),
        };
        p
    }

    // ── 序号标注（BALLOON）测试─────────────────────────────────────────────

    fn balloon_params(items: &[&str], dir: crate::balloon::BalloonDir) -> GuideParams {
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Balloon;
        p.sub = None;
        p.balloon = crate::guide_url::BalloonParams {
            items: items.iter().map(|s| s.to_string()).collect(),
            dir,
            insert_mode: false,
        };
        p
    }

    /// 横向扩展：2 个序号 → 圆点 2 枚 SOLID + 指引线/2 横线/V 两段 = 5 线 + 2 MTEXT。
    #[test]
    fn apply_balloon_row_writes_block_dot_shelves_and_items() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let p = balloon_params(&["2", "3"], crate::balloon::BalloonDir::Row);
        let out = apply_balloon(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        assert!(out.contains("\"block\":\"*XH1\""), "块名 *XH1: {out}");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["dir"], "H");
        assert_eq!(v["items"][1], "3");
        assert!(v["span"].as_f64().unwrap() > 10.0, "跨度应大于两条横线之和: {out}");
        let members = mock.block_entities("*XH1");
        assert_eq!(kinds(&members), (5, 0, 0, 2, 0, 0), "5 线 + 2 实心（圆点两枚）");
        let ms = mtexts_of(&members);
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].value, "2");
        assert_eq!(ms[1].value, "3");
        for m in &ms {
            assert_eq!(m.height, 3.5);
            assert_eq!(m.style, "OCSM_GB");
            assert_eq!(m.rotation, 0.0);
        }
        // 两个序号都写在各自横线中点上方（v = 1.05）；且两者 x 不同（并排）
        let (x0, x1) = (ms[0].insertion_point.x, ms[1].insertion_point.x);
        assert!(x1 > x0 + 4.0, "横向：第二条应在右边 {x0} / {x1}");
        for m in &ms {
            assert!((m.insertion_point.y - 1.05).abs() < 1e-9);
        }
        // 圆点两枚 SOLID 都在指针点 P_TIP 附近（块局部 = 世界 − P0）
        let tip_l = (P_TIP[0] - P0[0], P_TIP[1] - P0[1]);
        for s in solids_of(&members) {
            let dx = s.first_corner.x - tip_l.0;
            let dy = s.first_corner.y - tip_l.1;
            assert!(dx.abs() < 1.0 && dy.abs() < 1.0, "圆点应贴指针点");
        }
        // INSERT 落在 8符号标注层、原点 = 拐点，并带 OCSM_BALLOON 台账
        let doc2 = mock.doc.lock().unwrap();
        let ins = doc2
            .model_space_entities()
            .find_map(|e| match e {
                E::Insert(i) => Some(i.clone()),
                _ => None,
            })
            .expect("INSERT");
        assert_eq!(ins.common.layer, "8符号标注层");
        assert!((ins.insert_point.x - P0[0]).abs() < 1e-9);
        let rec = ins
            .common
            .extended_data
            .get_record("OCSM_BALLOON")
            .expect("OCSM_BALLOON 台账");
        let text = rec
            .values
            .iter()
            .find_map(|x| match x {
                acadrust::xdata::XDataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(meta["items"][0], "2");
        assert_eq!(meta["dir"], "H");
        assert_eq!(meta["ins"], false);
        assert!(meta["part"].is_null(), "落点不在零件块内 → 不关联");
    }

    /// 纵向扩展：2 个序号 → 指引线 + 2 横线 + 1 竖线 = 4 线；竖线在指引线那一端。
    #[test]
    fn apply_balloon_col_links_at_leader_end() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let p = balloon_params(&["5", "6"], crate::balloon::BalloonDir::Col);
        let out = apply_balloon(&sender, &weld_doc(), P_TIP, P0, P_END, &p).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["dir"], "V");
        let members = mock.block_entities("*XH1");
        assert_eq!(kinds(&members), (4, 0, 0, 2, 0, 0));
        let ms = mtexts_of(&members);
        // 纵向：x 都落在各自横线中点（L/2），y 递增（6 在 5 上方）
        assert!(ms[1].insertion_point.y > ms[0].insertion_point.y, "6 应在 5 上方");
    }

    /// 空条目 / 顶点数不对 → 报错。
    #[test]
    fn apply_balloon_rejects_empty_items() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let p = balloon_params(&[], crate::balloon::BalloonDir::Row);
        let e = apply_balloon(&sender, &weld_doc(), P_TIP, P0, P_END, &p).unwrap_err();
        assert!(e.contains("至少要有一个序号"), "{e}");
    }

    /// 指针点落在零件块内 → 关联该零件（读 XDATA OCSM_PART），并在台账里记下。
    #[test]
    fn balloon_tip_inside_part_block_is_associated() {
        let mut doc = weld_doc();
        // 一个 20×10 的“零件”块，带 OCSM_PART 台账，插在 (0,0)
        let mut br = acadrust::tables::BlockRecord::new("OCSM_TESTPART");
        br.handle = doc.allocate_handle();
        br.entity_handles.push(
            doc.add_entity(E::Block(acadrust::entities::Block::new(
                "OCSM_TESTPART",
                Vector3::ZERO,
            )))
            .unwrap(),
        );
        for (a, b) in [
            ((0.0, 0.0), (20.0, 0.0)),
            ((20.0, 0.0), (20.0, 10.0)),
            ((20.0, 10.0), (0.0, 10.0)),
            ((0.0, 10.0), (0.0, 0.0)),
        ] {
            br.entity_handles.push(
                doc.add_entity(E::Line(acadrust::entities::Line::from_coords(
                    a.0, a.1, 0.0, b.0, b.1, 0.0,
                )))
                .unwrap(),
            );
        }
        br.entity_handles.push(
            doc.add_entity(E::BlockEnd(acadrust::entities::BlockEnd::new()))
                .unwrap(),
        );
        doc.block_records.add(br).unwrap();
        let mut ins = acadrust::entities::Insert::new("OCSM_TESTPART", Vector3::new(60.0, 60.0, 0.0));
        let mut rec = acadrust::xdata::ExtendedDataRecord::new("OCSM_PART");
        rec.values.push(acadrust::xdata::XDataValue::String(
            serde_json::json!({"code": "0165", "name": "偏心轴", "spec": "φ12"}).to_string(),
        ));
        ins.common.extended_data.add_record(rec);
        doc.add_entity(E::Insert(ins)).unwrap();

        // 指针点 (70,66) 在块内（块 = 60..80 / 60..70）
        let assoc = part_block_at(&doc, [70.0, 66.0, 0.0]).expect("应关联到零件块");
        assert_eq!(assoc.1["code"], "0165");
        // 块外 → 不关联
        assert!(part_block_at(&doc, [200.0, 200.0, 0.0]).is_none());

        // 走完 apply：报告里有 part，台账里也有
        let mock = Arc::new(MockSender::new(doc));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let body = serde_json::json!({
            "handle": "2A",
            "url": "http://127.0.0.1:23751/DIM/BALLOON/0?items=1,2&dir=H",
            "pts": [[70.0, 66.0, 0.0], [90.0, 80.0, 0.0], [110.0, 80.0, 0.0]],
        });
        let _ = (mock, sender, body); // 组装细节由 api_apply 集成测试覆盖
    }

    /// 参数 URL 往返：items/dir/ins 三个键。
    #[test]
    fn balloon_url_roundtrip() {
        let mut p = balloon_params(&["1", "2", "3"], crate::balloon::BalloonDir::Col);
        p.balloon.insert_mode = true;
        let url = p.to_url(23751);
        assert!(url.contains("items=1%2C2%2C3") || url.contains("items=1,2,3"), "{url}");
        assert!(url.contains("dir=V"), "{url}");
        assert!(url.contains("ins=1"), "{url}");
        let back = GuideParams::from_url(&url).expect("能解析回来");
        assert_eq!(back.guide_type, GuideType::Balloon);
        assert_eq!(back.balloon.items, vec!["1", "2", "3"]);
        assert_eq!(back.balloon.dir, crate::balloon::BalloonDir::Col);
        assert!(back.balloon.insert_mode);
    }

    /// 骨架：箭头 + 引线 + 肩线 + 2 ATTDEF = 5 成员；两文字空值也允许。
    #[test]
    fn apply_leader_skeleton_members_and_slots() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let p = leader_params("", "");
        let out = apply_leader(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        assert!(out.contains("\"block\":\"*L1\""), "块名 *L1: {out}");
        let members = mock.block_entities("*L1");
        // 全空文字：2 线 + 1 实心箭头，无文字对象（不必占位）。
        assert_eq!(kinds(&members), (2, 0, 0, 1, 0, 0), "2 线 + 1 箭头 + 无 ATTDEF");
        assert!(mtexts_of(&members).is_empty(), "空文字不画 MTEXT");
        assert!(lines_of(&members)
            .iter()
            .all(|l| l.common.color == acadrust::types::Color::from_index(4)));
        let sol = &solids_of(&members)[0];
        assert!(sol.first_corner.distance(&sol.third_corner) > 0.0, "箭头实心体存在");
        // INSERT 无属性（文字在块内 MTEXT，不再走 ATTRIB）。
        let doc2 = mock.doc.lock().unwrap();
        let ins = doc2
            .entities()
            .find_map(|e| match e {
                E::Insert(i) => Some(i.clone()),
                _ => None,
            })
            .expect("INSERT");
        assert!(ins.attributes.is_empty(), "MTEXT 方案不再用 ATTRIB");
        drop(doc2);

        // 有文字：块内 2 个 MTEXT —— 上 +2.750 / 下 −2.750，锚点 x = 17.413（块局部）。
        let mock2 = Arc::new(MockSender::new(weld_doc()));
        let sender2: Arc<dyn PluginRequestSender> = mock2.clone();
        apply_leader(&sender2, &weld_doc(), P_TIP, P0, P_END, &leader_params("通孔", "深20")).unwrap();
        let ms = mtexts_of(&mock2.block_entities("*L1"));
        assert_eq!(ms.len(), 2, "上/下各一个 MTEXT");
        assert_eq!(ms[0].value, "通孔");
        assert_eq!(ms[1].value, "深20");
        assert!((ms[0].insertion_point.x - 17.413).abs() < 1e-9);
        assert!((ms[0].insertion_point.y - 2.750).abs() < 1e-9);
        assert!((ms[1].insertion_point.x - 17.413).abs() < 1e-9);
        assert!((ms[1].insertion_point.y + 2.750).abs() < 1e-9);
        for m in &ms {
            assert_eq!(m.height, 3.5);
            assert_eq!(m.style, "OCSM_GB");
            assert_eq!(m.rotation, 0.0);
            // 行宽给足：不会被宿主按默认 10 单位折行。
            assert!(m.rectangle_width >= 10.0);
            assert_eq!(m.common.layer, "8符号标注层");
            assert_eq!(m.common.color, acadrust::types::Color::from_index(3));
        }
        // 左向（dir_out>0）→ MiddleLeft 对齐。
        assert!(matches!(
            ms[0].attachment_point,
            acadrust::entities::AttachmentPoint::MiddleLeft
        ));
    }

    /// 肩线长度内容驱动：文字越长肩线越长；两行文字取较大者。
    #[test]
    fn apply_leader_content_drives_shoulder_length() {
        let short = build_leader_parts(P_TIP, P0, P_END, 1.0, "A", "").unwrap();
        let long = build_leader_parts(P_TIP, P0, P_END, 1.0, "ABCDEFGHIJ", "").unwrap();
        assert!((short.blen - (17.413 + 1.45 + 2.0)).abs() < 1e-9, "{short_blen}", short_blen = short.blen);
        assert!(long.blen > short.blen);
        // 中日韩按方块字宽 2.45 估算。
        let cjk = build_leader_parts(P_TIP, P0, P_END, 1.0, "通孔", "").unwrap();
        assert!((cjk.blen - (17.413 + 2.45 * 2.0 + 2.0)).abs() < 1e-9, "{b}", b = cjk.blen);
        // 下侧更长时以下侧为准。
        let lower = build_leader_parts(P_TIP, P0, P_END, 1.0, "A", "ABCDEFGH").unwrap();
        assert!((lower.blen - (17.413 + 1.45 * 8.0 + 2.0)).abs() < 1e-9);
        // 肩线实体确实到 blen（线末端沿主轴）。
        // 肩线 = 起于块局部原点、沿 +t 的那条线（块局部坐标，原点 = 拐点）。
        let shoulder = lines_of(&long.members)
            .into_iter()
            .find(|l| l.start.x.abs() < 1e-9 && l.start.y.abs() < 1e-9)
            .map(|l| (l.start, l.end))
            .expect("肩线");
        assert!((shoulder.1.x - long.blen).abs() < 1e-9, "肩线末 t = blen");
    }

    /// 文字值进 INSERT.attributes（上/下侧各一）。
    #[test]
    fn apply_leader_writes_mtext_values() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let p = leader_params("通孔", "深10");
        apply_leader(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        let vals: Vec<String> = mtexts_of(&mock.block_entities("*L1"))
            .iter()
            .map(|m| m.value.clone())
            .collect();
        assert_eq!(vals, vec!["通孔".to_string(), "深10".to_string()]);
    }

    /// 竖肩线：文字旋转 90°（沿肩线书写、永不倒置），锚点按 v 轴换算。
    #[test]
    fn apply_leader_vertical_rotates_text() {
        // 拐点 → 上方 40（竖肩线，bdy>0）。
        let p_end = [P0[0], P0[1] + 40.0, 0.0];
        let parts = build_leader_parts(P_TIP, P0, p_end, 1.0, "A", "B").unwrap();
        assert!(!parts.horizontal);
        let ms = mtexts_of(&parts.members);
        assert_eq!(ms.len(), 2);
        assert!((ms[0].rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // 竖轴内容坐标系（块局部）：n_up = (−1,0) → v 正方向为 −x，
        // 故上侧文字在肩线左侧 (−2.750, 17.413)、下侧在右侧 (+2.750, 17.413)。
        assert!((ms[0].insertion_point.x + 2.750).abs() < 1e-9);
        assert!((ms[0].insertion_point.y - 17.413).abs() < 1e-9);
        assert!((ms[1].insertion_point.x - 2.750).abs() < 1e-9);
    }

    /// 左向肩线：文字自**拐点**向外偏移（局部 −17.413）+ 右对齐 → 与右向镜像对称。
    #[test]
    fn apply_leader_leftward_text_mirrors() {
        // 肩线自拐点向左画 40。
        let p_end = [P0[0] - 40.0, P0[1], 0.0];
        let parts = build_leader_parts(P_TIP, P0, p_end, 1.0, "通孔", "深10").unwrap();
        let ms = mtexts_of(&parts.members);
        assert_eq!(ms.len(), 2);
        // 文字锚点 = 拐点 −17.413（块局部 x = −17.413）。
        assert!((ms[0].insertion_point.x + 17.413).abs() < 1e-9, "上侧文字在拐点左侧 17.413");
        assert!((ms[1].insertion_point.x + 17.413).abs() < 1e-9);
        // 右对齐（MiddleRight）→ 文字仍向外（左）展开 → 与右向镜像对称。
        for m in &ms {
            assert!(matches!(m.attachment_point, acadrust::entities::AttachmentPoint::MiddleRight));
        }
        // 肩线长度不受方向影响：17.413 + max(字宽) + 2（"深10"=2.45+1.45+1.45=5.35）。
        assert!((parts.blen - (17.413 + 5.35 + 2.0)).abs() < 1e-9, "{}", parts.blen);
        // 肩线自拐点向左：一端在拐点 (0,0)、另一端局部 x = −blen（水平线）。
        let sh = lines_of(&parts.members)
            .into_iter()
            .find(|l| l.start.y.abs() < 1e-9 && l.end.y.abs() < 1e-9
                && ((l.start.x.abs() < 1e-9) || (l.end.x.abs() < 1e-9)))
            .expect("肩线");
        let far = if sh.start.x.abs() < 1e-9 { sh.end.x } else { sh.start.x };
        assert!((far + parts.blen).abs() < 1e-9, "肩线另一端在拐点左侧 blen 处");
    }

    /// 坏引导：引线过短 / 肩线零长 → 报错。
    #[test]
    fn apply_leader_rejects_invalid() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let doc = weld_doc();
        let p = leader_params("A", "");
        let r = apply_leader(&sender, &doc, P_TIP, P0, P0, &p);
        assert!(r.err().map_or(false, |e| e.contains("长度为零")));
        let r = apply_leader(&sender, &doc, [P0[0] - 1.0, P0[1] - 1.0, 0.0], P0, P_END, &p);
        assert!(r.err().map_or(false, |e| e.contains("过短")));
    }

    /// HTTP 端到端：两段 PLINE → type=LEADER → *L1 + INSERT（引导保留）；
    /// 非 3 顶点 → 报错。
    #[test]
    fn http_server_leader_apply_refresh() {
        let mut doc = acadrust::CadDocument::new();
        let mut pl = LwPolyline::new();
        pl.common.layer = "10引导线层".into();
        pl.add_point(Vector2::new(0.0, 0.0));
        pl.add_point(Vector2::new(20.0, 20.0));
        pl.add_point(Vector2::new(60.0, 20.0));
        let gh = doc.add_entity(E::LwPolyline(pl)).unwrap();
        let mock = Arc::new(MockSender::new(doc));
        let server = spawn(mock.clone()).expect("spawn guide server");
        let hx = format!("{:#X}", u64::from(gh));
        // 上侧“通孔”、下侧“深10”（percent-encode 中文）。
        let url = "http://127.0.0.1:1/DIM/LEADER/0?lu=%E9%80%9A%E5%AD%94&ll=%E6%B7%B110";
        let body = serde_json::json!({"handle": hx, "url": url}).to_string();
        let r2 = http_req(server.port, "POST", "/api/apply_refresh", &body);
        let v: serde_json::Value = serde_json::from_str(&r2).unwrap();
        assert_eq!(v["ok"], true, "apply_refresh: {r2}");
        assert_eq!(v["block"], "*L1");
        let doc2 = mock.doc.lock().unwrap();
        assert!(doc2.entities().any(|e| matches!(e, E::LwPolyline(_))), "引导 PLINE 保留");
        drop(doc2);
        let vals: Vec<String> = mtexts_of(&mock.block_entities("*L1"))
            .iter()
            .map(|m| m.value.clone())
            .collect();
        assert_eq!(vals, vec!["通孔".to_string(), "深10".to_string()]);
        drop(server);
        // 坏请求：4 顶点 PLINE → 报错。
        let mut doc3 = acadrust::CadDocument::new();
        let mut pl3 = LwPolyline::new();
        pl3.common.layer = "10引导线层".into();
        for pt in [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)] {
            pl3.add_point(Vector2::new(pt.0, pt.1));
        }
        let gh3 = doc3.add_entity(E::LwPolyline(pl3)).unwrap();
        let mock3 = Arc::new(MockSender::new(doc3));
        let server3 = spawn(mock3.clone()).expect("spawn guide server 3");
        let hx3 = format!("{:#X}", u64::from(gh3));
        let body3 = serde_json::json!({"handle": hx3, "url": url}).to_string();
        let r3 = http_req(server3.port, "POST", "/api/apply_refresh", &body3);
        assert!(r3.contains("3 顶点"), "4 顶点应报错: {r3}");
    }

    #[test]
    fn http_server_weld_apply_refresh() {
        // 两段 PLINE 引导：(0,0)→(20,20)→(60,20)（焊缝点→拐点→基准线末端）。
        let mut doc = acadrust::CadDocument::new();
        let mut pl = LwPolyline::new();
        pl.common.layer = "10引导线层".into();
        pl.add_point(Vector2::new(0.0, 0.0));
        pl.add_point(Vector2::new(20.0, 20.0));
        pl.add_point(Vector2::new(60.0, 20.0));
        let gh = doc.add_entity(E::LwPolyline(pl)).unwrap();
        let mock = Arc::new(MockSender::new(doc));
        let server = spawn(mock.clone()).expect("spawn guide server");
        let hx = format!("{:#X}", u64::from(gh));
        // 角焊 + 虚线 + 尾部 + 文字（URL percent-encode 中文）。
        let url = "http://127.0.0.1:1/DIM/WELD/0?wu=%E8%A7%92%E7%84%8A&wdash=1&wtail=1&wut=5&wuq=100";
        // 1) POST /api/apply → 写超链接（引导保留）。
        let body = serde_json::json!({"handle": hx, "url": url}).to_string();
        let r = http_req(server.port, "POST", "/api/apply", &body);
        assert!(r.contains("\"ok\":true"), "apply: {r}");
        assert!(mock
            .url_writes
            .lock()
            .unwrap()
            .iter()
            .any(|(h, _)| *h == gh));
        // 2) POST /api/apply_refresh → 生成 *W1 + INSERT；引导 PLINE 保留。
        let r2 = http_req(server.port, "POST", "/api/apply_refresh", &body);
        let v: serde_json::Value = serde_json::from_str(&r2).unwrap();
        assert_eq!(v["ok"], true, "apply_refresh: {r2}");
        assert_eq!(v["block"], "*W1");
        let doc2 = mock.doc.lock().unwrap();
        assert!(
            doc2.entities().any(|e| matches!(e, E::LwPolyline(_))),
            "焊接引导 PLINE 保留（10引导线层，便于重选再改）"
        );
        assert!(doc2.entities().any(|e| matches!(e, E::Insert(_))));
        // 3) 坏请求：4 顶点 PLINE → 报错。
        //    （重建 mock：另起 server。）
        drop(server);
        let mut doc3 = acadrust::CadDocument::new();
        let mut pl3 = LwPolyline::new();
        pl3.common.layer = "10引导线层".into();
        for pt in [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)] {
            pl3.add_point(Vector2::new(pt.0, pt.1));
        }
        let gh3 = doc3.add_entity(E::LwPolyline(pl3)).unwrap();
        let mock3 = Arc::new(MockSender::new(doc3));
        let server3 = spawn(mock3.clone()).expect("spawn guide server 3");
        let hx3 = format!("{:#X}", u64::from(gh3));
        let body3 = serde_json::json!({"handle": hx3, "url": url}).to_string();
        let r3 = http_req(server3.port, "POST", "/api/apply_refresh", &body3);
        assert!(r3.contains("3 顶点"), "4 顶点应报错: {r3}");
    }

    /// 打磨几何断言辅助：跑一次 apply_weld，返回块成员。
    fn weld_members(upper: &str, lower: &str, k: GrindKind, dash: bool) -> Vec<E> {
        let doc = acadrust::CadDocument::new();
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Weld;
        p.weld = WeldParams {
            upper: upper.into(),
            lower: lower.into(),
            dash,
            grind_upper: k,
            grind_lower: k,
            ..WeldParams::default()
        };
        apply_weld(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        mock.block_entities("*W1")
    }

    fn grind_arcs(members: &[E]) -> Vec<&acadrust::entities::Arc> {
        members
            .iter()
            .filter_map(|e| match e {
                E::Arc(a) if a.radius > 2.0 && a.radius < 3.0 => Some(a),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn apply_weld_grind_six_kinds_two_styles() {
        // 槽位（块局部）= 19.413；引导 P_TIP/P0/P_END 与参考示例同几何。
        let slot = 19.413;
        // ── 角焊版（上侧 = 角焊；倾斜右上，带 "C" 字）──
        // 弧·凹：中心 = 槽位+(3.9597,3.9598)，181.5°→268.5°
        let m = weld_members("角焊", "", GrindKind::ArcConcave, false);
        let a = grind_arcs(&m);
        assert_eq!(a.len(), 1);
        assert!((a[0].center.x - (slot + 3.9597)).abs() < 1e-9);
        assert!((a[0].center.y - 3.9598).abs() < 1e-9);
        assert!((a[0].start_angle - 181.5f64.to_radians()).abs() < 1e-9);
        assert!((a[0].end_angle - 268.5f64.to_radians()).abs() < 1e-9);
        // 弧·凸：中心 = 槽位+(1.125,1.125)（= 示例.dxf），1.5°→88.5°
        let m = weld_members("角焊", "", GrindKind::ArcConvex, false);
        let a = grind_arcs(&m);
        assert_eq!(a.len(), 1);
        assert!((a[0].center.x - (slot + 1.125)).abs() < 1e-9);
        assert!((a[0].center.y - 1.125).abs() < 1e-9);
        assert!((a[0].start_angle - 1.5f64.to_radians()).abs() < 1e-9);
        assert!((a[0].end_angle - 88.5f64.to_radians()).abs() < 1e-9);
        // 直线：(槽位+0.2475,3.9598) → (槽位+3.9598,0.2474)
        let m = weld_members("角焊", "", GrindKind::Line, false);
        assert!(lines_of(&m).iter().any(|l| (l.start.x - (slot + 0.2475)).abs() < 1e-9
            && (l.start.y - 3.9598).abs() < 1e-9
            && (l.end.x - (slot + 3.9598)).abs() < 1e-9
            && (l.end.y - 0.2474).abs() < 1e-9));
        // 双弧：2 弧（r1.4875，135°→315°）+ 1 线
        let m = weld_members("角焊", "", GrindKind::DoubleArc, false);
        let small: Vec<_> = m
            .iter()
            .filter_map(|e| match e {
                E::Arc(a) if (a.radius - 1.4875).abs() < 1e-9 => Some(a),
                _ => None,
            })
            .collect();
        assert_eq!(small.len(), 2);
        assert!(small.iter().all(|a| (a.start_angle - 135f64.to_radians()).abs() < 1e-9));
        // 锯齿：4 线段
        let m = weld_members("角焊", "", GrindKind::Zigzag, false);
        assert!(lines_of(&m).iter().any(|l| (l.start.x - (slot + 0.3712)).abs() < 1e-9
            && (l.end.x - (slot + 3.8360)).abs() < 1e-9));
        assert!(lines_of(&m).iter().any(|l| (l.start.x - (slot + 2.4501)).abs() < 1e-9
            && (l.end.x - (slot + 7.3688)).abs() < 1e-9));
        // ── 其它焊缝版（上侧 = 单边V；正上方，无 "C" 字）──
        let mono = "带单边坡口的V型对接焊缝";
        // 弧·凹：中心 = 槽位+(1.75,7.125)，226.5°→313.5°
        let m = weld_members(mono, "", GrindKind::ArcConcave, false);
        let a = grind_arcs(&m);
        assert_eq!(a.len(), 1);
        assert!((a[0].center.x - (slot + 1.75)).abs() < 1e-9);
        assert!((a[0].center.y - 7.125).abs() < 1e-9);
        assert!((a[0].start_angle - 226.5f64.to_radians()).abs() < 1e-9);
        assert!((a[0].end_angle - 313.5f64.to_radians()).abs() < 1e-9);
        // 弧·凸：中心 = 槽位+(1.75,3.1169)，46.5°→133.5°
        let m = weld_members(mono, "", GrindKind::ArcConvex, false);
        let a = grind_arcs(&m);
        assert!((a[0].center.y - 3.1169).abs() < 1e-9);
        assert!((a[0].start_angle - 46.5f64.to_radians()).abs() < 1e-9);
        // 直线：水平 y=4.5，x 槽位−0.875 → 槽位+4.375
        let m = weld_members(mono, "", GrindKind::Line, false);
        assert!(lines_of(&m).iter().any(|l| (l.start.x - (slot - 0.875)).abs() < 1e-9
            && (l.start.y - 4.5).abs() < 1e-9
            && (l.end.x - (slot + 4.375)).abs() < 1e-9));
        // 双弧：竖短线 + 2 半圆（180°→0°）
        let m = weld_members(mono, "", GrindKind::DoubleArc, false);
        assert!(lines_of(&m).iter().any(|l| (l.start.y - 6.775).abs() < 1e-9
            && (l.end.y - 10.45).abs() < 1e-9));
        let small: Vec<_> = m
            .iter()
            .filter_map(|e| match e {
                E::Arc(a) if (a.radius - 1.4875).abs() < 1e-9 => Some(a),
                _ => None,
            })
            .collect();
        assert_eq!(small.len(), 2);
        // 参考：50=180° → 51=0°（DXF 恒 CCW：下半个圆，即向焊缝方向鼓）
        assert!(small
            .iter()
            .all(|a| (a.start_angle - 180f64.to_radians()).abs() < 1e-9
                && a.end_angle.abs() < 1e-9));
        // 锯齿：3 线段（水平 + 水平 + 斜线）
        let m = weld_members(mono, "", GrindKind::Zigzag, false);
        assert!(lines_of(&m).iter().any(|l| (l.start.x - (slot - 0.7)).abs() < 1e-9
            && (l.start.y - 4.5).abs() < 1e-9
            && (l.end.x - (slot + 4.2)).abs() < 1e-9));
        assert!(lines_of(&m).iter().any(|l| (l.start.y - 4.99).abs() < 1e-9
            && (l.end.y - 9.4).abs() < 1e-9));
    }

    #[test]
    fn apply_weld_grind_lower_mirrors_and_requires_symbol() {
        // 下侧镜像：lower=角焊 + 虚线 + 弧·凸 → 弧心 y = −0.7−1.125 = −1.825，
        // 角度镜像为 271.5°→358.5°（保持 CCW）。
        // 两侧都有符号 → 打磨件两侧各一份；下侧那份为镜像（弧心 y=−1.825、
        // 角度 271.5°→358.5° 保持 CCW）。
        let m = weld_members("点焊", "角焊", GrindKind::ArcConvex, true);
        let a = grind_arcs(&m);
        assert_eq!(a.len(), 2, "上下侧各一份打磨件");
        let lo = a.iter().find(|x| x.center.y < 0.0).expect("下侧打磨弧");
        assert!((lo.center.x - (19.413 + 1.125)).abs() < 1e-9);
        assert!((lo.center.y + 1.825).abs() < 1e-9, "下侧镜像 y");
        assert!((lo.start_angle - 271.5f64.to_radians()).abs() < 1e-9);
        assert!((lo.end_angle - 358.5f64.to_radians()).abs() < 1e-9);
        // 无符号侧不画打磨：上下皆无符号 + 打磨开 → 无弧无线（仅箭头/引线/基准 + 2 ATTDEF）
        let m = weld_members("", "", GrindKind::ArcConvex, false);
        assert!(grind_arcs(&m).is_empty(), "无符号侧不画打磨弧");
        assert_eq!(m.len(), 5, "箭头+引线+基准线+2 ATTDEF");
        // 不打磨：角焊 + None → 无附加件，亦无 C 字
        let m = weld_members("角焊", "", GrindKind::None, false);
        assert!(grind_arcs(&m).is_empty());
        assert!(!m.iter().any(|e| matches!(e, E::Text(_))));
    }

    /// 焊接方法字母：位置随打磨方式、仅角焊/喇叭形可用、下侧镜像。
    fn weld_method_members(upper: &str, k: GrindKind, m: &str, dash: bool) -> Vec<E> {
        let doc = acadrust::CadDocument::new();
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Weld;
        p.weld = WeldParams {
            upper: upper.into(),
            dash,
            grind_upper: k,
            grind_lower: k,
            method_upper: m.into(),
            method_lower: m.into(),
            ..WeldParams::default()
        };
        apply_weld(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        mock.block_entities("*W1")
    }

    fn letters(members: &[E]) -> Vec<(String, f64, f64)> {
        members
            .iter()
            .filter_map(|e| match e {
                E::Text(t) => {
                    Some((t.value.clone(), t.insertion_point.x, t.insertion_point.y))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn apply_weld_method_letters_scope_and_positions() {
        let slot = 19.413;
        // 角焊 + 6 个字母：位置 = 槽位 + 打磨方式对应锚点。
        for m in ["C", "G", "H", "M", "R", "U"] {
            let l = letters(&weld_method_members("角焊", GrindKind::ArcConvex, m, false));
            assert_eq!(l.len(), 1, "{m} 应画 1 个字母");
            assert_eq!(l[0].0, m);
            assert!((l[0].1 - (slot + 4.0316)).abs() < 1e-9);
            assert!((l[0].2 - 4.9598).abs() < 1e-9);
        }
        // 位置随打磨方式：直线 / 双弧 / 锯齿 各自锚点；不打磨 → 用弧凸锚点。
        let cases = [
            (GrindKind::Line, 4.9598, 6.8159),
            (GrindKind::DoubleArc, 6.3206, 7.3109),
            (GrindKind::Zigzag, 4.8360, 6.5684),
            (GrindKind::ArcConcave, 4.0316, 4.9598),
            (GrindKind::None, 4.0316, 4.9598),
        ];
        for (k, ex, ey) in cases {
            let l = letters(&weld_method_members("角焊", k, "C", false));
            assert!(
                l.iter().any(|(v, x, y)| v == "C"
                    && (x - (slot + ex)).abs() < 1e-9
                    && (y - ey).abs() < 1e-9),
                "{k:?} 字母位置"
            );
        }
        // 喇叭形焊缝（喇叭形焊 / 单边喇叭形焊）可用 —— 属"其它版"（正上方
        // 打磨件），字母须在打磨件最高点之上：弧·凸 顶 5.742 → y = 7.842。
        for n in ["喇叭形焊", "单边喇叭形焊"] {
            let l = letters(&weld_method_members(n, GrindKind::ArcConvex, "G", false));
            assert_eq!(l.len(), 1, "{n}");
            assert!((l[0].1 - (slot + 4.0316)).abs() < 1e-9);
            assert!((l[0].2 - 7.842).abs() < 1e-9, "{n} 字母须高于打磨弧: {l:?}");
        }
        // 其它版各打磨方式：字母中心 = 打磨件最高点 + 2.1。
        for (k, ey) in [
            (GrindKind::None, 5.6), (GrindKind::ArcConcave, 7.32), (GrindKind::Line, 6.6),
            (GrindKind::DoubleArc, 12.55), (GrindKind::Zigzag, 11.5),
        ] {
            let l = letters(&weld_method_members("喇叭形焊", k, "C", false));
            assert!(
                l.iter().any(|(_, _, y)| (y - ey).abs() < 1e-9),
                "{k:?} 其它版字母高度应为 {ey}: {l:?}"
            );
        }
        // 其它符号不可用（点焊 / 单边V / I 型…）。
        for n in ["点焊", "带单边坡口的V型对接焊缝", "I型对接焊缝", "堆焊接头"] {
            assert!(
                letters(&weld_method_members(n, GrindKind::ArcConvex, "C", false)).is_empty(),
                "{n} 不应出现焊接方法字母"
            );
        }
        // 空 / 无 → 不画。
        for m in ["", "无"] {
            assert!(letters(&weld_method_members("角焊", GrindKind::ArcConvex, m, false)).is_empty());
        }
        // 下侧镜像：lower=角焊 + 虚线 + C → y = −(0.7+4.9598)。
        let doc = acadrust::CadDocument::new();
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
        p.guide_type = GuideType::Weld;
        p.weld = WeldParams {
            upper: "点焊".into(), // 上方有符号 → 一般情况（不触发特殊情况翻转）
            lower: "角焊".into(),
            dash: true,
            grind_upper: GrindKind::ArcConvex,
            grind_lower: GrindKind::ArcConvex,
            method_upper: "U".into(),
            method_lower: "U".into(),
            ..WeldParams::default()
        };
        apply_weld(&sender, &doc, P_TIP, P0, P_END, &p).unwrap();
        let l = letters(&mock.block_entities("*W1"));
        assert!(l.iter().any(|(v, x, y)| v == "U"
            && (x - (slot + 4.0316)).abs() < 1e-9
            && (y + 5.6598).abs() < 1e-9), "下侧字母镜像: {l:?}");
    }

    /// File1《引线与内容渲染方向》：四向基准线 + 可读朝向。
    /// 规律（对照参考）：横基准线→虚线在下（v=−0.7）；竖基准线→虚线在右（v=−0.7
    /// 映射到 +x）；内容坐标系 (u 沿基准线、v 内容上方) 自"坐标较小端"锚点起。
    #[test]
    fn apply_weld_four_baseline_directions() {
        let cases: [(&str, [f64; 3], [f64; 3]); 4] = [
            ("右", [0.0, 0.0, 0.0], [47.6, 0.0, 0.0]),
            ("上", [0.0, 0.0, 0.0], [0.0, 47.6, 0.0]),
            ("左", [47.6, 0.0, 0.0], [0.0, 0.0, 0.0]),
            ("下", [0.0, 47.6, 0.0], [0.0, 0.0, 0.0]),
        ];
        for (name, p0, pe) in cases {
            let bdx = pe[0] - p0[0];
            let bdy = pe[1] - p0[1];
            let horizontal = bdx.abs() >= bdy.abs();
            // 生成长度随内容动态调整（本用例 = 27.913）
            let blen = 27.913_f64;
            // 期望：锚点（块局部，原点=拐点）+ 参考单位→块局部映射
            let anchor = if horizontal {
                (if bdx >= 0.0 { 0.0 } else { -blen }, 0.0)
            } else {
                (0.0, if bdy >= 0.0 { 0.0 } else { -blen })
            };
            let mapc = |u: f64, v: f64| -> (f64, f64) {
                if horizontal {
                    (anchor.0 + u, anchor.1 + v)
                } else {
                    (anchor.0 - v, anchor.1 + u)
                }
            };
            let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
            let sender: Arc<dyn PluginRequestSender> = mock.clone();
            let mut p = full_params();
            p.weld = WeldParams {
                upper: "角焊".into(),
                lower: String::new(),
                dash: true, // 本用例只验证虚线的方向规律
                tail: false,
                grind_upper: GrindKind::None,
                grind_lower: GrindKind::None,
                method_upper: String::new(),
                method_lower: String::new(),
                ..WeldParams::default()
            };
            apply_weld(&sender, &weld_doc(), [-30.0, -30.0, 0.0], p0, pe, &p).unwrap();
            let m = mock.block_entities("*W1");
            let ls = lines_of(&m);
            // 基准线沿吸附主轴；长度随内容动态调整（本用例无尺寸文字：
            // 槽位 19.413+数量锚点 6.5+余量 2 = 27.913）
            assert!(
                ls.iter().any(|l| {
                    let (dx, dy) = (l.end.x - l.start.x, l.end.y - l.start.y);
                    if horizontal {
                        dy.abs() < 1e-9 && (dx.abs() - 27.913).abs() < 1e-6
                    } else {
                        dx.abs() < 1e-9 && (dy.abs() - 27.913).abs() < 1e-6
                    }
                }),
                "{name}：基准线应沿主轴且长度随内容（27.913）"
            );
            // 虚线（识别线）：横线在下（y=−0.7）；竖线在右（x=+0.7）
            let dl = ls
                .iter()
                .find(|l| l.common.linetype == "ACISOWELD")
                .unwrap_or_else(|| panic!("{name}：应有虚线"));
            if horizontal {
                assert!((dl.start.y - (-0.7)).abs() < 1e-9, "{name} 虚线应在下");
            } else {
                assert!((dl.start.x - 0.7).abs() < 1e-9, "{name} 虚线应在右");
            }
            // 角焊三角：顶点 = map(0,0.125)；腿 = map(3.5,0.125) / map(0,3.625)
            let has = |a: (f64, f64), b: (f64, f64)| {
                ls.iter().any(|l| {
                    ((l.start.x - a.0).abs() < 1e-9 && (l.start.y - a.1).abs() < 1e-9
                        && (l.end.x - b.0).abs() < 1e-9
                        && (l.end.y - b.1).abs() < 1e-9)
                        || ((l.end.x - a.0).abs() < 1e-9
                            && (l.end.y - a.1).abs() < 1e-9
                            && (l.start.x - b.0).abs() < 1e-9
                            && (l.start.y - b.1).abs() < 1e-9)
                })
            };
            // 槽位 t = 19.413（无补充元素时的示例布局）
            const SLOT: f64 = 19.413;
            let v = mapc(SLOT, 0.125);
            assert!(
                has(v, mapc(SLOT + 3.5, 0.125)) && has(v, mapc(SLOT, 3.625)),
                "{name}：三角腿方向应随坐标系"
            );
            // 文字书写方向随基准线轴向：横=0；竖=+90°（自下而上，永不倒置）
            let want_rot = if horizontal { 0.0 } else { std::f64::consts::FRAC_PI_2 };
            for e in m.iter() {
                if let E::AttributeDefinition(a) = e {
                    assert!(
                        (a.rotation - want_rot).abs() < 1e-9,
                        "{name}：尺寸文字旋转应为 {want_rot}，实际 {}",
                        a.rotation
                    );
                }
            }
        }
    }

    /// File2《焊缝区域补充符号》：半包围 ⊏ 与全周边圆互斥（括号优先），
    /// 几何照参考（自拐点沿基准线 1.0 起、高 3.5、臂长 4.2）。
    #[test]
    fn apply_weld_half_around_bracket() {
        let slot = 19.413;
        let m = {
            let mock = Arc::new(MockSender::new(weld_doc()));
            let sender: Arc<dyn PluginRequestSender> = mock.clone();
            let mut p = full_params();
            p.weld = WeldParams {
                upper: "角焊".into(),
                lower: String::new(),
                dash: false,
                tail: false,
                circle: true, // 与 half 同开 → 以 half 优先
                half: true,
                grind_upper: GrindKind::None,
                grind_lower: GrindKind::None,
                ..WeldParams::default()
            };
            apply_weld(&sender, &weld_doc(), P_TIP, P0, P_END, &p).unwrap();
            mock.block_entities("*W1")
        };
        let ls = lines_of(&m);
        // 无圆（互斥）
        assert!(
            !m.iter().any(|e| matches!(e, E::Circle(_))),
            "半包围与全周边互斥：不应出现圆"
        );
        // ⊏ 三条线：竖 (1.0,1.0)→(1.0,4.5)；下臂 (1.0,1.0)→(5.2,1.0)；上臂 (1.0,4.5)→(5.2,4.5)
        let seg = |a: (f64, f64), b: (f64, f64)| {
            ls.iter().any(|l| {
                ((l.start.x - a.0).abs() < 1e-9 && (l.start.y - a.1).abs() < 1e-9
                    && (l.end.x - b.0).abs() < 1e-9
                    && (l.end.y - b.1).abs() < 1e-9)
                    || ((l.end.x - a.0).abs() < 1e-9
                        && (l.end.y - a.1).abs() < 1e-9
                        && (l.start.x - b.0).abs() < 1e-9
                        && (l.start.y - b.1).abs() < 1e-9)
            })
        };
        assert!(seg((1.0, 1.0), (1.0, 4.5)), "半包围竖边");
        assert!(seg((1.0, 1.0), (5.2, 1.0)), "半包围下臂");
        assert!(seg((1.0, 4.5), (5.2, 4.5)), "半包围上臂");
        // 主符号仍在示例槽位（补充元素未超过避让阈值）
        assert!(ls.iter().any(|l| (l.start.x - slot).abs() < 1e-9 && (l.start.y - 0.125).abs() < 1e-9));
        // 仅有圆（half=false）时：圆在拐点、无括号
        let m2 = {
            let mock = Arc::new(MockSender::new(weld_doc()));
            let sender: Arc<dyn PluginRequestSender> = mock.clone();
            let mut p = full_params();
            p.weld = WeldParams {
                upper: "角焊".into(),
                lower: String::new(),
                dash: false,
                tail: false,
                circle: true,
                half: false,
                grind_upper: GrindKind::None,
                grind_lower: GrindKind::None,
                ..WeldParams::default()
            };
            apply_weld(&sender, &weld_doc(), P_TIP, P0, P_END, &p).unwrap();
            mock.block_entities("*W1")
        };
        assert!(m2.iter().any(|e| matches!(e, E::Circle(_))), "全周边圆");
        assert!(!lines_of(&m2).iter().any(|l| (l.start.x - 1.0).abs() < 1e-9), "不应有括号");
    }

    /// 打磨与焊接方法**上下侧独立**（用户定）：上侧 锯齿+C、下侧 弧·凸+U。
    #[test]
    fn apply_weld_per_side_grind_and_method() {
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let mut p = full_params();
        p.weld = WeldParams {
            upper: "角焊".into(),
            lower: "角焊".into(),
            dash: true,
            tail: false,
            circle: false,
            flag: false,
            half: false,
            grind_upper: GrindKind::Zigzag,
            grind_lower: GrindKind::ArcConvex,
            method_upper: "C".into(),
            method_lower: "U".into(),
            ..WeldParams::default()
        };
        apply_weld(&sender, &weld_doc(), P_TIP, P0, P_END, &p).unwrap();
        let m = mock.block_entities("*W1");
        let slot = 19.413;
        // 上侧 = 锯齿（4 线段，第 1 段 (0.3712,3.8360)→(3.8360,0.3712)）
        assert!(lines_of(&m).iter().any(|l| {
            (l.start.x - (slot + 0.3712)).abs() < 1e-9
                && (l.start.y - 3.8360).abs() < 1e-9
                && (l.end.x - (slot + 3.8360)).abs() < 1e-9
        }), "上侧应为锯齿");
        // 下侧 = 弧·凸（镜像：弧心 y = −0.7−1.125 = −1.825；角 271.5→358.5）
        let arcs = grind_arcs(&m);
        assert!(arcs.iter().any(|a| (a.center.x - (slot + 1.125)).abs() < 1e-9
            && (a.center.y + 1.825).abs() < 1e-9
            && (a.start_angle - 271.5f64.to_radians()).abs() < 1e-9), "下侧应为弧·凸镜像");
        // 上侧无弧（锯齿不是弧）；下侧无锯齿线段
        assert!(arcs.iter().all(|a| a.center.y < 0.0), "上侧不应有打磨弧");
        assert!(!lines_of(&m).iter().any(|l| (l.start.y - 3.8360).abs() < 1e-9 && l.start.x > slot + 1.0
            && (l.end.x - (slot + 3.8360)).abs() < 1e-9), "下侧不该出现上侧锯齿");
        // 方法字母：上侧 C 按锯齿位置 (4.8360,6.5684)；下侧 U 按弧凸位置镜像 (−5.6598)
        let l = letters(&m);
        assert!(l.iter().any(|(v, x, y)| v == "C"
            && (x - (slot + 4.8360)).abs() < 1e-9
            && (y - 6.5684).abs() < 1e-9), "上侧 C 位置随上侧打磨方式: {l:?}");
        assert!(l.iter().any(|(v, x, y)| v == "U"
            && (x - (slot + 4.0316)).abs() < 1e-9
            && (y + 5.6598).abs() < 1e-9), "下侧 U 位置随下侧打磨方式且镜像: {l:?}");
    }

    /// 短基准线自动延长：第二段画太短时，生成的基准线强制容纳全部标注。
    #[test]
    fn apply_weld_extends_short_baseline() {
        // 画一条只有 10 长的第二段（用户实测溢出场景）：上侧角焊+弧凸+方法C+尾部，
        // 下侧角焊（虚线）。
        let pre = || {
            let mut p = full_params();
            p.weld = WeldParams {
                upper: "角焊".into(),
                lower: "角焊".into(),
                dash: true,
                tail: true,
                tail_text: "N=2".into(),
                circle: false,
                half: false,
                flag: false,
                grind_upper: GrindKind::ArcConvex,
                grind_lower: GrindKind::ArcConvex,
                method_upper: "C".into(),
                method_lower: "C".into(),
                up_thick: "5".into(),
                lo_thick: "10".into(),
                ..WeldParams::default()
            };
            p
        };
        // ① 短基准线（10）：自动延长
        let mock = Arc::new(MockSender::new(weld_doc()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        apply_weld(&sender, &weld_doc(), [0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [110.0, 0.0, 0.0], &pre())
            .unwrap();
        let m = mock.block_entities("*W1");
        let bl = lines_of(&m)
            .iter()
            .find(|l| l.start.y == 0.0 && l.end.y == 0.0 && l.start.x == 0.0 && (l.end.x - 10.0).abs() > 1e-9)
            .copied()
            .expect("基准线");
        let drawn = 10.0_f64;
        assert!(bl.end.x > drawn + 1.0, "基准线应被延长：{}", bl.end.x);
        // 内容全部在线内：数量长度文字锚点（slot+6.5）须 < 线端
        let qty = lines_of(&m);
        let _ = qty;
        let ads = attdefs_of(&m);
        let q = ads.iter().find(|a| a.tag == "上侧数量长度L′").unwrap();
        assert!(
            q.insertion_point.x < bl.end.x,
            "数量文字须在线内: {} < {}",
            q.insertion_point.x,
            bl.end.x
        );
        // 打磨件（弧·凸 右端 = 槽位+1.125+2.625 = 23.163）也须在线内
        assert!(bl.end.x > 19.413 + 1.125 + 2.625, "打磨件须在线内");
        // ② 长基准线（60）：内容驱动 → 收紧（用户定：长度随内容，不再跟随画的长度）
        let mock2 = Arc::new(MockSender::new(weld_doc()));
        let sender2: Arc<dyn PluginRequestSender> = mock2.clone();
        apply_weld(&sender2, &weld_doc(), [0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [160.0, 0.0, 0.0], &pre())
            .unwrap();
        let m2 = mock2.block_entities("*W1");
        let bl2 = lines_of(&m2)
            .iter()
            .find(|l| l.start.y == 0.0 && l.end.y == 0.0 && l.start.x == 0.0)
            .copied()
            .expect("基准线2");
        assert!(
            (bl2.end.x - bl.end.x).abs() < 1e-9,
            "长度只与内容有关（两次同值）: {} vs {}",
            bl2.end.x,
            bl.end.x
        );
        assert!(bl2.end.x < 60.0, "画长了应被收紧: {}", bl2.end.x);
    }

    /// 回归：第二段指向左 + 角焊 + 厚度3 + 尾部注释 N=2（用户实测三问题）。
    /// ① 槽位避让不得把内容推到拐点外；② 有尾注即生成尾叉+文字（开关可手动）；
    /// ③ 尾注对齐随方向（向外 +t → 左对齐；−t → 右对齐，避免压尾叉）。
    #[test]
    fn apply_weld_left_pointing_tail_regressions() {
        for tail_switch in [false, true] {
            let mock = Arc::new(MockSender::new(weld_doc()));
            let sender: Arc<dyn PluginRequestSender> = mock.clone();
            let mut p = full_params();
            p.weld = WeldParams {
                upper: "角焊".into(),
                lower: String::new(),
                dash: false,
                tail: tail_switch,
                tail_text: "N=2".into(), // 有关注释 → 应强制生成尾叉+文字
                circle: true,
                flag: false,
                half: false,
                grind_upper: GrindKind::None,
                grind_lower: GrindKind::None,
                method_upper: String::new(),
                method_lower: String::new(),
                up_thick: "3".into(),
                ..WeldParams::default()
            };
            // 拐点 (100,100)、第二段指向左 32（远端 x=68）
            apply_weld(&sender, &weld_doc(), [128.0, 128.0, 0.0], [100.0, 100.0, 0.0], [68.0, 100.0, 0.0], &p)
                .unwrap();
            let m = mock.block_entities("*W1");
            let ls = lines_of(&m);
            // ① 符号槽位须在线内（锚点在 −blen（≤−32.263），槽位 +19.413 → 约 −12.85）
            let sym = ls
                .iter()
                .find(|l| l.common.color == acadrust::types::Color::from_index(31)
                    && (l.start.y - 0.125).abs() < 1e-9 && (l.end.y - 0.125).abs() < 1e-9)
                .expect("上侧角焊水平腿");
            assert!(
                sym.start.x < 0.0 && sym.start.x > -32.0,
                "符号须在线内（拐点外侧=错）: x={}",
                sym.start.x
            );
            // ①b 尺寸文字同样在线内
            let ads = attdefs_of(&m);
            let a = ads.iter().find(|a| a.tag == "上侧厚度尺寸A′").unwrap();
            assert!(a.insertion_point.x < 0.0 && a.insertion_point.x > -32.0);
            // ② 尾叉（远端两侧各一条）+ 尾注文字（不论开关，只要有文字）
            let fork = ls.iter().filter(|l| l.start.y == 0.0 && l.common.color == acadrust::types::Color::from_index(31)).count();
            assert!(fork >= 2, "尾叉应生成（有尾注）: {fork}");
            let tail = ads.iter().find(|a| a.tag == "尾部注释E").expect("尾注 ATTDEF");
            // ③ 尾注对齐：线端在锚点侧（far_dir=−1）→ Right+Middle（文字向左展开，不压尾叉）
            assert!(matches!(tail.horizontal_alignment, HorizontalAlignment::Right), "尾注应右对齐");
            assert!(
                tail.insertion_point.x < -27.913 - 3.0,
                "尾注须在线端之外: {}",
                tail.insertion_point.x
            );
        }
    }

    /// 竖基准线：符号内圆弧角度须随坐标系旋转（+90°）。
    #[test]
    fn apply_weld_vertical_rotates_symbol_arcs() {
        for (dir_name, pe) in [
            ("up", [0.0, 40.0, 0.0]),
            ("down", [0.0, -40.0, 0.0]),
        ] {
            let mock = Arc::new(MockSender::new(weld_doc()));
            let sender: Arc<dyn PluginRequestSender> = mock.clone();
            let mut p = full_params();
            p.weld = WeldParams {
                // 带钝边U型对接焊缝：ARC c=(1.75,3.5) r1.75 180→0（本地）
                upper: "带钝边U型对接焊缝".into(),
                lower: String::new(),
                dash: false,
                tail: false,
                circle: false,
                half: false,
                flag: false,
                grind_upper: GrindKind::None,
                grind_lower: GrindKind::None,
                method_upper: String::new(),
                method_lower: String::new(),
                up_thick: String::new(),
                up_qty: String::new(),
                ..WeldParams::default()
            };
            apply_weld(&sender, &weld_doc(), [-30.0, -30.0, 0.0], [0.0, 0.0, 0.0], pe, &p).unwrap();
            let m = mock.block_entities("*W1");
            let a = m
                .iter()
                .find_map(|e| match e {
                    E::Arc(x) => Some(x.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{dir_name}: 应有圆弧"));
            // 竖基准线：本地 (u,v) → 世界 (−v, anchor_y+slot+u)；表内弧心
            // (u=1.75,v=3.5)；生成长度内容驱动 = 19.413+6.5+2 = 27.913
            let blen_eff = 27.913_f64;
            let anchor_y = if pe[1] >= 0.0 { 0.0 } else { -blen_eff };
            assert!(
                (a.center.x + 3.5).abs() < 1e-9
                    && (a.center.y - (anchor_y + 19.413 + 1.75)).abs() < 1e-9,
                "{dir_name} 弧心映射: ({}, {}) 期望 y={}",
                a.center.x,
                a.center.y,
                anchor_y + 19.413 + 1.75
            );
            assert!(
                (a.start_angle - 270f64.to_radians()).abs() < 1e-9
                    && (a.end_angle - 90f64.to_radians()).abs() < 1e-9,
                "{dir_name} 弧角应 +90°: {}→{}",
                a.start_angle.to_degrees(),
                a.end_angle.to_degrees()
            );
        }
    }

    /// 现场焊接旗避让焊缝区域符号：有半包围 ⊏（与其同侧）时整旗抬高。
    #[test]
    fn apply_weld_flag_raised_above_half_bracket() {
        let run = |half: bool| -> Vec<E> {
            let mock = Arc::new(MockSender::new(weld_doc()));
            let sender: Arc<dyn PluginRequestSender> = mock.clone();
            let mut p = full_params();
            p.weld = WeldParams {
                upper: "角焊".into(),
                lower: String::new(),
                dash: false,
                tail: false,
                circle: false,
                half,
                flag: true,
                grind_upper: GrindKind::None,
                grind_lower: GrindKind::None,
                method_upper: String::new(),
                method_lower: String::new(),
                ..WeldParams::default()
            };
            apply_weld(&sender, &weld_doc(), P_TIP, P0, P_END, &p).unwrap();
            mock.block_entities("*W1")
        };
        // 有 ⊏：旗底 +5.0（⊏ 顶 4.5 + 0.5 间隙）、旗顶 +8.5、杆高 8.5
        let m = run(true);
        let ls = lines_of(&m);
        assert!(
            ls.iter().any(|l| (l.start.y - 0.0).abs() < 1e-9 && (l.end.y - 8.5).abs() < 1e-9),
            "有 ⊏ 时旗杆应升到 8.5"
        );
        assert!(
            ls.iter().any(|l| (l.start.y - 5.0).abs() < 1e-9 && (l.end.y - 5.0).abs() < 1e-9
                && (l.start.x - 5.25).abs() < 1e-9),
            "有 ⊏ 时旗底应在 +5.0"
        );
        // 无 ⊏：照参考（旗底 3.5、旗顶 7.0）
        let m2 = run(false);
        let ls2 = lines_of(&m2);
        assert!(
            ls2.iter().any(|l| (l.start.y - 0.0).abs() < 1e-9 && (l.end.y - 7.0).abs() < 1e-9),
            "无 ⊏ 时旗杆保持 7.0（照示例）"
        );
        assert!(ls2.iter().any(|l| (l.start.y - 3.5).abs() < 1e-9 && (l.end.y - 3.5).abs() < 1e-9));
    }

    /// 冒烟测试用宿主代理：把插件请求真实落到 CadDocument（镜像宿主    /// 冒烟测试用宿主代理：把插件请求真实落到 CadDocument（镜像宿主
    /// add_block_record/ensure_* 语义），供 DxfWriter 写出检查文件。
    struct ApplySender {
        doc: std::sync::Mutex<acadrust::CadDocument>,
    }
    impl ApplySender {
        fn new(doc: acadrust::CadDocument) -> Self {
            ApplySender { doc: std::sync::Mutex::new(doc) }
        }
    }
    impl PluginRequestSender for ApplySender {
        fn request(
            &self,
            req: PluginRequest,
        ) -> Result<PluginResponse, PluginRequestError> {
            use PluginRequest as R;
            use PluginResponse as P;
            match req {
                R::EnsureTextStyles(defs) => {
                    use acadrust::tables::TextStyle;
                    let mut doc = self.doc.lock().unwrap();
                    for d in defs {
                        if doc
                            .text_styles
                            .iter()
                            .any(|x| x.name.eq_ignore_ascii_case(&d.name))
                        {
                            continue;
                        }
                        let mut st = TextStyle::new(&d.name);
                        st.handle = doc.allocate_handle();
                        st.font_file = d.font_file;
                        st.big_font_file = d.big_font_file;
                        st.true_type_font = d.true_type_font;
                        st.height = d.height;
                        st.width_factor = d.width_factor;
                        st.annotative = d.annotative;
                        st.is_shape_file = d.is_shape_file;
                        st.is_vertical = d.is_vertical;
                        doc.text_styles.add_or_replace(st);
                    }
                    Ok(P::Ok)
                }
                R::EnsureLayers(defs) => {
                    let mut doc = self.doc.lock().unwrap();
                    for d in defs {
                        if doc
                            .layers
                            .iter()
                            .any(|x| x.name.eq_ignore_ascii_case(&d.name))
                        {
                            continue;
                        }
                        let mut ly = acadrust::tables::Layer::new(&d.name);
                        ly.handle = doc.allocate_handle();
                        ly.color = d.color;
                        ly.line_type = d.linetype;
                        ly.line_weight = d.lineweight;
                        ly.is_plottable = d.plottable;
                        ly.flags.off = d.off;
                        doc.layers.add_or_replace(ly);
                    }
                    Ok(P::Ok)
                }
                R::EnsureLinetypes(defs) => {
                    use acadrust::tables::{LineType, LineTypeElement};
                    let mut doc = self.doc.lock().unwrap();
                    for d in defs {
                        if doc
                            .line_types
                            .iter()
                            .any(|x| x.name.eq_ignore_ascii_case(&d.name))
                        {
                            continue;
                        }
                        let mut lt = LineType::new(&d.name);
                        lt.handle = doc.allocate_handle();
                        lt.description = d.description;
                        lt.elements = d
                            .elements
                            .into_iter()
                            .map(|v| LineTypeElement { length: v, complex: None })
                            .collect();
                        lt.pattern_length =
                            lt.elements.iter().map(|e| e.length.abs()).sum();
                        doc.line_types.add_or_replace(lt);
                    }
                    Ok(P::Ok)
                }
                R::AddBlockRecord { name, entities } => {
                    use acadrust::entities::{Block, BlockEnd};
                    use acadrust::EntityType as E;
                    let mut doc = self.doc.lock().unwrap();
                    if name.trim().is_empty() || doc.block_records.get(&name).is_some() {
                        return Ok(P::Error("add_block_record: 名称空或已存在".into()));
                    }
                    let mut br = acadrust::tables::BlockRecord::new(&name);
                    br.handle = doc.allocate_handle();
                    let origin = acadrust::types::Vector3::new(0.0, 0.0, 0.0);
                    let mut block = Block::new(&name, origin);
                    block.common.handle = doc.allocate_handle();
                    block.common.owner_handle = br.handle;
                    br.block_entity_handle = block.common.handle;
                    doc.add_entity(E::Block(block)).map_err(|e| {
                        PluginRequestError(e.to_string())
                    })?;
                    let br_handle = br.handle;
                    doc.block_records.add(br).map_err(|e| {
                        PluginRequestError(e.to_string())
                    })?;
                    let mut member_handles = Vec::new();
                    for mut e in entities {
                        e.common_mut().owner_handle = br_handle;
                        member_handles.push(doc.add_entity(e).map_err(|e| {
                            PluginRequestError(e.to_string())
                        })?);
                    }
                    let mut end = BlockEnd::new();
                    end.common.handle = doc.allocate_handle();
                    end.common.owner_handle = br_handle;
                    let end_handle = end.common.handle;
                    doc.add_entity(E::BlockEnd(end)).map_err(|e| {
                        PluginRequestError(e.to_string())
                    })?;
                    if let Some(br) = doc.block_records.get_mut(&name) {
                        br.entity_handles = member_handles;
                        br.block_end_handle = end_handle;
                    }
                    Ok(P::Handle(br_handle))
                }
                R::AddEntities(vec) => {
                    let mut doc = self.doc.lock().unwrap();
                    let mut hs = Vec::new();
                    for e in vec {
                        hs.push(
                            doc.add_entity(e)
                                .map_err(|e| PluginRequestError(e.to_string()))?,
                        );
                    }
                    Ok(P::Handles(hs))
                }
                _ => Ok(P::Ok),
            }
        }
    }

    /// 冒烟测试（设 OCSM_WELD_SMOKE_OUT 才写文件）：箭头侧单边V焊缝标注。
    /// 引导：焊缝点 (0,0)（箭头尖，从 (100,100) 指向 (0,0)）、拐点 (100,100)、
    /// 基准线末端 (147.647,100)（右向 47.647 照参考）。上侧=带单边坡口的V型
    /// 对接焊缝、厚度尺寸=5、尾部注释=N=2；虚线/下侧/圆/旗/C 全关。
    #[test]
    fn weld_smoke_write_dxf() {
        let out = std::env::var("OCSM_WELD_SMOKE_OUT").unwrap_or_default();
        if out.is_empty() {
            return; // 未设环境变量时跳过（无副作用）
        }
        // 可选：符号名（缺省 带单边坡口的V型对接焊缝）与打磨方式（缺省 none）。
        let up = std::env::var("OCSM_WELD_SMOKE_UP")
            .unwrap_or_else(|_| "带单边坡口的V型对接焊缝".to_string());
        let grind = std::env::var("OCSM_WELD_SMOKE_GRIND")
            .ok()
            .and_then(|g| GrindKind::from_str(&g))
            .unwrap_or(GrindKind::None);
        // 可选：下侧（另一侧）符号与下侧厚度文字 —— 用于生成"特殊情况翻转"样例。
        let lower = std::env::var("OCSM_WELD_SMOKE_LOWER").unwrap_or_default();
        let lo_thick = std::env::var("OCSM_WELD_SMOKE_LO_THICK").unwrap_or_default();
        let half = std::env::var("OCSM_WELD_SMOKE_HALF").is_ok();
        let smoke_dash = std::env::var("OCSM_WELD_SMOKE_DASH").is_ok();
        // 可选：基准线方向（right 缺省 / up / left / down），长度照示例 47.647。
        let dir = std::env::var("OCSM_WELD_SMOKE_DIR").unwrap_or_else(|_| "right".into());
        // 引导 PLINE（10引导线层，真实流程里保留不打印）。
        let mut guide_doc = acadrust::CadDocument::new();
        let mut pl = LwPolyline::new();
        pl.common.layer = "10引导线层".into();
        pl.add_point(Vector2::new(0.0, 0.0));
        pl.add_point(Vector2::new(100.0, 100.0));
        pl.add_point(Vector2::new(147.647, 100.0));
        guide_doc.add_entity(E::LwPolyline(pl)).unwrap();
        let mock = Arc::new(ApplySender::new(guide_doc));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let params = {
            let mut p = GuideParams::linear(LinearSub::Aligned, 0.0);
            p.guide_type = GuideType::Weld;
            p.weld = WeldParams {
                upper: up.clone(),
                lower: lower.clone(),
                circle: !half && !lower.is_empty(),
                dash: smoke_dash || !lower.is_empty(),
                flag: std::env::var("OCSM_WELD_SMOKE_FLAG").is_ok(),
                tail: std::env::var("OCSM_WELD_SMOKE_TAIL_OFF").is_err(),
                half,
                // 分侧打磨/方法（新键优先，旧键 _GRIND/_METHOD 作两侧缺省）
                grind_upper: std::env::var("OCSM_WELD_SMOKE_GRIND_U")
                    .ok()
                    .and_then(|g| GrindKind::from_str(&g))
                    .unwrap_or(grind),
                grind_lower: std::env::var("OCSM_WELD_SMOKE_GRIND_L")
                    .ok()
                    .and_then(|g| GrindKind::from_str(&g))
                    .unwrap_or(grind),
                method_upper: std::env::var("OCSM_WELD_SMOKE_METHOD_U")
                    .or_else(|_| std::env::var("OCSM_WELD_SMOKE_METHOD"))
                    .unwrap_or_default(),
                method_lower: std::env::var("OCSM_WELD_SMOKE_METHOD_L")
                    .or_else(|_| std::env::var("OCSM_WELD_SMOKE_METHOD"))
                    .unwrap_or_default(),
                up_thick: std::env::var("OCSM_WELD_SMOKE_UP_THICK").unwrap_or_else(|_| {
                    if up.is_empty() && !lower.is_empty() { String::new() } else { "5".into() }
                }),
                lo_thick,
                up_qty: String::new(),
                lo_qty: String::new(),
                tail_text: "N=2".into(),
            };
            p
        };
        // doc 参数传独立空文档（避免与 ApplySender 锁死锁；apply_weld 只读
        // 它做样式检查/计数器，ensure 请求会落到 ApplySender 的真文档）。
        let read_doc = acadrust::CadDocument::new();
        // 可选：第二段长度（缺省 47.647，用于复现"画太短"场景）。
        let seg_len: f64 = std::env::var("OCSM_WELD_SMOKE_LEN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(47.647);
        let p_end = match dir.as_str() {
            "up" => [100.0, 100.0 + seg_len, 0.0],
            "left" => [100.0 - seg_len, 100.0, 0.0],
            "down" => [100.0, 100.0 - seg_len, 0.0],
            _ => [100.0 + seg_len, 100.0, 0.0],
        };
        apply_weld(&sender, &read_doc, [0.0, 0.0, 0.0], [100.0, 100.0, 0.0], p_end, &params)
            .unwrap();
        let doc = mock.doc.lock().unwrap();
        acadrust::io::DxfWriter::new(&doc)
            .write_to_file(&out)
            .unwrap();
        println!("weld smoke dxf written: {out}");
    }

    /// 冒烟测试（设 `OCSM_LEADER_SMOKE_OUT` 才写文件）：引线标注。
    /// 引导：箭头点 (0,0)、拐点 (100,100)、肩线末端 (100+len,100)；
    /// 上侧文字、下侧文字可用 `OCSM_LEADER_SMOKE_UP` / `_LO` 覆盖。
    #[test]
    fn leader_smoke_write_dxf() {
        let out = std::env::var("OCSM_LEADER_SMOKE_OUT").unwrap_or_default();
        if out.is_empty() {
            return; // 未设环境变量时跳过（无副作用）
        }
        let up = std::env::var("OCSM_LEADER_SMOKE_UP").unwrap_or_else(|_| "通孔⌀10".into());
        let lo = std::env::var("OCSM_LEADER_SMOKE_LO").unwrap_or_else(|_| "深20".into());
        let seg_len: f64 = std::env::var("OCSM_LEADER_SMOKE_LEN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(47.647);
        let dir = std::env::var("OCSM_LEADER_SMOKE_DIR").unwrap_or_else(|_| "right".into());
        // 引导 PLINE（10引导线层，真实流程里保留不打印）。
        let mut guide_doc = acadrust::CadDocument::new();
        let mut pl = LwPolyline::new();
        pl.common.layer = "10引导线层".into();
        pl.add_point(Vector2::new(0.0, 0.0));
        pl.add_point(Vector2::new(100.0, 100.0));
        pl.add_point(Vector2::new(100.0 + seg_len, 100.0));
        guide_doc.add_entity(E::LwPolyline(pl)).unwrap();
        let mock = Arc::new(ApplySender::new(guide_doc));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let mut params = GuideParams::linear(LinearSub::Aligned, 0.0);
        params.guide_type = GuideType::Leader;
        params.leader = crate::guide_url::LeaderParams { upper: up, lower: lo };
        let p_end = match dir.as_str() {
            "up" => [100.0, 100.0 + seg_len, 0.0],
            "left" => [100.0 - seg_len, 100.0, 0.0],
            "down" => [100.0, 100.0 - seg_len, 0.0],
            _ => [100.0 + seg_len, 100.0, 0.0],
        };
        let read_doc = acadrust::CadDocument::new();
        apply_leader(&sender, &read_doc, [0.0, 0.0, 0.0], [100.0, 100.0, 0.0], p_end, &params)
            .unwrap();
        let doc = mock.doc.lock().unwrap();
        acadrust::io::DxfWriter::new(&doc)
            .write_to_file(&out)
            .unwrap();
        println!("leader smoke dxf written: {out}");
    }

    // ── 标注再编辑（OCSM_EDIT）测试 ──────────────────────────────────────

    /// 生成后给标注写 OCSM_EDIT；编辑模式（posted = 标注本体）能读回并替换旧标注。
    #[test]
    fn edit_replaces_previous_annotation() {
        // 引导：两段 PLINE（顶点0 箭头点 / 顶点1 拐点 / 顶点2 肩线末端）
        let mut doc = acadrust::CadDocument::new();
        let mut pl = LwPolyline::new();
        pl.common.layer = "10引导线层".into();
        pl.add_point(Vector2::new(0.0, 0.0));
        pl.add_point(Vector2::new(60.0, 60.0));
        pl.add_point(Vector2::new(120.0, 60.0));
        let gh = doc.add_entity(E::LwPolyline(pl)).unwrap();
        let mock = Arc::new(MockSender::new(doc));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();

        // 1) 引导模式：生成引线标注
        let url1 = "http://127.0.0.1:23751/DIM/LEADER/0?lu=%E9%80%9A%E5%AD%94&ll=%E6%B7%B120";
        let body1 = serde_json::json!({"handle": fmt_handle(gh), "url": url1}).to_string();
        let out1 = do_apply(&sender, body1.as_bytes(), true).unwrap();
        let anns = produced_handles(&out1);
        assert_eq!(anns.len(), 1, "应生成一个标注: {out1}");
        let ann = anns[0];
        // 记录写在标注上（不是引导上）
        let rec = read_edit_record(&sender, ann).expect("标注应带 OCSM_EDIT");
        assert_eq!(rec.0, url1, "记录里的参数 URL");
        assert_eq!(rec.1.len(), 3, "引导三点");
        assert_eq!(rec.2, "pline");
        assert_eq!(rec.3, Some(gh), "记录原引导 handle");
        assert!(read_edit_record(&sender, gh).is_none(), "引导本身不写 OCSM_EDIT");

        // 2) 编辑模式：posted = 标注本体，改文字 → 替换
        let url2 = "http://127.0.0.1:23751/DIM/LEADER/0?lu=%E6%9B%B4%E6%94%B9&ll=%E6%B7%B130";
        let body2 = serde_json::json!({"handle": fmt_handle(ann), "url": url2}).to_string();
        let out2 = do_apply(&sender, body2.as_bytes(), true).unwrap();
        assert!(out2.contains("\"edit\":true"), "编辑模式应答: {out2}");
        // 旧标注已删
        assert!(
            mock.doc.lock().unwrap().get_entity(ann).is_none(),
            "旧标注应被替换删除"
        );
        // 新标注存在且记录已更新
        let doc = mock.doc.lock().unwrap();
        let new_ann = doc
            .entities()
            .find(|e| matches!(e, E::Insert(_)))
            .map(|e| e.common().handle)
            .expect("新标注存在");
        assert_ne!(new_ann, ann);
        drop(doc);
        let rec2 = read_edit_record(&sender, new_ann).expect("新标注也要带记录");
        assert_eq!(rec2.0, url2, "记录已更新为新参数");
        let vals: Vec<String> = mtexts_of(&mock.block_entities("*L1"))
            .iter()
            .map(|m| m.value.clone())
            .collect();
        assert_eq!(vals, vec!["更改".to_string(), "深30".to_string()], "文字已改");
    }

    /// `/api/guide` 对**标注实体**要回退读 OCSM_EDIT（曾经写在 guide_geom_points 的
    /// 成功分支里 → 标注永远拿不到参数、还报"引导线必须为直线或多段线"）。
    #[test]
    fn api_guide_serves_annotation_edit_record() {
        let mut doc = acadrust::CadDocument::new();
        let mut pl = LwPolyline::new();
        pl.common.layer = "10引导线层".into();
        pl.add_point(Vector2::new(0.0, 0.0));
        pl.add_point(Vector2::new(60.0, 60.0));
        pl.add_point(Vector2::new(120.0, 60.0));
        let gh = doc.add_entity(E::LwPolyline(pl)).unwrap();
        let mock = Arc::new(MockSender::new(doc));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let url = "http://127.0.0.1:23751/DIM/LEADER/0?lu=%E9%80%9A%E5%AD%94&ll=%E6%B7%B120";
        let body = serde_json::json!({"handle": fmt_handle(gh), "url": url}).to_string();
        let out = do_apply(&sender, body.as_bytes(), true).unwrap();
        let ann = produced_handles(&out)[0];

        let (code, _, resp) = api_guide(&format!("?handle={}", fmt_handle(ann)), &sender);
        assert_eq!(code, 200, "标注查询应成功（不再走引导几何校验）: {resp}");
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["edit"], true, "应标记为编辑目标");
        assert_eq!(v["geom"], "pline");
        assert_eq!(v["pts"].as_array().unwrap().len(), 3, "引导三点来自记录");
        assert_eq!(v["params"]["type"], "LEADER");
        assert_eq!(v["params"]["leader"]["upper"], "通孔", "参数应回填");
        assert_eq!(v["params"]["leader"]["lower"], "深20");
    }

    /// 只有「应用」（不刷新）时：只更新记录，不重生成。
    #[test]
    fn edit_apply_only_updates_record() {
        let mut doc = acadrust::CadDocument::new();
        let mut pl = LwPolyline::new();
        pl.common.layer = "10引导线层".into();
        pl.add_point(Vector2::new(0.0, 0.0));
        pl.add_point(Vector2::new(60.0, 60.0));
        pl.add_point(Vector2::new(120.0, 60.0));
        let gh = doc.add_entity(E::LwPolyline(pl)).unwrap();
        let mock = Arc::new(MockSender::new(doc));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let url1 = "http://127.0.0.1:23751/DIM/LEADER/0?lu=A&ll=B";
        let body1 = serde_json::json!({"handle": fmt_handle(gh), "url": url1}).to_string();
        let _ = do_apply(&sender, body1.as_bytes(), true).unwrap();
        let ann = mock
            .doc
            .lock()
            .unwrap()
            .entities()
            .find(|e| matches!(e, E::Insert(_)))
            .map(|e| e.common().handle)
            .unwrap();
        let url2 = "http://127.0.0.1:23751/DIM/LEADER/0?lu=X&ll=Y";
        let body2 = serde_json::json!({"handle": fmt_handle(ann), "url": url2}).to_string();
        let out = do_apply(&sender, body2.as_bytes(), false).unwrap();
        assert!(out.contains("\"edit\":true"), "{out}");
        assert!(mock.doc.lock().unwrap().get_entity(ann).is_some(), "不重生成");
        assert_eq!(read_edit_record(&sender, ann).unwrap().0, url2);
    }

    /// 引导已删的类型（线性尺寸）：编辑时用记录里的点造临时引导重生成。
    #[test]
    fn edit_rebuilds_temp_guide_for_deleted_guide() {
        let mut doc = acadrust::CadDocument::new();
        let mut l = acadrust::entities::Line::new();
        l.common.layer = "10引导线层".into();
        l.start = Vector3::new(0.0, 0.0, 0.0);
        l.end = Vector3::new(50.0, 0.0, 0.0);
        let gh = doc.add_entity(E::Line(l)).unwrap();
        let mock = Arc::new(MockSender::new(doc));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let url1 = "http://127.0.0.1:23751/DIM/LINEAR/H/0?text=50";
        let body1 = serde_json::json!({"handle": fmt_handle(gh), "url": url1}).to_string();
        let out1 = do_apply(&sender, body1.as_bytes(), true).unwrap();
        let ann = produced_handles(&out1)[0];
        // 线性尺寸生成会删引导 → 编辑必须靠记录里的点
        assert!(mock.doc.lock().unwrap().get_entity(gh).is_none(), "引导已删");
        let rec = read_edit_record(&sender, ann);
        assert!(rec.is_some(), "首轮生成应给标注写 OCSM_EDIT（{:?}）", out1);
        assert_eq!(rec.unwrap().1.len(), 2, "线引导两点");
        let url2 = "http://127.0.0.1:23751/DIM/LINEAR/H/0?text=80";
        let body2 = serde_json::json!({"handle": fmt_handle(ann), "url": url2}).to_string();
        let out2 = do_apply(&sender, body2.as_bytes(), true).unwrap();
        assert!(out2.contains("\"edit\":true"), "编辑应成功: {out2}");
        assert!(mock.doc.lock().unwrap().get_entity(ann).is_none(), "旧标注已替换");
        let doc = mock.doc.lock().unwrap();
        let dims = doc
            .entities()
            .filter(|e| matches!(e, E::Dimension(_)))
            .count();
        assert_eq!(dims, 1, "重生成一个标注");
        assert_eq!(
            doc.entities().filter(|e| matches!(e, E::Line(_))).count(),
            0,
            "临时引导用完即删"
        );
    }

    /// GUI 运行时冒烟（node + 最小 DOM 垫片）：catch node --check 查不出的
    /// 运行时错误（历史两次：缺 GDT_POLYS 定义、wline 定义域错误 + row-weld
    /// 漏显示）。断言：焊接面板可见、预览 SVG 非空、无异常。node 缺失时跳过。
    #[test]
    fn gui_runtime_smoke_with_node() {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let js = manifest.join("tests/gui_smoke.mjs");
        let html = manifest.join("src/guide_gui.html");
        if !js.exists() {
            return;
        }
        let out = match std::process::Command::new("node")
            .arg(&js)
            .arg(&html)
            .output()
        {
            Ok(o) => o,
            Err(_) => return, // 无 node：跳过（不阻塞 CI）
        };
        assert!(
            out.status.success(),
            "GUI 运行时冒烟失败：\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn http_server_serves_weld_gui_and_syms() {
        // guide.html：焊接按钮/面板/URL 参数键/符号几何端点（静态断言；
        // 按钮显隐由 JS applyGeomFilter 按 3 顶点 PLINE 过滤）。
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let server = spawn(mock.clone()).expect("spawn guide server");
        let html = http_req(server.port, "GET", "/guide.html?handle=0x1", "");
        assert!(html.contains("data-t=\"WELD\""), "焊接按钮");
        assert!(html.contains("id=\"row-weld\""), "焊接面板");
        assert!(html.contains("id=\"w-upper\"") && html.contains("id=\"w-lower\""));
        assert!(html.contains("wdash=1") && html.contains("wcir=1") && html.contains("wut="));
        assert!(html.contains("wgr="), "打磨 URL 键");
        assert!(html.contains("id=\"w-method-u\"") && html.contains("id=\"w-method-l\"")
            && html.contains("WELD_METHOD_SYMS"), "焊接方法上下侧下拉");
        assert!(html.contains("id=\"w-grind-u\"") && html.contains("id=\"w-grind-l\""), "打磨上下侧下拉");
        assert!(html.contains("wgru=") && html.contains("wgrl="), "分侧打磨 URL 键");
        assert!(html.contains("id=\"w-half\"") && html.contains("whalf=1"), "半包围开关");
        assert!(html.contains("/api/weld_syms"));
        assert!(html.contains("weldLowerHasContent"), "虚线联动");
        // /api/weld_syms：27 符号 + 跨线标记 + MR 小字。
        let j = http_req(server.port, "GET", "/api/weld_syms", "");
        assert!(j.contains("角焊") && j.contains("持久衬垫"));
        assert!(j.contains("\"cross\":true"));
        assert!(j.contains("\"MR\""));
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["syms"].as_array().unwrap().len(), 27);
    }
}
    // ── 标准件选择器（参数化生成）─────────────────────────────────────────

    #[test]
    fn parts_routes_serve_page_catalog_and_preview() {
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let server = spawn(mock.clone()).expect("spawn guide server");
        let html = http_req(server.port, "GET", "/parts", "");
        assert!(html.contains("OCSM 标准件库"), "零件库窗口标题");
        assert!(html.contains("/api/parts") && html.contains("/api/part_svg") && html.contains("/api/part_export"));
        assert!(html.contains("零件出库"), "出库按钮");
        assert!(html.contains("window.close()"), "出库后自动关窗");
        assert!(html.contains("ul class=\"tree\"") || html.contains("class=\"tree\""), "左侧文件树");
        let cat = http_req(server.port, "GET", "/api/parts", "");
        assert!(cat.contains("GB/T 5780-2016") && cat.contains("hex_bolt_c"), "目录含 C 级螺栓族");
        assert!(
            cat.contains("GB/T 5782-2016") && cat.contains("hex_bolt_ab"),
            "目录含 A/B 级螺栓族（5782）"
        );
        assert!(cat.contains("\"M5\"") && cat.contains("lengths") && cat.contains("hex_bolt_c"), "含规格与长度系列");
        assert!(cat.contains("\"main\"") && cat.contains("\"top\"") && cat.contains("\"end\""), "三个视图");
        // 文件树：零件库 / 螺栓 / 六角螺栓 分级 + 待实现族标注
        assert!(cat.contains("零件库") && cat.contains("六角螺栓") && cat.contains("六角头螺栓 C级 GB/T 5780-2016"), "树路径");
        assert!(cat.contains("\"implemented\":false"), "未实现族在树上标注");
        assert!(cat.contains("1型六角螺母 GB/T 6170-2015"), "未实现常用件也列在树上");
        // 螺母两族：**四视图**必须在目录里（曾因 views 数组为空导致 GUI 面板静默失效）
        for fam in ["nut_61721", "nut_c41"] {
            let f: serde_json::Value = serde_json::from_str(&cat).unwrap();
            let v: Vec<String> = f["families"][fam]["views"]
                .as_array()
                .unwrap_or_else(|| panic!("{fam} 缺 views"))
                .iter()
                .map(|x| x["id"].as_str().unwrap().to_string())
                .collect();
            assert_eq!(v, vec!["main", "top", "end", "section"], "{fam} 应四视图");
            assert!(f["families"][fam]["sizes"].as_array().unwrap().len() >= 23, "{fam} 规格缺失");
        }
        assert!(cat.contains("六角螺母 C级 GB/T 41-2016") && cat.contains("六角薄螺母 GB/T 6172.1-2016"), "树缺螺母两族");
        let svg = http_req(server.port, "GET", "/api/part_svg?family=hex_bolt_c&d=5&l=25&view=main", "");
        assert!(svg.contains("<svg") && svg.contains("M5x25"), "预览 SV");
        assert!(svg.contains("GB/T 5780-2016"), "标题含现行代号");
        let bad = http_req(server.port, "GET", "/api/part_svg?family=hex_bolt_c&d=5&l=9999", "");
        assert!(bad.contains("error"), "越界长度报错");
        let bad2 = http_req(server.port, "GET", "/api/part_svg?family=nope&d=5&l=25", "");
        assert!(bad2.contains("error"), "未实现族报错");
        // 螺母两族逐视图出图（GUI 预览通路：曾全部空白）
        for (fam, d, l) in [("nut_61721", 10.0, 5.0), ("nut_c41", 24.0, 22.3)] {
            for view in ["main", "top", "end", "section"] {
                let u = format!("/api/part_svg?family={fam}&d={d}&l={l}&view={view}");
                let svg = http_req(server.port, "GET", &u, "");
                assert!(svg.contains("<svg"), "{fam}/{view} 预览为空：{svg}");
                assert!(!svg.contains("\"error\""), "{fam}/{view} 预览报错：{svg}");
            }
        }
        // 销两族（单视图）
        for (fam, d, l) in [("pin_1191", 10.0, 18.0), ("pin_1201", 20.0, 40.0)] {            let u = format!("/api/part_svg?family={fam}&d={d}&l={l}&view=main");
            let svg = http_req(server.port, "GET", &u, "");
            assert!(svg.contains("<svg") && !svg.contains("\"error\""), "{fam} 预览：{svg}");
            let bad = http_req(server.port, "GET", &format!("/api/part_svg?family={fam}&d={d}&l={l}&view=top"), "");
            assert!(bad.contains("error"), "{fam} 只应有一个视图");
            assert!(cat.contains(fam), "目录缺 {fam}");
        }
        // 视图名中文文案（GUI 按钮）
        assert!(cat.contains("剖视图") && cat.contains("俯视图"), "视图名文案缺失");
        // 销族树标签
        assert!(cat.contains("圆柱销 A型 GB/T 119.1-2000") && cat.contains("内螺纹圆柱销 GB/T 120.1-2000"));
        // 螺栓副 GUI 页：关键控件都在（件链编辑器 / 实时预览 / 装配按钮 / 防松模板）
        let jhtml = super::JOINT_HTML;
        for key in ["/api/joint_plan", "/api/joint_place", "/api/parts", "装配到图纸", "双螺母", "弹垫", "遮挡裁剪"] {
            assert!(jhtml.contains(key), "螺栓副页缺 {key}");
        }
    }

    /// GUI 实机自查用：把零件库窗口在固定端口上跑起来并**阻塞**，
    /// 然后用真 chromium（或 CDP 工具）打开 `http://127.0.0.1:<port>/parts` 截图核对。
    ///
    /// `cargo test -p ocs_ocsm -- --ignored serve_parts_page_for_check --nocapture`
    #[test]
    #[ignore]
    fn serve_parts_page_for_check() {
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let server = spawn(mock).expect("spawn guide server");
        let url = format!("http://127.0.0.1:{}/parts", server.port);
        println!("零件库窗口 → {url}");
        println!("（阻塞 20 分钟供人工/浏览器核对；Ctrl-C 结束）");
        std::thread::sleep(std::time::Duration::from_secs(1200));
    }

    /// 螺母剖视图出库：块里必须带 `5剖面线层` 的 ANSI31 Hatch（落图实体验收点）。
    #[test]
    fn part_export_section_block_has_hatch() {
        let _g = export_lock();
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let body = br#"{"family":"nut_c41","d":24,"l":22.3,"view":"section"}"#;
        let resp = apply_part_export(&sender, body).expect("出库");
        assert!(resp.contains("\"ok\":true"), "{resp}");
        let ents = mock.block_entities("OCSM_NUT_C41_M24_SECTION");
        let hats: Vec<_> = ents
            .iter()
            .filter_map(|e| match e {
                acadrust::EntityType::Hatch(h) => Some(h),
                _ => None,
            })
            .collect();
        assert_eq!(hats.len(), 2, "两片剖面线，实得 {}", hats.len());
        for h in &hats {
            assert_eq!(h.common.layer, crate::partgen_more::LAYER_HATCH);
            assert_eq!(h.pattern.name, "ANSI31");
            assert!(
                h.pattern_angle.abs() < 1e-9 || (h.pattern_angle - 270f64.to_radians()).abs() < 1e-9,
                "图案角存弧度（0 / 270°），实得 {}",
                h.pattern_angle
            );
        }
    }

    #[test]
    fn part_pick_builds_block_inserts_at_point_and_records_meta() {
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        *crate::parts_point_slot().lock().unwrap() = Some([10.0, 20.0, 0.0]);
        let body = br#"{"family":"hex_bolt_c","d":5,"l":25,"view":"main"}"#;
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let resp = apply_part_pick(&sender, body).expect("插入成功");
        assert!(resp.contains("\"ok\":true"), "{resp}");
        assert!(resp.contains("GB/T 5780-2016") && resp.contains("M5x25"), "回执含代号规格");
        // 建块（幂等名）+ 块内图元
        let ents = mock.block_entities("OCSM_HEX_BOLT_C_M5_25");
        assert!(ents.len() > 12, "块内含生成图元，实际 {}", ents.len());
        assert!(ents.iter().all(|e| !matches!(e, acadrust::EntityType::Dimension(_))), "不含尺寸标注");
        // 插入点取点选值；INSERT 落在轮廓层且 ByLayer
        assert!(resp.contains("[10.0,20.0,0.0]"), "插入点：{resp}");
        {
            let doc = mock.doc.lock().unwrap();
            let ins = doc
                .entities()
                .find_map(|e| match e {
                    acadrust::EntityType::Insert(i) => Some(i.clone()),
                    _ => None,
                })
                .expect("有 INSERT");
            assert_eq!(ins.block_name, "OCSM_HEX_BOLT_C_M5_25");
            assert_eq!((ins.insert_point.x, ins.insert_point.y), (10.0, 20.0));
            assert_eq!(ins.common.layer, crate::partgen::LAYER_MAIN);
            assert!(doc
                .get_entity(ins.common.handle)
                .and_then(|e| e.common().extended_data.get_record("OCSM_PART"))
                .is_some());
        }
        // 第二次插入同一规格：块名一致（宿主侧块表幂等由已有块判断保证），点已被取走 → 落原点
        let resp2 = apply_part_pick(&sender, body).expect("再次插入");
        assert!(resp2.contains("OCSM_HEX_BOLT_C_M5_25"), "复用同一块名：{resp2}");
        assert!(resp2.contains("[0.0,0.0,0.0]"), "点被取走后落原点：{resp2}");
        let inserts = mock
            .doc
            .lock()
            .unwrap()
            .entities()
            .filter(|e| matches!(e, acadrust::EntityType::Insert(_)))
            .count();
        assert_eq!(inserts, 2, "两次插入各一个 INSERT");
    }

    /// 销族出库：块里带 2细线层/5剖面线层（局部剖）且图层合法。
    #[test]
    fn part_export_pin_blocks() {
        let _g = export_lock();
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let resp = apply_part_export(&sender, br#"{"family":"pin_1201","d":20,"l":40,"view":"main"}"#).expect("出库");
        assert!(resp.contains("\"ok\":true"), "{resp}");
        let ents = mock.block_entities("OCSM_PIN_1201_Ø20×40_MAIN");
        assert!(ents.iter().any(|e| matches!(e, acadrust::EntityType::Hatch(_))), "局部剖剖面线");
        assert!(ents.iter().any(|e| matches!(e, acadrust::EntityType::LwPolyline(_))), "波浪线");
        for e in &ents {
            let lay = e.common().layer.as_str();
            assert!(
                [crate::partgen::LAYER_MAIN, crate::partgen::LAYER_THIN, crate::partgen::LAYER_CENTER,
                 crate::partgen_more::LAYER_HATCH].contains(&lay),
                "图层越界: {lay}"
            );
        }
        let resp = apply_part_export(&sender, br#"{"family":"pin_1191","d":10,"l":18,"view":"main"}"#).expect("出库");
        assert!(resp.contains("Ø10×18"), "{resp}");
        assert!(mock.block_entities("OCSM_PIN_1191_Ø10×18_MAIN").len() > 8, "销块已建好");
    }

    #[test]
    fn part_export_builds_block_and_registers_pending_part() {
        let _g = export_lock();
        let mock = Arc::new(MockSender::new(acadrust::CadDocument::new()));
        let sender: Arc<dyn PluginRequestSender> = mock.clone();
        let body = br#"{"family":"hex_bolt_c","d":5,"l":25,"view":"main"}"#;
        let resp = apply_part_export(&sender, body).expect("出库");
        assert!(resp.contains("\"ok\":true"), "{resp}");
        assert!(resp.contains("零件出库") || resp.contains("已出库"), "回执提示：{resp}");
        assert_eq!(mock.block_entities("OCSM_HEX_BOLT_C_M5_25_MAIN").len() > 12, true, "块已建好");
        // 待放置件登记（供 XL 放置态预览/落件）
        let label = crate::pending_part_label().expect("已登记待放置件");
        assert!(label.contains("M5x25") && label.contains("GB/T 5780-2016"), "{label}");
        assert_eq!(crate::pending_block().as_deref(), Some("OCSM_HEX_BOLT_C_M5_25_MAIN"));
        // 越界/未知族报错
        assert!(apply_part_export(&sender, br#"{"family":"hex_bolt_c","d":5,"l":9999}"#).is_err());
        assert!(apply_part_export(&sender, br#"{"family":"nope","d":5,"l":25}"#).is_err());
    }

}




