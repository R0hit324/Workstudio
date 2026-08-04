use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Modal, Phase, RunStatus};
use crate::editor::highlight::StyledSpan;
use crate::runner::OutputKind;
use crate::theme::Theme;

pub fn draw(f: &mut Frame, app: &mut App) {
    let total = f.area();
    app.layout.total = total;

    if matches!(app.phase, Phase::Setup) {
        draw_setup(f, app, total);
        return;
    }

    let [top, mid, status] = Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(total);
    draw_topbar(f, app, top);
    draw_statusbar(f, app, status);

    let [sidebar, editor_area] = Layout::horizontal([Constraint::Length(24), Constraint::Min(0)]).areas(mid);
    app.layout.sidebar = sidebar;
    draw_sidebar(f, app, sidebar);
    draw_editor_area(f, app, editor_area);

    draw_toast(f, app, total);
    draw_modal(f, app, total);
}

fn bg(f: &mut Frame, area: Rect, color: Color) {
    f.buffer_mut().set_style(area, Style::new().bg(color));
}

// ── SETUP SCREEN ──

fn draw_setup(f: &mut Frame, app: &mut App, total: Rect) {
    let th = app.theme();
    bg(f, total, th.bg);
    let w = total.width.min(72);
    let h = 24.min(total.height.saturating_sub(4));
    let area = centered(total, w, h);
    bg(f, area, th.surface);

    let inner = area.inner(Margin::new(2, 2));
    let [logo, tag, name, room, mode, addr, err_row, btn] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .areas(inner);

    let logo_line = Line::from(vec![
        Span::styled("</> ", Style::new().fg(th.accent).add_modifier(Modifier::BOLD)),
        Span::styled("Nexus Code", Style::new().fg(th.text).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(logo_line), logo);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Collaborative IDE — write, run, and share code with your team over the local network.",
            Style::new().fg(th.text2),
        ))),
        tag,
    );

    draw_field(f, "Name", &mut app.setup.name, name, th, app.setup.focus == 0);
    draw_field(f, "Workspace Name", &mut app.setup.room, room, th, app.setup.focus == 1);

    // host / join mode toggle
    let mode_style = if app.setup.focus == 2 {
        Style::new().fg(th.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(th.text2)
    };
    let mut mode_spans: Vec<Span> = Vec::new();
    if app.setup.focus == 2 {
        mode_spans.push(Span::styled("> ", Style::new().fg(th.accent)));
    }
    mode_spans.push(Span::styled(
        if app.setup.join { "○ Host" } else { "● Host" },
        mode_style,
    ));
    mode_spans.push(Span::styled(" this device      ", Style::new().fg(th.text2)));
    mode_spans.push(Span::styled(
        if app.setup.join { "● Join" } else { "○ Join" },
        mode_style,
    ));
    mode_spans.push(Span::styled(" another device", Style::new().fg(th.text2)));
    f.render_widget(Paragraph::new(Line::from(mode_spans)), mode);

    if app.setup.join {
        draw_field(f, "Host Address (ip:port)", &mut app.setup.addr, addr, th, app.setup.focus == 3);
    } else {
        draw_field(
            f,
            "Workspace Directory",
            &mut app.setup.dir,
            addr,
            th,
            app.setup.focus == 3,
        );
    }

    if let Some(err) = &app.setup.error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(err.clone(), Style::new().fg(th.red)))),
            err_row,
        );
    }

    let launch = app.setup.total_focus() - 1;
    let btn_style = if app.setup.focus == launch {
        Style::new().bg(th.accent).fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(th.text2)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            if app.setup.join { "  Join Workspace →" } else { "  Start Session →" },
            btn_style,
        )))
        .alignment(Alignment::Center),
        btn,
    );
}

fn draw_field(
    f: &mut Frame,
    label: &str,
    ta: &mut tui_textarea::TextArea<'static>,
    area: Rect,
    th: &'static Theme,
    focused: bool,
) {
    let [lbl, input] = Layout::vertical([Constraint::Length(1), Constraint::Length(2)]).areas(area);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            label.to_uppercase(),
            Style::new().fg(th.text3),
        ))),
        lbl,
    );
    let mut block = Block::default().borders(Borders::ALL);
    if focused {
        block = block.border_style(Style::new().fg(th.accent));
    } else {
        block = block.border_style(Style::new().fg(th.border));
    }
    ta.set_block(block);
    ta.set_style(Style::new().fg(th.text));
    f.render_widget(&*ta, input);
}

// ── TOPBAR ──

fn draw_topbar(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    bg(f, area, th.surface);
    let half = area.width / 2;

    let left = Rect { width: half, ..area };
    let mut spans = vec![
        Span::styled("</>", Style::new().fg(th.accent).add_modifier(Modifier::BOLD)),
        Span::styled(" Nexus Code ", Style::new().fg(th.text).add_modifier(Modifier::BOLD)),
        Span::styled("│ ", Style::new().fg(th.text3)),
    ];
    if app.active_file.is_some() {
        for name in app.files.keys() {
            let f0 = &app.files[name];
            let is_active = Some(name) == app.active_file.as_ref();
            let style = if is_active {
                Style::new().fg(th.text).bg(th.surface3)
            } else {
                Style::new().fg(th.text3)
            };
            spans.push(Span::styled("● ", Style::new().fg(f0.lang.color)));
            spans.push(Span::styled(name.clone(), style));
            spans.push(Span::styled(" ", style));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)), left);

    let right = Rect { x: area.x + half, width: area.width - half, ..area };
    let mut spans = vec![];
    for (label, color) in [("+ New", th.text2), ("💾 Save", th.text2), ("＋ Invite", th.text2)] {
        spans.push(Span::styled(
            format!(" {label} "),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Right),
        right,
    );
}

// ── SIDEBAR ──

fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    bg(f, area, th.surface);

    let files_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let _ = files_area;
    let inner = area.inner(Margin::new(1, 1));

    let member_lines = (app.members.len() + 1).min(area.height.saturating_sub(6) as usize / 2);
    let [files_list, _, members_list] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length((member_lines * 2) as u16),
    ])
    .areas(inner);

    let mut file_lines: Vec<Line> = vec![Line::from(Span::styled("FILES", Style::new().fg(th.text3).add_modifier(Modifier::BOLD)))];
    for name in app.files.keys() {
        let f0 = &app.files[name];
        let is_active = Some(name) == app.active_file.as_ref();
        let style = if is_active {
            Style::new().fg(th.accent2).bg(th.surface3).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(th.text2)
        };
        file_lines.push(Line::from(vec![
            Span::styled("● ", Style::new().fg(f0.lang.color)),
            Span::styled(name.clone(), style),
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(file_lines)), files_list);

    let mut mem_lines: Vec<Line> = vec![Line::from(Span::styled("TEAM", Style::new().fg(th.text3).add_modifier(Modifier::BOLD)))];
    let mut all = vec![(app.cfg.name.clone(), app.my_color, true)];
    for m in app.members.values() {
        all.push((m.name.clone(), m.color, false));
    }
    all.dedup_by(|a, b| a.0 == b.0);
    for (name, color, is_me) in all.into_iter().take(member_lines) {
        let initials: String = name.chars().take(2).collect::<String>().to_uppercase();
        mem_lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", initials),
                Style::new().fg(color).bg(th.surface3).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{}{}", name, if is_me { " (you)" } else { "" }),
                Style::new().fg(th.text2),
            ),
            Span::styled(" ●", Style::new().fg(th.green)),
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(mem_lines)), members_list);
}

// ── EDITOR AREA ──

fn draw_editor_area(f: &mut Frame, app: &mut App, area: Rect) {
    let out_h = app.output_height.min(area.height.saturating_sub(4));
    let [toolbar, editor, resize, output] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(out_h),
    ])
    .areas(area);

    draw_editor_toolbar(f, app, toolbar);
    draw_editor(f, app, editor);
    draw_resize(f, app, resize);
    draw_output(f, app, output);
}

fn draw_editor_toolbar(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    bg(f, area, th.surface2);

    let name = app.active_file.clone().unwrap_or_default();
    let lang = app.lang_of(&name);
    let (row, col) = app.active_ta().map(|t| t.cursor()).unwrap_or((0, 0));

    let mut spans = vec![
        Span::styled(
            format!(" {} ", lang.name),
            Style::new().fg(lang.color).bg(th.surface3).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ⚡ Format  ", Style::new().fg(th.text2)),
        Span::styled("  ☀ Theme  ", Style::new().fg(th.text2)),
    ];

    let run_style = match app.run_status {
        RunStatus::Running => Style::new().fg(th.amber),
        RunStatus::Ok => Style::new().fg(th.green),
        RunStatus::Err => Style::new().fg(th.red),
        RunStatus::Idle => Style::new().fg(th.green),
    };
    let run_label = match app.run_status {
        RunStatus::Running => "⏳ Running...",
        _ => "▶ Run",
    };
    spans.push(Span::styled("   ", Style::new()));
    spans.push(Span::styled(format!(" {run_label} "), run_style.add_modifier(Modifier::BOLD)));

    let half = area.width.saturating_sub(30).max(10) as usize;
    let line = Line::from(spans);
    let line2 = Line::from(vec![
        Span::styled("   ", Style::new()),
        Span::styled(format!("Ln {}, Col {}  ", row + 1, col + 1), Style::new().fg(th.text2)),
    ]);
    let _ = half;
    f.render_widget(Paragraph::new(line), area);
    f.render_widget(Paragraph::new(line2).alignment(Alignment::Right), area);
}

fn draw_resize(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    bg(f, area, th.surface2);
    let dots = "─".repeat(area.width.max(1) as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(dots, Style::new().fg(th.border)))),
        area,
    );
}

fn draw_editor(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    bg(f, area, th.surface);

    let name = match app.active_file.clone() {
        Some(n) => n,
        None => return,
    };
    let lang_id: &'static str = app.lang_of(&name).id;
    let lines: Vec<String> = app.files[&name].ta.lines().to_vec();
    let (cursor_row, cursor_col) = app.files[&name].ta.cursor();
    let selection = app.files[&name].ta.selection_range();
    let remote_cursors = app.files[&name].cursors.clone();
    let total_lines = lines.len();

    // gutter width
    let gutter_w = (total_lines.to_string().len() as u16).max(3) + 1;
    let content = Rect {
        x: area.x + gutter_w,
        width: area.width.saturating_sub(gutter_w),
        ..area
    };
    app.layout.gutter_width = gutter_w;
    app.layout.editor_content = content;

    let height = content.height as usize;
    let width = content.width as usize;

    // clamp vertical scroll to keep cursor visible
    if cursor_row < app.scroll_row {
        app.scroll_row = cursor_row;
    }
    if cursor_row >= app.scroll_row + height {
        app.scroll_row = cursor_row.saturating_add(1).saturating_sub(height);
    }
    if cursor_col < app.scroll_col {
        app.scroll_col = cursor_col;
    }
    if cursor_col >= app.scroll_col + width {
        app.scroll_col = cursor_col.saturating_add(1).saturating_sub(width);
    }

    // highlight visible lines
    let end = (app.scroll_row + height).min(total_lines);
    let visible_idx: Vec<usize> = (app.scroll_row..end).collect();
    let visible_strs: Vec<&str> = visible_idx.iter().map(|&i| lines[i].as_str()).collect();
    let highlighted = app.hl.highlight_lines(lang_id, &visible_strs, app.dark);

    // build Text of visible lines with horizontal scroll applied
    let mut text_lines: Vec<Line> = Vec::with_capacity(visible_idx.len());
    for spans in &highlighted {
        let clipped = clip_spans(spans, app.scroll_col);
        text_lines.push(spans_to_line(&clipped).patch_style(Style::new().fg(th.text)));
    }

    let text = Text::from(text_lines);
    f.render_widget(Paragraph::new(text).style(Style::new().fg(th.text)), content);

    // line numbers
    let ln_style = Style::new().fg(th.line_nums);
    for (i, idx) in visible_idx.iter().enumerate() {
        let row = content.y + i as u16;
        let active = *idx == cursor_row;
        let style = if active {
            Style::new().fg(th.accent).add_modifier(Modifier::BOLD)
        } else {
            ln_style
        };
        let num = (idx + 1).to_string();
        let x = area.x + gutter_w - 1 - num.len() as u16;
        let buf = f.buffer_mut();
        for (off, ch) in num.chars().enumerate() {
            let cx = x + off as u16;
            if cx < area.x + gutter_w && row < area.y + area.height {
                buf[(cx, row)].set_char(ch);
                buf[(cx, row)].set_style(style);
            }
        }
    }

    // selection highlight
    if let Some(((r1, c1), (r2, c2))) = selection {
        let buf = f.buffer_mut();
        for r in r1..=r2 {
            let vis = r as i64 - app.scroll_row as i64;
            if vis < 0 || vis >= height as i64 {
                continue;
            }
            let row = (content.y as i64 + vis) as u16;
            let start = if r == r1 { c1 } else { 0 };
            let line_len = lines.get(r).map(|l| l.chars().count()).unwrap_or(0);
            let endc = if r == r2 { c2 } else { line_len };
            for c in start..endc {
                let vc = c as i64 - app.scroll_col as i64;
                if vc < 0 || vc >= width as i64 {
                    continue;
                }
                let cx = (content.x as i64 + vc) as u16;
                buf[(cx, row)].set_bg(th.selection_bg);
            }
        }
    }

    // remote cursors
    {
        let buf = f.buffer_mut();
        for (id, rc) in &remote_cursors {
            let color = app.members.get(id).map(|m| m.color).unwrap_or(th.accent);
            let vis = rc.line as i64 - app.scroll_row as i64;
            if vis < 0 || vis >= height as i64 {
                continue;
            }
            let row = (content.y as i64 + vis) as u16;
            let vc = rc.col as i64 - app.scroll_col as i64;
            if vc < 0 || vc >= width as i64 {
                continue;
            }
            let cx = (content.x as i64 + vc) as u16;
            buf[(cx, row)].set_bg(color);
            buf[(cx, row)].set_fg(th.surface);
            buf[(cx, row)].set_style(Style::new().add_modifier(Modifier::REVERSED));
            // name chip: member initials in a colored block, right of the caret
            // if there is room, otherwise left of it.
            if let Some(mem) = app.members.get(id) {
                let chip: Vec<char> = mem.name.chars().take(2).collect::<String>().to_uppercase().chars().collect();
                let len = chip.len() as i64;
                let start = if cx as i64 + 1 + len < (content.x as i64 + width as i64) {
                    cx as i64 + 1
                } else if cx as i64 - len >= content.x as i64 {
                    cx as i64 - len
                } else {
                    -1
                };
                if start >= 0 {
                    for (k, ch) in chip.iter().enumerate() {
                        let lx = (start + k as i64) as u16;
                        buf[(lx, row)].set_char(*ch);
                        buf[(lx, row)].set_fg(th.surface);
                        buf[(lx, row)].set_bg(color);
                        buf[(lx, row)].set_style(Style::new().add_modifier(Modifier::BOLD));
                    }
                }
            }
        }
    }

    // caret
    {
        let vis = cursor_row as i64 - app.scroll_row as i64;
        let vc = cursor_col as i64 - app.scroll_col as i64;
        if vis >= 0 && vis < height as i64 && vc >= 0 && vc < width as i64 {
            let cx = (content.x as i64 + vc) as u16;
            let cy = (content.y as i64 + vis) as u16;
            let buf = f.buffer_mut();
            buf[(cx, cy)].set_bg(th.accent);
            buf[(cx, cy)].set_fg(th.surface);
            buf[(cx, cy)].set_style(Style::new().add_modifier(Modifier::REVERSED));
            f.set_cursor_position((cx, cy));
        }
    }

    // placeholder
    if lines.len() == 1 && lines[0].is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "// Start coding here...",
                Style::new().fg(th.text3),
            ))),
            content,
        );
    }

    // autocomplete
    if let Some(ac) = &app.ac {
        let cur_col_vis = cursor_col as i64 - app.scroll_col as i64;
        let cur_row_vis = cursor_row as i64 - app.scroll_row as i64;
        let ac_h = (ac.items.len() as u16 + 2).min(content.height.saturating_sub(2));
        let ac_w = 30.min(content.width.saturating_sub(2));
        let ax = (content.x as i64 + cur_col_vis + 1)
            .clamp(content.x as i64, (content.x + content.width - ac_w) as i64) as u16;
        let ay = (content.y as i64 + cur_row_vis + 1)
            .clamp(content.y as i64, (area.y + area.height - ac_h) as i64) as u16;
        let ac_area = Rect::new(ax, ay, ac_w, ac_h);
        let mut lines: Vec<Line> = Vec::new();
        for (i, item) in ac.items.iter().enumerate() {
            let sel = i == ac.index;
            let style = if sel {
                Style::new().fg(th.surface).bg(th.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(th.text2)
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {}", item.word), style),
                Span::styled(
                    format!(" [{}]", item.kind),
                    if sel { style } else { Style::new().fg(th.text3) },
                ),
            ]));
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(th.accent));
        f.render_widget(
            Paragraph::new(Text::from(lines))
                .block(block)
                .wrap(Wrap { trim: false }),
            ac_area,
        );
    }
}

// ── OUTPUT PANEL ──

fn draw_output(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    bg(f, area, th.surface);

    let inner = area.inner(Margin::new(1, 0));
    let [header, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    app.layout.output_area = body;

    let status_style = match app.run_status {
        RunStatus::Running => Style::new().fg(th.amber),
        RunStatus::Ok => Style::new().fg(th.green),
        RunStatus::Err => Style::new().fg(th.red),
        RunStatus::Idle => Style::new().fg(th.text3),
    };
    let title = match app.run_status {
        RunStatus::Running => "Running...",
        RunStatus::Ok => "✓ Completed",
        RunStatus::Err => "✗ Error",
        RunStatus::Idle => "Output",
    };
    let hspan = vec![
        Span::styled("● ", status_style),
        Span::styled(title, Style::new().fg(th.text2).add_modifier(Modifier::BOLD)),
        Span::styled("   Clear   Copy", Style::new().fg(th.text3)),
    ];
    let head_line = Line::from(hspan);
    f.render_widget(Paragraph::new(head_line), header);

    // body — scrollable
    let max_scroll = app.output.len().saturating_sub(body.height as usize);
    if app.output_scroll > max_scroll {
        app.output_scroll = max_scroll;
    }
    let start = app.output_scroll;
    let end = (start + body.height as usize).min(app.output.len());
    let mut lines: Vec<Line> = Vec::new();
    for (text, kind) in &app.output[start..end] {
        let style = match kind {
            OutputKind::Stdout => Style::new().fg(th.text),
            OutputKind::Stderr => Style::new().fg(th.red),
            OutputKind::Info => Style::new().fg(th.text3),
        };
        lines.push(Line::from(Span::styled(text.clone(), style)));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("// Press F5 to run your code", Style::new().fg(th.text3))));
    }
    f.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::new().fg(th.text)),
        body,
    );
}

// ── STATUS BAR ──

fn draw_statusbar(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.theme();
    bg(f, area, th.surface);
    let dot_style = if app.connected {
        Style::new().fg(th.green)
    } else {
        Style::new().fg(th.text3)
    };
    let name = app.active_file.clone().unwrap_or_default();
    let lang = app.lang_of(&name);

    let mut spans = vec![
        Span::styled("● ", dot_style),
        Span::styled(
            format!(" {} users", app.members.len() + 1),
            Style::new().fg(th.text2),
        ),
        Span::styled("  |  ", Style::new().fg(th.text3)),
        Span::styled(app.conn_label.clone(), Style::new().fg(th.text2)),
        Span::styled("  |  ", Style::new().fg(th.text3)),
        Span::styled(
            format!("Room: {}", app.cfg.room),
            Style::new().fg(th.text2),
        ),
    ];
    if app.cfg.is_host() && !app.git_branch.is_empty() {
        spans.push(Span::styled("  |  ", Style::new().fg(th.text3)));
        spans.push(Span::styled(
            format!("⎇ {} · {} commits", app.git_branch, app.git_commits),
            Style::new().fg(th.green),
        ));
    }
    spans.push(Span::styled("  |  ", Style::new().fg(th.text3)));
    spans.push(Span::styled(lang.name, Style::new().fg(lang.color)));
    let right = format!("  {}", app.last_saved);
    spans.push(Span::styled(right, Style::new().fg(th.text3)));
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        area,
    );
}

// ── TOAST / MODAL ──

fn draw_toast(f: &mut Frame, app: &mut App, total: Rect) {
    if let Some((msg, _)) = &app.toast {
        let th = app.theme();
        let w = (msg.chars().count() as u16 + 6).min(total.width.saturating_sub(4));
        let area = Rect {
            x: total.x + total.width.saturating_sub(w + 2),
            y: total.y + total.height.saturating_sub(3),
            width: w,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {msg}  "),
                Style::new().fg(th.text).bg(th.surface3),
            ))),
            area,
        );
    }
}

fn draw_modal(f: &mut Frame, app: &mut App, total: Rect) {
    match &app.modal {
        Modal::None => {}
        Modal::Help => draw_help_modal(f, app, total),
        Modal::Invite => draw_invite_modal(f, app, total),
        Modal::GitLog => draw_git_log_modal(f, app, total),
        Modal::ConfirmClose(name) => {
            let name = name.clone();
            draw_confirm_modal(f, app, total, &name)
        }
    }
}

fn draw_confirm_modal(f: &mut Frame, app: &mut App, total: Rect, name: &str) {
    let th = app.theme();
    let w = 44.min(total.width.saturating_sub(4));
    let h = 7;
    let area = centered(total, w, h);
    f.render_widget(Block::default().borders(Borders::ALL).border_style(Style::new().fg(th.accent)), area);
    let inner = area.inner(Margin::new(2, 1));
    f.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                format!("Close '{name}'?"),
                Style::new().fg(th.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled("[y]es  [n]o  [Esc]", Style::new().fg(th.text2))),
        ])),
        inner,
    );
}

fn draw_invite_modal(f: &mut Frame, app: &mut App, total: Rect) {
    let th = app.theme();
    let w = 62.min(total.width.saturating_sub(4));
    let h = 12;
    let area = centered(total, w, h);
    f.render_widget(Block::default().borders(Borders::ALL).border_style(Style::new().fg(th.accent)), area);
    let inner = area.inner(Margin::new(2, 1));

    let ip = crate::util::local_ip();
    let addr = if ip.is_empty() {
        format!("<host-ip>:{}", app.cfg.port)
    } else {
        format!("{ip}:{}", app.cfg.port)
    };

    let mut lines = vec![
        Line::from(Span::styled("🔗 Invite to Workspace", Style::new().fg(th.text).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(
            "Join from any device on the same network:",
            Style::new().fg(th.text2),
        )),
        Line::from(Span::styled(
            format!("    nexus --name <your-name> --connect {addr}"),
            Style::new().fg(th.accent2),
        )),
        Line::from(""),
    ];
    if app.cfg.is_host() {
        lines.push(Line::from(Span::styled(
            "The host listens on 0.0.0.0, so devices on any subnet can connect via the IP above.",
            Style::new().fg(th.text3),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "You joined another device — share your session by having them run the command above.",
            Style::new().fg(th.text3),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Enter / Esc to close]",
        Style::new().fg(th.text3),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }), inner);
}

fn draw_help_modal(f: &mut Frame, app: &mut App, total: Rect) {
    let th = app.theme();
    let w = 54.min(total.width.saturating_sub(4));
    let h = 18.min(total.height.saturating_sub(4));
    let area = centered(total, w, h);
    f.render_widget(Block::default().borders(Borders::ALL).border_style(Style::new().fg(th.accent)), area);
    let inner = area.inner(Margin::new(2, 1));
    let rows = [
        ("Ctrl+N", "new file"),
        ("F4", "close file"),
        ("Ctrl+S", "save (host commits to git)"),
        ("F5", "run code (python3 / Piston)"),
        ("Ctrl+F", "format"),
        ("Ctrl+T / F6", "toggle theme"),
        ("Ctrl+G", "invite / join command"),
        ("Ctrl+J", "git log"),
        ("F1 / ?", "this help"),
        ("Ctrl+Q", "quit"),
        ("Ctrl+Up/Down", "resize output panel"),
        ("", ""),
        ("Arrows / mouse", "edit & navigate"),
        ("Tab / Enter", "autocomplete when open"),
    ];
    let mut lines: Vec<Line> = Vec::new();
    for (k, v) in rows {
        if k.is_empty() {
            lines.push(Line::default());
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(format!("  {k}  "), Style::new().fg(th.accent).add_modifier(Modifier::BOLD)),
            Span::styled(v, Style::new().fg(th.text2)),
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_git_log_modal(f: &mut Frame, app: &mut App, total: Rect) {
    let th = app.theme();
    let w = 60.min(total.width.saturating_sub(4));
    let h = 20.min(total.height.saturating_sub(4));
    let area = centered(total, w, h);
    f.render_widget(Block::default().borders(Borders::ALL).border_style(Style::new().fg(th.accent)), area);
    let inner = area.inner(Margin::new(2, 1));
    let title = if app.cfg.is_host() && !app.git_branch.is_empty() {
        format!("Git log · {} · {} commit(s)", app.git_branch, app.git_commits)
    } else {
        "Git log".to_string()
    };
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        title,
        Style::new().fg(th.text).add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::default());
    if app.git_log.is_empty() {
        if app.cfg.is_host() {
            lines.push(Line::from(Span::styled(
                "No commits yet — Ctrl+S to save & commit.",
                Style::new().fg(th.text2),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "You joined a session — git lives on the host's machine.",
                Style::new().fg(th.text2),
            )));
        }
    } else {
        for entry in &app.git_log {
            lines.push(Line::from(Span::styled(entry.clone(), Style::new().fg(th.text2))));
        }
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "  [Enter / Esc to close]",
        Style::new().fg(th.text3),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

// ── HELPERS ──

fn centered(total: Rect, w: u16, h: u16) -> Rect {
    Rect {
        x: total.x + total.width.saturating_sub(w) / 2,
        y: total.y + total.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

fn spans_to_line(spans: &[StyledSpan]) -> Line<'static> {
    Line::from(
        spans
            .iter()
            .map(|s| ratatui::text::Span::styled(s.text.clone(), s.style))
            .collect::<Vec<_>>(),
    )
}

fn clip_spans(spans: &[StyledSpan], skip: usize) -> Vec<StyledSpan> {
    let mut rem = skip;
    let mut out = Vec::new();
    for s in spans {
        let n = s.text.chars().count();
        if rem >= n {
            rem -= n;
            continue;
        }
        let text: String = s.text.chars().skip(rem).collect();
        out.push(StyledSpan {
            text,
            style: s.style,
        });
        rem = 0;
    }
    out
}
