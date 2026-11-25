#!/usr/bin/env python3
"""po_kani_results.py <kani log, regular output> <map tsv>
For each harness of the map (piece, function, harness, listed lines): the overflow checks Kani reports at each listed
line of src/v16.rs. A piece holds if every listed line has at least one overflow check and every overflow check there
has status SUCCESS (not FAILURE, not UNREACHABLE). Prints one row per piece: piece, verdict, per-line statuses."""
import re, sys
log, mp = sys.argv[1], sys.argv[2]
text = open(log).read()
blocks = {}
for m in re.finditer(r"Checking harness v16::v16_proofs::(\w+)\.\.\.(.*?)(?=Checking harness |\Z)", text, re.S):
    blocks[m.group(1)] = m.group(2)
OVER = re.compile(r"overflow", re.I)
for line in open(mp):
    if not line.strip():
        continue
    piece, fn, harness, listed = line.rstrip("\n").split("\t")
    lines_ = [int(x) for x in listed.split(",")]
    b = blocks.get(harness)
    if b is None:
        print(f"{piece}\tNOT RUN")
        continue
    checks = re.findall(r"Check \d+: [^\n]*\n\s*- Status: (\w+)\n\s*- Description: \"([^\"]*)\"\n\s*- Location: src/v16\.rs:(\d+):",
                        b)
    per = {}
    for status, desc, ln in checks:
        if OVER.search(desc) and int(ln) in lines_:
            per.setdefault(int(ln), []).append(status)
    ok = all(per.get(l) and all(s == "SUCCESS" for s in per[l]) for l in lines_)
    print(f"{piece}\t{'HOLDS' if ok else 'OPEN'}\t" + "; ".join(f"{l}: {','.join(per.get(l, ['no check']))}" for l in lines_))
