# dup-diag: false packet-threshold fires from reordered gap snapshots

Status: DIAGNOSIS + PROPOSED LAW. No engine fix ships with this file
(FORMULA-FIRST). Feeds: status §52 (the c8 duplicate verdict), ADR-0059
(per-path hole law), paper §7.1.
Evidence: `docs/l1-raw/s52/rep52-diag-ld.tgz`, 6 BASE + 6 CAND c8 rows.
Red test: `net::tests::mp_packet_threshold_declares_only_lost_seqs_under_report_reordering`
(`raptorpath/src/net/tests.rs`). It was committed failing (12eafc33) and is
now `#[ignore = "known RED …"]` until the law ships. It runs with
`--include-ignored`.

- **Scenario.** Two FIFO legs and one real loss that holds the frontier.
  The test drives the shipped fold and decision: `mp_ingest_evidence` and
  `mp_packet_threshold_fires`, extracted verbatim from `serve_gaps`, which
  calls them.
- **On main.** With in-order return, exactly the lost seq is declared. With
  the p1 return leg 9 ms slower, the channel declares **182** seqs lost for
  1 real loss.
- **The law, test-locally.** `mp_snapshot_version_filter_restores_packet_threshold_soundness`
  checks the §2 law on the same scenario and passes. It changes no engine
  code.

## 1. The defect

The sender applies a receiver snapshot (one WindowAck's frontier plus its
SACK ranges, inverted to gaps) in the order snapshots ARRIVE. The receiver
generates them in one order and sends each on the leg the triggering data
arrived on (`net/receiver.rs` ~2008, `send_control_datagram(.., path_id, ..)`).
The timer re-advertisement goes out on every live leg (~950). The two return
legs differ by about 15 ms (c8 server-side netem: p0 5 ms ± 3, p1 20 ms ± 5).
So a p1-carried snapshot routinely reaches the sender after p0-carried
snapshots that the receiver generated up to ~15 ms later.

Nothing on the gap path orders them:

- `net/control_msg.rs:708–745` inverts the SACK ranges and forwards the gaps
  with no version check. The only ordering guard in the handler is the
  cumulative point's `fetch_max` (`:490`, commented "acks arrive on multiple
  paths, out of order").
- `serve_gaps` coalescing (`net/sender_phases.rs:646–667`) keeps the
  last-DEQUEUED snapshot as "the newest". That is the last-arrived one, which
  is the stale one in exactly this case.
- The packet-threshold evidence is a monotone accumulator over ALL processed
  snapshots (`mp_ingest_evidence`, `net/recovery_laws.rs`, called from
  `sender_phases.rs:774`). The gap seq it judges comes from the CURRENT
  snapshot (`sender_phases.rs:916`).
- The stale skip does not catch it: under `RWM_STORE_SACK_RELEASE` a SACKed
  seq stays in `sent_store` until the cumulative point passes it
  (`net/sack.rs` §`sack_release_mark`), so `sent_store.get` succeeds and the
  retransmit is emitted.

The mechanism: a p0 seq `s` is a short-lived cross-path hole (p0 holes heal
in `hn_p50` ≈ 6 ms in CAND). A p1-carried snapshot generated while `s` was
missing reaches the sender after fresher p0 snapshots showed `s` delivered,
along with many later p0 seqs. Those snapshots already ingested the later p0
seqs as evidence, so `mp_fast_lost(ev_p0, s)` is true. The channel fires on a
seq the sender has already been told was delivered. The retransmit mostly
goes on p1 (`fp/on`: 72 % of fires go out on p1), and the receiver counts it
as a p1 duplicate.

What the evidence channel itself gets right (checked, not the cause):

- Seq order on a leg equals arrival order on that leg. That is MEASURED at
  the receiver, not inferred from reading the send path: `[SUCC]` `sp_n`
  counts holes exposed and closed by arrivals on the SAME leg
  (`net/succ.rs:362`).
  - It is 5.3 k–9.0 k per run in all 12 rows, which is loss scale.
  - The p50 is 0.43–0.72 s (CAND) and 4.7–5.2 s (BASE): retransmit
    closures.
  - There is no ms-scale same-leg population, as an in-leg displacement on
    the sender would produce.

  By reading: there is one emission site (`emit_source.rs:430`), seqs are
  assigned `0,1,2,…`, and `dgq full=0 err=0`. `TxBatch` staging, the
  io-owner drain and `emit_burst.rs` were not read. The `sp` reading is what
  rules them out.
- `source_path_map` is never rewritten. There is one insert per block id,
  and `SeqRing` is exact. Keying by original path, the brief's leading
  hypothesis, cannot produce these numbers. A p0-original `q > s` delivered
  by a retransmit must be reported missing, then retransmitted on p1, which
  takes ≥ 20 ms on the wire plus p1's local dwell (`thr_p50` ≈ 393 ms). All
  of that would have to happen before `s`, which heals in ~6 ms. A coded
  fill of a later p0 seq is the one same-speed way to make false evidence.
  It is bounded by `fill_coded` ≈ 1.5 k per run. Its overlap with this
  window is not measured.
- The delivered-interval derivation is sound. The receiver's SACK is an
  honest prefix (`received_sack_ranges`). The one seq past the last gap is a
  SACKed seq. Truncation at `MAX_NACK_GAPS` only shortens the list.

### Evidence in the c8 logs

1. **Re-stamping proves out-of-order application.** `[HOLD]` stamps a hole
   on first listing, removes it when a snapshot covering it stops listing it
   (`holddown.rs::on_report`), and stamps it again if it is listed again. The
   receiver's received set above the frontier only grows (`SeqBits` prunes a
   prefix only, `receiver.rs:674`). So re-listing after resolution can only
   come from an OLDER snapshot processed later.
   - CAND s7-r3: p0 `fed` = 445 858 > 435 285 p0 placements (`[C8CONV-S] sp`).
     There are more resolutions than seqs, so at least 10.6 k re-stamps in
     that row.
   - Across CAND rows p0 `fed` runs 195 k–446 k. BASE runs 16.6 k–19.2 k.
     Part of the CAND rise is first-time listings of young holes that the
     64-gap horizon now exposes. `fed` alone does not split re-stamps from
     first stamps; only `fed − sp` > 0 bounds the re-stamps from below.
2. **The heal time is report cadence, not flight time.** `hy_p50` on p0 is
   5.1–5.6 ms in all six CAND rows; p1's is 2.8 ms. That is the next fresh
   snapshot after the stale one (gap reports at ≥ 2 ms, `GAP_ACK_MIN_INTERVAL`).
   No flight on either leg (dwell-inclusive clocks of 300+ ms) heals that
   fast. The brief's reading, "the original arrived 5 ms after the hole was
   declared lost", inverts the order: the original had ALREADY been reported
   delivered when the channel fired.
3. **Scaling.** Across the six CAND rows, p0 `hy` rises with p0 `fed`
   (re-stamps) and with p0 `xp_n`:

   | row | fed | xp_n | hy | y | p1 dup |
   |---|---|---|---|---|---|
   | s42-r3 | 195 377 | 16 758 | 1 042 | 3 730 | 1 420 |
   | s42-r1 | 257 845 | 20 358 | 1 689 | 4 968 | 1 754 |
   | s7-r1 | 260 032 | 20 720 | 2 148 | 5 446 | 1 934 |
   | s7-r2 | 271 746 | 21 007 | 2 179 | 5 719 | 1 997 |
   | s42-r2 | 404 120 | 31 485 | 3 237 | 6 253 | 3 334 |
   | s7-r3 | 445 858 | 31 229 | 3 339 | 6 554 | 3 537 |

### Counter arithmetic (means over 6 rows, CAND − BASE)

| counter | BASE | CAND | Δ |
|---|---|---|---|
| fired | 15 082 | 17 940 | +2 858 |
| y (packet threshold) | 330 | 5 445 | +5 115 |
| r (time threshold) | 14 752 | 12 495 | −2 257 |
| hy on p0 | 160 | 2 272 | +2 113 |
| hy on p1 | 287 | 696 | +409 |
| dup total | 1 765 | 3 729 | +1 964 |
| dup p1 | 684 | 2 329 | +1 646 |
| dup p0 | 1 081 | 1 400 | +319 |

Two parts make up Δy = +5 115:

- **−Δr = 2 257 is a migration.** Real losses are now caught by the packet
  threshold at first report instead of by the time threshold. The lever B
  horizon (64 gaps) puts young p0 holes in the list, which is the lever's
  intended effect.
- **Δfired = +2 858 are additional fires.** Of those, 2 522 classify as
  `hy` (healed within srtt/2 of the copy): 2 113 on p0 and 409 on p1. The p1
  share has the same signature, `hy_p50` ≈ 2.8 ms. Δdup = +1 964 ≈ 78 % of
  Δhy. Copies lost on the wire (≈ 3–4 % leg loss) explain a few points of
  the remaining 22 %; the rest is unexplained.
- **Split by leg.** The extra duplicates land 84 % on p1 (+1 646) and 16 % on
  p0 (+319). Over ALL fires the emission split is 72 / 28 (p1 / p0, `fp/on`).
  The placement of the spurious subset is not logged, so the difference
  between the two splits is not attributed.
- **The BASE dups (~1.77 k) are a separate population.** They are present
  with `y` ≈ 0 (s7-r3: y = 0, dup 1 583), so this mechanism does not explain
  them. Candidates are `fill_coded` (a coded repair closes the hole and the
  copy lands; `[RFA] fill_coded` ≈ 1.8 k per BASE run) and time-channel
  fires on stale-listed holes. They are not apportioned here.

The time channel shares the defect in kind: it also judges `G(R)` of a
stale `R`. Its share is NOT measured. At first listing a p0 hole's flight
age is `age_p50` ≈ 330 ms against `thr_p50` ≈ 393 ms, with `ripe_frac`
0.010–0.042. But `age_p90` equals `thr_p50` (393 216), and a stale
re-listing is older than the first listing. So some stale-driven fires may
be counted in `r`, not `y`. The corrected law below removes both.

## 2. The corrected law

Every snapshot `R` carries a receiver version that already exists on the
wire (v9):

    V(R) = next_expected(R) + received_above(R)

- `next_expected`: the count of the delivered prefix. Received from the
  wire.
- `received_above`: the exact count of received seqs ≥ `next_expected`
  (`net/sack.rs::ReceivedAbove::sync/on_insert`). Received from the wire.

`V` is non-decreasing in receiver time and strictly increases with every
newly received seq:

- A new receipt adds 1.
- A frontier advance over received seqs moves count from one term to the
  other.
- A shed or force-deliver jump adds the holes it skips (≥ 0).

Equal `V` means an identical received set, because the set above the
frontier only grows (`SeqBits::prune_below` is a prefix prune). So `V(R') ≥
V(R)` implies `recv(R) ⊆ recv(R') ∪ [0, ne(R'))`: an older snapshot carries
no fact the newest does not.

The law judges only the newest snapshot:

    V*_t = max{ V(R) : R processed by t }

    H(R, t) = G(R)   if V(R) ≥ V*_t
            = ∅      otherwise

The packet threshold and the time threshold are both evaluated on
`H(R, t)` only, unchanged otherwise:

    lost_P(s, t) ⇔ s ∈ H(R, t) ∧ |{ q ∈ E_π(s)(t) : q > s }| ≥ k_P

    lost_T(s, t) ⇔ s ∈ H(R, t) ∧ age(s, t) ≥ max( 9/8 · max(srtt_π, ewma_π), f )

Per-symbol provenance:

| symbol | provenance |
|---|---|
| `G(R)` | measured: `sack_to_gaps(next_expected, sack_ranges)` of snapshot `R` |
| `V(R)` | measured: two v9 wire fields, no new field |
| `E_j(t)` | measured: per-path delivered evidence, ingested from accepted snapshots only (a rejected one is a subset of the newest, so dropping it loses no fact) |
| `π(s)` | measured: `source_path_map`, the original placement path |
| `k_P = 3` | cited: RFC 9002 §6.1.1 kPacketThreshold |
| `9/8` | cited: RFC 9002 §6.1.2 kTimeThreshold |
| `f` | declared floor: `NACK_RETX_COOLDOWN_FLOOR_US`, the kGranularity analog (existing) |
| `≥` (ties accepted) | required, not tuned: the receiver's timer re-advertisement re-sends an unchanged snapshot (equal `V`) so that a stalled hole is re-presented after the per-seq cooldown; rejecting ties would remove the only re-presentation of a hole with no further arrivals |

Shape check (CLAUDE.md, design review on the formula):

- **What it changes.** The sentence is "decide on the receiver's newest
  knowledge". The expression is a max-filter on a monotone version.
- **Order.** O(1) state (one `u64`), O(1) per snapshot.
- **Units.** `V` is in seqs.
- **Monotonicity.** `H` shrinks only for snapshots strictly older than one
  already applied.
- **No new constant.** There is no clamp, so no bind-fraction gauge is
  needed. Its mechanism gauge would count the snapshots rejected per
  return leg.

Equivalent set form (the invariant the red test asserts):

    s ∈ H(R, t)  ⇒  s ∉ D_t,   D_t = ∪{ recv(R') : R' processed by t }

The receiver's received set is monotone above the frontier, so the version
filter and "never judge a seq any applied snapshot showed delivered" agree.
The version form is preferred for three reasons. It needs no per-seq set.
It also stops the `[HOLD]` gauge from re-stamping, which inflates its `fed`
and `hn`. And it does not rely on `sack_released`, which exists only under
`RWM_STORE_SACK_RELEASE`.

It does not conflict with the `sack.rs` retention note ("a released symbol
remains retransmittable"). The law changes which seqs are hole candidates,
not what the store keeps. A seq the receiver reports missing in its newest
snapshot is still served from `sent_store`.

### Composition with the triangle (NO-MODE-SWITCH)

- **δ** enters `H` nowhere. The δ-honest shed reads the same `H`, and a
  receiver-side shed or force-deliver jump keeps `V` monotone.
- **ρ** enters `H` nowhere. In EVICT (ρ < 1) the window clamp applies to
  `H` exactly as it applies to `G` today.
- **Hints.** There is no hint branch.
- **Path count.** At N = 1 there is one return leg and snapshots arrive in
  generation order, so `V(R) ≥ V*` always holds and `H = G` bit-exactly. No
  path-count branch is needed; the filter is simply never active.

The filter is an information-order fact about the wire, continuous in
nothing because it is a property of no dial.

### Domain of the law

- **Tail sweep.** The sender's own tail sweep synthesizes a gap batch for
  the cumulative blocker. That batch carries no receiver snapshot and so no
  `V`. It must bypass the filter. Plumbing it as `V = 0` would reject it and
  remove the backstop for a hole with no successors.
- **Lifetime.** `V*` lives exactly as long as the seq space it versions: one
  transfer, the same scope as `mp_evid_max` (`net/mod.rs` ~1897). A path
  joining mid-transfer does not reset it; the version is the receiver's,
  not a path's.
- **Shed and force-deliver.** That `V` stays monotone through a receiver
  shed or force-deliver jump is argued from reading (`sack.rs::ReceivedAbove`,
  `receiver.rs:674`) and is NOT pinned by a test. See §5.

## 3. Predicted effect on c8 (CAND + this law, before any measurement)

These are point predictions with a stated band. Each band is a row-to-row
spread or a mechanism remainder, not a fitted quantity.

| reading | CAND now (mean) | predicted | basis |
|---|---|---|---|
| hy on p0 | 2 272 | ≤ 300 | BASE level (160) plus time-channel young fires on the larger horizon |
| hy on p1 | 696 | ≤ 350 | BASE level (287) |
| fired (= y + r) | 17 940 | 15 400 ± 700 | −Δhy (2 522) |
| y, r split | 5 445 / 12 495 | not predicted separately | stale-driven fires may sit in either channel (§1); only their sum is registered |
| dup total | 3 729 | 1 800 ± 300 | −0.78·Δhy; the BASE population remains |
| dup p1 | 2 329 | 700 ± 150 | BASE p1 level (684) |
| p0 `[HOLD] fed` | 195 k–446 k | ≤ 1 × p0 placements; no row above `sp` | re-stamps vanish |
| goodput | 107.19 Mbit/s | +0.2 to +0.5 Mbit/s | ≈ 2 k fewer 1 275-B copies (2.5 MB of 600 MB, mostly on the 20 Mbit leg); below the CAND row SD (≈ 0.33), so not resolvable at n = 6 |
| tail | 0.19–0.63 s | unchanged (same range) | the copies removed are not on the tail's critical path; the tail gain is the horizon's |

Falsifiers:

- **The heal-at-report-cadence signature survives.** If `hy_p50` stays
  ≈ 5 ms on p0 with p0 `hy` > 1 000, the mechanism is not (only) snapshot
  order. `fed > sp` is a weak falsifier: it fired in only one row before
  the fix.
- **Δdup does not follow Δhy.** If dup p1 stays > 1 200, the duplicates
  come from elsewhere.

## 4. Other findings recorded here

- **`[QCLK] site=sender` is a clamp that always binds. Defect finding.** The
  sender site is `tail_sweep_timeout_us` (`recovery_laws.rs`):
  `clamp(2·srtt_pooled, TAIL_SWEEP_MIN_US = 25 ms, TAIL_SWEEP_MAX_US = 100 ms)`,
  recorded at `net/mod.rs` ~2716. `w_us_p05` = 100 000 in all 12 rows, while
  pooled srtt is 258–468 ms. The law is a constant 100 ms over c8's
  operating range, so the bind fraction is ≈ 1. It is not the
  duplicate cause: `[FCAUSE] timer` = 3 per run.
- **`[QCLK] site=receiver` is a degenerate input. Defect finding.** The
  receiver site is `hole_nack_refresh` (`clamp(2·srtt, 25 ms, 100 ms)`,
  `receiver.rs` ~731). It reads `srtt_us_mean` = 50 000 exactly and `sigma`
  n = 0 in every line: the receiver's tx-view srtt is a constant, never a
  measurement, and 2 × 50 ms lands on the 100 ms ceiling. The re-advertise
  cadence is therefore a constant.
- **Clock premise.** The brief says the per-path clock under-reads the wire
  RTT. That is not supported. At decision time the p0 threshold is
  `thr_p50` = 393 ms in CAND and 197–229 ms in BASE. It is dwell-inclusive
  and matches
  the flight age (`age_p50` ≈ 330 ms), which is also measured from
  emission. End-of-run `rtt=/wrtt=` are post-drain snapshots that vary 15–245
  ms across rows, so `rtt=17/wrtt=60` is one such snapshot, not the
  operating clock. Where the dwell sits is a CANDIDATE, not a finding. The
  quinn per-path datagram buffer is one: 4 MiB ≈ 335 ms at 100 Mbit. But
  `dgq sp=4194304` is end-of-run free space and does not show occupancy
  during the run.
- **The `[HOLD]` gauge inherits the defect.** Its `fed`/`hn`/`obs_*` count
  re-stamps from stale snapshots. The size of the inflation is bounded
  below only by `fed − sp` (≥ 10.6 k in CAND s7-r3).
  The same version filter, applied before `on_report`/`on_reported`, cures
  it.
- **The external recovery model mirrors the shipped fold.**
  `tests/common/recovery_model.rs` ~588 applies snapshots in a single order,
  so it cannot exhibit this defect. A cross-leg return delay belongs in it
  when the law ships.

## 5. Open

- **Not measured on the wire.** Whether reports really cross in the order
  modelled is not measured per report. A per-report `V` rejection counter
  per return leg is the mechanism gauge to add with the fix. It should read
  ≈ the p1 share of gap reports, ~1/6 of `rep`.
- **Plumbing V to `serve_gaps`.** The gap channel's tuple
  `(FireCause, path, gaps)` does not carry `V` today, so the
  implementation must add it at `control_msg.rs:740`. The
  `[HOLD]` `on_report`/`on_reported` calls need it too. Wire bytes do not
  change.
- **Pin V's monotonicity.** A unit test should pin it under receiver shed
  and force-deliver before the law ships.
- **BASE's ~1.77 k duplicates are unexplained.** They are not attributed by
  this diagnosis (see §1).
- **The tail clamp needs its own derivation.** `TAIL_SWEEP_MAX_US` is
  binding, and the receiver's constant 50 ms srtt input is degenerate. Each
  needs its own ledger verdict before either clock is re-derived.
