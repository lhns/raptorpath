#!/usr/bin/env python3
"""Precise object-transfer benchmark for the L1 harness.

Measures wall-clock completion of an N-byte object INCLUDING an
application-level ack (1 byte back), i.e. full delivery — the same
semantics as the L0 gate's completion metric. Microsecond resolution
(fixes iperf3's ~1s reporting floor on small objects).

Protocols:
  tcp    — kernel TCP; --cc sets TCP_CONGESTION (cubic|bbr|reno)
  mptcp  — kernel MPTCP v1 (IPPROTO_MPTCP=262); path manager must be
           configured (ip mptcp endpoint ...)

Server:  transfer_bench.py server --port 9900 [--proto tcp|mptcp]
Client:  transfer_bench.py client --host H --port 9900 --bytes 1800000 \
           --runs 10 [--proto tcp|mptcp] [--cc cubic]
Client emits one JSON line per run and a summary line.
"""

import argparse
import json
import socket
import statistics
import sys
import time

IPPROTO_MPTCP = 262
TCP_CONGESTION = 13
CHUNK = 65536


def make_socket(proto: str) -> socket.socket:
    if proto == "mptcp":
        return socket.socket(socket.AF_INET, socket.SOCK_STREAM, IPPROTO_MPTCP)
    return socket.socket(socket.AF_INET, socket.SOCK_STREAM)


def server(args) -> None:
    s = make_socket(args.proto)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind((args.bind, args.port))
    s.listen(8)
    print(f"server listening on {args.bind}:{args.port} proto={args.proto}", flush=True)
    while True:
        conn, _ = s.accept()
        try:
          # Objects loop on one connection (warm-flow, L2 ws3); clean
          # EOF at a header boundary ends the connection.
          while True:
            # First 16 bytes: decimal object size, space padded.
            hdr = b""
            while len(hdr) < 16:
                b = conn.recv(16 - len(hdr))
                if not b:
                    if hdr:
                        raise ConnectionError("short header")
                    raise EOFError
                hdr += b
            n = int(hdr.decode().strip())
            got = 0
            while got < n:
                b = conn.recv(min(CHUNK, n - got))
                if not b:
                    raise ConnectionError(f"short body {got}/{n}")
                got += len(b)
            conn.sendall(b"A")  # application-level delivery ack
        except EOFError:
            pass
        except ConnectionError as e:
            print(f"conn error: {e}", file=sys.stderr, flush=True)
        finally:
            conn.close()


def client(args) -> None:
    payload = b"\xa5" * CHUNK
    times = []
    warm = getattr(args, "warm", False)
    wc = None
    if warm:
        # Warm-flow geometry (L2 ws3): one connection for all objects,
        # comparable to quinn-perf's warm QUIC connection.
        wc = make_socket(args.proto)
        if args.cc and args.proto == "tcp":
            wc.setsockopt(socket.IPPROTO_TCP, TCP_CONGESTION, args.cc.encode())
        wc.settimeout(300)
        wc.connect((args.host, args.port))
    for run in range(1, args.runs + 1):
        if warm:
            c = wc
            t0 = time.perf_counter()
        else:
            c = make_socket(args.proto)
            if args.cc and args.proto == "tcp":
                c.setsockopt(socket.IPPROTO_TCP, TCP_CONGESTION, args.cc.encode())
            c.settimeout(300)
            t0 = time.perf_counter()
            c.connect((args.host, args.port))
        c.sendall(f"{args.bytes:<16d}".encode())
        left = args.bytes
        while left > 0:
            k = min(CHUNK, left)
            c.sendall(payload[:k])
            left -= k
        ack = c.recv(1)
        t1 = time.perf_counter()
        if not warm:
            c.close()
        if ack != b"A":
            raise RuntimeError("missing delivery ack")
        secs = t1 - t0
        times.append(secs)
        print(json.dumps({
            "proto": args.proto, "cc": args.cc, "bytes": args.bytes,
            "run": run, "seconds": round(secs, 6),
            "mbps": round(args.bytes * 8 / secs / 1e6, 3),
        }), flush=True)
    mean = statistics.mean(times)
    p = sorted(times)
    print(json.dumps({
        "summary": True, "proto": args.proto, "cc": args.cc,
        "bytes": args.bytes, "runs": args.runs,
        "mean_s": round(mean, 4),
        "min_s": round(p[0], 4),
        "median_s": round(p[len(p) // 2], 4),
        "max_s": round(p[-1], 4),
        "stdev_s": round(statistics.stdev(times), 4) if len(times) > 1 else 0.0,
        "mean_mbps": round(args.bytes * 8 / mean / 1e6, 3),
    }), flush=True)


def stream_server(args) -> None:
    """Receive a fixed-rate message stream; report one-way latency
    percentiles (send timestamps embedded; namespaces share the kernel
    clock, so one-way latency is directly measurable)."""
    s = make_socket(args.proto)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind((args.bind, args.port))
    s.listen(4)
    print(f"stream server on {args.bind}:{args.port}", flush=True)
    while True:
        conn, _ = s.accept()
        conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        lats = []
        buf = b""
        try:
            while True:
                b_ = conn.recv(65536)
                if not b_:
                    break
                buf += b_
                while len(buf) >= 4:
                    n = int.from_bytes(buf[:4], "big")
                    if len(buf) < 4 + n:
                        break
                    msg, buf = buf[4:4+n], buf[4+n:]
                    sent_ns = int.from_bytes(msg[:8], "big")
                    lats.append((time.time_ns() - sent_ns) / 1e6)
        finally:
            conn.close()
        if lats:
            v = sorted(lats)
            q = lambda p_: v[min(len(v)-1, int(len(v)*p_))]
            print(json.dumps({
                "summary": True, "mode": "stream", "count": len(v),
                "p50_ms": round(q(0.50), 3), "p90_ms": round(q(0.90), 3),
                "p99_ms": round(q(0.99), 3), "p999_ms": round(q(0.999), 3),
                "max_ms": round(v[-1], 3), "mean_ms": round(sum(v)/len(v), 3),
            }), flush=True)


def stream_client(args) -> None:
    """Send fixed-size messages at a fixed rate for a duration."""
    c = make_socket(args.proto)
    if args.cc and args.proto == "tcp":
        c.setsockopt(socket.IPPROTO_TCP, TCP_CONGESTION, args.cc.encode())
    c.connect((args.host, args.port))
    c.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    payload = b"Z" * max(0, args.size - 8)
    interval = 1.0 / args.rate
    end = time.perf_counter() + args.duration
    sent = 0
    nxt = time.perf_counter()
    while time.perf_counter() < end:
        msg = time.time_ns().to_bytes(8, "big") + payload
        c.sendall(len(msg).to_bytes(4, "big") + msg)
        sent += 1
        nxt += interval
        d = nxt - time.perf_counter()
        if d > 0:
            time.sleep(d)
    c.close()
    print(json.dumps({"stream_client_done": True, "sent": sent}), flush=True)


def main() -> None:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="mode", required=True)
    s = sub.add_parser("server")
    s.add_argument("--bind", default="0.0.0.0")
    s.add_argument("--port", type=int, default=9900)
    s.add_argument("--proto", default="tcp", choices=["tcp", "mptcp"])
    c = sub.add_parser("client")
    c.add_argument("--host", required=True)
    c.add_argument("--port", type=int, default=9900)
    c.add_argument("--bytes", type=int, default=1_800_000)
    c.add_argument("--runs", type=int, default=10)
    c.add_argument("--proto", default="tcp", choices=["tcp", "mptcp"])
    c.add_argument("--cc", default=None)
    c.add_argument("--warm", action="store_true",
                   help="reuse one connection for all objects")
    ss = sub.add_parser("stream-server")
    ss.add_argument("--bind", default="0.0.0.0")
    ss.add_argument("--port", type=int, default=9910)
    ss.add_argument("--proto", default="tcp", choices=["tcp", "mptcp"])
    sc = sub.add_parser("stream-client")
    sc.add_argument("--host", required=True)
    sc.add_argument("--port", type=int, default=9910)
    sc.add_argument("--size", type=int, default=1200)
    sc.add_argument("--rate", type=float, default=50.0)
    sc.add_argument("--duration", type=float, default=30.0)
    sc.add_argument("--proto", default="tcp", choices=["tcp", "mptcp"])
    sc.add_argument("--cc", default=None)
    args = ap.parse_args()
    if args.mode == "server":
        server(args)
    elif args.mode == "stream-server":
        stream_server(args)
    elif args.mode == "stream-client":
        stream_client(args)
    else:
        client(args)


if __name__ == "__main__":
    main()
