# Roadmap

Tech tree of the Raptorpath work: what is done, what failed, what comes next. Everything between the
`GENERATED` markers is rebuilt from `docs/roadmap/nodes.yaml`; do not edit it by hand.

## How to update

1. Edit `docs/roadmap/nodes.yaml` (the one source of truth; fields and enums are enforced by `check.py`).
2. Run `python tools/roadmap/build.py` to rewrite the generated regions below.
3. Run `python tools/roadmap/check.py`; it fails on dangling edges, cycles, bad enums and stale regions.
4. Tests for the tools: `python tools/roadmap/test_roadmap.py`.

Text outside the markers is hand-written and is preserved by the build.

## Legend

<!-- BEGIN GENERATED: legend -->
Colour is never the only signal: every node label carries a status tag on its second line.

```mermaid
flowchart LR
    lg_vsucc["very successful"]
    lg_succ["successful"]
    lg_mixed["mixed"]
    lg_bad["bad"]
    lg_dead["dead end / superseded / abandoned"]
    lg_inprog["in progress"]
    lg_next["next"]
    lg_blocked["blocked"]
    lg_parked["parked"]
    lg_unrated["done, unrated"]
    lg_userdec["user decision pending"]
    lg_k1{{"user-decision"}}
    lg_k2[/"law"/]
    lg_k3>"defect-verdict"]
    lg_k4["other kinds"]
    lg_k1 --> lg_k2
    lg_k3 -.->|replaced by| lg_k4
    classDef vsucc fill:#2e9e4f,stroke:#1b6b34,color:#fff
    classDef succ fill:#bfe8c8,stroke:#2e9e4f,color:#111
    classDef mixed fill:#ffd37a,stroke:#c98a00,color:#111
    classDef bad fill:#f28b82,stroke:#b3261e,color:#111
    classDef dead fill:#e6e6e6,stroke:#777,stroke-dasharray:5 5,color:#555
    classDef inprog fill:#a8cdf5,stroke:#1f6fc5,color:#111
    classDef next fill:#fff,stroke:#333,color:#111
    classDef blocked fill:#fff,stroke:#b3261e,stroke-dasharray:2 2,color:#111
    classDef parked fill:#f3f3f3,stroke:#999,color:#555
    classDef unrated fill:#fff,stroke:#333,stroke-dasharray:1 3,color:#111
    classDef userdec fill:#d9c2f0,stroke:#7b3fb5,color:#111
    classDef stub fill:#fff,stroke:#aaa,stroke-dasharray:2 2,color:#777,font-size:11px
    class lg_vsucc vsucc
    class lg_succ succ
    class lg_mixed mixed
    class lg_bad bad
    class lg_dead dead
    class lg_inprog inprog
    class lg_next next
    class lg_blocked blocked
    class lg_parked parked
    class lg_unrated unrated
    class lg_userdec userdec
```

| Tag | Meaning |
|---|---|
| `✔ done · <outcome>` | finished; outcome is the recorded verdict (green = very successful, light green = successful, amber = mixed, red = bad) |
| `▶ in progress` | being worked on (blue) |
| `○ next` | planned, not started (white) |
| `⛔ blocked` | waiting on a prerequisite or an outside call (white, red dashed border) |
| `⏸ parked` | deliberately set aside (light grey) |
| `✖ dead end` | refuted or archived, nothing reused (grey, dashed border) |
| `↷ superseded` | replaced by another node, drawn with a dashed `replaced by` arrow (grey, dashed border) |

Shapes: hexagon = user decision (purple while pending), parallelogram = law, flag = defect verdict, rectangle = everything else.
Solid arrow = hard prerequisite (prerequisite to dependant). Dashed arrow = `replaced by`.
Amber dotted arrow = `⚠ qualified by`: a later defect or verdict limits the rating of the done node it leaves; that node's label ends in `⚠`.
<!-- END GENERATED: legend -->

## Overview

<!-- BEGIN GENERATED: overview -->
```mermaid
flowchart TB
    subgraph sg_infra["infra - repo and simplification"]
        direction TB
        infra_example_done["Example done node<br/>✔ done · successful"]
        infra_example_abandoned["Example abandoned<br/>node<br/>↷ superseded"]
        infra_example_next["Example next node<br/>○ next"]
    end
    infra_example_done --> infra_example_abandoned
    infra_example_abandoned -.->|replaced by| infra_example_next
    infra_example_done --> infra_example_next
    classDef vsucc fill:#2e9e4f,stroke:#1b6b34,color:#fff
    classDef succ fill:#bfe8c8,stroke:#2e9e4f,color:#111
    classDef mixed fill:#ffd37a,stroke:#c98a00,color:#111
    classDef bad fill:#f28b82,stroke:#b3261e,color:#111
    classDef dead fill:#e6e6e6,stroke:#777,stroke-dasharray:5 5,color:#555
    classDef inprog fill:#a8cdf5,stroke:#1f6fc5,color:#111
    classDef next fill:#fff,stroke:#333,color:#111
    classDef blocked fill:#fff,stroke:#b3261e,stroke-dasharray:2 2,color:#111
    classDef parked fill:#f3f3f3,stroke:#999,color:#555
    classDef unrated fill:#fff,stroke:#333,stroke-dasharray:1 3,color:#111
    classDef userdec fill:#d9c2f0,stroke:#7b3fb5,color:#111
    classDef stub fill:#fff,stroke:#aaa,stroke-dasharray:2 2,color:#777,font-size:11px
    class infra_example_done succ
    class infra_example_abandoned dead
    class infra_example_next next
```
<!-- END GENERATED: overview -->

## Timeline

<!-- BEGIN GENERATED: timeline -->
%% today: 2026-11-13

```mermaid
gantt
    dateFormat YYYY-MM-DD
    axisFormat %Y-%m-%d
    section infra - repo and simplification
    Example done node :infra_example_done, 2026-11-13, 2026-11-13
    Example abandoned node :crit, infra_example_abandoned, 2026-11-13, 2026-11-13
```

Not shown (no `start`): `infra-example-next`
<!-- END GENERATED: timeline -->

## Tracks

### cod - coding core

<!-- BEGIN GENERATED: track-cod -->
_No nodes in this track yet._
<!-- END GENERATED: track-cod -->

### rec - recovery

<!-- BEGIN GENERATED: track-rec -->
_No nodes in this track yet._
<!-- END GENERATED: track-rec -->

### rate - rate law

<!-- BEGIN GENERATED: track-rate -->
_No nodes in this track yet._
<!-- END GENERATED: track-rate -->

### store - store law

<!-- BEGIN GENERATED: track-store -->
_No nodes in this track yet._
<!-- END GENERATED: track-store -->

### place - placement

<!-- BEGIN GENERATED: track-place -->
_No nodes in this track yet._
<!-- END GENERATED: track-place -->

### cc - congestion control

<!-- BEGIN GENERATED: track-cc -->
_No nodes in this track yet._
<!-- END GENERATED: track-cc -->

### perf - performance

<!-- BEGIN GENERATED: track-perf -->
_No nodes in this track yet._
<!-- END GENERATED: track-perf -->

### meas - measurement

<!-- BEGIN GENERATED: track-meas -->
_No nodes in this track yet._
<!-- END GENERATED: track-meas -->

### model - model and paper

<!-- BEGIN GENERATED: track-model -->
_No nodes in this track yet._
<!-- END GENERATED: track-model -->

### obs - observability

<!-- BEGIN GENERATED: track-obs -->
_No nodes in this track yet._
<!-- END GENERATED: track-obs -->

### infra - repo and simplification

<!-- BEGIN GENERATED: track-infra -->
```mermaid
flowchart TB
    infra_example_done["Example done node<br/>✔ done · successful"]
    infra_example_abandoned["Example abandoned<br/>node<br/>↷ superseded"]
    infra_example_next["Example next node<br/>○ next"]
    infra_example_done --> infra_example_abandoned
    infra_example_abandoned -.->|replaced by| infra_example_next
    infra_example_done --> infra_example_next
    classDef vsucc fill:#2e9e4f,stroke:#1b6b34,color:#fff
    classDef succ fill:#bfe8c8,stroke:#2e9e4f,color:#111
    classDef mixed fill:#ffd37a,stroke:#c98a00,color:#111
    classDef bad fill:#f28b82,stroke:#b3261e,color:#111
    classDef dead fill:#e6e6e6,stroke:#777,stroke-dasharray:5 5,color:#555
    classDef inprog fill:#a8cdf5,stroke:#1f6fc5,color:#111
    classDef next fill:#fff,stroke:#333,color:#111
    classDef blocked fill:#fff,stroke:#b3261e,stroke-dasharray:2 2,color:#111
    classDef parked fill:#f3f3f3,stroke:#999,color:#555
    classDef unrated fill:#fff,stroke:#333,stroke-dasharray:1 3,color:#111
    classDef userdec fill:#d9c2f0,stroke:#7b3fb5,color:#111
    classDef stub fill:#fff,stroke:#aaa,stroke-dasharray:2 2,color:#777,font-size:11px
    class infra_example_done succ
    class infra_example_abandoned dead
    class infra_example_next next
```
<!-- END GENERATED: track-infra -->

## Node table

<!-- BEGIN GENERATED: table -->
### infra - repo and simplification

| id | title | state | outcome | headline | evidence | depends_on |
|---|---|---|---|---|---|---|
| `infra-example-done` | Example done node | done | successful |  | branch roadmap-tool |  |
| `infra-example-abandoned` | Example abandoned node | abandoned | superseded |  | branch example-archived | infra-example-done |
| `infra-example-next` | Example next node | next | unrated |  |  | infra-example-done |
<!-- END GENERATED: table -->

## Details per node

_Hand-written section, to be filled in later._
