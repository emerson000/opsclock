//! Core domain entities: clocks, layouts, and the timer/stopwatch state machine.

use jiff::{tz::TimeZone, Timestamp};
use serde::{Deserialize, Serialize};

/// Where a timezone clock gets its wall time from.
#[derive(Clone)]
pub enum Source {
    /// A real IANA zone (DST-aware).
    Zone(TimeZone),
    /// A fixed UTC offset in minutes (e.g. `+330` for UTC+05:30).
    Fixed(i32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Grid,
    Split,
    Sidebar,
    Wall,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelMode {
    City,
    Mil,
}

/// How a clock's time is drawn in the body area.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClockStyle {
    /// Single-line terminal text (small, crisp).
    Plain,
    /// Dot-matrix with the dim off-segment "ghost" backing layer.
    Led,
    /// Large block numerals — lit segments only, no ghost backing.
    Clean,
}

/// Per-clock dot size (cells per lit dot) bounds for [`Clock::adjust_size`].
pub const MIN_SIZE: u8 = 1;
pub const MAX_SIZE: u8 = 6;
pub const DEFAULT_SIZE: u8 = 3;

/// One clock in the ordered list. Exactly one clock is always selected by `App`.
pub enum Clock {
    Tz {
        name: String,
        source: Source,
        style: ClockStyle,
        size: u8,
    },
    Timer {
        name: String,
        duration_ms: i64,
        elapsed_ms: i64,
        running: bool,
        last_start: Timestamp,
        style: ClockStyle,
        size: u8,
        /// Set once we've fired the desktop notification for this run.
        notified: bool,
    },
    Stopwatch {
        name: String,
        elapsed_ms: i64,
        running: bool,
        last_start: Timestamp,
        style: ClockStyle,
        size: u8,
    },
    /// Counts down to a fixed absolute instant (a date/time), then holds at zero.
    Countdown {
        name: String,
        target: Timestamp,
        style: ClockStyle,
        size: u8,
        notified: bool,
    },
}

/// Milliseconds elapsed from `a` to `b` (may be negative).
pub fn ms_between(a: Timestamp, b: Timestamp) -> i64 {
    b.as_millisecond() - a.as_millisecond()
}

impl Clock {
    pub fn name(&self) -> &str {
        match self {
            Clock::Tz { name, .. }
            | Clock::Timer { name, .. }
            | Clock::Stopwatch { name, .. }
            | Clock::Countdown { name, .. } => name,
        }
    }

    pub fn style(&self) -> ClockStyle {
        match self {
            Clock::Tz { style, .. }
            | Clock::Timer { style, .. }
            | Clock::Stopwatch { style, .. }
            | Clock::Countdown { style, .. } => *style,
        }
    }

    pub fn set_style(&mut self, v: ClockStyle) {
        match self {
            Clock::Tz { style, .. }
            | Clock::Timer { style, .. }
            | Clock::Stopwatch { style, .. }
            | Clock::Countdown { style, .. } => *style = v,
        }
    }

    /// Cycle through display styles: plain → LED → clean → plain.
    pub fn cycle_style(&mut self) {
        let next = match self.style() {
            ClockStyle::Plain => ClockStyle::Led,
            ClockStyle::Led => ClockStyle::Clean,
            ClockStyle::Clean => ClockStyle::Plain,
        };
        self.set_style(next);
    }

    pub fn size(&self) -> u8 {
        match self {
            Clock::Tz { size, .. }
            | Clock::Timer { size, .. }
            | Clock::Stopwatch { size, .. }
            | Clock::Countdown { size, .. } => *size,
        }
    }

    fn set_size(&mut self, v: u8) {
        match self {
            Clock::Tz { size, .. }
            | Clock::Timer { size, .. }
            | Clock::Stopwatch { size, .. }
            | Clock::Countdown { size, .. } => *size = v,
        }
    }

    /// Grow/shrink the dot size by `delta`, clamped to [`MIN_SIZE`], [`MAX_SIZE`].
    pub fn adjust_size(&mut self, delta: i8) {
        let next = (self.size() as i16 + delta as i16).clamp(MIN_SIZE as i16, MAX_SIZE as i16);
        self.set_size(next as u8);
    }

    pub fn is_tz(&self) -> bool {
        matches!(self, Clock::Tz { .. })
    }

    /// Elapsed milliseconds for a timer/stopwatch (0 for a tz clock).
    /// `current = elapsed + (running ? now - last_start : 0)`.
    pub fn current_ms(&self, now: Timestamp) -> i64 {
        match self {
            Clock::Tz { .. } | Clock::Countdown { .. } => 0,
            Clock::Timer {
                elapsed_ms,
                running,
                last_start,
                ..
            }
            | Clock::Stopwatch {
                elapsed_ms,
                running,
                last_start,
                ..
            } => {
                let live = if *running {
                    ms_between(*last_start, now).max(0)
                } else {
                    0
                };
                elapsed_ms + live
            }
        }
    }

    /// Milliseconds remaining for a countdown-to-date clock (0 for others).
    pub fn remaining_ms(&self, now: Timestamp) -> i64 {
        match self {
            Clock::Countdown { target, .. } => {
                (target.as_millisecond() - now.as_millisecond()).max(0)
            }
            _ => 0,
        }
    }

    /// A timer/countdown that has reached (or passed) its target.
    pub fn expired(&self, now: Timestamp) -> bool {
        match self {
            Clock::Timer { duration_ms, .. } => self.current_ms(now) >= *duration_ms,
            Clock::Countdown { target, .. } => now.as_millisecond() >= target.as_millisecond(),
            _ => false,
        }
    }

    /// `space`: timer expired → restart; else toggle pause (fold current into elapsed on pause).
    pub fn on_space(&mut self, now: Timestamp) {
        match self {
            Clock::Timer { .. } => {
                let cur = self.current_ms(now);
                let expired = self.expired(now);
                if let Clock::Timer {
                    duration_ms: _,
                    elapsed_ms,
                    running,
                    last_start,
                    notified,
                    ..
                } = self
                {
                    if expired {
                        *elapsed_ms = 0;
                        *last_start = now;
                        *running = true;
                        *notified = false;
                    } else if *running {
                        *elapsed_ms = cur;
                        *running = false;
                    } else {
                        *last_start = now;
                        *running = true;
                    }
                }
            }
            Clock::Stopwatch { .. } => {
                let cur = self.current_ms(now);
                if let Clock::Stopwatch {
                    elapsed_ms,
                    running,
                    last_start,
                    ..
                } = self
                {
                    if *running {
                        *elapsed_ms = cur;
                        *running = false;
                    } else {
                        *last_start = now;
                        *running = true;
                    }
                }
            }
            // Countdown-to-date and tz clocks ignore run/pause.
            Clock::Tz { .. } | Clock::Countdown { .. } => {}
        }
    }

    /// `r`: timer → restart running; stopwatch → reset to 0, stopped.
    pub fn on_reset(&mut self, now: Timestamp) {
        match self {
            Clock::Timer {
                elapsed_ms,
                running,
                last_start,
                notified,
                ..
            } => {
                *elapsed_ms = 0;
                *last_start = now;
                *running = true;
                *notified = false;
            }
            Clock::Stopwatch {
                elapsed_ms,
                running,
                last_start,
                ..
            } => {
                *elapsed_ms = 0;
                *last_start = now;
                *running = false;
            }
            Clock::Tz { .. } | Clock::Countdown { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: i64) -> Timestamp {
        Timestamp::from_millisecond(ms).unwrap()
    }

    fn timer() -> Clock {
        Clock::Timer {
            name: "T".into(),
            duration_ms: 60_000,
            elapsed_ms: 0,
            running: true,
            last_start: t(0),
            style: ClockStyle::Led,
            size: DEFAULT_SIZE,
            notified: false,
        }
    }

    #[test]
    fn timer_pause_fold() {
        let mut c = timer();
        assert_eq!(c.current_ms(t(10_000)), 10_000);
        c.on_space(t(10_000)); // pause
        assert_eq!(c.current_ms(t(50_000)), 10_000); // frozen
        c.on_space(t(50_000)); // resume
        assert_eq!(c.current_ms(t(55_000)), 15_000);
    }

    #[test]
    fn timer_expiry_and_restart() {
        let mut c = timer();
        assert!(c.expired(t(60_000)));
        assert!(!c.expired(t(59_999)));
        c.on_space(t(70_000)); // expired -> restart
        assert!(!c.expired(t(70_001)));
        assert_eq!(c.current_ms(t(75_000)), 5_000);
    }

    #[test]
    fn style_cycles() {
        let mut c = timer();
        c.set_style(ClockStyle::Plain);
        c.cycle_style();
        assert_eq!(c.style(), ClockStyle::Led);
        c.cycle_style();
        assert_eq!(c.style(), ClockStyle::Clean);
        c.cycle_style();
        assert_eq!(c.style(), ClockStyle::Plain);
    }

    #[test]
    fn size_clamps() {
        let mut c = timer();
        assert_eq!(c.size(), DEFAULT_SIZE);
        for _ in 0..10 {
            c.adjust_size(1);
        }
        assert_eq!(c.size(), MAX_SIZE);
        for _ in 0..10 {
            c.adjust_size(-1);
        }
        assert_eq!(c.size(), MIN_SIZE);
    }

    #[test]
    fn stopwatch_reset() {
        let mut c = Clock::Stopwatch {
            name: "S".into(),
            elapsed_ms: 0,
            running: true,
            last_start: t(0),
            style: ClockStyle::Led,
            size: DEFAULT_SIZE,
        };
        assert_eq!(c.current_ms(t(30_000)), 30_000);
        c.on_reset(t(30_000));
        assert_eq!(c.current_ms(t(40_000)), 0); // reset stops at 0
    }
}
