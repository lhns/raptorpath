Status §42 Next-1: the store law's per-window noise, plus the RTprop-filter check. Offline and read-only.
LD = <worktree>/raptorpath/docs/l1-raw/loop-diag. Run from this directory or with absolute paths; Python 3, stdlib only.

s42noise.py   python -I s42noise.py LD > s42noise.out
  Per-cap tables for p27 c2/c8/c1s/c1d and p34 c1s/c1d (LIVE DEF rows; [LOOPDIAG] windows t >= 1.0 s with n > 0;
  ask_w = 1.1 sum L_i (rtprop_us + rtt_p50_us - rtt_min_us)). It gives the map and fixed point (reproducing ask27 /
  fixed_point27), cv of ask_w per row, and sign agreement at W = 1, mov2/4/8, blk2/4/8 and row means.
  Its "W*" lines use the map margin and ignore between-row variance. Do not cite them; use s42wstar.out.

s42wstar.py   python -I s42wstar.py LD > s42wstar.out   (imports s42noise.py from this directory)
  At the caps around each fixed point: model and empirical W*, decisions per transfer third, D's consecutive same-sign
  share, the dead band, and the open-loop jitter prediction. The authoritative W*. Ignore its 8.1e7 at p34 c1s 384
  (outlier row, not a bracket cap).

s42step.py    python -I s42step.py LD/p29/diag-ld > s42step.out
  p29 PWR rows: lag correlation of d ln(window-mean nbar) against dS at lags 0-4. Not resolvable at 250 ms.

s42rtprop.py  python -I s42rtprop.py LD > s42rtprop.out   (imports s42noise.py)
  The row-median map with RTprop := each row's and leg's lowest post-warm-up rtt_min_us (an expired 10 s filter under
  a standing queue), with S unchanged. Prints the original and new map per cap, Tmin - rtprop_us per leg, the
  classifier and every crossing.
