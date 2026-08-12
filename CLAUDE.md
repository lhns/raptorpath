# Raptorpath — standing rules for agents

## THE NO-MODE-SWITCH INVARIANT (non-negotiable)

The architecture's central claim (paper §16.20/§16.26, ADR-0064) is ONE
machine parameterized by the (δ, ρ, r) triangle on measured anchors —
continuous in δ, with NO mode bit. This applies to the visualizer as much
as the engine. The user rejected the architectural mode-switches twice
(the decoder split; the hint-selected CC), and the visualizer reproduced
the same defect three times through well-meaning shortcuts (fixed-anchor
at a preset; a ρ-gated hint flip; a threshold-keyed hint boundary) — the
user caught and rejected every one of them in direct review. The pattern,
not the instances, is the enemy.

Concretely, in the visualizer and any model of the machine:

- NO `if (hint === ...)`, NO threshold on δ or ρ that selects a different
  code path, law, or constructor argument. If two behaviors must both
  exist, express them as ONE formula continuous in the dial (e.g. the
  shipped rate law: r(β) = (1−β)·r_anchor + β·r_late-is-fine, both terms
  always computed, β = bulkness_of_delta(δ)).
- ρ is the retention contract — an INDEPENDENT dial of the triangle. It
  must compose with δ everywhere; it never selects a machine.
- The protocol hints (Bulk/Auto/Realtime) are NAMED POINTS on the dials,
  never modes.
- A behavior step across any preset point is a defect even if each side
  is individually correct.

Enforcement (do not weaken these; extend them when touching this area):
- `raptorpath-visualizer/test_visualizer.mjs` — law continuity+monotonicity
  through every preset (±2% nudges) and sim-behavior continuity across the
  Bulk preset.
- `raptorpath-visualizer/test_visualizer_ui.mjs` — ONE hint string at every
  dial position including ρ < 1 (stub-DOM routing gate).
- `raptorpath-wasm` `test_continuum_one_law_across_the_dial`.
Both mjs gates run in `build_visualizer.sh`; a failure fails the build.

## Testing discipline (the lesson behind the gates)

Ordinal tests ("A more than B") do not catch routing bugs — assert
ABSOLUTE law invariants at the anchor points, and assert that the wiring
between layers actually routes there (MEASUREMENT DISCIPLINE rule 1 in
`raptorpath/docs/goal-gate.md`: prove the mechanism under test executes).
Every documented model-vs-engine divergence must carry a test that BOUNDS
it, not prose that describes it.

## FORMULA-FIRST LAWS (ADR-0070)

A law that is measured exhaustively but never READ as a formula is not
verified — it is only pinned. The store-cap law carried nine always-on
absolute pins, two component benches and an L1 gauge for a month while
being quadratic in the path count where its own doc comment described a
linear quantity, and one term of it had no provenance in the repository
at all. Every pin passed. They were all asserting that the code computes
the model; none asked whether the model was right.

- **No law ships without its formula and its derivation IN THE PAPER,
  before the code.** Each symbol gets a one-line provenance: measured
  (with the sweep), cited (with the reference), a declared dial, or a
  resource bound stated outside the law. "Argued in a commit message" is
  not provenance, and a constant with none does not ship.
- **Design review presents the FORMULA, not the diff.** Put the
  expression on a line by itself next to the sentence it is supposed to
  implement, and check that the sentence and the expression agree in
  SHAPE (order in N, units, monotonicity) before looking at any number.
- **Every clamp gets a bind-fraction gauge**, reported. A clamp that
  always binds turns its law into a constant and hides the law's shape
  from every measurement taken through it.
- **A law measured pinned or degenerate over its operating range is a
  DEFECT FINDING requiring a ledger verdict — never an explanatory
  footnote.** See MEASUREMENT DISCIPLINE 17 and 18 in
  `raptorpath/docs/goal-gate.md`.

## Scope rules

- The visualizer (`raptorpath-visualizer/` + `raptorpath-wasm/`) is a
  self-contained L0 model: never change engine crates (`raptorpath`,
  `raptorpath-math`, ...) for a visualizer feature.
- The wasm sim's golden fingerprints pin the model era; re-capture them
  (`GOLDEN_CAPTURE=1`) only for a DELIBERATE model change, and say so.
