"""Unit tests for the stage-2 toy (docs/derivations/skip-rule-efficiency.md 7.8).
Run from tools/toy:  python -m unittest test_toy2   (stage 1's test_toy.py must still pass as well)."""
import ast
import dataclasses
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


DIALS = {"rho", "delta", "hint", "T", "Hk", "Te", "r_fec", "evict_window", "r0e_max"}        # Te: stage 3d, T_eff = plant_T(cfg, T), the knob under another name
# HZ, STEP/SPIKE/KINK, R0e (7.3, 7.9); the two stage-3 law functions (three-knobs.md 9.2) and the share_T readout are the only places T is compared
EXEMPT = {"plant_scale", "hz_hold", "r0e_buffer", "on_r0e_expiry", "intake_expired_fn", "resend_admissible", "within_lifetime", "staged_past_lifetime",
          "dequeue_expired",      # dequeue_expired: three-knobs.md 13.2 (stage 3b)
          "protected",            # protected: three-knobs.md 15.3 and 15.6.1 item 1 (stage 3c)
          "plant_T",              # plant_T: three-knobs.md 17.5 (stage 3d), the planted controls routed through T
          "p_fec_normal",         # proof stage 1: the shipped p_fec cut (W22, recorded debt), bounded by test_p_fec_normal_port_and_its_cut
          "evict_unservable", "force_drain_due", "force_drain_keep",       # proof stage 1: today's Realtime branches, modelled as shipped (plan 2.3 and 8)
          # stage 3e (three-knobs-stage3e-build.md 2): the repair laws as shipped and the two lifetime comparisons of the repair span; by name only
          "normal_survival_as", "normal_quantile_as", "p_fec_normal_as", "z_for_tail_target", "r_star_with_z", "r_saturation", "soft_saturate",
          "controller_rate_parts", "derive_window_parts", "bulkness", "t_tail_of", "rate_law", "beta_upper", "span_edge_ok", "decoded_late",
          "saturation_pressure", "rb_hold", "wait_limit_tau", "wait_limit_K", "wait_limit_Kc", "tau_below_t"}


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
    def test_proof_s1_fields_are_covered(self):
        for f in ("evict_window", "r0e_max", "r_fec"):
            self.assertTrue(deny_scan("def f(c, n):\n    return n > c.%s\n" % f), f)
        self.assertFalse(deny_scan("def p_fec_normal(r_fec):\n    return r_fec <= 0\n"))        # the shipped cut, named debt W22

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
        lk = dict(link=dict(C=10000.0, Q=1250.0), rate=12000.0, n_total=12000, dead=None, t_limit=6.0)
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
        (R1) with no floor so that nothing passes.  Every resend is lost (pc = 1), so no shed hole is ever filled: then ADR-0084's
        outstanding count (passes + pending) equals the shipped cumulative shed count.  Where a shed hole is filled later the two differ by design."""
        toy2.CELLS["T-pc1"] = dict(kind="iid", p0=0.03, pc=1.0)
        self.addCleanup(toy2.CELLS.pop, "T-pc1")
        common = dict(cell="T-pc1", recv="R1", evict_window=math.inf, n_total=5000, dead=(0, 0.15, 0.05), t_limit=3.0)
        a = toy2.run(rt_cfg(want_trace=True, **common))
        b = toy2.run(rt_cfg(want_trace=True, budget="rho", rho=1.0 - 0.03, **common))
        self.assertGreater(a["gauges"]["verdicts"], 0)
        self.assertEqual(a["_trace"], b["_trace"])
        for k in ("n_src", "delivered", "share_any", "verdicts", "orig_lost", "stall_total"):
            self.assertEqual(a[k], b[k], k)

    def test_a_silent_shed_of_the_open_tail_leaves_the_object_unfinished(self):
        """Shipped behaviour, pinned and not patched (plan 2.3): with no floor the receiver never learns of a lost tail, so an object
        whose last units are shed does not complete.  A datagram stream that continues past its outage is unaffected."""
        n = 12147
        tail = dict(n_total=n, dead=(n - 347, (n - 347) / 10000.0, 1.5), t_limit=20.0, evict_window=math.inf)
        silent = toy2.run(rt_cfg(**tail))
        self.assertFalse(silent["finished"])
        self.assertGreater(silent["gauges"]["verdicts"], 0)
        loud = toy2.run(rt_cfg(sender="S1c", floor_send=True, **tail))          # control: with ADR-0084's floor the object completes
        self.assertTrue(loud["finished"])

    def test_defaults_are_the_old_arm(self):
        c = toy2.Cfg()
        self.assertEqual((c.budget, c.floor_send, c.evict_window, c.place, c.r0e_max), ("rho", True, math.inf, "stripe", math.inf))

    def test_l1_gauge_is_zero_in_the_new_model(self):
        r = toy2.run(small_s5("CL", 0, 0.4, 0.0))
        self.assertEqual(r["gauges"]["l1_viol"], 0)


# ---- stage 3e (three-knobs-stage3e-build.md 7): repair, the decoder, the span edges, the estimated-loss arm -------------
import ident3e_cfgs  # noqa: E402

RATE_REF = os.path.join(HERE, "rate_ref3e.json")


def e_cfg(cell="K-L10", seed=0, T=0.12, H=0.0, repair=None, **kw):
    """The new model (S5) on one of the batch cells, with the stage 3e fields."""
    return batch3d.s5_cfg(cell, seed, T, H, repair=repair, **kw)


def rt_e(seed=0, T=0.12, repair=None, **kw):
    """RT: K-L10 without its outage, short."""
    kw.setdefault("dead", None)
    kw.setdefault("n_total", 600)
    return e_cfg("K-L10", seed, T, 0.0, repair, **kw)


def id3e_t(a, b):
    import batch3e
    return batch3e.id3e(a, b)


class SpanSim(toy2.Sim):
    """A sender with a pinned rate_hat and a recording forward path, to call stage_repair on constructed state."""

    def __init__(self, cfg, rh=1000.0):
        super().__init__(cfg)
        self._rh = rh
        self.fwd_calls = []

    def rate_hat(self, now):
        return self._rh

    def fwd(self, t, path=0):
        self.fwd_calls.append(t)
        return t + 0.005, t


def span_sim(edge="worth", T=0.4, n_sent=500, srtt=0.05, **kw):
    """S5 with a repair arm, n_sent first sends at 1 ms spacing (h(s) = s ms), srtt as given: o_hat = srtt - RTprop/2 = srtt - 0.005,
    D = srtt/2 so A* = ceil(1000 srtt / 2), Delta = 1."""
    rep = kw.pop("repair", dict(r=0.3, edge=edge))
    c = toy2.Cfg(cell="I2", sender="S5", recv="RH", seed=1, rate=1000.0, n_total=3000, delta=50.0, rho=1.0, check_p1=False, T=T, Hk=0.0,
                 rtt_est="rfc", trigger_law="trigger", owd_est="fwd", floor_law="protect", repair=rep, **kw)
    sm = SpanSim(c)
    sm.n_sent = n_sent
    for s in range(n_sent):
        sm.h_seq[s] = s * 0.001
    sm.srtt = srtt
    sm.W_cur = 200.0
    return sm


class Stage3e(unittest.TestCase):
    # -- 1 defaults ---------------------------------------------------------------------------------------------
    def test_1_defaults_and_old_senders_reproduce_the_base(self):
        c = toy2.Cfg()
        self.assertIsNone(c.repair)
        self.assertIsNone(c.repair_seam)
        self.assertEqual((c.o_bias, c.R_bias, c.rev_q, c.tau_extra), (0.0, 0.0, 0.0, ()))
        with open(os.path.join(HERE, "ident3e_ref.json")) as f:
            ref = json.load(f)
        built = ident3e_cfgs.cfgs(toy2, batch3d, batch3c)
        self.assertEqual(sorted(built), sorted(ref))
        for nm, cf in built.items():
            d = ident3e_cfgs.digest(toy2.run(cf))
            for k, v in ref[nm].items():
                if k == "gauges":
                    for g, x in v.items():
                        self.assertEqual(d["gauges"][g], x, (nm, "gauge", g))
                else:
                    self.assertEqual(d[k], v, (nm, k))

    # -- 2 the rate law against raptorpath-math ---------------------------------------------------------------
    def test_2_rate_law_matches_raptorpath_math(self):
        with open(RATE_REF) as f:
            ref = json.load(f)
        self.assertGreaterEqual(len(ref["grid"]), 500)
        for e in ref["grid"]:
            L = toy2.rate_law(e["eps"], e["srtt"], e["rate"], e["delta"])
            self.assertEqual(L["W"], e["W"], e)
            self.assertAlmostEqual(L["r"], e["r"], delta=1e-9, msg=str(e))
            self.assertAlmostEqual(L["beta"], e["beta"], delta=1e-12)
            self.assertAlmostEqual(min(max(L["W_pre"], 16.0), 512.0), e["W_clamped"], delta=1e-9, msg=str(e))
            self.assertAlmostEqual(L["r_sat"], e["r_sat"], delta=1e-9, msg=str(e))

    def test_2_registration_table(self):
        import check_rate3e
        for eps, srtt, rate, W, r in check_rate3e.TABLE:
            L = toy2.rate_law(eps, srtt, rate, 50.0)
            self.assertEqual(L["W"], W, (eps, srtt, rate))
            self.assertAlmostEqual(L["r"], r, delta=1e-5, msg=str((eps, srtt, rate)))
        self.assertEqual(check_rate3e.check(), [])
        self.assertAlmostEqual(toy2.rate_law(0.03, 0.0115, 10000.0, 50.0)["r_sat"], 0.135, delta=1e-9)
        self.assertAlmostEqual(toy2.rate_law(0.10, 0.101, 50.0, 50.0)["r_sat"], 0.385, delta=1e-9)

    def test_2_delta_continuity_at_the_call_point(self):
        r0 = toy2.rate_law(0.10, 0.101, 50.0, 50.0)["r"]
        for x in (0.98, 0.99, 1.01, 1.02):
            self.assertAlmostEqual(toy2.rate_law(0.10, 0.101, 50.0, 50.0 * x)["r"], r0, delta=3e-5)

    # -- 3 the A&S ports ------------------------------------------------------------------------------------------
    def test_3_as_ports_match_the_reference(self):
        with open(RATE_REF) as f:
            ref = json.load(f)
        self.assertEqual(len(ref["survival"]), 8)
        for z, v in ref["survival"]:
            self.assertAlmostEqual(toy2.normal_survival_as(z), v, delta=1e-12, msg=str(z))
        self.assertEqual(len(ref["quantile"]), 6)
        for p, v in ref["quantile"]:
            self.assertAlmostEqual(toy2.normal_quantile_as(p), v, delta=1e-9, msg=str(p))
        for r, e, w, s2, v in ref["p_fec"]:
            self.assertAlmostEqual(toy2.p_fec_normal_as(r, e, w, s2), v, delta=1e-12, msg=str((r, e, w)))
        self.assertAlmostEqual(toy2.p_fec_normal_as(0.2, 0.03, 100.0, 1.0), toy2.p_fec_normal(0.2, 0.03, 100.0, 1.0), delta=2e-7)   # the declared erfc difference

    # -- 4 delta = 0.005 gives r = 0 exactly ----------------------------------------------------------------------
    def test_4_bulk_delta_gives_r_zero_and_rl_equals_off(self):
        self.assertEqual(toy2.rate_law(0.03, 0.0115, 10000.0, 0.005)["r"], 0.0)
        for seed in range(3):
            def mk(rp):
                c = e_cfg("CL@b", seed, 0.4, 0.0, repair=rp, n_total=3000, dead=(0, 0.1, 0.1))
                c.delta = 0.005
                c.want_trace = True
                return c
            a = toy2.run(mk(None))
            b = toy2.run(mk(dict(r="law", edge="worth")))
            self.assertEqual(id3e_t(a, b), [], seed)
            self.assertEqual(a["_trace"], b["_trace"], seed)
            self.assertEqual(b["gauges"]["rep_credits"], 0)
            self.assertGreater(b["gauges"]["law_reads"], 0)                         # control: the law was read

    # -- 5 the span on constructed state -----------------------------------------------------------------------
    def test_5_span_no_bind(self):
        sm = span_sim("worth")
        sm.stage_repair(0.5005)
        self.assertEqual(sm.astar_cur, 25)
        self.assertEqual(sm.g["rep_members_sum"], 25)                              # [474, 499)
        self.assertEqual((sm.g["rep_sent"], sm.g["rep_cut_edge"], sm.g["l1_viol_rep"], sm.g["worth_viol_rep"]), (1, 0, 0, 0))

    def test_5_span_worth_cut(self):
        sm = span_sim("worth", T=0.07)                                             # age + 0.045 < 0.07: age < 0.025, h > 0.4755: s >= 476
        sm.stage_repair(0.5005)
        self.assertEqual((sm.g["rep_members_sum"], sm.g["rep_cut_edge"]), (499 - 476, 1))
        self.assertEqual(sm.g["worth_viol_rep"], 0)

    def test_5_span_age_cut(self):
        sm = span_sim("age", T=0.02)                                               # age < 0.02: h > 0.4805: s >= 481
        sm.stage_repair(0.5005)
        self.assertEqual((sm.g["rep_members_sum"], sm.g["rep_cut_edge"]), (499 - 481, 1))
        self.assertGreater(sm.g["worth_viol_rep"], 0)                               # the age edge carries members the worth test refuses

    def test_5_span_floor_cut(self):
        sm = span_sim("floor")
        sm.F_ann = 490
        sm.stage_repair(0.5005)
        self.assertEqual((sm.g["rep_members_sum"], sm.g["rep_cut_edge"]), (499 - 490, 1))

    def test_5_span_evict_cut(self):
        sm = span_sim("evict", evict_window=20)
        sm.verdicted = {480, 481}
        sm.stage_repair(0.5005)                                                    # n_sent - lo = 26 > 20: lo = 480, then past the sheds: 482
        self.assertEqual((sm.g["rep_members_sum"], sm.g["rep_cut_edge"]), (499 - 482, 1))
        sm2 = span_sim("evict", evict_window=math.inf)
        sm2.verdicted = {474}
        sm2.stage_repair(0.5005)
        self.assertEqual(sm2.g["rep_members_sum"], 499 - 475)                      # an infinite window never binds; the shed seq at the low end does

    def test_5_empty_span_counted_by_cause(self):
        young = span_sim("worth", n_sent=1)
        young.stage_repair(0.0005)
        self.assertEqual((young.g["rep_drop_young"], young.g["rep_drop_edge"], young.g["rep_sent"]), (1, 0, 0))
        edge = span_sim("worth", T=0.001)
        edge.stage_repair(0.5005)
        self.assertEqual((edge.g["rep_drop_edge"], edge.g["rep_drop_young"], edge.g["rep_sent"]), (1, 0, 0))
        self.assertEqual(edge.fwd_calls, [])                                       # nothing was staged, no draw taken

    def test_5_astar_is_one_before_the_first_rtt_sample(self):
        sm = span_sim("worth")
        sm.srtt = None
        sm.stage_repair(0.5005)
        self.assertEqual((sm.astar_cur, sm.g["rep_members_sum"]), (1, 1))
        self.assertGreaterEqual(sm.g["astar_floor_bind"], 1)

    def test_5_span_never_starts_below_the_encoder_window(self):
        sm = span_sim("worth")
        sm.A_known = 500
        sm.W_cur = 25.0                                                            # A* = 25 = W, ws = 475 > end - A* = 474
        sm.stage_repair(0.5005)
        self.assertEqual(sm.g["rep_members_sum"], 499 - 475)

    # -- 6 the L1 control ----------------------------------------------------------------------------------------
    def test_6_l1_control(self):
        rp = dict(r=0.3835, edge="worth")
        broken = toy2.run(rt_e(T=0.04, repair=rp, repair_seam="no_edge"))
        self.assertGreater(broken["gauges"]["l1_viol_rep"], 0)
        fixed = toy2.run(rt_e(T=0.04, repair=rp))
        self.assertEqual(fixed["gauges"]["l1_viol_rep"], 0)

    # -- 7 the worth gauge --------------------------------------------------------------------------------------
    def test_7_worth_gauge(self):
        def run_ov(edge):
            c = e_cfg("OV", 0, 0.03, 0.0, repair=dict(r=0.3, edge=edge), n_total=3000)
            return toy2.run(c)
        self.assertGreater(run_ov("age")["gauges"]["worth_viol_rep"], 0)           # the age edge carries members that cannot arrive in time
        self.assertEqual(run_ov("worth")["gauges"]["worth_viol_rep"], 0)

    # -- 8 the decoder on constructed systems ---------------------------------------------------------------
    @staticmethod
    def rx(missing, n=12, T=0.4, **kw):
        """A receiver that has seen every seq below n except `missing`, with the holes open and nothing released."""
        c = toy2.Cfg(cell="I2", sender="S5", recv="RH", seed=1, n_total=max(100, n), delta=50.0, rho=1.0, check_p1=False, T=T, Hk=math.inf,
                     repair=dict(r=0.3, edge="worth"), **kw)
        sm = toy2.Sim(c)
        sm.n_sent = n
        for s in range(n):
            sm.h_seq[s] = s * 0.01
        for s in range(n):
            if s not in missing:
                sm.on_data(s * 0.01 + 0.005, s, 0, s * 0.01)
        return sm

    def test_8_peeling(self):
        sm = self.rx({5})
        sm.on_repair(0.2, (0, 5, 6, 0.19))
        g = sm.g
        self.assertEqual((g["dec_units"], g["rep_used"], g["rep_innov"], g["dup"]), (1, 1, 1, 0))
        self.assertTrue(sm.got[5])
        self.assertGreaterEqual(sm.dlv[5], 0)
        self.assertEqual(sm.dec_flag[5], 1)

    def test_8_two_by_two_block(self):
        sm = self.rx({3, 4})
        sm.on_repair(0.2, (0, 3, 5, 0.19))
        self.assertEqual(sm.g["dec_units"], 0)                                     # one equation, two unknowns
        sm.on_repair(0.21, (1, 3, 5, 0.2))
        self.assertEqual((sm.g["dec_units"], sm.g["rep_used"], sm.g["dup"]), (2, 2, 0))        # test 17 as well
        self.assertTrue(sm.got[3] and sm.got[4])

    def test_8_over_determined_block_decodes_only_what_it_determines(self):
        sm = self.rx({3, 4, 5, 6})
        sm.on_repair(0.2, (0, 3, 5, 0.19))                                         # {h1,h2}
        sm.on_repair(0.21, (1, 3, 7, 0.2))                                         # {h1,h2,h3,h4}
        self.assertEqual(sm.g["dec_units"], 0)
        sm.on_repair(0.22, (2, 3, 5, 0.21))                                        # {h1,h2} again
        self.assertEqual(sm.g["dec_units"], 2)
        self.assertTrue(sm.got[3] and sm.got[4] and not sm.got[5] and not sm.got[6])

    def test_8_chain_decodes_nothing(self):
        sm = self.rx({3, 4, 5})
        sm.on_repair(0.2, (0, 3, 5, 0.19))                                         # {h1,h2}
        sm.on_repair(0.21, (1, 4, 6, 0.2))                                         # {h2,h3}
        self.assertEqual(sm.g["dec_units"], 0)
        self.assertFalse(sm.got[3] or sm.got[4] or sm.got[5])

    def test_8_redundant_equation(self):
        sm = self.rx({5})
        sm.on_repair(0.2, (0, 0, 3, 0.19))                                         # every member already held
        self.assertEqual((sm.g["rep_redundant"], sm.g["rep_innov"], sm.g["dec_units"]), (1, 0, 0))

    def test_8_cascade_through_an_older_equation_on_a_later_arrival(self):
        sm = self.rx({3, 4})
        sm.on_repair(0.2, (0, 3, 5, 0.19))                                         # {h3,h4}: undetermined
        self.assertEqual(sm.g["dec_units"], 0)
        sm.on_data(0.25, 3, 0, 0.03)                                               # h3 arrives as data: the equation now determines h4
        self.assertEqual(sm.g["dec_units"], 1)
        self.assertTrue(sm.got[4])
        self.assertEqual(sm.dec_flag[4], 1)
        self.assertEqual(sm.g["dup"], 0)

    def test_8_decode_after_a_release(self):
        sm = self.rx({5})
        sm.on_release(0.2, 5)
        self.assertIn(5, sm.released)
        sm.on_repair(0.21, (0, 5, 6, 0.2))
        self.assertEqual((sm.g["dec_units"], sm.g["dec_after_release"]), (1, 1))

    def test_8_decode_after_a_floor_pass(self):
        sm = self.rx({5})
        sm.apply_floor(0.2, 6, 0.2)
        self.assertIn(5, sm.passed)
        sm.on_repair(0.21, (0, 5, 6, 0.2))
        self.assertEqual((sm.g["dec_units"], sm.g["late_floor_dec"], sm.g["late_floor"]), (1, 1, 1))

    def test_8_decode_after_the_lifetime(self):
        sm = self.rx({5}, T=0.4)
        sm.on_repair(1.0, (0, 5, 6, 0.99))                                         # age 0.95 > T
        self.assertEqual((sm.g["dec_late"], sm.g["rep_late_only"], sm.g["dec_on_time"]), (1, 1, 0))
        ok = self.rx({5}, T=0.4)
        ok.on_repair(0.2, (0, 5, 6, 0.19))                                         # age 0.15 <= T
        self.assertEqual((ok.g["dec_late"], ok.g["rep_late_only"], ok.g["dec_on_time"]), (0, 0, 1))

    @staticmethod
    def determined_ref(adj_of, X, E):
        """The brief's recursive form of the matching, for the differential test."""
        adj = {x: sorted(f for f in adj_of[x] if f in E) for x in X}
        meq, mx = {}, {}

        def aug(x, seen):
            for f in adj[x]:
                if f in seen:
                    continue
                seen.add(f)
                if f not in meq or aug(meq[f], seen):
                    meq[f] = x
                    mx[x] = f
                    return True
            return False
        for x in sorted(X):
            aug(x, set())
        bad = {x for x in X if x not in mx}
        st = list(bad)
        while st:
            x = st.pop()
            for f in adj[x]:
                y = meq.get(f)
                if y is not None and y not in bad:
                    bad.add(y)
                    st.append(y)
        return sorted(X - bad), meq

    def test_8_the_matching_matches_the_recursive_form_and_survives_a_deep_chain(self):
        rnd = random.Random(3)
        sm = self.rx(set(), n=20)
        for _ in range(300):
            nx, ne = rnd.randint(1, 9), rnd.randint(1, 9)
            X = set(range(nx))
            eq_of = {x: set() for x in X}
            for e in range(ne):
                for x in rnd.sample(sorted(X), rnd.randint(1, nx)):
                    eq_of[x].add(e)
            E = set(range(ne))
            sm.eq_of = {x: v for x, v in eq_of.items() if v}
            X2 = set(sm.eq_of)
            self.assertEqual(sm.determined(E, X2), self.determined_ref(eq_of, X2, E))
        n = 1500                                                                   # unknown k in equations k - 1 and k: augmenting paths of length k
        sm.eq_of = {k: {e for e in (k - 1, k) if 0 <= e < n} for k in range(n + 1)}
        dec, _ = sm.determined(set(range(n)), set(range(n + 1)))                   # beyond Python's recursion limit
        self.assertEqual(dec, [])

    # -- 9 emission ------------------------------------------------------------------------------------------------
    def test_9_emission_accounting(self):
        r = 0.2
        sm = toy2.Sim(rt_e(T=0.4, repair=dict(r=r, edge="worth")))
        out = sm.run()
        g = out["gauges"]
        n = out["n_src"]
        self.assertIn(g["rep_credits"], (math.floor(n * r), math.floor(n * r) - 1))
        self.assertEqual(g["rep_sent"] + g["rep_drop_edge"] + g["rep_drop_young"], g["rep_credits"])
        self.assertLess(g["owed_max"], 1.0 + r)
        self.assertGreater(g["rep_sent"], 0)

    def test_9_none_while_paused(self):
        calls = []

        class P(toy2.Sim):
            def stage_repair(self, now):
                calls.append(self.paused)
                super().stage_repair(now)
        c = e_cfg("OV", 0, 0.4, 0.0, repair=dict(r=0.2, edge="worth"), n_total=6000, pause_B=60)
        out = P(c).run()
        self.assertGreater(out["pause_events"], 0)
        self.assertGreater(len(calls), 0)
        self.assertFalse(any(calls))

    def test_9_r_zero_pushes_nothing(self):
        a = toy2.Sim(rt_e(repair=None))
        b = toy2.Sim(rt_e(repair=dict(r=0.0, edge="worth")))
        a.run()
        ob = b.run()
        self.assertEqual((a.ctr, a.ps_ctr), (b.ctr, b.ps_ctr))
        self.assertEqual(ob["gauges"]["rep_credits"], 0)
        self.assertEqual(ob["gauges"]["owed_max"], 0.0)

    # -- 10 the loss draw ---------------------------------------------------------------------------------------
    def test_10_loss_draw_uses_salt_repair_and_p0(self):
        self.assertEqual(toy2.SALT_REPAIR, 7)
        sm = span_sim("worth")
        n = 200
        for i in range(n):
            sm.stage_repair(0.5005 + 1e-6 * i)
        kept = sorted(pl[0] for (_, _, k, pl) in sm.heap if k == 11)
        want = [j for j in range(n) if not toy2.u01(sm.cfg.seed, j, 0, 0, toy2.SALT_REPAIR) < sm.p0]
        self.assertEqual(kept, want)
        self.assertEqual(sm.g["rep_lost"], n - len(want))
        self.assertGreater(sm.g["rep_lost"], 0)

    def test_10_the_outage_drops_repairs(self):
        sm = span_sim("worth", dead=(0, 0.4, 0.2))                                 # the window [0.4, 0.6) covers dep
        for i in range(20):
            sm.stage_repair(0.5005 + 1e-6 * i)
        self.assertEqual(sm.g["rep_lost"], 20)
        self.assertEqual([1 for (_, _, k, _) in sm.heap if k == 11], [])

    # -- 11 the estimated loss -----------------------------------------------------------------------------------
    def test_11_beta_upper_matches_estimator_rs(self):
        def beta_quantile(a, b, p):                                                # raptorpath-math/src/estimator.rs, transcribed
            mean = a / (a + b)
            var = (a * b) / ((a + b) ** 2 * (a + b + 1.0))
            return min(max(mean + toy2.normal_quantile_as(p) * math.sqrt(var), 0.0), 1.0)
        rnd = random.Random(7)
        for _ in range(20):
            a, b = rnd.uniform(0.5, 5000.0), rnd.uniform(0.5, 800.0)               # a received, b lost
            self.assertAlmostEqual(toy2.beta_upper(a, b), beta_quantile(b, a, 0.95), delta=1e-12)

    def test_11_cold_value_and_a_burst_of_new_holes(self):
        sm = toy2.Sim(rt_e(repair=dict(r="est", edge="worth")))
        self.assertEqual(sm.eps_est, toy2.beta_upper(1.0, 1.0))
        rep = (0.1, 0, 0, -1, (), (), 0, 0.0, None, None, 4000, 1000, 0.0)
        sm.on_report(0.2, rep)
        self.assertGreater(sm.eps_est, 0.15)
        self.assertAlmostEqual(sm.beta_a, 0.995 + 4000)
        self.assertAlmostEqual(sm.beta_b, 0.995 + 1000)

    def test_11_a_dequeue_expiry_is_not_a_fate(self):
        tot = [0, 0]

        class P(toy2.Sim):
            def on_report(self, now, rep):
                tot[0] += rep[10]
                tot[1] += rep[11]
                super().on_report(now, rep)
        c = e_cfg("OV", 0, 0.1, 0.0, repair=dict(r="est", edge="worth"), n_total=24000)
        sm = P(c)
        out = sm.run()
        self.assertGreater(out["intake_expired_deq"] + out["intake_expired"], 0)
        self.assertLess(out["n_src"], out["units"])
        self.assertLessEqual(tot[0] + tot[1] + sm.raw_rcv + sm.raw_lost, out["n_src"])    # expired units get no seq, so they are in no batch

    # -- 12 today's Realtime with repair ------------------------------------------------------------------------
    def test_12_budget_with_repair(self):
        seen = []

        class P(toy2.Sim):
            def refresh_shed_budget(self):
                super().refresh_shed_budget()
                if self.srtt is not None:
                    rp = self.cfg.repair
                    eps = self.eps_est if rp["r"] == "est" else self.p0
                    seen.append((self.shed_budget, toy2.residual_loss(eps, self.r_cur, float(self.astar_cur), 1.0), self.p0))
        sm = P(rt_cfg(repair=dict(r="law", edge="evict"), dead=(0, 0.5, 0.3)))
        sm.run()
        self.assertTrue(seen)
        self.assertTrue(all(a == b for a, b, _ in seen))
        self.assertLess(min(a for a, _, _ in seen), seen[0][2])                    # repair lowers the budget below p0
        off = toy2.Sim(rt_cfg())
        off.run()
        self.assertAlmostEqual(off.shed_budget, 0.03)                              # repair off: eps_hat, as proof-s1

    # -- 13 RTT sampling -------------------------------------------------------------------------------------------
    def test_13_rtt_sampling_ignores_a_decoded_seq(self):
        def sample(dec):
            sm = toy2.Sim(rt_e(repair=dict(r=0.0, edge="worth")))
            sm.n_sent = 10
            for s in range(10):
                sm.sent_t[s] = s * 0.02
            sm.got[7] = 1
            sm.arr[7] = 0.2
            sm.dec_flag[7] = 1 if dec else 0
            sm.on_report(0.3, (0.25, 0, 0, 7, (), (), 0, 0.0, None, None, 0, 0, 0.0))
            return sm.srtt
        self.assertIsNone(sample(True))
        self.assertAlmostEqual(sample(False), 0.3 - 7 * 0.02)

    # -- 14 T = inf identity under repair -----------------------------------------------------------------------
    def test_14_t_inf_identity_under_repair(self):
        for cell, kw in (("OV", dict(n_total=3000)), ("CL", dict(n_total=3000, dead=(0, 0.1, 0.1)))):
            for seed in range(3):
                a = e_cfg(cell, seed, INF, INF, repair=dict(r="law", edge="worth"), **kw)
                lc, d = batch3d.base(cell, **kw)
                b = toy2.Cfg(cell=lc, sender="S1c", recv="R1", seed=seed, delta=50.0, rho=1.0, repair=dict(r="law", edge="none"), **d)
                a.want_trace = b.want_trace = True
                ra, rb = toy2.run(a), toy2.run(b)
                self.assertEqual(id3e_t(rb, ra), [], (cell, seed))
                self.assertEqual(ra["_trace"], rb["_trace"], (cell, seed))
                self.assertGreater(ra["gauges"]["rep_sent"], 0)

    # -- 15 robustness hooks --------------------------------------------------------------------------------------
    def test_15_o_bias_and_r_bias(self):
        a = toy2.Sim(rt_e(repair=dict(r=0.0, edge="worth")))
        b = toy2.Sim(rt_e(repair=dict(r=0.0, edge="worth"), o_bias=0.01))
        for sm in (a, b):
            sm.srtt = 0.1
        self.assertAlmostEqual(b.o_hat() - a.o_hat(), 0.01, delta=1e-12)
        self.assertEqual(a.R_now(), b.R_now())

        def deq_m(rb):
            seen = []
            orig = toy2.dequeue_expired
            toy2.dequeue_expired = lambda T, age, m: (seen.append(m), False)[1]
            try:
                c = e_cfg("OV", 0, 0.4, 0.0, repair=dict(r=0.0, edge="worth"), n_total=3000, R_bias=rb)
                sm = toy2.Sim(c)
                sm.emit(0.5)                                                       # a backlog: unit 1 is already due at t = 0.5
            finally:
                toy2.dequeue_expired = orig
            return seen[0], sm.R_now()
        m0, R0 = deq_m(0.0)
        m1, R1 = deq_m(0.01)
        self.assertAlmostEqual(m1 - m0, 0.01, delta=1e-12)
        self.assertEqual(R0, R1)

    def test_15_rev_q_raises_the_mean_srtt(self):
        def srtt_mean(rq):
            c = e_cfg("CL", 0, 0.4, 0.0, n_total=10000, dead=None, rev_q=rq)
            return toy2.run(c)["srtt_mean"]
        self.assertAlmostEqual(srtt_mean(0.1) - srtt_mean(0.0), 0.1, delta=0.01)

    def test_15_rel_owd(self):
        def o_value(rq, mode):
            c = e_cfg("K-I2", 0, 0.4, 0.0, repair=dict(r=0.0, edge="worth"), dead=None, n_total=600, J=0.0, rev_q=rq, owd_est=mode)
            sm = toy2.Sim(c)
            sm.run()
            return sm.o_hat()
        fwd0, rel0 = o_value(0.0, "fwd"), o_value(0.0, "rel")
        self.assertAlmostEqual(rel0, 0.005, delta=1e-9)                            # the one-way delay: no queue anywhere
        self.assertAlmostEqual(fwd0, rel0, delta=0.002)                            # srtt carries up to one report tick of waiting (reported: not 1e-9)
        fwd1, rel1 = o_value(0.1, "fwd"), o_value(0.1, "rel")
        self.assertAlmostEqual(rel1, 0.005, delta=0.001)
        self.assertGreater(fwd1 - rel1, 0.09)                                      # the reverse queue reads as forward delay

    # -- 16 the deny test ---------------------------------------------------------------------------------------------
    def test_16_deny_test_with_the_new_names(self):
        for nm in ("rate_law", "span_edge_ok", "decoded_late", "beta_upper", "p_fec_normal_as", "bulkness", "controller_rate_parts"):
            self.assertIn(nm, EXEMPT)
        with open(os.path.join(HERE, "toy2.py"), encoding="utf-8") as f:
            src = f.read()
        self.assertEqual(deny_scan(src), [])
        self.assertIn("def span_edge_ok", src)
        self.assertFalse(deny_scan("def span_edge_ok(edge, T, age, o):\n    return age + o < T\n"))
        self.assertFalse(deny_scan("def decoded_late(T, age):\n    return age > T\n"))
        self.assertTrue(deny_scan("def stage(c, age):\n    return age > c.T\n"))                     # control: the same comparison elsewhere is flagged
        self.assertTrue(deny_scan("def decode(T, age):\n    return age > T\n"))
        self.assertTrue(deny_scan("def stage(c):\n    return c.delta < 1\n"))

    # -- 17 the decoder counts each unit once ---------------------------------------------------------------------------
    def test_17_each_unit_once(self):
        sm = self.rx({3, 4, 5})
        sm.on_repair(0.2, (0, 3, 5, 0.19))                                         # {3,4}
        sm.on_repair(0.21, (1, 4, 6, 0.2))                                         # {4,5}
        sm.on_repair(0.22, (2, 5, 6, 0.21))                                        # {5}: decodes 5, then 4 through (1), then 3 through (0)
        self.assertEqual(sm.g["dec_units"], 3)
        self.assertEqual(sm.g["dup"], 0)
        self.assertTrue(all(sm.dlv[s] >= 0 for s in (3, 4, 5)))
        self.assertEqual(sm.g["rep_used"], 3)
        self.assertEqual(len(sm.eqs), 0)

    # -- readouts ----------------------------------------------------------------------------------------------------
    def test_rep3e_block_exists_with_repair_off_and_survives_json(self):
        out = toy2.run(rt_e(repair=None, tau_extra=(0.12,), want_trace=True))
        self.assertIn("rep3e", out)
        r3 = out["rep3e"]
        for k in ("r_mean", "W_mean", "sat_pressure", "W_floor_bind_frac", "W_cap_bind_frac", "eps_est_mean", "p_dec_meas", "p_fec_law",
                  "rep_link_share", "rep_members_mean", "recovery_time_0p15", "mu_eff_hat", "share_by"):
            self.assertIn(k, r3)
        self.assertEqual(r3["r_mean"], 0.0)
        back = json.loads(json.dumps(r3))
        self.assertAlmostEqual(back["share_by"]["0.12"], r3["share_by"][0.12])
        n = out["n_src"]
        dl = np.asarray(out["_dlv"][:n])
        age = dl - np.asarray(out["_h"][:n])
        self.assertAlmostEqual(r3["share_by"][0.12], float(((dl >= 0) & (age <= 0.12)).sum()) / out["units"], places=12)

    def test_rep3e_with_repair_reads_the_law(self):
        out = toy2.run(rt_e(repair=dict(r="law", edge="worth")))
        r3 = out["rep3e"]
        self.assertAlmostEqual(r3["r_mean"], 0.3835, delta=0.01)
        self.assertGreater(r3["W_floor_bind_frac"], 0.9)                           # all but the readings before the first RTT sample
        self.assertGreater(r3["sat_pressure"], 0.5)
        self.assertGreater(r3["p_dec_meas"], 0.3)

    def test_recovery_time_at_0p15_on_a_synthetic_run(self):
        dt = 0.001
        ages = np.full(20000, 0.01)
        ages[3000:5000] = 0.2                                                      # late for 0.15 s but within 0.4 s, from 3.0 s to 5.0 s
        r4 = toy2.recovery_time(ages, dt, 0.4, 2.0, 3.0)
        r15 = toy2.recovery_time(ages, dt, 0.15, 2.0, 3.0)
        self.assertEqual(r4[1], "ok")
        self.assertAlmostEqual(r4[0], 0.0, delta=0.11)
        self.assertEqual(r15[1], "ok")
        self.assertAlmostEqual(r15[0], 2.0, delta=0.6)


class Stage3eBracket(unittest.TestCase):
    """Arm RB (amendment 3): r held at the law's cap r_max from the outage start until rb_A after its end, the law outside."""

    def test_default_and_the_window(self):
        c = toy2.Cfg()
        self.assertEqual((c.rb_A, c.rb_unit), (3.0, "s"))

    def test_r_is_the_cap_inside_the_window_and_the_law_outside(self):
        seen = []

        class P(toy2.Sim):
            def read_law(self, now):
                before = self.t_eval
                super().read_law(now)
                if self.t_eval == now and self.law_last is not None:
                    seen.append((now, self.r_cur, self.law_last["r"]))
        c = e_cfg("CL", 0, 0.4, 0.0, repair=dict(r="rb", edge="worth"), n_total=30000, dead=(0, 0.5, 0.5), rb_A=0.5)
        P(c).run()
        inside = [x for x in seen if 0.5 <= x[0] < 1.5]
        outside = [x for x in seen if x[0] < 0.5 or x[0] >= 1.5]
        self.assertTrue(inside and outside)
        self.assertTrue(all(r == toy2.R_MAX for _, r, _ in inside))
        self.assertTrue(all(r == law for _, r, law in outside))
        self.assertTrue(any(law < toy2.R_MAX for _, _, law in inside))                      # control: the law itself was below the cap there

    def test_the_symbol_unit_counts_first_sends_after_the_outage(self):
        seen = []

        class P(toy2.Sim):
            def read_law(self, now):
                super().read_law(now)
                if self.t_eval == now and self.law_last is not None:
                    seen.append((now, self.r_cur, self.law_last["r"], self.fs_after_outage))
        c = e_cfg("CL", 0, 0.4, 0.0, repair=dict(r="rb", edge="worth"), n_total=30000, dead=(0, 0.5, 0.5), rb_A=2000.0, rb_unit="sym")
        P(c).run()
        held = [x for x in seen if x[0] >= 1.0 and x[3] < 2000]
        free = [x for x in seen if x[0] >= 1.0 and x[3] >= 2000]
        self.assertTrue(held and free)
        self.assertTrue(all(r == toy2.R_MAX for _, r, _, _ in held))
        self.assertTrue(all(r == law for _, r, law, _ in free))
        self.assertTrue(all(x[3] == 0 for x in seen if x[0] < 1.0))                           # nothing counted before the outage end

    def test_an_empty_window_equals_rl(self):
        for seed in range(2):
            a = e_cfg("K-L10", seed, 0.12, 0.0, repair=dict(r="law", edge="worth"), dead=None, n_total=600)
            b = e_cfg("K-L10", seed, 0.12, 0.0, repair=dict(r="rb", edge="worth"), dead=None, n_total=600)
            a.want_trace = b.want_trace = True
            ra, rb = toy2.run(a), toy2.run(b)
            self.assertEqual(id3e_t(ra, rb), [])
            self.assertEqual(ra["_trace"], rb["_trace"])
            self.assertEqual(ra["gauges"]["rep_sent"], rb["gauges"]["rep_sent"])


class Stage3eDecodedByCause(unittest.TestCase):
    """Gauges that split the decoded units by why their first send was missing (read-only: no behaviour changes)."""

    def test_a_decoded_unit_whose_first_send_the_link_queue_dropped_is_counted(self):
        sm = Stage3e.rx({5, 6})
        sm.qd_first[5] = 1                                                                     # the link queue dropped seq 5's first send
        sm.on_repair(0.2, (0, 5, 6, 0.19))
        sm.on_repair(0.21, (1, 6, 7, 0.2))
        self.assertEqual(sm.g["dec_units"], 2)
        self.assertEqual(sm.g["dec_units_qdrop"], 1)

    def test_the_queue_drop_of_a_first_send_is_counted_once_per_unit_on_an_overloaded_link(self):
        lk = dict(link=dict(C=10000.0, Q=1250.0), rate=12000.0, n_total=12000, dead=None, t_limit=6.0)
        out = toy2.run(rt_cfg(repair=dict(r="law", edge="evict"), **lk))
        g = out["gauges"]
        self.assertGreater(g["first_qdrop"], 0)
        self.assertLessEqual(g["first_qdrop"], g["queue_drop"])
        self.assertLessEqual(g["dec_units_qdrop"], min(g["dec_units"], g["first_qdrop"]))
        off = toy2.run(rt_cfg(**lk))
        self.assertEqual(off["gauges"]["dec_units_qdrop"], 0)                                  # nothing decodes without repair


class Rev4Gauge(unittest.TestCase):
    """Revision 4, step 1 (three-knobs-revision4.md 3.1, 3.2, 5.3): the wait limit's inputs as readouts only.  No decision reads them."""

    def test_defaults(self):
        c = toy2.Cfg()
        self.assertEqual((c.rev4_gauge, c.dhat_bias), (False, 0.0))

    def test_tau_is_inert_at_t_inf_and_never_nan(self):
        for delta in (0.005, 0.5, 50.0, 1e6):
            for x in (0.0, 1.0, 1.3, 100.0):
                for t_ov in (0.0, 1e6):
                    K = toy2.wait_limit_K(delta, x, t_ov)
                    tau = toy2.wait_limit_tau(math.inf, K, math.inf)
                    self.assertEqual(tau, math.inf)
                    self.assertFalse(math.isnan(tau))

    def test_tau_is_t_exactly_at_headroom(self):
        for T in (0.1, 0.4, 2.0, 1e9):
            for x in (0.0, 0.5, 1.0):
                self.assertEqual(toy2.wait_limit_tau(T, toy2.wait_limit_K(50.0, x, 1e3), T - 0.1 + 0.09), T)

    def test_shape_monotone_and_the_collapse_threshold(self):
        R, o = 0.102, 0.093
        den = lambda T: max(T - R + o, o)                                    # noqa: E731
        prev = -math.inf
        for T in (0.2, 0.4, 0.8, 2.0):
            tau = toy2.wait_limit_tau(T, 0.05, den(T))
            self.assertGreater(tau, prev)
            prev = tau
        for a, b in (((0.5, 1.3, 2.0), (50.0, 1.3, 2.0)), ((50.0, 1.3, 2.0), (50.0, 2.0, 2.0)), ((50.0, 1.3, 2.0), (50.0, 1.3, 3.0))):
            self.assertGreaterEqual(toy2.wait_limit_tau(0.4, toy2.wait_limit_K(*a), den(0.4)), toy2.wait_limit_tau(0.4, toy2.wait_limit_K(*b), den(0.4)))
        for T, o_ in ((0.4, 0.093), (1.0, 0.05), (2.0, 0.2)):
            Kc = toy2.wait_limit_Kc(T, R, o_)
            self.assertAlmostEqual(Kc, (T - R) * (T - R + o_), delta=1e-12)
            c = T - R
            self.assertAlmostEqual(c - Kc / (c + o_), 0.0, delta=1e-12)     # the effective margin reaches 0 at K_c
        K = toy2.wait_limit_K(50.0, 1.25, 1.0)
        self.assertAlmostEqual(K, 12.5, delta=1e-12)

    def test_dhat_needs_ten_qualifying_rounds_and_a_short_pause(self):
        d = toy2.DHat()
        self.assertEqual(d.value(), math.inf)
        for i in range(9):
            d.add_round(100, 0.1, True, 0.0, 0.1)
        self.assertEqual(d.value(), math.inf)                                    # nine rounds: not enough
        d.add_round(120, 0.1, True, 0.05, 0.1)
        self.assertEqual(d.value(), 1200.0)                                       # the maximum over the ten
        d.add_round(500, 0.1, True, 0.5, 0.1)                                     # a pause longer than R: the round does not qualify
        self.assertEqual(d.value(), math.inf)
        for i in range(10):
            d.add_round(100, 0.1, True, 0.0, 0.1)
        self.assertEqual(d.value(), 1000.0)                                       # ten clean rounds later it is back, the bad one forgotten
        d.add_round(100, 0.1, False, 0.0, 0.1)                                    # not flow-limited: the link may have idled
        self.assertEqual(d.value(), math.inf)
        z = toy2.DHat()
        for i in range(10):
            z.add_round(0, 0.1, True, 0.0, 0.1)
        self.assertGreater(z.value(), 0.0)                                        # D-hat > 0 always

    def test_the_estimator_cannot_read_the_cells_link(self):
        import inspect
        src = inspect.getsource(toy2.DHat) + inspect.getsource(toy2.Sim.dhat_close_round) + inspect.getsource(toy2.Sim.dhat_note_pause)
        for forbidden in ("link", "p0", "pc", "link_sv", "cell"):
            self.assertNotIn(forbidden, src)

    def test_gauges_change_nothing(self):
        for cell, kw in (("CL", dict(n_total=6000, dead=(0, 0.1, 0.1))), ("OV", dict(n_total=6000))):
            a = e_cfg(cell, 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), **kw)
            b = e_cfg(cell, 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), rev4_gauge=True, **kw)
            a.want_trace = b.want_trace = True
            ra, rb = toy2.run(a), toy2.run(b)
            import batch3e
            self.assertEqual(batch3e.id3e(ra, rb), [], cell)
            self.assertEqual(ra["_trace"], rb["_trace"], cell)
            self.assertNotIn("rev4", ra)
            self.assertIn("rev4", rb)

    def test_no_tick_is_below_t_with_headroom_and_overload_shows(self):
        ok = toy2.run(e_cfg("CL", 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), n_total=30000, dead=(0, 0.5, 0.5), rev4_gauge=True))["rev4"]
        self.assertGreater(ok["n_ticks"], 0)
        self.assertEqual(ok["n_tau_lt_T"], 0)
        ov = toy2.run(e_cfg("OV", 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), n_total=36000, rev4_gauge=True))["rev4"]
        self.assertGreater(ov["ov_frac"], 0.1)
        self.assertGreater(ov["n_tau_lt_T"], 0)
        self.assertIsNotNone(ov["collapse_t"])
        self.assertGreater(ov["in_flight_max"], 0)

    def test_in_flight_counts_copies_and_repairs(self):
        cfg = e_cfg("CL", 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), n_total=20000, dead=(0, 0.5, 0.5), rev4_gauge=True)
        sim = toy2.Sim(cfg)
        seen = []
        orig = sim.in_flight_now
        sim.in_flight_now = lambda: seen.append((orig(), max(0, sim.n_sent - 1 - sim.rhi_known))) or seen[-1][0]
        sim.run()
        self.assertTrue(seen)
        self.assertTrue(all(a >= b for a, b in seen))                    # never below the unreported first-send tail
        self.assertTrue(any(a > b for a, b in seen))                     # copies or repairs in flight raise it somewhere

    def test_a_biased_dhat_reads_lower(self):
        r0 = toy2.run(e_cfg("OV", 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), n_total=36000, rev4_gauge=True))["rev4"]
        r1 = toy2.run(e_cfg("OV", 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), n_total=36000, rev4_gauge=True, dhat_bias=0.10))["rev4"]
        self.assertGreaterEqual(r1["n_tau_lt_T"], r0["n_tau_lt_T"])
        self.assertAlmostEqual(r1["dhat_mean"] / r0["dhat_mean"], 0.9, delta=0.02)


class Rev4Step2(unittest.TestCase):
    """Revision 4 step 2 (three-knobs-revision4.md 3.1, 3.3, 5.2): the wait limit, the spare-capacity clamp and today's Realtime behind an engine queue.  All off by default."""

    @staticmethod
    def ov(design, n=24000, **kw):
        import batch3e
        n = kw.pop("n_total", n)
        rp = batch3e.rp("RL")
        if design == "new":
            return batch3e.new_cfg("OV", 0, 0.4, 0.0, 50.0, rp, n_total=n, **kw)
        return batch3e.rt_cfg("OV", 0, 50.0, rp, n_total=n, **kw)

    def test_defaults_off_and_fingerprint_stable(self):
        import batch3e
        c = toy2.Cfg()
        self.assertEqual((c.clamp, c.wait_limit, c.eq, c.cwnd_sim, c.rate2, c.rate2_t, c.eq_cap), (False, False, False, 0.0, 0.0, 0.0, 8192))
        a = self.ov("new")
        self.assertEqual(batch3e.fingerprint(a), batch3e.fingerprint(dataclasses.replace(a, clamp=False, wait_limit=False)))
        self.assertNotEqual(batch3e.fingerprint(a), batch3e.fingerprint(dataclasses.replace(a, clamp=True)))
        self.assertNotEqual(batch3e.fingerprint(a), batch3e.fingerprint(dataclasses.replace(a, cwnd_sim=2048.0)))

    def test_hand_over_clock_is_piecewise(self):
        sim = toy2.Sim(self.ov("new", rate2=8000.0, rate2_t=0.5, n_total=12000))
        self.assertAlmostEqual(sim.ht(0), 0.0, delta=1e-12)
        self.assertAlmostEqual(sim.ht(6000), 0.5, delta=1e-9)
        self.assertAlmostEqual(sim.ht(6000 + 8000), 1.5, delta=1e-9)
        for u in (0, 1, 5999, 6000, 6001, 11999):
            self.assertEqual(sim.hand_count(sim.ht(u)) - 1, u)                # the inverse hands over unit u at ht(u)

    def test_clamp_bounds_r_eff_and_changes_only_where_it_binds(self):
        import batch3e
        a = toy2.run(dataclasses.replace(self.ov("new"), rev4_gauge=True))
        b = toy2.run(dataclasses.replace(self.ov("new"), rev4_gauge=True, clamp=True))
        self.assertGreater(a["rev4"]["spare_bind"], 0.5)                       # OV: in_flight near B
        self.assertGreater(b["gauges"]["clamp_n"], 0)
        self.assertLessEqual(b["gauges"]["r_eff_over_spare"], 0)                # r_eff <= max(spare, 0) at every accrual
        self.assertLess(b["gauges"]["repair_sends"] + b["gauges"]["rep_sent"], a["gauges"]["repair_sends"] + a["gauges"]["rep_sent"] + 1)
        self.assertEqual(b["rev4"]["cwnd_sim"], 1000.0)

    def test_clamp_is_identity_without_repair_and_where_there_is_headroom(self):
        import batch3e
        for kw in (dict(), ):
            off = dataclasses.replace(batch3e.new_cfg("OV", 0, 0.4, 0.0, 50.0, None, n_total=12000), want_trace=True)
            on = dataclasses.replace(off, clamp=True)
            self.assertEqual(batch3e.id3e(toy2.run(off), toy2.run(on)), [])        # r = 0: nothing changes
        c = rt_e(repair=dict(r="law", edge="worth"), n_total=3000, want_trace=True)
        on = dataclasses.replace(c, clamp=True)
        ra, rb = toy2.run(c), toy2.run(on)
        self.assertEqual(ra["_trace"], rb["_trace"])                               # RT-like cell: in_flight is tens, spare >> r
        self.assertEqual(batch3e.id3e(ra, rb), [])

    def test_wait_limit_is_inert_at_headroom_and_at_t_inf_and_acts_in_overload(self):
        import batch3e
        c = e_cfg("CL", 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), n_total=30000, dead=(0, 0.5, 0.5), want_trace=True)
        on = dataclasses.replace(c, wait_limit=True)
        self.assertEqual(batch3e.id3e(toy2.run(c), toy2.run(on)), [])
        inf = dataclasses.replace(self.ov("new", n=24000), T=math.inf, want_trace=True)
        self.assertEqual(batch3e.id3e(toy2.run(inf), toy2.run(dataclasses.replace(inf, wait_limit=True))), [])
        a = toy2.run(dataclasses.replace(self.ov("new", n=60000), rev4_gauge=True))
        b = toy2.run(dataclasses.replace(self.ov("new", n=60000), rev4_gauge=True, wait_limit=True))
        self.assertGreater(b["rev4"]["deq_early_would"] + b["rev4"]["n_tau_lt_T"], 0)
        self.assertLess(b["dh_p50"], a["dh_p50"])                                   # the wait cap lowers the delay in overload

    def test_send_gate_default_off_and_never_stages_while_in_flight_is_at_cwnd(self):
        import batch3e
        self.assertFalse(toy2.Cfg().send_gate)
        a = toy2.run(dataclasses.replace(self.ov("new", n=36000), rev4_gauge=True))
        b = toy2.run(dataclasses.replace(self.ov("new", n=36000), rev4_gauge=True, send_gate=True))
        self.assertGreater(a["rev4"]["emit_over_B"], 0)                       # in_flight passes B without the gate (the store pause counts outstanding seqs)
        self.assertEqual(b["rev4"]["emit_over_B"], 0)                         # no payload staged while in_flight >= cwnd_sim
        self.assertGreaterEqual(b["gauges"]["pause_events"], a["gauges"]["pause_events"])
        self.assertEqual(batch3e.fingerprint(self.ov("new")), batch3e.fingerprint(dataclasses.replace(self.ov("new"), send_gate=False)))
        self.assertNotEqual(batch3e.fingerprint(self.ov("new")), batch3e.fingerprint(dataclasses.replace(self.ov("new"), send_gate=True)))

    def test_send_gate_is_identity_where_in_flight_stays_below_cwnd(self):
        import batch3e
        c = e_cfg("CL", 0, 0.4, 0.0, repair=dict(r="law", edge="worth"), n_total=20000, dead=(0, 0.5, 0.5), want_trace=True)
        on = dataclasses.replace(c, send_gate=True, cwnd_sim=1e6)
        self.assertEqual(batch3e.id3e(toy2.run(c), toy2.run(on)), [])

    def test_engine_queue_evicts_the_oldest_and_is_inert_without_overload(self):
        import batch3e
        base = self.ov("rt", n=4000, dead=None)
        calm = dataclasses.replace(base, rate=5000.0, eq=True, want_trace=True)       # below C = 10 000: the queue stays empty
        ref = dataclasses.replace(calm, eq=False)
        ra, rb = toy2.run(ref), toy2.run(calm)
        self.assertEqual(rb["gauges"]["fq_evict"], 0)
        self.assertEqual(ra["delivered"], rb["delivered"])
        over = dataclasses.replace(self.ov("rt", n=3000, dead=None), eq=True, eq_cap=100)
        r = toy2.run(over)
        self.assertGreater(r["gauges"]["fq_evict"], 0)
        self.assertLessEqual(r["gauges"]["fq_max"], 100)
        self.assertEqual(r["gauges"]["queue_drop"], 0)                                 # drained at the link rate: the link buffer stays empty

    def test_engine_queue_adds_residence_under_sustained_overload(self):
        eq = toy2.run(dataclasses.replace(self.ov("rt", n=9000, dead=None), eq=True, eq_cap=4000))
        st = toy2.run(self.ov("rt", n=9000, dead=None))
        self.assertGreater(eq["w_p50"], st["w_p50"])                                      # delivered units wait in the queue instead of being dropped at the link
        self.assertEqual(eq["gauges"]["queue_drop"], 0)
        self.assertGreater(st["gauges"]["queue_drop"], 0)
        self.assertGreater(eq["share_any"], st["share_any"])


if __name__ == "__main__":
    unittest.main()
