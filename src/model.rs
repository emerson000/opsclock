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

/// One clock in the ordered list. Exactly one clock is always selected by `App`.
pub enum Clock {
    Tz {
        name: String,
        source: Source,
        led: bool,
    },
    Timer {
        name: String,
        duration_ms: i64,
        elapsed_ms: i64,
        running: bool,
        last_start: Timestamp,
        led: bool,
        /// Set once we've fired the desktop notification for this run.
        notified: bool,
    },
    Stopwatch {
        name: String,
        elapsed_ms: i64,
        running: bool,
        last_start: Timestamp,
        led: bool,
    },
    /// Counts down to a fixed absolute instant (a date/time), then holds at zero.
    Countdown {
        name: String,
        target: Timestamp,
        led: bool,
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

    pub fn led(&self) -> bool {
        match self {
            Clock::Tz { led, .. }
            | Clock::Timer { led, .. }
            | Clock::Stopwatch { led, .. }
            | Clock::Countdown { led, .. } => *led,
        }
    }

    pub fn set_led(&mut self, v: bool) {
        match self {
            Clock::Tz { led, .. }
            | Clock::Timer { led, .. }
            | Clock::Stopwatch { led, .. }
            | Clock::Countdown { led, .. } => *led = v,
        }
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
            led: true,
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
    fn stopwatch_reset() {
        let mut c = Clock::Stopwatch {
            name: "S".into(),
            elapsed_ms: 0,
            running: true,
            last_start: t(0),
            led: true,
        };
        assert_eq!(c.current_ms(t(30_000)), 30_000);
        c.on_reset(t(30_000));
        assert_eq!(c.current_ms(t(40_000)), 0); // reset stops at 0
    }
}
