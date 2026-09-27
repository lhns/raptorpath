# Measurement discipline

The standing rules for every L1 (real kernel stack, benchmark VM) verdict in
this repository. A measurement section that misses any rule is not a verdict.
The rules keep their numbers 1–18, so a citation of "MEASUREMENT DISCIPLINE n"
anywhere in the tree points here. Decision record: ADR-0052.

Each rule is stated, then followed by one line of why it exists.

## The eighteen rules

1. **Prove the mechanism under test executed.** The recorded run must show it
   ran: a harness liveness guard (for example `cod>0` → `GUARD OK`), the
   gate's echo in `[GATES]`, or the enabling flag in the recorded command line.
   *Why:* six verdicts were once merged on runs in which the mechanism never
   executed; dead code measures noise.

2. **Record the full command line, environment and binary.** The exact
   harness invocation, every `RWM_*` variable, the commit and the binary's
   `sha256`. *Why:* without them a result can be neither reproduced nor
   voided, only called uncertain.

3. **Interleave same-binary arms within one session.** A/B arms alternate
   rep by rep on one binary. *Why:* same-configuration drift across sessions
   has measured 2.3×, larger than most claimed effects.

4. **Report both seeds and per-run distributions.** Per-seed medians and
   every per-rep value, not only pooled means. *Why:* pooled means hide
   bimodality.

5. **An effect must exceed the recorded noise floor.** A claimed delta must
   exceed the measured same-configuration spread and cross-session drift.
   *Why:* every effect of one whole era sat inside that spread.

6. **Prove liveness at the receiver, not only the sender.** A recovery
   verdict needs a receiver-side signal (decoder echo, `repairs_useful > 0`),
   not only emission counters. *Why:* repairs were once emitted and silently
   dropped by a mismatched decoder, so the run measured wire load only.

7. **Every battery arm must produce rows, or fail loudly.** Guard match
   pipelines with `|| true` under `set -e` and assert a per-arm result count;
   an arm with zero summaries fails the battery. *Why:* a no-match `grep`
   under `set -e` once removed whole arms without an error.

8. **Record known abort classes; never discard a result.** Aborted
   invocations (the seed-7 bring-up double-abort is the documented class) are
   recorded and retried per protocol; n is quoted per arm, and no mean with
   n < 8 appears without its n. *Why:* silent retries and dropped rows bias
   every downstream number.

9. **The hardware era is part of the configuration.** Put `lscpu` in every
   log header and compare only within one era; a cross-era comparison names
   the divide, and a verdict older than these rules is cited only with its
   era named. *Why:* the CPU feature set changes absolute throughput and CPU
   numbers.

10. **The synced tree must be CR-free.** Verify `tools/l1/lib.sh` holds 0 CR
    bytes (by count) before the first harness invocation; convert if not.
    *Why:* CRLF scripts fail or misbehave under bash on the VM.

11. **Pre-register before building or measuring.** Write the mechanism, the
    predicted effect size and cells, the falsification condition, and a
    re-read of the derivation for self-contained predictions of failure. A
    build whose prediction fails goes to the deprecation register, not to
    iteration, unless the failure names a new mechanism. *Why:* if the
    derivation already bounds the effect below relevance, the build is waste.
    The full protocol is below.

12. **The VM lock covers all VM activity.** Builds, probes, `iperf3`/`ping`,
    netem/tbf setup and cell validation all wait for the lock; non-holders
    work locally. *Why:* co-tenancy has contaminated measurements.

13. **The lock holder does not poll a running battery.** Launch detached,
    wait once for the expected duration (or on the terminal sentinel), and
    collect at the end; if progress must be checked, at most once per ~20 min,
    and record it. *Why:* each poll's SSH session and log scan on a loaded box
    produced the same aborted-invocation signature as a real abort class
    (121 retries polled vs 0 unpolled on one seed).

14. **Characterize the mechanism at component level before any L1 battery.**
    Drive it alone, locally and deterministically, through the shipped laws
    (extract them to pure functions if needed); report a distribution and
    which branch fired; state the number the battery should then see and what
    the bench cannot see. If the bench refutes the premise, the battery is
    not run. *Why:* both battery-first falsifications on record were premise
    errors a component bench would have caught in minutes.

15. **Every arm asserts its gate's echo, and drivers forward gates
    explicitly.** (a) One forwarding list: `tools/l1/lib.sh` owns
    `RWM_FORWARD`/`rwm_forward_env()`, and the forwarding audit test fails on a
    missing or stale knob. (b) Every gate has an echo (`[GATES]` or its own
    line). (c) Assert it two-sided on both endpoint logs: the arm shows ON and
    the control shows OFF. (d) Never rely on environment inheritance; a
    driver that must withhold a variable `unset`s it. *Why:* a gate with no
    echo cannot be shown to have reached the binary, so any verdict on it is
    unfalsifiable.

16. **Check every performance target against the cell's physical ceiling.**
    State the cell's shaped capacity, the baseline's measured utilisation
    (`tc -s qdisc` on every cell) and the headroom beside the target. Below 5 %
    headroom, write no throughput target for that cell; score a free axis
    (latency, occupancy, loss, CPU). The same check applies to
    no-regression clauses. *Why:* a target the link cannot produce fails
    identically for a good and a bad mechanism, so it carries no information.

17. **Every law carries an always-on scaling-structure test, and the
    unclamped formula is tested apart from its clamp.** Property-test the
    shape (ratio over N = 1…8, degree in each input, monotonicity and
    continuity in each dial) on synthetic inputs over ranges no cell reaches;
    assert the unclamped expression separately; check the formula against its
    sentence in the paper for shape before comparing numbers. A clamp may
    never be the only thing that makes a law sane. *Why:* a law passed nine
    absolute pins for a month while quadratic where its own comment described
    a linear quantity, because every test lived at N ∈ {1, 2}.

18. **A law measured pinned or degenerate is a defect finding with its own
    verdict.** Every clamp carries a bind-fraction gauge; above the
    threshold the report says "this law operates as a constant", a verdict
    names the law and the clamp and re-scopes earlier results, and no
    mechanism verdict is drawn from an arm in which that mechanism's law was
    pinned. *Why:* a clamp that always binds turns the law into a constant,
    and every measurement through it measures the constant.

## Verdict taxonomy

A pre-registration names its own outcome set; these are the shared terms, each
defined once. No verdict outside the pre-registered set may be recorded.

| term | meaning |
|---|---|
| `DELIVERED` | The pre-registered deliverable exists and every liveness witness and criterion it named is met. |
| `FLIP-RECOMMENDED` | An arm met every pre-registered flip clause. It is a recommendation: no battery flips its own default; the flip is a separate, reviewed commit. |
| `REFUTED-WITH-RECORD` | A pre-stated refuter fired. The mechanism is recorded as refuted with its data and goes to the deprecation register. |
| `NEEDS-MORE-<named>` | The battery cannot decide (power, confound, missing instrument). The close must name the instrument or measurement that would decide it. A null without power is never read as a refutation. |
| `INERT-AS-DERIVED` | The arm's execution witness fires, but no scored dimension moves beyond the control's spread. A result, not a failure: the derived form runs and does not matter at these cells. |
| `GUARD-UNDERPOWERED` | A guard leg (typically goodput) whose n cannot resolve the effect. Differences are reported with this label and never scored. |
| `UNSCOREABLE` | Fewer live rows at a scored cell than the pre-stated minimum after aborts and voids, or witnesses failing at a pre-stated share of reps. Nothing at that cell is scored. |
| `VOID` | A row or clause invalidated by a pre-stated condition (wrong-era binary, failed witness, a calibration that contradicts a permission). Listed in a void table, excluded from every denominator, never re-scoped after the fact. |
| `OUTSIDE THE PRE-REGISTERED SET` | An observation no pre-registered outcome names. Recorded as a finding with no verdict; the verdict is carried only by outcomes that fire literally. |
| `CONTROL-MOVED` | The control arm (or a scored column at a control cell) left its pre-stated identity or spread. No arm at that cell is scored against it. |

A rep with no summary line is a skipped datum (`NO_DATA`), not a zero and not an
abort. An absent gauge is printed `-` if and only if n = 0.

## Pre-registration protocol

1. **Its own commit, before VM contact.** The pre-registration is committed
   before the VM is touched and before any number exists. Engine arms, their
   unit tests and reachability tests are committed before it.
2. **The outcome set is fixed in advance.** Every outcome is named with its
   numeric condition, including the null and the "cannot score" outcomes.
   Amendments (for example cutting n to fit the time cap) are committed
   before any scored result is read, and change only what they say.
3. **Abort causes first, in priority order.** A table of abort classes with
   their tokens (lock, CRLF, binary `sha256`, sentinel unwritable, smoke,
   non-zero stage exit, bring-up exhausted) is part of the pre-registration,
   so an abort cannot be relabelled later. The scored section opens with the
   same table, filled.
4. **Smoke before battery.** A short smoke on the same binary must witness
   every required gauge and echo on both endpoints; if it fails, nothing is
   launched. Nothing in the smoke is a result.
5. **Sentinels are earned and proven writable.** The run directory is created
   unprivileged before any `sudo`; every absolute sentinel path is
   write+unlink probed at launch, not at exit. `DONE-*` is written only when
   its ledger exists, is non-empty and carries its own completion line;
   `FAILED-*` names the cause. The watcher waits on `DONE-ALL || FAILED-ALL`
   and on nothing else.
6. **Scored against the pre-registration only.** The scored section cites
   the pre-registration and its amendments and applies them literally.

## The VM protocol

- **Locks.** Take both `/tmp/rwm-vm.lock` and `/home/vibe/rp.lock` for the
  whole session (build, smoke and battery), and release both at exit.
- **Build and binary.** Build fresh on the VM from the committed branch;
  record the `sha256` and re-verify it before the smoke and before the battery.
- **CR check.** `tools/l1/lib.sh` holds 0 CR bytes after sync (rule 10).
- **Detached runs, no polling.** Launch with `setsid`/tmux; wait on the
  terminal sentinel (rule 13). Never `pgrep -f` a name that matches the
  watcher's own shell.
- **Process control.** `pkill -x raptorpath` (TERM, bounded wait, then KILL,
  as `stop_raptorpath` in `lib.sh` does) and nothing else.
- **Never touch** `ens18`, the firewall, `sshd`, or any network namespace not
  named `rp-*`.
- **Exit state, verified.** Zero `raptorpath` processes, no `rp-*` namespaces,
  both locks released.
- **Credentials stay out of git.** Docs say "the benchmark VM"; its address
  and key path are not committed.

## The five-hour cap

Every VM measurement is capped at 5 hours of wall time, smoke included. The
pre-registration budgets invocations from the measured per-invocation cost;
if the budget does not fit, n, seeds or cells are cut before launch and the
cut is written into the pre-registration. A battery that may overrun carries a
detached stopper that ends it at a balanced rep boundary or a hard deadline,
stops only `raptorpath`, lets the driver tear down its namespaces, clears both
locks and writes a named truncation sentinel (not a failure). A truncated
battery is scored at the n it reached, and a null at reduced power reads
`GUARD-UNDERPOWERED` or `NEEDS-MORE-<named>`, never a refutation.
