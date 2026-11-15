# Usage: python3 tests_per_gate.py <crate_dir> <gates.txt>
# For each gate: (L) #[test] fns whose body names the env literal "RWM_X";
# (F) #[test] fns naming the env literal OR the gate's RuntimeGates field
#     (as `.field`), its `field_active` accessor, or a `force_field` hook.
import os,re,sys
crate=sys.argv[1]; gates=[l.strip() for l in open(sys.argv[2]) if l.strip()]
g_src=open(os.path.join(crate,'src/gates.rs'),encoding='utf-8').read()
fieldmap={}
for m in re.finditer(r'^\s*(\w+):\s*[^\n]*?"(RWM_[A-Z0-9_]+)"',g_src,re.M): fieldmap.setdefault(m.group(2),set()).add(m.group(1))
for m in re.finditer(r'^\s*(\w+):\s*crate::gates::scheduler_gates::resolve_(\w+)\(',g_src,re.M): pass
sg=open(os.path.join(crate,'src/gates/scheduler_gates.rs'),encoding='utf-8').read()
for m in re.finditer(r'pub fn (\w+)_active\(\)[\s\S]*?\n}\n',sg):
    pass
# resolve_* helper -> env (scheduler_gates, estimator)
extra={'RWM_HONEST_ANCHOR':'honest_anchor','RWM_HONEST_K':'honest_k','RWM_POOL_ANCHOR':'pool_anchor','RWM_COLD_PLACE':'cold_place',
'RWM_PLACE_T_DERIVED':'place_t_derived','RWM_PLACE_HOL':'place_hol','RWM_PLACE_WDIV_DERIVED':'place_wdiv_derived','RWM_COPA_WIRE':'copa_wire',
'RWM_COPA_COMPETE':'copa_compete','RWM_ACK_MERGE':'ack_merge','RWM_LOSS_SENT_TRUTH':'loss_sent_truth','RWM_RELEASE_1TO1':'release_1to1',
'RWM_CHARGE_RECOVERY':'charge_recovery','RWM_SIDLE_DERIVED':'sidle_derived','RWM_EST_CADENCE':'est_cadence','RWM_PLACE_T':'place_t',
'RWM_RTOBS':'rtobs','RWM_DELTA':'delta','RWM_COPA_DELTA':'copa_delta','RWM_QUIC_CC':'quic_cc','RWM_STORE':'store_override'}
for k,v in extra.items(): fieldmap.setdefault(k,set()).add(v)
GENERIC={'diag','trace','unified','delta','gen_r','pipeline','store_override','rtobs'}
tests=[]
for d,_,fs in os.walk(crate):
    if 'target' in d: continue
    for f in fs:
        if not f.endswith('.rs'): continue
        t=open(os.path.join(d,f),encoding='utf-8').read()
        for m in re.finditer(r'#\[(?:tokio::)?test[^\]]*\]',t):
            i=t.find('{',m.end()); depth=0; j=i
            while j<len(t):
                if t[j]=='{': depth+=1
                elif t[j]=='}':
                    depth-=1
                    if depth==0: break
                j+=1
            tests.append(t[m.start():j+1])
print('total #[test] fns:',len(tests))
for g in gates:
    lit=re.compile('"'+re.escape(g)+'"')
    toks=[]
    for fld in fieldmap.get(g,()):
        toks.append(r'\b'+fld+r'_active\b'); toks.append(r'\bforce_'+fld+r'\b')
        if fld not in GENERIC: toks.append(r'\.'+fld+r'\b')
    rx=re.compile('|'.join(toks)) if toks else None
    L=sum(1 for b in tests if lit.search(b))
    F=sum(1 for b in tests if lit.search(b) or (rx and rx.search(b)))
    print(f"{g}\t{L}\t{F}\t{','.join(sorted(fieldmap.get(g,[])))}")

# Mechanism-function tests (tests that call the arm's own law fn), for the arms
# whose tests exercise the law directly rather than the gate.
MECH={'RWM_THREE_TERM/COMPOSED_CAP':r'\bthree_term_(store_cap|terms)\b',
 'RWM_HONEST_CAP/POOL_ANCHOR (honest caps)':r'\bhonest_(store_cap|cap_terms?)\b',
 'RWM_HOLDDOWN_Q':r'\bholddown_(us|window_n)\b',
 'RWM_RECV_REQUEST_LAW':r'\brequest_law_armed\b|recv_request_law',
 'RWM_LOSS_SENT_TRUTH/RELEASE_1TO1':r'\bsender_truth_(loss|release)_(batch|delta)\b',
 'RWM_DERIVED_SWEEP':r'\bderived_recovery_round_us\b',
 'RWM_SIDLE_DERIVED':r'\bstall_threshold_us\b',
 'RWM_INFL_CAP/LATE_BRAKE (percap brake)':r'\binfl_percap_full\b',
 'RWM_PLACE_HOL':r'\bplace_hol\b|frontier',
 'RWM_RANK_FEEDBACK':r'rank_feedback|rank_m\b'}
for k,v in MECH.items():
    rx=re.compile(v); print('MECH',k,sum(1 for b in tests if rx.search(b)))
