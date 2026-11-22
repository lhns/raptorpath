### Noise floors NF (95th percentile over 12 seeds of |A - A'|, largest over the six A/A cells; ms for times)

| metric | NF | | metric | NF (Part T cells) |
|---|---|---|---|---|
| v_p50 | 0.1096 | | v_p50 | 0.2189 |
| v_p99 | 1.488 | | v_p99 | 81.18 |
| v_p999 | 0.4041 | | v_p999 | 81.99 |
| v_mean | 0.1284 | | v_mean | 3.026 |
| e_p50 | 0.1096 | | e_p50 | 0.2189 |
| e_p99 | 1.488 | | e_p99 | 81.18 |
| e_p999 | 0.4041 | | e_p999 | 81.99 |
| e_mean | 0.1284 | | e_mean | 3.026 |
| G_ratio | 0.0002207 | | G_ratio | 0.001832 |
| max_ep_spend | 3 | | max_ep_spend | 9.35 |
| repair_per_delivered | 0.0002217 | | repair_per_delivered | 0.01585 |
| rbf | 0.007502 | | rbf | 0 |
| fp_rel_pd | 2.251e-06 | | fp_rel_pd | 0.0007797 |
| ooo_per_delivered | 0.0002155 | | ooo_per_delivered | 0.001742 |
| | | | tail_time | 85.12 |
| | | | tail_v_mean | 73.22 |
| | | | tail_v_p50 | 80.89 |

### NF_K and NF_P (A/A on the five-point sets)

| preset | readout | NF_K | NF_P |
|---|---|---|---|
| Auto | e_mean | 0.001195 | 0.0004607 |
| Auto | e_p99 | 0.04417 | 0.03722 |
| Auto | rbf | 0.008293 | 0.003821 |
| Auto | G_ratio | 0 | 0 |
| Auto | repair_per_delivered | 0 | 0 |
| Realtime | e_mean | 7.112e-08 | 4.575e-08 |
| Realtime | e_p99 | 0.0001885 | 9.456e-05 |
| Realtime | rbf | 0 | 0 |
| Realtime | G_ratio | 0 | 0 |
| Realtime | repair_per_delivered | 0 | 0 |
| Bulk (max of the two) | e_mean | 0.001195 | 0.0004607 |
| Bulk (max of the two) | e_p99 | 0.04417 | 0.03722 |
| Bulk (max of the two) | rbf | 0.008293 | 0.003821 |
| Bulk (max of the two) | G_ratio | 0 | 0 |
| Bulk (max of the two) | repair_per_delivered | 0 | 0 |

### A/A self-check (mean A/A difference against T)

95 readings equivalent, 16 undetermined, 7 shown.
- undetermined: I1-AU-rbf: -0.001473 [-0.003546, 0.0005946] vs m=0.000625: undetermined
- undetermined: I1-AU-ooo_per_delivered: -7.083e-06 [-1.708e-05, 2.917e-06] vs m=5e-06: undetermined
- undetermined: I2-AU-ooo_per_delivered: 2.083e-06 [-2.917e-06, 8.333e-06] vs m=5e-06: undetermined
- undetermined: G-RT-ooo_per_delivered: 1.958e-05 [-4.875e-05, 8.916e-05] vs m=5e-06: undetermined
- undetermined: G-AU-rbf: 0.0004045 [-0.0005722, 0.00138] vs m=0.000625: undetermined
- undetermined: G-AU-ooo_per_delivered: 1.042e-05 [-1.375e-05, 3.417e-05] vs m=5e-06: undetermined
- undetermined: T-R0e-v_mean: -0.261 [-1.132, 0.5597] vs m=1: undetermined
- undetermined: T-R0e-e_mean: -0.261 [-1.132, 0.5597] vs m=1: undetermined
- undetermined: T-R0e-max_ep_spend: -0.25 [-3.167, 2.25] vs m=1: undetermined
- undetermined: T-R1-v_p99: -12.12 [-33.83, 9.223] vs m=1: undetermined
- undetermined: T-R1-v_p999: -12.75 [-34.8, 9.177] vs m=1: undetermined
- undetermined: T-R1-e_p99: -12.12 [-33.83, 9.223] vs m=1: undetermined
- undetermined: T-R1-e_p999: -12.75 [-34.8, 9.177] vs m=1: undetermined
- undetermined: T-R1-tail_time: -10.41 [-33.27, 11.48] vs m=1: undetermined
- undetermined: T-R1-tail_v_mean: -10.77 [-30.5, 9.337] vs m=1: undetermined
- undetermined: T-R1-tail_v_p50: -12.07 [-33.74, 9.178] vs m=1: undetermined
- SHOWN: T-R0e-v_p99: -9.106 [-13.21, -4.198] vs m=1: shown smaller
- SHOWN: T-R0e-v_p999: -9.335 [-13.6, -4.306] vs m=1: shown smaller
- SHOWN: T-R0e-e_p99: -9.106 [-13.21, -4.198] vs m=1: shown smaller
- SHOWN: T-R0e-e_p999: -9.335 [-13.6, -4.306] vs m=1: shown smaller
- SHOWN: T-R0e-tail_time: -7.538 [-12.32, -2.257] vs m=1: shown smaller
- SHOWN: T-R0e-tail_v_mean: -9.113 [-13.28, -4.131] vs m=1: shown smaller
- SHOWN: T-R0e-tail_v_p50: -9.057 [-13.21, -4.083] vs m=1: shown smaller

### P4j: U against U*, d frozen at RTprop, I1 (12 seeds each)

| omega | preset | isolated holes (mean) | worst slack over seeds (<= 1e-9 required) | ratio_U isolated (mean) | ratio_U all holes (mean) | pass |
|---|---|---|---|---|---|---|
| 1 | Bulk | 308 | -1.61 | 1.000 | 1.000 | pass |
| 1 | Auto | 308 | -159 | 1.023 | 1.047 | pass |
| 1 | Realtime | 315 | -2.62e-10 | 2.023 | 2.017 | pass |
| 10 | Bulk | 308 | -1.61 | 1.000 | 1.000 | pass |
| 10 | Auto | 308 | -161 | 1.000 | 1.000 | pass |
| 10 | Realtime | 315 | 5.82e-11 | 2.023 | 1.976 | pass |

### P4h bridge to the stage 1 toy (I1, N = 1, Realtime, J = 0, 20 s, 12 seeds; stage 1 at commit with floor_with_copy = False)

- S0: seeds with identical n_src 12/12, delivered 12/12, n_holes 12/12, hol_sum_nonhole 12/12, iso_p50 12/12, G_final 12/12, max_ep_spend 12/12; verdict count equal in 12 of 12 seeds; floors sent stage 1 0 (of which refresh 0.0) against stage 2 0 (of which repeats 0.0), mean per run; hol p99 stage 1 13.967 ms against e p99 stage 2 14.125 ms
- S1c: seeds with identical n_src 12/12, delivered 12/12, n_holes 12/12, hol_sum_nonhole 4/12, iso_p50 12/12, G_final 12/12, max_ep_spend 12/12; verdict count equal in 12 of 12 seeds; floors sent stage 1 940 (of which refresh 0.3) against stage 2 1276 (of which repeats 74.5), mean per run; hol p99 stage 1 11.117 ms against e p99 stage 2 11.225 ms

### Wall time per run (s, mean)

- AA- 2.79 s over 144 runs
- AAT- 0.30 s over 48 runs
- KP- 2.54 s over 240 runs
- P4J- 2.53 s over 72 runs
- BR- 4.53 s over 24 runs
- REF- 2.63 s over 36 runs
