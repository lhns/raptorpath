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

s43res.py     python -I s43res.py LD > s43res.out   (imports s42noise.py from this directory)
  The residence-time store law ask = g·ΣLᵢ(R̄ − (Tminᵢ − RTpropᵢ)), R̄ = occupancy/D, g ∈ {0.8, 0.9, 1.0, 1.1}, on the p27
  c2/c8/c1s and p34 c1s static DEF rows. occupancy = [DIAG] win= numerator (a snapshot at the tick); D = Σ legs [LOOPDIAG]
  dlv_rate (delivered symbols/s = datagrams/s, L's units), cross-checked against the [DIAG] cum= ack delta. Per cap: the
  Little check (R̄, n/D, srtt snapshot mean as the only mean-loop-time proxy, Tp50, τ), the map and every crossing under a
  fresh (rtprop_us) and an expired (row floor of rtt_min_us) anchor, goodput fraction and Tp50 at the stable point, ask/n
  above the knee, the floor (ask_1/n, g_min, the g that reaches the knee; oracle L and L̂ = D), and per-window noise at the
  bracketing caps. c1s is printed twice: all LIVE rows (as s42) and with goodput outlier rows dropped (< 0.6x the cap's
  other rows: p34 DEF384 s42r1, p27 DEF192 s42r1, the 70 s row). c8 is one row per cap at most caps.
  A row's last window is dropped when the store has drained (occ < 0.1 cap at the transfer end; 4 windows, listed).

s43gain.py    python -I s43gain.py LD > s43gain.out   (imports s42noise, s42rtprop, s43res from this directory)
  The registered law G·ΣL(RTprop + S) at p27 c2 and c8, fresh and expired anchor: the gate (G = 1.1, reproduces 164/157/927
  and 324/287/492), (1) a debiased brake b0 = mean(Tmin − RTprop) over the two smallest caps added to the time term at
  G = 1.1, (2) no debias at G = 1.2. Fixed point, slope, band, G/Gmax, Tp50, every crossing, ask/n above the knee.

s43tau.py     python -I s43tau.py LD > s43tau.out   (imports s42noise, s43res, s43gain from this directory)
  ask = G·ΣL(RTprop + S + τ) at p27 c2 and c8, G ∈ {0.85, 0.9, 0.95, 1.0} × τ ∈ {0, 2.5, 5, 7.5, 10} ms, fresh and expired
  anchor: stable point, slope, band, G/Gmax, Tp50, the expired map's upward crossings, ask/n at 1 024 / 2 048 (c2's grid
  ends at 1 024), the point ×233.5/195 (§43's closed-loop offset) with its band, and OK pairs.

s43win.py     python -I s43win.py LD <worktree root> > s43win.out   (imports s42noise, s42rtprop, s43res; reads oracle43 via
  git show github/harness43). (1) §43b's pair rule applied to the recomputed s43tau sweep -> the chosen (G, τ) with its
  offline-to-×1.2 band, G/Gmax and Tp50. (2) Per cell: b0, β(n) = Tmin − expired anchor, θ(n) (exact map bound; the Little
  form beside it) and the window [τ_lo, min(τ_trap, τ_band)) for G = 0.85–1.0, for c2, c8 (thin) and c1s (p34 truncated;
  oracle43's uncapped [OCAP] windows binned by n for the bounds near and above the knee). (3) Whether τ = const, k·RTprop
  or k·b0 fits every cell with one coefficient.

s43win2.py    python -I s43win2.py LD <worktree root> > s43win2.out   (imports s43win; reads oracle43 via github/harness43)
  (1) Closed-loop line T(n) = a + b·(n − 163) of the law's time term (OCAP rtprop+S) on oracle43 c2 SL / SL-G12 / SL-B0
  last-third windows, residuals, n* = G·L·(a − 163b + τ)/(1 − G·L·b) per arm against measured, the (0.9, 5 ms) prediction,
  and ask/n under (0.9, 5) on the SH rows' windows. (2) c windows for τ = b0 + c. (3) c1s settling points at G = 0.9,
  τ = 3.5 / 5 on oracle43 bins and p34. (4) qsp_us (P90 − P50 of the last 256 RTT samples; code lines cited) against b0.
