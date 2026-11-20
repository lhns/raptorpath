#!/usr/bin/env python3
"""Tests for the pinned-law check (tools/l1/pinned_law_check.py, pinned_summary.py, pinned_gauges.py; S3 part 1).

  1. classifier fixtures with KNOWN answers (every gauge kind, the threshold edge, UZ with and without its witness);
  2. ledger reading: the threshold, the verdict scopes, an unknown scope string fails;
  3. the committed state: green, and the committed summary is fresh against the archive hashes;
  4. the known defect: F01 (pooled store cap) with its PINNED verdict removed is RED and names F01's c8 cells;
     an unlisted cell is RED; a bogus owed line is RED (stale entry); a UZ cell cannot be covered by a verdict.
Set PINNED_FULL=1 to also rebuild the summary from the archives (pinned_summary.py --check, about 20 s).
Run: python3 test_pinned_law_check.py"""
import copy
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import pinned_gauges as pg  # noqa: E402
import pinned_law_check as plc  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def scratch(text, name):
    d = tempfile.mkdtemp(prefix="pinned_")
    p = os.path.join(d, name)
    with open(p, "w", newline="\n", encoding="utf-8") as f:
        f.write(text)
    return p


# 1. classifier fixtures
mode = dict(kind="mode", tag="SPAN", field="x", absent_nr=False, w=None)
check("mode: 0.5 of ticks at the mode is PIN, just under is EX",
      plc.classify(mode, dict(n=100, present=100, mode=1.0, mode_n=50), 0.5)[0] == "PIN"
      and plc.classify(mode, dict(n=100, present=100, mode=1.0, mode_n=49), 0.5)[0] == "EX")
check("mode: the same 0.85 reading is PIN at 0.5 and EX at 0.9",
      plc.classify(mode, dict(n=100, present=100, mode=1.0, mode_n=85), 0.5)[0] == "PIN"
      and plc.classify(mode, dict(n=100, present=100, mode=1.0, mode_n=85), 0.9)[0] == "EX")
check("mode: field absent is NG, or NR when the spec says absent is NR",
      plc.classify(mode, dict(n=10, present=0), 0.5)[0] == "NG"
      and plc.classify(dict(mode, absent_nr=True), dict(n=10, present=0), 0.5)[0] == "NR")
bind = dict(kind="bind", tag="SPAN", pred="p", pre="q", absent_nr=False)
check("bind: precondition never held is NR, held with a low share is EX, high share is PIN",
      plc.classify(bind, dict(n=10, k=0, pre_n=0), 0.5)[0] == "NR"
      and plc.classify(bind, dict(n=10, k=1, pre_n=3), 0.5)[0] == "EX"
      and plc.classify(bind, dict(n=10, k=9, pre_n=3), 0.5)[0] == "PIN")
zero = dict(kind="zero", tag="DIAG", field="c", w="fired", pre=None)
check("zero counter: all 0 with its witness is PIN; all 0 WITHOUT a witness is UZ; a nonzero reading is EX",
      plc.classify(zero, dict(n=10, present=10, k0=10, w_n=4), 0.5)[0] == "PIN"
      and plc.classify(zero, dict(n=10, present=10, k0=10, w_n=0), 0.5)[0] == "UZ"
      and plc.classify(zero, dict(n=10, present=10, k0=2, w_n=4), 0.5)[0] == "EX")
check("zero counter with a precondition that never held is NR (the law's other branch ran)",
      plc.classify(dict(zero, pre="np1"), dict(n=10, present=10, k0=10, w_n=4, pre_n=0), 0.5)[0] == "NR")
modew = dict(mode, w="rr_pos")
check("mode at 0 needs its witness: owed = 0 with no rr > 0 tick is UZ",
      plc.classify(modew, dict(n=10, present=10, mode=0.0, mode_n=10, w_n=0), 0.5)[0] == "UZ"
      and plc.classify(modew, dict(n=10, present=10, mode=0.0, mode_n=10, w_n=3), 0.5)[0] == "PIN")
end_share = dict(kind="end", side="c", tag="SUMCAP", site=None, field="pin", how="share", off=None, absent_nr=True)
check("end share: median of the rows against the threshold; no line at the seat is NR when declared",
      plc.classify(end_share, dict(vals=[0.7, 0.8, 0.9]), 0.5)[0] == "PIN"
      and plc.classify(end_share, dict(vals=[0.7, 0.8, 0.9]), 0.9)[0] == "EX"
      and plc.classify(end_share, dict(vals=[None, None]), 0.5)[0] == "NR")
gate = dict(kind="gate", key="RWM_X", off="0")
check("gate: every row at its off value is NR, any row on is EX",
      plc.classify(gate, dict(vals=["0", "0"]), 0.5)[0] == "NR" and plc.classify(gate, dict(vals=["0", "1"]), 0.5)[0] == "EX")

# 2. ledger reading
lthr, verdicts = plc.read_ledger(plc.LEDGER)
check("ledger threshold is read as 0.5", lthr == 0.5, lthr)
check("ledger verdict tags are read (F01 carries PINNED(c8,c1s) and DISHONEST-INPUT)",
      verdicts.get("F01") == ["PINNED(c8,c1s)", "DISHONEST-INPUT"], verdicts.get("F01"))
check("F01's verdict covers the three c8 seats and no c2 seat", plc.covered_seats(verdicts["F01"], "F01") == {"c8 B", "c8 A", "c8 R"})
try:
    plc.covered_seats(["PINNED(somewhere new)"], "ZZ")
    check("an unknown verdict scope string fails", False)
except SystemExit as e:
    check("an unknown verdict scope string fails", "somewhere new" in str(e))
every = [t for tags in verdicts.values() for t in tags if t.startswith(("PINNED(", "DEGENERATE("))]
try:
    for r, tags in verdicts.items():
        plc.covered_seats(tags, r)
    check("every PINNED/DEGENERATE scope in the committed ledger has a seat mapping (%d tags)" % len(every), True)
except SystemExit as e:
    check("every PINNED/DEGENERATE scope in the committed ledger has a seat mapping", False, e)

# 3. the committed state
summary = json.load(open(plc.SUMMARY))
check("summary covers every seat and gauge", all(set(summary["seats"][s]["gauges"]) == set(pg.G) for s in pg.SEATS))
check("summary is fresh (spec fingerprint and archive sha256)", plc.stale_summary(summary) == [], plc.stale_summary(summary))
red = plc.run()
check("committed ledger + owed list: GREEN", red == [], red[:5])
n05 = len(plc.unverdicted(plc.classify_all(summary, 0.5), verdicts))
n09 = len(plc.unverdicted(plc.classify_all(summary, 0.9), verdicts))
check("the owed list holds exactly the unverdicted cells at the ledger threshold (%d; %d at 0.9)" % (n05, n09),
      len(plc.load_owed(plc.OWED)) == n05 and n09 <= n05)
if os.environ.get("PINNED_FULL"):
    r = subprocess.run([sys.executable, os.path.join(HERE, "pinned_summary.py"), "--check"], capture_output=True, text=True)
    check("pinned_summary.py --check: summary matches the archives", r.returncode == 0, r.stdout + r.stderr)

check("F01 replay through the existing caplog_replay port: no mismatch, the unclamped law moves", plc.replay_failures(summary) == [], plc.replay_failures(summary))
bad = copy.deepcopy(summary)
bad["replay"]["F01"]["docs/l1-raw/s60/rep60a-diag-ld.tgz"].update(mismatches=3, unclamped_distinct=1)
check("F01 replay: a logged cap that differs from the law, or a constant unclamped output, is RED", len(plc.replay_failures(bad)) == 2, plc.replay_failures(bad))

# 4. red runs on scratch copies
ledger_text = open(plc.LEDGER, encoding="utf-8").read()
no_f01 = ledger_text.replace('verdicts: ["PINNED(c8,c1s)", DISHONEST-INPUT]', "verdicts: [DISHONEST-INPUT]", 1)
check("scratch ledger differs from the committed one", no_f01 != ledger_text)
p = scratch(no_f01, "law-ledger.yaml")
red = plc.run(ledger=p)
check("F01 with its PINNED verdict removed: RED on every c8 cell (DISHONEST-INPUT does not cover a pin)",
      all(any(x.startswith("UNVERDICTED F01 %s PIN" % s) for x in red) for s in ("c8 B", "c8 A", "c8 R")), red[:5])
check("... and only F01 changes", all(x.split()[1] == "F01" for x in red), red[:5])
r = subprocess.run([sys.executable, os.path.join(HERE, "pinned_law_check.py"), "--ledger", p], capture_output=True, text=True)
check("CLI exits 1 on the scratch ledger", r.returncode == 1 and "FAIL UNVERDICTED F01 c8 R PIN" in r.stdout, r.stdout[-600:])

owed_text = open(plc.OWED, encoding="utf-8").read()
bogus = owed_text + "OWED E01 c2_A PIN | bogus entry\n"
red = plc.run(owed_path=scratch(bogus, "pinned_owed.txt"))
check("a bogus owed line is RED as a stale entry", len(red) == 1 and red[0].startswith("STALE owed entry E01 c2 A PIN"), red)
first = next(l for l in owed_text.splitlines() if l.startswith("OWED"))
red = plc.run(owed_path=scratch(owed_text.replace(first + "\n", "", 1), "pinned_owed.txt"))
check("a real unverdicted cell missing from the owed list is RED", len(red) == 1 and red[0].startswith("UNVERDICTED"), red)

fx = copy.deepcopy(summary)
fx["seats"]["c2 A"]["gauges"]["C04"].update(w_n=0)  # the counter reads 0 and the path never fired
fxp = scratch(json.dumps(fx), "pinned_summary.json")
red = plc.run(summary_path=fxp, check_fresh=False)
check("a zero counter without its witness (C04 at c2 A) is RED as UZ", any(x.startswith("UNVERDICTED C04 c2 A UZ") for x in red), red[:4])
v = dict(verdicts)
v["C04"] = ["PINNED(all one-way cells)", "DEGENERATE(all one-way cells)"]
check("a PINNED/DEGENERATE verdict does not cover a UZ cell",
      ("C04", "c2 A", "UZ") in plc.unverdicted(plc.classify_all(fx, 0.5), v))

print("%d failure(s)" % fails)
sys.exit(1 if fails else 0)
