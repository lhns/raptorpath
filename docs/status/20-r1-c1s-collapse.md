## 20. R1 c1s collapse — diagnosis — pre-registration

§17 left one unmeasured fact: R1 (`archive/thread-r1`, a5ef8b2) reads 185–192
Mbit/s at `c1s-400` in all six rows against MAIN's 344–658, with the server
receiver `[RDIAG]` 100 % busy at 13–21 k msg/s (MAIN 81 k at 77 %), while
`c1d-400`, `c2-100` (also single-path) and `c8-100` do not collapse. This
section finds the cause. It is a **diagnosis, not a ship battery**: nothing
here ships or is tuned; the proposed fix is a diagnostic arm only. Committed
before any VM contact; no number below is a result.

**Ranked hypotheses** (from reading a5ef8b2 against ce60079 and §17's ledgers).

- **H1 — the `[LAT]` pending store (fix (d), `net/lat.rs` `Pending` on
  `SeqRing`) costs O(gap) per message when one dead record sits at its
  front.** `SeqRing::remove` trims `None` slots off *both* ends; `insert` at
  `seq ≥ end` pads `None` from `end` up to `seq`. A record that is never
  taken — a duplicate source copy of an already-delivered seq (`dup_src`),
  which `note_arrival` records and no in-order delivery will ever take — sits
  at the ring's front until the FIFO evicts it, i.e. for `PENDING_MAX` =
  16 384 further insertions. Meanwhile every arrival that is **delivered at
  once** removes the ring's last live record, the back-trim walks down to the
  dead front record, and the next arrival pads all the way back up: 2 × (head −
  dead seq) slot operations per message, the gap growing from ~10³ to ~1.7·10⁴
  over the record's lifetime (bounded at 4 × 16 384 by the span guard).
  Order of magnitude: mean gap ~8·10³ × 2 ops × 1–3 ns ≈ 15–50 µs per
  message, against an observed ≈ 50 µs (R1) vs ≈ 10 µs (MAIN). The
  `HashMap` it replaced is O(1) regardless of a dead record.
  *Why c1s and not the others* (the discriminator is not N = 1 but how often
  the live set empties down to the dead record — the share of deliveries
  with no reorder wait, `[LAT] nowait / p0:n`, §17 ledgers s42 r1): c1s R1
  303 846 / 338 410 (≈ 90 %), c2 R1 13 834 / 84 522 (≈ 16 %: a waiting record
  stays at the ring's back, so the back-trim stops there), c1d R1 86 990 /
  176 136 on p0 plus cross-path waits. *Supply of dead records:* c1s R1
  `dup_src` = 79, `over` = 55–60 per run; ~60 × 16 384 ≈ 10⁶ insertions of
  front-pinning > 338 k arrivals, so the front is pinned nearly the whole
  run once the first duplicate lands. *Predicted coincidence, not chance:*
  the first `[RDIAG]` window of the c1s R1 s42 r1 server reads 63 % busy at
  44.7 k msg/s with `over=0`; the next reads 100 % at 20 k with `over=3`.
- **H2 — `SeqBits` range reads (fix (c)) are O(words) on a long span**
  (`count_in`, `each_run`/`next_set`/`next_clear`). Ranked low: every
  per-message read covers `[next_expected, highest_seen]`, ≤ the N = 1 store
  cap 1 024 seqs (16 words); the `count_in(ooo_frontier, u64::MAX)` scan
  runs only with `RWM_REASM_BDP` and the OOO arm, once per 2 s; the prune
  (every 2 s, `delivered − 2·200`) keeps the bitmap ≤ ~2 s of seqs.
- **H3 — the inbound pool (fix (a))**: a lock and a `paths` lookup per
  owner batch, not per message; identical at c1d. Low.
- **H4 — (b) / (e)** remove work only. Negligible.

**Arms** (file swaps, so a result names one file; `seq_ring.rs` is
byte-identical at ce60079 and a5ef8b2, R1's `lat.rs` changes only `Pending`'s
internals, no `Cargo.toml` change):

| arm | tree | H1 predicts at c1s-400 |
|---|---|---|
| `MAIN` | ce60079 | recovered |
| `R1` | a5ef8b2 | collapsed |
| `R1-d` | a5ef8b2 with `net/lat.rs` from ce60079 | recovered |
| `MAIN+d` | ce60079 with `net/lat.rs` from a5ef8b2 | collapsed |
| `FIX` | a5ef8b2 + `fix-lat-floor.patch` (below) | recovered |

`FIX` (the proposed fix, diagnostic only; the patch is committed in the
ledger directory with this section): `Pending` keeps a floor one past the
last in-order delivery; `note_delivery(d)` prunes every record below it
(dead: deliveries are in order) and `note_arrival` refuses a seq below it,
both counted in `over=` exactly as a FIFO eviction is (the same records end
up counted; only *when* changes). `[LAT]` is a read-only gauge that feeds no
law, so the NO-MODE-SWITCH invariant is not touched; there is no threshold,
hint or mode here. In OOO mode `note_delivery` is never called and nothing
changes. `FIX` is built and run only if the build budget allows (it is the
last build); its absence does not void the diagnosis.

**Mechanism arm (no VM, before the battery).** A single-file extraction of
a5ef8b2's `SeqRing` (verbatim) and the three `Pending` versions (MAIN's
`HashMap` + FIFO, R1's ring, FIX), `rustc -O`, ns per arrive-and-deliver on
an in-order stream of 300 k seqs with one duplicate of a seq delivered
`back` seqs ago every 5 600 arrivals (c1s's supply), `back` ∈ {1 000,
8 000}, plus a no-stray and a sparse-stray control. **Predicted:** with
strays every 5 600, R1 ≥ 10 × MAIN ns per arrival; without strays, R1 ≤ 2 ×
MAIN; FIX ≤ 2 × MAIN in every case. The source and its output go to the
ledger.

**Session.** Each tree is shipped with `git -c core.autocrlf=false -c
core.eol=lf archive` (the swaps and the patch applied after extraction,
`sha256` of the swapped file logged), built on the VM with `cargo build
--release --bin raptorpath` in its own fresh target dir, copied as
`bin/<arm>/raptorpath`, `sha256` in `BINSHA.txt`. No test suites (the
trees are §17's, tested there; the swaps are one file each). Envelope
`tools/l1/r1diag_run.sh`, both locks for the whole session, detached.

- *Diagnostic rows:* `c1s-400`, bulk, `--window-reliable`, the shipped
  defaults, `perf_rwm_c.sh` with `RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1
  RWM_RTOBS=2 RWM_PERF_TIMEOUT_S=150`, **n = 2 per arm** (seed 42 then seed
  7), arm order rotated per rep. `RWM_RTOBS=2` gives `[RXSEC]` on the
  a5ef8b2-based arms (`R1`, `R1-d`, `FIX`); ce60079 has no `[RXSEC]`.
- *Profiles:* `perf record -g --call-graph dwarf,16384 -F 999` of the server
  process for 3 s in the steady state of a 1 GB `c1s` transfer
  (`RWM_RTOBS=1`), on `R1` and `MAIN` (then `R1-d` if the budget allows), as
  §16's `q3diag_run2.sh`; `perf report` self and children text kept, the
  `perf.data` deleted.

**Classification per row** (fixed here): **collapsed** iff goodput ≤ 260
Mbit/s; **recovered** iff goodput ≥ 330 Mbit/s (MAIN's §17 worst is 344;
R1's best 192); between is `AMBIGUOUS`. A row needs a client summary and
`[PIPE] pipeline=window backend=Rlc hint=bulk` on the server; the tree
witness is two-sided: `ibp_hit=` on the server's `[IOWN]` for the
a5ef8b2-based arms, absent for the ce60079-based ones.

**Rule-1 witness (the mechanism executes, measured in-process).** On the
a5ef8b2-based arms, `[RXSEC]` of the server receiver: `book` (whose
brackets hold `note_arrival` and `note_delivery`, and the `[ETA]` /
`[SUCC]` / `[LATE]` feeds) as a share of the handler total and in µs per
message. **Predicted:** `R1` book ≥ 50 % of handler time and ≥ 10 µs per
message; `R1-d` and `FIX` book ≤ 25 % (the §17 c1d dev read was 15.8 %,
2.6 µs). The profile is corroborating, not gating: predicted on `R1` a hot
`[LAT]` path (`LatGauge::note_arrival` / `note_delivery`, `SeqRing` /
`VecDeque` push / pop, possibly inlined into `run_receiver`) of ≥ 20 % of
server samples, and < 2 % on `MAIN`.

**Outcomes** (one verdict, precedence order):
1. `UNREAD` — any of `MAIN`, `R1`, `R1-d`, `MAIN+d` lacks 2 classified rows.
2. `H1-CONFIRMED` — `MAIN` and `R1-d` recovered in every row, `R1` and
   `MAIN+d` collapsed in every row, and the `[RXSEC]` book prediction holds
   on `R1` and `R1-d`. (+ `FIX-VALIDATED` if `FIX` is recovered in both rows
   with book ≤ 25 %.)
3. `H1-NECESSARY-NOT-SUFFICIENT` — `R1-d` recovered but `MAIN+d` not
   collapsed: the ring needs another R1 change to bite (an interaction; next
   arm `MAIN+d+c`, a new amendment).
4. `H1-REFUTED` — `R1-d` collapsed: the cause is elsewhere; H2 is next (a
   `SeqBits`-revert arm, a new amendment).
5. `MIXED` — anything else, reported row by row.

**Red test (proposed with the fix, not shipped here).** In `net/lat.rs`
tests: one dead record (a duplicate of a delivered seq) at the front, then
N = 50 000 in-order arrive-and-deliver pairs; assert an **absolute** bound
on the ring's slot operations (a cheap pad/trim counter on `SeqRing`) of ≤
4 N, and the structural invariant *after `note_delivery(d)` no record below
`d` is held*. Red on a5ef8b2 (≈ 2 × gap per pair), green with the fix.

**Budget and rules.** Hard backstop = launch + 3 h (5 builds at the §17
loaded prior of up to ~35 min each could approach it; the builds run
`MAIN`, `R1`, `R1-d`, `MAIN+d`, `FIX` in that order and the envelope skips
`FIX`, then the `R1-d` profile, rather than cross the soft deadline);
launch precondition as §17 Amendment 2 (both locks free, loadavg < 2.0, no
`cargo` / `rustc`). `pkill -x raptorpath` only; no `ens18`, firewall,
`sshd` or non-`rp-*` namespace touched; exit state 0 `raptorpath`, 0 `rp-*`,
locks released, VM dirs removed. Ledgers to `docs/l1-raw/thread-r1-diag/`.

### 20. R1 c1s collapse — diagnosis — result

**Outcome: `H1-CONFIRMED` + `FIX-VALIDATED`.** The collapse is R1's fix (d):
the `[LAT]` pending store on `SeqRing` (`net/lat.rs` `Pending`). Session
2026-10-26 10:33–11:08Z, one envelope (`tools/l1/r1diag_run.sh` at 6329b1d),
both locks held throughout, launch load 1.76, 0 cargo / rustc at every row;
exit state 0 `raptorpath`, 0 `rp-*`, locks released, the VM directory
removed. All five arms built (340–508 s each), all 10 rows and all 3 profiles
ran. Ledgers: `docs/l1-raw/thread-r1-diag/` (`era.txt`, `BINSHA.txt`,
`summary.txt` from `tools/l1/r1diag_summary.sh`, `diag/` per-row logs,
`P-c1s-*.self.txt` / `.children.txt`, `mechanism/`, `fix-lat-floor.patch`).
MAIN's and R1's `sha256` equal §17's binaries.

| arm | goodput s42 / s7 (Mbit/s) | class | server `[RXSEC]` per msg: total / book (share) | `[RDIAG]` busy med |
|---|---|---|---|---|
| `MAIN` | 578 / 602 | recovered ×2 | (no instrument) | 88 / 79 |
| `R1` | 177 / 178 | collapsed ×2 | 51.7 / **36.2 µs (70 %)**; 51.7 / 35.1 µs (68 %) | 100 / 100 |
| `R1-d` | 559 / 527 | recovered ×2 | 14.7 / 1.26 µs (8.6 %); 14.3 / 1.27 µs (8.9 %) | 96 / 79 |
| `MAIN+d` | 164 / 179 | collapsed ×2 | (no instrument) | 89 / 100 |
| `FIX` | 483 / 599 | recovered ×2 | 14.5 / 1.03 µs (7.1 %); 13.1 / 1.00 µs (7.6 %) | 81 / 86 |

Every row carries `[PIPE] window/Rlc/bulk`; the tree witness is two-sided as
registered (`ibp_hit=` on the a5ef8b2-based arms only). `[LAT] nowait / p0:n`
≈ 0.89–0.92 on every arm (the same stream; only the store differs).

**The evidence, by layer.**
1. *Bisection.* Swapping one file decides it both ways: R1 with MAIN's
   `lat.rs` recovers (527–559), MAIN with R1's `lat.rs` collapses (164–179).
   The other four R1 changes (pool, in-place batch, `SeqBits`, lazy
   `received_ids`) are neither necessary nor sufficient; H2/H3 are cleared.
2. *Rule 1, in-process.* The receiver's `book` section (the brackets around
   `note_arrival` and `note_delivery`) costs 35–36 µs per message on R1 and
   1.0–1.3 µs on R1-d and FIX: 28–36× — the whole of R1's 51.7 vs 14.5 µs
   per message (the other sections together: 15.5 vs 13.4 µs).
3. *Profile (corroborating).* Server self time in
   `run_receiver::{{closure}}` is 56.0 % on R1 against 6.5 % (MAIN) and 5.9 %
   (R1-d); the store is inlined there, so the profile cannot name it by
   symbol — the `[RXSEC]` split and the bisection are what attribute it.
4. *Mechanism arm* (`mechanism/`, a5ef8b2's `SeqRing` verbatim, `rustc -O`
   on the Windows host). In-order stream delivered at once, one duplicate
   of an old seq every 5 600 arrivals: R1 17.5 µs per arrival (gap 1 000)
   and 53 µs (gap 8 000) against MAIN's `HashMap` 0.08 µs and FIX 0.03–0.04
   µs; with no duplicates R1 is the fastest (0.021 µs vs 0.074). The ring
   slot span right after an in-order insert reaches 16 381 on R1 and stays
   1 on FIX.

**The cause (file and line).** a5ef8b2 `raptorpath/src/net/lat.rs`,
`Pending::insert` / `Pending::take` over `net/seq_ring.rs`
`SeqRing::insert` (the `for _ in end..seq { push_back(None) }` pad) and
`SeqRing::remove` → `trim()` (the `while let Some(None) = slots.back()`
pop). A duplicate source copy of a delivered seq (`dup_src`, ~80 per c1s
run) is recorded by `note_arrival` and never taken, so it sits at the
ring's front for its FIFO lifetime (16 384 insertions). On a stream whose
arrivals are delivered at once (c1s: ~90 %), each delivery takes the ring's
only live record, the back-trim walks down to the dead record and the next
arrival pads back up — 2 × (head − dead seq) slot operations per message,
growing to ~1.7 × 10⁴. The `HashMap` it replaced is O(1) whatever is dead.
*Why c1s and not c2/c1d:* the cost needs both a dead front record and a
live set that empties on most arrivals. c2's deliveries mostly wait in the
reorder buffer (nowait ≈ 16 %), so a waiting record holds the ring's back
and the trim stops there; c1d has cross-path waits (nowait ≈ 49 % on p0).
That explanation is derived from §17's `[LAT]` counters; this session
measured c1s only.

**Proposed fix (validated here as the `FIX` arm; not shipped).**
`fix-lat-floor.patch`: `Pending` keeps `floor` = one past the last
in-order delivery; `note_delivery(d)` takes `d`'s record and prunes every
record below the floor (`SeqRing::prune_below`), and `note_arrival`
refuses a seq below it, both counted in `over=` as a FIFO eviction is. The
ring then spans only the live reorder window. No law reads `[LAT]`; there
is no threshold, hint or mode (NO-MODE-SWITCH untouched); in the OOO arm
`note_delivery` is never called, so nothing changes there. `over=` counts
the same records, sooner (the microbench: 52–53 vs MAIN's 50–51, the
difference being dead records MAIN still held at the end).

**Red test that would have caught it** (an absolute O(1) bound, not an
ordinal one): in `net/lat.rs` tests, an in-order stream of N = 50 000
arrive-then-deliver pairs with a duplicate of a seq delivered 1 000 earlier
every 5 600 arrivals; after each in-order insert assert the ring's slot span
`last_key − first + 1 ≤ 2`, and after each `note_delivery(d)` that no record
below `d` is held. On the extraction it reads 16 381 on a5ef8b2 (red) and 1
with the fix (green). §17's differential test missed it because it compared
records held and lost (identical: the defect is cost, not content) and its
random script had out-of-order deliveries that kept the ring populated.

**Not done here.** Shipping R1 with the fix is a new pre-registration (the
§17 battery on R1+FIX); the fix and the red test are in this ledger only.
