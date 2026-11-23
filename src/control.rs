//! The monitor controller: a worker thread that owns the [`Backend`],
//! coalesces requests (the newest value wins), fades, and paces writes so
//! slow monitors are not flooded.
//!
//! How it behaves:
//! - At start it enumerates the monitors and reads every supported feature.
//! - For each (monitor, feature) only the newest request is kept. A
//!   continuous feature fades from its last known value to the target; a
//!   non-continuous one (input, power, colour preset) is written once.
//! - Writes to one monitor are at least `Backend::min_interval_ms` apart, and
//!   a value equal to the last known one is not written again.
//! - Gamma ramps are remembered per monitor; repeats are skipped, and every
//!   monitor whose ramp was changed gets the identity ramp back on `stop`
//!   (or drop). After `rescan` the remembered ramps are applied again,
//!   because Windows resets gamma on display changes.
//! - A backend error is shown in the monitor's `error` until the next success.
use crate::model::{Backend, Feature, GammaRamp, Level, Monitor};
use crate::transition::Fade;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// One monitor with its last known levels.
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorSnapshot {
    pub monitor: Monitor,
    pub levels: BTreeMap<Feature, Level>,
    pub error: Option<String>,
    pub fading: bool,
}

/// Results of [`Controller::read_all`]: (VCP code, value or error).
pub type VcpDump = Vec<(u8, Result<Level, String>)>;

/// Shortest time between two fade steps when the backend allows any rate.
const TICK_MS: u64 = 15;

enum Msg {
    Percent {
        key: String,
        feature: Feature,
        percent: u32,
        fade_ms: u32,
    },
    Raw {
        key: String,
        feature: Feature,
        raw: u32,
    },
    Gamma {
        key: String,
        ramp: Option<Box<GammaRamp>>,
    },
    Rescan,
    ReadAll {
        key: String,
        reply: Sender<VcpDump>,
    },
    Stop,
}

#[derive(Default)]
struct Status {
    ready: bool,
    /// Messages sent but not yet handled.
    queued: usize,
    /// Writes and fades not yet finished.
    jobs: usize,
    stopped: bool,
}

struct Inner {
    snapshot: Mutex<Vec<MonitorSnapshot>>,
    status: Mutex<Status>,
    changed: Condvar,
}

pub struct Controller {
    tx: Mutex<Sender<Msg>>,
    inner: Arc<Inner>,
    thread: Option<JoinHandle<()>>,
}

impl Controller {
    /// Starts the worker; it enumerates the monitors and reads their levels.
    /// `on_change` is called (from the worker) whenever the snapshot changed.
    pub fn start(backend: Box<dyn Backend>, on_change: Box<dyn Fn() + Send>) -> Controller {
        let inner = Arc::new(Inner {
            snapshot: Mutex::new(Vec::new()),
            status: Mutex::new(Status::default()),
            changed: Condvar::new(),
        });
        let (tx, rx) = channel();
        let worker = Worker {
            backend,
            inner: inner.clone(),
            on_change,
            monitors: Vec::new(),
            jobs: BTreeMap::new(),
            last_write: HashMap::new(),
            gamma: BTreeMap::new(),
            touched: BTreeSet::new(),
            epoch: Instant::now(),
            published: None,
        };
        let thread = std::thread::Builder::new()
            .name("displayforge-control".into())
            .spawn(move || worker.run(rx))
            .ok();
        if thread.is_none() {
            // Could not start a thread: behave as stopped rather than hang.
            if let Ok(mut st) = inner.status.lock() {
                st.ready = true;
                st.stopped = true;
            }
        }
        Controller {
            tx: Mutex::new(tx),
            inner,
            thread,
        }
    }

    fn send(&self, msg: Msg) {
        if let Ok(mut st) = self.inner.status.lock() {
            if st.stopped {
                return;
            }
            st.queued += 1;
        }
        let sent = self
            .tx
            .lock()
            .map(|tx| tx.send(msg).is_ok())
            .unwrap_or(false);
        if !sent {
            if let Ok(mut st) = self.inner.status.lock() {
                st.queued = st.queued.saturating_sub(1);
            }
            self.inner.changed.notify_all();
        }
    }

    /// True once the first enumeration has finished.
    pub fn ready(&self) -> bool {
        self.inner.status.lock().map(|s| s.ready).unwrap_or(false)
    }

    pub fn snapshot(&self) -> Vec<MonitorSnapshot> {
        self.inner
            .snapshot
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Enumerates the monitors again. Levels already known for monitors
    /// that are still connected are kept; new monitors are read.
    pub fn rescan(&self) {
        self.send(Msg::Rescan);
    }

    /// Sets a continuous feature to a percentage of its maximum, fading over `fade_ms`.
    /// (For a non-continuous feature the number is used as the raw code.)
    pub fn set_percent(&self, key: &str, feature: Feature, percent: u32, fade_ms: u32) {
        self.send(Msg::Percent {
            key: key.to_string(),
            feature,
            percent: if feature.continuous() {
                percent.min(100)
            } else {
                percent
            },
            fade_ms,
        });
    }

    /// Sets a non-continuous feature to a raw code. (For a continuous
    /// feature, the raw value is written without a fade.)
    pub fn set_raw(&self, key: &str, feature: Feature, raw: u32) {
        self.send(Msg::Raw {
            key: key.to_string(),
            feature,
            raw,
        });
    }

    /// Sets the gamma ramp (`None` = identity). Repeats of the same ramp are skipped.
    pub fn set_gamma(&self, key: &str, ramp: Option<GammaRamp>) {
        self.send(Msg::Gamma {
            key: key.to_string(),
            ramp: ramp.map(Box::new),
        });
    }

    /// Reads every VCP code in the monitor's capabilities (blocking).
    /// Without a capabilities string, the codes of its known features are read.
    pub fn read_all(&self, key: &str) -> VcpDump {
        let (reply, rx) = channel();
        self.send(Msg::ReadAll {
            key: key.to_string(),
            reply,
        });
        rx.recv().unwrap_or_default()
    }

    /// Waits until the first scan is done and all queued writes and fades
    /// are finished. False on timeout.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let Ok(mut st) = self.inner.status.lock() else {
            return false;
        };
        loop {
            if st.stopped || (st.ready && st.queued == 0 && st.jobs == 0) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            match self.inner.changed.wait_timeout(st, deadline - now) {
                Ok((guard, _)) => st = guard,
                Err(_) => return false,
            }
        }
    }

    fn shutdown(&mut self) {
        if let Some(thread) = self.thread.take() {
            if let Ok(tx) = self.tx.lock() {
                let _ = tx.send(Msg::Stop);
            }
            let _ = thread.join();
        }
        if let Ok(mut st) = self.inner.status.lock() {
            st.stopped = true;
        }
        self.inner.changed.notify_all();
    }

    /// Stops the worker after finishing pending writes (jumping to their
    /// targets) and restoring the identity gamma ramp on every monitor whose
    /// ramp it changed.
    pub fn stop(mut self) {
        self.shutdown();
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// A pending write: the newest target for one (monitor, feature).
#[derive(Clone, Copy, Debug)]
struct Job {
    target: u32,
    /// `None`: write the target as soon as allowed.
    fade: Option<Fade>,
    /// Do not act before this time (ms since the worker's epoch).
    next_ms: u64,
}

struct Worker {
    backend: Box<dyn Backend>,
    inner: Arc<Inner>,
    on_change: Box<dyn Fn() + Send>,
    monitors: Vec<MonitorSnapshot>,
    jobs: BTreeMap<(String, Feature), Job>,
    /// When the last write to each monitor finished (ms, rounded up).
    last_write: HashMap<String, u64>,
    /// Non-identity ramps currently set.
    gamma: BTreeMap<String, GammaRamp>,
    /// Monitors whose ramp was ever changed.
    touched: BTreeSet<String>,
    epoch: Instant,
    published: Option<Vec<MonitorSnapshot>>,
}

impl Worker {
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    fn now_ms_ceil(&self) -> u64 {
        let e = self.epoch.elapsed();
        let ms = e.as_millis() as u64;
        if e.as_nanos().is_multiple_of(1_000_000) {
            ms
        } else {
            ms + 1
        }
    }

    fn find(&self, key: &str) -> Option<usize> {
        self.monitors.iter().position(|m| m.monitor.key == key)
    }

    fn run(mut self, rx: Receiver<Msg>) {
        let found = self.backend.enumerate();
        let mut monitors = Vec::with_capacity(found.len());
        for m in found {
            monitors.push(self.read_levels(m));
        }
        self.monitors = monitors;
        // Snapshot first, then `ready`, so a caller that sees ready also sees the monitors.
        self.publish();
        if let Ok(mut st) = self.inner.status.lock() {
            st.ready = true;
        }
        self.inner.changed.notify_all();
        (self.on_change)();
        loop {
            let first = match self.next_due() {
                None => match rx.recv() {
                    Ok(m) => Some(m),
                    Err(_) => Some(Msg::Stop),
                },
                Some(due) => {
                    let wait = due.saturating_sub(self.now_ms());
                    match rx.recv_timeout(Duration::from_millis(wait)) {
                        Ok(m) => Some(m),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => Some(Msg::Stop),
                    }
                }
            };
            let mut msgs: Vec<Msg> = first.into_iter().collect();
            msgs.extend(rx.try_iter());
            let handled = msgs.len();
            let mut stop = false;
            let mut ramps: BTreeMap<String, Option<GammaRamp>> = BTreeMap::new();
            for msg in msgs {
                match msg {
                    Msg::Percent {
                        key,
                        feature,
                        percent,
                        fade_ms,
                    } => self.queue_percent(&key, feature, percent, fade_ms),
                    Msg::Raw { key, feature, raw } => self.queue_raw(&key, feature, raw),
                    Msg::Gamma { key, ramp } => {
                        ramps.insert(key, ramp.map(|r| *r));
                    }
                    Msg::Rescan => self.rescan(),
                    Msg::ReadAll { key, reply } => {
                        let _ = reply.send(self.read_all(&key));
                    }
                    Msg::Stop => stop = true,
                }
            }
            for (key, ramp) in ramps {
                self.apply_gamma(&key, ramp);
            }
            if stop {
                self.finish();
                self.publish();
                if let Ok(mut st) = self.inner.status.lock() {
                    st.queued = 0;
                    st.jobs = 0;
                    st.stopped = true;
                }
                self.inner.changed.notify_all();
                return;
            }
            self.run_due();
            self.publish();
            if let Ok(mut st) = self.inner.status.lock() {
                st.queued = st.queued.saturating_sub(handled);
                st.jobs = self.jobs.len();
            }
            self.inner.changed.notify_all();
        }
    }

    /// Reads every supported feature of a newly found monitor.
    fn read_levels(&mut self, monitor: Monitor) -> MonitorSnapshot {
        let mut levels = BTreeMap::new();
        let mut first_error = None;
        for &f in &monitor.features {
            match self.backend.get(&monitor.key, f) {
                Ok(l) => {
                    levels.insert(f, l);
                }
                Err(e) => {
                    first_error.get_or_insert(e.0);
                }
            }
        }
        // Some monitors do not answer for every code; only an error when nothing could be read.
        let error = if levels.is_empty() { first_error } else { None };
        MonitorSnapshot {
            monitor,
            levels,
            error,
            fading: false,
        }
    }

    fn rescan(&mut self) {
        let found = self.backend.enumerate();
        let mut old = std::mem::take(&mut self.monitors);
        let mut next = Vec::with_capacity(found.len());
        for m in found {
            match old.iter().position(|o| o.monitor.key == m.key) {
                Some(i) => {
                    let mut snap = old.swap_remove(i);
                    snap.levels.retain(|f, _| m.features.contains(f));
                    snap.monitor = m;
                    next.push(snap);
                }
                None => next.push(self.read_levels(m)),
            }
        }
        self.monitors = next;
        let present: BTreeSet<String> = self
            .monitors
            .iter()
            .map(|m| m.monitor.key.clone())
            .collect();
        self.jobs.retain(|(key, _), _| present.contains(key));
        // A display change usually resets gamma: put the ramps back.
        let ramps: Vec<(String, GammaRamp)> = self
            .gamma
            .iter()
            .filter(|(k, _)| present.contains(*k))
            .map(|(k, r)| (k.clone(), r.clone()))
            .collect();
        for (key, ramp) in ramps {
            let result = self.backend.set_gamma(&key, Some(&ramp));
            self.note(&key, result.map_err(|e| e.0));
        }
    }

    fn read_all(&mut self, key: &str) -> VcpDump {
        let Some(i) = self.find(key) else {
            return Vec::new();
        };
        let m = &self.monitors[i].monitor;
        let mut codes: Vec<u8> = match &m.caps {
            Some(c) if !c.vcp.is_empty() => c.vcp.keys().copied().collect(),
            _ => m.features.iter().map(|f| f.vcp()).collect(),
        };
        codes.sort_unstable();
        codes.dedup();
        codes
            .into_iter()
            .map(|code| (code, self.backend.read_vcp(key, code).map_err(|e| e.0)))
            .collect()
    }

    /// Records the outcome of a backend call on a monitor.
    fn note(&mut self, key: &str, result: Result<(), String>) {
        if let Some(i) = self.find(key) {
            self.monitors[i].error = result.err();
        }
    }

    fn queue_percent(&mut self, key: &str, feature: Feature, percent: u32, fade_ms: u32) {
        if !feature.continuous() {
            return self.queue_raw(key, feature, percent);
        }
        let Some(i) = self.find(key) else { return };
        let now = self.now_ms();
        self.jobs.remove(&(key.to_string(), feature));
        let known = match self.monitors[i].levels.get(&feature) {
            Some(l) => Some(*l),
            None => match self.backend.get(key, feature) {
                Ok(l) => {
                    self.monitors[i].levels.insert(feature, l);
                    Some(l)
                }
                Err(_) => None,
            },
        };
        let (target, fade) = match known {
            Some(level) => {
                let target = level.raw_for(percent);
                if target == level.value {
                    return;
                }
                let fade =
                    (fade_ms > 0).then(|| Fade::new(level.value, target, now, fade_ms as u64));
                (target, fade)
            }
            // Unknown current value: write the target directly (assumes a maximum of 100).
            None => (percent, None),
        };
        self.jobs.insert(
            (key.to_string(), feature),
            Job {
                target,
                fade,
                next_ms: now,
            },
        );
    }

    fn queue_raw(&mut self, key: &str, feature: Feature, raw: u32) {
        let Some(i) = self.find(key) else { return };
        self.jobs.remove(&(key.to_string(), feature));
        // Codes such as the input are always sent (the monitor may have been
        // switched with its own buttons); levels are skipped when unchanged.
        if feature.continuous()
            && self.monitors[i].levels.get(&feature).map(|l| l.value) == Some(raw)
        {
            return;
        }
        let now = self.now_ms();
        self.jobs.insert(
            (key.to_string(), feature),
            Job {
                target: raw,
                fade: None,
                next_ms: now,
            },
        );
    }

    fn apply_gamma(&mut self, key: &str, ramp: Option<GammaRamp>) {
        let ramp = ramp.filter(|r| !r.is_identity());
        if self.gamma.get(key) == ramp.as_ref() {
            return;
        }
        let result = self.backend.set_gamma(key, ramp.as_ref());
        self.touched.insert(key.to_string());
        if result.is_ok() {
            match ramp {
                Some(r) => {
                    self.gamma.insert(key.to_string(), r);
                }
                None => {
                    self.gamma.remove(key);
                }
            }
        }
        self.note(key, result.map_err(|e| e.0));
    }

    /// The earliest time a job wants attention.
    fn next_due(&self) -> Option<u64> {
        self.jobs.values().map(|j| j.next_ms).min()
    }

    /// Writes one value; updates the known level and the error.
    fn write(&mut self, key: &str, feature: Feature, value: u32) -> bool {
        let result = self.backend.set(key, feature, value);
        let done = self.now_ms_ceil();
        self.last_write.insert(key.to_string(), done);
        let ok = result.is_ok();
        if let Some(i) = self.find(key) {
            if ok {
                self.monitors[i]
                    .levels
                    .entry(feature)
                    .or_insert(Level { value, max: 100 })
                    .value = value;
            }
        }
        self.note(key, result.map_err(|e| e.0));
        ok
    }

    fn run_due(&mut self) {
        let now = self.now_ms();
        let keys: Vec<(String, Feature)> = self.jobs.keys().cloned().collect();
        let mut written: BTreeSet<String> = BTreeSet::new();
        for k in keys {
            let Some(job) = self.jobs.get(&k).copied() else {
                continue;
            };
            if job.next_ms > now {
                continue;
            }
            let (key, feature) = (&k.0, k.1);
            let Some(i) = self.find(key) else {
                self.jobs.remove(&k);
                continue;
            };
            let interval = self.backend.min_interval_ms(key);
            let gate = self.last_write.get(key).map_or(0, |t| t + interval);
            if written.contains(key) || now < gate {
                if let Some(j) = self.jobs.get_mut(&k) {
                    j.next_ms = gate.max(now);
                }
                continue;
            }
            let value = job.fade.map_or(job.target, |f| f.value_at(now));
            let finished = job.fade.is_none_or(|f| f.done(now));
            let current = self.monitors[i].levels.get(&feature).map(|l| l.value);
            let force = !feature.continuous();
            if force || current != Some(value) {
                written.insert(key.clone());
                if !self.write(key, feature, value) {
                    self.jobs.remove(&k);
                    continue;
                }
            }
            if finished {
                self.jobs.remove(&k);
            } else if let Some(j) = self.jobs.get_mut(&k) {
                j.next_ms = now + interval.max(TICK_MS);
            }
        }
    }

    /// On stop: write every pending target at once (keeping the pace), then
    /// restore the identity ramp wherever gamma was changed.
    fn finish(&mut self) {
        let jobs: Vec<((String, Feature), Job)> =
            std::mem::take(&mut self.jobs).into_iter().collect();
        for ((key, feature), job) in jobs {
            let Some(i) = self.find(&key) else { continue };
            let current = self.monitors[i].levels.get(&feature).map(|l| l.value);
            if feature.continuous() && current == Some(job.target) {
                continue;
            }
            let interval = self.backend.min_interval_ms(&key);
            if let Some(&last) = self.last_write.get(&key) {
                let gate = last + interval;
                let now = self.now_ms();
                if now < gate {
                    std::thread::sleep(Duration::from_millis(gate - now));
                }
            }
            self.write(&key, feature, job.target);
        }
        let touched: Vec<String> = self.touched.iter().cloned().collect();
        for key in touched {
            let result = self.backend.set_gamma(&key, None);
            if result.is_ok() {
                self.gamma.remove(&key);
            }
            self.note(&key, result.map_err(|e| e.0));
        }
    }

    /// Publishes the snapshot (with fading flags) and calls `on_change` if it changed.
    fn publish(&mut self) {
        let mut snap = self.monitors.clone();
        for m in &mut snap {
            m.fading = self
                .jobs
                .iter()
                .any(|((key, _), j)| *key == m.monitor.key && j.fade.is_some());
        }
        if self.published.as_ref() == Some(&snap) {
            return;
        }
        if let Ok(mut s) = self.inner.snapshot.lock() {
            *s = snap.clone();
        }
        self.published = Some(snap);
        (self.on_change)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockBackend;
    use crate::model::{Capabilities, Connection, HwError, PxRect};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn monitor(key: &str, features: &[Feature]) -> Monitor {
        Monitor {
            key: key.into(),
            number: 1,
            name: format!("Monitor {key}"),
            device: format!("\\\\.\\DISPLAY{key}"),
            connection: Connection::DdcCi,
            rect: PxRect::new(0, 0, 1920, 1080),
            primary: true,
            features: features.to_vec(),
            caps: None,
            edid: None,
            note: None,
        }
    }

    const BASIC: &[Feature] = &[Feature::Brightness, Feature::Contrast, Feature::InputSource];

    /// MockBackend with a write interval, write timestamps and failure switch.
    struct Paced {
        inner: MockBackend,
        interval: u64,
        times: Arc<Mutex<Vec<(String, Instant)>>>,
        fail: Arc<AtomicBool>,
    }

    impl Paced {
        fn new(inner: MockBackend, interval: u64) -> Self {
            Paced {
                inner,
                interval,
                times: Arc::new(Mutex::new(Vec::new())),
                fail: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl Backend for Paced {
        fn enumerate(&mut self) -> Vec<Monitor> {
            self.inner.enumerate()
        }
        fn get(&mut self, key: &str, feature: Feature) -> Result<Level, HwError> {
            self.inner.get(key, feature)
        }
        fn set(&mut self, key: &str, feature: Feature, value: u32) -> Result<(), HwError> {
            self.times
                .lock()
                .unwrap()
                .push((key.to_string(), Instant::now()));
            if self.fail.load(Ordering::SeqCst) {
                return Err(HwError("The monitor did not answer.".into()));
            }
            self.inner.set(key, feature, value)
        }
        fn read_vcp(&mut self, key: &str, code: u8) -> Result<Level, HwError> {
            self.inner.read_vcp(key, code)
        }
        fn set_gamma(&mut self, key: &str, ramp: Option<&GammaRamp>) -> Result<(), HwError> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(HwError("Gamma refused.".into()));
            }
            self.inner.set_gamma(key, ramp)
        }
        fn min_interval_ms(&self, _key: &str) -> u64 {
            self.interval
        }
    }

    fn start(backend: impl Backend + 'static) -> (Controller, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = calls.clone();
        let c = Controller::start(
            Box::new(backend),
            Box::new(move || {
                c2.fetch_add(1, Ordering::SeqCst);
            }),
        );
        assert!(c.wait_idle(Duration::from_secs(5)));
        assert!(c.ready());
        (c, calls)
    }

    fn level(c: &Controller, key: &str, f: Feature) -> Option<u32> {
        c.snapshot()
            .into_iter()
            .find(|m| m.monitor.key == key)
            .and_then(|m| m.levels.get(&f).map(|l| l.value))
    }

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn starts_and_reads_levels() {
        let mock = MockBackend::new(vec![
            monitor("A", BASIC),
            monitor("B", &[Feature::Brightness]),
        ]);
        let (c, calls) = start(mock.clone());
        let snap = c.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(level(&c, "A", Feature::Brightness), Some(60));
        assert_eq!(level(&c, "A", Feature::Contrast), Some(50));
        assert_eq!(level(&c, "B", Feature::Contrast), None);
        assert!(snap.iter().all(|m| m.error.is_none() && !m.fading));
        assert!(calls.load(Ordering::SeqCst) >= 1);
        assert_eq!(mock.log().enumerations, 1);
        c.stop();
    }

    #[test]
    fn immediate_set_and_no_rewrite_of_same_value() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let (c, _) = start(mock.clone());
        c.set_percent("A", Feature::Brightness, 60, 0); // already 60
        assert!(c.wait_idle(WAIT));
        assert!(mock.log().writes.is_empty());
        c.set_percent("A", Feature::Brightness, 35, 0);
        assert!(c.wait_idle(WAIT));
        c.set_percent("A", Feature::Brightness, 35, 0);
        c.set_raw("A", Feature::Brightness, 35);
        assert!(c.wait_idle(WAIT));
        assert_eq!(
            mock.log().writes,
            vec![(String::from("A"), Feature::Brightness, 35)]
        );
        assert_eq!(level(&c, "A", Feature::Brightness), Some(35));
        c.stop();
    }

    #[test]
    fn percent_maps_onto_the_maximum() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        mock.put("A", Feature::Contrast, 100, 255);
        let (c, _) = start(mock.clone());
        c.set_percent("A", Feature::Contrast, 50, 0);
        assert!(c.wait_idle(WAIT));
        assert_eq!(mock.value("A", Feature::Contrast), Some(128));
        c.stop();
    }

    #[test]
    fn non_continuous_written_once_at_once() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let (c, _) = start(mock.clone());
        c.set_raw("A", Feature::InputSource, 0x11);
        assert!(c.wait_idle(WAIT));
        let writes = mock.log().writes;
        assert_eq!(
            writes,
            vec![(String::from("A"), Feature::InputSource, 0x11)]
        );
        assert_eq!(level(&c, "A", Feature::InputSource), Some(0x11));
        // A fade time is ignored for codes.
        c.set_percent("A", Feature::InputSource, 0x0F, 500);
        assert!(c.wait_idle(WAIT));
        assert_eq!(mock.log().writes.len(), 2);
        assert_eq!(mock.value("A", Feature::InputSource), Some(0x0F));
        c.stop();
    }

    #[test]
    fn coalesces_quick_requests() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let paced = Paced::new(mock.clone(), 40);
        let (c, _) = start(paced);
        for v in 0..=100 {
            c.set_percent("A", Feature::Brightness, v, 0);
        }
        assert!(c.wait_idle(WAIT));
        let writes = mock.log().writes;
        assert!(writes.len() <= 5, "{} writes", writes.len());
        assert_eq!(writes.last().unwrap().2, 100);
        assert_eq!(mock.value("A", Feature::Brightness), Some(100));
        assert_eq!(level(&c, "A", Feature::Brightness), Some(100));
        c.stop();
    }

    #[test]
    fn fades_end_on_target() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let (c, _) = start(mock.clone());
        c.set_percent("A", Feature::Brightness, 20, 400);
        // The snapshot shows the fade while it runs.
        let t0 = Instant::now();
        let mut saw_fading = false;
        while t0.elapsed() < Duration::from_millis(200) {
            if c.snapshot()[0].fading {
                saw_fading = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(saw_fading);
        assert!(c.wait_idle(WAIT));
        assert!(!c.snapshot()[0].fading);
        let values: Vec<u32> = mock.log().writes.iter().map(|w| w.2).collect();
        assert!(values.len() >= 2, "{values:?}");
        assert!(values.windows(2).all(|w| w[1] < w[0]), "{values:?}");
        assert_eq!(*values.last().unwrap(), 20);
        // Back up again.
        c.set_percent("A", Feature::Brightness, 90, 150);
        assert!(c.wait_idle(WAIT));
        assert_eq!(mock.value("A", Feature::Brightness), Some(90));
        c.stop();
    }

    #[test]
    fn new_target_during_a_fade_starts_from_where_it_is() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let (c, _) = start(mock.clone());
        c.set_percent("A", Feature::Brightness, 0, 400);
        std::thread::sleep(Duration::from_millis(200));
        c.set_percent("A", Feature::Brightness, 100, 200);
        assert!(c.wait_idle(WAIT));
        let values: Vec<u32> = mock.log().writes.iter().map(|w| w.2).collect();
        assert_eq!(*values.last().unwrap(), 100);
        // Down, then up: never a jump back to 60 and down again.
        let low = values
            .iter()
            .position(|&v| v == *values.iter().min().unwrap())
            .unwrap();
        assert!(values[..=low].windows(2).all(|w| w[1] < w[0]), "{values:?}");
        assert!(values[low..].windows(2).all(|w| w[1] > w[0]), "{values:?}");
        assert!(*values.iter().min().unwrap() > 0);
        c.stop();
    }

    #[test]
    fn writes_are_paced_per_monitor() {
        let mock = MockBackend::new(vec![monitor("A", BASIC), monitor("B", BASIC)]);
        let paced = Paced::new(mock.clone(), 40);
        let times = paced.times.clone();
        let (c, _) = start(paced);
        c.set_percent("A", Feature::Brightness, 0, 600);
        c.set_percent("A", Feature::Contrast, 100, 600);
        c.set_percent("B", Feature::Brightness, 100, 600);
        assert!(c.wait_idle(WAIT));
        assert_eq!(mock.value("A", Feature::Brightness), Some(0));
        assert_eq!(mock.value("A", Feature::Contrast), Some(100));
        assert_eq!(mock.value("B", Feature::Brightness), Some(100));
        let times = times.lock().unwrap().clone();
        for key in ["A", "B"] {
            let t: Vec<Instant> = times
                .iter()
                .filter(|(k, _)| k == key)
                .map(|x| x.1)
                .collect();
            // At least a step and the final write (more unless the machine is busy).
            assert!(t.len() >= 2, "{key}: {}", t.len());
            for w in t.windows(2) {
                assert!(
                    w[1] - w[0] >= Duration::from_millis(40),
                    "{:?}",
                    w[1] - w[0]
                );
            }
        }
        c.stop();
    }

    #[test]
    fn gamma_skips_repeats_and_is_restored_on_stop() {
        let mock = MockBackend::new(vec![monitor("A", BASIC), monitor("B", BASIC)]);
        let (c, _) = start(mock.clone());
        let warm = crate::color::ramp(3400, 1.0);
        c.set_gamma("A", Some(warm.clone()));
        c.set_gamma("A", Some(warm.clone()));
        assert!(c.wait_idle(WAIT));
        c.set_gamma("A", Some(warm.clone()));
        c.set_gamma("B", None); // identity, never changed: nothing to do
        c.set_gamma("B", Some(GammaRamp::identity()));
        assert!(c.wait_idle(WAIT));
        let log = mock.log();
        assert_eq!(log.gamma_writes, 1);
        assert_eq!(log.gamma.get("A"), Some(&warm));
        c.stop();
        let log = mock.log();
        assert!(log.gamma.is_empty());
        assert_eq!(log.gamma_writes, 2);
    }

    #[test]
    fn gamma_restored_on_drop_even_after_reset_to_identity() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        {
            let (c, _) = start(mock.clone());
            c.set_gamma("A", Some(crate::color::ramp(6500, 0.5)));
            assert!(c.wait_idle(WAIT));
            assert!(mock.log().gamma.contains_key("A"));
        }
        assert!(mock.log().gamma.is_empty());
        let (c, _) = start(mock.clone());
        c.set_gamma("A", Some(crate::color::ramp(5000, 1.0)));
        c.set_gamma("A", None);
        assert!(c.wait_idle(WAIT));
        drop(c);
        assert!(mock.log().gamma.is_empty());
    }

    #[test]
    fn errors_are_shown_and_cleared() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let paced = Paced::new(mock.clone(), 0);
        let fail = paced.fail.clone();
        let (c, calls) = start(paced);
        fail.store(true, Ordering::SeqCst);
        let before = calls.load(Ordering::SeqCst);
        c.set_percent("A", Feature::Brightness, 10, 0);
        assert!(c.wait_idle(WAIT));
        let snap = c.snapshot();
        assert_eq!(
            snap[0].error.as_deref(),
            Some("The monitor did not answer.")
        );
        assert_eq!(level(&c, "A", Feature::Brightness), Some(60));
        assert!(calls.load(Ordering::SeqCst) > before);
        // A failing fade stops instead of retrying forever.
        c.set_percent("A", Feature::Brightness, 10, 300);
        assert!(c.wait_idle(WAIT));
        fail.store(false, Ordering::SeqCst);
        c.set_percent("A", Feature::Brightness, 10, 0);
        assert!(c.wait_idle(WAIT));
        assert_eq!(c.snapshot()[0].error, None);
        assert_eq!(level(&c, "A", Feature::Brightness), Some(10));
        // Unsupported feature: the mock refuses, the error is shown.
        c.set_raw("A", Feature::PowerMode, 4);
        assert!(c.wait_idle(WAIT));
        assert!(c.snapshot()[0].error.is_some());
        c.stop();
    }

    #[test]
    fn rescan_keeps_known_levels() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let (c, _) = start(mock.clone());
        c.set_gamma("A", Some(crate::color::ramp(4000, 1.0)));
        assert!(c.wait_idle(WAIT));
        // The hardware changes behind our back; a rescan keeps what we knew.
        mock.put("A", Feature::Brightness, 5, 100);
        mock.set_monitors(vec![
            monitor("A", BASIC),
            monitor("C", &[Feature::Brightness]),
        ]);
        mock.put("C", Feature::Brightness, 42, 100);
        c.rescan();
        assert!(c.wait_idle(WAIT));
        assert_eq!(c.snapshot().len(), 2);
        assert_eq!(level(&c, "A", Feature::Brightness), Some(60));
        assert_eq!(level(&c, "C", Feature::Brightness), Some(42));
        assert_eq!(mock.log().enumerations, 2);
        // Gamma was put back after the rescan.
        assert_eq!(mock.log().gamma_writes, 2);
        // Unplugged monitors disappear; requests for them are ignored.
        mock.set_monitors(vec![monitor("C", &[Feature::Brightness])]);
        c.rescan();
        assert!(c.wait_idle(WAIT));
        assert_eq!(c.snapshot().len(), 1);
        c.set_percent("A", Feature::Brightness, 10, 0);
        c.set_percent("nope", Feature::Brightness, 10, 0);
        assert!(c.wait_idle(WAIT));
        assert!(mock.log().writes.is_empty());
        c.stop();
    }

    #[test]
    fn read_all_codes() {
        let mut m = monitor("A", BASIC);
        let mut caps = Capabilities::default();
        caps.vcp.insert(0x10, vec![]);
        caps.vcp.insert(0x12, vec![]);
        caps.vcp.insert(0x60, vec![0x0F, 0x11]);
        caps.vcp.insert(0xDF, vec![]);
        m.caps = Some(caps);
        let mock = MockBackend::new(vec![m, monitor("B", &[Feature::Brightness])]);
        let (c, _) = start(mock.clone());
        let dump = c.read_all("A");
        let codes: Vec<u8> = dump.iter().map(|d| d.0).collect();
        assert_eq!(codes, vec![0x10, 0x12, 0x60, 0xDF]);
        assert_eq!(dump[0].1.as_ref().unwrap().value, 60);
        assert_eq!(dump[2].1.as_ref().unwrap().value, 0x0F);
        assert!(dump[3].1.is_err());
        let dump = c.read_all("B");
        assert_eq!(dump.len(), 1);
        assert!(c.read_all("missing").is_empty());
        c.stop();
    }

    #[test]
    fn stop_finishes_pending_writes() {
        let mock = MockBackend::new(vec![monitor("A", BASIC)]);
        let (c, _) = start(mock.clone());
        c.set_percent("A", Feature::Brightness, 0, 2000);
        std::thread::sleep(Duration::from_millis(50));
        c.stop();
        assert_eq!(mock.value("A", Feature::Brightness), Some(0));
    }

    #[test]
    fn slow_start_is_not_ready_and_wait_idle_times_out() {
        let mut mock = MockBackend::new(vec![monitor("A", BASIC)]);
        mock.delay_ms = 100;
        let c = Controller::start(Box::new(mock), Box::new(|| {}));
        assert!(!c.ready());
        assert!(!c.wait_idle(Duration::from_millis(10)));
        // Woken as soon as the scan is done (about 0.4 s), not at the timeout.
        let t0 = Instant::now();
        assert!(c.wait_idle(WAIT));
        assert!(t0.elapsed() < Duration::from_secs(3), "{:?}", t0.elapsed());
        assert!(c.ready());
        assert_eq!(c.snapshot().len(), 1);
        drop(c);
    }
}
