//! Local **pi-web API client** for the built-in AI panel.
//!
//! Everything is local, plain HTTP + JSON + SSE, so this speaks the protocol
//! directly over `std::net::TcpStream` — no HTTP/TLS dependencies, no bridge
//! process, no Node.js. A worker thread owns the connection and pushes parsed
//! events into a channel; the UI polls that channel (see `Message::AiPoll`).
//!
//! Endpoints used (documented by the pi-web route manifest):
//!   GET  {endpoint}/api/sessions                  → session list
//!   GET  {endpoint}/api/agent/<id>/events         → SSE stream (text/event-stream)
//!   POST {endpoint}/api/agent/<id> {"prompt": …}  → send a user message

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

/// A transcript entry the UI renders.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    User(String),
    Assistant(String),
    Tool { id: String, name: String, output: String, done: bool },
    Notice(String),
}

/// Events sent from the worker thread to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    /// Connection state changed.
    Status(Status),
    /// Session list refreshed (`(id, first message)`).
    Sessions(Vec<(String, String)>),
    /// Append this entry.
    Push(Entry),
    /// The last `Tool` entry with this id changed.
    ToolUpdate { id: String, name: String, chunk: String, done: bool },
    /// The transcript was replaced wholesale (session switch / reconnect).
    Replace(Vec<Entry>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Connecting,
    Ready { session: String, streaming: bool },
    Error(String),
}

/// Commands the UI sends to the worker.
#[derive(Debug, Clone)]
pub enum Command {
    /// Send `prompt` to the active session.
    Send { session: String, prompt: String },
    /// Switch the streamed session.
    Watch(String),
    /// Drop the connection and reconnect from scratch.
    Reconnect,
}

/// Handle the panel keeps for its worker thread.
pub struct AiHandle {
    pub rx: Receiver<Event>,
    pub tx: Sender<Command>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl AiHandle {
    /// Start a worker for `endpoint` and immediately connect.
    pub fn start(endpoint: &str) -> Self {
        let (tx_ev, rx_ev) = channel();
        let (tx_cmd, rx_cmd) = channel();
        let endpoint = endpoint.trim_end_matches('/').to_string();
        let join = std::thread::Builder::new()
            .name("ocs-ai-client".into())
            .spawn(move || worker(endpoint, tx_ev, rx_cmd))
            .ok();
        Self { rx: rx_ev, tx: tx_cmd, join }
    }

    pub fn stop(&mut self) {
        // Dropping the command sender ends the worker's loop; the thread is
        // detached so a blocked read can never stall the UI.
        let (dead_tx, _) = channel();
        let old = std::mem::replace(&mut self.tx, dead_tx);
        drop(old);
        if let Some(join) = self.join.take() {
            let _ = join.thread().id();
        }
    }
}

/// Minimal blocking HTTP request over a fresh connection.
fn http(
    endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let host_port = endpoint
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string();
    let mut stream = TcpStream::connect(&host_port).map_err(|e| format!("连接 {host_port} 失败：{e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .ok();
    let payload = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    stream.read_to_string(&mut out).map_err(|e| e.to_string())?;
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    Ok(body)
}

/// Extract `"sessions":[…]` into `(id, first line of firstMessage)`.
fn parse_sessions(text: &str) -> Vec<(String, String)> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(list) = json.get("sessions").and_then(|s| s.as_array()) {
        for s in list {
            let id = s.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let first = s
                .get("firstMessage")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect::<String>();
            if !id.is_empty() {
                out.push((id, first));
            }
        }
    }
    out
}

fn content_text(msg: &serde_json::Value) -> String {
    let mut out = String::new();
    if let Some(parts) = msg.get("content").and_then(|c| c.as_array()) {
        for part in parts {
            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
    }
    out
}

/// The worker: connect, stream the active session, answer UI commands.
fn worker(endpoint: String, tx: Sender<Event>, rx: Receiver<Command>) {
    loop {
        let _ = tx.send(Event::Status(Status::Connecting));
        let sessions = match http(&endpoint, "GET", "/api/sessions", None) {
            Ok(body) => parse_sessions(&body),
            Err(e) => {
                let _ = tx.send(Event::Status(Status::Error(e)));
                // Back off, but stay responsive to commands/stop.
                match rx.recv_timeout(Duration::from_secs(5)) {
                    Ok(Command::Reconnect) | Ok(Command::Watch(_)) => continue,
                    Ok(_) => continue,
                    Err(_) => return,
                }
            }
        };
        let Some((active, _)) = sessions.first().cloned() else {
            let _ = tx.send(Event::Status(Status::Error("pi-web 没有会话".into())));
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(_) => continue,
                Err(_) => return,
            }
        };
        let _ = tx.send(Event::Sessions(sessions));
        let _ = tx.send(Event::Status(Status::Ready {
            session: active.clone(),
            streaming: false,
        }));

        // Stream the session's events. Reads are blocking; the UI thread never
        // waits on this thread.
        let host_port = endpoint.trim_start_matches("http://").to_string();
        let Ok(mut stream) = TcpStream::connect(&host_port) else {
            continue;
        };
        let req = format!(
            "GET /api/agent/{active}/events HTTP/1.1\r\nHost: {host_port}\r\nAccept: text/event-stream\r\nConnection: keep-alive\r\n\r\n"
        );
        if stream.write_all(req.as_bytes()).is_err() {
            continue;
        }
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(ev) = serde_json::from_str::<serde_json::Value>(data.trim()) {
                    handle_sse_event(&ev, &tx);
                }
            }
            line.clear();
            // Commands arriving while streaming are handled on this loop too.
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    Command::Send { session, prompt } => {
                        let body = serde_json::json!({ "prompt": prompt }).to_string();
                        let _ = http(
                            &endpoint,
                            "POST",
                            &format!("/api/agent/{session}"),
                            Some(&body),
                        );
                    }
                    Command::Reconnect | Command::Watch(_) => return,
                }
            }
        }
    }
}

/// Map one SSE payload to a UI event.
fn handle_sse_event(ev: &serde_json::Value, tx: &Sender<Event>) {
    let kind = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match kind {
        "connected" => {
            let session = ev
                .get("sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();
            let streaming = ev.get("isStreaming").and_then(|b| b.as_bool()).unwrap_or(false);
            let _ = tx.send(Event::Status(Status::Ready { session, streaming }));
        }
        "message_start" | "message_end" => {
            let msg = ev.get("message").cloned().unwrap_or_default();
            match msg.get("role").and_then(|r| r.as_str()).unwrap_or("") {
                "user" => {
                    let _ = tx.send(Event::Push(Entry::User(content_text(&msg))));
                }
                "assistant" => {
                    let _ = tx.send(Event::Push(Entry::Assistant(content_text(&msg))));
                }
                "toolResult" => {
                    let id = msg.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = msg.get("toolName").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
                    let _ = tx.send(Event::ToolUpdate {
                        id,
                        name,
                        chunk: content_text(&msg),
                        done: true,
                    });
                }
                _ => {}
            }
        }
        "tool_execution_update" => {
            let id = ev.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = ev.get("toolName").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
            let partial = ev.get("partialResult").cloned().unwrap_or_default();
            let _ = tx.send(Event::ToolUpdate {
                id,
                name,
                chunk: content_text(&partial),
                done: false,
            });
        }
        "tool_execution_end" => {
            let id = ev.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = ev.get("toolName").and_then(|v| v.as_str()).unwrap_or("tool").to_string();
            let result = ev.get("result").cloned().unwrap_or_default();
            let _ = tx.send(Event::ToolUpdate {
                id,
                name,
                chunk: content_text(&result),
                done: true,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_list_parses_ids_and_first_message() {
        let body = r#"{"sessions":[{"id":"abc","firstMessage":"第一行\n第二行","messageCount":3}]}"#;
        let got = parse_sessions(body);
        assert_eq!(got, vec![("abc".to_string(), "第一行".to_string())]);
    }

    #[test]
    fn content_text_joins_text_parts() {
        let msg = serde_json::json!({"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]});
        assert_eq!(content_text(&msg), "ab");
    }
}
