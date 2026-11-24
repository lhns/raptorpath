//! The receiver's consumer stage under the order hold H (L6, three-knobs.md
//! §2.6; plan docs/derivations/visualizer-three-knobs-plan.md §7 P4). Output
//! only: nothing here feeds back into the sender.
//!
//! A pure per-tick stage. In: the seqs the decoder released this tick and
//! the floor the receiver holds. A hole (a seq missing below a higher known
//! one) is detected when a higher seq decodes or a floor passes it, and is
//! released at `release_at(t_detect, H, t_floor, H_f)`. Delivery is in seq
//! order up to the first unreleased hole; a late fill of a released hole is
//! delivered at once, out of order (O3 = yes, §2.3). H = 0 delivers every
//! unit at its decode tick; H = ∞ with no floor pass is strict order.
//!
//! H_f = 0 (Q-V5): the model has no reordering, so no form of the floor
//! hold can be told apart here.

use crate::knobs;

/// The floor hold H_f (Q-V5 default).
pub const FLOOR_HOLD_S: f64 = 0.0;

pub struct Consumer {
    hold_s: f64,
    /// Per seq: decoded, delivery time (NaN until delivered), detection and
    /// floor-pass times (∞ until they happen), released (passed) or not.
    decoded: Vec<bool>,
    delivered_at: Vec<f64>,
    /// Per seq: delivered after a higher seq was delivered (the toy's ooo).
    ooo_seq: Vec<bool>,
    t_detect: Vec<f64>,
    t_floor: Vec<f64>,
    released: Vec<bool>,
    /// The delivery pointer: every seq below it is delivered or released.
    next: usize,
    /// One past the highest seq known (decoded or passed by a floor).
    known: usize,
    /// The floor already applied.
    floor: usize,
    max_delivered: Option<usize>,
    pub rel_by_hold: u32,
    pub rel_by_floor: u32,
    /// Units delivered after a higher seq was delivered.
    pub ooo: u32,
    /// Stall episodes (start s, length s): a detected, unreleased hole holds
    /// later known seqs at the pointer.
    pub stalls: Vec<(f64, f64)>,
    stall_start: Option<f64>,
}

impl Consumer {
    pub fn new(hold_s: f64) -> Self {
        Self {
            hold_s,
            decoded: Vec::new(),
            delivered_at: Vec::new(),
            ooo_seq: Vec::new(),
            t_detect: Vec::new(),
            t_floor: Vec::new(),
            released: Vec::new(),
            next: 0,
            known: 0,
            floor: 0,
            max_delivered: None,
            rel_by_hold: 0,
            rel_by_floor: 0,
            ooo: 0,
            stalls: Vec::new(),
            stall_start: None,
        }
    }

    fn grow(&mut self, n: usize) {
        if self.decoded.len() < n {
            self.decoded.resize(n, false);
            self.delivered_at.resize(n, f64::NAN);
            self.ooo_seq.resize(n, false);
            self.t_detect.resize(n, f64::INFINITY);
            self.t_floor.resize(n, f64::INFINITY);
            self.released.resize(n, false);
        }
    }

    /// Mark every unknown seq below `upto` as known; a missing one becomes a
    /// hole detected now.
    fn know(&mut self, upto: usize, now: f64) {
        self.grow(upto);
        for s in self.known..upto {
            if !self.decoded[s] {
                self.t_detect[s] = now;
            }
        }
        self.known = self.known.max(upto);
    }

    fn deliver(&mut self, s: usize, now: f64) {
        self.delivered_at[s] = now;
        if self.max_delivered.is_some_and(|m| m > s) {
            self.ooo += 1;
            self.ooo_seq[s] = true;
        }
        self.max_delivered = Some(self.max_delivered.map_or(s, |m| m.max(s)));
    }

    /// One tick at time `now` (s): the seqs decoded this tick, then the
    /// floor the receiver holds.
    pub fn step(&mut self, now: f64, decoded: &[u64], floor: u32) {
        for &q in decoded {
            let s = q as usize;
            self.grow(s + 1);
            self.decoded[s] = true;
            if s < self.next {
                self.deliver(s, now); // a late fill of a released hole
            }
            self.know(s + 1, now);
        }
        let f = floor as usize;
        if f > self.floor {
            self.grow(f);
            for s in self.floor..f {
                if self.t_floor[s] > now {
                    self.t_floor[s] = now;
                }
            }
            self.floor = f;
            self.know(f, now);
        }
        while self.next < self.known {
            let s = self.next;
            if self.decoded[s] {
                if self.delivered_at[s].is_nan() {
                    self.deliver(s, now);
                }
                self.next += 1;
                continue;
            }
            let (td, tf) = (self.t_detect[s], self.t_floor[s]);
            if knobs::release_at(td, self.hold_s, tf, FLOOR_HOLD_S) > now {
                break; // held: the stall is open
            }
            self.released[s] = true;
            if knobs::released_by_hold(td, self.hold_s, tf, FLOOR_HOLD_S) {
                self.rel_by_hold += 1;
            } else {
                self.rel_by_floor += 1;
            }
            self.next += 1;
        }
        let blocked = self.next < self.known;
        match (blocked, self.stall_start) {
            (true, None) => self.stall_start = Some(now),
            (false, Some(t0)) => {
                self.stalls.push((t0, now - t0));
                self.stall_start = None;
            }
            _ => {}
        }
    }

    /// Delivery time of seq s at the consumer (NaN until delivered).
    pub fn delivered_at(&self, s: usize) -> f64 {
        self.delivered_at.get(s).copied().unwrap_or(f64::NAN)
    }

    /// Whether seq s was delivered out of order (after a higher seq).
    pub fn out_of_order(&self, s: usize) -> bool {
        self.ooo_seq.get(s).copied().unwrap_or(false)
    }

    /// Holes released so far (by either term of L6).
    pub fn released_holes(&self) -> u32 {
        self.released.iter().filter(|&&r| r).count() as u32
    }

    /// Stall episodes, the open one included (up to `now`).
    pub fn stall_episodes(&self, now: f64) -> Vec<(f64, f64)> {
        let mut v = self.stalls.clone();
        if let Some(t0) = self.stall_start {
            v.push((t0, now - t0));
        }
        v
    }
}

// --- Readouts over units (plan §4.3; three-knobs §17.4) ---------------------

/// Recovery-time windows (three-knobs §17.4, registered for stage 3d):
/// hand-over windows of 0.5 s every 0.1 s; the reference deadline 0.4 s.
pub const RECOVERY_WINDOW_S: f64 = 0.5;
pub const RECOVERY_STEP_S: f64 = 0.1;
/// L0 reads hand-over [0.5 s, outage start − 0.1 s].
pub const RECOVERY_PRE_START_S: f64 = 0.5;
pub const RECOVERY_PRE_GAP_S: f64 = 0.1;
/// θ = L0 − max(0.02, 3·sd0).
pub const RECOVERY_THETA_MIN: f64 = 0.02;
pub const RECOVERY_THETA_SD: f64 = 3.0;
/// The readable end: the last hand-over minus 2 s (the object's tail is never read).
pub const RECOVERY_TAIL_S: f64 = 2.0;

/// On-time share of the units whose hand-over lies in [a, b); None if none.
fn share_in(h: &[f64], on_time: &[bool], a: f64, b: f64) -> Option<f64> {
    let lo = h.partition_point(|&x| x < a);
    let hi = h.partition_point(|&x| x < b);
    (hi > lo).then(|| on_time[lo..hi].iter().filter(|&&o| o).count() as f64 / (hi - lo) as f64)
}

/// Recovery time after the outage (§17.4): `t_rec − outage_end`, where
/// `t_rec` is the start of the first window at or after the outage end from
/// which every window to the readable end has an on-time share ≥ θ. `h` is
/// every unit's hand-over time (non-decreasing), `on_time` whether it was
/// delivered within the reference deadline. NaN: "not within the run".
pub fn recovery_time(h: &[f64], on_time: &[bool], outage_start: f64, outage_end: f64) -> f64 {
    let Some(&last) = h.last() else { return f64::NAN };
    let readable_end = last - RECOVERY_TAIL_S;
    let pre_end = outage_start - RECOVERY_PRE_GAP_S;
    let Some(l0) = share_in(h, on_time, RECOVERY_PRE_START_S, pre_end) else { return f64::NAN };
    let windows = |from: f64, to: f64| -> Vec<(f64, f64)> {
        // No window starts past the last hand-over (bounds the scan when
        // the outage start is ∞).
        let to = to.min(last + RECOVERY_WINDOW_S);
        let mut v = Vec::new();
        let mut k = 0u32;
        loop {
            let w = k as f64 * RECOVERY_STEP_S;
            k += 1;
            if w + RECOVERY_WINDOW_S > to {
                break;
            }
            if w < from {
                continue;
            }
            if let Some(sh) = share_in(h, on_time, w, w + RECOVERY_WINDOW_S) {
                v.push((w, sh));
            }
        }
        v
    };
    let pre: Vec<f64> = windows(RECOVERY_PRE_START_S, pre_end).iter().map(|x| x.1).collect();
    let sd0 = if pre.is_empty() {
        0.0
    } else {
        let m = pre.iter().sum::<f64>() / pre.len() as f64;
        (pre.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / pre.len() as f64).sqrt()
    };
    let theta = l0 - RECOVERY_THETA_MIN.max(RECOVERY_THETA_SD * sd0);
    let post = windows(outage_end, readable_end);
    // The first window from which every later readable window clears θ.
    let mut t_rec = f64::NAN;
    for (w, sh) in post.iter().rev() {
        if *sh < theta {
            break;
        }
        t_rec = *w;
    }
    t_rec - outage_end
}

/// The p-quantile (nearest rank, as `get_lat_percentile`) of `v`; ∞ entries
/// are values (undelivered units), so a quantile past the delivered share is ∞.
pub fn quantile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let idx = ((s.len() as f64 - 1.0) * p.clamp(0.0, 1.0)).round() as usize;
    s[idx.min(s.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    const INF: f64 = f64::INFINITY;

    /// Units 0..n decode at tick `dec[s]` (None = never), one floor event.
    fn drive(hold: f64, dec: &[Option<u32>], floor_at: Option<(u32, u32)>, ticks: u32) -> Consumer {
        let mut c = Consumer::new(hold);
        let mut f = 0;
        for t in 0..ticks {
            let now = t as f64 * 0.001;
            let d: Vec<u64> = dec
                .iter()
                .enumerate()
                .filter(|(_, x)| **x == Some(t))
                .map(|(s, _)| s as u64)
                .collect();
            if let Some((ft, fv)) = floor_at {
                if t == ft {
                    f = fv;
                }
            }
            c.step(now, &d, f);
        }
        c
    }

    #[test]
    fn h_zero_delivers_at_the_decode_tick() {
        let dec = [Some(0), None, Some(1), Some(2), Some(40), Some(3)];
        let c = drive(0.0, &dec, None, 60);
        for (s, d) in dec.iter().enumerate() {
            match d {
                Some(t) => assert_eq!(c.delivered_at(s), *t as f64 * 0.001, "seq {s}"),
                None => assert!(c.delivered_at(s).is_nan()),
            }
        }
        assert_eq!(c.rel_by_hold, 2); // seqs 1 and 4, each released at detection
        assert_eq!(c.ooo, 1); // seq 4 (decoded at 40) after seq 5
        assert!(c.out_of_order(4) && (0..6).filter(|&s| c.out_of_order(s)).count() == 1);
    }

    #[test]
    fn h_infinite_is_strict_order_until_the_hole_fills() {
        let dec = [Some(0), Some(30), Some(1), Some(2)];
        let c = drive(INF, &dec, None, 50);
        assert_eq!(c.delivered_at(0), 0.0);
        for s in 1..4 {
            assert_eq!(c.delivered_at(s), 30.0 * 0.001, "seq {s} waits for seq 1");
        }
        assert_eq!(c.ooo, 0);
        assert_eq!(c.rel_by_hold + c.rel_by_floor, 0);
        assert_eq!(c.stalls, vec![(0.001, 30.0 * 0.001 - 0.001)]);
    }

    #[test]
    fn the_floor_releases_a_held_hole_and_a_late_fill_is_out_of_order() {
        // seq 1 is missing from t = 1 ms; the floor 2 arrives at 10 ms; seq 1 fills at 20 ms.
        let dec = [Some(0), Some(20), Some(1)];
        let c = drive(INF, &dec, Some((10, 2)), 30);
        assert_eq!(c.delivered_at(2), 10.0 * 0.001);
        assert_eq!(c.delivered_at(1), 20.0 * 0.001);
        assert_eq!((c.rel_by_hold, c.rel_by_floor, c.ooo), (0, 1, 1));
        assert_eq!(c.released_holes(), 1);
    }

    #[test]
    fn the_hold_releases_at_detect_plus_h() {
        let dec = [Some(0), None, Some(5), Some(6)];
        let c = drive(0.02, &dec, None, 40);
        // seq 1 detected at 5 ms, released at 25 ms; 2 and 3 wait for it.
        for s in [2, 3] {
            let d = c.delivered_at(s);
            assert!((0.0249..0.0261).contains(&d), "seq {s} at {d}");
        }
        assert_eq!(c.rel_by_hold, 1);
    }

    #[test]
    fn recovery_time_on_a_synthetic_run() {
        // 100 units/s over 12 s; outage [2, 3) s; on time except units
        // handed over in [2, 4.25) s.
        let h: Vec<f64> = (0..1200).map(|u| u as f64 / 100.0).collect();
        let on: Vec<bool> = h.iter().map(|&x| !(2.0..4.25).contains(&x)).collect();
        let r = recovery_time(&h, &on, 2.0, 3.0);
        // The first clean 0.5 s window starts at 4.3 s (windows every 0.1 s).
        assert!((r - 1.3).abs() < 1e-9, "{r}");
        // No outage in the run: not within the run.
        assert!(recovery_time(&h, &on, INF, INF).is_nan());
        // A run too short to read: NaN, not a number.
        assert!(recovery_time(&h[..150], &on[..150], 1.0, 1.2).is_nan());
    }

    #[test]
    fn quantiles_treat_undelivered_as_infinite() {
        assert_eq!(quantile(&[0.1, 0.2, INF, INF], 0.0), 0.1);
        assert_eq!(quantile(&[0.1, 0.2, INF, INF], 1.0), INF);
        assert_eq!(quantile(&[0.1, 0.2, 0.3], 0.5), 0.2);
    }
}
