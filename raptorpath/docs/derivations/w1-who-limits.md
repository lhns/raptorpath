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

**Who limits** (one unit, wire in-flight bytes, single leg):
- The store's wire share is F_s = n·s − x·λ − B, where:
  - λ is the lag from QUIC ACK to SACK release, during which the store still counts a symbol;
  - B is quinn's backlog `hand − tx` (paper §6.1, "the authority gauge").
- The substrate holds authority iff F_s > W (paper §8.4, "The nested loop").
- Dividing by x̂ ≈ b̂ gives the condition in time units:

```text
   store limits  ⟺   G·(T_s + τ_s(δ)) − λ_eff   <   2·m̂ + A/b̂        (T_s = RTprop + S at the operating point; λ_eff = λ + B/x)
```

**λ_eff, bounded from the paper's own numbers** (paper §8.4: at §43b's point, about 190 symbols but only about 150 KB on the wire): 190 × 1 279 B ≈ 243 KB, so x·λ_eff ≈ 93 KB, about 7 ms at 12.5 MB/s. This is one inferred point and is not gauged (OPEN as a gauge).

**At N ≥ 2 the condition does not hold as written.** The store is one pooled Σ, but the windows are per leg, so the per-leg split of F_s is needed. That split is OPEN (c8).

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
| Can the engine read the controller's model? | **Yes, read-only, per `[DIAG]` tick.** `congestion_qcc()` → `congestion_state()` → `into_any` → `Qcc{mode, m̂, smoothed r_e, b̂, quinn min}`. `congestion_state()` clones **the whole controller** under the connection lock, including the sampler's send-record map (up to `RS_MAX_TRACKED` = 8 192 entries); the cost is gauged as `qcns`. That is fine for a diagnostic tick but **not for a law-rate input**. A store law that reads b̂ and m̂ (A's L̂ option, or C) needs the controller to publish them to an atomic handle (the same Arc pattern as the δ write seam), not the `[QCC]` clone. A, the recovery state and ê are not in `Qcc` today. | `io_owner.rs:395–400`, `:701–703`; `bbr_rs.rs:180`, `:198`, `:472–487`, `:939–970` |
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
| c2, BASE (quinn BBR) | pinned at N·knee = 2 048. The shipped law reaches the knee only through its anchor's ack-interval over-read, "the law's accidental probe"; with an honest sampler it contracts to its floor (c2: cap 41 → 10 in 3 s, 7.2 against 89.9 Mbit/s) | quinn BBR's window over-reads (≈ 180–600 KB against a true ≈ 125 KB) and never binds | **neither law:** the queue sits in the link (`wrtt` ≈ 95–137 ms; MAIN Tp50 ≈ 208 ms at 600 MB) | status §43 Result, §43b, §47; paper §6.1 (the g·x paragraph) |
| c2, CAND (`bbr_rs`, shipped store) | predicted still pinned (but see H1 below) | W = 2·b̂·m̂ + A ≈ 2 BDP̂ + A; network queue ≤ W − L·R̄₀ ≈ 4–12 ms | **substrate** on the wire; the store's dwell moves into B, so A_x does not fall | paper §8.4 "nested loop" |
| c2, oracle store (G = 0.9, τ = 5 ms) | n ≈ 187–191; about 150 KB of wire in-flight | window ≈ 180–190 KB at m̂ = 7 ms, ≈ 250–300 KB at m̂ = 10–12 ms | **store** (substrate is the safety net) | status §43b Result; paper §8.4 |
| c1s | — | window ≈ 410 KB, above the wire in-flight the sender reaches | store or receiver; the substrate binds only in the collapse tail | paper §8.4 |

**H1, a prediction to test and not a finding.**
- The shipped law, (1 + q)·Σ bw·RTprop, has **no S term**. With an honest anchor it asks 1.1·L·RTprop ≈ 1.1 × 9.773 × 10.4 ≈ 111 symbols at c2. That is below the knee band's lower edge of 128, and on the honest-sampler record it contracts all the way to the floor.
- It holds the pin only through the ack-interval over-read (paper §6.1).
- If `bbr_rs`'s bounded wire queue changes the ACK clustering behind that over-read, the shipped law loses its accidental probe and falls toward its floor. The result is STORE-LIMITED with a large deficit, not a few percent.
- Whether `bbr_rs` changes the over-read is unknown.
- **Test gauges:** CAND against BASE at c2, reading the `[SUMCAP]` `pin=` fraction, the cap value, and the anchor fields of `[DIAG]` (`btlbw=`, `est=`). Read them beside the STORE-LIMITED class.

## 3. Candidate forms (one δ seat each)

Common notation:
- e₀ is the no-queue loop excess: b₀ on the store's app-echo clock, ê on the controller's r_e.
- The δ map is always q(δ) = (b(δ) + 1)/30 ∈ [0.05, 0.10]: Realtime 0.050, Auto 0.0667, Bulk 0.100.

### A: the δ seat on the store; the substrate stays δ-free

```text
   n  =  clamp( G · Σᵢ L̂ᵢ · ( RTpropᵢ + Sᵢ + τ_s,i(δ) ),  floor,  N·knee )        (W unchanged: 2·b̂·m̂ + A)
   τ_s,i(δ)  =  b₀,ᵢ  +  c(δ)
     A(q):  c(δ) = q(δ)·RTpropᵢ              (provenanced: the shipped CoDel setpoint, RFC 8289)
     A(c):  c(δ) = c_B · q(δ)/q(Bulk)         (c_B ≈ 2.7 ms fits every cell at Bulk; c_B has no source, OPEN)
```

**Why this shape.**
- The shipped law is already "BDP plus a time allowance": (1 + q)·bw·RTprop = bw·(RTprop + q·RTprop).
- The status §44 erratum's debias result shows the law's undershoot at c2 is estimator bias. Adding the no-queue brake b₀ lands it on the knee (195 symbols, 0.985 Gmax), **but at gain 1.1, not at G < 1** (`s43gain.out`).
- b₀ cancels the jitter that the law otherwise reads as standing queue. c(δ) is then the δ-priced standing-queue allowance on top of it.
- G < 1 removes the high trap (§43b).

**τ forms tested against the measured τ windows** (`docs/l1-raw/s42/s43win.out` §3: the window is [τ_lo, min(τ_trap, τ_band)); b₀ and RTprop at the knee are from the same file):

| form | c2 (b₀ 2.28, RTprop 10.37) | c8 thin (b₀ 2.92, RTprop 15.21) | c1s (b₀ 0.30, RTprop 2.74; truncated minima) | common window |
|---|---|---|---|---|
| constant τ | [0.88, 9.24) | [2.88, 7.93) | [1.70, 3.94) | **[2.88, 3.94)** at G = 0.9, but no measured source and no δ |
| k·RTprop (the shipped q·RTprop is k = q) | k ∈ [0.09, 0.89) | [0.19, 0.52) | [0.62, 1.44) | **none** |
| k·b₀ (including b(δ)·b₀) | k ∈ [0.39, 4.05) | [0.99, 2.71) | [5.71, 13.21) | **none** (rejected: c1s needs k ≥ 5.7) |
| b₀ + c | c ∈ [−1.40, 6.96) | [−0.04, 5.01) | [1.40, 3.64) | **c ∈ [1.40, 3.64)** |
| A(q): b₀ + q(δ)·RTprop | Bulk 3.32, Auto 2.97, Realtime 2.80 | Bulk 4.44, Auto 3.93, Realtime 3.68 | Bulk 0.57, Auto 0.48, Realtime 0.44 | c2 ✓ and c8 ✓; **c1s below its lower edge** (c = 0.27 < 1.40) |
| A(c): b₀ + 2.7·q(δ)/0.1 | Bulk 4.98, Realtime 3.63 | Bulk 5.62, Realtime 4.27 | Bulk 3.00, Realtime 1.65 | ✓ at all three cells at Bulk |

Values are ms, at G = 0.9.
- At G = 0.95 the windows are c2 [0.12, 7.71), c8 [1.61, 5.38) and c1s [1.29, 3.13).
- At G = 1.0, c8's trap edge is 3.08, so every A form **fails c8 high**. **G < 1 is required.**

**Lower and upper edges mean different things.**
- The upper edge (τ_trap: no upward crossing above the knee) is a stability bound and applies at every δ. τ_s is largest at Bulk, so checking it at Bulk suffices.
- The lower edge (τ_lo: stable point ≥ n*/1.5) is a **weak** goodput requirement. Being inside it does not mean being at the knee; the c2 trade below shows how far apart the two are.
- The c1s windows were measured on truncated minima with quinn's BBR uncontrolled (§47), so they are soft. Re-measure under `bbr_rs` (OPEN).

**Predicted c2 trade.**
- Operating point: status §43b's closed-loop line, T(n) = 14.8 ms + 0.06 ms·(n − 163), which predicted four c2 arms (L = 9.773 datagrams/ms):

```text
   n* = G·L·(T(n*) + τ)  ⇒  n* = G·L·(14.8 − 0.06·163 + τ) / (1 − 0.06·G·L)
      G = 0.9:  n* = 18.63·(5.02 + τ)      (check: τ = 5 → 187; measured 191 / 187)
      map slope at n*: 0.06·G·L = 0.53 < 1  →  one fixed point
```

- Goodput: a line between the two **measured** closed-loop c2 points:
  - §43 SL: median n ≈ 163, −7 % against MAIN;
  - §43b SL-τ: n ≈ 191, −1.9 %.
- Below 163 the value is an extrapolation.

| form, δ point | τ_s (ms) | n* | goodput against MAIN | time term T(n*) |
|---|---|---|---|---|
| A(q) Bulk | 3.32 | 155 | **≈ −8.5 %** (extrapolated) | 14.3 ms |
| A(q) Auto | 2.97 | 149 | ≈ −9.5 % | 13.9 ms |
| A(q) Realtime | 2.80 | 146 | ≈ −10 % | 13.8 ms |
| A(c) Bulk | 4.98 | 186 | ≈ −1.9 % measured at τ = 5 (§43b, n 191); the line reads −2.8 % | 16.2 ms |
| A(c) Auto | 4.08 | 170 | ≈ −5.8 % | 15.2 ms |
| A(c) Realtime | 3.63 | 161 | ≈ −7.4 % | 14.7 ms |

- Delay at these points: Tp50 ≈ 16–19.6 ms (§43b measured 18–19.6 at τ = 5), against MAIN's ≈ 208 ms at 600 MB.
- **A(q) cannot reach the c2 knee at a trap-free G.** Reaching n ≈ 190 at τ = 3.32 needs G ≈ 0.985, which breaks rule 1 (G ≤ 0.946) and c8's trap edge. q's 5–10 % band is about 1.7 ms short at c2. The provenanced form therefore costs about 8.5 % at Bulk.
- A(c) reaches the knee at Bulk but has an unsourced c_B.
- Across the dial, A(c) gives δ a real lever (n moves 161 → 186, −7.4 % → −1.9 %). A(q)'s lever is small (n moves 146 → 155).

**Under PROMOTE-ELIGIBLE, A introduces a cost where `bbr_rs` had none.** At best (A(c)) it is −1.9 % goodput at Bulk, bought for about 10× lower delay. That is the §43b trade, and it is the user's call (U3).

**c8 (thin; offline only).**
- (0.9, 5) gives 313/288, 0.915/0.884 Gmax, Tp50 25.2/23.8 ms. A(q) at Bulk (4.44) sits slightly below that; A(c) at Bulk (5.62) slightly above.
- c8 has no closed-loop line, and the pooled-store-against-per-leg-window split is OPEN (Section 0).

**Who limits under A** (Section 0 condition at Bulk, c2, single leg):
- Store side: G·(T_s + τ_s) − λ_eff ≈ 0.9·(16.2 + 5.0) − 7 ≈ 12 ms (A(c)); for A(q) it is smaller.
- Substrate side: 2·m̂ + A/b̂ ≥ 14 ms at the legacy m̂ floor of 7 ms.
- So the **store limits at every δ**, which matches the paper's 150 KB of wire against a 180–300 KB window.
- δ* lies outside the dial at c2. At c8 it is OPEN (the pooled split). At c1s it is unresolved: 0.9·(≈ 7 + 3) − λ_eff against 4 ms + A/b̂; the gauge is the share of lines with B ≥ 98.

**Loop stability.**
- The window is unchanged, so its margin 2·m̂/R̄₀ stays 1.1–1.8 at c2, 1.6–2 at c1 and 1.3–2 on the c3 leg (paper §8.4).
- The store map slope is 0.53 on the closed-loop line, below 1.
- τ_trap (9.24 / 7.93 / 3.94 ms at G = 0.9) sits above every A Bulk value.

**Continuity.**
- q is affine in clamp(b, ½, 2), and b(δ) is continuous. τ_s is therefore continuous, and strictly monotone between the anchors.
- With a map slope below 1, the unique n*(δ) is continuous (implicit function).
- W does not depend on δ. **No step at any preset.**

**What A needs before it is an engine law (W3.3).**
- An engine L̂ (the store law is oracle-only today). `bbr_rs`'s b̂ is a candidate through a published atomic (Section 1), but under a store brake the flow is app-limited and b̂ is raise-only, so its staleness is OPEN.
- An engine b₀ (OPEN; `qsp_us` fails at c1s).
- G and, for A(c), c_B (OPEN).

**Under A, W1.3's planned change to `get_target_cwnd` dissolves into W3.3.** The δ work moves to the store law. `bbr_rs` keeps its window, apart from δ-neutral fixes (D) and gauges.

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
- **Seam:** both directions: δ in, and b̂/m̂/A out through a published atomic. The `[QCC]` clone is too costly at law rate (Section 1).

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
| **A(q)** | store, τ_s = b₀ + q(δ)·RTprop | W = 2b̂m̂ + A | store; the window is a safety net | none at c2; OPEN at c8 (pooled split); unresolved at c1s | window 2m̂/R̄₀ 1.1–1.8 (unchanged); store map slope 0.53 | n* 146–155: **≈ −8.5 % (Bulk) to −10 % (Realtime)**; Tp50 ≈ 16–17 ms vs ≈ 208 ms | inside the window (thin) | G, engine b₀, engine L̂ |
| **A(c)** | store, τ_s = b₀ + c_B·q(δ)/0.1 | the same | the same | the same | the same | n* 161–186: **≈ −1.9 % (Bulk) to −7.4 % (Realtime)**; Tp50 ≈ 18–19.6 ms at Bulk | inside the window (thin) | as A(q), plus c_B ≈ 2.7 ms |
| **B** | window, τ_w = ê + q(δ)·m̂ | store (pinned) | window on the wire, store on the dwell | n/a (δ prices only `wrtt`) | 1.04–1.08 + A/(b̂R̄₀) | goodput ≈ L if the margin holds; `wrtt` ≈ m̂ + 1 ms; **A_x ≈ unchanged** | the same | the ê source |
| **C** | shared, Θ = m̂ + ê + q(δ)·m̂ | store = Σ W + runway | tie (by construction) | none | 1.04–1.08; or ≥ 2m̂/R̄₀ with δ authority ≤ 5 % | A_x ≈ 25–30 ms; goodput ≈ L if the margin holds | per leg, the same | ê, λ̂, g_c |
| **D** | — | — | — | — | — | removes ≤ ≈ 2 % (PROBE-RTT), or fixes an estimator or recovery-window fault | — | — |

## 5. Which §48b outcome picks which

These are the plan W1.1 classes, applied per CAND row at c2 and c8. The class with the largest share in the lowest-goodput third names the cost.

**Why QUINN-LIMITED is an estimator question first.**
- Paper §8.4's map is x = min(L, W/R̄). A window that binds with b̂ = L costs no goodput: the queue it builds raises R̄ until x = L.
- So a goodput deficit on window-bound lines implies W < L·R̄₀ there. That means one of two things:
  - b̂ < L·(R̄₀ − A/L)/(2m̂), i.e. b̂ ≲ 0.55–0.9 L at c2's margins;
  - the window is not T(2) + A, for example `window()` = min(cwnd, recovery_window) with the recovery window near its T(1) floor (`bbr_rs.rs:774`, `:907–915`).
- The tempting readout 2·qrt/qsr does **not** discriminate. On a window-bound line, qsr already contains the queue the window builds (R̄ = W/L), so 2·qrt/qsr ≤ 1 on every such line.

| §48b outcome / class | what it says | pick | why |
|---|---|---|---|
| **PROMOTE-ELIGIBLE** (no cost) | the window costs nothing | **A** (A(c) if U3 accepts the cost) | the seat lands on the store and `bbr_rs` stays δ-free (paper §8.4 stands); W1.3's `get_target_cwnd` change dissolves into W3.3. **A then introduces the cost (≥ ≈ 2 % at c2 Bulk)** in exchange for about 10× lower delay |
| **STORE-LIMITED** (paused ≥ 50 %, B < 98) | the store binds | **A** | the cost is the shipped law's allowance. If H1 holds (the pin drops and the cap falls toward 111 or the floor), the shipped law has lost its accidental probe, and A's b₀ + c(δ) time term is the derived replacement. Confirm with `[SUMCAP]` `pin=`, the cap value, and `btlbw=`/`est=` |
| **QUINN-LIMITED** (B ≥ 98, qm ∈ {B, S, D}) | the window binds and quinn holds the backlog | **D first, then the seat by the PROMOTE-ELIGIBLE rule (A); C only by U1** | sub-readout: qbw/L on those lines, with R̄₀ taken from low-backlog lines (B < 98). If qbw < L, it is an estimator or recovery-window fault: fix it δ-neutrally. C inherits that fault, because C reads b̂ as its L̂. **B is never a stand-alone pick:** it leaves A_x unchanged. Recovery state is not gauged; adding it to `Qcc` is a prerequisite for this readout |
| **PROBE-RTT** (qm = R) | the dip is the cost | **D (drain)**; then the seat by the PROMOTE-ELIGIBLE rule | bounded at ≈ 0.8–2 %; a larger deficit means a mixed attribution: re-read the next class |
| **UNDER-READ** (qbw < 0.8 L) | the estimator is wrong | **D (sampler)**; blocks C | fix b̂ before anything reads it |
| **NOT-CONTROLLED / REGRESSION** | W1 stops (plan) | — | the derivation is parked |

**Net result.** Every attribution outcome puts the δ seat on the store (A). The window changes only through δ-neutral fixes (D). C stays as the user's alternative (U1): the substrate as the controller. **Which A form (q or c) to use is the open derivation item**, and it does not depend on the attribution.

## 6. Open items (each named; none is tuned here)

1. **c_B, or q's band (A).**
   - A(q) is provenanced but about 1.7 ms short at c2 (≈ −8.5 % at Bulk).
   - A(c) fits every window at c_B ≈ 2.7 ms, but c_B has no source.
   - This is U2.
2. **G (A).** Rule 1 of §43b requires G ≤ 0.946. The candidate provenance is the escape rate (§43b return-time floor).
3. **b₀'s engine source (A).** `qsp_us` reads 2–3 × b₀ at c1s. The candidates, neither validated:
   - a windowed min of the app-echo ratio (`EchoRatioMin`, `store_cap.rs`);
   - `bbr_rs`'s ê.
4. **c1s's τ window under `bbr_rs`.** The current window comes from truncated minima with quinn's BBR uncontrolled (§47).
5. **The engine L̂ (A; W3.3).** Does b̂ hold L under a sustained store brake (app-limited, raise-only)? And a law-rate read needs a published atomic, not the `[QCC]` clone.
6. **λ_eff = λ + B/x.** One inferred point is ≈ 7 ms at c2 (Section 0); it is not gauged. C needs it as λ̂.
7. **The pooled store against per-leg windows at N ≥ 2 (c8).** The per-leg share of F_s is needed.
8. **ê in the controller (B, C).** No estimator exists. Under-reading it is the unsafe side.
9. **A/b̂ and the recovery state.** Neither is in `Qcc`. Adding `max_ack_height` and the recovery state would close the c1s condition and the QUINN-LIMITED sub-readout.
10. **c8's closed-loop line.** Offline only (thin).

## 7. Questions for the user

- **U1. Is the substrate meant to be the controller (C), or the safety net under a δ-priced store (A)?**
  - C reverses paper §8.4's "the substrate reads no δ" and reopens §34 (b1).
  - A keeps both.
  - This is ADR-0068's question in concrete form. No §48b outcome forces C.
- **U2. Is q(δ)'s 5–10 % CoDel band the intended δ authority?**
  - Under A(q), δ moves the c2 point from 146 to 155 symbols, and Bulk sits ≈ 8.5 % under MAIN.
  - A(c) gives δ a real lever (−1.9 % to −7.4 %), but it needs a source for c_B ≈ 2.7 ms.
  - Every k·b₀ form, b(δ)·b₀ included, fails c1s.
  - This is status §43's "δ-authority revisit".
- **U3. Is about 2 % of Bulk goodput at c2 (A(c), the §43b point) worth Tp50 ≈ 18–19.6 ms against ≈ 208 ms?**
  - The cost grows toward Realtime by design.
  - A(q) costs ≈ 8.5 % at Bulk.
  - Under PROMOTE-ELIGIBLE this cost is new: the substrate alone costs nothing.

## 8. Tests the pick carries (CLAUDE.md: absolute, at the anchors, plus routing)

- **A:**
  - formula-agreement tests on τ_s at the three anchors, exactly: b₀ + c(δ) with q = 0.05/0.0667/0.10, with ±2 % δ nudges through each preset (continuity and monotonicity);
  - a `cc_sim` check that W does not depend on δ (W identical across hints);
  - a routing gate: the store law's echo shows the `delta_price` it read.
- **B or C:**
  - W at the anchors (absolute);
  - the ±2 % continuity and monotonicity tests;
  - the margin 1 + (q·m̂ + A/b̂)/R̄₀ ≥ a registered bound under the `cc_sim` jitter scenario;
  - ProbeRtt unchanged;
  - a routing witness: `[QCC]` echoes the δ the controller read.
- **Every divergence between this model and the engine** gets a bounding test or a registered readout, not prose. Examples: λ_eff inferred from one point; the goodput line extrapolated below n = 163.
