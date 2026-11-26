# Confirmation (three-knobs-revision4.md section 12) on the offline simulation

The chain `confirm3e.py` appends one section per cell below, in the registered order (A/A floors, CL-long, CL, CLw-long, CLc, CLc-v, CL-long-v, OV-long, OV-long-v). Nothing here is read from an earlier stage's stored rows.

## Reading choices made while building the reader (2026-11-26, before any data of this part)

- **Build A / build B / baselines.** A = S5 (T, H sweep of registration 13.2) + the clamp (cwnd_sim = the cell's store pause; for today's Realtime, which has none, the same B) + `floor_repeat = "evidenced_overdue"` + `elig = "rack"`. B = A + the wait limit, no send gate. ADR-0084, today's Realtime (stored arm and engine-queue arm) and today's Bulk carry the clamp and `elig = "rack"`; only the new model has the floor repeat. Today's Realtime's resends are all report-driven, so `elig = "rack"` moves nothing there (a test shows `resend_by_sweep` = 0 with the rule off).
- **The steady window** is per comparison and per seed: start = the later of the two arms' (max(fill, first RTT sample) + 1 s), fill = the first dequeue expiry (A, B), the first store pause (ADR-0084, Bulk), the first link-queue drop (stored Realtime arm), the first FIFO-full instant (engine-queue arm); an arm whose queue never fills starts one second after its first RTT sample. End = the last hand-over minus tau. In an outage cell the window excludes [outage start, the later of the two recovery ends]; a run that never recovers excludes the rest of the run.
- **Per-run series.** Every run stores, per 0.5 s hand-over window, the units handed over, delivered, delivered within 0.4 s and within 2 s, and a 1 %-resolution histogram of the delays of the delivered units (units never delivered or never sent count as infinite delay). Steady p50 is the median over the units handed over in the window, infinite when fewer than half arrive. The histogram bin (1 %) is below the 5 % delay bar. One stored run serves every cell that builds the same configuration (OV-long and OV-long-v share today's Realtime at delta 50), so the on-time counts are stored for both taus.
- **Matched share.** Outage cells (CL, CL-long, CLw-long): the axis is the whole-run delivered share; the readouts are the whole-run share on time within tau, the registered recovery time (censored at 14.5 s) and the steady p50 on the paired window. Headroom cells without an outage (CLc, CLc-v): the axis is the steady delivered share, the readouts the steady on time and the steady p50. The reason is that in an outage cell every row's steady delivered share is about 1 and does not vary with T, so only the whole-run share (which carries the outage loss) spans a range. The new curve is build A's H = 0 sweep (merged within max(NF_share, 1/units)), interpolated per seed at the baseline's axis value, a baseline below the curve's lowest share reading the lowest point (as in amendment 1.6c).
- **Dominance** (overload cells, delta = 0.005 video cells with delay reported only): each baseline against the build's best finite-T point. Best = the highest seed-mean steady on-time share; points within that readout's bar of the top are tied and the lower seed-mean p50 wins, then the smaller T. "Dominates" = no readout shown worse and at least one shown better; "equivalent" = none of either; "undetermined" if any readout is undetermined; otherwise "not dominated (a trade)".
- **Bars.** Shares max(0.05 (1 - share_ref), 1/units, NF); p50 max(NF_p50, 0.05 p50_ref); recovery max(0.05 ref, 0.1 s, NF). NF = the p95 over the first n seeds of |A - A'| on the paired window (A = build A at the cell's primary lifetime, A' its phase variant), n in {3, 6, 12}; a doubled comparison (24 or 12 seeds) uses the 12-seed floor, a comparison at fewer seeds the floor at the nearest count not above it. Only the primary-lifetime pair is run (the registered count of 96 rows has room for one pair per cell); the stage 3e floors, measured without the clamp, are not used. They are frozen to `out3e/confirm/confirm_nf.json` before any comparison is read.
- **Doubling.** One doubling: the arms of a comparison read undetermined run their seeds again to twice the count (at most 24), for those arms only, before the next cell; the cell is then read again with the doubled arms.
- **Gates (stop rules, `out3e/confirm/STOP`).** Every new-model run finishes; l1_viol, commit_viol, worth_viol_rep (new model) and l1_viol (ADR-0084) are zero; the absolute carrier check (`rep_fire_viol_carrier`) and the eligibility check (`elig_viol`) are zero; in H = 0 rows the receiver-floor check (`rep_supp_viol`) is zero (H = infinity rows hold the floor by design and are reported only); in a headroom cell B equals A at three seeds on every id3e key and the trace (a difference stops; amended before any data of this part, 2026-11-26: `n_tau_lt_T` and `deq_early_would` are reported per row, not a stop, so a live wait limit without effect is visible); S5 at T = infinity, H = infinity equals Bulk under the clamp and `elig = "rack"` on every key and trace, at every seed. The check of 10.7 on a suppression whose carrier is reported arrived has no witness in K-OV, OV, CL or CL-long (it reads 0 checks), so the liveness gates and the timeout witness carry the guarantee.
- **Labelled.** CL-long's build A rows are post-data (step 2, three-knobs-revision4.md 9.1) and confirm nothing. The video cells' p50 is reported only. The window bracket (cwnd_sim = B = 1 000 binds in the drain; at 2 048 it never binds) is stated with the CL-long reading.

- **A finding from the scratch smoke run (not data of this part).** In CL-long at T = 2, seed 2, with 0.75 of the units, build B's wait limit was live for 151 ticks (`n_tau_lt_T` = 151) with identical traces to build A, so no behaviour changed. Step 1 had read 0 ticks in CL-long (seed 0, full units); the smoke run contradicts that for this seed and scale. It is reported here as a finding, and the per-row count is now in every table.

## Confirmation (three-knobs-revision4.md 12): the A/A floors under build A (2026-11-26)

```
{
 "CL-long": {
  "3": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 3.899999999993353e-05,
   "wot": 3.899999999993353e-05
  },
  "6": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 7.750000000006363e-05,
   "wot": 7.750000000006363e-05
  },
  "12": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 0.0013202500000000616,
   "wot": 0.0013202500000000616
  }
 },
 "CL": {
  "3": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 7.799999999997808e-05,
   "wot": 7.799999999997808e-05
  },
  "6": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 0.00015499999999993297,
   "wot": 0.00015499999999993297
  },
  "12": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 0.002640499999999951,
   "wot": 0.002640499999999951
  }
 },
 "CLw-long": {
  "3": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 4.375000000000906e-05,
   "wot": 4.375000000000906e-05
  },
  "6": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 7.187499999994351e-05,
   "wot": 7.968750000000857e-05
  },
  "12": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 7.781249999997963e-05,
   "wot": 8.125000000003268e-05
  }
 },
 "CLc": {
  "3": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 9.00000000005896e-06,
   "wot": 9.00000000005896e-06
  },
  "6": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 7.500000000049134e-06,
   "wot": 7.500000000049134e-06
  },
  "12": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 1.0000000000065512e-05,
   "wot": 1.0000000000065512e-05
  }
 },
 "CLc-v": {
  "3": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 0.0,
   "wot": 0.0
  },
  "6": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 4.173479080012447e-05,
   "recovery": 0.0,
   "wshare": 0.0,
   "wot": 0.0
  },
  "12": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 2.5040874480074644e-05,
   "recovery": 0.0,
   "wshare": 0.0,
   "wot": 0.0
  }
 },
 "CL-long-v": {
  "3": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.09999999999999964,
   "wshare": 0.00017799999999996708,
   "wot": 0.00018449999999998746
  },
  "6": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.09999999999999964,
   "wshare": 0.00017499999999998073,
   "wot": 0.00018374999999998254
  },
  "12": {
   "share": 0.0,
   "ot": 0.0,
   "p50": 0.0,
   "recovery": 0.09999999999999964,
   "wshare": 0.000169000000000008,
   "wot": 0.00018224999999997272
  }
 },
 "OV-long": {
  "3": {
   "share": 0.00012876712328766527,
   "ot": 0.0001301369863013657,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 0.0001083333333333325,
   "wot": 0.000110416666666735
  },
  "6": {
   "share": 0.00012328767123287454,
   "ot": 0.00012671232876712568,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 0.00010520833333332869,
   "wot": 0.00010729166666673118
  },
  "12": {
   "share": 0.0001160958904109388,
   "ot": 0.00011986301369864559,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 9.895833333332103e-05,
   "wot": 0.00010104166666672352
  }
 },
 "OV-long-v": {
  "3": {
   "share": 2.2115384615373212e-05,
   "ot": 2.2115384615373212e-05,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 1.4375000000033111e-05,
   "wot": 1.4375000000033111e-05
  },
  "6": {
   "share": 2.7243589743569663e-05,
   "ot": 2.7243589743569663e-05,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 1.4062500000022737e-05,
   "wot": 1.4062500000022737e-05
  },
  "12": {
   "share": 2.532051282049807e-05,
   "ot": 2.532051282049807e-05,
   "p50": 0.0,
   "recovery": 0.0,
   "wshare": 1.3437500000001989e-05,
   "wot": 1.3437500000001989e-05
  }
 }
}
```

### CL-long

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

Labelled post-data: build A's CL-long rows (and the clamp rows of the new model there) were sized in step 2 (three-knobs-revision4.md 9.1) before this registration; they confirm nothing. What is new here is the baselines under the clamp and elig = rack.
| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 24 | 0.9396 | 0.9396 | 0.00 | 0.006 | 46 | 46 | 4586 (3795) | 0 | 0 | 0 |
| A; lifetime T=0p6, hold 0 (S5) | 24 | 0.9499 | 0.9075 | 0.51 | 0.007 | 37 | 37 | 4745 (3639) | 0 | 0 | 0 |
| A; lifetime T=0p8, hold 0 (S5) | 24 | 0.9599 | 0.8583 | 1.51 | 0.007 | 29 | 29 | 4912 (3494) | 0 | 0 | 0 |
| A; lifetime T=1, hold 0 (S5) | 24 | 0.9697 | 0.8091 | 2.50 | 0.007 | 22 | 22 | 5065 (3341) | 0 | 0 | 0 |
| A; lifetime T=1p2, hold 0 (S5) | 24 | 0.9798 | 0.7599 | 3.47 | 0.008 | 14 | 14 | 5225 (3188) | 0 | 0 | 0 |
| A; lifetime T=1p25, hold 0 (S5) | 24 | 0.9825 | 0.7478 | 3.72 | 0.008 | 11 | 11 | 5255 (3140) | 0 | 0 | 0 |
| A; lifetime T=1p3, hold 0 (S5) | 24 | 0.9850 | 0.7355 | 3.97 | 0.008 | 9 | 9 | 5309 (3117) | 0 | 0 | 0 |
| A; lifetime T=1p35, hold 0 (S5) | 24 | 0.9876 | 0.7233 | 4.22 | 0.008 | 7 | 7 | 5339 (3067) | 0 | 0 | 0 |
| A; lifetime T=1p4, hold 0 (S5) | 24 | 0.9899 | 0.7107 | 4.45 | 0.008 | 5 | 5 | 5382 (3035) | 0 | 0 | 0 |
| A; lifetime T=1p45, hold 0 (S5) | 24 | 0.9930 | 0.6953 | 4.77 | 0.005 | 4 | 4 | 5438 (2993) | 0 | 0 | 0 |
| A; lifetime T=1p5, hold 0 (S5) | 24 | 0.9953 | 0.6849 | 4.97 | 0.005 | 2 | 2 | 5585 (2973) | 0 | 0 | 0 |
| A; lifetime T=1p55, hold 0 (S5) | 24 | 0.9956 | 0.6719 | 5.25 | 0.005 | 1 | 1 | 5662 (2923) | 0 | 0 | 0 |
| A; lifetime T=1p6, hold 0 (S5) | 24 | 0.9959 | 0.6613 | 5.45 | 0.004 | 1 | 1 | 5752 (2890) | 0 | 0 | 0 |
| A; lifetime T=2, hold 0 (S5) | 24 | 0.9990 | 0.6368 | 5.94 | 0.002 | 0 | 0 | 6428 (2856) | 0 | 6 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 24 | 1.0000 | 0.6312 | 6.06 | 0.000 | 0 | 0 | 6673 (2845) | 0 | 0 | 0 |
| A; lifetime T=0p4, hold inf (S5) | 12 | 0.9396 | 0.9365 | 0.00 | 0.006 | 46 | 46 | 4572 (3784) | 0 | 0 | 0 |
| A; lifetime T=1p4, hold inf (S5) | 12 | 0.9897 | 0.6821 | 5.09 | 0.008 | 6 | 6 | 5365 (3013) | 0 | 0 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.6015 | 6.69 | 0.000 | 0 | 0 | 6661 (2824) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold 0 (S5) | 3 | 0.9399 | 0.9399 | 0.00 | 0.006 | 46 | 46 | 4630 (3843) | 0 | 0 | 0 |
| B; lifetime T=2, hold 0 (S5) | 3 | 0.9990 | 0.6384 | 5.87 | 0.002 | 0 | 0 | 6490 (2928) | 0 | 50 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 0.6328 | 6.00 | 0.000 | 0 | 0 | 6722 (2903) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9940 | 0.6448 | 5.67 | 0.011 | 4018 | 0 | 5292 (2474) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9940 | 0.6610 | 5.35 | 0.011 | 3339 | 0 | 5308 (2570) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 1954 | 0 | 5236 (2659) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 429 | 0 | 5241 (2661) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 24 | 0.9941 | 0.6787 | 4.98 | 0.011 | 0 | 0 | 5317 (2676) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.6015 | 6.69 | 0.000 | 0 | 0 | 6661 (2824) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.047 [0.0441, 0.0501] | 0.0178 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0.827 [0.766, 0.893] | 0.283 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.00037 [0.000355, 0.000385] | 0.000302 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0.0305 [0.0277, 0.0336] | 0.0169 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | 0.505 [0.446, 0.568] | 0.268 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000361 [0.000344, 0.000379] | 0.000302 | shown better |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | 0.0126 [0.00991, 0.0154] | 0.0161 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | 0.135 [0.0757, 0.2] | 0.249 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000277 [0.00026, 0.000292] | 0.0003 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | 0.0125 [0.00989, 0.0154] | 0.0161 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | 0.134 [0.075, 0.199] | 0.249 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000262 [0.000248, 0.000277] | 0.000299 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | 0.0123 [0.0107, 0.0143] | 0.0161 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | 0.129 [0.0861, 0.178] | 0.249 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000266 [0.000255, 0.000276] | 0.000299 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0145 [0.0142, 0.0149] | 0.00375 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0145 [0.0142, 0.0149] | 0.00375 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0145 [0.0142, 0.0149] | 0.00375 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000223 [0.000214, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0145 [0.0142, 0.0149] | 0.00375 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000223 [0.000214, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0145 [0.0142, 0.0149] | 0.00375 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000209 [0.000194, 0.000223] | 0.000291 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0145 [0.0142, 0.0149] | 0.00375 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000209 [0.000194, 0.000223] | 0.000291 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | whole-run on time | outside the swept range (12 seeds) | 0.0199 | outside the swept range |
| bulk; never gives up, strict order = today's Bulk (C3) | recovery | outside the swept range (12 seeds) | 0.335 | outside the swept range |
| bulk; never gives up, strict order = today's Bulk (C3) | steady p50 | outside the swept range (12 seeds) | 0.000297 | outside the swept range |
### CL
No reading: an empty steady window in CL seed 0

## 2026-11-26 Two reader fixes found by the first reads (CL-long, CL); re-read only, no runs

1. **Bulk by identity.** In CL-long Bulk read "outside the swept range": a strict-order row sits outside the any-order H = 0 curve. It is now read by identity against S5 at T = inf, H = inf (stage 3e's rule): identical on every id3e key and the whole trace at every seed (12 of 12 in CL-long and CL), outcome "equivalent (identity)". The object API is not in these cells. In the overload cells the dominance reading of Bulk is unchanged (best finite-T point against Bulk).
2. **CL, "empty steady window", cause and finding.** Not a missing gauge and not a start after the end: the exclusion of 12.2 ("[outage start, the later recovery end]") runs to the end of the run for every arm whose recovery time is censored. In a 10 s run (CL) the build-A rows at T >= 1.3 (9 of 12 seeds at T = 1.3, all 12 at T >= 1.35) and every ADR-0084 row never recover within the readable end, so the paired window with them is empty: 12.2's definition yields no steady window there. Registration finding: in CL the steady p50 against ADR-0084 and the late-T build-A points cannot be read; the outage's cost is carried by the whole-run on time and the registered recovery time, which are read as before. The reader now reports such a comparison as "no steady window ... not read (registration finding)" row by row (an empty window is NaN, not an exception), and the other rows of the cell are read.

### CLw-long

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 24 | 0.8675 | 0.8675 | 0.00 | 0.010 | 105 | 105 | 3402 (2927) | 0 | 0 | 0 |
| A; lifetime T=0p6, hold 0 (S5) | 24 | 0.8778 | 0.8491 | 0.10 | 0.010 | 97 | 97 | 3491 (2881) | 0 | 0 | 0 |
| A; lifetime T=0p8, hold 0 (S5) | 24 | 0.8878 | 0.8293 | 0.46 | 0.010 | 89 | 89 | 3576 (2835) | 0 | 0 | 0 |
| A; lifetime T=1, hold 0 (S5) | 24 | 0.8975 | 0.8091 | 0.86 | 0.010 | 81 | 81 | 3659 (2785) | 0 | 0 | 0 |
| A; lifetime T=1p2, hold 0 (S5) | 24 | 0.9077 | 0.7895 | 1.23 | 0.010 | 73 | 73 | 3753 (2743) | 0 | 0 | 0 |
| A; lifetime T=1p25, hold 0 (S5) | 24 | 0.9103 | 0.7847 | 1.33 | 0.010 | 71 | 71 | 3782 (2739) | 0 | 0 | 0 |
| A; lifetime T=1p3, hold 0 (S5) | 24 | 0.9129 | 0.7798 | 1.42 | 0.010 | 69 | 69 | 3799 (2724) | 0 | 0 | 0 |
| A; lifetime T=1p35, hold 0 (S5) | 24 | 0.9154 | 0.7748 | 1.53 | 0.010 | 67 | 67 | 3817 (2707) | 0 | 0 | 0 |
| A; lifetime T=1p4, hold 0 (S5) | 24 | 0.9177 | 0.7697 | 1.63 | 0.011 | 65 | 65 | 3834 (2691) | 0 | 0 | 0 |
| A; lifetime T=1p45, hold 0 (S5) | 24 | 0.9203 | 0.7647 | 1.73 | 0.011 | 63 | 63 | 3867 (2691) | 0 | 0 | 0 |
| A; lifetime T=1p5, hold 0 (S5) | 24 | 0.9228 | 0.7598 | 1.83 | 0.011 | 61 | 61 | 3886 (2678) | 0 | 0 | 0 |
| A; lifetime T=1p55, hold 0 (S5) | 24 | 0.9251 | 0.7547 | 1.93 | 0.011 | 59 | 59 | 3905 (2665) | 0 | 0 | 0 |
| A; lifetime T=1p6, hold 0 (S5) | 24 | 0.9277 | 0.7497 | 2.03 | 0.011 | 57 | 57 | 3925 (2652) | 0 | 0 | 0 |
| A; lifetime T=2, hold 0 (S5) | 24 | 0.9478 | 0.7102 | 2.82 | 0.011 | 41 | 41 | 4105 (2566) | 0 | 0 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 24 | 1.0000 | 0.5929 | 5.19 | 0.004 | 0 | 0 | 5670 (2367) | 0 | 0 | 0 |
| A; lifetime T=0p4, hold inf (S5) | 12 | 0.8674 | 0.8638 | 0.00 | 0.010 | 106 | 106 | 3396 (2925) | 0 | 0 | 0 |
| A; lifetime T=1p4, hold inf (S5) | 12 | 0.9177 | 0.7558 | 2.02 | 0.011 | 65 | 65 | 3828 (2698) | 0 | 0 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.5779 | 5.51 | 0.004 | 0 | 0 | 5661 (2370) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold 0 (S5) | 3 | 0.8677 | 0.8677 | 0.00 | 0.010 | 105 | 105 | 3401 (2957) | 0 | 0 | 0 |
| B; lifetime T=2, hold 0 (S5) | 3 | 0.9479 | 0.7104 | 2.80 | 0.011 | 41 | 41 | 4077 (2581) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 0.5921 | 5.20 | 0.004 | 0 | 0 | 5673 (2405) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9928 | 0.6002 | 5.02 | 0.015 | 3385 | 0 | 4305 (2080) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9928 | 0.6090 | 4.86 | 0.015 | 2645 | 0 | 4313 (2122) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9928 | 0.6157 | 4.72 | 0.015 | 1357 | 0 | 4266 (2121) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9928 | 0.6157 | 4.72 | 0.015 | 435 | 0 | 4266 (2122) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9928 | 0.6154 | 4.73 | 0.015 | 0 | 0 | 4383 (2114) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3513 (3146) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3513 (3146) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3427 (3070) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3427 (3070) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3360 (3009) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3360 (3009) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 24 | 1.0000 | 0.5779 | 5.53 | 0.004 | 0 | 0 | 5670 (2367) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.00879 [0.00749, 0.0102] | 0.02 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0.148 [0.131, 0.173] | 0.251 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.00022 [0.000208, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | -0.000168 [-0.00124, 0.00105] | 0.0195 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | -0.0121 [-0.0392, 0.0138] | 0.243 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000225 [0.000216, 0.00023] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | -0.00686 [-0.00798, -0.00561] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | -0.154 [-0.172, -0.129] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000205 [0.00019, 0.000219] | 0.000291 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | -0.00686 [-0.00798, -0.00561] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | -0.154 [-0.172, -0.129] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000196 [0.000181, 0.00021] | 0.000291 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | -0.00661 [-0.00734, -0.00593] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | -0.146 [-0.17, -0.12] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000196 [0.000181, 0.00021] | 0.000291 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0174 [0.0171, 0.0176] | 0.0075 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0174 [0.0171, 0.0176] | 0.0075 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.00017 [0.000156, 0.000184] | 0.000289 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.00017 [0.000156, 0.000184] | 0.000289 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | whole-run on time | 0.0151 [0.0139, 0.0163] | 0.0211 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | recovery | 0.346 [0.308, 0.383] | 0.277 | shown better |
| bulk; never gives up, strict order = today's Bulk (C3) | steady p50 | 0.000194 [0.000183, 0.000206] | 0.000291 | equivalent |

### CLc

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2147) | 0 | 0 | 0 |
| A; lifetime T=1, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| A; lifetime T=0p4, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2147) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2357 (2137) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2361 (2139) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2361 (2139) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1864 | 0 | 2380 (2141) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1864 | 0 | 2382 (2143) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1862 | 0 | 2382 (2142) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1862 | 0 | 2386 (2147) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2384 (2147) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |

Matched share. Headroom cell: the axis is the steady delivered share on the paired window. The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000199 [0.000185, 0.000214] | 0.000291 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000219 [0.000204, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000219 [0.000204, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000199 [0.000185, 0.000214] | 0.000291 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000199 [0.000185, 0.000214] | 0.000291 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | steady p50 | 0.000199 [0.000185, 0.000214] | 0.000291 | equivalent |

### CLc-v

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time within 2s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=1, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=1p5, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=3, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3257 (0) | 0 | 0 | 0 |
| B; lifetime T=2, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3257 (0) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3257 (0) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 338 | 0 | 3414 (354) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 338 | 0 | 3414 (354) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 337 | 0 | 3413 (353) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 334 | 0 | 3409 (350) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9971 | 0.9971 | - | 0.001 | 0 | 0 | 2983 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p005 (E-rt) | 12 | 0.9971 | 0.9971 | - | 0.001 | 0 | 0 | 2983 (0) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | 12 | 0.9971 | 0.9971 | - | 0.001 | 0 | 0 | 2983 (0) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |

Matched share. Headroom cell: the axis is the steady delivered share on the paired window. The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady on time | 0.00295 [0.00286, 0.00304] | 0.000148 | shown better |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.00465 [0.00462, 0.00468] | 0.000513 | shown better (p50 reported only) |
| rt; today's Realtime, delta=0p5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000237 [0.000228, 0.000251] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000237 [0.000228, 0.000251] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000223 [0.000204, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000223 [0.000204, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=50 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000204 [0.000189, 0.000218] | 0.000291 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000204 [0.000189, 0.000218] | 0.000291 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=0p005 (E-rt) | steady on time | 0.00295 [0.00286, 0.00304] | 0.000148 | shown better |
| rt; today's Realtime, delta=0p005 (E-rt) | steady p50 | 0.00465 [0.00462, 0.00468] | 0.000513 | shown better (p50 reported only) |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | steady on time | 0.00295 [0.00286, 0.00304] | 0.000148 | shown better |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | steady p50 | 0.00465 [0.00462, 0.00468] | 0.000513 | shown better (p50 reported only) |
| bulk; never gives up, strict order = today's Bulk (C3) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | steady p50 | 0.0107 [0.0106, 0.0108] | 0.000815 | shown better (p50 reported only) |

### CL-long-v

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time within 2s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=1, hold 0 (S5) | 12 | 0.9708 | 0.9708 | 2.50 | 0.017 | 22 | 22 | 6517 (0) | 0 | 0 | 0 |
| A; lifetime T=1p5, hold 0 (S5) | 12 | 0.9953 | 0.9953 | 4.74 | 0.010 | 2 | 2 | 6907 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold 0 (S5) | 12 | 0.9993 | 0.9991 | 5.62 | 0.003 | 0 | 0 | 7904 (95) | 0 | 11 | 0 |
| A; lifetime T=3, hold 0 (S5) | 12 | 1.0000 | 0.9989 | 5.67 | 0.003 | 0 | 0 | 8039 (69) | 0 | 11 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 0.9989 | 5.67 | 0.003 | 0 | 0 | 8039 (69) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 12 | 0.9708 | 0.9708 | 3.53 | 0.017 | 22 | 22 | 6517 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold inf (S5) | 12 | 0.9993 | 0.9981 | 6.59 | 0.003 | 0 | 0 | 7904 (95) | 0 | 11 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.9849 | 6.73 | 0.003 | 0 | 0 | 8039 (69) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 3 | 0.9707 | 0.9707 | 2.50 | 0.016 | 23 | 23 | 6554 (0) | 0 | 0 | 0 |
| B; lifetime T=2, hold 0 (S5) | 3 | 0.9993 | 0.9991 | 5.57 | 0.003 | 0 | 0 | 7936 (101) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 0.9990 | 5.60 | 0.004 | 0 | 0 | 8055 (79) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9923 | 0.9923 | 5.50 | 0.012 | 2422 | 0 | 6469 (413) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9923 | 0.9923 | 5.13 | 0.012 | 1576 | 0 | 6517 (452) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9922 | 0.9922 | 4.83 | 0.014 | 348 | 0 | 6320 (295) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9921 | 0.9921 | 4.83 | 0.015 | 128 | 0 | 6043 (69) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9921 | 0.9921 | 4.80 | 0.015 | 0 | 0 | 6020 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p005 (E-rt) | 12 | 0.9223 | 0.9223 | 0.00 | 0.001 | 0 | 0 | 5554 (0) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | 12 | 0.9223 | 0.9223 | 0.00 | 0.001 | 0 | 0 | 5554 (0) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.9849 | 6.73 | 0.003 | 0 | 0 | 8039 (69) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000385 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 1.03 [0.97, 1.1] | 0.275 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.00592 [0.00586, 0.00597] | 0.000579 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000384 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | 0.663 [0.593, 0.743] | 0.257 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.00568 [0.00561, 0.00575] | 0.000567 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000389 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | 0.372 [0.301, 0.451] | 0.242 | shown better |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.00553 [0.0055, 0.00557] | 0.000561 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000395 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | 0.382 [0.312, 0.462] | 0.242 | shown better |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.00556 [0.00553, 0.00559] | 0.000562 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000396 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | 0.351 [0.301, 0.395] | 0.24 | shown better |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.00556 [0.00553, 0.00559] | 0.000562 | shown better (p50 reported only) |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000138 [0.000124, 0.000153] | 0.000291 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000138 [0.000124, 0.000153] | 0.000291 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=0p005 (E-rt) | whole-run on time | 0.0484 [0.0481, 0.0487] | 0.00388 | shown better |
| rt; today's Realtime, delta=0p005 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=0p005 (E-rt) | steady p50 | 0.0169 [0.0162, 0.0175] | 0.00113 | shown better (p50 reported only) |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | whole-run on time | 0.0484 [0.0481, 0.0487] | 0.00388 | shown better |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | steady p50 | 0.0169 [0.0162, 0.0175] | 0.00113 | shown better (p50 reported only) |
| bulk; never gives up, strict order = today's Bulk (C3) | whole-run on time | 0.014 [0.0126, 0.0156] | 0.000757 | shown better |
| bulk; never gives up, strict order = today's Bulk (C3) | recovery | 1.06 [0.958, 1.15] | 0.336 | shown better |
| bulk; never gives up, strict order = today's Bulk (C3) | steady p50 | 0.0112 [0.0111, 0.0113] | 0.000844 | shown better (p50 reported only) |

### OV-long

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 6 | 0.8120 | 0.8120 | - | 0.993 | 0 | 0 | 400 (106) | 0 | 387146 | 386602 |
| A; lifetime T=0p6, hold 0 (S5) | 6 | 0.8160 | 0.0380 | - | 0.993 | 0 | 0 | 706 (144) | 0 | 386852 | 385706 |
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8239 | 0.0380 | - | 0.993 | 0 | 0 | 1340 (232) | 0 | 386320 | 384046 |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8438 | 0.0380 | - | 0.993 | 0 | 0 | 2950 (425) | 0 | 384762 | 379763 |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0380 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| A; lifetime T=0p4, hold inf (S5) | 6 | 0.8120 | 0.8120 | - | 0.993 | 0 | 0 | 400 (106) | 0 | 387146 | 386602 |
| A; lifetime T=2, hold inf (S5) | 6 | 0.8438 | 0.0261 | - | 0.993 | 0 | 0 | 2950 (425) | 0 | 384762 | 379763 |
| A; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.0261 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold 0 (S5) | 6 | 0.8045 | 0.8045 | - | 0.993 | 0 | 0 | 12680 (1610) | 0 | 373677 | 0 |
| B; lifetime T=0p6, hold 0 (S5) | 6 | 0.8045 | 0.8045 | - | 0.993 | 0 | 0 | 12684 (1609) | 0 | 373679 | 0 |
| B; lifetime T=1, hold 0 (S5) | 6 | 0.8045 | 0.8044 | - | 0.993 | 0 | 0 | 12695 (1615) | 0 | 373771 | 0 |
| B; lifetime T=2, hold 0 (S5) | 6 | 0.8045 | 0.8042 | - | 0.993 | 0 | 0 | 12694 (1614) | 0 | 373769 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0380 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold inf (S5) | 6 | 0.8045 | 0.8045 | - | 0.993 | 0 | 0 | 12680 (1610) | 0 | 373677 | 0 |
| B; lifetime T=2, hold inf (S5) | 6 | 0.8045 | 0.7908 | - | 0.993 | 0 | 0 | 12694 (1614) | 0 | 373769 | 0 |
| B; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.0261 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9974 | 0.0306 | - | 0.995 | 11089 | 0 | 14467 (277) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9974 | 0.0337 | - | 0.994 | 0 | 0 | 14467 (309) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 3 | 0.8105 | 0.8099 | - | 0.741 | 0 | 0 | 42 (4) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 3 | 0.8244 | 0.0403 | - | 0.792 | 0 | 0 | 9 (4) | 1 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 3 | 0.8104 | 0.8098 | - | 0.746 | 0 | 0 | 44 (4) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 3 | 0.8243 | 0.0400 | - | 0.749 | 0 | 0 | 11 (4) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.0266 | - | 0.994 | 0 | 0 | 15739 (1989) | 0 | 0 | 0 |

Dominance (12.3): each baseline against the build's best finite-T point per seed pairing (the highest seed-mean steady on-time share; ties within its bar broken by the lower p50, then by the smaller T). Dominates = not worse beyond the bar on any readout and better beyond it on at least one.

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 5.33 [5.33, 5.33] | not dominated (a trade) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 4.74 [4.74, 4.74] | not dominated (a trade) |
| A | rt; today's Realtime, delta=0p5 (E-rt) | T=0p4 | -0.00345 [-0.00358, -0.00326] | -0.00346 [-0.00359, -0.00326] | -0.217 [-0.217, -0.217] | not dominated (a trade) |
| A | rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | T=0p4 | -0.0043 [-0.00442, -0.00414] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | rt; today's Realtime, delta=50 (E-rt) | T=0p4 | -0.00345 [-0.00358, -0.00326] | -0.00346 [-0.00359, -0.00326] | -0.217 [-0.217, -0.217] | not dominated (a trade) |
| A | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=0p4 | -0.00423 [-0.00431, -0.00416] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.195 [-0.195, -0.195] | 0.805 [0.805, 0.805] | 4.98 [4.95, 5] | not dominated (a trade) |
| B | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.194 [-0.195, -0.194] | 0.803 [0.803, 0.803] | 5.57 [5.57, 5.57] | not dominated (a trade) |
| B | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.194 [-0.195, -0.194] | 0.803 [0.803, 0.803] | 4.98 [4.98, 4.98] | not dominated (a trade) |
| B | rt; today's Realtime, delta=0p5 (E-rt) | T=0p4 | -0.00507 [-0.00516, -0.00488] | -0.00507 [-0.00516, -0.00488] | 0.0784 [0.0784, 0.0784] | dominates |
| B | rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | T=0p4 | -0.00591 [-0.006, -0.00575] | 0.803 [0.803, 0.803] | 0.635 [0.635, 0.635] | dominates |
| B | rt; today's Realtime, delta=50 (E-rt) | T=0p4 | -0.00507 [-0.00516, -0.00488] | -0.00507 [-0.00516, -0.00488] | 0.0784 [0.0784, 0.0784] | dominates |
| B | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=0p4 | -0.00584 [-0.00589, -0.00577] | 0.803 [0.803, 0.803] | 0.635 [0.635, 0.635] | dominates |
| B | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.197 [-0.197, -0.197] | 0.803 [0.803, 0.803] | 5.22 [5.19, 5.24] | not dominated (a trade) |

### OV-long-v

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time within 2s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8277 | 0.8277 | - | 0.994 | 0 | 0 | 1528 (89) | 0 | 387518 | 351900 |
| A; lifetime T=1p5, hold 0 (S5) | 6 | 0.8377 | 0.8377 | - | 0.994 | 0 | 0 | 2353 (89) | 0 | 386695 | 326849 |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8476 | 0.8476 | - | 0.994 | 0 | 0 | 3192 (89) | 0 | 385854 | 301708 |
| A; lifetime T=3, hold 0 (S5) | 6 | 0.8676 | 0.2072 | - | 0.995 | 0 | 0 | 4893 (89) | 0 | 384147 | 252075 |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.2072 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 6 | 0.8277 | 0.8277 | - | 0.994 | 0 | 0 | 1528 (89) | 0 | 387518 | 351900 |
| A; lifetime T=2, hold inf (S5) | 6 | 0.8476 | 0.8476 | - | 0.994 | 0 | 0 | 3192 (89) | 0 | 385854 | 301708 |
| A; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.1861 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 6 | 0.8083 | 0.8083 | - | 0.994 | 0 | 0 | 11132 (90) | 0 | 376479 | 0 |
| B; lifetime T=1p5, hold 0 (S5) | 6 | 0.8145 | 0.8145 | - | 0.994 | 0 | 0 | 11375 (89) | 0 | 377769 | 0 |
| B; lifetime T=2, hold 0 (S5) | 6 | 0.8345 | 0.8345 | - | 0.994 | 0 | 0 | 11954 (89) | 0 | 377494 | 0 |
| B; lifetime T=3, hold 0 (S5) | 6 | 0.8659 | 0.2072 | - | 0.994 | 0 | 0 | 13031 (89) | 0 | 376802 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.2072 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| B; lifetime T=1, hold inf (S5) | 6 | 0.8083 | 0.8083 | - | 0.994 | 0 | 0 | 11132 (90) | 0 | 376479 | 0 |
| B; lifetime T=2, hold inf (S5) | 6 | 0.8345 | 0.8345 | - | 0.994 | 0 | 0 | 11954 (89) | 0 | 377494 | 0 |
| B; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.1861 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9971 | 0.1848 | - | 0.995 | 11086 | 0 | 14480 (6) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9971 | 0.2062 | - | 0.995 | 0 | 0 | 14472 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p005 (E-rt) | 3 | 0.8108 | 0.8108 | - | 0.800 | 0 | 0 | 46 (0) | 1 | 0 | 0 |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | 3 | 0.8249 | 0.8249 | - | 0.755 | 0 | 0 | 15 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 3 | 0.8104 | 0.8104 | - | 0.746 | 0 | 0 | 44 (4) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 3 | 0.8243 | 0.8243 | - | 0.749 | 0 | 0 | 11 (4) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.1876 | - | 0.995 | 0 | 0 | 16122 (85) | 0 | 0 | 0 |

Dominance (12.3): each baseline against the build's best finite-T point per seed pairing (the highest seed-mean steady on-time share; ties within its bar broken by the lower p50, then by the smaller T). Dominates = not worse beyond the bar on any readout and better beyond it on at least one.

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=1 | -0.189 [-0.189, -0.189] | 0.779 [0.779, 0.779] | 4.88 [4.84, 4.9] | not dominated (a trade) (p50 reported only) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=1 | -0.189 [-0.189, -0.189] | 0.751 [0.751, 0.751] | 4.27 [4.24, 4.29] | not dominated (a trade) (p50 reported only) |
| A | rt; today's Realtime, delta=0p005 (E-rt) | T=1 | -5.38e-05 [-0.000191, 0.00021] | -5.38e-05 [-0.000191, 0.00021] | -0.821 [-0.821, -0.821] | equivalent (p50 reported only) |
| A | rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | T=1 | -5.47e-05 [-0.000215, 0.000116] | -5.47e-05 [-0.000215, 0.000116] | -0.259 [-0.264, -0.256] | equivalent (p50 reported only) |
| A | rt; today's Realtime, delta=50 (E-rt) | T=1 | -5.38e-05 [-0.000191, 0.00021] | -5.38e-05 [-0.000191, 0.00021] | -0.821 [-0.821, -0.821] | equivalent (p50 reported only) |
| A | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=1 | 1.16e-05 [-5.91e-05, 9.41e-05] | 1.16e-05 [-5.91e-05, 9.41e-05] | -0.264 [-0.264, -0.264] | equivalent (p50 reported only) |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=1 | -0.192 [-0.192, -0.192] | 0.776 [0.775, 0.776] | 4.49 [4.45, 4.5] | not dominated (a trade) (p50 reported only) |
| B | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=1 | -0.212 [-0.213, -0.212] | 0.74 [0.74, 0.74] | 5.14 [5.11, 5.16] | not dominated (a trade) (p50 reported only) |
| B | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=1 | -0.212 [-0.213, -0.212] | 0.713 [0.712, 0.713] | 4.52 [4.51, 4.54] | not dominated (a trade) (p50 reported only) |
| B | rt; today's Realtime, delta=0p005 (E-rt) | T=1 | -0.0235 [-0.0237, -0.0234] | -0.0235 [-0.0237, -0.0234] | -0.503 [-0.515, -0.495] | not dominated (a trade) (p50 reported only) |
| B | rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | T=1 | -0.0235 [-0.0236, -0.0235] | -0.0235 [-0.0236, -0.0235] | 0.0584 [0.0496, 0.0631] | not dominated (a trade) (p50 reported only) |
| B | rt; today's Realtime, delta=50 (E-rt) | T=1 | -0.0235 [-0.0237, -0.0234] | -0.0235 [-0.0237, -0.0234] | -0.503 [-0.515, -0.495] | not dominated (a trade) (p50 reported only) |
| B | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=1 | -0.0235 [-0.0235, -0.0234] | -0.0235 [-0.0235, -0.0234] | 0.0535 [0.0423, 0.0625] | not dominated (a trade) (p50 reported only) |
| B | bulk; never gives up, strict order = today's Bulk (C3) | T=1 | -0.215 [-0.215, -0.215] | 0.737 [0.736, 0.737] | 4.75 [4.72, 4.78] | not dominated (a trade) (p50 reported only) |
