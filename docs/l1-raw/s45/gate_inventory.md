# Gate inventory at main 4926822: class table, scoreboard, Copa cwnd readers (status §45 addendum)

Read-only inventory; docs pinned to diag36 4241d64. GG = `ac1aed1:raptorpath/docs/goal-gate.md` (the old ledger). P§10:n = a paper row at 4241d64. Scripts in this directory: `tests_per_gate.py`, `sloc.py`, `secmap.py`.

## Findings that change the brief
- No arm on main was refuted by a battery on the current substrate (wire v9 = 7825196, 2026-09-30). Every §10 "off" verdict is dated 2026-09-09 or earlier.
- The precedent for removing an arm refuted on an older substrate is GG ~955–1030 (the deprecation register, ADR-0066):
  - `RWM_SRC_BP` was removed (8902d24) as superseded, with its re-test clause moved into the row text.
  - `RWM_RATE_WIRE` was removed (f1f32c5) with re-test "NO" on structural grounds.
  - The SRC_BP clause has since been lost: P§10 now says only "removed".
- ADR-0052's warn-on-activation stage no longer exists. `config::deprecated_env_flag` is gone and ADR-0066 was deleted in 94bf58d.
- Deleting a gate couples to four places:
  - `gates.rs::forwarding_audit`, which needs `lib.sh` `RWM_FORWARD` to equal the engine set (95 = 95);
  - the byte-pinned `[GATES]` echo test;
  - the battery harnesses, which `env -u` or pin `RWM_POOL_ANCHOR`, `RWM_EST_CADENCE`, `RWM_STORE`, `RWM_STORE_PATHS`, `RWM_PLAIN_RS` and `RWM_HONEST_CAP`;
  - the `place38_parse.py` witnesses.

## Class table (deletion candidates; ⚑ = classification uncertain)

Tests are given as "literal / literal plus field hooks", over 1 064 `#[test]` functions at main. "Law" counts tests that call the arm's law function directly.

| gate | class | verdict | date / substrate | tests |
|---|---|---|---|---|
| `RWM_PLAIN_RS` | REFUTED-CURRENT (diagnostic, n=3, not scored) ⚑ | S §23 (PRS arm collapses the law to its floor: c1s −72 %, c2 −92 %) | 2026-10-26 | 4/7 |
| `RWM_HONEST_CAP` | REFUTED-OLD ⚑; resolves on but inert without PLAIN_RS | the honest-inputs family, GG ~23640 | 2026-08-11 | 0/2; 12 law |
| `RWM_HONEST_K` | REFUTED-OLD | GG:23650 | 2026-08-11 | 1/3 |
| `RWM_POOL_ANCHOR` | REFUTED-OLD | P§10:3828 | 2026-08-07 | 1/4 |
| `RWM_RECV_REQUEST_LAW`, `RWM_RANK_FEEDBACK` | REFUTED-OLD, re-test clause applies (the π̂₀ fix was not re-run) | P§10:3851; S §2:111 | 2026-09-08 | 3/4, 2/3 |
| `RWM_HOLDDOWN_Q`, `RWM_REFRESH_FLOOR_US`, `RWM_DERIVED_SWEEP` | REFUTED-OLD; recovery arms measured at today's dwell-inflated H | P§10:3849, 3850, 3844 | 2026-08-12 to 2026-08-21 | 3/3, 0/1, 0/5 |
| `RWM_THREE_TERM`, `RWM_COMPOSED_CAP` | REFUTED-OLD; THREE_TERM is the only reader of the contract stall | P§10:3821, 3822 | 2026-08-10, 2026-08-13 | 1/4, 1/7; 12 law (shared) |
| `RWM_RECOV_SP` | REFUTED-OLD ⚑ (kept, open) | P§10:3842 | 2026-07-27 | 0/2 |
| `RWM_LOSS_SENT_TRUTH` | REFUTED-OLD with a current audit ⚑ | P§10:3827; S §3.8 | 2026-08-19 | 0/3 |
| `RWM_RELEASE_1TO1`, `RWM_CHARGE_RECOVERY`, `RWM_LATE_BRAKE` | NEEDS-MORE ⚑ | GG:31933, 31932, 31936 | 2026-08-18 | 0/3, 0/3, 1/4 |
| `RWM_PLACE_T_DERIVED`, `RWM_PLACE_HOL`, `RWM_PLACE_WDIV_DERIVED` | REFUTED-OLD ⚑; refuted while the lock-out was live | P§10:3808 | 2026-09-09 | 2/5, 2/5, 1/4 |
| `RWM_COLD_PLACE` | NEVER-TESTED on the wire ⚑ | GG:30455 | 2026-08-18 | 0/3 |
| `RWM_COMPLETION_EXPOSURE` | REFUTED-OLD ⚑ (direction only) | P§10:3803; S §39 | 2026-09-08 | 1/2 |
| `RWM_REPAIR_WAIT`, `RWM_PROACTIVE_PACER`, `RWM_XPATH_REPAIR`, `RWM_REASM_BDP` | REFUTED-OLD, **pre-discipline** (before 2026-07-13) | P§10:3800, 3802; GG:7985, 8091 | 2026-07-08 | 0/0, 0/1, 0/1, 0/0 |
| `RWM_INFL_CAP` | no verdict found ⚑ | GG:29508 | — | 0/2 |
| `RWM_COPA_COMPETE` | REFUTED-OLD (inert), NO-MODE-SWITCH debt | GG:11831 | 2026-07-19 | 0/1 |
| `RWM_COPA_WIRE`, `_FEED`, `RWM_CC_PACE`, `_HR`, `RWM_COPA_DELTA` | refuted as the default ⚑; kept as passthrough policy (user's call) | P§10:3809 | 2026-07-22/27 | 0–1 each |
| Generation seat: `RWM_GEN`, `RWM_PIPELINE`, `RWM_GEN_PIPE`, `RWM_GEN_R`, `RWM_GEN_RATE`, `RWM_GEN_RATE_FLOOR`, `RWM_GEN_INFLIGHT`, `RWM_OOO_RETAIN`, `RWM_WINDOW`, `RWM_REPORT_GENS`, `RWM_CODED_SRC`, `RWM_REACT_CAP`, `RWM_INFL_BDP` | OPEN, disposition pending; delete as one unit | S §3.2 | — | 0–2 each |

**Other classes**
- **CONTROL:** `RWM_STORE`, `RWM_STORE_GAIN`, `RWM_DELTA`, `RWM_PLACE_T`, `RWM_ANCHOR_HYGIENE`.
- **TEST-ONLY:** `RWM_MIN_R`, `RWM_L0_NETEM`, `RWM_L0_SEED`, `RWM_PERF_TIMEOUT_S`.
- **INSTRUMENT:** `RWM_DIAG`, `RWM_ACKDIAG`, `RWM_ACKDIAG_WINDOW_US`, `RWM_RTT_DUMP`, `RWM_RTT_DUMP_MAX`, `RWM_SUCC_DUMP`, `RWM_SUCC_DUMP_MAX`, `RWM_WALLDIAG`, `RWM_CPUPROF`, `RWM_RDIAG`, `RWM_RTOBS`, `RWM_FDIAG`, `RWM_TRACE`, `RWM_PFRAC`, `RWM_RS_TRACE`, `RWM_SIDLE_DERIVED`, `RWM_NO_REACTIVE`.
- **SHIPPED:** `RWM_UNIFIED`, `RWM_UNIFIED_SHED`, `RWM_TAPER_R`, `RWM_ASTAR_ANCHOR`, `RWM_MSTAR_ANCHOR`, `RWM_HONEST_ANCHOR`, `RWM_CLOCK_GAP`, `RWM_STORE_SACK_RELEASE`, `RWM_STORE_PATHS`, `RWM_STORE_PATH_POOL`, `RWM_STORE_BOOT`, `RWM_SUM_CAP`, `RWM_DELTA_CAP`, `RWM_ACK_MERGE`, `RWM_WIRE_COMPACT`, `RWM_EST_CADENCE`, `RWM_EMIT_BATCH`, `RWM_EMIT_BURST`, `RWM_RECOV_MP`, `RWM_RECOV_MP_LAW`, `RWM_RS_ATTR`, `RWM_MTU_FLOOR`, `RWM_RSTAR_TAIL` (on in code, missing from status §1's on-list).
- **`RWM_QUIC_CC`:**
  - `bbr` is SHIPPED;
  - `passthrough` is OPEN, a policy point;
  - `bbr_rs` is an INSTRUMENT;
  - `cubic` and `newreno` are legacy.

## Scoreboard at main 4926822
| item | value |
|---|---|
| distinct `RWM_` gates | 95 |
| §11.2 rows | 30 |
| hint-keyed decision sites | 6 engine + 1 harness (18 non-test grep lines) |
| Rust SLOC, non-blank, test files excluded | 46 124 (40 576 excluding inline `#[cfg(test)]` too) |
| smoothed RTT / RTprop / delivery rate / loss estimators | 4 / 3 + P10 / 6 / 3 |

Commands:
- `git grep -hoE '"RWM_[A-Z0-9_]+"' 4926822 -- raptorpath/src | tr -d '"' | grep -vE '^RWM_(TEST_|FORWARD$)' | sort -u | wc -l`
- `git show 4926822:raptorpath/docs/fec-arq-model.md | awk '/^### 11.2/{f=1;next} /^### 11.3/{f=0} f && /^\| /' | grep -vcE '^\| constant \|^\|---'`
- SLOC: `sloc.py`.

## Copa cwnd readers (PathState::cwnd)
"Gates neither intake nor the wire" is CONFIRMED at default. "Only feeds placement's queue term, spare_capacity and the active-path filter" is REFUTED as worded.

**Default readers**
- the placement queue term (`path.rs:467`, read at `place.rs:475`) and the ETA wire stamp (`emit_source.rs:351`);
- `spare_capacity` (`path.rs:496`), which **clamps repair emission** (`emit_source.rs:540`);
- `available()` / `active_paths`: gauges, plus **path selection at ρ < 1 and the Realtime duplicate** (`scheduler/mod.rs:600/621/636/662`, `emit_source.rs:313/478/837`, `sender_phases.rs:1164/1322`);
- **the in-flight leak backstop** (`net/tasks/report.rs:183`).

**Arm-only readers:** intake via COMPOSED_CAP and LATE_BRAKE (`net/mod.rs:2458`); `cc_pace` (`net/mod.rs:2800`); passthrough on the wire (`control_msg.rs:381`); PLACE_HOL r_ref (`place.rs:456`).

**Dead code**
- the per-path token bucket (`path.rs:1136–1165`): no reader, but `consume_pace_tokens` is still called at 3 sites;
- `copa_target_cwnd`;
- `schedule()`.

**Must stay**
- `min_rtt`/RTprop (many readers);
- the BtlBw anchor (`copa_bdp_anchor`, `btlbw_sym_per_s`), which feeds the **shipped store cap**;
- `srtt`;
- jitter (the sweep clock and the receiver).
