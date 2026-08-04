use crate::config::Config;
use crate::languages;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::protocol::{
    LinePatch, PatchMsg, Presence, SnapshotMsg, SnapReqMsg, WelcomeFile, Wire, EV_LOAD_REQ,
    EV_PATCH, EV_PRESENCE, EV_SNAPSHOT, EV_SNAP_REQ,
};
use super::{Cmd, Ev, StoredFile};

static NEXT_PEER: AtomicU64 = AtomicU64::new(1);

pub fn spawn(cfg: Config, ev_tx: mpsc::UnboundedSender<Ev>, rx: mpsc::UnboundedReceiver<Cmd>) {
    tokio::spawn(async move {
        if let Err(e) = Hub::new(cfg, ev_tx).run(rx).await {
            eprintln!("nexus host: {e}");
        }
    });
}

enum Incoming {
    Registered {
        peer: u64,
        out: mpsc::UnboundedSender<String>,
    },
    Msg {
        peer: u64,
        event: String,
        payload: Value,
    },
    Closed {
        peer: u64,
    },
}

struct FileState {
    lang: String,
    lines: Vec<String>,
    rev: u64,
}

struct Hub {
    cfg: Config,
    ev_tx: mpsc::UnboundedSender<Ev>,
    dir: PathBuf,
    store: BTreeMap<String, FileState>,
    members: BTreeMap<String, Presence>,
    peers: BTreeMap<u64, mpsc::UnboundedSender<String>>,
    dirty: bool,
    last_save: Instant,
}

impl Hub {
    fn new(cfg: Config, ev_tx: mpsc::UnboundedSender<Ev>) -> Self {
        let dir = crate::config::host_dir(&cfg);
        Self {
            cfg,
            ev_tx,
            dir,
            store: BTreeMap::new(),
            members: BTreeMap::new(),
            peers: BTreeMap::new(),
            dirty: false,
            last_save: Instant::now(),
        }
    }

    async fn run(
        mut self,
        mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(("0.0.0.0", self.cfg.port)).await.map_err(|e| {
            format!(
                "failed to bind host port {}: {e} (is another session running?)",
                self.cfg.port
            )
        })?;
        let _ = self.ev_tx.send(Ev::Connected);

        let (inc_tx, mut inc_rx) = mpsc::unbounded_channel::<Incoming>();
        tokio::spawn(accept_loop(listener, inc_tx));

        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        loop {
            tokio::select! {
                maybe = cmd_rx.recv() => {
                    match maybe {
                        Some(cmd) => self.handle_cmd(cmd),
                        None => break,
                    }
                }
                maybe = inc_rx.recv() => {
                    match maybe {
                        Some(Incoming::Registered { peer, out }) => self.register(peer, out),
                        Some(Incoming::Msg { peer, event, payload }) => self.handle_peer(peer, &event, payload),
                        Some(Incoming::Closed { peer }) => {
                            self.peers.remove(&peer);
                        }
                        None => break,
                    }
                }
                _ = ticker.tick() => {
                    if self.dirty && self.last_save.elapsed() > Duration::from_secs(2) {
                        self.persist();
                    }
                }
            }
        }
        self.persist();
        Ok(())
    }

    // ── local app commands (the host's own editor) ──

    fn handle_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Send { event, payload } => {
                self.track(&event, &payload);
                if event == EV_LOAD_REQ {
                    return;
                }
                let wire = Wire::evt(&event, payload);
                self.broadcast(&wire, None);
                if event == EV_SNAP_REQ {
                    if let Ok(req) =
                        serde_json::from_value::<SnapReqMsg>(wire.payload.clone().unwrap_or_default())
                    {
                        if let Some(snap) = self.build_snapshot(&req.file) {
                            let w = Wire::evt(EV_SNAPSHOT, snap);
                            self.broadcast(&w, None);
                        }
                    }
                }
            }
            Cmd::SaveAll(files) => {
                // Refresh store from what the app reported, then write to disk.
                for f in files {
                    let state = self.store.entry(f.name).or_insert_with(|| FileState {
                        lang: f.lang.clone(),
                        lines: Vec::new(),
                        rev: 0,
                    });
                    state.lang = f.lang;
                    state.lines = f.code.split('\n').map(|s| s.to_string()).collect();
                    state.rev += 1;
                }
                self.persist();
                // The host owns the workspace repo: commit as the session owner.
                if crate::git::is_repo(&self.dir) {
                    let author = if self.cfg.name.trim().is_empty() {
                        "host".to_string()
                    } else {
                        self.cfg.name.trim().to_string()
                    };
                    let msg = format!("nexus: save {}", self.cfg.room);
                    if let Err(e) = crate::git::commit(&self.dir, &msg, &author) {
                        eprintln!("nexus host: git commit: {e}");
                    }
                }
                let _ = self
                    .ev_tx
                    .send(Ev::SaveDone { count: self.store.len() });
            }
            Cmd::LoadAll => {
                let files = self.load_from_disk();
                let _ = self.ev_tx.send(Ev::Loaded { files });
            }
            Cmd::Shutdown => {
                self.persist();
            }
        }
    }

    // ── remote peer messages ──

    fn handle_peer(&mut self, peer: u64, event: &str, payload: Value) {
        if event == EV_LOAD_REQ {
            let wire = self.welcome();
            self.send_to_peer(peer, &wire);
            return;
        }
        self.track(event, &payload);
        let wire = Wire::evt(event, payload);
        // forward to other remote peers and to the local host editor
        self.broadcast(&wire, Some(peer));
        let _ = self
            .ev_tx
            .send(Ev::Broadcast { event: event.to_string(), payload: wire.payload.clone().unwrap_or(Value::Null) });
        // authoritative answer so joiners always get content even if no peer responds
        if event == EV_SNAP_REQ {
            if let Ok(req) = serde_json::from_value::<SnapReqMsg>(wire.payload.clone().unwrap_or_default()) {
                if let Some(snap) = self.build_snapshot(&req.file) {
                    let w = Wire::evt(EV_SNAPSHOT, snap);
                    self.broadcast(&w, Some(peer));
                }
            }
        }
    }

    fn register(&mut self, peer: u64, out: mpsc::UnboundedSender<String>) {
        self.peers.insert(peer, out);
        let wire = self.welcome();
        self.send_to_peer(peer, &wire);
    }

    // ── state tracking (authoritative store for persistence + welcome) ──

    fn track(&mut self, event: &str, payload: &Value) {
        match event {
            EV_PATCH => {
                if let Ok(m) = serde_json::from_value::<PatchMsg>(payload.clone()) {
                    let state = self
                        .store
                        .entry(m.file)
                        .or_insert_with(|| FileState {
                            lang: m.lang.clone(),
                            lines: Vec::new(),
                            rev: 0,
                        });
                    state.lang = m.lang;
                    apply_patches(&mut state.lines, &m.patches);
                    state.rev = state.rev.saturating_add(1);
                    self.dirty = true;
                }
            }
            EV_SNAPSHOT => {
                if let Ok(m) = serde_json::from_value::<SnapshotMsg>(payload.clone()) {
                    let state = self
                        .store
                        .entry(m.file)
                        .or_insert_with(|| FileState {
                            lang: m.lang.clone(),
                            lines: Vec::new(),
                            rev: 0,
                        });
                    state.lang = m.lang;
                    state.lines = m.lines;
                    if m.rev > state.rev {
                        state.rev = m.rev;
                    }
                    self.dirty = true;
                }
            }
            EV_PRESENCE => {
                if let Ok(p) = serde_json::from_value::<Presence>(payload.clone()) {
                    self.members.insert(p.id.clone(), p);
                }
            }
            _ => {}
        }
    }

    // ── persistence ──

    fn persist(&mut self) {
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            eprintln!("nexus host: create workspace dir: {e}");
            return;
        }
        for (name, st) in &self.store {
            if let Some(safe) = safe_name(name) {
                let path = self.dir.join(safe);
                if let Err(e) = std::fs::write(&path, st.lines.join("\n")) {
                    eprintln!("nexus host: write {name}: {e}");
                }
            }
        }
        self.dirty = false;
        self.last_save = Instant::now();
    }

    fn load_from_disk(&mut self) -> Vec<StoredFile> {
        let mut files = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            let mut names: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| {
                    !n.starts_with('.') && n != ".git" && !n.ends_with(".git")
                })
                .collect();
            names.sort();
            for name in names {
                let p = self.dir.join(&name);
                if !p.is_file() {
                    continue;
                }
                let Ok(code) = std::fs::read_to_string(&p) else {
                    continue;
                };
                if code.contains('\0') {
                    continue;
                }
                let lang = languages::guess_lang(&name).id.to_string();
                let state = FileState {
                    lang: lang.clone(),
                    lines: code.split('\n').map(|s| s.to_string()).collect(),
                    rev: 0,
                };
                self.store.insert(name.clone(), state);
                files.push(StoredFile { name, lang, code });
            }
        }
        files
    }

    // ── welcome / snapshot construction ──

    fn welcome(&self) -> Wire {
        let files: Vec<WelcomeFile> = self
            .store
            .iter()
            .map(|(name, st)| WelcomeFile {
                name: name.clone(),
                lang: st.lang.clone(),
                code: st.lines.join("\n"),
            })
            .collect();
        let members: Vec<Presence> = self.members.values().cloned().collect();
        Wire::welcome(files, members)
    }

    fn build_snapshot(&self, file: &str) -> Option<Value> {
        let st = self.store.get(file)?;
        Some(json!(SnapshotMsg {
            id: "host".to_string(),
            file: file.to_string(),
            lang: st.lang.clone(),
            rev: st.rev,
            lines: st.lines.clone(),
        }))
    }

    // ── outbound ──

    fn send_to_peer(&self, peer: u64, wire: &Wire) {
        if let Some(out) = self.peers.get(&peer) {
            if let Ok(s) = serde_json::to_string(wire) {
                let _ = out.send(s);
            }
        }
    }

    fn broadcast(&self, wire: &Wire, except: Option<u64>) {
        let payload = match serde_json::to_string(wire) {
            Ok(p) => p,
            Err(_) => return,
        };
        for (&id, out) in &self.peers {
            if Some(id) != except {
                let _ = out.send(payload.clone());
            }
        }
    }
}

async fn accept_loop(listener: TcpListener, inc_tx: mpsc::UnboundedSender<Incoming>) {
    while let Ok((stream, _)) = listener.accept().await {
        let inc_tx = inc_tx.clone();
        tokio::spawn(async move {
            let _ = handle_conn(stream, inc_tx).await;
        });
    }
}

async fn handle_conn(
    stream: TcpStream,
    inc_tx: mpsc::UnboundedSender<Incoming>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws: WebSocketStream<TcpStream> = tokio_tungstenite::accept_async(stream).await?;
    let (mut w, mut r) = ws.split();

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let peer = NEXT_PEER.fetch_add(1, Ordering::Relaxed);
    let _ = inc_tx.send(Incoming::Registered {
        peer,
        out: out_tx.clone(),
    });

    loop {
        tokio::select! {
            maybe_out = out_rx.recv() => {
                match maybe_out {
                    Some(s) => {
                        if w.send(Message::Text(s.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            maybe_msg = r.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(t))) => {
                        let text = t.to_string();
                        let wire: Wire = match serde_json::from_str(&text) {
                            Ok(w) => w,
                            Err(_) => continue,
                        };
                        if wire.ty == "evt" {
                            if let (Some(event), Some(payload)) = (wire.event, wire.payload) {
                                let _ = inc_tx.send(Incoming::Msg {
                                    peer,
                                    event,
                                    payload,
                                });
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Ping(p))) => {
                        let _ = w.send(Message::Pong(p)).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                    None => break,
                }
            }
        }
    }

    let _ = inc_tx.send(Incoming::Closed { peer });
    Ok(())
}

fn apply_patches(lines: &mut Vec<String>, patches: &[LinePatch]) {
    for p in patches {
        let start = p.start.min(lines.len());
        let end = (start + p.remove).min(lines.len());
        let mut new_lines = Vec::with_capacity(lines.len() - (end - start) + p.lines.len());
        new_lines.extend_from_slice(&lines[..start]);
        new_lines.extend_from_slice(&p.lines);
        new_lines.extend_from_slice(&lines[end..]);
        *lines = new_lines;
    }
}

fn safe_name(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() || n == "." || n == ".." || n.contains('/') || n.contains('\\') {
        return None;
    }
    Some(n.to_string())
}
