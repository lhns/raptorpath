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


DIALS = {"rho", "delta", "hint", "T", "Hk", "Te"}        # Te: stage 3d, T_eff = plant_T(cfg, T), the knob under another name
# HZ, STEP/SPIKE/KINK, R0e (7.3, 7.9); the two stage-3 law functions (three-knobs.md 9.2) and the share_T readout are the only places T is compared
EXEMPT = {"plant_scale", "hz_hold", "r0e_buffer", "on_r0e_expiry", "intake_expired_fn", "resend_admissible", "within_lifetime",
          "dequeue_expired",      # dequeue_expired: three-knobs.md 13.2 (stage 3b)
          "protected",            # protected: three-knobs.md 15.3 and 15.6.1 item 1 (stage 3c)
          "plant_T"}              # plant_T: three-knobs.md 17.5 (stage 3d), the planted controls routed through T


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

    def test_stage3_positive_controls(self):
        self.assertTrue(deny_scan("def f(c):\n    return c.T < 1\n"))                         # three-knobs.md 9.2 test 6
        self.assertTrue(deny_scan("def f(c):\n    return c.Hk == 0\n"))
        self.assertFalse(deny_scan("def intake_expired_fn(T, age):\n    return age >= T\n"))   # the named law functions are exempt
        self.assertTrue(deny_scan("def other(T, age):\n    return age >= T\n"))

    def test_stage3b_positive_controls(self):                                                  # three-knobs.md 13.7.1 test 8
        self.assertTrue(deny_scan("def dequeue_expiredish(c):\n    return c.T < 1\n"))
        self.assertFalse(deny_scan("def dequeue_expired(T, age, m):\n    return age + m >= T\n"))
        self.assertTrue(deny_scan("def f(c, age):\n    return age + 0.1 >= c.T\n"))



# ---- stage 3 (three-knobs.md 9.2 and 11) ------------------------------------------------------
P1_KEYS = ("n_src", "delivered", "never_delivered", "open_end", "v_mean", "v_p50", "v_p99", "v_p999", "e_mean", "e_p99",
           "hol_sum_nonhole", "n_holes", "stall_n", "stall_total", "stall_p99", "repair_per_delivered",
           "floors_per_delivered", "ooo_per_delivered", "t_last")


def s3(**kw):
    kw.setdefault("rho", 1.0)
    kw.setdefault("check_p1", False)
    return toy2.Cfg(**kw)


def obj3(cell="I2", sender="S2", recv="RH", seed=0, **kw):
    kw.setdefault("n_total", 12147)
    kw.setdefault("owd", 0.05 if cell == "L10" else 0.005)
    return s3(cell=cell, sender=sender, recv=recv, seed=seed, **kw)


class Stage3(unittest.TestCase):
    def test_1_t_inf_s2_rh_inf_equals_s0_r1(self):
        for seed in range(3):
            ref = toy2.run(s3(cell="I1", sender="S0", recv="R1", seed=seed, dur=5.0, want_trace=True))
            new = toy2.run(s3(cell="I1", sender="S2", recv="RH", seed=seed, dur=5.0, want_trace=True))
            self.assertEqual(new["_trace"], ref["_trace"])
            for k in P1_KEYS:
                self.assertEqual(new[k], ref[k], k)

    def test_2_t_1e6_equals_t_inf(self):
        for seed in range(3):
            a = toy2.run(s3(cell="I1", sender="S2", recv="RH", seed=seed, dur=5.0, want_trace=True))
            b = toy2.run(s3(cell="I1", sender="S2", recv="RH", seed=seed, dur=5.0, want_trace=True, T=1e6))
            self.assertEqual(a["_trace"], b["_trace"])
            for k in P1_KEYS:
                self.assertEqual(a[k], b[k], k)

    def test_3_l1_absolute_and_executes(self):
        T = 0.05
        # a 0.5 s outage at 10 000/s.  S2p (verdicted seqs stay in the outstanding count) pauses until the outage ends, so
        # units age in the intake and expire; S2 resumes at its first verdict, so almost nothing expires (P7)
        for excl, expect_expiry in ((False, True), (True, False)):
            sim = toy2.Sim(obj3(sender="S2", recv="RH", Hk=0.0, T=T, n_total=6000, dead=(0, 0.1, 0.5), want_trace=True,
                                pause_B=200, o_excl_verdict=excl))
            r = sim.run()
            if expect_expiry:
                self.assertGreater(r["intake_expired"], 0)               # the mechanism ran
            n_data = 0
            for (t, s, k, _, kind) in r["_trace"]:
                if kind in (0, 1):
                    n_data += 1
                    self.assertLess(t - sim.h_seq[s], T, (excl, t, s, k, kind))
            self.assertGreater(n_data, 1000)

    def test_4_l2_absolute_no_copy_past_the_bar(self):
        for cell, owd, T in (("I2", 0.005, 0.05), ("L10", 0.05, 0.3)):
            sim = toy2.Sim(obj3(cell=cell, sender="S2", recv="RH", Hk=0.0, T=T, owd=owd, want_trace=True))
            r = sim.run()
            ncopy = 0
            for (t, s, k, _, kind) in r["_trace"]:
                if kind == 1:
                    ncopy += 1
                    self.assertLess(t - sim.h_seq[s] + owd, T, (cell, t, s, k))
            self.assertGreater(ncopy, 0)
            self.assertGreater(r["l2_fail"], 0)                          # L2 refused something

    def test_5_rh_zero_has_no_non_hole_wait(self):
        for sender, T in (("S2", 0.4), ("S2", math.inf), ("S1c", math.inf)):
            r = toy2.run(s3(cell="I2", sender=sender, recv="RH", Hk=0.0, T=T, seed=1, dur=5.0))
            self.assertEqual(r["hol_sum_nonhole"], 0.0)
            self.assertEqual(r["v_p50"], 0.0)
            self.assertGreater(r["n_holes"], 0)

    def test_6_rh_inf_equals_r1_at_finite_t(self):
        a = toy2.run(obj3(sender="S2", recv="RH", T=0.4, seed=1))
        b = toy2.run(obj3(sender="S2", recv="R1", T=0.4, seed=1))
        for k in P1_KEYS + ("share_any", "share_by_2", "w_mean", "l2_fail", "intake_expired"):
            self.assertEqual(a[k], b[k], k)

    def test_7_fixed_reference_shares_and_w(self):
        for sender, kw in (("S1c", dict(rho=0.99, check_p1=True)), ("S2", dict(T=0.4, Hk=0.0))):
            c = obj3(sender=sender, recv="RH" if sender == "S2" else "R1", seed=2, want_trace=True, dead=(0, 0.3, 0.5), **kw)
            r = toy2.run(c)
            dlv, h = r["_dlv"], r["_h"]
            n, units = r["n_src"], r["units"]
            ok = [s for s in range(n) if dlv[s] >= 0]
            self.assertAlmostEqual(r["share_by_0p4"], sum(1 for s in ok if dlv[s] - h[s] <= 0.4) / units, places=12)
            self.assertAlmostEqual(r["share_by_2"], sum(1 for s in ok if dlv[s] - h[s] <= 2.0) / units, places=12)
            self.assertAlmostEqual(r["share_any"], len(ok) / units, places=12)
            w = np.asarray([dlv[s] - h[s] - c.owd for s in ok])
            self.assertAlmostEqual(r["w_mean"], float(w.mean()), places=12)
            self.assertAlmostEqual(r["w_p50"], float(np.percentile(w, 50)), places=12)
            self.assertAlmostEqual(r["w_p99"], float(np.percentile(w, 99)), places=12)
            self.assertLess(r["share_by_0p4"], r["share_by_2"] + 1e-12)
            self.assertLess(r["share_by_0p4"], r["share_any"])           # the outage window delays some units past 0.4 s or loses them

    def test_8_events_at_infinity_do_not_end_the_run(self):
        r = toy2.run(obj3(sender="S2", recv="RH", seed=1))              # T = H = inf: every timer sits at +inf
        self.assertFalse(r["stalled"])
        self.assertTrue(math.isfinite(r["t_last"]))
        self.assertEqual(r["intake_expired"], 0)

    def test_9_o_excl_verdict_matters_in_an_outage(self):
        a = toy2.run(obj3(sender="S2", recv="RH", Hk=0.0, T=0.4, seed=1, dead=(0, 0.4 * 1.2147, 1.5)))
        b = toy2.run(obj3(sender="S2", recv="RH", Hk=0.0, T=0.4, seed=1, dead=(0, 0.4 * 1.2147, 1.5), o_excl_verdict=False))
        self.assertLess(a["intake_expired"], b["intake_expired"])


# ---- stage 3b (three-knobs.md 13.7.1) -----------------------------------------------------------------
import json  # noqa: E402
import batch3  # noqa: E402
import batch3b  # noqa: E402

INF = math.inf


def keys_equal(a, b, keys=P1_KEYS):
    return [k for k in keys if not (a[k] == b[k] or (isinstance(a[k], float) and math.isnan(a[k]) and math.isnan(b[k])))]


class Stage3b(unittest.TestCase):
    CELLS3 = ("E-L10", "M-I2", "K-L10")

    def test_1_defaults_reproduce_stage3(self):
        c = toy2.Cfg()
        self.assertEqual((c.rtt_est, c.src_model, c.link, c.tail_elig, c.resend_gate, c.deq_expiry, c.evidence_verdict),
                         ("old", "old", None, "P", "owd", False, False))
        # one stored stage 3 run (out3 is not tracked; skipped when absent) re-run through the stage 3 runner
        f = os.path.join(HERE, "out3", "K", "K-L10.G-T0p4-H0p02-0.json")
        if os.path.exists(f):
            batch3.build()
            with open(f) as fh:
                old = json.load(fh)
            new = json.loads(json.dumps(batch3.run_one(batch3.JOBS["K-L10.G-T0p4-H0p02"][1](0))))
            for k in old:
                if k not in ("wall_s", "gauges"):
                    self.assertEqual(json.dumps(old[k]), json.dumps(new[k]), k)          # json: nan == nan
            for k in old["gauges"]:
                self.assertEqual(old["gauges"][k], new["gauges"][k], k)

    def test_2_s3_p_at_t_inf_equals_c3(self):
        batch3b.build()
        for cell in self.CELLS3:
            for seed in range(3):
                a = batch3b.cmp_cfg(cell, "C3", seed)
                b = batch3b.s3_cfg(cell, seed, INF, INF, tail="P")
                a.want_trace = b.want_trace = True
                ra, rb = toy2.run(a), toy2.run(b)
                self.assertEqual(keys_equal(ra, rb), [], (cell, seed))
                self.assertEqual(ra["_trace"], rb["_trace"], (cell, seed))

    def test_3_s3_t_1e6_equals_t_inf_both_tail_elig(self):
        for cell in ("M-I2", "K-L10"):
            for tail in ("P", "R"):
                for seed in range(2):
                    a = batch3b.s3_cfg(cell, seed, INF, INF, tail=tail)
                    b = batch3b.s3_cfg(cell, seed, 1e6, INF, tail=tail)
                    a.want_trace = b.want_trace = True
                    ra, rb = toy2.run(a), toy2.run(b)
                    self.assertEqual(keys_equal(ra, rb), [], (cell, tail, seed))
                    self.assertEqual(ra["_trace"], rb["_trace"], (cell, tail, seed))

    def _brake_decreases(self, sender, **kw):
        c = batch3b.s3_cfg("K-L10", 1, 0.4, 0.0, sender=sender, **kw)
        sim = toy2.Sim(c)
        log = []
        for name in ("on_data", "on_floor", "apply_floor", "on_report", "on_tick", "on_sweep", "on_release", "on_r0e_expiry",
                     "on_verdict_timer", "on_probe", "emit"):
            orig = getattr(sim, name)

            def wrap(*a, _o=orig, _n=name, **k):
                before = sim.O_ctl()
                r = _o(*a, **k)
                if sim.O_ctl() < before:
                    log.append(_n)
                return r
            setattr(sim, name, wrap)
        sim.run()
        return log, sim

    def test_4_brake_falls_only_on_feedback(self):
        log, sim = self._brake_decreases("S3")
        self.assertGreater(sim.g["verdicts"], 0)                     # the mechanism ran: verdicts were taken
        self.assertGreater(len(log), 0)
        self.assertEqual(set(log), {"on_report"})
        log2, _ = self._brake_decreases("S3x")                       # positive control: the exclusion arm releases on a verdict
        self.assertIn("on_verdict_timer", log2)

    def test_5_vta_zero_in_k_i2_and_spurious_verdicts_at_stage3(self):
        r = toy2.run(batch3b.s3_cfg("K-I2", 0, 0.14, 0.0))
        self.assertEqual(r["vta"], 0)
        r3 = toy2.run(batch3b.s3_cfg("K-L10", 0, 0.14, 0.0))
        self.assertEqual(r3["vta"], 0)
        r2 = toy2.run(batch3b.s2_cfg("K-L10", 0, 0.14, 0.0))          # witness: T - owd is below one round trip, stage 3 verdicts delivered data
        self.assertGreater(r2["vta"], 0)

    def test_6_dequeue_rule_in_overload(self):
        c = batch3b.s3_cfg("OV", 0, 0.25, 0.0, n_total=24000, want_trace=True)
        r = toy2.run(c)
        self.assertGreater(r["intake_expired_deq"], 0)                # the mechanism ran
        log = r["_deq_log"]
        self.assertGreater(len(log), 1000)
        for (t, u, age, R, younger) in log:
            self.assertFalse(age + R >= c.T and younger, (t, u, age, R))
        self.assertTrue(any(age + R >= c.T for (_, _, age, R, _) in log))     # the sole-unit clause also executed

    def test_7_link_q0_drops_everything_and_large_link_is_a_wire(self):
        c = batch3b.s3_cfg("K-L10", 0, INF, INF, link=dict(C=1000.0, Q=0.0), rate=50.0)
        r = toy2.run(c)
        self.assertEqual(r["delivered"], 0)
        self.assertEqual(r["gauges"]["queue_drop"], r["gauges"]["floors"] + r["gauges"]["repair_sends"] + r["n_src"])
        c = batch3b.s3_cfg("K-L10", 0, INF, INF, link=dict(C=1e9, Q=1e9), want_trace=True)
        sim = toy2.Sim(c)
        sim.run()
        for s in range(0, 3000, 97):
            d = sim.sent_t[s] + 1e-9
            self.assertAlmostEqual(sim.atilde[s], d + sim.delay.at(d), places=12)

    def test_8_probe_and_work_conserving_source_execute(self):
        r = toy2.run(batch3b.s3_cfg("K-L10", 0, INF, INF))
        self.assertGreater(r["probes"], 0)
        self.assertGreater(r["gauges"]["resend_by_R"], 0)
        c = batch3b.s3_cfg("CL", 0, INF, INF, n_total=20000, dead=(0, 0.5, 0.5))
        r = toy2.run(c)
        self.assertGreater(r["pause_events"], 0)                       # the pause, not the queue, brakes
        self.assertEqual(r["queue_drop"], r["queue_drop_in"] + r["queue_drop_out"])
        self.assertTrue(r["link_util_w"] > 0)

    def test_9_rfc_rtt_estimator_karn_and_seed(self):
        sim = toy2.Sim(batch3b.s3_cfg("K-I2", 0, INF, INF))
        self.assertAlmostEqual(sim.R_now(), 0.333 + 4 * 0.1665 + 0.002, places=9)
        sim.run()
        self.assertAlmostEqual(sim.srtt, 0.010, delta=0.004)           # 2 x owd of 5 ms, plus the report tick

    def test_10_resend_gate_evidence_executes(self):
        """The gate "evidence" must change the resend count somewhere.  Amendment (RTT sampling, stage 3d): the first version compared
        seed 0 only, and the corrected srtt made the two counts coincide there (331 = 331), so the witness reads all of seeds 0 to 4."""
        pairs = []
        for seed in range(5):
            a = toy2.run(batch3b.s3_cfg("K-L10", seed, 0.4, 0.0, resend_gate="owd"))
            b = toy2.run(batch3b.s3_cfg("K-L10", seed, 0.4, 0.0, resend_gate="evidence"))
            pairs.append((seed, a["gauges"]["repair_sends"], b["gauges"]["repair_sends"]))
        print("resend_gate owd vs evidence, repair_sends per seed:", pairs)
        self.assertTrue(any(x != y for _, x, y in pairs), pairs)                  # the gate executes


# ---- stage 3c (three-knobs.md 15.6.1) -----------------------------------------------------------------
import batch3c  # noqa: E402

# keys the S4-equals-C3 identity compares (15.6.1 test 2): every key of the output except the config labels, the RTT estimator's own
# readouts (the old-sweep arm in an off-link cell keeps stage 3's estimator), wall time and the gauge dict (a fixed subset of it is compared)
ID_EXCL = {"sender", "recv", "rho", "delta", "omega", "srtt", "srtt_mean", "R_mean", "wall_s", "gauges", "F_lag", "floor_hull_bind"}
ID_GAUGES = ("repair_sends", "serves", "floors", "floor_rep", "passes", "dup", "ooo_loss", "late", "pause_events", "queue_drop",
             "queue_drop_in", "queue_drop_out", "max_O", "fifo_viol")
SMALL = {"CL": dict(n_total=20000, dead=(0, 0.5, 0.5)), "OV": dict(n_total=24000)}


def id_diff(a, b):
    bad = [k for k in a if k not in ID_EXCL and not k.startswith("_") and json.dumps(a[k]) != json.dumps(b[k])]
    bad += [("g", k) for k in ID_GAUGES if a["gauges"][k] != b["gauges"][k]]
    return bad


class Stage3c(unittest.TestCase):
    CELLS3 = ("E-L10", "M-I2", "K-L10", "E-I2n", "CL", "OV")

    def _cfgs(self, cell, seed, T, H, **kw):
        kw = dict(kw)
        kw.update(SMALL.get(cell, {}))
        return batch3c.s4_cfg(cell, seed, T, H, **kw)

    def test_1_defaults_and_protected_law(self):
        c = toy2.Cfg()
        self.assertEqual((c.floor_law, c.floor_carrier, c.resend_gate, c.resend_brake, c.verdict_evidence),
                         ("verdict", "header", "owd", "none", "rev1"))
        # absolute values of the law at the anchor points: max(now, last_send + c) - h + srtt/2 < T
        P = toy2.protected
        self.assertTrue(P(0.4, 0.10, 0.0, 0.01, 0.0, 0.02))               # 0.10 + 0.01 = 0.11 < 0.4
        self.assertFalse(P(0.4, 0.395, 0.0, 0.01, 0.0, 0.02))             # 0.395 + 0.01 = 0.405
        self.assertFalse(P(0.4, 0.10, 0.39, 0.05, 0.0, 0.02))             # the cooldown pushes the earliest resend to 0.44
        self.assertTrue(P(math.inf, 5.0, 0.0, 0.01, 0.0, 0.02))
        self.assertTrue(P(0.4, 0.2, 0.0, 0.01, 0.0, 0.2))                 # 0.2 + 0.1 = 0.3 < 0.4
        self.assertFalse(P(0.4, 0.2, 0.0, 0.01, 0.0, 0.5))                # 0.2 + 0.25 = 0.45

    def test_2_s4_t_inf_equals_c3_every_cell_type(self):
        for cell in self.CELLS3:
            for seed in range(3):
                a = batch3c.cmp_cfg(cell, "C3", seed, **SMALL.get(cell, {}))
                b = self._cfgs(cell, seed, INF, INF)
                a.want_trace = b.want_trace = True
                ra, rb = toy2.run(a), toy2.run(b)
                self.assertEqual(id_diff(ra, rb), [], (cell, seed))
                self.assertEqual(ra["_trace"], rb["_trace"], (cell, seed))
                self.assertTrue(rb["finished"] and ra["finished"], (cell, seed))

    def test_3_s4_t_1e6_equals_t_inf(self):
        for cell in ("M-I2", "K-L10", "CL", "OV"):
            for seed in range(2):
                a = self._cfgs(cell, seed, INF, INF)
                b = self._cfgs(cell, seed, 1e6, INF)
                a.want_trace = b.want_trace = True
                ra, rb = toy2.run(a), toy2.run(b)
                self.assertEqual(id_diff(ra, rb), [], (cell, seed))
                self.assertEqual(ra["_trace"], rb["_trace"], (cell, seed))

    def test_4_commitment_seam_reads_zero_and_the_floor_executes(self):
        """The floor executes for S4 in every cell.  The commitment seam reads zero for S5 (the sender with the floor re-check), same cell and seed.
        Amendment (RTT sampling, stage 3d): S4 has no re-check and is known to violate the commitment (stage 3c: 143 violations in 28 runs);
        the zero asserted for S4 here came partly from the wrong srtt, and with the corrected srtt S4 reads 1 at CL seed 1."""
        for cell in ("K-L10", "OV", "CL"):
            s5 = toy2.Sim(small_s5(cell, 1, 0.4, INF))
            s5.run()
            self.assertEqual(s5.g["commit_viol"], 0, cell)
            sim = toy2.Sim(self._cfgs(cell, 1, 0.4, INF))
            r = sim.run()
            self.assertGreater(sim.F_ann, 0, cell)                                   # the floor advanced
            self.assertGreater(r["pass_floor"], 0, cell)                             # and the receiver passed holes by it
            self.assertGreater(sim.g["floor_carried"], 0, cell)                      # the header carrier executed
            self.assertEqual(r["late_floor"], 0, cell)
            self.assertEqual(r["verdicts"], 0, cell)                                 # 15.1: the verdict is unreachable
        # positive control: the seam does count a copy staged below an announced floor
        sim = toy2.Sim(self._cfgs("K-L10", 1, 0.4, INF))
        sim.F_ann = 10
        sim.n_sent = 20
        sim.send_copy(0.0, 5)
        self.assertEqual(sim.g["commit_viol"], 1)

    def test_5_no_floor_carried_or_sent_at_t_inf(self):
        for cell in ("K-L10", "M-I2", "OV"):
            sim = toy2.Sim(self._cfgs(cell, 0, INF, INF))
            seen = []
            orig = sim.on_report

            def wrap(now, rep, _o=orig, _s=sim):
                _o(now, rep)
                seen.append(_s.F_ann == _s.A_known)
            sim.on_report = wrap
            r = sim.run()
            self.assertTrue(seen and all(seen), cell)
            self.assertEqual((r["gauges"]["floors"], sim.g["floor_carried"]), (0, 0), cell)

    def test_6_brake_window_and_control(self):
        kw = dict(n_total=20000, dead=(0, 0.5, 0.5))
        sim = toy2.Sim(batch3c.cmp_cfg("CL", "C3", 0, **kw))
        r = sim.run()
        self.assertLessEqual(max(sim.copy_win.values()), 10)
        self.assertGreater(r["resend_braked"], 0)                                     # the brake bound
        sim = toy2.Sim(batch3c.cmp_cfg("CL", "C3", 0, resend_brake="none", **kw))
        sim.run()
        self.assertGreater(max(sim.copy_win.values()), 10)                            # control: without it a window exceeds 10

    def test_7_deny_test_with_protected_exempt(self):
        self.assertFalse(deny_scan("def protected(T, now, last_send, c, h, srtt_eff):\n    return max(now, last_send + c) - h + srtt_eff / 2 < T\n"))
        self.assertTrue(deny_scan("def protectedish(c, now):\n    return now < c.T\n"))
        self.assertTrue(deny_scan("def f(self, s):\n    return self.h_seq[s] + self.cfg.T < 3\n"))
        with open(os.path.join(HERE, "toy2.py"), encoding="utf-8") as f:
            src = f.read()
        self.assertEqual(deny_scan(src), [])
        self.assertNotIn("isinf", src)                                                # no isinf on a knob anywhere in the model

    def test_8_feedback_verdict_on_fabricated_state(self):
        def fresh():
            sim = toy2.Sim(batch3c.s4_cfg("K-L10", 0, 0.1, INF))
            sim.n_sent = 10
            for s in range(10):
                sim.h_seq[s] = 0.0
                sim.sent_t[s] = 0.0
                sim.last_send[s] = 0.01
            sim.M_known = {3}
            sim.rhi_known = 9
            sim.tau_rx = 0.05            # a later datagram arrived: last_send 0.01 < 0.05
            sim.last_gen = 0.06
            sim.srtt = 0.1
            return sim
        sim = fresh()                                                   # age 0.3 + srtt/2 >= T = 0.1: L2 fails
        sim.act(0.3, 0.06, [3], open_tail=False)
        self.assertIn(3, sim.verdicted)                                  # missing, report current, newer evidence: verdict
        sim = fresh()
        sim.M_known = set()                                              # nobody reported it missing: never judged
        sim.act(0.3, 0.06, [3], open_tail=True)
        self.assertEqual(sim.verdicted, set())
        sim = fresh()
        sim.tau_rx = 0.005                                               # no datagram sent after the last send has arrived
        sim.act(0.3, 0.06, [3], open_tail=False)
        self.assertEqual(sim.verdicted, set())
        sim = fresh()
        sim.last_send[3] = 0.2                                           # the report is older than the last send
        sim.tau_rx = 0.25
        sim.act(0.3, 0.06, [3], open_tail=False)
        self.assertEqual(sim.verdicted, set())

    def test_9_carrier_header_vs_datagram(self):
        a = toy2.Sim(batch3c.s4_cfg("OV", 0, 0.4, INF, n_total=36000))
        ra = a.run()
        b = toy2.Sim(batch3c.s4_cfg("OV", 0, 0.4, INF, carrier="datagram", n_total=36000))
        rb = b.run()
        self.assertGreater(a.g["floor_carried"], 1000)
        self.assertEqual(b.g["floor_carried"], 0)
        self.assertGreater(rb["gauges"]["floors"], 10 * max(ra["gauges"]["floors"], 1))   # the floor datagrams take link slots (option b)
        self.assertLessEqual(rb["gauges"]["floors"], 500 * 6.0)                             # at most one per 2 ms window (6 s run, with slack)

    def test_10_collapse_fixed_in_ov(self):
        h0 = toy2.run(batch3c.s4_cfg("OV", 0, 0.4, 0.0))
        hi = toy2.run(batch3c.s4_cfg("OV", 0, 0.4, INF))
        old = toy2.run(batch3b.s3_cfg("OV", 0, 0.4, INF))
        self.assertGreater(h0["share_by_0p4"], 0.75)
        self.assertGreater(hi["share_by_0p4"], h0["share_by_0p4"] - 0.05)       # one seed; the registered 0.03 is read on 12 seeds
        self.assertLess(old["share_by_0p4"], 0.3)                                     # the stage 3b collapse reproduces on the same cell

    def test_11_finishes_and_window_series_exist(self):
        r = toy2.run(self._cfgs("CL", 0, INF, 0.0))
        self.assertTrue(r["finished"])
        self.assertTrue(len(r["win_first"]) >= 2 and len(r["win_util"]) == len(r["win_first"]))
        self.assertEqual(sum(r["win_first"]), r["n_src"])


# ---- stage 3d (three-knobs.md 17.9, tests 2 to 10) ----------------------------------------------------
import batch3d  # noqa: E402

SMALL3D = {"CL": dict(n_total=20000, dead=(0, 0.5, 0.5)), "OV": dict(n_total=24000), "CL@b": dict(n_total=20000, dead=(0, 0.5, 0.5)),
           "OV@b": dict(n_total=24000)}


def small_s5(cell, seed, T, H, **kw):
    lc, d = batch3d.base(cell)
    d.update(SMALL3D.get(cell, {}))
    f = dict(evidence_verdict=False, verdict_evidence="none", floor_law="protect", floor_carrier="header", deq_expiry=True,
             o_excl_verdict=False, rtt_est="rfc", tail_elig="P", trigger_law="trigger", owd_est="fwd", recheck_floor=True)
    f.update(kw)
    d.update(f)
    return toy2.Cfg(cell=lc, sender="S5", recv="RH", seed=seed, delta=50.0, rho=1.0, check_p1=False, T=T, Hk=H, **d)


def small_c3(cell, seed, **kw):
    return batch3d.cmp_cfg(cell, "C3", seed, **dict(SMALL3D.get(cell, {}), **kw))


class Stage3d(unittest.TestCase):
    ID_CELLS = ("E-L10", "M-I2", "K-L10", "K-L10b", "CL", "OV", "X15", "P2", "K-L10@b", "CL@b", "OV@b")
    SEED_CTRL = 5

    def test_1_defaults(self):
        c = toy2.Cfg()
        self.assertEqual((c.trigger_law, c.owd_est, c.recheck_floor, c.sweep_form, c.rx_refresh, c.plant_T, c.paths, c.phi, c.reorder_rack, c.windows),
                         ("cooldown", "half", False, "tail", False, None, None, 0.5, False, False))
        self.assertEqual(batch3d.s5_cfg("OV", 0, 0.4, 0.0).sender, "S5")
        self.assertEqual(batch3d.s5_cfg("OV", 0, 0.4, 0.0).verdict_evidence, "none")

    def test_2_s5_t_inf_equals_c3_and_t_1e6_equals_t_inf(self):
        for cell in self.ID_CELLS:
            for seed in range(3):
                a = small_c3(cell, seed)
                b = small_s5(cell, seed, INF, INF)
                c = small_s5(cell, seed, 1e6, INF)
                a.want_trace = b.want_trace = c.want_trace = True
                ra, rb, rc = toy2.run(a), toy2.run(b), toy2.run(c)
                self.assertEqual(id_diff(ra, rb), [], (cell, seed))
                self.assertEqual(id_diff(rb, rc), [], (cell, seed))
                self.assertEqual(ra["_trace"], rb["_trace"], (cell, seed))
                self.assertEqual(rb["_trace"], rc["_trace"], (cell, seed))
                self.assertTrue(ra["finished"] and rb["finished"] and rc["finished"], (cell, seed))
        for form in ("tail", "blocker"):                              # P2 under both sweep forms
            kw = dict(sweep_form=form, rx_refresh=(form == "blocker"))
            a = batch3d.cmp_cfg("P2", "C3", 2, **kw)
            b = batch3d.s5_cfg("P2", 2, INF, INF, H_force=None, **kw)
            a.want_trace = b.want_trace = True
            ra, rb = toy2.run(a), toy2.run(b)
            self.assertEqual(id_diff(ra, rb), [], form)
            self.assertEqual(ra["_trace"], rb["_trace"], form)

    def test_3_commitment_is_absolute_and_the_control_reads_the_old_seam(self):
        # the planted positive control: 3c's law fields with the re-check disabled reproduce a violation in OV seed 5
        old = dict(trigger_law="cooldown", owd_est="half", recheck_floor=False)
        sim = toy2.Sim(batch3d.s5_cfg("OV", self.SEED_CTRL, 0.4, INF, **old))
        sim.run()
        self.assertGreater(sim.g["commit_viol"], 0)
        sim = toy2.Sim(batch3d.s5_cfg("OV", self.SEED_CTRL, 0.4, INF))
        r = sim.run()
        self.assertEqual(sim.g["commit_viol"], 0)
        self.assertLessEqual(sim.g["skip_below_floor"], 0.001 * sim.g["repair_sends"] + 1)
        self.assertTrue(r["finished"])
        # the re-check alone, on fabricated state: a candidate below the announced floor is skipped and counted, never sent
        sim = toy2.Sim(small_s5("K-L10", 1, 0.4, INF))
        sim.n_sent, sim.F_ann = 20, 10
        self.assertFalse(sim.send_copy(0.0, 5))
        self.assertEqual((sim.g["skip_below_floor"], sim.g["commit_viol"], sim.copies[5]), (1, 0, 0))

    def test_4_d_of_s_and_p_reach_from_the_candidate_builder(self):
        self.assertEqual(toy2.trigger_delay(True, 0.02, 0.05, 0.1), 0.02)                  # resent: the cooldown c
        self.assertEqual(toy2.trigger_delay(False, 0.02, 0.05, 0.1), 0.05)                 # never resent: min(srtt, P_reach)
        self.assertEqual(toy2.trigger_delay(False, 0.02, 0.05, 0.025), 0.025)
        self.assertEqual(toy2.trigger_delay(False, 0.02, 0.05, math.inf), 0.05)
        for form in ("tail", "blocker"):
            sim = toy2.Sim(small_s5("K-L10", 0, 0.4, INF, sweep_form=form))
            sim.n_sent = 10
            for s in range(10):
                sim.sent_t[s] = sim.last_send[s] = 0.0
            sim.A_known = 2
            sim.srtt = 0.2
            self.assertAlmostEqual(sim.d_of(2), min(0.2, sim.sweep_reach(2)))
            self.assertAlmostEqual(sim.d_of(5), min(0.2, sim.sweep_reach(5)))
            sim.copies[5] = 1
            self.assertEqual(sim.d_of(5), sim.cooldown())
            seen = []
            sim.act = lambda now, gen, holes, open_tail, _s=seen: _s.append(list(holes))
            if form == "tail":                                                                # the builder: every open-tail seq older than the period
                sim.rhi_known = 3
                sim.on_sweep(sim.sweep_period() + 0.001)
                self.assertEqual(seen[0], list(range(4, 10)))
                self.assertTrue(all(sim.sweep_reach(s) == sim.sweep_period() for s in seen[0]))
            else:                                                                             # only the cumulative blocker
                sim.bl_dl = 1.0
                sim.on_blocker(1.0, 1.0)
                self.assertEqual(seen[0], [2])
                self.assertEqual(sim.sweep_reach(2), sim.blocker_period())
                self.assertEqual(sim.sweep_reach(3), math.inf)

    def test_5_o_fwd_is_half_srtt_without_a_queue_and_owd_plus_queue_in_ov(self):
        sim = toy2.Sim(small_s5("K-L10", 0, 0.4, INF))
        sim.srtt = 2 * sim.cfg.owd                                 # RTprop: no queue
        self.assertAlmostEqual(sim.o_fwd(), sim.srtt / 2.0)
        sim.srtt = 2 * sim.cfg.owd + 0.2                           # 0.2 s of queue on the data direction
        self.assertAlmostEqual(sim.o_fwd(), sim.cfg.owd + 0.2)
        sim = toy2.Sim(small_s5("OV", 0, 0.4, INF))
        sim.run()
        q = sim.o_fwd() - sim.cfg.owd                              # the queue the estimator reads at the end of the run
        self.assertGreater(q, 0.05)
        self.assertGreater(sim.o_fwd(), sim.srtt / 2.0 + 0.03)     # srtt/2 would under-read it

    def test_6_blocker_sweep_and_receiver_refresh(self):
        sim = toy2.Sim(small_s5("K-L10@b", 0, INF, INF))
        fires, acts, refresh = [], [], []
        o_blocker, o_act, o_tick = sim.on_blocker, sim.act, sim.on_tick

        def on_blocker(now, dl, _o=o_blocker):
            if dl == sim.bl_dl:
                fires.append((now, dl, max(sim.last_send[sim.A_known], sim.bl_fire) + sim.blocker_period()))
            _o(now, dl)

        def act(now, gen, holes, open_tail, _o=o_act):
            if sim.trig == "sweep":
                acts.append((list(holes), sim.A_known))
            _o(now, gen, holes, open_tail)

        def on_tick(now, _o=o_tick):
            r0, lr = sim.g["refresh_reports"], sim.last_rep_t
            period = min(max(2.0 * sim.srtt_eff(), 0.025), 0.1)
            _o(now)
            if sim.g["refresh_reports"] != r0:
                refresh.append((now - lr, period))
        sim.on_blocker, sim.act, sim.on_tick = on_blocker, act, on_tick
        r = sim.run()
        self.assertGreater(len(fires), 5)
        self.assertGreater(len(acts), 0)
        self.assertTrue(all(h == [b] for h, b in acts))                      # only the cumulative blocker
        self.assertTrue(all(abs(now - dl) < 1e-9 and abs(dl - exp) < 1e-9 for now, dl, exp in fires))     # at the engine's deadline
        self.assertGreater(len(refresh), 0)
        self.assertTrue(all(p - 1e-9 <= gap <= p + sim.cfg.tick + 1e-9 for gap, p in refresh))            # at the clamped cadence
        self.assertTrue(r["finished"])
        # control: without the refresh no such reports
        sim = toy2.Sim(small_s5("K-L10@b", 0, INF, INF, rx_refresh=False))
        sim.run()
        self.assertEqual(sim.g["refresh_reports"], 0)

    def test_7_two_path_model(self):
        # path 0's draws are today's draws
        d0 = toy2.Delay(3, 0.005, 0.001, False, 5.0)
        d1 = toy2.Delay(3, 0.005, 0.001, False, 5.0, 0)
        self.assertEqual(d0.K, d1.K)
        self.assertNotEqual(d0.K, toy2.Delay(3, 0.005, 0.001, False, 5.0, 1).K)
        sim = toy2.Sim(batch3d.s5_cfg("P2", 2, 0.4, INF))
        self.assertEqual(sim.lost(7, 0, 0.0), sim.lost(7, 0, 0.0, toy2.SALT_LOSS, 0))
        self.assertEqual(sim.lost(7, 0, 0.0), toy.u01(2, 7, 0, 0, toy.SALT_LOSS) < 0.03)
        # copies go on A, first sends split by phi, each path FIFO, the paths not ordered against each other
        log = []
        o_fwd, o_copy = sim.fwd, sim.send_copy
        sim._cp = False

        def fwd(t, path=0, _o=o_fwd):
            log.append((sim._cp, path))
            return _o(t, path)

        def copy(now, s, _o=o_copy):
            sim._cp = True
            try:
                return _o(now, s)
            finally:
                sim._cp = False
        sim.fwd, sim.send_copy = fwd, copy
        r = sim.run()
        cp = [p for c, p in log if c]
        self.assertGreater(len(cp), 100)
        self.assertEqual(set(cp), {0})
        firsts = int(sum(sim.path_of[:sim.n_sent]))
        self.assertAlmostEqual(firsts / sim.n_sent, 0.5, delta=0.02)
        self.assertEqual(sim.g["fifo_viol"], 0)
        cross = sum(1 for s in range(200, 2000) if sim.path_of[s] == 1 and sim.path_of[s + 1] == 0 and sim.got[s] and sim.got[s + 1] and sim.arr[s] > sim.arr[s + 1])
        self.assertGreater(cross, 10)
        self.assertTrue(r["finished"])

        def fab(rack, tau):                       # reorder_rack suppresses the resend of an in-flight B unit (fabricated state)
            sm = toy2.Sim(batch3d.s5_cfg("P2", 0, INF, INF, reorder_rack=rack))
            sm.n_sent = 10
            for s in range(10):
                sm.sent_t[s] = sm.last_send[s] = 0.0
            sm.path_of[3] = 1
            sm.psrtt = [0.02, 0.07]                                    # skew_hat = 0.05
            sm.srtt = 0.07
            sm.on_report(0.08, (0.08, 3, 0, 9, (3,), (), 0, tau, (-1, -1)))
            return sm.copies[3]
        self.assertEqual(fab(True, 0.04), 0)                           # 0 + 0.05 < 0.04 is false: the hole may still be in flight
        self.assertEqual(fab(True, 0.06), 1)                           # a later datagram arrived beyond the skew: lost
        self.assertEqual(fab(False, 0.04), 1)                          # control: without the rule it is resent at once

    def test_8_plant_t_values_and_deny(self):
        P = toy2.plant_T
        c = lambda k: toy2.Cfg(plant_T=(k, 0.4))      # noqa: E731
        self.assertEqual(P(toy2.Cfg(), 0.3), 0.3)
        self.assertAlmostEqual(P(c("STEP"), 0.39), 0.78)
        self.assertAlmostEqual(P(c("STEP"), 0.4), 0.4)
        self.assertAlmostEqual(P(c("STEP"), 0.41), 0.41)
        self.assertAlmostEqual(P(c("SPIKE"), 0.4), 0.8)
        self.assertAlmostEqual(P(c("SPIKE"), 0.396), 0.396)
        self.assertAlmostEqual(P(c("KINK"), 0.396), 0.396 * 1.25)
        self.assertAlmostEqual(P(c("KINK"), 0.392), 0.392 * 1.5)
        self.assertAlmostEqual(P(c("KINK"), 0.4), 0.4)
        self.assertAlmostEqual(P(c("KINK"), 0.404), 0.404)
        for k in ("STEP", "SPIKE", "KINK"):
            self.assertEqual(P(c(k), INF), INF)
            self.assertEqual(P(c(k), 1e6), 1e6)
        self.assertFalse(deny_scan("def plant_T(cfg, T):\n    return 2 * T if T < cfg.plant_p else T\n"))     # exempt by name only
        self.assertTrue(deny_scan("def plant_Tish(cfg, T):\n    return 2 * T if T < 3 else T\n"))
        sim = toy2.Sim(batch3d.s5_cfg("OV", 0, 0.396, 0.0, plant_T=("STEP", 0.4)))
        self.assertAlmostEqual(sim.Te, 0.792)                                   # T_eff enters the laws; the readout keeps the declared T
        self.assertEqual(sim.cfg.T, 0.396)
        with open(os.path.join(HERE, "toy2.py"), encoding="utf-8") as f:
            src = f.read()
        self.assertEqual(deny_scan(src), [])
        self.assertNotIn("isinf", src)

    def test_9_s5v_takes_verdicts_on_real_state(self):
        sim = toy2.Sim(small_s5("OV", 0, 0.4, INF, verdict_evidence="feedback_tail", floor_law="verdict"))
        r = sim.run()
        self.assertGreater(r["verdicts"], 0)
        self.assertGreater(r["gauges"]["floors"], 0)                           # the verdict floor is sent at verdicts, as a datagram
        self.assertEqual(sim.g["floor_carried"], 0)
        self.assertTrue(r["finished"])
        sim = toy2.Sim(small_s5("OV", 0, 0.4, INF))                              # S5 itself judges nothing
        r = sim.run()
        self.assertEqual(r["verdicts"], 0)

    def test_10_windows_on_a_synthetic_run_with_a_known_recovery_time(self):
        dt, n = 0.01, 2000
        ages = np.full(n, 0.1)
        ages[500:1030] = 1.0                                      # late through the outage [5, 8) and until 10.3
        sh = toy2.window_shares(ages, dt, 0.4)
        self.assertAlmostEqual(sh[0], 1.0)                                 # [0, 0.5)
        self.assertAlmostEqual(sh[60], 0.0)                                # [6.0, 6.5)
        self.assertAlmostEqual(sh[102], 0.8)                               # [10.2, 10.7): the last 0.1 s of lateness
        rt, why = toy2.recovery_time(ages, dt, 0.4, 5.0, 8.0)
        self.assertAlmostEqual(rt, 2.3, places=6)
        ages2 = ages.copy()                                                # late to the end: not within the run
        ages2[1030:] = 1.0
        self.assertEqual(toy2.recovery_time(ages2, dt, 0.4, 5.0, 8.0), (None, "not within the run"))
        self.assertIsNone(toy2.recovery_time(ages, dt, 0.4, 0.4859, 1.9859)[0])      # an outage at 0.4859 s: no pre-outage level
        ages3 = ages.copy()                                                # an undelivered unit counts as late
        ages3[1100:1150] = np.inf
        self.assertAlmostEqual(toy2.window_shares(ages3, dt, 0.4)[110], 0.0)
        r = toy2.run(small_s5("CL", 0, 0.4, 0.0, windows=True, n_total=30000, dead=(0, 1.0, 0.5)))   # through the simulation
        self.assertIn("recovery_time", r)
        self.assertEqual(len(r["win_ot04"]), len(toy2.window_shares(np.zeros(30000), 1e-4, 0.4)))

    def test_11_s5v_liveness_gap_and_the_unregistered_supplement(self):
        # a hole whose send stamp equals the newest arrived stamp (sent in the same burst, nothing sent after): no feedback evidence, and
        # it lies below rhi_known, so the registered S5v rule never judges it; the supplement's timeout does
        def fab(ev):
            sim = toy2.Sim(small_s5("OV", 0, 0.4, 0.0, verdict_evidence=ev, floor_law="verdict"))
            sim.n_sent = 10
            for s in range(10):
                sim.h_seq[s] = 0.0
                sim.sent_t[s] = sim.last_send[s] = 40.0
            sim.M_known = {3}
            sim.rhi_known = 9
            sim.tau_rx = 40.0
            sim.last_gen = 41.0
            sim.srtt = 0.1
            sim.act(50.0, 41.0, [3], open_tail=True)
            return 3 in sim.verdicted
        self.assertFalse(fab("feedback_tail"))
        self.assertTrue(fab("feedback_tail_any"))

    def test_rtt_sample_only_from_arrived_seq(self):
        """Amendment to stage 3d: a floor jump past a never-arrived seq raises the receiver's rhi without an arrival;
        the report's RTT sample (rfc, Karn) must not be taken on that seq, while rhi_known still advances."""
        def fab(arrive_top):
            sm = toy2.Sim(small_s5("CL", 0, 0.4, 0.0))
            sm.n_sent = 10
            for s in range(10):
                sm.sent_t[s] = sm.last_send[s] = 0.001 * s
            for s in range(3):
                sm.on_data(0.01 + 0.001 * s, s, 0, sm.sent_t[s])
            if arrive_top:
                sm.on_data(0.02, 9, 0, sm.sent_t[9])
            else:
                sm.apply_floor(0.03, 10, 0.03)                         # the floor passes 3..9 with no arrival
            self.assertEqual(sm.rhi, 9)
            self.assertEqual(bool(sm.got[9]), arrive_top)
            sm.on_report(1.0, (0.5, 3, 0, sm.rhi, (), (), 0, 0.0))
            return sm
        a = fab(False)
        self.assertIsNone(a.srtt)                                      # no sample from a seq that never arrived
        self.assertEqual(a.rhi_known, 9)
        b = fab(True)                                                  # control: the same report on an arrived seq samples
        self.assertAlmostEqual(b.srtt, 1.0 - b.sent_t[9])
        self.assertEqual(b.rhi_known, 9)


# ---- proof stage 1 (three-knobs-proof-plan.md 2.3): the arm "today's Realtime" ------------------------------------
ENGINE_LIKE = dict(sweep_form="blocker", rx_refresh=True, rtt_est="rfc", src_model="wc", resend_gate="cooldown", resend_brake="engine", J=0.001)


def rt_cfg(**kw):
    """Today's Realtime on a short object cell with an outage (no link): the sender S1 (a shed sends no copy), the early-skip receiver R0e,
    the derived budget, the smoothed-RTT deadline, no floor, the 200-symbol window, no source pause."""
    d = dict(cell="I2", sender="S1", recv="R0e", seed=1, n_total=20000, dead=(0, 0.5, 0.3), delta=50.0, rho=1.0, check_p1=False,
             d_arm="EWMA", budget="resid", floor_send=False, evict_window=200, pause_B=10 ** 9, **ENGINE_LIKE)
    d.update(kw)
    return toy2.Cfg(**d)


class ProofS1TodaysRealtime(unittest.TestCase):
    def test_budget_reads_zero_cold_then_the_cells_loss(self):
        sm = toy2.Sim(rt_cfg())
        self.assertEqual(sm.shed_budget, 0.0)                              # no estimate yet: nothing is sheddable
        r = sm.run()
        self.assertEqual(r["gauges"]["seed_verdicts"], 0)                  # nothing was shed before the first RTT sample
        self.assertAlmostEqual(sm.shed_budget, 0.03)                       # eps_hat (1 - P_fec) with r = 0: the cell's first-send loss (Q21)

    def test_p_fec_normal_port_and_its_cut(self):
        self.assertEqual(toy2.p_fec_normal(0.0, 0.03, 100.0, 1.0), 0.0)           # r <= 0 (W22)
        self.assertEqual(toy2.p_fec_normal(0.03, 0.03, 100.0, 1.0), 0.0)          # r (1 - eps) <= eps: the 0 -> 1/2 step
        r, e, w, s2 = 0.2, 0.03, 100.0, 1.0
        z = math.sqrt(w) * (r * (1 - e) - e) / math.sqrt(e * (1 - e) * (r + s2))
        self.assertAlmostEqual(toy2.p_fec_normal(r, e, w, s2), 0.5 * math.erfc(-z / math.sqrt(2.0)), places=12)
        self.assertEqual(toy2.residual_loss(0.03, 0.0, 100.0, 1.0), 0.03)         # repair off: the budget is eps_hat
        self.assertLess(toy2.residual_loss(0.03, 0.2, 100.0, 1.0), 0.03)

    def test_a_hole_past_the_window_is_never_served(self):
        def gaps(window):
            r = toy2.run(rt_cfg(evict_window=window, want_trace=True))
            n, worst = 0, 0
            for (t, s, k, _, kind) in r["_trace"]:
                if kind == 0:
                    n += 1
                elif kind == 1:
                    worst = max(worst, n - s)
            return r, worst
        r, worst = gaps(200)
        self.assertLessEqual(worst, 200)
        self.assertGreater(r["gauges"]["evict_skip"], 0)                   # the window bound something
        r2, worst2 = gaps(math.inf)
        self.assertGreater(worst2, 200)                                    # control: without the window the same cell does serve them
        self.assertEqual(r2["gauges"]["evict_skip"], 0)

    def test_a_shed_sends_nothing(self):
        r = toy2.run(rt_cfg(want_trace=True))
        g = r["gauges"]
        self.assertGreater(g["verdicts"], 0)                               # it shed
        self.assertEqual(g["verdict_copies"], 0)
        self.assertEqual(g["floors"], 0)
        self.assertFalse([e for e in r["_trace"] if e[4] == 2])
        c = toy2.run(rt_cfg(sender="S1c", floor_send=True, want_trace=True))   # control: ADR-0084's sender sends both
        self.assertGreater(c["gauges"]["verdict_copies"], 0)
        self.assertGreater(c["gauges"]["floors"], 0)

    def test_no_source_pause_equals_the_pause_build_where_the_pause_never_binds(self):
        a = toy2.run(rt_cfg(dead=None, n_total=10000, pause_B=10 ** 9, want_trace=True))
        b = toy2.run(rt_cfg(dead=None, n_total=10000, pause_B=2048, want_trace=True))
        self.assertEqual(a["_trace"], b["_trace"])
        self.assertEqual(a["gauges"]["pause_events"], 0)

    def test_no_source_pause_lets_the_queue_drop_instead(self):
        lk = dict(link=dict(C=10000.0, Q=1250.0), rate=12000.0, n_total=36000, dead=None)
        a = toy2.run(rt_cfg(**lk))
        b = toy2.run(rt_cfg(pause_B=64, **lk))
        self.assertEqual(a["pause_events"], 0)
        self.assertGreater(a["queue_drop"], 0)
        self.assertGreater(b["pause_events"], 0)                            # control: a small B pauses the source

    def test_two_paths_duplicate_the_first_send(self):
        lc, d = batch3d.base("P2")
        d.update(n_total=3000, sender="S1")
        kw = dict(cell=lc, recv="R0e", seed=1, delta=50.0, rho=1.0, check_p1=False, d_arm="EWMA", budget="resid", floor_send=False,
                  evict_window=200, pause_B=10 ** 9)
        kw.update({k: v for k, v in d.items() if k not in kw})
        dup = toy2.run(toy2.Cfg(place="dup", **kw))
        self.assertEqual(dup["gauges"]["dup_first"], dup["n_src"])
        self.assertGreater(dup["gauges"]["dup"], 0.5 * dup["n_src"])      # both copies arrive for most units
        one = toy2.run(toy2.Cfg(**kw))
        self.assertEqual(one["gauges"]["dup_first"], 0)

    def test_the_receiver_force_drains_at_its_buffer_cap(self):
        peak = []

        class Probe(toy2.Sim):
            def r0e_arrival(self, now, s):
                super().r0e_arrival(now, s)
                peak.append(len(self.r0e_buf))
        sm = Probe(rt_cfg(r0e_max=20, dead=None, n_total=20000))
        r = sm.run()
        self.assertGreater(r["gauges"]["force_drains"], 0)
        self.assertLessEqual(max(peak), 20)
        sm0 = toy2.Sim(rt_cfg(dead=None, n_total=20000))
        self.assertEqual(sm0.run()["gauges"]["force_drains"], 0)           # control: no cap, no drain

    def test_taken_apart_it_equals_adr_0084s_sender_at_the_derived_allowance(self):
        """budget "resid", EWMA deadline, no copy, no floor, no window, no pause == S1 at rho = 1 - eps_hat, no floor.  The receiver is strict
        (R1) with no floor so that nothing passes and the sender's shed count equals ADR-0084's (passes + pending)."""
        common = dict(recv="R1", evict_window=math.inf, n_total=12000, dead=(0, 0.3, 0.3))
        a = toy2.run(rt_cfg(want_trace=True, **common))
        b = toy2.run(rt_cfg(want_trace=True, budget="rho", rho=1.0 - 0.03, **common))
        self.assertGreater(a["gauges"]["verdicts"], 0)
        self.assertEqual(a["_trace"], b["_trace"])
        for k in ("n_src", "delivered", "share_any", "verdicts", "orig_lost", "stall_total"):
            self.assertEqual(a[k], b[k], k)

    def test_defaults_are_the_old_arm(self):
        c = toy2.Cfg()
        self.assertEqual((c.budget, c.floor_send, c.evict_window, c.place, c.r0e_max), ("rho", True, math.inf, "stripe", math.inf))

    def test_l1_gauge_is_zero_in_the_new_model(self):
        r = toy2.run(small_s5("CL", 0, 0.4, 0.0))
        self.assertEqual(r["gauges"]["l1_viol"], 0)


if __name__ == "__main__":
    unittest.main()
