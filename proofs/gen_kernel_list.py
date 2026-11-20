#!/usr/bin/env python3
"""gen_kernel_list.py <src/v16.rs> [existing.tsv]
List the contract-annotated functions of an engine revision as
  label <tab> fn name <tab> fn@line <tab> attrs@line <tab> requires=N <tab> ensures=N <tab> modifies=N
Labels are kept from an existing list by function name; new functions get the next free label."""
import re, sys
src = open(sys.argv[1]).read().split("\n")
known = {}
if len(sys.argv) > 2:
    for line in open(sys.argv[2]):
        f = line.rstrip("\n").split("\t")
        if len(f) >= 2:
            known[f[1]] = f[0]
nxt = max([int(v[1:]) for v in known.values()] + [0]) + 1
rows = []
i = 0
n = len(src)
attr_re = re.compile(r'#\[cfg_attr\(all\(kani, feature = "contracts"\), kani::(requires|ensures|modifies)')
while i < n:
    if attr_re.search(src[i]):
        start = i
        counts = {"requires": 0, "ensures": 0, "modifies": 0}
        # walk forward over the attribute block until the fn line
        j = i
        while j < n:
            m = attr_re.search(src[j])
            if m:
                counts[m.group(1)] += 1
            fm = re.match(r"\s*(?:pub(?:\([a-z]+\))?\s+)?(?:const\s+)?fn\s+([a-zA-Z0-9_]+)", src[j])
            if fm:
                rows.append((fm.group(1), j + 1, start + 1, counts))
                break
            j += 1
        i = j + 1
    else:
        i += 1
for name, fn_line, attr_line, c in rows:
    if name not in known:
        known[name] = f"K{nxt:02d}"; nxt += 1
out = sorted(((known[name], name, fn_line, attr_line, c) for name, fn_line, attr_line, c in rows), key=lambda r: int(r[0][1:]))
for label, name, fn_line, attr_line, c in out:
    print(f"{label}\t{name}\tfn@{fn_line}\tattrs@{attr_line}\trequires={c['requires']}\tensures={c['ensures']}\tmodifies={c['modifies']}")
