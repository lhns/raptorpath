# Tech-tree node schema (docs/roadmap/nodes.yaml)

The fields, states, outcome scale, edges, granularity rules and generated regions are defined by the vendored tech-tree skill: `tools/roadmap/skill/schema.md` (version: `tools/roadmap/skill/VERSION`). Names of tracks, kinds and lanes, the overview band and the layout are in `docs/roadmap/tech-tree.yaml`. This file holds only what is specific to Raptorpath.

## Raptorpath's reading of the granularity rules
- **Own verdict:** a pre-registration plus its amendments, rehearsal, result and errata is ONE node.
- **Own evidence:** a status section, ADR, ledger entry or merged branch.
- **Forward nodes:** one lane and one done criterion each (a guard PASS, a checked derivation, a user call). A decision between two VM sessions makes two nodes.
- **Always a node:** each refuted or archived design; each `user-decision` gate; each §3.10 `defect-verdict`; each law in the law ledger.
- **Never a node:** a single commit, a bug fix inside a branch, a re-pin, a harness repair.
- **Overview:** 50-60 nodes (`check.py` warns outside the band).

## Kinds
`law`, `mechanism`, `instrument`, `infra`, `research`, `simplification`, `user-decision` (the skill's owner gate: outcome `n/a`, hexagon, listed under *Waiting on the owner*) and `defect-verdict` (a §3.10 finding: never rated, does not block its dependants).

## Lanes
- `vm`: the shared, serial VM; its forward nodes carry a unique `queue_order` and are listed under *VM queue*.
- `host-cargo`: needs cargo on the host (under the host mutex).
- `no-cargo`: needs no build: a derivation, a write-up, a registration.
- `user`: needs the owner.
- `none`: nothing to run (a closed or abstract node).

## Tracks
`cod` coding core, `rec` recovery, `rate` rate law, `store` store law, `place` placement, `cc` congestion control, `perf` performance, `meas` measurement, `model` model and paper, `obs` observability, `infra` repo and simplification; the scope of each is in `tech-tree.yaml`.

## Rating rule (differs from the pre-skill tool)
A rated outcome needs a `verdict_quote`, verbatim from the source that states the result. `check.py` fails without it. No quote: the node is `unrated`.
