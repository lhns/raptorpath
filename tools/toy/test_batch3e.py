"""Tests for stage 3e's runner (batch3e.py) and reader (summarize3e.py), on synthetic data and short runs (three-knobs-stage3e-build.md 8, 9 step 5).
Run from tools/toy:  python -m unittest test_batch3e"""
import json
import math
import os
import sys
import tempfile
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy2  # noqa: E402
import batch3e as B  # noqa: E402
import summarize3e as S  # noqa: E402

INF = math.inf
SEEDS = 12
REG_AMEND = {"CLw-long": 75, "RT": 55, "CL": 52, "OV": 47, "K-I2": 40, "K-L10": 40, "K-OV": 36, "OV-long": 34, "CLc": 30, "CL-long": 26,
             "CLc-v": 21, "CL-long-v": 21, "OV-long-v": 21, "Ec": 3, "E-I2": 3, "M-I2": 3}


class Cells(unittest.TestCase):
    def test_overrides_of_the_registered_cells(self):
        C = B.CELLS
        lc, k = C["CLw-long"]
        self.assertEqual((k["rate"], k["n_total"], k["dead"]), (8000.0, 160000, (0, 2.0, 3.0)))
        lc, k = C["CLs-long"]
        self.assertEqual((k["rate"], k["n_total"], k["link"], k["pause_B"]), (1000.0, 20000, dict(C=1250.0, Q=125.0), 100))
        self.assertIsNone(C["CLc"][1]["dead"])
        self.assertIsNone(C["RT"][1]["dead"])
        self.assertEqual(C["RQ"][1]["rev_q"], 0.1)
        lc, k = C["K-OV"]
        self.assertEqual((lc, k["link"], k["pause_B"], k["sweep_form"], k["rx_refresh"], k["t_limit"], k["rate"], k["owd"]),
                         ("L10", dict(C=40.0, Q=40.0), 8, "blocker", True, 120.0, 50.0, 0.05))
        self.assertIsNone(k["dead"])
        self.assertIsNone(C["Ec"][1]["dead"])
        self.assertEqual(C["CL"][1]["sweep_form"], "blocker")                      # CL@b
        self.assertTrue(C["CL"][1]["windows"])
        self.assertTrue(C["K-I2"][1]["windows"])                                   # a cell with an outage reads windows

    def test_cells_do_not_alias_the_batch_dicts(self):
        import batch3d
        B.CELLS["CL"][1]["rate"] = 1.0
        try:
            self.assertEqual(batch3d.CELLS["CL@b"][1]["rate"], 10000.0)
        finally:
            B.CELLS["CL"][1]["rate"] = 10000.0

    def test_builders(self):
        c = B.new_cfg("CL", 3, 0.4, 0.0, 50.0, dict(r="law", edge="worth"))
        self.assertEqual((c.sender, c.recv, c.T, c.Hk, c.delta, c.seed, c.rho, c.repair, c.rtt_est, c.owd_est, c.floor_law),
                         ("S5", "RH", 0.4, 0.0, 50.0, 3, 1.0, dict(r="law", edge="worth"), "rfc", "fwd", "protect"))
        a = B.adr_cfg("CL", 1, 0.99, 50.0, dict(r="law", edge="worth"))
        self.assertEqual((a.sender, a.recv, a.rho, a.repair["edge"], a.rtt_est), ("S1c", "R0e", 0.99, "none", "rfc"))
        r = B.rt_cfg("CL", 1, 5.0, dict(r="est", edge="worth"))
        self.assertEqual((r.sender, r.recv, r.budget, r.floor_send, r.evict_window, r.r0e_max, r.pause_B, r.d_arm, r.delta, r.repair),
                         ("S1", "R0e", "resid", False, 200, 500, 10 ** 9, "EWMA", 5.0, dict(r="est", edge="evict")))
        b = B.bulk_cfg("CL", 1, 50.0, None)
        self.assertEqual((b.sender, b.recv, b.rho, b.repair), ("S1c", "R1", 1.0, None))
        self.assertEqual(B.new_cfg("RT", 0, 0.12, 0.0, 50.0, None).tau_extra, (0.12,))     # RT reads the 0.12 s on-time share

    def test_the_t_inf_hinf_arm_equals_todays_bulk_under_repair_in_a_call_cell(self):
        rp = dict(r="law", edge="worth")
        for seed in range(2):
            a = B.new_cfg("RT", seed, INF, INF, 50.0, rp, n_total=500)
            b = B.bulk_cfg("RT", seed, 50.0, dict(rp, edge="none"), n_total=500)
            a.want_trace = b.want_trace = True
            ra, rb = toy2.run(a), toy2.run(b)
            self.assertEqual(B.id3e(ra, rb), [])
            self.assertEqual(ra["_trace"], rb["_trace"])
            self.assertGreater(ra["gauges"]["rep_sent"], 0)

    def test_old_arms_carry_the_rfc_estimator_because_the_identity_needs_it(self):
        """Witness for build 8.2's departure: with the old RTT estimator the Bulk arm of a call cell differs from S5 at T = inf under repair
        (srtt feeds r), with the RFC 6298 estimator it does not."""
        rp = dict(r="law", edge="worth")
        a = B.new_cfg("RT", 0, INF, INF, 50.0, rp, n_total=500)
        lit = B.bulk_cfg("RT", 0, 50.0, rp, n_total=500, rtt_est="old")
        rfc = B.bulk_cfg("RT", 0, 50.0, rp, n_total=500)
        for c in (a, lit, rfc):
            c.want_trace = True
        ra, rl, rr = toy2.run(a), toy2.run(lit), toy2.run(rfc)
        self.assertNotEqual(B.id3e(ra, rl), [])
        self.assertEqual(B.id3e(ra, rr), [])
        self.assertEqual(ra["_trace"], rr["_trace"])

    def test_a_delta_005_row_equals_its_off_row(self):
        for mode in (dict(r="law", edge="worth"), dict(r="est", edge="worth")):
            a = B.new_cfg("RT", 0, 0.12, 0.0, 0.005, None, n_total=500)
            b = B.new_cfg("RT", 0, 0.12, 0.0, 0.005, mode, n_total=500)
            a.want_trace = b.want_trace = True
            ra, rb = toy2.run(a), toy2.run(b)
            self.assertEqual(B.id3e(ra, rb), [])
            self.assertEqual(ra["_trace"], rb["_trace"])


class Parts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        B.build()

    def test_registered_counts_before_the_dedupe(self):
        P = B.PARTS
        self.assertEqual(len(P["MAIN"]), 59)
        self.assertEqual(len(P["DECIDE"]), 54)
        self.assertEqual(len(P["CROSS"]), 2)
        self.assertEqual(len(P["AA"]), 6 * 4)                                      # per cell: A and A' (phase variant 1) at T = inf and at the primary lifetime
        self.assertEqual(len(P["AA2"]), 8 * 4 + 3 * 2)                             # the file cells' primary lifetime is T = inf: one pair
        self.assertEqual(len(P["ROBUST"]), 32)
        self.assertEqual(len(P["SMOOTH"]), 272)
        am = [j for j in P["AMEND"] if j.nseeds == SEEDS]
        self.assertEqual(len(am), 507)
        self.assertEqual(len([j for j in P["AMEND"] if j.nseeds == 3]), 54)
        by = {}
        for j in am:
            by[j.cell] = by.get(j.cell, 0) + 1
        self.assertEqual(by, REG_AMEND)

    def test_main_arms_per_cell(self):
        by = {}
        for j in B.PARTS["MAIN"]:
            by.setdefault(j.cell, []).append(j)
        self.assertEqual({c: len(v) for c, v in by.items()}, {"CL": 5 + 1, "CL-long": 11 + 1, "OV": 9 + 1, "CLw-long": 10 + 1, "RT": 9 + 1, "CLs-long": 9 + 1})
        fixed = {(j.cell, j.arm): j.builder(0).repair["r"] for j in B.PARTS["MAIN"] if isinstance(j.builder(0).repair, dict) and isinstance(j.builder(0).repair["r"], float)}
        self.assertEqual(fixed[("CL", "RS")], 0.107)
        self.assertEqual(fixed[("OV", "RS")], 0.0998)
        self.assertEqual(fixed[("CLs-long", "RS")], 0.2295)
        self.assertEqual(fixed[("RT", "RF1")], 0.112)
        self.assertEqual(fixed[("RT", "RF3")], 0.36)
        self.assertEqual(fixed[("CL-long", "R0")], 0.0)
        self.assertEqual(fixed[("CLw-long", "RS-inf")], 0.116)
        arms = {(j.cell, j.arm): j.builder(0) for j in B.PARTS["MAIN"]}
        self.assertEqual(arms[("RT", "S5")].T, 0.12)
        self.assertEqual(arms[("RT", "RL-T0p4")].T, 0.4)
        self.assertEqual(arms[("OV", "RL-age")].repair["edge"], "age")
        self.assertEqual(arms[("OV", "RL-floor")].repair["edge"], "floor")
        self.assertEqual(arms[("OV", "RL-Hinf")].Hk, INF)
        self.assertEqual(arms[("CL-long", "S5-inf")].T, INF)
        self.assertEqual(arms[("CL-long", "AA")].variant, 1)

    def test_decide_is_cl_long_off_and_rl(self):
        self.assertEqual({j.cell for j in B.PARTS["DECIDE"]}, {"CL-long"})
        self.assertEqual(sorted({j.mode for j in B.PARTS["DECIDE"]}), ["OFF", "RL"])
        self.assertEqual(len([j for j in B.PARTS["DECIDE"] if j.key[0] == "lt"]), 36)
        self.assertEqual(len([j for j in B.PARTS["DECIDE"] if j.key[0] in ("adr", "rt", "bulk")]), 18)

    def test_cross_is_the_two_proof_stage_1_cross_jobs(self):
        c = [j.builder(0) for j in B.PARTS["CROSS"]]
        self.assertEqual(sorted((x.sender, x.recv) for x in c), [("S1", "R0e"), ("S1c", "RH")])
        self.assertTrue(all(x.repair is None and x.rho == 0.99 for x in c))

    def test_every_row_has_one_seed_set_and_the_identity_rows_three(self):
        for j in B.PARTS["AMEND"]:
            if j.nseeds == 3:
                cf = j.builder(0)
                self.assertEqual(cf.delta, 0.005)
                self.assertIsNotNone(cf.repair)
        for p, js in B.PARTS.items():
            for j in js:
                self.assertIn(j.nseeds, (3, SEEDS))

    def test_fingerprint_ignores_seed_and_trace_and_dedupes_across_parts(self):
        a = B.new_cfg("CL", 0, 0.4, 0.0, 50.0, None)
        b = B.new_cfg("CL", 7, 0.4, 0.0, 50.0, None)
        b.want_trace = True
        self.assertEqual(B.fingerprint(a), B.fingerprint(b))
        self.assertNotEqual(B.fingerprint(a), B.fingerprint(B.new_cfg("CL", 0, 0.6, 0.0, 50.0, None)))
        self.assertNotEqual(B.fingerprint(a), B.fingerprint(B.new_cfg("CL", 0, 0.4, 0.0, 50.0, dict(r="law", edge="worth"))))
        self.assertNotEqual(B.fingerprint(B.new_cfg("CL", 0, INF, 0.0, 50.0, None)), B.fingerprint(B.new_cfg("CL", 0, 1e9, 0.0, 50.0, None)))
        cnt = B.counts()
        total_cfgs = sum(len(v) for v in B.PARTS.values())
        self.assertLess(sum(c["new"] for c in cnt.values()), total_cfgs)           # the registered sweeps share points
        seen = set()
        for p in B.ORDER:                                                          # a fingerprint is attributed to the first part that asks for it
            for fp in B.attributed(p):
                self.assertNotIn(fp, seen)
                seen.add(fp)

    def test_job_names_say_what_the_row_does(self):
        for p in ("MAIN", "DECIDE"):
            self.assertTrue(all("(" in j.name for j in B.PARTS[p]))
        self.assertFalse(any("toy" in j.name.lower() for ps in B.PARTS.values() for j in ps))


class Runner(unittest.TestCase):
    def setUp(self):
        self.td = tempfile.TemporaryDirectory()
        self.old = B.OUT
        B.OUT = self.td.name

    def tearDown(self):
        B.OUT = self.old
        self.td.cleanup()

    def mark(self, *names):
        os.makedirs(B.OUT, exist_ok=True)
        for n in names:
            open(os.path.join(B.OUT, n), "w").close()

    def test_a_part_refuses_without_its_markers(self):
        miss = B.missing_markers("DECIDE")
        self.assertEqual(miss, ["PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread", "PASS-aa", "DONE-AA", "DONE-AA2"])
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread")
        self.assertEqual(B.missing_markers("AA"), [])
        self.assertEqual(B.missing_markers("DECIDE"), ["PASS-aa", "DONE-AA", "DONE-AA2"])
        self.mark("PASS-aa", "DONE-AA", "DONE-AA2")
        self.assertEqual(B.missing_markers("DECIDE"), [])
        self.assertEqual(B.missing_markers("MAIN"), ["DONE-DECIDE"])
        self.mark("DONE-DECIDE")
        self.assertEqual(B.missing_markers("MAIN"), [])
        self.assertEqual(B.missing_markers("CROSS"), [])
        self.assertEqual(B.missing_markers("AMEND"), ["DONE-MAIN"])
        self.assertEqual(B.missing_markers("ROBUST"), ["DONE-MAIN", "DONE-AMEND"])
        self.assertEqual(B.missing_markers("SMOOTH"), ["DONE-MAIN", "DONE-AMEND", "DONE-ROBUST"])
        self.assertEqual(B.missing_markers("DOUBLE:DECIDE"), [])
        self.assertEqual(B.missing_markers("DOUBLE:MAIN"), ["DONE-MAIN"])

    def test_stop_refuses_everything(self):
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread")
        self.assertIsNone(B.refusal("AA"))
        self.mark("STOP")
        self.assertIn("STOP", B.refusal("AA"))
        self.assertIn("STOP", B.refusal("CROSS"))

    def test_long_cells_run_with_one_worker(self):
        self.assertIn("worker", B.refusal_workers(["OV-long", "CL"], 2))
        self.assertIsNone(B.refusal_workers(["OV-long", "CL"], 1))
        self.assertIsNone(B.refusal_workers(["CL", "OV", "RT"], 2))
        self.assertIn("worker", B.refusal_workers(["CLw-long"], 2))
        self.assertIn("worker", B.refusal_workers(["RQ"], 2))
        self.assertIn("workers", B.refusal_workers(["CL"], 3))

    def test_gate_failures(self):
        ok = dict(finished=True, gauges=dict(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0))
        self.assertEqual(B.gate_failures(ok, "worth"), [])
        bad = json.loads(json.dumps(ok))
        bad["finished"] = False
        self.assertEqual(B.gate_failures(bad, "worth"), ["finished"])
        for k in ("l1_viol", "l1_viol_rep", "commit_viol"):
            b = json.loads(json.dumps(ok))
            b["gauges"][k] = 1
            self.assertEqual(B.gate_failures(b, "worth"), [k])
        b = json.loads(json.dumps(ok))
        b["gauges"]["worth_viol_rep"] = 3
        self.assertEqual(B.gate_failures(b, "worth"), ["worth_viol_rep"])
        self.assertEqual(B.gate_failures(b, "floor"), ["worth_viol_rep"])
        self.assertEqual(B.gate_failures(b, "age"), [])                            # the age edge is the positive control of the worth gauge
        self.assertEqual(B.gate_failures(b, None), [])
        nf = dict(finished=None, gauges=ok["gauges"])
        self.assertEqual(B.gate_failures(nf, "worth"), [])                         # a stream cell has no finished flag

    def test_id3e_excludes_rep3e_and_the_stage_3c_exclusions(self):
        a = dict(share_any=0.9, sender="S5", gauges={k: 1 for k in toy2_id_gauges()}, rep3e=dict(r_mean=0.1))
        b = dict(share_any=0.9, sender="S1c", gauges={k: 1 for k in toy2_id_gauges()}, rep3e=dict(r_mean=0.0))
        self.assertEqual(B.id3e(a, b), [])
        b["share_any"] = 0.8
        self.assertEqual(B.id3e(a, b), ["share_any"])
        b["share_any"] = 0.9
        b["gauges"]["dup"] = 2
        self.assertEqual(B.id3e(a, b), [("g", "dup")])

    def test_run_one_stores_a_trace_digest_and_a_json_safe_result(self):
        cfg = B.new_cfg("RT", 0, 0.12, 0.0, 50.0, dict(r="law", edge="worth"), n_total=300, tau_extra=(0.12,))
        r = B.run_one(cfg, trace=True)
        self.assertEqual(len(r["trace_sha"]), 64)
        self.assertGreater(r["trace_n"], 300)
        back = json.loads(json.dumps(r))
        self.assertIn("0.12", back["rep3e"]["share_by"])
        self.assertNotIn("_trace", r)

    def test_a_run_writes_its_file_once_and_skips_it_after(self):
        os.makedirs(os.path.join(B.OUT, "runs"), exist_ok=True)
        cfg = B.new_cfg("RT", 0, 0.12, 0.0, 50.0, None, n_total=300)
        fp = B.fingerprint(cfg)
        B.store(fp, 0, dict(x=1))
        self.assertTrue(os.path.exists(B.fname(fp, 0)))
        self.assertEqual(os.path.basename(B.fname(fp, 0)), "%s-0.json" % fp)
        self.assertEqual(B.load(fp, 0), dict(x=1))
        self.assertIsNone(B.load(fp, 1))

    def test_a_failed_gate_writes_stop_with_the_job_seed_and_key(self):
        bad = dict(finished=False, gauges=dict(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0))
        fails = B.after_run("CL | lifetime T=0.4 (S5)", "abc", 3, bad, "worth", [])
        self.assertEqual(fails, ["finished"])
        txt = open(os.path.join(B.OUT, "STOP")).read()
        self.assertIn("CL | lifetime T=0.4 (S5)", txt)
        self.assertIn("seed 3", txt)
        self.assertIn("finished", txt)

    def test_the_identity_pair_check_writes_stop_on_a_difference(self):
        a = dict(share_any=0.9, sender="S5", gauges={k: 1 for k in toy2_id_gauges()}, trace_sha="x", finished=True)
        b = json.loads(json.dumps(a))
        a["gauges"].update(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0)
        b["gauges"].update(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0)
        os.makedirs(os.path.join(B.OUT, "runs"), exist_ok=True)
        B.store("fa", 1, a)
        B.store("fb", 1, b)
        self.assertEqual(B.pair_failures("fa", 1, a, ["fb"]), [])
        b["trace_sha"] = "y"
        B.store("fb", 1, b)
        self.assertEqual(B.pair_failures("fa", 1, a, ["fb"]), ["trace differs from fb"])
        b["trace_sha"] = "x"
        b["share_any"] = 0.5
        B.store("fb", 1, b)
        self.assertEqual(B.pair_failures("fa", 1, a, ["fb"]), ["id3e differs from fb: ['share_any']"])
        self.assertEqual(B.pair_failures("fa", 2, a, ["fb"]), [])                  # the partner has not run at this seed yet


    def tiny(self, jobs):
        def build():
            B.PARTS.clear()
            for p in B.ORDER:
                B.PARTS[p] = []
            for name, T, mode, kw in jobs:
                B.add("AA", name, "RT", ("aa", 0), mode, lambda s, T=T, mode=mode, kw=kw: B.new_cfg("RT", s, T, 0.0, 50.0, B.rp(mode), n_total=300, **kw), nseeds=2)
            B.register()
        old = B.build
        B.build = build
        self.addCleanup(setattr, B, "build", old)

    def test_main_runs_a_tiny_part_resumes_and_writes_done(self):
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread")
        self.tiny([("tiny OFF (S5)", 0.12, "OFF", {}), ("tiny RL (S5)", 0.12, "RL", {})])
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 0)
        self.assertTrue(os.path.exists(os.path.join(B.OUT, "DONE-AA")))
        self.assertEqual(len(os.listdir(os.path.join(B.OUT, "runs"))), 4)
        with open(os.path.join(B.OUT, "AA.json")) as f:
            self.assertEqual(sorted(json.load(f)), ["tiny OFF (S5)", "tiny RL (S5)"])
        mt = os.path.getmtime(os.path.join(B.OUT, "runs", sorted(os.listdir(os.path.join(B.OUT, "runs")))[0]))
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 0)                   # nothing to do: the files are skipped
        self.assertEqual(mt, os.path.getmtime(os.path.join(B.OUT, "runs", sorted(os.listdir(os.path.join(B.OUT, "runs")))[0])))

    def test_main_stops_on_a_gate_failure_and_then_refuses(self):
        self.mark("PASS-ident", "PASS-r0", "PASS-rttbug", "PASS-rate", "PASS-simread")
        self.tiny([("tiny broken (S5)", 0.04, "RL", dict(repair_seam="no_edge"))])
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 1)
        self.assertTrue(os.path.exists(os.path.join(B.OUT, "STOP")))
        self.assertIn("l1_viol_rep", open(os.path.join(B.OUT, "STOP")).read())
        self.assertFalse(os.path.exists(os.path.join(B.OUT, "DONE-AA")))
        self.assertEqual(B.main(["--part", "AA", "--seeds", "2"]), 2)                   # nothing runs while STOP exists


class RateCheck(unittest.TestCase):
    def test_a_pass_writes_the_marker_and_a_difference_writes_stop(self):
        import check_rate3e
        with tempfile.TemporaryDirectory() as td:
            old, orig = check_rate3e.OUT, toy2.rate_law
            check_rate3e.OUT = td
            try:
                self.assertEqual(check_rate3e.main(), 0)
                self.assertTrue(os.path.exists(os.path.join(td, "PASS-rate")))
                os.remove(os.path.join(td, "PASS-rate"))
                toy2.rate_law = lambda p, srtt, rate, delta: dict(orig(p, srtt, rate, delta), r=orig(p, srtt, rate, delta)["r"] + 1e-6)
                self.assertEqual(check_rate3e.main(), 1)
                self.assertFalse(os.path.exists(os.path.join(td, "PASS-rate")))
                with open(os.path.join(td, "STOP")) as f:
                    self.assertIn("part RATE", f.read())
            finally:
                toy2.rate_law = orig
                check_rate3e.OUT = old


def toy2_id_gauges():
    import test_toy2
    return test_toy2.ID_GAUGES


# ---- the reader ---------------------------------------------------------------------------------------------------------
rng = np.random.default_rng(1)


def fake(share_any, y, rec=None, jit=1e-4, ns=SEEDS):
    """ns results of one row: share_any, the on-time share y, the recovery time."""
    out = {}
    for s in range(ns):
        out[s] = dict(share_any=(1.0 if share_any >= 1.0 else share_any + jit * rng.standard_normal()), share_by_0p4=y + jit * rng.standard_normal(),
                      recovery_time=(rec if rec is None else rec + jit * rng.standard_normal()), units=200000, finished=True,
                      gauges=dict(l1_viol=0, l1_viol_rep=0, commit_viol=0, worth_viol_rep=0))
    return out


NF = {"share_any": 0.0005, "share_by_0p4": 0.0005, "recovery_time": 0.05}


def fake_cell(new_pts, adr_pts, rt_pts=(), bulk=None, ns=SEEDS):
    """rows keyed as the reader keys them: ("lt", T, 0.0), ("adr", rho), ("rt", delta), ("bulk",)."""
    R = {}
    for T, (x, y, rec) in new_pts.items():
        R[("lt", T, 0.0)] = fake(x, y, rec, ns=ns)
    for rho, (x, y, rec) in adr_pts.items():
        R[("adr", rho)] = fake(x, y, rec, ns=ns)
    for d, (x, y, rec) in rt_pts:
        R[("rt", d)] = fake(x, y, rec, ns=ns)
    if bulk is not None:
        R[("bulk",)] = fake(*bulk, ns=ns)
    return R


class Reader(unittest.TestCase):
    NEW = {0.4: (0.90, 0.88, 0.0), 1.0: (0.96, 0.93, 2.0), 2.0: (0.99, 0.96, 4.0), INF: (1.0, 0.97, 6.0)}

    def read(self, R, **kw):
        return S.read_mode(R, NF, units=200000, ys=("share_by_0p4", "recovery_time"), nseeds=SEEDS, rec_cap=14.5, **kw)

    def test_a_better_curve_passes(self):
        adr = {0.95: (0.92, 0.80, 5.0), 0.99: (0.97, 0.85, 6.0), 1.0: (1.0, 0.86, 7.0)}
        out = self.read(fake_cell(self.NEW, adr, bulk=(1.0, 0.86, 7.0)))
        self.assertEqual(out["outcome"], "PASSES")
        self.assertTrue(any(c["rd"]["word"] == "shown better" for c in out["comps"]))
        self.assertEqual(out["undet"], set())

    def test_a_worse_curve_fails(self):
        adr = {0.95: (0.92, 0.99, 1.0), 0.99: (0.97, 0.99, 1.0), 1.0: (1.0, 0.99, 1.0)}
        out = self.read(fake_cell(self.NEW, adr))
        self.assertEqual(out["outcome"], "FAILS")

    def test_a_baseline_above_the_curve_is_outside_the_range_and_fails(self):
        adr = {0.99: (1.0, 0.5, 6.0)}
        out = self.read(fake_cell({0.4: (0.90, 0.88, 0.0), 2.0: (0.99, 0.96, 4.0)}, adr))      # the curve stops at 0.99 delivered
        self.assertEqual(out["outcome"], "FAILS")
        self.assertTrue(any(c["rd"]["word"] == "outside the swept range" for c in out["comps"]))

    def test_an_undetermined_comparison_names_the_rows_to_double(self):
        new = {0.4: (0.90, 0.60, 0.0), 2.0: (1.0, 0.60, 0.0)}
        R = fake_cell(new, {0.95: (0.95, 0.60, 0.0), 1.0: (1.0, 0.60, 0.0)})
        for s in range(SEEDS):                                                      # a wide spread around no difference, wider than the bar
            R[("adr", 0.95)][s]["share_by_0p4"] = 0.60 + np.linspace(-0.3, 0.3, SEEDS)[s]
        out = self.read(R)
        self.assertEqual(out["outcome"], "UNDETERMINED")
        self.assertIn(("adr", 0.95), out["undet"])                                  # the baseline point and the two bracketing new points
        self.assertIn(("lt", 0.4, 0.0), out["undet"])
        self.assertIn(("lt", 2.0, 0.0), out["undet"])

    def test_the_doubling_is_read_once_and_not_asked_for_again(self):
        new = {0.4: (0.90, 0.60, 0.0), 2.0: (1.0, 0.60, 0.0)}
        R = fake_cell(new, {0.95: (0.95, 0.60, 0.0), 1.0: (1.0, 0.60, 0.0)}, ns=24)
        for s in range(24):
            R[("adr", 0.95)][s]["share_by_0p4"] = 0.60 + (np.linspace(-0.3, 0.3, SEEDS)[s] if s < SEEDS else 0.0)
        once = S.read_mode(R, NF, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5)
        self.assertEqual(once["undet"], set())                                       # the 24-seed rows are on disk: read, not asked for again
        self.assertTrue(any(c["rd"].get("note") == "doubled to 24 seeds" for c in once["comps"]))
        R12 = {k: {s: v for s, v in rows.items() if s < SEEDS} for k, rows in R.items()}
        first = S.read_mode(R12, NF, units=200000, ys=("share_by_0p4",), nseeds=SEEDS, rec_cap=14.5)
        self.assertIn(("adr", 0.95), first["undet"])                                 # with 12 seeds only, it names the rows to double
        self.assertFalse(any(c["rd"].get("note") for c in first["comps"]))

    def test_reported_scope_is_not_under_the_verdict(self):
        adr = {0.95: (0.92, 0.80, 5.0), 1.0: (1.0, 0.86, 7.0)}
        R = fake_cell(self.NEW, adr, rt_pts=[(5.0, (0.95, 0.99, 0.0))])             # today's Realtime at delta 5 beats the new curve
        out = self.read(R, rt_verdict=(50.0,))
        self.assertTrue(any(c["base"] == ("rt", 5.0) and c["scope"] == "reported" for c in out["comps"]))
        self.assertEqual(out["outcome"], "PASSES")
        R2 = fake_cell(self.NEW, adr, rt_pts=[(50.0, (0.95, 0.99, 0.0))])
        self.assertEqual(self.read(R2, rt_verdict=(50.0,))["outcome"], "FAILS")      # at delta 50 it is under the verdict in RL and RE

    def test_a_censored_recovery_is_counted_not_dropped(self):
        self.assertEqual(S.rec_value(dict(recovery_time=None), 14.5), 14.5)
        self.assertEqual(S.rec_value(dict(recovery_time=2.5), 14.5), 2.5)
        self.assertEqual(S.rec_cap(dict(n_total=200000, rate=10000.0, dead=(0, 2.0, 1.5))), 14.5)
        self.assertAlmostEqual(S.rec_cap(dict(n_total=100000, rate=10000.0, dead=(0, 2.0, 1.5))), 4.5)

    def test_the_value_reader_finds_rep3e_and_gauges(self):
        r = dict(share_by_0p4=0.5, gauges=dict(rep_sent=7), rep3e=dict(share_by={"0.12": 0.9}))
        self.assertEqual(S.value(r, "share_by_0p12", 14.5), 0.9)
        self.assertEqual(S.value(r, "share_by_0p4", 14.5), 0.5)
        self.assertEqual(S.value(r, "rep_sent", 14.5), 7.0)

    def test_the_freeze_takes_the_95th_percentile_of_the_paired_differences(self):
        A = {s: dict(share_any=0.9 + 0.001 * s, share_by_0p4=0.8, recovery_time=1.0, gauges={}) for s in range(SEEDS)}
        Ap = {s: dict(share_any=0.9, share_by_0p4=0.8, recovery_time=1.0, gauges={}) for s in range(SEEDS)}
        nf = S.nf_from(A, Ap, 14.5, ("share_any", "share_by_0p4", "recovery_time"))
        self.assertAlmostEqual(nf["share_any"], float(np.percentile([0.001 * s for s in range(SEEDS)], 95)))
        self.assertEqual(nf["share_by_0p4"], 0.0)

    def test_the_stage_verdict(self):
        self.assertEqual(S.stage_verdict({"a": "PASSES", "b": "PASSES"}, better_somewhere=True), "PASSES")
        self.assertEqual(S.stage_verdict({"a": "PASSES", "b": "FAILS"}, better_somewhere=True), "FAILS")
        self.assertEqual(S.stage_verdict({"a": "PASSES", "b": "UNDETERMINED"}, better_somewhere=True), "PASSES")       # 13.3 read literally: only a FAILS blocks it
        self.assertEqual(S.stage_verdict({"a": "PASSES"}, better_somewhere=False), "UNDETERMINED")        # nothing shown better outside the identity cells


if __name__ == "__main__":
    unittest.main()
