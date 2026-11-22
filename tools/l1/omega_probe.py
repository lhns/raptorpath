#!/usr/bin/env python3
"""R0's paced, sequence-numbered probe (registration 11.1): UDP packets at a fixed rate whose IP id is the sequence number.

  python -I omega_probe.py <dst_ip> <rate_pps> <seconds> <sendlog>

Raw socket with a hand-built IP header (IP_HDRINCL), so the id is ours; run it inside an rp-* namespace as root (via the harness's nsx).
The send log has one line per packet, `<epoch> <ip id>`: it is R0's pre-reorderer reference, so no pre capture is needed. At most 65 536
packets per run (the id space): longer runs are refused.
"""
import socket
import struct
import sys
import time


def main(argv):
    if len(argv) != 5:
        print(__doc__)
        return 3
    dst, rate, secs, log = argv[1], float(argv[2]), float(argv[3]), argv[4]
    n = int(rate * secs)
    if n > 65536:
        print("REFUSED: more than 65536 packets would wrap the IP id", file=sys.stderr)
        return 3
    s = socket.socket(socket.AF_INET, socket.SOCK_RAW, socket.IPPROTO_RAW)
    s.setsockopt(socket.IPPROTO_IP, socket.IP_HDRINCL, 1)
    payload = b"\0" * 100
    udp = struct.pack("!HHHH", 40000, 9, 8 + len(payload), 0) + payload
    total = 20 + len(udp)
    d = socket.inet_aton(dst)
    t0 = time.perf_counter()
    e0 = time.time()
    with open(log, "w") as f:
        for i in range(n):
            target = t0 + i / rate
            while time.perf_counter() < target:
                pass
            hdr = struct.pack("!BBHHHBBH4s4s", 0x45, 0, total, i & 0xFFFF, 0, 64, 17, 0, b"\0\0\0\0", d)
            s.sendto(hdr + udp, (dst, 0))
            f.write("%.6f %d\n" % (e0 + (time.perf_counter() - t0), i & 0xFFFF))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
