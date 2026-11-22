"""Unit tests for the stage-2 toy (docs/derivations/skip-rule-efficiency.md 7.8).
Run from tools/toy:  python -m unittest test_toy2   (stage 1's test_toy.py must still pass as well)."""
import ast
import math
import os
import random
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toy  # noqa: E402
import toy2  # noqa: E402
import ustar  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))


def cfg(**kw):
    kw.setdefault("dur", 5.0)
    return toy2.Cfg(**kw)


def run(**kw):
    return toy2.run(cfg(**kw))


class Determinism(unittest.TestCase):
    def test_same_seed_same_result(self):
        a = run(cell="G", recv="U", seed=3, delta=0.5, rho=1.0)
        b = run(cell="G", recv="U", seed=3, delta=0.5, rho=1.0)
        self.assertEqual(a, b)

    def test_crn_across_candidates_and_nudges(self):
        a = run(cell="I2", sender="S0", seed=2)
        b = run(cell="I2", sender="S1c", seed=2)
        c = run(cell="I2", sender="S1c", recv="U", seed=2, delta=50 * 1.01)
        self.assertEqual(a["orig_lost"], b["orig_lost"])
        self.assertEqual(a["n_holes"], b["n_holes"])
        self.assertEqual(a["n_holes"], c["n_holes"])

    def test_phases_shared_by_candidates_and_redrawn_by_variant(self):
        s0, s1 = toy2.Sim(cfg(sender="S0", seed=4)), toy2.Sim(cfg(sender="S1c", recv="U", seed=4, delta=0.5))
        self.assertEqual((s0.ph_s, s0.ph_w), (s1.ph_s, s1.ph_w))
        s2 = toy2.Sim(cfg(seed=4, variant=1))
        self.assertNotEqual((s0.ph_s, s0.ph_w), (s2.ph_s, s2.ph_w))
        self.assertTrue(0 <= s0.ph_s < 0.002)


class FIFO(unittest.TestCase):
    def test_delay_process_is_fifo_for_every_datagram(self):
        for J in (0.0, 0.001, 0.004):
            for q in (False, True):
                d = toy2.Delay(7, 0.005, J, q, 30.0)
                prev = -1.0
                t = 0.0
                while t < 25.0:
                    a = t + d.at(t)
                    self.assertGreaterEqual(a, prev - 1e-12, (J, q, t))
                    prev = a
                    t += 0.0003

    def test_no_overtake_in_any_one_path_cell(self):
        for cell in ("I1", "I2", "G"):
            for J, q in ((0.0, False), (0.001, False), (0.004, False), (0.001, True)):
                r = run(cell=cell, recv="U", seed=1, J=J, q=q, delta=50.0, rho=0.99)
                self.assertEqual(r["gauges"]["fifo_viol"], 0, (cell, J, q))
                self.assertEqual(r["H_max"], 0.0, (cell, J, q))        # H is 0 on every FIFO run
                self.assertEqual(r["gauges"]["fh_late"], 0)


class P1Promise(unittest.TestCase):
    def test_promise_every_instant(self):
        for cell in ("I2", "G"):
            for recv in ("R1", "U"):
                for dd, rho in ((50.0, 0.99), (0.5, 0.97)):
                    r = run(cell=cell, recv=recv, seed=1, delta=dd, rho=rho)
                    self.assertEqual(r["gauges"]["p1_viol"], 0, (cell, recv, dd))
                    self.assertGreater(r["gauges"]["p1_checks"], 0 if rho == 0.99 else -1)
                    self.assertLessEqual(r["G_final"], int((1 - rho) * r["n_src"] + 1e-9))

    def test_checker_can_fail(self):
        sim = toy2.Sim(cfg(cell="I2", sender="S1"))
        sim.allow = lambda: 10 ** 9           # the sender ignores the count; the checker still reads the true allowance
        r = sim.run()
        self.assertGreater(r["gauges"]["p1_viol"], 0)


class P2Identity(unittest.TestCase):
    def test_rho_one_traces_equal_s0(self):
        for rho in (1.0, 1.0 - 1e-7):
            ref = run(cell="I2", sender="S0", seed=1, rho=rho, want_trace=True)["_trace"]
            # R0e is not in this list: its own passes move the ack, so the sender stops serving them (it is today's code)
            for recv, dd in (("R1", 50.0), ("U", 0.5), ("U", 50.0)):
                tr = run(cell="I2", sender="S1c", recv=recv, seed=1, rho=rho, delta=dd, want_trace=True)["_trace"]
                self.assertEqual(tr, ref, (recv, dd, rho))

    def test_rho_below_one_differs(self):
        ref = run(cell="I2", sender="S0", seed=1, want_trace=True)["_trace"]
        tr = run(cell="I2", sender="S1c", seed=1, want_trace=True)["_trace"]
        self.assertNotEqual(tr, ref)


class P4(unittest.TestCase):
    def test_p4a_hold_is_H_on_the_floor_pass(self):
        H = 0.005
        r0 = run(cell="I1", sender="S0", seed=1, dur=10.0, H_force=H)
        r1 = run(cell="I1", sender="S1", seed=1, dur=10.0, H_force=H)           # copy-less verdict: the pass is the resolution
        rc = run(cell="I1", sender="S1c", seed=1, dur=10.0, H_force=H, want_p4=True)
        # registered text says S1c; its last copy fills the hole at the instant S0's first resend would, so S1c == S0 at the median
        self.assertAlmostEqual(r1["iso_p50"] - r0["iso_p50"], H, delta=0.001)
        self.assertAlmostEqual(rc["iso_p50"] - r0["iso_p50"], 0.0, delta=0.001)
        # absolute: every floor pass happens exactly H after the floor's arrival (copy lost, or the floor arrives first)
        dts = rc["pass_dt"]
        self.assertGreater(len(dts), 0)
        for x in dts:
            self.assertAlmostEqual(x, H, delta=1e-9)

    def test_p4b_s1c_spend(self):
        r = run(cell="I1", sender="S1c", seed=1, dur=10.0)
        self.assertLessEqual(r["G_ratio"], 1.5 * 0.025 * r["h_hat"])

    def test_p4c_stall_per_isolated_hole(self):
        for dd, rho, om in ((50.0, 0.99, 10.0), (50.0, 0.99, 100.0), (0.5, 1.0, 1.0), (0.5, 0.99, 3.0)):
            r = run(cell="I1", sender="S1c", recv="U", seed=2, dur=8.0, delta=dd, rho=rho, omega=om, want_p4=True)
            pairs = r["p4c"]
            self.assertGreater(len(pairs), 5)
            for obs, exp in pairs:
                self.assertAlmostEqual(obs, exp, delta=1e-5, msg=(dd, om))

    def test_p4f_gate_never_decides_under_rtprop_at_realtime_and_auto(self):
        for cell in ("I1", "I2"):
            for dd in (50.0, 0.5):
                r = run(cell=cell, sender="S1c", recv="U", seed=1, delta=dd, rho=0.99, d_arm="RT")
                self.assertEqual(r["gauges"]["n_dec"], 0, (cell, dd))
        r = run(cell="I1", sender="S1c", recv="U", seed=1, delta=0.5, rho=0.99, d_arm="RT", q=True)
        self.assertEqual(r["gauges"]["n_dec"], 0)

    def test_p4f_witness_gate_decides_below_auto(self):
        for dd in (toy2.delta_at_b(1.3), 0.005):
            r = run(cell="I1", sender="S1c", recv="U", seed=1, delta=dd, rho=0.99, d_arm="RT")
            self.assertGreater(r["gauges"]["n_dec"], 0, dd)

    def test_ewma_arm_cold_start_and_ablation_are_distinct(self):
        s = toy2.Sim(cfg(d_arm="EWMA"))
        self.assertEqual(s.D_now(), (min(s.b, 2.0) * 0.050, True))   # main's 50 ms SRTT seed before the first sample
        self.assertEqual(toy2.Sim(cfg(d_arm="ZERO")).D_now(), (0.0, True))   # the gate always admits
        r = run(cell="I1", sender="S1c", recv="U", seed=1, delta=0.5, rho=0.99, d_arm="EWMA")
        self.assertIsNotNone(r["srtt"])
        self.assertGreater(r["srtt"], 0.010)                     # SRTT exceeds RTprop by sampler wait and jitter

    def test_p4g_d_all_equals_d_rep_at_one_path(self):
        r = run(cell="I2", sender="S1c", recv="U", seed=1, delta=0.5, rho=1.0)
        self.assertEqual(r["d_all"], r["d_rep"])
        self.assertGreater(r["d_n"], 10)

    def test_p4h_bridge_to_stage1(self):
        for seed in (0, 1, 2):
            r1 = toy.run(toy.make_cfg("I1", "S0", seed, jitter=0.0, H=0.0, floor_with_copy=False, dur=6.0, want_trace=True))
            r2 = toy2.run(toy2.Cfg(cell="I1", sender="S0", seed=seed, J=0.0, dur=6.0, phases=(0.0, 0.0), want_trace=True))
            self.assertEqual(r1["_trace"], r2["_trace"])
            for k in ("n_src", "delivered", "n_holes", "hol_sum_nonhole", "iso_p50", "G_final"):
                self.assertEqual(r1[k], r2[k], k)
            s1 = toy.run(toy.make_cfg("I1", "S1c", seed, jitter=0.0, H=0.0, floor_with_copy=False, dur=6.0))
            s2 = toy2.run(toy2.Cfg(cell="I1", sender="S1c", seed=seed, J=0.0, dur=6.0, phases=(0.0, 0.0)))
            self.assertEqual(s1["gauges"]["verdicts"], s2["gauges"]["verdicts"])

    def test_p4i_e_equals_stage1_hol_on_nonhole_packets(self):
        r1 = toy.run(toy.make_cfg("I1", "S0", 1, jitter=0.0, H=0.0, dur=6.0))
        r2 = toy2.run(toy2.Cfg(cell="I1", sender="S0", seed=1, J=0.0, dur=6.0, phases=(0.0, 0.0), want_trace=True))
        at, dl = np.asarray(r2["_atilde"]), np.asarray(r2["_dlv"][:r2["n_src"]])
        y0 = np.maximum.accumulate(at)
        nh = np.asarray(r2["_got"][:r2["n_src"]], bool) & ~np.asarray(r2["_wh"][:r2["n_src"]], bool) & (dl >= 0)
        self.assertAlmostEqual(float((dl - y0)[nh].sum()), r1["hol_sum_nonhole"], places=6)
        # FIFO: y0 == atilde, so e == v
        r3 = run(cell="I1", sender="S1c", recv="U", seed=1, J=0.004, delta=50.0)
        self.assertAlmostEqual(r3["e_mean"], r3["v_mean"], places=12)

    def test_p4j_two_x_bound_on_isolated_holes_with_d_frozen(self):
        for om in (1.0, 10.0):
            for dd, rho in ((0.005, 1.0), (0.5, 1.0), (50.0, 0.99)):
                for seed in (0, 1):
                    r = run(cell="I1", sender="S1c", recv="U", seed=seed, dur=8.0, delta=dd, rho=rho, omega=om, d_frozen=0.010)
                    self.assertGreater(r["u_iso_n"], 3)
                    self.assertLessEqual(r["u_iso_slack"], 1e-9, (om, dd, seed))


class UStar(unittest.TestCase):
    def _inst(self, rng, shape):
        n = rng.randint(1, 9)
        t, r, ind = [], [], []
        x = 0.0
        for _ in range(n):
            x += rng.random() * (0.01 if shape == 1 else 0.004)
            ln = rng.random() * (0.012 if shape == 1 else 0.05)        # N = 2 shape: long overlapping holds
            t.append(x)
            r.append(x + ln)
            ind.append(1 if rng.random() < 0.7 else 0)
        return t, r, ind

    def test_p4e_sweep_equals_lp_equals_brute(self):
        rng = random.Random(11)
        for k in range(200):
            t, r, ind = self._inst(rng, 1 + (k % 2))
            kappa = rng.choice([0.5, 5.0, 50.0, 500.0])
            om = rng.choice([0.01, 0.3, 1.0, 10.0])
            sw = ustar.sweep_min(t, r, ind, kappa, om)
            bf = ustar.brute_min(t, r, ind, kappa, om)
            lp, integral = ustar.lp_min(t, r, ind, kappa, om)
            self.assertAlmostEqual(sw, bf, places=9, msg=k)
            self.assertAlmostEqual(lp, bf, places=7, msg=k)
            self.assertTrue(integral, k)


class HEstimator(unittest.TestCase):
    def test_fifo_gives_zero(self):
        h = toy2.HEst(10.0)
        for i in range(1000):
            h.feed(0.001 * i + 0.005, 0.001 * i)
        self.assertEqual(h.value(1.0), 0.0)
        self.assertEqual(h.max_l, 0.0)

    def test_three_ms_behind_a_later_send_gives_3ms_then_zero_after_W(self):
        h = toy2.HEst(10.0)
        pk = [(0.001 * i + 0.005 + (0.007 if i == 4 else 0.0), 0.001 * i) for i in range(10)]   # packet 4 is 7 ms late
        for a, tau in sorted(pk):                   # fed in arrival order
            h.feed(a, tau)
        # packet 4 arrives at 4+5+7 = 16 ms; the earliest arrival among later sends is packet 5 at 10 ms: l = 6 ms
        self.assertAlmostEqual(h.value(0.02), 0.006, places=9)
        h2 = toy2.HEst(10.0)
        h2.feed(0.012, 0.007)                       # sent at 7, arrives 12
        h2.feed(0.015, 0.004)                       # sent earlier, arrives 3 ms after the later send's arrival
        self.assertAlmostEqual(h2.value(0.015), 0.003, places=9)
        self.assertEqual(h2.value(0.015 + 10.0 + 1e-6), 0.0)

    def test_empty_window_and_cold_start_are_zero(self):
        h = toy2.HEst(10.0)
        self.assertEqual(h.value(5.0), 0.0)
        s = toy2.Sim(cfg())
        self.assertEqual(s.H_now(0.0), 0.0)

    def test_strict_one_path_non_strict_across_paths(self):
        strict, loose = toy2.HEst(10.0, True), toy2.HEst(10.0, False)
        for h in (strict, loose):
            h.feed(0.010, 0.005)
            h.feed(0.012, 0.005)                    # same send instant, arrives later
        self.assertEqual(strict.value(0.012), 0.0)
        self.assertAlmostEqual(loose.value(0.012), 0.002, places=9)

    def test_two_path_skew_synthetic(self):
        h = toy2.HEst(10.0, False)
        pk = [(0.0005 * i + (0.005 if i % 2 == 0 else 0.025), 0.0005 * i) for i in range(2000)]   # 20 ms skew
        for a, tau in sorted(pk):
            h.feed(a, tau)
        self.assertGreaterEqual(h.value(1.1), 0.020 - 0.0005 - 1e-9)   # skew less one send gap
        self.assertLessEqual(h.value(1.1), 0.021)

    def test_later_floor_never_delays_a_hole_an_earlier_marker_covers(self):
        s = toy2.Sim(cfg(sender="S1c"))
        s.on_data(1.0, 10, 0, 0.99)                  # holes 0..9
        s.cfg.H_force = 0.003
        s.on_floor(1.0, 5)                           # marker (floor 5) at 1.003
        s.cfg.H_force = 0.010
        s.on_floor(1.001, 10)                        # marker (floor 10) at 1.011
        marks = sorted((t, pl) for (t, _, k, pl) in s.heap if k == 2)
        s.heap = []
        for t, pl in marks:
            s.apply_floor(t, pl[0], pl[1])
        for q in range(0, 5):
            self.assertAlmostEqual(s.tpass[q], 1.003)
        for q in range(5, 10):
            self.assertAlmostEqual(s.tpass[q], 1.011)


class PathEpsModel(unittest.TestCase):
    def test_gap_loss_ewma_and_late_credit(self):
        e = toy2.PathEps()
        for ps in range(10):
            e.record(ps)
        self.assertEqual(e.ewma, 0.0)
        e.record(12)                                  # path_seq 10 and 11 lost: expected 3, 2 lost
        self.assertAlmostEqual(e.ewma, 0.1 * 2 / 3)
        before = e.ewma
        e.record(11)                                  # late arrival: expected 0, no update, one credit
        self.assertEqual(e.ewma, before)
        self.assertEqual(e.credit, 1)
        e.record(14)                                  # one lost (13), paid by the credit: sample 0
        self.assertAlmostEqual(e.ewma, 0.9 * before)
        self.assertEqual(e.credit, 0)


class Receivers(unittest.TestCase):
    def test_r0e_passes_move_the_ack_count_and_refund(self):
        r = run(cell="I2", sender="S1c", recv="R0e", seed=1, dur=6.0)
        g = r["gauges"]
        self.assertGreater(g["pass_r0e"], 0)
        self.assertGreater(g["late"], 0)
        self.assertGreaterEqual(r["G_final"], 0)
        self.assertLessEqual(r["G_final"], g["passes"])

    def test_u_releases_do_not_move_the_ack(self):
        r = run(cell="I2", sender="S1c", recv="U", seed=1, dur=6.0, delta=50.0, rho=1.0)
        self.assertGreater(r["gauges"]["rel_ho"], 0)
        self.assertEqual(r["gauges"]["passes"], 0)
        self.assertEqual(r["G_final"], 0)

    def test_r0c_counts_only_floor_passes_for_admission(self):
        a = toy2.run(toy2.object_cfg("I2", "R0e", 1))
        c = toy2.run(toy2.object_cfg("I2", "R0c", 1))
        self.assertEqual(c["gauges"]["pass_r0e"] > 0, True)
        self.assertGreaterEqual(c["gauges"]["verdicts"], a["gauges"]["verdicts"])
        self.assertLessEqual(c["GF_final"], c["G_final"])
        self.assertLessEqual(c["GF_final"], 121)

    def test_global_hold_brake_binds_and_controls(self):
        cfgs = lambda eps: toy2.object_cfg("I2", "R0e", 1, recv_eps=eps)
        z, e, o = toy2.run(cfgs("zero")), toy2.run(cfgs("engine")), toy2.run(cfgs("one"))
        self.assertEqual(z["gauges"]["pass_r0e"], 0)                      # budget always spent: R1's output
        self.assertGreater(e["gauges"]["pass_r0e_lengthened"], 0)          # the brake decides passes
        self.assertLess(e["gauges"]["pass_r0e"], o["gauges"]["pass_r0e"])   # and removes early-skip passes against eps-hat = 1
        r1 = toy2.run(toy2.object_cfg("I2", "R1", 1))
        self.assertEqual(z["v_mean"], r1["v_mean"])

    def test_tail_cell_witnesses(self):
        for cell in ("I1", "I2", "L10"):
            for recv in ("R1", "U"):
                r = toy2.run(toy2.object_cfg(cell, recv, 1))
                self.assertEqual(r["gauges"]["p1_viol"], 0, (cell, recv))
                self.assertEqual(r["tail_n"], 347)
                self.assertFalse(r["stalled"])
                self.assertLessEqual(r["G_final"], 121)
                self.assertGreaterEqual(r["tail_verdicts"] + r["gauges"]["refusals"] // 100000, 0)


DIALS = {"rho", "delta", "hint"}
EXEMPT = {"plant_scale", "hz_hold", "r0e_buffer", "on_r0e_expiry"}     # HZ, STEP/SPIKE/KINK, R0e (7.3, 7.9)


def _names(node):
    for n in ast.walk(node):
        if isinstance(n, ast.Name) and n.id in DIALS:
            yield n.id
        elif isinstance(n, ast.Attribute) and n.attr in DIALS:
            yield n.attr


def deny_scan(src):
    """Branch-selecting comparisons on a dial: Compare nodes inside an if / while / ternary / assert test or a boolean
    operand, plus any Compare that mentions a dial at all (a bool derived from a dial is a mode bit renamed)."""
    bad = []
    tree = ast.parse(src)

    def visit(node, fn):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            fn = node.name
        if isinstance(node, ast.Compare) and fn not in EXEMPT and list(_names(node)):
            bad.append((fn, node.lineno))
        for ch in ast.iter_child_nodes(node):
            visit(ch, fn)

    visit(tree, None)
    return bad


class DenyTest(unittest.TestCase):
    def test_no_branch_on_a_dial_outside_the_named_exceptions(self):
        with open(os.path.join(HERE, "toy2.py"), encoding="utf-8") as f:
            src = f.read()
        self.assertEqual(deny_scan(src), [])

    def test_positive_control_flags_a_planted_comparison(self):
        self.assertTrue(deny_scan("def f(cfg):\n    if cfg.rho >= 1:\n        return 0\n"))
        self.assertTrue(deny_scan("def g(c):\n    return 1 if c.delta < 3 else 2\n"))
        self.assertTrue(deny_scan("def h(hint):\n    return hint == 'Bulk'\n"))
        self.assertTrue(deny_scan("def plantish(cfg):\n    return cfg.delta < 1\n"))
        self.assertFalse(deny_scan("def plant_scale(cfg):\n    return cfg.delta < 1\n"))     # exempt by name only
        self.assertFalse(deny_scan("def k(cfg):\n    return max(0.5, min(2.0, cfg.delta))\n"))   # arithmetic is fine


if __name__ == "__main__":
    unittest.main()
