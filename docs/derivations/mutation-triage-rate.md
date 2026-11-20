# Mutation triage, rate-law functions (S6)

Plan step S6 (`test-detection-plan.md`), roadmap node `infra-test-mutation`. Scope: the surviving
mutants in the functions behind the repair rate r: the FEC rate and its controller, P_fec, r*, the
taper, the delta dial and the three-variable solvers. Line numbers are those of github/main
`7123c754`. Baseline: `tools/ci/mutants-baseline.txt`.

## Counts

| | before | after |
|---|---|---|
| survivors in these functions | 134 | 32 |
| baseline file (all of raptorpath-math/core, 1 721 mutants) | 486 | 384 |

102 killed by 28 new tests (`raptorpath-math/tests/mutation_triage_rate.rs`). The 32 left are all
class (a): equivalent mutants. None is class (b) any more. Of those, 15 also point at dead or redundant code
(a+c, listed below). Not triaged here (next pass): `find_t_cut`, `p_recovered_within`, `boost_params`,
`derive_window`, `compute_buffer_max`, `beta_quantile`, and the estimators.

| function | before | after |
|---|---|---|
| FecRateController::compute_repair_rate | 14 | 2 |
| FecRateController::compute_triangle | 6 | 0 |
| TaperFunction::should_generate | 6 | 0 |
| bulkness_of_delta | 6 | 0 |
| burst_variance_factor | 3 | 0 |
| compute_delta | 5 | 1 |
| compute_r_star | 1 | 0 |
| compute_r_star_exact | 1 | 0 |
| compute_r_star_with_z | 2 | 0 |
| controller_rate | 12 | 4 |
| inner_feedback_floor | 9 | 5 |
| p_fec_exact | 1 | 1 |
| p_fec_exact_repairs | 3 | 3 |
| p_fec_normal | 5 | 3 |
| p_fec_recovery_by_time | 5 | 0 |
| r_saturation | 5 | 5 |
| r_star_bisect | 3 | 0 |
| r_star_mass | 7 | 4 |
| soft_saturate | 1 | 1 |
| solve_r_from_delta_rho | 10 | 1 |
| solve_r_from_time_budget | 17 | 1 |
| solve_rho_from_r_delta | 7 | 1 |
| zeta_of_delta | 5 | 0 |
| **total** | **134** | **32** |

## How the tests are absolute

No ordinal assertions. Each pins one of: a closed form at an anchor (P_fec, r*, Poisson tail, the dial
at its presets), a defining property of a solver (`delta(r) <= target` and `delta(r - 2e-6) > target`; the
inner-feedback floor solves S(r) = theta), a loose-target anchor (the solver returns the
information-theoretic floor eps/(1-eps) plus the bracket left at the 1e-6 stop, which also pins the stop
rule), a domain guard (each invalid input alone returns the documented value), or an exact tie
(r*(F(r0)) = r0 at a grid point and at the first bisection midpoint; fail == delta at the search ceiling).
`controller_rate` is pinned against the doc comment's formula with each term isolated.

## Table

Class: (a) equivalent, (b) missing test, now killed, (c) dead code or constant without provenance
(reported, not changed). `a+c`: equivalent because the code is dead or redundant.

| function | site | mutants | status | class | test / reason |
|---|---|---|---|---|---|
| `bulkness_of_delta` | lib.rs:199:9 | - with +; - with / | killed | b | `bulkness_is_the_log_position_between_the_anchors` |
| `bulkness_of_delta` | lib.rs:199:14 | / with %; / with * | killed | b | `bulkness_is_the_log_position_between_the_anchors` |
| `bulkness_of_delta` | lib.rs:199:19 | - with +; - with / | killed | b | `bulkness_is_the_log_position_between_the_anchors` |
| `burst_variance_factor` | lib.rs:87:17 | \|\| with && | killed | b | `burst_variance_factor_anchors_and_guards` |
| `burst_variance_factor` | lib.rs:89:12 | < with <=; < with == | killed | b | `burst_variance_factor_anchors_and_guards` |
| `compute_delta` | lib.rs:1230:23 | \|\| with && | killed | b | `compute_delta_closed_form_and_guards` |
| `compute_delta` | lib.rs:1233:17 | < with == | killed | b | `compute_delta_closed_form_and_guards` |
| `compute_delta` | lib.rs:1233:17 | < with <= | survives | a | fec_miss == 1e-15 to the bit is unconstructible (miss = eps x a multiple of 2^-53) |
| `compute_delta` | lib.rs:1235:22 | / with %; / with * | killed | b | `compute_delta_closed_form_and_guards` |
| `compute_r_star` | lib.rs:100:5 | whole function -> f64 with 1.0 | killed | b | `r_star_closed_form_and_domain_guards` |
| `compute_r_star_exact` | lib.rs:1192:25 | \|\| with && | killed | b | `p_fec_exact_anchors_and_search_ties` |
| `compute_r_star_with_z` | lib.rs:111:23 | \|\| with && | killed | b | `r_star_closed_form_and_domain_guards` |
| `compute_r_star_with_z` | lib.rs:113:33 | > with >= | killed | b | `r_star_closed_form_and_domain_guards` |
| `compute_repair_rate` | fec_rate_controller.rs:33:62 | * with +; * with / | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_repair_rate` | fec_rate_controller.rs:33:66 | / with %; / with * | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_repair_rate` | fec_rate_controller.rs:33:73 | - with +; - with / | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_repair_rate` | fec_rate_controller.rs:47:47 | > with <; > with == | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_repair_rate` | fec_rate_controller.rs:47:47 | > with >= | survives | a | `codec_overhead >= 0` admits 0, and 0 x P(decoder) = 0: same value |
| `compute_repair_rate` | fec_rate_controller.rs:47:53 | && with \|\| | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_repair_rate` | fec_rate_controller.rs:47:68 | > with <; > with == | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_repair_rate` | fec_rate_controller.rs:47:68 | > with >= | survives | a | `window_size >= 0` admits 0, and P(decoder) = 1-(1-eps)^0 = 0: same value |
| `compute_repair_rate` | fec_rate_controller.rs:48:33 | - with + | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_triangle` | fec_rate_controller.rs:71:62 | * with +; * with / | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_triangle` | fec_rate_controller.rs:71:66 | / with %; / with * | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `compute_triangle` | fec_rate_controller.rs:71:73 | - with +; - with / | killed | b | `controller_without_ge_data_uses_the_iid_fallback_transition` |
| `controller_rate` | lib.rs:737:10 | < with <=; < with == | killed | b | `controller_rate_* (five tests)` |
| `controller_rate` | lib.rs:756:58 | > with >= | survives | a | codec_overhead = 0 gives 0 x P(decoder) = 0 |
| `controller_rate` | lib.rs:756:64 | && with \|\| | killed | b | `controller_rate_* (five tests)` |
| `controller_rate` | lib.rs:756:78 | > with >= | survives | a | window = 0 gives P(decoder) = 0 |
| `controller_rate` | lib.rs:786:43 | && with \|\| | killed | b | `controller_rate_* (five tests)` |
| `controller_rate` | lib.rs:791:21 | > with >= | survives | a+c | eps_mass > 0 whenever mass.is_valid() (nz, m1 > 0): the else branch is dead; at denormal underflow r_star_mass(level = inf) returns 0 as well |
| `controller_rate` | lib.rs:814:10 | > with >= | survives | a | w = 0 contributes 0 x floor = 0 to a max against rate >= 0 |
| `controller_rate` | lib.rs:824:27 | && with \|\| | killed | b | `controller_rate_* (five tests)` |
| `controller_rate` | lib.rs:824:40 | > with >= | killed | b | `controller_rate_* (five tests)` |
| `controller_rate` | lib.rs:824:46 | && with \|\| | killed | b | `controller_rate_* (five tests)` |
| `controller_rate` | lib.rs:824:58 | > with >= | killed | b | `controller_rate_* (five tests)` |
| `inner_feedback_floor` | lib.rs:622:14 | > with >= | survives | a | p = 0: stall(0) = 0 <= theta returns 0 anyway |
| `inner_feedback_floor` | lib.rs:623:14 | < with <= | killed | b | `inner_feedback_floor_solves_its_defining_equation` |
| `inner_feedback_floor` | lib.rs:629:18 | > with >= | survives | a | r_cap = 0: the later paths return r_cap = 0 anyway |
| `inner_feedback_floor` | lib.rs:635:22 | / with %; / with * | killed | b | `inner_feedback_floor_solves_its_defining_equation` |
| `inner_feedback_floor` | lib.rs:642:23 | / with % | killed | b | `inner_feedback_floor_solves_its_defining_equation` |
| `inner_feedback_floor` | lib.rs:648:21 | > with ==; > with >= | survives | a+c | the infeasible early return is redundant: the bisection keeps hi = r_cap and returns it |
| `inner_feedback_floor` | lib.rs:655:23 | > with >= | survives | a | a tie stall(mid) == theta to the bit is unconstructible |
| `p_fec_exact` | lib.rs:1130:25 | \|\| with && | survives | a+c | the callee `p_fec_exact_repairs` returns 1.0 for the same inputs: the guard is duplicated |
| `p_fec_exact_repairs` | lib.rs:1144:15 | \|\| with && | survives | a+c | both callers (`p_fec_exact`, `compute_r_star_exact`) guard first: the callee guard is unreachable |
| `p_fec_exact_repairs` | lib.rs:1174:39 | - with +; - with / | survives | a+c | `.min(dmax-1)` never binds: at most W source losses, so d+1 <= off+W = dmax-1 |
| `p_fec_normal` | lib.rs:886:27 | \|\| with && | killed | b | `p_fec_normal_closed_form_and_domain_guards` |
| `p_fec_normal` | lib.rs:886:45 | \|\| with && | killed | b | `p_fec_normal_closed_form_and_domain_guards` |
| `p_fec_normal` | lib.rs:886:63 | \|\| with && | survives | a | eps >= 1 alone or r <= 0 alone already leaves numerator <= 0, which returns 0 |
| `p_fec_normal` | lib.rs:890:20 | < with <=; < with == | survives | a+c | `denominator < 1e-300` is redundant: denominator 0 gives z = inf, survival 0, P_fec 1.0 anyway; exactly 1e-300 is unreachable (the radicand underflows first) |
| `p_fec_recovery_by_time` | lib.rs:1370:23 | \|\| with && | killed | b | `p_fec_recovery_by_time_is_the_poisson_tail` |
| `p_fec_recovery_by_time` | lib.rs:1371:24 | * with / | killed | b | `p_fec_recovery_by_time_is_the_poisson_tail` |
| `p_fec_recovery_by_time` | lib.rs:1371:31 | - with + | killed | b | `p_fec_recovery_by_time_is_the_poisson_tail` |
| `p_fec_recovery_by_time` | lib.rs:1371:42 | / with * | killed | b | `p_fec_recovery_by_time_is_the_poisson_tail` |
| `p_fec_recovery_by_time` | lib.rs:1371:49 | + with - | killed | b | `p_fec_recovery_by_time_is_the_poisson_tail` |
| `r_saturation` | lib.rs:918:65 | < with <= | survives | a | eps = 1: every cost is inf, `inf < inf` is false, best_r stays 1.0 = the no-cap value |
| `r_saturation` | lib.rs:920:41 | > with >= | survives | a | window = 0: tail_svc = 0 and the argmin is the last grid point, 1.0 = the no-cap value |
| `r_saturation` | lib.rs:936:45 | + with - | survives | a+c | (1+r)/r = 1/r + 1: the constant 1 cannot move the argmin |
| `r_saturation` | lib.rs:937:42 | + with * | survives | a+c | C(1+r)W t = C W t + C W t r: the constant cannot move the argmin |
| `r_saturation` | lib.rs:939:17 | < with <= | survives | a | cost ties between grid points do not occur |
| `r_star_bisect` | lib.rs:1217:17 | > with ==; > with >= | killed | b | `p_fec_exact_anchors_and_search_ties` |
| `r_star_bisect` | lib.rs:1221:22 | > with >= | killed | b | `p_fec_exact_anchors_and_search_ties` |
| `r_star_mass` | lib.rs:486:24 | > with >= | killed | b | `r_star_mass_is_the_least_r_with_bound_at_most_delta, r_star_mass_zero_level_scale_is_zero_even_for_chunked_windows` |
| `r_star_mass` | lib.rs:511:43 | - with +; - with / | survives | a+c | `.min(MASS_SCALES-1)` never binds: x < MASS_SCALES gives lo+1 <= MASS_SCALES-1 |
| `r_star_mass` | lib.rs:519:35 | > with ==; > with >= | survives | a+c | the infeasible-at-ceiling early return is redundant: the grid scan and bisection also end at hi = 2.0 (a tie at the ceiling does too) |
| `r_star_mass` | lib.rs:530:35 | > with >= | killed | b | `r_star_mass_is_the_least_r_with_bound_at_most_delta, r_star_mass_zero_level_scale_is_zero_even_for_chunked_windows` |
| `r_star_mass` | lib.rs:542:23 | > with >= | killed | b | `r_star_mass_is_the_least_r_with_bound_at_most_delta, r_star_mass_zero_level_scale_is_zero_even_for_chunked_windows` |
| `should_generate` | lib.rs:877:9 | whole function -> bool with false; whole function -> bool with true | killed | b | `taper_density_and_generation_decision` |
| `should_generate` | lib.rs:878:14 | >= with < | killed | b | `taper_density_and_generation_decision` |
| `should_generate` | lib.rs:878:47 | < with <=; < with ==; < with > | killed | b | `taper_density_and_generation_decision` |
| `soft_saturate` | lib.rs:1084:12 | > with >= | survives | a | s = 0 only when r_sat = 0; the closed form then also returns 0 (softplus(-inf) = 0, NaN floors to 0) |
| `solve_r_from_delta_rho` | lib.rs:1303:26 | / with %; / with * | killed | b | `solve_r_from_delta_rho_* (three tests)` |
| `solve_r_from_delta_rho` | lib.rs:1303:33 | - with +; - with / | killed | b | `solve_r_from_delta_rho_* (three tests)` |
| `solve_r_from_delta_rho` | lib.rs:1306:15 | - with +; - with / | killed | b | `solve_r_from_delta_rho_* (three tests)` |
| `solve_r_from_delta_rho` | lib.rs:1306:20 | < with == | killed | b | `solve_r_from_delta_rho_* (three tests)` |
| `solve_r_from_delta_rho` | lib.rs:1306:20 | < with <= | survives | a | stop when hi-lo == 1e-6 exactly; the bracket is (2-lo)/2^k, never exactly 1e-6 |
| `solve_r_from_delta_rho` | lib.rs:1308:72 | > with ==; > with >= | killed | b | `solve_r_from_delta_rho_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1446:5 | whole function -> f64 with -1.0; whole function -> f64 with 0.0; whole function -> f64 with 1.0 | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1446:26 | / with %; / with * | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1446:33 | - with +; - with / | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1450:15 | - with +; - with / | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1450:20 | < with ==; < with > | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1450:20 | < with <= | survives | a | same: bracket (2-lo)/2^k is never exactly 1e-6 |
| `solve_r_from_time_budget` | lib.rs:1451:23 | + with *; + with - | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1451:29 | / with % | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_r_from_time_budget` | lib.rs:1453:14 | < with <=; < with == | killed | b | `solve_r_from_time_budget_* (three tests)` |
| `solve_rho_from_r_delta` | lib.rs:1325:22 | - with + | killed | b | `solve_rho_from_r_delta_* (two tests)` |
| `solve_rho_from_r_delta` | lib.rs:1327:15 | - with +; - with / | killed | b | `solve_rho_from_r_delta_* (two tests)` |
| `solve_rho_from_r_delta` | lib.rs:1327:20 | < with == | killed | b | `solve_rho_from_r_delta_* (two tests)` |
| `solve_rho_from_r_delta` | lib.rs:1327:20 | < with <= | survives | a | same: bracket (hi-0.5)/2^k is never exactly 1e-6 |
| `solve_rho_from_r_delta` | lib.rs:1329:70 | > with ==; > with >= | killed | b | `solve_rho_from_r_delta_* (two tests)` |
| `zeta_of_delta` | lib.rs:169:5 | whole function -> f64 with -1.0; whole function -> f64 with 0.0; whole function -> f64 with 1.0 | killed | b | `zeta_of_delta_at_the_presets` |
| `zeta_of_delta` | lib.rs:169:16 | / with %; / with * | killed | b | `zeta_of_delta_at_the_presets` |
## Findings (class c; code unchanged)

1. `r_saturation`: the `1` in `tail_rec`'s `(1+r)` and in `tail_svc`'s `(1+r)` is constant in r, so it
   cannot move the argmin (936, 937). The argmin is that of `B/(r(1-eps)) + C W r`, about
   sqrt(B/(C W (1-eps))) clamped by the P_fec cliff. The constant `C_DILUTION = 0.5` has no derivation
   in the repository ("c is a constant"); FORMULA-FIRST asks for provenance before it ships.
2. `p_fec_exact` (1130) and `p_fec_exact_repairs` (1144): the same guard three times
   (`p_fec_exact`, the callee, `compute_r_star_exact`). The callee's is unreachable.
3. `p_fec_exact_repairs` 1174: `.min(dmax - 1)` never binds (at most W source losses).
4. `r_star_mass` 511: `.min(MASS_SCALES - 1)` never binds (x < MASS_SCALES). 519: the early return at
   the ceiling is redundant with the scan and bisection, which also return 2.0.
5. `inner_feedback_floor` 648: the infeasible early return is redundant with the bisection (hi stays r_cap).
6. `controller_rate` 791: the `eps_mass > 0` else branch is dead when `mass.is_valid()`.
7. `p_fec_normal` 890: `denominator < 1e-300` is redundant (z = inf gives 1.0); the `eps >= 1` and `r <= 0`
   checks at 886 are covered by the `numerator <= 0` return. Also the known debt: `p_fec_normal`'s cut (W22).

Per CLAUDE.md, none of these clamps has a bind-fraction gauge; items 3, 4 and 5 would report 0 %.

## Evidence

- Targeted runs (mutants restricted to these functions, on the new tests): survivors as above.
- New tests green on unmutated code in `ci` (`math-core`).
- Baseline regenerated by removing exactly the killed entries; the full `mutants` job on this branch
  must end with `new 0, stale 0`.
