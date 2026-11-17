# F2's merge guard: guard v2 on the Bulk cells, plus a Realtime stage, pre-registration draft

Draft for the coordinator to register as its own status section (called §64 here; rename if the number differs). Nothing here has run. Written on github main `e495d186` (status §63 Result, owner decisions 2026-11-16 item 5 and 2026-11-17 items 11–13). The fix is `f2-impl` `626fdf9a`; its design is `docs/derivations/f2-framing-design.md` and ADR-0078 (Proposed), both on `f2-impl`.

**For the owner, in plain words.**
- F2's fix changes the release binary, so it needs a §41c guard against main before it merges (owner decision 5 and CLAUDE.md). The identical-sha waiver does not apply.
- **F2 goes alone, not at the bottom of a stack.** No other zero-behaviour step is ready to stack on it today (§1.2).
- **The Bulk cells are guard v2 unchanged** (§41d, as §54 ran it): 12 + 12 rows at c1s, c1d, c2 and c8, rank-minp, PASS iff p > 0.05. Bulk's data bytes are unchanged by construction, so the prediction is PASS.
- **One Realtime cell runs in a separate stage.** Behaviour there changes on purpose: main dies in the warm-up some of the time. So the stage does not compare CAND with BASE for the verdict. It scores an absolute reading on every CAND row (the §63 F2 reading, plus completion) and reports everything else.
- **Before launch the owner gives two words:** ADR-0078 to Accepted, and the go to merge wire v10, which refuses every v9 peer.
- **Cost:** about 3.5 h of VM time typical and about 6 h worst, with host cargo paused. Order: s62, then this guard, then E-pre.

## 1. BASE and CAND

### 1.1 The seats

| seat | tree | content |
|---|---|---|
| **BASE64** | github `main` at launch, pinned by amendment | `e495d186` or later, with the harness of §5 merged (tools only, as `harness54` was for §54). Engine identity: `git diff d2cae698 BASE64 -- raptorpath raptorpath-math raptorpath-core raptorpath-wasm gf256 vendor Cargo.toml Cargo.lock` is empty |
| **CAND64** | branch `f2-cand64` = BASE64 + `git merge --no-ff github/f2-impl` (`626fdf9a`) | all 16 commits of `main..f2-impl`: `f2-prep`'s three (repro, counter, audit), the six design commits, and the seven `f2-impl` commits |

- **Commit 1 (`7ce6523b`) never goes alone.** It is labelled v9 but writes LE-packed Realtime symbols that a real v9 peer misreads (design §9, "Commit rule"). The merge takes the branch whole, with `--no-ff`, and nothing is squashed.
- **Identity witnesses, checked when the amendment pins the seats:**
  - `git diff 626fdf9a CAND64 -- . ':!docs' ':!tools'` is empty, so the Linux suites at `626fdf9a` were run on CAND's engine tree. At the time of writing the trial merge of `e495d186` with `f2-impl` is clean (tree `e0a351c6`), and its diff to `626fdf9a` outside `docs/` is empty.
  - The engine patch-id of CAND64 over BASE64 equals §63 Amendment 1's `490a0d1f…` (the patch-id of `f2-impl` over main).
  - `git diff BASE64 CAND64 -- tools` is the two-line `suites_run.sh` seams entry only. Both seats run one harness.
- The rehearsal records both release shas (`LD_BIN_SHA_CAND`, `LD_BIN_SHA_BASE`), and an amendment pins them. Every launch must match.
  - Reported, not checked: §58 Amendment 3's known answer for main's engine (`0ec95258…`) holds only for `binid_run.sh`'s build path. The harness builds at its own path, and the binary embeds location data.

### 1.2 Alone, or the base of a stack: alone

Owner decision 5 says to stack zero-behaviour steps into one guard against main and to bisect only on failure. The F2 design puts F2 at the bottom of such a stack (design §7, §9 "VM order" item 3). **The recommendation is F2 alone now, and the next stacked guard takes post-F2 main as its BASE.** Why:

- **Nothing else is ready to stack:**
  - **Perleg Phase A** (`perleg-phase-a` `e5246d7f`) sits on `2aaac8e8`. It lacks `14954df7`, so `gates::forwarding_audit` is red on that branch as it stands. It has no Linux suite record. Its A1 step is bounded (≤ 16 ns), not bit-identical, so its trace pin changes by design and needs its own Linux run first. It merges cleanly with main and with `f2-impl` (trial `merge-tree`, no conflicts), so nothing in F2 blocks it later.
  - **Guard α's shared-core moves** (`shared-core-design.md`, steps 1.2–2.5) have no branches yet.
  - **The slaw gauge** (`slaw-gauge`) waits on the owner (status, "Still held").
- **F2 is not a zero-behaviour step.** It changes Realtime on purpose, and it breaks the wire (v10). Owner decision 5 covers zero-behaviour steps. A stack that had F2 at its base would mix the one candidate that changes behaviour with the ones that may not.
- **Attribution.** Alone, a FAIL points at F2 without a bisection run. In a stack, a FAIL costs one more guard of the same size (about 2.5 h).
- **The critical path.** Block 51 needs the merged binary (§63 Result, "What follows"). Waiting for Phase A's pre-flight would hold F2 for a step that is not on that path.
- **What this costs:** one more guard run in total, because Phase A later gets its own guard instead of sharing this one. That is about 2.5 h of VM time.

## 2. Cells, n and clauses

### 2.1 Stage G, the guard (scored): plan 41d unchanged

- **Rows:** plan 41d's 96 rows in plan 41c's order, Bulk hint, ρ = 1:
  - c1s-400 and c1d-400 at the guard size, timeout 150 s;
  - c2-100 and c8-100 at 600 MB, `RWM_PERF_TIMEOUT_S=200`;
  - 12 BASE + 12 CAND per cell, seeds 42 and 7, `RWM_DIAG=1`.
- **Clauses (v2, §41d):** goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli` and `lag_E_srv` at every cell; the in-order delay mean at c2 and c8. `rtp_floor_p0`/`p1` are reported only (§41d owner call 1).
- **Statistic:** rank-minp, one-sided, 10 000 joint relabellings (seed 4141), the max over every clause of the four cells. **GATE-41d PASS iff p > 0.05.**
- **Voids and replacement:** §41d's rules unchanged. A c2 or c8 row with under 30 s of `[DIAG]` ticks is VOID-SHORT. A DNF keeps its worst rank. At most 4 witness-voided slots are re-run once (`place38_parse.MAX_REPLACEMENTS`), and more is NO VERDICT.
- **Expected resolution** (§54 launch 1, the latest v2 ledger; d80): goodput c1s 5 %, c1d 8 %, c2 1 %, c8 2 %; `cpu_cli/GB` c1s 2.4 %, c1d 8 %, c2 5 %, c8 5 %; in-order delay c2 8 %, c8 20 %; lag 20–100 %.
- **Not plan 54:** its binary pre-check needs `[SNAPV]` absent in BASE, and main now ships it. Plan 64 maps to 41d the same way 54 did (§5).

**F2 row witnesses on stage G (scored, outside the min-p family):**
- **CAND rows, every cell:**
  - `[SYMSZ] local=1200 decoder=1200` on both ends (the executes witness: the v10 handshake ran and the receiver adopted the peer's size);
  - a `[CTLD]` line carrying `undeliv=` on the receiving end (the perf server), with `undeliv=0`, and 0 on the client too;
  - no `[UNFRAMABLE]` line on either end.
- **A CAND row that fails any of these is `F2-FAIL`.** It is never VOID, never replaced and never NO VERDICT. After the fix an unframable symbol can come only from an engine bug (owner decision 12), and at Bulk it is a fail-closed false positive. One such row fails the guard whatever p reads.
- **Seat witnesses:**
  - before any row, the CAND binary contains the strings `[SYMSZ]` and `[UNFRAMABLE]` and the BASE binary contains neither. Otherwise the run aborts with ABORT-SEAT;
  - every row's `bin_sha` matches its arm's build.
- **Prediction: GATE-41d PASS.**
  - Bulk and Auto data bytes are unchanged. `hotpath_pin` passes unchanged at commit 1, and commit 2 re-captures it for the version field only (ADR-0078, Consequences).
  - The per-symbol CPU change is predicted at ±2 ns to frame and +1–3 ns to read (design §7). At c1s that is about 1/40 of the `cpu_cli/GB` q95 (§3.1).
  - The control path changes once per path: the handshake carries the real size (1200, as before at Bulk), and `WindowStart` loses two fields.
  - **The registered risk is the false-fail rate,** 5 % by construction. §54's no-regression candidate read its min-p at c1s `cpu_cli/GB`, p = 0.066.
- **Prediction for the witnesses:** 48 of 48 CAND rows pass, and the BASE binary prints no `[SYMSZ]`.

### 2.2 Stage R, Realtime (its own ledger, after stage G; plan 52's pattern)

**Why it is separate.** Four facts force it out of the min-p family:
- BASE fails the Realtime warm-up some of the time. That is the defect: 4 of 9 draws in §63's pooled CONTROL. Those slots would exhaust stage G's 4-slot replacement cap and read NO VERDICT.
- No A/A ledger exists for any Realtime cell, so no clause there has a known resolution.
- Main has no `undeliv` counter (it came with `f2-prep`), so BASE cannot carry the F2 reading.
- The behaviour changes on purpose, so a CAND-against-BASE comparison is not a regression test there.

**The cell.**
- **c2-100, Realtime hint, ρ = 1:** `perf_rwm_c.sh`'s default `--window-reliable`, so `RWM_C_RELIABLE` is unset and scrubbed. 600 MB, `RWM_PERF_TIMEOUT_S=200`, `RWM_DIAG=1`.
  - Arms **RB** (BASE's binary) and **RC** (CAND's binary), 12 + 12, seeds 42 and 7 × 6 each, interleaved as plan 41c.
  - The size is from `store-law-n1`: c2-100 Realtime at ρ = 1 ran 100 MB in 11.5–11.7 s (about 69 Mbit/s). So 600 MB is about 70 s, above §41d's 30 s tick floor and inside the timeout.
- **Why ρ = 1 and not the shipped Realtime default ρ < 1:**
  - At ρ = 1 the rows complete, so goodput, CPU/GB and §41d's in-order delay (defined for reliable delivery only) can all be read.
  - Evicting rows DNF by design (§63 Result, the correction to Amendment 2).
  - ρ is an independent dial. The F2 reading at ρ < 1 is §63's: 5 of 5 rows on the same engine patch-id, unpaced U row included. Block E and E-pre read it again on the E tree plus F2. Together with this stage, both ends of ρ at the Realtime point carry the reading.
- **Limit, stated:** perf chunks fill the symbol (510 B of a 512 B symbol), so no row packs two or more packets into one symbol. Only `f2_realtime_packed` (commit 6, a loopback test, in the seams suite) covers that. The stage cannot see it.

**Scored: the Realtime reading, every RC row (RT-PASS iff 12 of 12).** Each RC row must show all of:
1. the warm-up acked;
2. `undeliv=0` on a `[CTLD]` line that is present on the receiving end (the server). A missing line is a missing witness, not a pass. The client's value is summed and reported (§63 Amendment 1);
3. no `[UNFRAMABLE]` on either end;
4. `[SYMSZ] local=512 decoder=512` on both ends;
5. the transfer completes inside its timeout (not DNF). At ρ = 1 a 70 s transfer has a 200 s budget.

- A row that fails any item is `RT-FAIL`, with the item named. It is never replaced.
- Infrastructure voids (VOID-COTENANT, VOID-ONDISK, CONTAMINATED) are replaced under the existing rule, at most 2 slots in this stage.

**Reported only:**
- **BASE warm-up deaths, k of 12.**
  - An RB row whose client prints `warm-up object never acked` is `BASE-F2-DEAD`: a reading, never replaced, never VOID-RC.
  - There is no prediction. The committed base rates are 1 of 6 at ρ = 1 Realtime (`store-law-n1`) and 4 of 9 at ρ < 1 (§63 pooled).
  - k = 0 makes the RC reading weak (nothing reproduced). That is reported, not voided (§63's rule).
- **Medians and [min–max], RC against the live RB rows:** goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli`, `lag_E_srv`, the in-order delay mean (`threadp1_parse.inorder_delay_stats`), and `rtp_floor_p0`.
  - The one-sided rank p is printed per readout.
  - The live RB rows are selected by a draw (the warm-up survived), and that draw may correlate with early loss at seed 42. So the comparison is biased in an unknown direction, and it is never a verdict.
- **The `undeliv` denominator** that §63 left owed: the server's received source-symbol count per RC row, from an existing log field (§5 item 4). It is n/a if no such field exists; this needs no engine change.
- **Owner flag (non-gating):** a reported readout with CAND worse beyond c2's Bulk d80 for the same clause (goodput 1 %, CPU/GB 5 %, in-order delay 8 %) and rank p ≤ 0.05 goes to the owner with the merge question. Behaviour on rows whose warm-up survives is predicted unchanged, because the packed bytes keep their size (ADR-0078, Consequences).

### 2.3 Decision rule

- **MERGE** iff all of:
  - GATE-41d PASS;
  - 0 `F2-FAIL` rows in stage G;
  - RT-PASS 12 of 12 in stage R;
  - the owner items of §3.2 are given.

  Then `f2-impl` merges to main with `--no-ff`. As in §54, the merged tree must differ from the guarded CAND64 only in files main gained after BASE64 (docs, roadmap, ledgers), and that check is recorded.
- **GATE-41d FAIL:** no merge. The failing clause and the session's resolution table go to the owner. There is no bisection, because F2 is alone. The next step is a diagnosis of that clause.
- **Any `F2-FAIL` or `RT-FAIL`:** no merge. It is a finding against the fix (a fail-closed false positive, a size defect, or a Realtime failure) and is diagnosed before anything else. A fix makes a new CAND and a new guard.
- **NO VERDICT** ⇒ re-run. **ABORT-TESTS** ⇒ fixed on the branch and re-run on a new CAND sha.
- **The reported readouts decide nothing by themselves.** The owner flag travels with the merge question.

## 3. Pre-flight items F2 still owes

### 3.1 Coordinator items

1. **`framing_ns` on the VM.**
   - Command: `taskset -c 2 cargo test -p raptorpath --lib --release framing_ns -- --ignored --nocapture` on CAND64's tree, fresh target, both locks held, host cargo paused. It runs in the rehearsal session, before the pin.
   - No host table is recorded on `f2-impl` (design §9 step 8 asked for one, and no commit carries it). This is the bench's first reading on any machine.
   - **The bound (formula first).** The change in CPU per GB that framing can cause must be at most a tenth of the c1s CPU clause's BASE noise:

     ```
     Δt_point × N_GB  ≤  f × q95_side × C_side
     ```

     | symbol | value | provenance |
     |---|---|---|
     | Δt_point | median CAND − median BASE, ns/call, per bench point | measured (this bench) |
     | N_GB | 10⁹ / 1198 ≈ 8.35·10⁵ source symbols per GB | perf chunks fill S − 2 = 1198 B at Bulk (§63, "Before the merge"). Each source symbol is framed once at the sender and read once at the receiver; repair symbols and retransmits are not re-framed |
     | C_side | `cpu_cli/GB` 15.35 s (sender: frame), `cpu_srv/GB` 17.86 s (receiver: read) | measured: §54 launch 1, c1s BASE medians |
     | q95_side | 0.0123 (cli), 0.0173 (srv) | measured: §54 launch 1, c1s, q95 of \|x\| over 400 half-per-seed splits of BASE alone |
     | f | 0.1 | declared margin: framing must sit an order of magnitude below the noise the guard's c1s CPU clauses cannot see |

     - So **frame ≤ +22.6 ns/call** at S = 1200, lengths 64, 600 and 1196, and **read ≤ +37.0 ns/call** at the same three points.
     - **Prediction** (design §7): frame ±2 ns, read +1–3 ns.
     - The packed point (S = 512, 3 × 160) is reported only, because no Realtime CPU clause has an A/A. A value above the read bound goes to the owner with the merge question.
   - **Unreadable rule:** a point whose BASE min–max spread over its 5 runs exceeds that point's bound is UNREADABLE. It is re-run once. If it is still UNREADABLE, it goes to the owner before the launch.
   - **A point over its bound blocks the launch.** It is a design finding (design §7's cost argument fails) and goes to the owner.
2. **The harness of §5**, tools only, on main before BASE64 is pinned, with its stub runs green.
3. **An amendment pinning BASE64, CAND64 and both release shas** after the rehearsal. It also records the identity witnesses of §1.1.
4. **Fable's review of this registration.**
5. **Status pages at the merge:**
   - §1's `PROTOCOL_VERSION = 9` line;
   - the receiver-timestamp texts that still say v10: `docs/status/42-…md:21`, `:76` and `docs/status/43-…md:213` (design §3). That field becomes v11.
6. **§63's "Before the merge" list, checked against `f2-impl`:**
   - the early-`add_path` and no-initial-path size gaps: closed (commit 5, `f2_peer_symbol_size`);
   - the `test-seams` tests in `suites_run.sh`: done (commit 6). They are **not** in `loopdiag_run_all.sh`'s `run_suites`, which is what a guard runs, so §5 item 5 adds them;
   - ADR-0078's deviations, including whether a fast close drops earlier deliveries: recorded (commit 7, "Fatal close" and the exit flush);
   - an end-to-end Realtime test with two or more packets in one symbol: done (commit 6, `f2_realtime_packed`);
   - `framing_ns` on the VM: item 1 above.
7. **§63's open VOID-RC (rc 1) on plan 63's path** does not carry over. Stage R runs on main's battery path, not plan 63's. §5's stub paths must show rc 0 on a live RC row and the `BASE-F2-DEAD` label (not VOID-RC) on a dead RB row.

### 3.2 Owner items

1. **ADR-0078 to Accepted.** Its Status reads "Proposed (… points 4–5 await the owner)". Decisions 11 and 12 (2026-11-17) settled the format, fail-closed and the size by default. The ADR changes status only on the owner's word.
2. **The go to merge an incompatible wire version.**
   - v10 refuses every v9 peer at the handshake, so a connect fails before any data symbol (design §3).
   - No ref scanned has a version ≥ 10 (design §3), and there is no Android client in this repo (design §5.1).
   - Any v9 build in use elsewhere stops interoperating at this merge.
3. **Confirm F2 alone** (§1.2), against the design's "F2 at the bottom of the stack". This is the coordinator's default; the owner can overrule it.
4. **Confirm the order** (§4): guard before E-pre. This is the coordinator's default; the owner can overrule it.

## 4. Cost and order

| step | content | typical | basis |
|---|---|---|---|
| rehearsal + `framing_ns` | `LD_PLAN=64 LD_REHEARSE=1`: two release builds, the tools tests, plan 41d's 3 smoke rows plus RB s42 and RC s42; then the bench build and run | 0.75 h | §63's rehearsal (two builds, 20 min), plus about 15 min for the bench build and run |
| launch | two builds (≈ 15 min), the suites on CAND with core and seams (≈ 60 min), the tools tests, the smoke, stage G (96 rows), stage R (24 rows), up to 6 replacements | 2.75 h | §54 launch 1: 17:40–19:26Z = 1.77 h for builds, suites, smoke and 96 rows (48 min of rows). Plus stage R, 24 × (≈ 70 s + ≈ 15 s) ≈ 34 min, and about 10 min for the core and seams lines |
| **total** | | **≈ 3.5 h** | host cargo paused throughout |

- **Worst case, about 6 h:** a NO VERDICT re-launch (+2.75 h), or a rehearsal re-run (+0.75 h).
- **`LD_HARD`:** launch + 4.5 h; rehearsal + 1.5 h.
- **Suite reuse is not budgeted.** `suites_run.sh` writes `TESTS-OK` but no `SRC-MANIFEST.sha256`, so `LD_PRIOR_TESTS` cannot accept the `626fdf9a` run. The suites run in-session, as §54's did.

**Order:**
1. **s62 first.** Owner decision 13 holds literally (s62 after §63). s62 is registered (~1.4 h typical, 2.8 h worst), and this guard still waits on its harness and the owner items.
2. **This guard.**
3. **E-pre**, on the E tree plus F2 as guarded.
4. Block E, then block 51 (which needs the merged binary).

- **Why the guard comes before E-pre.** E-pre runs on "the final F2 tree" (§63 Result). A guard FAIL changes F2, and that would waste E-pre's suites and CAND pin. The design doc's VM order also puts the guard first ("After the guard, E-pre"). §63's registration lists E-pre without placing the guard.
- **When to swap them:** if the owner items are not in when s62 ends, E-pre may go first. The cost of swapping is a redo of E-pre if the guard then changes F2.
- **Perleg Phase A's guard** follows on post-F2 main as BASE, stacked with whatever else is ready then.

## 5. Harness change (Sonnet worker)

**Branch:** `harness64` off github main, in its own worktree, **tools only**. Never touch `raptorpath/` or any engine crate. Commit there; push only if told. It merges to main before BASE64 is pinned, like `harness54`.

**Decided (do not reopen):** everything in §1–§2. The plan token is `64`.

1. **Port F2's row primitives from `harness63`** (`github/harness63`, commit `e8b61a73` and its follow-ups), adapted to main's `tools/l1`, without plan 63 or plan 51e's parts:
   - `l1common.f2_counts`, `f2_void` and `symsz`, with their `test_l1common.py` cases;
   - `perf_rwm_c.sh`'s `--- F2 undeliv=… unframable=…` line.
   - **The presence check reads the logs' `[CTLD]` lines, never the `--- F2` line,** which prints `undeliv=0` when no counter exists (BASE).
2. **`LD_PLAN=64` = plan 41d's stage `guard/`, unchanged** (map 64 → 41d before any branch, as 54 is mapped), plus:
   - the F2 row witnesses of §2.1 on CAND rows, behind `LD_WITNESS64=1`. A failing CAND row gets status **`F2-FAIL`**: new, never replaced, never counted toward NO VERDICT;
   - `f2_void` on every BASE row (expected None);
   - the binary pre-check: `[SYMSZ]` and `[UNFRAMABLE]` present in CAND's binary and absent in BASE's, else `ABORT-SEAT`;
   - **`score64`:** it prints `score41d`'s output byte for byte (a stub test asserts this), then the F2 witness table, then `GATE-64 PASS` iff GATE-41d PASS and `F2-FAIL` = 0, else `GATE-64 FAIL (<reason>)`.
3. **Stage R in `$RUN/rt64/`**, after stage G, with its own ledger and score (plan 52's pattern; `DONE-ALL` needs both stages):
   - rows: arms `RB`/`RC` on the BASE/CAND binaries, cell c2-100, `HINT=realtime` per row (plan 60's per-row hint mechanism), ρ = 1 (`RWM_C_RELIABLE` scrubbed), 600 MB, `RWM_PERF_TIMEOUT_S=200`, `RWM_DIAG=1`, seeds 42/7 × 6 per arm, plan 41c's interleave;
   - the pipe-echo witness: `REALTIME_ECHO59` on both ends and `--window-reliable` on the perf line;
   - row labels:
     - an RB row with `warm-up object never acked` → `BASE-F2-DEAD`, decided before the rc test;
     - an RC row → `RT-OK` or `RT-FAIL <item>` per §2.2 items 1–5;
     - infrastructure voids are replaced, at most 2 slots;
   - `rt64_score.py` prints:
     - `RT-PASS n/12` or `RT-FAIL`, and `BASE-F2-DEAD k/12`;
     - per readout, RC and live-RB medians with [min–max], plus the one-sided rank p;
     - the owner-flag lines (§2.2);
     - the denominator: the server's received source symbols per RC row, from an existing log field. Name the field; if none exists, print `n/a (no field)` and say so. Do not change the engine;
   - stage R's rehearsal rows: RB s42 and RC s42.
4. **The F2 witnesses read the receiving end** (the perf server), and the client's value is summed and reported, as `plan63` does.
5. **`run_suites`** (`loopdiag_run_all.sh`):
   - add `tst core "$CARGO" test -p raptorpath-core`;
   - when `raptorpath/Cargo.toml` declares the `test-seams` feature, add the seams line exactly as `f2-impl`'s `suites_run.sh` has it (release, `--features test-seams`, the three F2 tests, `--test-threads=1`);
   - on a tree without the feature, print `SEAMS n/a`. Plan 41d's existing output is otherwise unchanged.
6. **Budgets:** `BUDGET_S["64"]` from §4; `LD_HARD` = launch + 4.5 h, rehearsal + 1.5 h.
7. **Tests (red first where a path is new):**
   - `STUBRUN_FULL=1` 64/full ALL OK, and fast mode ALL OK;
   - the stub paths:
     - `cand-bulk-unframable` (`F2-FAIL`, not replaced, GATE-64 FAIL with p > 0.05);
     - `cand-bulk-symsz-missing`;
     - `cand-bulk-ctld-send-only`;
     - `base-has-symsz` (ABORT-SEAT);
     - `base-rt-dead` (`BASE-F2-DEAD`, not replaced, no NO VERDICT, not VOID-RC);
     - `cand-rt-dnf` (`RT-FAIL 5`);
     - `cand-rt-ctld-send-only` (`RT-FAIL 2`);
     - `cand-rt-symsz-1200` (`RT-FAIL 4`);
   - unchanged and green: `test_guard_v2` (v1 p 0.2471 / 0.0020 bit-identical, v2 on §48b p 0.0015), `test_pool54`, `test_snap53`, `test_harness_paths` and `test_bbr48_parse` (with the plan list extended), and every other `test_*.py`;
   - the bash tests `test_lib_battery.sh` and `test_abort_witness.sh` run on the VM. They are known Windows failures.

**Done:** all of item 7 green on the host (bash tests excepted) and on a `git archive` extract.

**Report:** the shas, the stub-run summary, the denominator field (or its absence), and every place the worker departed from this section.
