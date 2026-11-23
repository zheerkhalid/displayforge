//! Time zones and calendar arithmetic without a date library.
//!
//! Times are UTC seconds since 1970 (`i64`). A [`Tz`] gives the offset of
//! local time at an instant, so daylight-saving changes are handled by asking
//! for the offset at the instant in question.

/// A time zone: seconds east of UTC at a UTC instant.
pub trait Tz: Send + Sync {
    fn offset_at(&self, utc: i64) -> i32;
}

/// The time zone Windows is set to, including its daylight-saving rules.
pub struct SystemTz;

impl Tz for SystemTz {
    fn offset_at(&self, utc: i64) -> i32 {
        crate::sys::utc_offset_at(utc)
    }
}

/// A fixed offset (tests, demo).
pub struct FixedTz(pub i32);

impl Tz for FixedTz {
    fn offset_at(&self, _utc: i64) -> i32 {
        self.0
    }
}

/// A test zone with European-style daylight saving: `base` seconds east of
/// UTC in winter, `base + 3600` from the last Sunday of March 01:00 UTC to
/// the last Sunday of October 01:00 UTC.
pub struct EuTz(pub i32);

impl Tz for EuTz {
    fn offset_at(&self, utc: i64) -> i32 {
        let (y, _, _) = civil_from_days(utc.div_euclid(86_400));
        let start = last_sunday(y, 3) * 86_400 + 3600;
        let end = last_sunday(y, 10) * 86_400 + 3600;
        if utc >= start && utc < end {
            self.0 + 3600
        } else {
            self.0
        }
    }
}

/// Days since 1970-01-01 of the last Sunday of a month.
pub fn last_sunday(year: i32, month: u32) -> i64 {
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    let last = next - 1;
    // 1970-01-01 was a Thursday; weekday 0 = Sunday.
    let wd = (last + 4).rem_euclid(7);
    last - wd
}

/// Days since 1970-01-01 for a civil date (proleptic Gregorian).
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date (year, month, day) of a day number since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// The local day number (days since 1970-01-01 in local time) of an instant.
pub fn local_day(utc: i64, tz: &dyn Tz) -> i64 {
    (utc + tz.offset_at(utc) as i64).div_euclid(86_400)
}

/// Minutes after local midnight of an instant (0..1440).
pub fn local_minutes(utc: i64, tz: &dyn Tz) -> u32 {
    ((utc + tz.offset_at(utc) as i64).rem_euclid(86_400) / 60) as u32
}

/// The UTC instant of a local wall-clock time on a local day. In the hour
/// skipped by a daylight-saving change the time is moved forward; in the
/// hour that repeats, the first occurrence is used.
pub fn local_to_utc(day: i64, minutes: u32, tz: &dyn Tz) -> i64 {
    let wall = day * 86_400 + minutes as i64 * 60;
    // Try both offsets around the instant; prefer the earliest that maps back.
    let guess1 = wall - tz.offset_at(wall - 86_400 / 2) as i64;
    let guess2 = wall - tz.offset_at(wall + 86_400 / 2) as i64;
    let mut best: Option<i64> = None;
    for g in [guess1, guess2] {
        if g + tz.offset_at(g) as i64 == wall {
            best = Some(best.map_or(g, |b: i64| b.min(g)));
        }
    }
    best.unwrap_or_else(|| {
        // In a gap: use the offset from before the change, which lands after it.
        guess1.max(guess2)
    })
}

/// The current time, UTC seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// "07:45" for minutes after midnight.
pub fn fmt_minutes(minutes: u32) -> String {
    format!("{:02}:{:02}", (minutes / 60) % 24, minutes % 60)
}

/// "07:45" local time of an instant.
pub fn fmt_hm(utc: i64, tz: &dyn Tz) -> String {
    fmt_minutes(local_minutes(utc, tz))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 11, 24), (2100, 12, 31)] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn eu_dst_boundaries() {
        // 2026: DST from Sunday 29 March to Sunday 25 October.
        assert_eq!(civil_from_days(last_sunday(2026, 3)), (2026, 3, 29));
        assert_eq!(civil_from_days(last_sunday(2026, 10)), (2026, 10, 25));
        let tz = EuTz(3600);
        let day = days_from_civil(2026, 3, 29);
        assert_eq!(tz.offset_at(day * 86_400), 3600);
        assert_eq!(tz.offset_at(day * 86_400 + 2 * 3600), 7200);
        // 02:30 local does not exist that night; it lands after the change.
        let t = local_to_utc(day, 150, &tz);
        assert!(local_minutes(t, &tz) >= 180);
        // 12:00 local is 10:00 UTC in summer.
        assert_eq!(local_to_utc(day, 720, &tz), day * 86_400 + 10 * 3600);
        // Autumn: 02:30 happens twice; the first one is used.
        let day = days_from_civil(2026, 10, 25);
        let t = local_to_utc(day, 150, &tz);
        assert_eq!(t, day * 86_400 + 30 * 60);
        assert_eq!(local_minutes(t, &tz), 150);
    }

    #[test]
    fn local_parts() {
        let tz = FixedTz(3 * 3600);
        let t = days_from_civil(2026, 6, 1) * 86_400 + 22 * 3600; // 22:00 UTC
        assert_eq!(local_minutes(t, &tz), 60);
        assert_eq!(local_day(t, &tz), days_from_civil(2026, 6, 2));
        assert_eq!(fmt_hm(t, &tz), "01:00");
    }
}
