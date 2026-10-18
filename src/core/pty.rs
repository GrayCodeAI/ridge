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
}

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
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(mut p) = parser_clone.lock() {
                            p.process(&buf[..n]);
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
