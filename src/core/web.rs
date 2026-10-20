//! Luvus Web — browser client for live workspaces, terminals, and agent sessions.
//!
//! An optional browser client that pairs devices and shows live pane/agent state.

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::Html,
    routing::get,
    Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// WebSocket message from client to server.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe { channels: Vec<String> },
    #[serde(rename = "pane_input")]
    PaneInput { pane_id: usize, input: String },
}

/// WebSocket message from server to client.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ServerMessage {
    #[serde(rename = "pane_state")]
    PaneState {
        pane_id: usize,
        title: String,
        agent: Option<String>,
        status: String,
    },
}

/// Web server state shared across connections.
#[derive(Debug, Default, Clone)]
pub struct WebState {
    panes: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl WebState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_pane(&self, pane_id: usize, title: &str, agent: Option<&str>, status: &str) {
        let mut panes = self.panes.lock().unwrap();
        let entry = serde_json::json!({
            "pane_id": pane_id,
            "title": title,
            "agent": agent,
            "status": status,
        });
        if let Some(existing) = panes.iter_mut().find(|p| p["pane_id"] == pane_id) {
            *existing = entry;
        } else {
            panes.push(entry);
        }
    }

    pub fn panes_snapshot(&self) -> Vec<serde_json::Value> {
        self.panes.lock().unwrap().clone()
    }
}

/// Simple HTML dashboard frontend.
const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>Term — Luvus Web</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0f0f0f; color: #e0e0e0; }
        header { background: #1a1a1a; padding: 1rem; border-bottom: 1px solid #333; }
        h1 { font-size: 1.25rem; color: #00d4ff; }
        .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 1rem; padding: 1rem; }
        .pane-card { background: #1a1a1a; border: 1px solid #333; border-radius: 8px; padding: 1rem; }
        .pane-title { font-weight: bold; margin-bottom: 0.5rem; color: #00d4ff; }
        .pane-agent { font-size: 0.875rem; color: #888; margin-bottom: 0.5rem; }
        .pane-status { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 4px; font-size: 0.75rem; }
        .status-working { background: #0d4f1a; color: #4ade80; }
        .status-blocked { background: #7f1d1d; color: #fca5a5; }
        .status-idle { background: #454545; color: #9ca3af; }
        .status-done { background: #1d4ed8; color: #bfdbfe; }
        .status-unknown { background: #4b5563; color: #d1d5db; }
        .output { font-family: 'Menlo', monospace; font-size: 0.75rem; color: #a1a1aa; margin-top: 0.5rem; max-height: 200px; overflow-y: auto; }
        .status-indicator { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; }
        .live { background: #22c55e; box-shadow: 0 0 8px #22c55e; }
    </style>
</head>
<body>
    <header>
        <h1>🖥️ Term — Luvus Web</h1>
        <p id="status">Connecting...</p>
    </header>
    <div class="grid" id="grid"></div>

    <script>
        const wsUrl = `ws://${window.location.host}/ws`;
        let ws;
        const panes = {};

        function connect() {
            ws = new WebSocket(wsUrl);
            ws.onopen = () => {
                document.getElementById('status').textContent = 'Connected';
                document.getElementById('status').style.color = '#4ade80';
                ws.send(JSON.stringify({ type: 'subscribe', channels: ['panes'] }));
            };
            ws.onclose = () => {
                document.getElementById('status').textContent = 'Disconnected — retrying...';
                document.getElementById('status').style.color = '#fca5a5';
                setTimeout(connect, 2000);
            };
            ws.onmessage = (event) => {
                const msg = JSON.parse(event.data);
                if (msg.type === 'pane_state') {
                    updatePane(msg.pane_id, msg.title, msg.agent, msg.status);
                } else if (msg.type === 'pane_output') {
                    appendOutput(msg.pane_id, msg.output);
                }
            };
        }

        function updatePane(id, title, agent, status) {
            let card = document.getElementById(`pane-${id}`);
            if (!card) {
                card = document.createElement('div');
                card.id = `pane-${id}`;
                card.className = 'pane-card';
                card.innerHTML = `
                    <div class="pane-title">Pane ${id}</div>
                    <div class="pane-agent">${agent || '—'}</div>
                    <div class="pane-status" id="status-${id}"><span class="status-indicator live status-${status}"></span>${status}</div>
                    <div class="output" id="output-${id}"></div>
                `;
                document.getElementById('grid').appendChild(card);
            }
            card.querySelector('.pane-title').textContent = title || `Pane ${id}`;
            card.querySelector('.pane-agent').textContent = agent || '—';
            const statusEl = card.querySelector('.pane-status');
            statusEl.querySelector('.status-indicator').className = `status-indicator live status-${status}`;
            statusEl.querySelector('.status-indicator').nextSibling.textContent = ` ${status}`;
            statusEl.className = `pane-status status-${status}`;
        }

        function appendOutput(paneId, output) {
            const outputEl = document.getElementById(`output-${paneId}`);
            if (outputEl) {
                outputEl.textContent += output;
                outputEl.scrollTop = outputEl.scrollHeight;
            }
        }

        connect();
    </script>
</body>
</html>"#;

/// Start the Luvus Web server.
pub async fn start_web(state: Arc<WebState>, port: u16) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(move || async { Html(DASHBOARD_HTML) }))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let listener = TcpListener::bind(addr).await?;
    eprintln!("luvus web: http://localhost:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}

/// WebSocket handler.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<WebState>) {
    // Send initial pane state
    let panes = state.panes_snapshot();
    if !panes.is_empty() {
        let _ = socket.send(Message::Text(serde_json::to_string(&panes).unwrap())).await;
    }

    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                if let Ok(cmd) = serde_json::from_str::<ClientMessage>(&text) {
                    match cmd {
                        ClientMessage::Subscribe { .. } => {}
                        ClientMessage::PaneInput { pane_id, input } => {
                            eprintln!("pane input: {} -> {}", pane_id, input);
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_state_update_and_snapshot() {
        let state = WebState::new();
        state.update_pane(1, "Claude Code", Some("claude"), "working");
        state.update_pane(2, "Codex", Some("codex"), "idle");
        let panes = state.panes_snapshot();
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0]["pane_id"], 1);
        assert_eq!(panes[0]["status"], "working");
        assert_eq!(panes[1]["status"], "idle");
    }

    #[test]
    fn web_state_update_existing() {
        let state = WebState::new();
        state.update_pane(1, "Test", None, "idle");
        state.update_pane(1, "Updated", Some("agent"), "blocked");
        let panes = state.panes_snapshot();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0]["status"], "blocked");
    }
}
