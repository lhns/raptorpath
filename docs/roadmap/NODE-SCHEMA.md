# Tech-tree node schema (docs/roadmap/nodes.yaml)

One node = ONE thing that got, or will get, its own verdict.

## Granularity rules (apply in this order)
1. **Own verdict:** it could have been merged, refuted or parked independently of its neighbours. A pre-registration plus its amendments, rehearsal, result and errata is ONE node.
2. **Own evidence:** it has at least one status section, ADR, ledger or merged branch.
3. **Own edges:** two items with the same prerequisites, dependants and outcome merge into one node (record the absorbed items in `folded`).
4. **Forward nodes:** one lane and one done criterion each (a guard PASS, a checked derivation, a user call). A decision between two VM sessions makes two nodes.
5. **Always a node:**
   - each refuted or archived design (abandoned paths stay visible);
   - each user-decision gate;
   - each §3.10 defect verdict;
   - each law in the law ledger.
6. **Never a node:** a single commit, a bug fix inside a branch, a re-pin, a harness repair.
7. **Sanity check:** 25–35 nodes in the overview; 8–20 per track.

## Fields
```yaml
- id: rec-snapver          # stable kebab-case, prefixed by track code (see tracks)
  title: "Snapshot-order filter"   # ≤ 6 words
  track: rec               # one track code
  kind: law | mechanism | instrument | infra | research | user-decision | defect-verdict | simplification
  state: done | in-progress | next | blocked | parked | abandoned
  outcome: very-successful | successful | mixed | bad | dead-end | superseded | unrated
  evidence: ["status §53", "ADR-0072", "branch snapver-law", "docs/l1-raw/s53"]
  verdict_quote: "…"        # verbatim from the source, ≤ 300 chars ("" for forward nodes)
  headline: "c8 duplicates 4004 → 1291"   # one number with its unit, verbatim ("" if none)
  depends_on: [rec-gap-horizon]           # HARD prerequisites only
  replaced_by: []           # for superseded or abandoned nodes: the node that replaced it
  lane: vm | host-cargo | no-cargo | user | none
  start: 2026-11-13
  end: ""                   # "" if open
  folded: ["status §53 amendment 1", "branch dup-diag"]   # source items absorbed
  overview: true            # true for the ~25–35 nodes in the overview tree
```

## Outcome scale (from a recorded verdict only; else `unrated`)
- **very-successful:** merged, with a measured gain on a registered readout.
- **successful:** merged or accepted, with no headline gain.
- **mixed:** a gain with a recorded cost, or partial.
- **bad:** REGRESSION, or merged then reverted.
- **dead-end:** refuted or archived, with nothing reused.
- **superseded:** replaced by a named node (fill `replaced_by`).
- **unrated:** no recorded verdict.

`state` and `outcome` are separate fields. A done node can be `mixed`; an abandoned node is `dead-end` or `superseded`.

## Tracks (code: scope)
- `cod`: coding core — RaptorQ/FEC encoder and decoder, gf256, symbol framing, the generation seat
- `rec`: recovery — ARQ, SACK, the hole law, recovery clocks, gap horizon, snapshot order, tail sweep
- `rate`: the rate law and the (δ, ρ, r) triangle — FEC rate, P_fec, W*, hints as named points
- `store`: flow control and the store law — store cap, SACK-clocked release, the time term, δ's seat
- `place`: multipath placement — placement price, idle-leg lock-out, diversity weight, per-leg state
- `cc`: substrate congestion control — quinn BBR, `bbr_rs`, Copa, the BBR/Copa hybrid (ADR-0068), the pacing seam
- `perf`: threading and per-packet cost — threading redesign, I/O owner, receiver cost, allocation bounds
- `meas`: measurement and harness — the L1 harness, guards, stub run, instruments, measurement discipline
- `model`: the model, paper and visualizer — paper, wasm L0 model, NO-MODE-SWITCH audits, model-vs-engine bounds
- `obs`: observability — the metrics registry, OTel, the flight recorder
- `infra`: repo and simplification — restructure, simplify batches, archived arms, early platform work (handshake, TLS, MTU, migration)
