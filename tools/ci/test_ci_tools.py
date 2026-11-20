#!/usr/bin/env python3
"""Fast tests of the CI checkers, each shown red on a bad fixture and green on a good one:
the flake rule (ADR-0080), the shed-site rule (coverage_by_row.py), the expected-failure
scanner and verdict. Run: python tools/ci/test_ci_tools.py (needs pyyaml, as coverage_by_row does)."""
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import coverage_by_row as cov  # noqa: E402
import expected_failure as ef  # noqa: E402
import flake_rule as fr  # noqa: E402

Q = "# c\nslow_timing_test | alice | timing bound, diagnosed in status 12\n"


class FlakeRule(unittest.TestCase):
    def test_failure_outside_quarantine_fails(self):  # red
        out, bad = fr.report(["net::tests::flaky_one [cargo test -p x]"], fr.parse_quarantine(Q))
        self.assertTrue(bad)
        self.assertIn("FAIL  net::tests::flaky_one", out)

    def test_one_failure_in_many_runs_still_fails(self):  # the old rule passed it on a re-run
        lines = ["t_a [c]"]  # failed once in 30 runs, passed the other 29
        _, bad = fr.report(lines, {}, runs=30)
        self.assertTrue(bad)

    def test_quarantined_failure_passes_and_is_listed(self):  # green
        out, bad = fr.report(["net::tests::slow_timing_test [c]"], fr.parse_quarantine(Q))
        self.assertFalse(bad)
        self.assertIn("QUARANTINED", out)
        self.assertIn("alice", out)

    def test_no_failures_green(self):
        self.assertFalse(fr.report([], {})[1])

    def test_quarantine_needs_owner_and_reason(self):
        for bad in ("t | alice\n", "t | | reason\n", "t\n"):
            with self.assertRaises(ValueError):
                fr.parse_quarantine(bad)

    def test_shipped_quarantine_parses(self):
        fr.parse_quarantine(fr.DEFAULT_Q.read_text(encoding="utf-8"))

    def test_suffix_match_is_on_a_path_boundary(self):
        q = fr.parse_quarantine(Q)
        self.assertTrue(fr.is_quarantined("a::b::slow_timing_test", q))
        self.assertFalse(fr.is_quarantined("a::not_slow_timing_test", q))

    def test_pass_counts(self):
        rows = fr.pass_table(["t [c] ok"] * 29 + ["t [c] FAILED", "u [c] ok"], 30)
        self.assertEqual(rows, [("t", "c", 29, 1), ("u", "c", 1, 0)])

    def test_cli_exit_codes(self):
        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            (d / "q.txt").write_text(Q)
            (d / "f_bad.txt").write_text("other_test [c]\n")
            (d / "f_ok.txt").write_text("slow_timing_test [c]\n")
            run = lambda f: subprocess.run([sys.executable, str(HERE / "flake_rule.py"), "--quarantine", str(d / "q.txt"), "--failures", str(d / f)],
                                           capture_output=True).returncode
            self.assertEqual((run("f_bad.txt"), run("f_ok.txt")), (1, 0))
            chk = lambda n: subprocess.run([sys.executable, str(HERE / "flake_rule.py"), "--quarantine", str(d / "q.txt"), "--check", n]).returncode
            self.assertEqual((chk("slow_timing_test"), chk("other_test")), (0, 1))


def site(line=10, hits=None, reason=None):
    return {"desc": "d", "miss_reason": reason, "line": line, "by_group": hits or {}}


class ShedSiteRule(unittest.TestCase):
    G = ["a", "b"]

    def test_miss_everywhere_without_reason_fails(self):  # red
        v = cov.site_violations({"s": site(hits={"a": 0, "b": None})}, self.G)
        self.assertEqual(len(v), 1)
        self.assertIn("no miss_reason", v[0])

    def test_miss_everywhere_with_reason_passes(self):  # green
        self.assertEqual(cov.site_violations({"s": site(hits={"a": 0, "b": 0}, reason="ADR-0079")}, self.G), [])

    def test_hit_somewhere_passes(self):  # green
        self.assertEqual(cov.site_violations({"s": site(hits={"a": 0, "b": 3})}, self.G), [])

    def test_stale_reason_fails(self):  # the reason must not rot
        v = cov.site_violations({"s": site(hits={"a": 2}, reason="old")}, self.G)
        self.assertIn("remove it", v[0])

    def test_site_not_found_fails(self):
        self.assertEqual(len(cov.site_violations({"s": site(line="not found")}, self.G)), 1)

    def test_no_lcov_fails(self):
        self.assertEqual(len(cov.site_violations({"s": site()}, [])), 1)

    def test_shipped_sites_each_have_regex_and_reasons_are_text(self):
        import json
        for s in json.loads((HERE / "coverage_sites.json").read_text(encoding="utf-8"))["sites"]:
            self.assertTrue(s["regex"] and s["file"])
            self.assertIsInstance(s.get("miss_reason", ""), str)


class ExpectedFailure(unittest.TestCase):
    def tree(self, d, src):
        d = pathlib.Path(d)
        (d / "c/tests").mkdir(parents=True)
        (d / "c/Cargo.toml").write_text('[package]\nname="c"\n[[test]]\nname="t"\nrequired-features=["sim"]\n')
        (d / "c/tests/t.rs").write_text(src)
        return d

    SRC = ('#[test]\n#[ignore = "bench: measurement only"]\nfn m() {}\n'
           '#[test]\n#[ignore = "rec-evict-x: stuck"]\nfn defect() {}\n'
           '#[test]\n#[ignore]\nfn plain() {}\n')

    def test_scan_finds_only_node_id_reasons(self):
        with tempfile.TemporaryDirectory() as d:
            ts = ef.scan(self.tree(d, self.SRC))
            self.assertEqual([(t["id"], t["fn"], t["target"], t["features"]) for t in ts], [("rec-evict-x", "defect", "t", ["sim"])])
            self.assertIn("--features", ef.command(ts[0]))
            self.assertIn("tokio_unstable", ef.env_for(ts[0])["RUSTFLAGS"])

    def test_real_tree_finds_the_known_defect(self):
        ids = {(t["id"], t["fn"]) for t in ef.scan(HERE.parents[1])}
        self.assertIn(("rec-evict-blocker-unresolved", "liveness_after_shed_at_rho_below_1"), ids)

    def test_unknown_node_id_is_an_error_not_a_skip(self):
        with tempfile.TemporaryDirectory() as d:
            d = self.tree(d, self.SRC)
            (d / "docs/roadmap").mkdir(parents=True)
            (d / "docs/roadmap/nodes.yaml").write_text("- id: some-other-node\n")
            with self.assertRaises(SystemExit) as e:
                ef.scan(d)
            self.assertIn("not a node id", str(e.exception))

    T = {"id": "n-x", "fn": "defect"}

    def test_verdict_failing_defect_is_green(self):
        self.assertTrue(ef.verdict(self.T, "test defect ... FAILED\n")[0])

    def test_verdict_passing_defect_is_red(self):
        ok, msg = ef.verdict(self.T, "test defect ... ok\n")
        self.assertFalse(ok)
        self.assertIn("a defect test now passes: fix landed or the test rotted; un-ignore it or fix it", msg)

    def test_verdict_not_run_is_red(self):
        self.assertFalse(ef.verdict(self.T, "error[E0432]: unresolved import\n")[0])
        self.assertFalse(ef.verdict(self.T, "test defect ... ignored\n")[0])


if __name__ == "__main__":
    unittest.main()
