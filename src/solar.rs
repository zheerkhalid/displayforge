//! Sunrise and sunset, computed locally (NOAA solar position equations).
//!
//! The equations are those of the NOAA solar calculator (after Meeus,
//! "Astronomical Algorithms"): the sun's declination and the equation of time
//! give the solar noon and the hour angle at which the sun's centre is
//! 90.833 degrees from the zenith (refraction and the sun's radius included).
//! Each event is refined once at its own instant, which keeps the result
//! within a minute of the NOAA tables away from the polar circles.
use crate::timeutil::{local_to_utc, Tz};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SunTimes {
    /// UTC seconds; `None` when the sun does not rise / set that day (polar day or night).
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
    /// True when the sun stays up all day (polar day), false otherwise.
    pub polar_day: bool,
}

/// Zenith angle of sunrise and sunset, degrees.
pub const ZENITH: f64 = 90.833;

/// Declination (degrees) and equation of time (minutes) at a UTC instant.
fn sun_position(utc: f64) -> (f64, f64) {
    let jd = utc / 86_400.0 + 2_440_587.5;
    let t = (jd - 2_451_545.0) / 36_525.0;
    let l0 = (280.46646 + t * (36_000.769_83 + t * 0.000_303_2)).rem_euclid(360.0);
    let m = 357.52911 + t * (35_999.050_29 - 0.000_153_7 * t);
    let e = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);
    let mr = m.to_radians();
    let c = mr.sin() * (1.914_602 - t * (0.004_817 + 0.000_014 * t))
        + (2.0 * mr).sin() * (0.019_993 - 0.000_101 * t)
        + (3.0 * mr).sin() * 0.000_289;
    let true_long = l0 + c;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let lambda = (true_long - 0.00569 - 0.00478 * omega.sin()).to_radians();
    let eps0 =
        23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001_813))) / 60.0) / 60.0;
    let eps = (eps0 + 0.00256 * omega.cos()).to_radians();
    let decl = (eps.sin() * lambda.sin()).asin().to_degrees();
    let y = (eps / 2.0).tan().powi(2);
    let l0r = l0.to_radians();
    let eot = 4.0
        * (y * (2.0 * l0r).sin() - 2.0 * e * mr.sin() + 4.0 * e * y * mr.sin() * (2.0 * l0r).cos()
            - 0.5 * y * y * (4.0 * l0r).sin()
            - 1.25 * e * e * (2.0 * mr).sin())
        .to_degrees();
    (decl, eot)
}

/// Cosine of the sunrise hour angle; > 1 means the sun stays down, < -1 up.
fn cos_hour_angle(lat: f64, decl: f64) -> f64 {
    let (lat, decl) = (lat.to_radians(), decl.to_radians());
    ZENITH.to_radians().cos() / (lat.cos() * decl.cos()) - lat.tan() * decl.tan()
}

/// Solar noon (UTC seconds) of the UTC day `utc_day` at longitude `lon`.
fn solar_noon(utc_day: i64, lon: f64) -> f64 {
    let approx = utc_day as f64 * 86_400.0 + (720.0 - 4.0 * lon) * 60.0;
    let (_, eot) = sun_position(approx);
    approx - eot * 60.0
}

/// One event (`sign` -1 = sunrise, +1 = sunset) near `noon`, refined at its
/// own instant; `None` if the sun does not cross the horizon.
fn event(utc_day: i64, noon: f64, lat: f64, lon: f64, sign: f64) -> Option<f64> {
    let day0 = utc_day as f64 * 86_400.0;
    let mut t = noon;
    for _ in 0..3 {
        let (decl, eot) = sun_position(t);
        let ch = cos_hour_angle(lat, decl);
        if !(-1.0..=1.0).contains(&ch) {
            return None;
        }
        let ha = ch.acos().to_degrees();
        // Minutes after UTC midnight of the day the noon was computed for.
        let minutes = 720.0 - 4.0 * (lon - sign * ha) - eot;
        t = day0 + minutes * 60.0;
    }
    Some(t)
}

/// Sunrise and sunset on a local day (days since 1970-01-01 in local time) at a place.
///
/// The events are those around the solar noon closest to local noon of that
/// day. `lat` is north positive, `lon` east positive (degrees). When the sun
/// does not rise or set, both times are `None` and `polar_day` tells whether
/// it stays up (true) or down (false).
pub fn sun_times(day: i64, lat: f64, lon: f64, tz: &dyn Tz) -> SunTimes {
    if !lat.is_finite() || !lon.is_finite() {
        return SunTimes::default();
    }
    let lat = lat.clamp(-89.999, 89.999);
    let lon = (lon + 180.0).rem_euclid(360.0) - 180.0;
    let local_noon = local_to_utc(day, 720, tz) as f64;
    // The solar noon nearest to local noon (zones can be far from their meridian).
    let guess = (local_noon / 86_400.0).floor() as i64;
    let (utc_day, noon) = (guess - 1..=guess + 1)
        .map(|d| (d, solar_noon(d, lon)))
        .min_by(|a, b| {
            (a.1 - local_noon)
                .abs()
                .partial_cmp(&(b.1 - local_noon).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or((guess, local_noon));
    let (decl, _) = sun_position(noon);
    let ch = cos_hour_angle(lat, decl);
    if ch < -1.0 {
        return SunTimes {
            sunrise: None,
            sunset: None,
            polar_day: true,
        };
    }
    if ch > 1.0 {
        return SunTimes::default();
    }
    let rise = event(utc_day, noon, lat, lon, -1.0);
    let set = event(utc_day, noon, lat, lon, 1.0);
    SunTimes {
        sunrise: rise.map(|t| t.round() as i64),
        sunset: set.map(|t| t.round() as i64),
        polar_day: false,
    }
}

/// Length of daylight in minutes on a local day (1440 in polar day, 0 in polar night).
pub fn day_length_minutes(day: i64, lat: f64, lon: f64, tz: &dyn Tz) -> u32 {
    let s = sun_times(day, lat, lon, tz);
    match (s.sunrise, s.sunset) {
        (Some(r), Some(t)) if t > r => ((t - r) / 60) as u32,
        _ if s.polar_day => 1440,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeutil::{days_from_civil, local_minutes, EuTz, FixedTz};

    /// Asserts that `utc` shows as `hh:mm` local time, within `tol` minutes.
    fn near(utc: Option<i64>, tz: &dyn Tz, hh: u32, mm: u32, tol: i64) {
        let utc = utc.expect("event expected");
        let got = local_minutes(utc, tz) as i64;
        let want = (hh * 60 + mm) as i64;
        assert!(
            (got - want).abs() <= tol,
            "got {:02}:{:02}, want {hh:02}:{mm:02}",
            got / 60,
            got % 60
        );
    }

    #[test]
    fn london_midsummer() {
        let tz = EuTz(0); // GMT / BST
        let day = days_from_civil(2026, 6, 21);
        let s = sun_times(day, 51.5074, -0.1278, &tz);
        near(s.sunrise, &tz, 4, 43, 3);
        near(s.sunset, &tz, 21, 21, 3);
        assert!(!s.polar_day);
    }

    #[test]
    fn london_midwinter() {
        let tz = EuTz(0);
        let day = days_from_civil(2026, 12, 21);
        let s = sun_times(day, 51.5074, -0.1278, &tz);
        near(s.sunrise, &tz, 8, 4, 3);
        near(s.sunset, &tz, 15, 53, 3);
    }

    #[test]
    fn erbil_late_november() {
        let tz = FixedTz(3 * 3600);
        let day = days_from_civil(2026, 11, 24);
        let s = sun_times(day, 36.191, 44.009, &tz);
        // Solar noon about 11:51 (44 E in a 45 E zone, equation of time +13 min),
        // half a day of about 5 h 01 min.
        near(s.sunrise, &tz, 6, 50, 3);
        near(s.sunset, &tz, 16, 52, 3);
        // About ten hours of daylight.
        let len = day_length_minutes(day, 36.191, 44.009, &tz);
        assert!((600..630).contains(&len), "{len}");
    }

    #[test]
    fn sydney_southern_summer() {
        let tz = FixedTz(11 * 3600); // AEDT
        let day = days_from_civil(2026, 12, 21);
        let s = sun_times(day, -33.8688, 151.2093, &tz);
        near(s.sunrise, &tz, 5, 41, 3);
        near(s.sunset, &tz, 20, 5, 3);
        // Southern winter is short.
        let tz = FixedTz(10 * 3600);
        let june = days_from_civil(2026, 6, 21);
        assert!(day_length_minutes(june, -33.8688, 151.2093, &tz) < 600);
    }

    #[test]
    fn new_york_equinox() {
        let tz = FixedTz(-4 * 3600); // EDT
        let day = days_from_civil(2026, 3, 20);
        let s = sun_times(day, 40.7128, -74.006, &tz);
        near(s.sunrise, &tz, 7, 0, 4);
        near(s.sunset, &tz, 19, 12, 4);
    }

    #[test]
    fn tromso_polar_night_and_day() {
        let tz = EuTz(3600);
        let dec = days_from_civil(2026, 12, 21);
        let s = sun_times(dec, 69.6492, 18.9553, &tz);
        assert_eq!(s.sunrise, None);
        assert_eq!(s.sunset, None);
        assert!(!s.polar_day);
        assert_eq!(day_length_minutes(dec, 69.6492, 18.9553, &tz), 0);
        let jun = days_from_civil(2026, 6, 21);
        let s = sun_times(jun, 69.6492, 18.9553, &tz);
        assert_eq!(s.sunrise, None);
        assert!(s.polar_day);
        assert_eq!(day_length_minutes(jun, 69.6492, 18.9553, &tz), 1440);
        // In March the sun rises and sets normally.
        let mar = days_from_civil(2026, 3, 10);
        let s = sun_times(mar, 69.6492, 18.9553, &tz);
        assert!(s.sunrise.is_some() && s.sunset.is_some());
    }

    #[test]
    fn dst_change_day_follows_the_clock() {
        // Berlin-like zone; 2026-03-29 is the spring-forward day.
        let tz = EuTz(3600);
        let (lat, lon) = (52.52, 13.405);
        let before = sun_times(days_from_civil(2026, 3, 28), lat, lon, &tz);
        let change = sun_times(days_from_civil(2026, 3, 29), lat, lon, &tz);
        near(before.sunrise, &tz, 5, 50, 3);
        near(before.sunset, &tz, 18, 32, 3);
        near(change.sunrise, &tz, 6, 48, 3);
        near(change.sunset, &tz, 19, 34, 3);
        // In UTC the sunrise moves only about two minutes earlier.
        let d = before.sunrise.unwrap() + 86_400 - change.sunrise.unwrap();
        assert!((60..=180).contains(&d), "{d}");
        // And the events fall on the right local day.
        let day = days_from_civil(2026, 3, 29);
        assert_eq!(
            crate::timeutil::local_day(change.sunrise.unwrap(), &tz),
            day
        );
        assert_eq!(crate::timeutil::local_day(change.sunset.unwrap(), &tz), day);
    }

    #[test]
    fn far_from_meridian_zone() {
        // Kiritimati: UTC+14 at 157 W. Events must fall on the asked local day.
        let tz = FixedTz(14 * 3600);
        let day = days_from_civil(2026, 6, 1);
        let s = sun_times(day, 1.87, -157.4, &tz);
        let r = s.sunrise.unwrap();
        let t = s.sunset.unwrap();
        assert_eq!(crate::timeutil::local_day(r, &tz), day);
        assert_eq!(crate::timeutil::local_day(t, &tz), day);
        assert!((t - r) > 11 * 3600 && (t - r) < 13 * 3600);
    }

    #[test]
    fn bad_input_is_harmless() {
        let s = sun_times(0, f64::NAN, 0.0, &FixedTz(0));
        assert_eq!(s, SunTimes::default());
    }
}
