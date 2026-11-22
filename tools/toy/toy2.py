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


def delta_at_b(b):
    return DELTA_AUTO * 10.0 ** (-2.0 * math.log2(b))


@dataclass
class Cfg:
    cell: str = "I1"
    sender: str = "S1c"            # S0 | S1 (verdict, no copy; test only) | S1c
    recv: str = "R1"               # R1 | U | R0e | HZ
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
    recv_eps: str = "cum"          # R0e's eps_hat: cum (registered: cumulative missing fraction) | ewma (reported-only variant)


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
        self.nmax = (c.n_total if c.n_total else int(c.dur * c.rate)) + 16
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
        self.rhi_known = -1
        self.M_known = set()
        self.last_gen = -1.0
        self.verdicted = set()
        self.pending = set()
        self.last_floor = -1
        self.last_floor_t = -1e9
        self.floor_rep = {}
        self.src_next = 0.0
        self.paused = False
        self.srtt = None
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
                      r0e_budget_open=0, r0e_fallback=0, r0e_expiries=0, fh_late=0, fifo_viol=0)
        self.trace = [] if c.want_trace else None
        self.orig_lost = []
        self.heap = []
        self.ctr = 0
        self.t_last = 0.0
        self.last_arr = -1.0
        self.pass_dt = []
        self.eps_ew = 0.0

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

    def H_now(self, now):
        if self.cfg.H_force is not None:
            return float(self.cfg.H_force)
        return self.est.value(now)

    def D_now(self):
        """(D, enabled).  EWMA before its first sample: nothing is sheddable (enabled False), which is
        distinct from the ZERO ablation (D = 0, the gate always admits)."""
        arm = self.cfg.d_arm
        if arm == "RT":
            return self.D_rt, True
        if arm == "ZERO":
            return 0.0, True
        if self.srtt is None:
            return 0.0, False
        return min(self.b, 2.0) * self.srtt, True

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
        return None

    def fifo_note(self, a):
        if a < self.last_arr - 1e-12:
            self.g["fifo_viol"] += 1
        else:
            self.last_arr = a

    # ---- source ---------------------------------------------------------------------------
    def emit(self, t):
        c = self.cfg
        s = self.n_sent
        self.n_sent += 1
        self.sent_t[s] = t
        self.last_send[s] = t
        a = t + self.delay.at(t)
        self.atilde[s] = a
        self.fifo_note(a)
        if self.trace is not None:
            self.trace.append((round(t, 9), s, 0, 0, 0))
        if self.lost(s, 0, t):
            self.orig_lost.append(s)
        else:
            self.push(a, 0, (s, 0, t))
        nxt = t + self.dt
        o = self.O()
        if o > self.g["max_O"]:
            self.g["max_O"] = o
        if c.n_total:
            done = self.n_sent >= c.n_total
        else:
            done = nxt >= c.dur
        if done:
            self.src_next = math.inf
        elif o >= self.B:
            self.paused = True
            self.g["pause_events"] += 1
            self.src_next = math.inf
        else:
            self.src_next = nxt

    def send_copy(self, now, s):
        k = self.copies[s] + 1
        self.copies[s] = k
        self.last_send[s] = now
        self.g["repair_sends"] += 1
        if self.trace is not None:
            self.trace.append((round(now, 9), s, k, 0, 1))
        a = now + self.delay.at(now)
        self.fifo_note(a)
        if not self.lost(s, k, now):
            self.push(a, 0, (s, k, now))

    def send_floor(self, now, f):
        rep = self.floor_rep.get(f, 0)
        self.floor_rep[f] = rep + 1
        self.g["floors"] += 1
        if rep:
            self.g["floor_rep"] += 1
        if self.trace is not None:
            self.trace.append((round(now, 9), f, -1, 0, 2))
        self.last_floor_t = now
        a = now + self.delay.at(now)
        self.fifo_note(a)
        if not self.lost(f, rep, now, SALT_FLOORLOSS):
            self.push(a, 1, f)

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
        if self.t_front is None and self.n_sent >= self.nmax - 16 and not self.holes and self.rhi + 1 >= self.nmax - 16 \
                and self.cfg.n_total:
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

    def on_data(self, now, s, c, tau):
        got = self.got
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
                self.g["late"] += 1
                self.g["ooo_loss"] += 1
                self.tfill[s] = now
                self.dlv[s] = now
            self.dirty = True
            if self.cfg.recv == "R0e":
                self.r0e_buffer(now, s)
        self.advance(now)

    def on_floor(self, now, f):
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
            self.unrel.discard(m)
            self.released.discard(m)
            self.passed.add(m)
            self.tpass[m] = now
            self.passed_since_report.append(m)
            n += 1
        if n:
            self.G += n
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
        eps_hat = self.eps_ew if self.cfg.recv_eps == "ewma" else self.n_missing / max(frontier, 1)
        if self.r0e_given_up < eps_hat * frontier:
            hold = srtt / 2
            self.g["r0e_budget_open"] += 1
        else:
            hold = min(0.300, max(0.060, 4 * srtt))
            self.g["r0e_fallback"] += 1
        self.push(now + hold, 7, s)

    def on_r0e_expiry(self, now, j):
        self.g["r0e_expiries"] += 1
        m = self.min_hole()
        if m is not None and m < j:
            before = self.g["passes"]
            self.pass_below(now, j, "r0e")
            self.r0e_given_up += self.g["passes"] - before

    def on_tick(self, now):
        if self.dirty:
            self.dirty = False
            m = self.min_hole()
            A = m if m is not None else self.rhi + 1
            rep = (now, A, self.G, self.rhi, tuple(sorted(self.holes)), tuple(self.passed_since_report))
            self.passed_since_report = []
            self.push(now + self.cfg.owd, 3, rep)
        if now + self.cfg.tick <= self.t_end:
            self.push(now + self.cfg.tick, 4, None)

    # ---- sender ---------------------------------------------------------------------------
    def on_report(self, now, rep):
        gen, A, G, rhi, M, passed = rep
        if (A, G) > (self.A_known, self.G_known):
            if A > self.A_known:                       # RTT sample (7.2): the report's arrival minus the send time of
                smp = now - self.sent_t[A - 1]         # the highest seq it newly acknowledges
                self.srtt = smp if self.srtt is None else self.srtt + (smp - self.srtt) / 8.0
            self.A_known, self.G_known = A, G
        if gen > self.last_gen:
            self.last_gen = gen
            if rhi > self.rhi_known:
                self.rhi_known = rhi
            self.M_known = set(M)
        Ak, Mk, rk = self.A_known, self.M_known, self.rhi_known
        self.pending = {s for s in self.pending if s >= Ak and (s in Mk or s > rk)}
        if self.paused and self.O() < self.B:
            self.paused = False
            self.src_next = max(now, self.src_next if self.src_next != math.inf else now)
        self.act(now, gen, sorted(Mk), open_tail=False)

    def can_resend(self, s, gen):
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
        if c.sender != "S0":
            A_avail = self.allow() - (self.G_known + len(self.pending))
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
                    else:
                        g["gate_cold"] += 1
                    serve_seqs.append(s)
        else:
            serve_seqs = cands_
        for s in verdict_seqs:
            ver.add(s)
            self.pending.add(s)
        g["verdicts"] += len(verdict_seqs)
        F = self.floor_value() if verdict_seqs or self.pending else self.last_floor
        need_floor = bool(self.pending) and any(s < F for s in self.pending) and F > self.last_floor
        if verdict_seqs and c.sender == "S1c":
            for s in verdict_seqs:
                self.send_copy(now, s)
                g["verdict_copies"] += 1
        for s in serve_seqs:
            if self.can_resend(s, gen):
                self.send_copy(now, s)
                g["serves"] += 1
        if need_floor:
            self.last_floor = F
            self.send_floor(now, F)

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
        for s in range(lo, self.n_sent):
            if now - self.sent_t[s] < P:
                break
            if s not in self.verdicted:
                tail.append(s)
        if tail:
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
                self.on_data(t, pl[0], pl[1], pl[2])
            elif kind == 1:
                self.on_floor(t, pl)
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
        rbf_num = sum(1 for (s, t0, R, P, T) in rows if T < R)                 # released by H_o, then filled
        fp_rel = sum(1 for (s, t0, R, P, T) in rows if P < R and T == math.inf)   # floor pass was the release, filled later
        fp_after_ho = sum(1 for (s, t0, R, P, T) in rows if P < R and T < P)
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
            h_hat=self.n_missing / max(n, 1), orig_lost=len(self.orig_lost),
            rbf_num=rbf_num, rbf=rbf_num / max(n_holes, 1), fp_release_filled=fp_rel, fp_after_ho=fp_after_ho,
            ooo_per_delivered=g["ooo_loss"] / max(delivered, 1),
            d_all=(self.d_sum[0] / self.d_cnt[0]) if self.d_cnt[0] else None,
            d_rep=(self.d_sum[1] / self.d_cnt[1]) if self.d_cnt[1] else None,
            d_n=self.d_cnt[0], srtt=self.srtt, H_max=self.est.max_l, t_last=self.t_last,
            gauges=g,
        )
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
        if c.recv in ("U", "HZ") or c.want_p4:
            out.update(u_costs(self, rows))
        if c.want_p4:
            out["p4c"] = self.p4c(rows)
            out["pass_dt"] = self.pass_dt
        if c.want_trace:
            out["_trace"] = self.trace
            out["_arr"], out["_dlv"], out["_wh"], out["_got"] = arr, dlv, wh, got
            out["_atilde"], out["_sent_t"] = self.atilde[:n], self.sent_t[:n]
            out["_rows"] = rows
        return out

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
