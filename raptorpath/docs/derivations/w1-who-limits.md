# W1.2 / W3.1: who limits at each δ: one δ seat for the store cap and the `bbr_rs` window

Derivation only: no code, no VM. Plan W1.2 (the part that does not depend on attribution) and W3.1.
Sources:
- `bbr_rs.rs`, `quic.rs` and `io_owner.rs`: branch `bbr48cand` at `e5d7ffbd`.
- Every other `raptorpath/src` file: main at `fdad664e`.
- quinn-proto: 0.11.14 (the version `Cargo.lock` pins).
- Status §: `raptorpath/docs/status.md` on main. Paper §: `raptorpath/docs/fec-arq-model.md`.

The final pick waits on §48b's cost attribution (Section 5). Constants that are not derived are marked **OPEN**.

## 0. The formula first

Every in-flight brake in the machine has the same shape: a rate estimate times a base loop time plus a time allowance. Per leg i:

```text
   brake_k,i  =  g_k · x̂_k,i · ( base_k,i + τ_k,i )            [+ an additive byte term]

   store (shipped)     n·s = (1+q(δ)) · Σ bwᵢ·RTpropᵢ              ⇒ g = 1, base = RTprop, τ_s = q(δ)·RTprop
   store (oracle §43b) n·s = G · Σ Lᵢ·(RTpropᵢ + Sᵢ + τ)           ⇒ g = G = 0.9, base = RTprop + S, τ = 5 ms
   bbr_rs (ProbeBw)    W   = 2·b̂·m̂ + A  = b̂·(m̂ + m̂ + A/b̂)       ⇒ g = 1, base = m̂, τ_w = m̂ + A/b̂
```

Sources for the three lines:
- Shipped store: `store_cap.rs:130`, `:177`; paper §6.1.
- Oracle store: status §43b; paper §6.1 "closed-loop oracle arm".
- `bbr_rs` ProbeBw: `bbr_rs.rs:706–709`, constants `:1124–1141`; paper §8.4.

**Who limits** (one unit, wire in-flight bytes, per leg):
- The store's wire share is F_s = n·s − x·λ − B, where:
  - λ is the lag from QUIC ACK to SACK release, during which the store still counts a symbol (OPEN, ≥ 0);
  - B is quinn's backlog `hand − tx` (paper §6.1, "the authority gauge").
- The substrate holds authority iff F_s > W (paper §8.4, "The nested loop").
- Dividing by x̂ ≈ b̂ gives the condition in time units:

```text
   store limits  ⟺   G·(T_s + τ_s(δ)) − λ   <   2·m̂ + A/b̂            (T_s = RTprop + S at the operating point)
```

**Exactly one δ seat** means that δ enters exactly one of τ_s and τ_w, and only through `net::delta_price` → b(δ) → q(δ):
- b(δ) is `store_cap.rs:294/311`, which reads `raptorpath_math::span_horizon_b`;
- q(δ) is `store_cap.rs:130`, q = (b+1)/30.

No third map is added. Each candidate in Section 3 is one assignment of the seat.

**The dial ordering.** The δ-seated allowance is monotone in δ: q is non-increasing in δ and strictly monotone between the Bulk and Realtime anchors. The δ-free brake is constant in δ. So if the two brakes cross, they cross at most once, at some δ*. The ordering therefore needs checking at **one** anchor, the end where the δ-seated allowance is largest (Bulk for the store seat). If δ* lies inside [δ_Bulk, δ_Realtime], the formula is still continuous (a min of continuous functions), but δ loses authority on one side of δ*. That is an authority handover in the dial, the pattern CLAUDE.md warns about. Every candidate must show that δ* falls outside the dial (Section 4).

## 1. Seam check: how δ and a τ source can reach a controller inside quinn

| question | finding | evidence |
|---|---|---|
| Can `BbrRsConfig` carry δ today? | **No.** Its fields are `initial_window` and `clock` (a test seam). The factory ignores everything else. | `bbr_rs.rs:985–991`, `:1017–1020` |
| Is there a per-connection config path? | **Yes, by precedent.** `cc_factory_for_path` builds a per-path factory only for passthrough, sharing an `Arc<AtomicU64>` window that the engine writes (`set_cc_window_bytes`). Every other mode, `bbr_rs` included, gets the shared `quic_cc_factory()` (`BbrRsConfig::default()`). Each endpoint serves exactly one path, so per-path is per-connection. A δ (or τ) handle would follow the same shape: an `Arc<AtomicU64>` holding the f64 bits, in a per-path `BbrRsConfig` that the controller reads at each `on_end_acks`. `clone_box` and `congestion_state()` clones share the Arc. | `quic.rs:189–216`, `:440–460`, `:465–470`; `io_owner.rs:470–486` (`RouteProbeFactory` wraps any factory) |
| Where does the engine know δ? | **Before the transport exists:** `config.protocol_hint` is in scope at `QuicTransport::new` (`net/mod.rs:725`). `SenderPolicy::resolve` resolves b(δ) once (`sender_policy.rs:620`, `delta_b`). δ is otherwise read live through `net::delta_price(hint)` (`store_cap.rs:311`, with the `RWM_DELTA` override). **It can change at run time:** `Scheduler::set_protocol_hint` re-prices Copa per path (`scheduler/mod.rs:634`; only tests call it today). So a controller seat must read a live handle, not a value fixed at construction, and it must read `delta_price` with no new env var. | as cited |
| Does quinn's `RttEstimator` expose a variance? | **No.** 0.11.14 exposes only `get()` (smoothed), `conservative()` (= max(smoothed, latest)) and `min()`. The `var` and `latest` fields are private, and `pto_base` is `pub(crate)`. The `Controller` trait hands the estimator to `on_ack` only, and quinn calls `on_ack` **before** `rtt.update` for that ACK (status §48 claim-2 row). | quinn-proto 0.11.14 `connection/paths.rs:285–329`; `congestion.rs:17–35` |
| Does `bbr_rs` already have its own RTT series? | **Yes, and it is the better source.** Every ACK event gives r_e = a_e − max s_k on its own ack clock (`bbr_rs.rs:826–842`). It keeps the min m̂ and an EWMA `smoothed_loop`, gain 1/8 (`:833`), which the `[QCC]` gauge reports as `qsr_us`. A dispersion of r_e (an RFC 6298 mean deviation of r_e, or a windowed P90 − P50) can be computed inside the controller with no quinn change. Inside the controller, the loop excess ê = qsr − m̂ is the substrate's analogue of the store's no-queue brake b₀: both measure the loop time at zero standing queue minus the minimum (jitter, host and ACK delays). | `bbr_rs.rs:826–842`; paper §8.4 (R̄₀) |
| Can the engine read the controller's model? | **Yes, read-only, per `[DIAG]` tick.** `congestion_qcc()` → `congestion_state()` (a clone under the connection lock) → `into_any` → `Qcc{mode, m̂, smoothed r_e, b̂, quinn min}`. Each read costs one lock and one clone, gauged as `qcns`. So b̂, m̂ and qsr can reach a store law. A second field set (A, the recovery state) would need `Qcc` widened. | `io_owner.rs:395–400`, `:701–703`; `bbr_rs.rs:472–487`, `:939–970` |
| Engine-side dispersion sources | `qsp_us` = P90 − P50 of a leg's last 256 RTT samples (`scheduler/path.rs:775–798`). It matches b₀'s scale at c2 and c8 (×0.75–1.15) but reads 2–3 × b₀ at c1s (status §43b amendment 1), so it is not a drop-in b₀. | as cited |
| The actuator | quinn paces at 1.25·W/srtt and ignores `pacing_rate`. **W is the substrate's only actuator.** | quinn 0.11.14 `pacing.rs:92`; paper §8.4 |

**Seam conclusion.**
- A δ seat **in the window** needs two things:
  - a new per-path `BbrRsConfig` field (an `Arc` handle written from `delta_price`, following the passthrough pattern);
  - an edit to paper §8.4's "No mode switch" paragraph, which today states that the substrate "reads no δ, ρ or hint".
- A δ seat **in the store** needs no transport seam. Its τ source (b₀ or ê) has an in-controller candidate (from r_e) and an engine-side candidate (`qsp_us`); neither is validated (OPEN, Section 6).

## 2. The operating points as they stand (who limits today)

| cell | store | substrate | who limits | evidence |
|---|---|---|---|---|
| c2, BASE (quinn BBR) | pinned at N·knee = 2 048 (the Copa bw anchor over-reads ×3–20 under c2's jitter) | quinn BBR's window over-reads (≈ 180–600 KB against a true ≈ 125 KB) and never binds | **neither law:** the queue sits in the link (`wrtt` ≈ 95–137 ms; MAIN Tp50 ≈ 208 ms at 600 MB) | status §43 Result, §43b, §47; paper §6.1 |
| c2, CAND (`bbr_rs`, shipped store) | predicted still pinned (but see H1 below) | W = 2·b̂·m̂ + A ≈ 2 BDP̂ + A; network queue ≤ W − L·R̄₀ ≈ 4–12 ms | **substrate** on the wire; the store's dwell moves into B, so A_x does not fall | paper §8.4 "nested loop" |
| c2, oracle store (G = 0.9, τ = 5 ms) | n ≈ 187–191; about 150 KB of wire in-flight | window ≈ 180–190 KB at m̂ = 7 ms, ≈ 250–300 KB at m̂ = 10–12 ms | **store** (substrate is the safety net) | status §43b Result; paper §8.4 |
| c1s | — | window ≈ 410 KB, above the wire in-flight the sender reaches | store or receiver; the substrate binds only in the collapse tail | paper §8.4 |

**H1, a prediction to test and not a finding.**
- `bbr_rs` bounds the wire queue, so Copa's anchors may read less inflated, and the shipped law (1 + q)·Σ bw·RTprop may come off its pin.
- Its time allowance is then q·RTprop ≈ 1.04 ms at c2 Bulk, which is **below b₀ ≈ 2.3–2.5 ms**. The law would settle under the knee, as §43's plain start-low arm did (−7 % goodput at G = 1.1, τ = 0).
- **Test gauge:** CAND against BASE at c2, using the `[SUMCAP]` `pin=` fraction and the `[DCAP]` value. Read it beside the STORE-LIMITED class.

## 3. Candidate forms (one δ seat each)

Common notation:
- e₀ is the no-queue loop excess: b₀ on the store's app-echo clock, ê on the controller's r_e.
- The δ map is always q(δ) = (b(δ) + 1)/30 ∈ [0.05, 0.10]: Realtime 0.050, Auto 0.0667, Bulk 0.100.

### A: the δ seat on the store; the substrate stays δ-free

```text
   n  =  clamp( G · Σᵢ L̂ᵢ · ( RTpropᵢ + Sᵢ + τ_s,i(δ) ),  floor,  N·knee )
   τ_s,i(δ)  =  b₀,ᵢ  +  q(δ)·RTpropᵢ                    (W unchanged: 2·b̂·m̂ + A)
```

**Why this form.**
- The shipped law is already "BDP plus a time allowance": (1 + q)·bw·RTprop = bw·(RTprop + q·RTprop).
- §44's debias result shows that the shipped law's undershoot at c2 is estimator bias. Adding the no-queue brake b₀ lands the law on the knee at 0.985 Gmax with "q stays as it is" (status §44, `s43gain.out`).
- So b₀ restores the knee, and q(δ)·RTprop keeps its RFC 8289 provenance as the δ-priced standing-queue setpoint. τ > b₀ holds at every δ, so the law reaches the knee at every δ, as §43's addendum requires.
- G < 1 removes the high trap (§43b).

**τ forms tested against the measured τ windows** (`docs/l1-raw/s42/s43win.out` §3: the window is [τ_lo, min(τ_trap, τ_band)); b₀ and RTprop at the knee are from the same file):

| form | c2 (b₀ 2.28, RTprop 10.37) | c8 thin (b₀ 2.92, RTprop 15.21) | c1s (b₀ 0.30, RTprop 2.74; truncated minima) | common window |
|---|---|---|---|---|
| constant τ | [0.88, 9.24) | [2.88, 7.93) | [1.70, 3.94) | **[2.88, 3.94)** at G = 0.9, but no measured source and no δ |
| k·RTprop (the shipped q·RTprop is k = q) | k ∈ [0.09, 0.89) | [0.19, 0.52) | [0.62, 1.44) | **none**. Bulk q = 0.10 sits below c1s's band and just above c2's lower edge: the shipped law's undershoot |
| k·b₀ (including b(δ)·b₀) | k ∈ [0.39, 4.05) | [0.99, 2.71) | [5.71, 13.21) | **none** (rejected: c1s needs k ≥ 5.7) |
| b₀ + c | c ∈ [−1.40, 6.96) | [−0.04, 5.01) | [1.40, 3.64) | c ∈ [1.40, 3.64): c is a free constant (OPEN) |
| **b₀ + q(δ)·RTprop** (A) | Bulk 3.32, Auto 2.97, Realtime 2.80 | Bulk 4.44, Auto 3.93, Realtime 3.68 | Bulk 0.57, Auto 0.48, Realtime 0.44 | c2 ✓ and c8 ✓ at G = 0.9 and 0.95; **c1s below its lower edge** (c = 0.27 < 1.40) |

Values are ms, at G = 0.9 unless stated.
- At G = 0.95 the windows are c2 [0.12, 7.71), c8 [1.61, 5.38) and c1s [1.29, 3.13): A passes c2 and c8 and fails c1s low.
- At G = 1.0, c8's trap edge is 3.08 < 4.44, so A **fails c8 high**. **G < 1 is required.**

**Lower and upper edges mean different things.**
- The upper edge (τ_trap: no upward crossing above the knee) is a stability bound and applies at every δ. τ_s is largest at Bulk, so checking it at Bulk suffices.
- The lower edge (τ_lo: stable point ≥ n*/1.5) is a Bulk goodput target. Below it, at Realtime, is the intended trade.
- So A's only failure is c1s's goodput edge at Bulk. That edge rests on truncated minima measured with quinn's BBR uncontrolled (§47), so it is soft: re-measure c1s's b₀ under `bbr_rs` (OPEN).

**Predicted c2 trade** (status §43b's closed-loop line T(n) = 14.8 ms + 0.06 ms·(n − 163), which predicted four c2 arms; L = 9.773 datagrams/ms):

```text
   n* = G·L·(T(n*) + τ)  ⇒  n* = G·L·(14.8 − 0.06·163 + τ) / (1 − 0.06·G·L)
      G = 0.9:  n* = 18.63·(5.02 + τ)      (check: τ = 5 → 187; measured 191 / 187)
```

| δ point | τ_s (ms) | n* (G = 0.9) | time term T(n*) | goodput (offline G/Gmax at G = 0.9, then the closed-loop offset seen at τ = 5) |
|---|---|---|---|---|
| Bulk | 3.32 | 155 | 14.3 ms | offline ≈ 0.89 → closed loop ≈ 0.92 ⇒ **≈ 5–8 % below MAIN** |
| Auto | 2.97 | 149 | 13.9 ms | ≈ 0.90–0.91 closed loop |
| Realtime | 2.80 | 146 | 13.8 ms | ≈ 0.89–0.90 closed loop |

How the goodput column is computed:
- `s43win.out` gives offline G/Gmax 0.853/0.832 at τ = 2.5 and 0.948/0.936 at τ = 5. The other values are linear interpolations.
- At τ = 5 the closed loop measured −1.7 % against MAIN, where the offline map said −5 %. That +0.03 offset is applied as an assumption, not a law.
- Delay: Tp50 ≈ 16–17 ms against MAIN's ≈ 208 ms, the §43b order of improvement.

Over the whole Realtime→Bulk range, δ moves n* by about 6 % and the time term by about 0.5 ms. **With q's 5–10 % CoDel band, δ's authority at c2 is small.** Paper §6.1 already says the derivation "compresses the dial's authority" (user question U2).

At G = 0.95 (which fails the §43b rule-1 trap margin G·1.016·1.04 < 1 by 0.004), Bulk gives n* = 20.96·(5.02 + 3.32) ≈ 175, ≈ 0.93–0.95 Gmax. **G is the goodput lever. Its value is OPEN**; the candidate provenance is §43b's escape rate, return time = W·ln(H/n*) / −ln(G·L/D).

**c8 (thin; offline only).**
- (0.9, 5) gives 313/288, 0.915/0.884 Gmax, Tp50 25.2/23.8 ms.
- A's Bulk value of 4.44 sits just below that: ≈ 0.87–0.91 Gmax offline. No closed-loop line exists for c8 (OPEN).

**Who limits under A** (Section 0 condition at Bulk, c2):
- Store side: G·(T_s + τ_s) − λ ≈ 0.9·(14.3 + 3.3) − λ ≈ 15.8 ms − λ.
- Substrate side: 2·m̂ + A/b̂ ≥ 14–24 ms + A/b̂.
- So the **store limits at every δ**, but only for m̂ ≥ 8 ms. At the legacy 7 ms floor the margin is gone unless λ + A/b̂ ≳ 2 ms. The paper's byte figures (150 KB of store wire in-flight against a 180–300 KB window) agree.
- At c1s the condition (0.9·(T_s + 0.6) against 4 ms + A/b̂) is **not** guaranteed by this algebra. There the store and the receiver bound the wire first (paper §8.4).
- The δ* crossover lies outside the dial at c2 and c8, and is unresolved at c1s (gauge: B, the share of `hand − tx` ≥ 98).

**Loop stability.**
- The window is unchanged, so its margin 2·m̂/R̄₀ stays 1.1–1.8 at c2, 1.6–2 at c1 and 1.3–2 on the c3 leg (paper §8.4).
- The store map slope at the c2 point is ≈ 0.28 on the expired map (`s43win.out`, G = 0.9), below 1, so the point is unique.
- τ_trap (9.24 / 7.93 / 3.94 ms) sits above A's Bulk τ at all three cells.

**Continuity.**
- q is affine in clamp(b, ½, 2), and b(δ) is continuous. τ_s is therefore continuous, and strictly monotone between the anchors.
- With a map slope below 1, the unique n*(δ) is continuous (implicit function).
- W does not depend on δ. **No step at any preset.**

**What A needs before it is an engine law.**
- An engine L̂. The store law is oracle-only today (W3.3 OPEN). `bbr_rs`'s b̂ is a candidate through the `[QCC]` read seam, but under store limitation the flow is app-limited and b̂ is raise-only, so b̂'s staleness under a sustained store brake is OPEN.
- An engine b₀ (OPEN; `qsp_us` fails at c1s).
- G (OPEN).

### B: the δ seat on the window; the store stays δ-free

```text
   W  =  b̂·( m̂ + τ_w(δ) ) + A ,       τ_w(δ) = ê + q(δ)·m̂            (store: the shipped law, δ removed, or pinned)
```

- **Stability floor** (paper §8.4 map, b̂′ = min(L, (m̂ + τ_w)·b̂/R̄₀ + A/R̄₀)): it requires τ_w > R̄₀ − m̂ − A/b̂ = ē − A/b̂.
  - So a pure δ-scaled allowance (for example b(δ)·ê) **contracts at Realtime**: b = ½ gives τ_w = ½ê < ē. That is §47's collapse mechanism with a smaller coefficient.
  - The δ term must therefore sit on top of a no-queue floor, which forces the shape ê + q(δ)·m̂.
- **Margin:** (m̂ + ê + q·m̂ + A/b̂)/R̄₀ = 1 + (q(δ)·m̂ + A/b̂)/R̄₀.
  - At c2 that is ≈ 1.04 (Realtime) to 1.08 (Bulk) plus the A term, against 1.1–1.8 today.
  - Under-reading ê by more than q·m̂ + A/b̂ (≈ 0.5–1 ms at c2, ≈ 0.1–0.2 ms at c1) contracts b̂. The window law would then fail on a measurement error in the unsafe direction, which is §47's lesson.
- **Who limits:**
  - The window bounds the wire queue at ≤ q(δ)·m̂ + A/b̂ (≈ 0.5–1 ms + A/b̂ at c2).
  - If the store is looser than the window (pinned at c2), the remaining dwell moves into B, so **A_x does not fall** (paper §8.4). δ then prices the wire RTT, not delivered delay.
  - If the store is tighter, the store limits and the window's δ term has no authority. B gives δ authority over delivered delay only through a store that is both δ-free and tighter, which contradicts the condition itself.
- **c2 trade:** goodput ≈ L only while the margin holds (thin). `wrtt` falls to ≈ m̂ + 1 ms. A_x stays ≈ 208 ms at 600 MB (store pinned). **B does not deliver the δ trade on delivered delay.**
- **c8:** the same structure on each leg. The c3 leg's m̂ of 30–40 ms gives a larger absolute allowance, but the same margin ratio.
- **Continuity:** τ_w is continuous and monotone in δ. Per-preset ±2 % tests on W are needed.
- **Seam:** a δ handle in `BbrRsConfig` (Section 1) and a paper §8.4 change.

### C: one shared allowance; the window is the seat and the store is its image

```text
   Θᵢ(δ) = m̂ᵢ + êᵢ + q(δ)·m̂ᵢ                                   (one definition, one δ)
   Wᵢ    = b̂ᵢ·Θᵢ(δ) + Aᵢ                                       (substrate)
   n·s   = Σᵢ ( Wᵢ + b̂ᵢ·λ̂ᵢ )                                    (store = Σ windows + the ACK→SACK runway; no δ, no L̂ of its own)
```

- **One law and two actuators.** The store reads b̂, m̂ and A through the `[QCC]` seam. It never over-fills quinn (B → 0), and delivered delay ≈ Θ + λ, so **δ prices A_x**, which B cannot do.
- **No engine L̂ is needed:** b̂ is the engine L̂. Because the store fills the window, the flow is not app-limited and b̂ keeps sampling. This dissolves W3.3's L̂ dependency.
- **Who limits:** a tie by construction. The window limits the wire; the store limits only the runway. There is no crossover δ*.
- **Margin:** the same as B, 1 + (q·m̂ + A/b̂)/R̄₀ ≈ 1.04–1.08 at c2: thin.
  - Keeping g_c = 2 (Θ = 2m̂ + ê + q·m̂) restores the margin to ≥ 2m̂/R̄₀.
  - But then δ moves W by at most q·m̂/(2m̂) ≤ 5 %, so δ's authority is negligible.
  - **A window seat must choose between the probing margin and δ authority.** With q's band it cannot have both. Choosing is a user question (U2), unless a wider δ map is admitted.
- **c2 trade (with g_c = 2 kept):** wire in-flight ≈ 2·BDP̂ + A, so the operating point is the window's. Goodput ≈ L when the margin holds. A_x ≈ (2m̂ + ê + λ) ≈ 25–30 ms against 208 ms. δ moves it by < 1 ms.
- **What it reopens:**
  - §34 (b1), "quinn's BBR window as a store input", which was closed against the *broken* quinn BBR (§47 lists its re-reading as owed);
  - paper §8.4's "the substrate window is not a store-law input";
  - ADR-0068's open question, "is the store law meant to be the congestion controller?"
- **Seam:** both directions, δ in and b̂/m̂/A out (widen `Qcc`). The read cost is per tick (gauge `qcns`).

### D: δ-neutral fixes (not a seat; they address the other attribution classes)

- **PROBE-RTT.**
  - ProbeRtt holds W = 0.75·b̂·m̂ for ≥ 200 ms per ≤ 10 s (`bbr_rs.rs:610–650`).
  - The rate during the dip is ≈ 0.75·m̂/R̄₀·L ≈ 0.6 L at c2, so the cost bound is ≈ 0.4 × 0.2/10 ≈ **0.8 %**, plus the filter-decay recovery at ×2m̂/R̄₀ per round.
  - A PROBE-RTT share cannot explain a goodput deficit above ≈ 2 %. If it is the largest class, the fix is the drain (W2's Copa-style drain, or v2's shorter and shallower dip), with no δ involved.
- **UNDER-READ** (qbw < 0.8 L).
  - The sampler or filter is the fault: delivered bytes under GE loss, or app-limited raise-only staleness. The fix is in `RsBandwidthEstimation`, with no δ.
  - It also blocks C, which uses b̂ as L̂.

## 4. Summary table

| | δ seat | δ-free side | who limits (c2, Bulk) | crossover δ* in the dial? | margin (c2) | c2 trade, predicted | c8 | open constants |
|---|---|---|---|---|---|---|---|---|
| **A** | store τ_s = b₀ + q(δ)·RTprop | W = 2b̂m̂ + A | store; window is a safety net | none at c2 and c8 (m̂ ≥ 8 ms); unresolved at c1s | 2m̂/R̄₀ 1.1–1.8 (unchanged); store slope 0.28 | n* 155, ≈ 5–8 % below MAIN, Tp50 ≈ 16–17 ms vs ≈ 208 ms | inside the window; ≈ 0.87–0.91 Gmax offline (thin) | G, engine b₀, engine L̂ |
| **B** | window τ_w = ê + q(δ)·m̂ | store (pinned) | window on the wire, store on the dwell | n/a (δ prices only `wrtt`) | 1.04–1.08 + A/(b̂R̄₀) | goodput ≈ L if the margin holds; `wrtt` ≈ m̂ + 1 ms; **A_x ≈ unchanged** | same | ê source |
| **C** | Θ = m̂ + ê + q(δ)·m̂, shared | store = Σ W + runway | tie (by construction) | none | 1.04–1.08; or ≥ 2m̂/R̄₀ with δ authority ≤ 5 % | A_x ≈ 25–30 ms; goodput ≈ L if the margin holds | per leg, same | ê, λ̂, g_c |
| **D** | — | — | — | — | — | removes ≤ ≈ 2 % (PROBE-RTT) or an estimator fault | — | — |

## 5. Which §48b outcome picks which

These are the plan W1.1 classes, applied per CAND row at c2 and c8. The class with the largest share in the lowest-goodput third names the cost.

| §48b outcome / class | what it says | pick | why |
|---|---|---|---|
| **PROMOTE-ELIGIBLE** (no cost) | the window costs nothing | **A** | the seat lands on the store and `bbr_rs` stays δ-free (paper §8.4 stands); W1.3 changes nothing in `get_target_cwnd` beyond gauges |
| **STORE-LIMITED** (paused ≥ 50 %, B < 98) | the store binds below the knee | **A** | the cost is the store's allowance; H1's mechanism (q·RTprop < b₀ once the anchors deflate) is exactly what A's b₀ term fixes. Confirm with the `[SUMCAP]` `pin=` drop at c2 |
| **QUINN-LIMITED** (B ≥ 98, qm ∈ {B, S, D}) | the window binds and quinn holds the backlog | **A or C (user, U1)** | under A the store goes interior and the window returns to a safety net, so the cost disappears if the Section 0 condition holds at Bulk; but that needs A's engine L̂ and b₀ first. C fixes it now, with b̂ as L̂, at the price of §8.4's stance and a thin margin. **B is not a stand-alone pick:** it leaves A_x where it is. **Sub-readout first:** if 2·qrt/qsr < 1.1 on those lines, the cost is the window's jitter tolerance, a δ-free window question that comes before any seat |
| **PROBE-RTT** (qm = R) | the dip is the cost | **D (drain)**; seat by the PROMOTE-ELIGIBLE rule | bounded at ≈ 0.8–2 %; if the deficit is larger, the attribution is mixed: re-read the next class |
| **UNDER-READ** (qbw < 0.8 L) | the estimator is wrong | **D (sampler)**; this blocks C | fix b̂ before anything reads it |
| **NOT-CONTROLLED / REGRESSION** | W1 stops (plan) | — | the derivation is parked |

## 6. Open items (each named; none is tuned here)

1. **G** (A): rule 1 of §43b requires G ≤ 0.946. The candidate provenance is the escape rate (§43b return-time floor).
2. **b₀'s engine source** (A): `qsp_us` reads 2–3 × b₀ at c1s. Candidates are a windowed min of the app-echo ratio (`EchoRatioMin`, `store_cap.rs`) or `bbr_rs`'s ê. Neither is validated.
3. **c1s's τ window under `bbr_rs`:** the current window was measured with quinn's BBR uncontrolled on truncated minima (§47). A's c1s failure is on this soft edge.
4. **The engine L̂** (A; W3.3): whether b̂ holds L under a sustained store brake (app-limited, raise-only).
5. **λ, the ACK→SACK lag** (all candidates; C needs it as λ̂): not gauged.
6. **ê in the controller** (B, C): no estimator exists. Under-reading it is the unsafe side for B and C.
7. **A, the ack-aggregation term in time units** (A/b̂): not gauged in `[QCC]`. Adding `max_ack_height` to `Qcc` would close the Section 0 condition at c1s.
8. **c8's closed-loop line:** offline only (thin).

## 7. Questions for the user

- **U1. Where does δ live: the store (A), or the window with the store as its image (C)?**
  - C reverses paper §8.4's "the substrate reads no δ" and §34 (b1).
  - A keeps both, but waits on the engine L̂ and b₀.
  - This is ADR-0068's question in concrete form. It decides only under QUINN-LIMITED; the other outcomes pick A.
- **U2. Is q(δ)'s 5–10 % CoDel band the intended δ authority?**
  - Under every candidate, δ moves the c2 operating point by about 6 % in n and under 1 ms of delay.
  - A wider lever would reuse b(δ) (a 4× range), but every k·b₀ form fails c1s (Section 3 A table).
  - This is status §43's "δ-authority revisit".
- **U3. Is a Bulk goodput cost of ≈ 5–8 % at c2 (A at G = 0.9) acceptable for Tp50 ≈ 16–17 ms against ≈ 208 ms?** If not, G ≈ 0.94 buys back about 3–5 % at a thinner trap margin.

## 8. Tests the pick carries (CLAUDE.md: absolute, at the anchors, plus routing)

- **A:**
  - formula-agreement tests: τ_s at the three anchors, exactly (b₀ + q·RTprop with q = 0.05/0.0667/0.10), with ±2 % δ nudges through each preset (continuity and monotonicity);
  - a `cc_sim` check that W does not depend on δ (W identical across hints);
  - a routing gate: the store law's echo shows the `delta_price` it read.
- **B or C:**
  - W at the anchors (absolute);
  - the ±2 % continuity and monotonicity tests;
  - the margin 1 + (q·m̂ + A/b̂)/R̄₀ ≥ a registered bound under the `cc_sim` jitter scenario;
  - ProbeRtt unchanged;
  - a routing witness: `[QCC]` echoes the δ the controller read.
- **Every divergence between this model and the engine** (for example λ = 0 assumed, the offline +0.03 closed-loop offset) gets a bounding test or a registered readout, not prose.
