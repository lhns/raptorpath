#!/usr/bin/env python3
"""Tests for the adversarial cells (adv_cells.sh, adv_cells_validate.py, the battery wiring; ADR-0068 W5.3). Runs off-VM:

  * the netem/tc command lines each cell generates are checked against EXPECTED STRINGS (a stub `ip` records argv);
  * the safety rule: every command any cell code path generates touches only rp-cli / rp-srv, cli0 / srv0, and never ens18, the
    firewall, sysctl, systemd or sshd (scan of the generated commands and of the sources);
  * the validator's thresholds are tied to those command lines, its estimators are checked against a discrete token-bucket
    simulation, and its verdicts against fixtures (a model of each cell passes; a wrong cell/mismatched mechanism fails the
    checks that must catch it);
  * the battery wiring (lib.sh ADV_CELLS, cell_spec, loopdiag_parse, harness_paths) agrees, and perf_rwm_c.sh's topology routing
    (lib.sh topo_script / topo_up_args) is executed;
  * the prober itself runs over 127.0.0.1 (--local --quick), unshaped.
What it cannot show: that tc/netem on the VM produce these numbers. That is `adv_cells.sh check <cell>` on the VM.
Run: python3 test_adv_cells.py"""
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
import adv_cells_validate as av  # noqa: E402
import harness_paths as hp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""), flush=True)
    if not ok:
        fails += 1


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


TMP = tempfile.mkdtemp()
BIN = os.path.join(TMP, "bin")
os.makedirs(BIN)
with open(os.path.join(BIN, "ip"), "w", newline="\n") as fh:      # records argv, one line per call; prints nothing (so `netns list` is empty)
    fh.write('#!/bin/bash\nprintf \'%s\\n\' "$*" >> "$IP_LOG"\n')
with open(os.path.join(BIN, "python3"), "w", newline="\n") as fh:   # the scripts say python3; run it with this interpreter
    fh.write('#!/bin/sh\nexec "%s" "$@"\n' % sys.executable.replace("\\", "/"))
for n in ("ip", "python3"):
    os.chmod(os.path.join(BIN, n), 0o755)
IP_LOG = os.path.join(TMP, "ip.log")


def sh(script, extra_env=None, timeout=120):
    env = dict(os.environ, IP_LOG=px(IP_LOG), PATH=px(BIN) + os.pathsep + os.environ.get("PATH", ""))
    env.update(extra_env or {})
    open(IP_LOG, "w").close()
    cp = subprocess.run([BASH, "-c", script], cwd=HERE, env=env, capture_output=True, text=True, errors="replace", timeout=timeout)
    return cp.returncode, [ln for ln in open(IP_LOG).read().splitlines() if ln], cp.stdout, cp.stderr


if not BASH:
    print("SKIP no MSYS/GNU bash on this host")
    sys.exit(0)

CELLS = ["c2ctl", "jit0", "jit5", "jit15", "jit25", "shal8", "pol100"]

# ---------------------------------------------------------------------------------------------------------------------------------
# 1. The exact command lines (seed 42).
PRE = ["netns del rp-cli", "netns del rp-srv", "netns add rp-cli", "netns add rp-srv",
       "link add cli0 netns rp-cli type veth peer name srv0 netns rp-srv",
       "-n rp-cli addr add 10.77.0.1/24 dev cli0", "-n rp-srv addr add 10.77.0.2/24 dev srv0",
       "-n rp-cli link set cli0 up", "-n rp-srv link set srv0 up", "-n rp-cli link set lo up", "-n rp-srv link set lo up"]
PING = ["netns exec rp-cli ping -c 2 -i 0.2 -W 2 10.77.0.2"]
OFF = ["netns exec rp-cli ethtool -K cli0 gro off gso off tso off", "netns exec rp-srv ethtool -K srv0 gro off gso off tso off"]
C = "netns exec rp-cli tc qdisc add dev cli0 "
S = "netns exec rp-srv tc qdisc add dev srv0 "
EXPECT = {
    "c2ctl": [C + "root netem delay 5ms 3ms rate 100mbit loss gemodel 1.3% 50% seed 42", S + "root netem delay 5ms 3ms rate 100mbit seed 42"],
    "jit0": [C + "root netem delay 20ms rate 100mbit loss gemodel 1.3% 50% seed 42", S + "root netem delay 20ms rate 100mbit seed 42"],
    "jit5": [C + "root netem delay 20ms 5ms 25% rate 100mbit loss gemodel 1.3% 50% seed 42", S + "root netem delay 20ms 5ms 25% rate 100mbit seed 42"],
    "jit15": [C + "root netem delay 20ms 15ms 25% rate 100mbit loss gemodel 1.3% 50% seed 42", S + "root netem delay 20ms 15ms 25% rate 100mbit seed 42"],
    "jit25": [C + "root netem delay 20ms 25ms 25% rate 100mbit loss gemodel 1.3% 50% seed 42", S + "root netem delay 20ms 25ms 25% rate 100mbit seed 42"],
    "shal8": OFF + [C + "root handle 1: tbf rate 100mbit burst 15140b limit 30280b",
                    C + "parent 1:1 handle 10: netem limit 8 loss gemodel 1.3% 50% seed 42",
                    S + "root netem delay 10ms rate 100mbit"],
    "pol100": OFF + [C + "root netem delay 5ms limit 4000 seed 42",
                     S + "root netem delay 5ms rate 100mbit",
                     S + "handle ffff: ingress",
                     "netns exec rp-srv tc filter add dev srv0 parent ffff: protocol all u32 match u32 0 0 police rate 100mbit burst 16k mtu 65535 drop flowid :1"],
}
UP_LOGS = {}
for cell in CELLS:
    rc, log, out, err = sh("bash adv_cells.sh up %s --seed 42" % cell)
    UP_LOGS[cell] = log
    check("up %s: exact command sequence" % cell, rc == 0 and log == PRE + EXPECT[cell] + PING, (rc, err, [x for x in log if x not in PRE + PING]))
rc, log, _, _ = sh("bash adv_cells.sh up jit15")
check("up without --seed adds no seed words", rc == 0 and C + "root netem delay 20ms 15ms 25% rate 100mbit loss gemodel 1.3% 50%" in log, log)
rc, log, _, _ = sh("bash adv_cells.sh down")
check("down deletes exactly rp-cli and rp-srv", rc == 0 and log == ["netns del rp-cli", "netns del rp-srv"], log)
rc, log, _, err = sh("bash adv_cells.sh up bogus")
check("an unknown cell is refused before any command", rc != 0 and log == [] and "unknown adv cell" in err, (rc, log, err))
pol = "".join(UP_LOGS["pol100"])
check("pol100 is a POLICER, not a shaper: police ... drop on srv0 ingress, no rate/tbf qdisc on the data direction",
      "police rate 100mbit burst 16k mtu 65535 drop" in pol and "tbf" not in pol and "cli0 root netem delay 5ms limit 4000 seed 42" in pol, pol)
check("shal8 is a shaper with an 8-packet queue: tbf 100mbit + child netem limit 8", "tbf rate 100mbit burst 15140b limit 30280b" in "".join(UP_LOGS["shal8"])
      and "netem limit 8 " in "".join(UP_LOGS["shal8"]))

# ---------------------------------------------------------------------------------------------------------------------------------
# 2. The validator's cell table is tied to the generated command lines.
def netem_delay(line):
    m = re.search(r"netem delay (\d+)ms(?: (\d+)ms)?", line)
    return (int(m.group(1)), int(m.group(2) or 0)) if m else None


for cell in CELLS:
    sp = av.CELLS[cell]
    lines = UP_LOGS[cell]
    data = next((x for x in lines if "dev cli0" in x and "netem delay" in x), None)
    ack = next((x for x in lines if "dev srv0" in x and "netem delay" in x and "ingress" not in x), None)
    d, a = netem_delay(data) if data else (0, 0), netem_delay(ack)
    ok = (d[0] if data else 0) == sp["data_ms"] and a[0] == sp["ack_ms"] and (d[1] if data else 0) == sp["jit_ms"] and a[1] == sp["jit_ms"]
    check("%s: validator's delays/jitter (%s/%s +-%s) equal the netem lines" % (cell, sp["data_ms"], sp["ack_ms"], sp["jit_ms"]), ok, (data, ack, sp))
m = re.search(r"burst (\d+)b", "".join(UP_LOGS["shal8"]))
check("shal8: validator bucket = tbf burst, queue_pkts = netem limit",
      int(m.group(1)) == av.CELLS["shal8"]["bucket_b"] and "netem limit %d " % av.CELLS["shal8"]["queue_pkts"] in "".join(UP_LOGS["shal8"]))
check("pol100: validator bucket = police burst 16k = 16384 B", "burst 16k" in pol and av.CELLS["pol100"]["bucket_b"] == 16 * 1024)

# ---------------------------------------------------------------------------------------------------------------------------------
# 3. Safety: only rp-* namespaces, cli0/srv0, no management interface / firewall / sshd / sysctl / systemd.
FORBID = re.compile(r"ens18|iptables|ip6tables|nft\b|nftables|ufw|firewall|sshd|\bssh\b|systemctl|\bservice\b|sysctl|resolv|dhcp|NetworkManager|\bnetplan\b|\broute\b|ip rule", re.I)
ALLOW_IP = [
    r"^netns (add|del) rp-(cli|srv)$",
    r"^netns list$",
    r"^netns exec rp-(cli|srv) (tc (qdisc (add|show)|filter (add|show)|-s (qdisc|filter) show) .*dev (cli0|srv0)\b.*|ethtool -K (cli0|srv0) gro off gso off tso off|ping -c \d+ .*10\.77\.0\.[12]|python3 .*)$",
    r"^link add cli0 netns rp-cli type veth peer name srv0 netns rp-srv$",
    r"^-n rp-(cli|srv) (addr add 10\.77\.0\.[12]/24 dev (cli0|srv0)|link set (cli0|srv0|lo) up|-br addr)$",
]
generated = []
for cell in CELLS:
    for script in ("bash adv_cells.sh up %s --seed 7" % cell, "bash adv_cells.sh status", "bash adv_cells.sh counters", "bash adv_cells.sh down"):
        generated += sh(script)[1]
gen_ok = all(any(re.match(p, ln) for p in ALLOW_IP) for ln in generated)
check("every ip invocation of up/status/counters/down matches the rp-*-only grammar (%d commands)" % len(generated), gen_ok,
      [ln for ln in generated if not any(re.match(p, ln) for p in ALLOW_IP)])
check("no generated command mentions ens18, a firewall, sshd, sysctl or systemd", not any(FORBID.search(ln) for ln in generated), [ln for ln in generated if FORBID.search(ln)])
check("every namespace the cells add or delete is rp-cli or rp-srv",
      all(re.match(r"^rp-(cli|srv)$", ln.split()[2]) for ln in generated if re.match(r"^netns (add|del) ", ln)) and any(ln.startswith("netns add") for ln in generated))


def strip_comments(text, py=False):
    out = []
    in_doc = False
    for ln in text.splitlines():
        t = ln.strip()
        if py:
            if t.count('"""') % 2 == 1:
                in_doc = not in_doc
                continue
            if in_doc:
                continue
        if t.startswith("#"):
            continue
        out.append(ln)
    return "\n".join(out)


for fn, py in (("adv_cells.sh", False), ("adv_cells_validate.py", True)):
    src = strip_comments(open(os.path.join(HERE, fn), encoding="utf-8").read(), py)
    bad = [ln for ln in src.splitlines() if FORBID.search(ln)]
    check("%s: code (not comments) never mentions ens18/firewall/sshd/sysctl/systemd" % fn, not bad, bad)
sh_src = strip_comments(open(os.path.join(HERE, "adv_cells.sh"), encoding="utf-8").read())
check("adv_cells.sh deletes namespaces only through the guarded NS_CLI/NS_SRV loop (no other 'netns del')",
      len(re.findall(r"netns del", sh_src)) == 1 and 'guard_ns "$ns"' in sh_src)
allv = list(av.TC_CMDS.values()) + [av.ns_prefixes(False)[0] + ["python3"], av.ns_prefixes(False)[1] + ["python3"]]
check("adv_cells_validate.py outside-Python commands: only `ip netns exec rp-*` and read-only `tc ... show`",
      all(c[:3] == ["ip", "netns", "exec"] and c[3] in ("rp-cli", "rp-srv") for c in allv) and all(("show" in c) for c in av.TC_CMDS.values()), allv)
check("--local runs no namespace command at all", av.ns_prefixes(True)[:2] == ([], []))

# ---------------------------------------------------------------------------------------------------------------------------------
# 4. Estimators against a discrete token-bucket simulation; fixtures from it.
def bucket_burst(bucket_b, qlimit, pkt_len, n, t_send_us):
    """n packets arriving evenly over t_send_us into a token bucket (starts full, 12.5 B/us) in front of a qlimit-packet queue
    (qlimit 0 = policer: excess dropped on arrival). Returns (delivered, 1-based index of the first dropped packet or None)."""
    tokens, q, deliv, first = float(bucket_b), 0, 0, None
    gap = t_send_us / float(n)
    for i in range(n):
        tokens = min(float(bucket_b), tokens + av.RATE_BPUS * (gap if i else 0))
        if q == 0 and tokens >= pkt_len:
            tokens -= pkt_len
            deliv += 1
            continue
        if q < qlimit:
            q += 1
        elif first is None:
            first = i + 1
        while q and tokens >= pkt_len:
            tokens -= pkt_len
            q -= 1
            deliv += 1
    deliv += q                       # the queue drains after the burst
    return deliv, first


for cell in ("shal8", "pol100"):
    sp = av.CELLS[cell]
    q = sp.get("queue_pkts", 0)
    ests = []
    for t_send in (150.0, 300.0, 600.0):
        d, first = bucket_burst(sp["bucket_b"], q, sp["pkt_len"], 200, t_send)
        ests.append(d - (sp["bucket_b"] + av.RATE_BPUS * t_send) / sp["pkt_len"])
    check("%s: held-packets estimator recovers the queue depth %d in simulation (got %s)" % (cell, q, [round(e, 1) for e in ests]),
          all(abs(e - q) <= 1.5 for e in ests), ests)
d, first = bucket_burst(15140, 8, 1242, 200, 100.0)
check("shal8 simulation: with a full bucket the first drop is the 9th packet that has to be QUEUED (index %s = 12 bucket packets + 8 queued + 1)" % first,
      first == 12 + 8 + 1, first)
d0, first0 = bucket_burst(0, 8, 1242, 200, 1e-3)
check("shal8 simulation: with an empty bucket the drop is at the 9th packet (index %s)" % first0, first0 == 9, first0)

check("inversions: FIFO with losses = 0, one late packet = 1", av.inversions([0, 1, 3, 4, 8]) == 0 and av.inversions([0, 2, 1, 3]) == 1)
check("loss_runs: seqs 0..9, received {0,1,4,5,9}: lost runs [2,3] -> mean 2.5 over 2 runs", av.loss_runs([0, 1, 4, 5, 9], 10) == (2.5, 2))
check("pct: linear interpolation", av.pct([1, 2, 3, 4], 50) == 2.5 and av.pct([5], 99) == 5 and av.pct([], 50) is None)

TC_SHAL8 = """qdisc tbf 1: root refcnt 2 rate 100Mbit burst 15140b lat 4ms
 Sent 9876543 bytes 7000 pkt (dropped 0, overlimits 311 requeues 0)
 backlog 0b 0p requeues 0
qdisc netem 10: parent 1:1 limit 8 loss gemodel p 1.3% r 50% 1-h 100% 1-k 0%
 Sent 9876543 bytes 7000 pkt (dropped 4242, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
"""
TC_POL_CLI = """qdisc netem 8001: root refcnt 2 limit 4000 delay 5ms seed 42
 Sent 6000000 bytes 4900 pkt (dropped 0, overlimits 0 requeues 0)
 backlog 0b 0p requeues 0
"""


def tc_pol_ingress(dropped):
    return """filter protocol all pref 49152 u32 chain 0
filter protocol all pref 49152 u32 chain 0 fh 800: ht divisor 1
filter protocol all pref 49152 u32 chain 0 fh 800::800 order 2048 key ht 800 bkt 0 flowid :1 not_in_hw
  match 00000000/00000000 at 0
	action order 1:  police 0x1 rate 100Mbit burst 16Kb mtu 64Kb action drop overhead 0b
	ref 1 bind 1

 	Action statistics:
	Sent 7777777 bytes 6300 pkt (dropped %d, overlimits %d requeues 0)
	backlog 0b 0p requeues 0
""" % (dropped, dropped)


check("qdisc_blocks: the child netem's drops are read from the netem block, not the tbf's",
      av.qdisc_blocks(TC_SHAL8) == {"tbf": (0, 7000), "netem": (4242, 7000)}, av.qdisc_blocks(TC_SHAL8))
check("police_dropped reads the action statistics", av.police_dropped(tc_pol_ingress(1234)) == 1234 and av.police_dropped("nothing") is None)


# ---------------------------------------------------------------------------------------------------------------------------------
# 5. Fixtures: a model of each cell (what netem/tbf/police are documented to do) -> measurement dicts for `judge`.
def model(cell, seed=1, jit=None, dense_inv=0, ack_inv=0):
    """A measurement as the prober would record it on a cell behaving as documented. `jit` overrides the jitter (to build wrong-cell
    fixtures)."""
    rnd = random.Random(seed)
    sp = av.CELLS[cell]
    J = sp["jit_ms"] if jit is None else jit
    ge = cell not in ("pol100",)           # GE loss on the data direction (c2ctl, jit*, shal8)

    def draw(mu):
        return max(0.0, mu + rnd.uniform(-J, J)) + rnd.uniform(0.0, 0.05) if J else mu + rnd.uniform(0.0, 0.3)

    owd, rtt = [], []
    for _ in range(600):
        if ge and rnd.random() < 0.025:
            continue
        a = draw(sp["ack_ms"])
        d = draw(sp["data_ms"])
        owd.append(d)
        rtt.append(d + a)
    kl = sp["klass"]
    trials = []
    for _ in range(20):
        t = rnd.uniform(200.0, 500.0)
        if kl == "deep":
            dl = int(200 * (0.975 if ge else 1.0)) - rnd.randint(0, 2)
        else:
            dl = bucket_burst(sp["bucket_b"], sp.get("queue_pkts", 0), sp["pkt_len"], 200, t)[0] + rnd.choice((0, 0, -1))
        first = bucket_burst(sp["bucket_b"], sp.get("queue_pkts", 0), sp["pkt_len"], 200, t)[1] if kl != "deep" else None
        trials.append(dict(sent=200, delivered=dl, t_send_us=t, first_drop_idx=first, send_mbit=200 * av.IP_LEN * 8 / t))
    base = av.stats_ms(owd)["p50"]
    infl = {"deep": 98.0, "shallow": 0.6, "police": 0.05}[kl]
    sent = 60000
    fl_owd = [base + infl + rnd.uniform(-0.2, 0.2) for _ in range(500)]
    delivered_mbit = 99.0
    recv = int(delivered_mbit / 150.0 * sent)
    m = dict(cell=cell,
             sparse=dict(sent=600, recv=len(owd), echo=len(rtt), owd_ms=av.stats_ms(owd), rtt_ms=av.stats_ms(rtt)),
             dense=dict(sent=10000, recv=9750 if ge else 10000, echo=9500 if ge else 10000, data_inversions=dense_inv, ack_inversions=ack_inv),
             burst=dict(trials=trials),
             flood=dict(sent=sent, recv=recv, offered_mbit=150.0, delivered_mbit=delivered_mbit, loss=1 - recv / float(sent),
                        owd_ms=av.stats_ms(fl_owd), loss_run_mean=1.4, loss_runs=int((sent - recv) / 1.4)),
             tc=None)
    lost = sent - recv
    if kl == "police":
        m["tc"] = dict(cli0=TC_POL_CLI, srv0="", srv0_ingress=tc_pol_ingress(lost))
    elif kl == "shallow":
        m["tc"] = dict(cli0=TC_SHAL8, srv0="", srv0_ingress="")
    else:
        m["tc"] = dict(cli0="qdisc netem 8001: root refcnt 2 limit 1000 delay 20ms\n Sent 1 bytes 1 pkt (dropped 9, overlimits 0 requeues 0)\n", srv0="", srv0_ingress="")
    return json.loads(json.dumps(m))


def failed(cell, m):
    return sorted(n for n, ok, _ in av.judge(cell, m) if not ok)


for cell in CELLS:
    bad = failed(cell, model(cell))
    check("judge: the documented behaviour of %s passes every check" % cell, bad == [], bad)
res = av.judge("jit15", model("jit15"))
check("judge: the delay-floor, FIFO, queue and flood checks are all present for a jitter cell",
      {n for n, _, _ in res} >= {"owd_floor", "owd_ceiling", "owd_median", "owd_iqr", "owd_std", "owd_span", "rtt_floor", "no_reorder_data", "no_reorder_ack", "queue_deep", "flood_delay"},
      [n for n, _, _ in res])
check("judge: pol100 additionally checks the police counter, the drop pattern and that the data netem never drops",
      {n for n, _, _ in av.judge("pol100", model("pol100"))} >= {"police_counter", "police_drop_pattern", "police_loss_rate", "data_netem_never_drops", "queue_depth"})

# Wrong mechanism -> the checks that exist to catch it must fail.
check("jitter dose-response discriminates: jit5's data judged as jit15 fails the distribution checks",
      {"owd_floor", "owd_iqr", "owd_std", "owd_span"} <= set(failed("jit15", model("jit15", jit=5))), failed("jit15", model("jit15", jit=5)))
check("... and jit15's data judged as jit5 fails (ceiling/IQR/std)",
      {"owd_ceiling", "owd_iqr", "owd_std"} <= set(failed("jit5", model("jit15"))), failed("jit5", model("jit15")))
check("jit0 given 15 ms jitter fails the no-jitter checks", {"owd_iqr", "owd_std"} <= set(failed("jit0", model("jit0", jit=15))))
check("a reordering path fails: 3 data-direction inversions", failed("jit15", model("jit15", dense_inv=3)) == ["no_reorder_data"], failed("jit15", model("jit15", dense_inv=3)))
check("a reordering path fails: 1 ack-direction inversion", failed("jit15", model("jit15", ack_inv=1)) == ["no_reorder_ack"])
check("a deep-buffer cell is not shal8: queue depth and flood delay fail", {"queue_depth", "flood_delay"} <= set(failed("shal8", model("jit0"))), failed("shal8", model("jit0")))
check("a shaper is not a policer (shal8 judged as pol100): queue depth, counter, drop pattern/ netem-drop fail",
      {"queue_depth", "police_counter"} <= set(failed("pol100", model("shal8"))), failed("pol100", model("shal8")))
check("a deep shaper is not a policer: the standing queue delay is caught", "flood_delay" in failed("pol100", model("jit0")), failed("pol100", model("jit0")))
check("a policer is not a shaper: pol100 judged as shal8 (held 0 pkts, no queue-drop counter)", {"queue_depth", "queue_drop_counter"} <= set(failed("shal8", model("pol100"))), failed("shal8", model("pol100")))
check("a policer is not a deep buffer: pol100 judged as jit0 fails queue_deep and flood_delay", {"queue_deep", "flood_delay"} <= set(failed("jit0", model("pol100"))))
m = model("pol100")
m["tc"]["cli0"] = TC_POL_CLI.replace("dropped 0", "dropped 5")
check("pol100: a drop in the delay-only data netem fails data_netem_never_drops", failed("pol100", m) == ["data_netem_never_drops"], failed("pol100", m))
m = model("pol100")
m["tc"]["srv0_ingress"] = tc_pol_ingress(3)
check("pol100: loss the policer's counter does not account for fails police_counter", "police_counter" in failed("pol100", m))
m = model("shal8")
m["burst"]["trials"] = [dict(t, delivered=12 + 2) for t in m["burst"]["trials"]]
check("shal8: a queue of ~0 packets (delivered = bucket only) fails queue_depth", "queue_depth" in failed("shal8", m))
m = model("jit0")
for key in ("min", "p5", "p25", "p50", "p75", "p95", "p99", "max", "mean"):
    m["sparse"]["owd_ms"][key] += 5.0
check("the delay floor is enforced: jit0 shifted by +5 ms fails owd_floor/owd_median/owd_ceiling", {"owd_floor", "owd_median", "owd_ceiling"} <= set(failed("jit0", m)))
m = model("shal8")
m["sparse"]["rtt_ms"]["min"] = 3.0
check("the RTT floor is enforced: shal8 RTprop must be 10 ms", failed("shal8", m) == ["rtt_floor"], failed("shal8", m))
m = model("pol100")
m["flood"]["offered_mbit"] = 110.0
check("an under-driving prober is a FAIL, not a pass (flood_overdrives)", "flood_overdrives" in failed("pol100", m))
m = model("shal8")
for t in m["burst"]["trials"]:
    t["send_mbit"] = 120.0
check("an under-driving burst is a FAIL (burst_overdrives)", "burst_overdrives" in failed("shal8", m))
lines, nfail = av.report("jit0", av.judge("jit0", model("jit0")))
check("report: VALCHECK lines then one VALCELL-RESULT PASS line", lines[-1].startswith("VALCELL-RESULT jit0 PASS") and nfail == 0 and lines[0].startswith("VALCHECK jit0 sparse_delivery PASS"), lines[-1])
lines, nfail = av.report("jit0", av.judge("jit0", model("jit0", dense_inv=2)))
check("report: a failed check makes the cell FAIL", lines[-1].startswith("VALCELL-RESULT jit0 FAIL") and nfail == 1 and "no_reorder_data FAIL" in "\n".join(lines))

# analyze(): raw records -> measurement (synthetic, hand-checkable).
info = dict(params=dict(sparse_n=4, dense_n=4), echoes={"1": [[0, 0, 40_000_000], [1, 50_000_000, 90_000_000]], "2": [[0, 0, 1], [2, 0, 1], [1, 0, 1]]},
            burst=[dict(trial=0, sent=5, t_send_us=100.0)], flood=dict(sent=6, elapsed_s=1.0, offered_mbit=150.0))
server = {"1": [[0, 0, 0, 20_000_000], [0, 1, 50_000_000, 71_000_000], [0, 3, 150_000_000, 171_000_000]],
          "2": [[0, 0, 0, 1], [0, 1, 0, 1], [0, 3, 0, 1]],
          "3": [[0, 0, 0, 1], [0, 1, 0, 1], [0, 4, 0, 1]],
          "4": [[0, 0, 0, 5_000_000], [0, 2, 1_500_000_000, 1_506_000_000], [0, 3, 1_600_000_000, 1_606_000_000], [0, 5, 1_700_000_000, 1_707_000_000]]}
a = av.analyze(info, server)
check("analyze: sparse owd/rtt in ms", a["sparse"]["recv"] == 3 and abs(a["sparse"]["owd_ms"]["min"] - 20.0) < 1e-9 and abs(a["sparse"]["rtt_ms"]["max"] - 40.0) < 1e-9, a["sparse"])
check("analyze: ack inversions counted in ECHO arrival order, data inversions in server order", a["dense"]["ack_inversions"] == 1 and a["dense"]["data_inversions"] == 0, a["dense"])
check("analyze: burst delivered=3, first missing index is the 3rd packet", a["burst"]["trials"][0]["delivered"] == 3 and a["burst"]["trials"][0]["first_drop_idx"] == 3, a["burst"])
check("analyze: flood steady state skips the first second; loss and loss runs from seqs",
      a["flood"]["recv"] == 4 and a["flood"]["owd_ms"]["n"] == 3 and abs(a["flood"]["loss"] - (1 - 4 / 6.0)) < 1e-9 and a["flood"]["loss_runs"] == 2, a["flood"])

# ---------------------------------------------------------------------------------------------------------------------------------
# 6. `adv_cells.sh check` end to end under the stub `ip`, judging a fixture.
good = os.path.join(TMP, "good.json")
badf = os.path.join(TMP, "bad.json")
json.dump(model("jit15"), open(good, "w"))
json.dump(model("jit15", dense_inv=4), open(badf, "w"))
rc, log, out, err = sh("bash adv_cells.sh check jit15 --seed 42", {"ADV_MEASURE_FIXTURE": px(good)})
check("check: a passing fixture exits 0, prints VALCELL-RESULT PASS, and tears down last",
      rc == 0 and "VALCELL-RESULT jit15 PASS" in out and log[-2:] == ["netns del rp-cli", "netns del rp-srv"] and C + "root netem delay 20ms 15ms 25% rate 100mbit loss gemodel 1.3% 50% seed 42" in log, (rc, out[-400:], err[-300:], log[-3:]))
rc, log, out, err = sh("bash adv_cells.sh check jit15", {"ADV_MEASURE_FIXTURE": px(badf)})
check("check: a failing fixture exits non-zero, names the failed check, and STILL tears down", rc == 1 and "no_reorder_data FAIL" in out and log[-2:] == ["netns del rp-cli", "netns del rp-srv"], (rc, out[-400:], log[-3:]))
gen2 = []
for cell in CELLS:
    json.dump(model(cell), open(good, "w"))
    rc, log, out, err = sh("bash adv_cells.sh check %s" % cell, {"ADV_MEASURE_FIXTURE": px(good)})
    gen2 += log
    check("check %s with its model fixture: exit 0" % cell, rc == 0 and ("VALCELL-RESULT %s PASS" % cell) in out, (rc, out[-300:], err[-300:]))
check("check's whole command stream is in the rp-*-only grammar", all(any(re.match(p, ln) for p in ALLOW_IP) for ln in gen2), [ln for ln in gen2 if not any(re.match(p, ln) for p in ALLOW_IP)])
rc, _, _, _ = sh("bash adv_cells.sh known jit25")
rc2, _, _, _ = sh("bash adv_cells.sh known c2")
check("known: jit25 yes, c2 no", rc == 0 and rc2 != 0)

# ---------------------------------------------------------------------------------------------------------------------------------
# 7. Battery wiring.
lib_cells = sh("source ./lib.sh; echo $ADV_CELLS")[2].split()
check("lib.sh ADV_CELLS == validator CELLS == harness_paths.ADV_CELLS keys == the cells the tests build", sorted(lib_cells) == sorted(av.CELLS) == sorted(hp.ADV_CELLS) == sorted(CELLS), (lib_cells, sorted(av.CELLS)))
usage = sh("bash adv_cells.sh")[3]
check("adv_cells.sh usage lists every cell", all(c in usage.split("cells:")[-1].split() for c in CELLS), usage)
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
spec = {m.group(1): tuple(m.group(2).split()) for m in re.finditer(r'^\s+(\S+)\)\s+echo "([^"]*)" ;;$', bat, re.M)}
for scen, cellname in hp.ADV_CELLS.items():
    want = (scen, scen, "single")
    got = spec.get(cellname)
    check("loopdiag_battery.sh selects %s by name: cell_spec %s = '%s %s single <bytes>'" % (scen, cellname, scen, scen), got is not None and got[:3] == want and got[3].isdigit(), got)
import loopdiag_parse as lp  # noqa: E402
check("loopdiag_parse.ADV_CELL_SPEC equals the battery's cell_spec for the adversarial cells and is registered with the stage-3 parser",
      all(lp.ADV_CELL_SPEC[c] == (spec[c][0], spec[c][1], spec[c][2], int(spec[c][3])) and lp.sp.CELL_SPEC[c] == lp.ADV_CELL_SPEC[c] and lp.sp.N_LEGS[c] == 1 for c in hp.ADV_CELLS.values()))
check("existing cells keep their cell_spec (c2-100 / c1s-400 / c8-100 untouched)", spec["c2-100"] == ("c2", "c2", "single", "100000000") and spec["c8-100"] == ("c2", "c3", "dual", "100000000"))
check("no adversarial cell name collides with a scenario_params scenario (they route through adv_cells.sh, not topo_dual.sh)",
      all(sh("source ./lib.sh; scenario_params %s" % c)[0] != 0 for c in CELLS))

# perf_rwm_c.sh's topology routing (lib.sh topo_script / topo_up_args), executed.
def route(mode, a, b, seed="42"):
    rc, _, out, _ = sh("source ./lib.sh; topo_script %s %s %s; echo rc=$?" % (mode, a, b))
    script = out.strip().splitlines()[0] if out.strip() and not out.startswith("rc=") else ""
    rc2, _, out2, _ = sh("source ./lib.sh; topo_up_args %s %s %s %s" % (mode, a, b, seed))
    return script, "rc=0" in out, out2.strip()


check("routing: single jit15 jit15 -> adv_cells.sh up jit15 --seed 42", route("single", "jit15", "jit15") == ("./adv_cells.sh", True, "jit15 --seed 42"), route("single", "jit15", "jit15"))
check("routing: a seed list keeps leg 0's seed (adv cells are single-leg)", route("single", "shal8", "shal8", "7,9")[2] == "shal8 --seed 7")
check("routing: a bare seed is leg 0's own seed, as topo_dual.sh gives leg 0", route("single", "pol100", "pol100", "42")[2] == "pol100 --seed 42")
check("routing: existing single cell c2 c2 still goes to topo_dual.sh with `c2 c2 --seed 42`", route("single", "c2", "c2") == ("./topo_dual.sh", True, "c2 c2 --seed 42"), route("single", "c2", "c2"))
check("routing: dual c2 c3 unchanged", route("dual", "c2", "c3", "7") == ("./topo_dual.sh", True, "c2 c3 --seed 7"))
check("routing: quad unchanged (A A B B)", route("quad", "c2", "c3", "42") == ("./topo_quad.sh", True, "c2 c2 c3 c3 --seed 42"))
check("routing: an adversarial cell in dual or quad mode is refused", route("dual", "jit15", "jit15")[1] is False and route("quad", "shal8", "shal8")[1] is False)
check("routing: an adversarial cell paired with another scenario is refused", route("single", "jit15", "c2")[1] is False and route("single", "c2", "pol100")[1] is False)
perf = open(os.path.join(HERE, "perf_rwm_c.sh"), encoding="utf-8").read()
check("perf_rwm_c.sh uses topo_script/topo_up_args and its cleanup also runs adv_cells.sh down",
      'TOPO=$(topo_script "$MODE" "$SCENA" "$SCENB") || exit 2' in perf and "topo_up_args" in perf and "bash ./adv_cells.sh down" in perf)

# ---------------------------------------------------------------------------------------------------------------------------------
# 8. The prober's machinery over 127.0.0.1 (no namespaces, no shaping: not judged).
mfile = os.path.join(TMP, "local.json")
cp = subprocess.run([sys.executable, os.path.join(HERE, "adv_cells_validate.py"), "measure", "jit0", "--local", "--quick", "--out", mfile, "--outdir", os.path.join(TMP, "val")],
                    capture_output=True, text=True, timeout=120)
ok = cp.returncode == 0 and os.path.exists(mfile)
check("prober --local --quick exits 0 and writes a measurement", ok, (cp.returncode, cp.stderr[-400:]))
if ok:
    mm = json.load(open(mfile))
    check("prober (loopback): sparse delivered and echoed, no inversions, flood offered near its target",
          mm["sparse"]["recv"] >= 57 and mm["sparse"]["echo"] >= 57 and mm["dense"]["data_inversions"] == 0 and mm["dense"]["ack_inversions"] == 0
          and mm["flood"]["recv"] > 0 and mm["flood"]["sent"] > 0 and mm["sparse"]["owd_ms"]["p50"] < 5.0, mm["sparse"])
    check("prober (loopback): the unshaped burst is NOT cut (a queue-less path delivers the burst) -- the shallow check would fail here",
          av.pct([t["delivered"] for t in mm["burst"]["trials"]], 50) >= 190)
    sink = failed("shal8", mm)
    check("an unshaped loopback fails the shal8 verdict (queue_depth/flood checks) -- the judge cannot pass a path that is not the cell", "queue_depth" in sink, sink)

shutil.rmtree(TMP, ignore_errors=True)
if fails:
    print("FAILED %d" % fails)
    sys.exit(1)
print("ALL OK")
