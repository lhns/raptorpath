#!/usr/bin/env python3
"""Pinned-law check over the committed L1 logs (MEASUREMENT DISCIPLINE 1 and 18; test-detection plan S3 part 1).

For every ledger row with a gauge spec (pinned_gauges.py) and every seat, classify the law from the parsed log
summary (pinned_summary.json, rebuilt from docs/l1-raw by pinned_summary.py):
  EX  varies | PIN  sits at one bound or constant on >= threshold of ticks | NR  its precondition never held,
  or it is off by gate | UZ  a counter reads 0 and its execution witness is absent | NG  no gauge or no reading.
The threshold is the ledger's pinned_threshold (a bind fraction >= 0.5, read as the share of ticks at the bound);
the audit's 90 % reading is reported next to it. A PIN, NR or UZ cell fails unless the ledger row carries a
PINNED(...) or DEGENERATE(...) verdict whose scope covers the seat (UZ is never covered by a verdict) or the
cell is on the owed list (pinned_owed.txt). An owed entry that no longer reproduces fails too.

  pinned_law_check.py [--ledger F] [--owed F] [--summary F] [--threshold X] [--table] [--audit-diff] [--write-owed]
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
import pinned_gauges as pg  # noqa: E402

LEDGER = os.path.join(ROOT, "docs", "derivations", "law-ledger.yaml")
OWED = os.path.join(HERE, "pinned_owed.txt")
SUMMARY = os.path.join(HERE, "pinned_summary.json")
AUDIT = os.path.join(ROOT, "docs", "derivations", "liveness-audit.md")
NEEDS_VERDICT = ("PIN", "NR", "UZ")


def read_ledger(path):
    """-> (threshold, {row id: [verdict tags]}). No yaml module (the VM python may lack it); the shape is fixed by render.py."""
    text = open(path, encoding="utf-8").read()
    m = re.search(r'^\s*pinned_threshold:\s*"[^"]*?>=\s*([0-9.]+)', text, re.M)
    if not m:
        raise SystemExit("cannot read pinned_threshold from " + path)
    verdicts, cur = {}, None
    for line in text.splitlines():
        m2 = re.match(r"- id: (\S+)", line)
        if m2:
            cur = m2.group(1)
            continue
        m3 = re.match(r"  verdicts: \[(.*)\]\s*$", line)
        if m3 and cur:
            tags = re.findall(r'"([^"]*)"|([^,\[\]"]+)', m3.group(1))
            verdicts[cur] = [(a or b).strip() for a, b in tags if (a or b).strip()]
    return float(m.group(1)), verdicts


def covered_seats(tags, row):
    """Seats a row's PINNED(..)/DEGENERATE(..) verdicts cover; an unlisted scope string is an error."""
    seats = set()
    for t in tags:
        m = re.match(r"(PINNED|DEGENERATE)\((.*)\)$", t)
        if not m:
            continue
        inner = m.group(2)
        parts = [inner] if inner in pg.SCOPES else [s.strip() for s in inner.split(",")]
        for s in parts:
            if s not in pg.SCOPES:
                raise SystemExit("%s: verdict scope %r has no seat mapping in pinned_gauges.SCOPES" % (row, s))
            seats.update(pg.SCOPES[s])
    return seats


def classify(g, d, thr):
    """-> (class, reading) for one gauge's summary d at threshold thr."""
    k = g["kind"]
    if k == "gate":
        v = d["vals"]
        if all(x is None for x in v):
            return "NG", "no [GATES] echo"
        if all(x == g["off"] for x in v):
            return "NR", "%s=%s on %d/%d rows" % (g["key"], g["off"], len(v), len(v))
        return "EX", "gate on"
    if k == "end":
        v = [x for x in d["vals"] if x is not None]
        if not v:
            return ("NR", "no %s line at the seat" % g["tag"]) if g["absent_nr"] else ("NG", "no %s line" % g["tag"])
        if g["how"] == "off":
            if sum(v) == len(v):
                return "NR", "%s=%s on %d/%d rows" % (g["field"], g["off"], len(v), len(v))
            return "EX", "%s != %s on %d/%d rows" % (g["field"], g["off"], len(v) - sum(v), len(v))
        if g["how"] == "latched":
            return ("PIN" if sum(v) / len(v) >= thr else "EX"), "latched in %d/%d rows" % (sum(v), len(v))
        med = sorted(v)[len(v) // 2]
        return ("PIN" if med >= thr else "EX"), "%s median %.3f over %d rows" % (g["field"], med, len(v))
    n = d["n"]
    if k == "mode":
        if d["present"] == 0:
            return ("NR", "no %s field at the seat" % g["field"]) if g["absent_nr"] else ("NG", "no %s field" % g["field"])
        sh = d["mode_n"] / n
        if d["mode"] == 0 and g.get("w") and sh >= thr and d.get("w_n", 0) == 0:
            return "UZ", "%s = 0 on %.3f of %d ticks, witness %s never held" % (g["field"], sh, n, g["w"])
        return ("PIN" if sh >= thr else "EX"), "%s mode %g on %.3f of %d ticks" % (g["field"], d["mode"], sh, n)
    if k == "bind":
        if n == 0:
            return "NG", "no ticks"
        if g["pre"] and d.get("pre_n", 0) == 0:
            return "NR", "precondition %s never held in %d ticks" % (g["pre"], n)
        sh = d["k"] / n
        return ("PIN" if sh >= thr else "EX"), "%s on %.3f of %d ticks" % (g["pred"], sh, n)
    if k == "zero":
        if d["present"] == 0:
            return "NG", "no %s field" % g["field"]
        if g["pre"] and d.get("pre_n", 0) == 0:
            return "NR", "precondition %s never held in %d ticks" % (g["pre"], n)
        if d["k0"] == n:
            if d["w_n"] == 0:
                return "UZ", "%s = 0 on all %d ticks, witness %s never held" % (g["field"], n, g["w"])
            return "PIN", "%s = 0 on all %d ticks (witness %s on %d)" % (g["field"], n, g["w"], d["w_n"])
        return ("PIN" if d["k0"] / n >= thr else "EX"), "%s = 0 on %.3f of %d ticks" % (g["field"], d["k0"] / n, n)
    raise SystemExit("unknown gauge kind " + k)


def classify_all(summary, thr):
    return {(row, seat): classify(g, summary["seats"][seat]["gauges"][row], thr) for row, g in pg.G.items() for seat in pg.SEATS}


def unverdicted(cells, verdicts):
    out = {}
    for (row, seat), (cls, reading) in cells.items():
        if cls not in NEEDS_VERDICT:
            continue
        if cls != "UZ" and seat in covered_seats(verdicts.get(row, []), row):
            continue
        out[(row, seat, cls)] = reading
    return out


def owed_line(key, reading):
    return "OWED %s %s %s | %s" % (key[0], key[1].replace(" ", "_"), key[2], reading)


def load_owed(path):
    owed = {}
    for line in open(path, encoding="utf-8"):
        if not line.strip() or line.startswith("#"):
            continue
        head, _, reading = line.partition("|")
        f = head.split()
        if len(f) != 4 or f[0] != "OWED" or f[3] not in NEEDS_VERDICT:
            raise SystemExit("bad owed line: " + line.strip())
        owed[(f[1], f[2].replace("_", " "), f[3])] = reading.strip()
    return owed


def stale_summary(summary):
    import pinned_summary as ps
    bad = []
    if summary.get("spec") != ps.spec_fingerprint():
        bad.append("summary is stale: pinned_gauges.py or plib.py changed (run pinned_summary.py --write)")
    for arch, sha in summary["archives"].items():
        p = os.path.join(ROOT, arch)
        if not os.path.exists(p):
            print("%s: archive absent (export-ignored): sha check skipped" % arch)
        elif ps.sha256(p) != sha:
            bad.append("summary is stale: %s changed (run pinned_summary.py --write)" % arch)
    return bad


def replay_failures(summary):
    """F01 replay (existing port store-law/caplog_replay.py over the committed [CAPLOG] inputs): the logged cap must equal the law's
    output for the logged inputs, and the unclamped law output must take more than one value (else the law is a constant)."""
    bad = []
    for arch, d in summary.get("replay", {}).get("F01", {}).items():
        if d["records"] == 0 or d["replayed"] != d["records"]:
            bad.append("F01 replay %s: %d records, %d replayed" % (arch, d["records"], d["replayed"]))
        if d["mismatches"]:
            bad.append("F01 replay %s: %d logged caps differ from the law's output for the logged inputs" % (arch, d["mismatches"]))
        if d["unclamped_distinct"] < 2:
            bad.append("F01 replay %s: the unclamped law output takes one value over the logged range" % arch)
    return bad


def run(ledger=LEDGER, owed_path=OWED, summary_path=SUMMARY, threshold=None, check_fresh=True):
    """-> list of failure strings (empty = green)."""
    summary = json.load(open(summary_path))
    lthr, verdicts = read_ledger(ledger)
    fails = (stale_summary(summary) if check_fresh else []) + replay_failures(summary)
    unv = unverdicted(classify_all(summary, lthr if threshold is None else threshold), verdicts)
    owed = load_owed(owed_path)
    for key, reading in sorted(unv.items()):
        if key not in owed:
            fails.append("UNVERDICTED %s %s %s (%s): no PINNED/DEGENERATE verdict covering that seat in the ledger, not on the owed list" % (key[0], key[1], key[2], reading))
    for key in sorted(owed):
        if key not in unv:
            fails.append("STALE owed entry %s %s %s no longer reproduces" % key)
    return fails


def counts(cells):
    c = {}
    for cls, _ in cells.values():
        c[cls] = c.get(cls, 0) + 1
    return dict(sorted(c.items()))


def audit_cells():
    """The audit's section 1 table, parsed as liveness-audit/count.py does: row id -> seat -> class."""
    out = {}
    for line in open(AUDIT, encoding="utf-8"):
        if not re.match(r"\| [A-Z]\d", line):
            continue
        cols = [c.strip() for c in line.split("|")]
        ids = re.findall(r"[A-Z]\d\d", cols[1])
        cl = []
        for c in cols[2:9]:
            c = c.replace("\\*", "").replace("*", "")
            cl.append("PIN" if c.startswith("PIN") else c)
        if ids:
            out[ids[0]] = dict(zip(pg.SEATS, cl))
    return out


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--ledger", default=LEDGER)
    ap.add_argument("--owed", default=OWED)
    ap.add_argument("--summary", default=SUMMARY)
    ap.add_argument("--threshold", type=float)
    ap.add_argument("--table", action="store_true")
    ap.add_argument("--audit-diff", action="store_true")
    ap.add_argument("--write-owed", action="store_true")
    a = ap.parse_args(argv)
    summary = json.load(open(a.summary))
    lthr, verdicts = read_ledger(a.ledger)
    thr = lthr if a.threshold is None else a.threshold
    cells, cells9 = classify_all(summary, thr), classify_all(summary, 0.9)
    unv, unv9 = unverdicted(cells, verdicts), unverdicted(cells9, verdicts)
    if a.write_owed:
        with open(a.owed, "w", newline="\n", encoding="utf-8") as f:
            f.write("# Pinned-law cells that read PIN, NR or UZ with no ledger verdict (first-day state, threshold %g).\n"
                    "# Remove a line when the ledger row gets a verdict covering the seat or the reading changes; the test fails\n"
                    "# on a line that no longer reproduces and on any unlisted cell. UZ = a counter reads 0 without its witness.\n"
                    "# OWED row seat class | reading\n" % thr)
            for key in sorted(unv):
                f.write(owed_line(key, unv[key]) + "\n")
        print("wrote %d owed lines to %s" % (len(unv), a.owed))
        return 0
    if a.table:
        print("row  " + "  ".join("%-4s" % s.replace(" ", "") for s in pg.SEATS))
        for row in pg.G:
            print("%-4s " % row + "  ".join("%-4s" % cells[(row, s)][0] for s in pg.SEATS))
    if a.audit_diff:
        aud = audit_cells()
        mism, agree = [], 0
        for (row, seat), (cls, reading) in sorted(cells9.items()):
            ac = aud.get(row, {}).get(seat)
            if ac == cls:
                agree += 1
            else:
                mism.append("%s %s: audit %s, recomputed %s (%s)" % (row, seat, ac, cls, reading))
        print("audit agreement at 0.9: %d of %d gauged cells" % (agree, len(cells9)))
        print("\n".join(mism))
    for arch, d in summary.get("replay", {}).get("F01", {}).items():
        print("F01 replay %s: %d records, %d mismatches, unclamped law output takes %d values, cap at ceiling N*pool on %.3f of %d pooled records"
              % (os.path.basename(arch), d["records"], d["mismatches"], d["unclamped_distinct"], d["at_ceiling"] / d["pooled"], d["pooled"]))
    print("threshold %g: %s; unverdicted PIN/NR/UZ cells %d" % (thr, counts(cells), len(unv)))
    print("threshold 0.9: %s; unverdicted PIN/NR/UZ cells %d" % (counts(cells9), len(unv9)))
    fails = run(a.ledger, a.owed, a.summary, a.threshold, check_fresh=(a.summary == SUMMARY))
    for f in fails[:40]:
        print("FAIL " + f)
    if len(fails) > 40:
        print("... %d more" % (len(fails) - 40))
    print("pinned-law check: %s" % ("RED, %d failure(s)" % len(fails) if fails else "green"))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
