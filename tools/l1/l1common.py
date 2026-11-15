"""Shared helpers for the L1 log parsers (r_parse, recvlaw_parse, place_parse,
eta_s4, place_score, latt_probe).

One definition of each reading rule, so two parsers cannot disagree about
what a gauge line says:

- `read` turns an endpoint log or a ledger into clean lines: a `tracing`
  record glued onto a readout line is split off first (`split_interleaved`),
  then ANSI colour codes and CR bytes are removed.
- `last_with` reads a CUMULATIVE gauge: its last line, except that an
  exit-flush line carrying `final=1` wins wherever it sits.
- `field` returns the token after `key=` where `key` starts a whitespace-
  delimited token (so `n=` never matches inside `gen=`); `-` (the engine's
  "n = 0" rendering) and a missing key are both None, never 0.
- `q` is the ONE quantile rule: linear interpolation between closest ranks
  (h = (n-1)·p), so the median of an even sample is the mean of the two
  middle values and does not flip between the lower and upper middle.

Every helper degrades to None instead of raising: a parser that dies on a
dead invocation deletes the rows the abort accounting is made of.
"""
import math
import re

ANSI = re.compile(r"\x1b\[[0-9;]*m")

# `final=1` as its OWN token: `xfinal=1` or `final=10` are not the flag.
FINAL_RE = re.compile(r"(?:^|\s)final=1(?:\s|$)")

# A `tracing` record (optionally colour-coded ISO timestamp + level) can be
# interleaved onto the SAME physical line as a gauge readout: the receiver's
# `[LAT] ... final=1` flush has been seen followed on one line by "cleaning up
# TUN interface", which glues the timestamp's first digit onto `final=1`.
# Split at the start of each embedded record BEFORE stripping colour codes.
_TRACE_SPLIT = re.compile(
    r"(?<!^)(?=(?:\x1b\[[0-9;]*m)?\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}"
    r"(?:\.\d+)?Z?(?:\x1b\[[0-9;]*m)?\s+(?:\x1b\[[0-9;]*m)?\s*"
    r"(?:TRACE|DEBUG|INFO|WARN|ERROR))"
)

_NUM_PREFIX = re.compile(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?")


def strip(line):
    """Remove ANSI colour codes, CR bytes and the trailing newline."""
    return ANSI.sub("", line).replace("\r", "").rstrip("\n")


def split_interleaved(line):
    """One physical log line -> the readout(s) it carries, each stripped."""
    # The optional colour code lets the lookahead fire twice at one record
    # (before and after the code), leaving an empty piece: drop those.
    pieces = [strip(p) for p in _TRACE_SPLIT.split(line)]
    return [p for p in pieces if p != ""] or [""]


def read(path):
    """A log file as a list of clean readout lines; [] if absent/unreadable."""
    if not path:
        return []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return [piece for ln in f for piece in split_interleaved(ln)]
    except OSError:
        return []


def is_final(line):
    """True iff the line carries the exit-flush `final=1` token."""
    return bool(line) and FINAL_RE.search(line) is not None


def last_with(lines, tag):
    """The reading of a cumulative gauge: the last line containing `tag`,
    except that a `final=1` line containing `tag` wins wherever it sits.
    None when the gauge never emitted (a reading, not a zero)."""
    last = None
    for ln in reversed(lines):
        if tag in ln:
            if is_final(ln):
                return ln.strip()
            if last is None:
                last = ln.strip()
    return last


def fnum(s):
    """A token -> float, or None (`-`, empty, malformed)."""
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def inum(s):
    """A token -> int, or None. Accepts an integral float token (`3.0`)."""
    try:
        return int(s)
    except (TypeError, ValueError):
        v = fnum(s)
        return int(v) if v is not None and v.is_integer() else None


def numeric_prefix(s):
    """The number a token starts with (`41.0ms` -> 41.0, `3/5` -> 3.0), or
    None when it does not start with one (`-`, `none`, empty)."""
    if s is None:
        return None
    m = _NUM_PREFIX.match(s)
    return float(m.group(0)) if m else None


def field(line, key):
    """The raw token after `key=` on a gauge line, or None.

    `key` may be given with or without its trailing `=` (a key ending in `:`
    is used as is). It must START a whitespace-delimited token, and the first
    such occurrence wins -- the aggregate head of a line precedes its
    per-path `p<id>:` slots. `-` is the absent reading and returns None."""
    if not line:
        return None
    if not key.endswith(("=", ":")):
        key += "="
    m = re.search(r"(?:^|\s)" + re.escape(key) + r"(\S*)", line)
    if not m or m.group(1) in ("", "-"):
        return None
    return m.group(1)


def gate_tok(lines, name):
    """The token `[GATES]` echoed for `name` on the LAST `[GATES]` line."""
    g = [ln for ln in lines if "[GATES]" in ln]
    if not g:
        return None
    m = re.search(r"(?<![\w])" + re.escape(name) + r"=(\S+)", g[-1])
    return m.group(1) if m else None


def gate(lines, name):
    """A boolean gate's echoed value (0/1) off the last `[GATES]` line."""
    t = gate_tok(lines, name)
    return int(t) if t in ("0", "1") else None


def q(v, p, ndigits=None):
    """THE quantile rule: linear interpolation between closest ranks,
    h = (n-1)·p (numpy's default). None on an empty sample."""
    xs = sorted(x for x in v if x is not None)
    if not xs:
        return None
    h = (len(xs) - 1) * min(max(p, 0.0), 1.0)
    lo = math.floor(h)
    hi = min(lo + 1, len(xs) - 1)
    r = xs[lo] + (h - lo) * (xs[hi] - xs[lo])
    return round(r, ndigits) if ndigits is not None else r


def med(v, ndigits=None):
    return q(v, 0.5, ndigits)


# ── Datagram-level loss truth ────────────────────────────────────────────
#
# `perf_rwm_c.sh` prints one `[TRUTH]` line per data-direction leg at run end
# (lib.sh `truth_compute`), to its stdout and into the `-q.txt` capture:
#
#   [TRUTH] leg=0 dev=cli0 egress_dgrams=N egress_skbs=S gso=G
#           netem_sent_dgrams=M netem_dropped_skbs=D backlog=B lost=L
#           loss=X rcvbuf_drops=R rcvbuf_scope=netns
#
# `loss` is the per-datagram wire loss of that leg (egress counter ahead of
# netem against netem's sent datagrams). netem's own `dropped` counts skbs and
# is never loss truth. `rcvbuf_drops` is per receiver NETNS, not per leg.

TRUTH_INT_KEYS = ("egress_dgrams", "egress_skbs", "netem_sent_dgrams",
                  "netem_dropped_skbs", "backlog", "lost", "rcvbuf_drops")
TRUTH_FLOAT_KEYS = ("gso", "loss")


def truth(lines):
    """`[TRUTH]` lines -> {leg (int): {key: value}}. Absent/`-` values are
    None. A leg printed more than once (a driver log and its `-q.txt` read
    together) keeps its LAST line. {} when no `[TRUTH]` line exists."""
    out = {}
    for ln in lines or []:
        if not ln or "[TRUTH]" not in ln:
            continue
        leg = inum(field(ln, "leg"))
        if leg is None:
            continue
        row = {"dev": field(ln, "dev")}
        for k in TRUTH_INT_KEYS:
            row[k] = inum(field(ln, k))
        for k in TRUTH_FLOAT_KEYS:
            row[k] = fnum(field(ln, k))
        out[leg] = row
    return out


def truth_columns(lines, n_legs=None):
    """The flat, additive parser columns for the truth lines:
    `truth_loss_p<i>`, `truth_lost_p<i>`, `truth_egress_p<i>`,
    `truth_gso_p<i>` per leg (`p<i>` lines up with the engine's `[DIAG] p<i>:`)
    and one `truth_rcvbuf_drops` (per netns). With `n_legs` every leg below it
    gets its columns, None when unread, so a row's shape does not depend on
    whether the capture existed."""
    t = truth(lines)
    legs = set(t)
    if n_legs:
        legs |= set(range(n_legs))
    cols = {}
    for i in sorted(legs):
        r = t.get(i, {})
        cols["truth_loss_p%d" % i] = r.get("loss")
        cols["truth_lost_p%d" % i] = r.get("lost")
        cols["truth_egress_p%d" % i] = r.get("egress_dgrams")
        cols["truth_gso_p%d" % i] = r.get("gso")
    rcv = [r.get("rcvbuf_drops") for r in t.values() if r.get("rcvbuf_drops") is not None]
    cols["truth_rcvbuf_drops"] = rcv[-1] if rcv else None
    return cols


# ── Runtime observability: [THR] and [LAG] (threading redesign P0) ───────
#
# The engine (src/runtime_obs.rs) prints, per window, one line per tokio worker,
# one per OS thread (Linux) and one sum line, then one [LAG] line:
#
#   [THR] rt phase=xfer side=server obj=1 worker=0 busy_s=1.000 busy_frac=0.500
#         park=20 unpark=17 wall_s=2.000
#   [THR] os phase=xfer side=server obj=1 tid=11 comm=rp-w-0 cpu_s=1.000
#         cores=0.500 wall_s=2.000
#   [THR] sum phase=xfer side=server obj=1 workers=1 busy_s=1.000 threads=2
#         cpu_s=1.400 cores=0.700 wall_s=2.000
#   [LAG] phase=xfer side=server obj=1 tick_ms=10 n=200 p50_us=600 p99_us=980
#         max_us=1500 dropped=0
#
# `phase=xfer` brackets one perf object (`run=<r>` on the client, `obj=<id>`
# on the server; the server's warm-up object 0 has its own tiny window);
# `phase=run` is cumulative since process start (no window key).

_WIN_KEYS = ("run", "obj")


def _window_of(line):
    """The window key of a [THR]/[LAG] line: `run=1` / `obj=3`, or '' (none)."""
    for k in _WIN_KEYS:
        v = field(line, k)
        if v is not None:
            return "%s=%s" % (k, v)
    return ""


def _thr_windows(lines, phase):
    """{window key: [lines]} of the [THR]/[LAG] lines of `phase`."""
    out = {}
    for ln in lines or []:
        if not ln or ("[THR] " not in ln and "[LAG] " not in ln):
            continue
        if field(ln, "phase") != phase:
            continue
        out.setdefault(_window_of(ln), []).append(ln)
    return out


def thr(lines, phase="xfer", window=None):
    """One [THR]+[LAG] window -> a dict, or None when that window is absent.

    `window` is the key (`'run=1'`, `'obj=1'`); None picks the window whose
    `[THR] sum` line has the LONGEST `wall_s` (the measured object, not the
    server's 64-byte warm-up). Keys: `window`, `side`, `wall_s`, `workers`
    (list of {worker, busy_s, busy_frac, park, unpark}), `threads` (list of
    {tid, comm, cpu_s, cores}; None when the OS reader was unavailable),
    `sum` ({workers, busy_s, threads, cpu_s, cores}), `lag` ({n, p50_us,
    p99_us, max_us, dropped} or None)."""
    wins = _thr_windows(lines, phase)
    if not wins:
        return None
    if window is None:
        best, best_wall = None, -1.0
        for k, ls in wins.items():
            for ln in ls:
                if "[THR] sum " in ln:
                    w = fnum(field(ln, "wall_s"))
                    if w is not None and w > best_wall:
                        best, best_wall = k, w
        if best is None:
            return None
        window = best
    ls = wins.get(window)
    if not ls:
        return None
    res = {"window": window, "side": None, "wall_s": None, "workers": [],
           "threads": None, "sum": None, "lag": None}
    for ln in ls:
        res["side"] = res["side"] or field(ln, "side")
        if "[THR] rt " in ln:
            res["workers"].append({
                "worker": inum(field(ln, "worker")),
                "busy_s": fnum(field(ln, "busy_s")),
                "busy_frac": fnum(field(ln, "busy_frac")),
                "park": inum(field(ln, "park")),
                "unpark": inum(field(ln, "unpark")),
            })
        elif "[THR] os " in ln and "[THR] os unavailable" not in ln:
            if res["threads"] is None:
                res["threads"] = []
            res["threads"].append({
                "tid": inum(field(ln, "tid")),
                "comm": field(ln, "comm"),
                "cpu_s": fnum(field(ln, "cpu_s")),
                "cores": fnum(field(ln, "cores")),
            })
        elif "[THR] sum " in ln:
            res["wall_s"] = fnum(field(ln, "wall_s"))
            res["sum"] = {
                "workers": inum(field(ln, "workers")),
                "busy_s": fnum(field(ln, "busy_s")),
                "threads": inum(field(ln, "threads")),
                "cpu_s": fnum(field(ln, "cpu_s")),
                "cores": fnum(field(ln, "cores")),
            }
        elif "[LAG] " in ln and "[LAG] io " not in ln:
            # `[LAG] io` (threading Q1) is a dedicated I/O runtime's probe,
            # read by `io_columns`; this is the main runtime's.
            res["lag"] = {
                "n": inum(field(ln, "n")),
                "p50_us": fnum(field(ln, "p50_us")),
                "p99_us": fnum(field(ln, "p99_us")),
                "max_us": fnum(field(ln, "max_us")),
                "dropped": inum(field(ln, "dropped")),
            }
    return res


def thr_columns(lines, prefix, phase="xfer", window=None):
    """Flat, additive row columns for one [THR]/[LAG] window, all keyed
    `<prefix>_…` (None when unread, so the row shape never depends on the
    capture): wall_s, n_workers, busy_frac per worker RANKED descending
    (`busy_r1` is the busiest worker, whichever index it had: tasks migrate),
    park/unpark totals per second of wall, the OS threads' cores ranked
    (`thr_r1..3`) with the hottest one's comm (`top_comm`), the main thread
    (`main_cores`, comm `raptorpath`), all `rp-w-*` threads summed
    (`workers_cores`), the process sum (`cores`), and lag p50/p99/max (µs)."""
    t = thr(lines, phase, window)

    def k(s):
        return "%s_%s" % (prefix, s)

    cols = {k("wall_s"): t["wall_s"] if t else None,
            k("n_workers"): len(t["workers"]) if t else None}
    ws = t["workers"] if t else []
    ranked = sorted((w["busy_frac"] for w in ws if w["busy_frac"] is not None), reverse=True)
    for i in range(6):
        cols[k("busy_r%d" % (i + 1))] = ranked[i] if i < len(ranked) else None
    wall = t["wall_s"] if t else None
    for key in ("park", "unpark"):
        vals = [w[key] for w in ws if w[key] is not None]
        cols[k(key + "_per_s")] = (sum(vals) / wall) if (vals and wall) else None
    th = [x for x in ((t["threads"] if t else None) or []) if x["cores"] is not None]
    tranked = sorted(th, key=lambda x: x["cores"], reverse=True)
    for i in range(3):
        cols[k("thr_r%d" % (i + 1))] = tranked[i]["cores"] if i < len(tranked) else None
    cols[k("top_comm")] = tranked[0]["comm"] if tranked else None
    mains = [x["cores"] for x in th if x["comm"] == "raptorpath"]
    cols[k("main_cores")] = mains[0] if mains else None
    rpw = [x["cores"] for x in th if (x["comm"] or "").startswith("rp-w-")]
    cols[k("workers_cores")] = sum(rpw) if rpw else None
    s = t["sum"] if t else None
    cols[k("cores")] = s["cores"] if s else None
    lg = t["lag"] if t else None
    for key in ("p50_us", "p99_us", "max_us", "n"):
        cols[k("lag_" + key)] = lg[key] if lg else None
    return cols
