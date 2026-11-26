"""Stage-3e runner (three-knobs-stage3e-build.md 8; registration 5, 9 and 13).  One file per run, keyed by a fingerprint of the configuration
(tmp + rename, existing files skipped, so a killed run resumes):

    python batch3e.py --part AA|AA2|TIME|DECIDE|CROSS|EARLY|MAIN|REST|ROBUST|SMOOTH|DOUBLE:<PART> [--seeds 12] [--seed0 0] [--only PREFIX]
                      [--cells A,B] [--light-workers N] [--heavy-workers N] [--reuse-dirs 3d=PATH,ps1=PATH] [--list]
    python batch3e.py --smoke-pool

Order of registration 14.6: AA, AA2, TIME (its total gates the run), DECIDE, EARLY (RL rows of Ec, RT, OV and CL-long-v), MAIN, REST (the other AMEND
rows, long overload cells last); ROBUST and SMOOTH only after the reader has written VERDICT-PASSES.  Seeds (14.3, 14.4): 12 on DECIDE and the near-tie
cells, 6 elsewhere, 3 for the delta = 0.005 identity rows, the old senders of OV-long and OV-long-v (dial ends only) and SMOOTH; A/A 12 everywhere.
Two pools (14.7): light (up to 8 workers) and heavy (old senders in the long cells, up to 2), each run waiting for free memory.
Reuse (14.2): a repair-off fingerprint stored by the re-baseline or proof stage 1 (--reuse-dirs) is used only once check_ident3e.py --reuse wrote PASS-reuse.

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

import re

import psutil

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import toy2  # noqa: E402
import batch3  # noqa: E402
import batch3b  # noqa: E402
import batch3c  # noqa: E402
import batch3d  # noqa: E402
import test_toy2  # noqa: E402

OUT = os.environ.get("BATCH3E_OUT", os.path.join(HERE, "out3e"))        # BATCH3E_OUT and BATCH3E_SMOKE exist for --smoke-pool only
SMOKE = bool(os.environ.get("BATCH3E_SMOKE"))
INF = math.inf
SEEDS = 12
SEEDS_ID = 3
SEEDS_LIGHT = 6
SEEDS12 = ("CL-long", "CL", "CLw-long", "RT", "CLc", "CLc-v", "CL-long-v", "K-I2", "K-L10")      # registration 14.3: the deciding row and the near-tie cells
HEAVY_CELLS = ("CL-long", "CLw-long", "CL-long-v", "OV-long", "OV-long-v", "RQ")                 # the long cells (RQ is CL-long with a reverse queue)
OLD_ENDS = {"OV-long": dict(adr=(0.95, 1.0)), "OV-long-v": dict(adr=(0.95, 1.0))}        # 14.4, narrowed 2026-11-26: ADR-0084 and Bulk at their dial ends only; today's Realtime in full
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


def cell_seeds(cell):
    """12 paired seeds in CL-long and the near-tie cells, 6 elsewhere (registration 14.3)."""
    return SEEDS if cell in SEEDS12 else SEEDS_LIGHT


def row_nseeds(cell, key, mode):
    """The seeds of one design row in one mode: 3 for a delta = 0.005 identity row and for the old senders of the long overload cells (their ends only)."""
    if mode in ("RL", "RE") and row_delta(cell, key) == 0.005:
        return SEEDS_ID
    if cell in OLD_ENDS and key[0] in ("adr", "bulk"):                       # today's Realtime is not cut: under overload it does not read about 0 on time
        return SEEDS_ID
    return cell_seeds(cell)


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
    return dict(r={"RL": "law", "RE": "est", "RB": "rb", "RB025": "rb"}.get(mode, mode), edge=edge)


# ---- names (F6: every row says what its sender and receiver do; the code name stands in parentheses) -----------------------------------
def n_lt(T, H):
    return "lifetime T=%s, hold %s (S5)" % (TAG(T), "inf" if H == INF else TAG(H))


def n_adr(rho):
    return "ADR-0084 sender at rho=%s, today's receiver (C1)" % TAG(rho)


def n_rt(d):
    return "today's Realtime, delta=%s (E-rt)" % TAG(d)


N_BULK = "never gives up, strict order = today's Bulk (C3)"


def n_mode(mode):
    return {"OFF": "repair off", "RL": "repair RL (live law, true loss)", "RE": "repair RE (live law, estimated loss)",
            "RB": "repair RB (r_max through the outage and 3 s after, the law outside)",
            "RB025": "repair RB (r_max through the outage and 0.25 s after, the law outside)"}.get(mode, "repair fixed r=%g" % (mode,) if not isinstance(mode, str) else mode)


class Job:
    __slots__ = ("name", "part", "cell", "key", "mode", "arm", "builder", "nseeds", "trace", "edge", "partners", "fp", "design")

    def __init__(self, name, part, cell, key, mode, builder, nseeds, arm, trace, partners):
        self.name, self.part, self.cell, self.key, self.mode, self.arm = name, part, cell, key, mode, arm
        self.builder, self.nseeds, self.trace, self.partners = builder, nseeds, trace, partners
        cfg = builder(0)
        self.fp = fingerprint(cfg)
        self.edge = None if cfg.repair is None else cfg.repair["edge"]
        self.design = {"lt": "new", "arm": "new", "aa": "new", "sm": "new", "o_bias": "new", "R_bias": "new", "rel": "new", "adr": "adr", "cross": "adr",
                       "rt": "rt", "smrt": "rt", "bulk": "bulk"}[key[0]]


MODE_KW = {"RB025": dict(rb_A=0.25, rb_unit="s")}                           # amendment 3: the bracket's aftermath at the engine's forgetting time


def row_builder(cell, key, mode, **kw):
    """The builder of one design row of a cell in a repair mode (the new model's edge is worth, the old designs' their own)."""
    base, d = base_of(cell), delta_of(cell)
    kw = dict(MODE_KW.get(mode, {}), **kw)
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
    if cell in OLD_ENDS:                                                     # 14.4: the old senders read about 0 on time there; only the ends of their dials
        return keys + [("adr", r) for r in OLD_ENDS[cell]["adr"]] + [("rt", x) for x in DELTA_SET + ([0.005] if cell in VIDEO else [])] + [("bulk",)]
    keys += [("adr", r) for r in RHO_SET] + [("rt", x) for x in DELTA_SET + ([0.005] if cell in VIDEO else [])] + [("bulk",)]
    return keys


def row_name(key):
    return {"lt": lambda: n_lt(key[1], key[2]), "adr": lambda: n_adr(key[1]), "rt": lambda: n_rt(key[1]), "bulk": lambda: N_BULK}[key[0]]()


def row_delta(cell, key):
    """The delta a row runs at: its own for today's Realtime, the cell's otherwise."""
    return key[1] if key[0] == "rt" else delta_of(cell)


PARTS = {}
ORDER = ["AA", "AA2", "DECIDE", "CROSS", "MAIN", "EARLY", "REST", "RB", "ROBUST", "SMOOTH", "EXPL"]       # attribution of a fingerprint; TIME is timing only, AMEND = EARLY + REST
EARLY_CELLS = ("Ec", "RT", "OV", "CL-long-v")                                # one cell per use case (registration 14.6)
FPS = {}


REV4_FIELDS = ("rev4_gauge", "dhat_bias", "clamp", "wait_limit", "eq", "eq_cap", "cwnd_sim", "rate2", "rate2_t")                                    # fields added after the first runs were stored: left out of the hash at their default, so stored fingerprints stay valid


def fingerprint(cfg):
    """sha256 of the configuration without its seed (and without the trace switch, which changes no output): one run per fingerprint."""
    d = dataclasses.asdict(cfg)
    d.pop("seed")
    d.pop("want_trace")
    for f in dataclasses.fields(cfg):
        if f.name in REV4_FIELDS and d[f.name] == f.default:
            d.pop(f.name)
    return hashlib.sha256(json.dumps(d, sort_keys=True, default=repr).encode()).hexdigest()[:20]


def add(part, name, cell, key, mode, builder, nseeds=SEEDS, arm=None, trace=False, partners=()):
    PARTS[part].append(Job(name, part, cell, key, mode, builder, nseeds, arm, trace, list(partners)))


def partners_of(cell, key, mode):
    """Identity partners (registration 13.2, 13.6): the T = inf, H = inf arm and today's Bulk of the same mode; a delta = 0.005 row and its OFF row."""
    out = []
    if key == ("lt", INF, INF):
        out.append(row_builder(cell, ("bulk",), mode))
    if mode in ("RL", "RE") and row_delta(cell, key) == 0.005:                # RB at delta 0.005 differs from OFF by the cap's repairs: no identity
        out.append(row_builder(cell, key, "OFF"))
    return out


def job_name(cell, design, mode):
    return "%s | %s | %s" % (cell, design, n_mode(mode))


def add_row(part, cell, key, mode, nseeds=None, arm=None):
    """One design row in one mode.  Identity partners run with a trace digest (registration 13.6 stop rules)."""
    nseeds = row_nseeds(cell, key, mode) if nseeds is None else nseeds
    pb = partners_of(cell, key, mode)
    add(part, job_name(cell, row_name(key), mode), cell, key, mode, row_builder(cell, key, mode), nseeds, arm, trace=bool(pb), partners=pb)


# ---- the parts (registration 5, 13; build 8.3) --------------------------------------------------------------------------------------
def primary_T(cell):
    """The cell's primary lifetime: 0.4 s, RT 0.12 s, the video cells 2 s, the file cells never give up."""
    return INF if cell in FILE_CELLS else T_MAIN.get(cell, 2.0 if cell in VIDEO else 0.4)


def aa_jobs(part, cells):                                                    # (the A/A rows run at 12 seeds in every cell: registration 14.3)
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
                cell, ("arm", arm), mode, b, cell_seeds(cell) if cell in ("OV", "CLs-long") else SEEDS, arm, trace=tr, partners=pb)
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
                if fingerprint(row_builder(cell, key, mode)(0)) in exclude.get(cell, ()):
                    continue
                add_row("AMEND", cell, key, mode)                            # seeds by row_nseeds: r = 0 exactly at delta 0.005 is a 3-seed identity row
    for j in PARTS["AMEND"]:                                                 # EARLY: the RL rows of the four use-case cells (14.6); REST: everything else
        j.part = "EARLY" if (j.cell in EARLY_CELLS and j.mode == "RL") else "REST"
    PARTS["EARLY"] = [j for j in PARTS["AMEND"] if j.part == "EARLY"]
    for cell in ("RT", "OV"):                                                # MAIN's RL arm is a sweep point AMEND leaves out; the reader needs the primary lifetime in EARLY
        add_row("EARLY", cell, ("lt", primary_T(cell), 0.0), "RL")
    PARTS["REST"] = sorted([j for j in PARTS["AMEND"] if j.part == "REST"], key=cell_order)


BASE_HELD_CELLS = ("K-OV",)      # 2026-11-26: K-OV's baseline rows do not finish within t_limit = 120 s (ADR-0084 at rho 0.95 sends 2 681 of 3 000 units, paused 119.8 s of 120): held for a decision


def expl_jobs():
    """Exploratory, reported only, never under the verdict: sizing a sender wait limit.  OV, repair off on both sides, 6 seeds: the new model at T = 0.15, 0.2 and 0.4 (H = 0)
    and today's Realtime at delta 50."""
    for key in (("lt", 0.15, 0.0), ("lt", 0.2, 0.0), ("lt", 0.4, 0.0), ("rt", 50.0)):
        add("EXPL", "exploratory: sizing a sender wait limit | " + job_name("OV", row_name(key), "OFF"), "OV", key, "OFF", row_builder("OV", key, "OFF"), SEEDS_LIGHT, arm="EXPL")


RB_CELLS = ("CL-long", "CLw-long", "CL-long-v")                              # the long-outage cells (amendment 3)


def rb_jobs():
    """Arm RB, for every design: r at the law's cap from the outage start until rb_A after its end, the law outside (amendment 3)."""
    for cell in RB_CELLS:
        for mode in ("RB025",):                                              # rb_A = 0.25 s (about 4 900 symbols: the engine's forgetting time); the 3 s variant is cut
            for key in rows_of(cell):
                add_row("RB", cell, key, mode, SEEDS)


def robust_jobs():
    for cell in ("OV", "CL-long"):
        for x in (-0.020, -0.010, -0.005, 0.005, 0.010, 0.020):
            add("ROBUST", "%s | one-way estimate biased by %+g s: %s | %s" % (cell, x, n_lt(0.4, 0.0), n_mode("RL")), cell, ("o_bias", x), "RL",
                lambda s, cell=cell, x=x: new_cfg(cell, s, 0.4, 0.0, 50.0, rp("RL"), o_bias=x), SEEDS_LIGHT)
    for x in (-0.020, -0.010, -0.005, 0.005, 0.010, 0.020):
        add("ROBUST", "OV | dequeue margin biased by %+g s: %s | %s" % (x, n_lt(0.4, 0.0), n_mode("RL")), "OV", ("R_bias", x), "RL",
            lambda s, x=x: new_cfg("OV", s, 0.4, 0.0, 50.0, rp("RL"), R_bias=x), SEEDS_LIGHT)
    for T in (0.4, INF):
        add_row("ROBUST", "RQ", ("lt", T, 0.0), "RL", SEEDS_LIGHT)
    for rho in RHO_SET:
        add_row("ROBUST", "RQ", ("adr", rho), "RL", SEEDS_LIGHT)
    for d in DELTA_SET:
        add_row("ROBUST", "RQ", ("rt", d), "RL", SEEDS_LIGHT)
    add_row("ROBUST", "RQ", ("bulk",), "RL", SEEDS_LIGHT)
    add("ROBUST", "RQ | A/A: lifetime T=inf, hold 0, phase variant 1 (S5) | %s" % n_mode("RL"), "RQ", ("aa", "tinf", 1), "RL",
        lambda s: new_cfg("RQ", s, INF, 0.0, 50.0, rp("RL"), variant=1), SEEDS_LIGHT)
    for cell in ("RQ", "OV"):
        add("ROBUST", "%s | relative one-way delay: %s | %s" % (cell, n_lt(0.4, 0.0), n_mode("RL")), cell, ("rel",), "RL",
            lambda s, cell=cell: new_cfg(cell, s, 0.4, 0.0, 50.0, rp("RL"), owd_est="rel"), SEEDS_LIGHT)


SMOOTH_GROUPS = {}                                                           # fine sweeps read by kp_stats: name -> jobs in the order of their dial


def smooth_jobs():
    def lt(cell, T, H, delta=50.0, tag="", group=None):
        add("SMOOTH", "%s | %s%s | %s" % (cell, n_lt(T, H), tag, n_mode("RL")), cell, ("sm", T, H, delta), "RL",
            lambda s, cell=cell, T=T, H=H, delta=delta: new_cfg(base_of(cell), s, T, H, delta, rp("RL")), SEEDS_ID)       # SMOOTH: 3 paired seeds (14.5)
        if group:
            SMOOTH_GROUPS.setdefault(group, []).append(PARTS["SMOOTH"][-1])
    for cell in ("OV", "CL-long", "K-L10"):
        for k in range(17):
            lt(cell, round(0.360 + 0.005 * k, 3), 0.0, group="T 0.360-0.440 | " + cell)
        for T in (5.0, 10.0, 20.0, 40.0):
            lt(cell, T, 0.0)
        for H in [round(0.001 * k, 3) for k in range(11)]:
            lt(cell, 0.4, H, group="H 0-10 ms | " + cell)
        for H in [round(0.0196 + 0.0001 * k, 4) for k in range(9)]:
            lt(cell, 0.4, H, group="H 19.6-20.4 ms | " + cell)
    for k in range(17):
        lt("CL-long-v", round(1.8 + 0.025 * k, 3), 0.0, 0.005, group="T 1.800-2.200 | CL-long-v")
    for cell in ("RT", "CL-long"):
        for p in (0.005, 0.5, 50.0):
            for k in range(11):
                dl = round(p * (0.90 + 0.02 * k), 6)
                lt(cell, 0.4, 0.0, dl, tag=", delta=%g" % dl, group="delta around %g, new model | %s" % (p, cell))
                add("SMOOTH", "%s | %s | %s" % (cell, n_rt(dl), n_mode("RL")), cell, ("smrt", dl), "RL",
                    lambda s, cell=cell, dl=dl: rt_cfg(base_of(cell), s, dl, rp("RL", "evict")), SEEDS_ID)
                SMOOTH_GROUPS.setdefault("delta around %g, today's Realtime | %s" % (p, cell), []).append(PARTS["SMOOTH"][-1])


def build():
    PARTS.clear()
    FPS.clear()
    SMOOTH_GROUPS.clear()
    for p in ORDER + ["AMEND", "TIME", "BASE"]:
        PARTS[p] = []
    if SMOKE:                                                                # --smoke-pool: two tiny configurations, nothing else
        for nm, mode in (("smoke OFF (S5)", "OFF"), ("smoke RL (S5)", "RL")):
            add("AA", nm, "RT", ("aa", mode), mode, lambda s, mode=mode: new_cfg("RT", s, 0.12, 0.0, 50.0, rp(mode), n_total=300), nseeds=2)
        register()
        return
    aa_jobs("AA", MAIN_CELLS)
    aa_jobs("AA2", AA2_CELLS)
    decide_jobs()
    cross_jobs()
    main_jobs()
    amend_jobs()
    rb_jobs()
    robust_jobs()
    smooth_jobs()
    expl_jobs()
    time_jobs()
    PARTS["BASE"] = [j for p in ("EARLY", "REST") for j in PARTS[p] if j.design in ("adr", "rt", "bulk") and j.cell not in BASE_HELD_CELLS]   # every baseline row: valid whatever Revision 4 does
    register()


def register():
    """The registry: one entry per fingerprint, its seeds, its trace switch and its identity partners."""
    FPS.clear()
    for p in ORDER:
        for j in PARTS[p]:
            e = FPS.setdefault(j.fp, dict(builder=j.builder, nseeds=0, trace=False, partners=set(), first=p, jobs=[], edge=j.edge, job=j))
            e["nseeds"] = max(e["nseeds"], j.nseeds)
            e["trace"] = e["trace"] or j.trace
            e["jobs"].append(j.name)
            for pb in j.partners:                                            # an identity partner runs with a trace digest at the same seeds
                c2 = pb(0)
                fp2 = fingerprint(c2)
                e["partners"].add(fp2)
                e2 = FPS.setdefault(fp2, dict(builder=pb, nseeds=0, trace=False, partners=set(), first=p, jobs=[], edge=None if c2.repair is None else c2.repair["edge"], job=j))
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


REUSE = {"index": {}}                                                        # fingerprint -> (kind, dir, part, job name, seeds stored)


def _safe(name):
    return "".join(ch if ch.isalnum() or ch in "=.-" else "_" for ch in name)


def set_reuse(specs):
    """Index the re-baseline's stage 3d outputs ("3d=PATH", layout PATH/<part>/<name>-<seed>.json) and proof stage 1's ("ps1=PATH") by fingerprint
    (registration 14.2).  Only repair-off configurations are indexed; they are used only once out3e/PASS-reuse exists."""
    REUSE["index"] = {}
    for spec in specs:
        kind, path = spec.split("=", 1)
        if kind == "3d":
            batch3d.build()
            for name, (part, fn, ns) in batch3d.JOBS.items():
                REUSE["index"].setdefault(fingerprint(fn(0)), (kind, path, part, name, ns))
        elif kind == "ps1":
            import proof_s1
            proof_s1.build()
            for name, (part, fn) in proof_s1.JOBS.items():
                REUSE["index"].setdefault(fingerprint(fn(0)), (kind, path, part, _safe(name), 12))
        else:
            raise ValueError(spec)


def reuse_on():
    return os.path.exists(os.path.join(OUT, "PASS-reuse"))


def _reused_path(fp, seed):
    e = REUSE["index"].get(fp)
    if e is None or not reuse_on() or seed >= e[4]:
        return None
    path = os.path.join(e[1], e[2], "%s-%d.json" % (e[3], seed))
    return path if os.path.exists(path) else None


def stored_result(fp, seed):
    """The re-baselined or proof stage 1 result of a fingerprint, whether or not PASS-reuse exists (check_ident3e --reuse compares against it)."""
    e = REUSE["index"].get(fp)
    if e is None or seed >= e[4]:
        return None
    path = os.path.join(e[1], e[2], "%s-%d.json" % (e[3], seed))
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f)


def have_run(fp, seed):
    return os.path.exists(fname(fp, seed)) or _reused_path(fp, seed) is not None


def load(fp, seed):
    path = fname(fp, seed)
    if not os.path.exists(path):
        path = _reused_path(fp, seed)
        if path is None:
            return None
    with open(path) as f:
        return json.load(f)


PRE = ["PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate"]                              # PASS-reuse is optional: without it nothing is reused; PASS-simread gates MAIN only
CHAIN = ["AA", "AA2", "TIME", "DECIDE", "EARLY", "MAIN", "REST", "ROBUST", "SMOOTH", "RB"]    # RB, ROBUST and SMOOTH follow a PASSES verdict       # registration 14.6
DONE_MARK = {"TIME": "PASS-time"}


def _need(part):
    if part in ("CROSS", "EXPL", "BASE"):
        earlier = ["AA", "AA2", "DECIDE"]
    elif part == "RB":
        earlier = CHAIN[:CHAIN.index("REST") + 1]                            # reported only: after the verdict, independent of ROBUST and SMOOTH
    else:
        earlier = CHAIN[:CHAIN.index(part)]
    need = PRE + ([] if part in ("AA", "AA2") else ["PASS-aa"]) + [DONE_MARK.get(p, "DONE-" + p) for p in earlier]
    if part == "MAIN":
        need.insert(len(PRE), "PASS-simread")                                # the simulator read (registration 9 step 6) gates MAIN
    if part in ("ROBUST", "SMOOTH", "RB"):
        need.append("VERDICT-PASSES")                                        # written by the reader when the stage verdict reads PASSES (14.5)
    return need


def missing_markers(part, partial=False):
    """partial: a DOUBLE of one cell of a part still running cell by cell (REST) does not need the part's DONE marker."""
    if SMOKE:
        return []
    need = _need(part.split(":")[1]) + ([] if partial else ["DONE-" + part.split(":")[1]]) if part.startswith("DOUBLE:") else _need(part)
    return [m for m in need if not os.path.exists(os.path.join(OUT, m))]


def refusal(part, partial=False):
    if os.path.exists(os.path.join(OUT, "STOP")):
        with open(os.path.join(OUT, "STOP")) as f:
            return "out3e/STOP exists, nothing runs until the coordinator clears it: %s" % f.read().strip()
    miss = missing_markers(part, partial)
    if miss:
        return "markers missing for part %s: %s" % (part, ", ".join(miss))
    return None


def refusal_workers(n):
    """One pool of at most 9 workers (the owner's decision: the measured peaks are 146 to 431 MB, not 1.5 GB)."""
    if n < 1:
        return "at least one worker"
    if n > 9:
        return "at most 9 workers"
    return None


def is_heavy(cfg):
    """An old sender (ADR-0084 S1c, today's Realtime S1, today's Bulk S1c) in a long cell, recognised by its size (rate, units)."""
    sig = {(CELLS[base_of(c)][1]["rate"], CELLS[base_of(c)][1]["n_total"]) for c in HEAVY_CELLS}
    return cfg.sender in ("S1c", "S1") and (cfg.rate, cfg.n_total) in sig


MEM_MARGIN_GB = 1.0                                                          # free memory kept beyond a run's own peak
DEFAULT_PEAK_MB = 450.0                                                      # an unmeasured class: above the largest measured peak (431 MB)
PEAKS = {}                                                                   # (cell, design) -> peak MB, read from the TIME rows


def need_gb(peak_mb):
    """Free memory a run waits for before it starts: its measured peak plus the margin."""
    return peak_mb / 1024.0 + MEM_MARGIN_GB


def load_peaks():
    path = os.path.join(OUT, "time.json")
    PEAKS.clear()
    if os.path.exists(path):
        with open(path) as f:
            for cell, design, sec, peak in json.load(f)["rows"]:
                PEAKS[(cell, design)] = peak


def need_for_fp(fp):
    j = FPS[fp]["job"]
    return need_gb(PEAKS.get((j.cell, j.design), DEFAULT_PEAK_MB))


def free_gb():
    return psutil.virtual_memory().available / 2.0 ** 30


def wait_for_memory(need, avail=free_gb, sleep=time.sleep, poll=5.0):
    """Block until `avail()` (GB) is at least `need`; returns the number of waits."""
    n = 0
    while avail() < need:
        sleep(poll)
        n += 1
    return n


def gate_failures(r, edge, design=None):
    """The per-run gates (registration 9 and 13.6): finished, l1_viol, l1_viol_rep, commit_viol, and worth_viol_rep under the worth or floor edge.
    Today's Realtime (design "rt") sends no floor, so an object whose tail is shed never finishes: for it `finished` is recorded (`unfinished`,
    `delivered_by_t_limit`), not a stop (amendment of 2026-11-26)."""
    g = r["gauges"]
    bad = []
    if r.get("finished") is False and design != "rt":
        bad.append("finished")
    for k in ("l1_viol", "l1_viol_rep", "commit_viol"):
        if g.get(k, 0):
            bad.append(k)
    if edge in ("worth", "floor") and g.get("worth_viol_rep", 0):
        bad.append("worth_viol_rep")
    return bad


def id3e(a, b):
    """test_toy2.id_diff with the stage 3e readouts excluded as a block: every top-level key outside ID_EXCL and rep3e, plus ID_GAUGES."""
    excl = test_toy2.ID_EXCL | {"rep3e", "trace_sha", "trace_n", "unfinished", "delivered_by_t_limit"}          # a stored run also carries its trace digest, compared separately
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


def after_run(name, fp, seed, r, edge, partners, design=None):
    fails = gate_failures(r, edge, design) + pair_failures(fp, seed, r, partners)
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
    r["unfinished"] = int(r.get("finished") is False)                        # recorded per run: a stop for every design but today's Realtime
    r["delivered_by_t_limit"] = r["share_any"]
    return r


def work(task):
    fp, seed = task
    e = FPS[fp]
    wait_for_memory(need_for_fp(fp))                                         # each run waits for its measured peak plus a margin
    t = time.time()
    r = run_one(e["builder"](seed), e["trace"])
    r["wall_s"] = time.time() - t
    store(fp, seed, r)
    return fp, seed, r["wall_s"]


def cell_order(job):
    order = ["K-I2", "K-L10", "K-OV", "RT", "CLc", "CL", "CLw-long", "CL-long", "OV", "Ec", "E-I2", "M-I2", "CLc-v", "CL-long-v", "OV-long", "OV-long-v"]
    return order.index(job.cell) if job.cell in order else -1


# ---- TIME (registration 14.8) --------------------------------------------------------------------------------------------------------
TIME_CELLS = ("CL", "CL-long", "OV", "OV-long", "CLw-long", "CLc", "CLs-long", "RT", "K-I2", "K-L10", "K-OV", "Ec", "E-I2", "M-I2", "CLc-v", "CL-long-v", "OV-long-v")


def time_jobs():
    """One seed of each cell x design under repair RL: the new model at the cell's named point, ADR-0084 at 0.99, today's Realtime at delta 50
    (0.005 in the video and file cells) and today's Bulk.  Timing only: not registered, never stored."""
    for cell in TIME_CELLS:
        keys = [("lt", primary_T(cell), 0.0), ("adr", 0.99), ("rt", 0.005 if delta_of(cell) == 0.005 else 50.0), ("bulk",)]
        for key in keys:
            add("TIME", "TIME " + job_name(cell, row_name(key), "RL"), cell, key, "RL", row_builder(cell, key, "RL"), 1)


def plan_classes():
    """Runs to execute before the verdict by class (cell, design, mode): the fingerprints of AA, AA2, DECIDE, EARLY, MAIN and REST, their seeds, less
    the seeds a re-baseline or proof stage 1 row already holds when reuse is on.  Doublings are not included."""
    out = {}
    for p in ("AA", "AA2", "DECIDE", "EARLY", "MAIN", "REST"):
        for fp in attributed(p):
            j = FPS[fp]["job"]
            n = sum(1 for s in range(FPS[fp]["nseeds"]) if not (reuse_on() and _reused_path(fp, s) is not None))
            mode = j.mode if isinstance(j.mode, str) else "RL"
            k = (j.cell, j.design, mode)
            out[k] = out.get(k, 0) + n
    return out


def time_total(times, classes, workers=9):
    """Registration 14.7's sum for one pool of `workers`.  times: (cell, design) -> (seconds of the RL run, peak MB); classes: (cell, design, mode) -> runs.
    A repair-off class takes half its RL time (the ratio of 14.1).  Returns (hours, table)."""
    rows, total = [], 0.0
    for (cell, design, mode), n in sorted(classes.items(), key=lambda kv: (kv[0][0], kv[0][1], str(kv[0][2]))):
        if n == 0 or (cell, design) not in times:
            continue
        sec, peak = times[(cell, design)]
        t = sec * (0.5 if mode == "OFF" else 1.0)
        hours = n * t / workers / 3600.0
        total += hours
        rows.append(dict(cell=cell, design=design, mode=mode, runs=n, seconds_each=t, peak_mb=peak, workers=workers, hours=hours))
    return total, rows


_TIME_ROW = re.compile(r"^TIME\s+(\S+)\s+(\S+)\s+([0-9.]+) s\s+([0-9.]+) MB")


def time_rows_from_log(text, bound_s=10.0):
    """(cell, design) -> (seconds, peak MB) from `TIME cell design sec s MB MB` lines; a row not recorded is bounded at bound_s seconds and the largest
    recorded peak.  Returns (rows, the recorded rows of a long cell that are missing)."""
    rows = {}
    for line in text.splitlines():
        m = _TIME_ROW.match(line.strip())
        if m:
            rows[(m.group(1), m.group(2))] = (float(m.group(3)), float(m.group(4)))
    worst = max([v[1] for v in rows.values()], default=DEFAULT_PEAK_MB)
    missing = [(j.cell, j.design) for j in PARTS["TIME"] if j.cell in HEAVY_CELLS and (j.cell, j.design) not in rows]
    for j in PARTS["TIME"]:
        rows.setdefault((j.cell, j.design), (bound_s, worst))
    return rows, missing


def time_gate(total_h, table, limit_h=24.0):
    """PASS-time if the total is at most about 24 h; else STOP with the table and the total (the cells then move to the VM, whole)."""
    os.makedirs(OUT, exist_ok=True)
    if total_h <= limit_h:
        open(os.path.join(OUT, "PASS-time"), "w").close()
        return True
    write_stop("TIME gate: total %.1f h over %.0f h; whole cells move to the VM (every arm and seed of a cell on one machine and one commit). Table: %s"
               % (total_h, limit_h, json.dumps(table)[:4000]))
    return False


def run_time(results_path, from_log=""):
    """TIME (registration 14.8).  Runs the rows one at a time, slowest first, recording wall time and peak resident memory (sampled every 0.5 s); each row is
    appended to out3e/time_rows.jsonl as it finishes, so a restart skips the rows done.  With from_log the rows come from a TIME log and the ones missing are
    bounded at 10 s (nothing is run).  Writes time.json, the table (results file), TIME_TOTAL.txt, and PASS-time or STOP."""
    import threading
    os.makedirs(OUT, exist_ok=True)
    jobs = sorted(PARTS["TIME"], key=lambda j: -(CELLS[base_of(j.cell)][1]["n_total"] * (3 if is_heavy(j.builder(0)) else 1)))
    bounded = set()
    if from_log:
        with open(from_log, encoding="utf-8", errors="replace") as f:
            times, missing = time_rows_from_log(f.read())
        if missing:
            print("TIME: long cells not covered by the log:", missing)
            write_stop("TIME from log: rows missing for long cells %s" % missing)
            return False
        done = set()
        with open(from_log, encoding="utf-8", errors="replace") as f:
            for line in f:
                m = _TIME_ROW.match(line.strip())
                if m:
                    done.add((m.group(1), m.group(2)))
        bounded = {(j.cell, j.design) for j in jobs} - done
        rows = [(j.cell, j.design) + times[(j.cell, j.design)] for j in jobs]
    else:
        rowfile = os.path.join(OUT, "time_rows.jsonl")
        times = {}
        if os.path.exists(rowfile):
            with open(rowfile) as f:
                for line in f:
                    r = json.loads(line)
                    times[(r[0], r[1])] = (r[2], r[3])
        proc = psutil.Process()
        for j in jobs:
            if (j.cell, j.design) in times:
                continue
            peak, stop = [0.0], threading.Event()

            def watch():
                while not stop.is_set():
                    peak[0] = max(peak[0], proc.memory_info().rss / 2.0 ** 20)
                    stop.wait(0.5)
            th = threading.Thread(target=watch, daemon=True)
            th.start()
            wait_for_memory(need_gb(DEFAULT_PEAK_MB))
            t0 = time.time()
            run_one(j.builder(0), False)
            sec = time.time() - t0
            stop.set()
            th.join()
            peak[0] = max(peak[0], proc.memory_info().rss / 2.0 ** 20)
            times[(j.cell, j.design)] = (sec, peak[0])
            with open(rowfile, "a") as f:
                f.write(json.dumps([j.cell, j.design, sec, peak[0]]) + "\n")
            print("TIME %-10s %-5s %7.1f s %7.0f MB" % (j.cell, j.design, sec, peak[0]), flush=True)
        rows = [(j.cell, j.design) + times[(j.cell, j.design)] for j in jobs]
    total_h, table = time_total(times, plan_classes())
    with open(os.path.join(OUT, "time.json"), "w") as f:
        json.dump(dict(rows=rows, total_hours=total_h, table=table, bounded=sorted(bounded)), f)
    with open(os.path.join(OUT, "TIME_TOTAL.txt"), "w") as f:
        f.write("TIME total, one pool of 9 workers: %.2f h (gate 24 h)\n" % total_h)
    md = ["", "## TIME (registration 14.8): one seed of each cell x design under repair RL, one process", "",
          "| cell | design | wall (s) | peak RSS (MB) | |", "|---|---|---|---|---|"] + ["| %s | %s | %.1f | %.0f | %s |" % (r + ("bounded at 10 s, not run" if (r[0], r[1]) in bounded else "",)) for r in rows]
    md += ["", "Total over the registered run counts before the verdict, one pool of 9 workers: **%.1f h** (gate: 24 h) -> %s" % (total_h, "PASS-time" if total_h <= 24 else "STOP"), ""]
    with open(results_path, "a", newline="\n") as f:
        f.write("\n".join(md))
    return time_gate(total_h, table)



def write_joblist(part, jobs):
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "%s.json" % part.replace(":", "-")), "w") as f:
        json.dump({j.name: j.fp for j in jobs}, f, indent=1)


class run_tasks:
    """Runs the tasks in one pool of `workers` and yields (fingerprint, seed, wall) as they finish; with one worker they run in this process."""

    def __init__(self, tasks, workers):
        import queue
        self.q, self.n, self.pool = queue.Queue(), len(tasks), None
        self.tasks = tasks
        if workers <= 1 or not tasks:
            return
        self.pool = mp.Pool(workers, initializer=build)
        for t in tasks:
            self.pool.apply_async(work, (t,), callback=self.q.put, error_callback=self.q.put)

    def __iter__(self):
        if self.pool is None:
            for t in self.tasks:
                yield work(t)
            return
        for _ in range(self.n):
            r = self.q.get()
            if isinstance(r, BaseException):
                self.close()
                raise r
            yield r
        self.close()

    def close(self):
        if self.pool is not None:
            self.pool.terminate()


def smoke_pool():
    """--smoke-pool: the --workers 2 path on two tiny configurations (RT, 300 units, repair off and RL, 2 seeds), in a temporary store, with the
    markers waived.  Not a part of any registered run; start it only on a quiet machine."""
    import subprocess
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        env = dict(os.environ, BATCH3E_OUT=td, BATCH3E_SMOKE="1")
        r = subprocess.run([sys.executable, os.path.abspath(__file__), "--part", "AA", "--workers", "2", "--seeds", "2"], env=env, cwd=HERE,
                           capture_output=True, text=True)
        files = sorted(os.listdir(os.path.join(td, "runs"))) if os.path.isdir(os.path.join(td, "runs")) else []
        ok = r.returncode == 0 and len(files) == 4 and os.path.exists(os.path.join(td, "DONE-AA"))
        print(r.stdout[-600:], r.stderr[-300:])
        print("SMOKE POOL:", "PASS" if ok else "FAIL", "(rc %d, %d run files)" % (r.returncode, len(files)))
        return 0 if ok else 1


def main(argv=None):
    if "--smoke-pool" in (argv if argv is not None else sys.argv[1:]):
        return smoke_pool()
    ap = argparse.ArgumentParser()
    ap.add_argument("--part", required=True)                                 # (--smoke-pool needs no part)
    ap.add_argument("--seeds", type=int, default=SEEDS)
    ap.add_argument("--seed0", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--cells", default="")
    ap.add_argument("--workers", type=int, default=1, help="one pool, at most 9")
    ap.add_argument("--from-log", default="", help="TIME: take the rows from a TIME log instead of running them; rows missing are bounded at 10 s")
    ap.add_argument("--reuse-dirs", default="")
    ap.add_argument("--results", default=os.path.join(HERE, "results-stage3e.md"))
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--double-partial", action="store_true", help="DOUBLE of one cell while its part is still running cell by cell")
    a = ap.parse_args(argv)
    build()
    if a.reuse_dirs:
        set_reuse([x for x in a.reuse_dirs.split(",") if x])
    if a.list:
        tot = dict(configs=0, new=0, runs=0)
        for p, c in counts().items():
            print("%-8s %4d configurations, %4d after the dedupe, %5d runs" % (p, c["configs"], c["new"], c["runs"]))
            for k in tot:
                tot[k] += c[k]
        print("%-8s %4d configurations, %4d after the dedupe, %5d runs" % ("total", tot["configs"], tot["new"], tot["runs"]))
        if REUSE["index"]:
            n = sum(1 for fp in FPS for s_ in range(FPS[fp]["nseeds"]) if stored_result(fp, s_) is not None)
            print("reusable from the stored rows (used only after PASS-reuse): %d runs" % n)
        if a.part != "ALL":
            for j in PARTS.get(a.part, []):
                print("%s  %s" % (j.fp, j.name))
        return 0
    if a.part == "TIME":
        why = refusal("TIME")
        if why:
            print("REFUSED:", why)
            return 2
        return 0 if run_time(a.results, a.from_log) else 1
    double = a.part.startswith("DOUBLE:")
    base = a.part.split(":")[1] if double else a.part
    if base not in PARTS:
        print("unknown part", a.part)
        return 2
    why = refusal(a.part, a.double_partial)
    if why:
        print("REFUSED:", why)
        return 2
    cells = [c for c in a.cells.split(",") if c]
    jobs = [j for j in PARTS[base] if j.name.startswith(a.only) and (not cells or j.cell in cells)]
    if double:
        with open(os.path.join(OUT, "double-%s.json" % base)) as f:
            keep = set(json.load(f))
        fps = [fp for fp in dict.fromkeys(j.fp for j in jobs) if fp in keep]
        seeds = None                                                         # each fingerprint doubles from its own count n to 2n (3 to 6, 6 to 12, 12 to 24)
        limit = None
    else:
        write_joblist(base, PARTS[base])
        fps = list(dict.fromkeys(j.fp for j in jobs))
        seeds = range(a.seed0, a.seed0 + a.seeds)
        limit = None
    cellname = {j.fp: j for j in jobs}
    why = refusal_workers(a.workers)
    if why:
        print("REFUSED:", why)
        return 2

    def seeds_of(fp):
        n = FPS[fp]["nseeds"]
        return range(n, 2 * n) if double else [s for s in range(a.seed0, a.seed0 + a.seeds) if s < n]
    tasks = [(fp, s) for fp in fps for s in seeds_of(fp) if not have_run(fp, s)]
    tasks.sort(key=lambda t: (cell_order(cellname[t[0]]) if base == "REST" else 0, FPS[t[0]]["job"].cell in HEAVY_CELLS, t[1]))
    print("%d runs to do (%d configurations)" % (len(tasks), len(fps)), flush=True)
    t0 = time.time()
    rc = 0
    load_peaks()
    results = run_tasks(tasks, a.workers)
    for i, (fp, s, w) in enumerate(results, 1):
        r = load(fp, s)
        fails = after_run(cellname[fp].name, fp, s, r, FPS[fp]["edge"], FPS[fp]["partners"], cellname[fp].design)
        print("[%6.0fs] %d/%d %s %d %.1fs%s" % (time.time() - t0, i, len(tasks), cellname[fp].name, s, w, "  STOP: " + "; ".join(fails) if fails else ""), flush=True)
        if fails:
            rc = 1
            results.close()
            break
    if rc == 0:
        want = [(fp, s) for fp in fps for s in seeds_of(fp)]
        if not cells and not a.only and base != "BASE" and all(have_run(fp, s) for fp, s in want):
            open(os.path.join(OUT, "DONE-%s" % a.part.replace(":", "-")), "w").close()
            print("DONE-%s" % a.part.replace(":", "-"))
    print("[%6.0fs] done" % (time.time() - t0), flush=True)
    return rc


if __name__ == "__main__":
    mp.freeze_support()
    sys.exit(main())
