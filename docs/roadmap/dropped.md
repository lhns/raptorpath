# Reviewed and left out of the tree

Items from the history extraction that were checked and given no node (granularity rule 6 in `tools/roadmap/skill/schema.md`: a cleanup pass, an instrument repair, test hygiene, record keeping). Kept so the decision is not made twice. Moved here from the former top-level `dropped` list of `nodes.yaml`; the tree schema has no such list.

- **status §1**: living summary (status §1 default stack); every gate it lists is a node elsewhere
- **status §2**: living summary (status §2 verdict table); each row is recorded under its own section
- **branch cleanup pass c292268..main**: routine cleanup pass after threading (stale comments, dead io_owner code), no verdict
- **branch gauge reachability**: instrument repair (teardown gauges moved to a destructor), rule 6
- **commits 21de95b5, 1d835478, bea1e4fd (sched flake fixes)**: HashMap-order flaky-test fixes and one RFC 6298 assertion; test hygiene with no verdict and no nearest node
- **commits 346e6a0c, 0128bb5c, 10e693b8, bee23f6f (merge sweep items 1-6)**: per-item merges inside the 2026-08-18 cap-law ladder arc (K into formula, sender-truth loss estimator, un-metered channels, quad lock-in refuted); results are in the paper section 10 rows and the Ladder Battery on store-sum-cap, no node of their own
- **commits 6e386bf6, 91e74e2a, 32c7f473, 3d2a7907, d0ef8a3b (goal-gate ledger entries)**: goal-gate record keeping (suite results, dead-code refactor, SF anchor suspect refuted, gauge reachability, RFA note); same family as the existing 'branch gauge reachability' drop
