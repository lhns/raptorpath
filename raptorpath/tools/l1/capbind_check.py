#!/usr/bin/env python3
"""CAPBIND — the standard BIND-FRACTION readout for the store-cap chain.

ADR-0070 "The store-cap law on trial", prevention kit item 2. The N^2 defect
in `path_scaled_store_cap` survived five sessions of measurement because the
clamp ate the evidence: `clamp(gain*N*Sigma, floor, N*knee)` was pinned at its
ceiling on essentially every dual-cell refresh, so every battery that read the
realized cap was reading the CEILING and reporting it as the law. The pinning
was in the ledgers the whole time (`occcap_p50` = 4096 in 69/69 c7-A reps) and
was filed as a cell fact rather than as a law defect.

This module makes that reading standard and unmissable: for every (cell, arm)
group it prints, per KNOWN bind value, the fraction of reps whose realized
median cap sat exactly on it --

    CAPBIND cell=c7 arm=A ceiling=4096 frac=1.000 n=69 name=N*knee

-- and a WARN line whenever a fraction exceeds 0.5:

    WARN c7/A: cap == 4096 (N*knee) in 100.0% of reps -- the law is
         operating as a CONSTANT over its measured range.

THE 0.5 IS A REPORTING AID, NOT A LAW. Nothing downstream keys on it; no gate
passes or fails on it; it is the level at which "the majority of this arm's
reps never saw the law vary" becomes true, and it exists so that a degenerate
law announces itself in the report instead of waiting for a formula review
(discipline: "a measurement showing a law pinned over its operating range is a
defect finding, never an explanatory footnote").

THE BIND VALUES ARE THE CHAIN'S OWN CONSTANTS, quoted by site, never fitted:

    10            `STORE_CAP_FLOOR`      floor of the pooled laws (DERIVED, §16.59)
    128           `RWM_STORE_BOOT`       `store_boot_cap`, the cold fallback
    1024          `RELIABLE_STORE_MAX`   the single-path (N < 2) latch
    N * 2048      `RWM_STORE_PATH_POOL`  the pooled ceiling, N = live paths
    4096          `WIN_STORE_MAX`        the memory bound / three-term ceiling

`N` comes from the CELL's geometry (`CELL_PATHS`), because the cap is a
per-transfer quantity and the batteries name their cells. An unknown cell
contributes its floor/boot/1024/4096 binds and skips the N-dependent one
rather than guessing a path count.

Usable standalone as well as imported:

    capbind_check.py <ledger.log> [...]      # any *RESULT json rows
"""
import json
import re
import sys
from collections import defaultdict

# ── the chain's constants, by site ──────────────────────────────────────────
# `net::sender_policy::STORE_CAP_FLOOR` — DERIVED 2026-08-18 (paper §16.59) as
# `max(ANCHOR_MIN_SAMPLES * MERGED_ACK_SYMBOLS_PER_SAMPLE, RFC6928_INITIAL_WINDOW)`
# = max(8*1, 10) = 10, replacing the bare 64 ADR-0070 finding 5 recorded as
# PROVENANCE ABSENT. NOTE: there is no `RWM_STORE_FLOOR` gate and there never
# was — the earlier docstring named one that does not exist. Ledgers collected
# BEFORE 2026-08-18 were produced on the 64 and are read with LEGACY_STORE_FLOOR.
STORE_FLOOR = 10        # sender_policy::STORE_CAP_FLOOR (derived)
LEGACY_STORE_FLOOR = 64  # the pre-2026-08-18 bare constant, for old ledgers
STORE_BOOT = 128        # `store_boot_cap`, `RWM_STORE_BOOT`
RELIABLE_STORE_MAX = 1024   # net/mod.rs, the N < 2 latch
STORE_PATH_POOL = 2048  # `RWM_STORE_PATH_POOL`, the per-path knee
WIN_STORE_MAX = 4096    # net/mod.rs:3215

#: The live-path count of each named L1 cell. Cells are DEFINED by their
#: geometry in the battery drivers (`tools/l1/*_battery.sh`, `topo*.sh`), so
#: this is transcription, not inference. `c8L` is c8 at the 200 MB length;
#: `c9` is the quad-path cell (ADR-0070 prevention kit item 3).
CELL_PATHS = {
    "c1": 1, "c2": 1, "c2r100": 1, "c3": 1, "sc2": 1,
    # the advanced-cell family (`adv_cells.sh`) — all single-path, c2-class
    "c2ctl": 1, "jit0": 1, "jit5": 1, "jit15": 1, "jit25": 1,
    "shal8": 1, "pol100": 1,
    "c7": 2, "c8": 2, "c8L": 2, "c8t": 2,
    "c9": 4,
}

#: The warn level. A REPORTING AID (see the module docstring) -- no gate,
#: no criterion, no flip decision reads it.
WARN_FRAC = 0.5


def known_binds(cell):
    """[(label, value)] for `cell` -- the values the realized cap can sit on
    without the law having varied. The N-dependent ceiling is omitted (rather
    than guessed) for a cell whose geometry is not transcribed above."""
    binds = [
        ("floor", STORE_FLOOR),
        ("floor_legacy", LEGACY_STORE_FLOOR),
        ("boot", STORE_BOOT),
        ("RELIABLE_STORE_MAX", RELIABLE_STORE_MAX),
        ("WIN_STORE_MAX", WIN_STORE_MAX),
    ]
    n = CELL_PATHS.get(cell)
    if n is not None and n >= 2:
        binds.append(("N*knee", n * STORE_PATH_POOL))
    # Values COLLIDE by construction at a dual (2*2048 == WIN_STORE_MAX), and
    # a colliding value is one bind with two names, not two binds: report it
    # once, named for every clamp it could be, so the readout never claims to
    # distinguish clamps the number cannot distinguish.
    by_val = {}
    for label, v in binds:
        by_val.setdefault(v, []).append(label)
    return [("=".join(labels), v) for v, labels in sorted(by_val.items())]


def capbind_lines(rows, cap_key="occcap_p50", cells=None, arms=None):
    """The CAPBIND block for `rows` (parsed *RESULT dicts), as a list of
    printable strings. Groups by (cell, arm); pools over seeds, because a
    bind value is an INTEGER the law either sat on or did not -- no goodput
    statistic is pooled here and none is claimed."""
    g = defaultdict(list)
    for r in rows:
        v = r.get(cap_key)
        if v is None:
            continue
        g[(r.get("cell"), r.get("arm"))].append(v)

    order = sorted(g, key=lambda k: (str(k[0]), str(k[1])))
    if cells:
        order = [k for k in order if k[0] in cells]
    if arms:
        order = [k for k in order if k[1] in arms]

    out = ["CAPBIND -- realized store cap vs the chain's known clamps"
           f" (key={cap_key}; warn at frac > {WARN_FRAC:.2f}, a reporting aid,"
           " ADR-0070 kit item 2)"]
    if not order:
        out.append(f"  (no rows carry `{cap_key}` -- nothing to check)")
        return out
    for cell, arm in order:
        caps = g[(cell, arm)]
        n = len(caps)
        hits = []
        for label, val in known_binds(cell):
            k = sum(1 for c in caps if int(round(c)) == val)
            if k:
                hits.append((label, val, k / n))
        if not hits:
            lo, hi = min(caps), max(caps)
            out.append(f"CAPBIND cell={cell} arm={arm} ceiling=none frac=0.000 n={n}"
                       f" name=interior  (range {lo:.0f}..{hi:.0f})")
            continue
        for label, val, frac in sorted(hits, key=lambda h: -h[2]):
            out.append(f"CAPBIND cell={cell} arm={arm} ceiling={val}"
                       f" frac={frac:.3f} n={n} name={label}")
            if frac > WARN_FRAC:
                out.append(f"  WARN {cell}/{arm}: cap == {val} ({label}) in"
                           f" {100*frac:.1f}% of reps -- the law is operating as a"
                           " CONSTANT over its measured range")
    return out


def print_capbind(rows, **kw):
    for ln in capbind_lines(rows, **kw):
        print(ln)


_RESULT = re.compile(r"\b[A-Z0-9]+RESULT (\{.*)$")


def load_any(paths):
    """Every `<TAG>RESULT {json}` row in `paths`, whatever the battery."""
    rows = []
    for p in paths:
        with open(p, errors="replace") as f:
            for ln in f:
                m = _RESULT.search(ln)
                if not m:
                    continue
                try:
                    rows.append(json.loads(m.group(1)))
                except Exception:
                    pass
    return rows


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    print_capbind(load_any(sys.argv[1:]))
