//! Per-layout rectangle computation. Tile drawing lives in `render.rs`.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Grid: n≤1 → 1 col, n≤4 → 2 cols, else 3. Returns one Rect per clock (in order).
pub fn grid_rects(area: Rect, n: usize) -> Vec<Rect> {
    let cols = if n <= 1 {
        1
    } else if n <= 4 {
        2
    } else {
        3
    };
    let rows = n.div_ceil(cols);
    let row_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Ratio(1, rows as u32); rows])
        .spacing(1)
        .split(area);

    let mut out = Vec::with_capacity(n);
    for r in row_rects.iter() {
        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Ratio(1, cols as u32); cols])
            .spacing(1)
            .split(*r);
        for c in cells.iter() {
            if out.len() < n {
                out.push(*c);
            }
        }
    }
    out
}

/// Split: selected clock fills a ~60% left pane; the rest stack in the right column.
/// Returns one Rect per clock (indexed by clock position).
pub fn split_rects(area: Rect, n: usize, sel: usize) -> Vec<Rect> {
    if n <= 1 {
        return vec![area];
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .spacing(1)
        .split(area);
    let (left, right) = (cols[0], cols[1]);

    let others: Vec<usize> = (0..n).filter(|&i| i != sel).collect();
    let right_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Ratio(1, others.len() as u32); others.len()])
        .spacing(1)
        .split(right);

    let mut out = vec![Rect::default(); n];
    out[sel] = left;
    for (slot, &i) in others.iter().enumerate() {
        out[i] = right_rects[slot];
    }
    out
}

/// Sidebar: a left strip (≤40 cols) of single-line rows. Returns (strip, per-clock rows).
pub fn sidebar_rows(area: Rect, n: usize) -> (Rect, Vec<Rect>) {
    let width = area.width.min(40);
    let strip = Rect {
        x: area.x,
        y: area.y,
        width,
        height: area.height,
    };
    let mut constraints = vec![Constraint::Length(1); n];
    constraints.push(Constraint::Min(0));
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .spacing(0)
        .split(strip);
    let row_rects: Vec<Rect> = rows.iter().take(n).copied().collect();
    (strip, row_rects)
}
