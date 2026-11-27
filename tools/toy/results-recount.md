# Recount against the no-queue arm, and the 13.10 dequeue-time falsifier (2026-11-27)

Re-read of stored runs and committed readings, plus one rerun of stored configurations (part 2). No rule, bar or law changes.

## Part 1: the B' verdict against today's Realtime, recounted on the no-queue ("stored") arm alone

**Which verdict.** The phrase "7 of 8" is not written in any results file. It is read here as: build B' against today's Realtime in the eight cells of three-knobs-revision4.md 12. In the six headroom cells B' is identical to A trace for trace (`results-confirm13.md`, "Headroom identity"), so those cells carry build A's reading. Sources, current word per cell: CL-long `results-confirm3e.md`; CLw-long, CLc, CLc-v, CL-long-v `reread-confirm-4cells.md` (CL-long-v supplementary set `supp-confirm-cllongv.md`); OV-long, OV-long-v, OV-long-m `results-confirm13.md`; OV-long-q `results-confirm13q.md`. CL had no committed reading (its first read stopped on an empty window; the reader fix came after): it is re-read here with `reread_confirm.py CL` on the stored runs, the registered reader unchanged.

Every cell has both arms; no cell lacks a no-queue arm. ADR-0084 and Bulk lines do not depend on the arm and are not repeated. K-OV has no Realtime arm (reported only).

| cell | readout(s) | verdict vs engine-queue arm | verdict vs no-queue arm | rested only on the engine-queue arm? |
|---|---|---|---|---|
| CL-long | whole-run on time / recovery / steady p50, delta in {0.5, 5, 50} | better (on time +0.0145, bar 0.0038); recovery, p50 equivalent | the same | no: the two arms' rows are identical (the FIFO never fills at lambda < C) |
| CL (re-read here) | same | better (on time +0.0291, bar 0.0075); recovery equivalent; p50 not read (empty window, registration finding) | the same | no: identical rows |
| CLw-long | same | better (on time +0.0174, bar 0.0075); recovery, p50 equivalent | the same | no: identical rows |
| CLc | steady on time / steady p50 | equivalent | the same | no: identical rows |
| CLc-v | steady on time / p50 (reported only) | equivalent at delta 0.5, 5, 50; better at delta 0.005 (on time +0.0030, bar 0.00015) | the same | no: identical rows |
| CL-long-v | whole-run on time / recovery / p50 (reported only) | **mixed: on time better (+0.046 to +0.048), recovery shown worse (-2.5 s)** as registered; the post-data supplementary set reads recovery equivalent | the same | no: identical rows. This is the one cell of eight that is not "equivalent or better" as registered |
| OV-long | dominance: share / on time / p50, best point T = 0.4 | dominates (share -0.0059, on time +0.803, p50 0.635 s better) | **dominates** (share -0.0051, on time -0.0051, both within bar; p50 0.078 s better) | no, but against the no-queue arm the word rests on p50 alone |
| OV-long-v | dominance on share / on time within 2 s, p50 reported only, T = 1 | equivalent (share -0.0017; p50 0.09 s worse, reported only) | equivalent (share -0.0017; p50 0.65 s worse, reported only) | no |

**Count.** It depends on which mapping "7 of 8" stands for; the per-cell words above hold under any of them.
- B' over the eight 12 cells (headroom cells by identity with A): 7 of 8 equivalent or better against the no-queue arm alone, as against both arms; the cell that is not is CL-long-v (registered recovery "shown worse"; 8 of 8 on its post-data supplementary set). This is an inference: `results-confirm3e.md` item 3 names misses for A (OV-long) and B (OV-long-v), not CL-long-v. Under this mapping no "better" word rests only on the engine-queue arm, and the count "unchanged" in the commit title holds only for it.
- If the count includes build A in the OV cells, or OV-long-m / OV-long-q, words do move (table below): A's OV-long "dominates" exists only against the engine-queue arm, and B''s "trade" in OV-long-m and OV-long-q becomes "worse".
- Under any mapping: OV-long's "dominates" for B' rests, against the no-queue arm, on 0.078 s of delay p50 alone (share and on time within their bars). The +0.80 on time and the 0.6 s delay lead exist only against the engine-queue arm.
- The CL re-read also reads ADR-0084 at rho = 1 "undetermined" on whole-run on time; a doubling of that arm is owed by the registered rule (not run here).

**Outside the eight (the 13.x cells and build A), the words that do rest on the engine-queue arm:**

| cell / build | vs engine-queue arm | vs no-queue arm |
|---|---|---|
| OV-long-m, B' (delta 0.1) | a trade (share -0.0099, on time +0.799, p50 0.538 s better) | worse (share -0.0090, p50 0.026 s worse) |
| OV-long-q, B' (delta 0.118) | a trade (share -0.0099, on time +0.799, p50 0.55 s better) | worse (share -0.0092, p50 0.012 s worse) |
| OV-long / OV-long-m / OV-long-q, build A | dominates | worse (p50 0.217 s worse) |
| OV-long-v, build A | equivalent | equivalent |

## Part 2: the 13.10 falsifier (written before the rerun's 6-seed read)

**Claim under test (three-knobs-revision4.md 13.10.1).** Item 2's excess at T = 1 and 2 is the descent's shed moved into pre-onset windows by hand-over binning. Binned by dequeue time, item 2 should fall to the steady term d_ss.

**Prediction (13.10.3, OV-long-q, d_ss from each row's own rates), against the hand-over reading it replaces:**

| T | band l (s) | predicted, dequeue binning (= d_ss) | measured, hand-over binning (13.10.3) |
|---|---|---|---|
| 0.4 | 0 | 0.0009 | 0.0010 +- 0.0003 |
| 0.6 | 0 | 0.0011 | 0.0008 +- 0.0005 |
| 1 | 0.38 | 0.0011 | 0.0047 +- 0.0004 |
| 2 | 1.22 | 0.0017 | 0.0077 +- 0.0005 |

**Reading rule (fixed before the read).**
- The stored runs keep only hand-over series, so the rows are rerun: OV-long-q, A and B' at T in {0.4, 0.6, 1, 2}, H = 0, 6 seeds (48 runs), the stored configurations unchanged. Each run must reproduce its stored `delivered`, and the hand-over item 2 recomputed from its per-seq arrays must equal the stored series' reading.
- Dequeue time of a unit: the time of the emit call that takes it off the intake. A sent unit's is its first send time; a shed unit (dequeue or intake expiry) is dropped in the emit call that sends the next unit, so its dequeue time is that unit's send time (toy2 `emit`).
- Windows: item 2's set (13.9.5) on the dequeue-time axis: 0.5 s windows from the A-paired window's start to the last window that ends at or before the seed-mean onset (`onset_t`, itself a dequeue-time event).
- Primary readout: per window, (delivered_A - delivered_B') / (lambda * 0.5) over the units dequeued in it, averaged over the windows, then over seeds. d_ss is a delivered-rate difference over lambda, so this is the form it predicts. B''s dequeue count per window is not lambda * 0.5 (its cap moves with x-hat); the share form delivered/dequeued, which carries that term, is reported next to it.
- Bar: two seed standard errors of the per-seed mean (13.10.4).
- Word per row: **pass** if |measured - d_ss| <= bar and the hand-over reading lies outside measured +- bar; **no power** if both fit; **fail** if d_ss lies outside the bar. T = 0.4 and 0.6 have no band, so both binnings predict the same number there: they are controls and read "no power" by construction if they agree.
- Disclosure: seed 0 at T = 0.4 and 2 was read from the two stored per-seq pickles of 13.10 while building the reader (dequeue-binned share form 0.0012 and 0.0028; delivered-difference form at T = 2 about 0.0018).

### The read (after the rerun)

48 runs, 707 s wall at 4 workers on this host (`python d1310/deq_item2.py run OUTDIR --workers 4`, then `read OUTDIR`; OUTDIR is scratch, not committed). Every run reproduced its stored `delivered`, and the hand-over item 2 from its per-seq arrays equals the stored series' reading at 6 of 6 seeds in every row; on these window ids the 6-seed hand-over means are 0.00096 / 0.00078 / 0.00466 / 0.00759, reproducing 13.10.3.

**Correction after the read: the registered primary readout is blind to the alternative.** Under overload the link is full, so B''s delivered count per dequeue window is set by C and the hole account (in seed 0 at T = 2 it equals B''s sends per window in every window), not by how much is shed. An extra pre-onset shed would show only in B''s dequeue count, never in its delivered count. So the primary (delivered difference over lambda W) tests only that the hole account closes; the binning claim is tested by the share form (delivered/dequeued, which carries the dequeue count) and by the dequeue surplus directly. The row words below are moved to the share form after the read, with its own 2 se; the primary is kept as the hole-account check.

| T | windows (dequeue time) | share form (2 se) | dequeue surplus sum(n_B' - n_A), seed mean (per seed) | surplus an early shed would need | d_ss (predicted) | hand-over reading | word (share form) | primary, hole account (2 se) |
|---|---|---|---|---|---|---|---|---|
| 0.4 | [3.0, 4.5] s, 3 | 0.00072 (0.00052) | -2 (5 12 11 -21 -3 -14) | 0 (no band) | 0.0009 | 0.00096 | no power (control) | 0.00080 (0.00009): fail, marginal (0.0009 rounded; 0.00089 from the row's rates, the miss equals the bar) |
| 0.6 | [4.0, 6.0] s, 4 | 0.00067 (0.00057) | -20 (-19 -21 -25 -17 -20 -18) | 0 (no band) | 0.0011 | 0.00078 | no power (control) | 0.00135 (0.00056) |
| 1 | [6.0, 9.5] s, 7 | 0.00131 (0.00066) | +15 (26 9 -17 35 44 -8) | about +150 | 0.0011 | 0.00466 | **pass** | 0.00105 (0.00035) |
| 2 | [11.5, 18.0] s, 13 | 0.00085 (0.00145) | -62 (115 -210 -202 -94 171 -152) | about +470 | 0.0017 | 0.00759 | **pass** | 0.00165 (0.00019) |

"Surplus an early shed would need" = (hand-over reading - d_ss) x lambda x the windows' span.

**Outcome.** The falsifier does not fire. At T = 1 and 2, the rows that can tell the two explanations apart, item 2 binned by dequeue time reads 0.0013 and 0.0009 (share form), within two seed errors of the steady term and far outside the hand-over readings (0.0047, 0.0076). B' dequeues no more units than A before the onset (+15 and -62 against the +150 and +470 an early shed would need). 13.10.1's mechanism holds: in dequeue time the cap sheds nothing extra before the onset, and the hand-over excess was the binning. The primary readout closes on d_ss at T = 0.6, 1 and 2, and misses marginally at T = 0.4 (a control, hole account only; recorded, not tuned).
- The prediction block above and this read were committed together; the share-form words were chosen after the read (see the correction).
- Scope: OV-long-q only. OV-long-m was not rerun (another 48 runs, about 12 min here).
