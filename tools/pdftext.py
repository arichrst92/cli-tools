#!/usr/bin/env python3
# ponytail: stdlib-only PDF text extract (no pypdf/pip). Handles FlateDecode + Tj/TJ.
import sys, re, zlib

data = open(sys.argv[1], "rb").read()

out = []
for m in re.finditer(rb"stream\r?\n(.*?)\r?\nendstream", data, re.DOTALL):
    raw = m.group(1)
    try:
        s = zlib.decompress(raw)
    except Exception:
        s = raw  # maybe uncompressed
    # pull text from Tj and TJ operators
    txt = []
    # ( ... ) Tj
    for t in re.finditer(rb"\((?:[^()\\]|\\.)*\)\s*Tj", s):
        txt.append(t.group(0))
    # [ ... ] TJ
    for t in re.finditer(rb"\[(?:[^\[\]\\]|\\.)*\]\s*TJ", s):
        txt.append(t.group(0))
    if not txt:
        continue
    for frag in txt:
        # extract string literals inside
        for lit in re.finditer(rb"\(((?:[^()\\]|\\.)*)\)", frag):
            b = lit.group(1)
            b = b.replace(b"\\(", b"(").replace(b"\\)", b")").replace(b"\\\\", b"\\")
            try:
                out.append(b.decode("latin-1"))
            except Exception:
                pass
        if frag.endswith(b"Tj"):
            out.append("\n")
        else:
            out.append(" ")
print("".join(out))
