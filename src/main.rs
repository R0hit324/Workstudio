mod app;
mod config;
mod editor;
mod git;
mod languages;
mod runner;
mod sync;
mod theme;
mod ui;
mod util;

use std::io;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, Event as CrosstermEvent};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = config::Args::parse();
    let cfg = config::load(&args);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, event::EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    // user event channel
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<CrosstermEvent>();
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if ev_tx.send(ev).is_err() {
                break;
            }
        }
    });

    // runner channel: main owns receiver, app owns sender
    let (runner_tx, mut runner_rx) = tokio::sync::mpsc::unbounded_channel::<runner::RunnerEvent>();

    // sync channels: app owns the command sender, main owns the event receiver
    let (sync_cmd_tx, sync_cmd_rx) = tokio::sync::mpsc::unbounded_channel::<sync::Cmd>();
    let (sync_ev_tx, mut sync_rx) = tokio::sync::mpsc::unbounded_channel::<sync::Ev>();
    let mut app = app::App::new(
        cfg.clone(),
        Some(sync::SyncClient { tx: sync_cmd_tx }),
        runner_tx,
    );

    // Start the LAN backend now if setup is already complete (the mode comes
    // from saved config); otherwise App::finish_setup starts it with the
    // user-chosen host/join mode.
    if cfg.is_setup_complete() {
        sync::spawn_backend(cfg.clone(), sync_ev_tx, sync_cmd_rx);
    } else {
        app.sync_pending = Some((sync_ev_tx, sync_cmd_rx));
    }

    let mut ticker = tokio::time::interval(Duration::from_millis(100));

    let result: Result<(), Box<dyn std::error::Error>> = loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        tokio::select! {
            ev = ev_rx.recv() => {
                match ev {
                    Some(ev) => handle_event(&mut app, ev),
                    None => break Ok(()),
                }
            }
            sev = sync_rx.recv() => {
                if let Some(ev) = sev {
                    app.on_sync_event(ev);
                }
            }
            rev = runner_rx.recv() => {
                if let Some(ev) = rev {
                    app.on_runner_event(ev);
                }
            }
            _ = ticker.tick() => {
                app.tick();
            }
        }
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

fn handle_event(app: &mut app::App, ev: CrosstermEvent) {
    match ev {
        CrosstermEvent::Key(k) => app.on_key(k),
        CrosstermEvent::Mouse(m) => {
            let (col, row) = (m.column, m.row);
            app.on_mouse(m.kind, col, row);
        }
        CrosstermEvent::Resize(_, _) => {}
        _ => {}
    }
}
