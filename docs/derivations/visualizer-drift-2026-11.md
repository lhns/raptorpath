# Visualizer drift audit (2026-11)

Question: is the L0 model (`raptorpath-wasm/` + `raptorpath-visualizer/`)
current against the engine and the measured findings, and what needs updating?
This is an analysis only. No model code was changed and no cargo was run. The
probes ran under node, read-only, on the committed `interactive-visualizer.html`.

- **Base.** `github/main` at `248bdbac`.
- **Model unchanged.** `git diff 2d3b2069 github/main` over
  `raptorpath-wasm`, `raptorpath-visualizer` and `raptorpath-math` is empty, and
  the html was last built at `8f3fd7c5`. So every model line number in
  `github/wasm-modeswitch-audit` (`f2e7b2f6`, "the audit") still holds, and so
  do the law-ledger rows M01–M04, C13 and R03 that cite it.
- **Inputs.** The reconciliation record `w3-paper-reconciliation.md` is **not
  on main**. It exists only on `github/paper-recon` (`4e3b3a96`), at the old
  path `raptorpath/docs/derivations/`.

## Gates (run here)

| gate | result |
|---|---|
| `node raptorpath-visualizer/test_visualizer.mjs` (main) | exit 0, all checks pass |
| `node raptorpath-visualizer/test_visualizer_ui.mjs` (main) | exit 0, all checks pass |
| the audit branch's `test_visualizer.mjs` (tightened §7d3 tolerance) on main's html | exit 0 |
| native `raptorpath-wasm` tests (golden, pool, continuum) | **not run** (no cargo) |

## (a) Items

Status key:
- **current:** true today;
- **stale:** text that is wrong today;
- **defect:** breaks the invariant or parity;
- **missing-test:** a documented divergence with no bounding test.

### A. The audit's sites against main

| id | where (main) | status | evidence |
|---|---|---|---|
| W11 β = 0 fast path | `raptorpath-wasm/src/lib.rs:883` `if self.bulkness > 0.0` | **defect** (still present) | line verbatim. Value step bounded by §7d3 Auto ±2 % (`f6535e90`). Structurally it is the §4.5-banned path. Ledger M03 |
| W14 T_cut at ρ < 1 | `lib.rs:1401` `if self.rho < 1.0 { find_t_cut … max(·, 2·RTT) }` | **defect** (still present) | line verbatim. Pinned by its floor or cap at every measured ρ < 1. Steps at ρ* 0.983332 / 0.988175. Ledger C13 |
| V5/V6 cartoon shed budget | `interactive-visualizer.html:2643–2644`, read at `:2691` | **defect** (still present) | lines verbatim. Budget 0 → ε̂ at δ* (0.0050002 at ε 10 %, i.e. at Bulk). Ledger M04 |
| W22 `p_fec_normal` cut | `raptorpath-math/src/lib.rs:886–888` `if numerator <= 0.0 { return 0.0 }` | **defect + missing-test** (still present) | step 0 → ½ at r = ε/(1−ε). The paper's form is continuous (`fec-arq-model.md:699`). There is still no test that bounds the divergence. Ledger R03 |
| dead classic machine W2/W4/W5/W6/W9/W17, V1 | `lib.rs:1022–1044`, `:666`, `:706`, `:784`, `:1689`; html `:1705` | **defect (dead)**, still present | lines verbatim. Ledger M03 |
| audit's engine refs | `net/emit_source.rs` shed residual call | stale line refs only | `:588` → `:593`; the `if reliable` sites `:308/:815` → `:313/:820`; `sender_phases.rs:980/:1122` → `:973/:985/:1118`. `fec_rate.rs:346/:515` and `fec_rate/tests.rs:262–282` unchanged |
| audit §7d3 tolerance | branch only | current, unmerged | passes on main's html (above) |

### B. Engine changes since the last model commit (`78a898a5`)

| change | modeled? | status | evidence |
|---|---|---|---|
| snapshot-order law, `87c2135d` (paper §7.1, A.6). Ungated, so on by default | no | **current**: no parity claim, and the law is vacuous in the model | the sim has per-symbol acks with instantaneous forward flight (`lib.rs:335–339`). It has no gap reports, so no report can arrive out of receiver order. The page names no snapshot or gap channel |
| removed default-off arms `RWM_PLACE_HOL`, `HOLDDOWN_Q`, `REFRESH_FLOOR_US`, `DERIVED_SWEEP`, `PLACE_T_DERIVED` | no | **current** | no match in the wasm or the html. The model's placement is a declared simplification ("highest-goodput path with a spare slot", `lib.rs:308`, `:604–607`, `:1198`) with no claim to match any placement arm |
| `00a42e38`, `f5fdbf64`, `07afc9b7`, `d12fcd41`, `1b5b0b94` (`[SNAPV]` diag), `d5bd282a` (merge) | — | n/a | an extraction, tests and path moves; no law the model reads |
| side effect: paper §11.3 "store headroom" row still cites "the default-off placement frontier arm (`scheduler/place.rs:409`)" | — | **stale (paper)** | the arm was removed in `57955ea4`. Paper-layer, not model |

### C. Store and pool

| item | where | status | evidence |
|---|---|---|---|
| what the model's pool is | `lib.rs:352` `STORE_PER_PATH = 512`; stall at `:1303` | current as code; **underived** (ledger M01) | a constant 512·N slot count with no BDP or δ input. "Scaled to the 2000-symbol transfer" (html `:422`) has no derivation |
| its bounding test | `lib.rs:2780` `wasm_pool_is_constant_per_path_whatever_the_dial_and_bdp` | **holds** (the model is unchanged since; the native run was not executed here) and **misdescribes the engine** | it pins the model side only; it measures no gap to the engine. Its comment says the engine cap is `ceil((1+q(δ))·bdp)`, "moves with the BDP". The shipped cap is `clamp(·, floor, max(n_live·2048, floor))` (`net/store_cap.rs:90`). The law's value sits at that clamp for 0.74–0.98 of each c8 run and ≈ 0.7 at c1s (`[SUMCAP] pin=`, status §53 Result, §55 F01 PINNED), and on every c8 row at every W (§54 launch 2). Measured, the engine at c8 also runs a per-path constant, but at 2048, not 512 |
| model regime against the engine's at c8 | probe on the committed html (script below) | **divergent in regime, undocumented** | two different gauges. The model's pin fraction is 1 by construction (a constant). Its **stall fraction** (ticks with occupancy ≥ pool) is 0.000 on its own c8 preset (peak occupancy 0.08–0.09 of 1024), and also on c7, the default cell and RTT 100. It is non-zero only at 1 path with RTT ≥ 200 and cap 8 (0.57–0.86). The engine's c8 sender is store-gated, `paused` ≈ 80 % (status §53 Result), which is the comparable quantity. *Inference, not tested:* the model has fixed capacity and no queue, so it cannot reproduce the queue-inflated inputs (E03 `min_rtt` 7–33×, E04 `max_bw` 11–24×) that push the engine's law to its clamp |
| page: "knee ≈ 2048 symbols/path" | html `:421` | **stale (now known false)** | §56, where W is the total store (N·pool): c2 (1 path) is flat down to W ≈ 192–256, c8 (2 paths) flat to W ≈ 512, i.e. 256 per path (knee 0.95 of control at W ≈ 340), and c1s gains goodput all the way to the 2048 control. 2048 per path is a pinned clamp constant, not a measured knee |
| page: the pool as a "current law" | html `:166–167` ("carries the current laws … the path-scaled pool"), `:270` ("the path-scaled law"), `:269`, `:2469–2470` ("path-scaled, §6.1") | **stale (now known false)** | the model's pool is a constant (M01). The engine's §6.1 law is measured pinned at its clamp (F01). The store law that would replace it (`store-law.md` rev 6) is derived, not shipped |
| page: the binding mechanism "crank RTT … until cap·RTT outgrows the pool" | html `:270` | current for the model, **misleading for the engine** | the engine binds at c8 with a BDP of ≈ 190 symbols against a 4096 ceiling (§54 launch 2 "Scale"). Its binder is the inputs, not the BDP |
| paper §11.3 store row, wasm clause: "scales its pool per path with no guard" | `fec-arq-model.md` §11.3 | **stale (paper)**, understated | paper-recon 4b: the pool is a constant per path and does not implement §6.1 |

### D. Model-vs-engine divergences and their bounding tests

The sources are paper §11.3 (visualizer rows), paper-recon (not on main) and
the page's own "model vs engine" table (html `:389–426`).

| divergence | bounding test | status |
|---|---|---|
| Bulk tail anchor 0.05 vs engine 1e-3 (M02) | `test_sim_engine_tail_anchor_divergence_is_bounded` (`lib.rs:2229`) | current |
| `bulkness > 0` guard at Auto (recon 16b) | `test_visualizer.mjs` §7d3, Auto ±2 % (`f6535e90`) | value bounded; the structural part is W11 above |
| pool constant 512·N (recon 4b, M01) | `lib.rs:2780` | bounded as a model pin; its comment needs correcting (C above) |
| W22 `p_fec_normal` vs the paper's Φ (R03) | none | **missing-test** (CLAUDE.md) |
| W* latency budget 0 = 1 SRTT at every δ, in both the engine and the model (recon N3) | the math pins the function (`raptorpath-math` `derive_window_zero_budget_is_one_rtt…`) | model = engine; **missing-test** at the model's call sites (`lib.rs:1709–1716`, html `:2021`) |
| page "loss detection" row: engine = "RFC 9002 per path, 9/8 + kPacketThreshold 3" vs model = 1 own-RTT clock | `test_per_path_clock_phantom_counter` and §7f (ordinal and phantom counts) | **stale + missing-test**. At N = 1 the engine runs the pre-RFC `srtt/2` age gate (`legacy_age_ripe`, `net/recovery_laws.rs:266`, called at `sender_phases.rs:957`; `RWM_RECOV_SP` is default off, `gates.rs:371`; ledger C04). At c8 its holes were seen only at ≈ 3.5 s age (C02, measured before the snapshot-order law shipped in `87c2135d`). No test asserts that the model's clock is exactly 1·RTT_i from send |
| page "shed law" row: engine receiver arm has a "δ-derived hold" | — | **imprecise**. The engine's hold is two laws behind one signature (`net/shed.rs:51–70`): SRTT/2 when the shed law is armed and the budget is open, otherwise `(4·SRTT).clamp(60, 300 ms)`. SRTT/2 equals b(δ)·SRTT only at Realtime, the one seat that reaches it. It is hard-coded, not read from the dial (paper §11.3 row 2, ledger S05) |
| page row: decoder (no per-span A* emission in the sim) | — | declared structural simplification; no number to bound |
| page row: A*_q rounded, no generation grid | — | declared; the bound would be ≤ ½ symbol by construction. Untested, low value |
| page row: χ horizon on a 2000-symbol transfer vs the engine's MB-scale objects | — | declared and current. The engine tunnel's χ ≡ 0 (recon N1) makes the page's engine cell ("a negligible tail") an understatement: on the tunnel χ is never evaluated |
| page rows: anchors (J = RTT/8, configured RTT), no CC, instantaneous flight, 8-B symbols | — | structural simplifications, declared. No number to bound; acceptable as is |

### E. Stale page text (no behaviour)

| text | where | status |
|---|---|---|
| era stamp "era 2026-07-28 · main 7a3aff6" | html `:382` | stale (the model was built at `8f3fd7c5`, and main is `248bdbac`) |
| `§16.19/.20/.21/.24/.25/.26`, `§14.26/.29`, `§12.4`, `§17.2`, `§16.6` | html `:95`, `:165–167`, `:383`, `:394–424`, `:2507` | stale: the paper has 11 sections plus Appendix A. `8f3fd7c5` re-pointed the comments but not these strings. CLAUDE.md's own "§16.20/§16.26" is the same drift |
| "ρ < 1 = §6.1 T_cut give-up" | html `:401`, `:404` | stale: T_cut is §3.5 / §4.10; §6.1 is the pooled cap |
| "the sim … via the "custom" hint" | `lib.rs:329–331` | stale: the UI passes `"continuum"` (html `:1987`) |

## (b) Recommended updates, in priority order

The order is by what can ship now. Items 5 and 6 are the live behaviour
defects (V5/V6, W14, W22), and they are blocked on the owner's calls.

Page text lives directly in `interactive-visualizer.html`. `build_visualizer.sh`
only replaces the embedded wasm block. So text-only fixes need no wasm rebuild,
and the gates run on the edited file as is.

Must fix (invariant or parity defects):

| # | change | layer | GOLDEN_CAPTURE=1? | unshipped law? |
|---|---|---|---|---|
| 1 | Delete the W11 guard and the dead classic machine (audit F1 alias table + F2; W17 → χ_raw; V1). The fix is removal, not a new formula | wasm model (+ html V1) | no expected. Run the golden test **without** capture first; the audit argues bit-identity and measured it on wasm32. Native is unverified | no |
| 2 | Add the W22 bounding test: \|P_code − P_paper\| = P_paper below ε/(1−ε), 0 above it, ≤ ½ at the floor (audit Coverage row) | tests only. Put it in `raptorpath-wasm` (calling `raptorpath_math::p_fec_normal` against the paper's Φ form) to stay in visualizer scope; a test inside `raptorpath-math` touches an engine crate and needs the owner's word | no | no |
| 3 | Correct the store text: 2048 is the engine's **clamp**, measured pinned (F01), not a knee. The model's 512·N is an underived constant, not a law. Fix the `wasm_pool_…` comment to state the clamp and the F01 bind fractions. Add a page/§11.3 line saying the model's pool does not bind at its c8 preset while the engine's does | page + test comment (+ paper §11.3 wasm clause) | no | the store law (`store-law.md` rev 6) is **unshipped: do not model it**. M01 waits on F01 |
| 4 | Correct the "model vs engine" table. N = 1 loss detection is the `srtt/2` age gate, not RFC 9002 (C04). The receiver hold is a hard-coded SRTT/2 that equals b·SRTT only at Realtime, with a clamped 4·SRTT fallback (S05) | page | no | no |
| 5 | V5/V6 + W14 as one give-up law (audit F3+F4, option (i) or (ii)), with the `test_visualizer_ui.mjs:181` gate change | wasm model + page + gates | F3: no (JS only). F4: no on the golden rows (ρ = 1); `test_custom_rho_gives_up_late_losses` and `test_bulk_composes_with_rho` may move | **owner's call pending**. Option (ii) depends on the W22/R03 fix, which is the owner's call (paper form vs the engine pin `fec_rate/tests.rs:262–282`). Do not code it before the §5.6 line exists |
| 6 | W22 law fix itself | `raptorpath-math` (an engine crate, shared with the engine shed budget and soft cap) | **assume yes**: `r_saturation` feeds the sim's soft cap, and the engine bit-pin tables may move | no new law, but the owner's call and a §4.2 line first. It is not a visualizer-scoped change |

Nice to have:

| # | change | layer | GOLDEN? | unshipped law? |
|---|---|---|---|---|
| 7 | Merge the audit's tightened §7d3 tolerance (passes on main's html) | tests only | no | no |
| 8 | Fix the stale text in E: the era stamp, §16/§14/§12/§17 → the current numbering, the T_cut section, the "custom" comment | page + wasm comment | no (but a rebuild refreshes the embedded html) | no |
| 9 | Bound the per-path clock: assert the model fires a retransmit exactly at 1·RTT_i from send | tests only | no | no |
| 10 | Bound the W* call sites: the model passes budget 0 (1 SRTT) at every δ, matching the engine | tests only | no | no; recon question 1 (b(δ)·SRTT) would be a deliberate change with a capture |
| 11 | A page-table row "snapshot order: engine judges only the newest receiver snapshot; model has no gap reports (vacuous)" | page | no | no |
| 12 | A stall-fraction readout for the pool (ticks with occupancy ≥ pool / ticks), beside the stall count. The pin fraction is trivially 1 for a constant, so it is not worth showing | page | no | no |

## Probe

`probe_pool.mjs` (scratch, not committed) loads the committed html's engine
block the way `test_visualizer.mjs` does. It runs
`Simulation.multipath(…, "continuum", δ)` at δ ∈ {0.005, 0.5, 50} on these
cells: the default cell, c7, c8, and 1 path at RTT 100 / cap 4, RTT 200 /
cap 8 and RTT 500 / cap 8. It counts the ticks with `get_store_occupancy() ≥
get_pool_cap()` (the stall fraction).

| cell | stall fraction | peak occupancy / cap |
|---|---|---|
| c8 | 0.000 at every δ | 0.08–0.09 |
| c7 | 0.000 | 0.09–0.11 |
| default | 0.000 | 0.31–0.41 |
| RTT 100 / cap 4 | 0.000 | 0.59–0.80 |
| RTT 200 / cap 8 | 0.61–0.63 | 1.00 |
| RTT 500 / cap 8 | 0.57–0.86 | 1.00 |

## Not verified here

- Native wasm tests: the golden test, `wasm_pool_…` and the continuum test.
- Whether the engine's measured rate ever crosses ε/(1−ε) (audit W22 "conflicting sources").
- The engine-side line numbers beyond those listed above.
- Engine drift before `78a898a5`: the html stamps era `7a3aff6` and was built
  at `8f3fd7c5`. The window before `78a898a5` is covered only through paper
  §11.3, paper-recon and the law ledger, not by a commit-by-commit read.
- The inference that the model cannot reproduce the engine's pinned store
  because it has no queue (C, "model regime").
