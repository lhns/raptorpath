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
