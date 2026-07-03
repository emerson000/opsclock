//! Design-token colors mapped to `ratatui::Color::Rgb`. Values are final.

use ratatui::style::Color;

const fn rgb(hex: u32) -> Color {
    Color::Rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

/// App background.
pub const BG: Color = rgb(0x050202);
/// Clock tile background.
pub const TILE_BG: Color = rgb(0x0b0404);
/// Overlay panel background.
pub const PANEL_BG: Color = rgb(0x0c0404);

/// LED lit / primary accent (bright red).
pub const LED: Color = rgb(0xff3b2f);

/// Selected clock label.
pub const SEL_LABEL: Color = rgb(0xff6b5f);
/// Body text.
pub const BODY: Color = rgb(0xc4574c);
/// Dim chrome.
pub const DIM: Color = rgb(0x8a3a32);
/// Dimmer chrome.
pub const DIMMER: Color = rgb(0x7a2c26);
/// Dimmest chrome (status, key bar).
pub const DIMMEST: Color = rgb(0x6b2a24);
/// Help-overlay description text.
pub const DESC: Color = rgb(0xa04338);

/// Unselected tile border.
pub const BORDER: Color = rgb(0x331110);
/// Selected tile border (== LED).
pub const BORDER_SEL: Color = rgb(0xff3b2f);
/// Header / key-bar separator border.
pub const BORDER_HEADER: Color = rgb(0x2a0e0b);
/// Conversion-banner / input-bar border.
pub const BORDER_BANNER: Color = rgb(0x4a1a14);

/// Amber accent (day chip fg, conversion label text).
pub const AMBER: Color = rgb(0xffb347);
/// Bright amber (HOLD, CONVERTED indicator, SYNCING).
pub const AMBER_BRIGHT: Color = rgb(0xffd23f);
/// Day-chip background.
pub const CHIP_BG: Color = rgb(0x2a1002);
/// Conversion-banner label text.
pub const BANNER_TEXT: Color = rgb(0xffb0a8);

/// Green (sync OK, LIVE).
pub const GREEN: Color = rgb(0x58d17b);

/// Input-bar background.
pub const INPUT_BG: Color = rgb(0x160705);
/// Conversion-banner background.
pub const BANNER_BG: Color = rgb(0x200a06);
/// Error text.
pub const ERROR: Color = rgb(0xff6b5f);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_is_bright_red() {
        assert_eq!(LED, Color::Rgb(0xff, 0x3b, 0x2f));
    }

    #[test]
    fn chip_bg_token() {
        assert_eq!(CHIP_BG, Color::Rgb(0x2a, 0x10, 0x02));
    }
}
