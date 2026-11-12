# Observability and performance analysis: what we measure, what we miss, the design

Status: design, no code. ADR draft: `docs/adr/0074-metrics-registry-and-otel-export.md`
(Proposed). Reference tree: `github/main` `eaca2e54`. Status sections cited: §21,
§41c, §47, §48, §48b and the c8 completion-tail diagnosis.

**What is decided (user plus advisor).**
- **One source.** A single internal metrics registry is the only source of
  every metric. It records on atomics and never allocates on a data path.
- **Three renderings of it.**
  - The `/status` JSON.
  - The existing tagged log lines, **byte-identical** in format.
  - An optional OTLP exporter behind a cargo feature, off by default.
- **Exemplars** link histograms to a sampled or flight-recorded per-symbol
  trace.
- **Instruments never change a law.**

The design uses standards where they fit:
- OpenTelemetry metrics and traces, its semantic conventions, OTLP;
- qlog for QUIC events;
- perf, pprof and flamegraphs for CPU;
- criterion-style ns benches.

Legend used throughout:
- **A**: always on and readable at run time (`/status`, or an always-printed
  line).
- **E**: always on, but printed only at end of run (`Drop` teardown lines,
  the perf JSON summary).
- **G**: only on a gated log line.
- **M**: missing. No engine source exists, or the quantity is computed only
  outside the process or post hoc.
- "counted always" means the counting runs on every arm and only the print
  is gated; "counting gated" means the counter itself does not exist when the
  gate is off.

## 1. Coverage: every scored or recently diagnosed quantity

| # | quantity (where scored or diagnosed) | in `/status`? | in a log tag? (tag, gate, cost of the gate) | class | missing |
|---|---|---|---|---|---|
| 1 | goodput (§21, §41c R1) | no; `throughput_mbps` is the *offered* rate (§2) | perf JSON `mbps` (`perf.rs:321`, perf mode only, end of object) | E | a run-time delivered-bytes counter and goodput over the transfer window, in TUN mode too |
| 2 | client CPU per GB (§21, §41c) | no | none in-process on the shipped path. `[THR] sum cpu_s` (`RWM_RTOBS≥1`, counting gated). Scored from `/usr/bin/time` user+sys of the **whole invocation incl. warm-up** (`perf_rwm_c.sh:480`) | G | in-process `process.cpu.time` over the transfer window |
| 3 | server CPU per GB | no | as 2. The harness reads `/proc` ticks | G | as 2 |
| 4 | `lag_E` cli/srv = p99 − min of `[LAG]` (§28/§32 form of §21's clause) | no | `[LAG]` (`RWM_RTOBS≥1`, 10 ms probe task; counting gated) | G | an always-on timer-lag histogram over the whole transfer |
| 5 | `rtp_floor` per leg (min `rtp_us`) | no | `[DIAG]` `rtp_us` (`RWM_DIAG`, 250 ms) | G | `raptorpath.path.rtt.rtprop` gauge |
| 6 | §48 R2a (share of lines with `qcwnd` < BDP) | no; `cwnd` is Copa's window (§2) | `[DIAG]` `qcwnd` (`RWM_DIAG`; the source `PathView.cwnd` is counted always, `io_owner.rs:1359`) | G | wire-window gauge |
| 7 | §48 R2b (`qcwnd` < ½ BDP **and** B = hand − tx ≥ 98) | no | `[DIAG]` `qcwnd` + `dgq<id>[hand tx …]` (`RWM_DIAG`; the dgq audit's **counting is gated**: `dg_audit` is set from `RWM_DIAG` when the transport is built) | G | always-on hand/tx counters; a per-tick R2b indicator |
| 8 | wire RTT `wrtt` (§48 R3) | no | `[DIAG]` `rtt/wrtt/rtp` (`RWM_DIAG`; `PathView.rtt_us` counted always) | G | `raptorpath.path.rtt.wire` |
| 9 | app-echo RTT A_x (§48 R3) | `rtt_ms`, but mixed with the peer's report and the 50 ms seed (§2) | `[DIAG]` `rtt` | A (misleading) | a clean one-clock `rtt.echo` |
| 10 | send-buffer dwell B = hand − tx (§48 R2b/R3, §48b supplement) | no | `[DIAG]` `dgq`, `dgev` (`RWM_DIAG`, counting gated) | G | `raptorpath.path.dwell.send_buffer` |
| 11 | §48 R4: `qm` occupancy, m̂ (`qrt_us`), loop margin 2·m̂/R̄ (`qsr_us`) | no | a separate `[QCC]` line under `RWM_DIAG` on `bbr48cand` only (`diag.rs:952–974` there, read through `transport.qcc_gauge` at the owner's snapshot); **not on main** | M (main) | `cc.wire.state`, `.bw`, `.min_rtt`, which exist only while `bbr_rs` is the controller |
| 12 | ProbeRtt time and its goodput cost (§48b ≈ 0.8 %) | no | none on main; on `bbr48cand`, the `qm` share of `[DIAG]` lines | M | `cc.wire.probe_rtt.time`; goodput per tick |
| 13 | quinn's lifetime `rtt.min()` `qmin_us` (§47 witness, confirmed §48b) | no | none on main. On `bbr48cand` it is `[QCC]` `qmin_us`, read from the controller side at 4.3–14.6 µs per read (`qcns`, §48b) | M | `cc.wire.quinn_min_rtt` (tick-only read) |
| 14 | delivered delay (`[LAT]` decomposition) | no | `[LAT]` (`RWM_DIAG`/`RWM_FDIAG`, 1 s; **counted always**, `LatGauge` fed per delivered symbol) | G | delivery-delay histogram rendered always |
| 15 | hole backlog H, H0 at source end (c8 tail law) | no | `[RANK] holes`, `[REQ] holes`, `[SACKI]` (`RWM_DIAG`/`RWM_FDIAG`); H0 was reconstructed post hoc | G | `recovery.holes.open` |
| 16 | first-report hole age (c8: 3.1 s / 5.2 s vs c2 197 ms) | no | `[HOLD]` `age_p50/90_us` (always, **teardown only**, one line per path that fired). Its clock: the age of the seq's live flight at the sender's first report, measured from the original's send, so it **includes the sender's own queue dwell** (`holddown.rs:166–170`) | E | an age histogram per tick, on the same clock |
| 17 | recovery causes | no | `[FCAUSE]` (always, teardown), `[RACK]`, `[RFA]` Drop | E | per-cause counters at run time |
| 18 | completion tail duration (c8: 3.9 s BASE / 4.7 s CAND, 70 % of the deficit) | no | none. Reconstructed post hoc from `[DIAG]` `t`, `infl` and dgq | M | `transfer.duration` split main/tail |
| 19 | store-cap engagement (`[SUMCAP]`, `[DCAP]`) | no | `[SUMCAP]`/`[DCAP]` (default ON gates, teardown) | E | run-time engagement |
| 20 | store cap value series and `paused` (§48b supplement: `paused` p50 71–79 %) | no | `[DIAG]` `win`, `paused` (`RWM_DIAG`, `DiagState`, counting gated) | G | `store.cap`, `store.occupancy`, `store.paused.time` |
| 21 | placement shares (§48b: 0.823/0.177) | `symbols_sent` per path (counts source **and** copies **and** coded: 8 increment sites) | `[C8CONV-S] sp`, `[DIAG]` per path | A (approx.) | a source-only per-path counter |
| 22 | per-leg loss `plc` (the cumulative fed loss, checked against `[TRUTH]`) and `plu` (the BOCD predictive upper quantile at 0.95 that the rate law reads, `diag.rs:680–690`) | only `loss_rate` = `pl`, the per-ack EWMA that decays to 0 between losses, and `peer_loss_rate`. Neither is `plc` or `plu` | `[DIAG]` `plc`, `plu` (`RWM_DIAG`) | G | `path.loss.fed_cum` (`plc`), `path.loss.upper` (`plu`) |
| 23 | coded share | `fec.total_repair_symbols`, `total_source_symbols`, `overhead_ratio` (copies `total_copy_symbols` **not** rendered) | `[DIAG]` `cod`, `cum` | A | copies in `/status` |
| 24 | repair rate r (the dial's realized r) | no | `[DIAG]` `rce=`, `eff_pace` | G | `dial.r`, `dial.beta` |
| 25 | decode failures | `blocks.decoded_fail`: **no writer anywhere** (§2) | `[SUCC]` `aban_n` (counted always; printed under `RWM_DIAG`/`RWM_FDIAG`) | G | a real counter |
| 26 | CPU per task, wakes/parks (`[THR]`, `[TASK]`, `[RDIAG]`, `[CPUPROF]`) | no | `[THR]` (`RWM_RTOBS≥1`, tokio stable `worker_park_count`); `[TASK]` (`RWM_RTOBS=2`); `[RDIAG]` (`RWM_RDIAG`); `[CPUPROF]` (`RWM_CPUPROF`, teardown). All counting gated | G | park/poll counters always; per-task CPU stays G (priced) |
| 27 | datagram batching (c8 slow leg 6.80 → 5.18 dg/batch) | no | `[IOWN]` (`RWM_RTOBS≥1`; `OwnerCounters` base counters are **counted always**, `io_owner.rs:174`) | G | render the existing counters |
| 28 | evictions (`dgq full`; §48b: the `dgev` column is the backlog, not evictions) | no | `[DIAG]` `dgq[… full …]` (counting gated) | G | `store.evictions` |
| 29 | δ / ρ / r dial values | no | `[PIPE]` prints the hint *name* (always); `[LATE]` prints `delta` (G); no numeric ρ | G | `dial.*` gauges |

**Counts (29 rows): A 3 (one misleading, one approximate), E 4, G 18, M 4.**
- In the requested three bins: **covered always-on 7** (A + E, of which only
  3 are readable while the run is live), **covered by a gated log only 18**,
  **missing 4**.
- Of the 18 G rows, 5 are counted always and printed gated (6's `qcwnd`,
  8, 14, 25, 27). For these the fix is a rendering, with no
  new data-path cost.
- The rest need new always-on counting and are priced in §4.

Two side findings:
- `CLAUDE.md` cites `raptorpath/docs/goal-gate.md` for measurement rule 1,
  but `main` has no such file. `docs/measurement-discipline.md` exists.
- The harness does not read `/status` at all. Every scored clause comes
  from stderr tags, the perf JSON, or out-of-process `/usr/bin/time`.

## 2. `/status` field audit (who writes what)

`/status` is served only when `status_addr` is configured
(`net/mod.rs:1268`). It is `SharedStats::snapshot` (`monitor/stats.rs:131`).

| field | writer(s) (file:line) | what it actually is | verdict |
|---|---|---|---|
| `blocks.encoded/decoded_ok/decoded_fail/pending` | **none**. The only reader is `main.rs:511–515` (the `status` CLI) and the only writers are tests (`tests/monitoring_test.rs:70–81`). The block pipeline was deleted (ADR-0069). | always 0 | **dead**: remove, or move under `legacy` (Q1) |
| `fec.actual_failure_rate`, `fec.pi_correction` | **none** (`FecStats` bits never stored; `control/fec_rate.rs:574` has same-named fields that never reach `SharedStats`) | always 0 | **dead** |
| `fec.target_tail_loss` | `net/mod.rs:778`, once from config | config echo | fine |
| `paths[].rtt_ms` | Three writers, last one wins:<br>(a) `on_window_ack`, `control_msg.rs:692–697`: the raw app-echo sample `now − echo_send_timestamp_us` (`:504`), only when `echo > 0`;<br>(b) `on_path_report`, `control_msg.rs:459–460`: **the peer's** reported `avg_rtt_us`;<br>(c) `on_ack`, `:358–360`: an echo sample. The receiver builds and sends the per-batch `Ack` only `if !suppress_legacy_ack` (`receiver.rs:2160–2164`), which is the default `RWM_ACK_MERGE=1` (`gates/scheduler_gates.rs:274`), so (c) does not run on the shipped path.<br>Seed: 50 ms (`stats.rs:305`), shown as if measured. | It alternates between this end's **unsmoothed app-echo** sample and the peer's average. Neither is quinn's wire RTT (`QuicTransport::wire_rtt`, `transport/quic.rs:491`), which excludes the app-layer and send-buffer dwell. That difference was 206 ms against 21.8 ms at c2 under `bbr_rs` (§48b R3). | **misleading** (two clocks, two directions, seed) |
| `paths[].jitter_us` | Three writers:<br>`receiver.rs:1276`: this end's RFC 3550 **arrival** jitter, per arrival;<br>`control_msg.rs:461`: the peer's reported jitter;<br>`control_msg.rs:696`: the jitter echoed in the WindowAck, i.e. the peer's arrival jitter for **our** TX direction. | two directions in one field | **misleading** |
| `paths[].throughput_mbps` | `control_msg.rs:362` (on_ack, off-path) and `:657–658` (window ack). It stores `estimator.throughput()`: the EWMA (`control/estimator.rs:492`) of the report task's `symbols_sent` delta × symbol size / dt (`net/tasks/report.rs:63–85`, `:155`). | the **offered** symbol rate on this path: source, copies and coded, excluding QUIC overhead. It is not goodput and not wire bytes. | **misleading by name** |
| `paths[].cwnd`, `in_flight` | `control_msg.rs:363–364`, `:659–660`; `copa_feed.rs:136–137` | **Copa's in-app window and ledger, in symbols.** It gates in-app placement: `available() = cwnd − in_flight` (`scheduler/path.rs:456`) filters every placement choice (`scheduler/mod.rs:267, 315, 531, 552, 567, 593`). It reaches the wire only under `RWM_QUIC_CC=passthrough` (`set_cc_window_bytes` is a no-op otherwise, `quic.rs:465–468`). The wire is gated by quinn's controller window, `PathView.cwnd` (`io_owner.rs:1359`, `qcwnd`, bytes), which `/status` does not expose. Copa's window grows without bound while app-limited (c8 tail diagnosis). | **misleading** (unit, and which window binds) |
| `paths[].in_slow_start` | `control_msg.rs:365`, `:661`, from `PathState.in_slow_start = copa.ramping` (`scheduler/path.rs:517`) | Copa's ramp flag, not quinn's Startup (`qm = S`) | **misleading** |
| `paths[].symbols_received` | `control_msg.rs:366`, `:662–663` (sender side: `d_received` from the ack's counters) | the count of this end's symbols that **the peer acknowledged**, not symbols this end received. It is also read back by the receiver at `receiver.rs:2034`. | **misleading by name** |
| `paths[].symbols_sent` | 8 sites (`emit_source.rs:418, 480, 849`, `sender_phases.rs:457, 559, 1087, 1166`, `net/mod.rs:3416`) | every symbol handed to the transport on the path: source, copies and coded | fine, if documented |
| `paths[].loss_rate` | `control_msg.rs:361`, `:653–656` | the fed loss estimator EWMA | fine (= `pl`) |
| `paths[].peer_loss_rate` | `control_msg.rs:462–463` | the peer's reported loss (monitoring only) | fine |
| `paths[].active` | `scheduler/mod.rs:515`, `tx_inputs.rs:179`, `tasks/report.rs:164` | liveness | fine |
| `fec.total_source_symbols`, `total_repair_symbols`, `overhead_ratio` | `emit_source.rs:420`; `FecStats::record_correction` | coded repair only. `total_copy_symbols` and `taper_copy` exist but are not rendered, so `overhead_ratio` hides the copies. | incomplete |

The consumers that would see any change are the `raptorpath status` CLI
(`main.rs:486–515`) and `tests/monitoring_test.rs`. §6.1 keeps every v1 key
and corrects or documents each one; renaming is Q1.

## 3. The "every aspect" test: could this session's diagnoses be read from always-on metrics?

| diagnosis | what it needed | readable today from always-on metrics? | instruments that would make it so |
|---|---|---|---|
| **§47 quinn min-RTT collapse** (c1s) | the `qcwnd` series; the ProbeRtt fingerprint (`qcwnd` = 4·MTU at t = 10.0/20.1/30.1 s); quinn's `rtt.min()`; the loss burst before it (`qce`, lost packets) | **No.** `qcwnd`/`qce`/`qlp` exist only in `RWM_DIAG` `[DIAG]` (a rerun under the gate). `qmin` is not exposed on main. `/status` `cwnd` is Copa's window. | `cc.wire.window` (gauge + per-tick histogram); `cc.wire.quinn_min_rtt` (the latched-low witness; a controller-side read, tick only); `wire.congestion_events`, `.lost_packets` (class T from the owner's existing `publish`); the **collapse detector** (R2b shape, §7.3) dumping the flight recorder |
| **§48b c8 completion-tail backlog** | H0 and μ; the 20-gap horizon actually used; the pooled recovery clock C and which leg sets it; ranges per report; first-report age; the cooldown-suppressed share; the tail duration | **No.** It was reconstructed post hoc from gated `[DIAG]`, `[SACKI]`, teardown `[HOLD]` and a `[SUCCDUMP]` rerun. | `recovery.holes.open`, `.horizon.used`, `.clock` (with the setting leg as a second gauge), `.cooldown_suppressed`, `.holes.first_report_age`, `transfer.duration` main/tail; the **tail detector** dumps the recorder at source end |
| **quinn send-buffer dwell inflating the recovery clock** (open in the tail diagnosis: does leg 1's app-echo clock include the B ≈ 327-datagram dwell?) | echo RTT against wire RTT per leg, at the same instants, plus B | **No.** `/status` `rtt_ms` mixes clocks; `wrtt` and B are gated. | `rtt.echo` and `rtt.wire` side by side; `dwell.send_buffer` (B). A per-tick derived `rtt.echo − rtt.wire` against B / tx rate (Little's law over the FIFO DATAGRAM queue: the k-th handed datagram is the k-th DATAGRAM frame sent, so dwell is the lag between the `hand` and `frame_tx.datagram` counters) answers the question directly |
| **ProbeRtt goodput cost** (≈ 0.84 % c2, ≈ 0.78 % c8) | controller state per tick, plus goodput per tick | **No** (on `bbr48cand` it was the `qm` share of `[DIAG]` lines times `good`). | `cc.wire.state` gauge, `cc.wire.probe_rtt.time` counter, `transfer.delivered` per tick. The cost is then Σ over ProbeRtt ticks of (mean − tick goodput), computable by any OTel backend |
| **slow-leg batching CPU** (c8: tx 6.80 → 5.18 dg/batch, parks +6.7 %) | datagrams per batch per leg, both directions; tokio parks; sender wake cycles; CPU per seam | **No.** `[IOWN]` and `[THR]` are `RWM_RTOBS≥1`, `[CPUPROF]` is `RWM_CPUPROF`. The owner counters themselves are counted always. | render `io.tx.batches/datagrams`, `io.rx.*` (existing atomics, zero new cost); `runtime.parks` (tokio stable metrics at tick); `process.cpu.time` over the transfer window. Per-seam CPU stays G, and a differential flamegraph (§8.5) is the standard tool that would have located it |

**Verdict:** none of the five could be read from always-on metrics today.
All five could be with the catalogue in §4:
- every needed value is either already counted (quinn stats, owner
  counters, `LatGauge`), or
- one H0/H2 record at a site that already runs per event.

## 4. Instrument catalogue

**Conventions.**
- **Names.** Instruments sit in the `raptorpath.*` namespace. Where an OTel
  semantic convention exists, the instrument uses its name and unit. The
  `process.*` metric conventions are **Development** status, so the semconv
  version is pinned in the scope's `schema_url`:
  - `process.cpu.time` (`s`, attribute `cpu.mode` = user | system);
  - `process.memory.usage` (`By`);
  - `process.context_switches` (`{context_switch}`, attribute
    `process.context_switch.type` = voluntary | involuntary);
  - `process.thread.count`.
- **Units.** Units are UCUM: `us`, `s`, `By`, `{symbol}`, `{datagram}`,
  `{hole}`, `1`. **Every instrument exports in the unit it records.** A µs
  histogram is exported in `us`: converting its bounds to seconds would
  multiply 2^(i/8) by 10⁻⁶, which is no longer an exponential-histogram
  bound.
- **Attributes** form a closed set, the one the ADR states:
  - `rp.path` is the path index 0..63, from the dense table;
  - on class-T process and task instruments only, fixed enumerations:
    semconv's `cpu.mode` and `process.context_switch.type`, and `rp.task`
    from the fixed task-name set (≤ 16);
  - nothing else. Any other split (a phase, a recovery cause, a symbol
    kind) is a separate instrument, not a label;
  - role (`client` | `server`) and `service.instance.id` are **resource**
    attributes, set once and never sent per point.
  - δ, ρ and r are gauges, never attributes. No hint name, gate name or
    arm label is ever an attribute. A `[GATES]`-style build/config echo is
    one `raptorpath.build.info` gauge = 1 whose attributes are resolved
    once at start-up (commit, gate digest).

**Cost classes** (per record, on the recording thread). Every ns figure in
this document is an **estimate, unmeasured until the P1 bench**:
- **H0**, a single-writer counter or gauge: `load` + `store`, no lock
  prefix, ≈ 1 ns. Each instrument has one writing task, named in the
  "recorded at" column. A second writer requires `fetch_add` and an entry
  in the table.
- **H1**, a shared counter: `fetch_add` Relaxed, ≈ 5–8 ns uncontended.
- **H2**, a histogram: index ≈ 3 ns (§5.3) plus one H0 bucket increment,
  plus H0 `count` and `sum`.
- **T**, collection-tick only: no data-path cost; computed by the collector
  from other instruments or from quinn's `Connection::stats()` (already
  read by the owner's `publish`, `io_owner.rs:1352`).
- **G**, gated deep instrumentation: count and print both under an
  `RWM_*` gate (e.g. the `[LAT]` `SeqRing`, `[SUCC]`'s hole map,
  `[RTTDUMP]`). G instruments enter the registry as instruments only when
  their gate is on; the gate's cost is priced by its own registration.

The rate at the busiest cell (c1s, ≈ 1 Gbit/s at 1 350 B) is ≈ 90 k
symbols/s per end. One H2 record per symbol is ≈ 0.3–0.5 ms of CPU per
second (0.03–0.05 % of a core); one H0 per symbol is ≈ 0.1 ms/s. The budget
for the whole always-on set is **≤ 10 instrument records per symbol and
≤ 0.5 % of the sending thread's CPU**, priced by the ns bench (§8.1).

### 4.1 Transfer and delivery (receiver unless noted)

| name | kind | unit | semantics / clock | recorded at | cost |
|---|---|---|---|---|---|
| `raptorpath.transfer.delivered` | counter | By | application bytes delivered in order to the consumer (TUN/perf sink) | receiver delivery | H0 per batch |
| `raptorpath.transfer.goodput` | gauge | bit/s | delivered bytes over the transfer window (first to last delivery); the perf JSON `mbps` is the scored source and this one must agree with it (§8.2) | collector | T |
| `raptorpath.transfer.duration.main`, `.tail` | gauge | s | the transfer window split at "source end emitted" (the c8 tail diagnosis split). Two instruments, no `phase` label | collector | T |
| `raptorpath.delivery.delay` | histogram | us | send stamp → in-order delivery (the `[LAT]` decomposition's end-to-end, app-echo clock domain). Sampled to every symbol | receiver delivery | H2 |
| `raptorpath.delivery.hol_wait` | histogram | us | arrival → in-order delivery (head-of-line part of the above) | receiver delivery | H2 |
| `raptorpath.decode.failures` | counter | {event} | decoder give-ups (`aban` class) — must stay 0 at ρ = 1 | receiver | H0 |
| `raptorpath.decode.recovered` | counter | {symbol} | sources reconstructed from coded repair (`[SUCC]` `rep`) | receiver | H0 |

### 4.2 Paths: the two RTT clocks, loss, placement

| name | kind | unit | semantics / clock | recorded at | cost |
|---|---|---|---|---|---|
| `raptorpath.path.rtt.echo` | histogram + gauge (srtt, min) | us | **app-echo RTT** (`now − echo_send_timestamp_us`, `control_msg.rs:504`): includes the sender's store dwell, the owner hop and quinn's send-buffer dwell | sender ack arm | H2 per ack |
| `raptorpath.path.rtt.wire` | gauge (srtt, min, latest) | us | **quinn's packet-timed RTT** (`QuicTransport::wire_rtt`, `quic.rs:491`): excludes the app/send-buffer dwell. On main quinn exposes only the smoothed `rtt()` and `PathStats`. Quinn's lifetime `rtt.min()` (the §47 latched-low witness `qmin_us`) needs the controller-side read that `bbr48cand` added (`qcc_gauge`), as `cc.wire.quinn_min_rtt`, tick only | owner `publish` | T |
| `raptorpath.path.rtt.rtprop` | gauge | us | the engine's RTprop (`PathState::min_rtt`) — the scored `rtp_floor` is its run minimum | sender | H0 |
| `raptorpath.path.dwell.send_buffer` | gauge + histogram (per tick) | {datagram} | B = `hand − tx − evicted − send_err`: datagrams handed to quinn and not yet on the wire (the §48 R2b/R3 backlog) | owner | T (counters exist; the audit is `RWM_DIAG`-gated today — §1) |
| `raptorpath.path.sent` | counter | {symbol} | symbols handed to the transport on this path, attribute-free split by **kind** as three instruments: `.source`, `.copy`, `.coded` (no `kind` label: three counters keep the record H0) | sender | H0 |
| `raptorpath.path.wire.sent_packets`, `.lost_packets`, `.congestion_events` | counter | {packet}/{event} | quinn `PathStats` | owner publish | T |
| `raptorpath.path.loss.ewma` | gauge | 1 | `pl`: the per-ack EWMA (today's `/status` `loss_rate`) | sender ack arm | H0 |
| `raptorpath.path.loss.fed_cum` | gauge | 1 | `plc`: the estimator's cumulative fed loss | sender ack arm | H0 |
| `raptorpath.path.loss.upper` | gauge | 1 | `plu`: the BOCD predictive upper quantile at 0.95, which the rate law reads | sender | H0 |
| `raptorpath.path.loss.peer` | gauge | 1 | the peer's reported incoming loss (monitoring only; not `plu`) | sender report arm | H0 |
| `raptorpath.path.placement.share` | gauge | 1 | this path's share of placed source symbols over the last tick | collector from `.source` | T |
| `raptorpath.path.active` | gauge | 1 | liveness | sender | H0 |

### 4.3 Congestion: the in-app window and the wire window, separately

| name | kind | unit | semantics | recorded at | cost |
|---|---|---|---|---|---|
| `raptorpath.cc.app.window` | gauge | {symbol} | Copa's in-app window: gates **in-app placement** (`available() > 0` filters, `scheduler/mod.rs:267–593`); reaches quinn only under `RWM_QUIC_CC=passthrough` (`quic.rs:465`) | sender | H0 |
| `raptorpath.cc.app.in_flight` | gauge | {symbol} | the engine's in-flight ledger | sender | H0 |
| `raptorpath.cc.wire.window` | gauge | By | quinn's controller window (`qcwnd`) — the one that gates the wire | owner publish | T |
| `raptorpath.cc.wire.state` | gauge | 1 | `bbr_rs` phase as an integer gauge (0 S, 1 D, 2 B, 3 R) — `qm`; one gauge, not a label | owner publish | T |
| `raptorpath.cc.wire.probe_rtt.time` | counter | s | time spent in ProbeRtt (the §48b ≈ 0.8 % goodput cost) | owner publish (phase-change edge) | T |
| `raptorpath.cc.wire.bw`, `.min_rtt`, `.loop_rtt` | gauge | bit/s, us | the controller's b̂, m̂ and smoothed loop time (`qbw`, `qrt_us`, `qsr_us`; §48 R4). They exist only while `bbr_rs` is the controller. Read through `qcc_gauge` at 4–15 µs per read, so tick only | owner snapshot | T |
| `raptorpath.cc.wire.quinn_min_rtt` | gauge | us | quinn's lifetime `rtt.min()` (`qmin_us`) | owner snapshot | T |

### 4.4 Recovery and the store

| name | kind | unit | semantics | recorded at | cost |
|---|---|---|---|---|---|
| `raptorpath.recovery.fires` | counter ×4 | {event} | per cause: `.gap_data`, `.timer`, `.taper`, `.request` (the `[FCAUSE]` classes; one counter per cause) | sender recovery phase | H0 |
| `raptorpath.recovery.holes.open` | gauge | {hole} | unrecovered holes above the frontier (H; H0 at source end is the tail law's numerator) | sender, from SACK state | H0 per report |
| `raptorpath.recovery.holes.first_report_age` | histogram | us | hole detection → first report naming it (c8: 3.1 s / 5.2 s vs c2 197 ms) | sender, per new gap | H2 per new gap |
| `raptorpath.recovery.horizon.used` | gauge | {range} | gaps the sender acted on this report (bounded by `MAX_NACK_GAPS`) and ranges carried (≤ `MAX_SACK_RANGES`) | sender | H0 |
| `raptorpath.recovery.clock` | gauge | us | the pooled recovery clock C (max over legs) and the leg that sets it | sender | H0 |
| `raptorpath.recovery.cooldown_suppressed` | counter | {event} | gap visits suppressed by cooldown (97 % at c8 main phase) | sender | H0 |
| `raptorpath.store.cap` | gauge | {symbol} | the store-cap law's value | sender | H0 |
| `raptorpath.store.occupancy` | gauge | {symbol} | store fill | sender | H0 |
| `raptorpath.store.paused.time` | counter | s | time the store gate held emission (`paused` share) | sender | H0 per edge |
| `raptorpath.store.evictions` | counter | {datagram} | `dgq full` evictions (distinct from the `dgev` backlog column, §48b) | owner | H0 |

### 4.5 The (δ, ρ, r) dial and coding

| name | kind | unit | semantics | recorded at | cost |
|---|---|---|---|---|---|
| `raptorpath.dial.delta`, `.rho` | gauge | s, 1 | the contract's dial position as resolved (never a label) | start-up, on change | T |
| `raptorpath.dial.r` | gauge | 1 | the realized repair rate r(β) | sender | H0 per update |
| `raptorpath.dial.beta` | gauge | 1 | β = bulkness_of_delta(δ) as used by the rate law | sender | H0 |
| `raptorpath.coding.coded_share` | gauge | 1 | coded / (source + copy + coded) over the tick | collector | T |

### 4.6 CPU, runtime and I/O

| name | kind | unit | semantics | recorded at | cost |
|---|---|---|---|---|---|
| `process.cpu.time` | counter | s | `CLOCK_PROCESS_CPUTIME_ID` split user/sys (`getrusage`); read at tick **and at transfer start/end** so CPU per GB is computable over the transfer window, not the whole invocation | collector | T |
| `raptorpath.task.cpu.time` | counter | s | per named task (`[TASK]`'s thread-CPU-in-poll); attribute `rp.task` from the fixed task-name set (bounded: ≤ 16 names) | task wrapper | G today (`RWM_RTOBS=2`); see §7.4 |
| `raptorpath.task.polls`, `raptorpath.runtime.parks` | counter | {event} | wake / park counts (`[THR]`) | task wrapper / tokio metrics | H0 / T |
| `raptorpath.io.tx.batches`, `.tx.datagrams`, `.rx.batches`, `.rx.datagrams` | counter | {batch}/{datagram} | owner batching (`OwnerCounters`, `io_owner.rs:174`) — datagrams per batch = ratio (the c8 slow-leg batching finding) | owner | H0 (exists, always on) |
| `raptorpath.io.send_errors`, `.rx_capped` | counter | {event} | owner counters | owner | H0 (exists) |
| `raptorpath.timer.lag` | histogram | us | runtime timer lateness (`[LAG]`; scored `lag_E` = p99 − min) | lag probe | H2 per probe tick |

## 5. The registry

### 5.1 Storage

```text
SharedStats (per engine instance, Arc'd; client and server in one loopback
process keep two)
 └─ metrics: Registry
     ├─ scalars:  [Cell; N_SCALAR]                 // process-scope counters/gauges
     ├─ per_path: [PathCells; STATS_DENSE_PATHS]   // OnceLock'd with the dense table
     │             └─ [Cell; N_PATH_SCALAR], [Hist; N_PATH_HIST]
     ├─ hists:    [Hist; N_HIST]
     └─ fr:       FlightRecorder handles (§6)
Cell = AtomicU64 in a #[repr(align(64))] group per writer task   (f64 gauges as bits)
Hist = { buckets: [AtomicU64; NB], zero: AtomicU64, count, sum, min, max }
```

- **The descriptor table is `const`**: name, kind, unit, description, the
  writer task, and the cost class. Each instrument is a typed handle
  (`Ctr(u16)`, `Gauge(u16)`, `Hist(u16)`), so a record is an array index.
  There is no lookup by name and no attribute slice on the data path.
- **Padding.** Cells written by different tasks sit on separate cache
  lines. Cells with the same writer are packed together, so the sender's
  per-symbol counters share one line and the receiver's another.
- **Migration.** `PathStats`' existing atomics and `OwnerCounters` become
  registry cells. The existing accessors stay for the duration of
  migration (§9), so no call site changes meaning in the same commit as
  its storage.

### 5.2 Collection

- A collector task on the main runtime ticks every `RWM_METRICS_TICK_MS`
  (default 1 000 ms). It reads every cell with Relaxed loads into a
  preallocated `Snapshot`, a plain struct of arrays.
- Derived instruments (class T) are computed from two consecutive
  snapshots: shares, goodput, coded share, datagrams per batch, quinn
  `PathStats` already published by the owner.
- **Temporality is cumulative** (the OTLP default, and Prometheus-
  compatible). Nothing on the data path is ever reset: a reader never
  writes, so the "last line wins" cumulative reading the harness relies on
  holds for every rendering.
- **Who takes snapshots.**
  - `/status` and the exporter take the collector's latest `Snapshot`: an
    `Arc` behind a `parking_lot::Mutex` (already a dependency), locked
    only off the data path, once per tick and per request.
  - **The log tags take their own snapshots at their own cadence**:
    `[DIAG]` every 250 ms in the sender loop, the receiver block every
    1 s, the teardown lines at `Drop`. Their read/reset windows become
    the delta between two of the tag's own consecutive snapshots, kept
    in `TagLocals`.
  - This keeps per-line values, and the line-share statistics scored from
    them (R2a/R2b), exactly as they are. Rendering tags from the 1 s
    collector snapshot would keep the bytes' format but shift the values.
- The test in §8.3 relies on one property: **every rendering is a pure
  function of a `Snapshot`** (plus, for windowed values, the previous
  snapshot from the same renderer).

### 5.3 Histograms (no allocation, exact OTel buckets)

- The OTel **exponential histogram at scale 3** (bucket bounds
  2^(i/8), relative width 9.05 %) is computed exactly, without `log`:
  - convert the integer sample `v` to `f64` (exact below 2^53);
  - take its exponent `e` and the top mantissa bits `m`;
  - OTel buckets are **upper-inclusive**: bucket i holds
    (base^i, base^(i+1)] (`metrics.proto`, `opentelemetry-proto` 0.33.1,
    lines 563–564 and 600–601).
  - So `index = 8·e + Σ_{k=1..7} [m > T_k] − [m = 0]`, with `T_k` the
    mantissa bits of 2^(k/8) in a 7-entry `const` table. The `[m = 0]`
    term puts an exact power of two, which is common for integer µs, in
    the bucket below. The `T_k` with k ≥ 1 are irrational powers, so no
    mantissa equals them.
- This is branchless: one conversion and seven compares, ≈ 2–3 ns. It is
  **not** the log-linear `leading_zeros + shift` index, whose bounds are
  `2^e·(1 + k/8)` and are not OTel's.
- A unit test checks every boundary: each `T_k` against `2^(k/8)`, and the
  recorded index against the rendered `(lower, upper]` for values on both
  sides of every bound, exact powers of two included.
- **Range.** The range per instrument is fixed in the descriptor: µs
  instruments 1 µs .. 2^27 µs (134 s), 27 octaves × 8 = 216 buckets
  (1.7 KB), plus zero, under and over. Under and over are counted, never
  dropped: the over count is a visible resource bound, as `[LAT]` `over=`
  is today.
- **Renderings.**
  - OTLP: native `ExponentialHistogram`, scale 3, offset = first non-empty
    bucket. A backend may down-scale losslessly (3 → 0 merges by 8).
  - `/status`: count, sum, min, max and p50/p90/p99 read from the buckets.
- **The log tags keep their exact sample stores.** `quantile.rs` prints
  nearest-rank quantiles of real samples. A bucketed histogram cannot
  reproduce those bytes, so the existing sample stores remain the source of
  every quantile a tag prints, and the same record call feeds the registry
  histogram. A test bounds `|hist_quantile − nearest_rank| ≤ one bucket
  width (9.05 % relative)` over randomized sample sets: a declared,
  bounded rendering difference (§8.3).

### 5.4 Why not the OTel SDK's instruments (read at source, `opentelemetry_sdk` 0.33.1)

The SDK is not called on any data path, for four reasons read at source:
- `ValueMap::measure` (`metrics/internal/mod.rs:127–177`) takes an `RwLock`
  read and hashes the attribute slice on every record. When the slice is
  not in sorted order it allocates: `sort_and_dedup` does
  `attributes.to_vec()` (`:427–435`, the TODO there says so). The reader
  count of the `RwLock` is a cache line shared by every recording thread.
- Histogram buckets are a `Mutex<Buckets>` (`internal/histogram.rs:19–45`).
- Bound instruments, which skip the lookup, are behind
  `experimental_metrics_bound_instruments` (`opentelemetry` 0.33.1
  `instruments/counter.rs:41`).
- The SDK records no exemplars: every data point is built with
  `exemplars: vec![]` (`internal/aggregate.rs:271, 303, 308, 350, 355,
  401, 458`; `internal/exponential_histogram.rs:481, 541`). No code path
  outside tests fills them. Its
  `ResourceMetrics` cannot be built outside the crate (fields
  `pub(crate)`, `data/mod.rs:13–17`), so pre-aggregated data cannot pass
  through its `PushMetricExporter`. The OTel API also has no asynchronous
  histogram.

So the registry renders OTLP itself (§6.3), from the `opentelemetry-proto`
message types.

## 6. Renderings

### 6.1 `/status` v2
- **Additive.** Every v1 key stays, with its semantics fixed or documented
  (§2). New objects are added: `paths[i].rtt.{echo,wire,rtprop}`,
  `paths[i].cc.{app,wire}`, `paths[i].dwell`, `recovery`, `store`, `dial`,
  `cpu`, `io`, `hist.*`, and `schema: 2`.
- **Dead keys.** `blocks.*`, `fec.actual_failure_rate` and
  `fec.pi_correction` are moved under `legacy`, or removed: open question
  Q1. The `status` CLI (`main.rs:486–515`) and
  `tests/monitoring_test.rs` are their only readers.
- The handler serializes the latest `Snapshot`, so it is cheap and never
  touches a data-path cell.

### 6.2 The log tags, byte-identical
Each tag's formatter becomes `fn render_<tag>(&Snapshot, &TagLocals) ->
String`:
- `TagLocals` carries the values that stay outside the registry: exact
  sample stores, and the per-tag window bookkeeping.
- **Gating is unchanged.** A tag prints only under its gate, at its cadence
  and on its stream.
- **Pinned to the ledgers.** The red-first pin for each tag is a
  **ledger round-trip test**. Lines are taken from committed ledgers
  (`docs/l1-raw/**`, scrubbed), and each line is parsed into
  (`Snapshot`, `TagLocals`) by a test-only inverse. Rendering must
  reproduce the line byte for byte. The test is written and green against
  the current formatter before the counters move.
- After migration each tag is one commit. If the ledger round-trip test
  goes red, the commit is wrong.

### 6.3 The OTLP exporter (feature `otel`, off by default)
- **Build.** Cargo feature `otel` on the `raptorpath` crate only;
  `raptorpath-wasm` and `raptorpath-math` are untouched. Without the
  feature the module does not compile in and the binary is unchanged.
  This is checked by a test that greps the `[GATES]` echo and by a size
  check.
- **Runtime.** The exporter is enabled only when the feature is built in
  **and** `OTEL_SDK_DISABLED` is not `true` **and** an endpoint is
  configured. It is never on by default.
- **Data.** On each export tick it maps the latest `Snapshot` to an
  `ExportMetricsServiceRequest`, built from `opentelemetry-proto` 0.33
  `metrics.v1` types:
  - counters → `Sum` (monotonic, cumulative);
  - gauges → `Gauge`;
  - histograms → `ExponentialHistogram`, with exemplars from §7.5.
  The request goes out as OTLP/HTTP POST `/v1/metrics`.
- **Configuration** is the standard environment, parsed once at start-up:
  - `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT`;
  - `OTEL_EXPORTER_OTLP_PROTOCOL` and `…_METRICS_PROTOCOL`;
  - `OTEL_EXPORTER_OTLP_HEADERS` and `OTEL_EXPORTER_OTLP_TIMEOUT`;
  - `OTEL_METRIC_EXPORT_INTERVAL` (spec default 60 000 ms) and
    `OTEL_METRIC_EXPORT_TIMEOUT`;
  - `OTEL_SERVICE_NAME` and `OTEL_RESOURCE_ATTRIBUTES`;
  - `OTEL_SDK_DISABLED`;
  - `OTEL_METRICS_EXEMPLAR_FILTER` (`always_off` | `always_on` |
    `trace_based`).
  
  **Any unsupported value fails start-up with a named error**, for example
  `grpc` when only HTTP is built, or a non-`cumulative` temporality
  preference. It is never silently ignored.
- **Protocol** (open question Q2). The spec's default protocol is
  `http/protobuf`. Two options:
  - **(a)** `http/protobuf` through `prost` (spec-default, one more crate
    family). This is the recommendation.
  - **(b)** `http/json` only, via `opentelemetry-proto`'s `with-serde`
    (no `prost`, but a non-default protocol that must be named explicitly).
  
  `grpc` is not offered; tonic and h2 are not worth the VM build memory.
- **Transport.** `hyper` 1.8 (already in `Cargo.lock` through `axum`) plus
  `hyper-util`'s client. TLS (`hyper-rustls`, with `rustls` already locked)
  only under a second feature `otel-tls`. The default deployment is a
  localhost collector.
- **Isolation.** The exporter task runs on the main runtime at the lowest
  priority: one task, one in-flight request, and the snapshot is dropped if
  the previous export has not finished. It never blocks the collector.
  Exporter failures are counted in `raptorpath.otel.export.failures` and
  warned at powers of two (`warn_pow2`, the owner's precedent).
- **Logs signal.** The engine's `tracing` events stay on stderr as today.
  The ledgers are stderr and must not change. Sending them over OTLP logs
  is deferred: it would duplicate every tag line, and the tags are already
  rendered from the registry.

## 7. The flight recorder and per-symbol correlation

### 7.1 Identity: what each event carries
- **Instance:** `(PathView.serial-class instance id, role)` in the dump
  header. Never per record.
- **Source symbol:** `seq`, the window's global source seq.
- **Path:** `path` index, plus `path_seq` (the v9 per-path batch seq,
  already on the wire).
- **Send stamp:** the batch send stamp (`echo_send_timestamp_us`'s origin,
  already on the wire). It is the cross-end join key, as `[SUCC]` uses it
  (`succ.rs`: originals are stamped in seq order, so a closing copy stamped
  later than its exposer is a copy).
- **Generation** `gen`: the sender's transmit count for that seq (0 =
  original, k = k-th copy), with **kind** ∈ {source, gap copy, taper copy,
  request copy, coded}. For a coded symbol: its repair id and its window
  `[lo, hi]`.
  - **`gen` is sender-side only.** The wire carries no retransmit bit, and
    no wire change is proposed for instrumentation. The receiver's events
    join to the sender's offline by `(seq, send stamp)`.
- **Receiver events:** arrival `(seq, path, stamp)`, hole detected, hole
  closed (orig / rep / aban, `[SUCC]`'s classes), in-order delivery.
- **Owner events** (per batch, not per datagram): hand count, tx count,
  B at the poll.

### 7.2 Recording at ≈ 90 k symbols/s within a bounded cost
- **Record: 16 bytes.**
  - `t_us: u32` (µs since the ring epoch; wraps at 71 min, and the epoch
    is re-anchored in every dump header);
  - `seq_lo: u32` (a `SeqHigh` marker event when the high half changes);
  - `kind: u8`, `path: u8`, `gen: u8`, `flags: u8`;
  - `aux: u32`, kind-specific: `path_seq`, the stamp's low bits, B, an RTT
    sample, a hole age.
- **The clock is the one the site already holds** (the loop's `now_us()`
  per wake or batch). The recorder adds **no clock read**. Resolution is
  the site's own: per-symbol order is exact, and timing is per batch.
- **Ownership.**
  - One ring per producing task (sender loop, receiver loop, each owner),
    owned by that task's state. The task is the only writer, so the write
    is `buf[head & mask] = rec; head += 1`, with no atomic.
  - A dump request is an `AtomicBool` the task checks once per loop
    iteration (≈ 1 ns per wake, not per symbol). On a request the task
    **swaps** its ring with a preallocated spare (an O(1) pointer swap) and
    sends the full ring to a writer thread. No memcpy happens on the data
    path; the loop is never stalled by a dump.
  - **The wedged-task case** is the stall detector's own case: a task
    that never wakes never swaps.
    - So the 16-byte record is two `AtomicU64` written with Relaxed
      stores (the same instructions as plain stores on x86-64, and no data
      race), and `head` is published with Release once per batch.
    - If a request is not acknowledged within 2 ticks, the writer thread
      copies the ring itself. It reads `head` before and after and
      discards the records the writer could have overwritten in between.
- **Events per symbol:** about 2 per end:
  - sender: emit, plus SACK or cumulative cover;
  - receiver: arrival, plus delivery;
  - copies, coded symbols and holes add a few percent;
  - owner events are per batch.
  
  That is ≈ 180 k events/s per end, **≈ 2.9 MB/s**.
- **Cost:** ≈ 2–4 ns per record, so **≈ 0.4–0.7 ms of CPU per second per
  end (≈ 0.05–0.07 % of a core)**. That is far below anything the §41c
  guard can see. It is therefore bounded by the ns bench, by
  `alloc_per_symbol` (zero added), and by in-process CPU per byte
  (§8.1), not by the guard. The streaming-store cache pressure is
  measured, not assumed.
- **One mechanism, one continuous parameter: the sample mask.**
  - A seq is kept iff `hash(seq) & mask == 0`, with
    `hash = (seq · 0x9E3779B97F4A7C15) >> 32`. The hash is deterministic,
    so sender and receiver keep **the same seqs** without coordination,
    and every sampled seq is joinable end to end.
  - `mask = 0` records every symbol; `mask = 63` records 1/64.
  - Coded symbols are sampled by repair id.
  - Ring capacity is a byte budget `RWM_FR_BYTES`. At full rate, 8 MB
    holds ≈ 2.8 s at c1s and ≈ 25 s at 100 Mbit/s.
  - The default (full rate with a small ring, or 1/64 with a long one) is
    Q3. Both are the same code at different mask values; there is no mode
    bit.
- **Always kept, separately:** a small *sampled* ring at mask 63 (≈ 2.8 k
  events/s, 1 MB ≈ 23 s at c1s). This is the ring exemplars point into
  (§7.5), and it is written at end of run whenever the recorder is on
  (≈ 1 MB).

### 7.3 What triggers a dump: read-only detectors
- **Where they run.** Detectors run on the collector tick and read only
  the `Snapshot`. Their parameters are **instrument parameters**: no
  detector reads δ or ρ, and no law, gate, emission or admission decision
  can read a detector. This is enforced by module visibility (§8.3).
  NO-MODE-SWITCH governs laws. A detector only chooses which bytes go to
  disk; it is not a law, and it is named here so that nobody ever wires
  one into one.
- **The detectors:**
  - **Stall.** The in-order frontier has not advanced for ≥ 8 ×
    (max over live legs of the wire SRTT) while data is outstanding. This
    is the `[WEDGE]` condition, made scale-free.
  - **Collapse (the R2b shape, §48).** On an active leg, the wire window
    is < ½ × (delivery rate × R_ref) **and** B ≥ 98 datagrams (§48's
    constant).
    - R_ref is the **10 s windowed minimum of the per-tick wire SRTT**,
      kept by the collector. It cannot latch the way quinn's lifetime
      `rtt.min()` does: a reference taken from that would be blind to
      §47 itself.
    - This is an online approximation, not R2b. R2b stays computed by
      the harness with the cell's fixed `BDP_leg`. Detector hits are
      reported beside R2b, never instead of it.
  - **Backlog growth.** `recovery.holes.open` rose over 3 consecutive
    ticks while the pooled clock C exceeds the 4 s-bin service bound (the
    tail diagnosis's K/C).
  - **Tail.** Source end was reached with `holes.open > 0`. The dump is
    taken at completion and carries H0 and the tail duration.
  - **Timer lag.** A tick's lag p99 exceeds 10 × its run median. This
    covers owner stalls and preemption.
- **Bounds.** Each detector fires at most once per 10 s. There are at most
  `RWM_FR_DUMPS` (default 4) dumps per run, written to `RWM_FR_DIR`
  (`/dev/shm` on the VM, per the agent rules). Each firing is also an
  online OTel event (§7.4) and a counter
  `raptorpath.fr.detector.fired` (one counter per detector; no label).

### 7.4 Output and mapping: compact binary ring plus an offline converter (chosen)
- **Dump format:** a header, then raw records. The header holds the
  instance, role, wall epoch, `seq` high bits, mask, ring bytes, the
  registry `Snapshot` at dump time, and the `[GATES]` digest.
- **The converter** is `tools/fr/fr2otel.py`, Python like the harness.
  It joins sender and receiver dumps by `(seq, stamp)` and writes:
  - **OTLP/JSON traces.**
    - One trace per transfer:
      `trace_id = H(instance pair, transfer id)`.
    - One span per sampled source symbol, from first send to in-order
      delivery: `span_id = H(seq)`.
    - Span **events** per transmission (`gen`, kind, path) and per
      arrival.
    - One span per recovery round (one processed report).
    - These load into any OTel backend (Jaeger, Tempo) through a
      collector's OTLP receiver.
  - **qlog** (JSON-SEQ, main schema) with a custom `raptorpath:` event
    namespace (`symbol_sent`, `symbol_received`, `hole_detected`,
    `symbol_delivered`), so qvis can draw it.
    - quinn's own qlog is real: quinn 0.11 feature `qlog` → quinn-proto
      0.11.14 `connection/qlog.rs` emits `PacketSent`, `PacketReceived`,
      `PacketLost` and `MetricsUpdated`.
    - It takes a `Mutex` and serializes JSON **per packet**
      (`qlog.rs:33–44`), so it belongs in the gated deep set (per
      connection, via `TransportConfig`), never always on. The converter
      can merge the two files on the shared clock.
  - **Perfetto / Chrome trace JSON**, for one timeline beside CPU samples.
- **Why not online OTel spans per symbol.** An SDK span per symbol
  allocates its attribute and event vectors and passes a batch-processor
  queue: on the order of µs. At 180 k events/s that is tens of percent of
  a core. Even sampled 1/64 it allocates on the data path, and the ledger
  rule forbids perturbing what is measured. The ring costs ns and never
  allocates.
- **Online OTel signals (feature `otel`)** are kept only for low-rate
  events, rendered by the same OTLP code (`/v1/traces`, `/v1/logs`): the
  transfer span, detector firings, and "dump written" (carrying the file
  name, so a backend shows where the per-symbol trace is).

### 7.5 Exemplars: metric → trace
- **What is recorded.** Exemplars are recorded only for **sampled** seqs
  (the hash test of §7.2).
- **How.** Each H2 histogram has one exemplar slot per octave (27 slots of
  `value, seq, t`).
  - When a sampled seq is recorded, the single writer updates its octave's
    slot under a sequence counter: bump to odd, three stores, bump to
    even. The collector retries a read that saw an odd or changed count,
    so it never pairs one record's value with another's seq.
  - Cost is ≈ 1 ns amortized (1/64 of records; an estimate).
- **Rendering.** The slot becomes an OTLP `Exemplar` with
  `trace_id = H(transfer)` and `span_id = H(seq)`, the same functions as
  the converter. A click from a p99 bucket therefore lands on that
  symbol's span.
- **No dangling links.** Within the sampled ring's retention every
  exemplar's span exists, because the sampled ring is written at end of
  run. Exemplars older than the retention are not emitted.
- **Filter.** `OTEL_METRICS_EXEMPLAR_FILTER` is honored: `always_off`
  emits none; `trace_based` treats the deterministic sampler as the trace
  sampling decision.
- **Precedent.** `[SUCCDUMP]` already ships per-hole raw records for
  offline joining. The recorder generalizes it: binary instead of stderr
  text, so it no longer perturbs as `[RTTDUMP]`'s own header warns
  (`rttdump.rs:17–21`).

## 8. Benchmarking and analysis methodology going forward

### 8.1 Pricing every always-on instrument (the primary bound)
- **`benches/metrics_bench.rs`** (criterion, host, under the cargo mutex)
  measures ns per operation for:
  - an H0 counter, an H1 counter, the exponential index, an H2 record;
  - a ring record; the dump swap;
  - a collector `Snapshot` at 8 paths;
  - each tag render; and the OTLP encode of one snapshot.

  It sets a regression bound per operation (2× its first measurement,
  registered then).
- **`tests/alloc_per_symbol.rs`** is extended: allocations per symbol
  with the full always-on set and the recorder on must equal those with
  everything off. The counting allocator already exists.
- **In-process CPU per byte.** A loopback perf run with instruments on
  against off reads `process.cpu.time` over the transfer window, n ≥ 10
  interleaved. On the VM, `perf stat -e instructions:u,cycles:u` per row:
  instructions retired vary far less run to run than time.
- **The §41c guard is a backstop, not the bound.** It resolves ≈ 1–2.4 %
  goodput and 5–8 % CPU per GB (the §48b resolution readout), so it cannot
  certify a 0.1 % overhead. A metrics-on/off guard session is still run
  once, before default-on, as a liveness and no-regression check.

### 8.2 How batteries, the guard, cc_sim and the parsers consume the registry
- **Snapshot file.** `RWM_METRICS_SNAPSHOT=<path>` writes one JSON line
  per collector tick plus a `final` line. Each line is an OTLP/JSON
  `ExportMetricsServiceRequest`, so the file loads into any OTLP tool and
  into a collector's file receiver. It is written by the collector task,
  off the data path, to `/dev/shm` on the VM.
- **`tools/l1/l1common.py`** gains `registry_series(path)` and
  `registry_final(path)`.
  - Each scored clause gets a parity test on a rehearsal capture:
    parser(log tag) equals parser(snapshot) within the declared rendering
    bound (exact for counters and gauges; one bucket for quantiles).
  - Examples: `rtp_floor` from `[DIAG]` equals the run minimum of
    `rtt.rtprop`; R2b from `[DIAG]` equals R2b from the snapshot
    series.
- **Committed ledgers stay re-scorable as they are.** The tags do not
  change by a byte (§6.2), and the old parsers keep scoring them.
  - A clause moves to the snapshot source only through a registration
    that prints both sources' A/A resolution on the same rows.
  - The snapshot file is committed (scrubbed) beside the logs.
- **The goodput clause** keeps the perf JSON `mbps` as its scored value.
  `transfer.goodput` must agree with it on every row (a parity check),
  and it gives TUN-mode runs a goodput at all.
- **cc_sim host gates** (`transport/cc_sim.rs`) build a registry without
  an engine. They read the same `cc.wire.*` handles the VM readouts use,
  so §48's R2a, R2b and R4 are one implementation in the simulator and in
  vivo. This replaces parsing quinn's `Debug` string, whose cost was 4–15 µs
  per read (`qcns`, §48b), with tick-time reads.

### 8.3 Single source: the bounding tests
- **`metrics::tests::three_renderings_one_snapshot`.**
  - Fill a registry with distinct primes per cell, plus `TagLocals`.
  - Render `/status`, every tag, and the OTLP request (encoded, then
    decoded).
  - Assert every value equal across renderings: exact for integers;
    no unit conversion anywhere (every instrument exports in its recorded
    unit); for a histogram
    quantile against a tag's nearest-rank, within one bucket (9.05 %).
  - Any rendering that deliberately differs (rounding in a tag) has its
    own bounding assertion. No difference is left as prose.
- **`tests/metrics_loopback.rs`.**
  - Run the engine on loopback with `/status`, a tag gate, and an
    in-process fake OTLP receiver (an axum route: no new dependency).
  - At the end, compare the final `/status`, the final tag line, and the
    last received OTLP request.
  - This proves the wiring routes to the registry, not just the
    formatters (measurement rule 1: the mechanism executes).
- **`metrics::tests::instruments_are_write_only_for_laws`.**
  - The registry API is split by visibility: `record` is `pub(crate)`,
    while `Snapshot` and the cell reads are `pub(in crate::monitor)`.
  - A source-pin test (the `no_eprintln_in_engine_source` precedent,
    `readout.rs:66`) asserts that no file under `scheduler/`, `control/`,
    `fec/`, or the `net/` law modules names `Snapshot`, a detector, or a
    recorder read.
  - Instruments can thus never change a law, by construction.

### 8.4 Always-on minimum against gated deep instrumentation
- **Always on.**
  - The §4 catalogue in classes H0, H1, H2 and T: ≤ 10 records per
    symbol, ≤ 0.5 % of a thread.
  - The sampled flight-recorder ring.
  - Per Q3, possibly the full-rate small ring.
- **Gated (each priced by its own registration, as today).**
  - `[LAT]`'s `SeqRing` decomposition beyond the end-to-end histogram;
    `[SUCC]`'s hole map; `[SUCCDUMP]`; `[RTTDUMP]`; `[ACKDIAG]`.
  - `[CPUPROF]` seams; `RWM_RTOBS=2` (`[TASK]`, `[QSOCK]`, `[RXSEC]`,
    `[TXSEC]`); the 10 ms lag probe if Q4 keeps it gated.
  - quinn qlog; perf sampling.
- A gated instrument appears in the registry (and so in OTLP and
  `/status`) when its gate is on, under the same name, so a deep run and
  a shipped run share one schema.

### 8.5 CPU profiling with standard formats
- **VM.**
  - `perf record -F 997 -g --call-graph dwarf -p <pid>`, on a
    **dedicated, unscored rerun**: sampling perturbs, the rule already
    applied to `[RTTDUMP]`.
  - `perf script | inferno-collapse-perf` gives folded stacks;
    `inferno-flamegraph` gives SVG; `inferno-diff-folded` gives the
    differential flamegraph of CAND against BASE.
  - The differential is the standard instrument for the §48b c8 CPU
    question. It would have priced "finer slow-leg batching" directly
    instead of fitting a per-batch price.
  - `perf_to_profile`, or the folded stacks, gives pprof for tooling.
- **Host.** `cargo bench` (criterion) for ns questions; `cargo flamegraph`
  for local profiles.
- **`RWM_CPUPROF`** keeps its seams. Its totals render as registry
  counters `raptorpath.cpu.seam.time` (class G, one counter per seam), so
  its `unattr` share sits beside `process.cpu.time` in one snapshot.
- **The OTLP profiles signal** (`opentelemetry-proto` feature `profiles`)
  is still Development status. It is noted for later and not adopted.

### 8.6 The guard's blind spots
- **CPU below ≈ 5 %** (the §48b c2/c8 CPU/GB d80 of 5–8 %).
  - **(i)** Score CPU over the **transfer window** from in-process
    `process.cpu.time`. Today's `/usr/bin/time` covers the whole
    invocation, including warm-up and handshake (`perf_rwm_c.sh:480`).
  - **(ii)** Add `perf stat -e instructions:u` per row as a second CPU
    clause.
  - **(iii)** Keep the per-task split (`[THR]` parks plus owner batches,
    now always on), so that *which* task moved is read, not inferred.
- **Lag at 20–100 %.**
  - `lag_E` = p99 − min of one window of 10 ms probes. The min is an
    extreme and the p99 rests on a few hundred samples.
  - Score from the whole-transfer lag histogram with its count, and use
    a less extreme statistic (p99 − p50, or the mean excess over p50).
- **Every new clause enters by registration,** with its A/A resolution
  table printed from BASE alone first (§41c's own practice). Old clauses
  are not re-scored.

## 9. Phased plan (red-first, pre-registered, NO-MODE-SWITCH)

Each phase is one branch and one merge. No phase changes a law, a gate
default, or a byte of any committed log format.

| phase | content | red-first pins | evidence to merge |
|---|---|---|---|
| P0 | this doc + ADR-0074 (Proposed) | — | user decision on Q1–Q6 |
| P1 | `monitor/metrics/` core: const descriptors, cells, the exact scale-3 exponential histogram, exemplar slots, `Snapshot`, collector; write-only visibility split. No call site moves | bucket-boundary test; `alloc`-free record (counting allocator); `instruments_are_write_only_for_laws` | host suites; ns bench recorded |
| P2 | `/status` truth: v2 additive objects; fix or move every §2 row; the handler serves `Snapshot` | one red test per §2 row (e.g. `status_rtt_is_one_clock`, `status_blocks_have_a_writer_or_are_absent`, `status_cwnd_is_labelled_app_window`) | suites; Q1 applied |
| P3 | always-on instruments: move `PathStats`, `FecStats` and the owner counters into cells; add the missing §4 rows (rtt.wire/echo, dwell, holes, clock, horizon, store, dial, CPU window, batching, timer lag if Q4) | reachability test per instrument (precedent: `*_reachability.rs`), proving the record site executes on loopback | ns bench under budget; `alloc_per_symbol` unchanged; a **pre-registered metrics-on/off guard session** (§41c statistic) as backstop |
| P4 | tags render from `Snapshot` + `TagLocals`, one tag per commit, in harness-weight order: `[DIAG]`, `[LAT]`, `[LAG]`/`[THR]`, `[FCAUSE]`/`[HOLD]`, `[SUMCAP]`/`[DCAP]`, `[RDIAG]`, then the rest | the ledger round-trip test per tag, green on the **old** formatter before the move | round-trip stays green; `test_*_parse.py` suites green |
| P5 | `RWM_METRICS_SNAPSHOT` + `l1common.registry_*` + parity tests | parity test red until the snapshot carries the clause | a rehearsal capture with parity printed |
| P6 | feature `otel`: OTLP/HTTP metrics exporter, `OTEL_*` parsing with loud failures, fake-receiver loopback | `three_renderings_one_snapshot`; env parser table test | VM build memory measured with the feature on (it never enters a battery binary before that) |
| P7 | flight recorder, detectors, `fr2otel.py` (OTLP traces, qlog, Perfetto), exemplars, online low-rate spans | ring wrap/swap tests; a detector test per signature on cc_sim/loopback; converter round-trip on a synthetic dump | cost per §7.2 measured; Q3 decided on it |
| P8 | CPU and lag clause upgrades (§8.6) | — | a registration with the A/A resolution of each new clause |

## 10. Dependencies and risk

- **No feature (default build).**
  - P1–P5 and P7 add **no crate**: atomics, serde_json, axum and tokio
    are already locked.
  - The wasm crates are never touched.
- **`otel` feature.**
  - `opentelemetry-proto` 0.33.1 (MSRV 1.75; the host toolchain is
    1.95), with features `metrics,trace,logs` plus either
    `gen-tonic-messages` (`prost` 0.14 only; `tonic` stays out unless
    feature `gen-tonic`) or `with-serde` (`serde`, `const-hex`,
    `base64`).
  - **Its manifest makes `opentelemetry` and `opentelemetry_sdk` 0.33.1
    non-optional dependencies** (`Cargo.toml:106, 110`). They compile in,
    but the design calls neither at run time. That is compile time and
    memory, not hot-path risk.
  - `hyper-util` (client) on the already locked `hyper` 1.8.
  - `hyper-rustls` only under `otel-tls`.
- **VM build memory.** The `prost` derive expansion of the OTLP protos is
  the heavy part. Measure peak RSS of `cargo build --release --features
  otel` under the host mutex and once on the VM before the feature enters
  any battery binary. The default build is unchanged.
- **Version drift.** The SDK's record path, exemplar support and
  bound-instrument status were read at 0.33.1. Any bump re-reads them
  (§5.4), since the reasons this design bypasses the SDK may change.
- **Semantic-convention drift.** Only stable `process.*` names are used.
  Every other name is `raptorpath.*`, versioned by the OTLP scope version.

## 11. Open questions for the user

- **Q1. `/status` v1 dead and misleading keys.** `blocks.*`,
  `fec.actual_failure_rate` and `fec.pi_correction` have no writer. Should
  they be removed, kept under `legacy`, or kept as-is and documented?
  Should `cwnd`, `rtt_ms`, `jitter_us`, `throughput_mbps` and
  `symbols_received` be renamed? Renaming breaks the `status` CLI and
  `monitoring_test.rs`. The proposal is additive v2, with old keys fixed
  to one documented meaning.
- **Q2. OTLP protocol.** Choose between:
  - `http/protobuf`, the spec default, which adds `prost`;
  - `http/json` only, which adds no `prost` but is a non-default protocol
    the operator must name.
  
  The recommendation is (a).
- **Q3. Flight-recorder default.** Choose between:
  - a full-rate ring always on (8 MB per end, ≈ 0.05–0.07 % of a core,
    ≈ 2.8 s of c1s history);
  - only the 1/64 sampled ring always on, with full rate on request.
  
  Same code, different mask. The decision should follow P7's measured
  cost.
- **Q4. Timer lag.** Should the 10 ms lag probe (today `RWM_RTOBS≥1`)
  become always on as a histogram? This is the fix for the lag clause's
  blind spot. Its cost is one task wake per 10 ms.
- **Q5. Collector tick and export interval.** The proposal is a 1 s
  registry tick and the spec's 60 s OTLP export by default, set by
  `OTEL_METRIC_EXPORT_INTERVAL`.
- **Q6. ADR-0013 Layer 4.** Confirm that it is superseded. Prometheus
  then comes through a collector's Prometheus exporter, not a `/metrics`
  endpoint. A native `/metrics` text rendering of the same `Snapshot` would
  be a fourth renderer of about 100 lines, if wanted.
