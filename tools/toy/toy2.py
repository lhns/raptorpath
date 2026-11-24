"""Toy model, stage 2, one path (docs/derivations/skip-rule-efficiency.md section 7: Parts A, T, B).

Adapted from toy.py (stage 1, untouched).  No constant here is provenance for the engine; every
free choice is a Cfg field or a named function with its source.  The dials enter as numbers:
delta only in H_o = omega*d/delta and in b(delta); rho only in allow = floor((1-rho)*S_src).  No
comparison on rho, delta or a hint name selects a branch, except in the named functions
plant_scale (STEP, SPIKE, KINK), hz_hold (HZ) and the R0e receiver (test_toy2 deny test).
"""
import bisect
import heapq
import math
from collections import deque
from dataclasses import dataclass

import numpy as np

import toy  # stage 1: u01, CELLS, ge_states, salts (read only)

u01 = toy.u01
SALT_LOSS, SALT_FLOORLOSS = toy.SALT_LOSS, toy.SALT_FLOORLOSS
SALT_PLACE = toy.SALT_PLACE
SALT_KNOT, SALT_PH = 5, 6

CELLS = dict(toy.CELLS)
CELLS["L10"] = dict(kind="iid", p0=0.10, pc=0.10)     # Part T, toy choice (7.13): high loss, RTT 100 ms
PRESETS = {"Bulk": 0.005, "Auto": 0.5, "Realtime": 50.0}
RHO = {"Bulk": 1.0, "Auto": 1.0, "Realtime": 0.99}    # as shipped (7.9)
DELTA_AUTO = 0.5


def b_of_delta(d):
    """b(delta) = clamp(2^(-1/2 log10(delta/delta_auto)), 1/2, 2), paper 5.4."""
    return max(0.5, min(2.0, 2.0 ** (-0.5 * math.log10(d / DELTA_AUTO))))


def intake_expired_fn(T, age):
    """L1 (three-knobs.md 2.1, 9.2): a unit that has outlived T is dropped at the intake.  One of the two places T is compared."""
    return age >= T


def resend_admissible(T, age, x):
    """L2 (three-knobs.md 2.2, 9.2): a resend is worth sending only if it can arrive before T.  x = one-way delay."""
    return age + x < T


def within_lifetime(T, age):
    """L3 readout (three-knobs.md 2.3, 9.2): which delivered units arrived with age <= T.  A measurement, never a branch; named
    here so that the deny test keeps T out of every other comparison.  The third place T is compared (worker decision)."""
    return age <= T


def dequeue_expired(T, age, m):
    """Stage 3b, three-knobs.md 13.2: at the dequeue a unit gives its slot to a younger one when it can no longer arrive in
    time.  m is the ack timeout R, an upper bound on the one-way delay.  The fourth place T is compared."""
    return age + m >= T


def delta_at_b(b):
    return DELTA_AUTO * 10.0 ** (-2.0 * math.log2(b))


@dataclass
class Cfg:
    cell: str = "I1"
    sender: str = "S1c"            # S0 | S1 (verdict, no copy; test only) | S1c | S2 | S2c (stage 3: L1, L2; S2c adds a copy per verdict) | S3 | S3x (stage 3b: the S2 family on Cfg's stage 3b fields; S3x keeps the O exclusion)
    recv: str = "R1"               # R1 | U | R0e | HZ | RH (stage 3: hold Hk)
    seed: int = 0
    dur: float = 20.0
    rate: float = 10000.0
    n_total: int = 0               # >0: object mode (Part T), the source stops after n_total seqs
    owd: float = 0.005
    J: float = 0.001               # delay-process amplitude, 0 = none
    q: bool = False                # light standing queue Q (7.2)
    delta: float = 50.0
    rho: float = 0.99
    omega: float = 1.0
    dsel: str = "all"              # d_all | d_rep
    d_frozen: object = None        # P4j: d held at this value from the start
    d_arm: str = "RT"              # RT (paper) | EWMA (engine) | ZERO (ablation: gate always admits)
    plant: object = None           # None | STEP | SPIKE | KINK (planted controls, 7.9)
    plant_p: float = 0.0
    hz: object = None              # HZ calibration dict
    H_force: object = None
    W: float = 10.0
    variant: int = 0               # clock-phase draw (A/A arm A' uses 1)
    phases: object = None          # (sampler phase, sweep phase) override; (0, 0) = stage 1's
    tick: float = 0.002
    sweep_lo: float = 0.025
    sweep_hi: float = 0.100
    pause_B: int = 2048
    grace: float = 1.0
    dead: object = None            # (first dead seq, window start, window length), Part T
    t_limit: float = 40.0
    want_trace: bool = False
    want_p4: bool = False
    check_p1: bool = True
    r0e_hold: str = "global"       # global: one hold re-read at drain time (reorder.rs:18,126,174); entry: fixed per entry at buffering (history)
    T: float = math.inf            # stage 3: lifetime knob, seconds from hand-over (three-knobs.md 9.2)
    Hk: float = math.inf           # stage 3: order hold knob, seconds from hole detection
    o_excl_verdict: bool = True    # stage 3: L7, verdicted seqs leave the outstanding count (S2p sets False)
    rtt_est: str = "old"           # stage 3b: "rfc" = RFC 6298 srtt/rttvar, Karn's rule, sampled from the largest newly received seq; defines R_now()
    src_model: str = "old"         # stage 3b: "wc" = work-conserving source (a waiting backlog is sent at once, limited by the pause and the link)
    link: object = None            # stage 3b: dict(C=units/s, Q=queue in units) turns on the bottleneck
    tail_elig: str = "P"           # stage 3b: "R" = the probe timer (one datagram per R for the oldest open-tail seq)
    resend_gate: str = "owd"       # stage 3b: "evidence" gates every resend on evidence of loss (link cells)
    deq_expiry: bool = False       # stage 3b: dequeue expiry (a + R >= T with a younger unit waiting)
    evidence_verdict: bool = False  # stage 3b: verdicts only on evidence of loss newer than the last send
    recv_eps: str = "engine"       # R0e's eps_hat: engine (per-batch EWMA, alpha 0.1, estimator.rs:92,103-110) | cum (cumulative missing fraction) | ewma (per-seq, alpha 1/256)


def ge_cell(cell, seed, slots, dur_slots_unused=None):
    c = CELLS[cell]
    return [toy.ge_states(seed * 4 + 0, slots, c["mean"] / c["pbad"], c["burst"])]


class Delay:
    """arrival = tau + at(tau); at(t) = owd + J(t) + Q(t).  J piecewise linear through U[0,J] knots 2J apart
    (slope in [-1/2, 1/2], so the path is FIFO for every datagram); Q is the sawtooth of 7.2."""

    def __init__(self, seed, owd, J, q, horizon):
        self.owd, self.J, self.q = owd, J, q
        if J > 0:
            self.kappa = 2.0 * J
            n = int(horizon / self.kappa) + 4
            self.K = [J * u01(seed, k, 0, 0, SALT_KNOT) for k in range(n)]

    def at(self, t):
        d = self.owd
        if self.J > 0:
            k = int(t / self.kappa)
            K = self.K
            if k + 1 >= len(K):
                k = len(K) - 2
            f = (t - k * self.kappa) / self.kappa
            d += K[k] + (K[k + 1] - K[k]) * f
        if self.q:
            ph = t - math.floor(t)
            d += 0.010 * ph / 0.98 if ph < 0.98 else 0.010 * (1.0 - (ph - 0.98) / 0.02)
        return d


class HEst:
    """floor-hold.md section 2: H(t) = max{ l_i : t-W <= a_i <= t }, l_i = (a_i - min{a_j : j arrived before i, j sent after i})+.
    strict=True: 'sent after' is tau_j > tau_i (one path); False: tau_j >= tau_i (across paths)."""

    def __init__(self, W=10.0, strict=True):
        self.W, self.strict = W, strict
        self.rt, self.ra, self.lo = [], [], 0      # records: arrivals whose tau exceeds every earlier tau
        self.dq = deque()                          # monotone deque of (a, l) for the windowed max
        self.max_l = 0.0

    def feed(self, a, tau):
        rt, ra = self.rt, self.ra
        ell = 0.0
        if not rt or tau > rt[-1]:
            rt.append(tau)
            ra.append(a)
        else:
            f = bisect.bisect_right if self.strict else bisect.bisect_left
            i = f(rt, tau, self.lo)
            if i < len(rt):
                ell = max(0.0, a - ra[i])
        lim = a - self.W
        while self.lo < len(ra) and ra[self.lo] < lim:
            self.lo += 1
        if self.lo > 4096:
            del rt[:self.lo], ra[:self.lo]
            self.lo = 0
        dq = self.dq
        while dq and dq[-1][1] <= ell:
            dq.pop()
        dq.append((a, ell))
        if ell > self.max_l:
            self.max_l = ell
        return ell

    def value(self, t):
        dq = self.dq
        lim = t - self.W
        while dq and dq[0][0] < lim:
            dq.popleft()
        return dq[0][1] if dq else 0.0


def plant_scale(cfg, ho):
    """Planted controls (7.9): deliberate defects, named in the deny test.  U at its preset p."""
    p = cfg.plant_p
    if cfg.plant == "STEP":
        return ho * 2.0 if cfg.delta < p else ho
    if cfg.plant == "SPIKE":
        return ho * 2.0 if cfg.delta == p else ho
    if cfg.plant == "KINK":
        return ho * (1.0 + 25.0 * max(0.0, 1.0 - cfg.delta / p))
    return ho


def hz_hold(cfg):
    """HZ, ordering-hold 3.1's global-argmin rule, the P3 positive control: release at detect if delta > delta*,
    else hold to the end of the first-repair fill mass.  Returns the hold in seconds."""
    cal = cfg.hz
    dstar = cal["omega"] * cal["d"] * (1.0 - cal["eps"]) / cal["tau1"]
    return 0.0 if cfg.delta > dstar else cal["q99"]


class PathEps:
    """The engine-like incoming-loss EWMA (estimator.rs:41-50, 92, 103-110; net/mod.rs PathBatchTracker::record_batch): one
    batch is one datagram keyed on a per-path path_seq.  expected = gap to the highest path_seq seen, received = 1; the sample
    (e - r)/e updates an EWMA with alpha 0.1.  A late arrival (path_seq below the highest) has expected 0: it skips the update
    and its symbol becomes a loss credit that pays down a later loss."""

    def __init__(self, alpha=0.1):
        self.alpha, self.hi, self.credit, self.ewma = alpha, -1, 0, 0.0

    def record(self, ps):
        if ps > self.hi:
            e = ps - self.hi
            loss = e - 1
            pay = min(self.credit, loss)
            self.credit -= pay
            loss -= pay
            self.ewma = self.alpha * (loss / e) + (1.0 - self.alpha) * self.ewma
            self.hi = ps
        else:
            self.credit += 1


class Sim:
    def __init__(self, cfg: Cfg):
        self.cfg = cfg
        c = cfg
        self.cell = CELLS[c.cell]
        self.rtt = 2 * c.owd                           # RTprop, exact
        self.b = b_of_delta(c.delta)
        self.D_rt = min(self.b * self.rtt, 2 * self.rtt)
        self.dt = 1.0 / c.rate
        if c.n_total:
            self.dur = c.n_total / c.rate
            self.t_end = c.t_limit
        else:
            self.dur = c.dur
            self.t_end = c.dur + c.grace
        self.units_total = c.n_total if c.n_total else int(c.dur * c.rate)
        self.nmax = self.units_total + 16
        self.B = c.pause_B
        self.sweep_P = min(max(2 * self.rtt, c.sweep_lo), c.sweep_hi)
        if c.phases is None:
            self.ph_s = c.tick * u01(c.seed, c.variant, 0, 0, SALT_PH)
            self.ph_w = self.sweep_P * u01(c.seed, c.variant, 1, 0, SALT_PH)
        else:
            self.ph_s, self.ph_w = c.phases
        k = self.cell["kind"]
        self.loss_kind = k
        self.p0 = self.cell.get("p0", 0.0)
        self.pc = self.cell.get("pc", 0.0)
        if k == "ge":
            slots = int((self.t_end + 1) * c.rate) + 8
            self.ge = toy.ge_states(c.seed * 4, slots, self.cell["mean"] / self.cell["pbad"], self.cell["burst"])
            self.pbad = self.cell["pbad"]
        self.delay = Delay(c.seed, c.owd, c.J, c.q, self.t_end + 2.0)
        self.est = HEst(c.W, True)
        # sender state
        self.n_sent = 0
        self.sent_t = [0.0] * self.nmax
        self.atilde = [0.0] * self.nmax              # loss-free arrival of the original (7.4)
        self.copies = bytearray(self.nmax)
        self.last_send = [0.0] * self.nmax
        self.A_known = 0
        self.G_known = 0
        self.GF_known = 0
        self.rhi_known = -1
        self.M_known = set()
        self.last_gen = -1.0
        self.verdicted = set()
        self.pending = set()
        self.last_floor = -1
        self.last_floor_t = -1e9
        self.floor_rep = {}
        self.src_next = 0.0
        self.src_done = False
        self.u_next = 0                              # next application unit (handed over at u * dt)
        self.h_seq = [0.0] * self.nmax               # hand-over time of the unit each seq carries
        self.intake_expired = 0
        self.s2fam = c.sender in ("S2", "S2c", "S3", "S3x")
        self.s3 = c.sender in ("S3", "S3x")
        self.resume_on_verdict = c.sender != "S3"       # S3: the pause is released by receiver feedback only (13.1)
        self.O_ctl = self.O_eff if self.s2fam else self.O
        self.paused = False
        self.srtt = None
        self.rttvar = 0.0
        self.tau_rx = -1.0                           # sender: newest send stamp the receiver reported having received (13.1)
        self.tau_rx_recv = -1.0                      # receiver: maximum send stamp over data arrivals
        self.link_free = 0.0
        self.link_busy = 0.0
        self.busy_w = 0.0
        self.link_sv = (1.0 / c.link["C"]) if c.link is not None else 0.0
        self.link_qs = (c.link["Q"] / c.link["C"]) if c.link is not None else 0.0
        self.intake_expired_deq = 0
        self.pause_t0 = None
        self.pause_log = []
        self.R_sum = 0.0
        self.srtt_sum = 0.0
        self.rep_n = 0
        self.floor_arrivals = []
        self.deq_log = []
        self.trig = "report"
        self.oe = (c.dead[1] + c.dead[2]) if c.dead is not None else None
        self.first_emit_oe = None
        # receiver state
        self.got = bytearray(self.nmax)
        self.arr = [0.0] * self.nmax
        self.dlv = [-1.0] * self.nmax
        self.was_hole = bytearray(self.nmax)
        self.holes = {}                              # open at the ack level: seq -> detect time
        self.hheap = []
        self.unrel = set()                           # open and not released to the consumer
        self.uheap = []
        self.released = set()
        self.passed = set()
        self.G = 0
        self.GF = 0                                  # floor passes net of their late fills (R0c's count)
        self.passed_by = {}
        self.rhi = -1
        self.next_deliver = 0
        self.F_rx = 0
        self.dirty = False
        self.passed_since_report = []
        self.tdet, self.tfill, self.tpass, self.trel, self.fillc = {}, {}, {}, {}, {}
        self.hoq = {}
        self.fl_created = set()
        self.ep_start = None
        self.ep_pass = 0
        self.episodes = []
        self.hol = []
        self.d_sum = [0.0, 0.0]                      # all, rep
        self.d_cnt = [0, 0]
        self.n_missing = 0
        self.r0e_given_up = 0
        self.t_front = None
        self.stop_at = None
        self.g = dict(verdicts=0, verdict_copies=0, floors=0, floor_rep=0, repair_sends=0, serves=0, refusals=0,
                      age_gate_fail=0, n_dec=0, gate_cold=0, passes=0, pass_floor=0, pass_r0e=0, late=0, ooo_loss=0,
                      dup=0, p1_viol=0, p1_worst=0, p1_checks=0, pause_events=0, max_O=0, rel_ho=0,
                      r0e_budget_open=0, r0e_fallback=0, r0e_expiries=0, fh_late=0, fifo_viol=0, seed_verdicts=0, n_dec_seed=0, r0e_fallback_open=0, pass_r0e_fb=0, r0e_entries_lengthened=0, pass_r0e_lengthened=0, late_floor=0, late_r0e=0,
                      l2_fail=0, rel_by_floor=0, probes=0, queue_drop=0, queue_drop_in=0, queue_drop_out=0,
                      resend_by_R=0, resend_by_report=0, resend_by_sweep=0)
        self.trace = [] if c.want_trace else None
        self.orig_lost = []
        self.heap = []
        self.ctr = 0
        self.t_last = 0.0
        self.last_arr = -1.0
        self.pass_dt = []
        self.eps_ew = 0.0
        self.eps_trk = PathEps()
        self.r0e_buf = []
        self.r0e_timer_t = math.inf
        self.ps_ctr = 0

    # ---- helpers --------------------------------------------------------------------------
    def push(self, t, kind, payload):
        self.ctr += 1
        heapq.heappush(self.heap, (t, self.ctr, kind, payload))

    def lost(self, seq, copy, t, salt=SALT_LOSS):
        dd = self.cfg.dead
        if dd is not None and seq >= dd[0] and dd[1] <= t < dd[1] + dd[2]:
            return True
        u = u01(self.cfg.seed, seq, copy, 0, salt)
        if self.loss_kind == "iid":
            return u < (self.p0 if copy == 0 and salt == SALT_LOSS else self.pc)
        if self.loss_kind == "ge":
            i = int(t * self.cfg.rate)
            if i >= len(self.ge):
                i = len(self.ge) - 1
            return bool(self.ge[i]) and u < self.pbad
        return False

    def true_allow(self):
        return int((1 - self.cfg.rho) * self.n_sent + 1e-9)

    def allow(self):
        return self.true_allow()

    def O(self):
        return len(self.M_known) + max(0, self.n_sent - 1 - self.rhi_known)

    def O_eff(self):
        """L7 (three-knobs.md 2.8): verdicted seqs not yet passed by the ack leave the outstanding count (0 at T = inf)."""
        return self.O() - (len(self.pending) if self.cfg.o_excl_verdict else 0)

    def H_now(self, now):
        if self.cfg.H_force is not None:
            return float(self.cfg.H_force)
        return self.est.value(now)

    def D_now(self):
        """(D, enabled).  EWMA before its first sample uses main's SRTT seed of 50 ms (7.2, a known main defect,
        modelled on purpose).  ZERO is the ablation: D = 0, the gate always admits."""
        arm = self.cfg.d_arm
        if arm == "RT":
            return self.D_rt, True
        if arm == "ZERO":
            return 0.0, True
        seed = 0.050 if self.srtt is None else self.srtt        # main's 50 ms seed until the first sample replaces it
        return min(self.b, 2.0) * seed, True

    def d_now(self):
        if self.cfg.d_frozen is not None:
            return self.cfg.d_frozen
        i = 0 if self.cfg.dsel == "all" else 1
        return self.d_sum[i] / self.d_cnt[i] if self.d_cnt[i] else None

    def ho_now(self):
        """The release hold of a hole detected now (None = absent = infinity)."""
        r = self.cfg.recv
        if r == "U":
            d = self.d_now()
            if d is None:
                return None
            return plant_scale(self.cfg, self.cfg.omega * d / self.cfg.delta)
        if r == "HZ":
            return hz_hold(self.cfg)
        if r == "RH":
            return self.cfg.Hk                       # L6: release at t_detect + H; the floor pass is unchanged
        return None

    def rtt_sample(self, r):
        """RFC 6298 2.2 and 2.3 (stage 3b): first sample srtt = r, rttvar = r/2; then rttvar first, srtt second."""
        if self.srtt is None:
            self.srtt, self.rttvar = r, r / 2.0
        else:
            self.rttvar = 0.75 * self.rttvar + 0.25 * abs(self.srtt - r)
            self.srtt = 0.875 * self.srtt + 0.125 * r

    def R_now(self):
        """The ack timeout R = srtt + max(4 rttvar, g) + ack_delay_max (three-knobs.md 13, RFC 6298 2, RFC 9002 6.2.1), g and
        ack_delay_max = the toy's tick; before the first sample the RFC 9002 6.2.2 seed (333 ms, rttvar 166.5 ms)."""
        sr, rv = (0.333, 0.1665) if self.srtt is None else (self.srtt, self.rttvar)
        return sr + max(4.0 * rv, self.cfg.tick) + self.cfg.tick

    def o_hat(self):
        """One-way delay estimate used by L2: the exact owd for S2 (stage 3); srtt/2 with the RFC seed for S3."""
        if self.s3 and self.cfg.rtt_est == "rfc":
            return (0.333 if self.srtt is None else self.srtt) / 2.0
        return self.rtt / 2

    def evidence(self, s, now):
        """13.1: evidence of loss newer than the last send of s: a datagram sent later has arrived, or s is overdue (R)."""
        ls = self.last_send[s]
        return ls < self.tau_rx or now >= ls + self.R_now()

    def fwd(self, t):
        """A forward send at t.  Returns (arrival, departure); arrival is None when the bottleneck queue drops it (13.7.1 item 6).
        With no link: arrival = t + delay.at(t), departure = t."""
        lk = self.cfg.link
        if lk is None:
            return t + self.delay.at(t), t
        if max(0.0, self.link_free - t) >= self.link_qs:
            self.g["queue_drop"] += 1
            dd = self.cfg.dead
            if dd is not None and dd[1] <= t < dd[1] + dd[2]:
                self.g["queue_drop_in"] += 1
            else:
                self.g["queue_drop_out"] += 1
            return None, t
        d = max(t, self.link_free) + self.link_sv
        self.link_busy += self.link_sv
        self.busy_w += max(0.0, min(d, 10.0) - max(d - self.link_sv, 1.0))      # link-busy time inside [1 s, 10 s]
        self.link_free = d
        return d + self.delay.at(d), d

    def fifo_note(self, a):
        if a < self.last_arr - 1e-12:
            self.g["fifo_viol"] += 1
        else:
            self.last_arr = a

    # ---- source ---------------------------------------------------------------------------
    def emit(self, t):
        c = self.cfg
        # L1 at the intake: units that have outlived T before they could be sent are dropped, never get a seq
        while self.u_next < self.units_total and intake_expired_fn(c.T, t - self.u_next * self.dt):
            self.intake_expired += 1
            self.u_next += 1
        if c.deq_expiry:                                         # 13.2: expire the head at the dequeue when a younger unit waits (after L1)
            while (self.u_next + 1 < self.units_total
                   and (self.u_next + 1) * self.dt <= t
                   and dequeue_expired(c.T, t - self.u_next * self.dt, self.R_now())):
                self.intake_expired_deq += 1
                self.u_next += 1
        if c.n_total and self.u_next >= self.units_total:        # streaming mode ends on nxt >= dur (float edge: stage 1 sends one more)
            self.src_done = True
            self.src_next = math.inf
            self.advance(t)
            return
        s = self.n_sent
        if self.trace is not None:
            self.deq_log.append((t, self.u_next, t - self.u_next * self.dt, self.R_now(),
                                 self.u_next + 1 < self.units_total and (self.u_next + 1) * self.dt <= t))
        if self.oe is not None and self.first_emit_oe is None and t >= self.oe:
            self.first_emit_oe = t
        self.h_seq[s] = self.u_next * self.dt
        self.u_next += 1
        self.n_sent += 1
        self.sent_t[s] = t
        self.last_send[s] = t
        a, dep = self.fwd(t)
        if a is None:                                # dropped at the bottleneck queue: no arrival, no loss draw
            self.atilde[s] = t + self.delay.at(t)
        else:
            self.atilde[s] = a
            self.fifo_note(a)
        if self.trace is not None:
            self.trace.append((round(t, 9), s, 0, 0, 0))
        ps = self.ps_ctr
        self.ps_ctr += 1
        if a is None or self.lost(s, 0, dep):
            self.orig_lost.append(s)
        else:
            self.push(a, 0, (s, 0, t, ps))
        if self.s2fam:                               # the per-seq verdict timer (at +inf it never fires)
            self.push(max(t, self.h_seq[s] + c.T - self.o_hat()), 8, s)
        if s == 0 and c.tail_elig == "R":            # the probe timer (13.4): exactly one pending event of kind 9
            self.push(t + self.R_now(), 9, None)
        nxt = t + self.dt
        o = self.O()
        if o > self.g["max_O"]:
            self.g["max_O"] = o
        o = self.O_ctl()
        if c.n_total:
            done = self.u_next >= self.units_total
        else:
            done = nxt >= c.dur
        if done:
            self.src_done = True
            self.src_next = math.inf
        elif o >= self.B:
            self.paused = True
            self.pause_t0 = t
            self.g["pause_events"] += 1
            self.src_next = math.inf
        elif c.src_model == "wc":                    # work-conserving: a waiting backlog is sent at once, else the next unit at its own time
            self.src_next = max(t, self.u_next * self.dt)
        else:
            self.src_next = nxt

    def send_copy(self, now, s):
        k = self.copies[s] + 1
        self.copies[s] = k
        self.last_send[s] = now
        self.g["repair_sends"] += 1
        if self.trace is not None:
            self.trace.append((round(now, 9), s, k, 0, 1))
        a, dep = self.fwd(now)
        if a is not None:
            self.fifo_note(a)
        ps = self.ps_ctr
        self.ps_ctr += 1
        if a is not None and not self.lost(s, k, dep):
            self.push(a, 0, (s, k, now, ps))

    def send_floor(self, now, f):
        rep = self.floor_rep.get(f, 0)
        self.floor_rep[f] = rep + 1
        self.g["floors"] += 1
        if rep:
            self.g["floor_rep"] += 1
        if self.trace is not None:
            self.trace.append((round(now, 9), f, -1, 0, 2))
        self.last_floor_t = now
        a, dep = self.fwd(now)
        if a is not None:
            self.fifo_note(a)
        ps = self.ps_ctr
        self.ps_ctr += 1
        if a is not None and not self.lost(f, rep, dep, SALT_FLOORLOSS):
            self.push(a, 1, (f, ps))

    # ---- receiver -------------------------------------------------------------------------
    def min_hole(self):
        h, hs = self.hheap, self.holes
        while h and h[0] not in hs:
            heapq.heappop(h)
        return h[0] if h else None

    def min_unrel(self):
        h, hs = self.uheap, self.unrel
        while h and h[0] not in hs:
            heapq.heappop(h)
        return h[0] if h else None

    def advance(self, now):
        m = self.min_unrel()
        lim = m if m is not None else self.rhi + 1
        k = self.next_deliver
        got, arr, dlv, hol = self.got, self.arr, self.dlv, self.hol
        while k < lim:
            if got[k]:
                dlv[k] = now
                hol.append(now - arr[k])
            k += 1
        self.next_deliver = k
        if m is not None:
            if self.ep_start is None:
                self.ep_start = now
                self.ep_pass = 0
        elif self.ep_start is not None:
            self.episodes.append((self.ep_start, now - self.ep_start, self.ep_pass))
            self.ep_start = None
        if self.t_front is None and self.src_done and not self.holes and self.rhi + 1 >= self.n_sent and self.cfg.n_total:
            self.t_front = now
            self.stop_at = min(self.t_end, now + self.cfg.grace)

    def new_hole(self, q, now, from_floor=False):
        self.holes[q] = now
        heapq.heappush(self.hheap, q)
        self.unrel.add(q)
        heapq.heappush(self.uheap, q)
        self.was_hole[q] = 1
        self.tdet[q] = now
        if from_floor:
            self.fl_created.add(q)
            return
        self.n_missing += 1
        ho = self.ho_now()
        if self.cfg.want_p4:
            self.hoq[q] = ho
        if ho is not None:
            self.push(now + ho, 6, q)

    def on_data(self, now, s, c, tau, ps=None):
        got = self.got
        if tau > self.tau_rx_recv:
            self.tau_rx_recv = tau
        if ps is not None:
            self.eps_trk.record(ps)
        if got[s]:
            self.g["dup"] += 1
        else:
            got[s] = 1
            self.arr[s] = now
            ell = self.est.feed(now, tau)
            if ell > 0:
                self.g["fh_late"] += 1
            if s in self.holes:
                t0 = self.holes.pop(s)
                self.tfill[s] = now
                self.fillc[s] = c
                self.d_sum[0] += now - t0
                self.d_cnt[0] += 1
                if c > 0:
                    self.d_sum[1] += now - t0
                    self.d_cnt[1] += 1
                if s in self.unrel:
                    self.unrel.discard(s)
                else:
                    self.released.discard(s)         # released earlier: out-of-order delivery now
                    self.dlv[s] = now
                    self.g["ooo_loss"] += 1
            elif s > self.rhi:
                for q in range(self.rhi + 1, s):
                    self.new_hole(q, now)
                    self.eps_ew += (1.0 - self.eps_ew) / 256.0
                self.eps_ew -= self.eps_ew / 256.0
                self.rhi = s
            elif s in self.passed:
                self.passed.discard(s)
                self.G -= 1
                who_ = self.passed_by.pop(s, "")
                self.g["late_" + who_] += 1                      # passed, then filled: out of order in the engine (reorder.rs:75-82)
                if who_ == "floor":
                    self.GF -= 1
                self.g["late"] += 1
                self.g["ooo_loss"] += 1
                self.tfill[s] = now
                self.dlv[s] = now
            self.dirty = True
            if self.cfg.recv in ("R0e", "R0c"):
                if self.cfg.r0e_hold == "entry":
                    self.r0e_buffer(now, s)
                else:
                    self.r0e_arrival(now, s)
        self.advance(now)

    def on_floor(self, now, f, ps=None):
        self.floor_arrivals.append(now)
        if ps is not None:
            self.eps_trk.record(ps)
        h = self.H_now(now)
        if h <= 0:
            self.apply_floor(now, f, now)
        else:
            self.push(now + h, 2, (f, now))

    def apply_floor(self, now, f, t_arr):
        if f <= self.F_rx:
            return
        if self.cfg.want_p4:
            self.pass_dt.append(now - t_arr)
        self.F_rx = f
        if f - 1 > self.rhi:
            for q in range(self.rhi + 1, f):
                self.new_hole(q, now, True)
            self.rhi = f - 1
        self.pass_below(now, f, "floor")

    def pass_below(self, now, f, who):
        n = 0
        while True:
            m = self.min_hole()
            if m is None or m >= f:
                break
            heapq.heappop(self.hheap)
            del self.holes[m]
            if who == "floor" and m in self.unrel:
                self.g["rel_by_floor"] += 1          # the floor pass, not the hold, releases this hole
            self.unrel.discard(m)
            self.released.discard(m)
            self.passed.add(m)
            self.passed_by[m] = who
            self.tpass[m] = now
            self.passed_since_report.append(m)
            n += 1
        if n:
            self.G += n
            if who == "floor":
                self.GF += n
            self.g["passes"] += n
            self.g["pass_" + who] += n
            self.ep_pass += n
            self.dirty = True
            if self.cfg.check_p1:
                allow = self.true_allow()
                self.g["p1_checks"] += 1
                if self.G > allow:
                    self.g["p1_viol"] += 1
                    self.g["p1_worst"] = max(self.g["p1_worst"], self.G - allow)
        self.advance(now)

    def on_release(self, now, s):
        if s in self.unrel:
            self.unrel.discard(s)
            self.released.add(s)
            self.trel[s] = now
            self.g["rel_ho"] += 1
            self.advance(now)

    # R0e: today's early-skip receiver (shed.rs shed_recv_hold, shed_recv_budget_ok; 7.3).  Named exception.
    def r0e_buffer(self, now, s):
        m = self.min_hole()
        if m is None or s <= m:
            return
        srtt = self.srtt if self.srtt is not None else 0.05
        frontier = self.rhi + 1
        if self.cfg.recv_eps == "engine":
            eps_hat = self.eps_trk.ewma
        elif self.cfg.recv_eps == "zero":                     # planted control: the budget is always spent
            eps_hat = 0.0
        elif self.cfg.recv_eps == "one":                      # planted control: the budget is never spent
            eps_hat = 1.0
        elif self.cfg.recv_eps == "ewma":
            eps_hat = self.eps_ew
        else:
            eps_hat = self.n_missing / max(frontier, 1)
        if self.r0e_given_up < eps_hat * frontier:
            hold = srtt / 2
            fb = 0
            self.g["r0e_budget_open"] += 1
        else:
            hold = min(0.300, max(0.060, 4 * srtt))
            fb = 1
            self.g["r0e_fallback"] += 1
            self.g["r0e_fallback_open"] += 1               # buffered above an unpassed hole by construction
        self.push(now + hold, 7, (s, fb))

    def r0e_eps(self):
        if self.cfg.recv_eps == "engine":
            return self.eps_trk.ewma
        if self.cfg.recv_eps == "zero":
            return 0.0
        if self.cfg.recv_eps == "one":
            return 1.0
        if self.cfg.recv_eps == "ewma":
            return self.eps_ew
        return self.n_missing / max(self.rhi + 1, 1)

    def r0e_hold_now(self):
        """shed_recv_hold(srtt, on, budget_open) with the budget re-evaluated now (shed.rs:389): one global hold."""
        srtt = self.srtt if self.srtt is not None else 0.05
        if self.r0e_given_up < self.r0e_eps() * self.next_deliver:
            self.g["r0e_budget_open"] += 1
            return srtt / 2, False
        self.g["r0e_fallback"] += 1
        return min(0.300, max(0.060, 4 * srtt)), True

    def r0e_arrival(self, now, s):
        m = self.min_hole()
        if m is not None and s > m:
            self.r0e_buf.append([now, s, False])
        self.r0e_drain(now)

    def r0e_drain(self, now):
        """reorder.rs drain_expired: an entry expires when now - buffered_at >= the current hold; the expiry passes every hole
        below the largest expired entry.  The hold is re-read at each evaluation, so a closing budget lengthens every buffered entry."""
        buf = self.r0e_buf
        while True:
            m = self.min_hole()
            if m is None:
                buf.clear()
                break
            if buf:
                buf[:] = [e for e in buf if e[1] > m]            # entries below the first hole are deliverable and leave
            if not buf:
                break
            hold, fb = self.r0e_hold_now()
            exp = [e for e in buf if now - e[0] >= hold - 1e-12]
            if fb:
                short = (self.srtt if self.srtt is not None else 0.05) / 2
                for e in buf:
                    if not e[2] and now - e[0] >= short - 1e-12 and e not in exp:
                        e[2] = True
                        self.g["r0e_entries_lengthened"] += 1
            if not exp:
                dl = min(e[0] for e in buf) + hold
                if dl < self.r0e_timer_t or self.r0e_timer_t <= now:
                    self.r0e_timer_t = dl
                    self.push(dl, 7, None)
                break
            self.g["r0e_expiries"] += 1
            j = max(e[1] for e in exp)
            before = self.g["passes"]
            self.pass_below(now, j, "r0e")
            n = self.g["passes"] - before
            self.r0e_given_up += n
            if fb:
                self.g["pass_r0e_fb"] += n
            if any(e[2] for e in exp):
                self.g["pass_r0e_lengthened"] += n

    def on_r0e_expiry(self, now, pl):
        if pl is None:                                           # global-hold timer
            self.r0e_timer_t = math.inf
            self.r0e_drain(now)
            return
        j, fb = pl
        self.g["r0e_expiries"] += 1
        m = self.min_hole()
        if m is not None and m < j:
            before = self.g["passes"]
            self.pass_below(now, j, "r0e")
            self.r0e_given_up += self.g["passes"] - before
            if fb:
                self.g["pass_r0e_fb"] += self.g["passes"] - before

    def on_tick(self, now):
        if self.dirty:
            self.dirty = False
            m = self.min_hole()
            A = m if m is not None else self.rhi + 1
            rep = (now, A, self.G, self.rhi, tuple(sorted(self.holes)), tuple(self.passed_since_report), self.GF, self.tau_rx_recv)
            self.passed_since_report = []
            self.push(now + self.cfg.owd, 3, rep)
        if now + self.cfg.tick <= self.t_end:
            self.push(now + self.cfg.tick, 4, None)

    # ---- sender ---------------------------------------------------------------------------
    def on_report(self, now, rep):
        gen, A, G, rhi, M, passed, GF, tau = rep
        if tau > self.tau_rx:
            self.tau_rx = tau
        if (A, G) > (self.A_known, self.G_known):
            if A > self.A_known and self.cfg.rtt_est == "old":   # RTT sample (7.2): the report's arrival minus the send time of
                smp = now - self.sent_t[A - 1]         # the highest seq it newly acknowledges
                self.srtt = smp if self.srtt is None else self.srtt + (smp - self.srtt) / 8.0
            self.A_known, self.G_known, self.GF_known = A, G, GF
        if gen > self.last_gen:
            self.last_gen = gen
            if rhi > self.rhi_known:
                if self.cfg.rtt_est == "rfc" and self.copies[rhi] == 0:      # 13.7.1 item 1: largest newly received seq, Karn's rule
                    self.rtt_sample(now - self.sent_t[rhi])
                self.rhi_known = rhi
            self.M_known = set(M)
        Ak, Mk, rk = self.A_known, self.M_known, self.rhi_known
        self.pending = {s for s in self.pending if s >= Ak and (s in Mk or s > rk)}
        self.R_sum += self.R_now()
        self.srtt_sum += 0.333 if self.srtt is None else self.srtt
        self.rep_n += 1
        self.maybe_resume(now)
        self.trig = "report"
        self.act(now, gen, sorted(Mk), open_tail=False)

    def maybe_resume(self, now):
        if self.paused and self.O_ctl() < self.B:
            self.paused = False
            self.pause_log.append((self.pause_t0, now))
            if self.cfg.src_model == "wc":
                self.src_next = max(now, self.u_next * self.dt)
            else:
                self.src_next = max(now, self.src_next if self.src_next != math.inf else now)

    def on_verdict_timer(self, now, s):
        """S2 / S2c: the per-seq verdict at t_h + T - owd (three-knobs.md 2.2).  No floor of its own: the next sweep or
        report-driven act sends it."""
        if s >= self.A_known and (s in self.M_known or s > self.rhi_known) and s not in self.verdicted:
            c = self.cfg
            if c.evidence_verdict:                                   # 13.7.1 item 2: judge only what is known lost
                x = self.o_hat()
                h = self.h_seq[s]
                if resend_admissible(c.T, now - h, x):
                    self.push(max(h + c.T - x, now + 1e-9), 8, s)
                    return
                if not self.evidence(s, now):
                    self.push(max(self.last_send[s] + self.R_now(), now + 1e-9), 8, s)
                    return
            self.verdicted.add(s)
            self.pending.add(s)
            self.g["verdicts"] += 1
            self.g["l2_fail"] += 1
            if self.cfg.sender == "S2c":
                self.send_copy(now, s)
                self.g["verdict_copies"] += 1
            if self.resume_on_verdict:
                self.maybe_resume(now)

    def on_probe(self, now):
        """13.4: the R timer.  One datagram per timeout for the oldest open-tail seq, then re-armed."""
        lo = max(self.rhi_known + 1, self.A_known)
        ver = self.verdicted
        while lo < self.n_sent and lo in ver:
            lo += 1
        if lo >= self.n_sent:
            self.push(now + self.R_now(), 9, None)
        elif now >= self.last_send[lo] + self.R_now():
            self.trig = "R"
            self.act(now, now, [lo], open_tail=True)
            self.g["probes"] += 1
            self.push(now + self.R_now(), 9, None)
        else:
            self.push(max(self.last_send[lo] + self.R_now(), now + 1e-9), 9, None)

    def can_resend(self, s, gen, now=None):
        if self.cfg.resend_gate == "evidence":                       # 13.1: the same predicate gates resends
            return self.copies[s] == 0 or self.evidence(s, now)
        if self.copies[s] == 0:
            return True
        return gen >= self.last_send[s] + self.cfg.owd + self.cfg.J + 1e-9

    def floor_value(self):
        Mk, ver = self.M_known, self.verdicted
        f1 = None
        for s in sorted(Mk):
            if s not in ver:
                f1 = s
                break
        f2 = self.rhi_known + 1
        while f2 in ver:
            f2 += 1
        return f2 if f1 is None else min(f1, f2)

    def act(self, now, gen, holes, open_tail):
        c = self.cfg
        ver = self.verdicted
        cands_ = [s for s in holes if s not in ver]
        if not cands_:
            return
        g = self.g
        verdict_seqs, serve_seqs = [], []
        if self.s2fam:
            x = self.o_hat()                                          # S2: the exact owd, as the D_RTprop arm idealises; S3: srtt/2
            for s in cands_:
                if resend_admissible(c.T, now - self.h_seq[s], x):
                    serve_seqs.append(s)
                elif not c.evidence_verdict or self.evidence(s, now):
                    verdict_seqs.append(s)
            g["l2_fail"] += len(verdict_seqs)
        elif c.sender != "S0":
            held = self.GF_known if c.recv == "R0c" else self.G_known     # R0c: only the floor pass's passes admit nothing else
            A_avail = self.allow() - (held + len(self.pending))
            D, en = self.D_now()
            for s in cands_:
                age = now - self.sent_t[s]
                if en and age > D:
                    if A_avail >= 1:
                        verdict_seqs.append(s)
                        A_avail -= 1
                    else:
                        g["refusals"] += 1
                        serve_seqs.append(s)
                else:
                    if en:
                        g["age_gate_fail"] += 1
                        if A_avail >= 1:
                            g["n_dec"] += 1
                            if self.srtt is None:
                                g["n_dec_seed"] += 1
                    else:
                        g["gate_cold"] += 1
                    serve_seqs.append(s)
        else:
            serve_seqs = cands_
        for s in verdict_seqs:
            ver.add(s)
            self.pending.add(s)
        g["verdicts"] += len(verdict_seqs)
        if self.srtt is None:
            g["seed_verdicts"] += len(verdict_seqs)
        F = self.floor_value() if verdict_seqs or self.pending else self.last_floor
        need_floor = bool(self.pending) and any(s < F for s in self.pending) and F > self.last_floor
        if verdict_seqs and c.sender in ("S1c", "S2c"):
            for s in verdict_seqs:
                self.send_copy(now, s)
                g["verdict_copies"] += 1
        for s in serve_seqs:
            if self.can_resend(s, gen, now):
                self.send_copy(now, s)
                g["serves"] += 1
                g["resend_by_" + self.trig] += 1
        if need_floor:
            self.last_floor = F
            self.send_floor(now, F)
        if verdict_seqs and self.resume_on_verdict:
            self.maybe_resume(now)

    def floor_repeat(self, now):
        """The floor is sent again at each tail-sweep fire while F > A_rx (7.2)."""
        if not self.pending:
            return
        F = self.floor_value()
        if any(s < F for s in self.pending) and F > self.A_known:
            self.last_floor = max(self.last_floor, F)
            self.send_floor(now, F)

    def on_sweep(self, now):
        P = self.sweep_P
        lo = self.rhi_known + 1
        tail = []
        elig = P if self.cfg.tail_elig == "P" else self.R_now()
        for s in range(lo, self.n_sent):
            if self.cfg.tail_elig != "P":                # 13.4: the open tail is the probe timer's job
                break
            if now - self.sent_t[s] < P:
                break
            if s not in self.verdicted:
                tail.append(s)
        if self.cfg.n_total:
            # object mode: the sweep also re-serves reported holes.  Without it a finished stream sends no new
            # reports, so holes whose copies were lost are never served again (a stall the engine's NACK refresh avoids)
            known = [x for x in self.M_known if x not in self.verdicted and now - self.last_send[x] >= elig]
            tail = sorted(set(tail) | set(known))
        if tail:
            self.trig = "sweep" if self.cfg.tail_elig == "P" else "R"
            self.act(now, now, tail, open_tail=True)
        if self.last_floor_t != now:
            self.floor_repeat(now)
        if now + P <= self.t_end:
            self.push(now + P, 5, None)

    # ---- main loop ------------------------------------------------------------------------
    def run(self):
        c = self.cfg
        self.push(self.ph_s + c.tick, 4, None)
        self.push(self.ph_w + self.sweep_P, 5, None)
        heap = self.heap
        INF = math.inf
        while True:
            tn = heap[0][0] if heap else INF
            if self.src_next <= tn and self.src_next < INF:
                self.emit(self.src_next)
                continue
            if tn == INF or tn > self.t_end:
                break
            if self.stop_at is not None and tn > self.stop_at:
                break
            t, _, kind, pl = heapq.heappop(heap)
            self.t_last = t
            if kind == 0:
                self.on_data(t, pl[0], pl[1], pl[2], pl[3])
            elif kind == 1:
                self.on_floor(t, pl[0], pl[1])
            elif kind == 2:
                self.apply_floor(t, pl[0], pl[1])
            elif kind == 3:
                self.on_report(t, pl)
            elif kind == 4:
                self.on_tick(t)
            elif kind == 5:
                self.on_sweep(t)
            elif kind == 6:
                self.on_release(t, pl)
            elif kind == 7:
                self.on_r0e_expiry(t, pl)
            elif kind == 8:
                self.on_verdict_timer(t, pl)
            elif kind == 9:
                self.on_probe(t)
        return self.summary()

    # ---- readouts -------------------------------------------------------------------------
    def hole_table(self):
        """Per hole created by data arrival: (t_detect, R fill time or inf, pass time or inf, release time or inf)."""
        rows = []
        inf = math.inf
        for s, t0 in self.tdet.items():
            if s in self.fl_created:
                continue
            rows.append((s, t0, self.tfill.get(s, inf), self.tpass.get(s, inf), self.trel.get(s, inf)))
        rows.sort(key=lambda r: (r[1], r[0]))
        return rows

    def summary(self):
        from ustar import u_costs
        c = self.cfg
        g = dict(self.g)
        n = self.n_sent
        arr, dlv, wh, got = self.arr, self.dlv, self.was_hole, self.got
        at = np.asarray(self.atilde[:n])
        dl = np.asarray(dlv[:n])
        ok = dl >= 0
        y0 = np.maximum.accumulate(at)
        v = (dl - at)[ok]
        e = (dl - y0)[ok]
        qs = (50, 99, 99.9)
        pv = np.percentile(v, qs) if len(v) else [float("nan")] * 3
        pe = np.percentile(e, qs) if len(e) else [float("nan")] * 3
        delivered = int(ok.sum())
        eps_ = np.asarray([x[1] for x in self.episodes]) if self.episodes else np.zeros(1)
        hs = 0.0
        for j in range(min(n, self.next_deliver)):
            if got[j] and not wh[j] and dlv[j] >= 0:
                hs += dlv[j] - arr[j]
        # isolated holes: no other lost original within +-300 seqs
        W = 300
        ol = self.orig_lost
        iso = []
        for i, s in enumerate(ol):
            if (i > 0 and s - ol[i - 1] <= W) or (i + 1 < len(ol) and ol[i + 1] - s <= W):
                continue
            if s not in self.tdet:
                continue
            tr = [x for x in (self.tfill.get(s), self.tpass.get(s)) if x is not None]
            if tr:
                iso.append(min(tr) - self.tdet[s])
        iso = np.asarray(iso) if iso else np.zeros(0)
        rows = self.hole_table()
        n_holes = len(rows)
        rbf_num = sum(1 for (s, t0, R, P, Tr) in rows if Tr < R)                 # released by H_o, then filled
        fp_rel = sum(1 for (s, t0, R, P, Tr) in rows if P < R and Tr == math.inf)   # floor pass was the release, filled later
        fp_after_ho = sum(1 for (s, t0, R, P, Tr) in rows if P < R and Tr < P)
        out = dict(
            cell=c.cell, sender=c.sender, recv=c.recv, seed=c.seed, delta=c.delta, rho=c.rho, omega=c.omega,
            n_src=n, delivered=delivered, never_delivered=n - delivered, open_end=len(self.holes), passed_unfilled=len(self.passed),
            G_final=self.G, G_ratio=self.G / max(n, 1), one_minus_rho=1 - c.rho,
            v_p50=float(pv[0]), v_p99=float(pv[1]), v_p999=float(pv[2]), v_mean=float(v.mean()) if len(v) else float("nan"),
            e_p50=float(pe[0]), e_p99=float(pe[1]), e_p999=float(pe[2]), e_mean=float(e.mean()) if len(e) else float("nan"),
            hol_sum_nonhole=hs, n_holes=n_holes, n_missing=self.n_missing,
            stall_n=len(self.episodes), stall_p99=float(np.percentile(eps_, 99)), stall_total=float(eps_.sum()),
            max_ep_spend=int(max((x[2] for x in self.episodes), default=0)),
            iso_n=int(len(iso)), iso_p50=float(np.median(iso)) if len(iso) else float("nan"),
            iso_mean=float(iso.mean()) if len(iso) else float("nan"),
            repair_per_delivered=g["repair_sends"] / max(delivered, 1),
            floors_per_delivered=g["floors"] / max(delivered, 1), copies_per_delivered=g["verdict_copies"] / max(delivered, 1),
            GF_final=self.GF, GF_ratio=self.GF / max(n, 1),
            h_hat=self.n_missing / max(n, 1), orig_lost=len(self.orig_lost),
            rbf_num=rbf_num, rbf=rbf_num / max(n_holes, 1), fp_release_filled=fp_rel, fp_after_ho=fp_after_ho,
            ooo_per_delivered=g["ooo_loss"] / max(delivered, 1),
            d_all=(self.d_sum[0] / self.d_cnt[0]) if self.d_cnt[0] else None,
            d_rep=(self.d_sum[1] / self.d_cnt[1]) if self.d_cnt[1] else None,
            d_n=self.d_cnt[0], srtt=self.srtt, H_max=self.est.max_l, t_last=self.t_last,
            gauges=g,
        )
        # stage 3 readouts (three-knobs.md 9.2, 11 items 1 and 2): h(s) is the unit's application time; w is measured from it
        units = self.units_total
        hs_ = np.asarray(self.h_seq[:n])
        age = (dl - hs_)[ok]
        w = age - c.owd
        out.update(
            units=units, intake_expired=self.intake_expired, share_any=delivered / max(units, 1),
            share_T=int(within_lifetime(c.T, age).sum()) / max(units, 1),
            share_by_0p4=int((age <= 0.4).sum()) / max(units, 1), share_by_2=int((age <= 2.0).sum()) / max(units, 1),
            w_mean=float(w.mean()) if len(w) else float("nan"),
            w_p50=float(np.percentile(w, 50)) if len(w) else float("nan"),
            w_p99=float(np.percentile(w, 99)) if len(w) else float("nan"),
            l2_fail=g["l2_fail"], rel_by_hold=g["rel_ho"], rel_by_floor=g["rel_by_floor"],
            finish=(self.t_front - (units - 1) * self.dt) if (c.n_total and self.t_front is not None) else None)
        self.summary3b(out, n, ok, dl, hs_)
        if c.n_total:
            out["tail_time"] = (self.t_front - self.sent_t[n - 1]) if self.t_front is not None else None
            out["stalled"] = self.t_front is None
            if c.dead is not None:
                d0 = c.dead[0]
                tail = [s for s in range(d0, n)]
                out["tail_n"] = len(tail)
                out["tail_verdicts"] = sum(1 for s in tail if s in self.verdicted)
                w_end = c.dead[1] + c.dead[2]
                out["tail_filled_after_window"] = sum(1 for s in tail if got[s] and arr[s] >= w_end)
                out["tail_passed"] = sum(1 for s in tail if s in self.tpass)
                tv = np.asarray([dlv[s] - self.atilde[s] for s in tail if dlv[s] >= 0])
                out["tail_v_mean"] = float(tv.mean()) if len(tv) else None
                out["tail_v_p50"] = float(np.median(tv)) if len(tv) else None
                out["tail_delivered"] = int(len(tv))
                out["tail_verdicted"] = [s for s in tail if s in self.verdicted]
                out["tail_v"] = [(dlv[s] - self.atilde[s]) if dlv[s] >= 0 else None for s in tail]
                # when the frontier is released past the seq: the floor or early-skip pass if it was passed, else its delivery
                out["tail_res"] = [((self.tpass[s] if s in self.tpass else dlv[s]) - self.atilde[s]) if (s in self.tpass or dlv[s] >= 0) else None for s in tail]
        if c.recv in ("U", "HZ") or c.want_p4:
            out.update(u_costs(self, rows))
        if c.want_p4:
            out["p4c"] = self.p4c(rows)
            out["pass_dt"] = self.pass_dt
        if c.want_trace:
            out["_trace"] = self.trace
            out["_arr"], out["_dlv"], out["_wh"], out["_got"] = arr, dlv, wh, got
            out["_atilde"], out["_sent_t"] = self.atilde[:n], self.sent_t[:n]
            out["_h"] = self.h_seq[:n]
            out["_rows"] = rows
        return out

    def summary3b(self, out, n, ok, dl, hs_):
        """Stage 3b readouts (three-knobs.md 13.7.1): equal-footing delay quantiles over every unit handed over, evidence gauge,
        link and pause gauges, R, resend triggers, the strict-order stall's parts."""
        c, g = self.cfg, self.g
        units = self.units_total
        m = max(units, n)                                # streaming mode can send one unit more than int(dur * rate)
        d = np.full(m, np.inf)
        d[:n][ok] = (dl - hs_)[ok]                       # never-delivered and intake-expired units stay at +inf
        d.sort()
        for q in (50, 90, 99):
            out["dh_p%d" % q] = float(d[max(0, math.ceil(q / 100.0 * m) - 1)])
        out["verdicts"] = g["verdicts"]
        out["vta"] = sum(1 for s in self.verdicted if self.got[s])
        out["intake_expired_deq"] = self.intake_expired_deq
        for k in ("queue_drop", "queue_drop_in", "queue_drop_out", "probes", "pause_events", "resend_by_R", "resend_by_report", "resend_by_sweep"):
            out[k] = g[k]
        out["link_util_w"] = self.busy_w / 9.0
        plog = list(self.pause_log)
        if self.paused:
            plog.append((self.pause_t0, self.t_last))
        out["pause_time"] = sum(b - a for a, b in plog)
        pw = sum(max(0.0, min(b, 10.0) - max(a, 1.0)) for a, b in plog)
        out["pause_time_w"] = pw
        out["throttle"] = "link" if out["link_util_w"] >= 0.98 else ("pause" if pw > 4.5 else "none")
        out["R_mean"] = self.R_sum / max(self.rep_n, 1)
        out["srtt_mean"] = self.srtt_sum / max(self.rep_n, 1)
        out["resume_delay"] = (self.first_emit_oe - self.oe) if (self.oe is not None and self.first_emit_oe is not None) else None
        out["stall_max"] = max((e[1] for e in self.episodes), default=0.0)
        parts = None
        tf = next((x for x in self.floor_arrivals if x >= self.oe), None) if self.oe is not None else None
        if tf is not None:
            # the longest stall episode still open when the first floor after the outage arrives (the outage's own episode);
            # a longer episode elsewhere in the run says nothing about 13.5's parts
            eps = [e for e in self.episodes if e[0] - 1e-9 <= tf <= e[0] + e[1] + 1e-9]
            if eps:
                st, ln, _ = max(eps, key=lambda e: e[1])
                ep_end = st + ln
                fills = [v for v in self.tfill.values() if tf <= v <= ep_end]
                p1 = tf - self.oe
                p2 = (max(fills) - tf) if fills else 0.0
                parts = (p1, p2, ep_end - self.oe - p1 - p2)
        out["stall_parts"] = parts
        if c.want_trace:
            out["_deq_log"] = self.deq_log

    def p4c(self, rows):
        """(observed consumer stall, expected min(H_o, R - t_det, t_pass - t_det)) per isolated hole; the stall is read
        off the next packet's wait: y_{s+1} - arrival_{s+1}."""
        ol = self.orig_lost
        res = []
        info = {r[0]: r for r in rows}
        for i, s in enumerate(ol):
            if (i > 0 and s - ol[i - 1] <= 300) or (i + 1 < len(ol) and ol[i + 1] - s <= 300):
                continue
            r = info.get(s)
            if r is None or not self.got[s + 1] or self.dlv[s + 1] < 0:
                continue
            ho = self.hoq.get(s)
            exp = min(x - r[1] for x in (r[2], r[3], (r[1] + ho) if ho is not None else math.inf))
            if exp == math.inf:
                continue
            res.append((self.dlv[s + 1] - self.arr[s + 1], exp))
        return res


def run(cfg: Cfg):
    return Sim(cfg).run()


def object_cfg(cell, recv, seed, **kw):
    """Part T: the short object of sim_lost_tail (7.13)."""
    n = 12147
    kw.setdefault("dead", (n - 347, (n - 347) / 10000.0, 1.5))
    kw.setdefault("owd", 0.05 if cell == "L10" else 0.005)
    return Cfg(cell=cell, sender="S1c", recv=recv, seed=seed, n_total=n, delta=50.0, rho=0.99, **kw)
