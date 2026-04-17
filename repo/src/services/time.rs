use chrono::{Duration, NaiveDateTime, Utc};

/// Default offset override via env var `CIVICOPS_LOCAL_OFFSET_MINUTES`.
/// Falls back to the host's local UTC offset when unset or malformed.
/// Value is clamped to the i16 range Postgres expects for `*_offset_minutes`.
fn resolve_offset_minutes() -> i16 {
    if let Ok(raw) = std::env::var("CIVICOPS_LOCAL_OFFSET_MINUTES") {
        if let Ok(v) = raw.trim().parse::<i32>() {
            return v.clamp(-(24 * 60), 24 * 60) as i16;
        }
    }
    // Fallback: derive from the host's local offset without pulling in a TZ crate.
    let local = chrono::Local::now();
    let secs = local.offset().local_minus_utc();
    ((secs / 60).clamp(-(24 * 60), 24 * 60)) as i16
}

/// Returns (local naive timestamp, offset_minutes).
///
/// We store wall-clock in `TIMESTAMP` and carry the UTC offset alongside it so the
/// client can faithfully reproduce the local time the row was written at. This is
/// the requirement called out in the audit: timestamps must represent local time +
/// offset, not UTC with a zero offset.
pub fn now_utc_naive() -> (NaiveDateTime, i16) {
    let off = resolve_offset_minutes();
    let utc_naive = Utc::now().naive_utc();
    let local_naive = utc_naive + Duration::minutes(off as i64);
    (local_naive, off)
}

pub fn format_date_mdy(d: &chrono::NaiveDate) -> String {
    d.format("%m/%d/%Y").to_string()
}

pub fn parse_date_mdy(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s, "%m/%d/%Y").ok()
}

pub fn format_time_12h(t: &chrono::NaiveTime) -> String {
    t.format("%I:%M %p").to_string()
}

pub fn parse_time_12h(s: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(s, "%I:%M %p")
        .or_else(|_| chrono::NaiveTime::parse_from_str(s, "%l:%M %p"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use serial_test::serial;

    #[test]
    fn date_mdy_roundtrip() {
        let d = parse_date_mdy("07/04/2026").unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 7, 4).unwrap());
        assert_eq!(format_date_mdy(&d), "07/04/2026");
    }

    #[test]
    fn bad_date_returns_none() {
        assert!(parse_date_mdy("2026-07-04").is_none());
        assert!(parse_date_mdy("13/01/2026").is_none());
    }

    #[test]
    fn time_12h_roundtrip() {
        let t = parse_time_12h("3:15 PM").unwrap();
        assert_eq!(format_time_12h(&t), "03:15 PM");
    }

    #[test]
    fn time_12h_rejects_24h_format() {
        assert!(parse_time_12h("15:15").is_none());
    }

    #[test]
    #[serial]
    fn env_override_sets_offset() {
        std::env::set_var("CIVICOPS_LOCAL_OFFSET_MINUTES", "-300");
        let (_, off) = now_utc_naive();
        assert_eq!(off, -300);
        std::env::remove_var("CIVICOPS_LOCAL_OFFSET_MINUTES");
    }

    #[test]
    #[serial]
    fn env_override_clamps_absurd_values() {
        std::env::set_var("CIVICOPS_LOCAL_OFFSET_MINUTES", "999999");
        let (_, off) = now_utc_naive();
        assert_eq!(off, 24 * 60);
        std::env::remove_var("CIVICOPS_LOCAL_OFFSET_MINUTES");
    }

    #[test]
    #[serial]
    fn local_timestamp_reflects_offset() {
        std::env::set_var("CIVICOPS_LOCAL_OFFSET_MINUTES", "60");
        let (local, off) = now_utc_naive();
        std::env::remove_var("CIVICOPS_LOCAL_OFFSET_MINUTES");
        assert_eq!(off, 60);
        let utc = Utc::now().naive_utc();
        let delta = (local - utc).num_minutes();
        // Allow a small skew for the few microseconds between the two reads.
        assert!((delta - 60).abs() <= 1, "delta={delta}");
    }
}
