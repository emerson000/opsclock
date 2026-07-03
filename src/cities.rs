//! Resolve a user query ("tokyo", "atlanta", "UTC+5:30") to a display name +
//! clock source. Order: curated city table (exact → prefix → substring) →
//! fixed-offset parse → jiff tzdb city match.

use crate::citydb::CITIES;
use crate::model::Source;
use crate::time::offset_str;

/// Rank a query against the curated city table: exact match wins, then a city
/// name that starts with the query, then any substring match.
fn match_city(up: &str) -> Option<&'static (&'static str, &'static str)> {
    CITIES
        .iter()
        .find(|(name, _)| *name == up)
        .or_else(|| CITIES.iter().find(|(name, _)| name.starts_with(up)))
        .or_else(|| CITIES.iter().find(|(name, _)| name.contains(up)))
}

/// Resolve a query to (display name, Source), or None on failure.
pub fn resolve(query: &str) -> Option<(String, Source)> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    let up = q.to_uppercase();

    // 1. Curated city table.
    if let Some((name, zone)) = match_city(&up) {
        if let Ok(tz) = jiff::tz::TimeZone::get(zone) {
            return Some((name.to_string(), Source::Zone(tz)));
        }
    }

    // 2. Fixed offset — takes priority over tzdb so "-8" isn't swallowed by
    //    a POSIX-style `Etc/GMT-8` zone name.
    if let Some(off) = parse_offset(q) {
        return Some((format!("UTC{}", offset_str(off)), Source::Fixed(off)));
    }

    // 3. tzdb city match (last path segment, `_`→space) for any IANA city not
    //    in the curated table.
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

    fn zone_of(src: &Source) -> Option<String> {
        match src {
            Source::Zone(tz) => tz.iana_name().map(|s| s.to_string()),
            Source::Fixed(_) => None,
        }
    }

    #[test]
    fn curated_hit() {
        let (name, src) = resolve("tok").unwrap();
        assert_eq!(name, "TOKYO");
        assert!(matches!(src, Source::Zone(_)));
    }

    #[test]
    fn major_cities_without_own_zone() {
        // Cities that have no IANA zone of their own must still resolve.
        for (query, name, zone) in [
            ("atlanta", "ATLANTA", "America/New_York"),
            ("boston", "BOSTON", "America/New_York"),
            ("seattle", "SEATTLE", "America/Los_Angeles"),
            ("dallas", "DALLAS", "America/Chicago"),
            ("miami", "MIAMI", "America/New_York"),
            ("toronto", "TORONTO", "America/Toronto"),
            ("mumbai", "MUMBAI", "Asia/Kolkata"),
            ("cape town", "CAPE TOWN", "Africa/Johannesburg"),
            ("melbourne", "MELBOURNE", "Australia/Melbourne"),
        ] {
            let (got_name, src) = resolve(query).unwrap_or_else(|| panic!("{query} unresolved"));
            assert_eq!(got_name, name, "name for {query}");
            assert_eq!(zone_of(&src).as_deref(), Some(zone), "zone for {query}");
        }
    }

    #[test]
    fn exact_beats_substring() {
        // "paris" is exact even though other names contain the substring.
        assert_eq!(resolve("paris").unwrap().0, "PARIS");
        // "york" only appears inside NEW YORK.
        assert_eq!(resolve("york").unwrap().0, "NEW YORK");
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
