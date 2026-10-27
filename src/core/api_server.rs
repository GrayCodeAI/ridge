//! Agent-native socket API (Herdr-inspired).
//!
//! A Unix-domain socket server that accepts JSON commands from external
//! processes (agents, scripts, orchestrators). Commands:
//!   - list_panes    → list all panes with id, title, agent, status
//!   - spawn_agent   → spawn an agent in a pane
//!   - send_message  → write text to a pane's PTY
//!   - get_status    → get app/dashboard state as JSON
//!   - close_pane    → close a pane by id

use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::thread;

/// Socket path under the ridge data dir.
pub fn socket_path() -> std::path::PathBuf {
    crate::config::data_dir().join("ridge.sock")
}

/// Incoming API command.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd")]
enum ApiCommand {
    ListPanes,
    GetStatus,
    SpawnAgent { agent: String, cwd: Option<String> },
    SendMessage { pane: usize, message: String },
    ClosePane { pane: usize },
    GetOutput { pane: usize, cursor: usize },
}

/// Outgoing API response.
#[derive(Debug, Serialize)]
#[serde(tag = "ok", rename_all = "snake_case")]
enum ApiResponse {
    Ok { data: serde_json::Value },
    Err { message: String },
}

/// Handle a single command on a stream connection.
fn handle_command(app: &Arc<Mutex<crate::app::App>>, cmd: ApiCommand) -> ApiResponse {
    match cmd {
        ApiCommand::ListPanes => {
            let app = app.lock().unwrap();
            let panes: Vec<_> = app
                .panes
                .iter()
                .map(|p| {
                    json!({
                        "id": p.id,
                        "title": p.title,
                        "agent": p.agent_id,
                        "status": p.status.label(),
                        "cost_usd": p.cost_usd,
                    })
                })
                .collect();
            ApiResponse::Ok { data: json!(panes) }
        }
        ApiCommand::GetStatus => {
            let app = app.lock().unwrap();
            ApiResponse::Ok {
                data: serde_json::from_str(&app.dashboard_json()).unwrap_or_default(),
            }
        }
        ApiCommand::SpawnAgent { agent, cwd } => {
            let mut app = app.lock().unwrap();
            let cwd_path = cwd
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            app.spawn_agent(&agent, cwd_path, None);
            let pane_id = app.panes.last().map(|p| p.id).unwrap_or(0);
            ApiResponse::Ok {
                data: json!({ "pane_id": pane_id, "agent": agent }),
            }
        }
        ApiCommand::SendMessage { pane, message } => {
            let mut app = app.lock().unwrap();
            if let Some(pane) = app.panes.iter_mut().find(|p| p.id == pane) {
                if let Some(pty) = &mut pane.pty {
                    let _ = pty.write(message.as_bytes());
                    let _ = pty.write(b"\r");
                    ApiResponse::Ok {
                        data: json!({ "pane": pane.id, "sent": message.len() }),
                    }
                } else {
                    ApiResponse::Err {
                        message: format!("pane {} has no PTY", pane.id),
                    }
                }
            } else {
                ApiResponse::Err {
                    message: format!("pane {pane} not found"),
                }
            }
        }
        ApiCommand::ClosePane { pane } => {
            let mut app = app.lock().unwrap();
            app.close_pane_by_id(pane);
            ApiResponse::Ok {
                data: json!({ "pane": pane, "closed": true }),
            }
        }
        ApiCommand::GetOutput { pane, cursor } => {
            let app = app.lock().unwrap();
            if let Some(pane) = app.panes.iter().find(|p| p.id == pane) {
                if let Some(pty) = &pane.pty {
                    let (chunk, next_cursor) = pty.take_raw_since(cursor);
                    let output = String::from_utf8_lossy(&chunk).to_string();
                    ApiResponse::Ok {
                        data: json!({ "pane": pane.id, "output": output, "next_cursor": next_cursor }),
                    }
                } else {
                    ApiResponse::Err {
                        message: format!("pane {} has no PTY", pane.id),
                    }
                }
            } else {
                ApiResponse::Err {
                    message: format!("pane {} not found", pane),
                }
            }
        }
    }
}

/// Read one JSON command from a stream, respond with JSON.
#[cfg(unix)]
fn handle_stream(app: &Arc<Mutex<crate::app::App>>, mut stream: UnixStream) -> std::io::Result<()> {
    let mut buf = [0u8; 65536];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let cmd: Result<ApiCommand, _> = serde_json::from_slice(&buf[..n]);
    let response = match cmd {
        Ok(cmd) => handle_command(app, cmd),
        Err(e) => ApiResponse::Err {
            message: format!("parse error: {e}"),
        },
    };
    let resp_bytes = serde_json::to_vec(&response).unwrap_or_default();
    stream.write_all(&resp_bytes)?;
    Ok(())
}

/// Start the socket API server. Runs forever in the calling thread.
#[cfg(unix)]
pub fn start_server(app: Arc<Mutex<crate::app::App>>) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    let _ = std::fs::remove_file(&path); // remove stale socket
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    eprintln!("ridge api socket: {}", path.display());
    loop {
        let (stream, _) = listener.accept()?;
        let app = Arc::clone(&app);
        thread::spawn(move || {
            let _ = handle_stream(&app, stream);
        });
    }
}

/// Unix-domain sockets are not available on Windows; report the unsupported
/// flag instead of failing to compile.
#[cfg(not(unix))]
pub fn start_server(_app: Arc<Mutex<crate::app::App>>) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "ridge --api requires Unix-domain sockets (not available on Windows)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_under_data_dir() {
        let p = socket_path();
        assert!(p.to_string_lossy().contains("ridge"));
    }
}
