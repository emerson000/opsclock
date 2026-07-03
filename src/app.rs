//! Application state and the keymap dispatcher (mirrors the prototype's `onKey`).

use crate::config::Config;
use crate::model::{Clock, LabelMode, Layout};
use crate::ntp::{self, SyncState};
use crate::time;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jiff::Timestamp;
use std::path::PathBuf;

/// Static NTP server table (name, stratum, cosmetic delay label).
pub const NTP_SERVERS: &[(&str, u8, &str)] = &[
    ("time.nist.gov", 1, "12ms"),
    ("pool.ntp.org", 2, "8ms"),
    ("time.google.com", 1, "6ms"),
    ("time.cloudflare.com", 3, "4ms"),
    ("time.windows.com", 2, "21ms"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    SetTime,
    Timer,
    Add,
}

pub enum Mode {
    Normal,
    Input {
        kind: InputKind,
        buffer: String,
        error: Option<String>,
    },
    Help,
    Ntp {
        sel: usize,
    },
}

/// Active time-conversion state.
pub struct Conv {
    pub instant: Timestamp,
    pub label: String,
}

/// Outcome of handling a key: keep running or quit.
#[derive(PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

pub struct App {
    pub layout: Layout,
    pub clocks: Vec<Clock>,
    pub sel: usize,
    pub label_mode: LabelMode,
    pub mode: Mode,
    pub conv: Option<Conv>,
    pub ntp_server: String,
    pub offset_ms: i64,
    pub ntp: SyncState,
    pub led_default: bool,
    /// Zen mode: hide all chrome and tile decoration, leaving only the time.
    pub zen: bool,
    pub config_path: PathBuf,
    pub blink: bool,
}

impl App {
    pub fn new(cfg: Config, config_path: PathBuf) -> Self {
        let clocks = cfg.to_clocks();
        let clocks = if clocks.is_empty() {
            Config::default().to_clocks()
        } else {
            clocks
        };
        App {
            layout: cfg.layout,
            clocks,
            sel: 0,
            label_mode: cfg.label_mode,
            mode: Mode::Normal,
            conv: None,
            ntp_server: cfg.ntp_server,
            offset_ms: 0,
            ntp: SyncState::Idle,
            led_default: cfg.led_default,
            zen: false,
            config_path,
            blink: false,
        }
    }

    /// System time adjusted by the measured NTP offset.
    pub fn now(&self) -> Timestamp {
        Timestamp::from_millisecond(Timestamp::now().as_millisecond() + self.offset_ms)
            .unwrap_or_else(|_| Timestamp::now())
    }

    /// The instant clocks should display (converted instant, or live now).
    pub fn disp(&self) -> Timestamp {
        self.conv
            .as_ref()
            .map(|c| c.instant)
            .unwrap_or_else(|| self.now())
    }

    fn n(&self) -> usize {
        self.clocks.len()
    }

    /// Count existing clocks of a kind, for auto-naming (TIMER / TIMER 2 …).
    fn kind_name(&self, base: &str, is_timer: bool) -> String {
        let count = self
            .clocks
            .iter()
            .filter(|c| match c {
                Clock::Timer { .. } => is_timer,
                Clock::Stopwatch { .. } => !is_timer,
                _ => false,
            })
            .count();
        if count == 0 {
            base.to_string()
        } else {
            format!("{} {}", base, count + 1)
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Flow {
        // Global quit.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c'))
        {
            return Flow::Quit;
        }

        match &self.mode {
            Mode::Input { .. } => self.on_key_input(key),
            Mode::Help => self.on_key_help(key),
            Mode::Ntp { .. } => self.on_key_ntp(key),
            Mode::Normal => return self.on_key_normal(key),
        }
        Flow::Continue
    }

    fn on_key_input(&mut self, key: KeyEvent) {
        let Mode::Input { buffer, error, .. } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.submit_input(),
            KeyCode::Backspace => {
                buffer.pop();
                *error = None;
            }
            KeyCode::Char(c) if buffer.chars().count() < 32 => {
                buffer.push(c);
                *error = None;
            }
            _ => {}
        }
    }

    fn on_key_help(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            self.mode = Mode::Normal;
        }
    }

    fn on_key_ntp(&mut self, key: KeyEvent) {
        let count = NTP_SERVERS.len();
        let Mode::Ntp { sel } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => *sel = (*sel + 1) % count,
            KeyCode::Up | KeyCode::Char('k') => *sel = (*sel + count - 1) % count,
            KeyCode::Enter => {
                let idx = *sel;
                if !matches!(self.ntp, SyncState::Syncing(_)) {
                    self.ntp_server = NTP_SERVERS[idx].0.to_string();
                    self.ntp = SyncState::Syncing(ntp::spawn_sync(self.ntp_server.clone()));
                }
            }
            KeyCode::Esc | KeyCode::Char('n') => self.mode = Mode::Normal,
            _ => {}
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent) -> Flow {
        let now = self.now();
        let n = self.n();
        match key.code {
            KeyCode::Right | KeyCode::Tab | KeyCode::Down => self.sel = (self.sel + 1) % n,
            KeyCode::Left | KeyCode::Up | KeyCode::BackTab => self.sel = (self.sel + n - 1) % n,
            KeyCode::Esc => {
                // Esc gives an obvious way out of zen mode; otherwise resume live.
                if self.zen {
                    self.zen = false;
                } else {
                    self.conv = None;
                }
            }
            KeyCode::Char(c) => return self.on_char(c, now),
            _ => {}
        }
        Flow::Continue
    }

    fn on_char(&mut self, c: char, now: Timestamp) -> Flow {
        match c {
            'q' => return Flow::Quit,
            '1' => self.layout = Layout::Grid,
            '2' => self.layout = Layout::Split,
            '3' => self.layout = Layout::Sidebar,
            '4' => self.layout = Layout::Wall,
            'l' => {
                let v = !self.clocks[self.sel].led();
                self.clocks[self.sel].set_led(v);
            }
            'o' => {
                self.label_mode = match self.label_mode {
                    LabelMode::City => LabelMode::Mil,
                    LabelMode::Mil => LabelMode::City,
                }
            }
            'z' => self.zen = !self.zen,
            't' => {
                if self.clocks[self.sel].is_tz() {
                    self.mode = Mode::Input {
                        kind: InputKind::SetTime,
                        buffer: String::new(),
                        error: None,
                    };
                }
            }
            'T' => {
                self.mode = Mode::Input {
                    kind: InputKind::Timer,
                    buffer: String::new(),
                    error: None,
                }
            }
            's' => {
                let name = self.kind_name("STOPWATCH", false);
                self.clocks.push(Clock::Stopwatch {
                    name,
                    elapsed_ms: 0,
                    running: true,
                    last_start: now,
                    led: self.led_default,
                });
                self.sel = self.clocks.len() - 1;
            }
            ' ' => self.clocks[self.sel].on_space(now),
            'r' => self.clocks[self.sel].on_reset(now),
            'a' => {
                self.mode = Mode::Input {
                    kind: InputKind::Add,
                    buffer: String::new(),
                    error: None,
                }
            }
            'x' => {
                if self.n() > 1 {
                    self.clocks.remove(self.sel);
                    self.sel = self.sel.min(self.clocks.len() - 1);
                }
            }
            'n' => {
                let sel = NTP_SERVERS
                    .iter()
                    .position(|s| s.0 == self.ntp_server)
                    .unwrap_or(0);
                self.mode = Mode::Ntp { sel };
            }
            '?' => self.mode = Mode::Help,
            _ => {}
        }
        Flow::Continue
    }

    fn submit_input(&mut self) {
        let (kind, raw) = match &self.mode {
            Mode::Input { kind, buffer, .. } => (*kind, buffer.trim().to_string()),
            _ => return,
        };
        match kind {
            InputKind::Timer => self.submit_timer(&raw),
            InputKind::Add => self.submit_add(&raw),
            InputKind::SetTime => self.submit_settime(&raw),
        }
    }

    fn set_input_error(&mut self, msg: &str) {
        if let Mode::Input { error, .. } = &mut self.mode {
            *error = Some(msg.to_string());
        }
    }

    fn submit_timer(&mut self, raw: &str) {
        // A relative duration (20m / 1h30m / 00:20:00) makes a countdown timer.
        if let Some(dur) = time::parse_duration(raw) {
            if dur > 0 {
                let name = self.kind_name("TIMER", true);
                self.clocks.push(Clock::Timer {
                    name,
                    duration_ms: dur,
                    elapsed_ms: 0,
                    running: true,
                    last_start: self.now(),
                    led: self.led_default,
                    notified: false,
                });
                self.sel = self.clocks.len() - 1;
                self.mode = Mode::Normal;
                return;
            }
        }
        // Otherwise, a natural-language target date/time ("tomorrow at 12pm").
        let tz = jiff::tz::TimeZone::system();
        if let Some(target) = time::parse_target(raw, self.now(), &tz) {
            let name = self.countdown_name();
            self.clocks.push(Clock::Countdown {
                name,
                target,
                led: self.led_default,
                notified: false,
            });
            self.sel = self.clocks.len() - 1;
            self.mode = Mode::Normal;
            return;
        }
        self.set_input_error("BAD DURATION / DATE");
    }

    fn countdown_name(&self) -> String {
        let n = self
            .clocks
            .iter()
            .filter(|c| matches!(c, Clock::Countdown { .. }))
            .count();
        if n == 0 {
            "COUNTDOWN".to_string()
        } else {
            format!("COUNTDOWN {}", n + 1)
        }
    }

    fn submit_add(&mut self, raw: &str) {
        match crate::cities::resolve(raw) {
            Some((name, source)) => {
                self.clocks.push(Clock::Tz {
                    name,
                    source,
                    led: self.led_default,
                });
                self.sel = self.clocks.len() - 1;
                self.mode = Mode::Normal;
            }
            None => self.set_input_error("UNKNOWN CITY / OFFSET"),
        }
    }

    fn submit_settime(&mut self, raw: &str) {
        let parsed = parse_settime(raw);
        let Some((h, mi, tomorrow)) = parsed else {
            self.set_input_error("USE HH:MM [TOMORROW]");
            return;
        };
        let (name, source) = match &self.clocks[self.sel] {
            Clock::Tz { name, source, .. } => (name.clone(), source.clone()),
            _ => return,
        };
        let now = self.now();
        let w = time::wall_of(&source, now);
        // Civil date in the clock's zone, +1 day if "tomorrow".
        let mut date = jiff::civil::Date::new(w.year, w.month, w.day).expect("valid date");
        if tomorrow {
            date = date.tomorrow().unwrap_or(date);
        }
        let instant = time::zoned_instant(&source, date.year(), date.month(), date.day(), h, mi);
        let label = format!(
            "SHOWING {:02}:{:02}{} @ {} — ALL CLOCKS CONVERTED",
            h,
            mi,
            if tomorrow { " TOMORROW" } else { "" },
            name
        );
        self.conv = Some(Conv { instant, label });
        self.mode = Mode::Normal;
    }

    /// Per-frame update: refresh blink phase, poll NTP, fire timer notifications.
    pub fn tick(&mut self) {
        self.blink = (Timestamp::now().as_millisecond() / 500) % 2 == 0;

        // Poll the NTP background thread.
        if let SyncState::Syncing(rx) = &self.ntp {
            match rx.try_recv() {
                Ok(Ok(offset)) => {
                    self.offset_ms = offset;
                    self.ntp = SyncState::Idle;
                }
                Ok(Err(msg)) => self.ntp = SyncState::Failed(msg),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ntp = SyncState::Failed("SYNC FAILED".into());
                }
            }
        }

        // Desktop notification when a running timer first crosses zero.
        let now = self.now();
        for c in &mut self.clocks {
            if let Clock::Timer {
                name,
                duration_ms,
                elapsed_ms,
                running,
                last_start,
                notified,
                ..
            } = c
            {
                if *running && !*notified {
                    let cur = *elapsed_ms + crate::model::ms_between(*last_start, now).max(0);
                    if cur >= *duration_ms {
                        notify_timer(name);
                        *notified = true;
                    }
                }
            } else if let Clock::Countdown {
                name,
                target,
                notified,
                ..
            } = c
            {
                if !*notified && now.as_millisecond() >= target.as_millisecond() {
                    notify_timer(name);
                    *notified = true;
                }
            }
        }
    }

    /// Persist current state to the config file.
    pub fn save(&self) {
        let cfg = Config::from_state(
            &self.clocks,
            self.layout,
            self.label_mode,
            &self.ntp_server,
            self.led_default,
        );
        cfg.save(&self.config_path);
    }
}

/// Parse `HH:MM [tomorrow|tmw|+1|today]` (case-insensitive) → (h, mi, tomorrow).
fn parse_settime(raw: &str) -> Option<(i8, i8, bool)> {
    let s = raw.trim().to_lowercase();
    let mut it = s.splitn(2, |c: char| c.is_whitespace());
    let time_part = it.next()?;
    let word = it.next().map(|w| w.trim()).filter(|w| !w.is_empty());

    let (h_str, m_str) = time_part.split_once(':')?;
    if h_str.is_empty() || h_str.len() > 2 || !h_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if m_str.len() != 2 || !m_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let h: i8 = h_str.parse().ok()?;
    let mi: i8 = m_str.parse().ok()?;
    if h > 23 || mi > 59 {
        return None;
    }
    let tomorrow = match word {
        None | Some("today") => false,
        Some("tomorrow") | Some("tmw") | Some("+1") => true,
        Some(_) => return None,
    };
    Some((h, mi, tomorrow))
}

/// Desktop notification when a timer reaches zero (behind the `notify` feature).
#[cfg(feature = "notify")]
pub fn notify_timer(name: &str) {
    let _ = notify_rust::Notification::new()
        .summary("OPSCLOCK")
        .body(&format!("{name} — T-00:00:00"))
        .show();
}

#[cfg(not(feature = "notify"))]
pub fn notify_timer(_name: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn app4() -> App {
        App::new(Config::default(), PathBuf::from("/tmp/opsclock-test.toml"))
    }
    fn press(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn select_wraps() {
        let mut a = app4();
        a.sel = 3;
        a.on_key(key(KeyCode::Right));
        assert_eq!(a.sel, 0);
        a.on_key(key(KeyCode::Left));
        assert_eq!(a.sel, 3);
    }

    #[test]
    fn layout_keys() {
        let mut a = app4();
        a.on_key(press('2'));
        assert_eq!(a.layout, Layout::Split);
        a.on_key(press('4'));
        assert_eq!(a.layout, Layout::Wall);
    }

    #[test]
    fn add_timer_flow() {
        let mut a = app4();
        a.on_key(press('T'));
        for ch in "20m".chars() {
            a.on_key(press(ch));
        }
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.clocks.len(), 5);
        assert!(matches!(a.clocks[4], Clock::Timer { .. }));
        assert_eq!(a.sel, 4);
    }

    #[test]
    fn timer_prompt_accepts_date_target() {
        let mut a = app4();
        a.on_key(press('T'));
        for ch in "tomorrow at 12pm".chars() {
            a.on_key(press(ch));
        }
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.clocks.len(), 5);
        assert!(matches!(a.clocks[4], Clock::Countdown { .. }));
        assert_eq!(a.clocks[4].name(), "COUNTDOWN");
        assert!(matches!(a.mode, Mode::Normal));
    }

    #[test]
    fn bad_timer_input_keeps_prompt() {
        let mut a = app4();
        a.on_key(press('T'));
        for ch in "wibble".chars() {
            a.on_key(press(ch));
        }
        a.on_key(key(KeyCode::Enter));
        assert!(matches!(a.mode, Mode::Input { error: Some(_), .. }));
        assert_eq!(a.clocks.len(), 4);
    }

    #[test]
    fn bad_duration_keeps_input() {
        let mut a = app4();
        a.on_key(press('T'));
        for ch in "zz".chars() {
            a.on_key(press(ch));
        }
        a.on_key(key(KeyCode::Enter));
        assert!(matches!(a.mode, Mode::Input { error: Some(_), .. }));
        assert_eq!(a.clocks.len(), 4);
    }

    #[test]
    fn close_refuses_last() {
        let mut a = app4();
        for _ in 0..3 {
            a.on_key(press('x'));
        }
        assert_eq!(a.clocks.len(), 1);
        a.on_key(press('x'));
        assert_eq!(a.clocks.len(), 1);
    }

    #[test]
    fn stopwatch_instant() {
        let mut a = app4();
        a.on_key(press('s'));
        assert_eq!(a.clocks.len(), 5);
        assert!(matches!(a.clocks[4], Clock::Stopwatch { .. }));
        assert_eq!(a.sel, 4);
    }

    #[test]
    fn add_city_flow() {
        let mut a = app4();
        a.on_key(press('a'));
        for ch in "paris".chars() {
            a.on_key(press(ch));
        }
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.clocks.len(), 5);
        assert_eq!(a.clocks[4].name(), "PARIS");
    }

    #[test]
    fn settime_sets_conv() {
        let mut a = app4();
        a.sel = 2; // TOKYO
        a.on_key(press('t'));
        for ch in "17:00 tomorrow".chars() {
            a.on_key(press(ch));
        }
        a.on_key(key(KeyCode::Enter));
        assert!(a.conv.is_some());
        assert!(a.conv.as_ref().unwrap().label.contains("TOMORROW"));
        a.on_key(key(KeyCode::Esc));
        assert!(a.conv.is_none());
    }

    #[test]
    fn label_toggle() {
        let mut a = app4();
        a.on_key(press('o'));
        assert_eq!(a.label_mode, LabelMode::Mil);
    }

    #[test]
    fn settime_parser() {
        assert_eq!(parse_settime("17:00"), Some((17, 0, false)));
        assert_eq!(parse_settime("09:30 tomorrow"), Some((9, 30, true)));
        assert_eq!(parse_settime("9:05 +1"), Some((9, 5, true)));
        assert_eq!(parse_settime("25:00"), None);
        assert_eq!(parse_settime("17:0"), None);
    }
}
