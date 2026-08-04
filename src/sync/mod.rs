pub mod host;
pub mod join;
pub mod protocol;

use crate::config::Config;
use serde_json::Value;
use tokio::sync::mpsc;

pub struct StoredFile {
    pub name: String,
    pub lang: String,
    pub code: String,
}

#[allow(dead_code)]
pub enum Cmd {
    Send { event: String, payload: Value },
    SaveAll(Vec<StoredFile>),
    LoadAll,
    Shutdown,
}

#[allow(dead_code)]
pub enum Ev {
    Connected,
    Disconnected(String),
    Broadcast { event: String, payload: Value },
    Loaded { files: Vec<StoredFile> },
    SaveDone { count: usize },
    SaveErr(String),
    Err(String),
}

#[derive(Clone)]
pub struct SyncClient {
    pub tx: mpsc::UnboundedSender<Cmd>,
}

impl SyncClient {
    pub fn broadcast(&self, event: &str, payload: Value) {
        let _ = self.tx.send(Cmd::Send {
            event: event.to_string(),
            payload,
        });
    }

    pub fn save_all(&self, files: Vec<StoredFile>) {
        let _ = self.tx.send(Cmd::SaveAll(files));
    }

    pub fn load_all(&self) {
        let _ = self.tx.send(Cmd::LoadAll);
    }

    #[allow(dead_code)]
    pub fn shutdown(&self) {
        let _ = self.tx.send(Cmd::Shutdown);
    }
}

/// Start the LAN backend (host or join) once the mode is known.
pub fn spawn_backend(cfg: Config, ev_tx: mpsc::UnboundedSender<Ev>, cmd_rx: mpsc::UnboundedReceiver<Cmd>) {
    if cfg.is_host() {
        host::spawn(cfg, ev_tx, cmd_rx);
    } else {
        join::spawn(cfg, ev_tx, cmd_rx);
    }
}
