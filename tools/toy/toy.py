"""Toy model for docs/derivations/skip-rule-efficiency.md section 6 (stage 1: sender rules).

No constant here is provenance for the engine.  Every free choice is a Cfg field with a
note on where it came from (spec section, or "toy choice").  Candidate rules differ only by
composition flags (Cand); there is no comparison on rho, delta or the hint in rule code:
rho enters through allow = floor((1 - rho) * S_src) alone.
"""
import heapq
import math
from dataclasses import dataclass, field

import numpy as np

M64 = (1 << 64) - 1


def u01(seed, seq, copy, path, salt):
    """Stateless uniform in [0,1) keyed by (seed, seq, copy, path, salt): common random numbers."""
    x = (((seed * 0x1000000 + seq) * 64 + (copy & 63)) * 4 + path) * 8 + salt
    x = (x * 0x9E3779B97F4A7C15) & M64
    x ^= x >> 30
    x = (x * 0xBF58476D1CE4E5B9) & M64
    x ^= x >> 27
    x = (x * 0x94D049BB133111EB) & M64
    x ^= x >> 31
    return (x >> 11) * (1.0 / (1 << 53))


SALT_LOSS, SALT_JIT, SALT_PLACE, SALT_FLOORLOSS, SALT_FLOORJIT = 0, 1, 2, 3, 4

# ---- candidates: composition flags only -------------------------------------------------
@dataclass(frozen=True)
class Cand:
    name: str
    verdicts: bool = False   # False = S0 (serve until filled)
    copy: bool = False       # last copy with every verdict (ADR-0084 Amendment 2)
    run_atomic: bool = False
    t_rho: bool = False      # online T_rho, Kaplan-Meier under own verdicts


CANDS = {
    "S0": Cand("S0"),
    "S1": Cand("S1", verdicts=True),
    "S1c": Cand("S1c", verdicts=True, copy=True),
    "S2": Cand("S2", verdicts=True, copy=True, run_atomic=True),
    "S3": Cand("S3", verdicts=True, copy=True, run_atomic=True, t_rho=True),
}

# ---- loss cells (v1 of the spec, section 6 table; I1 numbers from the worked example) -------
CELLS = {
    "I1": dict(kind="iid", p0=0.005, pc=0.025),
    "I2": dict(kind="iid", p0=0.03, pc=0.10),
    "G": dict(kind="ge", mean=0.025, burst=8.0, pbad=0.5),
    "B10": dict(kind="none", blackout=(10.0, 0.2)),
    "B60": dict(kind="none", blackout=(60.0, 0.2)),
    "T": dict(kind="none", dead_tail=346),
}
CELL_DUR = {"B60": 70.0}


@dataclass
class Cfg:
    cell: str = "I1"
    cand: str = "S0"
    seed: int = 0
    dur: float = 20.0
    rate: float = 10000.0
    owds: tuple = (0.005,)          # forward one-way delays per path; path 0 is the fast one
    jitter: float = 0.001           # per-packet jitter, uniform [0, jitter]; spec base 1 ms
    fifo: bool = True               # in-path FIFO kept (originals only, so candidates stay coupled by CRN)
    phi_slow: float = 0.0           # N = 2 placement probability on path 1
    rho: float = 0.99
    H: object = "h8"                # seconds, ("srtt", frac), or "h8" (ordering-hold section 8)
    D_frac: float = 0.5             # D = D_frac * RTprop (ADR-0084 text: RTprop/2 at Realtime)
    tick: float = 0.002             # gap-report sampler
    sweep_lo: float = 0.025         # clamp(2 SRTT, 25, 100) ms, rho-step-sites hole_nack_refresh
    sweep_hi: float = 0.100
    pause_B: int = 2048             # source pause at O >= B*N (ledger F01)
    grace: float = 1.0
    blackout_path: object = None    # None = every path; else a path index
    km_refresh: float = 0.05
    want_trace: bool = False        # keep per-seq arrays (LP) and the sender trace (P2)
    check_p1: bool = True
    floor_with_copy: bool = True    # True: floor frame rides the copy's datagram (same fate); False: separate control datagram, independent loss

    @property
    def n_paths(self):
        return len(self.owds)


def ge_states(seed, n_slots, mean, burst):
    """Gilbert-Elliott state per slot (1 = bad); stationary bad fraction = mean/pbad is given by caller."""
    rng = np.random.default_rng(seed * 7919 + 13)
    p_exit = 1.0 / burst
    p_enter = mean / (1.0 - mean) * p_exit
    out = np.zeros(n_slots, dtype=np.uint8)
    i, bad = 0, False
    while i < n_slots:
        ln = int(rng.geometric(p_exit if bad else p_enter))
        if bad:
            out[i:i + ln] = 1
        i += ln
        bad = not bad
    return out


class Sim:
    def __init__(self, cfg: Cfg):
        self.cfg = cfg
        c = cfg
        self.cand = CANDS[c.cand]
        cell = CELLS[c.cell]
        self.cell = cell
        self.owd = list(c.owds)
        self.owd0 = min(self.owd)
        self.fast = self.owd.index(self.owd0)
        self.rtt = 2 * self.owd0                       # reports return on the fast path
        self.D = c.D_frac * self.rtt
        self.hole_law = c.n_paths > 1                  # spec: RFC 9002 hole law at N = 2
        self.dt = 1.0 / c.rate
        self.nmax = int(c.dur * c.rate) + 16
        self.B = c.pause_B * c.n_paths
        self.sweep_P = min(max(2 * self.rtt, c.sweep_lo), c.sweep_hi)
        self.t_end = c.dur + c.grace
        # loss model
        k = cell["kind"]
        self.loss_kind = k
        self.p0 = cell.get("p0", 0.0)
        self.pc = cell.get("pc", 0.0)
        if k == "ge":
            pbad = cell["pbad"]
            slots = int((c.dur + c.grace + 1) * c.rate) + 8
            self.ge = [ge_states(c.seed * 4 + p, slots, cell["mean"] / pbad, cell["burst"]) for p in range(c.n_paths)]
            self.pbad = pbad
        self.blk = None
        if "blackout" in cell:
            t0, ln = cell["blackout"]
            self.blk = (t0, t0 + ln)
        self.dead_from = None
        if "dead_tail" in cell:
            self.dead_from = int(c.dur * c.rate) - cell["dead_tail"]
        # sender state
        self.n_sent = 0
        self.sent_t = [0.0] * self.nmax
        self.path_of = bytearray(self.nmax)
        self.copies = bytearray(self.nmax)          # copies sent after the original
        self.last_send = [0.0] * self.nmax
        self.last_orig_arr = [-1.0] * c.n_paths
        self.A_known = 0
        self.G_known = 0
        self.rhi_known = -1
        self.M_known = set()
        self.last_gen = -1.0
        self.verdicted = set()
        self.pending = set()
        self.last_floor = -1
        self.last_floor_t = -1e9
        self.src_next = 0.0
        self.paused = False
        self.floor_ctr = 0
        # receiver state
        self.got = bytearray(self.nmax)
        self.arr = [0.0] * self.nmax
        self.dlv = [-1.0] * self.nmax
        self.was_hole = bytearray(self.nmax)
        self.holes = set()
        self.hheap = []
        self.passed = set()
        self.G = 0
        self.rhi = -1
        self.next_deliver = 0
        self.F_rx = 0
        self.dirty = False
        self.passed_since_report = []
        self.tdet, self.tfill, self.tpass, self.fillc = {}, {}, {}, {}
        self.spread = 0.0
        self.ep_start = None
        self.ep_pass = 0
        self.episodes = []        # (start, length, passes)
        self.hol = []
        # KM
        self.seen = set()
        self.n_reported = 0
        self.km_age, self.km_ev = [], []
        self.km_next = 0.0
        self.S_hat = None
        self.T_rho_cache = self.D
        self.vtime = {}
        # gauges / counters
        g = dict(verdicts=0, verdict_copies=0, floors_ctl=0, floors_att=0, repair_sends=0, serves=0, refusals=0,
                 age_gate_fail=0, trho_eval=0, trho_dbind=0, trho_tmax=0, passes=0, late=0, dup=0, p1_viol=0,
                 p1_worst=0, pause_events=0, max_O=0, floor_refresh=0, p1_checks=0)
        self.g = g
        self.trace = [] if c.want_trace else None
        self.orig_lost = []
        self.heap = []
        self.ctr = 0

    # ---- link ---------------------------------------------------------------------------
    def H_now(self):
        h = self.cfg.H
        if isinstance(h, tuple):
            return h[1] * self.rtt
        if h == "h8":
            return max(0.0, max(self.owd) - self.owd[self.fast]) + self.spread
        return float(h)

    def in_blackout(self, t, path=None):
        if self.blk is None:
            return False
        bp = self.cfg.blackout_path
        if bp is not None and path is not None and path != bp:
            return False
        return self.blk[0] <= t < self.blk[1]

    def lost(self, seq, copy, path, t, salt=SALT_LOSS):
        if self.in_blackout(t, path):
            return True
        if self.dead_from is not None and seq >= self.dead_from and salt == SALT_LOSS:
            return True
        u = u01(self.cfg.seed, seq, copy, path, salt)
        if self.loss_kind == "iid":
            return u < (self.p0 if copy == 0 and salt == SALT_LOSS else self.pc)
        if self.loss_kind == "ge":
            st = self.ge[path]
            i = int(t * self.cfg.rate)
            if i >= len(st):
                i = len(st) - 1
            return bool(st[i]) and u < self.pbad
        return False

    def push(self, t, kind, payload):
        self.ctr += 1
        heapq.heappush(self.heap, (t, self.ctr, kind, payload))

    # ---- source -------------------------------------------------------------------------
    def O(self):
        return len(self.M_known) + max(0, self.n_sent - 1 - self.rhi_known)

    def emit(self, t):
        c = self.cfg
        s = self.n_sent
        self.n_sent += 1
        self.sent_t[s] = t
        self.last_send[s] = t
        if c.n_paths > 1 and u01(c.seed, s, 0, 0, SALT_PLACE) < c.phi_slow:
            p = 1 if self.fast == 0 else 0
        else:
            p = self.fast
        self.path_of[s] = p
        if self.trace is not None:
            self.trace.append((round(t, 9), s, 0, p, 0))
        if self.lost(s, 0, p, t):
            self.orig_lost.append(s)
        else:
            a = t + self.owd[p] + c.jitter * u01(c.seed, s, 0, p, SALT_JIT)
            if c.fifo:
                lo = self.last_orig_arr[p] + 1e-9
                if a < lo:
                    a = lo
                self.last_orig_arr[p] = a
            self.push(a, 0, (s, 0, -1))
        nxt = t + self.dt
        o = self.O()
        if o > self.g["max_O"]:
            self.g["max_O"] = o
        if nxt >= c.dur:
            self.src_next = math.inf
        elif o >= self.B:
            self.paused = True
            self.g["pause_events"] += 1
            self.src_next = math.inf
        else:
            self.src_next = nxt

    def send_copy(self, now, s, floor):
        c = self.cfg
        k = self.copies[s] + 1
        self.copies[s] = k
        self.last_send[s] = now
        self.g["repair_sends"] += 1
        p = self.fast
        if self.trace is not None:
            self.trace.append((round(now, 9), s, k, p, 1))
        if not self.lost(s, k, p, now):
            self.push(now + self.owd[p] + c.jitter * u01(c.seed, s, k, p, SALT_JIT), 0, (s, k, floor))

    def send_floor_only(self, now, f):
        c = self.cfg
        self.floor_ctr += 1
        self.g["floors_ctl"] += 1
        p = self.fast
        if self.trace is not None:
            self.trace.append((round(now, 9), f, -1, p, 2))
        if not self.lost(self.floor_ctr, 1, p, now, SALT_FLOORLOSS):
            self.push(now + self.owd[p] + c.jitter * u01(c.seed, self.floor_ctr, 0, p, SALT_FLOORJIT), 1, f)

    # ---- receiver -----------------------------------------------------------------------
    def min_hole(self):
        h, hs = self.hheap, self.holes
        while h and h[0] not in hs:
            heapq.heappop(h)
        return h[0] if h else None

    def advance(self, now):
        m = self.min_hole()
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

    def on_data(self, now, s, c, floor):
        got = self.got
        if got[s]:
            self.g["dup"] += 1
        else:
            got[s] = 1
            self.arr[s] = now
            if s in self.holes:
                self.holes.discard(s)
                self.tfill[s] = now
                self.fillc[s] = c
                if c == 0:
                    d = now - self.tdet[s]
                    if d > self.spread:
                        self.spread = d
            elif s > self.rhi:
                for q in range(self.rhi + 1, s):
                    self.holes.add(q)
                    heapq.heappush(self.hheap, q)
                    self.was_hole[q] = 1
                    self.tdet[q] = now
                self.rhi = s
            elif s in self.passed:
                self.passed.discard(s)
                self.G -= 1
                self.g["late"] += 1
                self.tfill[s] = now
                self.dlv[s] = now
            self.dirty = True
        self.advance(now)
        if floor >= 0:
            self.on_floor(now, floor)

    def on_floor(self, now, f):
        h = self.H_now()
        if h <= 0:
            self.apply_floor(now, f)
        else:
            self.push(now + h, 2, f)

    def apply_floor(self, now, f):
        if f <= self.F_rx:
            return
        self.F_rx = f
        if f - 1 > self.rhi:
            for q in range(self.rhi + 1, f):
                self.holes.add(q)
                heapq.heappush(self.hheap, q)
                self.was_hole[q] = 1
                self.tdet[q] = now
            self.rhi = f - 1
        n = 0
        while True:
            m = self.min_hole()
            if m is None or m >= f:
                break
            heapq.heappop(self.hheap)
            self.holes.discard(m)
            self.passed.add(m)
            self.tpass[m] = now
            self.passed_since_report.append(m)
            n += 1
        if n:
            self.G += n
            self.g["passes"] += n
            self.ep_pass += n
            self.dirty = True
            if self.cfg.check_p1:
                allow = int((1 - self.cfg.rho) * self.n_sent + 1e-9)
                self.g["p1_checks"] += 1
                if self.G > allow:
                    self.g["p1_viol"] += 1
                    self.g["p1_worst"] = max(self.g["p1_worst"], self.G - allow)
        self.advance(now)

    def on_tick(self, now):
        if self.dirty:
            self.dirty = False
            m = self.min_hole()
            A = m if m is not None else self.rhi + 1
            if not self.in_blackout(now):
                rep = (now, A, self.G, self.rhi, tuple(sorted(self.holes)), tuple(self.passed_since_report))
                self.passed_since_report = []
                self.push(now + self.owd[self.fast], 3, rep)
            else:
                self.dirty = True
        if now + self.cfg.tick <= self.t_end:
            self.push(now + self.cfg.tick, 4, None)

    # ---- sender -------------------------------------------------------------------------
    def allow(self):
        return int((1 - self.cfg.rho) * self.n_sent + 1e-9)

    def on_report(self, now, rep):
        gen, A, G, rhi, M, passed = rep
        if (A, G) > (self.A_known, self.G_known):          # pair-merge (spec 3.1)
            self.A_known, self.G_known = A, G
        if gen > self.last_gen:
            self.last_gen = gen
            if rhi > self.rhi_known:
                self.rhi_known = rhi
            newM = set(M)
            self._km_update(gen, newM, set(passed))
            self.M_known = newM
        Ak, Mk, rk = self.A_known, self.M_known, self.rhi_known
        self.pending = {s for s in self.pending if s >= Ak and (s in Mk or s > rk)}
        if self.paused and self.O() < self.B:
            self.paused = False
            self.src_next = max(now, self.src_next if self.src_next != math.inf else now)
        self.act(now, gen, [s for s in sorted(Mk)], open_tail=False)

    def _km_update(self, gen, newM, passed):
        sent_t = self.sent_t
        for s in newM:
            if s not in self.seen:
                self.seen.add(s)
                self.n_reported += 1
        for s in self.M_known:
            if s not in newM:
                age = gen - sent_t[s]
                self.km_age.append(age)
                self.km_ev.append(0 if s in passed else 1)

    def _km_refresh(self, now):
        if now < self.km_next:
            return
        self.km_next = now + self.cfg.km_refresh
        ages = list(self.km_age)
        ev = list(self.km_ev)
        for s in self.M_known:                      # open holes: right-censored at their current age
            ages.append(now - self.sent_t[s])
            ev.append(0)
        if not ages:
            self.S_hat = None
            return
        a = np.asarray(ages)
        e = np.asarray(ev, dtype=float)
        o = np.lexsort((1 - e, a))
        a, e = a[o], e[o]
        n = len(a)
        at_risk = n - np.arange(n)
        surv = np.cumprod(1.0 - e / at_risk)
        self.S_hat = (np.concatenate(([0.0], a)), np.concatenate(([1.0], surv)))

    def T_rho(self, now):
        """T_rho = max(D, S^-1((1-rho)/h) - L); gauges for D-bind and t_max-bind."""
        self._km_refresh(now)
        g = self.g
        g["trho_eval"] += 1
        if self.S_hat is None or self.n_sent == 0 or self.n_reported == 0:
            g["trho_dbind"] += 1
            return self.D
        h = self.n_reported / self.n_sent
        p = (1 - self.cfg.rho) / h
        t, S = self.S_hat
        tmax = float(t[-1])
        if p >= 1.0:
            inv = 0.0
        elif S[-1] > p:
            inv = tmax
            g["trho_tmax"] += 1
        else:
            inv = float(np.interp(p, S[::-1], t[::-1]))
        L = self.owd[self.fast] + self.H_now()
        v = inv - L
        if v <= self.D:
            g["trho_dbind"] += 1
            return self.D
        return v

    def ripe(self, s, gen):
        if not self.hole_law:
            return True
        return gen - self.sent_t[s] >= 1.125 * self.owd[self.path_of[s]]

    def can_resend(self, s, gen):
        if self.copies[s] == 0:
            return True
        return gen >= self.last_send[s] + self.owd[self.fast] + self.cfg.jitter + 1e-9

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
        cand = self.cand
        ver = self.verdicted
        cands_ = []
        for s in holes:
            if s in ver:
                continue
            if open_tail or self.ripe(s, gen):
                cands_.append(s)
        if not cands_:
            self._floor_housekeeping(now)
            return
        # runs: maximal blocks of consecutive seqs
        runs, cur = [], [cands_[0]]
        for s in cands_[1:]:
            if s == cur[-1] + 1:
                cur.append(s)
            else:
                runs.append(cur)
                cur = [s]
        runs.append(cur)
        g = self.g
        verdict_seqs, serve_seqs = [], []
        if cand.verdicts:
            A_avail = self.allow() - (self.G_known + len(self.pending))
            thr = None
            for run in runs:
                age = now - self.sent_t[run[0]]
                if cand.run_atomic:
                    if thr is None:
                        thr = self.T_rho(now) if cand.t_rho else self.D
                    gate = age >= thr if cand.t_rho else age > thr
                    if not gate:
                        g["age_gate_fail"] += 1
                        serve_seqs.extend(run)
                    elif len(run) <= A_avail:
                        verdict_seqs.extend(run)
                        A_avail -= len(run)
                    else:
                        g["refusals"] += 1
                        serve_seqs.extend(run)
                else:
                    for s in run:
                        if now - self.sent_t[s] > self.D:
                            if A_avail >= 1:
                                verdict_seqs.append(s)
                                A_avail -= 1
                            else:
                                g["refusals"] += 1
                                serve_seqs.append(s)
                        else:
                            g["age_gate_fail"] += 1
                            serve_seqs.append(s)
        else:
            for run in runs:
                serve_seqs.extend(run)
        for s in verdict_seqs:
            ver.add(s)
            self.pending.add(s)
            self.vtime[s] = now
        g["verdicts"] += len(verdict_seqs)
        F = self.floor_value() if verdict_seqs or self.pending else self.last_floor
        need_floor = bool(self.pending) and any(s < F for s in self.pending) and F > self.last_floor
        fl = F if (need_floor and self.cfg.floor_with_copy) else -1
        sent_att = False
        if verdict_seqs and self.cand.copy:
            for s in verdict_seqs:
                self.send_copy(now, s, fl)
                g["verdict_copies"] += 1
            sent_att = fl >= 0
        for s in serve_seqs:
            if self.can_resend(s, gen):
                self.send_copy(now, s, -1)
                g["serves"] += 1
        if need_floor:
            self.last_floor = F
            self.last_floor_t = now
            if sent_att and self.cfg.floor_with_copy:
                g["floors_att"] += 1
            else:
                self.send_floor_only(now, F)
        self._floor_housekeeping(now)

    def _floor_housekeeping(self, now):
        """Refresh a floor that may have been lost with its copy: resend if a verdict is still pending."""
        if self.pending and self.last_floor >= 0 and now - self.last_floor_t >= self.rtt + 2 * self.cfg.tick + self.H_now() + self.cfg.jitter:
            F = self.floor_value()
            if any(s < F for s in self.pending):
                self.last_floor = max(self.last_floor, F)
                self.last_floor_t = now
                self.g["floor_refresh"] += 1
                self.send_floor_only(now, self.last_floor)

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
        else:
            self._floor_housekeeping(now)
        if now + P <= self.t_end:
            self.push(now + P, 5, None)

    # ---- main loop ----------------------------------------------------------------------
    def run(self):
        c = self.cfg
        self.push(c.tick, 4, None)
        self.push(self.sweep_P, 5, None)
        heap = self.heap
        INF = math.inf
        while True:
            tn = heap[0][0] if heap else INF
            if self.src_next <= tn and self.src_next < INF:
                self.emit(self.src_next)
                continue
            if tn == INF or tn > self.t_end:
                break
            t, _, kind, pl = heapq.heappop(heap)
            if kind == 0:
                self.on_data(t, pl[0], pl[1], pl[2])
            elif kind == 1:
                self.on_floor(t, pl)
            elif kind == 2:
                self.apply_floor(t, pl)
            elif kind == 3:
                self.on_report(t, pl)
            elif kind == 4:
                self.on_tick(t)
            elif kind == 5:
                self.on_sweep(t)
        return self.summary()

    # ---- readouts -----------------------------------------------------------------------
    def summary(self):
        c = self.cfg
        g = dict(self.g)
        n = self.n_sent
        hol = np.asarray(self.hol) if self.hol else np.zeros(1)
        eps = np.asarray([e[1] for e in self.episodes]) if self.episodes else np.zeros(1)
        delivered = int(sum(self.got))
        # isolated holes: no other lost original within +-W seqs (policy independent, CRN)
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
        # per-packet HOL over non-hole packets (same set across candidates)
        arr, dlv, wh, got = self.arr, self.dlv, self.was_hole, self.got
        hs = 0.0
        for j in range(min(n, self.next_deliver)):
            if got[j] and not wh[j] and dlv[j] >= 0:
                hs += dlv[j] - arr[j]
        eps_pass = max((e[2] for e in self.episodes), default=0)
        unresolved = len(self.holes)
        out = dict(
            cell=c.cell, cand=c.cand, seed=c.seed, dur=c.dur, rho=c.rho, n_src=n, delivered=delivered,
            G_final=self.G, G_ratio=self.G / max(n, 1), one_minus_rho=1 - c.rho,
            hol_p50=float(np.percentile(hol, 50)), hol_p99=float(np.percentile(hol, 99)), hol_mean=float(hol.mean()),
            hol_sum_nonhole=hs, n_holes=len(self.tdet), unresolved=unresolved,
            stall_n=len(self.episodes), stall_p50=float(np.percentile(eps, 50)), stall_p99=float(np.percentile(eps, 99)),
            stall_total=float(eps.sum()), max_ep_spend=int(eps_pass),
            iso_n=int(len(iso)), iso_p50=float(np.median(iso)) if len(iso) else float("nan"),
            iso_mean=float(iso.mean()) if len(iso) else float("nan"),
            repair_per_delivered=g["repair_sends"] / max(delivered, 1),
            ctl_per_delivered=g["floors_ctl"] / max(delivered, 1),
            h_hat=self.n_reported / max(n, 1), orig_lost=len(self.orig_lost),
            gauges=g,
        )
        if self.S_hat is not None:
            out["S_hat_tmax"] = float(self.S_hat[0][-1])
            if self.cand.t_rho:
                out["T_rho_final"] = self.T_rho(self.t_end)
        if c.want_trace:
            out["_trace"] = self.trace
            out["_arr"], out["_dlv"], out["_wh"], out["_got"] = arr, dlv, wh, got
            out["_tdet"], out["_tfill"], out["_sent_t"] = self.tdet, self.tfill, self.sent_t
            out["_H"] = self.H_now()
        return out


def run(cfg: Cfg):
    return Sim(cfg).run()


def make_cfg(cell, cand, seed, topo="n1", **kw):
    topos = {"n1": (0.005,), "n1r50": (0.025,), "n2": (0.005, 0.025)}
    kw.setdefault("dur", CELL_DUR.get(cell, 20.0))
    return Cfg(cell=cell, cand=cand, seed=seed, owds=topos[topo], **kw)
