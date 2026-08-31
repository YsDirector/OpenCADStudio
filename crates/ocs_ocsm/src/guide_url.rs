//! 引导线 URL 协议（Guide-Line Protocol）。
//!
//! 引导线是 `10引导线层` 上的一条直线，两端点即标注的第一/第二点，其上挂
//! PE_URL 超链接承载标注参数。格式：
//!
//! ```text
//! http://127.0.0.1:<port>/DIM/<TYPE>/[<SUB>]/<dist>?text=<enc>&tol=<enc>&sym=<enc>&ver=<v>&let=<L>
//! TYPE: LINEAR | DIAMETER | RADIUS | DATUM | VIEW
//! SUB (LINEAR): H(水平) | V(竖直) | A(对齐)
//! dist: 与 P2 距离（尺寸线沿法向相对 P2 的偏移，正负定侧）
//! text: 标注文字（"<>" = 测量值，可自定义）；tol/sym 可空，URL-encoded
//! ver (DATUM): 1996 | 2008（GB/T 1182 版本，默认 2008）；let (DATUM): 基准字母（默认 A）
//! let (VIEW): 向视图字母（必填）；scale (VIEW): 比例文本（可缺省，如 1:1）；
//! flip (VIEW): 翻转方向（1 = 加旋转弧线箭头 → 样式3）；mx/my (VIEW): 标记放置点（缺省 0,0）
//! ```
//!
//! 端口为标注更新服务器端口；解析时忽略（本机单实例）。

/// 标注主类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideType {
    Linear,
    Diameter,
    Radius,
    Datum,
    View,
    Angle,
    Section,
}

impl GuideType {
    pub fn as_str(self) -> &'static str {
        match self {
            GuideType::Linear => "LINEAR",
            GuideType::Diameter => "DIAMETER",
            GuideType::Radius => "RADIUS",
            GuideType::Datum => "DATUM",
            GuideType::View => "VIEW",
            GuideType::Angle => "ANGLE",
            GuideType::Section => "SECTION",
        }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "LINEAR" => Some(GuideType::Linear),
            "DIAMETER" => Some(GuideType::Diameter),
            "RADIUS" => Some(GuideType::Radius),
            "DATUM" => Some(GuideType::Datum),
            "VIEW" => Some(GuideType::View),
            "ANGLE" => Some(GuideType::Angle),
            "SECTION" | "SEC" => Some(GuideType::Section),
            _ => None,
        }
    }
}

/// 线性标注的子类型（对齐方式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearSub {
    Horizontal,
    Vertical,
    Aligned,
}

impl LinearSub {
    pub fn as_str(self) -> &'static str {
        match self {
            LinearSub::Horizontal => "H",
            LinearSub::Vertical => "V",
            LinearSub::Aligned => "A",
        }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "H" => Some(LinearSub::Horizontal),
            "V" => Some(LinearSub::Vertical),
            "A" => Some(LinearSub::Aligned),
            _ => None,
        }
    }
    /// 显示名（GUI 用）。
#[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            LinearSub::Horizontal => "水平",
            LinearSub::Vertical => "竖直",
            LinearSub::Aligned => "对齐",
        }
    }
}

/// 基准标注标准版本（GB/T 1182）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumVersion {
    /// GB/T 1182-1996：圆 + 字母 + 短横线。
    GB1996,
    /// GB/T 1182-2008：方框 + 字母 + 实心三角箭头。
    GB2008,
}

impl DatumVersion {
    pub fn as_str(self) -> &'static str {
        match self {
            DatumVersion::GB1996 => "1996",
            DatumVersion::GB2008 => "2008",
        }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "1996" => Some(DatumVersion::GB1996),
            "2008" => Some(DatumVersion::GB2008),
            _ => None,
        }
    }
}

/// 向视图翻转方向（三态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlipDir {
    /// 无（默认）——不输出弧线箭头。
    #[default]
    None,
    /// 顺时针翻转——弧线箭头为逆时针的镜像（绕 Y 轴，x→−x）。
    Clockwise,
    /// 逆时针翻转——弧线箭头（对照 OCSMDIMGULIDE5-symbol.dxf 样式3）。
    CounterClockwise,
}

impl FlipDir {
    pub fn as_str(self) -> &'static str {
        match self {
            FlipDir::None => "none",
            FlipDir::Clockwise => "cw",
            FlipDir::CounterClockwise => "ccw",
        }
    }
    fn from_url(v: &str) -> Option<Self> {
        match v {
            "" | "0" | "false" | "no" | "none" | "off" => Some(FlipDir::None),
            "1" | "true" | "yes" | "ccw" | "counter" | "counterclockwise" | "逆" => {
                Some(FlipDir::CounterClockwise)
            }
            "cw" | "clock" | "clockwise" | "顺" => Some(FlipDir::Clockwise),
            _ => None,
        }
    }
}

/// 剖切符号的视向箭头侧（相对剖切路径前进方向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectionSide {
    /// 默认右侧：箭头朝剖切路径前进方向逆时针旋转 90°。
    #[default]
    Right,
    /// 左侧：顺时针旋转 90°。
    Left,
}

impl SectionSide {
    pub fn as_str(self) -> &'static str {
        match self {
            SectionSide::Right => "R",
            SectionSide::Left => "L",
        }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "R" | "RIGHT" | "右" => Some(SectionSide::Right),
            "L" | "LEFT" | "左" => Some(SectionSide::Left),
            _ => None,
        }
    }
    /// 沿剖切路径前进方向 τ 的箭头侧单位向量。
    pub fn dir(self, tau: (f64, f64)) -> (f64, f64) {
        match self {
            // 右侧 = 逆时针旋转 90°；左侧 = 顺时针旋转 90°。
            SectionSide::Right => (-tau.1, tau.0),
            SectionSide::Left => (tau.1, -tau.0),
        }
    }
    /// 显示名（GUI 用）。
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            SectionSide::Right => "向右",
            SectionSide::Left => "向左",
        }
    }
}

/// 角度标注的角模式（两段 PLINE 引导线，顶点即转折点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AngleMode {
    /// 劣角（锐角，默认）。
    #[default]
    Minor,
    /// 优角（360°−劣角）。
    Reflex,
}

impl AngleMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AngleMode::Minor => "M",
            AngleMode::Reflex => "R",
        }
    }
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "M" | "MINOR" | "劣" => Some(AngleMode::Minor),
            "R" | "REFLEX" | "REF" | "优" => Some(AngleMode::Reflex),
            _ => None,
        }
    }
    /// 显示名（GUI 用）。
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            AngleMode::Minor => "劣角",
            AngleMode::Reflex => "优角",
        }
    }
}

/// 解析后的标注参数。
#[derive(Debug, Clone, PartialEq)]
pub struct GuideParams {
    pub guide_type: GuideType,
    /// 仅 LINEAR 有子类型。
    pub sub: Option<LinearSub>,
    /// 与 P2 距离（法向偏移）。
    pub dist: f64,
    /// 标注文字（`<>` 或 None = 测量值）。
    pub text: Option<String>,
    /// 公差（可空）。
    pub tol: Option<String>,
    /// 上偏差（公差堆叠 `\S{up}^{dn}`，对照公差.dxf 29C）。
    pub up: Option<String>,
    /// 下偏差（公差堆叠）。
    pub dn: Option<String>,
    /// ISO 286 配合代号（如 "H7" / "g6" / "H7/g6"）：服务端结合测量值算极限偏差。
    pub fit: Option<String>,
    /// 符号（可空）。
    pub sym: Option<String>,
    /// 测量值小数位数（None = 默认 2；渲染时消零：12.70→12.7、12.00→12）。
    pub dec: Option<u32>,
    /// 基准标注标准版本（仅 DATUM；默认 2008）。
    pub ver: DatumVersion,
    /// 基准字母（仅 DATUM；默认 "A"）。
    pub letter: Option<String>,
    /// 向视图比例文本（仅 VIEW；可缺省，如 "1:1"）。
    pub scale: Option<String>,
    /// 向视图翻转方向（仅 VIEW；None=无，Clockwise=顺时针，CounterClockwise=逆时针）。
    pub flip: FlipDir,
    /// 向视图标记放置点（仅 VIEW；缺省 (0,0)，字母中心锚定）。
    pub marker: Option<(f64, f64)>,
    /// 角度标注角模式（仅 ANGLE；默认劣角 Minor）。
    pub angle_mode: AngleMode,
    /// 剖切符号视向箭头侧（仅 SECTION；默认 Right）。
    pub section_side: SectionSide,
    /// 剖切符号是否显示视向箭头（仅 SECTION；默认 true）。
    pub show_arrow: bool,
}

impl GuideParams {
    /// 构造一个线性标注参数。
#[allow(dead_code)]
    pub fn linear(sub: LinearSub, dist: f64) -> Self {
        GuideParams {
            guide_type: GuideType::Linear,
            sub: Some(sub),
            dist,
            text: None,
            tol: None,
            up: None,
            dn: None,
            fit: None,
            sym: None,
            dec: None,
            ver: DatumVersion::GB2008,
            letter: None,
            scale: None,
            flip: FlipDir::None,
            marker: None,
            angle_mode: AngleMode::Minor,
            section_side: SectionSide::Right,
            show_arrow: true,
        }
    }

    /// 解析引导线 URL。失败返回 None。
    pub fn from_url(url: &str) -> Option<GuideParams> {
        // 去掉 scheme://authority，留下 path?query
        let rest = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))?;
        let path = rest.split_once('/').map(|(_, p)| p).unwrap_or(rest);
        let (path, query) = path.split_once('?').map_or((path, ""), |(p, q)| (p, q));
        let mut segs = path.split('/').filter(|s| !s.is_empty());
        let cmd = segs.next()?.to_ascii_uppercase();
        if cmd != "DIM" {
            return None;
        }
        let type_raw = segs.next()?;
        let guide_type = GuideType::from_str(type_raw)?;

        // 剩余段：线性 = [SUB, dist]；角度 = [mode, dist]（mode 可缺省 → 劣角）。
        let sub = if guide_type == GuideType::Linear {
            Some(LinearSub::from_str(segs.next()?)?)
        } else {
            None
        };
        let mut angle_mode = AngleMode::Minor;
        let dist: f64 = if guide_type == GuideType::Angle {
            // 第 3 段可能是角模式（M/C/R），也可能是直接 dist（缺省劣角）。
            let s3 = segs.next()?;
            if let Ok(d) = s3.parse::<f64>() {
                d
            } else {
                angle_mode = AngleMode::from_str(s3)?;
                segs.next()?.parse().ok()?
            }
        } else {
            segs.next()?.parse().ok()?
        };

        // query 参数（text/tol/up/dn/sym）。
        let mut text = None;
        let mut tol = None;
        let mut up: Option<String> = None;
        let mut dn: Option<String> = None;
        let mut sym = None;
        let mut fit: Option<String> = None;
        let mut dec = None;
        let mut ver = DatumVersion::GB2008;
        let mut letter = None;
        let mut scale = None;
        let mut flip = FlipDir::None;
        let mut marker = None;
        let mut section_side = SectionSide::Right;
        let mut show_arrow = true;
        for pair in query.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let v = percent_decode(v);
            match k {
                "text" => text = Some(v),
                "tol" => tol = Some(v),
                "up" => up = Some(v),
                "dn" => dn = Some(v),
                "up" => up = Some(v),
                "dn" => dn = Some(v),
                "fit" => fit = Some(v),
                "sym" => sym = Some(v),
                "dec" => dec = v.parse::<u32>().ok().map(|d| d.min(8)),
                "ver" => ver = DatumVersion::from_str(&v).unwrap_or(DatumVersion::GB2008),
                "let" => letter = Some(v),
                "scale" => scale = (!v.is_empty()).then_some(v),
                "flip" => flip = FlipDir::from_url(&v).unwrap_or(FlipDir::None),
                "mx" | "my" => {
                    let (mut mx, mut my) = marker.unwrap_or((0.0, 0.0));
                    if k == "mx" {
                        mx = v.parse().ok()?;
                    } else {
                        my = v.parse().ok()?;
                    }
                    marker = Some((mx, my));
                }
                "side" => section_side = SectionSide::from_str(&v).unwrap_or(SectionSide::Right),
                "show" => show_arrow = !matches!(&*v, "0" | "false" | "no" | "off" | "隐藏"),
                _ => {}
            }
        }

        Some(GuideParams {
            guide_type,
            sub,
            dist,
            text,
            tol,
            up,
            dn,
            fit,
            sym,
            dec,
            ver,
            letter,
            scale,
            flip,
            marker,
            angle_mode,
            section_side,
            show_arrow,
        })
    }

    /// 生成引导线 URL。`port` 为标注更新服务器端口。
#[allow(dead_code)]
    pub fn to_url(&self, port: u16) -> String {
        let mut url = format!("http://127.0.0.1:{port}/DIM/{}", self.guide_type.as_str());
        // 仅 LINEAR 带子类型段；ANGLE 非劣角带角模式段；其它类型（含 VIEW）无 sub。
        if self.guide_type == GuideType::Linear {
            if let Some(sub) = self.sub {
                url.push('/');
                url.push_str(sub.as_str());
            }
        } else if self.guide_type == GuideType::Angle && self.angle_mode != AngleMode::Minor {
            url.push('/');
            url.push_str(self.angle_mode.as_str());
        }
        url.push('/');
        url.push_str(&format_dist(self.dist));
        let mut q = Vec::new();
        if let Some(t) = &self.text {
            if !t.is_empty() {
                q.push(format!("text={}", percent_encode(t)));
            }
        }
        if let Some(t) = &self.tol {
            if !t.is_empty() {
                q.push(format!("tol={}", percent_encode(t)));
            }
        }
        if let Some(u) = &self.up {
            if !u.is_empty() {
                q.push(format!("up={}", percent_encode(u)));
            }
        }
        if let Some(d) = &self.dn {
            if !d.is_empty() {
                q.push(format!("dn={}", percent_encode(d)));
            }
        }
        if let Some(f) = &self.fit {
            if !f.is_empty() {
                q.push(format!("fit={}", percent_encode(f)));
            }
        }
        if let Some(s) = &self.sym {
            if !s.is_empty() {
                q.push(format!("sym={}", percent_encode(s)));
            }
        }
        if let Some(d) = self.dec {
            q.push(format!("dec={d}"));
        }
        if let Some(l) = &self.letter {
            q.push(format!("let={}", percent_encode(l)));
        }
        // 基准版本：默认 2008 不输出（简洁），非默认才带。
        if self.ver != DatumVersion::GB2008 {
            q.push(format!("ver={}", self.ver.as_str()));
        }
        // 向视图：比例 / 翻转方向 / 标记放置点。
        if let Some(s) = &self.scale {
            if !s.is_empty() {
                q.push(format!("scale={}", percent_encode(s)));
            }
        }
        if self.flip != FlipDir::None {
            q.push(format!("flip={}", self.flip.as_str()));
        }
        if let Some((mx, my)) = self.marker {
            q.push(format!("mx={mx}"));
            q.push(format!("my={my}"));
        }
        // 剖切符号：视向箭头侧 / 是否显示箭头。
        if self.section_side != SectionSide::Right {
            q.push(format!("side={}", self.section_side.as_str()));
        }
        if !self.show_arrow {
            q.push("show=0".to_string());
        }
        if !q.is_empty() {
            url.push('?');
            url.push_str(&q.join("&"));
        }
        url
    }
}

/// 距离格式化：去掉多余小数（-50 / -14.2 / 10）。
#[allow(dead_code)]
fn format_dist(d: f64) -> String {
    if d.fract() == 0.0 && d.abs() < 1e12 {
        format!("{}", d as i64)
    } else {
        format!("{d}")
    }
}

/// 极简 percent-decode（%XX 与保留字符直通）。
pub(crate) fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex(bytes[i + 1]);
            let lo = hex(bytes[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 极简 percent-encode（仅编码非 ASCII 与保留字符；中文等走 UTF-8 字节）。
#[allow(dead_code)]
fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        let keep = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'<' | b'>');
        if keep {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_linear_horizontal() {
        let p = GuideParams::from_url("http://127.0.0.1:23751/DIM/LINEAR/H/-50").unwrap();
        assert_eq!(p.guide_type, GuideType::Linear);
        assert_eq!(p.sub, Some(LinearSub::Horizontal));
        assert_eq!(p.dist, -50.0);
        assert_eq!(p.text, None);
    }

    #[test]
    fn parse_linear_vertical_with_text() {
        let p = GuideParams::from_url(
            "http://127.0.0.1:23751/DIM/LINEAR/V/-14.2?text=%3C%3E&tol=%2B0.1",
        )
        .unwrap();
        assert_eq!(p.sub, Some(LinearSub::Vertical));
        assert_eq!(p.dist, -14.2);
        assert_eq!(p.text.as_deref(), Some("<>"));
        assert_eq!(p.tol.as_deref(), Some("+0.1"));
    }

    #[test]
    fn parse_aligned_and_decimal_dist() {
        let p = GuideParams::from_url("http://127.0.0.1:9/DIM/LINEAR/A/10.0").unwrap();
        assert_eq!(p.sub, Some(LinearSub::Aligned));
        assert_eq!(p.dist, 10.0);
    }

    #[test]
    fn parse_other_types() {
        assert_eq!(
            GuideParams::from_url("http://x/DIM/RADIUS/8.0").unwrap().guide_type,
            GuideType::Radius
        );
        assert_eq!(
            GuideParams::from_url("http://x/DIM/DATUM/0").unwrap().guide_type,
            GuideType::Datum
        );
    }

    #[test]
    fn parse_section_side_arrow() {
        use crate::guide_url::SectionSide as SS;
        // 默认：右侧 + 显示箭头。
        let p = GuideParams::from_url("http://x/DIM/SECTION/0").unwrap();
        assert_eq!(p.guide_type, GuideType::Section);
        assert_eq!(p.section_side, SS::Right);
        assert!(p.show_arrow);
        // 左侧 + 隐藏箭头 + 字母 + 比例 + 放置点。
        let p = GuideParams::from_url(
            "http://x/DIM/SECTION/0?side=L&show=0&let=B&scale=1%3A1&mx=10&my=-5",
        )
        .unwrap();
        assert_eq!(p.section_side, SS::Left);
        assert!(!p.show_arrow);
        assert_eq!(p.letter.as_deref(), Some("B"));
        assert_eq!(p.scale.as_deref(), Some("1:1"));
        assert_eq!(p.marker, Some((10.0, -5.0)));
        // SEC 别名。
        assert_eq!(
            GuideParams::from_url("http://x/DIM/SEC/0").unwrap().guide_type,
            GuideType::Section
        );
        // to_url 往返。
        let mut p2 = GuideParams::linear(LinearSub::Aligned, -14.2);
        p2.guide_type = GuideType::Section;
        p2.letter = Some("C".into());
        p2.scale = Some("1:2".into());
        p2.marker = Some((12.0, 34.0));
        let back = GuideParams::from_url(&p2.to_url(1)).unwrap();
        assert_eq!(back.guide_type, GuideType::Section);
        assert_eq!(back.letter.as_deref(), Some("C"));
        assert_eq!(back.scale.as_deref(), Some("1:2"));
        assert_eq!(back.marker, Some((12.0, 34.0)));
        // 左侧往返（side=L）。
        p2.section_side = SS::Left;
        p2.show_arrow = false;
        let back2 = GuideParams::from_url(&p2.to_url(1)).unwrap();
        assert_eq!(back2.section_side, SS::Left);
        assert!(!back2.show_arrow);
        // 默认往返（右侧+显箭）。
        let mut p3 = GuideParams::linear(LinearSub::Aligned, -14.2);
        p3.guide_type = GuideType::Section;
        let back3 = GuideParams::from_url(&p3.to_url(1)).unwrap();
        assert_eq!(back3.section_side, SS::Right);
        assert!(back3.show_arrow);
    }

    #[test]
    fn reject_non_dim_or_malformed() {
        assert!(GuideParams::from_url("http://x/FOO/LINEAR/H/1").is_none());
        assert!(GuideParams::from_url("http://x/DIM/LINEAR/H/abc").is_none());
        assert!(GuideParams::from_url("http://x/DIM/LINEAR").is_none());
        assert!(GuideParams::from_url("not-a-url").is_none());
    }

    #[test]
    fn roundtrip() {
        let p = GuideParams::linear(LinearSub::Vertical, -14.2);
        let url = p.to_url(23751);
        let back = GuideParams::from_url(&url).unwrap();
        assert_eq!(back, p);
        // 中文文字（公差/文字 percent-encode 往返）。
        let p2 = GuideParams {
            text: Some("50±0.05".into()),
            tol: Some("+0.1".into()),
            sym: Some("×2".into()),
            ..p
        };
        let url2 = p2.to_url(23751);
        let back2 = GuideParams::from_url(&url2).unwrap();
        assert_eq!(back2, p2);
    }

    #[test]
    fn dist_formatting() {
        let p = GuideParams::linear(LinearSub::Aligned, -14.2);
        assert!(p.to_url(1).ends_with("/DIM/LINEAR/A/-14.2"));
        let p2 = GuideParams::linear(LinearSub::Aligned, 50.0);
        assert!(p2.to_url(1).ends_with("/DIM/LINEAR/A/50"));
    }

    #[test]
    fn parse_dec_query() {
        // dec 参数解析；无 dec = None（生成端默认 2）。
        let p = GuideParams::from_url("http://x/DIM/DIAMETER/-9?dec=3").unwrap();
        assert_eq!(p.dec, Some(3));
        let p = GuideParams::from_url("http://x/DIM/DIAMETER/-9").unwrap();
        assert_eq!(p.dec, None);
        // 非法/越界：>8 截到 8；非数字 = None。
        let p = GuideParams::from_url("http://x/DIM/DIAMETER/-9?dec=99").unwrap();
        assert_eq!(p.dec, Some(8));
        let p = GuideParams::from_url("http://x/DIM/DIAMETER/-9?dec=abc").unwrap();
        assert_eq!(p.dec, None);
        // to_url 往返。
        let mut p = GuideParams::linear(LinearSub::Aligned, -14.2);
        p.dec = Some(2);
        let back = GuideParams::from_url(&p.to_url(1)).unwrap();
        assert_eq!(back.dec, Some(2));
    }

    #[test]
    fn parse_datum_ver_letter() {
        use crate::guide_url::DatumVersion as DV;
        // 默认 2008 / A。
        let p = GuideParams::from_url("http://x/DIM/DATUM/0").unwrap();
        assert_eq!(p.ver, DV::GB2008);
        assert_eq!(p.letter, None);
        // 1996 + 字母 B。
        let p = GuideParams::from_url("http://x/DIM/DATUM/0?ver=1996&let=B").unwrap();
        assert_eq!(p.ver, DV::GB1996);
        assert_eq!(p.letter.as_deref(), Some("B"));
        // 非法 ver 回退 2008。
        let p = GuideParams::from_url("http://x/DIM/DATUM/0?ver=2015").unwrap();
        assert_eq!(p.ver, DV::GB2008);
        // to_url 往返。
        let mut p = GuideParams::linear(LinearSub::Aligned, -14.2);
        p.ver = DV::GB1996;
        p.letter = Some("C".into());
        let back = GuideParams::from_url(&p.to_url(1)).unwrap();
        assert_eq!(back.ver, DV::GB1996);
        assert_eq!(back.letter.as_deref(), Some("C"));
    }

    #[test]
    fn parse_view_scale_flip_marker() {
        // 全部参数：字母 B + 比例 1:1 + 翻转 + 标记放置点(10,-5)。
        let p =
            GuideParams::from_url("http://x/DIM/VIEW/0?let=B&scale=1%3A1&flip=1&mx=10&my=-5")
                .unwrap();
        assert_eq!(p.guide_type, GuideType::View);
        assert_eq!(p.letter.as_deref(), Some("B"));
        assert_eq!(p.scale.as_deref(), Some("1:1"));
        assert_eq!(p.flip, FlipDir::CounterClockwise);
        assert_eq!(p.marker, Some((10.0, -5.0)));
        // 顺时针翻转：flip=cw → Clockwise；flip=ccw → CounterClockwise。
        let p = GuideParams::from_url("http://x/DIM/VIEW/0?flip=cw").unwrap();
        assert_eq!(p.flip, FlipDir::Clockwise);
        let p = GuideParams::from_url("http://x/DIM/VIEW/0?flip=ccw").unwrap();
        assert_eq!(p.flip, FlipDir::CounterClockwise);
        // 旧值 flip=1 向后兼容 → 逆时针。
        let p = GuideParams::from_url("http://x/DIM/VIEW/0?flip=1").unwrap();
        assert_eq!(p.flip, FlipDir::CounterClockwise);
        // 缺省：字母 A、无比例、不翻转、标记 (0,0)。
        let p = GuideParams::from_url("http://x/DIM/VIEW/0").unwrap();
        assert_eq!(p.letter, None);
        assert_eq!(p.scale, None);
        assert_eq!(p.flip, FlipDir::None);
        assert_eq!(p.marker, None);
        // 部分坐标（只给 mx）。
        let p = GuideParams::from_url("http://x/DIM/VIEW/0?mx=3.5").unwrap();
        assert_eq!(p.marker, Some((3.5, 0.0)));
        // 非法坐标 → None。
        assert!(GuideParams::from_url("http://x/DIM/VIEW/0?mx=abc").is_none());
        // to_url 往返。
        let mut p = GuideParams::linear(LinearSub::Aligned, -14.2);
        p.guide_type = GuideType::View;
        p.letter = Some("A".into());
        p.scale = Some("1:2".into());
        p.flip = FlipDir::CounterClockwise;
        p.marker = Some((12.0, 34.0));
        let back = GuideParams::from_url(&p.to_url(1)).unwrap();
        assert_eq!(back.letter.as_deref(), Some("A"));
        assert_eq!(back.scale.as_deref(), Some("1:2"));
        assert_eq!(back.flip, FlipDir::CounterClockwise);
        assert_eq!(back.marker, Some((12.0, 34.0)));
        // 顺时针 to_url 往返（flip=cw）。
        p.flip = FlipDir::Clockwise;
        let back = GuideParams::from_url(&p.to_url(1)).unwrap();
        assert_eq!(back.flip, FlipDir::Clockwise);
    }
}
