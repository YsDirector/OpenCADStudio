//! **OCS Pi Extension**: the built-in Pi assistant panel — a native chat view
//! over the local `pi-web` HTTP API.
//!
//! Everything here is plain iced — same header chrome (pin / close / drag),
//! same dock behaviour, same theme and DPI handling as the Properties panel,
//! and it renders natively on both X11 and Wayland. No child windows, no
//! external processes, nothing to align.
//!
//! Data source (all local, plain HTTP + JSON, see `crate::pi`):
//!   GET  {endpoint}/api/sessions                 session list
//!   GET  {endpoint}/api/agent/<id>/events        SSE transcript/tool stream
//!   POST {endpoint}/api/agent/<id> {"type":"prompt",…}  send a message
//!
//! The client lives on a worker thread; `PiPanelState::apply` folds its events
//! into render state. This module owns the state and the widgets.

use std::collections::{HashSet, VecDeque};

use iced::widget::{
    button, column, container, mouse_area, pick_list, row, scrollable, text, text_editor, tooltip,
    Space,
};
use iced::{Background, Border, Color, Element, Font, Length, Theme};

use crate::app::Message;
use crate::pi::{self, Entry, Event, Part, Status};

/// Scrollable widget id of the transcript (for scroll-to-bottom).
pub const TRANSCRIPT_ID: &str = "pi_transcript_scroll";
/// Fixed composer height in px.
const COMPOSER_H: f32 = 72.0;
/// Live entry ids start above this base so they never collide with the
/// content-hashed ids of backfilled (`Replace`) entries.
const LIVE_ID_BASE: u64 = 1 << 62;

// ── Sub-messages (routed through `Message::Pi`) ─────────────────────────────

#[derive(Debug, Clone)]
pub enum PiMsg {
    /// 10 Hz drain of the worker channel.
    Poll,
    /// The composer editor processed an action.
    Editor(text_editor::Action),
    /// Send the composer text (Enter or the send button).
    Send,
    /// Switch the followed session.
    SessionPick(String),
    /// Expand/collapse a transcript entry (thinking/tool/…).
    ToggleEntry(u64),
    /// Connect (or drop and reconnect) the worker.
    Reconnect,
}

// ── View model ──────────────────────────────────────────────────────────────

/// One rendered transcript entry.
#[derive(Debug, Clone, PartialEq)]
pub enum PiEntryKind {
    User(String),
    Assistant(String),
    Thinking(String),
    Tool { id: String, name: String, output: String, done: bool, is_error: bool },
    Notice(String),
}

/// A transcript entry plus per-entry UI state (collapsed/expanded).
#[derive(Debug, Clone)]
pub struct PiEntry {
    pub id: u64,
    pub kind: PiEntryKind,
    pub expanded: bool,
}

/// Blocks of the assistant message currently streaming in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreamingMsg {
    pub blocks: Vec<StreamBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamBlock {
    Thinking { idx: usize, text: String },
    Text { idx: usize, text: String },
}

/// Connection status shown in the status strip.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PiStatus {
    /// Not started yet — no worker running.
    #[default]
    Idle,
    Connecting,
    Ready { session: String, streaming: bool },
    Error(String),
}

/// A badge next to a tool row: (label, palette swatch to take the color from).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Badge {
    Running,
    Done,
    Failed,
}

impl Badge {
    fn label(self) -> &'static str {
        match self {
            Badge::Running => "运行中…",
            Badge::Done => "完成",
            Badge::Failed => "出错",
        }
    }

    fn color(self, theme: &Theme) -> Color {
        let palette = theme.palette();
        match self {
            Badge::Running => palette.warning.base.color,
            Badge::Done => palette.success.base.color,
            Badge::Failed => palette.danger.base.color,
        }
    }
}

/// State for one document tab's Pi panel.
pub struct PiPanelState {
    /// Base URL of the local `pi-web` server (no trailing slash).
    pub endpoint: String,
    pub status: PiStatus,
    /// Session list (`/api/sessions`), newest activity first.
    pub sessions: Vec<pi::SessionInfo>,
    /// The session being followed (default: the newest-active one).
    pub active: Option<String>,
    pub entries: Vec<PiEntry>,
    pub streaming: StreamingMsg,
    /// Composer text.
    pub input: text_editor::Content,
    /// Echo tickets: texts optimistically inserted, waiting for the SSE echo
    /// (`message_end` role=user) to be consumed. FIFO — queued follow-ups
    /// deliver in send order.
    pub pending_echoes: VecDeque<String>,
    /// Server-side follow-up queue length (`queue_update`), if known.
    pub queued_followups: Option<usize>,
    /// Expand flags for *live* blocks (streaming thinking), keyed by block id
    /// — they don't exist as entries yet.
    live_expanded: HashSet<u64>,
    /// Monotonic id source for live entries.
    next_id: u64,
    /// Worker handle (None while stopped).
    pub worker: Option<crate::pi::PiHandle>,
}

impl Default for PiPanelState {
    fn default() -> Self {
        Self {
            endpoint: std::env::var("OCS_PI_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:30141".to_string()),
            status: PiStatus::Idle,
            sessions: Vec::new(),
            active: None,
            entries: Vec::new(),
            streaming: StreamingMsg::default(),
            input: text_editor::Content::new(),
            pending_echoes: VecDeque::new(),
            queued_followups: None,
            live_expanded: HashSet::new(),
            next_id: 0,
            worker: None,
        }
    }
}

impl PiPanelState {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Start the worker if none is running.
    pub fn ensure_worker(&mut self) {
        if self.worker.is_none() {
            self.worker = Some(crate::pi::PiHandle::start(&self.endpoint));
        }
    }

    /// Stop the worker (panel closed).
    pub fn stop_worker(&mut self) {
        if let Some(w) = &self.worker {
            w.stop();
        }
        self.worker = None;
    }

    pub fn send_command(&self, cmd: pi::Command) {
        if let Some(w) = &self.worker {
            let _ = w.tx.send(cmd);
        }
    }

    /// Render the panel (delegates to the free `view`).
    pub fn view(&self, width: f32, auto_collapse: bool) -> Element<'_, Message> {
        view(self, width, auto_collapse)
    }

    /// Whether an agent run is currently streaming.
    pub fn is_streaming(&self) -> bool {
        matches!(&self.status, PiStatus::Ready { streaming: true, .. })
    }

    /// Human-readable status for the status strip.
    pub fn status_label(&self) -> String {
        match &self.status {
            PiStatus::Idle => "未连接".to_string(),
            PiStatus::Connecting => "正在连接 pi-web…".to_string(),
            PiStatus::Ready { session, streaming } => {
                let dot = if *streaming { "● 生成中" } else { "○ 空闲" };
                let short: String = session.chars().take(8).collect();
                format!("{dot} · 会话 {short}")
            }
            PiStatus::Error(e) => format!("连接失败：{e}"),
        }
    }

    /// Fold one worker event into render state. Returns whether transcript
    /// content changed (→ the caller may scroll to the bottom).
    pub fn apply(&mut self, ev: Event) -> bool {
        match ev {
            Event::Status(s) => {
                self.status = match &s {
                    Status::Connecting => PiStatus::Connecting,
                    Status::Ready { session, streaming } => {
                        self.active = Some(session.clone());
                        PiStatus::Ready {
                            session: session.clone(),
                            streaming: *streaming,
                        }
                    }
                    Status::Error(e) => PiStatus::Error(e.clone()),
                };
                false
            }
            Event::Sessions(list) => {
                self.sessions = list;
                // Default: follow the newest-active session until the user
                // picks one explicitly.
                if self.active.is_none() {
                    self.active = self.sessions.first().map(|s| s.id.clone());
                }
                false
            }
            Event::Replace(entries) => {
                self.entries = entries.into_iter().map(stable_entry).collect();
                self.streaming = StreamingMsg::default();
                true
            }
            Event::User(msg_text) => {
                // Consume the matching optimistic bubble instead of doubling.
                if self.pending_echoes.front().map(|f| *f == msg_text).unwrap_or(false) {
                    self.pending_echoes.pop_front();
                    return false;
                }
                if matches!(self.entries.last().map(|e| &e.kind), Some(PiEntryKind::User(t)) if *t == msg_text) {
                    return false;
                }
                self.push_kind(PiEntryKind::User(msg_text));
                true
            }
            Event::MsgStart { parts } => {
                self.streaming = StreamingMsg {
                    blocks: parts
                        .iter()
                        .filter_map(|(idx, part)| match part {
                            Part::Thinking(t) => {
                                Some(StreamBlock::Thinking { idx: *idx, text: t.clone() })
                            }
                            Part::Text(t) => Some(StreamBlock::Text { idx: *idx, text: t.clone() }),
                            Part::ToolCall { .. } => None,
                        })
                        .collect(),
                };
                for (_, part) in &parts {
                    if let Part::ToolCall { id, name } = part {
                        self.ensure_tool(id, name);
                    }
                }
                true
            }
            Event::Delta { kind, idx, chunk } => {
                let blocks = &mut self.streaming.blocks;
                if let Some(slot) = blocks.iter_mut().find(|b| match (kind, b) {
                    (pi::DeltaKind::Text, StreamBlock::Text { idx: i, .. }) => *i == idx,
                    (pi::DeltaKind::Thinking, StreamBlock::Thinking { idx: i, .. }) => *i == idx,
                    _ => false,
                }) {
                    match slot {
                        StreamBlock::Text { text: t, .. } | StreamBlock::Thinking { text: t, .. } => {
                            t.push_str(&chunk);
                        }
                    }
                } else {
                    blocks.push(match kind {
                        pi::DeltaKind::Text => StreamBlock::Text { idx, text: chunk },
                        pi::DeltaKind::Thinking => StreamBlock::Thinking { idx, text: chunk },
                    });
                }
                true
            }
            Event::AssistantEnd { parts } => {
                // The final parts are authoritative — flush them into entries
                // and drop the live buffer wholesale (same content, no visible
                // flicker).
                for (_, part) in parts {
                    match part {
                        Part::Thinking(t) => {
                            if !t.is_empty() {
                                self.push_kind(PiEntryKind::Thinking(t));
                            }
                        }
                        Part::Text(t) => {
                            if !t.is_empty() {
                                self.push_kind(PiEntryKind::Assistant(t));
                            }
                        }
                        Part::ToolCall { id, name } => self.ensure_tool(&id, &name),
                    }
                }
                self.streaming = StreamingMsg::default();
                true
            }
            Event::ToolCallStart { id, name } => {
                self.ensure_tool(&id, &name);
                true
            }
            Event::ToolPartial { id, name, output } => {
                self.set_tool(&id, &name, output, false, false);
                true
            }
            Event::ToolResult { id, name, output, is_error } => {
                self.set_tool(&id, &name, output, true, is_error);
                true
            }
            Event::Queue { follow_up, .. } => {
                self.queued_followups = Some(follow_up.len());
                false
            }
            Event::Streaming(v) => {
                if let PiStatus::Ready { streaming, .. } = &mut self.status {
                    *streaming = v;
                }
                false
            }
            Event::StreamError(msg) => {
                if !matches!(self.entries.last().map(|e| &e.kind), Some(PiEntryKind::Notice(n)) if *n == msg) {
                    self.push_kind(PiEntryKind::Notice(msg));
                    return true;
                }
                false
            }
            Event::SendFailed { message } => {
                // Remove the optimistic bubble of the most recent send and
                // surface the rejection.
                if let Some(echo) = self.pending_echoes.pop_back() {
                    if let Some(pos) = self
                        .entries
                        .iter()
                        .rposition(|e| matches!(&e.kind, PiEntryKind::User(t) if *t == echo))
                    {
                        self.entries.remove(pos);
                    }
                }
                self.push_kind(PiEntryKind::Notice(format!("发送失败：{message}")));
                true
            }
        }
    }

    /// Send the composer text: optimistic user bubble + worker command.
    /// Returns whether the transcript changed (bubble pushed).
    pub fn send_current(&mut self) -> bool {
        let message = self.input.text().trim().to_string();
        if message.is_empty() {
            return false;
        }
        let session = self.active.clone().unwrap_or_default();
        let queued = self.is_streaming();
        self.pending_echoes.push_back(message.clone());
        self.push_kind(PiEntryKind::User(message.clone()));
        self.input = text_editor::Content::new();
        self.send_command(pi::Command::Send { session, message, queued });
        true
    }

    /// Switch the followed session (clears the view; backfill refills it).
    pub fn watch_session(&mut self, id: String) {
        self.active = Some(id.clone());
        self.entries.clear();
        self.streaming = StreamingMsg::default();
        self.pending_echoes.clear();
        self.queued_followups = None;
        self.status = PiStatus::Connecting;
        self.send_command(pi::Command::Watch(id));
    }

    /// Toggle one entry's expanded flag (entries first, then live blocks).
    pub fn toggle_entry(&mut self, id: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.expanded = !e.expanded;
        } else if !self.live_expanded.remove(&id) {
            self.live_expanded.insert(id);
        }
    }

    /// Push a new live entry with a fresh id; returns the id.
    fn push_kind(&mut self, kind: PiEntryKind) -> u64 {
        let id = self.next_live_id();
        self.entries.push(PiEntry { id, kind, expanded: false });
        id
    }

    fn next_live_id(&mut self) -> u64 {
        self.next_id += 1;
        LIVE_ID_BASE + self.next_id
    }

    /// Find-or-create the Tool entry for a call id (content-addressed so a
    /// reconnect's backfill keeps identity and expand state stable).
    fn ensure_tool(&mut self, id: &str, name: &str) {
        let exists = self
            .entries
            .iter()
            .any(|e| matches!(&e.kind, PiEntryKind::Tool { id: tid, .. } if tid == id));
        if !exists {
            self.entries.push(PiEntry {
                id: stable_id("tool", id, 0),
                kind: PiEntryKind::Tool {
                    id: id.to_string(),
                    name: name.to_string(),
                    output: String::new(),
                    done: false,
                    is_error: false,
                },
                expanded: false,
            });
        }
    }

    /// Update a Tool entry's output by call id (find-or-create).
    fn set_tool(&mut self, id: &str, name: &str, output: String, done: bool, is_error: bool) {
        let pos = self
            .entries
            .iter()
            .rposition(|e| matches!(&e.kind, PiEntryKind::Tool { id: tid, .. } if tid == id));
        if let Some(pos) = pos {
            if let PiEntryKind::Tool { name: n, output: o, done: d, is_error: e, .. } =
                &mut self.entries[pos].kind
            {
                *n = name.to_string();
                *o = output;
                *d = done;
                *e = is_error;
            }
        } else {
            self.entries.push(PiEntry {
                id: stable_id("tool", id, 0),
                kind: PiEntryKind::Tool {
                    id: id.to_string(),
                    name: name.to_string(),
                    output,
                    done,
                    is_error,
                },
                expanded: false,
            });
        }
    }

    /// Stable id for a streaming block (expand state keyed by kind+index).
    fn live_block_id(idx: usize, tag: &str) -> u64 {
        stable_id(tag, &format!("live-{idx}"), 0)
    }
}

// ── Stable ids (backfill) ───────────────────────────────────────────────────

/// FNV-1a — cheap deterministic hash for entry ids.
fn fnv1a(data: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Deterministic id from a kind tag + content key + occurrence index, so a
/// reconnect's backfill produces the same ids the live session used.
fn stable_id(tag: &str, key: &str, occurrence: usize) -> u64 {
    fnv1a(&format!("{tag}\u{1}{key}\u{1}{occurrence}"))
}

/// Map a backfilled [`Entry`] to a renderable [`PiEntry`] with a stable id.
fn stable_entry(entry: Entry) -> PiEntry {
    let (tag, key, kind) = match entry {
        Entry::User(t) => ("u", t.clone(), PiEntryKind::User(t)),
        Entry::Assistant(t) => ("a", t.clone(), PiEntryKind::Assistant(t)),
        Entry::Thinking(t) => ("t", t.clone(), PiEntryKind::Thinking(t)),
        Entry::Tool { id, name, output, done, is_error } => {
            ("tool", id.clone(), PiEntryKind::Tool { id, name, output, done, is_error })
        }
        Entry::Notice(n) => ("n", n.clone(), PiEntryKind::Notice(n)),
    };
    PiEntry { id: stable_id(tag, &key, 0), kind, expanded: false }
}

// ── Panel chrome (header identical to the Properties panel) ─────────────────

fn header(auto_collapse: bool) -> Element<'static, Message> {
    use crate::ui::dock::{DockMsg, PanelId};
    let pin_icon = if auto_collapse {
        crate::ui::icons::themed_primary_weak_text(crate::ui::icons::PIN, 12.0)
    } else {
        crate::ui::icons::themed_secondary(crate::ui::icons::PIN, 12.0)
    };
    let pin = button(pin_icon)
        .on_press(Message::Dock(DockMsg::AutoCollapseToggle(PanelId::Pi)))
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
    .on_press(Message::Dock(DockMsg::Close(PanelId::Pi)))
    .style(button::subtle)
    .padding([3, 5]);
    let close = tooltip(close, text("Close").size(10), tooltip::Position::Bottom).gap(4);

    mouse_area(
        container(
            row![
                text("Pi 助手").size(12),
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
    .on_press(Message::Dock(DockMsg::DockGrab(PanelId::Pi)))
    .interaction(iced::mouse::Interaction::Grab)
    .into()
}

// ── Text helpers ────────────────────────────────────────────────────────────

fn secondary_color(theme: &Theme) -> Color {
    theme.palette().secondary.base.text
}

fn warning_color(theme: &Theme) -> Color {
    theme.palette().warning.base.text
}

/// Small secondary-colored label above an entry.
fn tag_label(label: &str) -> iced::widget::Text<'_, Theme> {
    text(label.to_string())
        .size(10)
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(secondary_color(theme)),
        })
}

/// The collapsible one-line header used by thinking / tool entries.
fn toggle_row(
    title: String,
    badge: Option<Badge>,
    expanded: bool,
    id: u64,
) -> Element<'static, Message> {
    let mut head = row![
        text(if expanded { "▾" } else { "▸" }).size(10),
        text(title).size(10),
    ]
    .spacing(4)
    .align_y(iced::Center);
    if let Some(badge) = badge {
        head = head.push(
            text(badge.label())
                .size(10)
                .style(move |theme: &Theme| iced::widget::text::Style {
                    color: Some(badge.color(theme)),
                }),
        );
    }
    button(head)
        .on_press(Message::Pi(PiMsg::ToggleEntry(id)))
        .style(|theme: &Theme, status| button::subtle(theme, status))
        .padding([2, 4])
        .into()
}

// ── Rows above the transcript ─────────────────────────────────────────────

/// Session switcher: follow-newest by default, user can pin another one.
fn session_picker(state: &PiPanelState) -> Element<'_, Message> {
    let selected = state
        .active
        .as_ref()
        .and_then(|id| state.sessions.iter().find(|s| &s.id == id));
    let label = |s: &pi::SessionInfo| {
        if s.label.is_empty() {
            "（无标题会话）".to_string()
        } else {
            s.label.clone()
        }
    };
    let picker = pick_list(selected, state.sessions.as_slice(), label)
        .placeholder("选择会话…")
        .width(Length::Fill)
        .text_size(11)
        .padding([2, 6])
        .menu_height(240.0)
        .on_select(|s: pi::SessionInfo| Message::Pi(PiMsg::SessionPick(s.id.clone())));
    container(picker).width(Length::Fill).padding([4, 6]).into()
}

/// Status strip: connection label, optional reconnect button.
fn status_strip(state: &PiPanelState) -> Element<'_, Message> {
    let errored = matches!(state.status, PiStatus::Error(_));
    let idle = matches!(state.status, PiStatus::Idle);
    let mut strip = row![
        text(state.status_label())
            .size(10)
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(secondary_color(theme)),
            })
            .width(Length::Fill),
    ]
    .spacing(4)
    .align_y(iced::Center);
    if errored || idle {
        let reconnect = button(text(if idle { "连接" } else { "重连" }).size(10))
            .on_press(Message::Pi(PiMsg::Reconnect))
            .style(|theme: &Theme, status| button::subtle(theme, status))
            .padding([2, 6]);
        strip = strip.push(reconnect);
    }
    container(strip).width(Length::Fill).padding([2, 8]).into()
}

// ── Transcript ────────────────────────────────────────────────────────

fn entry_view(e: &PiEntry) -> Element<'_, Message> {
    match &e.kind {
        PiEntryKind::User(t) => container(
            column![tag_label("你"), text(t.clone()).size(12)]
                .spacing(2)
                .width(Length::Fill),
        )
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().primary.weak.color)),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .width(Length::Fill)
        .padding([6, 8])
        .into(),
        PiEntryKind::Assistant(t) => column![
            tag_label("Pi"),
            text(t.clone()).size(12).width(Length::Fill),
        ]
        .spacing(2)
        .padding([2, 4])
        .width(Length::Fill)
        .into(),
        PiEntryKind::Thinking(t) => {
            let head = toggle_row("思考".to_string(), None, e.expanded, e.id);
            let mut body = column![head].spacing(4).padding([2, 4]).width(Length::Fill);
            if e.expanded && !t.is_empty() {
                body = body.push(
                    text(t.clone())
                        .size(11)
                        .style(|theme: &Theme| iced::widget::text::Style {
                            color: Some(secondary_color(theme)),
                        })
                        .width(Length::Fill),
                );
            }
            body.into()
        }
        PiEntryKind::Tool { name, output, done, is_error, .. } => {
            let badge = if !*done {
                Badge::Running
            } else if *is_error {
                Badge::Failed
            } else {
                Badge::Done
            };
            let head =
                toggle_row(format!("工具 · {name}"), Some(badge), e.expanded, e.id);
            let mut body = column![head].spacing(4).padding([2, 4]).width(Length::Fill);
            if e.expanded && !output.is_empty() {
                body = body.push(
                    text(output.clone())
                        .size(11)
                        .font(Font::MONOSPACE)
                        .width(Length::Fill),
                );
            }
            body.into()
        }
        PiEntryKind::Notice(n) => container(
            text(n.clone())
                .size(11)
                .style(|theme: &Theme| iced::widget::text::Style {
                    color: Some(warning_color(theme)),
                }),
        )
        .width(Length::Fill)
        .padding([2, 4])
        .into(),
    }
}

/// The live assistant message (thinking blocks + growing text).
fn streaming_view(state: &PiPanelState) -> Option<Element<'_, Message>> {
    if state.streaming.blocks.is_empty() {
        return None;
    }
    let mut col = column![].spacing(2).width(Length::Fill);
    for block in &state.streaming.blocks {
        match block {
            StreamBlock::Thinking { idx, text: t } => {
                // Live thinking stays a collapsed summary unless toggled; the
                // toggle id is stable across deltas so expand state holds.
                let id = PiPanelState::live_block_id(*idx, "t");
                let expanded = state.live_expanded.contains(&id);
                let head = toggle_row("思考中…".to_string(), None, expanded, id);
                let mut body = column![head].spacing(4).padding([2, 4]).width(Length::Fill);
                if expanded && !t.is_empty() {
                    body = body.push(
                        text(t.clone())
                            .size(11)
                            .style(|theme: &Theme| iced::widget::text::Style {
                                color: Some(secondary_color(theme)),
                            })
                            .width(Length::Fill),
                    );
                }
                col = col.push(body);
            }
            StreamBlock::Text { text: t, .. } => {
                let body = if t.is_empty() { "…".to_string() } else { t.clone() };
                col = col.push(
                    column![tag_label("Pi"), text(body).size(12)]
                        .spacing(2)
                        .padding([2, 4])
                        .width(Length::Fill),
                );
            }
        }
    }
    Some(col.into())
}

/// Empty-state hint shown when the transcript has nothing to render.
fn empty_state(state: &PiPanelState) -> Element<'static, Message> {
    let hint: String = match &state.status {
        PiStatus::Error(_) => {
            "未连接 pi-web。\n启动方式：pi-web（或 node ~/.local/bin/pi-web）。\n默认端点 http://127.0.0.1:30141，\n可用环境变量 OCS_PI_ENDPOINT 覆盖。".to_string()
        }
        PiStatus::Ready { .. } if state.sessions.is_empty() => {
            "pi-web 没有会话。\n在网页端新建一个会话后，本面板会自动跟随。".to_string()
        }
        PiStatus::Ready { .. } => "暂无消息 — 在下方输入框发送第一条".to_string(),
        PiStatus::Connecting => "正在连接…".to_string(),
        PiStatus::Idle => "未连接 — 点击上方「连接」开始".to_string(),
    };
    container(
        column![
            text("Pi 助手").size(13),
            text(hint)
                .size(11)
                .style(|theme: &Theme| iced::widget::text::Style {
                    color: Some(secondary_color(theme)),
                }),
        ]
        .spacing(8),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .padding(16)
    .into()
}

// ── Rows below the transcript ───────────────────────────────────────────────

/// Composer: multiline editor + send row. Enter sends, Shift+Enter breaks the
/// line (see the `key_binding` interception below).
fn composer(state: &PiPanelState) -> Element<'_, Message> {
    let can_send = !state.input.text().trim().is_empty();
    let editor = text_editor(&state.input)
        .placeholder("向 Pi 发送…（Enter 发送 / Shift+Enter 换行）")
        .size(12)
        .height(Length::Fixed(COMPOSER_H))
        .padding(4)
        .key_binding(|kp| {
            use iced::keyboard::{key::Named, Key};
            use iced::widget::text_editor::{Binding, Status};
            let focused = matches!(kp.status, Status::Focused { .. });
            let plain_enter = matches!(kp.key, Key::Named(Named::Enter)) && !kp.modifiers.shift();
            if focused && plain_enter {
                Some(Binding::Custom(Message::Pi(PiMsg::Send)))
            } else {
                Binding::from_key_press(kp)
            }
        })
        .on_action(|a| Message::Pi(PiMsg::Editor(a)));

    let send = button(text("发送").size(11))
        .on_press_maybe(can_send.then(|| Message::Pi(PiMsg::Send)))
        .style(move |theme: &Theme, status| {
            let mut style = button::subtle(theme, status);
            if can_send {
                let palette = theme.palette();
                style.background = Some(Background::Color(palette.primary.weak.color));
                style.text_color = palette.primary.weak.text;
            }
            style
        })
        .padding([4, 10]);

    let mut hint_text = if state.is_streaming() {
        "Pi 生成中…Enter 会排队为后续消息".to_string()
    } else {
        "Enter 发送 / Shift+Enter 换行".to_string()
    };
    if let Some(n) = state.queued_followups {
        if n > 0 {
            hint_text = format!("已排队 {n} 条 · {hint_text}");
        }
    }
    let hint = text(hint_text)
        .size(10)
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(secondary_color(theme)),
        });

    container(
        column![
            editor,
            row![hint, Space::new().width(Length::Fill), send]
                .spacing(6)
                .align_y(iced::Center),
        ]
        .spacing(4),
    )
    .style(|theme: &Theme| container::Style {
        border: Border {
            color: theme.palette().background.strong.color,
            width: 1.0,
            ..Default::default()
        },
        background: Some(Background::Color(theme.palette().background.weak.color)),
        ..Default::default()
    })
    .width(Length::Fill)
    .padding([6, 8])
    .into()
}

// ── Panel body ──────────────────────────────────────────────────────────────

/// Panel chrome: title + pin + close, exactly like the Properties panel so the
/// dock drag/resize/collapse affordances all behave identically.
pub fn view(state: &PiPanelState, width: f32, auto_collapse: bool) -> Element<'_, Message> {
    let nothing_to_show =
        state.entries.is_empty() && state.streaming.blocks.is_empty();
    let transcript: Element<'_, Message> = if nothing_to_show {
        empty_state(state)
    } else {
        let mut list = column![].spacing(10).width(Length::Fill);
        for e in &state.entries {
            list = list.push(entry_view(e));
        }
        if let Some(live) = streaming_view(state) {
            list = list.push(live);
        }
        scrollable(list)
            .id(iced::widget::Id::new(TRANSCRIPT_ID))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    // **`Fixed(width)` is load-bearing**: the dock hands every expanded panel
    // the column width it computed, and a `Fill`-width panel instead competes
    // with the drawing canvas for the same row space (which made the panel
    // take half the window). Every built-in panel pins its width this way.
    column![header(auto_collapse), session_picker(state), status_strip(state), transcript, composer(state)]
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi::DeltaKind;

    fn state() -> PiPanelState {
        PiPanelState::empty()
    }

    #[test]
    fn user_echo_consumes_optimistic_bubble() {
        let mut s = state();
        s.input = text_editor::Content::with_text("你好");
        assert!(s.send_current());
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.pending_echoes.len(), 1);
        // The SSE echo of the same message is consumed, not duplicated.
        assert!(!s.apply(Event::User("你好".into())));
        assert_eq!(s.entries.len(), 1);
        // A different user message still renders.
        assert!(s.apply(Event::User("再来一条".into())));
        assert_eq!(s.entries.len(), 2);
    }

    #[test]
    fn streaming_text_appends_in_place_and_finalizes() {
        let mut s = state();
        assert!(s.apply(Event::MsgStart { parts: vec![] }));
        assert!(s.apply(Event::Delta { kind: DeltaKind::Text, idx: 0, chunk: "你好".into() }));
        assert!(s.apply(Event::Delta { kind: DeltaKind::Text, idx: 0, chunk: "，世界".into() }));
        // Still one live block, not entries.
        assert_eq!(s.streaming.blocks.len(), 1);
        assert_eq!(s.entries.len(), 0);
        assert!(s.apply(Event::AssistantEnd { parts: vec![(0, Part::Text("你好，世界".into()))] }));
        // Final flush lands as one Assistant entry; live buffer cleared.
        assert_eq!(s.entries.len(), 1);
        assert!(matches!(&s.entries[0].kind, PiEntryKind::Assistant(t) if t == "你好，世界"));
        assert!(s.streaming.blocks.is_empty());
    }

    #[test]
    fn thinking_block_streams_collapsed_and_lands_before_text() {
        let mut s = state();
        s.apply(Event::MsgStart { parts: vec![] });
        s.apply(Event::Delta { kind: DeltaKind::Thinking, idx: 0, chunk: "先想".into() });
        s.apply(Event::Delta { kind: DeltaKind::Thinking, idx: 0, chunk: "一想".into() });
        s.apply(Event::Delta { kind: DeltaKind::Text, idx: 1, chunk: "答案".into() });
        // Two live blocks: thinking at idx 0, text at idx 1.
        assert_eq!(s.streaming.blocks.len(), 2);
        assert!(s.apply(Event::AssistantEnd {
            parts: vec![(0, Part::Thinking("先想一想".into())), (1, Part::Text("答案".into()))],
        }));
        assert_eq!(s.entries.len(), 2);
        assert!(matches!(&s.entries[0].kind, PiEntryKind::Thinking(_)));
        assert!(matches!(&s.entries[1].kind, PiEntryKind::Assistant(_)));
        // Thinking entries start collapsed.
        assert!(!s.entries[0].expanded);
    }

    #[test]
    fn tool_lifecycle_keyed_by_call_id() {
        let mut s = state();
        s.apply(Event::ToolCallStart { id: "t1".into(), name: "bash".into() });
        s.apply(Event::ToolPartial { id: "t1".into(), name: "bash".into(), output: "部分输出".into() });
        assert_eq!(s.entries.len(), 1);
        assert!(matches!(&s.entries[0].kind, PiEntryKind::Tool { output, done: false, .. } if output == "部分输出"));
        // Duplicate start (toolcall_start + tool_execution_start) is idempotent.
        s.apply(Event::ToolCallStart { id: "t1".into(), name: "bash".into() });
        assert_eq!(s.entries.len(), 1);
        s.apply(Event::ToolResult { id: "t1".into(), name: "bash".into(), output: "全部输出".into(), is_error: false });
        assert!(matches!(&s.entries[0].kind, PiEntryKind::Tool { output, done: true, is_error: false, .. } if output == "全部输出"));
    }

    #[test]
    fn backfill_replace_keeps_stable_tool_ids() {
        let mut s = state();
        s.apply(Event::ToolCallStart { id: "t9".into(), name: "read".into() });
        let live_tool_id = s.entries[0].id;
        // Reconnect: backfill reconstructs the same tool call from the file.
        let replaced = s.apply(Event::Replace(vec![Entry::Tool {
            id: "t9".into(),
            name: "read".into(),
            output: "内容".into(),
            done: true,
            is_error: false,
        }]));
        assert!(replaced);
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].id, live_tool_id, "tool id must survive reconnect backfill");
    }

    #[test]
    fn send_failed_removes_optimistic_bubble() {
        let mut s = state();
        s.input = text_editor::Content::with_text("会被拒绝的消息");
        s.send_current();
        assert_eq!(s.entries.len(), 1);
        assert!(s.apply(Event::SendFailed { message: "Session not found".into() }));
        // Bubble removed, notice added.
        assert!(matches!(&s.entries[0].kind, PiEntryKind::Notice(n) if n.contains("Session not found")));
        assert_eq!(s.entries.len(), 1);
        assert!(s.pending_echoes.is_empty());
    }

    #[test]
    fn queued_send_marks_message_and_streaming_flag_updates() {
        let mut s = state();
        s.status = PiStatus::Ready { session: "s1".into(), streaming: false };
        s.apply(Event::Streaming(true));
        assert!(s.is_streaming());
        s.apply(Event::Streaming(false));
        assert!(!s.is_streaming());
        s.apply(Event::Queue { steering: vec![], follow_up: vec!["a".into(), "b".into()] });
        assert_eq!(s.queued_followups, Some(2));
    }

    #[test]
    fn user_picked_session_beats_follow_newest() {
        let mut s = state();
        s.apply(Event::Sessions(vec![
            pi::SessionInfo { id: "newest".into(), label: "最新".into(), path: None },
            pi::SessionInfo { id: "picked".into(), label: "手选".into(), path: None },
        ]));
        // Default: newest.
        assert_eq!(s.active.as_deref(), Some("newest"));
        s.watch_session("picked".into());
        assert_eq!(s.active.as_deref(), Some("picked"));
        assert!(s.entries.is_empty());
        // A later session list refresh must not steal the user's pick.
        s.apply(Event::Sessions(vec![pi::SessionInfo {
            id: "newer".into(),
            label: "更新".into(),
            path: None,
        }]));
        assert_eq!(s.active.as_deref(), Some("picked"));
    }

    #[test]
    fn status_ready_updates_active_session() {
        let mut s = state();
        s.apply(Event::Status(Status::Ready { session: "abc".into(), streaming: true }));
        assert_eq!(s.active.as_deref(), Some("abc"));
        assert!(s.is_streaming());
        assert!(s.status_label().contains("生成中"));
    }
}
