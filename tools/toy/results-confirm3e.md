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
