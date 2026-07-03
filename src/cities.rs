//! Resolve a user query ("tokyo", "UTC+5:30") to a display name + clock source.
//! Order: curated city substring → jiff tzdb city match → fixed-offset parse.

use crate::model::Source;
use crate::time::offset_str;

/// The prototype's curated 32-city list: (display name, IANA zone).
pub const CITIES: &[(&str, &str)] = &[
    ("UTC", "UTC"),
    ("LONDON", "Europe/London"),
    ("PARIS", "Europe/Paris"),
    ("BERLIN", "Europe/Berlin"),
    ("CAIRO", "Africa/Cairo"),
    ("MOSCOW", "Europe/Moscow"),
    ("DUBAI", "Asia/Dubai"),
    ("KARACHI", "Asia/Karachi"),
    ("DELHI", "Asia/Kolkata"),
    ("MUMBAI", "Asia/Kolkata"),
    ("DHAKA", "Asia/Dhaka"),
    ("BANGKOK", "Asia/Bangkok"),
    ("SINGAPORE", "Asia/Singapore"),
    ("HONG KONG", "Asia/Hong_Kong"),
    ("SHANGHAI", "Asia/Shanghai"),
    ("TOKYO", "Asia/Tokyo"),
    ("SEOUL", "Asia/Seoul"),
    ("SYDNEY", "Australia/Sydney"),
    ("AUCKLAND", "Pacific/Auckland"),
    ("HONOLULU", "Pacific/Honolulu"),
    ("ANCHORAGE", "America/Anchorage"),
    ("LOS ANGELES", "America/Los_Angeles"),
    ("DENVER", "America/Denver"),
    ("CHICAGO", "America/Chicago"),
    ("HOUSTON", "America/Chicago"),
    ("NEW YORK", "America/New_York"),
    ("SAO PAULO", "America/Sao_Paulo"),
    ("BUENOS AIRES", "America/Argentina/Buenos_Aires"),
    ("REYKJAVIK", "Atlantic/Reykjavik"),
    ("JOHANNESBURG", "Africa/Johannesburg"),
    ("KYIV", "Europe/Kyiv"),
    ("ISTANBUL", "Europe/Istanbul"),
];

/// Resolve a query to (display name, Source), or None on failure.
pub fn resolve(query: &str) -> Option<(String, Source)> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let up = q.to_uppercase();

    // 1. Curated substring match.
    if let Some((name, zone)) = CITIES.iter().find(|(name, _)| name.contains(&up as &str)) {
        if let Ok(tz) = jiff::tz::TimeZone::get(zone) {
            return Some((name.to_string(), Source::Zone(tz)));
        }
    }

    // 2. Fixed offset — takes priority over tzdb so "-8" isn't swallowed by
    //    a POSIX-style `Etc/GMT-8` zone name.
    if let Some(off) = parse_offset(q) {
        return Some((format!("UTC{}", offset_str(off)), Source::Fixed(off)));
    }

    // 3. tzdb city match (last path segment, `_`→space).
    tzdb_match(&up).map(|(name, tz)| (name, Source::Zone(tz)))
}

fn tzdb_match(up: &str) -> Option<(String, jiff::tz::TimeZone)> {
    for name in jiff::tz::db().available() {
        let zone = name.as_str();
        let city = zone.rsplit('/').next().unwrap_or(zone).replace('_', " ");
        if city.to_uppercase().contains(up) {
            if let Ok(tz) = jiff::tz::TimeZone::get(zone) {
                return Some((city.to_uppercase(), tz));
            }
        }
    }
    None
}

/// Parse `[utc]±H[:MM]` (case-insensitive) to signed offset minutes.
fn parse_offset(s: &str) -> Option<i32> {
    let mut t = s.trim().to_lowercase();
    if let Some(rest) = t.strip_prefix("utc") {
        t = rest.trim().to_string();
    }
    let mut chars = t.chars().peekable();
    let sign = match chars.peek() {
        Some('+') => {
            chars.next();
            1
        }
        Some('-') => {
            chars.next();
            -1
        }
        _ => return None,
    };
    let rest: String = chars.collect();
    let (h_str, m_str) = match rest.split_once(':') {
        Some((h, m)) => (h, m),
        None => (rest.as_str(), "0"),
    };
    if h_str.is_empty() || h_str.len() > 2 || !h_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if m_str.is_empty() || m_str.len() > 2 || !m_str.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let h: i32 = h_str.parse().ok()?;
    let m: i32 = m_str.parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some(sign * (h * 60 + m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_hit() {
        let (name, src) = resolve("tok").unwrap();
        assert_eq!(name, "TOKYO");
        assert!(matches!(src, Source::Zone(_)));
    }

    #[test]
    fn curated_multiword() {
        let (name, _) = resolve("new york").unwrap();
        assert_eq!(name, "NEW YORK");
    }

    #[test]
    fn offset_parse() {
        let (name, src) = resolve("UTC+5:30").unwrap();
        assert_eq!(name, "UTC+05:30");
        assert!(matches!(src, Source::Fixed(330)));
    }

    #[test]
    fn neg_offset() {
        let (_n, src) = resolve("-8").unwrap();
        assert!(matches!(src, Source::Fixed(-480)));
    }

    #[test]
    fn tzdb_fallback() {
        // Not in the curated list, but is a real IANA city.
        let (name, src) = resolve("kathmandu").unwrap();
        assert_eq!(name, "KATHMANDU");
        assert!(matches!(src, Source::Zone(_)));
    }

    #[test]
    fn garbage() {
        assert!(resolve("zzzzz").is_none());
        assert!(resolve("").is_none());
    }
}
