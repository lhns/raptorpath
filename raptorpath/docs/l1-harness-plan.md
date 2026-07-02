# L1 Harness Plan — real stacks over netem (ADR-0051)

Upgrades the claim from "surpasses SimRetx/SimQuic models" (L0, in-process)
to "surpasses real TCP CUBIC / BBR / QUIC / MPTCP" — the same win
conditions as `docs/goal-gate.md`, measured on real kernel stacks over
emulated links.

## Test host

Fedora VM (see `.claude/vm-credentials.env` — NEVER committed; `.claude/`
is gitignored). Kernel with BBR, MPTCP (`net.mptcp.enabled=1`), netem,
network namespaces; 6 cores / 8 GB. Access: key-based SSH
(`~/.ssh/raptorpath_vm_ed25519`).

## Safety rules (hard, encoded in tools/l1/lib.sh)

The VM is reached over SSH via `ens18`. Breaking that interface locks us
out. Therefore:

1. **Never** attach qdiscs to, reconfigure, or move `ens18` (or the root
   namespace's `lo`). All shaping happens on veth devices INSIDE
   namespaces created by the harness.
2. All harness namespaces are prefixed `rp-`; cleanup only ever deletes
   `rp-*` namespaces and their veths. `tools/l1/cleanup.sh` is idempotent.
3. No firewall (firewalld/nftables) changes; no sshd config changes; no
   removal of networking packages.
4. Every experiment runs under `timeout`; topology scripts trap EXIT for
   cleanup.

## Topology (single path)

```
[rp-cli ns] cli0 ── veth ── srv0 [rp-srv ns]
     10.77.0.1/24            10.77.0.2/24

netem on BOTH veth egresses: delay <one_way> [jitter], rate <capacity>,
loss gemodel <p>% <q>%      (defaults 1-h=100%, 1-k=0% == paper h_B=1, h_G=0)
```

netem's Gilbert-Elliott IS the paper §2.4 model — parameters map verbatim.
Loss is applied on the data direction (srv-bound egress from cli0 carries
ACKs; apply GE loss on srv0→cli0? no: DATA flows cli→srv for upload tests;
we run iperf3 with the server in rp-srv, so data egress = cli0. Loss on
cli0 egress only, ACK path clean — matching the L0 gate's forward-loss
model. A `--symmetric` flag adds loss both ways for sensitivity checks.)

## Scenario map (identical to ADR-0051 / paper 2.4)

| Cell | rate | one-way delay | jitter | gemodel p q |
|------|------|---------------|--------|-------------|
| C1 DC | 1gbit | 1ms | 0 | 0.05% 50% |
| C2 WiFi | 100mbit | 5ms | 3ms | 1.3% 50% |
| C3 LTE | 20mbit | 20ms | 5ms | 2% 40% |
| C4 Sat | 20mbit | 100ms | 10ms | 3% 30% |
| C5 BadWiFi | 50mbit | 5ms | 3ms | 5.3% 30% |
| C7/C8 dual | two veth pairs between the same namespaces (phase 2) | | | |

## Baselines and phases

**Phase 1 (this iteration): real TCP.**
- iperf3 `-C cubic` and `-C bbr`, fixed transfer size (default 1.8 MB to
  match the L0 gate object, plus 100 MB for steady-state throughput),
  JSON output → completion time, retransmit counts, mean goodput.
- Expected: CUBIC collapses on lossy cells (the 1.22/sqrt(eps) law —
  validating the L0 SimRetx model), BBR mostly loss-blind (validating
  SimQuic-class behavior).

**Phase 2: QUIC + MPTCP.**
- quinn perf example (rust toolchain on the VM) for real QUIC/BBR-class.
- `mptcpize run iperf3` over the dual-path topology for MPTCP v1.

**Phase 3: raptorpath itself.**
- Build the raptorpath binary on the VM (Linux TUN backend), run
  sender/receiver in rp-cli/rp-srv namespaces over the same links, same
  transfer sizes. Gate conditions from docs/goal-gate.md apply unchanged:
  lossy cells completion <= 0.9x / p99 <= 0.7x best baseline; C1 tie;
  multipath beats best single + MPTCP.

**Phase 4: automation.**
- One driver script sweeping cells x baselines x N runs, seeds via netem
  `seed` option for reproducibility, results as JSON + a table appended to
  docs/goal-gate.md ("L1 results").

## Metrics

- Completion time (1.8 MB object): iperf3 JSON `sum.seconds` (and later
  raptorpath's own log).
- Goodput (100 MB steady): `sum_received.bits_per_second`.
- TCP retransmits: `sum_sent.retransmits`.
- Latency percentiles (phase 3, raptorpath vs QUIC): application-level
  timestamping; for TCP baselines, completion time is the primary metric
  (per-packet p99 needs the app layer).

## Status

- [x] VM reachable, key auth, sudo
- [x] iproute-tc, iperf3, kernel-modules-extra (sch_netem), mptcpd, jq
- [x] netem gemodel verified inside a namespace
- [ ] Phase 1 smoke: C2 CUBIC vs BBR (this session)
- [ ] rust toolchain on VM (for quinn + raptorpath)
- [ ] Phases 2-4
