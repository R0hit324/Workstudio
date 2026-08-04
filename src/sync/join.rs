use crate::config::Config;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::protocol::{Wire, EV_LOAD_REQ, EV_PRESENCE};
use super::{Cmd, Ev, StoredFile};

pub fn spawn(cfg: Config, ev_tx: mpsc::UnboundedSender<Ev>, rx: mpsc::UnboundedReceiver<Cmd>) {
    tokio::spawn(async move {
        run(cfg, ev_tx, rx).await;
    });
}

async fn run(
    cfg: Config,
    ev_tx: mpsc::UnboundedSender<Ev>,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
) {
    let url = cfg.join_url();
    let mut backoff = Duration::from_millis(500);
    loop {
        match session(&url, &ev_tx, &mut cmd_rx).await {
            Ok(()) => backoff = Duration::from_millis(500),
            Err(e) => {
                let _ = ev_tx.send(Ev::Disconnected(e.to_string()));
                tokio::time::sleep(backoff).await;
                backoff = std::cmp::min(backoff * 2, Duration::from_secs(15));
            }
        }
    }
}

async fn session(
    url: &str,
    ev_tx: &mpsc::UnboundedSender<Ev>,
    cmd_rx: &mut mpsc::UnboundedReceiver<Cmd>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ws, _) = tokio_tungstenite::connect_async(url).await?;
    let (w, mut r) = ws.split();
    let _ = ev_tx.send(Ev::Connected);

    let reader_ev = ev_tx.clone();
    let reader = tokio::spawn(async move {
        while let Some(msg) = r.next().await {
            match msg {
                Ok(Message::Text(t)) => {
                    let text = t.to_string();
                    let wire: Wire = match serde_json::from_str(&text) {
                        Ok(w) => w,
                        Err(_) => continue,
                    };
                    match wire.ty.as_str() {
                        "welcome" => {
                            let files: Vec<StoredFile> = wire
                                .files
                                .unwrap_or_default()
                                .into_iter()
                                .map(|f| StoredFile {
                                    name: f.name,
                                    lang: f.lang,
                                    code: f.code,
                                    rev: f.rev,
                                })
                                .collect();
                            let _ = reader_ev.send(Ev::Loaded { files });
                            for m in wire.members.unwrap_or_default() {
                                if let Ok(payload) = serde_json::to_value(m) {
                                    let _ = reader_ev.send(Ev::Broadcast {
                                        event: EV_PRESENCE.to_string(),
                                        payload,
                                    });
                                }
                            }
                        }
                        "evt" => {
                            if let (Some(event), Some(payload)) = (wire.event, wire.payload) {
                                let _ = reader_ev.send(Ev::Broadcast { event, payload });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }
    });

    let mut writer = w;
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            Cmd::Send { event, payload } => {
                let wire = json!({"ty": "evt", "event": event, "payload": payload});
                if writer
                    .send(Message::Text(wire.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Cmd::SaveAll(files) => {
                // The host persists automatically; this is just a local ack.
                let _ = ev_tx.send(Ev::SaveDone { count: files.len() });
            }
            Cmd::LoadAll => {
                let wire = json!({"ty": "evt", "event": EV_LOAD_REQ, "payload": Value::Null});
                if writer
                    .send(Message::Text(wire.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Cmd::Shutdown => break,
        }
    }
    reader.abort();
    Ok(())
}
