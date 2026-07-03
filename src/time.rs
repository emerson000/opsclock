//! Pure time helpers: duration/offset parsing, military-zone letters, and
//! jiff-backed wall-time + set-time conversion. No terminal or I/O.

use crate::model::Source;
use jiff::{civil, tz::Offset, Timestamp};

/// Zero-padded two digits.
fn p2(n: i64) -> String {
    format!("{:02}", n)
}

/// `+05:30` / `-08:00` from a signed offset in minutes.
pub fn offset_str(off_min: i32) -> String {
    let sign = if off_min < 0 { '-' } else { '+' };
    let a = off_min.unsigned_abs();
    format!("{}{}:{}", sign, p2((a / 60) as i64), p2((a % 60) as i64))
}

/// `HH:MM:SS` from milliseconds, clamped at zero. Hours may exceed 99.
pub fn fmt_dur(ms: i64) -> String {
    let t = (ms.max(0) as f64 / 1000.0).round() as i64;
    format!("{}:{}:{}", p2(t / 3600), p2((t / 60) % 60), p2(t % 60))
}

/// Parse a timer duration into milliseconds. Accepts `H:MM:SS`, `MM:SS`, or a
/// sum of `\d+(h|m|s)` tokens (`1h30m`, `90s`, `20m`). Returns None if nothing
/// parsed. The caller rejects non-positive totals.
pub fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    // Colon form: 1-3 digits, then one or two ":dd" groups.
    if let Some(ms) = parse_colon(&s) {
        return Some(ms);
    }
    // Unit-token form.
    parse_units(&s)
}

fn parse_colon(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    // First field 1-3 digits, rest exactly 2 digits, all numeric.
    let first = parts[0];
    if first.is_empty() || first.len() > 3 || !first.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    for p in &parts[1..] {
        if p.len() != 2 || !p.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    let nums: Vec<i64> = parts.iter().map(|p| p.parse().unwrap()).collect();
    let secs = if nums.len() == 3 {
        nums[0] * 3600 + nums[1] * 60 + nums[2]
    } else {
        nums[0] * 60 + nums[1]
    };
    Some(secs * 1000)
}

fn parse_units(s: &str) -> Option<i64> {
    let mut total: i64 = 0;
    let mut any = false;
    let mut num = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
        } else if matches!(ch, 'h' | 'm' | 's') {
            if num.is_empty() {
                return None;
            }
            let n: i64 = num.parse().ok()?;
            let mult = match ch {
                'h' => 3600,
                'm' => 60,
                _ => 1,
            };
            total += n * mult * 1000;
            any = true;
            num.clear();
        } else if ch.is_whitespace() {
            // allow spaces between tokens
        } else {
            return None;
        }
    }
    // Trailing bare number with no unit is invalid.
    if !num.is_empty() {
        return None;
    }
    if any {
        Some(total)
    } else {
        None
    }
}

/// NATO phonetic letter + word for a whole-hour offset, else None.
/// +1..+12 → A..M (J skipped); 0 → Z (ZULU); -1..-12 → N..Y.
pub fn mil(off_min: i32) -> Option<(char, &'static str)> {
    if off_min % 60 != 0 {
        return None;
    }
    let h = off_min / 60;
    const PL: &[u8] = b"ABCDEFGHIKLM";
    const PW: [&str; 12] = [
        "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT", "GOLF", "HOTEL", "INDIA", "KILO",
        "LIMA", "MIKE",
    ];
    const NL: &[u8] = b"NOPQRSTUVWXY";
    const NW: [&str; 12] = [
        "NOVEMBER", "OSCAR", "PAPA", "QUEBEC", "ROMEO", "SIERRA", "TANGO", "UNIFORM", "VICTOR",
        "WHISKEY", "XRAY", "YANKEE",
    ];
    if h == 0 {
        Some(('Z', "ZULU"))
    } else if (1..=12).contains(&h) {
        Some((PL[(h - 1) as usize] as char, PW[(h - 1) as usize]))
    } else if (-12..=-1).contains(&h) {
        let i = (-h - 1) as usize;
        Some((NL[i] as char, NW[i]))
    } else {
        None
    }
}

/// Civil wall-clock parts for a clock at a given instant, plus its UTC offset.
#[derive(Clone, Debug)]
pub struct Wall {
    pub year: i16,
    pub month: i8,
    pub day: i8,
    pub hour: i8,
    pub min: i8,
    pub sec: i8,
    pub weekday: &'static str,
    pub off_min: i32,
}

const WD: [&str; 7] = ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"];

fn wall_from_dt(dt: civil::DateTime, off_min: i32) -> Wall {
    // jiff Weekday::to_monday_zero_offset(): Monday=0 .. Sunday=6
    let widx = dt.weekday().to_monday_zero_offset() as usize;
    Wall {
        year: dt.year(),
        month: dt.month(),
        day: dt.day(),
        hour: dt.hour(),
        min: dt.minute(),
        sec: dt.second(),
        weekday: WD[widx],
        off_min,
    }
}

/// Wall-clock parts for `source` at instant `at`.
pub fn wall_of(source: &Source, at: Timestamp) -> Wall {
    match source {
        Source::Zone(tz) => {
            let off_secs = tz.to_offset(at).seconds();
            let dt = at.to_zoned(tz.clone()).datetime();
            wall_from_dt(dt, off_secs / 60)
        }
        Source::Fixed(off_min) => {
            let off = Offset::from_seconds(off_min * 60).expect("valid fixed offset");
            let dt = off.to_datetime(at);
            wall_from_dt(dt, *off_min)
        }
    }
}

/// The instant meaning "`h:mi` wall time on `y-mo-d` civil date in this zone".
/// jiff resolves DST gaps/folds; we fall back to the raw civil instant on error.
pub fn zoned_instant(source: &Source, y: i16, mo: i8, d: i8, h: i8, mi: i8) -> Timestamp {
    let dt = civil::DateTime::new(y, mo, d, h, mi, 0, 0).expect("valid civil datetime");
    match source {
        Source::Zone(tz) => dt
            .to_zoned(tz.clone())
            .map(|z| z.timestamp())
            .unwrap_or_else(|_| Timestamp::UNIX_EPOCH),
        Source::Fixed(off_min) => {
            let off = Offset::from_seconds(off_min * 60).expect("valid fixed offset");
            off.to_timestamp(dt).unwrap_or(Timestamp::UNIX_EPOCH)
        }
    }
}

/// Day number (days since 1970-01-01) for a wall date — used for day-delta chips.
/// Howard Hinnant's `days_from_civil` algorithm; valid for the Gregorian calendar.
pub fn day_number(w: &Wall) -> i64 {
    let (mut y, m, d) = (w.year as i64, w.month as i64, w.day as i64);
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    #[test]
    fn dur_units() {
        assert_eq!(parse_duration("20m"), Some(20 * 60 * 1000));
        assert_eq!(parse_duration("1h30m"), Some(90 * 60 * 1000));
        assert_eq!(parse_duration("90s"), Some(90 * 1000));
    }

    #[test]
    fn dur_colon() {
        assert_eq!(parse_duration("00:20:00"), Some(20 * 60 * 1000));
        assert_eq!(parse_duration("5:00"), Some(5 * 60 * 1000));
        assert_eq!(parse_duration("1:02:03"), Some((3600 + 2 * 60 + 3) * 1000));
    }

    #[test]
    fn dur_bad() {
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("12"), None); // bare number, no unit
        assert_eq!(parse_duration("1:2:3"), None); // wrong field widths
    }

    #[test]
    fn offset_fmt() {
        assert_eq!(offset_str(330), "+05:30");
        assert_eq!(offset_str(-480), "-08:00");
        assert_eq!(offset_str(0), "+00:00");
    }

    #[test]
    fn military() {
        assert_eq!(mil(60), Some(('A', "ALPHA")));
        assert_eq!(mil(9 * 60), Some(('I', "INDIA")));
        assert_eq!(mil(0), Some(('Z', "ZULU")));
        assert_eq!(mil(-60), Some(('N', "NOVEMBER")));
        assert_eq!(mil(-12 * 60), Some(('Y', "YANKEE")));
        assert_eq!(mil(330), None);
        assert_eq!(mil(13 * 60), None);
    }

    #[test]
    fn dur_fmt() {
        assert_eq!(fmt_dur(0), "00:00:00");
        assert_eq!(fmt_dur(-5), "00:00:00");
        assert_eq!(fmt_dur(3661 * 1000), "01:01:01");
    }

    #[test]
    fn tokyo_offset() {
        let tz = Source::Zone(jiff::tz::TimeZone::get("Asia/Tokyo").unwrap());
        let ts: Timestamp = "2026-07-03T00:00:00Z".parse().unwrap();
        let w = wall_of(&tz, ts);
        assert_eq!(w.off_min, 540);
        assert_eq!(w.hour, 9);
    }

    #[test]
    fn fixed_offset_wall() {
        let s = Source::Fixed(330);
        let ts: Timestamp = "2026-07-03T00:00:00Z".parse().unwrap();
        let w = wall_of(&s, ts);
        assert_eq!(w.off_min, 330);
        assert_eq!((w.hour, w.min), (5, 30));
    }

    #[test]
    fn settime_roundtrip() {
        let tz = Source::Zone(jiff::tz::TimeZone::get("Asia/Tokyo").unwrap());
        let inst = zoned_instant(&tz, 2026, 7, 3, 17, 0);
        let w = wall_of(&tz, inst);
        assert_eq!((w.hour, w.min), (17, 0));
    }

    #[test]
    fn settime_dst_gap() {
        // US spring-forward 2026-03-08 02:30 America/Chicago does not exist;
        // jiff must resolve it without panicking.
        let tz = Source::Zone(jiff::tz::TimeZone::get("America/Chicago").unwrap());
        let inst = zoned_instant(&tz, 2026, 3, 8, 2, 30);
        assert!(inst != Timestamp::UNIX_EPOCH);
    }
}
