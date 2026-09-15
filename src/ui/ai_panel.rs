//! Built-in **AI assistant** panel: a native chat view over the local `pi-web`
//! HTTP API.
//!
//! Everything here is plain iced — same header chrome (pin / close / drag),
//! same dock behaviour, same theme and DPI handling as the Properties panel, and
//! it renders natively on both X11 and Wayland. No child windows, no external
//! processes, nothing to align.
//!
//! Data source (all local, plain HTTP + JSON):
//!   GET  {endpoint}/api/sessions                 session list
//!   GET  {endpoint}/api/agent/<id>/events        SSE transcript/tool stream
//!   POST {endpoint}/api/agent/<id> {"prompt":…}  send a message
//!
//! The client lives on a worker thread (see `crate::ai`); this module owns the
//! state that thread feeds and turns it into widgets.

use iced::widget::{button, container, mouse_area, row, text, tooltip, Space};
use iced::{Background, Element, Length, Theme};

use crate::app::Message;

/// One rendered transcript entry.
#[derive(Debug, Clone, PartialEq)]
pub enum AiEntry {
    /// A message the user sent.
    User(String),
    /// Assistant text (grown in place while streaming).
    Assistant(String),
    /// A tool call: name, streamed/typed output, finished flag.
    Tool { name: String, output: String, done: bool },
    /// Host-side notice (connection problems, session switches, …).
    Notice(String),
}

/// Connection status shown in the panel header.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum AiStatus {
    /// Not started yet — no worker running.
    #[default]
    Idle,
    Connecting,
    Ready {
        session: String,
        streaming: bool,
    },
    Error(String),
}

/// State for one document tab's AI panel.
pub struct AiPanelState {
    /// Base URL of the local `pi-web` server (no trailing slash).
    pub endpoint: String,
    pub status: AiStatus,
    /// Session list (`/api/sessions`), newest activity first.
    pub sessions: Vec<(String, String)>,
    pub active: Option<String>,
    pub entries: Vec<AiEntry>,
    /// Composer text.
    pub input: String,
    /// Worker handles (None while stopped).
    pub worker: Option<crate::ai::AiHandle>,
}

impl Default for AiPanelState {
    fn default() -> Self {
        Self {
            endpoint: std::env::var("OCS_PI_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:30141".to_string()),
            status: AiStatus::Idle,
            sessions: Vec::new(),
            active: None,
            entries: Vec::new(),
            input: String::new(),
            worker: None,
        }
    }
}

impl AiPanelState {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Human-readable status for the header/subtitle.
    pub fn status_label(&self) -> String {
        match &self.status {
            AiStatus::Idle => "AI 助手：未连接（命令 AI 打开）".to_string(),
            AiStatus::Connecting => "正在连接 pi-web…".to_string(),
            AiStatus::Ready { session, streaming } => {
                let dot = if *streaming { "● 生成中" } else { "○ 空闲" };
                format!("{dot} · {session}")
            }
            AiStatus::Error(e) => format!("连接失败：{e}"),
        }
    }

    pub fn view(&self, width: f32, auto_collapse: bool) -> Element<'_, Message> {
        crate::ui::ai_panel::view(self, width, auto_collapse)
    }
}

/// Panel chrome: title + pin + close, exactly like the Properties panel so the
/// dock drag/resize/collapse affordances all behave identically.
fn header(auto_collapse: bool) -> Element<'static, Message> {
    use crate::ui::dock::{DockMsg, PanelId};
    let pin_icon = if auto_collapse {
        crate::ui::icons::themed_primary_weak_text(crate::ui::icons::PIN, 12.0)
    } else {
        crate::ui::icons::themed_secondary(crate::ui::icons::PIN, 12.0)
    };
    let pin = button(pin_icon)
        .on_press(Message::Dock(DockMsg::AutoCollapseToggle(PanelId::Ai)))
        .style(move |theme: &Theme, status| {
            let mut style = button::subtle(theme, status);
            if auto_collapse {
                let palette = theme.palette();
                style.background = Some(Background::Color(palette.primary.weak.color));
                style.text_color = palette.primary.weak.text;
                style.border.color = palette.primary.base.color;
                style.border.width = 1.0;
            }
            style
        })
        .padding([3, 5]);
    let pin = tooltip(pin, text("Auto").size(10), tooltip::Position::Bottom).gap(4);

    let close = button(crate::ui::icons::themed_secondary(
        crate::ui::icons::CLOSE,
        12.0,
    ))
    .on_press(Message::Dock(DockMsg::Close(PanelId::Ai)))
    .style(button::subtle)
    .padding([3, 5]);
    let close = tooltip(close, text("Close").size(10), tooltip::Position::Bottom).gap(4);

    mouse_area(
        container(
            row![
                text("AI 助手").size(12),
                Space::new().width(Length::Fill),
                pin,
                close,
            ]
            .spacing(3)
            .align_y(iced::Center),
        )
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().background.weak.color)),
            ..Default::default()
        })
        .width(Length::Fill)
        .padding([3, 6]),
    )
    .on_press(Message::Dock(DockMsg::DockGrab(PanelId::Ai)))
    .interaction(iced::mouse::Interaction::Grab)
    .into()
}

/// Panel body. Phase 1 shows the connection state; the transcript + composer
/// land with the client (`crate::ai`).
pub fn view(state: &AiPanelState, width: f32, auto_collapse: bool) -> Element<'_, Message> {
    use iced::widget::column;
    let body = column![
        text(state.status_label()).size(12),
        text(format!("接口：{}", state.endpoint)).size(10),
    ]
    .spacing(6)
    .padding([8, 8]);

    // **`Fixed(width)` is load-bearing**: the dock hands every expanded panel
    // the column width it computed, and a `Fill`-width panel instead competes
    // with the drawing canvas for the same row space (which made the panel take
    // half the window). Every built-in panel pins its width this way.
    column![header(auto_collapse), body]
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .into()
}
