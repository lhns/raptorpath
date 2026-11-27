# RTT sample from a never-arrived seq in the offline simulation: count and fix

Code: `tools/toy/rttbug.py` (subclass `CSim` of `toy2.Sim`; `toy2.py` untouched; counter-only mode verified bit-identical to the plain `Sim` on 4 arms, all keys), `tools/toy/rttbug_diag.py`, aggregation `agg.py`. Worktree of `toy-stage3-knobs` e17b4c64. Nothing committed.

## 1. Confirmation from the code

- `toy2.py:972-981` `apply_floor`: `if f - 1 > self.rhi: for q in range(self.rhi + 1, f): self.new_hole(q, now, True)` then `self.rhi = f - 1`. No arrival involved: `got[f-1]` stays 0.
- `toy2.py:1142` the receiver report carries `self.rhi` (field 3).
- `toy2.py:1169-1171` `on_report`: `if rhi > self.rhi_known: if self.paths is None and self.cfg.rtt_est == "rfc" and self.copies[rhi] == 0: self.rtt_sample(now - self.sent_t[rhi])`. The only guard is Karn (`copies == 0`), so a seq the floor passed with no arrival is sampled: `now - sent_t` is then the age of a unit that was lost or is still queued.
- Confirmed. Scope: one-path arms with `rtt_est="rfc"` (every S5 arm, `@b` cells, CL/OV/K cells via LINK_KW). The two-path branch (`toy2.py:1163-1168`) samples `rep[8][p]` = `rhi_path`, set only in `on_data`, so P2 cannot be affected (counted: 0 / 14 601 per 12 seeds). The old-RTT branch (`toy2.py:1156`, sample on `A - 1`, used by arms C1 and by cells without rfc) could in principle hit a floor-passed seq too; counted separately: 0 / 18 875.
- Definition of "never arrived" used: `got[rhi] == 0` or `arr[rhi] > report creation time` when the report is built. In every hit (count run and fix runs) `got[rhi]` was still 0 at the sample (never arrived by then; `rfc_bad_noarr_ever == rfc_bad` in every run).
- Mechanism seen in CL-long, T = 0.4 s, seed 0 (diag): outage 2.0 to 3.5 s; floor jumps to 20 994 after the outage; the report made at 3.5127 s carries rhi = 20 993, sent at 2.0993 s, never arrived; sample = 1.418 s; srtt 10.87 ms -> 186.8 ms (RFC 6298 first-order jump, a 17x step), which feeds R_now, the trigger delay and the sweep period. In OV-long (T = 0.4) the bad samples are 0.100 s on srtt 0.0999 s (no visible step).

## 2. Affected samples per cell and arm (seed 0, every batch3d configuration)

443 configurations run, one seed (seed 0) each: all batch3d configurations except the NT-/NH- nudge variants of the four long cells (CL-long, OV-long, CL-long@b, OV-long@b; 32 skipped; the unnudged arms of those cells are counted). Total rfc samples with a never-arrived seq: **656 of 1848237 (0.035 %)**; configurations with at least one: 68 of 443. old-branch (A-1) samples: 0 of 18 875; two-path samples: 0 of 19 486. (2 of 443 runs did not finish, the known S5v ablation.)

Per cell, summed over its arms (arms listed with non-zero counts):

| cell | arms run | affected / total rfc samples | arms with affected > 0 (affected / total) |
|---|---|---|---|
| CL-long | 11 | 3 / 101516 | G-T0p4-H0 1/9244; G-T0p4-Hinf 1/9244; G-T0p4-H0p02 1/9244 |
| OV-long | 14 | 10 / 312301 | G-T0p4-H0 2/20188; G-T0p4-Hinf 2/20188; G-T2-H0 2/20985; G-T2-Hinf 2/20985; G-T0p4-H0p02 2/20188 |
| CL-long@b | 11 | 3 / 101390 | G-T0p4-H0 1/9223; G-T0p4-Hinf 1/9223; G-T0p4-H0p02 1/9223 |
| OV-long@b | 14 | 10 / 307845 | G-T0p4-H0 2/20192; G-T0p4-Hinf 2/20192; G-T2-H0 2/20993; G-T2-Hinf 2/20993; G-T0p4-H0p02 2/20192 |
| CL | 19 | 11 / 81861 | G-T0p4-H0 1/4244; G-T0p4-Hinf 1/4244; G-T0p4-H0p02 1/4244; NT-0p98 1/4244; NH-0p98 1/4244; NT-0p99 1/4244; NH-0p99 1/4244; NT-1p01 1/4244; NH-1p01 1/4244; NT-1p02 1/4244; NH-1p02 1/4244 |
| OV | 38 | 7 / 207007 | NT-0p98 1/5183; NT-0p99 2/5184; PL-SPIKE-0p98 1/5183; PL-SPIKE-0p99 2/5184; PL-KINK-0p99 1/5234 |
| CL@b | 19 | 11 / 80945 | G-T0p4-H0 1/4223; G-T0p4-Hinf 1/4223; G-T0p4-H0p02 1/4223; NT-0p98 1/4223; NH-0p98 1/4223; NT-0p99 1/4223; NH-0p99 1/4223; NT-1p01 1/4223; NH-1p01 1/4223; NT-1p02 1/4235; NH-1p02 1/4223 |
| OV@b | 23 | 1 / 127199 | NT-0p98 1/5188 |
| E-I2 | 19 | 0 / 10047 | none |
| E-L10 | 19 | 0 / 10047 | none |
| M-I2 | 19 | 0 / 5726 | none |
| M-L10 | 19 | 0 / 5756 | none |
| K-I2 | 20 | 0 / 51174 | none |
| K-L10 | 49 | 65 / 123957 | SW-0p15 65/2701 |
| K-L10b | 34 | 0 / 67893 | none |
| K-I2@b | 19 | 0 / 54017 | none |
| K-L10@b | 19 | 1 / 45404 | NT-0p98 1/2394 |
| K-L10b@b | 19 | 11 / 45589 | G-T0p4-H0 1/2402; G-T0p4-Hinf 1/2402; G-T0p4-H0p02 1/2402; NT-0p98 1/2402; NH-0p98 1/2402; NT-0p99 1/2402; NH-0p99 1/2402; NT-1p01 1/2402; NH-1p01 1/2402; NT-1p02 1/2401; NH-1p02 1/2402 |
| X15 | 21 | 452 / 54472 | SW-0p04 89/2790; SW-0p042 89/2790; SW-0p044 89/2790; SW-0p046 89/2790; SW-0p048 89/2790; SW-0p05 7/2708 |
| X20 | 21 | 71 / 54091 | SW-0p055 17/2718; SW-0p057 17/2718; SW-0p059 17/2718; SW-0p061 17/2718; SW-0p063 3/2704 |
| P2 | 16 | 0 / 0 | none |

Full per-arm table (affected / total rfc samples; the old-branch and two-path counts are 0 everywhere):

| cell.arm | affected / total |
|---|---|
| CL-long.C1 | 0 / 9243 |
| OV-long.C1 | 0 / 26008 |
| CL-long@b.C1 | 0 / 9228 |
| OV-long@b.C1 | 0 / 25271 |
| OV-long.SUP-S5v-H0 | 0 / 20186 |
| OV-long.SUP-S5v-Hinf | 0 / 20186 |
| OV-long@b.SUP-S5v-H0 | 0 / 20193 |
| OV-long@b.SUP-S5v-Hinf | 0 / 20193 |
| CL-long.C3S | 0 / 9220 |
| CL-long.ID-1e6 | 0 / 9220 |
| CL-long.AA-1 | 0 / 9221 |
| CL-long.C3 | 0 / 9220 |
| CL-long.G-T0p4-H0 | 1 / 9244 |
| CL-long.G-T0p4-Hinf | 1 / 9244 |
| CL-long.G-T2-H0 | 0 / 9220 |
| CL-long.G-T2-Hinf | 0 / 9220 |
| CL-long.G-Tinf-H0 | 0 / 9220 |
| CL-long.G-T0p4-H0p02 | 1 / 9244 |
| OV-long.C3S | 0 / 25760 |
| OV-long.ID-1e6 | 0 / 25760 |
| OV-long.AA-1 | 0 / 25735 |
| OV-long.C3 | 0 / 25760 |
| OV-long.G-T0p4-H0 | 2 / 20188 |
| OV-long.G-T0p4-Hinf | 2 / 20188 |
| OV-long.G-T2-H0 | 2 / 20985 |
| OV-long.G-T2-Hinf | 2 / 20985 |
| OV-long.S5v-H0 | 0 / 20186 |
| OV-long.S5v-Hinf | 0 / 20186 |
| OV-long.G-T0p4-H0p02 | 2 / 20188 |
| CL-long@b.C3S | 0 / 9212 |
| CL-long@b.ID-1e6 | 0 / 9212 |
| CL-long@b.AA-1 | 0 / 9221 |
| CL-long@b.C3 | 0 / 9212 |
| CL-long@b.G-T0p4-H0 | 1 / 9223 |
| CL-long@b.G-T0p4-Hinf | 1 / 9223 |
| CL-long@b.G-T2-H0 | 0 / 9212 |
| CL-long@b.G-T2-Hinf | 0 / 9212 |
| CL-long@b.G-Tinf-H0 | 0 / 9212 |
| CL-long@b.G-T0p4-H0p02 | 1 / 9223 |
| OV-long@b.C3S | 0 / 24810 |
| OV-long@b.ID-1e6 | 0 / 24810 |
| OV-long@b.AA-1 | 0 / 24810 |
| OV-long@b.C3 | 0 / 24810 |
| OV-long@b.G-T0p4-H0 | 2 / 20192 |
| OV-long@b.G-T0p4-Hinf | 2 / 20192 |
| OV-long@b.G-T2-H0 | 2 / 20993 |
| OV-long@b.G-T2-Hinf | 2 / 20993 |
| OV-long@b.S5v-H0 | 0 / 20193 |
| OV-long@b.S5v-Hinf | 0 / 20193 |
| OV-long@b.G-T0p4-H0p02 | 2 / 20192 |
| CL.C1 | 0 / 4352 |
| OV.C1 | 0 / 6499 |
| CL@b.C1 | 0 / 4255 |
| OV@b.C1 | 0 / 6314 |
| CL.C3 | 0 / 4404 |
| CL.C3S | 0 / 4404 |
| CL.ID-1e6 | 0 / 4404 |
| CL.AA-1 | 0 / 4403 |
| CL.G-T0p4-H0 | 1 / 4244 |
| CL.G-T0p4-Hinf | 1 / 4244 |
| CL.G-T2-H0 | 0 / 4403 |
| CL.G-T2-Hinf | 0 / 4403 |
| CL.G-Tinf-H0 | 0 / 4404 |
| CL.G-T0p4-H0p02 | 1 / 4244 |
| CL.NT-0p98 | 1 / 4244 |
| CL.NH-0p98 | 1 / 4244 |
| CL.NT-0p99 | 1 / 4244 |
| CL.NH-0p99 | 1 / 4244 |
| CL.NT-1p01 | 1 / 4244 |
| CL.NH-1p01 | 1 / 4244 |
| CL.NT-1p02 | 1 / 4244 |
| CL.NH-1p02 | 1 / 4244 |
| OV.C3 | 0 / 6438 |
| OV.C3S | 0 / 6438 |
| OV.ID-1e6 | 0 / 6438 |
| OV.AA-1 | 0 / 6432 |
| OV.G-T0p4-H0 | 0 / 5188 |
| OV.G-T0p4-Hinf | 0 / 5188 |
| OV.G-T2-H0 | 0 / 5985 |
| OV.G-T2-Hinf | 0 / 5985 |
| OV.G-Tinf-H0 | 0 / 6438 |
| OV.G-T0p4-H0p02 | 0 / 5188 |
| OV.NT-0p98 | 1 / 5183 |
| OV.NH-0p98 | 0 / 5188 |
| OV.NT-0p99 | 2 / 5184 |
| OV.NH-0p99 | 0 / 5188 |
| OV.NT-1p01 | 0 / 5190 |
| OV.NH-1p01 | 0 / 5188 |
| OV.NT-1p02 | 0 / 5193 |
| OV.NH-1p02 | 0 / 5188 |
| CL@b.C3 | 0 / 4319 |
| CL@b.C3S | 0 / 4319 |
| CL@b.ID-1e6 | 0 / 4319 |
| CL@b.AA-1 | 0 / 4321 |
| CL@b.G-T0p4-H0 | 1 / 4223 |
| CL@b.G-T0p4-Hinf | 1 / 4223 |
| CL@b.G-T2-H0 | 0 / 4314 |
| CL@b.G-T2-Hinf | 0 / 4314 |
| CL@b.G-Tinf-H0 | 0 / 4319 |
| CL@b.G-T0p4-H0p02 | 1 / 4223 |
| CL@b.NT-0p98 | 1 / 4223 |
| CL@b.NH-0p98 | 1 / 4223 |
| CL@b.NT-0p99 | 1 / 4223 |
| CL@b.NH-0p99 | 1 / 4223 |
| CL@b.NT-1p01 | 1 / 4223 |
| CL@b.NH-1p01 | 1 / 4223 |
| CL@b.NT-1p02 | 1 / 4235 |
| CL@b.NH-1p02 | 1 / 4223 |
| OV@b.C3 | 0 / 6202 |
| OV@b.C3S | 0 / 6202 |
| OV@b.ID-1e6 | 0 / 6202 |
| OV@b.AA-1 | 0 / 6202 |
| OV@b.G-T0p4-H0 | 0 / 5193 |
| OV@b.G-T0p4-Hinf | 0 / 5193 |
| OV@b.G-T2-H0 | 0 / 5992 |
| OV@b.G-T2-Hinf | 0 / 5992 |
| OV@b.G-Tinf-H0 | 0 / 6202 |
| OV@b.G-T0p4-H0p02 | 0 / 5193 |
| OV@b.NT-0p98 | 1 / 5188 |
| OV@b.NH-0p98 | 0 / 5193 |
| OV@b.NT-0p99 | 0 / 5190 |
| OV@b.NH-0p99 | 0 / 5193 |
| OV@b.NT-1p01 | 0 / 5194 |
| OV@b.NH-1p01 | 0 / 5193 |
| OV@b.NT-1p02 | 0 / 5196 |
| OV@b.NH-1p02 | 0 / 5193 |
| OV.S5v-H0 | 0 / 5186 |
| OV.S5v-Hinf | 0 / 5186 |
| OV@b.S5v-H0 | 0 / 5193 |
| OV@b.S5v-Hinf | 0 / 5193 |
| OV.PL-STEP-0p98 | 0 / 5374 |
| OV.PL-STEP-0p99 | 0 / 5380 |
| OV.PL-STEP-1 | 0 / 5188 |
| OV.PL-STEP-1p01 | 0 / 5190 |
| OV.PL-STEP-1p02 | 0 / 5193 |
| OV.PL-SPIKE-0p98 | 1 / 5183 |
| OV.PL-SPIKE-0p99 | 2 / 5184 |
| OV.PL-SPIKE-1 | 0 / 5385 |
| OV.PL-SPIKE-1p01 | 0 / 5190 |
| OV.PL-SPIKE-1p02 | 0 / 5193 |
| OV.PL-KINK-0p98 | 0 / 5279 |
| OV.PL-KINK-0p99 | 1 / 5234 |
| OV.PL-KINK-1 | 0 / 5188 |
| OV.PL-KINK-1p01 | 0 / 5190 |
| OV.PL-KINK-1p02 | 0 / 5193 |
| OV.SUP-S5v-H0 | 0 / 5186 |
| OV.SUP-S5v-Hinf | 0 / 5186 |
| OV@b.SUP-S5v-H0 | 0 / 5193 |
| OV@b.SUP-S5v-Hinf | 0 / 5193 |
| E-I2.C1 | 0 / 0 |
| E-L10.C1 | 0 / 0 |
| M-I2.C1 | 0 / 0 |
| M-L10.C1 | 0 / 0 |
| K-I2.C1 | 0 / 0 |
| K-L10.C1 | 0 / 0 |
| K-L10b.C1 | 0 / 0 |
| K-I2@b.C1 | 0 / 2843 |
| K-L10@b.C1 | 0 / 2331 |
| K-L10b@b.C1 | 0 / 2348 |
| E-I2.C3 | 0 / 0 |
| E-I2.C3S | 0 / 591 |
| E-I2.ID-1e6 | 0 / 591 |
| E-I2.AA-1 | 0 / 591 |
| E-I2.G-T0p4-H0 | 0 / 591 |
| E-I2.G-T0p4-Hinf | 0 / 591 |
| E-I2.G-T2-H0 | 0 / 591 |
| E-I2.G-T2-Hinf | 0 / 591 |
| E-I2.G-Tinf-H0 | 0 / 591 |
| E-I2.G-T0p4-H0p02 | 0 / 591 |
| E-I2.NT-0p98 | 0 / 591 |
| E-I2.NH-0p98 | 0 / 591 |
| E-I2.NT-0p99 | 0 / 591 |
| E-I2.NH-0p99 | 0 / 591 |
| E-I2.NT-1p01 | 0 / 591 |
| E-I2.NH-1p01 | 0 / 591 |
| E-I2.NT-1p02 | 0 / 591 |
| E-I2.NH-1p02 | 0 / 591 |
| E-L10.C3 | 0 / 0 |
| E-L10.C3S | 0 / 591 |
| E-L10.ID-1e6 | 0 / 591 |
| E-L10.AA-1 | 0 / 591 |
| E-L10.G-T0p4-H0 | 0 / 591 |
| E-L10.G-T0p4-Hinf | 0 / 591 |
| E-L10.G-T2-H0 | 0 / 591 |
| E-L10.G-T2-Hinf | 0 / 591 |
| E-L10.G-Tinf-H0 | 0 / 591 |
| E-L10.G-T0p4-H0p02 | 0 / 591 |
| E-L10.NT-0p98 | 0 / 591 |
| E-L10.NH-0p98 | 0 / 591 |
| E-L10.NT-0p99 | 0 / 591 |
| E-L10.NH-0p99 | 0 / 591 |
| E-L10.NT-1p01 | 0 / 591 |
| E-L10.NH-1p01 | 0 / 591 |
| E-L10.NT-1p02 | 0 / 591 |
| E-L10.NH-1p02 | 0 / 591 |
| M-I2.C3 | 0 / 0 |
| M-I2.C3S | 0 / 507 |
| M-I2.ID-1e6 | 0 / 507 |
| M-I2.AA-1 | 0 / 507 |
| M-I2.G-T0p4-H0 | 0 / 244 |
| M-I2.G-T0p4-Hinf | 0 / 244 |
| M-I2.G-T2-H0 | 0 / 507 |
| M-I2.G-T2-Hinf | 0 / 507 |
| M-I2.G-Tinf-H0 | 0 / 507 |
| M-I2.G-T0p4-H0p02 | 0 / 244 |
| M-I2.NT-0p98 | 0 / 244 |
| M-I2.NH-0p98 | 0 / 244 |
| M-I2.NT-0p99 | 0 / 244 |
| M-I2.NH-0p99 | 0 / 244 |
| M-I2.NT-1p01 | 0 / 244 |
| M-I2.NH-1p01 | 0 / 244 |
| M-I2.NT-1p02 | 0 / 244 |
| M-I2.NH-1p02 | 0 / 244 |
| M-L10.C3 | 0 / 0 |
| M-L10.C3S | 0 / 512 |
| M-L10.ID-1e6 | 0 / 512 |
| M-L10.AA-1 | 0 / 512 |
| M-L10.G-T0p4-H0 | 0 / 244 |
| M-L10.G-T0p4-Hinf | 0 / 244 |
| M-L10.G-T2-H0 | 0 / 512 |
| M-L10.G-T2-Hinf | 0 / 512 |
| M-L10.G-Tinf-H0 | 0 / 512 |
| M-L10.G-T0p4-H0p02 | 0 / 244 |
| M-L10.NT-0p98 | 0 / 244 |
| M-L10.NH-0p98 | 0 / 244 |
| M-L10.NT-0p99 | 0 / 244 |
| M-L10.NH-0p99 | 0 / 244 |
| M-L10.NT-1p01 | 0 / 244 |
| M-L10.NH-1p01 | 0 / 244 |
| M-L10.NT-1p02 | 0 / 244 |
| M-L10.NH-1p02 | 0 / 244 |
| K-I2.C3 | 0 / 0 |
| K-I2.C3S | 0 / 2843 |
| K-I2.ID-1e6 | 0 / 2843 |
| K-I2.AA-1 | 0 / 2843 |
| K-I2.G-T0p4-H0 | 0 / 2843 |
| K-I2.G-T0p4-Hinf | 0 / 2843 |
| K-I2.G-T2-H0 | 0 / 2843 |
| K-I2.G-T2-Hinf | 0 / 2843 |
| K-I2.G-Tinf-H0 | 0 / 2843 |
| K-I2.G-T0p4-H0p02 | 0 / 2843 |
| K-I2.NT-0p98 | 0 / 2843 |
| K-I2.NH-0p98 | 0 / 2843 |
| K-I2.NT-0p99 | 0 / 2843 |
| K-I2.NH-0p99 | 0 / 2843 |
| K-I2.NT-1p01 | 0 / 2843 |
| K-I2.NH-1p01 | 0 / 2843 |
| K-I2.NT-1p02 | 0 / 2843 |
| K-I2.NH-1p02 | 0 / 2843 |
| K-L10.C3 | 0 / 0 |
| K-L10.C3S | 0 / 2636 |
| K-L10.ID-1e6 | 0 / 2636 |
| K-L10.AA-1 | 0 / 2636 |
| K-L10.G-T0p4-H0 | 0 / 2636 |
| K-L10.G-T0p4-Hinf | 0 / 2636 |
| K-L10.G-T2-H0 | 0 / 2636 |
| K-L10.G-T2-Hinf | 0 / 2636 |
| K-L10.G-Tinf-H0 | 0 / 2636 |
| K-L10.G-T0p4-H0p02 | 0 / 2636 |
| K-L10.NT-0p98 | 0 / 2636 |
| K-L10.NH-0p98 | 0 / 2636 |
| K-L10.NT-0p99 | 0 / 2636 |
| K-L10.NH-0p99 | 0 / 2636 |
| K-L10.NT-1p01 | 0 / 2636 |
| K-L10.NH-1p01 | 0 / 2636 |
| K-L10.NT-1p02 | 0 / 2636 |
| K-L10.NH-1p02 | 0 / 2636 |
| K-L10b.C3 | 0 / 0 |
| K-L10b.C3S | 0 / 2121 |
| K-L10b.ID-1e6 | 0 / 2121 |
| K-L10b.AA-1 | 0 / 2114 |
| K-L10b.G-T0p4-H0 | 0 / 2121 |
| K-L10b.G-T0p4-Hinf | 0 / 2121 |
| K-L10b.G-T2-H0 | 0 / 2121 |
| K-L10b.G-T2-Hinf | 0 / 2121 |
| K-L10b.G-Tinf-H0 | 0 / 2121 |
| K-L10b.G-T0p4-H0p02 | 0 / 2121 |
| K-L10b.NT-0p98 | 0 / 2121 |
| K-L10b.NH-0p98 | 0 / 2121 |
| K-L10b.NT-0p99 | 0 / 2121 |
| K-L10b.NH-0p99 | 0 / 2121 |
| K-L10b.NT-1p01 | 0 / 2121 |
| K-L10b.NH-1p01 | 0 / 2121 |
| K-L10b.NT-1p02 | 0 / 2121 |
| K-L10b.NH-1p02 | 0 / 2121 |
| K-I2@b.C3 | 0 / 2843 |
| K-I2@b.C3S | 0 / 2843 |
| K-I2@b.ID-1e6 | 0 / 2843 |
| K-I2@b.AA-1 | 0 / 2843 |
| K-I2@b.G-T0p4-H0 | 0 / 2843 |
| K-I2@b.G-T0p4-Hinf | 0 / 2843 |
| K-I2@b.G-T2-H0 | 0 / 2843 |
| K-I2@b.G-T2-Hinf | 0 / 2843 |
| K-I2@b.G-Tinf-H0 | 0 / 2843 |
| K-I2@b.G-T0p4-H0p02 | 0 / 2843 |
| K-I2@b.NT-0p98 | 0 / 2843 |
| K-I2@b.NH-0p98 | 0 / 2843 |
| K-I2@b.NT-0p99 | 0 / 2843 |
| K-I2@b.NH-0p99 | 0 / 2843 |
| K-I2@b.NT-1p01 | 0 / 2843 |
| K-I2@b.NH-1p01 | 0 / 2843 |
| K-I2@b.NT-1p02 | 0 / 2843 |
| K-I2@b.NH-1p02 | 0 / 2843 |
| K-L10@b.C3 | 0 / 2393 |
| K-L10@b.C3S | 0 / 2393 |
| K-L10@b.ID-1e6 | 0 / 2393 |
| K-L10@b.AA-1 | 0 / 2391 |
| K-L10@b.G-T0p4-H0 | 0 / 2393 |
| K-L10@b.G-T0p4-Hinf | 0 / 2393 |
| K-L10@b.G-T2-H0 | 0 / 2393 |
| K-L10@b.G-T2-Hinf | 0 / 2393 |
| K-L10@b.G-Tinf-H0 | 0 / 2393 |
| K-L10@b.G-T0p4-H0p02 | 0 / 2393 |
| K-L10@b.NT-0p98 | 1 / 2394 |
| K-L10@b.NH-0p98 | 0 / 2393 |
| K-L10@b.NT-0p99 | 0 / 2393 |
| K-L10@b.NH-0p99 | 0 / 2393 |
| K-L10@b.NT-1p01 | 0 / 2393 |
| K-L10@b.NH-1p01 | 0 / 2393 |
| K-L10@b.NT-1p02 | 0 / 2393 |
| K-L10@b.NH-1p02 | 0 / 2393 |
| K-L10b@b.C3 | 0 / 2403 |
| K-L10b@b.C3S | 0 / 2403 |
| K-L10b@b.ID-1e6 | 0 / 2403 |
| K-L10b@b.AA-1 | 0 / 2402 |
| K-L10b@b.G-T0p4-H0 | 1 / 2402 |
| K-L10b@b.G-T0p4-Hinf | 1 / 2402 |
| K-L10b@b.G-T2-H0 | 0 / 2403 |
| K-L10b@b.G-T2-Hinf | 0 / 2403 |
| K-L10b@b.G-Tinf-H0 | 0 / 2403 |
| K-L10b@b.G-T0p4-H0p02 | 1 / 2402 |
| K-L10b@b.NT-0p98 | 1 / 2402 |
| K-L10b@b.NH-0p98 | 1 / 2402 |
| K-L10b@b.NT-0p99 | 1 / 2402 |
| K-L10b@b.NH-0p99 | 1 / 2402 |
| K-L10b@b.NT-1p01 | 1 / 2402 |
| K-L10b@b.NH-1p01 | 1 / 2402 |
| K-L10b@b.NT-1p02 | 1 / 2401 |
| K-L10b@b.NH-1p02 | 1 / 2402 |
| X15.C3 | 0 / 0 |
| X15.C3S | 0 / 2701 |
| X15.ID-1e6 | 0 / 2701 |
| X15.AA-1 | 0 / 2701 |
| X15.SW-0p04 | 89 / 2790 |
| X15.SW-0p042 | 89 / 2790 |
| X15.SW-0p044 | 89 / 2790 |
| X15.SW-0p046 | 89 / 2790 |
| X15.SW-0p048 | 89 / 2790 |
| X15.SW-0p05 | 7 / 2708 |
| X15.SW-0p052 | 0 / 2701 |
| X15.SW-0p054 | 0 / 2701 |
| X15.SW-0p056 | 0 / 2701 |
| X15.SW-0p058 | 0 / 2701 |
| X15.SW-0p06 | 0 / 2701 |
| X15.SW-0p062 | 0 / 2701 |
| X15.SW-0p064 | 0 / 2701 |
| X15.SW-0p066 | 0 / 2701 |
| X15.SW-0p068 | 0 / 2701 |
| X15.SW-0p07 | 0 / 2701 |
| X15.G-T0p4-H0 | 0 / 2701 |
| X20.C3 | 0 / 0 |
| X20.C3S | 0 / 2701 |
| X20.ID-1e6 | 0 / 2701 |
| X20.AA-1 | 0 / 2701 |
| X20.SW-0p055 | 17 / 2718 |
| X20.SW-0p057 | 17 / 2718 |
| X20.SW-0p059 | 17 / 2718 |
| X20.SW-0p061 | 17 / 2718 |
| X20.SW-0p063 | 3 / 2704 |
| X20.SW-0p065 | 0 / 2701 |
| X20.SW-0p067 | 0 / 2701 |
| X20.SW-0p069 | 0 / 2701 |
| X20.SW-0p071 | 0 / 2701 |
| X20.SW-0p073 | 0 / 2701 |
| X20.SW-0p075 | 0 / 2701 |
| X20.SW-0p077 | 0 / 2701 |
| X20.SW-0p079 | 0 / 2701 |
| X20.SW-0p081 | 0 / 2701 |
| X20.SW-0p083 | 0 / 2701 |
| X20.SW-0p085 | 0 / 2701 |
| X20.G-T0p4-H0 | 0 / 2701 |
| K-L10.SW-0p15 | 65 / 2701 |
| K-L10.SW-0p155 | 0 / 2636 |
| K-L10.SW-0p16 | 0 / 2636 |
| K-L10.SW-0p165 | 0 / 2636 |
| K-L10.SW-0p17 | 0 / 2636 |
| K-L10.SW-0p175 | 0 / 2636 |
| K-L10.SW-0p18 | 0 / 2636 |
| K-L10.SW-0p185 | 0 / 2636 |
| K-L10.SW-0p19 | 0 / 2636 |
| K-L10.SW-0p195 | 0 / 2636 |
| K-L10.SW-0p2 | 0 / 2636 |
| K-L10.SW-0p205 | 0 / 2636 |
| K-L10.SW-0p21 | 0 / 2636 |
| K-L10.SW-0p215 | 0 / 2636 |
| K-L10b.SW-0p18 | 0 / 2123 |
| K-L10b.SW-0p185 | 0 / 2123 |
| K-L10b.SW-0p19 | 0 / 2123 |
| K-L10b.SW-0p195 | 0 / 2123 |
| K-L10b.SW-0p2 | 0 / 2123 |
| K-L10b.SW-0p205 | 0 / 2123 |
| K-L10b.SW-0p21 | 0 / 2123 |
| K-L10b.SW-0p215 | 0 / 2123 |
| K-L10b.SW-0p22 | 0 / 2123 |
| K-L10b.SW-0p225 | 0 / 2123 |
| K-L10b.SW-0p23 | 0 / 2123 |
| K-L10b.SW-0p235 | 0 / 2123 |
| K-L10b.SW-0p24 | 0 / 2123 |
| K-L10b.SW-0p245 | 0 / 2123 |
| K-L10.PL-STEP-0p98 | 0 / 2636 |
| K-L10.PL-STEP-0p99 | 0 / 2636 |
| K-L10.PL-STEP-1 | 0 / 2636 |
| K-L10.PL-STEP-1p01 | 0 / 2636 |
| K-L10.PL-STEP-1p02 | 0 / 2636 |
| K-L10.PL-SPIKE-0p98 | 0 / 2636 |
| K-L10.PL-SPIKE-0p99 | 0 / 2636 |
| K-L10.PL-SPIKE-1 | 0 / 2636 |
| K-L10.PL-SPIKE-1p01 | 0 / 2636 |
| K-L10.PL-SPIKE-1p02 | 0 / 2636 |
| K-L10.PL-KINK-0p98 | 0 / 2636 |
| K-L10.PL-KINK-0p99 | 0 / 2636 |
| K-L10.PL-KINK-1 | 0 / 2636 |
| K-L10.PL-KINK-1p01 | 0 / 2636 |
| K-L10.PL-KINK-1p02 | 0 / 2636 |
| K-I2.S4ref | 0 / 2843 |
| K-L10.S4ref | 0 / 2636 |
| K-L10b.S4ref | 0 / 2121 |
| P2.C3 | 0 / 0 |
| P2.C3S | 0 / 0 |
| P2.ID-1e6 | 0 / 0 |
| P2.AA-1 | 0 / 0 |
| P2.G-T0p075-H0-Fest | 0 / 0 |
| P2.G-T0p075-H0-F0 | 0 / 0 |
| P2.G-T0p075-Hinf-Fest | 0 / 0 |
| P2.G-T0p075-Hinf-F0 | 0 / 0 |
| P2.G-T0p4-H0-Fest | 0 / 0 |
| P2.G-T0p4-H0-F0 | 0 / 0 |
| P2.G-T0p4-Hinf-Fest | 0 / 0 |
| P2.G-T0p4-Hinf-F0 | 0 / 0 |
| P2.G-Tinf-H0-Fest | 0 / 0 |
| P2.G-Tinf-H0-F0 | 0 / 0 |
| P2.G-Tinf-Hinf-Fest | 0 / 0 |
| P2.G-Tinf-Hinf-F0 | 0 / 0 |

## 3. The fix and its effect (12 seeds, same seeds, unfixed vs fixed)

Fix (`CSim(fix=True)`): when the report's rhi had not arrived by the time the report was made, skip the RTT sample; `rhi_known` still advances. Nothing else changes. Flag defaults off; `toy2.py` is not modified. A variant that substitutes the highest actually-arrived seq was not run. Arms: CL-long never give up any order (`G-Tinf-H0`) and strict (`C3S`, = S5 at T = inf, H = inf); lifetime 2 s any order and strict; lifetime 0.4 s at H = 0 and H = inf; OV and OV-long: every S5 arm (`C3S`, T 0.4 / 2 / inf where defined, H 0 / inf; OV-long has no T = inf arm); P2: T = 75 ms, H 0 and inf, floor hold est and 0. Shares are percent of units handed over; recovery time in seconds (n/a = the cell has no recovery time, the arm never returns or the cell has no outage window); srtt, o_hat (o_fwd = srtt - RTprop/2) and delay p50 are end-of-run / run values in ms. Delta = fixed minus unfixed, paired by seed.

### Affected samples per arm (12 seeds, unfixed run; bad = RTT sample on a seq that had not arrived when the report was made)

| cell.arm | seeds | bad / samples | seeds with bad > 0 | seeds where any readout differs after the fix |
|---|---|---|---|---|
| CL-long.G-Tinf-H0 | 12 | 0 / 110642 | 0 | 0 |
| CL-long.C3S | 12 | 0 / 110642 | 0 | 0 |
| CL-long.G-T2-H0 | 12 | 0 / 110642 | 0 | 0 |
| CL-long.G-T2-Hinf | 12 | 0 / 110642 | 0 | 0 |
| CL-long.G-T0p4-H0 | 12 | 12 / 110895 | 12 | 12 |
| CL-long.G-T0p4-Hinf | 12 | 12 / 110895 | 12 | 12 |
| OV.C3S | 12 | 0 / 77172 | 0 | 0 |
| OV.G-T0p4-H0 | 12 | 5 / 62242 | 4 | 3 |
| OV.G-T0p4-Hinf | 12 | 5 / 62242 | 4 | 3 |
| OV.G-T2-H0 | 12 | 0 / 71841 | 0 | 0 |
| OV.G-T2-Hinf | 12 | 0 / 71841 | 0 | 0 |
| OV.G-Tinf-H0 | 12 | 0 / 77172 | 0 | 0 |
| OV-long.C3S | 12 | 0 / 308736 | 0 | 0 |
| OV-long.G-T0p4-H0 | 12 | 25 / 242242 | 11 | 7 |
| OV-long.G-T0p4-Hinf | 12 | 25 / 242242 | 11 | 7 |
| OV-long.G-T2-H0 | 12 | 14 / 251843 | 11 | 7 |
| OV-long.G-T2-Hinf | 12 | 14 / 251843 | 11 | 7 |
| P2.G-T0p075-H0-Fest | 12 | 0 / 14601 | 0 | 0 |
| P2.G-T0p075-H0-F0 | 12 | 0 / 14601 | 0 | 0 |
| P2.G-T0p075-Hinf-Fest | 12 | 0 / 14601 | 0 | 0 |
| P2.G-T0p075-Hinf-F0 | 12 | 0 / 14601 | 0 | 0 |

### share_any (mean over seeds, unfixed -> fixed; paired delta mean [min, max]; units %)

| cell.arm | unfixed | fixed | delta mean [min, max] | n numeric |
|---|---|---|---|---|
| CL-long.G-Tinf-H0 | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.C3S | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-H0 | 99.936 | 99.936 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-Hinf | 99.936 | 99.936 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T0p4-H0 | 92.381 | 94.144 | +1.763 [+1.750, +1.776] | 12 of 12 |
| CL-long.G-T0p4-Hinf | 92.381 | 94.144 | +1.763 [+1.750, +1.776] | 12 of 12 |
| OV.C3S | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-H0 | 83.093 | 83.093 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-Hinf | 83.093 | 83.093 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T2-H0 | 93.970 | 93.970 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T2-Hinf | 93.970 | 93.970 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-Tinf-H0 | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.C3S | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-H0 | 81.152 | 81.152 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-Hinf | 81.152 | 81.152 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-H0 | 83.872 | 83.872 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-Hinf | 83.872 | 83.872 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-Fest | 96.979 | 96.979 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-F0 | 96.979 | 96.979 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-Fest | 96.979 | 96.979 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-F0 | 96.979 | 96.979 | +0.000 [+0.000, +0.000] | 12 of 12 |

### share_T (mean over seeds, unfixed -> fixed; paired delta mean [min, max]; units %)

| cell.arm | unfixed | fixed | delta mean [min, max] | n numeric |
|---|---|---|---|---|
| CL-long.G-Tinf-H0 | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.C3S | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-H0 | 99.919 | 99.919 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-Hinf | 99.831 | 99.831 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T0p4-H0 | 92.381 | 94.143 | +1.762 [+1.750, +1.776] | 12 of 12 |
| CL-long.G-T0p4-Hinf | 92.326 | 93.267 | +0.941 [+0.881, +1.014] | 12 of 12 |
| OV.C3S | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-H0 | 83.092 | 83.092 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-Hinf | 82.828 | 82.828 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T2-H0 | 93.969 | 93.969 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T2-Hinf | 93.688 | 93.688 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-Tinf-H0 | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.C3S | 100.000 | 100.000 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-H0 | 81.152 | 81.152 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-Hinf | 81.086 | 81.086 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-H0 | 83.872 | 83.872 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-Hinf | 83.801 | 83.801 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-Fest | 96.979 | 96.979 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-F0 | 96.979 | 96.979 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-Fest | 2.033 | 2.033 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-F0 | 91.494 | 91.494 | +0.000 [+0.000, +0.000] | 12 of 12 |

### recovery_time (mean over seeds, unfixed -> fixed; paired delta mean [min, max]; units s)

| cell.arm | unfixed | fixed | delta mean [min, max] | n numeric |
|---|---|---|---|---|
| CL-long.G-Tinf-H0 | 6.66 | 6.66 | +0.00 [+0.00, +0.00] | 12 of 12 |
| CL-long.C3S | 7.79 | 7.79 | +0.00 [+0.00, +0.00] | 12 of 12 |
| CL-long.G-T2-H0 | 6.61 | 6.61 | +0.00 [+0.00, +0.00] | 12 of 12 |
| CL-long.G-T2-Hinf | 7.78 | 7.78 | +0.00 [+0.00, +0.00] | 12 of 12 |
| CL-long.G-T0p4-H0 | 0.10 | 0.00 | -0.10 [-0.10, -0.10] | 12 of 12 |
| CL-long.G-T0p4-Hinf | 0.10 | 0.00 | -0.10 [-0.10, -0.10] | 12 of 12 |
| OV.C3S | n/a | n/a | n/a | 0 of 12 |
| OV.G-T0p4-H0 | n/a | n/a | n/a | 0 of 12 |
| OV.G-T0p4-Hinf | n/a | n/a | n/a | 0 of 12 |
| OV.G-T2-H0 | n/a | n/a | n/a | 0 of 12 |
| OV.G-T2-Hinf | n/a | n/a | n/a | 0 of 12 |
| OV.G-Tinf-H0 | n/a | n/a | n/a | 0 of 12 |
| OV-long.C3S | n/a | n/a | n/a | 0 of 12 |
| OV-long.G-T0p4-H0 | n/a | n/a | n/a | 0 of 12 |
| OV-long.G-T0p4-Hinf | n/a | n/a | n/a | 0 of 12 |
| OV-long.G-T2-H0 | n/a | n/a | n/a | 0 of 12 |
| OV-long.G-T2-Hinf | n/a | n/a | n/a | 0 of 12 |
| P2.G-T0p075-H0-Fest | n/a | n/a | n/a | 0 of 12 |
| P2.G-T0p075-H0-F0 | n/a | n/a | n/a | 0 of 12 |
| P2.G-T0p075-Hinf-Fest | n/a | n/a | n/a | 0 of 12 |
| P2.G-T0p075-Hinf-F0 | n/a | n/a | n/a | 0 of 12 |

### srtt (mean over seeds, unfixed -> fixed; paired delta mean [min, max]; units ms)

| cell.arm | unfixed | fixed | delta mean [min, max] | n numeric |
|---|---|---|---|---|
| CL-long.G-Tinf-H0 | 10.765 | 10.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.C3S | 10.765 | 10.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-H0 | 10.765 | 10.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-Hinf | 10.765 | 10.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T0p4-H0 | 10.767 | 10.749 | -0.017 [-0.223, +0.012] | 12 of 12 |
| CL-long.G-T0p4-Hinf | 10.767 | 10.749 | -0.017 [-0.223, +0.012] | 12 of 12 |
| OV.C3S | 102.695 | 102.695 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-H0 | 99.876 | 99.876 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-Hinf | 99.876 | 99.876 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV.G-T2-H0 | 99.886 | 99.886 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T2-Hinf | 99.886 | 99.886 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-Tinf-H0 | 102.695 | 102.695 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.C3S | 103.043 | 103.043 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-H0 | 99.885 | 99.885 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-Hinf | 99.885 | 99.885 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-H0 | 99.810 | 99.810 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-Hinf | 99.810 | 99.810 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-Fest | 70.746 | 70.746 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-F0 | 70.746 | 70.746 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-Fest | 70.746 | 70.746 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-F0 | 70.746 | 70.746 | +0.000 [+0.000, +0.000] | 12 of 12 |

### o_hat_end (mean over seeds, unfixed -> fixed; paired delta mean [min, max]; units ms)

| cell.arm | unfixed | fixed | delta mean [min, max] | n numeric |
|---|---|---|---|---|
| CL-long.G-Tinf-H0 | 5.765 | 5.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.C3S | 5.765 | 5.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-H0 | 5.765 | 5.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-Hinf | 5.765 | 5.765 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T0p4-H0 | 5.767 | 5.749 | -0.017 [-0.223, +0.012] | 12 of 12 |
| CL-long.G-T0p4-Hinf | 5.767 | 5.749 | -0.017 [-0.223, +0.012] | 12 of 12 |
| OV.C3S | 97.695 | 97.695 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-H0 | 94.876 | 94.876 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-Hinf | 94.876 | 94.876 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV.G-T2-H0 | 94.886 | 94.886 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T2-Hinf | 94.886 | 94.886 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-Tinf-H0 | 97.695 | 97.695 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.C3S | 98.043 | 98.043 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-H0 | 94.885 | 94.885 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-Hinf | 94.885 | 94.885 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-H0 | 94.810 | 94.810 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-Hinf | 94.810 | 94.810 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-Fest | 10.838 | 10.838 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-F0 | 10.838 | 10.838 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-Fest | 10.838 | 10.838 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-F0 | 10.838 | 10.838 | +0.000 [+0.000, +0.000] | 12 of 12 |

### dh_p50 (mean over seeds, unfixed -> fixed; paired delta mean [min, max]; units ms)

| cell.arm | unfixed | fixed | delta mean [min, max] | n numeric |
|---|---|---|---|---|
| CL-long.G-Tinf-H0 | 82.041 | 82.041 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.C3S | 209.468 | 209.468 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-H0 | 75.558 | 75.558 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T2-Hinf | 197.304 | 197.304 | +0.000 [+0.000, +0.000] | 12 of 12 |
| CL-long.G-T0p4-H0 | 5.630 | 5.680 | +0.050 [+0.048, +0.052] | 12 of 12 |
| CL-long.G-T0p4-Hinf | 16.628 | 17.176 | +0.549 [+0.468, +0.618] | 12 of 12 |
| OV.C3S | 1634.067 | 1634.067 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-H0 | 389.570 | 389.570 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV.G-T0p4-Hinf | 389.617 | 389.617 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV.G-T2-H0 | 1437.925 | 1437.925 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-T2-Hinf | 1634.067 | 1634.067 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV.G-Tinf-H0 | 1437.925 | 1437.925 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.C3S | 5932.143 | 5932.143 | +0.000 [+0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-H0 | 389.656 | 389.656 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV-long.G-T0p4-Hinf | 389.666 | 389.666 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-H0 | 1989.547 | 1989.547 | -0.000 [-0.000, +0.000] | 12 of 12 |
| OV-long.G-T2-Hinf | 1989.559 | 1989.559 | -0.000 [-0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-Fest | 60.100 | 60.100 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-H0-F0 | 60.100 | 60.100 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-Fest | 121.388 | 121.388 | +0.000 [+0.000, +0.000] | 12 of 12 |
| P2.G-T0p075-Hinf-F0 | 70.618 | 70.618 | +0.000 [+0.000, +0.000] | 12 of 12 |