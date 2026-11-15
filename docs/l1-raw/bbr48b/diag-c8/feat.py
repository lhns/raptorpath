# POST HOC, not registered. Extract per-run features from s48b full logs -> feat.json
import json, os, re, sys
D = sys.argv[1]; L = os.path.join(D, "full/diag-ld")
NUM = re.compile(r"^-?[0-9.]+$")
def kv(line):
    out = {}; pre = ""
    for tok in line.replace("]", " ").split():
        m = re.match(r"^p(\d+):(.*)$", tok)
        if m: pre = "p" + m.group(1) + "."; tok = m.group(2)
        if "[" in tok: tok = tok.split("[")[-1]
        if "=" not in tok: continue
        k, v = tok.split("=", 1)
        v = v.rstrip("%").replace("ms", "").replace("Mbit", "")
        if "/" in v: v = v.split("/")[0]
        if NUM.match(v):
            try: out[pre + k] = float(v)
            except ValueError: pass
    return out
def parse(path, side):
    f = {}; rd = []; park = 0; unpark = 0; busy = 0.0
    for ln in open(path, encoding="utf-8", errors="replace"):
        s = ln.strip()
        if side == "s" and "obj=0" in s: continue
        if s.startswith("[IOWN] phase=xfer"):
            p = re.search(r"path=(\d+)", s).group(1)
            for k, v in kv(s).items():
                if k not in ("path", "run", "obj"): f[f"iown{p}.{k}"] = v
        elif s.startswith("[THR] rt phase=xfer"):
            d = kv(s); park += d.get("park", 0); unpark += d.get("unpark", 0)
        elif s.startswith("[THR] sum phase=xfer"):
            d = kv(s); f["thr.busy_s"] = d["busy_s"]; f["thr.cpu_s"] = d["cpu_s"]; f["thr.wall_s"] = d["wall_s"]
        elif s.startswith("[RDIAG]"):
            rd.append(kv(s))
        elif s.startswith("[LAG] phase=xfer"):
            d = kv(s); f["lag.p50"] = d.get("p50_us"); f["lag.p99"] = d.get("p99_us")
        elif s.startswith("    CPU:"):
            m = re.search(r"CPUSRV=([0-9.]+)s CPUCLI=([0-9.]+)s", s); f["cpu_srv"] = float(m.group(1)); f["cpu_cli"] = float(m.group(2))
        elif s.startswith("[TRUTH]"):
            d = kv(s); lg = int(d["leg"])
            for k in ("egress_dgrams", "egress_skbs", "gso", "lost"): f[f"truth{lg}.{k}"] = d[k]
        else:
            m = re.match(r"^\[([A-Z0-9-]+)\]", s)
            if m and m.group(1) in ("RFA", "SUCC", "LAT", "LATE", "RANK", "WIDLE", "C8CONV-R", "CTLD", "HOLD", "QCLK", "RACK", "FCAUSE", "ETA", "DCAP", "SHEDH") and "t=" not in s.split()[1][:2]:
                tag = m.group(1)
                if tag == "ETA": tag += "." + (re.search(r"site=(\w+)", s).group(1))
                if tag == "HOLD":
                    mm = re.search(r"path=(\d+)", s); tag += mm.group(1) if mm else "x"
                for k, v in kv(s).items(): f[f"{tag}.{k}"] = v   # last occurrence wins (final summary)
    f["thr.park"] = park; f["thr.unpark"] = unpark
    rdx = [r for r in rd if r.get("msgs", 0) > 50]
    if rdx:
        for k in ("busy", "msgs", "rows", "max_span", "coeff_kb", "q_avg", "q_max", "prune_n"):
            vals = [r[k] for r in rdx if k in r]
            if vals: f[f"rdiag.mean_{k}"] = sum(vals) / len(vals)
        f["rdiag.n"] = len(rdx)
    return f
rows = {}
for ln in open(os.path.join(D, "run/bbr48/ld.log"), encoding="utf-8", errors="replace"):
    if ln.startswith("P38ROW "):
        r = json.loads(ln[7:]); rows[(r["cell"], r["arm"], str(r["seed"]), int(r["rep"]))] = r
out = []
for (cell, arm, seed, rep), r in sorted(rows.items()):
    stem = os.path.join(L, f"{cell}@def-{arm}-s{seed}-r{rep}-a1")
    rec = {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "seconds": r["seconds"], "mbps": r["mbps"], "bytes": r["bytes"],
           "coded_share": r.get("coded_share"), "ld_cpu_cli": r["cpu_cli"], "ld_cpu_srv": r["cpu_srv"]}
    for side, fn in (("cli", "-c.log"), ("srv", "-s.log"), ("drv", "-drv.out")):
        for k, v in parse(stem + fn, side[0]).items(): rec[f"{side}.{k}"] = v
    out.append(rec)
json.dump(out, open(os.path.join(D, "diag-c8/feat.json"), "w"), indent=0)
print(len(out), "rows", len(out[0]), "keys")
