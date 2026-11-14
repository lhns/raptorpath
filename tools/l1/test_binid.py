#!/usr/bin/env python3
"""binid_run.sh against a fake cargo. SKIPs when flock(1) is absent
(take_lock refuses without it), as test_lib_battery.sh's flock section."""
import os, shutil, subprocess, tarfile, tempfile, unittest

HERE = os.path.dirname(os.path.abspath(__file__))
FAKE_CARGO = r'''#!/bin/bash
# build: fake binary = concatenated src/*.txt (fails if src/FAIL exists); test: pass
if [ "$1" = build ]; then
  [ -e FAIL ] && { echo "error: boom" >&2; exit 101; }
  mkdir -p "$CARGO_TARGET_DIR/release"
  cat *.txt > "$CARGO_TARGET_DIR/release/raptorpath"
else
  echo "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
fi
'''


def have(tool):
    return shutil.which(tool) is not None


@unittest.skipUnless(have("flock") and have("bash") and have("pgrep"),
                     "SKIP: flock(1)/bash/pgrep missing (take_lock needs flock)")
class BinId(unittest.TestCase):
    def setUp(self):
        self.td = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.td, True)
        bindir = os.path.join(self.td, "bin")
        os.mkdir(bindir)
        p = os.path.join(bindir, "cargo")
        with open(p, "w", newline="\n") as f:
            f.write(FAKE_CARGO)
        os.chmod(p, 0o755)
        self.env = dict(os.environ, PATH=bindir + os.pathsep + os.environ["PATH"],
                        RWM_VM_LOCK=os.path.join(self.td, "vm.lock"),
                        RWM_RP_LOCK=os.path.join(self.td, "rp.lock"),
                        BI_ROOT=os.path.join(self.td, "root"),
                        BI_WORK=os.path.join(self.td, "work"))

    def tree(self, name, content, extra=()):
        d = os.path.join(self.td, name + ".d")
        os.mkdir(d)
        with open(os.path.join(d, "a.txt"), "w") as f:
            f.write(content)
        for e in extra:
            with open(os.path.join(d, e), "w") as f:
                f.write("x")
        tgz = os.path.join(self.td, name + ".tgz")
        with tarfile.open(tgz, "w:gz") as t:
            for f in os.listdir(d):
                t.add(os.path.join(d, f), arcname=f)
        return tgz

    def run_binid(self, *args):
        r = subprocess.run(["bash", os.path.join(HERE, "binid_run.sh"), *args],
                           env=self.env, capture_output=True, text=True)
        with open(os.path.join(self.env["BI_ROOT"], "BINID.txt")) as f:
            return r, f.read().splitlines()

    def test_ok(self):
        a, b = self.tree("a", "AAA", ["COMMIT"]), self.tree("b", "BBB")
        r, out = self.run_binid(f"base={a}", f"new={b}", f"base={a}")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        ids = [l for l in out if l.startswith("BINID ") and "sha256=" in l]
        self.assertEqual([l.split()[1] for l in ids], ["base", "new", "base"])
        sha = [l.split()[2] for l in ids]
        self.assertEqual(sha[0], sha[2])
        self.assertNotEqual(sha[0], sha[1])
        tests = [l for l in out if l.startswith("BINTEST ")]
        self.assertEqual(len(tests), 3)
        self.assertIn("rc=0 passed=3 failed=0", tests[0])
        self.assertEqual(out[-1], "BINID-DONE")

    def test_build_fail(self):
        a, b = self.tree("a", "AAA", ["FAIL"]), self.tree("b", "BBB")
        r, out = self.run_binid(f"bad={a}", f"good={b}")
        self.assertEqual(r.returncode, 1)
        self.assertIn("BINID bad BUILD-FAIL", out[0])
        self.assertIn("sha256=", out[1])
        self.assertEqual(out[-1], "BINID-DONE")
        self.assertFalse([l for l in out if l.startswith("BINTEST bad")])


if __name__ == "__main__":
    unittest.main()
