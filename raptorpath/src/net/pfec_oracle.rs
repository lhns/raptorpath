//! Executed P_fec sliding oracle (test-only). Spec:
//! `docs/derivations/pfec-oracle-spec.md` (§6 governs over §0-§5);
//! derivation context `docs/derivations/pfec-continuous.md` §8.
//!
//! Slots are source sends. One GE chain over sources and repairs.
//!
//! CALLED from the engine (so the oracle runs the shipped code):
//!   `RlcWindowEncoder` / `RlcWindowDecoder`, `decode_inert_floor`,
//!   `TaperBudget` + `TaperFunction`, `span_seat` / `trailing_span`
//!   (extracted from `emit_source.rs`), `shed_deadline_us`,
//!   `delta_budget_b_of`, `shed_recv_hold`.
//! COPIED (string-pinned in `copied_engine_lines_are_still_in_the_engine`;
//! a pin guards text, K4 is what validates behaviour):
//!   the repair-debt loop (`emit_source.rs`), ack debt cancellation
//!   (`sender_phases.rs`, switch `ack_cancel`), the encoder window-cap
//!   eviction (`mod.rs`), the GE channel, the ack clock (cadence `m_bar`,
//!   delay `f_slots`) and the per-seat resolution rule.
//!
//! The readings (`reading_*`) are `#[ignore]`d: they run under a
//! registration, not in CI. K0-K3 gate them; K4 needs an engine row (see
//! `k4_w1_matches_engine_row`).

use super::emit_source::{span_seat, trailing_span, SpanPlacement};
use super::receiver::decode_inert_floor;
use super::*;
use crate::control::{LossEstimator, TaperBudget, TaperFunction};
use crate::fec::{RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder};
use std::collections::VecDeque;

// ---------------------------------------------------------------- RNG + GE

/// SplitMix64: seedable, no dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Gilbert-Elliott, loss iff Bad. p + q = 2/(sigma2 + 1), p = eps (p + q)
/// (the `ge_pq` of `pfec_check.py`).
struct Ge {
    p: f64,
    q: f64,
    bad: bool,
    rng: Rng,
}
impl Ge {
    fn new(eps: f64, sigma2: f64, seed: u64, start_good: bool) -> Self {
        let t = 2.0 / (sigma2 + 1.0);
        let mut rng = Rng(seed);
        let bad = !start_good && rng.f64() < eps;
        Ge { p: eps * t, q: (1.0 - eps) * t, bad, rng }
    }
    /// One wire symbol: step the chain, then lost iff Bad.
    fn step(&mut self) -> bool {
        let u = self.rng.f64();
        self.bad = if self.bad { u >= self.q } else { u < self.p };
        self.bad
    }
}

// ------------------------------------------------------------- the machine

/// How the receiver resolves a hole for the ack clock (a clock event only,
/// never counted as FEC).
#[derive(Clone, Copy)]
enum Resolve {
    /// Evicting seat: give up `hold` slots after detection.
    Evict { hold: u64 },
    /// Reliable seat: ARQ lands `l_arq` slots after detection.
    Arq { l_arq: u64 },
}

#[derive(Clone)]
struct Cfg {
    eps: f64,
    sigma2: f64,
    /// The handed r (what the law is evaluated at).
    rate: f64,
    spare: f64,
    // `span_seat` inputs (sym/s, s, dial b, s).
    rate_sym: f64,
    rtprop_s: f64,
    b: f64,
    jitter_s: f64,
    win_cap: usize,
    keep_behind: usize,
    /// Sources per reported ack, and the report delay in slots.
    m_bar: u64,
    f_slots: u64,
    ack_cancel: bool,
    resolve: Resolve,
    n: usize,
    seed: u64,
    /// Scripted source losses (repairs lossless); `None` = the GE chain.
    script: Option<Vec<usize>>,
    start_good: bool,
}

impl Cfg {
    fn base(eps: f64, sigma2: f64, rate: f64, n: usize, seed: u64) -> Cfg {
        Cfg {
            eps,
            sigma2,
            rate,
            spare: rate + 1.0,
            rate_sym: 95.0,
            rtprop_s: 0.1,
            b: 0.5,
            jitter_s: 0.005,
            win_cap: 200,
            keep_behind: 100,
            m_bar: 1,
            f_slots: 0,
            ack_cancel: true,
            resolve: Resolve::Evict { hold: 1 << 30 },
            n,
            seed,
            script: None,
            start_good: false,
        }
    }
    /// An A* row (L0 c3, Realtime b = 1/2, rate ~95 sym/s): F = 2 A*, hold
    /// H = srtt/2 from the executed `shed_recv_hold`, so A* = ceil(rate b RTT)
    /// is hit at rate b RTT = A* - 1/2.
    fn astar_row(a_star: u64, delta: u64, rate: f64, n: usize, seed: u64) -> Cfg {
        let mut c = Cfg::base(0.048, 3.76, rate, n, seed);
        let rtt = (a_star as f64 - 0.5) / (c.rate_sym * c.b);
        c.rtprop_s = rtt;
        c.jitter_s = (delta as f64 - 0.5) / c.rate_sym;
        c.f_slots = (c.rate_sym * rtt).round() as u64;
        let h = shed_recv_hold(Duration::from_secs_f64(rtt), true, true);
        c.resolve = Resolve::Evict { hold: (h.as_secs_f64() * c.rate_sym).round() as u64 };
        c
    }
}

#[derive(Clone, Copy)]
struct Hole {
    seq: u32,
    /// Slots from its send / from its detection to the decode; None = never.
    lag_send: Option<u32>,
    lag_det: Option<u32>,
}

#[derive(Default)]
struct Out {
    holes: Vec<Hole>,
    sources: u64,
    emitted: u64,
    /// max |grant - rate| over every `accrue` call (E1: exactly 0).
    max_grant_dev: f64,
    ack_advances: u64,
    acked_sum: u64,
    ranges: u64,
    leadings: u64,
    bad_payload: u64,
    /// First `Range` placement slot seen (for K1).
    first_cover: Vec<(u64, u64)>,
}

impl Out {
    fn n(&self) -> usize {
        self.holes.len()
    }
    /// P(recovered by FEC within d slots of its send); None = no deadline.
    fn p_at(&self, d: Option<u64>) -> f64 {
        let hit = self
            .holes
            .iter()
            .filter(|h| h.lag_send.is_some_and(|l| d.is_none_or(|d| l as u64 <= d)))
            .count();
        hit as f64 / self.holes.len().max(1) as f64
    }
    /// W1 = emitted / (r x sources).
    fn w1(&self, rate: f64) -> f64 {
        self.emitted as f64 / (rate * self.sources as f64).max(f64::MIN_POSITIVE)
    }
    /// Sources acked per advance-observing iteration (the gauge m-bar).
    fn m_gauge(&self) -> f64 {
        self.acked_sum as f64 / self.ack_advances.max(1) as f64
    }
    /// Misses at deadline d, and the size of the largest cluster of misses
    /// (gaps <= `gap` slots between consecutive misses).
    fn misses(&self, d: Option<u64>, gap: u32) -> (usize, usize) {
        let mut seqs: Vec<u32> = self
            .holes
            .iter()
            .filter(|h| !h.lag_send.is_some_and(|l| d.is_none_or(|d| l as u64 <= d)))
            .map(|h| h.seq)
            .collect();
        seqs.sort_unstable();
        let (mut best, mut cur) = (0usize, 0usize);
        for (i, s) in seqs.iter().enumerate() {
            cur = if i > 0 && s - seqs[i - 1] <= gap { cur + 1 } else { 1 };
            best = best.max(cur);
        }
        (seqs.len(), best)
    }
}

const SYM: u16 = 1; // smallest symbol size: cost is linear in it
fn payload(seq: u64) -> u8 {
    (seq.wrapping_mul(0x9E).wrapping_add(7) & 0xff) as u8
}

/// Run the sliding machine for `cfg.n` source sends. Holes in the last
/// `margin` slots are not counted (their repairs are not yet emitted).
fn run(cfg: &Cfg) -> Out {
    let n = cfg.n;
    let mut ch = Ge::new(cfg.eps, cfg.sigma2, cfg.seed, cfg.start_good);
    let mut enc = RlcWindowEncoder::new(SYM);
    let mut dec = RlcWindowDecoder::new(SYM);
    let mut budget = TaperBudget::new();
    // inert by E1; the engine builds it from the live estimator
    let taper = TaperFunction::from_estimator(&LossEstimator::new(), cfg.rate);
    let mut out = Out::default();

    // per-seq state: 0 pending, bit0 arrived, bit1 decoded, bit2 resolved
    let mut st = vec![0u8; n];
    let mut detect: Vec<u32> = vec![u32::MAX; n];
    let mut lag_send: Vec<u32> = vec![u32::MAX; n];
    let mut lag_det: Vec<u32> = vec![u32::MAX; n];
    let mut lost = vec![false; n];
    let mut highest_seen: Option<usize> = None;
    let mut queue: VecDeque<usize> = VecDeque::new(); // detected, unresolved
    let mut next_expected = 0usize;
    let mut pending_acks: VecDeque<(u64, u64)> = VecDeque::new();
    let (mut prev_ack, mut debt, mut offset) = (0u64, 0.0f64, 0u64);

    for t in 0..n {
        let now = t as u64;
        // -- ack arrivals (on_ack_advance)
        while pending_acks.front().is_some_and(|a| a.0 <= now) {
            let (_, ack) = pending_acks.pop_front().unwrap();
            if ack > prev_ack {
                let newly_acked = ack - prev_ack;
                out.ack_advances += 1;
                out.acked_sum += newly_acked;
                if cfg.ack_cancel {
                    let debt_reduction = newly_acked as f64 * cfg.rate;
                    debt = (debt - debt_reduction).max(0.0);
                }
                enc.advance(ack.saturating_sub(cfg.keep_behind as u64));
                offset = 0;
                prev_ack = ack;
            }
        }
        // -- the source symbol
        let p = [payload(t as u64)];
        let sym = enc.add_source(&p);
        out.sources += 1;
        let is_lost = match &cfg.script {
            Some(s) => s.contains(&t),
            None => ch.step(),
        };
        lost[t] = is_lost;
        let mut delivered: Vec<(u64, bytes::Bytes)> = Vec::new();
        if !is_lost {
            st[t] |= 1;
            st[t] |= 4;
            delivered.extend(dec.add_symbol(&sym));
            // detection: a higher seq arrived -> lower unarrived seqs detected
            let from = highest_seen.map_or(0, |h| h + 1);
            for s in from..t {
                if st[s] & 1 == 0 {
                    detect[s] = t as u32;
                    queue.push_back(s);
                }
            }
            highest_seen = Some(t);
        }
        // -- span seat (called), grant (called), debt loop (copied)
        let (a_star, delta) = span_seat(
            cfg.rate_sym,
            cfg.rtprop_s,
            cfg.b,
            cfg.jitter_s,
            enc.window_size(),
        );
        let grant = budget.accrue(cfg.rate, offset, &taper, enc.window_size(), cfg.spare);
        out.max_grant_dev = out.max_grant_dev.max((grant - cfg.rate).abs());
        debt += grant;
        offset += 1;
        while debt >= 1.0 && enc.window_size() > 0 {
            debt -= 1.0;
            let (ws, we) = enc.window_span();
            let rep = match trailing_span(ws, we, a_star, delta) {
                SpanPlacement::Range { start, count } => {
                    out.ranges += 1;
                    if cfg.script.is_some() {
                        out.first_cover.push((now, start + count as u64));
                    }
                    enc.generate_repair_range(start, count)
                        .unwrap_or_else(|| enc.generate_repair())
                }
                SpanPlacement::Leading => {
                    out.leadings += 1;
                    enc.generate_repair()
                }
            };
            out.emitted += 1;
            let rep_lost = cfg.script.is_none() && ch.step();
            if !rep_lost {
                delivered.extend(dec.add_symbol(&rep));
            }
        }
        // -- decodes
        for (seq, data) in delivered {
            let s = seq as usize;
            if s >= n || st[s] & 1 != 0 || st[s] & 2 != 0 {
                continue;
            }
            st[s] |= 2 | 4;
            lag_send[s] = (now - seq) as u32;
            lag_det[s] = if detect[s] == u32::MAX { 0 } else { (now as u32) - detect[s] };
            if data.first() != Some(&payload(seq)) {
                out.bad_payload += 1;
            }
        }
        // -- resolution rule (a clock event only)
        while let Some(&f) = queue.front() {
            if st[f] & 4 != 0 {
                queue.pop_front();
                continue;
            }
            let wait = match cfg.resolve {
                Resolve::Evict { hold } => hold,
                Resolve::Arq { l_arq } => l_arq,
            };
            if detect[f] as u64 + wait <= now {
                st[f] |= 4;
                queue.pop_front();
            } else {
                break;
            }
        }
        while next_expected <= t && st[next_expected] & 4 != 0 {
            next_expected += 1;
        }
        // -- ack report, delivered f slots later
        if (now + 1) % cfg.m_bar == 0 {
            pending_acks.push_back((now + cfg.f_slots, next_expected as u64));
        }
        // -- receiver prune, as receiver.rs does
        dec.advance(decode_inert_floor(next_expected as u64, cfg.win_cap as u64));
        // -- encoder window-cap eviction (copied; EVICT seat)
        if enc.window_size() > cfg.win_cap {
            let (oldest, _) = enc.window_span();
            enc.advance(oldest + (enc.window_size() - cfg.win_cap) as u64);
        }
    }
    let margin = 300.min(n / 4);
    for s in 0..n.saturating_sub(margin) {
        if lost[s] {
            out.holes.push(Hole {
                seq: s as u32,
                lag_send: (lag_send[s] != u32::MAX).then_some(lag_send[s]),
                lag_det: (lag_det[s] != u32::MAX).then_some(lag_det[s]),
            });
        }
    }
    out
}

/// Mean and standard error over seeds.
fn mean_se(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    if v.len() < 2 {
        return (m, 0.0);
    }
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1.0);
    (m, (var / n).sqrt())
}

// -------------------------------------------------------- block machine (K2)

struct BlockOut {
    recovered: u64,
    lost: u64,
    mismatch: u64,
    blocks: u64,
}

/// K2: placement replaced by fixed blocks. After a block's W sources emit
/// floor(rW) + Bernoulli(frac) repairs over [kW, (k+1)W). One GE chain,
/// starting Good (`temporal_oracle.rs:1361-1423`).
fn run_blocks(eps: f64, sigma2: f64, w: usize, r: f64, blocks: usize, seed: u64) -> BlockOut {
    let mut ch = Ge::new(eps, sigma2, seed, true);
    let mut rng = Rng(seed ^ 0xA5A5_5A5A_1234_5678);
    let mut enc = RlcWindowEncoder::new(SYM);
    let mut dec = RlcWindowDecoder::new(SYM);
    let mut o = BlockOut { recovered: 0, lost: 0, mismatch: 0, blocks: 0 };
    for k in 0..blocks {
        let base = (k * w) as u64;
        let mut lost = vec![false; w];
        let mut nlost = 0u64;
        for (i, l) in lost.iter_mut().enumerate() {
            let sym = enc.add_source(&[payload(base + i as u64)]);
            *l = ch.step();
            if *l {
                nlost += 1;
            } else {
                dec.add_symbol(&sym);
            }
        }
        let x = r * w as f64;
        let mut nrep = x.floor() as u64;
        if rng.f64() < x.fract() {
            nrep += 1;
        }
        let (mut surv, mut rec) = (0u64, 0u64);
        for _ in 0..nrep {
            let rep = enc
                .generate_repair_range(base, w as u16)
                .expect("block window present");
            if !ch.step() {
                surv += 1;
                for (seq, _) in dec.add_symbol(&rep) {
                    if lost[(seq - base) as usize] {
                        rec += 1;
                    }
                }
            }
        }
        o.recovered += rec;
        o.lost += nlost;
        // the block predicate: every loss recovered iff survivors >= losses
        let decoder_full = rec == nlost;
        if decoder_full != (surv >= nlost) {
            o.mismatch += 1;
        }
        o.blocks += 1;
        enc.advance(base + w as u64);
        dec.advance(base + w as u64);
    }
    o
}

/// C5 'after': exact per-loss recovery with n repairs after the window's W
/// sources, GE-correlated repair erasures, stationary start (port of
/// `pfec_review.py::palm_joint_exact_n`, layout 'after').
fn c5_after_n(eps: f64, sigma2: f64, w: usize, n: usize) -> f64 {
    let t = 2.0 / (sigma2 + 1.0);
    let (p, q) = (eps * t, (1.0 - eps) * t);
    let idx = |k: usize, c: usize| k * (n + 1) + c;
    let mut f = vec![[0.0f64; 2]; (w + 1) * (n + 1)];
    for i in 0..w + n {
        let is_rep = i >= w;
        let mut nf = vec![[0.0f64; 2]; (w + 1) * (n + 1)];
        for k in 0..=w {
            for c in 0..=n {
                let cur = f[idx(k, c)];
                let (tg, tb) = if i == 0 {
                    if k == 0 && c == 0 { (1.0 - eps, eps) } else { (0.0, 0.0) }
                } else {
                    (cur[0] * (1.0 - p) + cur[1] * q, cur[0] * p + cur[1] * (1.0 - q))
                };
                if tg == 0.0 && tb == 0.0 {
                    continue;
                }
                if is_rep {
                    if c < n {
                        nf[idx(k, c + 1)][0] += tg;
                    }
                    nf[idx(k, c)][1] += tb;
                } else {
                    nf[idx(k, c)][0] += tg;
                    if k < w {
                        nf[idx(k + 1, c)][1] += tb;
                    }
                }
            }
        }
        f = nf;
    }
    let (mut num, mut den) = (0.0, 0.0);
    for k in 0..=w {
        for c in 0..=n {
            let m = f[idx(k, c)][0] + f[idx(k, c)][1];
            den += m * k as f64;
            if c >= k {
                num += m * k as f64;
            }
        }
    }
    num / den
}

fn c5_after(eps: f64, sigma2: f64, w: usize, r: f64) -> f64 {
    let x = r * w as f64;
    let n0 = x.floor() as usize;
    let fr = x - n0 as f64;
    let a = c5_after_n(eps, sigma2, w, n0);
    if fr == 0.0 { a } else { (1.0 - fr) * a + fr * c5_after_n(eps, sigma2, w, n0 + 1) }
}

/// Closed-form r*(eps, sigma2, W, t) (`pfec_review.py::r_star_closed`).
fn r_star_closed(e: f64, s2: f64, w: f64, t: f64) -> f64 {
    let z = raptorpath_math::normal_quantile(1.0 - t / e);
    (e / (1.0 - e) + z * (e * s2 / (w * (1.0 - e))).sqrt()).max(0.0)
}

// -------------------------------------------------------------- cell tables

/// W 201 rows, c2/c3 nominal (spec §3 table): rate = link/9600 bit, RTT,
/// F = rate RTT, Delta = ceil(rate J), reliable seat, d = L_arq = 1.5 srtt.
struct W201 {
    name: &'static str,
    eps: f64,
    sigma2: f64,
    rate_sym: f64,
    rtt_s: f64,
    jitter_s: f64,
    /// r_c = eps/(1-eps), the Auto r* and the Realtime r*.
    r_auto: f64,
}
const W201_CELLS: [W201; 2] = [
    W201 { name: "c2", eps: 0.013 / 0.513, sigma2: 1.0 + 2.0 * (1.0 - 0.513) / 0.513, rate_sym: 10_417.0, rtt_s: 0.010, jitter_s: 0.003, r_auto: 0.0910 },
    W201 { name: "c3", eps: 0.02 / 0.42, sigma2: 1.0 + 2.0 * (1.0 - 0.42) / 0.42, rate_sym: 2_083.0, rtt_s: 0.040, jitter_s: 0.005, r_auto: 0.1579 },
];

impl W201 {
    fn cfg(&self, r: f64, n: usize, seed: u64, m_bar: u64) -> (Cfg, u64) {
        let mut c = Cfg::base(self.eps, self.sigma2, r, n, seed);
        c.rate_sym = self.rate_sym;
        // A* = W = window_size: rtprop is set so rate D exceeds any window
        c.rtprop_s = 0.1;
        c.b = 2.0;
        c.jitter_s = self.jitter_s;
        c.keep_behind = c.win_cap; // derived W* clamps to win_cap
        c.m_bar = m_bar;
        c.f_slots = (self.rate_sym * self.rtt_s).round() as u64;
        let l_arq = (1.5 * self.rate_sym * self.rtt_s).round() as u64;
        c.resolve = Resolve::Arq { l_arq };
        (c, l_arq)
    }
    fn r_c(&self) -> f64 {
        self.eps / (1.0 - self.eps)
    }
}

// ------------------------------------------------------------ drift guards

fn norm_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// An engine edit to a copied line fails the oracle's build instead of
/// drifting silently (spec §1.1).
#[test]
fn copied_engine_lines_are_still_in_the_engine() {
    let emit = norm_ws(include_str!("emit_source.rs"));
    let phases = norm_ws(include_str!("sender_phases.rs"));
    let modrs = norm_ws(include_str!("mod.rs"));
    for s in [
        "st.repair_debt += repair_rate;",
        "while st.repair_debt >= 1.0 && st.encoder.window_size() > 0 {",
        "st.repair_debt -= 1.0;",
    ] {
        assert!(emit.contains(s), "emit_source.rs lost copied line: {s}");
    }
    for s in [
        "let debt_reduction = newly_acked as f64 * repair_rate;",
        "st.repair_debt = (st.repair_debt - debt_reduction).max(0.0);",
        "st.encoder.advance(ack.saturating_sub(keep_behind));",
    ] {
        assert!(phases.contains(s), "sender_phases.rs lost copied line: {s}");
    }
    let evict = "if !generation && st.encoder.window_size() > pol.win_cap {";
    assert!(modrs.contains(evict), "mod.rs lost the eviction condition");
}

// ------------------------------------------------------- known-answer tests

/// K0: r = 0 emits nothing, recovered = 0, R = eps; and grant == rate
/// exactly on every `accrue` call (E1), also at a W 201 rate.
#[test]
fn k0_r_zero_and_grant_is_rate() {
    let mut c = Cfg::astar_row(4, 1, 0.0, 20_000, 11);
    let o = run(&c);
    assert!(o.n() > 100, "need holes: {}", o.n());
    assert_eq!(o.emitted, 0);
    assert_eq!(o.p_at(None), 0.0);
    assert_eq!(o.max_grant_dev, 0.0);
    let r = crate::control::fec_rate::residual_loss_after_fec(c.eps, 0.0, 4.0, c.sigma2);
    assert!((r - c.eps).abs() < 1e-12, "R(r=0) = {r} != eps {}", c.eps);
    // E1 at the W 201 seat and the A* seat: grant is the rate, bit for bit
    for r in [0.0791, 0.158, 0.34, 0.5] {
        c.rate = r;
        c.spare = r + 1.0;
        c.n = 5_000;
        assert_eq!(run(&c).max_grant_dev, 0.0, "E1 at r {r}");
    }
    let (w, _) = W201_CELLS[1].cfg(0.158, 3_000, 5, 1);
    assert_eq!(run(&w).max_grant_dev, 0.0);
}

/// K1: lossless except source s; rate 1, spare 1, ack_cancel off. The first
/// covering repair is emitted at send slot s + Delta: lag = Delta, `Range`.
#[test]
fn k1_scripted_single_loss_lag_is_delta() {
    let s = 50usize;
    for delta in [1u64, 2, 5] {
        let mut c = Cfg::astar_row(7, delta, 1.0, 120, 3);
        c.spare = 1.0;
        c.ack_cancel = false;
        c.script = Some(vec![s]);
        let o = run(&c);
        assert_eq!(o.holes.len(), 1);
        assert_eq!(o.holes[0].lag_send, Some(delta as u32), "Delta {delta}");
        // the placement at slot s + Delta is a Range covering s
        let (_, end) = *o
            .first_cover
            .iter()
            .find(|(t, _)| *t == s as u64 + delta)
            .expect("a Range placement at s + Delta");
        assert!(end > s as u64);
        // and `trailing_span` agrees with it directly
        match trailing_span(0, s as u64 + delta, 7, delta) {
            SpanPlacement::Range { start, count } => {
                assert!(start <= s as u64 && s as u64 <= start + count as u64 - 1)
            }
            SpanPlacement::Leading => panic!("Leading where Range expected"),
        }
        // nothing before s + Delta covers s
        assert!(o.first_cover.iter().all(|(t, e)| *t >= s as u64 + delta || *e <= s as u64));
    }
}

/// (eps, delta) scenarios of `temporal_oracle.rs:1476`, q = 0.5, W = 64.
const K2_SCENARIOS: [(f64, f64); 5] =
    [(0.05, 0.02), (0.05, 0.03), (0.10, 0.04), (0.10, 0.06), (0.08, 0.03)];

/// K2: block configuration against the exact C5 'after' value. Asserts the
/// upper side P <= C5 + 3 SE (the codec can only fall short of generic
/// rank); reports the per-block mismatch count (decoder outcome differs from
/// surv >= nlost).
#[test]
fn k2_block_configuration_against_c5() {
    let (blocks, seeds, w) = (1500usize, 12u64, 64usize);
    let mut mism = 0u64;
    for (i, &(eps, delta)) in K2_SCENARIOS.iter().enumerate() {
        let q = 0.5;
        let p = eps * q / (1.0 - eps);
        let sigma2 = 2.0 / (p + q) - 1.0;
        let r = r_star_closed(eps, sigma2, w as f64, delta);
        let c5 = c5_after(eps, sigma2, w, r);
        let mut ps = Vec::new();
        for s in 0..seeds {
            let o = run_blocks(eps, sigma2, w, r, blocks, 1000 * (i as u64 + 1) + s);
            ps.push(o.recovered as f64 / o.lost.max(1) as f64);
            mism += o.mismatch;
        }
        let (m, se) = mean_se(&ps);
        println!("K2 eps {eps} delta {delta} r* {r:.4}: P {m:.4} (SE {se:.4}) C5 {c5:.4} mismatch so far {mism}");
        assert!(m <= c5 + 3.0 * se, "K2 upper side at eps {eps} delta {delta}: {m} > {c5} + 3 {se}");
    }
    println!("K2 mismatch blocks: {mism}");
}

/// R1b rows of `pfec_review.out`: (A*, r, Delta, P_py, spread over its 6
/// runs). eps .048, sigma2 3.76.
const K3_ROWS: [(u64, f64, u64, f64, f64); 12] = [
    (4, 0.29, 1, 0.3438, 0.0060),
    (4, 0.29, 2, 0.3576, 0.0013),
    (4, 0.34, 1, 0.4022, 0.0187),
    (4, 0.34, 2, 0.4187, 0.0044),
    (5, 0.29, 1, 0.3895, 0.0042),
    (5, 0.29, 2, 0.3967, 0.0083),
    (5, 0.34, 1, 0.4515, 0.0053),
    (5, 0.34, 2, 0.5551, 0.0163),
    (7, 0.29, 1, 0.5230, 0.0221),
    (7, 0.29, 2, 0.5369, 0.0334),
    (7, 0.34, 1, 0.5796, 0.0217),
    (7, 0.34, 2, 0.6155, 0.0167),
];

/// K3: P(inf) at the Python bound's premises (ack_cancel off = uniform debt
/// accrual, copies off, same Delta, no deadline). Asserts only
/// P_rust(inf) <= P_py + tolerance, tolerance = 3 SE_rust + the Python
/// row's own run-to-run spread (its SE is over 3 seeds only).
#[test]
fn k3_p_inf_against_the_python_bound() {
    let (n, seeds) = (40_000usize, 8u64);
    for &(a, r, d, py, spread) in &K3_ROWS {
        let mut ps = Vec::new();
        for s in 0..seeds {
            let mut c = Cfg::astar_row(a, d, r, n, 77 + s);
            c.ack_cancel = false;
            c.resolve = Resolve::Evict { hold: 200 }; // lets the ack pass a hole; decodes are still counted at any lag
            ps.push(run(&c).p_at(None));
        }
        let (m, se) = mean_se(&ps);
        println!("K3 A* {a} r {r} Delta {d}: P_rust {m:.4} (SE {se:.4}) P_py {py:.4} (spread {spread:.4})");
        assert!(m <= py + 3.0 * se + spread, "K3 A* {a} r {r} Delta {d}: {m} > {py}");
    }
}

/// K4: reproduce the engine's W1 at a recorded row's inputs. NEEDS an engine
/// row with fixed (eps, sigma2, r = mean rr, A*, Delta, m-bar) and
/// W1 = cod/(rr src). The repo's `docs/l1-raw/store-law-n1/diag-sl/
/// c1s-400-LAW-realtime-s42-r1-a1-c.log` is the closest (rr mean 0.10 over 44
/// [SPAN] lines, but its A* runs 1..201, its c1s eps/sigma2 are unrecorded
/// there, and its m-bar gauge is 0.8), so no single-point comparison exists;
/// the evicting-seat VM row of spec §6.1 is the one that settles it. Fill
/// `K4_ROW` from it, then drop the `#[ignore]`.
#[test]
#[ignore = "needs a recorded engine W1 row: spec 6.1 evicting-seat VM row (c3, realtime, evicting, RWM_DIAG)"]
fn k4_w1_matches_engine_row() {
    struct Row {
        eps: f64,
        sigma2: f64,
        rr: f64,
        a_star: u64,
        delta: u64,
        m_bar: u64,
        w1_engine: f64,
    }
    // placeholder shape only: replace with the recorded row
    let row = Row { eps: 0.048, sigma2: 3.76, rr: 0.119, a_star: 4, delta: 1, m_bar: 1, w1_engine: 0.67 };
    let mut ws = Vec::new();
    for s in 0..8u64 {
        let mut c = Cfg::astar_row(row.a_star, row.delta, row.rr, 40_000, 500 + s);
        c.eps = row.eps;
        c.sigma2 = row.sigma2;
        c.m_bar = row.m_bar;
        ws.push(run(&c).w1(row.rr));
    }
    let (m, se) = mean_se(&ws);
    println!("K4 W1 oracle {m:.3} (SE {se:.3}) engine {:.3}", row.w1_engine);
    assert!((m - row.w1_engine).abs() <= 3.0 * se, "K4: W1 {m} vs engine {}", row.w1_engine);
}

// ---------------------------------------------------------------- timing

/// Spec 6.8: one W 201 c3 cell (r 0.158, Delta 11, primary arm, m-bar 1),
/// µs per source and per hole. Run: `cargo test [--release] -p raptorpath
/// --lib pfec_oracle::timing -- --ignored --nocapture`; env PFEC_N sets the
/// sources (default 1e5).
#[test]
#[ignore = "timing run"]
fn timing_w201_c3_cell() {
    let n: usize = std::env::var("PFEC_N").ok().and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let (c, l_arq) = W201_CELLS[1].cfg(0.158, n, 42, 1);
    let t0 = std::time::Instant::now();
    let o = run(&c);
    let el = t0.elapsed().as_secs_f64();
    println!(
        "TIMING n {n} holes {} emitted {} W1 {:.3} Delta {} d {l_arq}: {:.1} s = {:.1} us/source = {:.1} us/hole; P(d) {:.4} m_gauge {:.1}",
        o.n(),
        o.emitted,
        o.w1(c.rate),
        span_seat(c.rate_sym, c.rtprop_s, c.b, c.jitter_s, 201).1,
        el,
        el * 1e6 / n as f64,
        el * 1e6 / o.n().max(1) as f64,
        o.p_at(Some(l_arq)),
        o.m_gauge(),
    );
}

// --------------------------------------------------------------- readings
// `#[ignore]`d: they run under a registration, after K0-K4 pass.

/// R1a rows of `pfec_review.out` (A*, r, shipped, C6, C5 'after'): the
/// frozen predictions at the m-bar sweep's cells.
const FROZEN_ASTAR: [(u64, f64, f64, f64, f64); 10] = [
    (3, 0.29, 0.8208, 0.2606, 0.2023),
    (3, 0.34, 0.8650, 0.3057, 0.2376),
    (4, 0.29, 0.8555, 0.3015, 0.2466),
    (4, 0.34, 0.8986, 0.3566, 0.2932),
    (5, 0.29, 0.8821, 0.3454, 0.2938),
    (5, 0.34, 0.9228, 0.4090, 0.3491),
    (6, 0.29, 0.9030, 0.3895, 0.3405),
    (6, 0.34, 0.9406, 0.4598, 0.4031),
    (7, 0.29, 0.9196, 0.4322, 0.3856),
    (7, 0.34, 0.9540, 0.5009, 0.4489),
];

/// The m-bar sweep {1, 4, 16, 64, 256} at the A* cells (primary d = A* - 1;
/// readouts A*, A* + Delta, inf). The full 20-cell grid also has r 0.2 and
/// 0.5 and the r = 0.5 r_c row: `PFEC_ASTAR_GRID=full`.
#[test]
#[ignore = "reading: run under a registration"]
fn reading_mbar_sweep_astar() {
    let full = std::env::var("PFEC_ASTAR_GRID").is_ok_and(|v| v == "full");
    let (n, seeds) = (
        std::env::var("PFEC_N").ok().and_then(|s| s.parse().ok()).unwrap_or(40_000usize),
        5u64,
    );
    let rs: &[f64] = if full { &[0.2, 0.29, 0.34, 0.5] } else { &[0.29, 0.34] };
    println!("m_bar A* r | P(A*-1) P(A*) P(A*+D) P(inf) W1 m_gauge | shipped C6 C5");
    for &m_bar in &[1u64, 4, 16, 64, 256] {
        for a in 3..=7u64 {
            for &r in rs {
                let (mut pa, mut pb, mut pc, mut pi, mut w1, mut mg) = (vec![], vec![], vec![], vec![], vec![], vec![]);
                for s in 0..seeds {
                    let mut c = Cfg::astar_row(a, 1, r, n, 900 + s);
                    c.m_bar = m_bar;
                    let o = run(&c);
                    pa.push(o.p_at(Some(a - 1)));
                    pb.push(o.p_at(Some(a)));
                    pc.push(o.p_at(Some(a + 1)));
                    pi.push(o.p_at(None));
                    w1.push(o.w1(r));
                    mg.push(o.m_gauge());
                }
                let f = FROZEN_ASTAR.iter().find(|x| x.0 == a && x.1 == r);
                let (m1, s1) = mean_se(&pa);
                println!(
                    "{m_bar:>4} {a} {r:.2} | {m1:.3}(SE {s1:.3}) {:.3} {:.3} {:.3} {:.3} {:.1} | {}",
                    mean_se(&pb).0, mean_se(&pc).0, mean_se(&pi).0, mean_se(&w1).0, mean_se(&mg).0,
                    f.map_or("-".to_string(), |f| format!("{:.3} {:.3} {:.3}", f.2, f.3, f.4)),
                );
            }
        }
    }
}

/// W 201 body rows (c2/c3 at 0.5, 1.5, 2 r_c; report-only), d = L_arq,
/// m-bar per the reliable-seat gauge range (`PFEC_MBAR`, default 1).
#[test]
#[ignore = "reading: run under a registration"]
fn reading_w201_body_rows() {
    let n: usize = std::env::var("PFEC_N").ok().and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let m_bar: u64 = std::env::var("PFEC_MBAR").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    for cell in &W201_CELLS {
        for mult in [0.5, 1.5, 2.0] {
            let r = mult * cell.r_c();
            let mut ps = Vec::new();
            let mut holes = 0;
            for s in 0..5u64 {
                let (c, l_arq) = cell.cfg(r, n, 300 + s, m_bar);
                let o = run(&c);
                holes += o.n();
                ps.push(o.p_at(Some(l_arq)));
            }
            let (m, se) = mean_se(&ps);
            println!("W201 {} {mult} r_c (r {r:.4}): P(L_arq) {m:.4} (SE {se:.4}) holes {holes} m_bar {m_bar}", cell.name);
        }
    }
}

/// W 201 Auto r* tail rows, in miss counts (spec 6.5). 10^5 holes need
/// ~2e6 (c3) to 4e6 (c2) sources: `PFEC_N`.
#[test]
#[ignore = "reading: run under a registration"]
fn reading_w201_tail_miss_counts() {
    let m_bar: u64 = std::env::var("PFEC_MBAR").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    for (cell, t_thresh, n_def, cluster_ok) in [(&W201_CELLS[0], 310usize, 4_000_000usize, 164usize), (&W201_CELLS[1], 171, 2_000_000, 85)] {
        let n: usize = std::env::var("PFEC_N").ok().and_then(|s| s.parse().ok()).unwrap_or(n_def);
        let (c, l_arq) = cell.cfg(cell.r_auto, n, 4242, m_bar);
        let o = run(&c);
        let (k, biggest) = o.misses(Some(l_arq), 20);
        let verdict = if k < t_thresh { "refutes C6 and C5" } else { "refutes shipped and Phi" };
        println!(
            "TAIL {}: holes {} misses {k} T {t_thresh} -> {verdict}; largest cluster {biggest} (void if > {cluster_ok}{})",
            cell.name,
            o.n(),
            if biggest > cluster_ok { ": VOID" } else { "" },
        );
    }
}
