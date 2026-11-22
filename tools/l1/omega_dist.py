#!/usr/bin/env python3
"""netem delay tables for the disorder-price (omega) harness (docs/derivations/disorder-price-registration.md 4.1, 11.1).

A netem table entry x gives the delay  mu + sigma * x / 8192  (kernel NETEM_DIST_SCALE).  With mu = 0 and sigma = dt,
an entry of 8192 is +dt and an entry of 0 is no delay: the two-point table of 4.1.  A fraction p of the entries is 8192.
The zero table (all entries 0) is the CLEAN arm and the warm-up table.  Tables are loaded through TC_LIB_DIR; nothing is
written under /usr/lib.

  omega_dist.py make <dir> <name> <delayed_entries> <size>    write <dir>/<name>.dist
  omega_dist.py entries <rate_pps> <spacing_s> <L>            print the delayed-entry count, or SKIPPED-TABLE
"""
import os
import sys

SCALE = 8192


def entries_for(rate_pps, spacing_s, L):
    """Delayed entries of an L-entry table that give one event per spacing_s at rate_pps; None = SKIPPED-TABLE (p below 1/(2L))."""
    p = 1.0 / (rate_pps * spacing_s)
    n = int(round(p * L))
    return n if n >= 1 else None


def table(delayed, size):
    """size entries, `delayed` of them SCALE, spread evenly (Bresenham), the rest 0."""
    if not (0 <= delayed <= size) or size < 1:
        raise ValueError("bad table: %r of %r" % (delayed, size))
    out, acc = [], 0
    for _ in range(size):
        acc += delayed
        if acc >= size:
            acc -= size
            out.append(SCALE)
        else:
            out.append(0)
    return out


def write(path, tbl):
    with open(path, "w") as f:
        for i in range(0, len(tbl), 16):
            f.write(" ".join(str(v) for v in tbl[i:i + 16]) + "\n")


def main(argv):
    if len(argv) == 6 and argv[1] == "make":
        d, name, k, size = argv[2], argv[3], int(argv[4]), int(argv[5])
        os.makedirs(d, exist_ok=True)
        write(os.path.join(d, name + ".dist"), table(k, size))
        return 0
    if len(argv) == 5 and argv[1] == "entries":
        n = entries_for(float(argv[2]), float(argv[3]), int(argv[4]))
        print("SKIPPED-TABLE" if n is None else n)
        return 0 if n is not None else 1
    print(__doc__)
    return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv))
