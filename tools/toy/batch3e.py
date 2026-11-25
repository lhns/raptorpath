"""Stage-3e runner (three-knobs-stage3e-build.md 8)."""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import test_toy2  # noqa: E402


def id3e(a, b):
    """test_toy2.id_diff with the stage 3e readouts (the rep3e block) excluded: every top-level key outside ID_EXCL and rep3e, plus ID_GAUGES."""
    excl = test_toy2.ID_EXCL | {"rep3e"}
    bad = [k for k in a if k not in excl and not k.startswith("_") and json.dumps(a[k]) != json.dumps(b[k])]
    bad += [("g", k) for k in test_toy2.ID_GAUGES if a["gauges"][k] != b["gauges"][k]]
    return bad
