# Stage 3e build brief: repair in the offline simulation (implementation, no design choices left)

**What this is.** The exact code that stage 3e of the offline simulation needs: `docs/derivations/three-knobs-stage3e-registration.md` §1–§13 on this branch (`reg-stage3e`). Every choice is made here.

**Precedence.**
- **This brief is authoritative for code.** The registration's §3 is superseded by it, as the registration says.
- **The registration is authoritative** for arms, cells, readouts, predictions, the reading rule and the run order.
- If the two disagree anywhere outside the registration's §3, the worker stops and reports the conflict instead of choosing.

**Rules for the worker.**
- **Base.** The local branch `proof-s1` at `b2addffb`, which is built on the RTT-sampling fix. Create the branch `toy-stage3e` from it. Work only in `tools/toy/`.
- **Commit and push.** Commit in the order of §9. Push `toy-stage3e` to `github` only, with no force-push. Before every push, run the secret scan the coordinator gives on the diff; it must be empty.
- **Do not change:**
  - the existing behaviour of any field at its default;
  - `batch3*.py`, `proof_s1.py` and the existing tests, except to add tests;
  - the engine, the derivation and the roadmap.
- **No runs beyond the tests, the identity checks and one timing run** (§9 step 5). The batch parts of the registration's §13.6 are started by the coordinator.
- **Prose** in comments and docs says "the offline simulation", never "toy". File names stay as they are.
- **Memory:** at most 2 worker processes, and the identity checks alone on the machine.

## 1. Files

| file | what |
|---|---|
| `toy2.py` | §2–§6: new constants, law functions, `Cfg` fields, sender, receiver and gauges |
| `batch3e.py` | §8: cells, arm builders, parts, markers, stop checks, fingerprint dedupe |
| `summarize3e.py` | §8.5: the reader. It reuses `proof_s1.py`'s functions |
| `check_ident3e.py` | §7.1: the identity check |
| `rate_ref3e.json` | §7.2: reference values from `raptorpath-math`, made by a scratch crate outside the repository |
| `test_toy2.py` | add class `Stage3e` (§7) and extend `EXEMPT` |
| `results-stage3e.md` | created empty, with a header naming the registration commit; filled by the runner's reader |

## 2. Constants and law functions (`toy2.py`, module level)

Add each function name below to the deny test's `EXEMPT` set, by name only.

```python
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
    max_overhead R_MAX, codec CODEC_RLC, completion exposure 0.  Returns (rate, raw before the cap, r_sat or None)."""
    if p < 1e-10: return 0.0, 0.0, None
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
    return min(max(rate, 0.0), R_MAX), raw, r_sat

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
    a, raw, r_sat = controller_rate_parts(p, 1.0, 1.0, W, tsy, srtt, tsm, tail, False)
    l, _, _ = controller_rate_parts(p, 1.0, 1.0, W, tsy, srtt, tsm, min(max(BASE_TAIL * 100.0, 1e-9), 0.1), True)
    beta = bulkness(delta)
    return dict(r=(1.0 - beta) * a + beta * l, W=W, W_pre=W_pre, raw=raw, r_sat=r_sat, beta=beta)

def beta_upper(a, b, conf=0.95):
    """estimator.rs loss_rate_upper = beta_quantile(b, a, conf): the normal approximation of the Beta quantile, clamped to [0, 1]."""
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

def saturation_pressure(raw, r_sat):
    s = 0.1 * r_sat if r_sat else 0.0
    if not (s > 0.0): return 0.0
    x = (raw - r_sat) / s
    return 1.0 / (1.0 + math.exp(-x)) if x > -700 else 0.0
```

`span_edge_ok` and `decoded_late` compare T, so they go in `EXEMPT` like `resend_admissible`. The branch on `edge` is an arm selector (a string), as `floor_law` is.

## 3. `Cfg` fields (append; the defaults reproduce `proof-s1`)

```python
repair: object = None          # stage 3e: None = no repair; dict(r="law" | "est" | float, edge="worth" | "age" | "floor" | "none" | "evict")
repair_seam: object = None     # stage 3e, test only: "no_edge" disables the lifetime edge (the L1 positive control)
o_bias: float = 0.0            # stage 3e robustness: added to the one-way estimate o_fwd (L2, protected, the worth edge)
R_bias: float = 0.0            # stage 3e robustness: added to R in the dequeue rule only
rev_q: float = 0.0             # stage 3e robustness: a standing reverse queue, seconds added to every report's flight
tau_extra: tuple = ()          # stage 3e: extra on-time readouts (RT: (0.12,)), reported in out["rep3e"]["share_by"]
```

`owd_est` gains the value `"rel"` (§6.3).

## 4. Sender (`Sim`)

### 4.1 State (in `__init__`, always initialised; nothing reads it when `repair is None`)

```python
self.owed = 0.0; self.n_rep = 0; self.t_eval = -math.inf
self.r_cur = 0.0; self.W_cur = float(W_CAP); self.law_last = None
self.fs_times = deque()                 # first-send times, for rate_hat
self.t_first = None
self.beta_a = 1.0; self.beta_b = 1.0    # the estimated-loss arm (estimator.rs: start at 1, 1)
self.eps_est = beta_upper(1.0, 1.0)
self.astar_cur = 1
```

The shed set for the `"evict"` edge is `self.verdicted`: in `E-rt` (sender S1) a shed is a verdict.

### 4.2 Helpers

```python
def rate_hat(self, now):
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
    self.r_cur = L["r"] if rp["r"] in ("law", "est") else float(rp["r"])
    self.law_last = L
    self.t_eval = now
    g = self.g
    g["law_reads"] += 1
    g["W_floor_bind"] += L["W_pre"] < 16.0
    g["W_cap_bind"] += round(min(max(L["W_pre"], 16.0), 512.0)) > W_CAP
    g["sat_pressure_sum"] += saturation_pressure(L["raw"], L["r_sat"])
    g["W_sum"] += L["W"]; g["r_sum"] += self.r_cur

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
    if raw > self.W_cur: self.g["astar_W_bind"] += 1
    if raw < 1: self.g["astar_floor_bind"] += 1
    return int(min(max(raw, 1), self.W_cur))

def delta_off(self, now):
    raw = math.ceil(self.rate_hat(now) * self.cfg.J)
    if raw < 1 or raw > 64: self.g["delta_bind"] += 1
    return int(min(max(raw, 1), 64))
```

Every gauge key used here and in §4.3–§5 is added to the `self.g` dict initialiser, with value 0, or 0.0 for the `_sum` keys.

### 4.3 Emission (in `emit`, directly after the `if first_lost and not dup_ok: self.orig_lost.append(s)` lines)

```python
if c.repair is not None:
    if self.t_first is None:
        self.t_first = t
    self.fs_times.append(t)
    if t - self.t_eval >= min(RATE_CADENCE_MAX, self.srtt_eff() / 4.0):
        self.read_law(t)
    self.owed += self.r_cur
    if self.owed >= 1.0:
        self.owed -= 1.0
        self.g["rep_credits"] += 1
        self.stage_repair(t)
    if self.owed > self.g["owed_max"]:
        self.g["owed_max"] = self.owed
```

At r = 0, `owed` stays 0. No event is pushed, no draw is taken, and `ctr` and `ps_ctr` do not move. `read_law` pushes nothing.

### 4.4 `stage_repair(now)`

```python
def stage_repair(self, now):
    c = self.cfg; g = self.g; e = c.repair["edge"]
    A = self.astar(now); self.astar_cur = A
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
    j = self.n_rep; self.n_rep += 1
    o = self.o_hat()
    for s in range(lo, end):
        age = now - self.h_seq[s]
        if staged_past_lifetime(self.Te, age):
            g["l1_viol_rep"] += 1
        if not resend_admissible(self.Te, age, o):
            g["worth_viol_rep"] += 1
    g["rep_sent"] += 1; g["rep_members_sum"] += end - lo
    self.wadd("repair", now)
    a, dep = self.fwd(now)
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
```

Notes:
- The `"evict"` branch uses the existing `evict_unservable(window, n_sent, s)` (`n_sent − s > window`). So `lo` rises to `n_sent − window` exactly when it lies out of the window, and an infinite window never binds.
- Add `"repair"` to `self.win`'s keys (the window series).
- `resend_admissible(T, age, o)` is the existing L2 function (`age + o < T`). It is reused as the worth test so that the gauge and L2 cannot drift apart.
- A repair sent into the outage is lost through the `dead` test, as data is.

### 4.5 Today's Realtime's budget with repair (replace the body of `refresh_shed_budget`)

```python
def refresh_shed_budget(self):
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
```

- With `repair is None` this returns `residual_loss(p0, 0, 0, 1) = p0`, exactly as `proof-s1`.
- `residual_loss` keeps the existing erfc `p_fec_normal`. That is a declared difference of at most 1.5·10⁻⁷ from the engine's A&S form, and it keeps `proof-s1`'s pinned test unchanged.

### 4.6 The estimated loss (in `on_report`, inside `if gen > self.last_gen:`, before `self.M_known = set(M)`)

```python
if self.cfg.repair is not None:
    rcv, lst = rep[10], rep[11]
    self.beta_a = BETA_DECAY * self.beta_a + rcv
    self.beta_b = BETA_DECAY * self.beta_b + lst
    self.eps_est = beta_upper(self.beta_a, self.beta_b)
    self.g["eps_est_sum"] += self.eps_est; self.g["eps_est_n"] += 1
```

### 4.7 RTT sampling excludes decoded seqs

In `on_report`'s RFC branch, change `if self.got[rhi] and self.arr[rhi] <= gen:` to `if self.got[rhi] and self.arr[rhi] <= gen and not self.dec_flag[rhi]:`.

`dec_flag` is a receiver `bytearray(self.nmax)` (§5), so with no decodes the condition is unchanged. This is the registration's `rhi_smp` rule: the fixed code's sample, with decode raises excluded.

### 4.8 Robustness hooks

- **`o_fwd`:** `return self.srtt_p(0) - self.rtt / 2.0 + self.cfg.o_bias`. With `owd_est == "rel"`, return `self.rtt / 2.0 + self.qf_known + self.cfg.o_bias` instead.
- **`o_hat`:** extend the `"fwd"` test to `self.cfg.owd_est in ("fwd", "rel")`, so that both go through `o_fwd`.
- **`update_floor` and `prot_seq`:** change `if c.owd_est != "half"` and `c.owd_est == "half"` so that `"rel"` behaves like `"fwd"`. They already read `2·o_fwd()`.
- **Dequeue:** in `emit`, `dequeue_expired(self.Te, t - self.u_next * self.dt, self.R_now() + c.R_bias)`.
- **Reverse queue:** in `on_tick`, push the report at `now + self.cfg.owd + self.cfg.rev_q`.
- **`qf_known`:** sender state, initialised 0.0, set from `rep[12]` in `on_report` (with `gen > last_gen`) when `owd_est == "rel"`.

## 5. Receiver

### 5.1 State

```python
self.dec_flag = bytearray(self.nmax)
self.eqs = {}              # eq id -> set of unknown seqs
self.eq_of = {}            # unknown seq -> set of eq ids
self.eq_meta = {}          # eq id -> (repair index j, send time)
self.raw_rcv = 0; self.raw_lost = 0          # first-send fates since the last report (the estimated-loss arm)
self.drel = deque()        # (t, d_rel) for owd_est "rel", a 10 s window minimum (the RTprop window, 17.1)
self.qf = 0.0
self.n_eq = 0              # equation ids
```

### 5.2 Counting first-send fates

- In `on_data`, in the branch where a new seq arrives (`got[s]` was 0), when `c == 0`: `self.raw_rcv += 1`.
- In `new_hole`, when `from_floor` is False: `self.raw_lost += 1`.
- A hole is created once, so a later fill does not change the count.

### 5.3 The report

In `on_tick`, build the report as now and append:
- when `self.paths is None`, `None` at index 8 (with paths, index 8 is already the per-path tuple);
- `None` at index 9 (reserved);
- `self.raw_rcv` (index 10) and `self.raw_lost` (index 11);
- `self.qf` (index 12).

Then reset `raw_rcv` and `raw_lost` to 0.
- `on_report` keeps reading `rep[8]` only when `self.paths is not None`, as now.
- The appended fields change no output.

### 5.4 `on_data` changes

- `tau` may be `None` (a decode). Then skip the `tau_rx_recv` update and `self.est.feed`, and do not update `rhi_path`.
- When `owd_est == "rel"` and `c >= 0`: `d = now - tau`; append `(now, d)` to `drel`; pop entries older than `now − 10.0` from the left; `self.qf = d - min(x for _, x in self.drel)`. A monotonic deque is allowed if it gives the same value.
- **Arrival of an already-received seq:** keep the existing `dup` count, and if `self.dec_flag[s]`, also count `dup_after_dec`.
- **Arrival of a new seq that is an unknown of some equation** (`s in self.eq_of`): after the existing bookkeeping, call `self.dec_known(now, s)`.

### 5.5 `on_repair(now, pl)` (event kind 11; add `elif kind == 11: self.on_repair(t, pl)` to `run`)

```python
def on_repair(self, now, pl):
    j, lo, end, ts = pl
    g = self.g
    if ts > self.tau_rx_recv:
        self.tau_rx_recv = ts                    # RACK counts every datagram
    U = {s for s in range(lo, end) if not self.got[s]}
    if not U:
        g["rep_redundant"] += 1
        return
    g["rep_innov"] += 1
    self.n_eq += 1
    e = self.n_eq
    self.eqs[e] = U; self.eq_meta[e] = (j, ts)
    for s in U:
        self.eq_of.setdefault(s, set()).add(e)
    self.decode(now, e)
```

### 5.6 `decode(now, e0)`: the generic-rank rule on one component

```python
def component(self, e0):
    E, X, stack = {e0}, set(), [e0]
    while stack:
        e = stack.pop()
        for s in self.eqs[e]:
            if s not in X:
                X.add(s)
                for f in self.eq_of.get(s, ()):
                    if f not in E:
                        E.add(f); stack.append(f)
    return E, X

def determined(self, E, X):
    """Max matching unknown -> equation (augmenting paths), then the unknowns not reached by an alternating path from an unmatched
    unknown (Dulmage-Mendelsohn: outside the under-determined part)."""
    adj = {x: sorted(f for f in self.eq_of[x] if f in E) for x in X}
    meq, mx = {}, {}
    def aug(x, seen):
        for f in adj[x]:
            if f in seen: continue
            seen.add(f)
            if f not in meq or aug(meq[f], seen):
                meq[f] = x; mx[x] = f; return True
        return False
    for x in sorted(X):
        aug(x, set())
    bad = {x for x in X if x not in mx}; st = list(bad)
    while st:
        x = st.pop()
        for f in adj[x]:
            y = meq.get(f)
            if y is not None and y not in bad:
                bad.add(y); st.append(y)
    return sorted(X - bad), meq

def decode(self, now, e0):
    if e0 not in self.eqs:
        return
    E, X = self.component(e0)
    if len(X) > self.g["dec_comp_max"]:
        self.g["dec_comp_max"] = len(X)
    dec, meq = self.determined(E, X)
    if not dec:
        return
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
            del self.eqs[f]; self.eq_meta.pop(f, None)
    for s in dec:                                             # 2. then count and deliver each decoded unit once
        self.dec_flag[s] = 1
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
    for f in sorted(touched):                                 # 3. then the cascade, on equations still live
        if f in self.eqs:
            self.decode(now, f)

def dec_known(self, now, s):
    """s arrived as data: remove it from every equation, drop empty equations, and re-run decode on each touched equation still live."""
    touched = self.eq_of.pop(s, set())
    for f in touched:
        if f in self.eqs:
            self.eqs[f].discard(s)
            if not self.eqs[f]:
                del self.eqs[f]; self.eq_meta.pop(f, None)
    for f in sorted(touched):
        if f in self.eqs:
            self.decode(now, f)
```

Notes:
- **Readout T:** the `T` used by `within_lifetime` and `decoded_late` here is the declared `cfg.T` (readouts use the declared T, §17.5).
- **No recursion past what is needed.** `on_data(.., -1, ..)` for a decoded s must not call `dec_known` again: s was popped from `eq_of` before delivery, so the `s in self.eq_of` test of §5.4 is false.
- **R0e receiver:** the existing `r0e_arrival` path in `on_data` runs for a decode too, because a decoded unit enters the reorder buffer like an arrival.

## 6. Gauges and readouts (`summary`)

The new gauge counters of §4–§5 go into the existing `gauges` dict (`self.g`). Every new derived readout goes into one new top-level block, `out["rep3e"]`, so that the identity function `id3e` (§8.4) can exclude it as a whole. The block holds:
- `r_mean = r_sum/law_reads`, `W_mean`, `sat_pressure = sat_pressure_sum/law_reads`, `W_floor_bind_frac`, `W_cap_bind_frac`;
- `eps_est_mean`;
- `p_dec_meas = dec_units / max(len(orig_lost), 1)`;
- `p_fec_law`: the law's `p_fec_normal_as(r_cur, eps, W_cur, 1)` at the last reading;
- `rep_link_share` = repairs / all link datagrams over the windows with link busy ≥ 0.98 (from the `repair`, `first`, `resend` and `busy` window series);
- `rep_members_mean`;
- `recovery_time_0p15`: the existing `recovery_time` at `thr = 0.15`, computed where `recovery_time` is now;
- `mu_eff_hat` = C × first sends / link datagrams over the saturated windows after `oe` (link cells only).

- `share_by`: a dict `{τ: share}` for every τ in `cfg.tau_extra`, the share of units handed over that were delivered within τ (the existing `share_by_0p4` rule at another τ). RT runs with `tau_extra=(0.12,)`, and the reader reads `rep3e["share_by"][0.12]` as `share_by_0p12`.

Every new key is listed in `summary`'s docstring with its registration section.

## 7. Tests, red first (`test_toy2.py`, class `Stage3e`)

Write every test first and commit it failing (they fail on the missing names). Then write the code until they pass. Run `python -m unittest test_toy test_toy2 test_proof_s1`. Every existing test must still pass.

1. **Defaults.** `Cfg().repair is None`, `o_bias == R_bias == rev_q == 0`. A `proof-s1` configuration of each sender family (S5, C1, C3, E-rt) runs identically with the new code: compare every key and the trace against a stored digest made from `b2addffb` (the same mechanism as `check_ident_proof_s1.py`).
2. **The rate law against `raptorpath-math`.**
   - Every entry of `rate_ref3e.json` matches `rate_law` to 1e-9 in r and exactly in W.
   - The registration's §2.1 table holds to 1e-5.
   - The reference grid: ε ∈ {0.01, 0.03, 0.0344, 0.05, 0.1, 0.135, 0.3}, srtt ∈ {0.0115, 0.05, 0.1, 0.3}, rate ∈ {50, 1 000, 8 000, 10 000, 11 250}, δ ∈ {0.005, 0.5, 5, 50}.
   - The scratch crate is the registration's Appendix A program, extended to print r, W, `W_pre` and `r_sat` for the grid as JSON. It lives outside the repository, with `raptorpath-math` as a path dependency. Only its JSON output and its source in a code block of `results-stage3e.md` are committed.
3. **A&S ports.** `normal_survival_as` and `normal_quantile_as` match the reference JSON's values at z ∈ {−9, −3, −1, 0, 0.5, 2, 4.5, 9} and p ∈ {1e-12, 1e-7, 0.05, 0.5, 0.95, 1 − 1e-7}.
4. **δ = 0.005 gives r = 0 exactly:** `rate_law(0.03, 0.0115, 10000, 0.005)["r"] == 0.0`. So an RL run at δ = 0.005 equals the OFF run under `id3e` (§8.4) and in its trace (one short cell, 3 seeds).
5. **The span on constructed state** (subclass `Sim`, set `n_sent`, `h_seq`, `A_known`, `F_ann`, `verdicted`, `srtt`, then call `stage_repair` with `fwd` patched to record):
   - no bind;
   - a worth cut;
   - an age cut;
   - a floor cut;
   - an evict cut (the window and a shed seq at the low end);
   - an empty span counted as `rep_drop_edge` against `rep_drop_young`;
   - A* = 1 before the first RTT sample;
   - `lo ≥ ws = A_known − W`.
6. **The L1 control.**
   - RT at T = 0.04 with `repair = dict(r=0.3835, edge="worth")` and `repair_seam = "no_edge"`: `l1_viol_rep > 0`.
   - Without the seam: 0.
7. **The worth gauge.** OV (3 000 units) with `edge = "age"`: `worth_viol_rep > 0`. With `"worth"`: 0.
8. **The decoder on constructed systems** (call `on_repair` with fabricated payloads):
   - peeling;
   - a 2 × 2 block;
   - `{h1,h2}, {h1,h2}, {h1,h2,h3,h4}`, which decodes h1 and h2 only;
   - `{h1,h2}, {h2,h3}`, which decodes nothing;
   - a redundant equation (`rep_redundant`);
   - a cascade through an older equation on a later data arrival;
   - a decode after an H = 0 release (`dec_after_release`);
   - a decode after a floor pass (`late_floor_dec`);
   - a decode after T (`dec_late`, and `rep_late_only` for its equation).
9. **Emission.**
   - Over N first sends, `rep_credits == floor(N·r)` or one less, and `rep_sent + rep_drop_edge + rep_drop_young == rep_credits`.
   - At most one repair per first send (`owed_max < 1 + r`).
   - None while paused.
   - At r = 0: no kind-11 event, and `ctr` equal to the repair-off run.
10. **The loss draw** uses `SALT_REPAIR` and p0, and the outage drops repairs.
11. **The estimated loss.**
    - `beta_upper` equals a direct transcription of `estimator.rs` `beta_quantile(b, a, 0.95)` on 20 random (a, b) pairs to 1e-12.
    - A report batch listing 1 000 new holes after 4 000 received moves `eps_est` above 0.15.
    - A dequeue expiry changes `raw_rcv` and `raw_lost` by 0.
    - The cold value is `beta_upper(1, 1)`.
12. **Today's Realtime with repair.**
    - `shed_budget == residual_loss(eps, r_cur, astar_cur, 1)` after a run;
    - with `repair is None` it is p0, as in `proof-s1`.
13. **RTT sampling** ignores a decoded seq above the newest arrival (constructed).
14. **T = ∞ identity under repair:** S5 at T = ∞, H = ∞ with `repair = dict(r="law", edge="worth")` equals C3 with `repair = dict(r="law", edge="none")` under `id3e` and in the sender trace, in OV and CL (3 000 units each), seeds 0 to 2.
17. **The decoder counts each unit once:** on the 2 × 2 block `{h1,h2}, {h1,h2}`, `dec_units == 2`, `rep_used == 2` and `dup == 0`; on a cascade, every decoded seq is delivered once (`dlv` set once, no `dup`).
15. **Robustness hooks.**
    - `o_bias = 0.01` raises `o_hat` by exactly 0.01.
    - `R_bias` changes only the dequeue call.
    - `rev_q = 0.1` raises the mean srtt by about 0.1 in a 1 s run.
    - `owd_est = "rel"`: equal to `"fwd"` within 1e-9 on a cell with no reverse queue and no forward queue; with `rev_q = 0.1` and no forward queue, `o_hat` ≈ `rtt/2` within 1 ms while `"fwd"` reads about +0.05.
16. **The deny test** passes with the new `EXEMPT` names, and its planted positive control still fails as intended.

## 8. Runner (`batch3e.py`) and reader (`summarize3e.py`)

### 8.1 Cells (`CELLS`, keyed by name; each is `(loss cell, kwargs)`)

Built from `batch3d.CELLS` and `batch3.CELLS` by copying the dicts. Where a key is listed it overrides; `LINK_KW` is `batch3c.LINK_KW` (`pause_B` 1000).

| name | from | overrides |
|---|---|---|
| `CL` | `batch3d.CELLS["CL@b"]` | — |
| `CL-long` | `CL-long@b` | — |
| `OV` | `OV@b` | — |
| `OV-long` | `OV-long@b` | — |
| `CLw-long` | `CL-long@b` | `rate=8000.0, n_total=160000, dead=(0, 2.0, 3.0)` |
| `CLc` | `CL@b` | `dead=None` |
| `CLs-long` | `CL-long@b` | `rate=1000.0, n_total=20000, link=dict(C=1250.0, Q=125.0), pause_B=100` |
| `RQ` | `CL-long@b` | `rev_q=0.1` |
| `RT` | `batch3.CELLS["K-L10"]` | `dead=None` |
| `K-I2`, `K-L10` | `batch3.CELLS` | — |
| `K-OV` | `batch3.CELLS["K-L10"]` | `dead=None, link=dict(C=40.0, Q=40.0), t_limit=120.0, **LINK_KW` with `pause_B=8`, `sweep_form="blocker"`, `rx_refresh=True` |
| `E-I2`, `M-I2` | `batch3.CELLS` | — |
| `Ec` | `batch3.CELLS["E-I2"]` | `dead=None` |

- `windows=True` in every cell with an outage or a link.
- The video cells are `CLc`, `CL-long` and `OV-long` run with `delta=0.005` and τ = 2. They are named `CLc-v`, `CL-long-v` and `OV-long-v` in job names only.

### 8.2 Arm builders (each takes `cell, seed, delta, repair`, plus its dial)

- `new_cfg(cell, seed, T, H, delta, repair, variant=0, **kw)`: the field dict of `batch3d.s5_cfg` (`evidence_verdict=False, verdict_evidence="none", floor_law="protect", floor_carrier="header", deq_expiry=True, o_excl_verdict=False, rtt_est="rfc", tail_elig="P", trigger_law="trigger", owd_est="fwd", recheck_floor=True`), `sender="S5", recv="RH", rho=1.0, check_p1=False, T=T, Hk=H, delta=delta, repair=repair`. The edge is in `repair` (`"worth"` unless the arm says otherwise).
- `adr_cfg(cell, seed, rho, delta, repair)`: `sender="S1c", recv="R0e", rho=rho`, the cell's kwargs, `repair` with `edge="none"`.
- `rt_cfg(cell, seed, delta, repair)`: `proof_s1.rt_cfg`'s fields (`sender="S1", recv="R0e", rho=1.0, check_p1=False, d_arm="EWMA", budget="resid", floor_send=False, evict_window=200, r0e_max=500, pause_B=10**9`), `repair` with `edge="evict"`.
- `bulk_cfg(cell, seed, delta, repair)`: `sender="S1c", recv="R1", rho=1.0`, `repair` with `edge="none"`.
- **Repair modes:**
  - `OFF = None`;
  - `RL = dict(r="law", edge=…)`;
  - `RE = dict(r="est", edge=…)`;
  - fixed arms: `dict(r=0.0875 or 0.107 or 0.0998 or 0.116 or 0.2295 or 0.112 or 0.36, edge="worth")`, as the registration's §5 table gives per cell.
- **The MAIN arms** follow the registration's §5 table exactly:
  - `RL-age` uses `edge="age"`;
  - `RL-floor` uses `edge="floor"`;
  - `RL-Hinf` is `new_cfg(.., T=0.4, H=inf, repair=RL)`;
  - `R0` is `dict(r=0.0, edge="worth")`.

### 8.3 Parts

The parts are MAIN, DECIDE, CROSS, AA (registered A/A), AA2, AMEND, ROBUST, SMOOTH and DOUBLE. Each is the list of jobs in the registration:
- MAIN: §5;
- DECIDE: §13.5;
- CROSS: `proof_s1`'s two cross jobs plus `adr_cfg(.., 0.99)` with `recv="RH", Hk=0` and `sender="S1"`, repair off;
- AMEND: §13.2's cell table × repair modes × {new sweep, the H = ∞ points, ADR-0084 ρ set, today's Realtime δ set, today's Bulk}; the file cells' identity rows; the δ = 0.005 RL and RE rows with 3 seeds;
- ROBUST and SMOOTH: §13.2.

**Job keys.** Every job is keyed by a fingerprint: the `Cfg`'s `asdict` with `seed` removed, as a sorted JSON string, hashed with sha256. A fingerprint appears once across all parts; a second request reuses the first's files. `--list` prints per part the configurations and runs after this dedupe. **Report that final count** in `results-stage3e.md` before any decision part runs.

**Seeds.** 12 (0 to 11). The δ = 0.005 identity rows take 3. DOUBLE runs seeds 12 to 23 of the fingerprints that the reader names (§8.5).

**Output.** One store for every part: `out3e/runs/<fingerprint>-<seed>.json`, written tmp + rename, skipped if present (resumable), as `batch3d.py`. A part's job list is `out3e/<PART>.json`, mapping each job name to its fingerprint, so a fingerprint shared by two parts is one file.

**Workers.**
- `--workers` at most 2;
- long cells (`*-long*`, `OV-long`) only with `--workers 1`; the runner refuses otherwise;
- order inside a part: as the registration's §13.6, longest runs last.

### 8.4 Markers and stops

- Markers live in `out3e/`: `PASS-ident`, `PASS-r0`, `PASS-rttbug`, `PASS-rate`, `PASS-simread`, `PASS-aa`, then `DONE-<PART>` per part.
- `batch3e.py --part X` refuses unless every earlier marker of the registration's §9 and §13.6 exists, and refuses everything while `out3e/STOP` exists.
- `PASS-rttbug` and `PASS-simread` are written by the coordinator, not by the runner.
- **After each run, the runner checks these and, on any failure, writes `STOP` (with the job, the seed and the failing key) and exits non-zero:**
  - `finished`;
  - `l1_viol == 0`;
  - `l1_viol_rep == 0`;
  - `commit_viol == 0`;
  - `worth_viol_rep == 0` under the worth or floor edge;
  - for the T = ∞, H = ∞ new arm and its same-mode today's Bulk arm, both present at the same seed: `id3e` empty and the sender traces equal;
  - for a δ = 0.005 RL or RE row and its OFF row: `id3e` empty and the traces equal.
- **`id3e(a, b)`** is `test_toy2.id_diff` with `"rep3e"` added to the excluded keys: every top-level key outside `test_toy2.ID_EXCL` and `rep3e`, plus `test_toy2.ID_GAUGES`. That is the key set of `Stage3d.test_2`. Import `ID_EXCL` and `ID_GAUGES` from `test_toy2`; do not copy them.

### 8.5 Reader (`summarize3e.py`)

Import from `proof_s1`: `val`, `vec`, `boot`, `read_good`, `t_bar`, `nf_of`, `merge_points`, `curve_at` (with `clamp_low=True`), `compare`, `compare_rev`, `classify` and `bracket`. Do not copy them.

**Per cell, per repair mode, per y readout:**
- y is `share_by_0p4` (τ = 0.4) or `share_by_2` (video). In outage cells, `recovery_time` is a second y, with "not within the run" censored at the readable end as `proof_s1` does. In file cells, `finish` and `stall_max` are read for identity only.
- Build the new curve from the H = 0 sweep and compare every baseline point, then ADR-0084's curve both ways.
- Scope "verdict": ADR-0084 in every mode; today's Bulk and the object API in every mode; today's Realtime at δ = 50 in RL and RE.
- Scope "reported": today's Realtime in OFF, and today's Realtime at δ ≠ 50 in every mode.

**Outputs:**
- `classify` gives each cell's outcome.
- The undetermined comparisons are listed with the fingerprints to double. These are the baseline point and the two bracketing new points from `bracket`, or the lowest point under the dominance rule. They are written to `out3e/double-<PART>.json`, which DOUBLE runs once. The reader re-reads with 24 seeds and never asks for a second doubling.
- The A/A floor is frozen as `nf_stage3e.json` (`proof_s1.freeze`'s rule) before any decision part, extended to AA2's cells.
- The stage verdict follows the registration's §13.3.
- Every reading is written to `results-stage3e.md`, losses included, with rows named by behaviour (F6).

## 9. Order of work and commits

1. Commit the tests of §7 (failing), with the message "stage 3e: red tests".
2. Build the scratch crate, generate `rate_ref3e.json`, and commit it.
3. Write §2–§6 and commit when §7 passes, with the message "stage 3e: repair, decoder, span edges, estimated-loss arm, gauges".
4. Write `check_ident3e.py`: the registration's §9 steps 2–3 and §13.1, that is, `proof-s1` and `check_ident3d.py` configurations with every new field at its default, and R0 against repair off. Run it alone and commit its result line.
5. Write `batch3e.py` and `summarize3e.py`, with tests for the reader on synthetic curves as `test_proof_s1.py` has them. Commit them, and add the `--list` count to `results-stage3e.md`.
6. **One timing run** (the only run besides the tests and the identity): today's Bulk with RL in CL-long, seed 0, alone on the machine. Record its wall time and `dec_comp_max` in `results-stage3e.md`. Edge-`"none"` components after an outage can be large; if the run exceeds 30 min, stop and report before anything else.
7. Push `toy-stage3e` (no force, scan empty) and report:
   - the head;
   - the test count;
   - the identity result;
   - the deduplicated configuration and run counts per part;
   - any place where this brief could not be followed as written.
