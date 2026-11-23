//! Resolving the daily schedule: which point is in effect, and gradual transitions.
//!
//! Every point happens once per local day (a clock time, or sunrise/sunset
//! plus an offset). The point in effect at an instant is the one that most
//! recently happened, looking back into the previous days, so an evening
//! point carries on past midnight until the morning point. Points are
//! ordered by when they actually happen on the day, so "Sunset + 60 min" may
//! come after a fixed 22:00 point in summer and before it in winter.
use crate::settings::{Location, Preset, Schedule, SchedulePoint, When, KELVIN_NEUTRAL};
use crate::timeutil::{local_day, local_to_utc, Tz};

/// What the schedule asks for at an instant.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolved {
    pub brightness: Option<i32>,
    pub kelvin: Option<u32>,
    /// Index of the point now in effect.
    pub point: Option<usize>,
    /// When that point took effect (UTC).
    pub since: i64,
    /// Inside a gradual transition.
    pub transitioning: bool,
    /// The next point and when it takes effect (UTC).
    pub next: Option<(usize, i64)>,
}

/// When a point happens on a local day, or `None` (no location for a sun
/// event, or the sun does not rise/set that day).
pub fn point_time(when: &When, day: i64, loc: Option<&Location>, tz: &dyn Tz) -> Option<i64> {
    match *when {
        When::Time { minutes } => Some(local_to_utc(day, minutes.min(1439), tz)),
        When::Sunrise { offset } | When::Sunset { offset } => {
            let loc = loc?;
            let sun = crate::solar::sun_times(day, loc.lat, loc.lon, tz);
            let event = if matches!(when, When::Sunrise { .. }) {
                sun.sunrise
            } else {
                sun.sunset
            };
            event.map(|t| t + offset as i64 * 60)
        }
    }
}

/// A point's own values: a preset's brightness and night light when it names
/// a known preset (a preset without night light means neutral, 6500 K),
/// else the point's brightness and kelvin.
pub fn point_values(point: &SchedulePoint, presets: &[Preset]) -> (Option<i32>, Option<u32>) {
    if let Some(name) = point.preset.as_deref() {
        if let Some(p) = presets.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
            return (Some(p.brightness), Some(p.kelvin.unwrap_or(KELVIN_NEUTRAL)));
        }
    }
    (point.brightness, point.kelvin)
}

/// One occurrence of a point.
#[derive(Clone, Copy, Debug)]
struct Occurrence {
    at: i64,
    index: usize,
}

/// The occurrences of all placeable points on local days `from..=to`, in time order.
fn occurrences(
    schedule: &Schedule,
    loc: Option<&Location>,
    from: i64,
    to: i64,
    tz: &dyn Tz,
) -> Vec<Occurrence> {
    let mut list = Vec::new();
    for day in from..=to {
        for (index, p) in schedule.points.iter().enumerate() {
            if let Some(at) = point_time(&p.when, day, loc, tz) {
                list.push(Occurrence { at, index });
            }
        }
    }
    list.sort_by_key(|o| (o.at, o.index));
    list
}

/// The values in effect after occurrence `k`: the point's own values, with
/// a missing brightness or kelvin carried forward from earlier occurrences.
fn carried(
    list: &[Occurrence],
    k: usize,
    schedule: &Schedule,
    presets: &[Preset],
) -> (Option<i32>, Option<u32>) {
    let (mut b, mut kv) = (None, None);
    for o in list[..=k].iter().rev() {
        let (pb, pk) = point_values(&schedule.points[o.index], presets);
        b = b.or(pb);
        kv = kv.or(pk);
        if b.is_some() && kv.is_some() {
            break;
        }
    }
    (b, kv)
}

fn lerp_i32(a: i32, b: i32, t: f64) -> i32 {
    (a as f64 + (b - a) as f64 * t).round() as i32
}

fn lerp_u32(a: u32, b: u32, t: f64) -> u32 {
    (a as f64 + (b as f64 - a as f64) * t).round().max(0.0) as u32
}

/// The schedule's values at `now`, or `None` if it is off or has no usable points.
pub fn resolve(
    schedule: &Schedule,
    presets: &[Preset],
    loc: Option<&Location>,
    now: i64,
    tz: &dyn Tz,
) -> Option<Resolved> {
    if !schedule.enabled {
        return None;
    }
    resolve_always(schedule, presets, loc, now, tz)
}

/// Like [`resolve`], but also for a schedule that is switched off (previews).
pub fn resolve_always(
    schedule: &Schedule,
    presets: &[Preset],
    loc: Option<&Location>,
    now: i64,
    tz: &dyn Tz,
) -> Option<Resolved> {
    if schedule.points.is_empty() {
        return None;
    }
    let today = local_day(now, tz);
    // Two days back so the previous occurrence of every point is present
    // (and the one before it, for transitions); two ahead for `next`.
    let list = occurrences(schedule, loc, today - 3, today + 2, tz);
    let k = list.iter().rposition(|o| o.at <= now)?;
    let cur = list[k];
    let (mut brightness, mut kelvin) = carried(&list, k, schedule, presets);
    if brightness.is_none() && kelvin.is_none() {
        return None;
    }
    let mut transitioning = false;
    let span = schedule.transition_minutes as i64 * 60;
    if span > 0 && now < cur.at + span && k > 0 {
        let (pb, pk) = carried(&list, k - 1, schedule, presets);
        let t = (now - cur.at) as f64 / span as f64;
        if let (Some(a), Some(b)) = (pb, brightness) {
            if a != b {
                brightness = Some(lerp_i32(a, b, t));
                transitioning = true;
            }
        }
        if let (Some(a), Some(b)) = (pk, kelvin) {
            if a != b {
                kelvin = Some(lerp_u32(a, b, t));
                transitioning = true;
            }
        }
    }
    let next = list.get(k + 1).map(|o| (o.index, o.at));
    Some(Resolved {
        brightness,
        kelvin,
        point: Some(cur.index),
        since: cur.at,
        transitioning,
        next,
    })
}

/// The schedule's values through a local day, for a chart: one entry every
/// `step_minutes` from 00:00 to 24:00 (inclusive), as (minutes after local
/// midnight, brightness, kelvin). Works whether or not the schedule is
/// switched on; entries are `None` where nothing is known.
pub fn day_curve(
    schedule: &Schedule,
    presets: &[Preset],
    loc: Option<&Location>,
    day: i64,
    tz: &dyn Tz,
    step_minutes: u32,
) -> Vec<(u32, Option<i32>, Option<u32>)> {
    let step = step_minutes.clamp(1, 1440);
    let mut out = Vec::new();
    let mut m = 0;
    while m <= 1440 {
        let t = if m == 1440 {
            local_to_utc(day + 1, 0, tz)
        } else {
            local_to_utc(day, m, tz)
        };
        let r = resolve_always(schedule, presets, loc, t, tz);
        out.push((
            m,
            r.as_ref().and_then(|r| r.brightness),
            r.as_ref().and_then(|r| r.kelvin),
        ));
        if m == 1440 {
            break;
        }
        m = (m + step).min(1440);
    }
    out
}

/// The times of every placeable point on a local day, in time order, as
/// (point index, UTC instant). Points that cannot be placed are left out.
pub fn day_points(
    schedule: &Schedule,
    loc: Option<&Location>,
    day: i64,
    tz: &dyn Tz,
) -> Vec<(usize, i64)> {
    occurrences(schedule, loc, day, day, tz)
        .into_iter()
        .map(|o| (o.index, o.at))
        .collect()
}

fn offset_text(offset: i32) -> String {
    let sign = if offset < 0 { "-" } else { "+" };
    let n = offset.unsigned_abs();
    let (h, m) = (n / 60, n % 60);
    let body = match (h, m) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    };
    format!(" {sign} {body}")
}

/// A short label such as "Sunset - 30 min" or "22:30".
pub fn when_label(when: &When) -> String {
    match *when {
        When::Time { minutes } => crate::timeutil::fmt_minutes(minutes.min(1439)),
        When::Sunrise { offset: 0 } => "Sunrise".into(),
        When::Sunset { offset: 0 } => "Sunset".into(),
        When::Sunrise { offset } => format!("Sunrise{}", offset_text(offset)),
        When::Sunset { offset } => format!("Sunset{}", offset_text(offset)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::default_presets;
    use crate::timeutil::{days_from_civil, local_minutes, EuTz, FixedTz};

    fn at(minutes: u32, b: i32, k: u32) -> SchedulePoint {
        SchedulePoint {
            when: When::Time { minutes },
            brightness: Some(b),
            kelvin: Some(k),
            preset: None,
        }
    }

    fn sched(points: Vec<SchedulePoint>, transition: u32) -> Schedule {
        Schedule {
            enabled: true,
            points,
            transition_minutes: transition,
        }
    }

    fn erbil() -> Location {
        Location {
            name: "Erbil".into(),
            lat: 36.191,
            lon: 44.009,
        }
    }

    const TZ3: FixedTz = FixedTz(3 * 3600);

    fn t(y: i32, mo: u32, d: u32, h: u32, mi: u32, tz: &dyn Tz) -> i64 {
        local_to_utc(days_from_civil(y, mo, d), h * 60 + mi, tz)
    }

    #[test]
    fn labels() {
        assert_eq!(when_label(&When::Time { minutes: 450 }), "07:30");
        assert_eq!(when_label(&When::Time { minutes: 0 }), "00:00");
        assert_eq!(when_label(&When::Sunrise { offset: 0 }), "Sunrise");
        assert_eq!(when_label(&When::Sunset { offset: -30 }), "Sunset - 30 min");
        assert_eq!(when_label(&When::Sunrise { offset: 60 }), "Sunrise + 1 h");
        assert_eq!(
            when_label(&When::Sunset { offset: -90 }),
            "Sunset - 1 h 30 min"
        );
        assert_eq!(when_label(&When::Sunset { offset: 5 }), "Sunset + 5 min");
    }

    #[test]
    fn disabled_or_empty_is_none() {
        let mut s = sched(vec![at(480, 80, 6500)], 0);
        let now = t(2026, 11, 24, 12, 0, &TZ3);
        assert!(resolve(&s, &[], None, now, &TZ3).is_some());
        s.enabled = false;
        assert!(resolve(&s, &[], None, now, &TZ3).is_none());
        assert!(resolve_always(&s, &[], None, now, &TZ3).is_some());
        let empty = sched(vec![], 0);
        assert!(resolve(&empty, &[], None, now, &TZ3).is_none());
    }

    #[test]
    fn evening_point_carries_past_midnight() {
        let s = sched(vec![at(7 * 60, 80, 6500), at(22 * 60 + 30, 30, 3400)], 0);
        let now = t(2026, 11, 24, 2, 0, &TZ3);
        let r = resolve(&s, &[], None, now, &TZ3).unwrap();
        assert_eq!(r.point, Some(1));
        assert_eq!(r.brightness, Some(30));
        assert_eq!(r.kelvin, Some(3400));
        assert_eq!(r.since, t(2026, 11, 23, 22, 30, &TZ3));
        assert_eq!(r.next, Some((0, t(2026, 11, 24, 7, 0, &TZ3))));
        assert!(!r.transitioning);
        // During the day the morning point, next is tonight's.
        let r = resolve(&s, &[], None, t(2026, 11, 24, 12, 0, &TZ3), &TZ3).unwrap();
        assert_eq!(r.point, Some(0));
        assert_eq!(r.next, Some((1, t(2026, 11, 24, 22, 30, &TZ3))));
        // Late evening: next is tomorrow morning.
        let r = resolve(&s, &[], None, t(2026, 11, 24, 23, 0, &TZ3), &TZ3).unwrap();
        assert_eq!(r.next, Some((0, t(2026, 11, 25, 7, 0, &TZ3))));
        // Exactly at a point's time it is in effect.
        let r = resolve(&s, &[], None, t(2026, 11, 24, 7, 0, &TZ3), &TZ3).unwrap();
        assert_eq!(r.point, Some(0));
    }

    #[test]
    fn gradual_transition() {
        let s = sched(vec![at(7 * 60, 80, 6500), at(20 * 60, 40, 3500)], 30);
        let mid = resolve(&s, &[], None, t(2026, 11, 24, 20, 15, &TZ3), &TZ3).unwrap();
        assert_eq!(mid.point, Some(1));
        assert!(mid.transitioning);
        assert_eq!(mid.brightness, Some(60));
        assert_eq!(mid.kelvin, Some(5000));
        let start = resolve(&s, &[], None, t(2026, 11, 24, 20, 0, &TZ3), &TZ3).unwrap();
        assert_eq!(start.brightness, Some(80));
        let end = resolve(&s, &[], None, t(2026, 11, 24, 20, 30, &TZ3), &TZ3).unwrap();
        assert!(!end.transitioning);
        assert_eq!(end.brightness, Some(40));
        assert_eq!(end.kelvin, Some(3500));
        // Morning transition goes the other way.
        let m = resolve(&s, &[], None, t(2026, 11, 24, 7, 10, &TZ3), &TZ3).unwrap();
        assert!(m.brightness.unwrap() > 40 && m.brightness.unwrap() < 80);
        assert_eq!(m.since, t(2026, 11, 24, 7, 0, &TZ3));
    }

    #[test]
    fn missing_values_carry_forward() {
        let s = sched(
            vec![
                at(7 * 60, 80, 6500),
                SchedulePoint {
                    when: When::Time { minutes: 18 * 60 },
                    brightness: None,
                    kelvin: Some(4000),
                    preset: None,
                },
                SchedulePoint {
                    when: When::Time { minutes: 22 * 60 },
                    brightness: Some(20),
                    kelvin: None,
                    preset: None,
                },
            ],
            0,
        );
        let r = resolve(&s, &[], None, t(2026, 11, 24, 19, 0, &TZ3), &TZ3).unwrap();
        assert_eq!((r.brightness, r.kelvin), (Some(80), Some(4000)));
        let r = resolve(&s, &[], None, t(2026, 11, 24, 23, 0, &TZ3), &TZ3).unwrap();
        assert_eq!((r.brightness, r.kelvin), (Some(20), Some(4000)));
        // A point without any values and nothing to carry forward: nothing to do.
        let only = sched(
            vec![SchedulePoint {
                when: When::Time { minutes: 60 },
                brightness: None,
                kelvin: None,
                preset: None,
            }],
            0,
        );
        assert!(resolve(&only, &[], None, t(2026, 11, 24, 3, 0, &TZ3), &TZ3).is_none());
    }

    #[test]
    fn preset_points_use_preset_values() {
        let presets = default_presets();
        let mut night = at(21 * 60, 99, 6000);
        night.preset = Some("night".into());
        let mut day = at(8 * 60, 99, 6000);
        day.preset = Some("Day".into());
        let mut missing = at(23 * 60, 11, 2000);
        missing.preset = Some("Gone".into());
        let s = sched(vec![day, night, missing], 0);
        let r = resolve(&s, &presets, None, t(2026, 11, 24, 21, 30, &TZ3), &TZ3).unwrap();
        assert_eq!((r.brightness, r.kelvin), (Some(25), Some(3200)));
        // "Day" has night light off: neutral.
        let r = resolve(&s, &presets, None, t(2026, 11, 24, 9, 0, &TZ3), &TZ3).unwrap();
        assert_eq!((r.brightness, r.kelvin), (Some(80), Some(6500)));
        // An unknown preset falls back to the point's own values.
        let r = resolve(&s, &presets, None, t(2026, 11, 24, 23, 30, &TZ3), &TZ3).unwrap();
        assert_eq!((r.brightness, r.kelvin), (Some(11), Some(2000)));
    }

    #[test]
    fn sun_points_with_offsets() {
        let loc = erbil();
        let day = days_from_civil(2026, 11, 24);
        let sun = crate::solar::sun_times(day, loc.lat, loc.lon, &TZ3);
        let rise = point_time(&When::Sunrise { offset: 0 }, day, Some(&loc), &TZ3).unwrap();
        assert_eq!(rise, sun.sunrise.unwrap());
        let set30 = point_time(&When::Sunset { offset: -30 }, day, Some(&loc), &TZ3).unwrap();
        assert_eq!(set30, sun.sunset.unwrap() - 1800);
        let s = sched(
            vec![
                SchedulePoint {
                    when: When::Sunrise { offset: 15 },
                    brightness: Some(80),
                    kelvin: Some(6500),
                    preset: None,
                },
                SchedulePoint {
                    when: When::Sunset { offset: -30 },
                    brightness: Some(50),
                    kelvin: Some(4500),
                    preset: None,
                },
            ],
            0,
        );
        let before = resolve(&s, &[], Some(&loc), set30 - 60, &TZ3).unwrap();
        assert_eq!(before.point, Some(0));
        assert_eq!(before.next, Some((1, set30)));
        let after = resolve(&s, &[], Some(&loc), set30 + 60, &TZ3).unwrap();
        assert_eq!(after.point, Some(1));
        assert_eq!(after.since, set30);
        // Next: tomorrow's sunrise + 15 min (sunrise moves about a minute a day).
        let next = after.next.unwrap();
        assert_eq!(next.0, 0);
        let drift = next.1 - (rise + 900 + 86_400);
        assert!((0..=150).contains(&drift), "{drift}");
    }

    #[test]
    fn no_location_skips_sun_points() {
        let s = Schedule {
            enabled: true,
            ..Schedule::default()
        };
        // Default: sunrise, sunset - 30, 22:30. Without a location only 22:30 is placed.
        let r = resolve(&s, &[], None, t(2026, 11, 24, 12, 0, &TZ3), &TZ3).unwrap();
        assert_eq!(r.point, Some(2));
        assert_eq!(r.brightness, Some(30));
        assert_eq!(r.next, Some((2, t(2026, 11, 24, 22, 30, &TZ3))));
        assert_eq!(point_time(&When::Sunset { offset: 0 }, 0, None, &TZ3), None);
        // Only sun points and no location: nothing.
        let sun_only = sched(
            vec![SchedulePoint {
                when: When::Sunrise { offset: 0 },
                ..Default::default()
            }],
            0,
        );
        assert!(resolve(&sun_only, &[], None, 0, &TZ3).is_none());
    }

    #[test]
    fn polar_night_skips_sun_points() {
        let tromso = Location {
            name: "Tromso".into(),
            lat: 69.649,
            lon: 18.955,
        };
        let tz = EuTz(3600);
        let s = sched(
            vec![
                SchedulePoint {
                    when: When::Sunrise { offset: 0 },
                    brightness: Some(90),
                    kelvin: Some(6500),
                    preset: None,
                },
                at(21 * 60, 30, 3000),
            ],
            0,
        );
        let r = resolve(&s, &[], Some(&tromso), t(2026, 12, 21, 12, 0, &tz), &tz).unwrap();
        assert_eq!(r.point, Some(1));
        assert_eq!(r.brightness, Some(30));
    }

    #[test]
    fn sunset_offset_can_pass_a_fixed_point() {
        let london = Location {
            name: "London".into(),
            lat: 51.507,
            lon: -0.128,
        };
        let tz = EuTz(0);
        let s = sched(
            vec![
                at(22 * 60, 30, 3000),
                SchedulePoint {
                    when: When::Sunset { offset: 60 },
                    brightness: Some(50),
                    kelvin: Some(4000),
                    preset: None,
                },
                at(7 * 60, 80, 6500),
            ],
            0,
        );
        // Midsummer: sunset ~21:21, +60 = ~22:21, after the 22:00 point.
        let r = resolve(&s, &[], Some(&london), t(2026, 6, 21, 22, 10, &tz), &tz).unwrap();
        assert_eq!(r.point, Some(0));
        assert_eq!(r.next.unwrap().0, 1);
        let r = resolve(&s, &[], Some(&london), t(2026, 6, 21, 23, 0, &tz), &tz).unwrap();
        assert_eq!(r.point, Some(1));
        assert_eq!(r.brightness, Some(50));
        // Winter: sunset ~15:53, +60 = 16:53, before 22:00.
        let r = resolve(&s, &[], Some(&london), t(2026, 12, 21, 23, 0, &tz), &tz).unwrap();
        assert_eq!(r.point, Some(0));
        let order: Vec<usize> = day_points(&s, Some(&london), days_from_civil(2026, 6, 21), &tz)
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert_eq!(order, vec![2, 0, 1]);
        let order: Vec<usize> = day_points(&s, Some(&london), days_from_civil(2026, 12, 21), &tz)
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert_eq!(order, vec![2, 1, 0]);
    }

    #[test]
    fn dst_days_with_a_0230_point() {
        let tz = EuTz(3600);
        let s = sched(vec![at(2 * 60 + 30, 20, 3000), at(8 * 60, 80, 6500)], 0);
        // Spring forward: 02:30 does not exist; it happens at 03:30 summer time.
        let spring = days_from_civil(2026, 3, 29);
        let p = point_time(&When::Time { minutes: 150 }, spring, None, &tz).unwrap();
        assert_eq!(local_minutes(p, &tz), 210);
        let r = resolve(&s, &[], None, p + 60, &tz).unwrap();
        assert_eq!(r.point, Some(0));
        let r = resolve(&s, &[], None, p - 60, &tz).unwrap();
        assert_eq!(r.point, Some(1)); // still yesterday's 08:00 point
        assert_eq!(r.next, Some((0, p)));
        // Fall back: 02:30 happens twice; the point fires at the first one.
        let fall = days_from_civil(2026, 10, 25);
        let p = point_time(&When::Time { minutes: 150 }, fall, None, &tz).unwrap();
        assert_eq!(p, fall * 86_400 + 30 * 60); // 00:30 UTC = 02:30 CEST
        let r = resolve(&s, &[], None, p + 3600, &tz).unwrap(); // the second 02:30
        assert_eq!(r.point, Some(0));
        assert_eq!(r.since, p);
        // The day after: back to normal (02:30 CET = 01:30 UTC).
        let p2 = point_time(&When::Time { minutes: 150 }, fall + 1, None, &tz).unwrap();
        assert_eq!(p2, (fall + 1) * 86_400 + 90 * 60);
        // A day curve on the short day has no gaps and ends at 24:00.
        let curve = day_curve(&s, &[], None, spring, &tz, 30);
        assert_eq!(curve.first().unwrap().0, 0);
        assert_eq!(curve.last().unwrap().0, 1440);
        assert_eq!(curve.len(), 49);
        assert!(curve.iter().all(|(_, b, k)| b.is_some() && k.is_some()));
    }

    #[test]
    fn day_curve_shape() {
        let mut s = sched(vec![at(7 * 60, 80, 6500), at(20 * 60, 40, 3500)], 60);
        s.enabled = false; // previews work while switched off
        let day = days_from_civil(2026, 11, 24);
        let curve = day_curve(&s, &[], None, day, &TZ3, 15);
        assert_eq!(curve.len(), 97);
        let at_min = |m: u32| curve.iter().find(|c| c.0 == m).unwrap();
        assert_eq!(at_min(0).1, Some(40));
        assert_eq!(at_min(7 * 60 + 30).1, Some(60));
        assert_eq!(at_min(12 * 60).1, Some(80));
        assert_eq!(at_min(12 * 60).2, Some(6500));
        assert_eq!(at_min(20 * 60 + 30).1, Some(60));
        assert_eq!(at_min(1440).1, Some(40));
        // Step that does not divide the day still ends at 24:00.
        let c = day_curve(&s, &[], None, day, &TZ3, 700);
        assert_eq!(
            c.iter().map(|c| c.0).collect::<Vec<_>>(),
            vec![0, 700, 1400, 1440]
        );
        // Nothing placeable: all None.
        let sun_only = sched(
            vec![SchedulePoint {
                when: When::Sunrise { offset: 0 },
                ..Default::default()
            }],
            0,
        );
        assert!(day_curve(&sun_only, &[], None, day, &TZ3, 60)
            .iter()
            .all(|c| c.1.is_none() && c.2.is_none()));
    }
}
