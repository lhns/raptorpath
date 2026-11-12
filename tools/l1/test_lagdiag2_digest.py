#!/usr/bin/env python3
"""Tests of lagdiag2_digest.py on synthetic fixtures (tiny fake logs in a temp
dir). Every outcome branch of A and B, the parsing of the round-2 tokens, the
witnesses and the precedence between B's branches."""
import contextlib
import io
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lagdiag2_digest as D2  # noqa: E402
import lagdiag_digest as D  # noqa: E402
import test_lagdiag_digest as T1  # noqa: E402  (round-1 fixtures)

MIN_LAG = 100
MIN_OS = 40


def poll(wall, iters, secs, other, end=None):
    """One sender [LPOLL] entry. secs: {name: us}."""
    st = ",".join(f"{k}:{v}" for k, v in secs.items())
    e = end if end is not None else T1.ORIGIN + 2_000_000
    return (f"[LPOLL] side=client task=sender end_unix_us={e} wall_us={wall} cpu_us={wall} vcsw=0 ivcsw=0 "
            f"iters={iters} secs={st} other={other}")


def write_row(d, tag, e_lag, e_os, rd_us=10_000, polls=(), broken=None, txsec=True):
    """A client log with the round-1 lines (T1.side_log) plus the round-2
    instruments. e_os None omits [OSLAG]. polls: ready [LPOLL] lines."""
    pre = os.path.join(d, tag)
    base = T1.side_log("client", MIN_LAG, MIN_LAG + e_lag)
    x = []
    if e_os is not None and broken != "oslag":
        x.append(f"[OSLAG] phase=xfer side=client obj=1 n=1000 p50_us=60 p99_us={MIN_OS + e_os} max_us={MIN_OS + e_os + 9} "
                 f"min_us={MIN_OS} skipped=0")
    if broken != "oslagraw":
        x.append(f"[OSLAGRAW] side=client origin_unix_us={T1.ORIGIN} xfer=0-10000000 n=1 printed=1 s=500:{MIN_OS}")
    dl = "" if broken == "delay" else f" run_delay_us={rd_us}"
    x.append(f"[THR] os phase=xfer side=client obj=1 tid=11 comm=rp-w-0 cpu_s=1.000 cores=0.100 wall_s=10.000{dl}")
    x.append(f"[THR] os phase=xfer side=client obj=1 tid=12 comm=raptorpath cpu_s=0.100 cores=0.010 wall_s=10.000{dl and ' run_delay_us=5'}")
    x.append("[THR] os phase=xfer side=client obj=1 tid=13 comm=rp-w-1 cpu_s=1.000 cores=0.100 wall_s=10.000" + dl)
    if txsec and broken != "txsec":
        x.append("[TXSEC] phase=xfer side=client obj=1 task=sender iters=1000 in=2000/1000/900 pre=3000/1000/50 "
                 "flush=500/1000/10 select=800000/1000/9000 emit=4000/900/300 await=flush,select wall_s=10.000")
    if broken == "sender_plain":
        x.append("[LPOLL] side=client task=sender end_unix_us=1 wall_us=2000 cpu_us=2000 vcsw=0 ivcsw=0")
    elif broken != "nosender":
        x.extend(polls)
    with open(pre + "-c.log", "w") as f:
        f.write(base + "\n".join(x) + "\n")
    with open(pre + "-s.log", "w") as f:
        f.write(T1.side_log("server", 50, 400))
    with open(pre + "-stat.txt", "w") as f:
        f.write(T1.stat_text())
    return pre


def good_polls(n=2, wall=12_000, iters=1, secs=None, other=1000):
    secs = secs if secs is not None else {"pre": 9000, "emit": 2000}
    return [poll(wall, iters, secs, other) for _ in range(n)]


def make_dir(e_lag, e_os, polls=None, n=18, rd=None):
    """n live rt2 rows: row i has E_lag = e_lag(i), E_os = e_os(i)."""
    d = tempfile.mkdtemp()
    for i in range(n):
        write_row(d, f"b{i + 1}-s42-rt2", e_lag(i), e_os(i), rd_us=(rd(i) if rd else 10_000),
                  polls=polls if polls is not None else good_polls())
    return d


def run_digest(d):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        D2.digest(d)
    return buf.getvalue()


def outcome(txt, which):
    return next(ln for ln in txt.splitlines() if ln.startswith(f"OUTCOME-{which} "))[len(f"OUTCOME-{which} "):]


class Parsing(unittest.TestCase):
    def test_round2_tokens(self):
        lines = [
            "[OSLAG] phase=xfer side=client obj=1 n=900 p50_us=61 p99_us=250 max_us=900 min_us=40 skipped=3",
            "[OSLAG] phase=run side=client n=9 p50_us=1 p99_us=2 max_us=3 min_us=0 skipped=0",
            "[THR] os phase=xfer side=client obj=1 tid=11 comm=rp-w-0 cpu_s=1 cores=1 wall_s=2.000 run_delay_us=700",
            "[THR] os phase=xfer side=client obj=1 tid=12 comm=rp-w-1 cpu_s=1 cores=1 wall_s=2.000 run_delay_us=300",
            "[THR] os phase=xfer side=client obj=1 tid=13 comm=raptorpath cpu_s=1 cores=1 wall_s=2.000 run_delay_us=-",
            "[TXSEC] phase=xfer side=client obj=1 task=sender iters=77 in=10/5/3 arm_in=20/4/9 await=flush,select wall_s=2.000",
            poll(5000, 3, {"in": 10, "pre": 4000}, 990),
            "[LPOLL] side=client task=report end_unix_us=1 wall_us=3000 cpu_us=1 vcsw=0 ivcsw=0 iters=1 secs=in:0 other=1",
            "[LPOLL] side=client summary n=2 dropped=0 printed=2",
        ]
        o = D2.parse_oslag(lines)
        self.assertEqual((o["n"], o["p99"], o["min"], o["skipped"], o["E"]), (900, 250.0, 40.0, 3, 210.0))
        t = D2.parse_thr_delay(lines)
        self.assertEqual((t["os_lines"], t["with_delay"], t["worker_delay_us"], t["wall_s"]), (3, 3, 1000, 2.0))
        self.assertEqual(t["per_s"], 500.0)
        x = D2.parse_txsec(lines)
        self.assertEqual(x["iters"], 77)
        self.assertEqual(x["secs"], {"in": (10, 5, 3), "arm_in": (20, 4, 9)})
        self.assertEqual(x["awaits"], ["flush", "select"])
        sp = D2.parse_sender_polls(lines)
        self.assertEqual(len(sp), 1, "only task=sender, never the summary")
        self.assertEqual((sp[0]["wall"], sp[0]["iters"], sp[0]["secs"], sp[0]["other"]),
                         (5000, 3, {"in": 10, "pre": 4000}, 990))
        self.assertEqual(D2.parse_secs("a:1,b:2"), {"a": 1, "b": 2})
        self.assertIsNone(D2.parse_secs("a:x"))
        self.assertIsNone(D2.parse_secs(None))

    def test_a_pre_round2_sender_poll_has_no_iters(self):
        sp = D2.parse_sender_polls(["[LPOLL] side=client task=sender end_unix_us=1 wall_us=2000 cpu_us=1 vcsw=0 ivcsw=0"])
        self.assertEqual((sp[0]["iters"], sp[0]["secs"], sp[0]["other"]), (None, None, None))


class Witness(unittest.TestCase):
    def check(self, broken, expect):
        d = tempfile.mkdtemp()
        pre = write_row(d, "smoke-s42-rt2", 800, 300, polls=good_polls(), broken=broken)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            ok = D2.smoke(pre)
        self.assertEqual(ok, expect is None, buf.getvalue())
        if expect:
            self.assertIn("FAIL " + expect, buf.getvalue())
        return buf.getvalue()

    def test_a_complete_row_passes(self):
        self.check(None, None)

    def test_each_round2_witness_is_required(self):
        self.check("oslag", "client [OSLAG] xfer with min_us")
        self.check("oslagraw", "client [OSLAGRAW]")
        self.check("txsec", "client [TXSEC] iters>0")
        self.check("sender_plain", "client [LPOLL] task=sender entries")
        self.check("nosender", "client [LPOLL] task=sender entries")
        self.check("delay", "client [THR] os lines")

    def test_digest_excludes_a_witness_failed_row(self):
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 100 + 50 * i)
        write_row(d, "b19-s7-rt2", 900, 100, polls=good_polls(), broken="delay")
        txt = run_digest(d)
        self.assertIn("live_rt2=18", txt)
        self.assertIn("witness_fail=1", txt)
        self.assertIn("b19-s7-rt2: client [THR] os lines without run_delay_us=", txt)

    def test_io_pressure_flags_an_outlier_and_never_voids(self):
        # §26 amendment 3: full avg10 is a covariate; a high-pressure row is
        # flagged only when its E_lag is a Tukey outlier, and stays live.
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 100 + 50 * i)
        pre = write_row(d, "b19-s7-rt2", 900, 100, polls=good_polls())
        with open(pre + "-stat.txt", "a", encoding="utf-8") as f:
            f.write("IO full avg10=3.00 avg60=1.00 avg300=0.50 total=1\n")
            f.write("IO full avg10=25.10 avg60=4.00 avg300=1.00 total=2\n")
        self.assertEqual(D2.io_full_avg10_max(pre + "-stat.txt"), 25.1)
        self.assertIsNone(D2.load_row(pre)["witness"])
        rows = [D2.load_row(p) for p in sorted(
            os.path.join(d, f[: -len("-c.log")]) for f in os.listdir(d) if f.endswith("-c.log"))]
        live2 = [r for r in rows if r["witness"] is None and r["rt"] == 2]
        lines = D2.io_flags(live2)
        self.assertTrue(lines[0].startswith("IO-COVARIATE live_rt2_rows_with_io_full_avg10>=20: 1;"), lines)
        e = D2.e_lag(next(r for r in live2 if r["tag"] == "b19-s7-rt2"))
        es = sorted(D2.e_lag(r) for r in live2)
        q1, q3 = D2.L.q(es, 0.25), D2.L.q(es, 0.75)
        is_out = e < q1 - 1.5 * (q3 - q1) or e > q3 + 1.5 * (q3 - q1)
        self.assertEqual(any("IO-FLAG b19-s7-rt2" in ln for ln in lines), is_out, lines)
        # A forced outlier under high pressure is flagged.
        r = dict(next(r for r in live2 if r["tag"] == "b19-s7-rt2"))
        r["tag"] = "fake-outlier"
        r["client"] = dict(r["client"], lag=dict(r["client"]["lag"], E=10 ** 7))
        lines = D2.io_flags(live2 + [r])
        self.assertTrue(any(ln.startswith("IO-FLAG fake-outlier") for ln in lines), lines)
        txt = run_digest(d)
        self.assertIn("live_rt2=19", txt)
        self.assertIn("IO-COVARIATE", txt)


class OutcomeA(unittest.TestCase):
    def test_os_level(self):
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 100 + 50 * i)
        txt = run_digest(d)
        self.assertEqual(outcome(txt, "A"), "OS-LEVEL", txt)
        self.assertIn("spearman_rho(E_lag,E_os)=1", txt)
        self.assertIn("k=6", txt)

    def test_runtime(self):
        e_os = lambda i: 100 + 3 * ((i * 7) % 18)
        rho = D.spearman([1000 + 100 * i for i in range(18)], [e_os(i) for i in range(18)])
        self.assertLess(rho, 0.3, "fixture sanity: E_os is uncorrelated with E_lag")
        d = make_dir(lambda i: 1000 + 100 * i, e_os)
        txt = run_digest(d)
        self.assertEqual(outcome(txt, "A"), "RUNTIME", txt)

    def test_mixed_when_rank_agrees_but_the_tail_does_not_grow(self):
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 1000 + i)
        txt = run_digest(d)
        self.assertEqual(outcome(txt, "A"), "MIXED", txt)

    def test_mixed_for_a_constant_os_clock(self):
        """An undefined rho satisfies neither bar."""
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 200)
        self.assertEqual(outcome(run_digest(d), "A"), "MIXED")

    def test_unsettled_instrument_below_18_live_rows(self):
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 100 + 50 * i, n=17)
        self.assertEqual(outcome(run_digest(d), "A"), "UNSETTLED-INSTRUMENT")

    def test_run_delay_correlations_are_reported(self):
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 100 + 50 * i, rd=lambda i: 1000 * (i + 1))
        txt = run_digest(d)
        self.assertIn("spearman_rho(E_lag,run_delay_us_per_s)=1 spearman_rho(E_os,run_delay_us_per_s)=1 n=18", txt)

    def test_rt1_rows_never_enter_the_outcomes(self):
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 100 + 50 * i, n=17)
        # An RTOBS=1 row prints no deep gauge; it is live but not counted.
        pre = os.path.join(d, "b3-s42-rt1")
        with open(pre + "-c.log", "w") as f:
            f.write(T1.side_log("client", MIN_LAG, MIN_LAG + 500, raw=False, summary=False, task_max=False))
        with open(pre + "-s.log", "w") as f:
            f.write(T1.side_log("server", 50, 400))
        with open(pre + "-stat.txt", "w") as f:
            f.write(T1.stat_text())
        txt = run_digest(d)
        self.assertIn("live_rt1=1", txt)
        self.assertEqual(outcome(txt, "A"), "UNSETTLED-INSTRUMENT", txt)


class OutcomeB(unittest.TestCase):
    def b(self, polls, n=18):
        d = make_dir(lambda i: 1000 + 100 * i, lambda i: 100 + 50 * i, polls=polls, n=n)
        txt = run_digest(d)
        return outcome(txt, "B"), txt

    def test_unbounded_per_wake_names_the_top_section(self):
        out, txt = self.b(good_polls(wall=12_000, iters=3, secs={"pre": 2000, "emit": 8000}, other=2000))
        self.assertEqual(out, "UNBOUNDED-PER-WAKE(emit)", txt)
        self.assertIn("polls >=10ms", txt)

    def test_long_iteration_names_the_section(self):
        out, txt = self.b(good_polls(wall=12_000, iters=1, secs={"pre": 9000, "emit": 2000}, other=1000))
        self.assertEqual(out, "LONG-ITERATION(pre)", txt)

    def test_unattributed_when_other_dominates(self):
        out, txt = self.b(good_polls(wall=12_000, iters=1, secs={"pre": 1000, "emit": 1000, "gen": 1000}, other=9000))
        self.assertEqual(out, "UNATTRIBUTED", txt)

    def test_mixed_otherwise(self):
        out, txt = self.b(good_polls(wall=12_000, iters=1, secs={"pre": 4000, "emit": 3000}, other=5000))
        self.assertEqual(out, "MIXED", txt)

    def test_unbounded_takes_precedence_over_unattributed(self):
        out, _ = self.b(good_polls(wall=12_000, iters=4, secs={"pre": 1000}, other=11_000))
        self.assertEqual(out, "UNBOUNDED-PER-WAKE(pre)")

    def test_long_iteration_needs_a_named_section_not_other(self):
        """other >= 50 % is UNATTRIBUTED, never a LONG-ITERATION(other)."""
        out, _ = self.b(good_polls(wall=12_000, iters=1, secs={"pre": 2000}, other=10_000))
        self.assertEqual(out, "UNATTRIBUTED")

    def test_falls_back_to_polls_over_1ms_when_fewer_than_20_big_ones(self):
        # 18 rows x 1 big poll (wall 12 ms) = 18 < 20: the basis is every poll.
        polls = good_polls(n=1, wall=12_000, iters=1, secs={"pre": 9000}, other=3000) + \
            good_polls(n=2, wall=2_000, iters=3, secs={"emit": 1800}, other=200)
        out, txt = self.b(polls)
        self.assertEqual(out, "UNBOUNDED-PER-WAKE(pre)", txt)   # pre holds the most wall
        self.assertIn(">=1ms (fewer than 20 polls >= 10 ms)", txt)
        # With exactly 20 big polls the >= 10 ms set decides.
        d = tempfile.mkdtemp()
        for i in range(10):
            write_row(d, f"b{i + 1}-s42-rt2", 1000 + 100 * i, 100 + 50 * i,
                      polls=good_polls(n=2, wall=12_000, iters=1, secs={"pre": 9000}, other=3000)
                      + good_polls(n=2, wall=2_000, iters=3, secs={"emit": 1800}, other=200))
        live = [D2.load_row(os.path.join(d, f"b{i + 1}-s42-rt2")) for i in range(10)]
        out, ev = D2.decide_b(live)
        self.assertEqual((ev["n_big"], out), (20, "LONG-ITERATION(pre)"))

    def test_shares_are_wall_weighted(self):
        s = D2.shares([{"secs": {"a": 100}, "other": 0}, {"secs": {"a": 100, "b": 700}, "other": 100}])
        self.assertAlmostEqual(s["a"], 0.2)
        self.assertAlmostEqual(s["b"], 0.7)
        self.assertAlmostEqual(s["other"], 0.1)

    def test_no_sender_poll_is_unsettled(self):
        d = tempfile.mkdtemp()
        out, _ = D2.decide_b([])
        self.assertEqual(out, "UNSETTLED-INSTRUMENT")

    def test_the_pooled_txsec_table_marks_the_awaits(self):
        _, txt = self.b(good_polls())
        self.assertIn("sec=select(await) us=", txt)
        self.assertIn("sec=pre us=", txt)


if __name__ == "__main__":
    unittest.main()
