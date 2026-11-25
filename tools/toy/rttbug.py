"""Report-only probe of the RTT-sample bug: apply_floor raises rhi without an arrival, on_report samples RTT on rhi.
CSim counts samples whose seq had not arrived when the report was made (no behaviour change); fix=True skips them.
Shipped toy2.py is untouched.  python rttbug.py count|cmp ..."""
import json, math, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy2, batch3b, batch3d

OUTD = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "rttbug-out")


class CSim(toy2.Sim):
    fix = False

    def __init__(self, cfg, fix=False):
        super().__init__(cfg)
        self.fix = fix
        self._suppress = False
        self.cnt = dict(rfc_total=0, rfc_bad=0, old_total=0, old_bad=0, p_total=0, p_bad=0, rfc_bad_noarr_ever=0)

    def _arrived(self, s, t):
        return bool(self.got[s]) and self.arr[s] <= t

    def on_report(self, now, rep):
        t_rep = rep[0]
        self._suppress = False
        c = self.cfg
        if (rep[1], rep[2]) > (self.A_known, self.G_known) and rep[1] > self.A_known and c.rtt_est == "old":
            self.cnt["old_total"] += 1
            if not self._arrived(rep[1] - 1, t_rep):
                self.cnt["old_bad"] += 1
        if rep[0] > self.last_gen:
            if self.paths is not None:
                for p in range(self.npath):
                    r = rep[8][p]
                    if r > self.rhi_pk[p] and self.copies[r] == 0:
                        self.cnt["p_total"] += 1
                        if not self._arrived(r, t_rep):
                            self.cnt["p_bad"] += 1
            if rep[3] > self.rhi_known and self.paths is None and c.rtt_est == "rfc" and self.copies[rep[3]] == 0:
                self.cnt["rfc_total"] += 1
                if not self._arrived(rep[3], t_rep):
                    self.cnt["rfc_bad"] += 1
                    if not self.got[rep[3]]:
                        self.cnt["rfc_bad_noarr_ever"] += 1
                    if self.fix:
                        self._suppress = True
        super().on_report(now, rep)
        self._suppress = False

    def rtt_sample(self, r):
        if self._suppress:
            return
        super().rtt_sample(r)


def run(cfg, fix=False):
    sim = CSim(cfg, fix)
    r = sim.run()
    r = batch3b.clean(r)
    r["_cnt"] = sim.cnt
    r["_o_hat_end"] = (sim.o_fwd() if sim.srtt is not None else None)
    return r


def main():
    os.makedirs(OUTD, exist_ok=True)
    batch3d.build()
    mode = sys.argv[1]
    if mode == "count":
        path = os.path.join(OUTD, "count.json")
        res = json.load(open(path)) if os.path.exists(path) else {}
        names = sorted(batch3d.JOBS, key=lambda n: -batch3d.weight(n) * -1)   # short first
        for n in names:
            if n in res:
                continue
            if "long" in n.split(".")[0] and (".NT-" in n or ".NH-" in n):
                continue            # long-cell nudge variants skipped (7 min each); the unnudged arms are counted
            t = time.time()
            r = run(batch3d.JOBS[n][1](0))
            res[n] = dict(cnt=r["_cnt"], srtt=r.get("srtt"), fin=r.get("finished"), wall=time.time() - t)
            json.dump(res, open(path + ".tmp", "w")); os.replace(path + ".tmp", path)
            print("%s %s %.1fs" % (n, r["_cnt"], time.time() - t), flush=True)
    elif mode == "cmp":
        names = sys.argv[2].split(","); seeds = range(int(sys.argv[3]))
        path = os.path.join(OUTD, "cmp.json")
        res = json.load(open(path)) if os.path.exists(path) else {}
        KEYS = ["share_any", "share_T", "share_by_0p4", "share_by_2", "recovery_time", "recovery_status", "srtt", "dh_p50", "finished"]
        for n in names:
            for s in seeds:
                k = "%s|%d" % (n, s)
                if k in res:
                    continue
                t = time.time()
                a = run(batch3d.JOBS[n][1](s), False)
                b = run(batch3d.JOBS[n][1](s), True)
                res[k] = dict(cnt=a["_cnt"], cntfix=b["_cnt"],
                              base={x: a.get(x) for x in KEYS + []} | {"o_hat_end": a["_o_hat_end"]},
                              fix={x: b.get(x) for x in KEYS} | {"o_hat_end": b["_o_hat_end"]})
                json.dump(res, open(path + ".tmp", "w")); os.replace(path + ".tmp", path)
                print(k, a["_cnt"]["rfc_bad"], "/", a["_cnt"]["rfc_total"], "%.1fs" % (time.time() - t), flush=True)
    elif mode == "ident":   # CSim count mode vs plain Sim, on given jobs
        for n in sys.argv[2].split(","):
            cfg = batch3d.JOBS[n][1](0)
            a = batch3b.clean(toy2.Sim(cfg).run()); b = run(batch3d.JOBS[n][1](0))
            b.pop("_cnt"); b.pop("_o_hat_end")
            diff = [k for k in a if a[k] != b.get(k) and not (isinstance(a[k], float) and math.isnan(a[k]))]
            print(n, "identical" if not diff else diff[:5], flush=True)


main()
