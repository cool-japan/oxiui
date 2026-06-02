//! Frame-aligned callback scheduling plus debounce/throttle helpers.
//!
//! [`Scheduler`] is a deterministic, virtual-clock scheduler: the caller drives
//! it by calling [`Scheduler::tick`] once per frame with the elapsed `dt`
//! (seconds). It maintains a monotonically-increasing virtual `now`, runs any
//! callbacks whose fire time has arrived, and supports one-shot timers
//! (`after`), repeating timers (`every`), and next-frame callbacks
//! (`request_frame`, the `requestAnimationFrame` analogue).
//!
//! Keeping time virtual (rather than reading a wall clock) makes the scheduler
//! testable and frame-rate independent, and lets the same logic run under wasm,
//! native, and headless test harnesses.
//!
//! [`Debounce`] and [`Throttle`] are standalone rate-limiters usable with the
//! same virtual clock or any `f32` timestamp source.

/// An opaque handle to a scheduled callback, usable for cancellation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(u64);

/// What a fired timer should do next.
enum Repeat {
    /// Fire once and drop.
    Once,
    /// Re-arm to fire again every `interval` seconds.
    Every(f32),
}

struct Timer {
    id: TimerId,
    /// Virtual time at which this timer next fires.
    fire_at: f32,
    repeat: Repeat,
    callback: Box<dyn FnMut()>,
}

/// A virtual-clock scheduler for frame-aligned callbacks.
#[derive(Default)]
pub struct Scheduler {
    now: f32,
    next_id: u64,
    timers: Vec<Timer>,
    /// Callbacks to run on the next `tick`, regardless of time (rAF analogue).
    frame_callbacks: Vec<(TimerId, Box<dyn FnMut()>)>,
}

impl Scheduler {
    /// Create a scheduler with its virtual clock at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current virtual time in seconds.
    pub fn now(&self) -> f32 {
        self.now
    }

    /// Number of pending timers (excluding next-frame callbacks).
    pub fn pending(&self) -> usize {
        self.timers.len()
    }

    /// Number of pending next-frame callbacks.
    pub fn pending_frames(&self) -> usize {
        self.frame_callbacks.len()
    }

    fn alloc_id(&mut self) -> TimerId {
        let id = TimerId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Schedule `callback` to fire once, `delay` seconds from now. A `delay`
    /// of `0` (or negative) fires on the next [`tick`](Scheduler::tick).
    pub fn after(&mut self, delay: f32, callback: impl FnMut() + 'static) -> TimerId {
        let id = self.alloc_id();
        self.timers.push(Timer {
            id,
            fire_at: self.now + delay.max(0.0),
            repeat: Repeat::Once,
            callback: Box::new(callback),
        });
        id
    }

    /// Schedule `callback` to fire every `interval` seconds, beginning
    /// `interval` seconds from now. `interval` is clamped to a tiny positive
    /// value to avoid a zero-period busy loop.
    pub fn every(&mut self, interval: f32, callback: impl FnMut() + 'static) -> TimerId {
        let interval = interval.max(1e-4);
        let id = self.alloc_id();
        self.timers.push(Timer {
            id,
            fire_at: self.now + interval,
            repeat: Repeat::Every(interval),
            callback: Box::new(callback),
        });
        id
    }

    /// Schedule `callback` to run on the next [`tick`](Scheduler::tick), once.
    /// This is the `requestAnimationFrame` analogue.
    pub fn request_frame(&mut self, callback: impl FnMut() + 'static) -> TimerId {
        let id = self.alloc_id();
        self.frame_callbacks.push((id, Box::new(callback)));
        id
    }

    /// Cancel a pending timer or next-frame callback. Returns `true` if found.
    pub fn cancel(&mut self, id: TimerId) -> bool {
        let before = self.timers.len() + self.frame_callbacks.len();
        self.timers.retain(|t| t.id != id);
        self.frame_callbacks.retain(|(fid, _)| *fid != id);
        self.timers.len() + self.frame_callbacks.len() != before
    }

    /// Advance the virtual clock by `dt` seconds and run every callback that is
    /// now due. Returns the number of callbacks fired.
    ///
    /// Repeating timers may fire multiple times within one large `dt`, and are
    /// re-armed relative to their *scheduled* fire time (so they don't drift).
    /// Next-frame callbacks always fire exactly once, before timers.
    pub fn tick(&mut self, dt: f32) -> usize {
        self.now += dt.max(0.0);
        let mut fired = 0;

        // Next-frame callbacks fire first, exactly once each.
        let frames = std::mem::take(&mut self.frame_callbacks);
        for (_, mut cb) in frames {
            cb();
            fired += 1;
        }

        // Timers: collect due ones (in fire-time order) and run them. Re-arm
        // repeats; this loop handles a single `dt` that spans several periods.
        loop {
            // Find the earliest due timer not yet past `now`.
            let due_idx = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, t)| t.fire_at <= self.now)
                .min_by(|(_, a), (_, b)| {
                    a.fire_at
                        .partial_cmp(&b.fire_at)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i);

            let Some(idx) = due_idx else { break };

            match self.timers[idx].repeat {
                Repeat::Once => {
                    let mut timer = self.timers.remove(idx);
                    (timer.callback)();
                    fired += 1;
                }
                Repeat::Every(interval) => {
                    (self.timers[idx].callback)();
                    fired += 1;
                    // Re-arm relative to the scheduled time to avoid drift.
                    self.timers[idx].fire_at += interval;
                }
            }
        }
        fired
    }
}

/// Trailing-edge debouncer: an action only fires once input has been quiet for
/// `delay` seconds. Each [`Debounce::signal`] resets the quiet timer; only when
/// [`Debounce::poll`] is called after `delay` of silence does it report ready.
#[derive(Clone, Copy, Debug)]
pub struct Debounce {
    delay: f32,
    /// Virtual time of the most recent signal, or `None` if idle/already fired.
    last_signal: Option<f32>,
}

impl Debounce {
    /// Create a debouncer with the given quiet-period `delay` in seconds.
    pub fn new(delay: f32) -> Self {
        Self {
            delay: delay.max(0.0),
            last_signal: None,
        }
    }

    /// Register activity at virtual time `now`, resetting the quiet timer.
    pub fn signal(&mut self, now: f32) {
        self.last_signal = Some(now);
    }

    /// If a signal is pending and `now` is at least `delay` past the last
    /// signal, consume it and return `true` (the action should fire). Otherwise
    /// `false`.
    pub fn poll(&mut self, now: f32) -> bool {
        match self.last_signal {
            Some(t) if now - t >= self.delay => {
                self.last_signal = None;
                true
            }
            _ => false,
        }
    }

    /// Whether a signal is currently waiting to fire.
    pub fn is_pending(&self) -> bool {
        self.last_signal.is_some()
    }
}

/// Leading-edge throttler: an action may fire at most once per `interval`
/// seconds. The first attempt fires immediately; subsequent attempts within the
/// interval are suppressed.
#[derive(Clone, Copy, Debug)]
pub struct Throttle {
    interval: f32,
    /// Virtual time the action last fired, or `None` if it never has.
    last_fire: Option<f32>,
}

impl Throttle {
    /// Create a throttle allowing one fire per `interval` seconds.
    pub fn new(interval: f32) -> Self {
        Self {
            interval: interval.max(0.0),
            last_fire: None,
        }
    }

    /// Attempt to fire at virtual time `now`. Returns `true` if allowed (and
    /// records the time); `false` if still within the cool-down window.
    pub fn try_fire(&mut self, now: f32) -> bool {
        match self.last_fire {
            Some(t) if now - t < self.interval => false,
            _ => {
                self.last_fire = Some(now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn after_fires_once_when_due() {
        let mut s = Scheduler::new();
        let n = Rc::new(Cell::new(0u32));
        let n_c = Rc::clone(&n);
        s.after(1.0, move || n_c.set(n_c.get() + 1));
        // Not due yet.
        assert_eq!(s.tick(0.5), 0);
        assert_eq!(n.get(), 0);
        // Now due.
        assert_eq!(s.tick(0.6), 1);
        assert_eq!(n.get(), 1);
        // Does not fire again.
        assert_eq!(s.tick(5.0), 0);
        assert_eq!(n.get(), 1);
        assert_eq!(s.pending(), 0);
    }

    #[test]
    fn every_repeats_and_handles_large_dt() {
        let mut s = Scheduler::new();
        let n = Rc::new(Cell::new(0u32));
        let n_c = Rc::clone(&n);
        s.every(1.0, move || n_c.set(n_c.get() + 1));
        // A single 3.5s tick spans three full intervals (1,2,3).
        let fired = s.tick(3.5);
        assert_eq!(fired, 3);
        assert_eq!(n.get(), 3);
        // Next interval at t=4.
        s.tick(0.6); // now = 4.1
        assert_eq!(n.get(), 4);
    }

    #[test]
    fn request_frame_fires_next_tick_only() {
        let mut s = Scheduler::new();
        let n = Rc::new(Cell::new(0u32));
        let n_c = Rc::clone(&n);
        s.request_frame(move || n_c.set(n_c.get() + 1));
        assert_eq!(s.pending_frames(), 1);
        assert_eq!(s.tick(0.0), 1);
        assert_eq!(n.get(), 1);
        // Gone after one tick.
        assert_eq!(s.tick(0.0), 0);
        assert_eq!(n.get(), 1);
    }

    #[test]
    fn cancel_prevents_fire() {
        let mut s = Scheduler::new();
        let n = Rc::new(Cell::new(0u32));
        let n_c = Rc::clone(&n);
        let id = s.after(1.0, move || n_c.set(n_c.get() + 1));
        assert!(s.cancel(id));
        assert!(!s.cancel(id));
        s.tick(2.0);
        assert_eq!(n.get(), 0);
    }

    #[test]
    fn debounce_fires_only_after_quiet_period() {
        let mut d = Debounce::new(0.3);
        d.signal(0.0);
        assert!(d.is_pending());
        // Re-signalled before the quiet period elapses -> resets.
        assert!(!d.poll(0.2));
        d.signal(0.2);
        assert!(!d.poll(0.4)); // only 0.2s since last signal
                               // 0.3s of quiet -> fires.
        assert!(d.poll(0.5));
        assert!(!d.is_pending());
        // Nothing pending -> no fire.
        assert!(!d.poll(1.0));
    }

    #[test]
    fn throttle_limits_rate_leading_edge() {
        let mut t = Throttle::new(1.0);
        assert!(t.try_fire(0.0)); // first fires
        assert!(!t.try_fire(0.5)); // within cooldown
        assert!(!t.try_fire(0.99));
        assert!(t.try_fire(1.0)); // cooldown elapsed
        assert!(!t.try_fire(1.5));
    }
}
