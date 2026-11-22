#!/usr/bin/env python3
"""Stub runs of the disorder-price (omega) envelope: omega_run_all.sh EXECUTED end to end on a Linux CI host without root, against stubs.

Stubbed on PATH: sudo, id, getent, ip, tc, ethtool, sysctl, ss, tcpdump, iperf3, ping, flock, pgrep, pkill, sleep (scaled down), and the raptorpath binary
(OMEGA_BIN). Their state lives in a sandbox (namespaces, qdisc configs and counters, sysctls, a fake /proc); every stub appends the call to STUB_LOG with the
namespace it ran in, so the test can assert that nothing ran outside rp-*. The real omega_cells / omega_parse / omega_score / omega_dist run on synthetic
captures (omega_stubgen.py) derived from each row's own params, so the digest, the witnesses and the scorer are exercised, not mocked.
Not stubbable: the VM itself (netns, netem, tcpdump, iperf3, the tunnel). The tc / ip command lines are covered by this contract only; R0 verifies them on the VM.

Paths: rehearse, full (R then D1 with outage and stage T, then the scorer), lock-held, bad-ns, budget, phase-cap, stopper (TRUNCATED at a round boundary),
hard-deadline (TRUNCATED, scorer reads nothing), missing-witness, sysctl-not-per-ns, sysctl-mismatch, hoststate-changed, bench-refutes, no-seed,
table-too-small, ids-not-consecutive, no-plug, tun-bringup-fail, crlf, tools-missing, no-fill, rel-no-events, hold-reorders, plus the static checks.
Exit code = the number of failures. Run: python -I test_omega_stubrun.py [path ...]  (no argument = all)
"""
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
fails = 0
FAILED = []
DIAGBUF = []


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[-1500:]) if not ok else ""), flush=True)
    if not ok:
        fails += 1
        FAILED.append("FAIL %s -- %s" % (name[:70], str(detail).replace(chr(10), " | ")[-110:]))


def xt_tail(path, n=14):
    """The row loop's xtrace up to the point the cleanup starts (what ran last before the session ended)."""
    if not os.path.exists(path):
        return "(no xtrace)"
    L = open(path, errors="replace").readlines()
    cut = next((i for i, l in enumerate(L) if "omega_cleanup" in l), len(L))
    return "".join(x[:170] + chr(10) if len(x) > 170 else x for x in L[max(0, cut - n):cut])


def find_bash():
    cands = [shutil.which("bash")]
    if os.name == "nt":      # System32\bash.exe is WSL's launcher, not the MSYS bash the scripts need
        cands = [c for c in cands if c and "system32" not in c.lower()] + [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"]
    return next((c for c in cands if c and os.path.exists(c)), None)


BASH = find_bash()


def px(p):
    p = os.path.abspath(p).replace("\\", "/")
    m = re.match(r"^([A-Za-z]):/(.*)$", p)
    return "/%s/%s" % (m.group(1).lower(), m.group(2)) if m and os.name == "nt" else p


HARNESS = ["omega_run_all.sh", "omega_topo.sh", "omega_battery.sh", "omega_tunnel.sh", "omega_sampler.sh", "omega_cells.py", "omega_dist.py", "omega_parse.py", "omega_score.py",
           "omega_probe.py", "omega_stubgen.py", "lib.sh", "lib_battery.sh"]

LOG_STUB = 'echo "ns=${STUB_NS:--} %s $*" >> "$STUB_LOG"\n'
STUBS = {
    "sudo": '#!/bin/bash\necho "sudo $*" >> "$STUB_LOG"\n[ "$1" = "-n" ] && shift\nexec "$@"\n',
    "id": '#!/bin/bash\ncase "$1" in -un) echo vmuser ;; *) exec /usr/bin/id "$@" ;; esac\n',
    "getent": '#!/bin/bash\necho "$2:x:1000:1000::$STUB_HOME:/bin/bash"\n',
    "flock": "#!/bin/bash\nexit 0\n",
    "pgrep": '#!/bin/bash\n[ -n "$STUB_RAPTORPATH_ALIVE" ] && exit 0\nexit 1\n',
    "pkill": '#!/bin/bash\necho "pkill $*" >> "$STUB_LOG"\nexit 0\n',
    "ethtool": "#!/bin/bash\n" + LOG_STUB % "ethtool" + "exit 0\n",
    "ping": "#!/bin/bash\n" + LOG_STUB % "ping" + '[ -n "$STUB_PING_FAIL" ] && exit 1\nexit 0\n',
    "ss": "#!/bin/bash\n" + LOG_STUB % "ss" + 'echo "ESTAB 0 0 10.99.0.1:40000 10.99.0.2:5301"\necho "	 cubic wscale:7,7 rto:210 rtt:12.5/3 cwnd:100 minrtt:2.5"\n',
    "sleep": '#!/bin/bash\nexec /bin/sleep "${STUB_SLEEP_S:-0.01}"\n',
    "ip": r'''#!/bin/bash
echo "ns=${STUB_NS:--} ip $*" >> "$STUB_LOG"
case "$1" in
  netns)
    case "$2" in
      add) mkdir -p "$STUB_STATE/ns"; : > "$STUB_STATE/ns/$3"; exit 0 ;;
      del) rm -f "$STUB_STATE/ns/$3"; rm -rf "$STUB_STATE/tc/$3"; exit 0 ;;
      list) for f in "$STUB_STATE"/ns/*; do [ -e "$f" ] && echo "$(basename "$f") (id: 0)"; done; exit 0 ;;
      exec) ns="$3"; shift 3; export STUB_NS="$ns"
            if [ "$1" = test ]; then k="${3##*/}"; case " $STUB_SYSCTL_ABSENT " in *" $k "*) exit 1 ;; esac; exit 0; fi
            exec "$@" ;;
    esac ;;
esac
exit 0
''',
    "sysctl": r'''#!/bin/bash
echo "ns=${STUB_NS:--} sysctl $*" >> "$STUB_LOG"
d="$STUB_STATE/sysctl/${STUB_NS:--}"; mkdir -p "$d"
case "$1" in
  -w) k="${2%%=*}"; echo "${2#*=}" > "$d/$k"
      if [ -n "$STUB_HOST_CHANGE" ] && [ ! -e "$STUB_STATE/hostchanged" ]; then echo 9 > "$OMEGA_PROCFS/sys/net/ipv4/tcp_sack"; : > "$STUB_STATE/hostchanged"; fi
      exit 0 ;;
  -n) if [ "$2" = "net.ipv4.$STUB_SYSCTL_WRONG" ]; then echo 99; exit 0; fi
      cat "$d/$2" 2>/dev/null || echo 0 ;;
esac
''',
    "tc": r'''#!/bin/bash
echo "ns=${STUB_NS:--} tc $*" >> "$STUB_LOG"
d="$STUB_STATE/tc/${STUB_NS:--}"; mkdir -p "$d"
[ "$1" = -s ] && shift
if [ "$1" = qdisc ]; then
  case "$2" in
    replace|add|change)
      dev=""; args=("$@")
      for ((i = 0; i < ${#args[@]}; i++)); do [ "${args[i]}" = dev ] && dev="${args[i+1]}"; done
      case " $* " in *" plug "*) [ -n "$STUB_NO_PLUG" ] && exit 2 ;; esac
      case " $* " in *" seed "*) [ -n "$STUB_NO_SEED" ] && exit 2 ;; esac
      case " $* " in *" distribution "*)
        name=""; for ((i = 0; i < ${#args[@]}; i++)); do [ "${args[i]}" = distribution ] && name="${args[i+1]}"; done
        f="$TC_LIB_DIR/$name.dist"; [ -f "$f" ] || exit 2
        [ "$(wc -w < "$f")" -gt "${STUB_MAXTABLE:-16384}" ] && exit 2 ;;
      esac
      case " $* " in *" root "*)
        kind=netem; case " $* " in *" prio "*) kind=prio ;; esac
        all="$*"; rest="${all#* root }"; rest="${rest#netem }"
        echo "qdisc $kind 8001: root refcnt 2 $rest" > "$d/$dev.conf" ;;
      esac
      exit 0 ;;
    show)
      dev="$4"; [ -f "$d/$dev.conf" ] && cat "$d/$dev.conf" || echo "qdisc noqueue 0: root refcnt 2"
      if [ -z "$STUB_TC_NOCOUNTERS" ]; then
        n=$(cat "$d/$dev.cnt" 2>/dev/null || echo 0); n=$((n + 100)); echo $n > "$d/$dev.cnt"
        echo " Sent $((n * 1500)) bytes $n pkt (dropped 0, overlimits 0 requeues 0)"
        b=0; [ "$dev" = cli0 ] && b="${STUB_BACKLOG:-100}"
        echo " backlog 0b ${b}p requeues 0"
      fi
      exit 0 ;;
  esac
fi
exit 0
''',
    "tcpdump": r'''#!/bin/bash
echo "ns=${STUB_NS:--} tcpdump $*" >> "$STUB_LOG"
mode=""; file=""
while [ $# -gt 0 ]; do case "$1" in -w) mode=w; file="$2"; shift ;; -r) mode=r; file="$2"; shift ;; esac; shift; done
if [ "$mode" = w ]; then
  echo PCAP > "$file"; trap 'exit 0' TERM INT
  while :; do /bin/sleep 0.05; done
fi
[ "$mode" = r ] && exec "$STUB_PY" -I "$STUB_GEN" pcap "$file"
exit 0
''',
    "iperf3": r'''#!/bin/bash
echo "ns=${STUB_NS:--} iperf3 $*" >> "$STUB_LOG"
case " $* " in *" -s "*) trap 'exit 0' TERM INT; while :; do /bin/sleep 0.05; done ;; esac
t=5; args=("$@"); for ((i = 0; i < ${#args[@]}; i++)); do [ "${args[i]}" = -t ] && t="${args[i+1]}"; done
[ "$t" -gt 5 ] && t=5
got=0; trap 'got=1' INT TERM
ticks=0; min=$(( ${STUB_IPERF_MIN_S:-0} * 20 )); mx=$((t * 20))
while :; do
  /bin/sleep 0.05; ticks=$((ticks + 1))
  [ "$got" = 1 ] && [ "$ticks" -ge "$min" ] && break
  [ "$ticks" -ge "$mx" ] && break
done
echo '{"intervals":[{"sum":{"start":0,"seconds":1,"bytes":1000000}},{"sum":{"start":1,"seconds":1,"bytes":1000000}}],"end":{"sum_received":{"bits_per_second":95000000}}}'
''',
}
RAPTORPATH = '#!/bin/bash\necho "ns=${STUB_NS:--} raptorpath $*" >> "$STUB_LOG"\ntrap \'exit 0\' TERM INT\nwhile :; do /bin/sleep 0.05; done\n'
PROBE = "import sys\nsys.path.insert(0, %r)\nimport omega_stubgen\nsys.exit(omega_stubgen.main(['x', 'probe'] + sys.argv[1:]))\n"


def make_sandbox(tag):
    sb = tempfile.mkdtemp(prefix="omega-stub-%s-" % tag)
    for sub in ("bin", "l1", "state", "home", "root", "locks", "proc/sys/net/ipv4"):
        os.makedirs(os.path.join(sb, sub), exist_ok=True)
    for name, body in STUBS.items():
        p = os.path.join(sb, "bin", name)
        with open(p, "w", newline="\n") as f:
            f.write(body)
        os.chmod(p, os.stat(p).st_mode | stat.S_IEXEC)
    for n, v in (("tcp_sack", "1"), ("tcp_dsack", "1"), ("tcp_timestamps", "1"), ("tcp_congestion_control", "cubic")):
        with open(os.path.join(sb, "proc/sys/net/ipv4", n), "w") as f:
            f.write(v + "\n")
    for f in HARNESS:
        with open(os.path.join(HERE, f), "rb") as src, open(os.path.join(sb, "l1", f), "wb") as dst:
            dst.write(src.read().replace(b"\r\n", b"\n"))
    rp = os.path.join(sb, "bin", "raptorpath-stub")
    with open(rp, "w", newline="\n") as f:
        f.write(RAPTORPATH)
    os.chmod(rp, 0o755)
    with open(os.path.join(sb, "l1", "stubprobe.py"), "w") as f:
        f.write(PROBE % px(os.path.join(sb, "l1")))
    open(os.path.join(sb, "log.txt"), "w").close()
    return sb


def cleanup(sb):
    if not os.environ.get("STUBRUN_KEEP"):
        shutil.rmtree(sb, ignore_errors=True)
    else:
        print("kept " + sb)


def sha(path):
    import hashlib
    return hashlib.sha256(open(path, "rb").read()).hexdigest()


def run(sb, session="R", extra=None, rehearse=False, hard=None, drop_tools=(), timeout=300):
    env = dict(os.environ)
    env.update({
        "PATH": os.pathsep.join([os.path.join(sb, "bin"), env.get("PATH", "")]),
        "HOME": px(os.path.join(sb, "home")), "STUB_HOME": px(os.path.join(sb, "home")),
        "STUB_LOG": px(os.path.join(sb, "log.txt")), "STUB_STATE": px(os.path.join(sb, "state")),
        "STUB_PY": px(sys.executable), "STUB_GEN": px(os.path.join(sb, "l1", "omega_stubgen.py")),
        "OMEGA_PYTHON": px(sys.executable), "OMEGA_PROBE": px(os.path.join(sb, "l1", "stubprobe.py")),
        "OMEGA_ROOT": px(os.path.join(sb, "root")), "OMEGA_LOCK_VM": px(os.path.join(sb, "locks", "vm.lock")),
        "OMEGA_LOCK_RP": px(os.path.join(sb, "locks", "rp.lock")), "OMEGA_PROCFS": px(os.path.join(sb, "proc")),
        "OMEGA_SESSION": session, "OMEGA_TINY": "1", "OMEGA_ROW_KB": "1",
        "OMEGA_HARD": str(hard if hard is not None else int(time.time()) + 86400),
        "OMEGA_WIN_S": "0.2", "OMEGA_NOFILL_S": "6", "OMEGA_EVENT_S": "0.3", "OMEGA_SETTLE_S": "0.5", "OMEGA_TUNQ_S": "1",
        "STUB_EV_S": "0.5", "STUB_IPERF_MIN_S": "0",
    })
    if rehearse:
        env["OMEGA_REHEARSE"] = "1"
    env.update(extra or {})
    for t in drop_tools:
        os.remove(os.path.join(sb, "bin", t))
    timeout = int(os.environ.get("STUBRUN_TIMEOUT", timeout))
    try:
        r = subprocess.run([BASH] + (["-x"] if os.environ.get("STUBRUN_TRACE") else []) + [px(os.path.join(sb, "l1", "omega_run_all.sh"))], env=env, capture_output=True, text=True, timeout=timeout, cwd=sb)
    except subprocess.TimeoutExpired as e:
        print("TIMEOUT sandbox kept: " + sb, flush=True)
        if os.environ.get("STUBRUN_TRACE"):
            print(chr(10).join(((e.stderr.decode(errors="replace") if isinstance(e.stderr, bytes) else (e.stderr or ""))).splitlines()[-25:]))
        return 124, (e.stdout or b"").decode(errors="replace") if isinstance(e.stdout, bytes) else (e.stdout or "")
    return r.returncode, r.stdout + r.stderr


def run_dir(sb, session):
    base = os.path.join(sb, "root")
    ds = sorted(d for d in os.listdir(base) if d.startswith(session + "-")) if os.path.isdir(base) else []
    return os.path.join(base, ds[-1]) if ds else None


def ledger(rd):
    out = []
    p = os.path.join(rd, "ledger.jsonl")
    if os.path.exists(p):
        for l in open(p):
            if l.strip():
                out.append(json.loads(l))
    return out


def log_lines(sb):
    return [l.rstrip("\n") for l in open(os.path.join(sb, "log.txt")) if l.strip()]


def diag(rd, sb=None):
    """Print what a failed run left behind (CI logs only show the checks)."""
    if not rd:
        print("DIAG no run directory")
        return
    print("DIAG session.log tail:" + chr(10) + "".join(open(os.path.join(rd, "session.log")).readlines()[-12:]) if os.path.exists(os.path.join(rd, "session.log")) else "DIAG no session.log")
    rows = os.path.join(rd, "rows")
    for r in sorted(os.listdir(rows))[:2] if os.path.isdir(rows) else []:
        d = os.path.join(rows, r)
        print("DIAG row %s: %s" % (r, sorted(os.listdir(d))))
        for f in ("backlog.txt", "sampler.err", "iperf.json.err", "tc-before-cli0.txt"):
            p = os.path.join(d, f)
            if os.path.exists(p):
                print("DIAG   %s: %d lines, head=%r" % (f, len(open(p).readlines()), open(p).read()[:200]))


def score(rd):
    if not rd:
        return 99, "no run directory"
    r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "omega_score.py"), os.path.join(rd, "ledger.jsonl"), "--boot", "100"], capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def vm_safe(lines):
    """Every namespace-bound call ran inside rp-*; no forbidden tool; kills only by exact name."""
    bad = []
    for l in lines:
        if l.startswith("sudo "):
            continue
        if l.startswith("pkill "):
            if not re.match(r"^pkill (-TERM|-KILL) -x raptorpath$", l):
                bad.append(l)
            continue
        m = re.match(r"^ns=(\S+) (\S+) ?(.*)$", l)
        if not m:
            bad.append(l)
            continue
        ns, tool, rest = m.groups()
        if tool == "ip" and ns == "-":
            if not re.match(r"^(netns (add|del) rp-[A-Za-z0-9_-]+|netns list|netns exec rp-[A-Za-z0-9_-]+ .*|-n rp-[A-Za-z0-9_-]+ .*)$", rest):
                bad.append(l)
        elif not re.match(r"^rp-[A-Za-z0-9_-]+$", ns):
            bad.append(l)
        if re.search(r"\b(modprobe|nft|iptables|ens18|killall)\b", l) or "-f" in rest.split() and tool == "pkill":
            bad.append(l)
    return bad


if BASH is None:
    print("SKIP no bash")
    sys.exit(0)
want = set(sys.argv[1:])


def wanted(n):
    return not want or n in want


# ───────────── static checks ─────────────
if wanted("static"):
    src = {f: open(os.path.join(HERE, f), encoding="utf8", newline="").read() for f in HARNESS if f.startswith("omega_") and not f.endswith("stubgen.py")}
    txt = "\n".join(src.values())
    check("static: no CR bytes in the harness scripts", all("\r" not in s for s in src.values()))
    check("static: no modprobe, nft, iptables, killall, pkill -f", not re.search(r"^\s*(sudo\s+)?(modprobe|nft|iptables|killall)\b|pkill\s+-f\b|pkill\s+-\w*f\b", "\n".join(
        l for l in txt.splitlines() if not l.lstrip().startswith("#")), re.M))
    code = [l for l in txt.splitlines() if not l.lstrip().startswith("#")]
    rawcmd = [l.strip() for l in code if re.match(r"^\s*(sudo\s+-n\s+)?(ip|tc|ethtool|sysctl)\s", l)]
    check("static: no bare ip / tc / ethtool / sysctl command: every call goes through nsx / nsip", not rawcmd, rawcmd[:5])
    wrappers = [l.strip() for l in code if "$OMEGA_SUDO ip" in l]
    check("static: ip is run through sudo only in the guarded wrappers (nsx, nsip, ns_add, ns_del, ns list)", 4 <= len(wrappers) <= 9 and all(("netns exec" in l or "ip -n" in l or "netns add" in l or "netns del" in l or "netns list" in l) for l in wrappers), wrappers)
    kills = [l.strip() for l in code if re.search(r"(^|[;&|]\s*|\s)kill\s", l)]
    check("static: every kill takes a recorded PID variable", kills and all(re.search(r'kill\s+(-\w+\s+)*"?\$', l) for l in kills), kills)
    check("static: pkill only with -x raptorpath", all("-x raptorpath" in l for l in code if re.search(r"\bpkill\b", l)))
    check("static: no VM user name, address or home path in the repo files",
          not re.search(r"/home/\w+|/Users/\w+|192\.168\.|\bssh\b|@[a-z0-9.-]+\.(de|com|net)", txt))
    check("static: the guards exist (rp-* names, ens18 and lo refused)", "omega_guard_ns" in src["omega_topo.sh"] and "omega_guard_dev" in src["omega_topo.sh"] and 'OMEGA_MGMT_IF:-ens18' in src["omega_topo.sh"])
    check("static: HOME resolved after the shebang the way place_battery.sh does",
          src["omega_run_all.sh"].splitlines()[1].startswith('HOME="$(getent passwd "${SUDO_USER:-$(id -un)}" | cut -d: -f6)"'))

# ───────────── guard unit paths (no session) ─────────────
if wanted("guards"):
    sb = make_sandbox("guard")
    env = dict(os.environ, PATH=px(os.path.join(sb, "bin")) + ":" + os.environ.get("PATH", ""), STUB_LOG=px(os.path.join(sb, "log.txt")), STUB_STATE=px(os.path.join(sb, "state")))
    topo = px(os.path.join(sb, "l1", "omega_topo.sh"))
    for label, cmd, tok in [("nsx on a non-rp namespace", "nsx default ip link", "not rp-* prefixed"), ("nsip on init", "nsip init link set lo down", "not rp-* prefixed"),
                            ("nsx on an rp-prefixed name with a slash", "nsx 'rp-a/b' ls", "unsafe name"), ("device ens18", "omega_guard_dev ens18", "management"),
                            ("device lo", "omega_guard_dev lo", "management"), ("an unknown device", "omega_guard_dev eth0", "not one of the harness"),
                            ("ns_add of a non-rp name", "ns_add netns-prod", "not rp-* prefixed"), ("ns_del of a non-rp name", "ns_del evil", "not rp-* prefixed")]:
        r = subprocess.run([BASH, "-c", "source %s; %s; echo SURVIVED" % (topo, cmd)], env=env, capture_output=True, text=True)
        out = r.stdout + r.stderr
        check("bad-ns: %s is refused (exit 6, ABORT-NS), nothing runs" % label, r.returncode == 6 and "ABORT-NS" in out and tok in out and "SURVIVED" not in out and log_lines(sb) == [], (r.returncode, out))
    r = subprocess.run([BASH, "-c", "source %s; nsx rp-cli ip link; omega_guard_dev cli0; echo FINE" % topo], env=env, capture_output=True, text=True)
    check("bad-ns: rp-* names and the harness's veth ends pass", "FINE" in r.stdout and r.returncode == 0, r.stdout + r.stderr)
    cleanup(sb)

if wanted("bad-ns"):
    sb = make_sandbox("badns")
    rc, out = run(sb, "R", {"OMEGA_NS_SRV": "prod-ns"}, rehearse=True)
    check("bad-ns: a configured non-rp name ends the session at launch (exit 6) with no VM command", rc == 6 and "ABORT-NS" in out and log_lines(sb) == [] and not os.path.exists(os.path.join(sb, "locks", "vm.lock")), (rc, out))
    cleanup(sb)

# ───────────── rehearsal ─────────────
if wanted("rehearse"):
    sb = make_sandbox("rehearse")
    rc, out = run(sb, "R", rehearse=True)
    rd = run_dir(sb, "R")
    L = ledger(rd) if rd else []
    rows = [x for x in L if x.get("kind") == "row"]
    if rc != 0 or not rd:
        diag(rd)
        print(out[-1500:])
    check("rehearse: exits 0 with DONE-ALL", rc == 0 and rd and os.path.exists(os.path.join(rd, "DONE-ALL")) and not os.path.exists(os.path.join(rd, "FAILED-ALL")), (rc, out))
    check("rehearse: one row per arm type and both outage arms, every row digested OK", sorted(r["arm"] for r in rows) == ["CLEAN", "HOLD@k_a=3", "OUT-HOLD", "OUT-REL", "REL@k_a=3"] and all(r["status"] == "OK" for r in rows), [(r["arm"], r["status"]) for r in rows])
    check("rehearse: the ledger header says rehearsal and the footer DONE-ALL", L and L[0].get("rehearsal") is True and L[-1].get("state") == "DONE-ALL")
    rel = next((r for r in rows if r["arm"] == "REL@k_a=3"), {})
    w = rel.get("witness", {})
    check("rehearse: execution witness per arm: tc counters moved, qdiscs in force, sysctls held, queue at level",
          all(r["witness"].get("counters_moved_mid1") and r["witness"].get("counters_moved_cli0") and r["witness"].get("reorderer_in_force") and r["witness"].get("bottleneck_in_force")
              and r["witness"].get("sysctl_ok") and r["witness"].get("warm_ok") for r in rows), [r["witness"] for r in rows])
    check("rehearse: the REL row has events with overtakers and valid labels", rel.get("events") and all("k_a" in e and e.get("valid") is True for e in rel["events"]), rel.get("events"))
    log = log_lines(sb)
    check("rehearse: every VM-bound call ran inside rp-* (none outside, no forbidden tool)", vm_safe(log) == [], vm_safe(log)[:5])
    check("rehearse: HOLD used the rate-limited FIFO at 10 x g (120 Mbit at g = 1000/s)", any("distribution" in l and "rate 120000000bit" in l for l in log))
    check("rehearse: sysctls were written only inside rp-cli and rp-srv, with the pre-flight in a fresh rp-pf", all(re.match(r"ns=rp-(cli|srv|mid|pf) ", l) for l in log if " sysctl -w" in l) and any("test -e" in l or "ns=rp-pf" in l for l in log))
    check("rehearse: tables loaded through TC_LIB_DIR under the run directory", rd and any(f.endswith(".dist") for f in os.listdir(os.path.join(rd, "tables"))))
    check("rehearse: locks released, no namespace left, exit state clean", not os.path.exists(os.path.join(sb, "locks", "vm.lock")) and not os.path.exists(os.path.join(sb, "locks", "rp.lock"))
          and not os.listdir(os.path.join(sb, "state", "ns")) and "exit_state_clean=1" in out, out[-600:])
    check("rehearse: no recorded PID is alive at exit", rd and not any(os.path.exists("/proc/%s" % p.strip()) for p in open(os.path.join(rd, "pids")) if p.strip()) if os.path.isdir("/proc") else True)
    src, so = score(rd)
    check("rehearse: the scorer refuses a rehearsal ledger (no scored result)", src == 2 and "REHEARSAL-NOT-SCORED" in so and "OMEGA" not in so, so)
    cleanup(sb)

# ───────────── refusals before any command ─────────────
if wanted("lock-held"):
    for which in ("vm.lock", "rp.lock"):
        sb = make_sandbox("lock")
        lk = os.path.join(sb, "locks", which)
        open(lk, "w").write("4242 someone-else\n")
        rc, out = run(sb, "R", rehearse=True)
        check("lock-held: %s present -> ABORT-LOCK (exit 4), no stub command, the foreign lock is untouched" % which,
              rc == 4 and "ABORT-LOCK" in out and log_lines(sb) == [] and open(lk).read() == "4242 someone-else\n" and run_dir(sb, "R") is None, (rc, out))
        cleanup(sb)

if wanted("budget"):
    sb = make_sandbox("budget")
    rc, out = run(sb, "R", rehearse=True, hard=int(time.time()) + 20)
    check("budget: a session that does not fit before OMEGA_HARD aborts (ABORT-BUDGET), no stub command, no lock taken",
          rc == 3 and "ABORT-BUDGET" in out and log_lines(sb) == [] and not os.path.exists(os.path.join(sb, "locks", "vm.lock")) and run_dir(sb, "R") is None, (rc, out))
    rc, out = run(sb, "R", {"OMEGA_HARD": ""}, rehearse=True)
    check("budget: no OMEGA_HARD at all aborts", rc == 3 and "ABORT-BUDGET" in out and log_lines(sb) == [], (rc, out))
    cleanup(sb)

if wanted("crlf"):
    sb = make_sandbox("crlf")
    p = os.path.join(sb, "l1", "omega_cells.py")
    open(p, "ab").write(b"\r\n")
    rc, out = run(sb, "R", rehearse=True)
    check("crlf: a CR byte in a harness file -> ABORT-CRLF (exit 3), no stub command", rc == 3 and "ABORT-CRLF" in out and log_lines(sb) == [], (rc, out))
    cleanup(sb)

if wanted("tools-missing"):
    sb = make_sandbox("tools")
    rc, out = run(sb, "R", rehearse=True, drop_tools=("iperf3",))
    check("tools-missing: iperf3 absent -> ABORT-TOOLS, nothing run", rc == 3 and "ABORT-TOOLS iperf3" in out and log_lines(sb) == [], (rc, out))
    cleanup(sb)

if wanted("amendment"):
    sb = make_sandbox("amend")
    rc, out = run(sb, "D1")
    check("amendment: a D session without pins or a finished session R -> ABORT-AMENDMENT, nothing run", rc == 3 and "ABORT-AMENDMENT" in out and log_lines(sb) == [], (rc, out))
    cleanup(sb)

# ───────────── abort classes in the stack ─────────────
for name, extra, tok, forbid in [
    ("sysctl-not-per-ns", {"STUB_SYSCTL_ABSENT": "tcp_recovery"}, "ABORT-SYSCTL", True),
    ("sysctl-mismatch", {"STUB_SYSCTL_WRONG": "tcp_dsack"}, "ABORT-SYSCTL", False),
    ("hoststate-changed", {"STUB_HOST_CHANGE": "1"}, "ABORT-HOSTSTATE", False),
    ("missing-witness", {"STUB_TC_NOCOUNTERS": "1"}, "ABORT-SMOKE", False),
]:
    if wanted(name):
        sb = make_sandbox(name)
        rc, out = run(sb, "R", extra, rehearse=True)
        rd = run_dir(sb, "R")
        log = log_lines(sb)
        ok = rc == 3 and tok in out and rd and os.path.exists(os.path.join(rd, "FAILED-ALL")) and not os.path.exists(os.path.join(rd, "DONE-ALL"))
        if forbid:
            ok = ok and not any(" sysctl -w" in l for l in log) and not any("ip netns add rp-cli" in l for l in log)
        check("%s: %s, FAILED-ALL%s" % (name, tok, ", nothing written" if forbid else ""), ok, (rc, out[-800:]))
        check("%s: locks released and no namespace left behind" % name, not os.path.exists(os.path.join(sb, "locks", "vm.lock")) and not os.listdir(os.path.join(sb, "state", "ns")), out[-300:])
        check("%s: only rp-* was touched" % name, vm_safe(log) == [], vm_safe(log)[:3])
        cleanup(sb)

if wanted("bench-refutes"):
    sb = make_sandbox("bench")
    rc, out = run(sb, "R", {"STUB_R0_BAD": "1"})
    rd = run_dir(sb, "R")
    check("bench-refutes: R0 refuting the reorderer -> ABORT-BENCH, no battery row", rc == 3 and "ABORT-BENCH" in out and rd and not [x for x in ledger(rd) if x.get("kind") == "row"] and os.path.exists(os.path.join(rd, "FAILED-ALL")), (rc, out[-900:]))
    cleanup(sb)

# ───────────── truncation ─────────────
if wanted("hard-deadline"):
    sb = make_sandbox("hard")
    rc, out = run(sb, "R", {"OMEGA_ROW_EST_S": "999999"}, rehearse=True, hard=int(time.time()) + 3000)
    rd = run_dir(sb, "R")
    L = ledger(rd) if rd else []
    check("hard-deadline: a row that cannot end before OMEGA_HARD is not started; TRUNCATED-R marker and footer (exit 5)",
          rc == 5 and "TRUNCATE-HARD" in out and rd and os.path.exists(os.path.join(rd, "TRUNCATED-R")) and not os.path.exists(os.path.join(rd, "DONE-ALL")) and L[-1].get("state") == "TRUNCATED-R", (rc, out[-700:]))
    src, so = score(rd)
    check("hard-deadline: the scorer reads nothing from a truncated ledger (TRUNCATED, no result, no PASS)", src == 2 and "TRUNCATED" in so and "PASS" not in so and "OMEGA" not in so and "DELIVERED" not in so, so)
    cleanup(sb)

if wanted("stopper"):
    sb = make_sandbox("stop")
    rc, out = run(sb, "R", {"OMEGA_STOP_AT": str(int(time.time()) + 1), "STUB_IPERF_MIN_S": "1"}, rehearse=True)
    rd = run_dir(sb, "R")
    L = ledger(rd) if rd else []
    rows = [x for x in L if x.get("kind") == "row"]
    check("stopper: the stop request ends the session at a round boundary: TRUNCATED-R, rows of the first group only",
          rc == 5 and "TRUNCATE-STOP" in out and rd and os.path.exists(os.path.join(rd, "TRUNCATED-R")) and 0 < len(rows) < 5 and L[-1].get("state") == "TRUNCATED-R", (rc, len(rows), out[-600:]))
    check("stopper: locks released and no namespace left after a truncation", not os.path.exists(os.path.join(sb, "locks", "vm.lock")) and not os.listdir(os.path.join(sb, "state", "ns")))
    cleanup(sb)

# ───────────── row-level outcomes (rehearsal is the vehicle: one row per arm) ─────────────
if wanted("no-fill"):
    sb = make_sandbox("nofill")
    rc, out = run(sb, "R", {"STUB_BACKLOG": "5", "OMEGA_NOFILL_S": "1"}, rehearse=True)
    rd = run_dir(sb, "R")
    rows = [x for x in ledger(rd) if x.get("kind") == "row"] if rd else []
    check("no-fill: a rep that never reaches 0.75 B is a NO_FILL row (excluded), the session goes on", rc == 0 and rows and all(r["status"] == "NO_FILL" for r in rows), (rc, [r["status"] for r in rows], out[-400:]))
    cleanup(sb)

for name, extra, arm in [("rel-no-events", {"STUB_REL_NO_EVENTS": "1"}, "REL@k_a=3"), ("hold-reorders", {"STUB_HOLD_REORDERS": "1"}, "HOLD@k_a=3")]:
    if wanted(name):
        sb = make_sandbox(name)
        rc, out = run(sb, "R", extra, rehearse=True)
        rd = run_dir(sb, "R")
        rows = {x["arm"]: x for x in ledger(rd) if x.get("kind") == "row"} if rd else {}
        if rows.get(arm, {}).get("status") != "VOID-WITNESS":
            diag(rd)
            print("DIAG rc=%s out tail: %s" % (rc, out[-900:]))
        check("%s: the %s row is VOID-WITNESS with its reason; the others stay OK" % (name, arm), rows.get(arm, {}).get("status") == "VOID-WITNESS" and rows.get("CLEAN", {}).get("status") == "OK", {a: r["status"] for a, r in rows.items()})
        cleanup(sb)

# ───────────── session R, full ─────────────
if wanted("full") or wanted("no-seed") or wanted("table-too-small") or wanted("ids-not-consecutive") or wanted("no-plug"):
    variants = [("full", {}), ("no-seed", {"STUB_NO_SEED": "1"}), ("table-too-small", {"STUB_MAXTABLE": "1024"}), ("ids-not-consecutive", {"STUB_IPID_STEP": "7"}), ("no-plug", {"STUB_NO_PLUG": "1"})]
    for name, extra in variants:
        if not wanted(name):
            continue
        sb = make_sandbox("R" + name)
        binp = os.path.join(sb, "bin", "raptorpath-stub")
        ex = dict(extra, OMEGA_BIN=px(binp), OMEGA_BIN_SHA=sha(binp), OMEGA_XTRACE="1")
        rc, out = run(sb, "R", ex)
        rd = run_dir(sb, "R")
        if rc != 0 and rd:
            xt = os.path.join(rd, "xtrace.log")
            DIAGBUF.append("DIAG R(%s) rc=%s xtrace before cleanup:%s%s" % (name, rc, chr(10), xt_tail(xt)))
        L = ledger(rd) if rd else []
        rows = [x for x in L if x.get("kind") == "row"]
        r0 = json.load(open(os.path.join(rd, "R0.json"))) if rd and os.path.exists(os.path.join(rd, "R0.json")) else {}
        if name == "full":
            check("full R: DONE-ALL; R0 passed; tool checks recorded", rc == 0 and rd and os.path.exists(os.path.join(rd, "DONE-ALL")) and "R0-PASS" in open(os.path.join(rd, "R0.log")).read() and r0.get("L") == 16384 and r0.get("seed_ok") is True, (rc, out[-900:]))
            qt = json.load(open(os.path.join(rd, "QT.json"))) if os.path.exists(os.path.join(rd, "QT.json")) else {}
            for f in ("tunq-f8.ssraw", "tunq-f8.ss", "tunq-f8.json"):
                pp = os.path.join(rd, f)
                print("DIAG %s: %r" % (f, open(pp).read()[:300] if os.path.exists(pp) else None))
            check("full R: the standing queue through the tunnel is measured (B_t pinned per rate) and the pinned binary's sha is recorded", qt.get("B_t_pkts") and "TUNSHA" in open(os.path.join(rd, "session.log")).read(), qt)
            check("full R: calibration and smoke rows only (nothing scored), AMENDMENT.txt written", rows and all(r["stage"] in ("calib", "smoke") for r in rows) and os.path.exists(os.path.join(rd, "AMENDMENT.txt")), [r["stage"] for r in rows][:5])
            check("full R: only rp-* was touched; raptorpath only by exact name", vm_safe(log_lines(sb)) == [], vm_safe(log_lines(sb))[:4])
            check("full R: the tunnel's raptorpath was stopped and no namespace remains", "exit_state_clean=1" in out and not os.listdir(os.path.join(sb, "state", "ns")), out[-300:])
            R_RD, R_SB = rd, sb
            # ── D1 from this R run ──
            pins = os.path.join(sb, "pins.json")
            json.dump({"B_t_pkts": {"1000": 100, "10000": 100}, "L": 16384, "stage_t_min_rtt_ms": 10.0}, open(pins, "w"))
            rc, out = run(sb, "D1", {"OMEGA_PINS": px(pins), "OMEGA_R_RUN": px(rd), "OMEGA_BIN": px(binp), "OMEGA_BIN_SHA": sha(binp)})
            rd2 = run_dir(sb, "D1")
            L2 = ledger(rd2) if rd2 else []
            rows2 = [x for x in L2 if x.get("kind") == "row"]
            stages = {}
            for r in rows2:
                stages[r["stage"]] = stages.get(r["stage"], 0) + 1
            check("full D1: DONE-ALL, the core rows, the outage check and stage T all ran", rc == 0 and rd2 and os.path.exists(os.path.join(rd2, "DONE-ALL")) and set(stages) == {"D", "OUT", "T"}, (rc, stages, out[-900:]))
            check("full D1: n = 2 per arm, ledger lines per row, rows.txt matches the ledger", stages.get("D") == 10 and len(rows2) == len([l for l in open(os.path.join(rd2, "rows.txt")) if l.strip()]), (stages, len(rows2)))
            check("full D1: stage T rows carry a digest from the tunnel path", any(r["stage"] == "T" and r["status"] == "OK" for r in rows2), [r["status"] for r in rows2 if r["stage"] == "T"])
            check("full D1: only rp-* was touched, raptorpath only by exact name", vm_safe(log_lines(sb)) == [], vm_safe(log_lines(sb))[:4])
            src, so = score(rd2)
            check("full D1: the scorer, at n = 2, scores nothing (UNSCOREABLE, no omega, no DELIVERED)", src == 2 and "UNSCOREABLE" in so and "DELIVERED" not in so and "OMEGA_1" not in so, so)
            # phase cap: a D run that used 19.9 h leaves no room
            os.makedirs(os.path.join(sb, "root", "D1-old"), exist_ok=True)
            open(os.path.join(sb, "root", "D1-old", "WALL_S"), "w").write("71900\n")
            rc, out = run(sb, "D1", {"OMEGA_PINS": px(pins), "OMEGA_R_RUN": px(rd), "OMEGA_BIN": px(binp), "OMEGA_BIN_SHA": sha(binp)})
            check("phase-cap: the Linux phase is capped at 20 h: ABORT-BUDGET", rc == 3 and "ABORT-BUDGET" in out and "20 h" in out, (rc, out[-400:]))
        elif name == "no-seed":
            check("no-seed: netem seed refused -> R0 records seed_ok false (rep halves stand in), no cell skipped", rc == 0 and r0.get("seed_ok") is False and "accepted=0" in open(os.path.join(rd, "R0.log")).read(), (rc, r0, out[-500:]))
        elif name == "table-too-small":
            check("table-too-small: the largest accepted table (L = 1024) is recorded", rc == 0 and r0.get("L") == 1024, (rc, r0, out[-500:]))
            from_cells = subprocess.run([sys.executable, "-I", os.path.join(HERE, "omega_cells.py"), "param", "g10000-rtt10-Qt", "REL@k_a=1"], capture_output=True, text=True)
            pj = subprocess.run([sys.executable, "-I", os.path.join(HERE, "omega_cells.py"), "pseudo", "g10000-rtt10-Qt"], capture_output=True, text=True)
            check("table-too-small: a cell whose p is below 1/L has no table (SKIPPED-TABLE)", json.loads(from_cells.stdout)["L"] == 16384 and subprocess.run([sys.executable, "-I", os.path.join(HERE, "omega_dist.py"), "entries", "10000", "4", "1024"], capture_output=True, text=True).stdout.strip() == "SKIPPED-TABLE", from_cells.stdout)
        elif name == "ids-not-consecutive":
            check("ids-not-consecutive: SKIPPED-SELECTOR recorded, the outage check is skipped", rc == 0 and "SKIPPED-SELECTOR" in open(os.path.join(rd, "R0.log")).read() and r0.get("skip_outage") is True, (rc, r0))
        elif name == "no-plug":
            check("no-plug: SKIPPED-PLUG recorded, the outage check is skipped, the session goes on", rc == 0 and "SKIPPED-PLUG" in open(os.path.join(rd, "R0.log")).read() and r0.get("skip_outage") is True, (rc, r0, out[-400:]))
        cleanup(sb)

if wanted("tun-bringup-fail"):
    sb = make_sandbox("tunfail")
    binp = os.path.join(sb, "bin", "raptorpath-stub")
    rc, out = run(sb, "R", {"OMEGA_BIN": px(binp), "OMEGA_BIN_SHA": sha(binp), "STUB_PING_FAIL": "1"})
    rd = run_dir(sb, "R")
    check("tun-bringup-fail: three attempts, TUN-BRINGUP-FAIL, the measurement is skipped, not the session", rc == 0 and "TUN-BRINGUP-FAIL" in out and rd and os.path.exists(os.path.join(rd, "DONE-ALL")) and out.count("TUN-BRINGUP-RETRY") == 3, (rc, out[-700:]))
    check("tun-bringup-fail: raptorpath was stopped by exact name", any(re.match(r"^pkill -TERM -x raptorpath$", l) for l in log_lines(sb)) and vm_safe(log_lines(sb)) == [], vm_safe(log_lines(sb))[:3])
    cleanup(sb)

if wanted("abort-build"):
    sb = make_sandbox("build")
    binp = os.path.join(sb, "bin", "raptorpath-stub")
    rc, out = run(sb, "R", {"OMEGA_BIN": px(binp), "OMEGA_BIN_SHA": "0" * 64})
    check("abort-build: a raptorpath sha that differs from the pin -> ABORT-BUILD", rc == 3 and "ABORT-BUILD" in out, (rc, out[-500:]))
    cleanup(sb)

if FAILED:
    print("=== SUMMARY: %d failed checks (first shown), first diagnostic last ===" % len(FAILED))
    for f in FAILED[:5]:
        print(f[:230])
    if DIAGBUF:
        print(DIAGBUF[0][:2600])
sys.exit(fails)
