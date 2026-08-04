use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_PORT: u16 = 8245;

#[derive(Parser, Debug, Clone)]
#[command(name = "nexus", version, about = "Nexus Code — collaborative IDE TUI (LAN)")]
pub struct Args {
    /// Your display name
    #[arg(long)]
    pub name: Option<String>,
    /// Workspace name (used as the on-disk workspace directory on the host)
    #[arg(long)]
    pub room: Option<String>,
    /// Port to host the workspace on (default 8245). Host binds 0.0.0.0 so any
    /// device on the local network (any subnet) can join via the host's IP.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    /// Join an existing host instead of hosting: e.g. --connect 192.168.1.10:8245
    #[arg(long)]
    pub connect: Option<String>,
    /// Workspace directory to host (host mode). Its contents are scanned and
    /// synced to all users. Defaults to the room-based data dir.
    #[arg(long)]
    pub dir: Option<String>,
    /// Enable web-app compatibility broadcasts (whole-file events)
    #[arg(long)]
    pub webcompat: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub name: String,
    pub room: String,
    pub port: u16,
    /// Empty string = host a session; otherwise "host:port" of a device to join.
    pub connect: String,
    /// Host-mode workspace directory. Empty = default room-based data dir.
    pub dir: String,
    pub webcompat: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: String::new(),
            room: "main".into(),
            port: DEFAULT_PORT,
            connect: String::new(),
            dir: String::new(),
            webcompat: false,
        }
    }
}

impl Config {
    pub fn is_setup_complete(&self) -> bool {
        !self.name.is_empty()
    }

    /// True when this device hosts the session (binds the LAN port).
    pub fn is_host(&self) -> bool {
        self.connect.trim().is_empty()
    }

    /// True when this device joins a remote host.
    pub fn is_join(&self) -> bool {
        !self.is_host()
    }

    /// WebSocket URL used when joining a remote host.
    pub fn join_url(&self) -> String {
        let addr = self.connect.trim();
        if let Some(host) = addr.strip_prefix("ws://") {
            return format!("ws://{host}");
        }
        if addr.contains(':') {
            format!("ws://{addr}")
        } else {
            format!("ws://{addr}:{}", self.port)
        }
    }
}

pub fn config_path() -> PathBuf {
    let dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.join("nexus").join("config.json")
}

/// Directory where the host persists workspace files (git-ready plain files).
pub fn workspace_dir(room: &str) -> PathBuf {
    let dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.join("nexus").join("workspace").join(room_slug(room))
}

/// Directory this session treats as its workspace root: the host's chosen
/// directory when set, otherwise the room-based data dir.
pub fn host_dir(cfg: &Config) -> PathBuf {
    if !cfg.dir.trim().is_empty() {
        PathBuf::from(cfg.dir.trim())
    } else {
        workspace_dir(&cfg.room)
    }
}

pub fn room_slug(room: &str) -> String {
    room.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

pub fn load(args: &Args) -> Config {
    let mut cfg = Config::default();

    if let Ok(raw) = std::fs::read_to_string(config_path()) {
        if let Ok(saved) = serde_json::from_str::<Config>(&raw) {
            cfg = saved;
        }
    }

    if let Some(v) = &args.name {
        cfg.name = v.clone();
    }
    if let Some(v) = &args.room {
        cfg.room = v.clone();
    }
    if args.port != DEFAULT_PORT {
        cfg.port = args.port;
    }
    if let Some(v) = &args.connect {
        cfg.connect = v.clone();
    }
    if let Some(v) = &args.dir {
        cfg.dir = v.clone();
    }
    if args.webcompat {
        cfg.webcompat = true;
    }

    save(&cfg);
    cfg
}

pub fn save(cfg: &Config) {
    let path = config_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, json);
    }
}
