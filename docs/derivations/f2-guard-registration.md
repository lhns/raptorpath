# F2's merge guard: guard v2 on the Bulk cells, plus a Realtime stage, pre-registration draft

Draft for the coordinator to register as its own status section (called §64 here; rename if the number differs). No guard row has run. Round 3 (2026-11-18) takes Fable's rulings on round 2: the bench bound stands, tonight's runs are a pre-read, the wider suite manifest, and stage R's void rule. Round 2 (2026-11-18) followed Fable's review and the night's VM results: the CAND is now `f2-impl` `ab9feec4`, its suites and `framing_ns` have run on the VM, `harness63`'s VOID-RC is fixed, and the launch reuses the suites, the binaries and the tools tests. Base text: github main `499e5f43` (status §63 Result, owner decisions 2026-11-16 item 5 and 2026-11-17 items 11–13). Design: `docs/derivations/f2-framing-design.md` and ADR-0078 (Proposed), both on `f2-impl`.

**For the owner, in plain words.**
- F2's fix changes the release binary, so it needs a §41c guard against main before it merges (owner decision 5 and CLAUDE.md). The identical-sha waiver does not apply.
- **F2 goes alone, not at the bottom of a stack** (a stated default, §1.2). No other zero-behaviour step is ready to stack on it today.
- **The Bulk cells are guard v2 unchanged** (§41d, as §54 ran it): 12 + 12 rows at c1s, c1d, c2 and c8, rank-minp, PASS iff p > 0.05. Bulk's data bytes are unchanged by construction, so the prediction is PASS.
- **One Realtime cell runs in a separate stage.** Behaviour there changes on purpose: main dies in the warm-up some of the time. That stage scores the F2 reading on every CAND row. CAND must also complete wherever BASE's surviving rows complete. Everything else is reported.
- **The framing bench gates the merge:** it must PASS, or you accept its printed cost.
- **Your word is needed for the merge, not for the launch.** It is asked once, with the guard's Result (§3.2). The merge means that all endpoints must upgrade together, because v10 refuses v9, and that an engine with no initial path refuses to start. ADR-0078 goes to Accepted on that go.
- **Cost:** about 2 h of VM time typical, 4.5 h worst, with host cargo paused. The engine suites and the builds are not repeated. Order: s62, then this guard, then E-pre (a stated default).

## 1. BASE and CAND

### 1.1 The seats

| seat | tree | content |
|---|---|---|
| **BASE64** | branch `g64-base` = github main + `harness64` (tools only), as `b51-tree` was for block 51 | main at the pin (`499e5f43` or later). Engine identity: `git diff d2cae698 BASE64 -- raptorpath raptorpath-math raptorpath-core raptorpath-wasm gf256 vendor Cargo.toml Cargo.lock` is empty. **Main takes `harness64` only after the Result** |
| **CAND64** | branch `g64-cand` = BASE64 + `git merge --no-ff github/f2-impl` (`ab9feec4`) | all 17 commits of `main..f2-impl`: `f2-prep`'s three (repro, counter, audit), the merge of `f2-prep` into `f2-design` (`07ac5f0e`), the five design commits, and the eight `f2-impl` commits. Commit 8 (`ab9feec4`) makes the read path allocation-free; it keeps the same bytes and the same behaviour |

- **Commit 1 (`7ce6523b`) never goes alone.** It is labelled v9 but writes LE-packed Realtime symbols that a real v9 peer misreads (design §9, "Commit rule"). The merge takes the branch whole, with `--no-ff`, and nothing is squashed.
- **Commit 8's `offer`, as read by the coordinator; the pin `ab9feec4` stands:**
  - once a packet is held, every later symbol queues behind it, and the queue flushes from its head;
  - a mid-symbol `Full` puts the rejected packet first, then the rest of that symbol;
  - `reliable_split_symbol_resumes_at_the_held_packet` covers a channel filling inside a 3-packet symbol.
- **Identity witnesses, checked when the amendment pins the seats:**
  - `git diff ab9feec4 CAND64 -- . ':!docs' ':!tools'` is empty, so the VM suites at `ab9feec4` were run on CAND's engine tree (§1.3). At the time of writing, the trial merge of main `499e5f43` with `f2-impl` is clean (tree `598d43f1`), and its diff to `ab9feec4` outside `docs/` is empty.
  - `git diff BASE64 CAND64 -- raptorpath Cargo.toml Cargo.lock | git patch-id --stable` equals the same over `d2cae698..ab9feec4`: `3e69ce68…`.
    - This replaces §63 Amendment 1's `490a0d1f…`, which was taken at commit 7. The same path set over `d2cae698..626fdf9a` reproduces `490a0d1f…`, so the method is §63's.
  - `git diff BASE64 CAND64 -- tools` is `f2-impl`'s two-line `suites_run.sh` seams entry only. Both seats run one harness.
- **Both release shas** (`LD_BIN_SHA_BASE`, `LD_BIN_SHA_CAND`) are recorded at the rehearsal and pinned by an amendment. The launch installs those binaries (§1.3) and checks them against the pins.
  - Reported, not checked: §58 Amendment 3's known answer for main's engine (`0ec95258…`) holds only for `binid_run.sh`'s build path. The harness builds at its own path, and the binary embeds location data.

### 1.2 Alone, or the base of a stack: alone (a stated default)

Owner decision 5 says to stack zero-behaviour steps into one guard against main and to bisect only on failure. The F2 design puts F2 at the bottom of such a stack (design §7, §9 "VM order" item 3). **The default is F2 alone now, and the next stacked guard takes post-F2 main as its BASE.** Why:

- **Nothing else is ready to stack:**
  - **Perleg Phase A** (`perleg-phase-a` `e5246d7f`) sits on `2aaac8e8`.
    - It contains `1cd13f5c` (checked: an ancestor) but lacks `14954df7`, the fix for `gates::forwarding_audit`, which has been red since `1cd13f5c`. So that test is red on the branch as it stands.
    - It has no Linux suite record.
    - Its A1 step is bounded (≤ 16 ns), not bit-identical, so its trace pin changes by design and needs its own Linux run first.
    - It merges cleanly with main and with `f2-impl` (trial `merge-tree`, no conflicts), so nothing in F2 blocks it later.
  - **Guard α's shared-core moves** (`shared-core-design.md`, steps 1.2–2.5) have no branches yet.
  - **The slaw gauge** (`slaw-gauge`) waits on the owner (status, "Still held").
- **F2 is not a zero-behaviour step.** It changes Realtime on purpose, and it breaks the wire (v10). Owner decision 5 covers zero-behaviour steps.
- **Attribution.** Alone, a FAIL points at F2 without a bisection run. In a stack, a FAIL costs one more guard.
- **The critical path.** Block 51 needs the merged binary (§63 Result, "What follows").
- **What this costs:** Phase A later gets its own guard instead of sharing this one, about 1.5 h with reuse.

### 1.3 Reuse: no engine suites, no builds and no tools tests in the launch

**Engine suites: reused through a reference-manifest HIT.** This is plan 60's mechanism (`suites_stage56`), extended to plan 64, over the **wider `src_manifest()` set.**
- **The record:** the VM suites at `ab9feec4` (2026-11-18), with its `TESTS.txt` copied into the ledger at the pin:
  - release 1151/0;
  - debug 1002 passed;
  - wasm 50/0, core 2/0, seams 3/0;
  - **one FLAKE line** under §8/§11/§12's rule: `two_path_client_fails_fast_when_server_is_killed`, 32.03 s against its 32 s bound, passed on the immediate solo re-run.
    - It is main's test, not F2's. It polls every 50 ms and its bound has no allowance for that poll, so a run that ends just after the bound reads as a failure.
    - Cited, not fixed here.
- **The set.** `src_manifest()` (`loopdiag_run_all.sh`) covers:
  - the workspace and every crate `Cargo.toml`, and `Cargo.lock`;
  - `raptorpath/src` and `raptorpath/tests`;
  - `raptorpath-math/src`, `raptorpath-core/src`, `raptorpath-wasm/src`, `gf256/src` and `vendor`;
  - but not `tools/` or `docs/`.

  So `raptorpath/Cargo.toml`'s `test-seams` declaration and every crate the suites compiled are inside it. The narrower `engine_manifest()` (`raptorpath/src` and `tests` only) is not used for plan 64.
- **The reference** is `tools/l1/ref64-manifest.sha256` on `harness64`, generated from the `ab9feec4` archive, the tree those suites ran on. Its lines are `src_manifest()`'s exactly:
  ```
  git archive ab9feec4 | tar -x -C /tmp/ref64
  (cd /tmp/ref64 && { ls Cargo.toml Cargo.lock ./*/Cargo.toml 2>/dev/null | sed 's#^\./##'
     find raptorpath/src raptorpath/tests raptorpath-math/src raptorpath-core/src raptorpath-wasm/src gf256/src vendor -type f 2>/dev/null; } \
   | LC_ALL=C sort -u | xargs -r sha256sum) > tools/l1/ref64-manifest.sha256
  ```
  `.gitattributes` makes archives LF, so a checkout and an archive of one commit give the same lines.
- **The host proof of the HIT, before the launch:**
  - the same command on the `g64-cand` archive gives a file that `cmp`s equal to `ref64-manifest.sha256`. It is recorded in the pin amendment;
  - **pre-proof, done:** on the trial merge tree `598d43f1` (main `499e5f43` + `f2-impl`, without `harness64`, which touches only the excluded `tools/`), the 212-line manifest `cmp`s equal to `ab9feec4`'s.
- **The check on the VM:** plan 64's suites stage computes `src_manifest()` and compares it with `LD_REF_MANIFEST` (default `tools/l1/ref64-manifest.sha256`).
  - **A HIT skips the engine suites.**
  - **A MISS stops the launch before any row** (`ABORT-SUITES`), as plans 60a/60b do. A MISS means CAND64 is not the tested tree. Running suites in the session would hide that.
  - `LD_PRIOR_TESTS` is refused: `suites_run.sh` writes `TESTS-OK` but no `SRC-MANIFEST.sha256`, and there is one reuse rule.
  - The git witness `git diff ab9feec4 CAND64 -- . ':!docs' ':!tools'` stays in the amendment as a second check.

**Binaries: the launch installs the rehearsal's two builds.** This is plan 60's `LD_BIN_PREBUILT`, extended to two seats: `LD_BIN_PREBUILT_BASE` and `LD_BIN_PREBUILT_CAND`.
- Each is the rehearsal's `target/release/raptorpath`, copied out of its run dir.
- Each is sha-checked against the amendment's pins before any row. A mismatch is `ABORT-BUILD`.

**Tools tests: the launch reuses the rehearsal's result when `tools/l1` is byte-identical.**
- The rehearsal runs every tools test, python and bash, and writes `TOOLS-TESTS-OK` together with `TOOLS-MANIFEST.sha256` (sha256 of every file under `tools/l1`, in C-locale order).
- The launch compares its own `tools/l1` manifest with the rehearsal's.
  - Equal: it copies the rehearsal's tools result (`LD_PRIOR_TOOLS=<rehearsal run dir>`).
  - Different, or the rehearsal record is missing: it runs the tools tests, about 8 min.
- Both seats share one `tools/l1` (§1.1), so one result covers both.

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
  - `[SYMSZ] local=1200 decoder=1200` on both ends. This is the executes witness: the v10 handshake ran, and the receiver adopted the peer's size;
  - a `[CTLD]` line carrying `undeliv=` on the receiving end (the perf server), and `undeliv=0` on every `[CTLD]` line of both ends. The client's line need not be present; a non-zero value on either end is a fail;
  - no `[UNFRAMABLE]` line on either end.
- **The lines exist on these rows** (checked on committed ledgers and code):
  - `[CTLD]` is on both ends of every §54 guard row: 24 of 24 server logs and 24 of 24 client logs at each of c1s, c1d, c2 and c8 (`docs/l1-raw/s54/guard-diag-ld.tgz`). It is a `RWM_DIAG` readout, and every guard row runs `RWM_DIAG=1`. `f2-impl` appends `undeliv=` to that same line (`receiver.rs`, the `[CTLD]` readout).
  - `[SYMSZ]` is an ungated `info!` at the decoder build (`f2-impl` `net/mod.rs`, where `recv_symbol_size` is chosen), printed once per process. §63 read it on both ends.
- **Status precedence on a stage G row, in this order:**
  1. an infrastructure void (VOID-COTENANT, VOID-ONDISK, CONTAMINATED, a seat or sha mismatch) comes first and is replaced under §41d's rule;
  2. `undeliv` > 0 or an `[UNFRAMABLE]` line is `F2-FAIL`, even on a DNF or VOID-RC row, because a fail-closed tunnel looks like a DNF;
  3. a missing witness line (`[SYMSZ]`, or the server's `[CTLD]`) is `F2-FAIL` only on a row that is otherwise LIVE or DNF;
  4. then §41d's own labels (VOID-SHORT, DNF, LIVE).
- **A CAND row that fails any of these is `F2-FAIL`.** It is never VOID, never replaced and never NO VERDICT. After the fix, an unframable symbol can come only from an engine bug (owner decision 12); at Bulk it is a fail-closed false positive. One such row fails the guard whatever p reads.
- **Seat witnesses:**
  - before any row, the CAND binary contains the strings `[SYMSZ]` and `[UNFRAMABLE]`, and the BASE binary contains neither. Otherwise the run aborts (ABORT-SEAT);
  - every row's `bin_sha` matches its arm's pin.
- **Prediction: GATE-41d PASS.**
  - Bulk and Auto data bytes are unchanged. `hotpath_pin` passes unchanged at commit 1; commit 2 re-captures it for the version field only (ADR-0078, Consequences).
  - The CPU change per symbol in `framing_ns`'s pre-read (§3.1) is under +22 ns to frame (an upper bound: it includes a seam main already pays) and under +6 ns to read. At c1s, +22 ns is 19 ms/GB on the client against `cpu_cli/GB` 15.35 s, i.e. +0.12 %, against the clause's d80 of 2.4 %.
  - The control path changes once per path: the handshake carries the real size (1200 at Bulk, as before), and `WindowStart` loses two fields.
  - **The registered risk is the false-fail rate,** 5 % by construction. §54's no-regression candidate read its min-p at c1s `cpu_cli/GB`, p = 0.066.
- **Prediction for the witnesses:** 48 of 48 CAND rows pass, and the BASE binary prints no `[SYMSZ]`.

### 2.2 Stage R, Realtime (its own ledger, after stage G; plan 52's pattern)

**Why it is separate.** Four facts force it out of the min-p family:
- BASE fails the Realtime warm-up some of the time. That is the defect: 4 of 9 draws in §63's pooled CONTROL. Those slots would exhaust stage G's 4-slot replacement cap and read NO VERDICT.
- No A/A ledger exists for any Realtime cell, so no clause there has a known resolution.
- Main has no `undeliv` counter (it came with `f2-prep`), so BASE cannot carry the F2 reading.
- The behaviour changes on purpose, so a CAND-against-BASE comparison is not a regression test there.

**The cell.**
- **c2-100, Realtime hint, ρ = 1, 100 MB, `RWM_PERF_TIMEOUT_S=150`, `RWM_DIAG=1`.**
  - Arms **RB** (BASE's binary) and **RC** (CAND's binary), 12 + 12, seeds 42 and 7 × 6 each, interleaved as plan 41c.
- **The bytes come from a committed row that completed. The timeout is this registration's choice.**
  - In `store-law-n1` (`docs/l1-raw/store-law-n1/diag-sl/c2-100-LAW-realtime-s42-r{1,2,3}-a1-*`), c2-100 Realtime at ρ = 1 ran 100 MB in 11.5–11.7 s, 68.6–69.5 Mbit/s, three rows. 100 MB is also c2's guard size.
  - The timeout of 150 s is plan 41c's guard-size timeout, 13× that duration. The committed rows do not record their timeout.
  - The rehearsal runs one row per seat (RB s42, RC s42) on the current binaries before any reading (§2.3).
  - 600 MB is not used. It was never run on a committed Realtime row, and a dead BASE warm-up waits out the whole timeout.
  - So the in-order delay mean is reported over a run of about 12 s, below §41d's 30 s floor, and is marked so.
- **ρ = 1 on both ends, and witnessed.**
  - Realtime's preset evicts, and each side resolves it on its own (`config.rs`: `window_reliable.unwrap_or(hint != Realtime)`).
  - `perf_rwm_c.sh` passes `$WR_FLAG` (`--window-reliable` under the default `RWM_C_RELIABLE=1`) to both the `--server` and the `--client` command lines. `RWM_C_RELIABLE` is scrubbed from the row env.
  - **The witness is the resolved config each end prints:** the `perf configuration` line (`main.rs`, after `config::resolve`) carries `window_reliable: true`.
    - It reads `true` on both ends of the `store-law-n1` rows above and `false` on both ends of §63's evicting rows (`rep63-diag-ld.tgz`), so it separates the two.
    - At Realtime the engine's own `reliable window mode` echo does not print (`net/mod.rs`: it sits in the non-Realtime branch), so the config line is the only witness.
    - A row without `window_reliable: true` on both ends is VOID-RHO, a harness fault that the rehearsal must catch.
- **Why ρ = 1 and not the shipped Realtime default ρ < 1:**
  - At ρ = 1 the rows complete, so goodput, CPU/GB and the delay readout can be read. Evicting rows DNF by design (§63 Result, the correction to Amendment 2).
  - ρ is an independent dial. The F2 reading at ρ < 1 is §63's: 5 of 5 rows, the unpaced U row included. Block E and E-pre read it again on the E tree plus F2. Together with this stage, both ends of ρ at the Realtime point carry the reading.
- **The other lines exist on main's binary:**
  - `warm-up object never acked` is in `perf.rs` (the warm-up `bail!`). The client exits on it after the full `RWM_PERF_TIMEOUT_S`.
  - `REALTIME_ECHO59` (`net/mod.rs`) prints on both ends of a ρ = 1 Realtime row (`store-law-n1` above).
- **Limit, stated:** perf chunks fill the symbol (a 508 B packet in a 512 B symbol, 510 B with its length field; `perf.rs` `chunk_payload_len`). So no row packs two or more packets into one symbol. Only `f2_realtime_packed` (commit 6, a loopback test in the seams suite) covers that, and the stage cannot see it.

**Scored on every RC row: the F2 reading only.**
1. the warm-up acked;
2. a `[CTLD]` line with `undeliv=` present on the receiving end (the server), and `undeliv=0` on every `[CTLD]` line of both ends. A missing server line is a missing witness, not a pass. The sum over both ends is reported (§63 Amendment 1);
3. no `[UNFRAMABLE]` on either end;
4. `[SYMSZ] local=512 decoder=512` on both ends.

**Scored, relative to BASE: completion.**
5. If every live RB row (warm-up acked, not VOID) completes inside its timeout, every RC row whose items 1–4 pass must complete too.
   - If any live RB row is DNF, RC completion is reported only. The seat then cannot complete either, and a CAND DNF is no evidence against F2.
   - If there are no live RB rows (k = 12), item 5 is not scored and is reported as such.

- **RT-PASS iff all 12 RC slots hold a row that passes items 1–4, and item 5 holds wherever it applies.**
- A row that fails an item is `RT-FAIL <item>`. It is never replaced.
- **A non-F2 void on an RC row** (VOID-COTENANT, VOID-ONDISK, CONTAMINATED, VOID-RHO, a sha mismatch):
  - its slot is replaced once, and at most 2 slots are replaced in this stage;
  - if the replacement also voids, or a third slot voids, the slot is reported as `RC-VOID` and counts as **not passed**. So "12 of 12" fails, and stage R reads `RT-INCOMPLETE`: not a pass, and not a finding against F2;
  - the merge then waits for a stage R re-run.
  - An F2 failure is never a void (items 1–4 above).

**Reported only:**
- **BASE warm-up deaths, k of 12.**
  - An RB row whose client prints `warm-up object never acked` is `BASE-F2-DEAD`: a reading, never replaced, never VOID-RC.
  - There is no prediction. The committed base rates are 1 of 6 at ρ = 1 Realtime (`store-law-n1`) and 4 of 9 at ρ < 1 (§63 pooled).
  - k = 0 makes the RC reading weak (nothing reproduced). That is reported, not voided (§63's rule).
- **Medians and [min–max], RC against the live RB rows:**
  - goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli`, `lag_E_srv`;
  - the in-order delay mean (`threadp1_parse.inorder_delay_stats`, under 30 s of ticks, marked);
  - `rtp_floor_p0`.

  The one-sided rank p is printed per readout. The live RB rows are selected by a draw (the warm-up survived), which may correlate with early loss at seed 42. So the comparison is biased in an unknown direction and is never a verdict.
- **The `undeliv` denominator,** ported from `harness63` `16a6ed17` (`plan63.row_metrics`):
  - `src_n` from the server's last `[RFA]` line (its final flush);
  - `rx=` and `dgrx0[fr=…]` from the server's last `[CTLD]` line.

  §63's CAND rows read 42–531 k source symbols.
- **Owner flag (non-gating):** a reported readout with CAND worse than c2's Bulk d80 for the same clause, with rank p ≤ 0.05, goes to the owner with the merge question.
  - The d80 values are the guard-size ones, because 100 MB is c2's guard size (`loopdiag_battery.sh` cell spec `c2 c2 single 100000000`): goodput 2.4 %, `cpu_cli/GB` 8 %, `cpu_srv/GB` 8 % (`guard-clause-dry.md` §11, the v1 column).
  - The in-order delay is not in the flag. It has no resolution on rows under 30 s; it is printed and marked.
  - Behaviour on rows whose warm-up survives is predicted unchanged, because packed symbols keep their size (ADR-0078, Consequences).
  - The read path's packed point is 30 ns faster (§3.1).

### 2.3 Rehearsal (`LD_PLAN=64 LD_REHEARSE=1`) must show

| check | NO-GO consequence |
|---|---|
| both builds; CAND's binary holds `[SYMSZ]` and `[UNFRAMABLE]`, BASE's neither; both shas recorded for the pin | ABORT-SEAT: the seats are wrong; fixed and re-rehearsed |
| **a completed row is LIVE on each seat:** plan 41d's c1s smoke rows, one BASE and one CAND, both LIVE (rc 0), not VOID-RC | a VOID-RC on a completed row is the §63 defect back (or a new one): fixed on `harness64` and re-rehearsed |
| plan 41d's 600 MB c8 CAND smoke row completes, no VOID-SHORT. On every CAND smoke row: `[SYMSZ]` 1200/1200 on both ends and the server's `[CTLD]` with `undeliv=0` | a missing line is a harness or log-level fault: fixed and re-rehearsed. `undeliv` > 0 or `[UNFRAMABLE]` is a finding against the fix: no launch |
| RC s42 passes §2.2 items 1–4, completes inside 150 s, is LIVE with rc 0, shows `window_reliable: true` on both ends, and has its readouts computed (not None) | a missing witness, a VOID-RC, a VOID-RHO or a None is a harness fault: fixed and re-rehearsed. A failed item is a finding against the fix: no launch |
| RB s42 is LIVE with `window_reliable: true` on both ends, or `BASE-F2-DEAD` | any other label (VOID-RC included) is a harness fault: fixed and re-rehearsed |
| every tools test passes on the VM, bash tests included; `TOOLS-TESTS-OK` and `TOOLS-MANIFEST.sha256` written | ABORT-TESTS: fixed on `harness64` |
| the reference manifest HITs (`suite-reuse HIT` in the era log) | a MISS: the tree is not `ab9feec4`'s; no launch until the seats are fixed |
| `framing_ns`, 10 runs on CAND's tree: the per-point medians and ranges printed, and the label | none for the launch: the label goes into the merge conditions (§2.4) |

- A rehearsal row is not re-run, and a failed CAND rehearsal row is the finding (§63's rule).
- The launch is a fresh `git archive` of the pinned CAND64 and BASE64 trees, with the rehearsal's two binaries installed (§1.3).

### 2.4 Decision rule

- **MERGE** iff all of:
  - GATE-41d PASS;
  - 0 `F2-FAIL` rows in stage G;
  - RT-PASS in stage R;
  - `framing_ns`'s rehearsal label (§3.1 item 1) is `BENCH PASS`, or the owner accepts the printed cost;
  - the owner's go (§3.2), asked with this Result.

  Then `f2-impl` merges to main with `--no-ff`, and main takes `harness64`'s tools. As in §54, the merged tree must differ from the guarded CAND64 only in files main gained after BASE64 (docs, roadmap, ledgers) and in `harness64`'s tools, and that check is recorded.
- **GATE-41d FAIL:** no merge. The failing clause and the session's resolution table go to the owner. There is no bisection, because F2 is alone. The next step is a diagnosis of that clause.
- **Any `F2-FAIL` or `RT-FAIL`:** no merge. It is a finding against the fix (a fail-closed false positive, a size defect, or a Realtime failure) and is diagnosed before anything else. A fix makes a new CAND, new suites and a new guard.
- **NO VERDICT** ⇒ re-run (the rehearsal's binaries and reuse stand).
- **The reported readouts decide nothing by themselves.** The owner flag travels with the merge question.

## 3. Pre-flight

### 3.1 Coordinator items

1. **`framing_ns`: the bound, a labelled pre-read, and the scored reading in the rehearsal.**

   **The bound (formula first; written at `84a88541`, before any number; it stands).** The change in CPU per GB that framing can cause must be at most a tenth of the c1s CPU clause's BASE noise:

   ```
   Δt_point × N_GB  ≤  f × q95_side × C_side
   ```

   | symbol | value | provenance |
   |---|---|---|
   | Δt_point | CAND − BASE, ns/call, per bench point: the per-point median over 10 runs | measured (the rehearsal's run, below) |
   | N_GB | 10⁹ / 1182 ≈ 8.46·10⁵ source symbols per GB | code: `perf.rs` `chunk_payload_len` gives 1196 − 14 (`HDR_LEN`) = 1182 B of object payload per Bulk chunk, one chunk per symbol. Each source symbol is framed once at the sender and read once at the receiver; repair symbols and retransmits are not re-framed |
   | C_side | `cpu_cli/GB` 15.35 s (sender: frame), `cpu_srv/GB` 17.86 s (receiver: read) | measured: §54 launch 1, c1s BASE medians |
   | q95_side | 0.0123 (cli), 0.0173 (srv) | measured: §54 launch 1, c1s, q95 of \|x\| over 400 half-per-seed splits of BASE alone |
   | f | 0.1 | declared margin: framing must sit an order of magnitude below the noise the guard's c1s CPU clauses cannot see |

   - **Why c1s:** it has the smallest q95 × C product of the four cells on both sides (cli: c1s 0.19, c2 0.48, c1d 0.86, c8 1.16 s/GB; srv: c1s 0.31, c1d 0.62, c2 1.29, c8 2.36 s/GB; §54 launch 1). N_GB is the same at every Bulk cell.
   - So **frame ≤ +22.3 ns/call** and **read ≤ +36.5 ns/call** at each S = 1200 point. The packed point (S = 512, 3 × 160) is reported only, because no Realtime CPU clause has an A/A.

   **The scored reading: a fresh run in the rehearsal (§2.3).**
   - On CAND64's tree, release, both locks held, host cargo paused, `taskset -c 2 cargo test -p raptorpath --lib --release framing_ns -- --ignored --nocapture`, **10 runs**.
   - The statistic per point is the **median over the 10 runs of (CAND median − BASE median)**, with the min–max of the 10 differences printed beside it.
   - Not the worst difference: a maximum grows with the number of runs.
   - **Label:** `BENCH PASS` iff the median is within the bound at all six S = 1200 points. Otherwise it is `BENCH OVER <points>`, printed with the cost each point implies: Δt × N_GB in ms/GB, and as a % of C_side.
   - **The label is a MERGE condition (§2.4): PASS, or the owner accepts the printed cost.** It does not block the launch.

   **The frame point over-reads, so it is an upper bound** (coordinator's code read, checked):
   - The bench's BASE is v9's `frame_window_packet_inner` inlined (`base_v9::frame`).
   - CAND calls the outer `frame_window_packet`, which runs the `cpuprof::timed(Seam::Frm, …)` seam: a `gauge()` lookup, with `RWM_CPUPROF` off by default.
   - Main's own `frame_window_packet` has the same seam (main `raptorpath/src/net/framing.rs:61–67`; `f2-impl` `:104–111`).
   - So the frame difference includes a cost main already pays. It is registered as an **upper bound** on F2's frame cost.
   - The read points have no seam on either side.

   **Pre-read (labelled; not scored).** Tonight's runs on the VM: release, `taskset -c 2`, 3 runs each of 5 alternating pairs × 2 000 000 calls. The raw outputs are committed: `docs/derivations/f2-guard/framing_ns-626fdf9a.txt` (commit 7) and `framing_ns-ab9feec4.txt` (commit 8). CAND − BASE per run, in ns/call:

   | point | commit 7 (`626fdf9a`) | commit 8 (`ab9feec4`) |
   |---|---|---|
   | frame S=1200 len=64 | +11.2, +9.2, +15.4 | +20.7, +8.7, +12.8 |
   | frame S=1200 len=600 | +6.4, +21.3, +10.6 | +4.8, +9.8, +20.0 |
   | frame S=1200 len=1196 | +5.6, +7.4, +8.3 | +9.4, +5.1, +4.7 |
   | read S=1200 len=64 | +50.7, +44.9, +51.7 | +2.0, +1.7, +2.8 |
   | read S=1200 len=600 | +55.1, +50.1, +49.0 | −1.7, +0.7, +5.4 |
   | read S=1200 len=1196 | +55.4, +53.1, +71.0 | +4.6, +4.9, +1.4 |
   | read S=512 3×160 packed | +12.5, +13.7, +14.4 | −32.4, −31.1, −33.9 |

   - Commit 8 removed the read path's extra allocation: +45–71 ns becomes −1.7 to +5.4 ns.
   - **Prediction (design §7): MISSED on frame.** Frame is positive in **18 of 18** point-runs over commits 7–8, from +4.7 to +21.3 ns, against the design's ±2 ns. Part of it is the seam above.
   - The read prediction (+1–3 ns) is about met at commit 8 (−1.7 to +5.4).
   - The packed point is faster at commit 8.
   - The 3-run medians at commit 8 (frame +12.8, +9.8, +5.1; read +2.0, +0.7, +4.6) sit inside the bounds. That is a pre-read, not the reading.
2. **The harness of §5,** tools only, on `harness64`, with its stub runs green. It goes into BASE64 and CAND64, not into main, before the Result.
3. **An amendment pinning BASE64, CAND64 and both release shas** after the rehearsal. It records the identity witnesses of §1.1, the suite record of §1.3 (copied `TESTS.txt`), and the reference-manifest `cmp`.
4. **Fable's review of this round.**
5. **Status pages at the merge:**
   - §1's `PROTOCOL_VERSION = 9` line;
   - the receiver-timestamp texts that still say v10: `docs/status/42-…md:21`, `:76` and `docs/status/43-…md:213` (design §3). That field becomes v11.
6. **§63's "Before the merge" list, checked against `f2-impl`:**
   - the early-`add_path` and no-initial-path size gaps: closed (commit 5, `f2_peer_symbol_size`);
   - the `test-seams` tests in `suites_run.sh`: done (commit 6), and they ran on the VM (seams 3/0). A guard that reuses the suites needs no change to `run_suites`. §5 item 5 adds them for a guard that runs the suites itself;
   - ADR-0078's deviations, including whether a fast close drops earlier deliveries: recorded (commit 7);
   - an end-to-end Realtime test with two or more packets in one symbol: done (commit 6, `f2_realtime_packed`);
   - `framing_ns` on the VM: pre-read done; the scored reading is in the rehearsal (item 1).
7. **§63's VOID-RC is solved** (`harness63` `16a6ed17`): a bare `grep -c` with no match, under `lib.sh`'s `set -e`, killed `perf_rwm_c.sh` (rc 1). `harness64` takes the fixed lines (`|| true` on every F2 substitution) and the red tests that caught it (`dbec2516`, `ff41d646`). The rehearsal asserts LIVE on a completed row of each seat (§2.3).

### 3.2 Owner items: they gate the merge, not the launch

They are asked **once, with the guard's Result**. If the guard passes, the question is:

> **Merge F2 (wire v10) to main?** The merge changes:
> - **all endpoints must upgrade together.** v10 refuses every v9 peer at the handshake, so a mixed pair fails to connect before any data symbol (design §3). No ref scanned has a version ≥ 10, and there is no Android client in this repo (design §3, §5.1). Any v9 build in use elsewhere stops interoperating;
> - **an engine with no initial path refuses to start** (a server with no bind address, a client with no peer). The receiver learns its decode size only from a peer's handshake, so there is no size to start with (ADR-0078, Implementation notes; commit 5). A runtime `add_path` is refused until a peer size is stored, and a path that announces another size is refused and removed;
> - Realtime stops dying in the warm-up (§63; this guard's stage R), and Bulk's data bytes are unchanged.
>
> On a yes, ADR-0078 goes from Proposed to Accepted in the merge.

**Stated defaults, not asked:**
- F2 alone (§1.2);
- the order (§4).

The owner can overrule either.

## 4. Cost and order

| step | content | typical | basis |
|---|---|---|---|
| rehearsal | two release builds (≈ 15 min), every tools test (≈ 8 min), the reference-manifest check, plan 41d's 3 smoke rows, RB s42 and RC s42, and `framing_ns` × 10 (a release lib-test build ≈ 8 min, plus 10 runs of ≈ 15 s) | 0.75 h | §63's rehearsal: two builds, 20 min. A dead RB s42 adds 2.5 min |
| launch | sha checks of the installed binaries, the manifest HIT, the tools reuse, the smoke, stage G (96 rows), stage R (24 rows), up to 6 replacements, scoring | 1.3 h | §54 launch 1: 48 min of guard rows, and 58 min before them, which reuse removes apart from the smoke (≈ 5 min). Stage R: 24 × (≈ 12 s + ≈ 15 s) ≈ 11 min, plus ≈ 2.5 min per dead RB row (k ≈ 2–5 gives 5–12 min). Replacements and scoring ≈ 10 min |
| **total** | | **≈ 2 h** | host cargo paused throughout; the suites are already done |

- **Worst case, about 4.5 h:** every RB row dead (+28 min), plus a NO VERDICT re-launch (+1.3 h) or a rehearsal re-run (+0.75 h). A stage R `RT-INCOMPLETE` re-run is about 20 min. A manifest MISS costs nothing on the VM, because the launch stops before any row; the seats are then fixed.
- **`LD_HARD`:** launch + 2.5 h; rehearsal + 1.25 h.
- Without the reuse it would be about 3.5 h (round 1's estimate).

**Order (a stated default):**
1. **s62 first.** Owner decision 13 holds literally (s62 after §63). s62 is registered (~1.4 h typical, 2.8 h worst), and this guard still waits on `harness64`.
2. **This guard.**
3. **E-pre**, on the E tree plus F2 as guarded (`ab9feec4`'s engine commits).
4. Block E, then block 51 (which needs the merged binary).

- **Why the guard comes before E-pre.** E-pre runs on "the final F2 tree" (§63 Result). A guard FAIL changes F2, which would waste E-pre's suites and CAND pin. The design doc's VM order also puts the guard first ("After the guard, E-pre").
- **When to swap them:** if `harness64` is not ready when s62 ends, E-pre may go first. The cost of swapping is a redo of E-pre if the guard then changes F2. The owner's go no longer blocks the launch (§3.2), so it is no longer a reason to swap.
- **Perleg Phase A's guard** follows on post-F2 main as its BASE, stacked with whatever else is ready then.

## 5. Harness change (Sonnet worker)

**Branch:** `harness64` off github main, in its own worktree, **tools only**. Never touch `raptorpath/` or any engine crate. Commit there; push only if told.
- It is merged into the seat branches `g64-base` and `g64-cand` (§1.1), **not into main**. Main takes it after the Result.

**Decided (do not reopen):** everything in §1–§2. The plan token is `64`.

1. **Port F2's row primitives from `harness63` at `16a6ed17`,** the fixed lines (`e8b61a73` and its follow-ups through `16a6ed17`), adapted to main's `tools/l1`, without the plan 63 and plan 51e parts:
   - `l1common.f2_counts`, `f2_void` and `symsz`, with their `test_l1common.py` cases;
   - `perf_rwm_c.sh`'s `--- F2 undeliv=… unframable=…` line, **as fixed:** every substitution ends in `|| true`, because of `lib.sh`'s `set -e`;
   - the red tests that caught the bug (`dbec2516`, `ff41d646`): the real perf script's F2 lines run under `set -e` in the unit test and in the stub driver, and a completed row is not VOID-RC;
   - the denominator readout of `plan63.row_metrics` (`src_n` from the server's last `[RFA]`; `rx=` and `dgrx0[fr=…]` from its last `[CTLD]`).
   - **The presence check reads the logs' `[CTLD]` lines, never the `--- F2` line.** That line prints `undeliv=0` when no counter exists (BASE).
2. **`LD_PLAN=64` = plan 41d's stage `guard/`, unchanged** (map 64 → 41d before any branch, as 54 is mapped), plus:
   - the F2 row witnesses of §2.1 on CAND rows, behind `LD_WITNESS64=1`, applied in §2.1's status precedence. A failing CAND row gets the new status **`F2-FAIL`**: never replaced, never counted toward NO VERDICT;
   - `f2_void` on every BASE row (expected None);
   - the binary pre-check: `[SYMSZ]` and `[UNFRAMABLE]` present in CAND's binary and absent in BASE's, else `ABORT-SEAT`;
   - **`score64`:** it prints `score41d`'s output byte for byte (a stub test asserts this), then the F2 witness table, then `GATE-64 PASS` iff GATE-41d PASS and `F2-FAIL` = 0, else `GATE-64 FAIL (<reason>)`.
3. **Stage R in `$RUN/rt64/`,** after stage G, with its own ledger and score (plan 52's pattern; `DONE-ALL` needs both stages):
   - **rows:** arms `RB`/`RC` on the BASE/CAND binaries, cell c2-100, `HINT=realtime` per row (plan 60's per-row hint mechanism), 100 MB, `RWM_PERF_TIMEOUT_S=150`, `RWM_DIAG=1`, seeds 42/7 × 6 per arm, plan 41c's interleave;
   - **ρ = 1 on both ends:**
     - `RWM_C_RELIABLE` is scrubbed from the env, so `perf_rwm_c.sh` passes `--window-reliable` to the server and the client;
     - the row witness is `window_reliable: true` in the `perf configuration` line of **both** logs, else VOID-RHO;
     - stub cases cover `true`/`true` (LIVE), `true`/`false` (VOID-RHO) and the §63-style `false`/`false` (VOID-RHO);
   - **the pipe-echo witness:** `REALTIME_ECHO59` on both ends;
   - **row labels:**
     - an RB row with `warm-up object never acked` → `BASE-F2-DEAD`, decided before the rc test;
     - an RC row → `RT-OK` or `RT-FAIL <item>` per §2.2 items 1–4;
     - item 5 is scored per stage, from the live RB rows' completions;
     - infrastructure voids (VOID-RHO included) are replaced, at most 2 slots;
   - **`rt64_score.py` prints:**
     - `RT-PASS` or `RT-FAIL (<rows and items>)`;
     - `BASE-F2-DEAD k/12`;
     - item 5's state (`SCORED`, `REPORTED (an RB row DNF)` or `NOT-SCORED (no live RB)`);
     - per readout, RC and live-RB medians with [min–max] and the one-sided rank p;
     - the owner-flag lines (§2.2);
     - the denominator per RC row (ported from `plan63.row_metrics`: `src_n` from the server's last `[RFA]`, `rx=` and `dgrx0[fr=…]` from its last `[CTLD]`);
     - `RT-INCOMPLETE` when a slot is `RC-VOID` (§2.2);
   - **stage R's rehearsal rows:** RB s42 and RC s42.
4. **The F2 witnesses:** presence is checked on the receiving end (the perf server's `[CTLD]`) and on both ends' `[SYMSZ]`. The `undeliv` value must be 0 on every `[CTLD]` line of both ends, and the sum is reported, as `plan63` does.
5. **`run_suites`** (`loopdiag_run_all.sh`), for a guard that does run its suites:
   - add `tst core "$CARGO" test -p raptorpath-core`;
   - when `raptorpath/Cargo.toml` declares the `test-seams` feature, add the seams line exactly as `f2-impl`'s `suites_run.sh` has it (release, `--features test-seams`, the three F2 tests, `--test-threads=1`);
   - on a tree without the feature, print `SEAMS n/a`. Plan 41d's existing output is otherwise unchanged.
6. **Reuse (§1.3):**
   - **suites:** plan 64 uses `suites_stage56`'s HIT/MISS logic, but compares `src_manifest()` (the wider set), not `engine_manifest()`, against `LD_REF_MANIFEST` (default `tools/l1/ref64-manifest.sha256`). Generate the file from the `ab9feec4` archive with §1.3's command and commit it. A MISS is `ABORT-SUITES` before any row, the rehearsal included, as for plan 60. `LD_PRIOR_TESTS` is refused. Plans 56/57/60 keep `engine_manifest()` unchanged;
   - **binaries:** `LD_BIN_PREBUILT_BASE` / `LD_BIN_PREBUILT_CAND`, plan 60's `LD_BIN_PREBUILT` per seat. Each is checked against `LD_BIN_SHA_BASE` / `LD_BIN_SHA_CAND` before any row (`ABORT-BUILD` on a mismatch). A launch without them builds as today;
   - **tools tests:** the rehearsal writes `TOOLS-MANIFEST.sha256` (sha256 of every file under `tools/l1`, C-locale order) beside `TOOLS-TESTS-OK`. `LD_PRIOR_TOOLS=<rehearsal run dir>` reuses it when the launch's own manifest is byte-identical, and prints `TOOLS-TESTS-REUSED <dir>`. Otherwise the tools tests run.
7. **The rehearsal's `framing_ns` step:** after the builds, on CAND's tree with its own release test target, under both locks, run `taskset -c 2 cargo test -p raptorpath --lib --release framing_ns -- --ignored --nocapture` 10 times into `bench/framing_ns.txt`. `framing64.py` parses it (the bench's line format, as in `docs/derivations/f2-guard/`), prints per point the median of the 10 (CAND − BASE) differences with their min–max, and prints the label of §3.1 item 1. Test it on the two committed pre-read files.
8. **Budgets:** `BUDGET_S["64"]` from §4, with every RB row budgeted at its full timeout (a dead warm-up waits 150 s); `LD_HARD` = launch + 2.5 h, rehearsal + 1.25 h.
9. **Tests (red first where a path is new):**
   - `STUBRUN_FULL=1` 64/full ALL OK, and fast mode ALL OK;
   - the stub paths:
     - `cand-bulk-unframable` (`F2-FAIL`, not replaced, GATE-64 FAIL with p > 0.05);
     - `cand-bulk-dnf-unframable` (`F2-FAIL`, not DNF);
     - `cand-bulk-symsz-missing`;
     - `cand-bulk-ctld-send-only`;
     - `base-has-symsz` (ABORT-SEAT);
     - `completed-row-live` (rc 0 on each seat under `set -e`);
     - `base-rt-dead` (`BASE-F2-DEAD`, not replaced, no NO VERDICT, not VOID-RC);
     - `rt-rho-one-end` (VOID-RHO);
     - `cand-rt-dnf-base-completes` (`RT-FAIL 5`);
     - `cand-rt-dnf-base-dnf` (item 5 REPORTED, RT-PASS if items 1–4 hold);
     - `cand-rt-ctld-send-only` (`RT-FAIL 2`);
     - `cand-rt-symsz-1200` (`RT-FAIL 4`);
     - `manifest-miss` (ABORT-SUITES, nothing run);
     - `manifest-miss-cargo-toml-only` (only `raptorpath/Cargo.toml` differs: ABORT-SUITES);
     - `rc-void-twice` (`RC-VOID`, `RT-INCOMPLETE`, not RT-PASS);
     - `prebuilt-sha-mismatch` (ABORT-BUILD);
     - `tools-manifest-differs` (the tools tests run);
   - unchanged and green: `test_guard_v2` (v1 p 0.2471 / 0.0020 bit-identical; v2 on §48b p 0.0015), `test_pool54`, `test_snap53`, `test_harness_paths` and `test_bbr48_parse` (with the plan list extended), and every other `test_*.py`;
   - the bash tests `test_lib_battery.sh` and `test_abort_witness.sh` run on the VM. They are known Windows failures.

**Done:** all of item 9 green on the host (bash tests excepted) and on a `git archive` extract.

**Report:** the shas, the stub-run summary, the committed `ref64-manifest.sha256` with its `cmp`, and every place where the work departed from this section.
