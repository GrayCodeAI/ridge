use crossterm::event::{Event as CtEvent, KeyEvent, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,
    Quit,
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            loop {
                if crossterm::event::poll(tick_rate).unwrap_or(false) {
                    match crossterm::event::read() {
                        Ok(CtEvent::Key(k)) => {
                            if tx_clone.send(Event::Key(k)).is_err() {
                                break;
                            }
                        }
                        Ok(CtEvent::Mouse(m)) => {
                            let _ = tx_clone.send(Event::Mouse(m));
                        }
                        Ok(CtEvent::Resize(w, h)) => {
                            let _ = tx_clone.send(Event::Resize(w, h));
                        }
                        _ => {}
                    }
                } else {
                    let _ = tx_clone.send(Event::Tick);
                }
            }
        });
        Self { rx, _tx: tx }
    }

    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}
EOF