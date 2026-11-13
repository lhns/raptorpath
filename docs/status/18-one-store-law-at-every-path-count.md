## 18. One store law at every path count (ADR-0072) — pre-registration

Status §16 finding 3 and paper §6.1 ("One path is not a case") name the
defect: `pooled_store_cap` returned `None` below two live paths, so one path
ran `clamp(2.0·BtlBw·RTprop, 10, 1024)`, a different law whose ceiling binds
in every post-warm-up c1s window. That is a path-count step of the pattern
CLAUDE.md forbids (per-path cap ×1.80 at the c1 anchor, ×0.55 at the sc2
anchor across N = 1 → 2). The code phase is done and is the subject of this
pre-registration; the battery has **not** run. Committed before any VM
measurement of the new binary (the VM was used for build and unit-test
suites only).

**The change** (`net/store_cap.rs`, `net/sender_phases.rs`; no wire change,
no engine crate outside `net`): `pooled_store_cap` applies
`clamp((1 + q(δ))·Σ bwᵢ·RTpropᵢ, floor, N·knee)` at every N ≥ 1; the three
default-off arms lose the same guard (`capw_store_cap`'s `terms.len() < 2`,
the pool-anchor `n_live >= 2`, the honest arm's `N·knee if N ≥ 2 else
store_max`); the Copa-sole seat shares the function. `RELIABLE_STORE_MAX` and
the gain 2.0 remain only on `RWM_STORE_PATHS=0` (and the boot `min()`).
`[SUMCAP] pin=` and `floor=` now report at N = 1. Red-first tests
(`net::tests::one_store_law_at_n1`): absolute N = 1 values at the three
presets, linearity from N = 1, δ continuity at N = 1, refresh routing,
bind gauges, control exactness, no-op at N ≥ 2.

**The design.** One binary. N = 1 control = `RWM_STORE_PATHS=0`, which runs
main's `clamp(2·bdp, 10, 1024)` chain bit-for-bit (pinned by
`control_arm_is_main_s_single_path_chain_exactly`); N ≥ 2 is a no-op by
construction (pinned by `n_two_and_up_is_unchanged_on_a_sampled_grid`).
All arms: shipped defaults otherwise, `RWM_GEN=0 RWM_DIAG=1`, window
pipeline, one invocation per fresh topology, `perf_rwm_c.sh`, harness
patterns of `threadd9_*`, both VM locks.

| leg | cells | arms | n | hint |
|---|---|---|---|---|
| scored | `c1s-400` | LAW vs CTL (`RWM_STORE_PATHS=0`), interleaved, arm order rotated per block | 6 per arm (seeds 42, 7; 3 reps each) | Bulk |
| control-moved | `c1d-400`, `c2-100`, `c8-100` | LAW only | 6 (seeds 42, 7) | Bulk |
| δ | `c1s-400`, `c2-100` | LAW only | 3 per (cell, hint), seed 42 | Auto, Realtime |

**Amendment (committed before any battery VM contact; the first draft of
this section was wrong in one place).** c2 is N = 2, where this change is a
no-op by construction (pinned by test), so LAW at c2 is main. CTL
(`RWM_STORE_PATHS=0`) at c2 is *not* main: it is the pre-ADR-0058 chain
(`clamp(2·Σ, 10, 1024)`). A LAW-vs-CTL contrast at c2 would score something
this change does not do. c2 therefore moves from the scored leg to the
control-moved leg; the scored leg is `c1s-400` alone.

*Witnesses* (a row without them is void, not null): the `[GATES]` echo shows
`RWM_STORE_PATHS` as set and the path-scaled-pool echo names every N ≥ 1;
`[SUMCAP]` reads `eng>0` on LAW and `eng=0` on CTL at N = 1 (rule 1: the
mechanism executes); the LAW arm's `[DIAG]` `win=` is not constant (it
tracks the law; CTL's reads the constant 1024). Control-moved: at c1d and
c8 the LAW arm is compared against the §16 rows, which were measured on the
pre-change engine, as a check that nothing moved at N ≥ 2 (a move there
would be a defect in the no-op claim).

*Abort table* (measurement-discipline pre-registration protocol 3; tokens
are fixed now so no abort is relabelled later; the result section opens with
this table, filled):

| cause | token |
|---|---|
| a VM lock not held by the envelope | `ABORT-LOCK` |
| CR bytes in the synced tree | `ABORT-CRLF` |
| binary `sha256` differs from the recorded one | `ABORT-SHA` |
| a sentinel path not write+unlink probed | `ABORT-SENTINEL-UNWRITABLE` |
| the smoke below fails | `ABORT-SMOKE` |
| the fresh build or the lib/parser tests fail in the session | `ABORT-BUILD` / `ABORT-TESTS` |
| a driver exits non-zero (row void, battery goes on) | `ABORT-RC` |
| no summary after the bring-up tries | `ABORT-BRINGUP` (row `NO_DATA`) |
| a compile or build process ran on the VM during the row | `VOID-COTENANT` |

*Smoke* (protocol 4; same binary, both arms, nothing in it is a result):
one `c1s-400` LAW and CTL invocation and one `c2-100` LAW
invocation must witness the `[GATES]` echo with `RWM_STORE_PATHS` as set,
`[SUMCAP] eng>0` on LAW (both cells) and `eng=0` on the `c1s-400` CTL, and a
non-constant `win=` on LAW; if any is missing, nothing else is launched.

*Gauges* (reported for every row, scored where named): `[SUMCAP] pin=` and
`floor=` (rule 18: the bind fraction of each clamp); per-window btlbw
over-read against delivered; `rtt` and RTprop; `[LAT]` `A_x` p50/p95 and
`q_p50`; paused %; store occupancy against the cap. The δ leg's witness:
per-window cap / (Σ bw·RTprop) equals 1 + q within one `ceil` in windows
where neither clamp binds (q = 0.050 Realtime, 0.0667 Auto).

*Scoring.* σ_pooled is the pooled within-arm standard deviation of the two
arms' reps in that cell; "2σ_pooled" is the half-width of the noise band.
**c1s outcomes** (LAW vs CTL; the first that holds in the order REFUTED,
PINNED, LATENCY-COST, GOODPUT-UP, INERT-AS-DERIVED is the verdict, and the
others that also hold are listed):

- **PINNED** (expected): LAW `[SUMCAP] pin` ≥ 0.25 — the knee binds because
  the c1 anchor over-reads ×7–9 (paper §6.1, "The c1 input"; 7 of 15 windows
  in the §16 r1 trajectory).
- **LATENCY-COST** (expected): goodput within 2σ_pooled of CTL and `A_x` p50
  up by more than 2σ_pooled — the store, and so the RTT, grows at the same
  goodput.
- **GOODPUT-UP**: goodput above CTL by more than 2σ_pooled.
- **INERT-AS-DERIVED**: goodput and `A_x` p50 both within 2σ_pooled of CTL
  and `pin` < 0.25.
- **REFUTED**: c1s goodput below CTL by more than 2σ_pooled, **or**
  `CONTROL-MOVED` at any N ≥ 2 cell (c1d, c2, c8).

**Other outcomes** (the shared taxonomy, restated with this battery's
numbers): `UNSCOREABLE` when fewer than 4 live rows remain at a scored
(cell, arm), or 2 or more witness-failed rows void the arm at that cell;
`CONTROL-MOVED` at c1d, c2 and c8 if
the LAW arm's `[SUMCAP]` is not `eng=N/N` (the cap tracking (1 + q)·Σ as on
main; c1d: cap ≈ 2734, pin 0 per §16), or its goodput median falls outside
the range of the §15 MAIN rows (c1d `677.9–755.1`, c2 `90.08–90.32`, c8
`82.9–102.9` Mbit/s; same cells, shipped default) — the no-op at N ≥ 2 is
pinned by a test, so a move there is a finding against the claim and no
other outcome is drawn from those cells;
anything else is `OUTSIDE THE PRE-REGISTERED SET`, recorded with no verdict.

**c2** is a no-op cell: it carries no scored expectation (rule 16 also
applies: its headroom is under 5 %, so no goodput target is set there).

**DECISION RULE (decided by the project lead): the law ships unless
REFUTED. PINNED and LATENCY-COST do not block, because the N = 1 step is a
no-mode-switch defect (CLAUDE.md) regardless; the c1 anchor over-read
(×7–9, paper §6.1 "The c1 input") is recorded as the next defect item.**

*Not decided here:* the anchor over-read itself, `RWM_STORE_PATH_POOL`
(stale knee), any change to q(δ). No threshold in the battery is a tuning
knob; none is changed after the data is read.

*Duration.* 12 (c1s LAW/CTL) + 18 (c1d, c2, c8 LAW) + 12 (δ leg) = 42
invocations, plus a 3-invocation smoke, at ≈ 10–20 s each (the §15 battery
ran 60 invocations in 556 s): about 15 minutes of measurement, one to two
hours of session wall with the build and lock waits, far inside the 5 h
budget.

### 18. One store law at every path count (ADR-0072) — result

Scored 2026-10-26 against the pre-registration above and its amendment,
literally; no threshold was touched after the data was read. **Verdict by
the pre-registered rule: `REFUTED` — through `CONTROL-MOVED` at `c1d-400`
and `c2-100`, not through the c1s contrast.** c1s itself reads **`PINNED`**
(expected): goodput is within the noise band, the knee binds in 70 % of
windows. The reading of the CONTROL-MOVED clauses is the open question and
is in *What this does and does not show* below; nothing ships or flips from
this section (the project lead decides).

*Binary and session.* One binary, `sha256 17418fd8…a69ff4`, built fresh on
the benchmark VM from tree `8a43cd2` (engine identical to `6323db5`; the
commits after it are the status amendment, the q2 source-pin test and the
harness). Envelope `tools/l1/storelaw_run_all.sh` (battery
`storelaw_battery.sh`, parser `storelaw_parse.py`, derived from
`threadd9_*`). The session waited 2 h 44 min for the VM locks held by
another agent (`r1-r1c`), then held both from 06:06:55Z: build 570 s, smoke
06:17:03Z, battery 06:17:03–06:25:06Z (483 s, 42 invocations, 11.5 s
each), q2 test, locks released 06:29:58Z, exit state 0 `raptorpath`, 0
`rp-*` namespaces. Ledgers: `docs/l1-raw/store-law-n1/` (`sl.log` with
every `SLROW`, `score.txt`, `smoke.log`, `smoke-check.txt`, `BINSHA.txt`,
`all-era.txt`, `all.out`, `build.log`, `q2test.log`, and the per-invocation
driver and both-endpoint logs under `diag-sl/`, `diag-smoke/`).

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` / `ABORT-CRLF` / `ABORT-SHA` / `ABORT-SENTINEL-UNWRITABLE` / `ABORT-BUILD` | no |
| `ABORT-SMOKE` | no: `SMOKE-PASS` (c1s LAW `eng=792/792` pin 0.72, `win=` 3 values; c1s CTL `eng=0/0`, `win=` constant; c2 LAW `eng=1551/1551` pin 0.00) |
| `ABORT-TESTS` | no (no cargo suite in the session; the q2 test alone, below) |
| `ABORT-RC` / `ABORT-BRINGUP` / `VOID-COTENANT` | 0 / 0 / 0 of 42 (no `RUN-RETRY`) |

*Rows.* 42 rows. LIVE 35; `WITNESS-FAIL` 7, all in the δ leg: `c1s LAW-auto`
r1 (`law-win-constant`: `win=` pinned at the 2048 knee in every
post-warm-up window — a constant, but the knee, not CTL's 1024), and 6
`LAW-realtime` rows (`window-line-missing-on-window`: the harness witness
`auto-selecting RLC windowed backend` is not printed when the hint is
named, so it is absent at Realtime; the `[PIPE]` echo reads
`pipeline=window hint=realtime`, `[SUMCAP]` engaged and `[DCAP]` read
`q=0.05` on all six). Per the void rule these rows are excluded and not
re-scoped.

*c1s-400 (scored; LAW n = 6, CTL n = 6; means).*

| | LAW | CTL |
|---|---|---|
| goodput Mbit/s | 601.9 (254.7–696.0) | 671.4 (620.5–743.1) |
| `[SUMCAP]` | `eng` 100 %, **pin 0.700** (0.56–0.90), floor 0 | `eng=0/0` |
| `win=` cap | 2048 (the knee) | 1024 (the constant) |
| `A_x` p50 µs (p95) | 25 771 (13–82 k) | 6 912 (11–16 k) |
| rtt ms / paused % / occupancy | 33.9 / 28 / 0.97 | 11.2 / 27 / 0.95 |
| btlbw anchor ÷ delivered | 15.6 | 9.2 |

Goodput LAW − CTL = −69.6 Mbit/s against σ_pooled 126.4 (2σ = 252.9), so
not `REFUTED` at c1s. `A_x` p50 +18.9 ms against σ_pooled 19.5 ms (2σ =
38.9), so not `LATENCY-COST`; not `GOODPUT-UP`; `pin` ≥ 0.25, so not
`INERT-AS-DERIVED`. Outcomes that hold: **`PINNED`** only. The pooled σ is
inflated by one LAW rep (seed 42 rep 2: 254.7 Mbit/s, pin 0.90, rtt 94 ms,
`A_x` p50 82 ms); the other five LAW reps average 671.3 against CTL 671.4,
and the `A_x` p50 excluding that rep is 14.5 ms against 6.9 ms (every LAW rep
above every CTL rep). That is an observation; the verdict above is the
pre-registered one. The anchor over-reads ×12–29 on LAW and ×8–10 on CTL, so
the law asks 1.9–2.9 k symbols and the knee answers (paper §6.1, "The c1
input": the next defect item).

*Control-moved leg (LAW only, n = 6 each; reference = the §15 MAIN
goodput range).*

| cell | goodput median [min–max] | §15 MAIN | `[SUMCAP]` | verdict |
|---|---|---|---|---|
| `c1d-400` | 547.5 [493.8–601.3] | 677.9–755.1 | `eng=N/N`, pin 0, cap 1194–1852 | **CONTROL-MOVED** |
| `c2-100` | 89.7 [88.2–90.2] | 90.08–90.32 | `eng=N/N`, pin 0, cap 555–1407 | **CONTROL-MOVED** |
| `c8-100` | 94.3 [82.5–104.1] | 82.9–102.9 | `eng=N/N`, pin 0, cap 1215–2933 | not moved |

The interior ceil identity holds at every N ≥ 2 row and every Bulk c2 row:
`[SUMCAP]` mean cap − mean ask ∈ [0.1, 0.7] with pin = floor = 0.

*δ leg.* `[DCAP] q` reads 0.0667 on all 6 Auto rows (c1s, c2) and 0.050 on
the 6 Realtime rows: the dial reaches the single path at all three hints
(the Realtime rows are voided by the harness witness above, reported for
information). The pre-registered "cap/anchor = 1 + q within one `ceil` in
interior windows" cannot be shown for the Auto rows from the gauges kept:
c1s Auto is pinned (pin 0.56–0.78) and c2 Auto is not interior (pin
0.02–0.03), so the mean cap − mean ask carries the clamped windows; the
Bulk rows, where it is interior, satisfy it (above). Realtime at c1s reads
87–151 Mbit/s (an unscored regime of its own; no CTL was run there).

*What this does and does not show.*

1. **The c1s contrast is as the derivation predicts**: the law engages at
   one path, the knee binds in 70 % of windows because the c1 anchor
   over-reads, the store and the RTT grow (rtt 11 → 34 ms, `A_x` p50 ×3.7),
   and goodput is not separated from CTL at this n. `PINNED` is a defect
   finding about the anchor (rule 18), not about the law.
2. **`CONTROL-MOVED` is applied literally and is not, on this evidence,
   attributable to the change.** At N ≥ 2 the new code is the old code (the
   sampled-grid no-op test, plus the same expression driving all three
   cells). The reference is a *different session and binary* (§15, main at
   `88476a4`, `RWM_RTOBS=1`): its c2 range is 0.24 Mbit/s wide, which a
   0.4 % shortfall (89.7 against 90.08) leaves, and `c1d` here (547) is 20
   % under §15 (728) and §16 (688–729) with an ask of 1.2–1.9 k against
   §16's 2.7 k, i.e. a different anchor trajectory in the same code path.
   Session and VM-era drift (the box ran at load 4–5 beside the previous
   agent's work) is the first suspect, **[H]**; it is not shown. What
   *would* decide it is a same-session main-binary control at `c1d-400`
   and `c2-100` (n = 6, interleaved with LAW), which the pre-registration
   did not include; it is recommended, not run, because the amendment
   fixed the leg design before the data.
3. **The decision rule as written fires `REFUTED`.** The lead decides
   whether the ruling stands or the control leg is repeated with a
   same-session reference.

**Ruling (project lead).** §18 stands as scored, `REFUTED` by its own rule;
it is not relabelled. The CONTROL-MOVED check used a cross-session
reference, a weakness of the design (the lead's), and §19 tests it. No ship.

*Also in this session.* `q2_one_task_per_loop_no_multiplexer` (source-pin
string updated for `task_obs::timed`, commit `fa0f915`) passes on the
fresh build (`q2test.log`, rc 0).
