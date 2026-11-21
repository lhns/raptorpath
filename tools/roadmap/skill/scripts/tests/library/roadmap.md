# Parsing library roadmap

A plain project: no queue, no special lanes.

## Legend

<!-- BEGIN GENERATED: legend -->
```mermaid
flowchart TB
    %% legend
    subgraph lg_g0["Done: colour is the rating"]
        direction LR
        lg_s0["✔ very successful"]
        lg_s1["✔ successful, or fixed"]
        lg_s2["✔ mixed"]
        lg_s3["✔ bad (regression)"]
        lg_s4["✔ done, not rated"]
    end
    subgraph lg_g1["Closed"]
        direction LR
        lg_s5["✖ dead end"]
        lg_s6["↷ superseded"]
    end
    subgraph lg_g2["Open: colour is the state"]
        direction LR
        lg_s7["▶ in progress"]
        lg_s8["⏳ built, waiting"]
        lg_s9["○ next"]
        lg_s10["⛔ blocked"]
        lg_s11["⏸ parked"]
    end
    subgraph lg_shape["Shape: kind"]
        direction LR
        lg_k0["○ change"]
        lg_k1[/"○ spike"/]
        lg_k2{{"○ decision (open, not<br/>drawn)"}}
        lg_k3{{"✔ decision (decided)"}}
    end
    subgraph lg_edge["Arrow (in the trees every arrow points down, forward in time)"]
        direction LR
        lg_e0a["prerequisite"]
        lg_e0b["depends on it"]
        lg_e1a["old design"]
        lg_e1b["replaced by"]
        lg_e2a["rated node"]
        lg_e2b["qualified by a later finding"]
    end
    lg_g0 ~~~ lg_g1 ~~~ lg_g2 ~~~ lg_shape ~~~ lg_edge
    lg_e0a --> lg_e0b
    lg_e1a -.-> lg_e1b
    lg_e2a -.-> lg_e2b
    linkStyle 6 stroke:#e69500,stroke-width:3px,stroke-dasharray:2 3
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
    classDef gate fill:#d9c2f0,stroke:#7b3fb5,color:#111
    classDef decided fill:#ece0f7,stroke:#7b3fb5,stroke-dasharray:2 2,color:#111
    classDef finding fill:#ffe0b2,stroke:#d84315,stroke-width:2px,color:#111
    classDef waiting fill:#e3f2fd,stroke:#1f6fc5,stroke-dasharray:5 5,color:#111
    class lg_s0 vsucc
    class lg_s1 succ
    class lg_s2 mixed
    class lg_s3 bad
    class lg_s4 unrated
    class lg_s5,lg_s6 dead
    class lg_s7 inprog
    class lg_s8 waiting
    class lg_s9,lg_k0,lg_k1,lg_e0a,lg_e0b,lg_e1a,lg_e1b,lg_e2a,lg_e2b next
    class lg_s10 blocked
    class lg_s11 parked
    class lg_k2 gate
    class lg_k3 decided
```

Owner calls are the purple shape. A decided owner call is drawn in the overview only, never in the track trees; an open one is listed under *Waiting on the owner*. The prefix on an overview box is its track. A prerequisite implied by a longer chain is not drawn; the node tables keep every edge.
<!-- END GENERATED: legend -->

## Ratings

<!-- BEGIN GENERATED: ratings -->
3 of 3 rated nodes are confirmed by the owner.
<!-- END GENERATED: ratings -->

## Overview

<!-- BEGIN GENERATED: overview -->
```mermaid
flowchart TB
    subgraph bd_0["Feb 2026"]
        core_tokenizer["core · ✔ Rewrite the<br/>tokenizer"]
        core_streaming[/"core · ✖ Streaming<br/>parse spike"/]
    end
    subgraph bd_1["Mar 2026"]
        docs_site["docs · ✔ Publish the<br/>docs site"]
    end
    subgraph bd_2["Now (in progress)"]
        core_error_recovery["core · ▶ Keep parsing<br/>after an error"]
    end
    subgraph bd_3["Next"]
        docs_tutorial["docs · ○ Write the<br/>tutorial"]
    end
    core_tokenizer ~~~ core_streaming
    core_streaming ~~~ docs_site
    docs_site ~~~ core_error_recovery
    core_error_recovery ~~~ docs_tutorial
    core_tokenizer --> core_streaming
    core_tokenizer --> core_error_recovery
    core_tokenizer --> docs_tutorial
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
    classDef gate fill:#d9c2f0,stroke:#7b3fb5,color:#111
    classDef decided fill:#ece0f7,stroke:#7b3fb5,stroke-dasharray:2 2,color:#111
    classDef finding fill:#ffe0b2,stroke:#d84315,stroke-width:2px,color:#111
    classDef waiting fill:#e3f2fd,stroke:#1f6fc5,stroke-dasharray:5 5,color:#111
    class core_tokenizer vsucc
    class core_streaming dead
    class core_error_recovery inprog
    class docs_tutorial next
    class docs_site succ
```
<!-- END GENERATED: overview -->

## In plain words

<!-- BEGIN GENERATED: plain-words -->
| title | status | in plain words |
|---|---|---|
| Rewrite the tokenizer | ✔ done · very-successful | The first stage of the parser is now almost three times faster. |
| Streaming parse spike | ✖ dead end | Parsing a file piece by piece did not work with our grammar. |
| Keep parsing after an error | ▶ in progress | Report every error in a file, not just the first. |
| Freeze the public API | ○ next | The maintainer decides when the public API stops changing. |
| Write the tutorial | ○ next | A first walk-through for new users. |
| Publish the docs site | ✔ done · successful | The reference pages are online. |
<!-- END GENERATED: plain-words -->

## Ready now

<!-- BEGIN GENERATED: ready-now -->
**local**

- `docs-tutorial`: Write the tutorial
<!-- END GENERATED: ready-now -->

## Running

<!-- BEGIN GENERATED: running -->
**local**

- `core-error-recovery`: Keep parsing after an error
<!-- END GENERATED: running -->

## Built, waiting

<!-- BEGIN GENERATED: built-waiting -->
_None._
<!-- END GENERATED: built-waiting -->

## Waiting on the owner

<!-- BEGIN GENERATED: waiting-on-owner -->
- `core-api-freeze`: Freeze the public API
<!-- END GENERATED: waiting-on-owner -->

## Timeline

<!-- BEGIN GENERATED: timeline -->
[The timeline](roadmap/timeline.md) shows every dated node as a bar from its start to its end.

Generated on 2026-06-30.
<!-- END GENERATED: timeline -->

## Done, by month

<!-- BEGIN GENERATED: done-by-month -->
<details><summary><b>Mar 2026</b>: 1 done, 1 abandoned</summary>

- `core-streaming`: Streaming parse spike (✖ dead end). Parsing a file piece by piece did not work with our grammar.
- `docs-site`: Publish the docs site (✔ done · successful). The reference pages are online.

</details>

<details><summary><b>Feb 2026</b>: 1 done</summary>

- `core-tokenizer`: Rewrite the tokenizer (✔ done · very-successful). The first stage of the parser is now almost three times faster.
  - Unicode escapes (2026-02-10)

</details>
<!-- END GENERATED: done-by-month -->

## Tracks

<!-- BEGIN GENERATED: tracks -->
Each track has its own page with its tree, node table and per-node details.

- [core - parser core](roadmap/tracks/core.md): 4 nodes (1 done, 1 in progress, 1 next, 1 abandoned)
- [docs - documentation](roadmap/tracks/docs.md): 2 nodes (1 done, 1 next)
<!-- END GENERATED: tracks -->
