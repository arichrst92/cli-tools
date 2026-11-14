# Usage: python3 tools/mkcat.py <out of pdftext.py> <catalog.tsv>
import re,sys
def fix(t):
    try: return t.encode('latin-1').decode('utf-8')
    except Exception: return t
L=[fix(l) for l in open(sys.argv[1],encoding='latin-1').read().splitlines()]
mods=[m.group(1) for l in L if (m:=re.match(r'^- (/\S+) - \d+ endpoint$',l))]
ms=set(mods); cur=None; out={}; ep=re.compile(r'^(GET|POST|PUT|PATCH|DELETE) (/\S*)(.*)$')
skip=('Validasi','Parameter','Akses menu','ECC - API','(skema')
for i,l in enumerate(L):
    s=l.strip()
    if s in ms: cur=s; continue
    m=ep.match(s)
    if not m or not cur: continue
    meth,p,rest=m.groups()
    if not p.startswith(('/admin','/api','/auth','/public','/upload','/web-action','/diagnostics','/health')): continue
    if not (p==cur or p.startswith(cur+'/') or p.startswith(cur+'?')): continue  # prose mention of another module
    p=p.rstrip(',.;:')
    q=''
    if '?' in p: p,q=p.split('?',1)
    summ=rest.strip(' -')
    summ=summ.lstrip('>').strip()
    if summ.startswith('->'): summ=summ[2:].strip()
    if summ.startswith(('=','/')): summ=''
    if not summ and i+1<len(L):
        n=L[i+1].strip()
        if n and not n.startswith(skip+('=','/')) and not ep.match(n): summ=n
    if q and '...' not in q: summ=(summ+' ' if summ else '')+'[?'+q+']'
    k=(meth,p)
    if k not in out or (not out[k][1] and summ): out[k]=(cur,summ[:110])
rows=sorted(out.items(),key=lambda x:(x[1][0],x[0][1],x[0][0]))
with open(sys.argv[2],'w',encoding='utf-8') as f:
    for (meth,p),(mod,s) in rows: f.write(f"{mod}\t{meth}\t{p}\t{s.replace(chr(9),' ')}\n")
print(len(rows),'endpoints',len(mods),'modules')
