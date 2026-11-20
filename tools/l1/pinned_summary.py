#!/usr/bin/env python3
"""Parsed summary of the committed L1 logs for the pinned-law check (S3 part 1).

  pinned_summary.py --write   rebuild tools/l1/pinned_summary.json from the archives in docs/l1-raw
  pinned_summary.py --check   rebuild and compare with the committed file (exit 1 on any difference)

The summary holds integer counts and per-row end values, not classes, so every threshold is applied
later from one file. It is deterministic (sorted keys, LF) and records each archive's sha256, so CI can
tell a changed log from a stale summary without unpacking 384 MB. Run it after adding a battery."""
import collections, hashlib, io, json, os, re, sys, tarfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "docs", "derivations", "liveness-audit"))
sys.path.insert(0, os.path.join(ROOT, "docs", "derivations", "store-law"))
import pinned_gauges as pg  # noqa: E402
from plib import parse, ANSI  # noqa: E402
import caplog_replay as cr  # noqa: E402

SUMMARY = os.path.join(HERE, "pinned_summary.json")
LATCH = {"w_us": ("w_us_min", "w_us_max"), "w_us_p": ("w_us_p05", "w_us_p95")}
GATES_RE = re.compile(r"(RWM_[A-Z0-9_]+)=(\S+)")


def spec_fingerprint():
    h = hashlib.sha256()
    for f in (os.path.join(HERE, "pinned_gauges.py"), os.path.join(ROOT, "docs/derivations/liveness-audit/plib.py"),
              os.path.join(ROOT, "docs/derivations/store-law/caplog_replay.py")):
        h.update(open(f, "rb").read().replace(b"\r\n", b"\n"))
    return h.hexdigest()


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def lines_of(fobj):
    for raw in io.TextIOWrapper(fobj, encoding="utf-8", errors="replace"):
        yield ANSI.sub("", raw.rstrip("\n"))


def tick_gauges():
    return {r: g for r, g in pg.G.items() if g["kind"] in ("mode", "bind", "zero")}


def scan_sender(fobj, acc, ends, gates):
    """One sender log: pooled tick counts into acc, last lines of the end tags, the [GATES] echo."""
    tg = tick_gauges()
    last = {}
    for line in lines_of(fobj):
        if not line.startswith("["):
            if "[GATES]" in line and not gates:
                gates.update(dict(GATES_RE.findall(line)))
            continue
        tag = line.split("]")[0][1:]
        if tag in ("DIAG", "SPAN"):
            p = parse(line)
            if p.get("t", 0) < 2 or (tag == "DIAG" and p.get("src", 0) <= 0):
                continue
            for row, g in tg.items():
                if g["tag"] != tag:
                    continue
                a = acc[row]
                a["n"] += 1
                if g["kind"] == "mode":
                    v = p.get(g["field"])
                    if v is not None:
                        a["present"] += 1
                        a["values"][v] += 1
                    if g.get("w"):
                        a["w_n"] += bool(pg.PRED[g["w"]](p))
                elif g["kind"] == "bind":
                    a["k"] += bool(pg.PRED[g["pred"]](p))
                    if g["pre"]:
                        a["pre_n"] += bool(pg.PRED[g["pre"]](p))
                else:
                    v = p.get(g["field"])
                    if v is not None:
                        a["present"] += 1
                        a["k0"] += v == 0
                    a["w_n"] += bool(pg.PRED[g["w"]](p))
                    if g["pre"]:
                        a["pre_n"] += bool(pg.PRED[g["pre"]](p))
        m = re.search(r"site=(\w+)", line)
        last[(tag, m.group(1) if m else None)] = line
    for row, g in pg.G.items():
        if g["kind"] == "end" and g["side"] == "c":
            ends[row].append(end_value(g, last))
    return last


def end_value(g, last):
    line = last.get((g["tag"], g["site"]))
    if line is None:
        return None
    p = parse(line)
    if g["how"] == "latched":
        lo, hi = LATCH[g["field"]]
        return None if lo not in p or hi not in p else int(p[lo] == p[hi])
    v = p.get(g["field"])
    if v is None:
        return None
    return int(v == g["off"]) if g["how"] == "off" else round(v, 6)


def scan_receiver(fobj, ends):
    last = {}
    for line in lines_of(fobj):
        if line.startswith("["):
            tag = line.split("]")[0][1:]
            m = re.search(r"site=(\w+)", line)
            last[(tag, m.group(1) if m else None)] = line
    for row, g in pg.G.items():
        if g["kind"] == "end" and g["side"] == "s":
            ends[row].append(end_value(g, last))


REPLAY_ARCHIVES = ["docs/l1-raw/s60/rep60a-diag-ld.tgz", "docs/l1-raw/s60/rep60b-diag-ld.tgz"]


def replay_f01(path):
    """F01 (pooled store cap): the committed [CAPLOG] inputs replayed through the existing port (store-law/caplog_replay.py).
    Counts: records, mismatches against the logged cap, pooled-branch records, how many sit at the ceiling N*pool,
    and how many distinct values the UNCLAMPED law output and the logged cap take."""
    c = collections.Counter()
    unclamped, caps = set(), set()
    with tarfile.open(path) as tf:
        for mem in tf:
            if not (mem.isfile() and mem.name.endswith("-c.log")):
                continue
            lines = list(lines_of(tf.extractfile(mem)))
            o = cr.replay_log(lines)
            c["files"] += 1
            c["records"] += o["records"]
            c["replayed"] += o["replayed"]
            c["mismatches"] += len(o["mismatches"])
            for ln in lines:
                r = cr.parse_line(ln)
                if r and r["br"] == "pooled" and r["on"] and r["psum"] > 0:
                    c["pooled"] += 1
                    c["at_ceiling"] += r["cap"] == max(r["n"] * r["pool"], r["floor"])
                    c["at_floor"] += r["cap"] == r["floor"]
                    unclamped.add(round((1 + r["q"] if r["dc"] else r["gain"]) * (1 if r["sum"] else r["n"]) * r["psum"], 6))
                    caps.add(r["cap"])
    d = dict(c)
    d["unclamped_distinct"], d["cap_distinct"] = len(unclamped), len(caps)
    return d


def build():
    out = {"spec": spec_fingerprint(), "archives": {}, "seats": {}, "replay": {"F01": {}}}
    for arch in REPLAY_ARCHIVES:
        out["archives"][arch] = sha256(os.path.join(ROOT, arch))
        out["replay"]["F01"][arch] = replay_f01(os.path.join(ROOT, arch))
    for seat in pg.SEATS:
        arch, marker, arm = pg.ARCHIVES[seat]
        path = os.path.join(ROOT, arch)
        out["archives"][arch] = sha256(path)
        acc = collections.defaultdict(lambda: dict(n=0, k=0, k0=0, pre_n=0, w_n=0, present=0, values=collections.Counter()))
        ends = collections.defaultdict(list)
        gates = {}
        rows = 0
        rx = re.compile(r"@def-%s-s\d+-r\d+-a\d+-(c|s)\.log$" % arm)
        with tarfile.open(path) as tf:
            for mem in tf:
                if not mem.isfile() or marker not in mem.name:
                    continue
                m = rx.search(mem.name)
                if not m:
                    continue
                f = tf.extractfile(mem)
                if m.group(1) == "c":
                    rows += 1
                    g1 = {}
                    scan_sender(f, acc, ends, g1)
                    for k in [g["key"] for g in pg.G.values() if g["kind"] == "gate"]:
                        ends["gate:" + k].append(g1.get(k))
                else:
                    scan_receiver(f, ends)
        res = {"rows": rows, "gauges": {}}
        for row, g in pg.G.items():
            if g["kind"] == "gate":
                res["gauges"][row] = {"vals": ends["gate:" + g["key"]]}
            elif g["kind"] == "end":
                res["gauges"][row] = {"vals": ends[row]}
            else:
                a = acc[row]
                d = {"n": a["n"], "present": a["present"]}
                if g["kind"] == "mode":
                    mv, mn = a["values"].most_common(1)[0] if a["values"] else (None, 0)
                    d.update(mode=mv, mode_n=mn)
                    if g.get("w"):
                        d["w_n"] = a["w_n"]
                elif g["kind"] == "bind":
                    d["k"] = a["k"]
                    if g["pre"]:
                        d["pre_n"] = a["pre_n"]
                else:
                    d.update(k0=a["k0"], w_n=a["w_n"])
                    if g["pre"]:
                        d["pre_n"] = a["pre_n"]
                res["gauges"][row] = d
        out["seats"][seat] = res
    return out


def dump(s):
    return json.dumps(s, indent=1, sort_keys=True) + "\n"


def main(argv):
    if len(argv) != 2 or argv[1] not in ("--write", "--check"):
        print(__doc__)
        return 2
    new = dump(build())
    if argv[1] == "--write":
        with open(SUMMARY, "w", newline="\n") as f:
            f.write(new)
        print("wrote", SUMMARY)
        return 0
    old = open(SUMMARY, newline="").read().replace("\r\n", "\n")
    if old != new:
        print("pinned_summary.json does not match the logs: run pinned_summary.py --write")
        return 1
    print("pinned_summary.json matches the logs")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
