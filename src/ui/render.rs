//! Full-frame rendering: chrome (header, banner, input, key bar), clock tiles
//! for each layout, and the help / NTP overlays.

use crate::app::{App, InputKind, Mode, NTP_SERVERS};
use crate::led::glyph;
use crate::model::{Clock, LabelMode, Layout as L, Source};
use crate::ntp::SyncState;
use crate::time::{self, Wall};
use crate::ui::layouts;
use crate::ui::theme as c;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Pre-computed display strings for one clock at the current instant.
struct View {
    label: String,
    sub: String,
    compact_sub: String,
    time_text: String,
    day: Option<String>,
    status: String,
    status_bright: bool,
    expired: bool,
    /// True during the blink's dark phase (only meaningful when `expired`).
    blink_off: bool,
    selected: bool,
    led: bool,
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    // Background.
    f.render_widget(Block::default().style(Style::default().bg(c::BG)), area);

    let now = app.now();
    let disp = app.disp();
    let local_tz = Source::Zone(jiff::tz::TimeZone::system());
    let local_now = time::wall_of(&local_tz, now);
    let local_disp = time::wall_of(&local_tz, disp);
    let local_day = time::day_number(&local_disp);

    let input_on = matches!(app.mode, Mode::Input { .. });

    // Zen mode: no header/banner/key bar — just the time, filling the screen.
    // The input bar still appears while typing so prompts remain usable.
    if app.zen {
        let main = if input_on {
            let parts = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(2)])
                .split(area);
            draw_input(f, parts[1], app);
            parts[0]
        } else {
            area
        };
        draw_main(f, main, app, disp, now, local_day);
        match &app.mode {
            Mode::Help => draw_help(f, area),
            Mode::Ntp { sel } => draw_ntp(f, area, app, *sel),
            _ => {}
        }
        return;
    }

    // Vertical chrome layout.
    let conv_on = app.conv.is_some();
    let mut constraints = vec![Constraint::Length(2)]; // header
    if conv_on {
        constraints.push(Constraint::Length(2)); // banner
    }
    constraints.push(Constraint::Min(3)); // main
    if input_on {
        constraints.push(Constraint::Length(2)); // input bar
    }
    constraints.push(Constraint::Length(3)); // key bar
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut idx = 0;
    let header = rows[idx];
    idx += 1;
    let banner = if conv_on {
        let r = rows[idx];
        idx += 1;
        Some(r)
    } else {
        None
    };
    let main = rows[idx];
    idx += 1;
    let input = if input_on {
        let r = rows[idx];
        idx += 1;
        Some(r)
    } else {
        None
    };
    let keybar = rows[idx];

    draw_header(f, header, app, &local_now);
    if let Some(b) = banner {
        draw_banner(f, b, app);
    }
    draw_main(f, main, app, disp, now, local_day);
    if let Some(i) = input {
        draw_input(f, i, app);
    }
    draw_keybar(f, keybar);

    match &app.mode {
        Mode::Help => draw_help(f, area),
        Mode::Ntp { sel } => draw_ntp(f, area, app, *sel),
        _ => {}
    }
}

// ---------- chrome ----------

fn draw_header(f: &mut Frame, area: Rect, app: &App, local: &Wall) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(c::BORDER_HEADER));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout_name = format!(
        "LAYOUT: {}",
        match app.layout {
            L::Grid => "GRID [1]",
            L::Split => "SPLIT [2]",
            L::Sidebar => "SIDEBAR [3]",
            L::Wall => "WALL [4]",
        }
    );
    let left = Line::from(vec![
        Span::styled(
            "OPSCLOCK",
            Style::default().fg(c::LED).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(layout_name, Style::default().fg(c::DIM)),
    ]);
    f.render_widget(Paragraph::new(left), inner);

    let local_time = format!("{:02}:{:02}:{:02}", local.hour, local.min, local.sec);
    let sync = format!(
        "SYNC {} {}{}MS",
        app.ntp_server,
        if app.offset_ms >= 0 { "+" } else { "" },
        app.offset_ms
    );
    let (mode_txt, mode_style) = if app.conv.is_some() {
        ("◉ CONVERTED", Style::default().fg(c::AMBER_BRIGHT))
    } else {
        ("● LIVE", Style::default().fg(c::GREEN))
    };
    let right = Line::from(vec![
        Span::styled(format!("LOCAL {}  ", local_time), Style::default().fg(c::DIM)),
        Span::styled(format!("{}  ", sync), Style::default().fg(c::GREEN)),
        Span::styled(mode_txt, mode_style),
    ]);
    f.render_widget(Paragraph::new(right).alignment(Alignment::Right), inner);
}

fn draw_banner(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(c::BORDER_BANNER))
        .style(Style::default().bg(c::BANNER_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let label = app.conv.as_ref().map(|cv| cv.label.clone()).unwrap_or_default();
    let dot_style = if app.blink {
        Style::default().fg(c::AMBER_BRIGHT)
    } else {
        Style::default().fg(c::BANNER_BG)
    };
    let left = Line::from(vec![
        Span::styled("◉ ", dot_style),
        Span::styled(label, Style::default().fg(c::BANNER_TEXT)),
    ]);
    f.render_widget(
        Paragraph::new(left).style(Style::default().bg(c::BANNER_BG)),
        inner,
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "ESC RESUME LIVE",
            Style::default().fg(c::DIMMER),
        )))
        .alignment(Alignment::Right)
        .style(Style::default().bg(c::BANNER_BG)),
        inner,
    );
}

fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    let (kind, buffer, error) = match &app.mode {
        Mode::Input { kind, buffer, error } => (*kind, buffer.clone(), error.clone()),
        _ => return,
    };
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(c::BORDER_BANNER))
        .style(Style::default().bg(c::INPUT_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let sel_name = app.clocks.get(app.sel).map(|c| c.name()).unwrap_or("");
    let prompt = match kind {
        InputKind::SetTime => format!("SET TIME @ {} ▸", sel_name),
        InputKind::Timer => "NEW TIMER (20m / 1h30m / tomorrow at 12pm) ▸".to_string(),
        InputKind::Add => "ADD CLOCK (city / UTC+5:30) ▸".to_string(),
    };
    let cursor = if app.blink { "█" } else { " " };
    let left = Line::from(vec![
        Span::styled(format!("{} ", prompt), Style::default().fg(c::LED)),
        Span::styled(buffer, Style::default().fg(c::BANNER_TEXT)),
        Span::styled(cursor, Style::default().fg(c::LED)),
    ]);
    f.render_widget(
        Paragraph::new(left).style(Style::default().bg(c::INPUT_BG)),
        inner,
    );

    let mut right_spans = Vec::new();
    if let Some(e) = error {
        right_spans.push(Span::styled(format!("{}   ", e), Style::default().fg(c::ERROR)));
    }
    right_spans.push(Span::styled(
        "ENTER OK · ESC CANCEL",
        Style::default().fg(c::DIMMER),
    ));
    f.render_widget(
        Paragraph::new(Line::from(right_spans))
            .alignment(Alignment::Right)
            .style(Style::default().bg(c::INPUT_BG)),
        inner,
    );
}

const KEYHINTS: &[(&str, &str)] = &[
    ("←→", "SELECT"),
    ("1-4", "LAYOUT"),
    ("l", "LED"),
    ("t", "SET TIME"),
    ("T", "TIMER"),
    ("s", "STOPWATCH"),
    ("␣", "RUN/PAUSE"),
    ("r", "RESET"),
    ("a", "ADD"),
    ("x", "CLOSE"),
    ("o", "LABELS"),
    ("n", "SYNC"),
    ("z", "ZEN"),
    ("?", "HELP"),
];

fn draw_keybar(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(c::BORDER_HEADER));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans = Vec::new();
    for (k, label) in KEYHINTS {
        spans.push(Span::styled(*k, Style::default().fg(c::BODY)));
        spans.push(Span::styled(format!(" {}   ", label), Style::default().fg(c::DIMMEST)));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: true }),
        inner,
    );
}

// ---------- main area ----------

fn draw_main(f: &mut Frame, area: Rect, app: &App, disp: jiff::Timestamp, now: jiff::Timestamp, local_day: i64) {
    let n = app.clocks.len();
    let views: Vec<View> = (0..n)
        .map(|i| build_view(app, i, disp, now, local_day))
        .collect();

    let zen = app.zen;
    match app.layout {
        L::Grid => {
            let rects = layouts::grid_rects(area, n);
            for (i, r) in rects.iter().enumerate() {
                draw_tile(f, *r, &views[i], zen);
            }
        }
        L::Split => {
            let rects = layouts::split_rects(area, n, app.sel);
            for (i, r) in rects.iter().enumerate() {
                if r.width > 0 && r.height > 0 {
                    draw_tile(f, *r, &views[i], zen);
                }
            }
        }
        L::Sidebar => {
            let (_, rows) = layouts::sidebar_rows(area, n);
            for (i, r) in rows.iter().enumerate() {
                draw_sidebar_row(f, *r, &views[i], zen);
            }
        }
        L::Wall => {
            draw_wall(f, area, &views[app.sel], zen);
        }
    }
}

fn build_view(app: &App, i: usize, disp: jiff::Timestamp, now: jiff::Timestamp, local_day: i64) -> View {
    let clock = &app.clocks[i];
    let selected = i == app.sel;
    let mut label = clock.name().to_string();
    let sub;
    let mut day = None;
    let status;
    let mut status_bright = false;
    let mut expired = false;
    let time_text;

    match clock {
        Clock::Tz { source, .. } => {
            let w = time::wall_of(source, disp);
            time_text = format!("{:02}:{:02}:{:02}", w.hour, w.min, w.sec);
            let m = time::mil(w.off_min);
            sub = format!(
                "UTC{}{}",
                time::offset_str(w.off_min),
                m.map(|(l, _)| format!(" · {}", l)).unwrap_or_default()
            );
            let dd = time::day_number(&w) - local_day;
            if dd != 0 {
                day = Some(format!("{}{} {}", if dd > 0 { "+" } else { "" }, dd, w.weekday));
            }
            status = if app.conv.is_some() { "CONV" } else { "LIVE" }.to_string();
            status_bright = app.conv.is_some();
            if app.label_mode == LabelMode::Mil {
                label = match m {
                    Some((l, word)) => format!("{} ({})", word, l),
                    None => format!("UTC{}", time::offset_str(w.off_min)),
                };
            }
        }
        Clock::Timer { duration_ms, .. } => {
            let cur = clock.current_ms(now);
            let rem = (*duration_ms - cur).max(0);
            expired = cur >= *duration_ms;
            time_text = format!("T-{}", time::fmt_dur(rem));
            sub = format!("COUNTDOWN {}", time::fmt_dur(*duration_ms));
            status = if expired {
                status_bright = true;
                "HOLD — r RESTART".to_string()
            } else if is_running(clock) {
                "RUNNING".to_string()
            } else {
                "PAUSED".to_string()
            };
        }
        Clock::Stopwatch { .. } => {
            let cur = clock.current_ms(now);
            time_text = format!("T+{}", time::fmt_dur(cur));
            sub = "STOPWATCH".to_string();
            status = if is_running(clock) { "RUNNING" } else { "PAUSED" }.to_string();
        }
        Clock::Countdown { target, .. } => {
            let rem = clock.remaining_ms(now);
            expired = clock.expired(now);
            time_text = format!("T-{}", time::fmt_dur(rem));
            let z = target.to_zoned(jiff::tz::TimeZone::system());
            sub = format!("→ {}", z.strftime("%a %d %b %H:%M"));
            status = if expired {
                status_bright = true;
                "REACHED".to_string()
            } else {
                "COUNTING".to_string()
            };
        }
    }

    // Sidebar compact sub: short offset / ↓timer / ↑stopwatch, or day chip.
    let compact_sub = if let Some(d) = &day {
        d.clone()
    } else {
        match clock {
            Clock::Tz { .. } => sub
                .strip_prefix("UTC")
                .unwrap_or(&sub)
                .split(" · ")
                .next()
                .unwrap_or("")
                .to_string(),
            Clock::Timer { .. } => format!("↓{}", time::fmt_dur(clock.current_ms(now))),
            Clock::Stopwatch { .. } => "↑".to_string(),
            Clock::Countdown { .. } => format!("↓{}", time::fmt_dur(clock.remaining_ms(now))),
        }
    };

    View {
        label,
        sub,
        compact_sub,
        time_text,
        day,
        status,
        status_bright,
        expired,
        blink_off: !app.blink,
        selected,
        led: clock.led(),
    }
}

fn is_running(c: &Clock) -> bool {
    matches!(
        c,
        Clock::Timer { running: true, .. } | Clock::Stopwatch { running: true, .. }
    )
}

// ---------- tiles ----------

fn draw_tile(f: &mut Frame, area: Rect, v: &View, zen: bool) {
    // Zen: no border, name, or footer — the time fills the whole cell.
    if zen {
        if area.width >= 2 && area.height >= 1 {
            draw_body(f, area, v, false);
        }
        return;
    }

    let border_style = if v.selected {
        Style::default().fg(c::BORDER_SEL)
    } else {
        Style::default().fg(c::BORDER)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(c::TILE_BG));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 || inner.width < 3 {
        return;
    }

    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let (head, body, foot) = (parts[0], parts[1], parts[2]);

    draw_tile_header(f, head, v);
    draw_body(f, body, v, false);
    draw_tile_footer(f, foot, v);
}

fn draw_tile_header(f: &mut Frame, area: Rect, v: &View) {
    let label_style = if v.selected {
        Style::default().fg(c::SEL_LABEL).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(c::BODY).add_modifier(Modifier::BOLD)
    };
    let mark = if v.selected { "▸ " } else { "" };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(mark, label_style),
            Span::styled(v.label.clone(), label_style),
        ])),
        area,
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            v.sub.clone(),
            Style::default().fg(c::DIMMER),
        )))
        .alignment(Alignment::Right),
        area,
    );
}

fn draw_tile_footer(f: &mut Frame, area: Rect, v: &View) {
    if let Some(day) = &v.day {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {} ", day),
                Style::default().fg(c::AMBER).bg(c::CHIP_BG),
            ))),
            area,
        );
    }
    let status_style = if v.status_bright {
        Style::default().fg(c::AMBER_BRIGHT)
    } else {
        Style::default().fg(c::DIMMEST)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(v.status.clone(), status_style)))
            .alignment(Alignment::Right),
        area,
    );
}

/// Body: LED dot-matrix art, or large plain text, centered.
fn draw_body(f: &mut Frame, area: Rect, v: &View, wall: bool) {
    let dim = v.expired && v.blink_off;
    let color = if dim { c::DIMMEST } else { c::LED };
    if v.led {
        let n_glyphs = v.time_text.chars().count();
        let max_dots = if wall { 4 } else { 3 };
        let dots = crate::led::dot_fit(n_glyphs, area.width, area.height).min(max_dots);
        let lines = build_art(&v.time_text, dots, color, c::GHOST);
        let art_h = lines.len() as u16;
        let vpad = area.height.saturating_sub(art_h) / 2;
        let mut all: Vec<Line> = Vec::new();
        for _ in 0..vpad {
            all.push(Line::from(""));
        }
        all.extend(lines);
        f.render_widget(Paragraph::new(all).alignment(Alignment::Center), area);
    } else {
        let vpad = area.height.saturating_sub(1) / 2;
        let mut all: Vec<Line> = Vec::new();
        for _ in 0..vpad {
            all.push(Line::from(""));
        }
        all.push(Line::from(Span::styled(
            v.time_text.clone(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
        f.render_widget(Paragraph::new(all).alignment(Alignment::Center), area);
    }
}

/// Build dot-matrix lines: every dot is a colored block cell (lit or ghost);
/// inter-glyph gaps are spaces. `dots` scales each cell to a dots×dots square.
fn build_art(text: &str, dots: usize, lit: ratatui::style::Color, ghost: ratatui::style::Color) -> Vec<Line<'static>> {
    #[derive(Clone, Copy)]
    enum Cell {
        Lit,
        Off,
        Gap,
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Line> = Vec::with_capacity(7 * dots);
    for r in 0..7 {
        let mut cells: Vec<Cell> = Vec::new();
        for (ci, ch) in chars.iter().enumerate() {
            let g = glyph(*ch);
            let bytes = g[r].as_bytes();
            for &b in bytes {
                cells.push(if b == b'1' { Cell::Lit } else { Cell::Off });
            }
            if ci < chars.len() - 1 {
                cells.push(Cell::Gap);
            }
        }
        let mut spans: Vec<Span> = Vec::new();
        for cell in &cells {
            let (ch, style) = match cell {
                Cell::Lit => ('█', Style::default().fg(lit)),
                Cell::Off => ('█', Style::default().fg(ghost)),
                Cell::Gap => (' ', Style::default()),
            };
            spans.push(Span::styled(ch.to_string().repeat(dots), style));
        }
        let line = Line::from(spans);
        for _ in 0..dots {
            out.push(line.clone());
        }
    }
    out
}

fn draw_sidebar_row(f: &mut Frame, area: Rect, v: &View, zen: bool) {
    let label_style = if v.selected {
        Style::default().fg(c::SEL_LABEL).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(c::BODY)
    };
    // Zen: only the time, left-aligned, no name or sub.
    if zen {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                v.time_text.clone(),
                Style::default().fg(c::LED),
            ))),
            area,
        );
        return;
    }
    let mark = if v.selected { "▸ " } else { "  " };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(mark, label_style),
            Span::styled(v.label.clone(), label_style),
        ])),
        area,
    );
    let right = if v.compact_sub.is_empty() {
        v.time_text.clone()
    } else {
        format!("{}  {}", v.compact_sub, v.time_text)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(right, Style::default().fg(c::BODY))))
            .alignment(Alignment::Right),
        area,
    );
}

fn draw_wall(f: &mut Frame, area: Rect, v: &View, zen: bool) {
    // Zen: just the giant time, no header or footer.
    if zen {
        draw_body(f, area, v, true);
        return;
    }
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    // Header row.
    let label_style = Style::default().fg(c::SEL_LABEL).add_modifier(Modifier::BOLD);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("▸ ", label_style),
            Span::styled(v.label.clone(), label_style),
        ])),
        parts[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(v.sub.clone(), Style::default().fg(c::DIMMER))))
            .alignment(Alignment::Right),
        parts[0],
    );
    draw_body(f, parts[1], v, true);
    draw_tile_footer(f, parts[2], v);
}

// ---------- overlays ----------

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

const HELP_ROWS: &[(&str, &str)] = &[
    ("← → TAB", "select clock"),
    ("t", "set time on clock — converts all (e.g. 17:00 tomorrow)"),
    ("1", "grid layout (workstation)"),
    ("T", "new countdown timer (20m, 1h30m, 00:20:00)"),
    ("2", "split layout (tmux panes)"),
    ("s", "new stopwatch (counts up)"),
    ("3", "sidebar layout (compact)"),
    ("SPACE", "run / pause selected timer"),
    ("4", "wall layout (giant clock)"),
    ("r", "reset / restart selected timer"),
    ("l", "toggle LED matrix style on clock"),
    ("a", "add clock (city or UTC+5:30)"),
    ("o", "labels: city ↔ military zone"),
    ("x", "close selected clock"),
    ("n", "time server sync"),
    ("z", "zen mode — hide all chrome, just the time"),
    ("ESC", "resume live / exit zen / close panel"),
];

fn draw_help(f: &mut Frame, area: Rect) {
    let panel = centered(area, 68, (HELP_ROWS.len() as u16) + 6);
    f.render_widget(Clear, panel);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(c::LED))
        .style(Style::default().bg(c::PANEL_BG));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "OPSCLOCK — KEY REFERENCE",
        Style::default().fg(c::LED).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    for (k, d) in HELP_ROWS {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<9}", k), Style::default().fg(c::SEL_LABEL)),
            Span::styled(*d, Style::default().fg(c::DESC)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("ESC CLOSE", Style::default().fg(c::DIMMER))));
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(c::PANEL_BG)),
        inner,
    );
}

fn draw_ntp(f: &mut Frame, area: Rect, app: &App, sel: usize) {
    let panel = centered(area, 62, (NTP_SERVERS.len() as u16) + 6);
    f.render_widget(Clear, panel);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(c::LED))
        .style(Style::default().bg(c::PANEL_BG));
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let busy = matches!(app.ntp, SyncState::Syncing(_));
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "TIME SYNC — SELECT SERVER",
        Style::default().fg(c::LED).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    for (i, (name, stratum, delay)) in NTP_SERVERS.iter().enumerate() {
        let selr = i == sel;
        let locked = *name == app.ntp_server && !busy;
        let mark = if selr { "▸" } else { " " };
        let state = if busy && selr {
            ("SYNCING…", Style::default().fg(c::AMBER_BRIGHT))
        } else if locked {
            ("LOCKED", Style::default().fg(c::GREEN))
        } else {
            ("", Style::default())
        };
        let row_style = if selr {
            Style::default().fg(c::BANNER_TEXT)
        } else {
            Style::default().fg(c::DESC)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", mark), Style::default().fg(c::LED)),
            Span::styled(format!("{:<20}", name), row_style),
            Span::styled(format!("STRATUM {:<3}", stratum), Style::default().fg(c::DIMMER)),
            Span::styled(format!("{:<7}", delay), Style::default().fg(c::DIMMER)),
            Span::styled(state.0, state.1),
        ]));
    }
    lines.push(Line::from(""));
    let status = match &app.ntp {
        SyncState::Syncing(_) => "CONTACTING SERVER…".to_string(),
        SyncState::Failed(msg) => msg.clone(),
        SyncState::Idle => "↑↓ SELECT · ENTER SYNC · ESC CLOSE".to_string(),
    };
    lines.push(Line::from(Span::styled(status, Style::default().fg(c::DIMMER))));
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(c::PANEL_BG)),
        inner,
    );
}
