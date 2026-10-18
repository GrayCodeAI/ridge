use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PtySession {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Mutex<Box<dyn Write + Send>>,
    pub parser: Arc<Mutex<vt100::Parser>>,
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub alive: Arc<std::sync::atomic::AtomicBool>,
    /// Raw session output (capped); used for timeline capture.
    pub raw_log: Arc<Mutex<Vec<u8>>>,
}

/// Cap for the raw session buffer (8 MiB). Older output is dropped.
pub const RAW_LOG_CAP: usize = 8 * 1024 * 1024;

impl PtySession {
    pub fn spawn(cmd: CommandBuilder, rows: u16, cols: u16) -> anyhow::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let child = pair.slave.spawn_command(cmd)?;
        // Drop slave so EOF propagates.
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));

        let parser_clone = Arc::clone(&parser);
        let alive_clone = Arc::clone(&alive);
        let raw_log = Arc::new(Mutex::new(Vec::new()));
        let raw_clone = Arc::clone(&raw_log);
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut p) = parser_clone.lock() {
                            p.process(&buf[..n]);
                        }
                        if let Ok(mut raw) = raw_clone.lock() {
                            raw.extend_from_slice(&buf[..n]);
                            if raw.len() > RAW_LOG_CAP {
                                let excess = raw.len() - RAW_LOG_CAP;
                                raw.drain(..excess);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            alive_clone.store(false, std::sync::atomic::Ordering::SeqCst);
        });

        Ok(Self {
            master: pair.master,
            writer: Mutex::new(writer),
            parser,
            child,
            alive,
            raw_log,
        })
    }

    pub fn write(&self, data: &[u8]) -> anyhow::Result<()> {
        let mut w = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("writer lock"))?;
        w.write_all(data)?;
        w.flush()?;
        Ok(())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Raw bytes since `cursor` (pane keeps its own cursor). Returns
    /// `(chunk, next_cursor)`; a stale cursor beyond the (front-truncated)
    /// buffer resumes at the current end, losing at most one gap.
    pub fn take_raw_since(&self, cursor: usize) -> (Vec<u8>, usize) {
        if let Ok(raw) = self.raw_log.lock() {
            let len = raw.len();
            let start = cursor.min(len);
            (raw[start..].to_vec(), len)
        } else {
            (Vec::new(), 0)
        }
    }

    pub fn screen_text(&self) -> String {
        if let Ok(p) = self.parser.lock() {
            let screen = p.screen();
            let mut out = String::new();
            for row in 0..screen.size().0 {
                for col in 0..screen.size().1 {
                    let cell = screen.cell(row, col);
                    if let Some(c) = cell {
                        out.push_str(&c.contents());
                    } else {
                        out.push(' ');
                    }
                }
                out.push('\n');
            }
            out
        } else {
            String::new()
        }
    }

    pub fn scrollback_tail(&self, n: usize) -> String {
        // vt100 parser keeps scrollback; fetch last n rows of screen for status heuristics.
        let text = self.screen_text();
        let lines: Vec<&str> = text.lines().collect();
        let start = lines.len().saturating_sub(n);
        lines[start..].join("\n")
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn echo_pty(msg: &str) -> PtySession {
        let mut cmd = CommandBuilder::new("echo");
        cmd.arg(msg);
        PtySession::spawn(cmd, 4, 80).expect("echo pty")
    }

    fn wait_for<F: Fn() -> bool>(pred: F) -> bool {
        for _ in 0..100 {
            if pred() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn raw_log_captures_output() {
        let pty = echo_pty("timeline-marker-a");
        let saw = wait_for(|| {
            pty.raw_log
                .lock()
                .map(|r| r.windows(17).any(|w| w == b"timeline-marker-a"))
                .unwrap_or(false)
        });
        assert!(saw, "raw log should contain echoed output");
    }

    #[test]
    fn take_raw_since_resumes() {
        let pty = echo_pty("timeline-marker-b");
        let ready = wait_for(|| !pty.take_raw_since(0).0.is_empty());
        assert!(ready, "output should arrive");

        let (chunk, cursor) = pty.take_raw_since(0);
        assert!(chunk.windows(17).any(|w| w == b"timeline-marker-b"));
        assert!(cursor > 0);

        // Draining again with the stored cursor yields nothing (no dup).
        let (rest, _next) = pty.take_raw_since(cursor);
        assert!(rest.is_empty());
    }
}
