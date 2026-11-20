#!/usr/bin/env python3
"""Run the vendored tech-tree skill's check.py with this repo's config (docs/roadmap/tech-tree.yaml).

Extra arguments pass through. Skill version: tools/roadmap/skill/VERSION.
"""
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CONFIG = HERE.parents[1] / "docs" / "roadmap" / "tech-tree.yaml"

if __name__ == "__main__":
    cmd = [sys.executable, str(HERE / "skill" / "scripts" / "check.py"), "--config", str(CONFIG), *sys.argv[1:]]
    sys.exit(subprocess.call(cmd))
