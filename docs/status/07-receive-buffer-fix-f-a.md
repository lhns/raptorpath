## 7. Receive-buffer fix (F-A) — pre-registration

The fix for V4 finding 1 (the c1 receiver's kernel UDP receive-buffer
overflow). Every endpoint socket now requests `SO_RCVBUF` = 4 000 000 B
(`B_req = R_line × T_pause × k_truesize / 2`, derived in fec-arq-model.md
§8.5, "The kernel receive buffer"). Each socket echoes `[RCVBUF] … granted=
clamped=` once at bind, and the receiver's `[CTLD]` line carries the
per-socket kernel drop counter `rxdrop<i>=`. Committed before the
verification run. No number below is a result.

**Probe already run (zero-build, recorded, not scored).** c1s-400, V4 NEW
binary (`aa88d648…`), seed 42, n = 8 per arm, interleaved, both VM locks
held.
- Arm B as specified (`ip netns exec rp-srv sysctl -w
  net.core.rmem_default=…`) **cannot run**. The write is refused with EPERM
  in a child netns: `net.core.rmem_*` is visible there but read-only, and
  only init_net can set it. So AN1's "per-netns" inference is refuted, and
  the harness-sysctl option is unavailable even as a probe. That session is
  therefore an A/A: 15 live rows, `rcvbuf_drops` 17–80 (medians 39 and 36),
  `plc`/truth medians 2.12 and 1.91, goodput medians 473.0 and 475.0 Mbit/s.
  One row is void: its driver script was edited while the run was in
  flight (rc 127).
- The replacement arm B is an `LD_PRELOAD` shim that requests
  `SO_RCVBUF` = 4 194 304 B on the server socket after `bind`. It is the
  engine fix's mechanism, applied from outside. Its witness on every row is
  the shim's echo `granted=8388608` and `ss -uamn` `rb8388608`. Results:
  A `rcvbuf_drops` 22–111 (median 64), `plc`/truth median 2.83, goodput
  median 449.3 Mbit/s; **B `rcvbuf_drops` 0 in 8/8**, `plc`/truth 0.97–1.07
  (median 1.01), goodput median 481.0 Mbit/s.
- Drop-count unit: the per-socket `ss` skmem `d` (= `sk_drops`, the field
  `SO_MEMINFO[SK_MEMINFO_DROPS]` returns) equalled the netns `RcvbufErrors`
  delta on every arm-A row (for example 69/69 and 57/57). It counts skbs,
  that is GRO superpackets (`[TRUTH] gso` median 9.45 at c1s).

**Binaries.** MAIN is the V4 NEW binary, `sha256 aa88d648def0b125…`. It is
reused because `git diff 7d8cec2b ddfc07a` over `raptorpath/src`,
`raptorpath-math`, `gf256`, `Cargo.lock` and `raptorpath/Cargo.toml` is
empty, so it is `main`'s engine. FIX is this branch's HEAD, archived with
`git archive`, built fresh on the VM (`cargo build --release --bin
raptorpath`), `sha256` recorded and re-checked before every invocation.
**Tests first** (FIX tree, on the VM, under the locks): `cargo test -p
raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2`,
`cargo test --doc -p raptorpath --release`, and `cargo test -p
raptorpath-wasm`. A real failure (one that does not pass on an isolated
re-run) is `ABORT-TESTS`. The red-first evidence is recorded beside them:
`every_endpoint_socket_reads_back_the_rcvbuf_floor` on the parent commit
(red) and on HEAD (green).

**Harness.** `perf_rwm_c.sh` from this commit's `tools/l1` (unmodified),
with `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150 RWM_C_PIPELINE=window
SEED=<seed>`, one run, `--window-reliable`, a fresh topology per invocation,
and every arm `env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
RWM_EMIT_BURST`. Rows are parsed by `verify4_parse.py row` under the arm
label `NEW`, because both binaries carry V4 NEW's witnesses. A 2 Hz `ss
-uamn` sampler in `rp-srv` records `rb`, `r` and `d` for every receiver
socket.

**Cells and plan.** `c1s-400` and `c1d-400`: MAIN against FIX, n = 8 per
seed × seeds 42 and 7 (16 per arm and cell). `c2-100` is the must-not-move
control, n = 4 per seed × seeds 42 and 7 (8 per arm). Blocks run rep 1 s42,
rep 1 s7, rep 2 s42, and so on. Within each block the cells run in the
order above, and the arm order alternates by rep (rule 3). 80 invocations,
about 25 min.

**Witnesses per row** (a failing row is `WITNESS-FAIL`, excluded and
counted):
- The verify4 NEW witness set: `[PIPE]` window/Rlc/bulk on both ends,
  `[GATES]`, cadence ACTIVE, and no emission batching.
- FIX: one `[RCVBUF]` echo per path on both endpoint logs, with `req=4000000`
  and `granted ≥ 2·min(req, rmem_max)` (8 000 000 on this VM); the server's
  `ss rb` reads 8000000; and `rxdrop<i>=` is present on the server's
  `[CTLD]`.
- MAIN: no `[RCVBUF]` line, and `ss rb` reads 212992.

**Outcomes per c1 cell** (X = FIX, REF = MAIN):
- **`FIXED`** iff all three hold:
  1. `rcvbuf_drops ≤ 2` in at least 15 of 16 FIX rows.
  2. The FIX median `plc`/truth is within [0.9, 1.3] on every leg.
  3. FIX goodput median ≥ MAIN median × (1 − rel_gp), with §5's rel_gp:
     `c1s-400` 4.9 %, `c1d-400` 5.6 %.
- **`NOT-FIXED-<clauses>`** names the clauses that failed. If (1) fails,
  the drain pause exceeds the 32 ms tolerance, and the ranked next lever is
  isolating the endpoint driver, not a larger buffer.
- **`UNSCOREABLE`** with fewer than 12 live rows in either arm, or ≥ 2
  witness-failed rows in either arm.

**Goodput** is also printed as `BETTER` / `SAME` / `WORSE` against the MAIN
median ± rel_gp. This is descriptive: the prediction is SAME-or-BETTER (the
probe gave +7 %, n = 8).

**Control `c2-100`:** `CONTROL-HELD` iff all of these hold:
- FIX goodput median is within MAIN median × (1 ± 0.014).
- `rcvbuf_drops` = 0 in every row of both arms.
- FIX median `plc`/truth is within [1/1.3, 1.3] × MAIN's.

Otherwise `CONTROL-MOVED` (named beside the c1 verdicts). Fewer than 6 live
rows per arm is `UNSCOREABLE`.

**Unit check (reported, not scored).** On FIX rows, the server's last
`[CTLD] rxdrop` sum is compared with the run's `rcvbuf_drops` and with the
final `ss d` value. If FIX drops are 0, this confirms the wiring only. The
unit evidence is the probe's non-zero A rows above.

**Abort causes, in priority order:**

| # | cause | token |
|---|---|---|
| 1 | either VM lock not free within the 25 min wait | `ABORT-LOCK` (nothing run) |
| 2 | CR bytes in the synced `tools/l1` scripts | `ABORT-CRLF` |
| 3 | build failure | `ABORT-BUILD` |
| 4 | a real test failure | `ABORT-TESTS` |
| 5 | binary `sha256` mismatch at any invocation | `ABORT-SHA` |
| 6 | smoke (one FIX c1s run) missing a FIX witness | `ABORT-SMOKE` |
| 7 | `raptorpath` already running at start | `BUSY` |

**Budget.** 5 h cap from the first ssh of the session. Build and tests are
about 45 min, the battery about 25 min. No truncation is expected; if the
cap binds, the battery stops at a rep boundary and is scored at the n it
reached.

**Amendment 1** (committed before any scored row exists). Session 1
(2026-10-04 23:44 UTC) is void and was not scored. It ended
`ABORT-SMOKE` with no battery row. The envelope wrote its driver output to
`/tmp/fa-drv.out`, which the probe had left behind owned by root, so the
smoke invocation never started (rc 1, 0 s). The smoke check then read the
probe's stale output and failed as it should. That session's test stage is
also not evidence for the green tree. It shared one cargo target between
the red tree (0597da4) and the FIX tree, and `git archive` gives every file
its commit time as mtime, so cargo reused the red tree's lib-test artifact
for the FIX tree. The FIX-tree run of
`every_endpoint_socket_reads_back_the_rcvbuf_floor` therefore failed with
the red value (212992). Its red-tree run does stand as the red evidence: it
was the first lib-test build in that target, from 0597da4's sources, and
failed with SO_RCVBUF 212992 < floor 8000000. Session 3 re-runs build,
tests, smoke and battery as pre-registered, with three changes: the FIX
tree's mtimes are refreshed, it uses its own target, and its temporary
files sit in the run directory. The verification FIX binary is session 3's
build.

### 7.1 Receive-buffer fix — result (scored against §7 and amendment 1)

**Session.** Session 3 ran 2026-10-05, 02:28–03:23 UTC, with both locks
held for the whole session. It was launched three times: the first two
attempts ended `ABORT-LOCK` while another battery held the locks, and
nothing ran in them. CPU: Xeon E5-2650 v3. FIX is commit 10b9d78, `sha256
85ae2568…`, the same hash as session 1's build. MAIN is `aa88d648…`. Raw
data is in `/home/vibe/fa/run3/` on the benchmark VM (`fa.log`, `diag/`,
`TESTS.txt`, `BINSHA.txt`).

**Abort table:** no abort cause fired. 81 rows: 1 smoke row and 80 battery
rows. Every one of the 80 battery rows is `LIVE` with all witnesses
passing, and none is excluded.

**Tests (FIX tree):**

| suite | passed | failed | ignored |
|---|---|---|---|
| `cargo test -p raptorpath -p raptorpath-math --release` | 1042 | 0 | 50 |
| `--doc` | rc 0 | | |
| `raptorpath-wasm` | 35 | 0 | |
| `rcvbuf` lib tests | 5 | 0 | |
| `rcvbuf_reachability` | 1 | 0 | |

Red-first: `every_endpoint_socket_reads_back_the_rcvbuf_floor` failed on
0597da4 (SO_RCVBUF 212992 < floor 8000000) and passes on 10b9d78.

**Witnesses.** Every FIX invocation echoed `[RCVBUF] … req=4000000
granted=8000000 via=SO_RCVBUF clamped=0` for every path, on both ends: 57
server and 57 client sockets. Its server `ss rb` read 8000000 on every row,
and its server `[CTLD]` carried `rxdrop<i>=` on every path. No MAIN row
echoed `[RCVBUF]`, and every MAIN `ss rb` read 212992.

| cell | arm | n | `rcvbuf_drops` (skbs) | `plc`/truth median per leg | goodput median (Mbit/s) [range]; per seed 42 / 7 |
|---|---|---|---|---|---|
| `c1s-400` | MAIN | 16 | 29–163, median 62 | 2.69 | 444.7 [411.7, 494.8]; 444.7 / 444.0 |
| `c1s-400` | FIX | 16 | **0 in 16/16** | **1.03** | 452.4 [377.2, 482.5]; 452.4 / 458.9 |
| `c1d-400` | MAIN | 16 | 4–104, median 27.5 | 1.34 / 1.63 | 284.2 [257.0, 359.5]; 284.0 / 284.6 |
| `c1d-400` | FIX | 16 | **0 in 16/16** | **1.02 / 1.04** | 283.7 [252.8, 327.5]; 295.1 / 278.7 |
| `c2-100` | MAIN | 8 | 0 | 0.99 | 88.9 [87.9, 89.2] |
| `c2-100` | FIX | 8 | 0 | 1.00 | 89.0 [88.5, 89.3] |

**Verdicts:**
- `c1s-400`: **`FIXED`**. Clause (1) holds in 16/16 rows, clause (2) at
  1.03, clause (3) at +1.7 %. Goodput reads `SAME` (descriptive).
- `c1d-400`: **`FIXED`**. Clause (1) holds in 16/16 rows, clause (2) at
  1.02 and 1.04, clause (3) at −0.2 %. Goodput reads `SAME`.
- `c2-100`: **`CONTROL-HELD`**. Goodput +0.03 %, 0 drops in both arms,
  feed 1.00 against 0.99.

**Unit check.** On all 40 FIX rows, the server's `rxdrop` sum, the netns
`RcvbufErrors` delta and the `ss d` value are all 0. This confirms the
wiring only. The unit evidence is the probe's: `ss d` (= `sk_drops`, the
field `SO_MEMINFO` reads) equalled `RcvbufErrors` on every non-zero row.
So the unit is skbs (GRO superpackets, about 9.4 datagrams each at c1s),
not datagrams. The token stays an instrument and is not fed to the
estimator.

**What it means.** The kernel receive buffer was the whole c1 loss-feed
excess. With the socket asking for 4 MB, the receiver drops nothing at
either c1 cell, and the engine's fed loss matches the wire's (`plc`/truth
1.02–1.04, down from 1.3–2.7). Goodput did not move beyond the MDE in
either direction: the probe's +7 % at c1s was not reproduced at n = 16. The
100 Mbit/s control is unchanged. V4 finding 1 is closed for hosts whose
`rmem_max` is at least 4 MB. On older hosts (`rmem_max` 212 992) the echo
reads `clamped=1` unless the engine runs as root, where `SO_RCVBUFFORCE`
applies.
