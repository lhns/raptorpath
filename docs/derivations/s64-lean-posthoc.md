# §64 stage R delay lean: three free reads (POST HOC)

**Everything here is post hoc.** None of it was registered, none of it is a verdict, and it does not reopen the F2 merge (§64 Result: GATE-41d PASS, RT-PASS). No VM, no cargo. It reads the committed ledger `docs/l1-raw/s64/s64-session.tgz` (sha256 `49d0deff…a233`) with the committed scorers on `harness64` `4feeb8fd` (`tools/l1/`). The scorers are not on main, so a rerun needs that checkout:

    git archive github/harness64 tools/l1 | tar x -C <dir>
    python -I -X utf8 docs/derivations/s64-lean-posthoc/s64_lean_posthoc.py <dir>/tools/l1 docs/l1-raw/s64/s64-session.tgz

Script: `s64-lean-posthoc/s64_lean_posthoc.py`. Output: `s64-lean-posthoc/s64_lean_posthoc.out`; two runs gave byte-identical output.

**Reproduction first.** The script loads the rows the way each scorer does, so row order and the RNG stream are the same. It asserts the committed numbers before printing anything new:
- all 22 stage G clause p's;
- stage G's c2 resolution lines;
- all 7 stage R READOUTS lines, x and p.

All of them reproduce.

## Read 1: `rtp_floor_p0` on stage G's four Bulk cells

The statistic is the guard's own: within-seed rank sums over the same 10 000 seeded relabellings, one-sided, with the floor clauses switched back on. The draws do not depend on the clause set, so these p's ride the guard's draws. q95 is the BASE half-split resolution, built the same way as the RESOLUTION lines.

| cell | BASE med (µs) | CAND med (µs) | x | p | q95\|x\| | paired: CAND worse |
|---|---|---|---|---|---|---|
| c1s | 2672 | 2673 | +0.04 % | 0.48 | 3.7 % | 6/12 |
| c1d | 2585 | 2642 | +2.2 % | 0.17 | 5.8 % | 7/12 |
| c2 (600 MB) | 11567 | 12209 | +5.6 % | 0.52 | 8.0 % | 6/12 |
| c8 (600 MB) | 8256 | 8678 | +5.1 % | 0.38 | 22.6 % | 8/12 |

The second legs are c1d p1 at +2.4 % (p 0.16) and c8 p1 at −1.6 % (p 0.83).

**Answer.** On leg 0 the sign is non-negative in all four cells, but there is no signal:
- c1s is a tie;
- every x is inside its cell's half-split q95;
- every rank p is between 0.17 and 0.52;
- the slot-paired split (read 1b) is a coin flip in every cell.

A uniform sign with this little behind it is what four noisy readings of nothing look like one time in 16 (one in 8 if c1s counts as a tie).

## Read 2: stage R paired by slot

There are 11 pairs. RC s42 r4 is dropped because its RB partner died. In this ledger the two arms of a slot run back to back. Sign test, ties dropped (there were none):

| readout | RC worse | median paired d | sign p, one-sided | sign p, two-sided |
|---|---|---|---|---|
| goodput (control) | 3/11 | −1.1 % | 0.97 | 0.23 |
| cpu_cli/GB (control) | 6/11 | +0.7 % | 0.50 | 1.00 |
| cpu_srv/GB (control) | 6/11 | +0.4 % | 0.50 | 1.00 |
| lag_E_cli | 8/11 | +15.0 % | 0.11 | 0.23 |
| lag_E_srv | 7/11 | +14.4 % | 0.27 | 0.55 |
| in-order delay mean | **9/11** | +29.8 % | **0.033** | 0.065 |
| rtp_floor_p0 | **9/11** | +6.4 % | **0.033** | 0.065 |

The in-order delay rows are all under the 30 s tick floor.

**Answer.**
- **Pairing sharpens the lean on two readouts:** in-order delay and the floor, each 9 of 11 at one-sided 0.033.
- **Multiplicity:** with seven independent readouts, the chance that at least one reaches 0.033 under the null would be about 0.2. The readouts are positively correlated, which lowers that chance, but it also makes two of them landing together unsurprising.
- **Co-movement:** the same pairs carry the lean. Five of the eleven pairs are worse on all four delay readouts.
- **Limit:** pairing removes slot-to-slot drift, but not the survivor selection of the live RB rows.

## Read 2b: where in the run the pairs differ (supplement, counts only)

Read from each stage R row's own client log (`rt64/diag-ld`). The script first checks that these logs reproduce each row's `rtp_floor_p0` and `inorder_mean` exactly.

- **The floor is one start-up sample.** On all 22 rows (11 RB, 11 RC), `rtp_floor_p0` is the run's first non-zero `rtp_us`, at t ≈ 0.3 s.
  - Copa's 10 s windowed `min_rtt` never rolls on a 12 s row, so the source-phase variant (t ≥ 1 s, src > 0) returns the same value.
  - At stage R the floor is therefore not a minimum over two moments. It reads the first RTT sample after warm-up, and F2 is a warm-up fix.
  - RC is higher in 9 of 11 pairs, by about 0.6 ms at the median.
- **The in-order gap runs through the whole row.** Split at each row's middle tick, RC is worse in 9 of 11 pairs on the first half of the ticks and 8 of 11 on the second half. The gap is not confined to start-up.

## Read 3: stage R's noise floor (live RB rows only)

Each half-per-seed split takes 2 of 5 s42 rows plus 3 of 6 s7 rows and compares them with the rest: 200 splits, 5 rows against 6.

This is a screen, not a calibrated floor:
- the real comparison is 12 rows against 11, so it is tighter than a split;
- RC's own spread is not RB's.

The calibrated numbers stay the committed rank p's and read 2's sign p's.

| readout | RB q95\|x\| (5 v 6) | stage R x | x / q95 | spread (max−min)/med, RB | RC | stage G c2 q95 (600 MB) |
|---|---|---|---|---|---|---|
| goodput | 1.2 % | −0.7 % | −0.61 | 0.03 | 0.03 | 0.2 % |
| cpu_cli/GB | 7.2 % | +4.7 % | +0.66 | 0.21 | 0.18 | 3.2 % |
| cpu_srv/GB | 6.4 % | +1.0 % | +0.15 | 0.23 | 0.18 | 3.7 % |
| lag_E_cli | 12.6 % | +13.5 % | +1.07 | 0.24 | 0.73 | 26.5 % |
| lag_E_srv | 23.7 % | +20.0 % | +0.84 | 0.73 | 1.31 | 73.0 % |
| in-order delay mean | 19.3 % | +12.6 % | +0.65 | 0.61 | 0.84 | 3.0 % |
| rtp_floor_p0 | 8.8 % | +5.7 % | +0.66 | 0.35 | 0.48 | 8.0 % |

**Answer.**
- **Stage G's table is the wrong basis for stage R.** RB's own half-split q95 at 100 MB Realtime is 13 % (lag_E_cli) and 24 % (lag_E_srv), against stage G's 27 % and 73 % on 600 MB Bulk c2.
- **Every readout except lag_E_cli is inside its q95.** lag_E_cli sits just at it (1.07).
- **RC is visibly wider than RB on every delay readout.** On lag_E_cli the spread is 0.73 against 0.24. This is reported, not interpreted.

## Plain words

**Nothing here shows a real regression. The lean is not pure noise either.**

**Against a regression:**
- In Bulk, where in-order delay resolves (c2 q95 3 %), CAND is *better*: −1.3 %, rank p 0.998, better in 10 of 12 slot pairs.
- No Bulk cell shows the floor moving beyond its noise.
- The fix's measured cost is nanoseconds per symbol. The gaps are about 0.6 ms (the floor) and about 15 ms (in-order delay).
- The live RB rows are survivors of the F2 draw.
- Seven correlated readouts were looked at.

**For a lean:**
- Pairing turns two stage R readouts into 9-of-11 splits.
- The in-order gap holds on both halves of the rows, not just at start-up.
- lag_E_cli sits at its own RB q95.
- All four delay readouts move together, on the same pairs.

**The delivery-path hypothesis.** Commit 8's `WindowDelivery::offer` rewrite is the only delivery-path change. It stays the hypothesis to keep in view. A Realtime-only effect on delivered delay is not ruled out by the Bulk reads, because stage G is a different point on the dials.

**What a registered follow-up should score: the in-order delay mean.**
- It is the delivered-delay readout guard v2 adopted (§41d), and its domain (reliable delivery) covers ρ = 1.
- Read 2b shows the gap is not a start-up artefact, so the whole-run mean is the right form; no start-up exclusion is needed.
- Stage R's rows (100 MB, about 12 s of ticks) are below the 30 s floor that gives it resolution. The follow-up therefore needs Realtime c2 rows at ρ = 1 long enough for at least 30 s of ticks: at about 68 Mbit/s, at least about 260 MB; 600 MB, about 70 s, would match stage G.
- RB and RC should be paired by slot, with the paired sign or rank test registered beforehand.
- Its noise floor should be measured from BASE before CAND is read.

**What it should not score:**
- `lag_E_cli` can ride along as a reported second readout, not a scored one. RB's q95 is 13 % at 100 MB Realtime but 27 % on 600 MB Bulk c2, and long rows may not keep it readable.
- `rtp_floor_p0` should not be scored. The owner made it reported-only (§41d). On these rows it is a single start-up RTT sample at t ≈ 0.3 s (read 2b), not a path reading. Its 9-of-11 lean is a warm-up observation; F2 changed warm-up, so it can be reported beside the follow-up, not scored.
