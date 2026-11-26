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


def staged_past_lifetime(T, age):
    """Proof stage 1, plan 1.1 gate: L1 violations, a payload staged (first send or copy) at age >= T.  A measurement, never a branch;
    named here so that the deny test keeps T out of every other comparison."""
    return age >= T


def dequeue_expired(T, age, m):
    """Stage 3b, three-knobs.md 13.2: at the dequeue a unit gives its slot to a younger one when it can no longer arrive in
    time.  m is the ack timeout R, an upper bound on the one-way delay.  The fourth place T is compared."""
    return age + m >= T


def protected(T, now, last_send, c, h, srtt_eff):
    """Stage 3c, three-knobs.md 15.3 and 15.6.1 item 1: L2 evaluated at the earliest resend still possible, max(now, last_send + c),
    with c the per-seq resend cooldown, h the hand-over time and srtt_eff / 2 the one-way delay estimate.  A seq that is not protected
    will never be resent, so the floor may pass it.  The fifth place T is compared."""
    return max(now, last_send + c) - h + srtt_eff / 2.0 < T


def trigger_delay(resent, c, srtt_path, p_reach):
    """Stage 3d, three-knobs.md 17.1: d(s), the delay from a seq's last send to its earliest possible next resend: the cooldown c
    (the engine's gate) once it has been resent, else its trigger, min(srtt of its path, the sweep's reach P_reach(s))."""
    return c if resent else min(srtt_path, p_reach)


def plant_T(cfg, T):
    """Stage 3d, three-knobs.md 17.5: the planted controls routed through the knob.  T_eff = plant_T(cfg, T) enters every sender law;
    the readouts use the declared T.  cfg.plant_T is None or (kind, p).  Named in the deny test (with plant_scale)."""
    if cfg.plant_T is None:
        return T
    kind, p = cfg.plant_T
    if kind == "STEP":
        return 2.0 * T if T < p else T
    if kind == "SPIKE":
        return 2.0 * T if T == p else T
    if kind == "KINK":
        return T * (1.0 + 25.0 * max(0.0, 1.0 - T / p))
    return T


def window_shares(ages, dt, thr, width=0.5, step=0.1):
    """Stage 3d, three-knobs.md 17.4: the share of the units handed over in each window [k step, k step + width) that were delivered
    within thr of hand-over (an expired or undelivered unit has age inf and counts as late).  ages is indexed by unit."""
    n = len(ages)
    cs = np.concatenate([[0], np.cumsum(ages <= thr)])
    out = []
    k = 0
    while True:
        lo = int(round(k * step / dt))
        hi = int(round((k * step + width) / dt))
        if hi > n:
            break
        out.append(float(cs[hi] - cs[lo]) / (hi - lo))
        k += 1
    return out


def recovery_time(ages, dt, thr, out_start, out_end, step=0.1, width=0.5, pre_from=0.5):
    """Stage 3d, three-knobs.md 17.4: recovery time after an outage, in seconds after its end, or (None, reason).
    L0 = the on-time share over hand-over [pre_from, out_start - 0.1]; sd0 = the standard deviation of the pre-outage window shares;
    theta = L0 - max(0.02, 3 sd0); t_rec = the start of the first window at or after out_end such that every window from it to the
    readable end (last hand-over - 2 s) has share >= theta."""
    n = len(ages)
    sh = window_shares(ages, dt, thr, width, step)
    a, b = pre_from, out_start - 0.1
    pre = [sh[k] for k in range(len(sh)) if k * step >= a - 1e-9 and k * step + width <= b + 1e-9]
    lo, hi = int(round(a / dt)), int(round(b / dt))
    if not pre or hi <= lo:
        return None, "no pre-outage level (the interval [%.3g, %.3g] holds no window)" % (a, b)
    L0 = float(np.sum(ages[lo:hi] <= thr)) / (hi - lo)
    theta = L0 - max(0.02, 3.0 * float(np.std(pre)))
    end = (n - 1) * dt - 2.0
    ks = [k for k in range(len(sh)) if k * step >= out_end - 1e-9 and k * step + width <= end + 1e-9]
    if not ks or sh[ks[-1]] < theta:
        return None, "not within the run"
    j = len(ks) - 1
    while j > 0 and sh[ks[j - 1]] >= theta:
        j -= 1
    return ks[j] * step - out_end, "ok"


def p_fec_normal(r_fec, eps, W, sigma2):
    """Proof stage 1, three-knobs-proof-plan.md 2.3: raptorpath-math p_fec_normal ported as shipped, including its cut (0 when r <= 0 or
    r (1 - eps) <= eps: recorded debt W22, the 0 -> 1/2 step).  Named in the deny test.  P_fec = Phi(sqrt(W) (r (1 - eps) - eps) / sqrt(eps (1 - eps) (r + sigma2)))."""
    if W <= 0.0 or eps <= 0.0 or eps >= 1.0 or r_fec <= 0.0:
        return 0.0
    num = r_fec * (1.0 - eps) - eps
    if num <= 0.0:
        return 0.0
    den = math.sqrt(eps * (1.0 - eps) * (r_fec + sigma2))
    if den < 1e-300:
        return 1.0
    return 1.0 - 0.5 * math.erfc(math.sqrt(W) * num / den / math.sqrt(2.0))


def residual_loss(eps, r_fec, W, sigma2):
    """The shed budget's fraction of the source count (shed.rs shed_allowed's budget_frac, residual_loss_after_fec): eps (1 - P_fec)."""
    return eps * (1.0 - p_fec_normal(r_fec, eps, W, sigma2))


def evict_unservable(window, n_sent, s):
    """Proof stage 1, plan 2.3: the engine's encoder window (MAX_WINDOW_SIZE = 200 symbols) holds the newest `window` sent seqs; a hole
    further behind can be neither served nor shed (rec-evict-blocker-unresolved, modelled as shipped).  Named in the deny test."""
    return n_sent - s > window


def force_drain_due(n_buffered, cap):
    """reorder.rs: the evict-policy buffer force-drains when it holds more than max_buffered entries.  Named in the deny test."""
    return n_buffered > cap


def force_drain_keep(cap):
    """reorder.rs force_drain_oldest drains until pending.len() <= max_buffered / 2."""
    return cap // 2


def delta_at_b(b):
    return DELTA_AUTO * 10.0 ** (-2.0 * math.log2(b))


# ---- stage 3e (three-knobs-stage3e-build.md 2): the repair laws as raptorpath-math and the engine ship them ----------------------
SALT_REPAIR = 7            # loss draw of a repair datagram (salts 0-6 are taken)
CODEC_RLC = 0.004          # paper 3.6: RLC codec overhead
R_MAX = 0.5                # engine max_overhead
W_CAP = 200                # sender MAX_WINDOW_SIZE
RATE_CADENCE_MAX = 0.005   # fec_rate.rs RATE_CADENCE_MAX_US
BASE_TAIL = 1e-5           # FecRateController target_tail_loss (paper 4.1)
BETA_DECAY = 0.995         # estimator.rs beta_decay
DELTA_BULK = 0.005


def normal_survival_as(z):
    """raptorpath-math normal_survival (Abramowitz & Stegun), ported exactly."""
    if z > 8.0: return 0.0
    if z < -8.0: return 1.0
    x = z / math.sqrt(2.0); ax = abs(x)
    t = 1.0 / (1.0 + 0.3275911 * ax)
    poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))))
    e = poly * math.exp(-ax * ax)
    return 0.5 * e if x >= 0.0 else 1.0 - 0.5 * e


def normal_quantile_as(p):
    """raptorpath-math normal_quantile (A&S 26.2.23), ported exactly."""
    if p <= 0.0: return -math.inf
    if p >= 1.0: return math.inf
    if abs(p - 0.5) < 1e-12: return 0.0
    sign, q = (-1.0, p) if p < 0.5 else (1.0, 1.0 - p)
    t = math.sqrt(-2.0 * math.log(q))
    num = 2.515517 + 0.802853 * t + 0.010328 * t * t
    den = 1.0 + 1.432788 * t + 0.189269 * t * t + 0.001308 * t * t * t
    return sign * (t - num / den)


def p_fec_normal_as(r, eps, W, s2):
    """raptorpath-math p_fec_normal with the A&S survival (the existing p_fec_normal, which uses erfc, stays for today's Realtime)."""
    if W <= 0.0 or eps <= 0.0 or eps >= 1.0 or r <= 0.0: return 0.0
    num = r * (1.0 - eps) - eps
    if num <= 0.0: return 0.0
    den = math.sqrt(eps * (1.0 - eps) * (r + s2))
    if den < 1e-300: return 1.0
    return 1.0 - normal_survival_as(math.sqrt(W) * num / den)


def z_for_tail_target(delta, eps):
    if eps <= 0.0: return -math.inf
    ratio = delta / eps
    if ratio >= 1.0: return -math.inf
    return normal_quantile_as(1.0 - max(ratio, 1e-15))


def r_star_with_z(eps, s2, W, z):
    if eps <= 0.0 or eps >= 1.0: return 0.0
    base = eps / (1.0 - eps)
    margin = z * math.sqrt(eps * s2 / (W * (1.0 - eps))) if W > 0.0 else 0.0
    return max(base + margin, 0.0)


def r_saturation(eps, s2, W, srtt, t_sym):
    ok = all(math.isfinite(v) for v in (eps, s2, W, srtt, t_sym)) and 0.0 < eps < 1.0 and s2 > 0 and W > 0 and srtt > 0 and t_sym > 0
    if not ok: return 1.0
    l_arq = 1.5 * srtt; b_hat = (s2 + 1.0) / 2.0
    best_r, best = 1.0, math.inf
    for i in range(199):
        r = 0.01 + 0.005 * i
        cost = (1.0 - p_fec_normal_as(r, eps, W, s2)) * l_arq + b_hat * t_sym * (1.0 + r) / (r * (1.0 - eps)) + 0.5 * (1.0 + r) * W * t_sym
        if cost < best: best, best_r = cost, r
    return best_r


def soft_saturate(rate, r_sat):
    s = 0.1 * r_sat
    if not (s > 0.0): return min(rate, r_sat)
    x = (r_sat - rate) / s
    sp = max(x, 0.0) + math.log1p(math.exp(-abs(x)))
    return max(r_sat - s * sp, 0.0)


def controller_rate_parts(p, s2, mean_burst, W, t_symbols, srtt, t_sym, tail, late):
    """raptorpath-math controller_rate with the mass term inert (no mass statistics), inner_feedback 0, the saturation cap on,
    max_overhead R_MAX, codec CODEC_RLC, completion exposure 0.  Returns (rate, raw before the cap, r_sat or None, (burst_bind, rmax_bind)); the flags say that r_burst exceeded the random-loss term and that the
    rate before the final clamp exceeded R_MAX (clamp bind gauges, registration 6)."""
    if p < 1e-10: return 0.0, 0.0, None, (False, False)
    d_eff = p if late else tail                              # late: p + (0.05 - p) * chi with chi = 0
    codec = CODEC_RLC * (1.0 - (1.0 - p) ** int(W)) if W > 0.0 else 0.0
    core = r_star_with_z(p, s2, W, z_for_tail_target(d_eff, p))
    rnd = core + codec if core > 0.0 else 0.0
    frac = min(max(1.0 - d_eff / p, 0.0), 1.0)
    burst = (max(mean_burst, 1.0) / t_symbols) * frac if t_symbols > 0.0 else 0.0
    raw = max(rnd, burst)
    r_sat = None
    rate = raw
    if t_sym > 0.0 and srtt > 0.0:
        r_sat = r_saturation(p, s2, W, srtt, t_sym)
        rate = soft_saturate(raw, r_sat)
    return min(max(rate, 0.0), R_MAX), raw, r_sat, (burst > rnd, rate > R_MAX)


def derive_window_parts(tail, eps, s2, srtt, rate):
    """raptorpath-math derive_window with latency_budget 0 (fec_rate.rs).  Returns (W after [16, 512], the value before that clamp)."""
    s2 = s2 if (math.isfinite(s2) and s2 >= 1.0) else 1.0
    budget = srtt if (math.isfinite(srtt) and srtt > 0.0) else 0.0
    w_lat = budget * rate if (math.isfinite(rate) and rate > 0.0 and budget > 0.0) else 512.0
    if math.isfinite(eps) and 0.0 < eps < 1.0:
        z = z_for_tail_target(tail, eps)
        w_over = z * z * s2 * (1.0 - eps) / (eps * 0.0625) if (math.isfinite(z) and z > 0.0) else 0.0
        w_bur = ((s2 + 1.0) / 2.0) / (eps * (1.0 - eps))
    else:
        w_over = w_bur = 0.0
    lo = min(w_bur, w_lat)
    w = min(max(w_over, lo), w_lat)
    return min(max(w, 16.0), 512.0), w


def bulkness(delta):
    a, b = math.log10(DELTA_AUTO), math.log10(DELTA_BULK)
    return min(max((a - math.log10(max(delta, 1e-12))) / (a - b), 0.0), 1.0)


def t_tail_of(delta):
    return min(max(BASE_TAIL * DELTA_AUTO / max(delta, 1e-12), 1e-9), 0.1)


def rate_law(p, srtt, rate, delta):
    """The rate law as the engine evaluates it at one operating point (registration 2.1, Appendix A): sigma2 = 1, mean burst 1,
    t_symbols = max(srtt * rate, 1), t_sym = 1 / rate, both terms of the beta mix always computed.
    Returns dict(r, W, W_pre, raw, r_sat, beta)."""
    tail = t_tail_of(delta)
    W, W_pre = derive_window_parts(tail, p, 1.0, srtt, rate)
    W = min(float(round(W)), float(W_CAP))
    tsy = max(srtt * rate, 1.0); tsm = 1.0 / rate if rate > 0.0 else 0.0
    a, raw, r_sat, flags = controller_rate_parts(p, 1.0, 1.0, W, tsy, srtt, tsm, tail, False)
    l, _, _, _ = controller_rate_parts(p, 1.0, 1.0, W, tsy, srtt, tsm, min(max(BASE_TAIL * 100.0, 1e-9), 0.1), True)
    beta = bulkness(delta)
    return dict(r=(1.0 - beta) * a + beta * l, W=W, W_pre=W_pre, raw=raw, r_sat=r_sat, beta=beta, burst_bind=flags[0], rmax_bind=flags[1])


def beta_upper(a, b, conf=0.95):
    """estimator.rs loss_rate_upper = beta_quantile(b, a, conf): the normal approximation of the Beta quantile, clamped to [0, 1].
    a = decayed received count, b = decayed lost count."""
    aa, bb = b, a                                            # beta_quantile(a=failures b, b=successes a)
    mean = aa / (aa + bb)
    var = (aa * bb) / ((aa + bb) ** 2 * (aa + bb + 1.0))
    return min(max(mean + normal_quantile_as(conf) * math.sqrt(var), 0.0), 1.0)


def span_edge_ok(edge, T, age, o):
    """Registration 2.3: does a member of age `age` pass the span's lifetime edge?  worth: a + o < T; age: a < T; none and evict: no lifetime
    term.  (floor and evict are applied as seq bounds in stage_repair.)"""
    if edge == "worth": return age + o < T
    if edge == "age": return age < T
    return True


def decoded_late(T, age):
    """Gauge: a decode after the lifetime.  A measurement, never a branch."""
    return age > T


# ---- Revision 4, step 1 (three-knobs-revision4.md 3.1, 3.2): the wait limit's inputs, as readouts only ---------------------------------
def wait_limit_K(delta, x, t_ov):
    """K = delta * (x - 1)+ * t_ov^2 [s^2]: the price of the bet, x = lambda_hat / mu_hat.  Zero at x <= 1."""
    return delta * max(x - 1.0, 0.0) * t_ov * t_ov


def wait_limit_tau(T, K, den):
    """tau = T - K / (T - R + o_hat) [s].  K finite, den positive: at T = inf, K / inf = 0.0 and the result is inf; at K = 0.0 the result is T exactly."""
    return T - K / den


def wait_limit_Kc(T, R, o):
    """The collapse threshold: K_c = (T - R)(T - R + o_hat); at K_c the effective margin T - R - K / (T - R + o_hat) is 0."""
    return (T - R) * (T - R + o)


def tau_below_t(tau, T):
    """Gauge: a tick at which the law would hold less than the lifetime.  A measurement, never a branch.  Named in EXEMPT."""
    return tau < T


class DHat:
    """The sender's delivery-rate estimate for source datagrams (paper 8.3's BtlBw anchor, BBR's delivery-rate sample restricted to source datagrams): the
    maximum over the last `window` rounds of (datagrams newly reported received / the round's duration), +inf unless all of those rounds qualified.  A round
    qualifies when the sender was flow-limited in it and no single pause in it lasted longer than R: a longer pause means overdue acks, no evidence."""

    def __init__(self, window=10):
        self.window = window
        self.rounds = deque(maxlen=window)

    def add_round(self, count, duration, flow_limited, max_pause, R):
        self.rounds.append((count / duration, flow_limited and not (max_pause > R)))

    def value(self):
        if len(self.rounds) < self.window or not all(ok for _, ok in self.rounds):
            return math.inf
        return max(max(s for s, _ in self.rounds), 1e-9)


def floor_repeat_needed(s_cands, last_key, rx_key, carry_vals, carry_keys, t_newest, R, now, overdue=None):
    """The repeat law of three-knobs-revision4.md 10.3 on one instant.  s_cands: the unresolved seqs in [A_known, F_ann), any order.  A standalone floor is sent iff some
    s has loss evidence (a datagram sent after s's last send has arrived: last_key[s] < rx_key; or silence: now >= t_newest + R) and no datagram carrying a floor above s has
    arrived (rx_key < carry_key(s), +inf if none yet).  carry_vals is the running maximum of the floor values carried, carry_keys the key of the first datagram carrying each.
    Returns (fire, s_star, by_timeout), s_star the highest evidenced seq.  The keys are send ordinals; with send stamps the same predicate suppresses a burst for good.
    overdue = (v, t): the newest data datagram that carried a floor sent it at t with value v (10.7): s is then only repeated for once now >= t + R, unless v <= s (nothing carried above s)."""
    silent = now >= t_newest + R
    star = None
    for s in sorted(s_cands, reverse=True):
        if last_key[s] < rx_key or silent:
            star = s
            break
    if star is None:
        return False, None, False
    for s in sorted(s_cands, reverse=True):
        if not (last_key[s] < rx_key or silent):
            continue
        i = bisect.bisect_right(carry_vals, s)
        ck = carry_keys[i] if i < len(carry_keys) else math.inf
        if not rx_key < ck:
            continue
        if overdue is not None and overdue[0] > s and now < overdue[1] + R:
            continue
        return True, s, not (last_key[s] < rx_key)
    return False, star, False


def engine_in_flight_horizon(srtt):
    """The engine's in-flight expiry horizon (scheduler/path.rs expire_in_flight, default): max(4 x SRTT, IN_FLIGHT_EXPIRY_MIN = 250 ms)."""
    return max(4.0 * srtt, 0.25)


def rb_hold(sim, now):
    """Amendment 3 (registration 15.3): is `now` inside RB's cap window, [outage start, outage end + A)?  The outage window is the cell's declared
    `dead` (an oracle: RB is a bracket, not a design).  rb_unit "sym": A counts first sends after the outage end.  Named in EXEMPT."""
    dd = sim.cfg.dead
    if dd is None:
        return False
    t0, t1 = dd[1], dd[1] + dd[2]
    if now < t0:
        return False
    if now < t1:
        return True
    if sim.cfg.rb_unit == "sym":
        return sim.fs_after_outage < sim.cfg.rb_A
    return now < t1 + sim.cfg.rb_A


def saturation_pressure(raw, r_sat):
    s = 0.1 * r_sat if r_sat else 0.0
    if not (s > 0.0): return 0.0
    x = (raw - r_sat) / s
    return 1.0 / (1.0 + math.exp(-x)) if x > -700 else 0.0


@dataclass
class Cfg:
    cell: str = "I1"
    sender: str = "S1c"            # S0 | S1 (verdict, no copy; test only) | S1c | S2 | S2c (stage 3: L1, L2; S2c adds a copy per verdict) | S3 | S3x (stage 3b: the S2 family on Cfg's stage 3b fields; S3x keeps the O exclusion) | S4 (stage 3c: S3 on the stage 3c fields)
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
    resend_gate: str = "owd"       # stage 3b: "evidence" gates every resend on evidence of loss (link cells); stage 3c: "cooldown" = the engine's per-seq cooldown max(srtt, 10 ms) (a seq with no copy yet is exempt, 15.6.1 item 5); "cooldown_all" = unregistered: no exemption
    deq_expiry: bool = False       # stage 3b: dequeue expiry (a + R >= T with a younger unit waiting)
    evidence_verdict: bool = False  # stage 3b: verdicts only on evidence of loss newer than the last send
    floor_law: str = "verdict"     # stage 3c: "protect" = the floor is the lowest seq the sender still means to deliver (three-knobs.md 15.3)
    floor_carrier: str = "header"  # stage 3c: "header" = the floor rides on every data datagram at zero link cost; "datagram" = at most one floor datagram per 2 ms (S4b)
    resend_brake: str = "none"     # stage 3c: "engine" = at most 10 copies per 5 ms window (15.4)
    verdict_evidence: str = "rev1"  # stage 3c: "feedback" = a verdict only for a seq a current report lists missing, with a later send received (15.1)
    trigger_law: str = "cooldown"  # stage 3d: "trigger" = protected() uses d(s): the cooldown for a resent seq, min(srtt of its path, P_reach) for a never-resent one (17.1)
    owd_est: str = "half"          # stage 3d: "fwd" = o_hat = srtt - RTprop/2 in L2 and protected() (17.1); "half" = srtt/2; stage 3e: "rel" = RTprop/2 + the relative forward queue the receiver measures (build 6.3)
    recheck_floor: bool = False    # stage 3d: before each copy is staged, test s >= F_ann against the current floor (17.1)
    sweep_form: str = "tail"       # stage 3d: "tail" = the whole-tail sweep; "blocker" = the engine's (only the cumulative blocker, measured srtt deadline) (17.3)
    rx_refresh: bool = False       # stage 3d: the receiver re-sends its report every clamp(2 srtt, 25, 100 ms) while it holds holes (17.3)
    plant_T: object = None         # stage 3d: None | (kind, p), kind STEP | SPIKE | KINK, routed through plant_T() into every sender law (17.5)
    paths: object = None           # stage 3d: None | list of dict(owd=, J=): the two-path model of 17.7 (path 0 is the fast path A)
    phi: float = 0.5               # stage 3d: share of first sends placed on path B
    reorder_rack: bool = False     # stage 3d: a report-driven hole is a candidate only if last_send + skew_hat < tau_rx (17.7)
    windows: bool = False          # stage 3d: the window readouts of 17.4 and 17.8
    budget: str = "rho"            # proof stage 1: "resid" = today's Realtime: the sender's own shed count may not exceed eps_hat (1 - P_fec) x the source count (shed.rs shed_allowed)
    floor_send: bool = True        # proof stage 1: False = a shed is silent: no floor datagram, repeated or not (today's Realtime)
    evict_window: float = math.inf  # proof stage 1: a hole more than this many seqs behind the send frontier is neither served nor shed (200 = the engine's encoder window)
    place: str = "stripe"          # proof stage 1: "dup" = every first send on path A plus a duplicate on path B (today's Realtime, emit_source.rs:508)
    r0e_max: float = math.inf      # proof stage 1: R0e's buffer cap: force-drain the oldest entries to half the cap when it holds more (reorder.rs)
    recv_eps: str = "engine"       # R0e's eps_hat: engine (per-batch EWMA, alpha 0.1, estimator.rs:92,103-110) | cum (cumulative missing fraction) | ewma (per-seq, alpha 1/256)
    repair: object = None          # stage 3e: None = no repair; dict(r="law" | "est" | float, edge="worth" | "age" | "floor" | "none" | "evict")
    repair_seam: object = None     # stage 3e, test only: "no_edge" disables the lifetime edge (the L1 positive control)
    o_bias: float = 0.0            # stage 3e robustness: added to the one-way estimate o_fwd (L2, protected, the worth edge)
    R_bias: float = 0.0            # stage 3e robustness: added to R in the dequeue rule only
    rev_q: float = 0.0             # stage 3e robustness: a standing reverse queue, seconds added to every report's flight
    tau_extra: tuple = ()          # stage 3e: extra on-time readouts (RT: (0.12,)), reported in out["rep3e"]["share_by"]
    rev4_gauge: bool = False       # revision 4, step 1: read lambda_hat, D_hat, t_ov, K, tau, spare as outputs (out["rev4"]); no decision reads them
    dhat_bias: float = 0.0         # revision 4, step 1: D_hat is read as D_hat * (1 - dhat_bias) in the gauges (the bias rows of 5.3)
    clamp: bool = False            # revision 4 step 2 (3.3): repair accrues min(r, max(spare, 0)), spare = (cwnd_sim - in_flight) / in_flight
    wait_limit: bool = False       # revision 4 step 2 (3.1): the dequeue expiry reads tau = T - K / (T - R + o_hat) in place of T
    floor_repeat: str = ""         # revision 4 section 10: "evidenced" = send a standalone floor only when the receiver may not know it (ordinals); "evidenced_overdue" (10.7) also waits until the newest covering data carrier is overdue (R); "" = today's rule
    elig: str = ""                 # revision 4 section 11.2: "rack" = the blocker sweep resends a seq only once its last send is overdue: deadline max(last_send(b) + max(P, R), last_fire + P), P_reach = max(P, R)
    inflight_to_rule: str = ""     # revision 4 step 2: "engine" = in_flight expires at max(4 x SRTT, 250 ms) (scheduler/path.rs expire_in_flight, copa.rs:307); "" = off
    inflight_to: float = 0.0       # revision 4 step 2: a datagram unreported for inflight_to x R is declared lost and leaves in_flight (RFC 9002's time threshold is 9/8); 0 = reports only
    send_gate: bool = False        # revision 4 step 2 (3.3): no payload is staged while in_flight >= cwnd_sim (the engine's available() = cwnd - in_flight > 0)
    eq: bool = False               # revision 4 step 2 (3.3): today's Realtime behind the engine's front queue (eq_cap symbols, evicts the oldest, drains at the link rate)
    eq_cap: int = 8192             # 4 MiB of 512 B symbols
    cwnd_sim: float = 0.0          # revision 4 step 2: the clamp's and the gauge's cwnd in datagrams; 0 = the cell's store pause pause_B
    rate2: float = 0.0             # revision 4 step 2 (OV-end): the source rate after rate2_t seconds; 0 = constant rate (work-conserving source only)
    rate2_t: float = 0.0
    rb_A: float = 3.0              # stage 3e amendment 3, arm RB (repair r = "rb"): r is held at R_MAX from the outage start until rb_A after its end
    rb_unit: str = "s"             # "s": rb_A seconds after the outage end; "sym": rb_A first sends after it


def ge_cell(cell, seed, slots, dur_slots_unused=None):
    c = CELLS[cell]
    return [toy.ge_states(seed * 4 + 0, slots, c["mean"] / c["pbad"], c["burst"])]


class Delay:
    """arrival = tau + at(tau); at(t) = owd + J(t) + Q(t).  J piecewise linear through U[0,J] knots 2J apart
    (slope in [-1/2, 1/2], so the path is FIFO for every datagram); Q is the sawtooth of 7.2."""

    def __init__(self, seed, owd, J, q, horizon, path=0):
        self.owd, self.J, self.q = owd, J, q
        if J > 0:
            self.kappa = 2.0 * J
            n = int(horizon / self.kappa) + 4
            self.K = [J * u01(seed, k, 0, path, SALT_KNOT) for k in range(n)]

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
        self.rtt = 2 * c.owd                           # RTprop, exact (the resend path's, path A)
        self.paths = c.paths
        self.npath = len(c.paths) if c.paths is not None else 1
        self.Te = plant_T(c, c.T)                      # T_eff (17.5): enters every sender law; readouts use the declared T
        self.b = b_of_delta(c.delta)
        self.D_rt = min(self.b * self.rtt, 2 * self.rtt)
        self.dt = 1.0 / c.rate
        self.n1 = int(round(c.rate2_t * c.rate)) if c.rate2 else 0           # units handed over before the rate changes
        self.dt2 = 1.0 / c.rate2 if c.rate2 else self.dt
        self.B_sim = c.cwnd_sim or float(c.pause_B)
        self.cnt_on = c.rev4_gauge or c.clamp or c.wait_limit or c.send_gate                # the in_flight count and the D-hat rounds are kept
        if c.n_total:
            self.dur = c.n_total / c.rate
            self.t_end = c.t_limit
        else:
            self.dur = c.dur
            self.t_end = c.dur + c.grace
        self.units_total = c.n_total if c.n_total else int(c.dur * c.rate)
        self.nmax = self.units_total + 16
        self.B = c.pause_B
        rtt_sw = self.rtt if c.paths is None else 2 * max(p["owd"] for p in c.paths)       # 17.7: the largest path rtt_base
        self.sweep_P = min(max(2 * rtt_sw, c.sweep_lo), c.sweep_hi)
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
        if c.paths is None:
            self.delays = [self.delay]
        else:
            self.delays = [Delay(c.seed, p["owd"], p["J"], c.q, self.t_end + 2.0, i) for i, p in enumerate(c.paths)]
            self.delay = self.delays[0]
        self.est = HEst(c.W, c.paths is None)          # non-strict across paths (17.7)
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
        self.s2fam = c.sender in ("S2", "S2c", "S3", "S3x", "S4", "S5")
        self.s3 = c.sender in ("S3", "S3x", "S4", "S5")
        self.resume_on_verdict = c.sender not in ("S3", "S4", "S5")       # S3, S4, S5: the pause is released by receiver feedback only (13.1)
        self.O_ctl = self.O_eff if self.s2fam else self.O
        self.paused = False
        self.srtt = None
        self.rttvar = 0.0
        self.rtt_unarrived = 0                       # stage 3d amendment gauge: RFC rhi samples skipped because the seq had not arrived
        self.rtt_unarrived_other = 0                 # counted only, behaviour unchanged: old-estimator (A - 1) and per-path samples on a seq not arrived at report time
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
        # stage 3d state
        self.psrtt = [None] * self.npath               # per-path RFC 6298 estimators (17.7)
        self.prttvar = [0.0] * self.npath
        self.rhi_pk = [-1] * self.npath                # sender: the newest seq per path the receiver reported
        self.rhi_path = [-1] * self.npath              # receiver: the newest seq received per path
        self.path_of = bytearray(self.nmax)            # the path of each seq's first send
        self.last_arr_l = [-1.0] * self.npath
        self.bl_dl = None                              # blocker sweep: the armed deadline
        self.bl_fire = -math.inf                       # blocker sweep: the last fire
        self.last_rep_t = -math.inf                    # receiver: the last report sent (refresh)
        self.first_deq_exp = None
        # stage 3c state
        self.rs_age = {}
        self.F_ann = 0                               # the announced floor (15.3): no copy of a seq below it is ever staged
        self.lu = 0                                  # lowest unresolved seq as of the last report (sender state)
        self.flag_sum = 0.0
        self.flag_n = 0
        self.copy_win = {}                           # copies sent per 5 ms window (the brake's counter)
        self.fl_win = -1                             # 2 ms window of the last floor datagram (carrier "datagram")
        self.win = {k: {} for k in ("first", "resend", "braked", "cool", "busy", "repair")}
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
        # stage 3e sender state (build 4.1); nothing reads it when repair is None
        self.owed = 0.0
        self.n_rep = 0
        self.t_eval = -math.inf
        self.r_cur = 0.0
        self.W_cur = float(W_CAP)
        self.law_last = None
        self.fs_times = deque()                    # first-send times, for rate_hat
        self.t_first = None
        self.beta_a = 1.0                          # the estimated-loss arm (estimator.rs: start at 1, 1)
        self.beta_b = 1.0
        self.eps_est = beta_upper(1.0, 1.0)
        self.astar_cur = 1
        self.dh = DHat()                           # revision 4 step 1 (gauge only)
        self.fr = c.floor_repeat in ("evidenced", "evidenced_overdue") and c.floor_law == "protect"      # the new model's floor repeat (revision 4 section 10)
        self.ord_ctr = 0                           # one ordinal per datagram sent (first sends, copies, repairs, floors): RFC 9002's packet number
        self.ord_time = []
        self.fr_od = c.floor_repeat == "evidenced_overdue"
        self.dcar_t = None                         # the newest DATA datagram (first send, copy, repair) that carried a floor, its send instant and carried value (10.7)
        self.dcar_v = None
        self.elig_rack = c.elig == "rack"
        self.bl_reach = 0.0                        # the reach used at the last blocker arming (the elig gauge's check)
        self.ord_ps = {}                           # ps (the per-datagram index of data and floor sends) -> ordinal
        self.ord_rep = {}                          # repair id -> ordinal
        self.last_ord = {}                         # seq -> ordinal of its last send
        self.rx_ord_cur = -1                       # receiver: the largest ordinal received, echoed in every report (rep[14])
        self.rx_ord = -1                           # sender: the same, as last reported
        self.carry_vals = []                       # the running maximum of the floor values carried, and the ordinal of the first datagram carrying each
        self.carry_ords = []
        self.t_newest = -math.inf                  # time of the newest non-floor send
        self.fq = deque() if c.eq else None        # today's Realtime behind the engine's front queue (revision 4 step 2)
        self.fq_busy = False
        self.sends_n = 0                           # every datagram sent (first sends, copies, repairs), in send order (gauge only)
        self.sends_cov = 0                         # the first sends_cov of them are covered by a report
        self.send_idx = []                         # send_idx[s] = position of seq s's first send in that order
        self.send_times = []                       # send time of each, in send order (the time threshold's bisect)
        self.rd_t0, self.rd_n0, self.rd_new, self.rd_flow, self.rd_maxpause = 0.0, 0, 0, False, 0.0
        self.t_ov0 = 0.0
        self.r4 = dict(ticks=0, ov=0, tau_lt_T=0, den_bind=0, floor_n=0, deq_early=0, dhat_inf=0, collapse_t=None, in_flight_max=0, spare_n=0, spare_bind=0,
                       dhat_sum=0.0, dhat_n=0, ratios=[], tau_min=math.inf, rounds=0, spare_bins={}, over_B=0, elig_n=0, elig_R_bind=0, elig_viol=0)
        self.fs_after_outage = 0                   # first sends after the outage end (RB with rb_unit "sym")
        self.qf_known = 0.0
        # stage 3e receiver state (build 5.1)
        self.dec_flag = bytearray(self.nmax)
        self.qd_first = bytearray(self.nmax)       # first send dropped at the link queue (gauge only: splits the decoded units by cause)
        self.raw_new = 0                           # source datagrams newly received since the last report (first sends and copies, once; the sender's D_hat counts them)
        self.eqs = {}                              # eq id -> set of unknown seqs
        self.eq_of = {}                            # unknown seq -> set of eq ids
        self.eq_meta = {}                          # eq id -> (repair index j, send time)
        self.raw_rcv = 0                           # first-send fates since the last report (the estimated-loss arm)
        self.raw_lost = 0
        self.drel = deque()                        # (t, d_rel) monotone minimum for owd_est "rel", a 10 s window (the RTprop window, 17.1)
        self.qf = 0.0
        self.n_eq = 0
        self.g = dict(verdicts=0, verdict_copies=0, floors=0, floor_rep=0, repair_sends=0, serves=0, refusals=0,
                      age_gate_fail=0, n_dec=0, gate_cold=0, passes=0, pass_floor=0, pass_r0e=0, late=0, ooo_loss=0,
                      dup=0, p1_viol=0, p1_worst=0, p1_checks=0, pause_events=0, max_O=0, rel_ho=0,
                      r0e_budget_open=0, r0e_fallback=0, r0e_expiries=0, fh_late=0, fifo_viol=0, seed_verdicts=0, n_dec_seed=0, r0e_fallback_open=0, pass_r0e_fb=0, r0e_entries_lengthened=0, pass_r0e_lengthened=0, late_floor=0, late_r0e=0,
                      l2_fail=0, rel_by_floor=0, probes=0, queue_drop=0, queue_drop_in=0, queue_drop_out=0,
                      resend_by_R=0, resend_by_report=0, resend_by_sweep=0,
                      evict_skip=0, force_drains=0, dup_first=0, l1_viol=0,
                      commit_viol=0, commit_skip=0, skip_below_floor=0, late_floor_ge1=0, l2_skip=0, bl_fires=0, refresh_reports=0, floor_carried=0, resend_braked=0, hull_upd=0, hull_bind=0,
                      law_reads=0, W_floor_bind=0, W_cap_bind=0, sat_pressure_sum=0.0, W_sum=0.0, r_sum=0.0, rep_credits=0, owed_max=0.0,
                      astar_floor_bind=0, D_cap_bind=0, astar_W_bind=0, delta_bind=0, rep_drop_edge=0, rep_drop_young=0, rep_cut_edge=0,
                      l1_viol_rep=0, worth_viol_rep=0, rep_sent=0, rep_members_sum=0, rep_qdrop=0, rep_lost=0, eps_est_sum=0.0, eps_est_n=0,
                      dup_after_dec=0, rep_redundant=0, rep_innov=0, dec_comp_max=0, rep_used=0, rep_late_only=0, dec_units=0, dec_on_time=0,
                      dec_late=0, dec_after_release=0, late_floor_dec=0, rmax_bind=0, burst_bind=0, first_qdrop=0, dec_units_qdrop=0)
        self.ser = {k: {} for k in ("r", "W", "n", "A", "nA", "dec")}          # per-1 s series of stage 3e (registration 6): sums and counts by second
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
        self.shed_budget = 0.0                      # proof stage 1: budget_frac, refreshed at each source emission (0 before the first estimate)
        if self.fr:
            for k in ("rep_suppressed", "rep_fired_by_timeout", "rep_fired_by_knowledge", "rep_supp_viol", "rep_checks", "rep_fire_carrier_young", "rep_fire_carrier_overdue", "rep_fire_carrier_none", "rep_fire_viol_carrier"):
                self.g[k] = 0
        if c.eq:
            self.g["fq_evict"] = 0
            self.g["fq_max"] = 0
        if c.clamp:
            self.g["clamp_n"] = 0
            self.g["r_eff_over_spare"] = 0.0

    # ---- helpers --------------------------------------------------------------------------
    def ht(self, u):
        """Hand-over time of unit u: u / rate, or u / rate until n1 units and then u / rate2 (OV-end)."""
        if not self.n1 or u <= self.n1:
            return u * self.dt
        return self.n1 * self.dt + (u - self.n1) * self.dt2

    def hand_count(self, x):
        """Units handed over by time x."""
        if x < 0.0:
            return 0
        if not self.n1:
            return min(self.units_total, int(x / self.dt) + 1)
        if x <= self.n1 * self.dt:
            return min(self.units_total, int(x / self.dt + 1e-9) + 1)
        return min(self.units_total, self.n1 + int((x - self.n1 * self.dt) / self.dt2 + 1e-9) + 1)

    def push(self, t, kind, payload):
        self.ctr += 1
        heapq.heappush(self.heap, (t, self.ctr, kind, payload))

    def lost(self, seq, copy, t, salt=SALT_LOSS, path=0):
        dd = self.cfg.dead
        if dd is not None and seq >= dd[0] and dd[1] <= t < dd[1] + dd[2]:
            return True
        u = u01(self.cfg.seed, seq, copy, path, salt)
        if self.loss_kind == "iid":
            return u < (self.p0 if copy == 0 and salt == SALT_LOSS else self.pc)
        if self.loss_kind == "ge":
            i = int(t * self.cfg.rate)
            if i >= len(self.ge):
                i = len(self.ge) - 1
            return bool(self.ge[i]) and u < self.pbad
        return False

    def refresh_shed_budget(self):
        """Today's Realtime refreshes the budget at each source emission.  eps_hat is the sender's loss estimate: the cell's first-send
        loss (Q21's true-loss default, a declared idealisation; the estimated-loss arm reads eps_est), and 0 until the first RTT sample.
        With repair the budget is eps_hat (1 - P_fec(r, A*)) as the engine forms it (emit_source.rs:629-642); with repair off, eps_hat."""
        if self.cfg.budget != "resid":
            return
        if self.srtt is None:
            self.shed_budget = 0.0
        elif self.loss_kind == "iid":
            rp = self.cfg.repair
            eps = self.eps_est if (rp is not None and rp["r"] == "est") else self.p0
            r = self.r_cur if rp is not None else 0.0
            self.shed_budget = residual_loss(eps, r, float(self.astar_cur) if rp is not None else 0.0, 1.0)
        else:
            raise NotImplementedError("budget resid needs the cell's loss as eps_hat; only iid cells")

    def shed_left(self):
        """shed.rs shed_allowed: shed_total + 1 <= budget_frac x src_total, as the number of further sheds allowed."""
        return int(self.shed_budget * self.n_sent + 1e-9) - len(self.verdicted)

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

    def srtt_p(self, p):
        """The smoothed RTT of path p (17.7); on one path the pooled srtt with the RFC seed."""
        if self.paths is None:
            return self.srtt_eff()
        return 0.333 if self.psrtt[p] is None else self.psrtt[p]

    def o_fwd(self):
        """17.1: the one-way delay of a resend, srtt - RTprop/2 on the resend path (path A), assuming the queueing is on the data direction."""
        if self.cfg.owd_est == "rel":                # stage 3e: the one-way delay as RTprop/2 plus the forward queue the receiver measured
            return self.rtt / 2.0 + self.qf_known + self.cfg.o_bias
        return self.srtt_p(0) - self.rtt / 2.0 + self.cfg.o_bias

    def o_hat(self):
        """One-way delay estimate used by L2: the exact owd for S2 (stage 3); srtt/2 with the RFC seed for S3; srtt - RTprop/2 (17.1) with owd_est "fwd"."""
        if self.s3 and self.cfg.rtt_est == "rfc":
            if self.cfg.owd_est in ("fwd", "rel"):
                return self.o_fwd()
            return (0.333 if self.srtt is None else self.srtt) / 2.0
        return self.rtt / 2

    def rtt_sample_p(self, p, r):
        """RFC 6298 on path p (17.7); the pooled srtt is the maximum over paths (an unsampled path reads the 333 ms seed), rttvar the maximum."""
        if self.psrtt[p] is None:
            self.psrtt[p], self.prttvar[p] = r, r / 2.0
        else:
            self.prttvar[p] = 0.75 * self.prttvar[p] + 0.25 * abs(self.psrtt[p] - r)
            self.psrtt[p] = 0.875 * self.psrtt[p] + 0.125 * r
        self.srtt = max(self.srtt_p(i) for i in range(self.npath))
        self.rttvar = max(self.prttvar)

    def blocker_period(self):
        """17.3: clamp(2 srtt_pooled, 25 ms, 100 ms), recomputed from the measured srtt."""
        return min(max(2.0 * self.srtt_eff(), self.cfg.sweep_lo), self.cfg.sweep_hi)

    def sweep_period(self):
        """The sweep period: fixed from the base RTT under the whole-tail sweep, the measured blocker period under the blocker sweep."""
        return self.sweep_P if self.cfg.sweep_form == "tail" else self.blocker_period()

    def sweep_reach(self, s):
        """P_reach(s) (17.1), read by the floor law and by the candidate builder alike: the age at which the sweep first makes s a
        candidate.  Whole-tail sweep: the period for every seq.  Blocker sweep: the period for the cumulative blocker, never for any other."""
        if self.cfg.sweep_form == "tail" or s == self.A_known:
            if self.elig_rack and self.cfg.sweep_form == "blocker":
                return max(self.sweep_period(), self.R_now())
            return self.sweep_period()
        return math.inf

    def d_of(self, s):
        """17.1 d(s) of a seq: the cooldown once resent, else its trigger."""
        return trigger_delay(self.copies[s] > 0, self.cooldown(), self.srtt_p(self.path_of[s]), self.sweep_reach(s))

    def prot_seq(self, s, now):
        """protected() of one seq under the configured laws (the same expression update_floor scans with)."""
        c = self.cfg
        se = self.srtt_eff() if c.owd_est == "half" else 2.0 * self.o_fwd()
        d = self.d_of(s) if c.trigger_law == "trigger" else self.cooldown()
        return protected(self.Te, now, self.last_send[s], d, self.h_seq[s], se)

    def srtt_eff(self):
        """The RFC 6298 smoothed RTT with the 333 ms seed before the first sample, for every arm alike (the cooldown and the floor)."""
        return 0.333 if self.srtt is None else self.srtt

    def cooldown(self):
        """c: the earliest resend after the last send.  Link cells (resend_gate "cooldown"): the engine's per-seq cooldown max(srtt, 10 ms)
        (recovery_laws.rs retx_cooldown_us); elsewhere the toy's own gate owd + J (stages 1 to 3b)."""
        if self.cfg.resend_gate in ("cooldown", "cooldown_all"):
            return max(self.srtt_eff(), 0.010)
        return self.cfg.owd + self.cfg.J

    def wadd(self, name, t, v=1):
        w = self.win[name]
        k = int(t)
        w[k] = w.get(k, 0) + v

    def update_floor(self, now):
        """15.6.1 item 2: F_ann = the lowest seq the sender still means to deliver, a running maximum.  Scan upward from
        max(F_ann, A_known) past every seq that is known received, verdicted or not protected; stop at the first unresolved,
        unverdicted, protected seq or at n_sent."""
        c = self.cfg
        s = max(self.F_ann, self.A_known)
        n = self.n_sent
        Mk, ver, rk, ls, hs = self.M_known, self.verdicted, self.rhi_known, self.last_send, self.h_seq
        se, cd = self.srtt_eff(), self.cooldown()
        if c.owd_est != "half":
            se = 2.0 * self.o_fwd()                  # protected() reads srtt_eff / 2 as its one-way term (17.1)
        Te = self.Te
        if c.trigger_law == "trigger":
            d_of = self.d_of
            while s < n:
                if (s in Mk or s > rk) and s not in ver and protected(Te, now, ls[s], d_of(s), hs[s], se):
                    break
                s += 1
        else:
            while s < n:
                if (s in Mk or s > rk) and s not in ver and protected(Te, now, ls[s], cd, hs[s], se):
                    break
                s += 1
        g = self.g
        g["hull_upd"] += 1
        if self.lu < self.F_ann and self.lu < n and self.prot_seq(self.lu, now):
            g["hull_bind"] += 1                      # the new minimum lies below the old floor: the running maximum absorbed it
        if s > self.F_ann:
            self.F_ann = s

    def carry_floor(self, a):
        """Carrier "header" (15.6.1 item 3): a delivered datagram arriving at a carries F_ann when some unresolved seq lies below it.
        The floor event follows the data event at the same instant and takes no link slot and no loss draw."""
        if self.cfg.floor_law == "protect" and self.cfg.floor_carrier == "header" and self.F_ann > self.lu:
            self.push(a, 1, (self.F_ann, None))
            self.g["floor_carried"] += 1

    def floor_datagram(self, now):
        """Carrier "datagram" (S4b): at most one floor datagram per 2 ms window, when F_ann has risen since the last one."""
        if self.cfg.floor_law == "protect" and self.cfg.floor_carrier == "datagram":
            w = int(now / 0.002)
            if w != self.fl_win and self.F_ann > self.A_known and self.F_ann > self.last_floor:
                self.fl_win = w
                self.last_floor = self.F_ann
                self.send_floor(now, self.F_ann)

    def fb_evidence(self, s):
        """15.1: a verdict only for a seq a current report lists missing and that a later send has overtaken (RACK's time test)."""
        ls = self.last_send[s]
        return s in self.M_known and self.last_gen > ls and ls < self.tau_rx

    def evidence(self, s, now):
        """13.1: evidence of loss newer than the last send of s: a datagram sent later has arrived, or s is overdue (R)."""
        ls = self.last_send[s]
        return ls < self.tau_rx or now >= ls + self.R_now()

    def fq_submit(self, now, fin):
        """Today's Realtime behind the engine's queue (3.3): a datagram handed to the front FIFO; when it is full the oldest queued one is evicted, silently.
        fin(a, dep) runs when the datagram leaves the queue onto the link (or fin(None, now) when evicted)."""
        q = self.fq
        if len(q) >= self.cfg.eq_cap:
            self.g["fq_evict"] += 1
            q.popleft()(None, now)
        q.append(fin)
        if len(q) > self.g["fq_max"]:
            self.g["fq_max"] = len(q)
        if not self.fq_busy:
            self.fq_busy = True
            self.push(now, 12, None)

    def on_fq(self, now):
        fin = self.fq.popleft()
        a, dep = self.fwd(now)
        fin(a, dep)
        if self.fq:
            self.push(now + self.link_sv, 12, None)
        else:
            self.fq_busy = False

    def fwd(self, t, path=0):
        """A forward send at t.  Returns (arrival, departure); arrival is None when the bottleneck queue drops it (13.7.1 item 6).
        With no link: arrival = t + delay.at(t), departure = t."""
        lk = self.cfg.link
        if lk is None:
            return t + self.delays[path].at(t), t
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
        w0, w1 = int(d - self.link_sv), int(d)
        if w0 == w1:
            self.wadd("busy", d, self.link_sv)
        else:
            self.wadd("busy", d - self.link_sv, w1 - (d - self.link_sv))
            self.wadd("busy", d, d - w1)
        self.link_free = d
        return d + self.delay.at(d), d

    def fifo_note(self, a, path=0):
        la = self.last_arr_l
        if a < la[path] - 1e-12:
            self.g["fifo_viol"] += 1
        else:
            la[path] = a

    # ---- source ---------------------------------------------------------------------------
    def emit(self, t):
        c = self.cfg
        # L1 at the intake: units that have outlived T before they could be sent are dropped, never get a seq
        while self.u_next < self.units_total and intake_expired_fn(self.Te, t - self.ht(self.u_next)):
            self.intake_expired += 1
            self.u_next += 1
        if c.deq_expiry:                                         # 13.2: expire the head at the dequeue when a younger unit waits (after L1)
            te_dq = self.tau_now(t)[0] if c.wait_limit else self.Te      # revision 4: the wait limit tau (= T exactly at x <= 1, +inf at T = inf)
            while (self.u_next + 1 < self.units_total
                   and self.ht(self.u_next + 1) <= t
                   and dequeue_expired(te_dq, t - self.ht(self.u_next), self.R_now() + c.R_bias)):
                if self.first_deq_exp is None:
                    self.first_deq_exp = t
                self.intake_expired_deq += 1
                self.u_next += 1
        if c.rev4_gauge and c.deq_expiry:
            self.rev4_tick(t)
        if c.n_total and self.u_next >= self.units_total:        # streaming mode ends on nxt >= dur (float edge: stage 1 sends one more)
            self.src_done = True
            self.src_next = math.inf
            self.advance(t)
            return
        s = self.n_sent
        if self.trace is not None:
            self.deq_log.append((t, self.u_next, t - self.ht(self.u_next), self.R_now(),
                                 self.u_next + 1 < self.units_total and self.ht(self.u_next + 1) <= t))
        if self.oe is not None and self.first_emit_oe is None and t >= self.oe:
            self.first_emit_oe = t
        self.h_seq[s] = self.ht(self.u_next)
        self.u_next += 1
        self.n_sent += 1
        self.sent_t[s] = t
        self.last_send[s] = t
        ordv = None
        if self.fr:
            ordv = self.next_ord(t)
            self.last_ord[s] = ordv
            self.t_newest = t
        if self.cnt_on:
            self.send_idx.append(self.sends_n)
            self.send_times.append(t)
            self.sends_n += 1
        if staged_past_lifetime(self.Te, t - self.h_seq[s]):
            self.g["l1_viol"] += 1                   # gate: no payload is staged at age >= T
        self.refresh_shed_budget()
        dup = c.place == "dup" and self.paths is not None
        pth = 0
        if self.paths is not None and not dup and u01(c.seed, s, 0, 0, SALT_PLACE) < c.phi:      # 17.7: first sends go on B with probability phi
            pth = 1
            self.path_of[s] = 1
        def fin_a(a, dep):
            if a is None:                            # dropped at the bottleneck queue (or evicted from the front queue): no arrival, no loss draw
                self.qd_first[s] = 1
                self.g["first_qdrop"] += 1
                self.atilde[s] = t + self.delays[pth].at(t)
            else:
                self.atilde[s] = a
                self.fifo_note(a, pth)

        deferred = self.fq is not None
        a = dep = None
        if not deferred:
            a, dep = self.fwd(t, pth)
            fin_a(a, dep)
        if self.trace is not None:
            self.trace.append((round(t, 9), s, 0, 0, 0))
        if c.floor_law == "protect":
            self.update_floor(t)
            if self.fr:
                self.note_carry(ordv)
            self.flag_sum += self.n_sent - self.F_ann
            self.flag_n += 1

        def fin_b(a, dep):
            ps = self.ps_ctr
            self.ps_ctr += 1
            if self.fr:
                self.ord_ps[ps] = ordv
            first_lost = a is None or self.lost(s, 0, dep, SALT_LOSS, pth)
            if not first_lost:
                self.push(a, 0, (s, 0, t, ps, pth))
                self.carry_floor(a)
            dup_ok = False
            if dup:                                  # today's Realtime: a duplicate first send on the second path (a recorded hint debt)
                a2, dep2 = self.fwd(t, 1)
                if a2 is not None:
                    self.fifo_note(a2, 1)
                ps2 = self.ps_ctr
                self.ps_ctr += 1
                self.g["dup_first"] += 1
                if a2 is not None and not self.lost(s, 0, dep2, SALT_LOSS, 1):
                    self.push(a2, 0, (s, 0, t, ps2, 1))
                    dup_ok = True
            if first_lost and not dup_ok:
                self.orig_lost.append(s)

        if deferred:
            self.fq_submit(t, lambda a, dep: (fin_a(a, dep), fin_b(a, dep)))
        else:
            fin_b(a, dep)
        if c.repair is not None:                     # stage 3e (build 4.3): repair credit accrues at each first send; at r = 0 nothing is pushed or drawn
            if self.t_first is None:
                self.t_first = t
            self.fs_times.append(t)
            if c.rev4_gauge:
                fl = self.in_flight_now(t)
                sp = (self.B_sim - fl) / fl if fl > 0 else math.inf
                self.r4["spare_n"] += 1
                self.r4["spare_bind"] += sp < self.r_cur
                bn = self.r4["spare_bins"].setdefault(int(t), [0, 0])      # per 1 s: first sends, first sends at which spare < r
                bn[0] += 1
                bn[1] += sp < self.r_cur
            if c.dead is not None and t >= c.dead[1] + c.dead[2]:
                self.fs_after_outage += 1
            if t - self.t_eval >= min(RATE_CADENCE_MAX, self.srtt_eff() / 4.0):
                self.read_law(t)
            if c.clamp:                              # the engine's clamp: r_eff = min(r, max(spare, 0)); nothing changes at r = 0
                fl = self.in_flight_now(t)
                sp = (self.B_sim - fl) / fl if fl > 0 else math.inf
                r_eff = min(self.r_cur, max(sp, 0.0))
                self.g["clamp_n"] += r_eff < self.r_cur
                self.g["r_eff_over_spare"] = max(self.g["r_eff_over_spare"], r_eff - max(sp, 0.0))
                self.owed += r_eff
            else:
                self.owed += self.r_cur
            if self.owed >= 1.0:
                self.owed -= 1.0
                self.g["rep_credits"] += 1
                self.stage_repair(t)
            if self.owed > self.g["owed_max"]:
                self.g["owed_max"] = self.owed
        if self.s2fam and c.verdict_evidence not in ("feedback", "none", "feedback_tail", "feedback_tail_any"):      # the per-seq verdict timer (at +inf it never fires); none under feedback verdicts
            self.push(max(t, self.h_seq[s] + self.Te - self.o_hat()), 8, s)
        self.wadd("first", t)
        self.floor_datagram(t)
        if s == 0 and c.tail_elig == "R":            # the probe timer (13.4): exactly one pending event of kind 9
            self.push(t + self.R_now(), 9, None)
        if self.bl_dl is None and c.sweep_form == "blocker":
            self.arm_blocker(t)
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
        elif o >= self.B or self.gate_block(t):
            self.paused = True
            self.pause_t0 = t
            self.g["pause_events"] += 1
            self.src_next = math.inf
        elif c.src_model == "wc":                    # work-conserving: a waiting backlog is sent at once, else the next unit at its own time
            self.src_next = max(t, self.ht(self.u_next))
        else:
            self.src_next = nxt

    def send_copy(self, now, s):
        """Stage a copy of s.  Returns False when the resend budget (15.4) refuses it; nothing is mutated then."""
        c = self.cfg
        if c.recheck_floor and c.floor_law == "protect":         # 17.1: the commitment is absolute: re-test against the current floor
            self.update_floor(now)
            if s < self.F_ann:
                self.g["skip_below_floor"] += 1
                return False
        w = int(now / 0.005)
        if c.resend_brake == "engine" and self.copy_win.get(w, 0) >= 10:
            self.g["resend_braked"] += 1
            self.wadd("braked", now)
            return False
        self.copy_win[w] = self.copy_win.get(w, 0) + 1
        self.wadd("resend", now)
        if staged_past_lifetime(self.Te, now - self.h_seq[s]):
            self.g["l1_viol"] += 1                   # gate: no payload is staged at age >= T
        if c.floor_law == "protect" and s < self.F_ann:
            self.g["commit_viol"] += 1               # the commitment seam: a copy below an announced floor (15.3)
        k = self.copies[s] + 1
        self.copies[s] = k
        if self.elig_rack and self.trig == "sweep" and s == self.A_known and now < self.last_send[s] + self.bl_reach - 1e-9:
            self.r4["elig_viol"] += 1                # a timer-driven copy of the blocker staged before its last send + max(P, R)
        self.last_send[s] = now
        ordv = None
        if self.fr:
            ordv = self.next_ord(now)
            self.last_ord[s] = ordv
            self.t_newest = now
        if self.cnt_on:
            self.send_times.append(now)
            self.sends_n += 1
        self.g["repair_sends"] += 1
        if self.trace is not None:
            self.trace.append((round(now, 9), s, k, 0, 1))
        def fin(a, dep):
            if a is not None:
                self.fifo_note(a)
            ps = self.ps_ctr
            self.ps_ctr += 1
            if self.fr:
                self.ord_ps[ps] = ordv
                self.note_carry(ordv)
            if a is not None and not self.lost(s, k, dep):
                self.push(a, 0, (s, k, now, ps, 0))      # every copy goes on path A (17.7)
                self.carry_floor(a)

        if self.fq is not None:
            if c.floor_law == "protect":
                self.update_floor(now)
            self.fq_submit(now, fin)
        else:
            a, dep = self.fwd(now)
            if c.floor_law == "protect":
                self.update_floor(now)
            fin(a, dep)
        if c.sweep_form == "blocker":
            self.arm_blocker(now)
        return True

    # ---- stage 3e: repair (build 4.2 - 4.4) ---------------------------------------------------------------
    def rate_hat(self, now):
        """First sends in (now - 8 srtt, now] over min(8 srtt, now - first send): the offline simulation's form of paper 5.3's anchor."""
        w = 8.0 * self.srtt_eff()
        q = self.fs_times
        while q and q[0] <= now - w:
            q.popleft()
        span = min(w, now - self.t_first) if self.t_first is not None else 0.0
        return len(q) / span if span > 0.0 else 0.0

    def eps_for_law(self):
        rp = self.cfg.repair
        return self.eps_est if rp["r"] == "est" else self.p0

    def read_law(self, now):
        """Re-read r and W (registration 2.1's cadence).  A fixed-r arm keeps r and still reads W."""
        rp = self.cfg.repair
        rh = self.rate_hat(now)
        if rh <= 0.0:
            return
        L = rate_law(self.eps_for_law(), self.srtt_eff(), rh, self.cfg.delta)
        self.W_cur = L["W"]
        if rp["r"] == "rb":                                                  # the bracket arm: the cap through the outage and its aftermath, the law outside
            self.r_cur = R_MAX if rb_hold(self, now) else L["r"]
        else:
            self.r_cur = L["r"] if rp["r"] in ("law", "est") else float(rp["r"])
        self.law_last = L
        self.t_eval = now
        g = self.g
        g["law_reads"] += 1
        g["W_floor_bind"] += L["W_pre"] < 16.0
        g["W_cap_bind"] += round(min(max(L["W_pre"], 16.0), 512.0)) > W_CAP
        g["sat_pressure_sum"] += saturation_pressure(L["raw"], L["r_sat"])
        g["W_sum"] += L["W"]
        g["r_sum"] += self.r_cur
        g["rmax_bind"] += L["rmax_bind"]
        g["burst_bind"] += L["burst_bind"]
        k = int(now)
        sr = self.ser
        sr["r"][k] = sr["r"].get(k, 0.0) + self.r_cur
        sr["W"][k] = sr["W"].get(k, 0.0) + L["W"]
        sr["n"][k] = sr["n"].get(k, 0) + 1

    # ---- revision 4, step 1: readouts only ------------------------------------------------------------
    def dhat_note_pause(self, dur):
        self.rd_flow = True
        self.rd_maxpause = max(self.rd_maxpause, dur)

    def dhat_close_round(self, now):
        dur = now - self.rd_t0
        if dur > 0.0:
            flow, mp = self.rd_flow, self.rd_maxpause
            if self.paused:
                flow, mp = True, max(mp, now - self.pause_t0)
            self.dh.add_round(self.rd_new, dur, flow, mp, self.R_now())
            self.r4["rounds"] += 1
        self.rd_t0, self.rd_n0, self.rd_new, self.rd_flow, self.rd_maxpause = now, self.n_sent, 0, self.paused, 0.0

    def dhat_on_report(self, now, rep, rhi):
        if self.rhi_known >= 0:
            self.sends_cov = max(self.sends_cov, self.send_idx[self.rhi_known] + 1)
        self.rd_new += rep[13] if len(rep) > 13 else 0
        if rhi >= self.rd_n0:                       # the report covers a datagram sent after the round began
            self.dhat_close_round(now)

    def in_flight_now(self, now):
        """Every datagram sent (first sends, copies, repairs) and not yet covered by a report, as the engine's in_flight counts: a report covering first-send
        seq rhi covers everything sent up to that first send's time."""
        cov = self.sends_cov
        if self.cfg.inflight_to_rule:
            cov = max(cov, bisect.bisect_right(self.send_times, now - engine_in_flight_horizon(self.srtt_eff())))
        elif self.cfg.inflight_to:
            cov = max(cov, bisect.bisect_right(self.send_times, now - self.cfg.inflight_to * self.R_now()))
        return self.sends_n - cov

    def lam_hat(self, t):
        """Units handed over per second over the rate law's window (8 srtt)."""
        w = min(8.0 * self.srtt_eff(), t)
        if w <= 0.0:
            return 0.0
        return (self.hand_count(t) - self.hand_count(t - w)) / w

    def next_ord(self, now):
        self.ord_ctr += 1
        self.ord_time.append(now)                  # ord_time[o - 1] = send time of ordinal o (a readout: the carrier's age at a repeat)
        return self.ord_ctr

    def note_carry(self, ordv, value=None):
        """A datagram sent now carries F_ann when some unresolved seq lies below it (the header carrier); a floor datagram carries its own value."""
        v = self.F_ann if value is None else value
        if value is None and not self.F_ann > self.lu:
            return
        if value is None:
            self.dcar_t = self.ord_time[ordv - 1]
            self.dcar_v = v
        if not self.carry_vals or v > self.carry_vals[-1]:
            self.carry_vals.append(v)
            self.carry_ords.append(ordv)

    def tau_now(self, t):
        """The wait limit at t: (tau, K, x, t_ov, R, o_hat, den0, D_hat, mu_hat).  t_ov0 is a timestamp of state, set while x <= 1; no branch selects a law."""
        c = self.cfg
        lam = self.lam_hat(t)
        raw = self.dh.value()
        mu = raw * (1.0 - c.dhat_bias)
        x = lam / mu
        if not x > 1.0:
            self.t_ov0 = t
        t_ov = t - self.t_ov0
        K = wait_limit_K(c.delta, x, t_ov)
        R, o = self.R_now(), self.o_hat()
        den0 = self.Te - R + o
        den = max(den0, o)
        return wait_limit_tau(self.Te, K, den), K, x, t_ov, R, o, den0, raw, mu

    def rev4_tick(self, t):
        """At each dequeue: lambda_hat, mu_hat = D_hat, t_ov, K, tau and the counts the revision's step 1 reads.  No decision reads them."""
        c, r4 = self.cfg, self.r4
        tau, K, x, t_ov, R, o, den0, raw, mu = self.tau_now(t)
        r4["ticks"] += 1
        r4["ov"] += x > 1.0
        r4["tau_lt_T"] += tau_below_t(tau, self.Te)
        r4["den_bind"] += den0 < o
        r4["tau_min"] = min(r4["tau_min"], tau)
        Kc = wait_limit_Kc(self.Te, R, o)
        if K > 0.0 and K >= Kc:
            r4["floor_n"] += 1
            if r4["collapse_t"] is None:
                r4["collapse_t"] = t
        if self.u_next + 1 < self.units_total and self.ht(self.u_next + 1) <= t and dequeue_expired(tau, t - self.ht(self.u_next), R):
            r4["deq_early"] += 1                    # the head the law would expire and section 13.2 holds
        fl = self.in_flight_now(t)
        r4["in_flight_max"] = max(r4["in_flight_max"], fl)
        r4["over_B"] += fl >= self.B_sim
        if raw < math.inf:
            r4["dhat_sum"] += mu
            r4["dhat_n"] += 1
            if r4["dhat_n"] % 200 == 1 and c.link is not None and len(r4["ratios"]) < 4000:     # the readout against the oracle: C / (1/(1 - eps) + r), a readout only
                oracle = c.link["C"] / (1.0 / (1.0 - self.p0) + self.r_cur)
                r4["ratios"].append(raw / oracle)
        else:
            r4["dhat_inf"] += 1

    def summary_rev4(self):
        r4 = self.r4
        n = max(r4["ticks"], 1)
        rs = sorted(r4["ratios"])
        q = lambda p: rs[min(len(rs) - 1, int(p * len(rs)))] if rs else None        # noqa: E731
        return dict(n_ticks=r4["ticks"], n_tau_lt_T=int(r4["tau_lt_T"]), ov_frac=r4["ov"] / n, floor_bind=r4["floor_n"] / n, collapse_t=r4["collapse_t"],
                    den_bind=int(r4["den_bind"]), deq_early_would=r4["deq_early"], dhat_inf_frac=r4["dhat_inf"] / n, dhat_mean=(r4["dhat_sum"] / r4["dhat_n"]) if r4["dhat_n"] else None,
                    dhat_ratio_q05=q(0.05), dhat_ratio_q50=q(0.5), dhat_ratio_q95=q(0.95), dhat_ratio_n=len(rs), in_flight_max=r4["in_flight_max"],
                    emit_over_B=r4["over_B"], elig_n=r4["elig_n"], elig_R_bind=r4["elig_R_bind"], elig_viol=r4["elig_viol"], send_gate=self.cfg.send_gate, cwnd_sim=self.B_sim, clamp=self.cfg.clamp, wait_limit=self.cfg.wait_limit, eq=self.cfg.eq,
                    spare_bind=(r4["spare_bind"] / r4["spare_n"]) if r4["spare_n"] else None, rounds=r4["rounds"], spare_bins=sorted((k, v[0], v[1]) for k, v in r4["spare_bins"].items()), tau_min=(r4["tau_min"] if r4["tau_min"] < math.inf else None))

    def astar(self, now):
        """Paper 5.3: A* = clamp(ceil(rate_hat * min(b*srtt, 2*srtt)), 1, W); 1 before the first RTT sample."""
        if self.srtt is None:
            self.g["astar_floor_bind"] += 1
            return 1
        se = self.srtt_eff()
        D = min(self.b * se, 2.0 * se)
        if self.b * se > 2.0 * se:
            self.g["D_cap_bind"] += 1
        raw = math.ceil(self.rate_hat(now) * D)
        if raw > self.W_cur:
            self.g["astar_W_bind"] += 1
        if raw < 1:
            self.g["astar_floor_bind"] += 1
        return int(min(max(raw, 1), self.W_cur))

    def delta_off(self, now):
        raw = math.ceil(self.rate_hat(now) * self.cfg.J)
        if raw < 1 or raw > 64:
            self.g["delta_bind"] += 1
        return int(min(max(raw, 1), 64))

    def stage_repair(self, now):
        c = self.cfg
        g = self.g
        e = c.repair["edge"]
        A = self.astar(now)
        self.astar_cur = A
        k = int(now)
        self.ser["A"][k] = self.ser["A"].get(k, 0) + A
        self.ser["nA"][k] = self.ser["nA"].get(k, 0) + 1
        D = self.delta_off(now)
        end = self.n_sent - D
        ws = max(0, self.A_known - int(self.W_cur))
        lo = max(ws, end - A)
        lo0 = lo
        if c.repair_seam != "no_edge":
            if e in ("worth", "age"):
                o = self.o_hat()
                while lo < end and not span_edge_ok(e, self.Te, now - self.h_seq[lo], o):
                    lo += 1
            elif e == "floor":
                lo = max(lo, self.F_ann)
            elif e == "evict":
                if evict_unservable(c.evict_window, self.n_sent, lo):
                    lo = self.n_sent - int(c.evict_window)
                while lo < end and lo in self.verdicted:
                    lo += 1
        if end <= lo:
            g["rep_drop_edge" if lo > lo0 else "rep_drop_young"] += 1
            return
        if lo > lo0:
            g["rep_cut_edge"] += 1
        j = self.n_rep
        self.n_rep += 1
        o = self.o_hat()
        for s in range(lo, end):
            age = now - self.h_seq[s]
            if staged_past_lifetime(self.Te, age):
                g["l1_viol_rep"] += 1
            if not resend_admissible(self.Te, age, o):
                g["worth_viol_rep"] += 1
        g["rep_sent"] += 1
        if self.cnt_on:
            self.send_times.append(now)
            self.sends_n += 1
        g["rep_members_sum"] += end - lo
        if self.trace is not None:
            self.trace.append((round(now, 9), lo, j, end, 3))        # kind 3: a repair over [lo, end) (the identity gates compare traces)
        self.wadd("repair", now)
        ordv = None
        if self.fr:
            ordv = self.next_ord(now)
            self.ord_rep[j] = ordv
            self.t_newest = now
            self.note_carry(ordv)

        def fin(a, dep):
            if a is not None:
                self.fifo_note(a)
            dd = c.dead
            lost = (a is None) or (dd is not None and lo >= dd[0] and dd[1] <= dep < dd[1] + dd[2]) \
                or (u01(c.seed, j, 0, 0, SALT_REPAIR) < self.p0)
            if a is None:
                g["rep_qdrop"] += 1
            elif lost:
                g["rep_lost"] += 1
            if not lost:
                self.push(a, 11, (j, lo, end, now))
                self.carry_floor(a)

        if self.fq is not None:
            self.fq_submit(now, fin)
        else:
            a, dep = self.fwd(now)
            fin(a, dep)

    def send_floor(self, now, f):
        if not self.cfg.floor_send:                  # today's Realtime: a shed is silent
            return
        rep = self.floor_rep.get(f, 0)
        self.floor_rep[f] = rep + 1
        self.g["floors"] += 1
        if rep:
            self.g["floor_rep"] += 1
        if self.trace is not None:
            self.trace.append((round(now, 9), f, -1, 0, 2))
        self.last_floor_t = now
        for p in range(self.npath):                      # ADR-0084: a floor datagram goes on every live path (17.7)
            a, dep = self.fwd(now, p)
            if a is not None:
                self.fifo_note(a, p)
            ps = self.ps_ctr
            self.ps_ctr += 1
            if self.fr:
                ordv = self.next_ord(now)
                self.ord_ps[ps] = ordv
                self.note_carry(ordv, f)
            if a is not None and not self.lost(f, rep, dep, SALT_FLOORLOSS, p):
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
        self.raw_lost += 1
        ho = self.ho_now()
        if self.cfg.want_p4:
            self.hoq[q] = ho
        if ho is not None:
            self.push(now + ho, 6, q)

    def on_data(self, now, s, c, tau, ps=None, path=0):
        """tau is None for a decoded unit (stage 3e): it carries no send stamp, no path and no delay sample."""
        got = self.got
        if self.fr and ps is not None:
            o = self.ord_ps.get(ps)
            if o is not None and o > self.rx_ord_cur:
                self.rx_ord_cur = o
        if tau is not None:
            if tau > self.tau_rx_recv:
                self.tau_rx_recv = tau
            if s > self.rhi_path[path]:
                self.rhi_path[path] = s
        if ps is not None:
            self.eps_trk.record(ps)
        if self.cfg.owd_est == "rel" and c >= 0:
            self.rel_sample(now, now - tau)
        if got[s]:
            self.g["dup"] += 1
            if self.dec_flag[s]:
                self.g["dup_after_dec"] += 1
        else:
            got[s] = 1
            self.arr[s] = now
            if c == 0:
                self.raw_rcv += 1
            if tau is not None:
                self.raw_new += 1
            if tau is not None:
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
                    if self.h_seq[s] >= 1.0:
                        self.g["late_floor_ge1"] += 1
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
            if s in self.eq_of:                      # stage 3e: s is an unknown of some equation; the equations shrink and may now decode
                self.dec_known(now, s)
        self.advance(now)

    def rel_sample(self, now, d):
        """owd_est "rel": the forward delay of an arrival minus its minimum over the last 10 s (a monotone deque gives the window minimum)."""
        dq = self.drel
        while dq and dq[-1][1] >= d:
            dq.pop()
        dq.append((now, d))
        while dq[0][0] < now - 10.0:
            dq.popleft()
        self.qf = d - dq[0][1]

    # ---- stage 3e: the decoder (build 5.5, 5.6) -----------------------------------------------------------
    def on_repair(self, now, pl):
        j, lo, end, ts = pl
        g = self.g
        if self.fr:
            o = self.ord_rep.get(j)
            if o is not None and o > self.rx_ord_cur:
                self.rx_ord_cur = o
        if ts > self.tau_rx_recv:
            self.tau_rx_recv = ts                    # RACK counts every datagram
        U = {s for s in range(lo, end) if not self.got[s]}
        if not U:
            g["rep_redundant"] += 1
            return
        g["rep_innov"] += 1
        self.n_eq += 1
        e = self.n_eq
        self.eqs[e] = U
        self.eq_meta[e] = (j, ts)
        for s in U:
            self.eq_of.setdefault(s, set()).add(e)
        self.decode(now, e)

    def component(self, e0):
        E, X, stack = {e0}, set(), [e0]
        while stack:
            e = stack.pop()
            for s in self.eqs[e]:
                if s not in X:
                    X.add(s)
                    for f in self.eq_of.get(s, ()):
                        if f not in E:
                            E.add(f)
                            stack.append(f)
        return E, X

    def determined(self, E, X):
        """Max matching unknown -> equation (augmenting paths), then the unknowns not reached by an alternating path from an unmatched
        unknown (Dulmage-Mendelsohn: outside the under-determined part).  The augmenting search is the brief's recursion, written with an
        explicit stack in the same visiting order (a component of a few hundred unknowns exceeds Python's recursion limit)."""
        adj = {x: sorted(f for f in self.eq_of[x] if f in E) for x in X}
        meq, mx = {}, {}

        def aug(x0, seen):
            stack = [(x0, iter(adj[x0]), None)]              # (unknown, its equations still to try, the equation under trial)
            while stack:
                x, it, _ = stack[-1]
                for f in it:
                    if f in seen:
                        continue
                    seen.add(f)
                    if f not in meq:                         # a free equation: augment along the whole stack
                        meq[f] = x
                        mx[x] = f
                        for xp, _, fp in stack[:-1]:
                            meq[fp] = xp
                            mx[xp] = fp
                        return True
                    stack[-1] = (x, it, f)
                    stack.append((meq[f], iter(adj[meq[f]]), None))
                    break
                else:
                    stack.pop()
            return False

        for x in sorted(X):
            aug(x, set())
        bad = {x for x in X if x not in mx}
        st = list(bad)
        while st:
            x = st.pop()
            for f in adj[x]:
                y = meq.get(f)
                if y is not None and y not in bad:
                    bad.add(y)
                    st.append(y)
        return sorted(X - bad), meq

    def decode(self, now, e0):
        """The generic-rank rule on one component; the cascade over the touched equations is a work list in the recursion's order."""
        work = [e0]
        while work:
            e0 = work.pop()
            if e0 not in self.eqs:
                continue
            E, X = self.component(e0)
            if len(X) > self.g["dec_comp_max"]:
                self.g["dec_comp_max"] = len(X)
            dec, meq = self.determined(E, X)
            if not dec:
                continue
            decset = set(dec)
            used = {f: x for f, x in meq.items() if x in decset}      # equation -> the decoded unknown it is matched to
            self.g["rep_used"] += len(used)
            self.g["rep_late_only"] += sum(1 for x in used.values() if decoded_late(self.cfg.T, now - self.h_seq[x]))
            touched = set()
            for s in dec:                                             # 1. detach every decoded unknown from every equation first
                for f in self.eq_of.pop(s, set()):
                    if f in self.eqs:
                        self.eqs[f].discard(s)
                        touched.add(f)
            for f in list(touched):
                if f in self.eqs and not self.eqs[f]:
                    del self.eqs[f]
                    self.eq_meta.pop(f, None)
            for s in dec:                                             # 2. then count and deliver each decoded unit once
                self.dec_flag[s] = 1
                if self.qd_first[s]:
                    self.g["dec_units_qdrop"] += 1
                self.ser["dec"][int(now)] = self.ser["dec"].get(int(now), 0) + 1
                age = now - self.h_seq[s]
                self.g["dec_units"] += 1
                if within_lifetime(self.cfg.T, age):
                    self.g["dec_on_time"] += 1
                if decoded_late(self.cfg.T, age):
                    self.g["dec_late"] += 1
                if s in self.released:
                    self.g["dec_after_release"] += 1
                if s in self.passed and self.passed_by.get(s) == "floor":
                    self.g["late_floor_dec"] += 1
                self.on_data(now, s, -1, None, None, 0)
            work.extend(reversed(sorted(touched)))                    # 3. then the cascade, on equations still live (popped in increasing order)

    def dec_known(self, now, s):
        """s arrived as data: remove it from every equation, drop empty equations, and re-run decode on each touched equation still live."""
        touched = self.eq_of.pop(s, set())
        for f in touched:
            if f in self.eqs:
                self.eqs[f].discard(s)
                if not self.eqs[f]:
                    del self.eqs[f]
                    self.eq_meta.pop(f, None)
        for f in sorted(touched):
            if f in self.eqs:
                self.decode(now, f)

    def on_floor(self, now, f, ps=None):
        if self.fr and ps is not None:
            o = self.ord_ps.get(ps)
            if o is not None and o > self.rx_ord_cur:
                self.rx_ord_cur = o
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
            if force_drain_due(len(self.r0e_buf), self.cfg.r0e_max):
                self.r0e_force_drain(now)
        self.r0e_drain(now)

    def r0e_force_drain(self, now):
        """reorder.rs force_drain_oldest: the oldest (lowest seq) entries are delivered until the buffer holds no more than half its cap; the
        holes below the largest delivered entry are passed."""
        buf = sorted(self.r0e_buf, key=lambda e: e[1])
        k = len(buf) - force_drain_keep(self.cfg.r0e_max)
        drained, self.r0e_buf = buf[:k], buf[k:]
        self.g["force_drains"] += 1
        before = self.g["passes"]
        self.pass_below(now, max(e[1] for e in drained), "r0e")
        self.r0e_given_up += self.g["passes"] - before

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
        refresh = (not self.dirty and self.cfg.rx_refresh and bool(self.holes)
                   and now >= self.last_rep_t + min(max(2.0 * self.srtt_eff(), self.cfg.sweep_lo), self.cfg.sweep_hi))
        if self.dirty or refresh:
            if refresh:
                self.g["refresh_reports"] += 1
            self.dirty = False
            self.last_rep_t = now
            m = self.min_hole()
            A = m if m is not None else self.rhi + 1
            rep = (now, A, self.G, self.rhi, tuple(sorted(self.holes)), tuple(self.passed_since_report), self.GF, self.tau_rx_recv)
            rep = rep + ((tuple(self.rhi_path),) if self.paths is not None else (None,)) + (None, self.raw_rcv, self.raw_lost, self.qf, self.raw_new, self.rx_ord_cur)
            self.raw_new = 0
            self.raw_rcv = 0
            self.raw_lost = 0
            self.passed_since_report = []
            self.push(now + self.cfg.owd + self.cfg.rev_q, 3, rep)
        if now + self.cfg.tick <= self.t_end:
            self.push(now + self.cfg.tick, 4, None)

    # ---- sender ---------------------------------------------------------------------------
    def on_report(self, now, rep):
        gen, A, G, rhi, M, passed, GF, tau = rep[:8]
        if tau > self.tau_rx:
            self.tau_rx = tau
        if (A, G) > (self.A_known, self.G_known):
            if A > self.A_known and self.cfg.rtt_est == "old":   # RTT sample (7.2): the report's arrival minus the send time of
                if not (self.got[A - 1] and self.arr[A - 1] <= gen):
                    self.rtt_unarrived_other += 1
                smp = now - self.sent_t[A - 1]         # the highest seq it newly acknowledges
                self.srtt = smp if self.srtt is None else self.srtt + (smp - self.srtt) / 8.0
            self.A_known, self.G_known, self.GF_known = A, G, GF
        if gen > self.last_gen:
            self.last_gen = gen
            if self.paths is not None:                                      # 17.7: one sample per path, from that path's newest seq
                for p in range(self.npath):
                    r = rep[8][p]
                    if r > self.rhi_pk[p]:
                        if self.copies[r] == 0:
                            if not (self.got[r] and self.arr[r] <= gen):
                                self.rtt_unarrived_other += 1
                            self.rtt_sample_p(p, now - self.sent_t[r])
                        self.rhi_pk[p] = r
            if rhi > self.rhi_known:
                # 13.7.1 item 1: largest newly received seq, Karn's rule.  Stage 3d amendment: the seq must have arrived when the report was made
                # (a floor jump raises rhi without an arrival; sampling it would time a unit that never came)
                if self.paths is None and self.cfg.rtt_est == "rfc" and self.copies[rhi] == 0:
                    if self.got[rhi] and self.arr[rhi] <= gen and not self.dec_flag[rhi]:
                        self.rtt_sample(now - self.sent_t[rhi])
                    else:
                        self.rtt_unarrived += 1
                self.rhi_known = rhi
            if self.cfg.repair is not None:                              # stage 3e: the estimated-loss arm (build 4.6)
                rcv, lst = rep[10], rep[11]
                self.beta_a = BETA_DECAY * self.beta_a + rcv
                self.beta_b = BETA_DECAY * self.beta_b + lst
                self.eps_est = beta_upper(self.beta_a, self.beta_b)
                self.g["eps_est_sum"] += self.eps_est
                self.g["eps_est_n"] += 1
            if self.cfg.owd_est == "rel":
                self.qf_known = rep[12]
            self.M_known = set(M)
        Ak, Mk, rk = self.A_known, self.M_known, self.rhi_known
        self.pending = {s for s in self.pending if s >= Ak and (s in Mk or s > rk)}
        if self.cfg.floor_law == "protect":
            self.lu = min(Mk) if Mk else rk + 1          # the lowest unresolved seq (holes are all >= A_known)
            self.update_floor(now)
        self.R_sum += self.R_now()
        self.srtt_sum += 0.333 if self.srtt is None else self.srtt
        self.rep_n += 1
        self.maybe_resume(now)
        if self.fr and len(rep) > 14:
            self.rx_ord = max(self.rx_ord, rep[14])
        if self.cnt_on:
            self.dhat_on_report(now, rep, rhi)
        self.trig = "report"
        holes = sorted(Mk)
        if self.cfg.reorder_rack and self.paths is not None:      # 17.7: RACK's time test with the reordering window = the measured skew
            skew = self.srtt_p(1) - self.srtt_p(0)
            holes = [s for s in holes if self.last_send[s] + skew < self.tau_rx]
        self.act(now, gen, holes, open_tail=False)
        self.arm_blocker(now)

    def gate_block(self, now):
        """The engine's send gate: a path takes data only while cwnd - in_flight > 0."""
        return self.cfg.send_gate and self.in_flight_now(now) >= self.B_sim

    def maybe_resume(self, now):
        if self.paused and self.O_ctl() < self.B and not self.gate_block(now):
            self.paused = False
            self.pause_log.append((self.pause_t0, now))
            if self.cnt_on:
                self.dhat_note_pause(now - self.pause_t0)
            if self.cfg.src_model == "wc":
                self.src_next = max(now, self.ht(self.u_next))
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
                if resend_admissible(self.Te, now - h, x):
                    self.push(max(h + self.Te - x, now + 1e-9), 8, s)
                    return
                if not self.evidence(s, now):
                    self.push(max(self.last_send[s] + self.R_now(), now + 1e-9), 8, s)
                    return
            self.verdicted.add(s)
            self.pending.add(s)
            self.g["verdicts"] += 1
            self.g["l2_fail"] += 1
            if self.cfg.sender == "S2c":
                if self.send_copy(now, s):
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
        if self.cfg.resend_gate == "cooldown":                       # 15.6.1 item 5: the engine's per-seq cooldown, every arm
            return self.copies[s] == 0 or now - self.last_send[s] >= self.cooldown()
        if self.cfg.resend_gate == "cooldown_all":                   # unregistered (results-stage3c.md, Q3c-2): the cooldown also binds a seq's first resend
            return now - self.last_send[s] >= self.cooldown()
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
        if c.floor_law == "protect":                                  # 15.6.1 item 4: the commitment; a seq below F_ann is never judged or resent
            n0 = len(cands_)
            cands_ = [s for s in cands_ if s >= self.F_ann]
            self.g["commit_skip"] += n0 - len(cands_)
        n1 = len(cands_)
        cands_ = [s for s in cands_ if not evict_unservable(c.evict_window, self.n_sent, s)]
        self.g["evict_skip"] += n1 - len(cands_)
        if not cands_:
            self.floor_datagram(now)
            return
        g = self.g
        verdict_seqs, serve_seqs = [], []
        if self.s2fam:
            x = self.o_hat()                                          # S2: the exact owd, as the D_RTprop arm idealises; S3: srtt/2
            for s in cands_:
                if resend_admissible(self.Te, now - self.h_seq[s], x):
                    serve_seqs.append(s)
                elif c.verdict_evidence == "none":                       # 17.2: a candidate failing L2 is skipped, never judged
                    g["l2_skip"] += 1
                elif c.verdict_evidence == "feedback_tail":              # 17.6 (S5v): 15.1's feedback verdict, plus 13.7.1's timeout for the open tail only
                    if self.fb_evidence(s) or (s > self.rhi_known and now >= self.last_send[s] + self.R_now()):
                        verdict_seqs.append(s)
                elif c.verdict_evidence == "feedback_tail_any":          # UNREGISTERED supplement (results-stage3d.md, Q3d-1): the timeout also for a hole no later send has overtaken
                    if self.fb_evidence(s) or ((s > self.rhi_known or self.last_send[s] >= self.tau_rx) and now >= self.last_send[s] + self.R_now()):
                        verdict_seqs.append(s)
                elif c.verdict_evidence == "feedback":                   # 15.1: only on receiver feedback
                    if self.fb_evidence(s):
                        verdict_seqs.append(s)
                elif not c.evidence_verdict or self.evidence(s, now):
                    verdict_seqs.append(s)
            g["l2_fail"] += len(verdict_seqs)
        elif c.sender != "S0":
            held = self.GF_known if c.recv == "R0c" else self.G_known     # R0c: only the floor pass's passes admit nothing else
            A_avail = self.shed_left() if c.budget == "resid" else self.allow() - (held + len(self.pending))
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
        if c.floor_law == "protect":
            need_floor = False                                           # the protected floor goes on the carrier (below) and on the sweep
        if verdict_seqs and c.sender in ("S1c", "S2c"):
            for s in verdict_seqs:
                if self.send_copy(now, s):
                    g["verdict_copies"] += 1
        for s in serve_seqs:
            if self.can_resend(s, gen, now):
                first = self.copies[s] == 0
                if self.send_copy(now, s):
                    g["serves"] += 1
                    g["resend_by_" + self.trig] += 1
                    if first:
                        self.rs_age.setdefault(self.trig, []).append(now - self.h_seq[s])      # age from hand-over at a seq's first resend (Q3c-8)
            else:
                self.wadd("cool", now)
        if need_floor:
            self.last_floor = F
            self.send_floor(now, F)
        self.floor_datagram(now)
        if verdict_seqs and self.resume_on_verdict:
            self.maybe_resume(now)

    def floor_repeat(self, now):
        """The floor is sent again at each tail-sweep fire while F > A_rx (7.2).  Protected floor (15.6.1 item 2): F_ann, no pending test."""
        if self.cfg.floor_law == "protect":
            if self.F_ann > self.A_known:
                if self.fr:
                    if not self.floor_repeat_evidenced(now):
                        return
                self.last_floor = max(self.last_floor, self.F_ann)
                self.send_floor(now, self.F_ann)
            return
        if not self.pending:
            return
        F = self.floor_value()
        if any(s < F for s in self.pending) and F > self.A_known:
            self.last_floor = max(self.last_floor, F)
            self.send_floor(now, F)

    def floor_repeat_evidenced(self, now):
        """Revision 4 section 10.3: repeat only when some unresolved s in [A_known, F_ann) has loss evidence and no datagram carrying a floor above s has arrived.
        Gauges the repeat as fired by knowledge or by timeout, and every suppression is checked against the receiver's own floor (it must already be above s*)."""
        g = self.g
        Mk, rk = self.M_known, self.rhi_known
        cands = [s for s in range(self.A_known, self.F_ann) if s in Mk or s > rk]
        if not cands:
            g["rep_suppressed"] += 1                 # today's rule would send (F_ann > A_known); nothing is unresolved below the floor
            return False
        od = (self.dcar_v, self.dcar_t) if (self.fr_od and self.dcar_t is not None) else None
        fire, star, by_to = floor_repeat_needed(cands, self.last_ord, self.rx_ord, self.carry_vals, self.carry_ords, self.t_newest, self.R_now(), now, od)
        if fire:
            if od is not None and od[0] > star and now < od[1] + self.R_now() - 1e-12:
                g["rep_fire_viol_carrier"] += 1      # the absolute check of 10.7: every repeat fires at least R after the newest covering data carrier was sent
            g["rep_fired_by_timeout" if by_to else "rep_fired_by_knowledge"] += 1
            i = bisect.bisect_right(self.carry_vals, star)           # the datagram that carries the covering floor: sent, not yet reported arrived
            if i >= len(self.carry_ords):
                g["rep_fire_carrier_none"] += 1
            elif now < self.ord_time[self.carry_ords[i] - 1] + self.R_now():
                g["rep_fire_carrier_young"] += 1                     # in flight and not yet overdue: the repeat tells the receiver nothing it will not learn
            else:
                g["rep_fire_carrier_overdue"] += 1
        else:
            g["rep_suppressed"] += 1                 # today's rule would send; either nothing is evidenced yet or the carrier has arrived
            if star is not None and not self.fr_od:
                g["rep_checks"] += 1
                if not self.F_rx > star:             # the absolute check: the receiver already holds a floor above the highest evidenced hole
                    g["rep_supp_viol"] += 1
        return fire

    def arm_blocker(self, now):
        """17.3: the engine's sweep deadline, max(last_send(blocker), last fire) + clamp(2 srtt_pooled, 25 ms, 100 ms), recomputed from
        the measured srtt whenever the blocker, its last send or the srtt may have changed.  Stale events are ignored by their payload."""
        if self.cfg.sweep_form != "blocker":
            return
        b = self.A_known
        if b >= self.n_sent:
            self.bl_dl = None
            return
        if self.elig_rack:
            P = self.blocker_period()
            reach = max(P, self.R_now())
            dl = max(self.last_send[b] + reach, self.bl_fire + P)
        else:
            dl = max(self.last_send[b], self.bl_fire) + self.blocker_period()
        if dl != self.bl_dl:
            if self.srtt is not None:
                self.r4["elig_n"] += 1
                self.r4["elig_R_bind"] += self.R_now() > self.blocker_period()      # the gauge: deadlines at which R exceeds P (counted after the first RTT sample)
            self.bl_reach = max(self.blocker_period(), self.R_now())
            self.bl_ls = self.last_send[b]
            self.bl_dl = dl
            self.push(max(dl, now), 10, dl)

    def on_blocker(self, now, dl):
        """The blocker sweep fires: only the cumulative blocker goes to the serve path (cooldown, budget, L2, floor re-check all apply),
        then the floor is recomputed and repeated while F_ann > A_known."""
        if dl != self.bl_dl:
            return
        self.bl_fire = now
        self.bl_dl = None
        self.g["bl_fires"] += 1
        if self.cfg.floor_law == "protect":
            self.update_floor(now)
        if self.A_known < self.n_sent:
            self.trig = "sweep"
            self.act(now, now, [self.A_known], open_tail=True)
        if self.last_floor_t != now:
            self.floor_repeat(now)
        self.arm_blocker(now)

    def on_sweep(self, now):
        if self.cfg.floor_law == "protect":
            self.update_floor(now)
        P = self.sweep_period()
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
        if c.sweep_form == "tail":
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
                self.on_data(t, pl[0], pl[1], pl[2], pl[3], pl[4])
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
            elif kind == 10:
                self.on_blocker(t, pl)
            elif kind == 11:
                self.on_repair(t, pl)
            elif kind == 12:
                self.on_fq(t)
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
        """Stage 3e adds to the output: out["rep3e"] (the readouts, listed in summary3e's docstring with their registration sections) and, in
        out["gauges"], the repair gauges (rep_*, dec_*, l1_viol_rep, worth_viol_rep, owed_max, the clamp bind counters, law_reads, eps_est_*)."""
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
        self.summary3c(out)
        self.summary3e(out, n, ok, dl, hs_)
        if c.rev4_gauge:
            out["rev4"] = self.summary_rev4()
        self.summary3d(out, n, ok, dl, hs_)
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
        out["rtt_unarrived"] = self.rtt_unarrived
        out["rtt_unarrived_other"] = self.rtt_unarrived_other
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

    def summary3c(self, out):
        """Stage 3c readouts (three-knobs.md 15.5, 15.6.1 item 7): the per-1 s series, the floor's gauges, finished, the stall parts
        (o), (oo), (i), (ii), (iii).  No mean over delivered units alone is taken."""
        c, g = self.cfg, self.g
        out["finished"] = (self.t_front is not None) if c.n_total else None
        out["pass_floor"] = g["pass_floor"]
        out["late_floor"] = g["late_floor"]
        out["resend_braked"] = g["resend_braked"]
        out["floor_hull_bind"] = g["hull_bind"] / max(g["hull_upd"], 1)
        out["F_lag"] = (self.flag_sum / self.flag_n) if self.flag_n else None
        out["max_copies_5ms"] = max(self.copy_win.values(), default=0)
        for k in ("report", "sweep"):
            v = self.rs_age.get(k, [])
            out["rs_age_%s_n" % k] = len(v)
            out["rs_age_%s_med" % k] = float(np.median(v)) if v else None
        plog = list(self.pause_log)
        if self.paused:
            plog.append((self.pause_t0, self.t_last))
        nwin = int(self.t_last) + 1
        for nm in self.win.values():
            if nm:
                nwin = max(nwin, max(nm) + 1)
        for k, nm in (("first", "win_first"), ("resend", "win_resend"), ("braked", "win_braked"), ("cool", "win_blk_cool")):
            out[nm] = [self.win[k].get(i, 0) for i in range(nwin)]
        out["win_blk_budget"] = out["win_braked"]
        out["win_util"] = [self.win["busy"].get(i, 0.0) for i in range(nwin)]
        pw = [0.0] * nwin
        for a_, b_ in plog:
            for i in range(int(a_), min(nwin, int(b_) + 1)):
                pw[i] += max(0.0, min(b_, i + 1.0) - max(a_, float(i)))
        out["win_pause"] = pw
        out["win_blk_pause"] = [x * c.rate for x in pw]              # hand-over instants that fell inside a pause
        parts = None
        eps = list(self.episodes)
        if self.ep_start is not None:
            eps.append((self.ep_start, self.t_last - self.ep_start, self.ep_pass))
        if c.dead is not None and c.n_total:
            rs, re_ = c.dead[1] + c.owd, c.dead[1] + c.dead[2] + c.owd          # the outage at the receiver
            ov = [e for e in eps if e[0] < re_ and e[0] + e[1] > rs]
            if ov:
                st, ln, _ = max(ov, key=lambda e: e[1])
                en = st + ln
                tf = next((x for x in self.floor_arrivals if x >= re_), None)      # first floor or F-carrying datagram after the outage
                tfc = en if tf is None else min(tf, en)
                fills = [v for v in self.tfill.values() if tfc <= v <= en]
                p_o = max(0.0, rs - st)
                p_oo = max(0.0, min(en, re_) - max(st, rs))
                p_i = max(0.0, tfc - max(re_, st))
                p_ii = max(0.0, max(fills) - tfc) if fills else 0.0
                p_iii = max(0.0, (en - st) - p_o - p_oo - p_i - p_ii)
                parts = dict(start=st, length=ln, o=p_o, oo=p_oo, i=p_i, ii=p_ii, iii=p_iii, floor_seen=tf is not None)
        out["stall3c"] = parts

    def summary3e(self, out, n, ok, dl, hs_):
        """Stage 3e readouts (registration 6, Amendment 1; build 6), all in one block so that id3e can exclude it as a whole:
        r_mean, W_mean (6, the law's readings), sat_pressure, W_floor_bind_frac, W_cap_bind_frac (6, the clamp bind gauges),
        eps_est_mean (2.1, the estimated-loss arm), p_dec_meas and p_fec_law (6, 11 item 1), rep_link_share (6, 2.5), rep_members_mean,
        recovery_time_0p15 and its status (6, 2.6; filled in summary3d where recovery_time is), mu_eff_hat (6, Q3e-7),
        rmax_bind_frac and burst_bind_frac (6, the clamp bind gauges), series (6: per 1 s repairs, r, W, A*, decoded units),
        share_by (13.2: {tau: share delivered within tau} for tau in cfg.tau_extra, the share_by_0p4 rule).
        The gauges (rep_*, dec_*, l1_viol_rep, worth_viol_rep, owed_max, the bind counters) are in out["gauges"]."""
        c, g = self.cfg, self.g
        nl = max(g["law_reads"], 1)
        p_fec = 0.0
        if c.repair is not None and self.law_last is not None:
            p_fec = p_fec_normal_as(self.r_cur, self.eps_for_law(), self.W_cur, 1.0)
        # link shares over the saturated windows (link busy >= 0.98); repair, first sends and resends are the link datagrams
        w = self.win
        nwin = max([int(self.t_last) + 1] + [max(x) + 1 for x in w.values() if x])
        sat = [i for i in range(nwin) if w["busy"].get(i, 0.0) >= 0.98]
        tot = sum(w["repair"].get(i, 0) + w["first"].get(i, 0) + w["resend"].get(i, 0) for i in sat)
        rep_share = sum(w["repair"].get(i, 0) for i in sat) / tot if tot else 0.0
        start = int(math.ceil(self.oe)) if self.oe is not None else 1
        sat2 = [i for i in sat if i >= start]
        tot2 = sum(w["repair"].get(i, 0) + w["first"].get(i, 0) + w["resend"].get(i, 0) for i in sat2)
        mu = (c.link["C"] * sum(w["first"].get(i, 0) for i in sat2) / tot2) if (c.link is not None and tot2) else None
        share_by = {}
        for tau in c.tau_extra:
            age = (np.asarray(dl) - np.asarray(hs_))[ok]
            share_by[tau] = int((age <= tau).sum()) / max(self.units_total, 1)
        out["rep3e"] = dict(
            r_mean=float(g["r_sum"] / nl), W_mean=float(g["W_sum"] / nl), sat_pressure=float(g["sat_pressure_sum"] / nl),
            W_floor_bind_frac=float(g["W_floor_bind"] / nl), W_cap_bind_frac=float(g["W_cap_bind"] / nl),
            eps_est_mean=float(g["eps_est_sum"] / max(g["eps_est_n"], 1)),
            p_dec_meas=float(g["dec_units"] / max(len(self.orig_lost), 1)), p_fec_law=float(p_fec),
            rep_link_share=float(rep_share), rep_members_mean=float(g["rep_members_sum"] / max(g["rep_sent"], 1)),
            recovery_time_0p15=None, recovery_status_0p15=None, mu_eff_hat=mu, share_by=share_by,
            rmax_bind_frac=float(g["rmax_bind"] / nl), burst_bind_frac=float(g["burst_bind"] / nl), series=self.series(nwin))

    def series(self, nwin):
        """Per 1 s window (by the time of the event): repairs sent, mean r read, mean W read, mean A* of a repair, units decoded."""
        sr, w = self.ser, self.win["repair"]
        return dict(repairs=[int(w.get(i, 0)) for i in range(nwin)],
                    r=[sr["r"][i] / sr["n"][i] if sr["n"].get(i) else None for i in range(nwin)],
                    W=[sr["W"][i] / sr["n"][i] if sr["n"].get(i) else None for i in range(nwin)],
                    astar=[sr["A"][i] / sr["nA"][i] if sr["nA"].get(i) else None for i in range(nwin)],
                    decoded=[int(sr["dec"].get(i, 0)) for i in range(nwin)])

    def summary3d(self, out, n, ok, dl, hs_):
        """Stage 3d readouts (three-knobs.md 17.4, 17.7, 17.8): the window series by hand-over time, recovery time, the first dequeue
        expiry, the floor hold estimate at the end."""
        c, g = self.cfg, self.g
        out["first_deq_exp"] = self.first_deq_exp
        out["H_f_end"] = self.est.value(self.t_last)
        if self.paths is not None:
            out["srtt_path"] = [self.psrtt[i] for i in range(self.npath)]
            pa = np.frombuffer(bytes(self.path_of[:n]), dtype=np.uint8)
            d_all = np.where(ok, dl - hs_, np.inf)                       # per seq: delay from hand-over, inf if never delivered
            for p, nm in ((0, "A"), (1, "B")):                           # 17.8 Q3d-8: the delay of the units first sent on each path
                d = np.sort(d_all[pa == p])
                for q in (50, 90, 99):
                    out["dh%s_p%d" % (nm, q)] = float(d[max(0, math.ceil(q / 100.0 * len(d)) - 1)]) if len(d) else float("nan")
        if not c.windows:
            return
        ages = np.full(self.units_total, np.inf)
        u = np.rint(np.asarray(hs_) / self.dt).astype(int)
        m = ok & (u < self.units_total)
        ages[u[m]] = (dl - hs_)[m]
        out["win_ot04"] = [round(x, 6) for x in window_shares(ages, self.dt, 0.4)]
        out["win_ot2"] = [round(x, 6) for x in window_shares(ages, self.dt, 2.0)]
        if c.dead is not None and c.n_total:
            out["recovery_time"], out["recovery_status"] = recovery_time(ages, self.dt, 0.4, c.dead[1], c.dead[1] + c.dead[2])
            out["rep3e"]["recovery_time_0p15"], out["rep3e"]["recovery_status_0p15"] = recovery_time(ages, self.dt, 0.15, c.dead[1], c.dead[1] + c.dead[2])

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
