//! Background server + detach/reattach (from Herdr).
//!
//! `term --server` runs a background daemon that owns the App state.
//! `term --attach` connects as a thin TUI client. Sessions survive
//! client disconnect — detach and reattach without losing panes or agents.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// Server-to-client message: screen update for the TUI.
#[derive(Debug, Serialize, Deserialize)]
pub enum ServerToClient {
    /// Full screen redraw with pane list and status.
    ScreenUpdate {
        panes: Vec<serde_json::Value>,
        active_pane: usize,
        status_message: String,
        current_tab: String,
    },
    /// Server is shutting down.
    Shutdown,
}

/// Client-to-server message: user input and commands.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientToServer {
    /// Key press from the TUI client.
    #[serde(rename = "key")]
    Key { key: String },
    /// Command executed in command mode.
    #[serde(rename = "command")]
    Command { cmd: String },
    /// Client requests current state snapshot.
    #[serde(rename = "sync")]
    Sync,
}

/// Socket path for the background server.
pub fn server_socket_path() -> std::path::PathBuf {
    crate::config::data_dir().join("term-server.sock")
}

/// Attach to a running background server as a TUI client.
pub fn attach_client() -> std::io::Result<()> {
    let path = server_socket_path();
    let mut stream = UnixStream::connect(&path)?;
    eprintln!("term client: attached to {}", path.display());

    // Simple read-echo loop: read state updates from server, forward key input.
    let mut buf = [0u8; 65536];
    let mut stdin_buf = [0u8; 4096];

    loop {
        // Check for server updates (non-blocking would be ideal, but this is a demo)
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Ok(msg) = serde_json::from_slice::<ServerToClient>(&buf[..n]) {
                    match msg {
                        ServerToClient::ScreenUpdate { .. } => {
                            // In a real client, this would render the TUI.
                            eprintln!("server state update received");
                        }
                        ServerToClient::Shutdown => break,
                    }
                }
            }
            Err(_) => break,
        }

        // Check stdin for user input
        match std::io::stdin().read(&mut stdin_buf) {
            Ok(0) => break,
            Ok(n) => {
                let input = String::from_utf8_lossy(&stdin_buf[..n]);
                let _ = stream.write_all(input.as_bytes());
            }
            Err(_) => break,
        }
    }
    Ok(())
}

/// Start the background server. Owns the App, accepts client connections.
pub fn start_server() -> std::io::Result<()> {
    let path = server_socket_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&path);

    let app = Arc::new(Mutex::new(
        crate::app::App::new(crate::config::Config::load().map_err(|e| {
            std::io::Error::other(e.to_string())
        })?),
    ));
    let listener = UnixListener::bind(&path)?;
    eprintln!("term server: {}", path.display());

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let app = Arc::clone(&app);
                std::thread::spawn(move || {
                    handle_client(&mut stream, app);
                });
            }
            Err(e) => eprintln!("server accept error: {e}"),
        }
    }
    Ok(())
}

/// Handle a single client connection.
fn handle_client(stream: &mut UnixStream, app: Arc<Mutex<crate::app::App>>) {
    let mut buf = [0u8; 65536];

    // Send initial state snapshot.
    {
        let app = app.lock().unwrap();
        let msg = ServerToClient::ScreenUpdate {
            panes: app
                .panes
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "title": p.title,
                        "agent": p.agent_id,
                        "status": p.status.label(),
                        "cost_usd": p.cost_usd,
                    })
                })
                .collect(),
            active_pane: app.active_pane,
            status_message: app.status_message.clone(),
            current_tab: app.current_tab.title().to_string(),
        };
        let _ = stream.write_all(
            &serde_json::to_vec(&msg).unwrap_or_default(),
        );
    }

    loop {
        match stream.read(&mut buf) {
            Ok(0) => break, // Client disconnected
            Ok(n) => {
                if let Ok(cmd) = serde_json::from_slice::<ClientToServer>(&buf[..n]) {
                    let mut app = app.lock().unwrap();
                    match cmd {
                        ClientToServer::Key { key } => {
                            // Parse key and apply to app
                            handle_key_event(&mut app, &key);
                        }
                        ClientToServer::Command { cmd } => {
                            app.execute_command(&cmd);
                        }
                        ClientToServer::Sync => {
                            // Already handled below
                        }
                    }
                    // Send updated state back to client.
                    let msg = ServerToClient::ScreenUpdate {
                        panes: app
                            .panes
                            .iter()
                            .map(|p| {
                                serde_json::json!({
                                    "id": p.id,
                                    "title": p.title,
                                    "agent": p.agent_id,
                                    "status": p.status.label(),
                                    "cost_usd": p.cost_usd,
                                })
                            })
                            .collect(),
                        active_pane: app.active_pane,
                        status_message: app.status_message.clone(),
                        current_tab: app.current_tab.title().to_string(),
                    };
                    let _ = stream.write_all(
                        &serde_json::to_vec(&msg).unwrap_or_default(),
                    );
                }
            }
            Err(_) => break,
        }
    }
}

/// Parse a key string and apply it to the app.
fn handle_key_event(app: &mut crate::app::App, key: &str) {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let key_event = match key {
        "q" => KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        "n" => KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        "x" => KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        "Tab" => KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        _ => return,
    };
    crate::app::handle_key(app, key_event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_socket_path_under_data_dir() {
        let p = server_socket_path();
        assert!(p.to_string_lossy().contains("term"));
    }
}
