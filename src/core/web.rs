//! Ridge Web — browser client for live workspaces, terminals, and agent sessions.
//!
//! Can run standalone (mock data) or connect to a running `ridge --api` instance
//! via the Unix domain socket to show real panes and stream live PTY output.

use axum::{
    Router,
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header},
    response::Html,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

/// Path to the ridge API socket (for connecting to a running `ridge --api` instance).
pub fn socket_path() -> PathBuf {
    crate::config::data_dir().join("ridge.sock")
}

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
    State {
        pane_id: usize,
        title: String,
        agent: Option<String>,
        status: String,
    },
    #[serde(rename = "pane_output")]
    Output { pane_id: usize, output: String },
    #[serde(rename = "pane_list")]
    List { panes: Vec<serde_json::Value> },
}

/// Fetch pane output from a running `ridge --api` instance via the Unix socket.
#[cfg(unix)]
fn fetch_pane_output(socket_path: &Path, pane_id: usize, cursor: usize) -> Option<(String, usize)> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).ok()?;
    let request = serde_json::to_string(&serde_json::json!({
        "cmd": "GetOutput",
        "pane": pane_id,
        "cursor": cursor,
    }))
    .ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut buf = Vec::new();
    stream.take(4 << 20).read_to_end(&mut buf).ok()?;
    parse_pane_output_response(&buf)
}

#[cfg(unix)]
fn parse_pane_output_response(buf: &[u8]) -> Option<(String, usize)> {
    let response: serde_json::Value = serde_json::from_slice(buf).ok()?;
    if response.get("ok")?.as_str()? != "ok" {
        return None;
    }
    let data = response.get("data")?;
    let output = data.get("output")?.as_str()?.to_string();
    let next_cursor = data.get("next_cursor")?.as_u64()? as usize;
    Some((output, next_cursor))
}

#[cfg(not(unix))]
fn fetch_pane_output(
    _socket_path: &Path,
    _pane_id: usize,
    _cursor: usize,
) -> Option<(String, usize)> {
    None
}

/// Track per-pane output cursors for polling.
#[derive(Debug, Clone, Default)]
pub struct PaneCursorTracker {
    cursors: Arc<Mutex<HashMap<usize, usize>>>,
}

impl PaneCursorTracker {
    pub fn new() -> Self {
        Self {
            cursors: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_cursor(&self, pane_id: usize) -> usize {
        self.cursors
            .lock()
            .unwrap()
            .get(&pane_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn set_cursor(&self, pane_id: usize, cursor: usize) {
        self.cursors.lock().unwrap().insert(pane_id, cursor);
    }
}

/// Web server state shared across connections.
#[derive(Debug, Clone)]
pub struct WebState {
    panes: Arc<Mutex<Vec<serde_json::Value>>>,
    /// Broadcast channel for live PTY output (pane_id, output_text)
    output_tx: broadcast::Sender<(usize, String)>,
    /// Optional socket path to connect to a running `ridge --api` instance.
    socket_path: Option<PathBuf>,
    /// Per-pane output cursors for polling PTY output.
    cursor_tracker: Arc<PaneCursorTracker>,
}

impl WebState {
    pub fn new() -> Self {
        let (output_tx, _) = broadcast::channel::<(usize, String)>(100);
        Self {
            panes: Arc::new(Mutex::new(Vec::new())),
            output_tx,
            socket_path: None,
            cursor_tracker: Arc::new(PaneCursorTracker::new()),
        }
    }

    pub fn new_with_socket(socket_path: PathBuf) -> Self {
        let (output_tx, _) = broadcast::channel::<(usize, String)>(100);
        Self {
            panes: Arc::new(Mutex::new(Vec::new())),
            output_tx,
            socket_path: Some(socket_path),
            cursor_tracker: Arc::new(PaneCursorTracker::new()),
        }
    }

    pub fn output_tx(&self) -> broadcast::Sender<(usize, String)> {
        self.output_tx.clone()
    }

    pub fn update_panes(&self, panes: Vec<serde_json::Value>) {
        *self.panes.lock().unwrap() = panes;
    }

    pub fn panes_snapshot(&self) -> Vec<serde_json::Value> {
        self.panes.lock().unwrap().clone()
    }

    pub fn socket_path(&self) -> Option<&PathBuf> {
        self.socket_path.as_ref()
    }

    pub fn cursor_tracker(&self) -> Arc<PaneCursorTracker> {
        self.cursor_tracker.clone()
    }
}

/// Fetch pane list from a running `ridge --api` instance via the Unix socket.
#[cfg(unix)]
fn fetch_panes_from_api(socket_path: &Path) -> Option<Vec<serde_json::Value>> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).ok()?;
    let request = serde_json::to_string(&serde_json::json!({"cmd": "ListPanes"})).ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut buf = [0u8; 65536];
    let n = stream.read(&mut buf).ok()?;
    if n == 0 {
        return None;
    }
    let response: serde_json::Value = serde_json::from_slice(&buf[..n]).ok()?;
    response.get("data").and_then(|d| d.as_array()).cloned()
}

#[cfg(not(unix))]
fn fetch_panes_from_api(_socket_path: &Path) -> Option<Vec<serde_json::Value>> {
    None
}

/// Background task that polls the API socket for pane updates.
async fn poll_api_socket(state: Arc<WebState>, socket_path: PathBuf, poll_interval_ms: u64) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(poll_interval_ms));
    loop {
        interval.tick().await;
        if let Some(panes) = fetch_panes_from_api(&socket_path) {
            state.update_panes(panes.clone());
            // Broadcast each pane's state to WebSocket clients
            for pane in &panes {
                let pane_id = pane.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let title = pane
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let agent = pane
                    .get("agent")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let status = pane
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let msg = serde_json::to_string(&ServerMessage::State {
                    pane_id,
                    title,
                    agent,
                    status,
                })
                .unwrap_or_default();
                let _ = state.output_tx().send((pane_id, msg));

                // Poll for PTY output from this pane
                let cursor = state.cursor_tracker().get_cursor(pane_id);
                if let Some((output, next_cursor)) =
                    fetch_pane_output(&socket_path, pane_id, cursor)
                {
                    if !output.is_empty() {
                        let _ = state.output_tx().send((
                            pane_id,
                            serde_json::to_string(&ServerMessage::Output { pane_id, output })
                                .unwrap_or_default(),
                        ));
                    }
                    state.cursor_tracker().set_cursor(pane_id, next_cursor);
                }
            }
        }
    }
}

/// Simple HTML dashboard frontend with live WebSocket updates.
const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Ridge Web</title>
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
        .output { font-family: 'Menlo', monospace; font-size: 0.75rem; color: #a1a1aa; margin-top: 0.5rem; max-height: 300px; overflow-y: auto; background: #000; padding: 0.5rem; border-radius: 4px; }
        .status-indicator { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; }
        .live { background: #22c55e; box-shadow: 0 0 8px #22c55e; }
        .input-area { margin-top: 0.5rem; }
        .input-area input { width: 100%; padding: 0.5rem; background: #000; border: 1px solid #333; color: #e0e0e0; border-radius: 4px; font-family: inherit; }
    </style>
</head>
<body>
    <header>
        <h1>🖥️ Ridge Web</h1>
        <p id="status">Connecting...</p>
    </header>
    <div class="grid" id="grid"></div>

    <script>
        const wsUrl = `ws://${window.location.host}/ws`;
        let ws;
        const panes = {};
        const outputs = {};

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
                } else if (msg.type === 'pane_list') {
                    for (const p of msg.panes) {
                        updatePane(p.id, p.title, p.agent, p.status);
                    }
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
                    <div class="pane-status" id="status-${id}"><span class="status-indicator live status-${status || 'unknown'}"></span>${status || 'unknown'}</div>
                    <div class="output" id="output-${id}"></div>
                    <div class="input-area"><input type="text" id="input-${id}" placeholder="Send message to pane ${id}..." onkeypress="if(event.key==='Enter')sendInput(${id})"></div>
                `;
                document.getElementById('grid').appendChild(card);
            }
            card.querySelector('.pane-title').textContent = title || `Pane ${id}`;
            card.querySelector('.pane-agent').textContent = agent || '—';
            const statusEl = card.querySelector('.pane-status');
            statusEl.querySelector('.status-indicator').className = `status-indicator live status-${status || 'unknown'}`;
            statusEl.querySelector('.status-indicator').nextSibling.textContent = ` ${status || 'unknown'}`;
            statusEl.className = `pane-status status-${status || 'unknown'}`;
        }

        function appendOutput(paneId, output) {
            let outputEl = document.getElementById(`output-${paneId}`);
            if (!outputEl) {
                outputEl = document.createElement('div');
                outputEl.id = `output-${paneId}`;
                outputEl.className = 'output';
                const card = document.getElementById(`pane-${paneId}`);
                if (card) card.appendChild(outputEl);
            }
            outputEl.textContent += output;
            outputEl.scrollTop = outputEl.scrollHeight;
        }

        function sendInput(paneId) {
            const input = document.getElementById(`input-${paneId}`);
            if (!input || !input.value) return;
            const text = input.value;
            ws.send(JSON.stringify({ type: 'pane_input', pane_id: paneId, input: text }));
            input.value = '';
        }

        connect();
    </script>
</body>
</html>"#;

/// Start the Luvus Web server.
/// If `socket_path` is provided, connects to a running `ridge --api` instance
/// to fetch real pane data. Otherwise serves mock/static state.
pub async fn start_web(
    state: Arc<WebState>,
    port: u16,
    socket_path: Option<PathBuf>,
) -> std::io::Result<()> {
    // If a socket path is given, spawn a background task to poll the API.
    if let Some(sp) = socket_path.clone() {
        let state_for_poll = state.clone();
        tokio::spawn(async move {
            poll_api_socket(state_for_poll, sp, 2000).await;
        });
    }

    let app = Router::new()
        .route("/", get(move || async { Html(DASHBOARD_HTML) }))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let listener = TcpListener::bind(addr).await?;
    eprintln!("luvus web: http://localhost:{}", port);
    axum::serve(listener, app).await?;
    Ok(())
}

/// WebSocket handler.
async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<WebState>>,
) -> impl axum::response::IntoResponse {
    if !allowed_websocket_origin(&headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

fn allowed_websocket_origin(headers: &HeaderMap) -> bool {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    match (host, origin) {
        (Some(host), Some(origin)) => {
            let local_host = host
                .strip_prefix("localhost:")
                .or_else(|| host.strip_prefix("127.0.0.1:"));
            local_host.is_some_and(|port| port.parse::<u16>().is_ok())
                && origin == format!("http://{host}")
        }
        _ => false,
    }
}

async fn handle_socket(mut socket: WebSocket, state: Arc<WebState>) {
    // Send initial pane state
    let panes = state.panes_snapshot();
    if !panes.is_empty() {
        let _ = socket
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::List { panes }).unwrap(),
            ))
            .await;
    }

    // Subscribe to output broadcast
    let mut rx = state.output_tx().subscribe();

    // Split socket into send and receive halves
    let (mut sender, mut receiver) = socket.split();

    // Task to forward broadcast messages to the client
    let forward_task = tokio::spawn(async move {
        while let Ok((_pane_id, output)) = rx.recv().await {
            if sender.send(Message::Text(output)).await.is_err() {
                break;
            }
        }
    });

    // Task to read messages from the client
    let receive_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(cmd) = serde_json::from_str::<ClientMessage>(&text) {
                        match cmd {
                            ClientMessage::Subscribe { .. } => {
                                // Already subscribed via broadcast channel
                            }
                            ClientMessage::PaneInput { pane_id, input } => {
                                // Forward to the running ridge instance via socket API
                                if let Some(sp) = state.socket_path() {
                                    let _ = send_to_api_pane(sp, pane_id, &input);
                                }
                            }
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = forward_task => {},
        _ = receive_task => {},
    }
}

/// Send a message to a pane via the ridge API socket.
#[cfg(unix)]
fn send_to_api_pane(socket_path: &Path, pane_id: usize, message: &str) -> std::io::Result<()> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path)?;
    let request = serde_json::to_string(&serde_json::json!({
        "cmd": "SendMessage",
        "pane": pane_id,
        "message": message,
    }))?;
    stream.write_all(request.as_bytes())?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "unexpected EOF",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn send_to_api_pane(_socket_path: &Path, _pane_id: usize, _message: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "ridge --web requires Unix-domain sockets (not available on Windows)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_rejects_foreign_origins_and_hosts() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "localhost:8080".parse().unwrap());
        headers.insert(header::ORIGIN, "http://localhost:8080".parse().unwrap());
        assert!(allowed_websocket_origin(&headers));
        headers.insert(header::ORIGIN, "https://attacker.example".parse().unwrap());
        assert!(!allowed_websocket_origin(&headers));
        headers.insert(header::ORIGIN, "http://localhost:8080".parse().unwrap());
        headers.insert(header::HOST, "attacker.example:8080".parse().unwrap());
        assert!(!allowed_websocket_origin(&headers));
    }

    #[cfg(unix)]
    #[test]
    fn pane_output_uses_api_response_envelope() {
        let body = br#"{"ok":"ok","data":{"pane":2,"output":"hello","next_cursor":5}}"#;
        assert_eq!(parse_pane_output_response(body), Some(("hello".into(), 5)));
    }

    #[test]
    fn web_state_update_and_snapshot() {
        let state = WebState::new();
        state.update_panes(vec![
            serde_json::json!({"pane_id": 1, "title": "Claude Code", "agent": "claude", "status": "working"}),
            serde_json::json!({"pane_id": 2, "title": "Codex", "agent": "codex", "status": "idle"}),
        ]);
        let panes = state.panes_snapshot();
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0]["pane_id"], 1);
        assert_eq!(panes[0]["status"], "working");
    }

    #[test]
    fn output_tx_channel_works() {
        let state = WebState::new();
        let mut rx = state.output_tx().subscribe();
        let _ = state.output_tx().send((1, "test output".to_string()));
        let _ = rx.try_recv();
    }
}
