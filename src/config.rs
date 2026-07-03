//! TOML config: clock list, layout, label mode, NTP server, LED default.
//! Load never fails hard — a broken file falls back to defaults.

use crate::model::{Clock, ClockStyle, LabelMode, Layout, Source, DEFAULT_SIZE};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_size() -> u8 {
    DEFAULT_SIZE
}

/// Map persisted (`led`, `clean`) flags to a runtime display style. `clean`
/// wins so files written before the clean style still round-trip sensibly.
fn style_of(led: bool, clean: bool) -> ClockStyle {
    if clean {
        ClockStyle::Clean
    } else if led {
        ClockStyle::Led
    } else {
        ClockStyle::Plain
    }
}

/// Split a runtime style back into the persisted (`led`, `clean`) flags. `Clean`
/// keeps `led = true` so older opsclock versions still render a large display.
fn flags_of(style: ClockStyle) -> (bool, bool) {
    match style {
        ClockStyle::Plain => (false, false),
        ClockStyle::Led => (true, false),
        ClockStyle::Clean => (true, true),
    }
}

/// A clock as persisted to disk (jiff `TimeZone` isn't directly serializable).
/// `led`/`clean`/`size` describe the display style; `clean` and `size` carry
/// serde defaults so configs written before those fields still load.
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
        #[serde(default)]
        clean: bool,
        #[serde(default = "default_size")]
        size: u8,
    },
    Timer {
        name: String,
        duration_ms: i64,
        led: bool,
        #[serde(default)]
        clean: bool,
        #[serde(default = "default_size")]
        size: u8,
    },
    Stopwatch {
        name: String,
        led: bool,
        #[serde(default)]
        clean: bool,
        #[serde(default = "default_size")]
        size: u8,
    },
    Countdown {
        name: String,
        target_ms: i64,
        led: bool,
        #[serde(default)]
        clean: bool,
        #[serde(default = "default_size")]
        size: u8,
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
            clean: false,
            size: DEFAULT_SIZE,
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
                    clean,
                    size,
                } => {
                    let source = match (zone, offset) {
                        (Some(z), _) => jiff::tz::TimeZone::get(z).ok().map(Source::Zone)?,
                        (None, Some(off)) => Source::Fixed(*off),
                        _ => return None,
                    };
                    Some(Clock::Tz {
                        name: name.clone(),
                        source,
                        style: style_of(*led, *clean),
                        size: *size,
                    })
                }
                ClockCfg::Timer {
                    name,
                    duration_ms,
                    led,
                    clean,
                    size,
                } => Some(Clock::Timer {
                    name: name.clone(),
                    duration_ms: *duration_ms,
                    elapsed_ms: 0,
                    running: false,
                    last_start: now,
                    style: style_of(*led, *clean),
                    size: *size,
                    notified: false,
                }),
                ClockCfg::Stopwatch {
                    name,
                    led,
                    clean,
                    size,
                } => Some(Clock::Stopwatch {
                    name: name.clone(),
                    elapsed_ms: 0,
                    running: false,
                    last_start: now,
                    style: style_of(*led, *clean),
                    size: *size,
                }),
                ClockCfg::Countdown {
                    name,
                    target_ms,
                    led,
                    clean,
                    size,
                } => Timestamp::from_millisecond(*target_ms)
                    .ok()
                    .map(|target| Clock::Countdown {
                        name: name.clone(),
                        target,
                        style: style_of(*led, *clean),
                        size: *size,
                        notified: false,
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
                Clock::Tz {
                    name,
                    source,
                    style,
                    size,
                } => {
                    let (zone, offset) = match source {
                        Source::Zone(tz) => (tz.iana_name().map(|s| s.to_string()), None),
                        Source::Fixed(off) => (None, Some(*off)),
                    };
                    let (led, clean) = flags_of(*style);
                    ClockCfg::Tz {
                        name: name.clone(),
                        zone,
                        offset,
                        led,
                        clean,
                        size: *size,
                    }
                }
                Clock::Timer {
                    name,
                    duration_ms,
                    style,
                    size,
                    ..
                } => {
                    let (led, clean) = flags_of(*style);
                    ClockCfg::Timer {
                        name: name.clone(),
                        duration_ms: *duration_ms,
                        led,
                        clean,
                        size: *size,
                    }
                }
                Clock::Stopwatch {
                    name, style, size, ..
                } => {
                    let (led, clean) = flags_of(*style);
                    ClockCfg::Stopwatch {
                        name: name.clone(),
                        led,
                        clean,
                        size: *size,
                    }
                }
                Clock::Countdown {
                    name,
                    target,
                    style,
                    size,
                    ..
                } => {
                    let (led, clean) = flags_of(*style);
                    ClockCfg::Countdown {
                        name: name.clone(),
                        target_ms: target.as_millisecond(),
                        led,
                        clean,
                        size: *size,
                    }
                }
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
    fn legacy_config_without_clean_or_size() {
        // A file written before the clean/size fields existed must still load,
        // defaulting clean=false (→ style follows `led`) and size=DEFAULT_SIZE.
        let s = r#"
            layout = "grid"
            label_mode = "city"
            ntp_server = "pool.ntp.org"
            led_default = true

            [[clocks]]
            kind = "tz"
            name = "UTC"
            zone = "UTC"
            led = false
        "#;
        let cfg: Config = toml::from_str(s).unwrap();
        let clocks = cfg.to_clocks();
        assert_eq!(clocks.len(), 1);
        assert_eq!(clocks[0].style(), ClockStyle::Plain);
        assert_eq!(clocks[0].size(), DEFAULT_SIZE);
    }

    #[test]
    fn clean_style_round_trips() {
        let clocks = vec![Clock::Tz {
            name: "UTC".into(),
            source: Source::Fixed(0),
            style: ClockStyle::Clean,
            size: 5,
        }];
        let cfg = Config::from_state(&clocks, Layout::Grid, LabelMode::City, "pool.ntp.org", true);
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        let out = back.to_clocks();
        assert_eq!(out[0].style(), ClockStyle::Clean);
        assert_eq!(out[0].size(), 5);
    }

    #[test]
    fn to_clocks_builds_four() {
        let clocks = Config::default().to_clocks();
        assert_eq!(clocks.len(), 4);
        assert!(clocks.iter().all(|c| c.is_tz()));
    }
}
