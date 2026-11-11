#!/usr/bin/env python3
"""Absolute checks for the §36 amendment-7 re-score (byte-floored trains)."""
import disp36 as D
from disp36_rescore7 import estimate, bw_median

R = 12.5e6  # bytes/s


def busy_run(n, b, t0=0):
    out, t = [], t0
    for _ in range(n):
        t += round(b * D.NS / R)
        out.append((t, b))
    return out


def test_busy_run_reads_the_rate_exactly():
    sk = busy_run(20000, 6278)  # 10 s at 12.5 MB/s
    mx, wm, cv = estimate(sk)
    assert abs(mx / R - 1) < 1e-3 and abs(wm / R - 1) < 1e-3 and cv < 1e-3, (mx / R, wm / R, cv)


def test_small_skbs_do_not_shorten_the_floor():
    # alternate 6278 B data and 40 B acks: the byte floor spans >= 4 data skbs, so it still reads R
    sk, t = [], 0
    for i in range(40000):
        b = 6278 if i % 2 == 0 else 40
        t += round(b * D.NS / R)
        sk.append((t, b))
    assert bw_median([b for _, b in sk]) == 6278
    mx, wm, _ = estimate(sk)
    assert abs(wm / R - 1) < 2e-3 and mx / R < 1.01, (mx / R, wm / R)


if __name__ == "__main__":
    test_busy_run_reads_the_rate_exactly()
    test_small_skbs_do_not_shorten_the_floor()
    print("ok")
