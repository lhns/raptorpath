#!/usr/bin/env python3
"""Tests of lagdiag_digest.py on synthetic fixtures (tiny fake logs in a temp dir)."""
import contextlib
import io
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lagdiag_digest as D  # noqa: E402

ORIGIN = 1_700_000_000_000_000   # unix us
XFER = (0, 10_000_000)           # 10 s window, us since origin
FLOOR = 100
EXCESS = 2000


def tick_at(i):
    """Mid-interval tick times (us since origin): the 100 ms stat interval
    index of tick i is 10 + 10 * i (even)."""
    return 1_050_000 + i * 1_000_000


def stat_text(steal_idx=(), busy_idx=(), n_int=105, ncpu=8, busy_n=6):
    """A 10 Hz stat file: interval k spans snapshots k and k+1, 10 jiffies per
    vCPU. steal_idx: intervals with 5 steal jiffies on cpu0; busy_idx:
    intervals with `busy_n` busy vCPUs (the rest idle)."""
    cum = [[0] * 10 for _ in range(ncpu)]
    out = []
    for k in range(n_int + 1):
        out.append(f"T {ORIGIN / 1e6 + k * 0.1:.9f}")
        agg = [sum(c[j] for c in cum) for j in range(10)]
        out.append("cpu  " + " ".join(map(str, agg)))
        for c in range(ncpu):
            out.append(f"cpu{c} " + " ".join(map(str, cum[c])))
        for c in range(ncpu):
            busy = k in busy_idx and c < busy_n
            steal = 5 if (k in steal_idx and c == 0) else 0
            if busy or steal:
                cum[c][0] += 10 - steal       # user
                cum[c][7] += steal            # steal
            else:
                cum[c][3] += 10               # idle
    return "\n".join(out) + "\n"


def side_log(side, floor, p99, ticks=(), polls=(), summary=True, raw=True, lag=True, task_max=True, min_us=True):
    """ticks: [(at, late)]; polls: [(task, end_us_since_origin_or_abs, wall)] (abs unix us)."""
    L = []
    if lag:
        m = f" min_us={floor}" if min_us else ""
        L.append(f"[LAG] phase=xfer side={side} obj=1 tick_ms=10 n=1000 p50_us=500 p99_us={p99} max_us={p99 + 10}{m} dropped=0")
    if raw:
        samples = [(500, floor)] + list(ticks)
        s = ",".join(f"{a}:{l}" for a, l in samples)
        L.append(f"[LAGRAW] side={side} origin_unix_us={ORIGIN} xfer={XFER[0]}-{XFER[1]} n={len(samples)} printed={len(samples)} s={s}")
    for task, end, wall in polls:
        L.append(f"[LPOLL] side={side} task={task} end_unix_us={end} wall_us={wall} cpu_us={wall} vcsw=0 ivcsw=0")
    if summary:
        L.append(f"[LPOLL] side={side} summary n={len(polls)} dropped=0 printed={len(polls)}")
    mp = " long_polls=1 max_poll_us=1234" if task_max else ""
    L.append(f"[TASK] phase=xfer side={side} obj=1 task=sender polls=10 cpu_us=1 wall_us=2 cores=0.1 busy=0.1 "
             f"asl_vol_us=0 asl_inv_us=5 asl_none_us=7 vcsw=0 ivcsw=0 cpu_per_poll_us=1.00 wall_s=10.000{mp}")
    return "\n".join(L) + "\n"


def write_row(d, tag, E, ticks_tasks, extra_polls=(), steal=(), busy=(), broken=None, min_us=FLOOR):
    """ticks_tasks: one entry per late tick: a task name (an overlapping 2 ms
    long poll ends at the tick) or None (no poll). E = client p99 - min."""
    ticks, polls = [], list(extra_polls)
    for i, task in enumerate(ticks_tasks):
        at = tick_at(i)
        ticks.append((at, min_us + EXCESS))
        if task:
            polls.append((task, ORIGIN + at, EXCESS))
    kw = {}
    if broken == "raw":
        kw["raw"] = False
    elif broken == "summary":
        kw["summary"] = False
    elif broken == "lag":
        kw["lag"] = False
    elif broken == "taskmax":
        kw["task_max"] = False
    pre = os.path.join(d, tag)
    with open(pre + "-c.log", "w") as f:
        f.write(side_log("client", min_us, min_us + E, ticks, polls, **kw))
    with open(pre + "-s.log", "w") as f:
        f.write(side_log("server", 50, 400))
    with open(pre + "-stat.txt", "w") as f:
        f.write(stat_text(steal, busy, n_int=(105 if broken != "stat" else 5)))
    return pre


class Arith(unittest.TestCase):
    def test_stall_interval_and_floor(self):
        raw = {"origin": ORIGIN, "xfer": (0, 5000), "n": 4,
               "samples": [(500, 100), (1000, 200), (3000, 3200), (9000, 9999)]}
        ticks, floor = D.late_ticks(raw)
        self.assertEqual(floor, 100)
        # 3200 - 100 = 3100 us late; (9000, ...) lies outside the window.
        self.assertEqual(len(ticks), 1)
        t = ticks[0]
        self.assertEqual((t["excess"], t["t_end"], t["t_start"]), (3100, ORIGIN + 3000, ORIGIN + 3000 - 3100))
        # 200 - 100 = 100 us: below LATE_US
        raw["samples"][1] = (1000, 1100)
        self.assertEqual(len(D.late_ticks(raw)[0]), 2)   # excess 1000 is late (>=)
        raw["samples"][1] = (1000, 1099)
        self.assertEqual(len(D.late_ticks(raw)[0]), 1)   # excess 999 is not

    def test_overlap_threshold_and_credit(self):
        tick = lambda: [{"t_start": 10_000, "t_end": 14_000}]
        # poll [13500, 14500] intersects [10000, 14000] by 500: overlapped.
        t = D.attribute(tick(), [{"task": "a", "end": 14_500, "wall": 1000}])[0]
        self.assertTrue(t["overlapped"] and t["task"] == "a")
        # by 499 ([13501, 14501] vs [10000, 14000]): not.
        t = D.attribute(tick(), [{"task": "a", "end": 14_501, "wall": 1000}])[0]
        self.assertFalse(t["overlapped"])
        self.assertIsNone(t["task"])
        # credit to the largest intersection
        polls = [{"task": "small", "end": 11_000, "wall": 1000},    # 1000
                 {"task": "big", "end": 13_000, "wall": 2500},      # 2500
                 {"task": "mid", "end": 12_000, "wall": 1500}]      # 1500
        t = D.attribute(tick(), polls)[0]
        self.assertTrue(t["overlapped"])
        self.assertEqual(t["task"], "big")

    def test_stat_parse_and_intervals(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "x-stat.txt")
            with open(p, "w") as f:
                f.write(stat_text(steal_idx={2}, busy_idx={1, 2}, n_int=4))
            snaps = D.parse_stat(p)
            self.assertEqual(len(snaps), 5)
            self.assertEqual(sorted(snaps[0][1]), [f"cpu{i}" for i in range(8)])   # aggregate excluded
            iv = D.stat_intervals(snaps)
            self.assertEqual(len(iv), 4)
            self.assertEqual([x["busy"] for x in iv], [0, 6, 6, 0])
            self.assertEqual([x["steal"] for x in iv], [0, 0, 5, 0])
            self.assertEqual(iv[0]["total"], 80)
            self.assertAlmostEqual(iv[1]["t1"] - iv[1]["t0"], 0.1, places=6)

    def test_busy_threshold_is_half_idle_plus_iowait(self):
        a = (0.0, {"cpu0": [0] * 10, "cpu1": [0] * 10})
        # cpu0: idle 5 + iowait 0 of 10 -> share 0.5 -> NOT busy; cpu1: idle 4 -> busy
        b = (0.1, {"cpu0": [5, 0, 0, 5, 0, 0, 0, 0, 0, 0], "cpu1": [6, 0, 0, 3, 1, 0, 0, 0, 0, 0]})
        iv = D.stat_intervals([a, b])
        # (idle+iowait)/total: cpu0 0.5 (not busy), cpu1 0.4 (busy)
        self.assertEqual(iv[0]["busy"], 1)

    def test_spearman(self):
        self.assertAlmostEqual(D.spearman([1, 2, 3, 4], [10, 20, 30, 40]), 1.0)
        self.assertAlmostEqual(D.spearman([1, 2, 3, 4], [4, 3, 2, 1]), -1.0)
        self.assertIsNone(D.spearman([1, 1, 1], [1, 2, 3]))
        self.assertAlmostEqual(D.spearman([1, 2, 2, 4], [1, 2, 3, 4]), 0.9486832980505138)


class Fixture(unittest.TestCase):
    def setUp(self):
        self._td = tempfile.TemporaryDirectory()
        self.d = self._td.name

    def tearDown(self):
        self._td.cleanup()

    def run_digest(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            D.digest(self.d)
        out = buf.getvalue()
        outcome = [ln for ln in out.splitlines() if ln.startswith("OUTCOME ")]
        self.assertEqual(len(outcome), 1, out)
        return outcome[0], out

    def rows24(self, mk, rt1=False):
        """24 RTOBS=2 rows with E = 100 * (i + 1); mk(i, high) -> kwargs."""
        for i in range(24):
            write_row(self.d, f"b{i // 2 + 1}-s{42 if i % 2 == 0 else 7}-rt2", 100 * (i + 1), **mk(i, i >= 16))
        if rt1:
            for j in range(6):
                write_row(self.d, f"b{2 * j + 2}-s42-rt1", 100 * (j + 1), [])


class Row(Fixture):
    def test_row_reading(self):
        write_row(self.d, "b1-s42-rt2", 3000, ["sender", None, "report"], steal={10 + 10 * 1}, busy={10})
        r = D.load_row(os.path.join(self.d, "b1-s42-rt2"))
        c = r["client"]
        self.assertIsNone(r["witness"])
        self.assertEqual((c["lag"]["p99"], c["lag"]["min"], c["lag"]["E"]), (3100, 100, 3000))
        self.assertEqual((c["late_n"], c["ovl_n"]), (3, 2))
        self.assertAlmostEqual(c["F"], 2 / 3)
        self.assertEqual(c["by_task"], {"sender": 1, "report": 1})
        # 2 long polls of 2000 us over a 10 s window
        self.assertAlmostEqual(c["lpw_s"], 4000 / 1e6 / 10)
        self.assertEqual(c["tasks"]["max_poll_us"], {"sender": 1234})
        self.assertEqual((c["tasks"]["asl_inv_us"], c["tasks"]["asl_none_us"]), (5, 7))
        # the one non-overlapped tick is tick 1 (interval 20); steal there, busy not
        self.assertEqual((c["cond_n"], c["cond_steal"], c["cond_busy"]), (1, 1, 0))
        self.assertEqual(c["base_steal"], 1)
        self.assertEqual(c["base_busy"], 1)
        # steal_frac: 5 of (8 vCPUs * 10 jiffies) in 1 of ~100 intervals
        self.assertAlmostEqual(c["steal_frac"], 5 / (80 * c["base_n"]))

    def test_witness_failures(self):
        for kind, why in (("raw", "LAGRAW"), ("summary", "LPOLL"), ("lag", "LAG"), ("stat", "stat")):
            write_row(self.d, f"w-{kind}-rt2", 500, [], broken=kind)
            r = D.load_row(os.path.join(self.d, f"w-{kind}-rt2"))
            self.assertIsNotNone(r["witness"], kind)
            self.assertIn(why, r["witness"])
        # RTOBS=1 rows need only the [LAG] line with min_us (no LAGRAW/LPOLL)
        write_row(self.d, "b2-s42-rt1", 500, [], broken="raw")
        self.assertIsNone(D.load_row(os.path.join(self.d, "b2-s42-rt1"))["witness"])
        write_row(self.d, "b2-s7-rt1", 500, [])
        # min_us missing in the [LAG] line
        pre = os.path.join(self.d, "nomin-rt2")
        write_row(self.d, "nomin-rt2", 500, [])
        with open(pre + "-c.log") as f:
            txt = f.read().replace(" min_us=100", "")
        with open(pre + "-c.log", "w") as f:
            f.write(txt)
        self.assertIn("min_us", D.load_row(pre)["witness"])

    def test_smoke(self):
        pre = write_row(self.d, "smoke-s42-rt2", 500, ["sender"])
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertTrue(D.smoke(pre))
            for kind in ("raw", "summary", "lag", "taskmax", "stat"):
                pre = write_row(self.d, f"smoke-bad{kind}-rt2", 500, [], broken=kind)
                self.assertFalse(D.smoke(pre), kind)


class Outcomes(Fixture):
    def test_in_process_single_task(self):
        # every tick overlapped by a sender poll; the high-E rows also carry
        # a 40 ms long poll elsewhere in the window (lpw/s far above the low rows).
        def mk(i, high):
            extra = [("sender", ORIGIN + 8_000_000, 40_000)] if high else []
            return dict(ticks_tasks=["sender"] * 3, extra_polls=extra)
        self.rows24(mk, rt1=True)
        outcome, out = self.run_digest()
        self.assertEqual(outcome, "OUTCOME IN-PROCESS(sender)", out)
        self.assertIn("PHASE-OFFSET", out)
        self.assertIn("PERTURB", out)
        self.assertIn("RTOBS-COMPARE", out)

    def test_in_process_mixed(self):
        def mk(i, high):
            extra = [("sender", ORIGIN + 8_000_000, 40_000)] if high else []
            return dict(ticks_tasks=["sender", "report", "receiver"] * 2, extra_polls=extra)
        self.rows24(mk)
        outcome, out = self.run_digest()
        self.assertEqual(outcome, "OUTCOME IN-PROCESS(MIXED:receiver,report)", out)

    def test_overlapped_but_no_lpw_gradient_is_unsettled(self):
        self.rows24(lambda i, high: dict(ticks_tasks=["sender"] * 3))
        self.assertEqual(self.run_digest()[0], "OUTCOME UNSETTLED")

    def test_vm_steal_conditional(self):
        # no poll overlaps; steal sits exactly on the intervals of the late ticks
        def mk(i, high):
            return dict(ticks_tasks=[None] * 3, steal={10 + 10 * k for k in range(3)})
        self.rows24(mk)
        outcome, out = self.run_digest()
        self.assertEqual(outcome, "OUTCOME VM-STEAL", out)

    def test_vm_steal_tercile_only(self):
        # steal on odd intervals of the high-E rows only: P(steal | tick) = 0
        # (ticks sit on even intervals) but the high tercile steals and the low does not
        def mk(i, high):
            return dict(ticks_tasks=[None] * 3, steal=set(range(1, 100, 2)) if high else set())
        self.rows24(mk)
        self.assertEqual(self.run_digest()[0], "OUTCOME VM-STEAL")

    def test_guest_cpu(self):
        def mk(i, high):
            return dict(ticks_tasks=[None] * 3, busy={10 + 10 * k for k in range(3)})
        self.rows24(mk)
        self.assertEqual(self.run_digest()[0], "OUTCOME GUEST-CPU")

    def test_unsettled_when_nothing_explains_the_ticks(self):
        self.rows24(lambda i, high: dict(ticks_tasks=[None] * 3))
        self.assertEqual(self.run_digest()[0], "OUTCOME UNSETTLED")

    def test_steal_takes_precedence_over_guest_cpu(self):
        def mk(i, high):
            iv = {10 + 10 * k for k in range(3)}
            return dict(ticks_tasks=[None] * 3, steal=iv, busy=iv)
        self.rows24(mk)
        self.assertEqual(self.run_digest()[0], "OUTCOME VM-STEAL")

    def test_in_process_takes_precedence_over_steal(self):
        def mk(i, high):
            extra = [("sender", ORIGIN + 8_000_000, 40_000)] if high else []
            return dict(ticks_tasks=["sender"] * 3, extra_polls=extra, steal={10, 20, 30})
        self.rows24(mk)
        self.assertEqual(self.run_digest()[0], "OUTCOME IN-PROCESS(sender)")

    def test_too_few_live_rows_is_instrument_failure(self):
        # 17 live RTOBS=2 rows: below the 18 floor, whatever they say
        for i in range(17):
            write_row(self.d, f"b{i}-s42-rt2", 100 * (i + 1), ["sender"] * 3)
        outcome, _ = self.run_digest()
        self.assertEqual(outcome, "OUTCOME UNSETTLED-INSTRUMENT")
        # one more live row reaches 18
        write_row(self.d, "b99-s42-rt2", 5000, ["sender"] * 3)
        self.assertNotEqual(self.run_digest()[0], "OUTCOME UNSETTLED-INSTRUMENT")

    def test_witness_failed_rows_are_excluded_and_counted(self):
        self.rows24(lambda i, high: dict(ticks_tasks=[None] * 3))
        for k in range(7):
            write_row(self.d, f"bad{k}-s42-rt2", 9999, ["sender"] * 3, broken="raw")
        outcome, out = self.run_digest()
        self.assertEqual(outcome, "OUTCOME UNSETTLED")      # 24 live >= 18
        self.assertIn("WITNESS-FAIL rows excluded: 7", out)
        # 7 more broken out of 24 live -> 17 live
        for i in range(0, 14, 2):
            write_row(self.d, f"b{i // 2 + 1}-s42-rt2", 100 * (i + 1), [None] * 3, broken="summary")
        self.assertEqual(self.run_digest()[0], "OUTCOME UNSETTLED-INSTRUMENT")

    def test_perturb_flag(self):
        self.rows24(lambda i, high: dict(ticks_tasks=[None] * 3))
        for j in range(6):
            write_row(self.d, f"b{2 * j + 2}-s42-rt1", 10, [])
        _, out = self.run_digest()
        line = [ln for ln in out.splitlines() if ln.startswith("PERTURB")][0]
        self.assertIn("PERTURBING", line)       # E median 1250 / 10


if __name__ == "__main__":
    unittest.main()
