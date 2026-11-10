# Usage: python3 sloc.py <src_dir>
# Counts non-blank lines in .rs files, (a) excluding test files
# (tests.rs, *_tests.rs, */tests/*), (b) additionally excluding inline
# `#[cfg(test)]` items (brace-matched), (c) additionally excluding comment-only lines.
import os,re,sys
root=sys.argv[1]; a=b=c=0; tf=0
def is_test_file(p):
    n=os.path.basename(p); return n=='tests.rs' or n.endswith('_tests.rs') or '/tests/' in p.replace(os.sep,'/')
for d,_,fs in os.walk(root):
    for f in fs:
        if not f.endswith('.rs'): continue
        p=os.path.join(d,f); L=open(p,encoding='utf-8').read().split('\n')
        if is_test_file(p): tf+=sum(1 for l in L if l.strip()); continue
        a+=sum(1 for l in L if l.strip())
        i=0; keep=[]
        while i<len(L):
            if L[i].strip().startswith('#[cfg(test)]'):
                # skip attribute + following item (brace matched, or to ';')
                j=i+1; depth=0; started=False
                while j<len(L):
                    s=re.sub(r'//.*','',L[j]); s=re.sub(r'"[^"]*"','""',s)
                    depth+=s.count('{')-s.count('}')
                    if '{' in s: started=True
                    if (started and depth<=0) or (not started and s.rstrip().endswith(';')): break
                    j+=1
                i=j+1; continue
            keep.append(L[i]); i+=1
        b+=sum(1 for l in keep if l.strip())
        c+=sum(1 for l in keep if l.strip() and not l.strip().startswith('//'))
print(f"non-blank, test files excluded: {a}\n  + inline #[cfg(test)] items excluded: {b}\n  + comment-only lines excluded: {c}\n(test files non-blank: {tf})")
