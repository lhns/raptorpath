#!/usr/bin/env python3
"""THE QUANTILE-NATIVE α-SWEEP's reporter — a SHIM, and deliberately nothing
more. `alpha_report.py` was extended ADDITIVELY for the new axis (the `W7`
liveness column and the UNSEPARATED-BY-CONSTRUCTION section), so there is ONE
scorer and no second dialect of it to drift."""
import os
import runpy
import sys

sys.argv[0] = "alpha_report.py"
runpy.run_path(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                            "alpha_report.py"), run_name="__main__")
