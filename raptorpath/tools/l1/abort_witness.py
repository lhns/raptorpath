#!/usr/bin/env python3
"""Reader for the ABORT-CAUSE WITNESS record written by `abort_witness.sh`.

goal-gate "Candidates Battery — RESULTS", THE ABORT CLASS row: the class was
left NEEDS-MORE with a named instrument, and this is the parser half of it.

WHY A SHARED MODULE AND NOT A COPIED BLOCK. `ccand_parse.py`, `ladder_parse.py`,
`flip_parse.py` and friends are the instruments earlier verdicts were read off
and they stay byte-identical — nothing here edits them. New batteries import
this module, so the `abort_cause=` column has ONE definition across sessions in
exactly the way `capbind_check.py` gives `CAPBIND` one.

THE RECORD is `key=value`, one line per key, values already sanitized to a
single line by the shell side. Unknown keys are carried through verbatim under
`aw_raw` rather than dropped: the witness is expected to grow capture points,
and a parser that silently discarded them would make the next capture point
invisible until someone remembered to edit this file too.

THE COLUMN CONTRACT, and the distinction that makes the column worth having:

  abort_cause   the FIRST failing instrumented step, or None when the record
                does not name one. First-write-wins is enforced on the shell
                side — everything downstream of the first failure is a
                consequence, and a last-write-wins witness would attribute
                every abort to `cli_exec`.

                  busy_precheck     `pgrep -x raptorpath` hit after `cleanup`'s
                                    SIGTERM. NO log file exists on either
                                    endpoint. The SIGTERM race.
                  topo_up /         `topo*.sh up` returned non-zero; `topo_step`
                  topo_step         additionally names the line and command that
                                    failed inside `up()`.
                  srv_bind          no `:7000` within 20 x 0.3 s.
                  cli_exec          the client pipeline's first stage failed.
                  guard_cod0        the generation sanity guard fired.
                  no_gates_unknown  EVERY instrumented step reported success and
                                    the engine still never echoed `[GATES]`.
                                    THIS IS THE RESIDUAL, and it is the value
                                    that falsifies the witness's own four
                                    hypotheses. A class concentrated here needs
                                    a NEW instrument, not a re-reading of this
                                    one.
                  None              no record, or no cause: the invocation is
                                    not an abort as far as the witness saw.

  abort_missing TRUE when the record file itself is absent. NOT the same as
                `abort_cause=None`: a missing record means the witness never ran
                (an old driver, or a battery that did not copy the file), and
                treating that as "no abort" is exactly the silent-attribution
                error the fixed-path `/tmp/rwm-q.txt` clearing was written to
                prevent.

  drain_pids_t0 THE ARM-CORRELATION COLUMN. Survivors of the previous
                invocation's teardown at the instant the `BUSY` pre-check
                samples them — measured on EVERY invocation, aborted or not, so
                it has a control. If the aborting arms carry survivors here and
                the others do not, the c8/seed-7 correlation (20 % control vs
                75 % RACK) is EXPLAINED by shutdown duration, which is an arm
                property. If neither carries survivors, the race is CLEARED.
"""
import os

#: Causes that mean the invocation died BEFORE the engine could echo `[GATES]`,
#: i.e. the ones that produce the abort class this witness was built for. Kept
#: as a set rather than as a substring test so a new cause has to be classified
#: deliberately instead of matching a prefix by accident.
PRE_TRANSFER = {"busy_precheck", "topo_up", "topo_step", "srv_bind", "cli_exec"}

#: The residual. Named separately because its meaning is "the four hypotheses
#: are all false here", which is a finding and not a cause.
RESIDUAL = "no_gates_unknown"

_INT_KEYS = ("drain_pids_t0", "drain_left", "srv_bound", "srv_waits",
             "srv_alive", "srv_pid", "cli_rc", "gates_cli", "gates_srv")


def read_witness(path):
    """Parse one witness record into a flat column dict.

    Returns the columns unconditionally — a missing or unreadable record yields
    `abort_missing=True` with every other column `None`, so a caller never has
    to branch on file existence and can never mistake "not instrumented" for
    "no abort".
    """
    out = {
        "abort_cause": None,
        "abort_detail": None,
        "abort_pre_transfer": None,
        "abort_missing": True,
        "abort_step_fail": None,
        "topo_fail_cmd": None,
        "topo_fail_line": None,
        "drain_pids_t0": None,
        "drain_ms": None,
        "drain_left": None,
        "srv_bound": None,
        "srv_waits": None,
        "srv_alive": None,
        "cli_rc": None,
        "gates_cli": None,
        "gates_srv": None,
        "ping_pathA_rc": None,
        "ping_pathB_rc": None,
        "ping_single_rc": None,
        "aw_raw": None,
    }
    if not path or not os.path.exists(path):
        return out

    kv = {}
    try:
        with open(path, "r", errors="replace") as f:
            for ln in f:
                if "=" not in ln:
                    continue
                k, v = ln.rstrip("\n").split("=", 1)
                kv[k] = v
    except OSError:
        return out

    out["abort_missing"] = False
    for k in ("abort_cause", "abort_detail", "topo_fail_cmd", "topo_fail_line",
              "drain_ms", "ping_pathA_rc", "ping_pathB_rc", "ping_single_rc"):
        if k in kv:
            out[k] = kv[k]
    for k in _INT_KEYS:
        if k in kv:
            try:
                out[k] = int(kv[k])
            except ValueError:
                out[k] = None

    # Every `step_<label>_rc` that is non-zero, in file order. The FIRST is the
    # cause; the rest are the consequence chain, and they are kept because a
    # cause that is itself a symptom is only visible in the chain.
    fails = [k[len("step_"):-len("_rc")] for k, v in kv.items()
             if k.startswith("step_") and k.endswith("_rc") and v not in ("0", "")]
    out["abort_step_fail"] = ",".join(fails) if fails else None

    c = out["abort_cause"]
    out["abort_pre_transfer"] = (c in PRE_TRANSFER) if c else None
    # Carried verbatim so a capture point added to the shell side is visible in
    # the ledger the day it lands, without a second edit here.
    out["aw_raw"] = "; ".join(f"{k}={v}" for k, v in kv.items())
    return out


def cause_or(path, fallback="no_record"):
    """The one-token form for a driver's ledger line (`abort_cause=<token>`)."""
    w = read_witness(path)
    if w["abort_missing"]:
        return fallback
    return w["abort_cause"] or "none"


if __name__ == "__main__":
    import sys
    import json
    print(json.dumps(read_witness(sys.argv[1] if len(sys.argv) > 1
                                  else "/tmp/rwm-abort.txt"), indent=2))
