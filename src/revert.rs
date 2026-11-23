//! The "keep these display settings?" countdown: revert unless confirmed.

/// Seconds the user has to confirm a display change.
pub const CONFIRM_SECONDS: u64 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevertState {
    Waiting,
    Confirmed,
    /// Time ran out or the user chose "Revert".
    Revert,
}

#[derive(Clone, Copy, Debug)]
pub struct RevertTimer {
    pub started_ms: u64,
    pub seconds: u64,
    pub state: RevertState,
}

impl RevertTimer {
    pub fn new(now_ms: u64, seconds: u64) -> Self {
        RevertTimer {
            started_ms: now_ms,
            seconds,
            state: RevertState::Waiting,
        }
    }

    /// When time runs out.
    pub fn deadline_ms(&self) -> u64 {
        self.started_ms
            .saturating_add(self.seconds.saturating_mul(1000))
    }

    /// Whole seconds left (rounded up), 0 when expired or no longer waiting.
    pub fn remaining(&self, now_ms: u64) -> u64 {
        if self.state != RevertState::Waiting {
            return 0;
        }
        let left = self
            .deadline_ms()
            .saturating_sub(now_ms.max(self.started_ms));
        left.div_ceil(1000)
    }

    /// Advances the timer; returns the state. Once confirmed or reverted the
    /// state is final.
    pub fn tick(&mut self, now_ms: u64) -> RevertState {
        if self.state == RevertState::Waiting && now_ms >= self.deadline_ms() {
            self.state = RevertState::Revert;
        }
        self.state
    }

    /// Keeps the change (ignored once reverted).
    pub fn confirm(&mut self) {
        if self.state == RevertState::Waiting {
            self.state = RevertState::Confirmed;
        }
    }

    /// Reverts now (ignored once confirmed).
    pub fn revert(&mut self) {
        if self.state == RevertState::Waiting {
            self.state = RevertState::Revert;
        }
    }

    pub fn is_waiting(&self) -> bool {
        self.state == RevertState::Waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_rounding_up() {
        let t = RevertTimer::new(10_000, CONFIRM_SECONDS);
        assert_eq!(t.remaining(10_000), 15);
        assert_eq!(t.remaining(10_001), 15);
        assert_eq!(t.remaining(10_999), 15);
        assert_eq!(t.remaining(11_000), 14);
        assert_eq!(t.remaining(24_000), 1);
        assert_eq!(t.remaining(24_999), 1);
        assert_eq!(t.remaining(25_000), 0);
        assert_eq!(t.remaining(99_000), 0);
        // A clock before the start does not add time.
        assert_eq!(t.remaining(0), 15);
    }

    #[test]
    fn times_out_exactly_at_the_deadline() {
        let mut t = RevertTimer::new(0, 15);
        assert_eq!(t.tick(14_999), RevertState::Waiting);
        assert!(t.is_waiting());
        assert_eq!(t.tick(15_000), RevertState::Revert);
        assert_eq!(t.tick(20_000), RevertState::Revert);
        // Final: a late confirm does not undo the revert.
        t.confirm();
        assert_eq!(t.state, RevertState::Revert);
    }

    #[test]
    fn confirm_and_revert_are_final() {
        let mut t = RevertTimer::new(0, 15);
        t.confirm();
        assert_eq!(t.tick(60_000), RevertState::Confirmed);
        assert_eq!(t.remaining(1000), 0);
        t.revert();
        assert_eq!(t.state, RevertState::Confirmed);

        let mut t = RevertTimer::new(0, 15);
        t.revert();
        assert_eq!(t.tick(1), RevertState::Revert);
        t.confirm();
        assert_eq!(t.state, RevertState::Revert);
    }

    #[test]
    fn zero_seconds_reverts_at_once() {
        let mut t = RevertTimer::new(5, 0);
        assert_eq!(t.remaining(5), 0);
        assert_eq!(t.tick(5), RevertState::Revert);
    }
}
