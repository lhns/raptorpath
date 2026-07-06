# Real cellular link traces — provenance

These are **real-world** time-varying cellular capacity traces recorded by a
mobile user with the "Saturator" tool, from:

> K. Winstein, A. Sivaraman, H. Balakrishnan, "Stochastic Forecasts Achieve
> High Throughput and Low Delay over Cellular Networks", USENIX NSDI 2013.

Source repository: <https://github.com/ravinet/mahimahi> (`traces/` directory),
downloaded 2026-07 from the `master` branch (raw.githubusercontent.com).

## Format

Each line is a timestamp in **milliseconds** from the start of the trace, and
represents an opportunity for one **1500-byte (12 kbit) packet** to be drained
from the bottleneck queue and cross the link. Repeated timestamps mean multiple
MTU packets could cross in that millisecond. These are **capacity** traces, not
loss traces (see the Rust harness for how a loss process is derived honestly via
a drop-tail queue at the trace's instantaneous capacity).

## Files vendored here

| file | network | dur | provenance |
|------|---------|-----|------------|
| `Verizon-LTE-short.down`     | Verizon LTE  | 140 s (full)    | verbatim |
| `ATT-LTE-driving-2016.down`  | AT&T LTE     | 120 s (full)    | verbatim |
| `TMobile-UMTS-driving.down`  | T-Mobile UMTS| first 120 s     | time-truncated |
| `TMobile-LTE-short.down`     | T-Mobile LTE | first 120 s     | time-truncated |
| `Verizon-LTE-driving.down`   | Verizon LTE  | first 120 s     | time-truncated |

Long driving traces are truncated to the first 120 s of wall-clock time to bound
repo size; truncation drops trailing lines only and does not alter the retained
loss dynamics. The `download-traces.sh` script alongside this file re-fetches the
full originals.
