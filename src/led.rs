//! 5×7 LED dot-matrix font and rendering helpers. Bitmaps are exact. A glyph
//! is 5 columns wide, 7 rows tall; `1` = lit cell.

/// Rows (top→bottom) for a glyph, or the blank glyph for anything unknown.
pub fn glyph(c: char) -> [&'static str; 7] {
    match c {
        '0' => ["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
        '1' => ["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
        '2' => ["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
        '3' => ["11111", "00010", "00100", "00010", "00001", "10001", "01110"],
        '4' => ["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
        '5' => ["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
        '6' => ["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
        '7' => ["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
        '8' => ["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
        '9' => ["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
        ':' => ["00000", "00100", "00100", "00000", "00100", "00100", "00000"],
        '-' => ["00000", "00000", "00000", "01110", "00000", "00000", "00000"],
        '+' => ["00000", "00100", "00100", "11111", "00100", "00100", "00000"],
        'T' => ["11111", "00100", "00100", "00100", "00100", "00100", "00100"],
        _ => ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
    }
}

/// Render `text` as 7 rows of dot-matrix art. Lit cells are `█`; unlit are space.
/// With `ghost = true`, every cell is `█` (the dark off-segment backing layer).
/// One blank column separates glyphs. (The renderer builds colored spans directly
/// from [`glyph`]; this string form backs the unit tests and any plain-text use.)
#[allow(dead_code)]
pub fn led_art(text: &str, ghost: bool) -> Vec<String> {
    let mut rows: Vec<String> = Vec::with_capacity(7);
    for r in 0..7 {
        let mut line = String::new();
        let chars: Vec<char> = text.chars().collect();
        for (ci, &c) in chars.iter().enumerate() {
            let g = glyph(c);
            let row = g[r];
            for cell in row.chars() {
                line.push(if ghost || cell == '1' { '█' } else { ' ' });
            }
            if ci < chars.len() - 1 {
                line.push(' ');
            }
        }
        rows.push(line);
    }
    rows
}

/// Grid width in cells for `n` glyphs: 5 per glyph + 1 gap between.
pub fn art_width(n_glyphs: usize) -> usize {
    if n_glyphs == 0 {
        0
    } else {
        n_glyphs * 6 - 1
    }
}

/// Largest integer dots-per-cell (≥1) so the art fits `avail_w × avail_h`.
/// The base grid is `art_width(n) × 7`.
pub fn dot_fit(n_glyphs: usize, avail_w: u16, avail_h: u16) -> usize {
    let w = art_width(n_glyphs).max(1) as u16;
    let by_w = (avail_w / w) as usize;
    let by_h = (avail_h / 7) as usize;
    by_w.min(by_h).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_dims() {
        let rows = led_art("12", false);
        assert_eq!(rows.len(), 7);
        // two glyphs: 5 + 1 gap + 5 = 11 columns.
        assert!(rows.iter().all(|r| r.chars().count() == 11));
    }

    #[test]
    fn ghost_all_lit() {
        let rows = led_art("1", true);
        assert!(rows.iter().all(|r| r.chars().all(|c| c == '█')));
    }

    #[test]
    fn width_calc() {
        assert_eq!(art_width(1), 5);
        assert_eq!(art_width(8), 47); // "T-00:00:00" style width for 8 glyphs
    }

    #[test]
    fn fit_at_least_one() {
        assert!(dot_fit(8, 10, 3) >= 1);
        assert!(dot_fit(1, 100, 70) >= 2);
    }
}
