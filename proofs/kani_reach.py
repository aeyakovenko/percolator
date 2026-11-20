#!/usr/bin/env python3
"""kani_reach.py <engine worktree> [--paths name,name,...]
What the Kani harnesses reach. Call graph of the engine (src/v16.rs, the head of header_writes.py:
callees resolved by receiver type where the text allows, otherwise by name, so reach can only be
too large) extended with the proof API (src/v16_kani_api.rs) and the harness bodies
(tests/proofs_v16.rs, src/v16_proofs.rs). For each harness: the deployed engine functions it
reaches. Prints the deployed functions no harness reaches, and with --paths one call path from a
harness to each named function."""
import sys, re, os, collections
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, here)
from rs_scan import scan
wt = sys.argv[1]
src_text = open(os.path.join(here, "header_writes.py")).read()
cut = src_text.index("# ------------------------------------------------------------------ check the CHECKS table against the source")
argv = sys.argv; sys.argv = ["header_writes.py", os.path.join(wt, "src/v16.rs"), "x"]
g = {"__name__": "hw"}; exec(compile(src_text[:cut], "header_writes.py", "exec"), g); sys.argv = argv
fns, by_name, by_id = g["fns"], g["by_name"], g["by_id"]          # deployed (default build) engine functions
CALL = g["CALL"]
def engine_callees_of_text(body):
    out = set()
    for mm in CALL.finditer(body):
        qual, dot, callee = mm.group(1), mm.group(2), mm.group(3)
        if callee not in by_name: continue
        cands = by_name[callee]
        if qual and qual not in ("Self", "self", "super", "crate"):
            typed = [c for c in cands if c.type == qual]
            if typed: cands = typed
        out |= {id(c) for c in cands}
    return out
def bodies(path):
    out = {}
    for kind, name, hdr, line, text in scan(open(path).read()):
        if kind == "fn" and "{" in text:
            out.setdefault(name, []).append(text[text.index("{"):])
    return out
api = bodies(os.path.join(wt, "src/v16_kani_api.rs"))
# engine functions that exist only under cfg(kani)/fuzz are not in `fns`; harnesses may call them as
# bridges, so scan them too (by name)
allfn = bodies(os.path.join(wt, "src/v16.rs"))
bridge = {n: b for n, b in allfn.items() if n not in by_name}
ident = re.compile(r"\b([a-z_][a-z0-9_]*)\s*(?:::\s*<[^>()]*>\s*)?\(")
def expand(body, seen_helpers, helpers):
    """engine fn ids reached directly from a non-engine body, going through api/bridge/local helpers"""
    out = engine_callees_of_text(body)
    for name in set(ident.findall(body)):
        for table in (api, bridge, helpers):
            if name in table and (id(table), name) not in seen_helpers:
                seen_helpers.add((id(table), name))
                for b in table[name]:
                    out |= expand(b, seen_helpers, helpers)
    return out
def closure(ids):
    seen, todo = set(ids), list(ids)
    parent = {}
    while todo:
        x = todo.pop()
        for c in by_id[x].calls:
            if c not in seen:
                seen.add(c); parent[c] = x; todo.append(c)
    return seen, parent
harness = {}      # name -> set of engine fn ids reached
first_path = {}   # engine fn id -> (harness, list of names)
for path in ("tests/proofs_v16.rs", "src/v16_proofs.rs"):
    text = open(os.path.join(wt, path)).read()
    helpers = bodies(os.path.join(wt, path))
    lines = text.split("\n"); i = 0
    while i < len(lines):
        if "#[kani::proof" in lines[i]:
            j = i
            while not re.match(r"\s*(pub )?fn [a-z_0-9]+\(", lines[j]): j += 1
            name = re.match(r"\s*(?:pub )?fn ([a-z_0-9]+)\(", lines[j]).group(1)
            direct = set()
            for b in helpers.get(name, []):
                direct |= expand(b, set(), helpers)
            reach, parent = closure(direct)
            harness[name] = reach
            for fid in reach:
                if fid not in first_path:
                    chain = [fid]
                    while chain[-1] in parent: chain.append(parent[chain[-1]])
                    first_path[fid] = (name, [by_id[c].name for c in reversed(chain)])
            i = j
        i += 1
reached = set().union(*harness.values()) if harness else set()
allids = {id(f) for f in fns}
if __name__ == "__main__":
    unreached = sorted((by_id[i] for i in allids - reached), key=lambda f: -(f.text.count("\n") + 1))
    mut = [f for f in unreached if "&mut self" in f.text.split("{")[0]]
    print(f"harnesses: {len(harness)}; deployed engine functions: {len(allids)}; reached by at least one harness: {len(reached & allids)}; "
          f"not reached: {len(unreached)} ({len(mut)} of them take &mut self)")
    if "--paths" in sys.argv:
        for n in sys.argv[sys.argv.index("--paths") + 1].split(","):
            for f in by_name.get(n, []):
                if id(f) in first_path:
                    h, chain = first_path[id(f)]
                    k = sum(1 for r in harness.values() if id(f) in r)
                    print(f"{n} ({f.type}): reached by {k} harnesses; e.g. {h}: {' -> '.join(chain)}")
                else:
                    print(f"{n} ({f.type}): NOT reached by any harness")
    else:
        for f in unreached:
            print(f"{f.text.count(chr(10)) + 1:5d}  {'mut ' if '&mut self' in f.text.split('{')[0] else '    '} {f.name}  ({f.type}, line {f.line})")
