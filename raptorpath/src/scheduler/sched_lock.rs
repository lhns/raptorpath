//! The scheduler's mutex, with a lock-order witness (threading P1, D2).
//!
//! **The rule.** No quinn call is made while the scheduler is held. quinn
//! takes its per-connection mutex inside every `Connection` method and holds
//! it across AES-GCM and `sendmsg`; a scheduler guard alive across such a call
//! makes every other scheduler user wait for a quinn driver hold (lock
//! nesting sched → quinn-state), and it would be a deadlock the day any quinn
//! callback (the passthrough CC reading engine state) takes the scheduler.
//!
//! **The witness.** [`SchedMutex`] is the only way to the scheduler. Its
//! guard, [`SchedGuard`], counts itself on a thread-local while alive
//! (`cfg(debug_assertions)` only), and every quinn-touching seam in
//! `transport::quic` calls [`assert_not_held`] first. A parking_lot guard is
//! `!Send`, so it can never be held across an `.await` in a spawned task:
//! "held on this thread" is exactly "held across this call". Release builds
//! compile the counter and the check away (the guard is a plain wrapper).
//!
//! The newtype deliberately does not `Deref` to the inner mutex: a bare
//! `parking_lot` guard would bypass the witness.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::Scheduler;

thread_local! {
    /// Scheduler guards alive on this thread (debug builds only).
    static HELD: Cell<u32> = const { Cell::new(0) };
}

/// Seam checks executed (debug builds; 0 in release). A test reads it to
/// prove the witness ran (MEASUREMENT DISCIPLINE rule 1).
static CHECKS: AtomicU64 = AtomicU64::new(0);
/// Seam checks that found the scheduler held (debug builds).
static VIOLATIONS: AtomicU64 = AtomicU64::new(0);
/// Scheduler acquisitions observed (debug builds): proves the counter is
/// live, so a zero `VIOLATIONS` is not a dead instrument.
static ACQUIRES: AtomicU64 = AtomicU64::new(0);
/// The first violating seam, for the failure message.
static FIRST: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);

/// `parking_lot::Mutex<Scheduler>` behind the lock-order witness.
pub struct SchedMutex(parking_lot::Mutex<Scheduler>);

/// The scheduler guard. Derefs to [`Scheduler`].
pub struct SchedGuard<'a> {
    g: parking_lot::MutexGuard<'a, Scheduler>,
}

impl SchedMutex {
    pub fn new(s: Scheduler) -> Self {
        Self(parking_lot::Mutex::new(s))
    }

    /// Acquire the scheduler.
    #[inline]
    pub fn lock(&self) -> SchedGuard<'_> {
        let g = self.0.lock();
        Self::enter();
        SchedGuard { g }
    }

    /// `parking_lot::Mutex::try_lock_for`, witnessed.
    pub fn try_lock_for(&self, d: Duration) -> Option<SchedGuard<'_>> {
        let g = self.0.try_lock_for(d)?;
        Self::enter();
        Some(SchedGuard { g })
    }

    #[inline]
    fn enter() {
        #[cfg(debug_assertions)]
        {
            HELD.with(|h| h.set(h.get() + 1));
            ACQUIRES.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for SchedGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        HELD.with(|h| h.set(h.get().saturating_sub(1)));
    }
}

impl std::ops::Deref for SchedGuard<'_> {
    type Target = Scheduler;
    #[inline]
    fn deref(&self) -> &Scheduler {
        &self.g
    }
}

impl std::ops::DerefMut for SchedGuard<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Scheduler {
        &mut self.g
    }
}

/// Scheduler guards alive on the calling thread (always 0 in release).
pub fn held_on_this_thread() -> u32 {
    HELD.with(|h| h.get())
}

/// The quinn-seam check: called at the top of every `transport::quic` method
/// that touches a `quinn::Connection`. Debug builds count the check, and on
/// a violation record the seam and panic; release builds do nothing.
#[inline]
#[track_caller]
pub fn assert_not_held(seam: &'static str) {
    #[cfg(debug_assertions)]
    {
        CHECKS.fetch_add(1, Ordering::Relaxed);
        let held = held_on_this_thread();
        if held > 0 {
            VIOLATIONS.fetch_add(1, Ordering::Relaxed);
            let at = std::panic::Location::caller();
            let msg = format!(
                "lock order: quinn seam `{seam}` ({at}) called with {held} scheduler guard(s) held on this thread"
            );
            FIRST.lock().get_or_insert_with(|| msg.clone());
            panic!("{msg}");
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = seam;
}

/// `(checks, violations, acquires)` so far in this process (all 0 in release).
pub fn witness_counts() -> (u64, u64, u64) {
    (
        CHECKS.load(Ordering::Relaxed),
        VIOLATIONS.load(Ordering::Relaxed),
        ACQUIRES.load(Ordering::Relaxed),
    )
}

/// The first recorded violation, if any.
pub fn first_violation() -> Option<String> {
    FIRST.lock().clone()
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;
    use crate::scheduler::WallClock;
    use std::sync::Arc;

    /// The witness detects what it exists to detect: a seam check with a
    /// guard alive on this thread panics; after the guard drops it passes.
    #[test]
    fn a_seam_check_under_a_live_guard_panics_and_passes_after_drop() {
        let m = SchedMutex::new(Scheduler::new(Arc::new(WallClock)));
        let g = m.lock();
        assert_eq!(held_on_this_thread(), 1);
        let r = std::panic::catch_unwind(|| assert_not_held("unit"));
        assert!(r.is_err(), "the check must fire with the scheduler held");
        drop(g);
        assert_eq!(held_on_this_thread(), 0);
        assert_not_held("unit");
        // A statement-scoped temporary is released at the `;`.
        let _n = m.lock().live_paths().len();
        assert_not_held("unit");
    }
}
