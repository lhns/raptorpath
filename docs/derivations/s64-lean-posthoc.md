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
- **Multiplicity:** with seven readouts, the chance that at least one reaches 0.033 under the null is about 0.2, even before allowing for correlation between them.
- **Co-movement:** the same pairs carry the lean. Five of the eleven pairs are worse on all four delay readouts.
- **Limit:** pairing removes slot-to-slot drift, but not the survivor selection of the live RB rows.

## Read 3: stage R's noise floor (live RB rows only)

Each half-per-seed split takes 2 of 5 s42 rows plus 3 of 6 s7 rows and compares them with the rest: 200 splits, 5 rows against 6. The real RC-vs-RB comparison is 12 against 11. Its noise is therefore roughly q95 / 1.45 (sqrt((1/5 + 1/6) / (1/12 + 1/11)), a normal approximation). So "x < q95" is a lenient screen, not evidence of no effect.

| readout | q95\|x\| (5 v 6) | ≈ floor (12 v 11) | stage R x | stage G c2 q95 (600 MB) |
|---|---|---|---|---|
| goodput | 1.2 % | 0.8 % | −0.7 % | 0.2 % |
| cpu_cli/GB | 7.2 % | 5.0 % | +4.7 % | 3.2 % |
| cpu_srv/GB | 6.4 % | 4.4 % | +1.0 % | 3.7 % |
| lag_E_cli | 12.6 % | 8.7 % | +13.5 % | 26.5 % |
| lag_E_srv | 23.7 % | 16.3 % | +20.0 % | 73.0 % |
| in-order delay mean | 19.3 % | 13.3 % | +12.6 % | 3.0 % |
| rtp_floor_p0 | 8.8 % | 6.0 % | +5.7 % | 8.0 % |

**Answer.**
- **The lag readouts are not unreadable at stage R.** At 100 MB Realtime the RB half-split q95 is 13 % (cli) and 24 % (srv). That is tighter than stage G's 600 MB Bulk c2 (27 % and 73 %), so the stage G table does not carry over to this link. That part of Fable's reading does not hold here.
- **Only lag_E_cli exceeds its floor.** Its x (+13.5 %) is just above its own 5-v-6 q95, and about 1.5 times the scaled floor.
- **lag_E_srv** is above the scaled floor (1.2 times) but inside q95.
- **In-order delay and the floor** sit at about 0.95 times the scaled floor, inside q95.

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
- lag_E_cli clears its own stage R noise floor.
- All four delay readouts move together, on the same pairs.

**The delivery-path hypothesis.** Commit 8's `WindowDelivery::offer` rewrite is the only delivery-path change. It stays the hypothesis to keep in view. A Realtime-only effect on delivered delay is not ruled out by the Bulk reads, because stage G is a different point on the dials.

**What a registered follow-up should score: the in-order delay mean.**
- It is the delivered-delay readout guard v2 adopted (§41d), and its domain (reliable delivery) covers ρ = 1.
- Stage R's rows (100 MB, about 12 s of ticks) are below the 30 s floor that gives it resolution. The follow-up therefore needs Realtime c2 rows at ρ = 1 long enough for at least 30 s of ticks: at about 68 Mbit/s, at least about 260 MB; 600 MB, about 70 s, would match stage G.
- RB and RC should be paired by slot, with the paired sign or rank test registered beforehand.
- Its noise floor should be measured from BASE before CAND is read.

**What it should not score:**
- `lag_E_cli` can ride along as a reported second readout. Its resolution at 100 MB is real but does not survive the jump to long rows (stage G c2: 27 %).
- `rtp_floor_p0` should not be scored. The owner made it reported-only, and it is a minimum over two unrelated moments, not a path reading (§41d, §53 Erratum 1).
