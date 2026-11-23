//! Fades: stepping a value from one level to another over time.

/// A fade from `from` to `to` over `duration_ms`, started at `start_ms`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub from: u32,
    pub to: u32,
    pub start_ms: u64,
    pub duration_ms: u64,
}

/// Ease-in-out (smoothstep) of a progress 0..=1.
pub fn ease_in_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Fade {
    pub fn new(from: u32, to: u32, start_ms: u64, duration_ms: u64) -> Fade {
        Fade {
            from,
            to,
            start_ms,
            duration_ms,
        }
    }

    /// The value at `now_ms` (eased), `to` once finished. Before the start it
    /// is `from`; a fade of zero length is `to` at once.
    pub fn value_at(&self, now_ms: u64) -> u32 {
        if self.duration_ms == 0 || self.done(now_ms) {
            return self.to;
        }
        if now_ms <= self.start_ms {
            return self.from;
        }
        let t = (now_ms - self.start_ms) as f64 / self.duration_ms as f64;
        let e = ease_in_out(t);
        let v = self.from as f64 + (self.to as f64 - self.from as f64) * e;
        // Never overshoot either end.
        let (lo, hi) = (self.from.min(self.to), self.from.max(self.to));
        (v.round() as u32).clamp(lo, hi)
    }

    pub fn done(&self, now_ms: u64) -> bool {
        now_ms >= self.start_ms.saturating_add(self.duration_ms)
    }

    /// When the fade ends.
    pub fn end_ms(&self) -> u64 {
        self.start_ms.saturating_add(self.duration_ms)
    }
}

/// The writes of a fade from `from` to `to` over `duration_ms`, as
/// (offset in ms from the start, value). Two writes are never closer than
/// `min_interval_ms` (at least 1 ms), a value equal to the previous one (or to
/// `from`) is not repeated, and the last entry is always exactly `to`.
/// Returns nothing when `from == to`; `[(0, to)]` for a zero duration.
pub fn plan_steps(from: u32, to: u32, duration_ms: u64, min_interval_ms: u64) -> Vec<(u64, u32)> {
    if from == to {
        return Vec::new();
    }
    if duration_ms == 0 {
        return vec![(0, to)];
    }
    let interval = min_interval_ms.max(1);
    let fade = Fade::new(from, to, 0, duration_ms);
    let mut steps: Vec<(u64, u32)> = Vec::new();
    let mut last = from;
    let mut t = interval;
    while t < duration_ms {
        let v = fade.value_at(t);
        if v != last {
            steps.push((t, v));
            last = v;
        }
        t += interval;
    }
    if last != to {
        let at = match steps.last() {
            Some(&(prev, _)) => duration_ms.max(prev + interval),
            None => duration_ms,
        };
        steps.push((at, to));
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ends_and_zero_duration() {
        let f = Fade::new(20, 80, 1000, 400);
        assert_eq!(f.value_at(0), 20);
        assert_eq!(f.value_at(1000), 20);
        assert_eq!(f.value_at(1200), 50);
        assert_eq!(f.value_at(1400), 80);
        assert_eq!(f.value_at(99_999), 80);
        assert!(!f.done(1399));
        assert!(f.done(1400));
        assert_eq!(f.end_ms(), 1400);
        let z = Fade::new(20, 80, 1000, 0);
        assert_eq!(z.value_at(1000), 80);
        assert_eq!(z.value_at(0), 80);
        assert!(z.done(1000));
    }

    #[test]
    fn eases_both_directions_monotonically() {
        for (a, b) in [(0u32, 100u32), (100, 0), (37, 38), (1000, 3)] {
            let f = Fade::new(a, b, 0, 500);
            let mut prev = a;
            for t in 0..=500 {
                let v = f.value_at(t);
                if a < b {
                    assert!(v >= prev && v <= b);
                } else {
                    assert!(v <= prev && v >= b);
                }
                prev = v;
            }
            assert_eq!(prev, b);
        }
        // Ease-in-out: slow at the ends, fast in the middle.
        let f = Fade::new(0, 1000, 0, 1000);
        let start = f.value_at(100) - f.value_at(0);
        let middle = f.value_at(550) - f.value_at(450);
        assert!(middle > 3 * start);
        assert_eq!(ease_in_out(0.0), 0.0);
        assert_eq!(ease_in_out(1.0), 1.0);
        assert_eq!(ease_in_out(0.5), 0.5);
    }

    #[test]
    fn plan_respects_interval_and_ends_on_target() {
        for (from, to, dur, iv) in [
            (0u32, 100u32, 400u64, 50u64),
            (100, 0, 400, 50),
            (10, 12, 1000, 20),
            (0, 100, 1000, 0),
            (50, 90, 30, 50),
            (0, 100, 401, 50),
        ] {
            let steps = plan_steps(from, to, dur, iv);
            assert_eq!(steps.last().unwrap().1, to, "{from}->{to}");
            let mut prev_t: Option<u64> = None;
            let mut prev_v = from;
            for &(t, v) in &steps {
                assert_ne!(v, prev_v, "repeat in {steps:?}");
                if let Some(p) = prev_t {
                    assert!(t >= p + iv.max(1), "too fast in {steps:?}");
                }
                prev_t = Some(t);
                prev_v = v;
            }
        }
        // A two-step change is two writes, not one per interval.
        assert_eq!(plan_steps(10, 12, 1000, 20).len(), 2);
        // Slow monitor: a fade shorter than the interval is a single write.
        assert_eq!(plan_steps(50, 90, 30, 50), vec![(30, 90)]);
        assert_eq!(plan_steps(5, 5, 400, 50), vec![]);
        assert_eq!(plan_steps(5, 9, 0, 50), vec![(0, 9)]);
        // 400 ms at 50 ms: at most 8 writes.
        assert!(plan_steps(0, 100, 400, 50).len() <= 8);
    }
}
