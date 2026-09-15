//! Update handlers for the **OCS Pi Extension** panel (`Message::Pi`).
//!
//! The panel's data lives in `DocumentTab::pi_panel`; worker events are
//! drained here on a 10 Hz timer (`PiMsg::Poll`) — never on the render path —
//! so a chatty stream can't stall the drawing.

use iced::Task;

use super::OpenCADStudio;
use crate::app::Message;
use crate::ui::pi_panel::{PiEntryKind, PiMsg};

impl OpenCADStudio {
    pub(in crate::app) fn on_pi_msg(&mut self, msg: PiMsg) -> Task<Message> {
        let tab = self.active_tab;
        match msg {
            PiMsg::Poll => {
                // Drain every tab's worker channel; apply only mutates per-tab
                // state, so hidden tabs cost nothing but the drain itself.
                let mut scroll = false;
                for (i, tab_state) in self.tabs.iter_mut().enumerate() {
                    let mut events = Vec::new();
                    if let Some(worker) = &tab_state.pi_panel.worker {
                        while let Ok(ev) = worker.rx.try_recv() {
                            events.push(ev);
                            if events.len() >= 512 {
                                break; // one frame's budget; rest polls next tick
                            }
                        }
                    }
                    for ev in events {
                        let changed = tab_state.pi_panel.apply(ev);
                        if i == tab && changed {
                            scroll = true;
                        }
                    }
                }
                if scroll {
                    scroll_transcript_to_bottom()
                } else {
                    Task::none()
                }
            }
            PiMsg::Editor(action) => {
                self.tabs[tab].pi_panel.input.perform(action);
                Task::none()
            }
            PiMsg::Send => {
                if self.tabs[tab].pi_panel.send_current() {
                    scroll_transcript_to_bottom()
                } else {
                    Task::none()
                }
            }
            PiMsg::SessionPick(id) => {
                // Switch what we follow; the worker refetches + backfills and
                // the panel replaces its list wholesale.
                self.tabs[tab].pi_panel.ensure_worker();
                self.tabs[tab].pi_panel.watch_session(id);
                Task::none()
            }
            PiMsg::ToggleEntry(id) => {
                self.tabs[tab].pi_panel.toggle_entry(id);
                Task::none()
            }
            PiMsg::Reconnect => {
                let panel = &mut self.tabs[tab].pi_panel;
                let had_worker = panel.worker.is_some();
                panel.ensure_worker();
                if had_worker {
                    panel.send_command(crate::pi::Command::Reconnect);
                }
                panel.status = crate::ui::pi_panel::PiStatus::Connecting;
                Task::none()
            }
        }
    }
}

/// Scroll the transcript scrollable to its very bottom.
fn scroll_transcript_to_bottom() -> Task<Message> {
    iced::widget::operation::snap_to_end(iced::widget::Id::new(
        crate::ui::pi_panel::TRANSCRIPT_ID,
    ))
}

/// Silence unused-import warning when entry kinds are only used in tests.
#[allow(dead_code)]
fn _assert_kinds(_: PiEntryKind) {}
