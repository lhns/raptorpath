# Owner requests and where they went

86 distinct requests or wishes from the owner's 308 messages in user-requests.txt (2026-07-02 to 2026-11-13), repeats collapsed. Not listed: status pings ("still running?", "continue", "resume"), git and VM chores, and questions answered in conversation. 1 never done.

| date | request | node or never done |
|---|---|---|
| 2026-07-02 | Scan the project and the paper; find errors in the approach | `meas-adr51-scenarios`, `model-math-verification` |
| 2026-07-02 | Define a goal: surpass TCP and react correctly in defined scenarios | `meas-adr51-scenarios` |
| 2026-07-02 | Measure on real hardware, not wall-clock time on a loaded laptop | `meas-l1-harness` |
| 2026-07-02 | Rebuild the visualizer as one HTML file with embedded wasm | `model-wasm-l0` |
| 2026-07-02 | Keep the whole triangle configurable alongside the protocol hint; bulk should not finish later than realtime | `model-vis-continuum`, `model-wasm-l0` |
| 2026-07-02 | Show latency, average latency and jitter in the visualizer, with a tooltip on every number | `model-wasm-l0` |
| 2026-07-02 | Plan real-world networking tests on the Linux VM | `meas-l1-harness` |
| 2026-07-04 | Define the next project phase and goal; model asymmetric paths | `meas-l2-multipath` |
| 2026-07-04 | Visualizer issues: "fixed r" is a target, overhead below the channel floor, derive the window size, soft saturation instead of a hard cap, taper at stream end | `rate-saturation-rstar` |
| 2026-07-04 | Fix the slow visualizer in realtime at high saturation | `model-wasm-l0` |
| 2026-07-04 | Find what raptorpath can learn from BBR | `cc-copa-lite` |
| 2026-07-04 | Stop switching between window and block mode; unify block and streaming | `cod-one-pipeline`, `cod-unified-span` |
| 2026-07-04 | Use windowed RLC across blocks for every profile and benchmark it | `cod-rlc-all-profiles` |
| 2026-07-06 | No mode switch between codes per profile; stay principled | `cod-unified-span` |
| 2026-07-06 | Get RLC as good as RaptorQ for bulk | `cod-one-pipeline` |
| 2026-07-06 | Visualizer: configurable number of paths with per-path link sliders | `model-wasm-l0` |
| 2026-07-06 | No equal-weight striping: water filling or probabilistic weighting; path affinity continuous, no hard switch | `place-marginal-cost` |
| 2026-07-06 | Retention follows the configurable reliability (ρ), ARQ never dropped | `rate-retention-is-rho`, `rec-reliable-window` |
| 2026-07-06 | Include the two loose knobs in the model in a grounded, provable way, with groundwork to verify the model | `meas-c8-aggregation-arc`, `model-law-ledger` |
| 2026-07-06 | Keep the protocol useful without a reorder buffer (unordered delivery) | `meas-c8-aggregation-arc` |
| 2026-07-06 | Test the oracle against real-world measurements | `meas-regime-map` |
| 2026-07-06 | Build generation-based cross-path fungible coding | `cod-generation-coding` |
| 2026-07-07 | Combine the dense and sparse decoders; use systematic source plus repair | `cod-systematic-wire` |
| 2026-07-07 | Do not give up on FEC; make it work | `rec-fec-vs-arq`, `rate-r-battery` |
| 2026-07-08 | Reuse repair symbols whose degree of freedom is freed when ARQ fills a hole | `rate-dof-ledger` |
| 2026-07-08 | Derive the window advance / overlap smoothly instead of a hard switch between bulk and realtime | `cod-unified-span` |
| 2026-07-08 | Research papers on the observed problems; rerun the whole benchmark suite | `meas-c8-aggregation-arc` |
| 2026-07-08 | Make another attempt at multipath aggregation with that knowledge | `cod-fmtcp`, `place-daps` |
| 2026-07-12 | Implement DAPS properly from the literature, with references and a glossary entry for BDP | `place-daps` |
| 2026-07-12 | Measure FEC beyond bulk; report achievable throughput under the simulated loss | `meas-regime-map` |
| 2026-07-13 | Run Copa as the only controller over a pass-through quinn controller | `cc-copa-sole-substrate` |
| 2026-07-13 | Bulk ARQ should approach N times one path's throughput | `meas-regime-map`, `store-xn-pool` |
| 2026-07-19 | Start the Copa work in parallel | `cc-copa-wire-signal` |
| 2026-07-21 | Understand why most changes performed worse and are off; be principled | `meas-adr52-discipline` |
| 2026-07-21 | Keep Cubic as a configuration option? | `cc-bbr-default` |
| 2026-07-21 | Move experiments and results into ADRs; decide which features to keep | `infra-consolidation` |
| 2026-07-21 | No BBR-for-bulk / Copa-for-latency mode switch; explore a Copa-BBR fusion in an ADR, a simple fix for now | `cc-adr68-fusion`, `cc-bbr-default`, `cc-hybrid` |
| 2026-08-07 | Remove the METTLE module (patent risk) without rewriting history | `cod-mettle` |
| 2026-08-08 | What about the hybrid Copa-BBR controller? | `cc-adr68-fusion`, `cc-hybrid` |
| 2026-08-08 | Test and tune mechanisms in isolation, not the whole machine | `meas-adr52-discipline` |
| 2026-08-08 | Plan a cleanup preserving behaviour; remove the legacy ack and unnecessary code | `infra-net-seams`, `perf-ack-merge` |
| 2026-08-09 | Find the model issue: measure terms separately, check them against the model, no hard-coded tuning, a principled derivation method | `store-three-term`, `meas-formula-first` |
| 2026-08-10 | Document the new formula in the paper first | `meas-formula-first` |
| 2026-08-10 | Review whether the last steps were correct or a false track | `meas-pipeline-verification` |
| 2026-08-11 | Back off and verify each stage of the encoder-decoder pipeline against estimates | `meas-pipeline-verification` |
| 2026-08-11 | Keep the architecture documented in the paper and up to date | `meas-pipeline-verification`, `model-paper-recon` |
| 2026-08-11 | Remove the legacy store cap | `store-cap-unification`, `store-sum-cap` |
| 2026-08-12 | Explain the store-cap formula; question N, the 2x gain, max versus average inputs | `store-sum-cap`, `store-delta-cap` |
| 2026-08-13 | Where do floor and knee come from; certify the formula is correct | `meas-formula-first`, `model-open-constants`, `store-engine-form-impl` |
| 2026-08-17 | Fix the defects you are confident about yourself | `store-sum-cap` |
| 2026-08-18 | Enable the fixed mechanisms; test with more than two paths | `store-sum-cap`, `meas-goal-0819` |
| 2026-08-18 | Search other papers to confirm the formulas | `store-delta-cap` |
| 2026-08-19 | A tuned constant ships only with justification | `store-delta-cap` |
| 2026-08-19 | Report which attributes improved by how much with all default changes, and what is still not working | `meas-progress-baseline` |
| 2026-08-19 | Fix the ping probe; do not mislead ourselves in measurements | `meas-goal-0819` |
| 2026-08-20 | Derive the recovery clock from something real (false-repair chance, the triangle); measure in isolation; keep both approaches until known | `rec-goal100`, `rec-cost-ratio-decision` |
| 2026-08-20 | Fix the measurement instrument, then find the formula | `rec-goal101` |
| 2026-09-06 | Try several methodical approaches in parallel to find the correct laws | `meas-attribution-audit`, `rate-r-battery`, `rec-receiver-law`, `place-stage2-battery` |
| 2026-09-08 | Cap every measurement at 5 hours | `meas-5h-cap` |
| 2026-09-08 | Did any improvement win? Look for deeper issues | `meas-wire-v8` |
| 2026-09-26 | Review and clean up the whole project: duplication, dead code, complexity, yapping in docs | `infra-cleanup-0927` |
| 2026-09-29 | Analyze self-caused issues and fix the defects before hunting laws | `infra-fix-program` |
| 2026-10-02 | Use AVX or similar for CPU optimisation | never done: no SIMD work after 2026-10-02 is recorded; the CPU work went to threading and batching instead (gf256 SIMD dates from March) |
| 2026-10-05 | Redesign threading properly (cache friendliness, affinity, preemption; look at cats-effect) | `perf-io-owner`, `perf-sched-split` |
| 2026-10-05 | Rethink the threading implementation; proper async is possible | `perf-threading-p2a`, `perf-io-owner` |
| 2026-10-26 | Run tests in memory instead of on disk | `store-input-search` |
| 2026-11-09 | Put follow-ups into the plan; work the formulas with the Fable advisor | `store-engine-form`, `model-law-ledger` |
| 2026-11-09 | Consider the laws together, not only in isolation; rate the whole model against newer research | `model-review-37` |
| 2026-11-10 | Put passing changes on main; clean up; re-do the testing and benchmarking setup | `place-wire-price`, `meas-guard-41c`, `meas-harness-consolidation` |
| 2026-11-10 | Ask Fable whether the formulas are overcomplicated | `infra-simplify-review` |
| 2026-11-10 | Parallelise; throw out what did not pass while testing | `infra-adr73`, `infra-simplify-batch1` |
| 2026-11-11 | Show the performance improvements since the Fable work started | `meas-progress-baseline` |
| 2026-11-12 | Fix the void +500 ms rows threshold and the RSTAR_TAIL paper/status mismatch | `meas-harness-consolidation` |
| 2026-11-12 | Archive the placement frontier experiment or give it a reliable test | `place-hol` |
| 2026-11-12 | Find a way for bbr_rs not to pay its cost | `cc-bbrrs-r2`, `cc-s48d-session` |
| 2026-11-12 | Build both the bbr_rs improvement and the Copa-BBR hybrid | `cc-bbrrs-r2`, `cc-hybrid` |
| 2026-11-12 | Improve tests and benchmarks; agents in parallel | `meas-harness-robustness`, `meas-guard-resolution`, `meas-test-hygiene` |
| 2026-11-12 | OTel metrics for every performance aspect, plus the flight recorder, following standards | `obs-otel-design`, `obs-impl` |
| 2026-11-12 | Answers to the OTel questions (remove dead keys, http/protobuf, ratio sampler, collector) | `obs-user-otel-answers` |
| 2026-11-12 | Vendor the quinn-proto fork for the pacing seam | `cc-user-pacing-s3`, `cc-pacing-seam` |
| 2026-11-12 | Restructure packages, code and docs; root README and gitattributes | `infra-restructure` |
| 2026-11-13 | Analyse with Fable why there are so many duplicates | `rec-snapver-filter` |
| 2026-11-13 | Explain rtp_floor_p0 before deciding | `meas-defect-rtp-floor`, `meas-guard-v2` |
| 2026-11-13 | Do the three recommendations (filter as law, store-budget test, guard clause) | `rec-snapver-law`, `store-s54-budget`, `meas-guard-clause-dry` |
| 2026-11-13 | Correct formulas first, then a product as fast as it can be; plan the next steps | `model-law-ledger`, `store-fix-maxbw-minrtt` |
| 2026-11-13 | A tech tree of the project, then a reusable skill as a PR | `infra-inventory-merges`, `infra-tech-tree-skill` |
