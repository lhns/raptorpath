#!/usr/bin/env python3
"""Launcher for the guard-statistic acceptance tests (guard47_checks.py). The checks run in the main process only: the resolution step may start
a multiprocessing pool, and on spawn platforms (Windows) its workers re-import the main module, which would otherwise run the whole test again
inside every worker (a respawn cascade that exhausts memory). Run: python3 test_guard47.py"""
import os
import runpy
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

if __name__ == "__main__":
    sys.path.insert(0, HERE)
    runpy.run_path(os.path.join(HERE, "guard47_checks.py"), run_name="__guard47_checks__")
