"""Stage-3e runner (three-knobs-stage3e-build.md 8; registration 5, 9 and 13).  One file per run, keyed by a fingerprint of the configuration
(tmp + rename, existing files skipped, so a killed run resumes):

    python batch3e.py --part AA|AA2|DECIDE|CROSS|MAIN|AMEND|ROBUST|SMOOTH|DOUBLE:<PART> [--seeds 12] [--seed0 0] [--only PREFIX]
                      [--cells A,B] [--workers N] [--list]

Store: out3e/runs/<fingerprint>-<seed>.json; a part's job list is out3e/<PART>.json (job name -> fingerprint).  A fingerprint is requested once
across all parts (the first part in ORDER that asks for it runs it); --list prints per part the configurations and the runs after that dedupe.
Markers (out3e/PASS-*, out3e/DONE-<PART>) gate the order of registration 9 and 13.6; out3e/STOP stops everything.  PASS-rttbug and PASS-simread
are written by the coordinator.  After every run the gates of registration 9 and 13.6 are checked and any failure writes STOP and exits non-zero.
"""
import argparse
import dataclasses
import hashlib
import json
import math
import multiprocessing as mp
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import toy2  # noqa: E402
import batch3  # noqa: E402
import batch3b  # noqa: E402
import batch3c  # noqa: E402
import batch3d  # noqa: E402
import test_toy2  # noqa: E402

OUT = os.path.join(HERE, "out3e")
INF = math.inf
SEEDS = 12
SEEDS_ID = 3
LINK_KW = batch3c.LINK_KW


# ---- cells (build 8.1) --------------------------------------------------------------------------------------------------------
def _copy(src, name):
    lc, kw = src[name]
    return lc, dict(kw)                                  # copies: the batch modules' own dicts stay untouched


def _with(cell, **kw):
    lc, d = cell
    d = dict(d)
    d.update(kw)
    return lc, d


def _cells():
    D, K = batch3d.CELLS, batch3.CELLS
    C = {}
    C["CL"] = _copy(D, "CL@b")
    C["CL-long"] = _copy(D, "CL-long@b")
    C["OV"] = _copy(D, "OV@b")
    C["OV-long"] = _copy(D, "OV-long@b")
    C["CLw-long"] = _with(C["CL-long"], rate=8000.0, n_total=160000, dead=(0, 2.0, 3.0))
    C["CLc"] = _with(C["CL"], dead=None)
    C["CLs-long"] = _with(C["CL-long"], rate=1000.0, n_total=20000, link=dict(C=1250.0, Q=125.0), pause_B=100)
    C["RQ"] = _with(C["CL-long"], rev_q=0.1)
    C["RT"] = _with(_copy(K, "K-L10"), dead=None)
    C["K-I2"] = _copy(K, "K-I2")
    C["K-L10"] = _copy(K, "K-L10")
    C["K-OV"] = _with(_copy(K, "K-L10"), dead=None, link=dict(C=40.0, Q=40.0), t_limit=120.0, **dict(LINK_KW, pause_B=8, sweep_form="blocker", rx_refresh=True))
    C["E-I2"] = _copy(K, "E-I2")
    C["M-I2"] = _copy(K, "M-I2")
    C["Ec"] = _with(_copy(K, "E-I2"), dead=None)
    for lc, kw in C.values():
        if kw.get("dead") is not None or kw.get("link") is not None:
            kw["windows"] = True
    return C


CELLS = _cells()
VIDEO = {"CLc-v": "CLc", "CL-long-v": "CL-long", "OV-long-v": "OV-long"}       # the video point: delta 0.005, tau 2 s
FILE_CELLS = ("Ec", "E-I2", "M-I2")
RE_CELLS = ("CL-long", "OV", "CLw-long", "RT")
LONG = ("CL-long", "OV-long", "CLw-long", "CLs-long", "RQ", "CL-long-v", "OV-long-v")
T_SET = [0.4, 0.6, 0.8, 1.0] + [round(1.2 + 0.05 * i, 2) for i in range(9)] + [2.0, INF]       # 15 points (proof stage 1, plan 1.4)
T_CALL = [0.08, 0.10, 0.12, 0.14, 0.16, 0.20, 0.40, INF]
SWEEP = {                                                                    # cell -> (T sweep at H = 0, T points at H = inf)
    "RT": (T_CALL, [0.12, 0.4, INF]), "K-I2": (T_CALL, [0.12, 0.4, INF]), "K-L10": (T_CALL, [0.12, 0.4, INF]),
    "K-OV": ([0.2, 0.3, 0.4, 0.6, 1.0, INF], [0.4, 1.0, INF]),
    "CLc": ([0.4, 1.0, INF], [0.4, 1.0, INF]),
    "CL": (T_SET, [0.4, 1.4, INF]), "CL-long": (T_SET, [0.4, 1.4, INF]), "CLw-long": (T_SET, [0.4, 1.4, INF]),
    "OV": ([0.4, 0.6, 1.0, 2.0, INF], [0.4, 2.0, INF]), "OV-long": ([0.4, 0.6, 1.0, 2.0, INF], [0.4, 2.0, INF]),
    "CLc-v": ([1.0, 1.5, 2.0, 3.0, INF], [1.0, 2.0, INF]), "CL-long-v": ([1.0, 1.5, 2.0, 3.0, INF], [1.0, 2.0, INF]),
    "OV-long-v": ([1.0, 1.5, 2.0, 3.0, INF], [1.0, 2.0, INF]),
    "Ec": ([INF], [INF]), "E-I2": ([INF], [INF]), "M-I2": ([INF], [INF]),
}
RHO_SET = [0.95, 0.98, 0.99, 0.995, 1.0]
DELTA_SET = [0.5, 5.0, 50.0]
T_MAIN = {"RT": 0.12}                                                        # the primary lifetime of a cell (0.4 s elsewhere)
TAG = batch3.tag


def base_of(cell):
    return VIDEO.get(cell, cell)


def delta_of(cell):
    return 0.005 if cell in VIDEO or cell in FILE_CELLS else 50.0


def tau_of(cell):
    return 2.0 if cell in VIDEO else 0.4


# ---- arm builders (build 8.2) ----------------------------------------------------------------------------------------------------
S5F = dict(evidence_verdict=False, verdict_evidence="none", floor_law="protect", floor_carrier="header", deq_expiry=True,
           o_excl_verdict=False, rtt_est="rfc", tail_elig="P", trigger_law="trigger", owd_est="fwd", recheck_floor=True)


def _fields(cell, arm, kw):
    """Cell keywords, then the arm's own fields (an arm decides its sender and receiver), then explicit overrides."""
    lc, d = CELLS[cell]
    f = dict(d)
    if cell == "RT":
        f["tau_extra"] = (0.12,)                                             # RT reads the 0.12 s on-time share beside the 0.4 s one
    f.update(arm)
    f.update(kw)
    return toy2.Cfg(cell=lc, **f)


def with_edge(repair, edge):
    return None if repair is None else dict(repair, edge=edge)


def new_cfg(cell, seed, T, H, delta, repair, variant=0, **kw):
    """The new model: S5 (the field set of batch3d.s5_cfg) with the lifetime T and the hold H; the edge is in `repair`."""
    arm = dict(S5F, sender="S5", recv="RH", seed=seed, delta=delta, rho=1.0, check_p1=False, T=T, Hk=H, variant=variant, repair=repair)
    return _fields(cell, arm, kw)


def adr_cfg(cell, seed, rho, delta, repair, **kw):
    """ADR-0084's sender at rho with today's receiver (C1): edge none."""
    arm = dict(sender="S1c", recv="R0e", seed=seed, delta=delta, rho=rho, rtt_est="rfc", repair=with_edge(repair, "none"))
    return _fields(cell, arm, kw)


def rt_cfg(cell, seed, delta, repair, **kw):
    """Today's Realtime (proof_s1.rt_cfg's fields): edge evict."""
    arm = dict(sender="S1", recv="R0e", seed=seed, delta=delta, rho=1.0, check_p1=False, d_arm="EWMA", budget="resid", floor_send=False,
               evict_window=200, r0e_max=500, pause_B=10 ** 9, rtt_est="rfc", repair=with_edge(repair, "evict"))
    return _fields(cell, arm, kw)


def bulk_cfg(cell, seed, delta, repair, **kw):
    """Today's Bulk (C3): never gives up, strict order; edge none."""
    arm = dict(sender="S1c", recv="R1", seed=seed, delta=delta, rho=1.0, rtt_est="rfc", repair=with_edge(repair, "none"))
    return _fields(cell, arm, kw)


def rp(mode, edge="worth"):
    """A repair mode: "OFF", "RL" (live law, true loss), "RE" (live law, estimated loss) or a fixed r."""
    if mode == "OFF":
        return None
    return dict(r={"RL": "law", "RE": "est"}.get(mode, mode), edge=edge)


# ---- names (F6: every row says what its sender and receiver do; the code name stands in parentheses) -----------------------------------
def n_lt(T, H):
    return "lifetime T=%s, hold %s (S5)" % (TAG(T), "inf" if H == INF else TAG(H))


def n_adr(rho):
    return "ADR-0084 sender at rho=%s, today's receiver (C1)" % TAG(rho)


def n_rt(d):
    return "today's Realtime, delta=%s (E-rt)" % TAG(d)


N_BULK = "never gives up, strict order = today's Bulk (C3)"


def n_mode(mode):
    return {"OFF": "repair off", "RL": "repair RL (live law, true loss)", "RE": "repair RE (live law, estimated loss)"}.get(mode, "repair fixed r=%g" % (mode,) if not isinstance(mode, str) else mode)


class Job:
    __slots__ = ("name", "part", "cell", "key", "mode", "arm", "builder", "nseeds", "trace", "edge", "partners", "fp")

    def __init__(self, name, part, cell, key, mode, builder, nseeds, arm, trace, partners):
        self.name, self.part, self.cell, self.key, self.mode, self.arm = name, part, cell, key, mode, arm
        self.builder, self.nseeds, self.trace, self.partners = builder, nseeds, trace, partners
        cfg = builder(0)
        self.fp = fingerprint(cfg)
        self.edge = None if cfg.repair is None else cfg.repair["edge"]


def row_builder(cell, key, mode, **kw):
    """The builder of one design row of a cell in a repair mode (the new model's edge is worth, the old designs' their own)."""
    base, d = base_of(cell), delta_of(cell)
    if key[0] == "lt":
        return lambda s: new_cfg(base, s, key[1], key[2], d, rp(mode, "worth"), **kw)
    if key[0] == "adr":
        return lambda s: adr_cfg(base, s, key[1], d, rp(mode, "none"), **kw)
    if key[0] == "rt":
        return lambda s: rt_cfg(base, s, key[1], rp(mode, "evict"), **kw)
    return lambda s: bulk_cfg(base, s, d, rp(mode, "none"), **kw)


def rows_of(cell):
    """Every design row of a cell: the new model's sweeps, ADR-0084 at its rho, today's Realtime at its delta, today's Bulk."""
    T, Hs = SWEEP[cell]
    keys = [("lt", t, 0.0) for t in T] + [("lt", t, INF) for t in Hs]
    if cell in FILE_CELLS:
        return [("lt", INF, 0.0), ("lt", INF, INF), ("bulk",)]
    keys += [("adr", r) for r in RHO_SET] + [("rt", x) for x in DELTA_SET + ([0.005] if cell in VIDEO else [])] + [("bulk",)]
    return keys


def row_name(key):
    return {"lt": lambda: n_lt(key[1], key[2]), "adr": lambda: n_adr(key[1]), "rt": lambda: n_rt(key[1]), "bulk": lambda: N_BULK}[key[0]]()


def row_delta(cell, key):
    """The delta a row runs at: its own for today's Realtime, the cell's otherwise."""
    return key[1] if key[0] == "rt" else delta_of(cell)


PARTS = {}
ORDER = ["AA", "AA2", "DECIDE", "CROSS", "MAIN", "AMEND", "ROBUST", "SMOOTH"]
FPS = {}


def fingerprint(cfg):
    """sha256 of the configuration without its seed (and without the trace switch, which changes no output): one run per fingerprint."""
    d = dataclasses.asdict(cfg)
    d.pop("seed")
    d.pop("want_trace")
    return hashlib.sha256(json.dumps(d, sort_keys=True, default=repr).encode()).hexdigest()[:20]


def add(part, name, cell, key, mode, builder, nseeds=SEEDS, arm=None, trace=False, partners=()):
    PARTS[part].append(Job(name, part, cell, key, mode, builder, nseeds, arm, trace, list(partners)))


def partners_of(cell, key, mode):
    """Identity partners (registration 13.2, 13.6): the T = inf, H = inf arm and today's Bulk of the same mode; a delta = 0.005 row and its OFF row."""
    out = []
    if key == ("lt", INF, INF):
        out.append(row_builder(cell, ("bulk",), mode))
    if mode != "OFF" and row_delta(cell, key) == 0.005:
        out.append(row_builder(cell, key, "OFF"))
    return out


def job_name(cell, design, mode):
    return "%s | %s | %s" % (cell, design, n_mode(mode))


def add_row(part, cell, key, mode, nseeds=SEEDS, arm=None):
    """One design row in one mode.  Identity partners run with a trace digest (registration 13.6 stop rules)."""
    pb = partners_of(cell, key, mode)
    add(part, job_name(cell, row_name(key), mode), cell, key, mode, row_builder(cell, key, mode), nseeds, arm, trace=bool(pb), partners=pb)


# ---- the parts (registration 5, 13; build 8.3) --------------------------------------------------------------------------------------
def primary_T(cell):
    """The cell's primary lifetime: 0.4 s, RT 0.12 s, the video cells 2 s, the file cells never give up."""
    return INF if cell in FILE_CELLS else T_MAIN.get(cell, 2.0 if cell in VIDEO else 0.4)


def aa_jobs(part, cells):
    """The A/A pairs at both registered readings (they conflict, see the results file): "tinf" (build 8.5: proof_s1.freeze's rule, S5 at T = inf, hold 0
    against its phase variant 1) and "primary" (registration 5: S5 at the cell's primary lifetime, hold 0, against its phase variant 1).  The
    reader's freeze takes either; the coordinator picks one before AA runs and the other pair's rows just stay unused (the A rows are rows of
    the sweeps and of MAIN anyway)."""
    for cell in cells:
        base, d = base_of(cell), delta_of(cell)
        for at, T in (("tinf", INF), ("primary", primary_T(cell))):
            if at == "primary" and T == INF:
                continue                                                         # the file cells' primary lifetime is T = inf: the same pair
            for var in (0, 1):
                add(part, "%s | A/A at %s: lifetime T=%s, hold 0%s (S5) | repair off" % (cell, at, TAG(T), ", phase variant 1" if var else ""), cell, ("aa", at, var), "OFF",
                    lambda s, base=base, d=d, var=var, T=T: new_cfg(base, s, T, 0.0, d, None, variant=var), arm=("AA" if var else "AA-ref") + ("" if at == "tinf" else "-primary"))


MAIN_CELLS = ("CL", "CL-long", "OV", "CLw-long", "RT", "CLs-long")
AA2_CELLS = ("K-I2", "K-L10", "K-OV", "CLc", "OV-long", "CLc-v", "CL-long-v", "OV-long-v", "Ec", "E-I2", "M-I2")
RS_R = {"CL": 0.107, "CL-long": 0.107, "OV": 0.0998, "CLs-long": 0.2295}        # the cell's unqueued law value (registration 5)
RS_INF_R = {"CL-long": 0.107, "CLs-long": 0.2295, "CLw-long": 0.116}
MAIN_ARMS = {
    "CL": ("S5", "R0", "RL", "RS", "RF3"),
    "CL-long": ("S5", "R0", "RL", "RE", "RS", "RF3", "RL-floor", "RL-Hinf", "S5-inf", "RL-inf", "RS-inf"),
    "OV": ("S5", "R0", "RL", "RE", "RS", "RF3", "RL-age", "RL-floor", "RL-Hinf"),
    "CLw-long": ("S5", "R0", "RL", "RE", "RF3", "S5-inf", "RL-inf", "RS-inf", "RF3-inf", "RE-inf"),
    "RT": ("S5", "R0", "RL", "RE", "RF1", "RF3", "RL-floor", "S5-T0p4", "RL-T0p4"),
    "CLs-long": ("S5", "R0", "RL", "RS", "RF1", "RF3", "S5-inf", "RL-inf", "RS-inf"),
}


def main_arm(cell, arm):
    """(T, H, mode, edge) of a MAIN arm in a cell (registration 5)."""
    T0 = T_MAIN.get(cell, 0.4)
    if arm == "S5":
        return T0, 0.0, "OFF", "worth"
    if arm == "R0":
        return T0, 0.0, 0.0, "worth"
    if arm in ("RL", "RE"):
        return T0, 0.0, arm, "worth"
    if arm == "RS":
        return T0, 0.0, RS_R[cell], "worth"
    if arm == "RF3":
        return T0, 0.0, 0.36, "worth"
    if arm == "RF1":
        return T0, 0.0, 0.112, "worth"
    if arm == "RL-age":
        return T0, 0.0, "RL", "age"
    if arm == "RL-floor":
        return T0, 0.0, "RL", "floor"
    if arm == "RL-Hinf":
        return T0, INF, "RL", "worth"
    if arm == "S5-inf":
        return INF, 0.0, "OFF", "worth"
    if arm == "RL-inf":
        return INF, 0.0, "RL", "worth"
    if arm == "RE-inf":
        return INF, 0.0, "RE", "worth"
    if arm == "RS-inf":
        return INF, 0.0, RS_INF_R[cell], "worth"
    if arm == "RF3-inf":
        return INF, 0.0, 0.36, "worth"
    if arm == "S5-T0p4":
        return 0.4, 0.0, "OFF", "worth"
    if arm == "RL-T0p4":
        return 0.4, 0.0, "RL", "worth"
    raise KeyError(arm)


def main_jobs():
    for cell in MAIN_CELLS:
        for arm in MAIN_ARMS[cell]:
            T, H, mode, edge = main_arm(cell, arm)
            b = (lambda s, cell=cell, T=T, H=H, mode=mode, edge=edge: new_cfg(base_of(cell), s, T, H, delta_of(cell), rp(mode, edge)))
            pb, tr = [], False
            if arm == "R0":                                                  # R0 equals S5 on every key and trace (registration Q3e-1)
                pb, tr = [lambda s, cell=cell, T=T: new_cfg(base_of(cell), s, T, 0.0, delta_of(cell), None)], True
            elif arm == "S5":
                tr = True
            add("MAIN", "%s | arm %s: %s | %s" % (cell, arm, n_lt(T, H) + (", edge %s" % edge if edge != "worth" else ""), n_mode(mode)),
                cell, ("arm", arm), mode, b, SEEDS, arm, trace=tr, partners=pb)
    for cell in MAIN_CELLS:                                                  # registration 5: the A/A arm is S5 with phase variant 1, at the arm's own lifetime
        base, d, T = base_of(cell), delta_of(cell), primary_T(cell)
        add("MAIN", "%s | arm AA: A/A, lifetime T=%s, hold 0, phase variant 1 (S5) | repair off" % (cell, TAG(T)), cell, ("aa", "primary", 1), "OFF",
            lambda s, base=base, d=d, T=T: new_cfg(base, s, T, 0.0, d, None, variant=1), SEEDS, "AA")


def decide_jobs():
    cell = "CL-long"
    for mode in ("OFF", "RL"):
        for key in rows_of(cell):
            add_row("DECIDE", cell, key, mode)


def cross_jobs():
    add("CROSS", "CL-long | ADR-0084 sender at rho=0.99, any-order receiver (cross) | repair off", "CL-long", ("cross", "any"), "OFF",
        lambda s: adr_cfg("CL-long", s, 0.99, 50.0, None, recv="RH", Hk=0.0))
    add("CROSS", "CL-long | ADR-0084 sender at rho=0.99 without its copy, today's receiver (cross, S1) | repair off", "CL-long", ("cross", "nocopy"), "OFF",
        lambda s: adr_cfg("CL-long", s, 0.99, 50.0, None, sender="S1"))


def amend_jobs():
    exclude = {}                                                             # per cell: the rows MAIN and DECIDE already hold (registration 13.5 counts them once)
    for p in ("MAIN", "DECIDE"):
        for j in PARTS[p]:
            exclude.setdefault(j.cell, set()).add(j.fp)
    for cell in ("K-I2", "K-L10", "K-OV", "RT", "CLc", "CL", "CLw-long", "CL-long", "OV", "Ec", "E-I2", "M-I2", "CLc-v", "CL-long-v", "OV-long", "OV-long-v"):
        modes = ["OFF", "RL"] + (["RE"] if cell in RE_CELLS else [])
        for mode in modes:
            for key in rows_of(cell):
                video_rt = delta_of(cell) == 0.005 and key[0] == "rt" and key[1] != 0.005
                if delta_of(cell) == 0.005 and mode != "OFF" and not video_rt:
                    nseeds = SEEDS_ID                                        # r = 0 exactly at delta 0.005: an identity row against OFF, 3 seeds
                else:
                    nseeds = SEEDS
                if fingerprint(row_builder(cell, key, mode)(0)) in exclude.get(cell, ()):
                    continue
                add_row("AMEND", cell, key, mode, nseeds)


def robust_jobs():
    for cell in ("OV", "CL-long"):
        for x in (-0.020, -0.010, -0.005, 0.005, 0.010, 0.020):
            add("ROBUST", "%s | one-way estimate biased by %+g s: %s | %s" % (cell, x, n_lt(0.4, 0.0), n_mode("RL")), cell, ("o_bias", x), "RL",
                lambda s, cell=cell, x=x: new_cfg(cell, s, 0.4, 0.0, 50.0, rp("RL"), o_bias=x))
    for x in (-0.020, -0.010, -0.005, 0.005, 0.010, 0.020):
        add("ROBUST", "OV | dequeue margin biased by %+g s: %s | %s" % (x, n_lt(0.4, 0.0), n_mode("RL")), "OV", ("R_bias", x), "RL",
            lambda s, x=x: new_cfg("OV", s, 0.4, 0.0, 50.0, rp("RL"), R_bias=x))
    for T in (0.4, INF):
        add_row("ROBUST", "RQ", ("lt", T, 0.0), "RL")
    for rho in RHO_SET:
        add_row("ROBUST", "RQ", ("adr", rho), "RL")
    for d in DELTA_SET:
        add_row("ROBUST", "RQ", ("rt", d), "RL")
    add_row("ROBUST", "RQ", ("bulk",), "RL")
    add("ROBUST", "RQ | A/A: lifetime T=inf, hold 0, phase variant 1 (S5) | %s" % n_mode("RL"), "RQ", ("aa", "tinf", 1), "RL",
        lambda s: new_cfg("RQ", s, INF, 0.0, 50.0, rp("RL"), variant=1))
    for cell in ("RQ", "OV"):
        add("ROBUST", "%s | relative one-way delay: %s | %s" % (cell, n_lt(0.4, 0.0), n_mode("RL")), cell, ("rel",), "RL",
            lambda s, cell=cell: new_cfg(cell, s, 0.4, 0.0, 50.0, rp("RL"), owd_est="rel"))


def smooth_jobs():
    def lt(cell, T, H, delta=50.0, tag=""):
        add("SMOOTH", "%s | %s%s | %s" % (cell, n_lt(T, H), tag, n_mode("RL")), cell, ("sm", T, H, delta), "RL",
            lambda s, cell=cell, T=T, H=H, delta=delta: new_cfg(base_of(cell), s, T, H, delta, rp("RL")))
    for cell in ("OV", "CL-long", "K-L10"):
        for k in range(17):
            lt(cell, round(0.360 + 0.005 * k, 3), 0.0)
        for T in (5.0, 10.0, 20.0, 40.0):
            lt(cell, T, 0.0)
        for H in [round(0.001 * k, 3) for k in range(11)] + [round(0.0196 + 0.0001 * k, 4) for k in range(9)]:
            lt(cell, 0.4, H)
    for k in range(17):
        lt("CL-long-v", round(1.8 + 0.025 * k, 3), 0.0, 0.005)
    for cell in ("RT", "CL-long"):
        for p in (0.005, 0.5, 50.0):
            for k in range(11):
                dl = round(p * (0.90 + 0.02 * k), 6)
                lt(cell, 0.4, 0.0, dl, tag=", delta=%g" % dl)
                add("SMOOTH", "%s | %s | %s" % (cell, n_rt(dl), n_mode("RL")), cell, ("smrt", dl), "RL",
                    lambda s, cell=cell, dl=dl: rt_cfg(base_of(cell), s, dl, rp("RL", "evict")))


def build():
    PARTS.clear()
    FPS.clear()
    for p in ORDER:
        PARTS[p] = []
    aa_jobs("AA", MAIN_CELLS)
    aa_jobs("AA2", AA2_CELLS)
    decide_jobs()
    cross_jobs()
    main_jobs()
    amend_jobs()
    robust_jobs()
    smooth_jobs()
    register()


def register():
    """The registry: one entry per fingerprint, its seeds, its trace switch and its identity partners."""
    FPS.clear()
    for p in ORDER:
        for j in PARTS[p]:
            e = FPS.setdefault(j.fp, dict(builder=j.builder, nseeds=0, trace=False, partners=set(), first=p, jobs=[], edge=j.edge))
            e["nseeds"] = max(e["nseeds"], j.nseeds)
            e["trace"] = e["trace"] or j.trace
            e["jobs"].append(j.name)
            for pb in j.partners:                                            # an identity partner runs with a trace digest at the same seeds
                c2 = pb(0)
                fp2 = fingerprint(c2)
                e["partners"].add(fp2)
                e2 = FPS.setdefault(fp2, dict(builder=pb, nseeds=0, trace=False, partners=set(), first=p, jobs=[], edge=None if c2.repair is None else c2.repair["edge"]))
                e2["partners"].add(j.fp)
                e2["trace"] = True
                e2["nseeds"] = max(e2["nseeds"], j.nseeds)


def attributed(part):
    """The fingerprints a part is the first to ask for (a second request reuses the first's files)."""
    seen = set()
    for p in ORDER:
        if p == part:
            return [fp for fp in dict.fromkeys(j.fp for j in PARTS[p]) if fp not in seen]
        seen.update(j.fp for j in PARTS[p])
    return []


def counts():
    out = {}
    for p in ORDER:
        fps = attributed(p)
        out[p] = dict(configs=len(PARTS[p]), new=len(fps), runs=sum(FPS[fp]["nseeds"] for fp in fps))
    return out


# ---- the store, the markers and the gates -------------------------------------------------------------------------------------------
def fname(fp, seed):
    return os.path.join(OUT, "runs", "%s-%d.json" % (fp, seed))


def store(fp, seed, obj):
    path = fname(fp, seed)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + ".tmp%d" % os.getpid()
    with open(tmp, "w") as f:
        json.dump(obj, f)
    os.replace(tmp, path)


def load(fp, seed):
    path = fname(fp, seed)
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f)


PRE = ["PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread"]
CHAIN = ["AA", "AA2", "DECIDE", "MAIN", "AMEND", "ROBUST", "SMOOTH"]


def _need(part):
    if part == "CROSS":
        earlier = ["AA", "AA2", "DECIDE"]
    else:
        earlier = CHAIN[:CHAIN.index(part)]
    return PRE + ([] if part in ("AA", "AA2") else ["PASS-aa"]) + ["DONE-" + p for p in earlier]


def missing_markers(part):
    need = _need(part.split(":")[1]) + ["DONE-" + part.split(":")[1]] if part.startswith("DOUBLE:") else _need(part)
    return [m for m in need if not os.path.exists(os.path.join(OUT, m))]


def refusal(part):
    if os.path.exists(os.path.join(OUT, "STOP")):
        with open(os.path.join(OUT, "STOP")) as f:
            return "out3e/STOP exists, nothing runs until the coordinator clears it: %s" % f.read().strip()
    miss = missing_markers(part)
    if miss:
        return "markers missing for part %s: %s" % (part, ", ".join(miss))
    return None


def refusal_workers(cells, workers):
    if workers > 2:
        return "at most 2 workers (memory)"
    if workers != 1 and any(c in LONG for c in cells):
        return "the long cells (%s) run with one worker" % ", ".join(sorted({c for c in cells if c in LONG}))
    return None


def gate_failures(r, edge):
    """The per-run gates (registration 9 and 13.6): finished, l1_viol, l1_viol_rep, commit_viol, and worth_viol_rep under the worth or floor edge."""
    g = r["gauges"]
    bad = []
    if r.get("finished") is False:
        bad.append("finished")
    for k in ("l1_viol", "l1_viol_rep", "commit_viol"):
        if g.get(k, 0):
            bad.append(k)
    if edge in ("worth", "floor") and g.get("worth_viol_rep", 0):
        bad.append("worth_viol_rep")
    return bad


def id3e(a, b):
    """test_toy2.id_diff with the stage 3e readouts excluded as a block: every top-level key outside ID_EXCL and rep3e, plus ID_GAUGES."""
    excl = test_toy2.ID_EXCL | {"rep3e", "trace_sha", "trace_n"}          # a stored run also carries its trace digest, compared separately
    bad = [k for k in a if k not in excl and not k.startswith("_") and json.dumps(a[k]) != json.dumps(b[k])]
    bad += [("g", k) for k in test_toy2.ID_GAUGES if a["gauges"][k] != b["gauges"][k]]
    return bad


def pair_failures(fp, seed, r, partners):
    """Identity pairs (the T = inf, H = inf arm against today's Bulk in the same mode; a delta = 0.005 row against OFF; R0 against S5):
    every key id3e compares and the sender trace, at the same seed, once both runs exist."""
    bad = []
    for fp2 in partners:
        r2 = load(fp2, seed)
        if r2 is None:
            continue
        if "trace_sha" in r and "trace_sha" in r2 and r["trace_sha"] != r2["trace_sha"]:
            bad.append("trace differs from %s" % fp2)
        d = id3e(r, r2)
        if d:
            bad.append("id3e differs from %s: %s" % (fp2, d))
    return bad


def write_stop(text):
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "STOP"), "w") as f:
        f.write(text + "\n")


def after_run(name, fp, seed, r, edge, partners):
    fails = gate_failures(r, edge) + pair_failures(fp, seed, r, partners)
    if fails:
        write_stop("job %s | fingerprint %s | seed %d | failing: %s" % (name, fp, seed, "; ".join(fails)))
    return fails


# ---- running --------------------------------------------------------------------------------------------------------------------
def run_one(cfg, trace=False):
    cfg = dataclasses.replace(cfg, want_trace=trace)
    sm = toy2.Sim(cfg)
    r = sm.run()
    extra = {}
    if trace:
        h = hashlib.sha256()
        for e in r["_trace"]:
            h.update(repr(e).encode())
        extra = dict(trace_sha=h.hexdigest(), trace_n=len(r["_trace"]))
    r = batch3b.clean(r)
    r.update(extra)
    return r


def work(task):
    fp, seed = task
    e = FPS[fp]
    t = time.time()
    r = run_one(e["builder"](seed), e["trace"])
    r["wall_s"] = time.time() - t
    store(fp, seed, r)
    return fp, seed, r["wall_s"]


def cell_order(job):
    order = ["K-I2", "K-L10", "K-OV", "RT", "CLc", "CL", "CLw-long", "CL-long", "OV", "Ec", "E-I2", "M-I2", "CLc-v", "CL-long-v", "OV-long", "OV-long-v"]
    return order.index(job.cell) if job.cell in order else -1


def write_joblist(part, jobs):
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "%s.json" % part.replace(":", "-")), "w") as f:
        json.dump({j.name: j.fp for j in jobs}, f, indent=1)


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--part", required=True)
    ap.add_argument("--seeds", type=int, default=SEEDS)
    ap.add_argument("--seed0", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--cells", default="")
    ap.add_argument("--workers", type=int, default=1)
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args(argv)
    build()
    if a.list:
        tot = dict(configs=0, new=0, runs=0)
        for p, c in counts().items():
            print("%-8s %4d configurations, %4d after the dedupe, %5d runs" % (p, c["configs"], c["new"], c["runs"]))
            for k in tot:
                tot[k] += c[k]
        print("%-8s %4d configurations, %4d after the dedupe, %5d runs" % ("total", tot["configs"], tot["new"], tot["runs"]))
        if a.part != "ALL":
            for j in PARTS.get(a.part, []):
                print("%s  %s" % (j.fp, j.name))
        return 0
    double = a.part.startswith("DOUBLE:")
    base = a.part.split(":")[1] if double else a.part
    if base not in PARTS:
        print("unknown part", a.part)
        return 2
    why = refusal(a.part)
    if why:
        print("REFUSED:", why)
        return 2
    cells = [c for c in a.cells.split(",") if c]
    jobs = [j for j in PARTS[base] if j.name.startswith(a.only) and (not cells or j.cell in cells)]
    if double:
        with open(os.path.join(OUT, "double-%s.json" % base)) as f:
            keep = set(json.load(f))
        fps = [fp for fp in dict.fromkeys(j.fp for j in jobs) if fp in keep]
        seeds = range(12, 24)
        limit = 24
    else:
        write_joblist(base, PARTS[base])
        fps = list(dict.fromkeys(j.fp for j in jobs))
        seeds = range(a.seed0, a.seed0 + a.seeds)
        limit = None
    cellname = {j.fp: j for j in jobs}
    why = refusal_workers([cellname[fp].cell for fp in fps], a.workers)
    if why:
        print("REFUSED:", why)
        return 2
    tasks = [(fp, s) for fp in fps for s in seeds if s < (limit or FPS[fp]["nseeds"]) and not os.path.exists(fname(fp, s))]
    tasks.sort(key=lambda t: (cell_order(cellname[t[0]]) if base == "AMEND" else 0, LONG.count(cellname[t[0]].cell), t[1]))
    print("%d runs to do (%d configurations)" % (len(tasks), len(fps)), flush=True)
    t0 = time.time()
    rc = 0
    if a.workers <= 1:
        it = (work(t) for t in tasks)
        pool = None
    else:
        pool = mp.Pool(a.workers, initializer=build)
        it = pool.imap_unordered(work, tasks, chunksize=1)
    for i, (fp, s, w) in enumerate(it, 1):
        r = load(fp, s)
        fails = after_run(cellname[fp].name, fp, s, r, FPS[fp]["edge"], FPS[fp]["partners"])
        print("[%6.0fs] %d/%d %s %d %.1fs%s" % (time.time() - t0, i, len(tasks), cellname[fp].name, s, w, "  STOP: " + "; ".join(fails) if fails else ""), flush=True)
        if fails:
            rc = 1
            break
    if pool is not None:
        pool.terminate()
    if rc == 0:
        want = [(fp, s) for fp in fps for s in seeds if s < (limit or FPS[fp]["nseeds"])]
        if not cells and not a.only and all(os.path.exists(fname(fp, s)) for fp, s in want):
            open(os.path.join(OUT, "DONE-%s" % a.part.replace(":", "-")), "w").close()
            print("DONE-%s" % a.part.replace(":", "-"))
    print("[%6.0fs] done" % (time.time() - t0), flush=True)
    return rc


if __name__ == "__main__":
    mp.freeze_support()
    sys.exit(main())
