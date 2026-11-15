"""Round 7: what the engine's repair-debt loop emits, per source, at a steady
ack cadence -- the oracle's first witness, predicted before it is built.

Engine wiring modelled (window sender):
  emit_source.rs:696-700   repair_debt += grant (grant == rate, see accrue below)
                           while repair_debt >= 1: emit one repair, debt -= 1
  sender_phases.rs:1278-1328  on each cumulative-ack advance:
                           repair_debt = max(0, repair_debt - newly_acked * rate)
  TaperBudget::accrue (fec_rate.rs:703-736) with rate <= spare (emit_source
  passes flat_rate = window_rate.min(spare)): owed = rate, grant = rate,
  owed -> 0 at every call, for any offset -> the taper shape is inert here.

Ack model (no ARQ, as in the oracle): a source is resolved on arrival, or at
its give-up age D (slots) if lost; FEC recovery would resolve it earlier, so
this OVER-states stalls and hence emission (an upper bound on the witness).
The cumulative ack reaches the sender F slots later.

Run: python -I emission_witness.py > emission_witness.out
"""
import numpy as np

def accrue(owed, rate, shape, W, spare):  # fec_rate.rs:703-736, verbatim logic
    owed = min(owed + rate, max(rate * W, 1.0))
    desire = rate * shape
    grant = min(owed, max(desire, rate), max(spare, 0.0), 1.0)
    return owed - grant, grant

def check_accrue():
    rng = np.random.default_rng(1)
    worst = 0.0
    owed = 0.0
    for _ in range(100_000):
        rate = rng.random() * 0.6
        spare = rate + rng.random()          # emit_source: rate = min(window_rate, spare) <= spare
        shape = rng.random() * 20
        owed, g = accrue(owed, rate, shape, 200, spare)
        worst = max(worst, abs(g - rate), owed)
    return worst

def witness(eps, s2, r, D, F, N=200_000, seed=3):
    rng = np.random.default_rng(seed)
    t_ = 2.0 / (s2 + 1.0); p, q = eps * t_, (1 - eps) * t_
    bad = rng.random() < eps
    lost = np.empty(N, bool)
    for i in range(N):
        u = rng.random(); bad = (u >= q) if bad else (u < p); lost[i] = bad
    resolve = np.arange(N) + np.where(lost, D, 0)          # slot at which s is resolved
    # frontier(t) = number of leading sources resolved by slot t
    debt = rng.random(); emitted = 0; prev_ack = 0
    run_max = np.maximum.accumulate(resolve)                # contiguous: s resolved and all before
    for t in range(N):
        debt += r
        while debt >= 1.0:
            debt -= 1.0; emitted += 1
        tf = t - F
        if tf >= 0:
            ack = int(np.searchsorted(run_max, tf, side="right"))  # sources with run_max <= tf
            if ack > prev_ack:
                debt = max(0.0, debt - (ack - prev_ack) * r)
                prev_ack = ack
    return emitted / (r * N)

print(f"accrue with rate <= spare: max |grant - rate|, max owed over 1e5 random calls = {check_accrue():.1e}")
print("emitted / (r * sources), ack-cancelled debt, give-up at D (upper bound on stalls):")
for eps, s2, r, D, F in [(0.048, 3.76, 0.34, 4, 4), (0.048, 3.76, 0.34, 4, 1), (0.048, 3.76, 0.5, 7, 4),
                         (0.0, 1.0, 0.34, 4, 4), (0.025, 2.9, 0.1, 201, 20), (0.048, 3.76, 0.16, 201, 40)]:
    if eps == 0.0:
        print(f"   eps 0 (lossless), r {r}, F {F}: startup only, r*F = {r*F:.2f} repairs total, then exactly 0")
        continue
    print(f"   eps {eps} s2 {s2} r {r} D {D} F {F}: {witness(eps, s2, r, D, F):.3f}")
