#!/usr/bin/env python3
"""Run the vendored tech-tree skill's history_gap.py with this repo's config (docs/roadmap/tech-tree.yaml).

Extra arguments pass through (--out DIR is required, --before YYYY-MM-DD). Skill version: tools/roadmap/skill/VERSION.
"""
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CONFIG = HERE.parents[1] / "docs" / "roadmap" / "tech-tree.yaml"

if __name__ == "__main__":
    cmd = [sys.executable, str(HERE / "skill" / "scripts" / "history_gap.py"), "--config", str(CONFIG), *sys.argv[1:]]
    sys.exit(subprocess.call(cmd))
