#!/usr/bin/env python3
"""The scorer of the disorder-price (omega) measurement: c_R, c_H, omega and every registered label
(docs/derivations/disorder-price-registration.md 1.2, 4.4, 7, 8, 9).

  python -I omega_score.py <ledger.jsonl> [--boot N] [--seed S] [--json out.json]
  python -I omega_score.py calibrate <ledger.jsonl>        per-configuration s2_rep, design effect, ICC and n (session R's calibration rows)

Exit: 0 scored; 2 nothing scored (TRUNCATED, REHEARSAL, UNSCOREABLE, no footer); 3 usage.

Rules that hold everywhere (the brief and the registration):
  * A truncated or footerless ledger is never scored: the scorer prints `TRUNCATED` and the n reached per cell, and no c, omega, outcome or PASS.
  * A rehearsal ledger is never scored (`REHEARSAL-NOT-SCORED`).
  * A missing field is never read as 0: a row with a missing required field is a MISSING row, void, and counts against the cell's live minimum.
  * A cell under 6 live reps, or under 32 valid rank-1 REL / 32 valid HOLD events, is UNSCOREABLE.

Ledger lines (JSON, one per line): {"kind":"header",...}, {"kind":"row",...}, {"kind":"footer","state":"DONE-ALL"|"TRUNCATED-<s>"|"FAILED-ALL"}.
Row: cfg arm armkind label rep seedhalf stage status tau_q_ms B witness events|pseudo good_bytes window_s n_events; an event: D idle retx drops
valid rank rank_in_rep. The row's own fields are produced by omega_parse.py digest.
"""
import json
import math
import random
import sys

MSS = 1448.0          # TCP payload of a 1500 B packet with timestamps; registration: MSS/S ~ 1.2
S_SYM = 1200.0        # symbol_size (perf.rs:52)
K = MSS / S_SYM
DELTA = {"Bulk": 0.005, "Auto": 0.5, "Realtime": 50.0}
HOLD_SETS = {"Bulk": (1.0, 61.0), "Auto": (1.0, 26.0), "Realtime": (0.0, 0.35)}
EDGES = [0.005, 0.26, 0.5, 26.0, 50.0, 2600.0]
REGIONS = ["OMEGA-Z0", "OMEGA-ZB", "OMEGA-GAP-BA", "OMEGA-ZA", "OMEGA-GAP-AR", "OMEGA-ZR", "OMEGA-Z6"]
DEC = {   # 9.2: (Bulk, Auto, Realtime) -> (token, default change)
    "OMEGA-Z0": (("RELEASE-BEFORE-FIRST-REPAIR", True), ("RELEASE-BEFORE-FIRST-REPAIR", True), ("RELEASE-BEFORE-FIRST-REPAIR", False)),
    "OMEGA-ZB": (("STEEP-AT-Bulk", False), ("RELEASE-AT-DETECT", True), ("RELEASE-AT-DETECT", False)),
    "OMEGA-GAP-BA": (("KEEPS-ORDER", False), ("RELEASE-BEFORE-FIRST-REPAIR", True), ("RELEASE-AT-DETECT", False)),
    "OMEGA-ZA": (("KEEPS-ORDER", False), ("STEEP-AT-Auto", True), ("RELEASE-BEFORE-FIRST-REPAIR", False)),
    "OMEGA-GAP-AR": (("KEEPS-ORDER", False), ("KEEPS-ORDER", False), ("RELEASE-BEFORE-FIRST-REPAIR", False)),
    "OMEGA-ZR": (("KEEPS-ORDER", False), ("KEEPS-ORDER", False), ("STEEP-AT-Realtime", False)),
    "OMEGA-Z6": (("KEEPS-ORDER", False), ("KEEPS-ORDER", False), ("HOLD-THROUGH-FIRST-REPAIR", True)),
}
PRESETS = ("Bulk", "Auto", "Realtime")
MIN_LIVE_REPS = 6
MIN_EVENTS = 32
ENV_DT_MAX_MS = 61.0
EVENT_REQ = ("D", "idle", "retx", "drops", "valid", "rank", "rank_in_rep")
ROW_REQ = ("cfg", "arm", "armkind", "label", "rep", "stage", "status", "witness")
NONSCORED_STAGES = ("R", "calib", "smoke")


# ───────────────────────── small statistics ─────────────────────────
def pct(sorted_vals, q):
    n = len(sorted_vals)
    if n == 0:
        return None
    i = q * (n - 1)
    lo, hi = int(math.floor(i)), int(math.ceil(i))
    return sorted_vals[lo] + (sorted_vals[hi] - sorted_vals[lo]) * (i - lo)


def ci(samples, alpha=0.05):
    if not samples:
        return None
    return pct(samples, alpha / 2.0), pct(samples, 1 - alpha / 2.0)


def ratio(reps, ks, kn, idx=None):
    it = reps if idx is None else (reps[j] for j in idx)
    s = n = 0.0
    for r in it:
        s += r[ks]
        n += r[kn]
    return s / n if n else None


def wilson(x, n, z=1.96):
    if n <= 0:
        return None
    p = x / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return p, c - h, c + h


def newcombe(x1, n1, x2, n2):
    """Newcombe hybrid-score 95% interval of p1 - p2 (here HOLD - CLEAN non-free shares); (diff, lo, hi) or None."""
    a, b = wilson(x1, n1), wilson(x2, n2)
    if not a or not b:
        return None
    d = a[0] - b[0]
    return d, d - math.sqrt((a[0] - a[1]) ** 2 + (b[2] - b[0]) ** 2), d + math.sqrt((a[2] - a[0]) ** 2 + (b[0] - b[1]) ** 2)


def design_effect(per_rep):
    """DE = 1 + (m-1) ICC from per-rep (successes, n) with a one-way ANOVA ICC clamped to [0,1]; >= 1."""
    reps = [(x, n) for x, n in per_rep if n > 0]
    k = len(reps)
    if k < 2:
        return 1.0
    N = sum(n for _, n in reps)
    if N <= k:
        return 1.0
    p = sum(x for x, _ in reps) / N
    m = N / k
    ssb = sum(n * (x / n - p) ** 2 for x, n in reps)
    ssw = sum(x * (1 - x / n) for x, n in reps)
    msb, msw = ssb / (k - 1), ssw / (N - k)
    n0 = (N - sum(n * n for _, n in reps) / N) / (k - 1)
    icc = max(0.0, min(1.0, (msb - msw) / (msb + (n0 - 1) * msw))) if (msb + (n0 - 1) * msw) > 0 else 0.0
    return max(1.0, 1 + (m - 1) * icc)


def chi2_lower(df, p=0.20):
    """Wilson-Hilferty approximation of the p-quantile of chi-square(df); p = 0.20 only (z = -0.8416)."""
    z = -0.8416212335729143
    a = 2.0 / (9.0 * df)
    return df * (1 - a + z * math.sqrt(a)) ** 3


def n_formula(s2_R, s2_H, R_df, omega_pred=1.2):
    """7.2: n = ceil(1.96^2 (s2_R,rep + s2_H,rep) / h^2), h = 0.3 max(omega_pred, 1.2), each variance at its upper 80% chi-square bound; clamped to [8, 24]."""
    up = lambda s2, df: s2 * df / chi2_lower(df)
    h = 0.3 * max(omega_pred, 1.2)
    n = math.ceil(1.96 ** 2 * (up(s2_R, R_df) + up(s2_H, R_df)) / (h * h))
    return max(8, min(24, n)), n


def mde_slot(sd_rep, n):
    return 2.8 * sd_rep * math.sqrt(2.0 / n) * K


# ───────────────────────── labels: pure decision functions ─────────────────────────
def classify(lo, hi):
    """9.2 zone of an omega interval: a region label, OMEGA-EDGE-<e> (exactly one boundary strictly inside), or NEEDS-MORE-POWER (more than one)."""
    inside = [e for e in EDGES if lo < e < hi]
    if len(inside) > 1:
        return "NEEDS-MORE-POWER"
    if len(inside) == 1:
        return "OMEGA-EDGE-%g" % inside[0]
    m = (lo + hi) / 2.0
    if hi < EDGES[0]:
        return REGIONS[0]
    if lo > EDGES[5]:
        return REGIONS[6]
    for i, e in enumerate(EDGES[1:], 1):
        if m < e or (m == e and i in (1, 3, 5)):     # zones are closed at their upper edge, gaps open
            return REGIONS[i]
    return REGIONS[6]


def region_of(lo, hi):
    c = classify(lo, hi)
    return c if c in REGIONS else None


def decide_9_2(label):
    """Per-preset outcome for a classify() label: {preset: (token, default_change)}. An EDGE reads both adjacent rows: decided where they agree, else NEEDS-MORE-POWER."""
    if label in DEC:
        return dict(zip(PRESETS, DEC[label]))
    if label.startswith("OMEGA-EDGE-"):
        e = float(label[len("OMEGA-EDGE-"):])
        i = EDGES.index(e)
        a, b = DEC[REGIONS[i]], DEC[REGIONS[i + 1]]
        return {p: (a[j] if a[j] == b[j] else ("NEEDS-MORE-POWER", False)) for j, p in enumerate(PRESETS)}
    return {p: ("NEEDS-MORE-POWER", False) for p in PRESETS}


def rent(preset, dt_ms, d_ms):
    """J_U's rent for a stall: delta_p * dt / d, in symbols."""
    return DELTA[preset] * dt_ms / d_ms


def rent_line(preset, side, cH, lo, hi, rent_v, floor):
    """Non-Bulk 9.1 per-arm outcome. cH point, [lo, hi] its 95% CI. UNDERPRICED needs lo > rent and cH above the A/A floor; COVERED needs hi <= rent."""
    if floor is None:
        return "RENT-UNDECIDED-%s-%s" % (preset, side)           # no A/A floor, no claim (rule 5)
    if lo > rent_v and cH > floor:
        return "RENT-UNDERPRICED-%s-%s" % (preset, side)
    if hi <= rent_v:
        return "RENT-COVERED-%s-%s" % (preset, side)
    return "RENT-UNDECIDED-%s-%s" % (preset, side)


def bulk_line(side, x_hi, x_lo, c_hi, c_lo, rent_v):
    """Bulk count rule (9.1): COVERED iff upper(x) upper(cnf) <= rent; UNDERPRICED iff lower(x) lower(cnf) > rent; else UNDECIDED."""
    if None in (x_hi, x_lo, c_hi, c_lo):
        return "RENT-UNDECIDED-Bulk-%s" % side
    if x_lo * c_lo > rent_v:
        return "RENT-UNDERPRICED-Bulk-%s" % side
    if x_hi * c_hi <= rent_v:
        return "RENT-COVERED-Bulk-%s" % side
    return "RENT-UNDECIDED-Bulk-%s" % side


def combine_lines(preset, side, lines):
    """One verdict line per (preset, side, queue level) from its per-arm lines. UNDERPRICED at some arm wins; COVERED only if every arm is covered."""
    if not lines:
        return "NO-ARMS-%s-%s" % (preset, side)
    if any(l.startswith("RENT-UNDERPRICED") for l in lines):
        return "RENT-UNDERPRICED-%s-%s" % (preset, side)
    if all(l.startswith("RENT-COVERED") for l in lines):
        return "RENT-COVERED-%s-%s" % (preset, side)
    return "RENT-UNDECIDED-%s-%s" % (preset, side)


def envelope(intervals):
    """7.4 item 1: [max lo, max hi] over the arms' (Bonferroni) intervals, so one noisy arm cannot lift it. None if empty."""
    if not intervals:
        return None
    return max(i[0] for i in intervals), max(i[1] for i in intervals)


def varies(pairs):
    """7.4 items 2-3. pairs: [(name_a, (lo,hi), name_b, (lo,hi), factor)]: a VARIES-<factor> label for each pair in different single regions with disjoint intervals."""
    out = []
    for na, ia, nb, ib, fac in pairs:
        ra, rb = region_of(*ia), region_of(*ib)
        if ra and rb and ra != rb and (ia[1] < ib[0] or ib[1] < ia[0]):
            out.append(("OMEGA-VARIES-%s" % fac, na, nb))
    return out


def steep(preset, om1, om_inf, d_ms, sigma_ms, skew_ms=20.0):
    """STEEP-AT-<preset> (8): omega_1's interval meets a round band (k = 1..3), or omega_inf's meets the skew band."""
    hits = []
    for k in (1, 2, 3):
        tau = d_ms + (k - 1) * 25.0
        star = DELTA[preset] * tau / d_ms
        b = max(0.02, 2 * sigma_ms / tau)
        a, z = star / (1 + b), star * (1 + b)
        if om1 and om1[0] <= z and om1[1] >= a:
            hits.append("round%d" % k)
    star = DELTA[preset] * skew_ms / d_ms
    b = max(0.02, 2 * sigma_ms / skew_ms)
    if om_inf and om_inf[0] <= star * (1 + b) and om_inf[1] >= star / (1 + b):
        hits.append("skew")
    return ("STEEP-AT-%s" % preset, hits) if hits else None


def skew_verdict(preset, om_inf, d_ms, skew_ms=20.0):
    """N = 2: keeps order iff omega_inf >= delta_p * skew / d."""
    if not om_inf:
        return "UNSCOREABLE"
    thr = DELTA[preset] * skew_ms / d_ms
    if om_inf[0] >= thr:
        return "KEEPS-ORDER"
    if om_inf[1] < thr:
        return "REORDERS-EVERY-SLOW-PATH-PACKET"
    return "NEEDS-MORE-POWER"


# ───────────────────────── ledger ─────────────────────────
def load(path):
    header, rows, footer = {}, [], None
    with open(path) as f:
        for ln, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                o = json.loads(line)
            except ValueError:
                raise SystemExit("ledger %s line %d is not JSON" % (path, ln))
            k = o.get("kind")
            if k == "header":
                header = o
            elif k == "row":
                rows.append(o)
            elif k == "footer":
                footer = o
    return header, rows, footer


def missing_fields(row):
    miss = [k for k in ROW_REQ if k not in row or row[k] is None]
    if row.get("status") == "OK" and not miss:
        k = row["armkind"]
        if k == "CLEAN":
            if not isinstance(row.get("pseudo"), dict) or not row["pseudo"]:
                miss.append("pseudo")
            else:
                for lab, evs in row["pseudo"].items():
                    miss += ["pseudo[%s].%s" % (lab, f) for e in evs for f in EVENT_REQ if f not in e or e[f] is None]
        else:
            if not isinstance(row.get("events"), list):
                miss.append("events")
            else:
                miss += ["events.%s" % f for e in row["events"] for f in EVENT_REQ if f not in e or e[f] is None]
                miss = sorted(set(miss))
    return miss


def classify_rows(rows):
    """-> (live rows, counts). A row is live when status OK, not rehearsal and no field missing."""
    live, cnt = [], {"live": 0, "void": 0, "no_fill": 0, "no_data": 0, "missing": 0}
    for r in rows:
        m = missing_fields(r)
        if m:
            r["_missing"] = m
            cnt["missing"] += 1
        elif r["status"] == "OK":
            live.append(r)
            cnt["live"] += 1
        elif r["status"] == "NO_FILL":
            cnt["no_fill"] += 1
        elif r["status"] == "NO_DATA":
            cnt["no_data"] += 1
        else:
            cnt["void"] += 1
    return live, cnt


# ───────────────────────── per-arm data for the bootstrap ─────────────────────────
def _vsum(evs):
    v = [e["D"] for e in evs if e["valid"]]
    return sum(v), len(v)


def rep_rel(r):
    ev = r["events"]
    s1, n1 = _vsum([e for e in ev if e["rank"] == 1])
    s2, n2 = _vsum([e for e in ev if e["rank"] >= 2])
    return {"s1": s1, "n1": n1, "s2": s2, "n2": n2, "E": r.get("n_events", len(ev)), "G": r.get("good_bytes"), "half": r.get("seedhalf"), "rep": r["rep"]}


def rep_hold(r):
    ev = r["events"]
    s, n = _vsum(ev)
    s1, n1 = _vsum([e for e in ev if e["rank_in_rep"] == 1])
    s2, n2 = _vsum([e for e in ev if e["rank_in_rep"] > 1])
    nf = [e for e in ev if e["valid"] and not (e["idle"] == 0 and e["drops"] == 0)]
    return {"s": s, "n": n, "s1": s1, "n1": n1, "s2": s2, "n2": n2, "E": r.get("n_events", len(ev)), "G": r.get("good_bytes"), "half": r.get("seedhalf"),
            "rep": r["rep"], "nf": len(nf), "nv": n, "snf": sum(e["D"] for e in nf), "nnf": len(nf)}


def rep_clean(r, lab):
    s, n = _vsum(r["pseudo"].get(lab, []))
    ev = [e for e in r["pseudo"].get(lab, []) if e["valid"]]
    nf = [e for e in ev if not (e["idle"] == 0 and e["drops"] == 0)]
    return {"s": s, "n": n, "half": r.get("seedhalf"), "G": r.get("good_bytes"), "rep": r["rep"], "nf": len(nf), "nv": n}


def build_cells(live):
    cells = {}
    for r in live:
        cells.setdefault((r["stage"], r["cfg"], r["arm"]), []).append(r)
    return cells


def scoreable(reps, kind):
    """(ok, why) for a REL or HOLD cell: >= 6 live reps and >= 32 valid rank-1 REL (HOLD: any rank) events."""
    if len(reps) < MIN_LIVE_REPS:
        return False, "%d live reps (< %d)" % (len(reps), MIN_LIVE_REPS)
    n = sum(r["n1"] for r in reps) if kind == "REL" else sum(r["n"] for r in reps)
    if n < MIN_EVENTS:
        return False, "%d valid %s events (< %d)" % (n, "rank-1 REL" if kind == "REL" else "HOLD", MIN_EVENTS)
    return True, ""


# ───────────────────────── the pair: c_R, c_H, omega ─────────────────────────
def boot_pair(R, H, C, nboot, rng, hold_first=False):
    """Cluster bootstrap: each iteration resamples reps within REL, HOLD and CLEAN; CLEAN's draw is shared by c_R and c_H.
    Returns point values and sorted samples for cR1, cRinf, cH, om1, ominf, guard. Any part whose arm is missing is None."""
    hk = ("s1", "n1") if hold_first else ("s", "n")

    def point(idxs):
        dc = ratio(C, "s", "n", idxs[2]) if C else None
        o = {"dc": dc}
        if R and dc is not None:
            o["cR1"] = _k(ratio(R, "s1", "n1", idxs[0]), dc)
            o["cRinf"] = _k(ratio(R, "s2", "n2", idxs[0]), dc)
        if H and dc is not None:
            o["cH"] = _k(ratio(H, hk[0], hk[1], idxs[1]), dc)
        if "cH" in o and o["cH"] is not None:
            for a, b in (("om1", "cR1"), ("ominf", "cRinf")):
                if o.get(b) is not None:
                    o[a] = o[b] - o["cH"]
        o["guard"] = _guard(R, H, C, idxs)
        return o

    full = point((None, None, None))
    samples = {k: [] for k in ("cR1", "cRinf", "cH", "om1", "ominf", "guard")}
    nR, nH, nC = len(R or ()), len(H or ()), len(C or ())
    for _ in range(nboot):
        idxs = (rng.choices(range(nR), k=nR) if nR else None, rng.choices(range(nH), k=nH) if nH else None, rng.choices(range(nC), k=nC) if nC else None)
        o = point(idxs)
        for k in samples:
            if o.get(k) is not None:
                samples[k].append(o[k])
    for k in samples:
        samples[k].sort()
    return full, samples


def _k(d, dc):
    return None if d is None or dc is None else (d - dc) * K


def _mean(reps, key, idx):
    v = [reps[j][key] for j in (idx if idx is not None else range(len(reps)))]
    return sum(v) / len(v) if v and None not in v else None


def _guard(R, H, C, idxs):
    """Goodput guard (1.2): [ (G_C - G_R)/E_R - (G_C - G_H)/E_H ] / S. None when any goodput or event count is absent."""
    if not (R and H and C):
        return None
    gc, gr, gh = _mean(C, "G", idxs[2]), _mean(R, "G", idxs[0]), _mean(H, "G", idxs[1])
    er, eh = _mean(R, "E", idxs[0]), _mean(H, "E", idxs[1])
    if None in (gc, gr, gh, er, eh) or er <= 0 or eh <= 0:
        return None
    return ((gc - gr) / er - (gc - gh) / eh) / S_SYM


def aa_floor(C):
    """CLEAN first half against its second through the same estimator (rule 5): |D1 - D2| * MSS/S; None if a half is empty."""
    a = [r for r in C if r["half"] == 1]
    b = [r for r in C if r["half"] == 2]
    da, db = ratio(a, "s", "n"), ratio(b, "s", "n")
    return None if da is None or db is None else abs(da - db) * K


def hold_rank_dependent(H, nboot, rng):
    """4.4: the first hold of a rep against the later ones, same estimator; True when their CIs are disjoint (HOLD-RANK-DEPENDENT)."""
    if sum(r["n1"] for r in H) < 2 or sum(r["n2"] for r in H) < 2:
        return False
    a, b = [], []
    for _ in range(nboot):
        ix = rng.choices(range(len(H)), k=len(H))
        x, y = ratio(H, "s1", "n1", ix), ratio(H, "s2", "n2", ix)
        if x is not None:
            a.append(x)
        if y is not None:
            b.append(y)
    a.sort()
    b.sort()
    ca, cb = ci(a), ci(b)
    return bool(ca and cb and (ca[1] < cb[0] or cb[1] < ca[0]))


# ───────────────────────── the whole ledger ─────────────────────────
def lab_of(rows):
    return rows[0]["label"]


def score(path, nboot=10000, seed=1):
    header, rows, footer = load(path)
    out = {"lines": [], "notes": [], "rent": {}, "header": {k: v for k, v in header.items() if k != "kind"}}
    L = out["lines"]
    state = (footer or {}).get("state")
    if header.get("rehearsal") or any(r.get("rehearsal") for r in rows):
        L.append("REHEARSAL-NOT-SCORED")
        out["exit"] = 2
        return out
    truncated = (state is None or state.startswith("TRUNCATED") or header.get("truncated") or any(r.get("truncated") for r in rows))
    if truncated:
        why = "no footer" if state is None else state
        reached = {}
        for r in rows:
            if r.get("stage") not in NONSCORED_STAGES:
                reached[(r["cfg"], r["arm"])] = reached.get((r["cfg"], r["arm"]), 0) + (r.get("status") == "OK")
        L.append("TRUNCATED %s" % why)
        L.append("n reached per cell (live reps; nothing is scored): " + ", ".join("%s/%s=%d" % (c, a, n) for (c, a), n in sorted(reached.items())))
        out["exit"] = 2
        out["measurement"] = "TRUNCATED"
        return out
    if state != "DONE-ALL":
        L.append("UNSCOREABLE ledger state %s (only DONE-ALL is scored)" % state)
        out["exit"] = 2
        return out
    rows = [r for r in rows if r.get("stage") not in NONSCORED_STAGES]
    live, cnt = classify_rows(rows)
    out["row_counts"] = cnt
    for r in rows:
        if r.get("_missing"):
            out["notes"].append("MISSING-FIELD %s/%s rep %s: %s" % (r.get("cfg"), r.get("arm"), r.get("rep"), ",".join(r["_missing"][:6])))
    # a stage whose witnesses fail at >= 25% of its reps is VOID (9.3)
    voidstage = []
    for st in sorted({r["stage"] for r in rows}):
        rs = [r for r in rows if r["stage"] == st]
        bad = sum(1 for r in rs if r["status"] == "VOID-WITNESS" or r.get("_missing"))
        if rs and bad / len(rs) >= 0.25:
            voidstage.append(st)
            L.append("VOID stage %s (%d of %d rows failed their witness)" % (st, bad, len(rs)))
    live = [r for r in live if r["stage"] not in voidstage]
    cells = build_cells(live)
    rng = random.Random(seed)
    d_ms = float(header.get("d_ms", 11.0))
    if "d_ms" not in header:
        out["notes"].append("d_ms absent from the header: the registered default 11 ms stands")
    sig = float(header.get("sigma_R_ms", 4.0))
    skew = float(header.get("skew_ms", 20.0))
    pairs = {}
    scored_cfg = {}
    cfgs = sorted({c for (st, c, a) in cells if st == "D"})
    for cfg in cfgs:
        clean_rows = cells.get(("D", cfg, "CLEAN"), [])
        arms_ = sorted({a for (st, c, a) in cells if st == "D" and c == cfg and a != "CLEAN"})
        labels = sorted({a.split("@", 1)[1] for a in arms_})
        for lab in labels:
            R = [rep_rel(r) for r in cells.get(("D", cfg, "REL@" + lab), [])]
            H = [rep_hold(r) for r in cells.get(("D", cfg, "HOLD@" + lab), [])]
            C = [rep_clean(r, lab) for r in clean_rows if lab in r["pseudo"]]
            dt_ms = next((r["dt_ms"] for r in cells.get(("D", cfg, "HOLD@" + lab), cells.get(("D", cfg, "REL@" + lab), [])) if "dt_ms" in r), None)
            p = {"cfg": cfg, "label": lab, "dt_ms": dt_ms, "why": []}
            okc, whyc = (len(C) >= MIN_LIVE_REPS and sum(c["n"] for c in C) >= 1), "CLEAN: %d live reps, %d valid pseudo-events" % (len(C), sum(c["n"] for c in C))
            okr, whyr = scoreable(R, "REL") if R else (False, "no REL rows")
            okh, whyh = scoreable(H, "HOLD") if H else (False, "no HOLD rows")
            if not okc:
                p["why"].append(whyc)
            if not okr:
                p["why"].append("REL " + whyr)
            if not okh:
                p["why"].append("HOLD " + whyh)
            hr = bool(okh and hold_rank_dependent(H, min(nboot, 2000), rng))
            if hr:
                L.append("HOLD-RANK-DEPENDENT %s %s (omega_1 uses the rep's first holds only)" % (cfg, lab))
            full, smp = boot_pair(R if okr and okc else None, H if okh and okc else None, C if okc else None, nboot, rng, hold_first=hr)
            p["full"], p["smp"] = full, smp
            p["floor"] = aa_floor(C) if okc else None
            p["tau_q_ms"] = next((r["tau_q_ms"] for r in clean_rows + cells.get(("D", cfg, "HOLD@" + lab), []) if r.get("tau_q_ms") is not None), None)
            p["maxD"] = max([e["D"] for r in cells.get(("D", cfg, "HOLD@" + lab), []) for e in r["events"] if e["valid"]] or [0])
            p["H"] = H if okh else None
            p["C"] = C if okc else None
            pairs[(cfg, lab)] = p
        scored_cfg[cfg] = any(p["smp"]["om1"] for (c, _), p in pairs.items() if c == cfg)

    def cis(p, k, alpha=0.05):
        return ci(p["smp"][k], alpha) if p["smp"][k] else None

    # 9.1: is the stall priced
    for q in ("Qt", "Qs"):
        for pre in PRESETS:
            for side in ("within", "beyond"):
                lines, arms_used, unscored = [], 0, 0
                for (cfg, lab), p in sorted(pairs.items()):
                    if not cfg.endswith(q) or p["dt_ms"] is None:
                        continue
                    lo_, hi_ = HOLD_SETS[pre]
                    if not (lo_ <= p["dt_ms"] <= hi_ + 1e-9):
                        continue
                    tq = p["tau_q_ms"]
                    if tq is None:
                        unscored += 1
                        continue
                    if (p["dt_ms"] <= tq) != (side == "within"):
                        continue
                    rv = rent(pre, p["dt_ms"], d_ms)
                    if pre == "Bulk":
                        ln = bulk_arm(p, rv, side, rng, min(nboot, 2000))
                    else:
                        c = cis(p, "cH")
                        ln = "UNSCOREABLE" if c is None else rent_line(pre, side, p["full"]["cH"], c[0], c[1], rv, p["floor"])
                    if ln == "UNSCOREABLE":
                        unscored += 1
                        continue
                    lines.append(ln)
                if not lines and unscored:
                    out["rent"]["%s/%s/%s" % (q, pre, side)] = "UNSCOREABLE"
                    L.append("UNSCOREABLE-%s-%s at %s (no scoreable arm)" % (pre, side, q))
                    continue
                v = combine_lines(pre, side, lines)
                if unscored and v.startswith("RENT-COVERED"):
                    v = "RENT-UNDECIDED-%s-%s" % (pre, side)
                out["rent"]["%s/%s/%s" % (q, pre, side)] = v
    for key, v in sorted(out["rent"].items()):
        q = key.split("/")[0]
        tag = "at " + ("Q_t" if q == "Qt" else "Q_s")
        if v.startswith("RENT-UNDERPRICED") and q == "Qs" and not out["rent"].get("Qt" + key[2:], "").startswith("RENT-UNDERPRICED"):
            tag += " only"
        L.append("%s %s" % (v, tag))
    # omega negative, window short
    for (cfg, lab), p in sorted(pairs.items()):
        c1 = cis(p, "om1")
        if c1 and c1[1] < 0 and p["floor"] is not None and abs(p["full"]["om1"]) > p["floor"]:
            L.append("OMEGA-NEGATIVE %s %s (omega_1 upper CI %.3g < 0; implies RENT-UNDERPRICED at every preset at %s)" % (cfg, lab, c1[1], cfg.split("-")[-1]))
        g = cis(p, "guard")
        if g and p["full"].get("ominf") is not None and g[0] > p["full"]["ominf"]:
            L.append("WINDOW-SHORT %s %s (goodput guard lower CI %.3g > slot omega_inf %.3g; the slot estimates there are VOID)" % (cfg, lab, g[0], p["full"]["ominf"]))
            p["smp"]["om1"], p["smp"]["ominf"] = [], []
    # 7.4: omega per configuration (Bonferroni envelope over dt <= 61 ms), then across configurations
    env = {}
    for cfg in cfgs:
        for key in ("om1", "ominf"):
            ps = [p for (c, _), p in pairs.items() if c == cfg and p["smp"][key] and p["dt_ms"] is not None and p["dt_ms"] <= ENV_DT_MAX_MS]
            m = len(ps)
            e = envelope([ci(p["smp"][key], 0.05 / m) for p in ps]) if m else None
            env[(cfg, key)] = e
        # VARIES within the configuration: arms in different zones with disjoint CIs
        ps = [(lab, ci(p["smp"]["om1"])) for (c, lab), p in sorted(pairs.items()) if c == cfg and p["smp"]["om1"] and p["dt_ms"] <= ENV_DT_MAX_MS]
        for tag, a, b in varies([(a[0], a[1], b[0], b[1], "DT") for i, a in enumerate(ps) for b in ps[i + 1:]]):
            L.append("%s %s %s vs %s" % (tag, cfg, a, b))
            out["varies"] = True
    vp = []
    cl = sorted(c for c in cfgs if env.get((c, "om1")))
    for i, a in enumerate(cl):
        for b in cl[i + 1:]:
            ga, gb = _parse(a), _parse(b)
            diff = [n for n, x, y in zip(("RATE", "RTT", "Q"), ga, gb) if x != y]
            if len(diff) == 1:
                vp.append((a, env[(a, "om1")], b, env[(b, "om1")], diff[0]))
    for tag, a, b in varies([(a, ia, b, ib, f) for a, ia, b, ib, f in vp]):
        L.append("%s %s vs %s (omega is not a constant; no constant is picked)" % (tag, a, b))
        out["varies"] = True
    qt = [c for c in cfgs if c.endswith("Qt")]
    qt_ok = [c for c in qt if env.get((c, "om1"))]
    out["omega"] = {}
    if len(qt_ok) < 3:
        L.append("UNSCOREABLE fewer than 3 of the 4 Q_t configurations scored (%d)" % len(qt_ok))
        out["measurement"] = "UNSCOREABLE"
    else:
        o1 = envelope([env[(c, "om1")] for c in qt_ok])
        oi = envelope([env[(c, "ominf")] for c in qt_ok if env.get((c, "ominf"))])
        out["omega"] = {"om1": o1, "ominf": oi}
        zl = classify(*o1)
        L.append("OMEGA_1 [%.4g, %.4g] -> %s" % (o1[0], o1[1], zl))
        if oi:
            L.append("OMEGA_INF [%.4g, %.4g]" % oi)
        dec = decide_9_2(zl)
        for pre in PRESETS:
            covered = any(out["rent"].get("Qt/%s/%s" % (pre, s), "").startswith("RENT-COVERED") for s in ("within", "beyond"))
            tok, dc = dec[pre]
            if not covered:
                L.append("9.2 %s: NOT-READ (no RENT-COVERED line at Q_t)" % pre)
                continue
            L.append("9.2 %s: %s%s" % (pre, tok, " [default change]" if dc else ""))
            st = steep(pre, o1, oi, d_ms, sig, skew)
            if st:
                L.append("%s (%s)" % st)
            L.append("N=2 skew %s: %s" % (pre, skew_verdict(pre, oi, d_ms, skew)))
        if zl == "NEEDS-MORE-POWER":
            L.append("NEEDS-MORE-POWER omega_1's interval contains more than one boundary")
    # 9.4 outage check
    oc = {a: [r for r in live if r["stage"] == "OUT" and r["arm"] == a] for a in ("CLEAN", "OUT-REL", "OUT-HOLD")}
    if all(oc.values()):
        L.append(outage(oc, nboot, rng))
    # 9.5 stage T against the Q_t core
    tl = stage_t(cells, pairs)
    L += tl
    if "measurement" not in out:
        has91 = all(any(k.endswith("/" + p + "/" + s) for k in out["rent"] for s in ("within", "beyond")) for p in PRESETS)
        out["measurement"] = "VOID" if voidstage else ("DELIVERED" if has91 and out["omega"] else "UNSCOREABLE")
    L.append("MEASUREMENT %s" % out["measurement"])
    out["exit"] = 0 if out["measurement"] in ("DELIVERED",) else 2
    # unscoreable cells
    out["unscoreable"] = {"%s/%s" % k: p["why"] for k, p in pairs.items() if p["why"]}
    for k, w in sorted(out["unscoreable"].items()):
        L.append("UNSCOREABLE %s: %s" % (k, "; ".join(w)))
    out["pairs"] = {"%s/%s" % k: {"dt_ms": p["dt_ms"], **{n: p["full"].get(n) for n in ("cR1", "cRinf", "cH", "om1", "ominf", "guard")},
                                  "ci_cH": cis(p, "cH"), "ci_om1": cis(p, "om1"), "ci_ominf": cis(p, "ominf"), "aa_floor": p["floor"]} for k, p in pairs.items()}
    return out


def _parse(cfg):
    a, b, c = cfg.split("-")
    return a, b, c


def bulk_arm(p, rv, side, rng, nboot):
    """Bulk's count rule for one arm (9.1): x = non-free share (HOLD - CLEAN) with a Newcombe interval over clusters, c_nf = mean D_e of a non-free HOLD event."""
    H, C = p["H"], p["C"]
    if not H or not C:
        return "UNSCOREABLE"
    xh = sum(r["nf"] for r in H)
    nh = sum(r["nv"] for r in H)
    xc = sum(r["nf"] for r in C)
    nc = sum(r["nv"] for r in C)
    deh = design_effect([(r["nf"], r["nv"]) for r in H])
    dec = design_effect([(r["nf"], r["nv"]) for r in C])
    nw = newcombe(xh / deh, nh / deh, xc / dec, nc / dec)
    if nw is None:
        return "UNSCOREABLE"
    _, x_lo, x_hi = nw
    cs = []
    use = [r for r in H if r["nnf"]]
    if sum(r["nnf"] for r in use) == 0:
        # no non-free hold in the arm: c_nf is unobserved. Its upper bound is the largest D_e seen in the arm, its lower bound 0,
        # so the arm can be COVERED (x * max D_e <= rent) but never UNDERPRICED.
        return bulk_line(side, max(x_hi, 0.0), max(x_lo, 0.0), p["maxD"] * K, 0.0, rv)
    for _ in range(nboot):
        ix = rng.choices(range(len(use)), k=len(use))
        v = ratio(use, "snf", "nnf", ix)
        if v is not None:
            cs.append(v)
    cs.sort()
    c = ci(cs)
    return bulk_line(side, max(x_hi, 0.0), max(x_lo, 0.0), c[1] * K, c[0] * K, rv)


def outage(oc, nboot, rng):
    """9.4: OUT-REL against OUT-HOLD (cost over CLEAN), CIs disjoint."""
    def reps(rs):
        return [{"s": sum(e["D"] for e in r["events"]), "n": len(r["events"])} for r in rs]
    R, H, C = reps(oc["OUT-REL"]), reps(oc["OUT-HOLD"]), reps(oc["CLEAN"])
    sr, sh = [], []
    for _ in range(nboot):
        c = ratio(C, "s", "n", rng.choices(range(len(C)), k=len(C)))
        r = ratio(R, "s", "n", rng.choices(range(len(R)), k=len(R)))
        h = ratio(H, "s", "n", rng.choices(range(len(H)), k=len(H)))
        if None not in (c, r, h):
            sr.append(r - c)
            sh.append(h - c)
    sr.sort()
    sh.sort()
    a, b = ci(sr), ci(sh)
    if not a or not b:
        return "UNSCOREABLE outage check"
    if a[0] > b[1]:
        return "OUT-RELEASE-WORSE (back to the owner: the inner TCP contradicts the 2026-11-21 acceptance)"
    if a[1] < b[0]:
        return "OUT-RELEASE-BETTER"
    return "OUT-NULL"


def stage_t(cells, pairs):
    """9.5: T-CONFIRMS iff, per arm, the excess spurious retransmissions and the excess bottleneck drops agree with the core (D, 10000, RTT 10, Q_t) within +-0.5 and the idle excess agrees in sign."""
    tcells = {a: rs for (st, c, a), rs in cells.items() if st == "T"}
    if not tcells.get("CLEAN"):
        return []
    core = "g10000-rtt10-Qt"
    out = []

    def mean_ev(rs, field, key="events", lab=None):
        v = [e[field] for r in rs for e in (r[key].get(lab, []) if key == "pseudo" else r[key]) if e.get("valid")]
        return sum(v) / len(v) if v else None

    for kind in ("REL", "HOLD"):
        for t in (1, 4):
            arm, lab = "%s@t=%d" % (kind, t), "t=%d" % t
            ta, tc = tcells.get(arm), tcells["CLEAN"]
            ca, cc = cells.get(("D", core, arm)), cells.get(("D", core, "CLEAN"))
            if not (ta and ca and cc) or not all(lab in r["pseudo"] for r in tc):
                out.append("UNSCOREABLE T-%s t=%d (missing stage T or core cells)" % (kind, t))
                continue
            res = []
            for field in ("spurious", "drops", "idle"):
                a1, a0 = mean_ev(ta, field), mean_ev(tc, field, "pseudo", lab)
                b1, b0 = mean_ev(ca, field), mean_ev(cc, field, "pseudo", lab) if all(lab in r["pseudo"] for r in cc) else None
                if None in (a1, a0, b1, b0):
                    res = None
                    break
                res.append((a1 - a0, b1 - b0))
            if res is None:
                out.append("UNSCOREABLE T-%s t=%d (a field is absent)" % (kind, t))
                continue
            ok = abs(res[0][0] - res[0][1]) <= 0.5 and abs(res[1][0] - res[1][1]) <= 0.5 and (res[2][0] > 0) == (res[2][1] > 0)
            out.append("%s %s" % ("T-CONFIRMS" if ok else "T-DIVERGES", arm))
    return out


def calibrate(path):
    """Session R: per configuration, s2_rep of REL and HOLD (k_f = 8), the design effect of the non-free indicator, and the n formula (7.2)."""
    _, rows, _ = load(path)
    live, _ = classify_rows([r for r in rows if r.get("stage") == "calib"])
    res = {}
    for cfg in sorted({r["cfg"] for r in live}):
        def per_rep(arm, fn):
            vals = []
            for r in live:
                if r["cfg"] == cfg and r["arm"] == arm:
                    d = fn(r)
                    if d:
                        vals.append(d)
            return vals

        def mean_d(evs):
            v = [e["D"] for e in evs if e["valid"]]
            return sum(v) / len(v) if v else None

        vr = [x for x in per_rep("REL@k_f=8", lambda r: mean_d([e for e in r["events"] if e["rank"] == 1])) if x is not None]
        vh = [x for x in per_rep("HOLD@k_f=8", lambda r: mean_d(r["events"])) if x is not None]
        if len(vr) < 2 or len(vh) < 2:
            res[cfg] = "UNSCOREABLE (< 2 reps with valid events)"
            continue
        var = lambda v: sum((x - sum(v) / len(v)) ** 2 for x in v) / (len(v) - 1)
        n, raw = n_formula(var(vr) * K * K, var(vh) * K * K, min(len(vr), len(vh)) - 1)
        de = design_effect([(sum(1 for e in r["events"] if e["valid"] and not (e["idle"] == 0 and e["drops"] == 0)), sum(1 for e in r["events"] if e["valid"])) for r in live if r["cfg"] == cfg and r["arm"] == "HOLD@k_f=8"])
        res[cfg] = {"s2_R": var(vr) * K * K, "s2_H": var(vh) * K * K, "DE": de, "n": n, "n_unclamped": raw, "POWER-LIMITED": raw > 24}
    return res


def render(out):
    return "\n".join(out["lines"] + ["NOTE " + n for n in out.get("notes", [])])


def main(argv):
    if len(argv) >= 3 and argv[1] == "calibrate":
        print(json.dumps(calibrate(argv[2]), indent=1, sort_keys=True))
        return 0
    if len(argv) < 2 or argv[1].startswith("-"):
        print(__doc__)
        return 3
    nboot, seed, jpath = 10000, 1, None
    a = argv[2:]
    while a:
        if a[0] == "--boot" and len(a) > 1:
            nboot = int(a[1])
            a = a[2:]
        elif a[0] == "--seed" and len(a) > 1:
            seed = int(a[1])
            a = a[2:]
        elif a[0] == "--json" and len(a) > 1:
            jpath = a[1]
            a = a[2:]
        else:
            print(__doc__)
            return 3
    out = score(argv[1], nboot, seed)
    print(render(out))
    if jpath and out["exit"] == 0:
        with open(jpath, "w") as f:
            json.dump(out, f, indent=1, default=str)
    return out["exit"]


if __name__ == "__main__":
    sys.exit(main(sys.argv))
