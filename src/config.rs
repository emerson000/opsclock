//! TOML config: clock list, layout, label mode, NTP server, LED default.
//! Load never fails hard — a broken file falls back to defaults.

use crate::model::{Clock, LabelMode, Layout, Source};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A clock as persisted to disk (jiff `TimeZone` isn't directly serializable).
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ClockCfg {
    Tz {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        zone: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offset: Option<i32>,
        led: bool,
    },
    Timer {
        name: String,
        duration_ms: i64,
        led: bool,
    },
    Stopwatch {
        name: String,
        led: bool,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub clocks: Vec<ClockCfg>,
    pub layout: Layout,
    pub label_mode: LabelMode,
    pub ntp_server: String,
    pub led_default: bool,
}

impl Default for Config {
    fn default() -> Self {
        let tz = |name: &str, zone: &str, led: bool| ClockCfg::Tz {
            name: name.into(),
            zone: Some(zone.into()),
            offset: None,
            led,
        };
        Config {
            clocks: vec![
                tz("UTC", "UTC", true),
                tz("HOUSTON", "America/Chicago", true),
                tz("TOKYO", "Asia/Tokyo", true),
                tz("LONDON", "Europe/London", false),
            ],
            layout: Layout::Grid,
            label_mode: LabelMode::City,
            ntp_server: "pool.ntp.org".into(),
            led_default: true,
        }
    }
}

impl Config {
    /// Default config path: `<config-dir>/opsclock/config.toml`.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("opsclock")
            .join("config.toml")
    }

    /// Load config from `path`; on any error, warn and return defaults.
    pub fn load(path: &PathBuf) -> Config {
        match std::fs::read_to_string(path) {
            Ok(s) => match toml::from_str(&s) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("opsclock: config parse error ({e}); using defaults");
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        }
    }

    /// Save config to `path`, creating parent dirs. Errors are non-fatal.
    pub fn save(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = toml::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }

    /// Build the runtime clock list. Timers/stopwatches load stopped at zero.
    pub fn to_clocks(&self) -> Vec<Clock> {
        let now = Timestamp::now();
        self.clocks
            .iter()
            .filter_map(|c| match c {
                ClockCfg::Tz {
                    name,
                    zone,
                    offset,
                    led,
                } => {
                    let source = match (zone, offset) {
                        (Some(z), _) => jiff::tz::TimeZone::get(z).ok().map(Source::Zone)?,
                        (None, Some(off)) => Source::Fixed(*off),
                        _ => return None,
                    };
                    Some(Clock::Tz {
                        name: name.clone(),
                        source,
                        led: *led,
                    })
                }
                ClockCfg::Timer {
                    name,
                    duration_ms,
                    led,
                } => Some(Clock::Timer {
                    name: name.clone(),
                    duration_ms: *duration_ms,
                    elapsed_ms: 0,
                    running: false,
                    last_start: now,
                    led: *led,
                    notified: false,
                }),
                ClockCfg::Stopwatch { name, led } => Some(Clock::Stopwatch {
                    name: name.clone(),
                    elapsed_ms: 0,
                    running: false,
                    last_start: now,
                    led: *led,
                }),
            })
            .collect()
    }

    /// Snapshot runtime state back into a serializable Config.
    pub fn from_state(
        clocks: &[Clock],
        layout: Layout,
        label_mode: LabelMode,
        ntp_server: &str,
        led_default: bool,
    ) -> Config {
        let clocks = clocks
            .iter()
            .map(|c| match c {
                Clock::Tz { name, source, led } => {
                    let (zone, offset) = match source {
                        Source::Zone(tz) => (tz.iana_name().map(|s| s.to_string()), None),
                        Source::Fixed(off) => (None, Some(*off)),
                    };
                    ClockCfg::Tz {
                        name: name.clone(),
                        zone,
                        offset,
                        led: *led,
                    }
                }
                Clock::Timer {
                    name,
                    duration_ms,
                    led,
                    ..
                } => ClockCfg::Timer {
                    name: name.clone(),
                    duration_ms: *duration_ms,
                    led: *led,
                },
                Clock::Stopwatch { name, led, .. } => ClockCfg::Stopwatch {
                    name: name.clone(),
                    led: *led,
                },
            })
            .collect();
        Config {
            clocks,
            layout,
            label_mode,
            ntp_server: ntp_server.to_string(),
            led_default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_four() {
        let cfg = Config::default();
        assert_eq!(cfg.clocks.len(), 4);
        match &cfg.clocks[0] {
            ClockCfg::Tz { name, .. } => assert_eq!(name, "UTC"),
            _ => panic!("first clock should be tz"),
        }
    }

    #[test]
    fn roundtrip_toml() {
        let cfg = Config::default();
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back.clocks.len(), 4);
        assert_eq!(back.ntp_server, "pool.ntp.org");
        assert_eq!(back.layout, Layout::Grid);
    }

    #[test]
    fn to_clocks_builds_four() {
        let clocks = Config::default().to_clocks();
        assert_eq!(clocks.len(), 4);
        assert!(clocks.iter().all(|c| c.is_tz()));
    }
}
