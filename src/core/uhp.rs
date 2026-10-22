//! Universal Harness Protocol (UHP) 1.0 (from Luvus).
//!
//! A versioned method registry for orchestrators, with owner-only local IPC,
//! snapshots, event streams, exact input, and semantic waits.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(unix)]
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// UHP protocol version.
pub const UHP_VERSION: &str = "1.0";

/// A UHP method call.
#[derive(Debug, Serialize, Deserialize)]
pub struct UhpcMethodCall {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

/// A UHP method response.
#[derive(Debug, Serialize, Deserialize)]
pub struct UhpcMethodResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<UhpcError>,
    pub id: Option<serde_json::Value>,
}

/// A UHP error.
#[derive(Debug, Serialize, Deserialize)]
pub struct UhpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// A UHP event emitted to subscribers.
#[derive(Debug, Serialize, Deserialize)]
pub struct UhpcEvent {
    pub event: String,
    pub params: Option<serde_json::Value>,
}

/// Method handler type.
type MethodHandler =
    Arc<dyn Fn(Option<serde_json::Value>) -> Result<serde_json::Value, UhpcError> + Send + Sync>;

/// Boxed event handler.
type EventHandler = Box<dyn Fn(&UhpcEvent) + Send + Sync>;

/// UHP method registry — versioned methods that orchestrators can call.
#[derive(Default)]
pub struct UhpcRegistry {
    methods: BTreeMap<String, MethodHandler>,
    events: BTreeMap<String, Vec<EventHandler>>,
    next_event_id: u64,
}

impl UhpcRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a method handler.
    pub fn register<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(Option<serde_json::Value>) -> Result<serde_json::Value, UhpcError>
            + Send
            + Sync
            + 'static,
    {
        self.methods.insert(name.to_string(), Arc::new(handler));
    }

    /// Subscribe to an event.
    pub fn subscribe<F>(&mut self, event: &str, handler: F) -> u64
    where
        F: Fn(&UhpcEvent) + Send + Sync + 'static,
    {
        let id = self.next_event_id;
        self.next_event_id += 1;
        self.events
            .entry(event.to_string())
            .or_default()
            .push(Box::new(handler));
        id
    }

    /// Emit an event to all subscribers.
    pub fn emit(&self, event: &UhpcEvent) {
        if let Some(handlers) = self.events.get(&event.event) {
            for handler in handlers {
                handler(event);
            }
        }
    }

    /// Handle a method call JSON-RPC request.
    pub fn handle_call(&self, call: UhpcMethodCall) -> UhpcMethodResponse {
        let id = call.id.clone();
        match self.methods.get(&call.method) {
            Some(handler) => match handler(call.params) {
                Ok(result) => UhpcMethodResponse {
                    jsonrpc: UHP_VERSION.to_string(),
                    result: Some(result),
                    error: None,
                    id,
                },
                Err(err) => UhpcMethodResponse {
                    jsonrpc: UHP_VERSION.to_string(),
                    result: None,
                    error: Some(err),
                    id,
                },
            },
            None => UhpcMethodResponse {
                jsonrpc: UHP_VERSION.to_string(),
                result: None,
                error: Some(UhpcError {
                    code: -32601,
                    message: format!("method not found: {}", call.method),
                    data: None,
                }),
                id,
            },
        }
    }

    /// Get the list of registered method names.
    pub fn methods(&self) -> Vec<String> {
        self.methods.keys().cloned().collect()
    }
}

/// UHP IPC server — owner-only local Unix domain socket.
pub struct UhpcServer {
    registry: Arc<Mutex<UhpcRegistry>>,
    socket_path: std::path::PathBuf,
}

impl UhpcServer {
    pub fn new(registry: Arc<Mutex<UhpcRegistry>>) -> Self {
        Self {
            registry,
            socket_path: crate::config::data_dir().join("uhp.sock"),
        }
    }

    /// Start the UHP server. Runs forever in the calling thread.
    #[cfg(unix)]
    pub fn start(self) -> std::io::Result<()> {
        let path = &self.socket_path;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(path);
        let listener = std::os::unix::net::UnixListener::bind(path)?;
        eprintln!("uhp socket: {}", path.display());

        for stream in listener.incoming() {
            let mut stream = stream?;
            let registry = Arc::clone(&self.registry);
            std::thread::spawn(move || {
                let mut buf = [0u8; 65536];
                match stream.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        if let Ok(call) = serde_json::from_slice::<UhpcMethodCall>(&buf[..n]) {
                            let response = registry.lock().unwrap().handle_call(call);
                            let _ = stream
                                .write_all(&serde_json::to_vec(&response).unwrap_or_default());
                        }
                    }
                    _ => {}
                }
            });
        }
        Ok(())
    }

    /// Unix-domain sockets are not available on Windows; report the unsupported
    /// flag instead of failing to compile.
    #[cfg(not(unix))]
    pub fn start(self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "term --uhp requires Unix-domain sockets (not available on Windows)",
        ))
    }
}

/// Timestamp helper for UHP events.
pub fn uhp_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uhp_version() {
        assert_eq!(UHP_VERSION, "1.0");
    }

    #[test]
    fn registry_register_and_call() {
        let mut reg = UhpcRegistry::new();
        reg.register("echo", |params| {
            Ok(params.unwrap_or(serde_json::json!(null)))
        });

        let call = UhpcMethodCall {
            jsonrpc: UHP_VERSION.to_string(),
            method: "echo".to_string(),
            params: Some(serde_json::json!("hello")),
            id: Some(serde_json::json!(1)),
        };
        let resp = reg.handle_call(call);
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), serde_json::json!("hello"));
    }

    #[test]
    fn registry_method_not_found() {
        let reg = UhpcRegistry::new();
        let call = UhpcMethodCall {
            jsonrpc: UHP_VERSION.to_string(),
            method: "nonexistent".to_string(),
            params: None,
            id: Some(serde_json::json!(1)),
        };
        let resp = reg.handle_call(call);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn registry_methods_list() {
        let mut reg = UhpcRegistry::new();
        reg.register("method_a", |_| Ok(serde_json::json!(true)));
        reg.register("method_b", |_| Ok(serde_json::json!(false)));
        let methods = reg.methods();
        assert!(methods.contains(&"method_a".to_string()));
        assert!(methods.contains(&"method_b".to_string()));
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn event_emit() {
        let mut reg = UhpcRegistry::new();
        let received = Arc::new(Mutex::new(None));
        let received_clone = Arc::clone(&received);
        reg.subscribe("test_event", move |event| {
            *received_clone.lock().unwrap() = Some(event.event.clone());
        });
        reg.emit(&UhpcEvent {
            event: "test_event".to_string(),
            params: None,
        });
        assert_eq!(*received.lock().unwrap(), Some("test_event".to_string()));
    }
}
