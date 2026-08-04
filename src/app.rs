use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::Color;
use serde_json::json;
use tui_textarea::{CursorMove, TextArea};

use crate::config::Config;
use crate::editor::{completions, highlight::Highlighter};
use crate::languages::{self, Language};
use crate::runner::{self, OutputKind, RunnerEvent};
use crate::sync::protocol::*;
use crate::sync::{Ev, SyncClient};
use crate::theme::{self, Theme};
use crate::util::{self, color_from_hex, now_ms, to_hex};

pub enum Phase {
    Setup,
    Editing,
}

pub struct SetupForm {
    pub name: TextArea<'static>,
    pub room: TextArea<'static>,
    /// true = join an existing device; false = host this session
    pub join: bool,
    pub addr: TextArea<'static>,
    pub dir: TextArea<'static>,
    pub focus: usize,
    pub error: Option<String>,
}

impl SetupForm {
    fn new(cfg: &Config) -> Self {
        let mut name = TextArea::default();
        name.insert_str(cfg.name.clone());
        let mut room = TextArea::default();
        room.insert_str(if cfg.room.is_empty() { "main".into() } else { cfg.room.clone() });
        let mut addr = TextArea::default();
        addr.insert_str(cfg.connect.clone());
        let mut dir = TextArea::default();
        if cfg.dir.trim().is_empty() {
            if let Some(d) = dirs::home_dir() {
                dir.insert_str(d.display().to_string());
            }
        } else {
            dir.insert_str(cfg.dir.clone());
        }
        Self {
            name,
            room,
            join: cfg.is_join(),
            addr,
            dir,
            focus: 0,
            error: None,
        }
    }

    pub fn total_focus(&self) -> usize {
        5 // name, room, mode, dir|addr, launch
    }
}

pub enum Modal {
    None,
    Invite,
    Help,
    ConfirmClose(String),
    GitLog,
}

pub struct EditorFile {
    pub lang: &'static Language,
    pub ta: TextArea<'static>,
    pub rev: u64,
    pub base_rev: u64,
    pub dirty: bool,
    pub prev_lines: Vec<String>,
    pub cursors: BTreeMap<String, (usize, usize)>,
}

pub struct Member {
    pub name: String,
    pub color: Color,
    pub ts: i64,
}

pub struct AcState {
    pub word: String,
    pub index: usize,
    pub items: Vec<completions::Completion>,
}

#[derive(Default, Clone, Copy)]
pub struct LayoutState {
    pub editor_content: Rect,
    pub gutter_width: u16,
    pub output_area: Rect,
    pub sidebar: Rect,
    pub total: Rect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Idle,
    Running,
    Ok,
    Err,
}

pub struct App {
    pub cfg: Config,
    pub dark: bool,
    pub phase: Phase,
    pub setup: SetupForm,
    pub files: BTreeMap<String, EditorFile>,
    pub active_file: Option<String>,
    pub members: BTreeMap<String, Member>,
    pub output: Vec<(String, OutputKind)>,
    pub output_scroll: usize,
    pub output_height: u16,
    pub modal: Modal,
    pub toast: Option<(String, Instant)>,
    pub sync: Option<SyncClient>,
    /// Backend channels waiting for the user to pick host/join on the setup screen.
    pub sync_pending: Option<(
        tokio::sync::mpsc::UnboundedSender<super::sync::Ev>,
        tokio::sync::mpsc::UnboundedReceiver<super::sync::Cmd>,
    )>,
    pub self_id: String,
    pub my_color: Color,
    pub connected: bool,
    pub conn_label: String,
    pub run_status: RunStatus,
    pub last_saved: String,
    pub hl: Highlighter,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub layout: LayoutState,
    pub ac: Option<AcState>,
    pub last_cursor_pos: (usize, usize),
    pub last_cursor_sent: Option<(usize, usize)>,
    pub last_presence: Instant,
    pub last_auto_save: Instant,
    pub last_snap_req: HashSet<String>,
    pub runner_tx: tokio::sync::mpsc::UnboundedSender<RunnerEvent>,
    pub file_counter: u64,
    pub git_branch: String,
    pub git_commits: usize,
    pub git_log: Vec<String>,
}

impl App {
    pub fn new(
        cfg: Config,
        sync: Option<SyncClient>,
        runner_tx: tokio::sync::mpsc::UnboundedSender<RunnerEvent>,
    ) -> Self {
        let self_id = util::random_id();
        let my_color = member_color(&cfg.name);
        let setup = SetupForm::new(&cfg);
        let phase = if cfg.is_setup_complete() {
            Phase::Editing
        } else {
            Phase::Setup
        };
        let conn_label = if cfg.is_host() {
            "Hosting on this device".into()
        } else {
            "Not connected".into()
        };
        let mut app = Self {
            cfg,
            dark: true,
            phase,
            setup,
            files: BTreeMap::new(),
            active_file: None,
            members: BTreeMap::new(),
            output: vec![("Press F5 to run your code".into(), OutputKind::Info)],
            output_scroll: 0,
            output_height: 8,
            modal: Modal::None,
            toast: None,
            sync,
            sync_pending: None,
            self_id,
            my_color,
            connected: false,
            conn_label,
            run_status: RunStatus::Idle,
            last_saved: "Unsaved".into(),
            hl: Highlighter::new(),
            scroll_row: 0,
            scroll_col: 0,
            layout: LayoutState::default(),
            ac: None,
            last_cursor_pos: (0, 0),
            last_cursor_sent: None,
            last_presence: Instant::now(),
            last_auto_save: Instant::now(),
            last_snap_req: HashSet::new(),
            runner_tx,
            file_counter: 0,
            git_branch: String::new(),
            git_commits: 0,
            git_log: Vec::new(),
        };
        if let Phase::Editing = app.phase {
            let hello = languages::default_lang().hello.to_string();
            app.create_file_internal("main.py", "python", &hello, true);
            app.request_load();
            if app.cfg.is_host() {
                app.refresh_git();
            }
        }
        app
    }

    pub fn theme(&self) -> &'static Theme {
        if self.dark {
            &theme::DARK
        } else {
            &theme::LIGHT
        }
    }

    // ── FILE MANAGEMENT ──

    pub fn create_file(&mut self, lang_id: &str) -> String {
        self.file_counter += 1;
        let lang = languages::by_id(lang_id).unwrap_or_else(languages::default_lang);
        let name = format!("file{}{}", self.file_counter, lang.ext);
        self.create_file_internal(&name, lang_id, lang.hello, false);
        name
    }

    pub fn create_file_internal(&mut self, name: &str, lang_id: &str, code: &str, silent: bool) {
        let lang = languages::by_id(lang_id)
            .or_else(|| Some(languages::guess_lang(name)))
            .unwrap_or_else(languages::default_lang);
        let lines: Vec<String> = code.split('\n').map(|s| s.to_string()).collect();
        let mut ta = TextArea::from(lines);
        ta.set_tab_length(2);
        ta.set_max_histories(100);
        let prev_lines = ta.lines().to_vec();
        self.files.insert(
            name.to_string(),
            EditorFile {
                lang,
                ta,
                rev: 0,
                base_rev: 0,
                dirty: !silent,
                prev_lines,
                cursors: BTreeMap::new(),
            },
        );
        self.set_active(name);
        self.send_file_msg(name, true);
        if !silent {
            self.toast = Some((format!("📄 Created {name}"), Instant::now()));
        }
    }

    pub fn set_active(&mut self, name: &str) {
        self.active_file = Some(name.to_string());
        self.ac = None;
        self.scroll_row = 0;
        self.scroll_col = 0;
        self.send_cursor_now();
    }

    pub fn close_file(&mut self, name: &str) {
        if !self.files.contains_key(name) {
            return;
        }
        self.files.remove(name);
        self.send_file_msg(name, false);
        if self.files.is_empty() {
            self.create_file_internal("main.py", "python", languages::default_lang().hello, true);
        } else if self.active_file.as_deref() == Some(name) {
            let first = self.files.keys().next().cloned().unwrap();
            self.set_active(&first);
        }
    }

    #[allow(dead_code)]
    pub fn rename_lang(&mut self, name: &str, lang_id: &str) {
        if let Some(f) = self.files.get_mut(name) {
            if let Some(l) = languages::by_id(lang_id) {
                f.lang = l;
                f.dirty = true;
            }
        }
    }

    // ── SYNC HELPERS ──

    pub fn request_load(&mut self) {
        if let Some(s) = &self.sync {
            s.load_all();
        }
    }

    pub fn save_all(&mut self) {
        match &self.sync {
            Some(s) => {
                let files: Vec<crate::sync::StoredFile> = self
                    .files
                    .iter()
                    .map(|(name, f)| crate::sync::StoredFile {
                        name: name.clone(),
                        lang: f.lang.id.to_string(),
                        code: f.ta.lines().join("\n"),
                    })
                    .collect();
                s.save_all(files);
                self.last_auto_save = Instant::now();
            }
            None => {
                self.last_saved = "Not connected — edits stay on this device".into();
                self.toast = Some(("Not connected — edits stay local".into(), Instant::now()));
            }
        }
    }

    /// Refresh git state for the host's workspace directory (branch, commit count).
    /// Joiners have no local repo to read, so this is host-only.
    pub fn refresh_git(&mut self) {
        if !self.cfg.is_host() {
            return;
        }
        let dir = crate::config::host_dir(&self.cfg);
        if crate::git::is_repo(&dir) {
            self.git_branch = crate::git::branch(&dir);
            self.git_commits = crate::git::commit_count(&dir);
        } else {
            self.git_branch.clear();
            self.git_commits = 0;
        }
    }

    pub fn open_git_log(&mut self) {
        self.git_log.clear();
        if self.cfg.is_host() {
            let dir = crate::config::host_dir(&self.cfg);
            self.git_log = crate::git::log(&dir, 25);
        }
        self.modal = Modal::GitLog;
    }

    pub fn send_presence(&mut self) {
        let payload = json!({
            "id": self.self_id,
            "name": self.cfg.name,
            "color": to_hex(self.my_color),
            "ts": now_ms(),
        });
        if let Some(s) = &self.sync {
            s.broadcast(EV_PRESENCE, payload.clone());
            if self.cfg.webcompat {
                s.broadcast(EV_WEB_PRESENCE, payload);
            }
        }
    }

    pub fn send_file_msg(&mut self, name: &str, open: bool) {
        if let Some(f) = self.files.get(name) {
            if let Some(s) = &self.sync {
                s.broadcast(
                    EV_FILE,
                    json!({
                        "id": self.self_id,
                        "name": name,
                        "lang": f.lang.id,
                        "open": open,
                    }),
                );
            }
        }
    }

    pub fn send_cursor_now(&mut self) {
        if self.active_file.is_none() {
            return;
        }
        if let Some(s) = &self.sync {
            let f = self.files.get(self.active_file.as_ref().unwrap()).unwrap();
            let (row, col) = f.ta.cursor();
            let msg = CursorMsg {
                id: self.self_id.clone(),
                name: self.cfg.name.clone(),
                color: to_hex(self.my_color),
                file: self.active_file.clone().unwrap(),
                line: row,
                col,
            };
            s.broadcast(EV_CURSOR, json!(msg));
            self.last_cursor_sent = Some((row, col));
        }
    }

    pub fn broadcast_edit(&mut self) {
        let name = match self.active_file.clone() {
            Some(n) => n,
            None => return,
        };
        let f = self.files.get_mut(&name).unwrap();
        f.rev += 1;
        f.base_rev = f.rev;
        f.dirty = true;
        let old = f.prev_lines.clone();
        let new = f.ta.lines().to_vec();
        let patches = compute_line_patches(&old, &new);
        if patches.is_empty() {
            return;
        }
        let msg = PatchMsg {
            id: self.self_id.clone(),
            file: name.clone(),
            lang: f.lang.id.to_string(),
            base_rev: f.rev,
            patches,
        };
        if let Some(s) = &self.sync {
            s.broadcast(EV_PATCH, json!(msg));
            if self.cfg.webcompat {
                s.broadcast(
                    EV_WEB_CODE,
                    json!({
                        "author": self.cfg.name,
                        "file": name,
                        "code": new.join("\n"),
                    }),
                );
            }
        }
        f.prev_lines = new;
    }

    pub fn send_snapshot(&mut self, name: &str) {
        if let Some(f) = self.files.get(name) {
            if let Some(s) = &self.sync {
                let msg = SnapshotMsg {
                    id: self.self_id.clone(),
                    file: name.to_string(),
                    lang: f.lang.id.to_string(),
                    rev: f.rev,
                    lines: f.ta.lines().to_vec(),
                };
                s.broadcast(EV_SNAPSHOT, json!(msg));
            }
        }
    }

    pub fn request_snapshot(&mut self, name: &str) {
        if self.last_snap_req.contains(name) {
            return;
        }
        self.last_snap_req.insert(name.to_string());
        if let Some(s) = &self.sync {
            let msg = SnapReqMsg {
                id: self.self_id.clone(),
                file: name.to_string(),
            };
            s.broadcast(EV_SNAP_REQ, json!(msg));
        }
    }

    // ── EVENT HANDLING ──

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match self.phase {
            Phase::Setup => self.setup_key(key),
            Phase::Editing => self.editing_key(key),
        }
    }

    fn setup_key(&mut self, key: KeyEvent) {
        let total = self.setup.total_focus();
        match key.code {
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                let launch = total - 1;
                if key.code == KeyCode::Enter && self.setup.focus == launch {
                    self.finish_setup();
                    return;
                }
                self.setup.focus = (self.setup.focus + 1) % total;
                return;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.setup.focus = (self.setup.focus + total - 1) % total;
                return;
            }
            KeyCode::Char(' ') if self.setup.focus == 2 => {
                self.setup.join = !self.setup.join;
                self.setup.error = None;
                return;
            }
            _ => {}
        }
        let target = match self.setup.focus {
            0 => &mut self.setup.name,
            1 => &mut self.setup.room,
            3 if self.setup.join => &mut self.setup.addr,
            3 => &mut self.setup.dir,
            _ => return,
        };
        if target.input(key) {
            self.setup.error = None;
        }
    }

    fn finish_setup(&mut self) {
        let name = self.setup.name.lines().join("").trim().to_string();
        if name.is_empty() {
            self.setup.error = Some("Please enter your name.".into());
            return;
        }
        let room = self.setup.room.lines().join("").trim().to_string();
        self.cfg.name = name;
        self.cfg.room = if room.is_empty() { "main".into() } else { room };
        if self.setup.join {
            let addr = self.setup.addr.lines().join("").trim().to_string();
            if addr.is_empty() {
                self.setup.error =
                    Some("Enter the host address to join (e.g. 192.168.1.10:8245).".into());
                return;
            }
            self.cfg.connect = addr;
            self.conn_label = format!("Joining {}...", self.cfg.connect);
        } else {
            self.cfg.connect.clear();
            let dir = self.setup.dir.lines().join("").trim().to_string();
            if dir.is_empty() {
                self.setup.error =
                    Some("Enter a directory to host (its contents become the workspace).".into());
                return;
            }
            self.cfg.dir = dir;
            let ip = crate::util::local_ip();
            self.conn_label = if ip.is_empty() {
                format!("Hosting on port {}", self.cfg.port)
            } else {
                format!("Hosting · join via {}:{}", ip, self.cfg.port)
            };
        }
        self.my_color = member_color(&self.cfg.name);
        crate::config::save(&self.cfg);
        // Start the LAN backend with the chosen mode (host or join).
        if let Some((ev_tx, cmd_rx)) = self.sync_pending.take() {
            crate::sync::spawn_backend(self.cfg.clone(), ev_tx, cmd_rx);
        }
        self.phase = Phase::Editing;
        let hello = languages::default_lang().hello.to_string();
        self.create_file_internal("main.py", "python", &hello, true);
        self.request_load();
        if self.cfg.is_host() {
            self.refresh_git();
        }
    }

    fn editing_key(&mut self, key: KeyEvent) {
        // Global shortcuts first
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Up => {
                    self.output_height = (self.output_height + 1).min(30);
                    return;
                }
                KeyCode::Down => {
                    self.output_height = self.output_height.saturating_sub(1).max(3);
                    return;
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    if self.connected {
                        self.save_all();
                    }
                    std::process::exit(0);
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.save_all();
                    return;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    let lang = self.current_lang_id();
                    self.create_file(lang);
                    return;
                }
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    self.dark = !self.dark;
                    return;
                }
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    self.format_code();
                    return;
                }
                KeyCode::Char('g') | KeyCode::Char('G') => {
                    self.modal = Modal::Invite;
                    return;
                }
                KeyCode::Char('j') | KeyCode::Char('J') => {
                    self.open_git_log();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Esc => {
                if !matches!(self.modal, Modal::None) {
                    self.modal = Modal::None;
                    return;
                }
                if self.ac.is_some() {
                    self.ac = None;
                    return;
                }
                return;
            }
            KeyCode::F(1) => {
                self.modal = Modal::Help;
                return;
            }
            KeyCode::F(4) => {
                if let Some(name) = self.active_file.clone() {
                    self.modal = Modal::ConfirmClose(name);
                }
                return;
            }
            KeyCode::F(5) => {
                self.run_code();
                return;
            }
            KeyCode::F(6) => {
                self.dark = !self.dark;
                return;
            }
            KeyCode::Char('?') => {
                self.modal = Modal::Help;
                return;
            }
            _ => {}
        }

        // Modal routing
        if !matches!(self.modal, Modal::None) {
            self.modal_key(key);
            return;
        }

        // Autocomplete navigation
        if self.ac.is_some() {
            match key.code {
                KeyCode::Down => {
                    self.ac.as_mut().unwrap().index =
                        (self.ac.as_ref().unwrap().index + 1).min(self.ac.as_ref().unwrap().items.len() - 1);
                    return;
                }
                KeyCode::Up => {
                    let idx = self.ac.as_ref().unwrap().index;
                    self.ac.as_mut().unwrap().index = idx.saturating_sub(1);
                    return;
                }
                KeyCode::Tab | KeyCode::Enter => {
                    let word = self.ac.as_ref().unwrap().items[self.ac.as_ref().unwrap().index].word.clone();
                    self.apply_completion(word);
                    return;
                }
                _ => {}
            }
        }

        // Editor input
        if self.active_file.is_none() {
            return;
        }
        let name = self.active_file.clone().unwrap();
        let (before_row, before_col) = self.files[&name].ta.cursor();
        let modified = self.files.get_mut(&name).unwrap().ta.input(key);
        if modified {
            self.broadcast_edit();
            self.ac = None;
        }
        let (row, col) = self.files[&name].ta.cursor();
        if (row, col) != (before_row, before_col) {
            self.refresh_completions();
            self.throttle_cursor((row, col));
        }
    }

    fn modal_key(&mut self, key: KeyEvent) {
        match &self.modal {
            Modal::ConfirmClose(name) => {
                let name = name.clone();
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        self.close_file(&name);
                        self.modal = Modal::None;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.modal = Modal::None,
                    _ => {}
                }
            }
            _ => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('c') | KeyCode::Char('q')) {
                    self.modal = Modal::None;
                }
            }
        }
    }

    // ── MOUSE ──

    pub fn on_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        let layout = self.layout;
        match kind {
            MouseEventKind::ScrollDown => {
                if layout.output_area.height > 0
                    && row >= layout.output_area.y
                    && row < layout.output_area.y + layout.output_area.height
                {
                    self.output_scroll = self.output_scroll.saturating_add(2);
                }
            }
            MouseEventKind::ScrollUp => {
                if layout.output_area.height > 0
                    && row >= layout.output_area.y
                    && row < layout.output_area.y + layout.output_area.height
                {
                    self.output_scroll = self.output_scroll.saturating_sub(2);
                }
            }
            MouseEventKind::Down(_) => {
                if self.active_file.is_none() {
                    return;
                }
                let ec = layout.editor_content;
                if col >= ec.x && col < ec.x + ec.width && row >= ec.y && row < ec.y + ec.height {
                    let gutter = layout.gutter_width as usize;
                    let text_col = col.saturating_sub(ec.x) as usize + gutter + self.scroll_col;
                    let text_row = (row - ec.y) as usize + self.scroll_row;
                    self.set_cursor(text_row, text_col);
                }
            }
            _ => {}
        }
    }

    fn set_cursor(&mut self, row: usize, col: usize) {
        let name = match self.active_file.clone() {
            Some(n) => n,
            None => return,
        };
        let f = self.files.get_mut(&name).unwrap();
        let max_row = f.ta.lines().len().saturating_sub(1);
        let row = row.min(max_row);
        f.ta.move_cursor(CursorMove::Head);
        for _ in 0..row {
            f.ta.move_cursor(CursorMove::Down);
        }
        let line_len = f.ta.lines()[row].chars().count();
        let col = col.min(line_len);
        for _ in 0..col {
            f.ta.move_cursor(CursorMove::Forward);
        }
        self.refresh_completions();
        self.send_cursor_now();
    }

    // ── COMPLETIONS ──

    fn refresh_completions(&mut self) {
        let name = match self.active_file.clone() {
            Some(n) => n,
            None => return,
        };
        let (row, col) = self.files[&name].ta.cursor();
        let line = self.files[&name].ta.lines().get(row).cloned().unwrap_or_default();
        let before = line.chars().take(col).collect::<String>();
        if let Some((word, items)) = completions::compute(self.files[&name].lang, &before) {
            self.ac = Some(AcState { word, index: 0, items });
        } else {
            self.ac = None;
        }
    }

    fn apply_completion(&mut self, word: String) {
        let name = match self.active_file.clone() {
            Some(n) => n,
            None => return,
        };
        let (row, col) = self.files[&name].ta.cursor();
        let line = self.files[&name].ta.lines().get(row).cloned().unwrap_or_default();
        let prefix_len = self.ac.as_ref().map(|a| a.word.chars().count()).unwrap_or(0);
        let mut new_line = line.chars().take(col.saturating_sub(prefix_len)).collect::<String>();
        new_line.push_str(&word);
        new_line.push_str(&line.chars().skip(col).collect::<String>());
        self.set_line(row, new_line, col.saturating_sub(prefix_len) + word.chars().count());
        self.ac = None;
    }

    fn set_line(&mut self, row: usize, line: String, cursor_col: usize) {
        let name = match self.active_file.clone() {
            Some(n) => n,
            None => return,
        };
        // Move to exact position
        {
            let f = self.files.get_mut(&name).unwrap();
            f.ta.move_cursor(CursorMove::Head);
            for _ in 0..row {
                f.ta.move_cursor(CursorMove::Down);
            }
            let line_len = f.ta.lines()[row].chars().count();
            let col = cursor_col.min(line_len);
            for _ in 0..col {
                f.ta.move_cursor(CursorMove::Forward);
            }
            let cur_line_len = f.ta.lines()[row].chars().count();
            for _ in 0..cur_line_len {
                f.ta.delete_next_char();
            }
            f.ta.insert_str(line.clone());
        }
        // reposition cursor
        let f = self.files.get_mut(&name).unwrap();
        f.ta.move_cursor(CursorMove::Head);
        for _ in 0..row {
            f.ta.move_cursor(CursorMove::Down);
        }
        let line_len = f.ta.lines()[row].chars().count();
        let col = cursor_col.min(line_len);
        for _ in 0..col {
            f.ta.move_cursor(CursorMove::Forward);
        }
        self.broadcast_edit();
    }

    fn throttle_cursor(&mut self, pos: (usize, usize)) {
        self.last_cursor_pos = pos;
        if self.last_cursor_sent != Some(pos) {
            self.send_cursor_now();
        }
    }

    // ── RUN ──

    pub fn run_code(&mut self) {
        if self.run_status == RunStatus::Running || self.active_file.is_none() {
            return;
        }
        let name = self.active_file.clone().unwrap();
        let f = &self.files[&name];
        let (lang, version) = match f.lang.piston {
            Some(p) => p,
            None => {
                self.output.push(("⚠ Language not supported for execution".into(), OutputKind::Stderr));
                self.run_status = RunStatus::Err;
                return;
            }
        };
        let code = f.ta.lines().join("\n");
        self.output.clear();
        let local = lang == "python";
        self.output.push((
            format!(
                "▶ Executing {name}{}...",
                if local { " (local python3)" } else { "" }
            ),
            OutputKind::Info,
        ));
        self.run_status = RunStatus::Running;
        self.output_scroll = 0;
        let tx = self.runner_tx.clone();
        runner::spawn_run(tx, lang, version, name, code);
    }

    pub fn on_runner_event(&mut self, ev: RunnerEvent) {
        match ev {
            RunnerEvent::Started => {
                self.run_status = RunStatus::Running;
            }
            RunnerEvent::Line(text, kind) => {
                self.output.push((text, kind));
                if self.output.len() > 2000 {
                    let drain = self.output.len() - 2000;
                    self.output.drain(0..drain);
                }
                self.output_scroll = 0;
            }
            RunnerEvent::Done { ok, exit_code } => {
                self.run_status = if ok { RunStatus::Ok } else { RunStatus::Err };
                if let Some(code) = exit_code {
                    self.output.push((format!("Exit code: {code}"), OutputKind::Info));
                }
                self.output.push((
                    if ok { "✓ Completed" } else { "✗ Error" }.to_string(),
                    OutputKind::Info,
                ));
                self.output_scroll = 0;
            }
            RunnerEvent::Failed(msg) => {
                self.output.push((msg, OutputKind::Stderr));
                self.run_status = RunStatus::Err;
                self.output_scroll = 0;
            }
        }
    }

    // ── FORMAT ──

    pub fn format_code(&mut self) {
        let name = match self.active_file.clone() {
            Some(n) => n,
            None => return,
        };
        let lines: Vec<String> = self.files[&name]
            .ta
            .lines()
            .iter()
            .map(|l| l.trim_end().to_string())
            .collect();
        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        let mut blank = 0;
        for l in lines {
            if l.is_empty() {
                blank += 1;
                if blank <= 1 {
                    out.push(l);
                }
                continue;
            }
            blank = 0;
            out.push(l);
        }
        let text = out.join("\n");
        self.set_text(&name, &text);
        self.toast = Some(("⚡ Formatted".into(), Instant::now()));
    }

    fn set_text(&mut self, name: &str, text: &str) {
        let lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
        if let Some(f) = self.files.get_mut(name) {
            let (row, col) = f.ta.cursor();
            f.ta = TextArea::from(lines.clone());
            f.ta.set_tab_length(2);
            f.ta.set_max_histories(100);
            f.prev_lines = lines.clone();
            f.rev += 1;
            f.base_rev = f.rev;
            f.dirty = true;
            // restore cursor
            f.ta.move_cursor(CursorMove::Head);
            for _ in 0..row {
                f.ta.move_cursor(CursorMove::Down);
            }
            let max = f.ta.lines().get(row).map(|l| l.chars().count()).unwrap_or(0);
            for _ in 0..col.min(max) {
                f.ta.move_cursor(CursorMove::Forward);
            }
        }
        self.broadcast_edit();
    }

    // ── SYNC EVENT HANDLING ──

    pub fn on_sync_event(&mut self, ev: Ev) {
        match ev {
            Ev::Connected => {
                self.connected = true;
                if !self.cfg.is_host() {
                    self.conn_label = format!("Connected · {}", self.cfg.connect);
                }
                self.send_presence();
                self.send_cursor_now();
                // Ask peers for snapshots of everything we have open
                let names: Vec<String> = self.files.keys().cloned().collect();
                for n in names {
                    self.request_snapshot(&n);
                }
            }
            Ev::Disconnected(e) => {
                self.connected = false;
                self.conn_label = format!("Offline — {e}");
            }
            Ev::Loaded { files } => {
                let count = files.len();
                for sf in &files {
                    let should_load = match self.files.get(&sf.name) {
                        Some(f) => !(f.dirty && !f.prev_lines.is_empty()),
                        None => true,
                    };
                    if !should_load {
                        continue;
                    }
                    if self.files.contains_key(&sf.name) {
                        self.set_text(&sf.name, &sf.code);
                        if let Some(f) = self.files.get_mut(&sf.name) {
                            f.dirty = false;
                        }
                    } else {
                        self.create_file_internal(&sf.name, &sf.lang, &sf.code, true);
                        if let Some(f) = self.files.get_mut(&sf.name) {
                            f.dirty = false;
                        }
                    }
                }
                self.last_saved = format!("Loaded {count} saved file(s)");
                if count > 0 {
                    self.toast = Some((format!("📂 Loaded {count} saved file(s)"), Instant::now()));
                }
                self.send_cursor_now();
            }
            Ev::SaveDone { count } => {
                self.last_saved = format!("Saved {} files · {}", count, time_now());
                self.toast = Some(("💾 Workspace saved".into(), Instant::now()));
                for f in self.files.values_mut() {
                    f.dirty = false;
                }
                self.refresh_git();
            }
            Ev::SaveErr(e) => {
                self.toast = Some((format!("⚠ Save failed: {e}"), Instant::now()));
            }
            Ev::Err(e) => {
                self.conn_label = format!("⚠ {e}");
            }
            Ev::Broadcast { event, payload } => self.on_broadcast(&event, payload),
        }
    }

    fn on_broadcast(&mut self, event: &str, payload: serde_json::Value) {
        match event {
            EV_PRESENCE => {
                if let Ok(p) = serde_json::from_value::<Presence>(payload) {
                    if p.id == self.self_id {
                        return;
                    }
                    self.members.insert(
                        p.id,
                        Member {
                            name: p.name,
                            color: color_from_hex(&p.color),
                            ts: p.ts,
                        },
                    );
                }
            }
            EV_WEB_PRESENCE => {
                if let Ok(p) = serde_json::from_value::<Presence>(payload) {
                    if p.id == self.self_id {
                        return;
                    }
                    self.members.insert(
                        p.id,
                        Member {
                            name: p.name,
                            color: color_from_hex(&p.color),
                            ts: p.ts,
                        },
                    );
                }
            }
            EV_CURSOR => {
                if let Ok(m) = serde_json::from_value::<CursorMsg>(payload) {
                    if m.id == self.self_id {
                        return;
                    }
                    for f in self.files.values_mut() {
                        f.cursors.remove(&m.id);
                    }
                    if let Some(f) = self.files.get_mut(&m.file) {
                        f.cursors.insert(m.id.clone(), (m.line, m.col));
                        self.members.insert(
                            m.id,
                            Member {
                                name: m.name,
                                color: color_from_hex(&m.color),
                                ts: now_ms(),
                            },
                        );
                    }
                }
            }
            EV_PATCH => {
                if let Ok(m) = serde_json::from_value::<PatchMsg>(payload) {
                    if m.id == self.self_id {
                        return;
                    }
                    if !self.files.contains_key(&m.file) {
                        self.create_file_internal(&m.file, &m.lang, "", true);
                    }
                    self.apply_patches(&m.file, &m.patches);
                }
            }
            EV_SNAPSHOT => {
                if let Ok(m) = serde_json::from_value::<SnapshotMsg>(payload) {
                    if m.id == self.self_id {
                        return;
                    }
                    let apply = match self.files.get(&m.file) {
                        None => true,
                        Some(f) => f.rev <= m.rev && !f.dirty,
                    };
                    if apply {
                        self.set_text(&m.file, &m.lines.join("\n"));
                        if let Some(f) = self.files.get_mut(&m.file) {
                            f.rev = m.rev;
                            f.base_rev = m.rev;
                            f.dirty = false;
                        }
                    }
                }
            }
            EV_SNAP_REQ => {
                if let Ok(m) = serde_json::from_value::<SnapReqMsg>(payload) {
                    if m.id == self.self_id {
                        return;
                    }
                    self.send_snapshot(&m.file);
                }
            }
            EV_FILE => {
                if let Ok(m) = serde_json::from_value::<FileMsg>(payload) {
                    if m.id == self.self_id {
                        return;
                    }
                    if m.open && !self.files.contains_key(&m.name) {
                        self.create_file_internal(&m.name, &m.lang, "", true);
                        self.request_snapshot(&m.name);
                    }
                }
            }
            EV_WEB_CODE => {
                if let Ok(m) = serde_json::from_value::<WebCodeMsg>(payload) {
                    if m.author == self.cfg.name {
                        return;
                    }
                    let is_active = self.active_file.as_deref() == Some(m.file.as_str());
                    if let Some(f) = self.files.get_mut(&m.file) {
                        if f.ta.lines().join("\n") == m.code {
                            return;
                        }
                    } else {
                        self.create_file_internal(&m.file, "python", "", true);
                    }
                    let _ = is_active;
                    self.set_text(&m.file, &m.code);
                }
            }
            _ => {}
        }
    }

    fn apply_patches(&mut self, file: &str, patches: &[LinePatch]) {
        let name = file.to_string();
        let was_active = self.active_file.as_deref() == Some(file);
        let (cursor_row, cursor_col) = if was_active {
            self.files[&name].ta.cursor()
        } else {
            (0, 0)
        };

        // Build new lines by applying patches
        let mut lines: Vec<String> = self.files[&name].ta.lines().to_vec();
        for p in patches {
            let start = p.start.min(lines.len());
            let end = (start + p.remove).min(lines.len());
            let mut new_lines = Vec::new();
            new_lines.extend_from_slice(&lines[..start]);
            new_lines.extend_from_slice(&p.lines);
            new_lines.extend_from_slice(&lines[end..]);
            lines = new_lines;
        }

        let ta = &mut self.files.get_mut(&name).unwrap().ta;
        *ta = TextArea::from(lines.clone());
        ta.set_tab_length(2);
        ta.set_max_histories(100);
        if was_active {
            ta.move_cursor(CursorMove::Head);
            for _ in 0..cursor_row {
                ta.move_cursor(CursorMove::Down);
            }
            let max = ta.lines().get(cursor_row).map(|l| l.chars().count()).unwrap_or(0);
            for _ in 0..cursor_col.min(max) {
                ta.move_cursor(CursorMove::Forward);
            }
        }
        let f = self.files.get_mut(&name).unwrap();
        f.prev_lines = lines;
        f.rev += 1;
        f.base_rev = f.rev;
        f.dirty = true;
    }

    // ── TICK ──

    pub fn tick(&mut self) {
        let now = Instant::now();
        if self.connected && now.duration_since(self.last_presence).as_secs() >= 5 {
            self.last_presence = now;
            self.send_presence();
            self.send_cursor_now();
            // prune stale members
            let now_ms = now_ms();
            let stale: Vec<String> = self
                .members
                .iter()
                .filter(|(_, m)| now_ms - m.ts > 15000)
                .map(|(k, _)| k.clone())
                .collect();
            for k in stale {
                self.members.remove(&k);
                for f in self.files.values_mut() {
                    f.cursors.remove(&k);
                }
            }
        }
        if self.connected && now.duration_since(self.last_auto_save).as_secs() >= 30 {
            self.save_all();
        }
        if let Some((_, t)) = self.toast {
            if now.duration_since(t).as_secs() >= 4 {
                self.toast = None;
            }
        }
        self.last_snap_req.clear();
    }

    pub fn current_lang_id(&self) -> &'static str {
        self.active_file
            .as_ref()
            .and_then(|n| self.files.get(n))
            .map(|f| f.lang.id)
            .unwrap_or("python")
    }

    pub fn active_ta(&self) -> Option<&TextArea<'static>> {
        self.active_file.as_ref().and_then(|n| self.files.get(n)).map(|f| &f.ta)
    }

    pub fn lang_of(&self, name: &str) -> &'static Language {
        self.files.get(name).map(|f| f.lang).unwrap_or_else(languages::default_lang)
    }
}

pub fn compute_line_patches(old: &[String], new: &[String]) -> Vec<LinePatch> {
    let mut start = 0;
    while start < old.len() && start < new.len() && old[start] == new[start] {
        start += 1;
    }
    let mut old_end = old.len();
    let mut new_end = new.len();
    while old_end > start && new_end > start && old[old_end - 1] == new[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    if old_end == start && new_end == start {
        return vec![];
    }
    vec![LinePatch {
        start,
        remove: old_end - start,
        lines: new[start..new_end].to_vec(),
    }]
}

pub fn member_color(name: &str) -> Color {
    let idx = name.chars().next().map(|c| c as usize).unwrap_or(0) % theme::MEMBER_COLORS.len();
    theme::MEMBER_COLORS[idx]
}

pub fn time_now() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}
