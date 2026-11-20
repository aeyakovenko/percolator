#!/usr/bin/env python3
"""changed_fns.py <old src/v16.rs> <new src/v16.rs> [<kernel list tsv of the new revision>]
Which proofs made against the old revision still stand for the new one.

1. Every `fn` of each file, keyed by (enclosing impl/trait/mod header, name), with its attribute
   block, signature and body. Reports functions whose text differs, new ones and removed ones.
2. Every `struct`/`enum` whose text differs, and the types that contain a changed type by value.
3. For each kernel of the list: is its own text unchanged, does it reach (by callee NAME, an
   over-approximation) a changed or new function, and does its text or a reached function's text
   name a changed type.

A kernel needs no new proof exactly when all three answers are "no".
Comments are not stripped: a comment-only edit counts as a change (conservative)."""
import re, sys, collections

from rs_scan import scan, index, norm

old_src, new_src = open(sys.argv[1]).read(), open(sys.argv[2]).read()
oi, ni = scan(old_src), scan(new_src)
of, nf = index(oi, ("fn",)), index(ni, ("fn",))
ot, nt = index(oi, ("struct", "enum")), index(ni, ("struct", "enum"))
print(f"functions: old {len(of)}, new {len(nf)}; types: old {len(ot)}, new {len(nt)}")

changed = [k for k in nf if k in of and norm(of[k][1]) != norm(nf[k][1])]
added = [k for k in nf if k not in of]
removed = [k for k in of if k not in nf]
print(f"\n## Functions: {len(changed)} changed, {len(added)} new, {len(removed)} removed")
def gate(text):
    m = re.findall(r"#\[cfg\(([^\n]*)\)\]", text.split("fn ")[0])
    return f" [cfg({m[-1]})]" if m else ""
for tag, ks, tab in (("changed", changed, nf), ("new", added, nf), ("removed", removed, of)):
    for k in ks:
        print(f"- {tag}: `{k[1]}` (line {tab[k][0]}){gate(tab[k][1])}  in `{k[0][:70]}`")

tchanged = [k for k in nt if k in ot and norm(ot[k][1]) != norm(nt[k][1])]
tadded = [k for k in nt if k not in ot]
tremoved = [k for k in ot if k not in nt]
print(f"\n## Types: {len(tchanged)} changed, {len(tadded)} new, {len(tremoved)} removed")
for tag, ks in (("changed", tchanged), ("new", tadded), ("removed", tremoved)):
    for k in ks:
        print(f"- {tag}: `{k[1]}`")
# types containing a changed type (by naming it anywhere in their definition), to a fixed point
tainted = {k[1] for k in tchanged} | {k[1] for k in tadded}
grew = True
while grew:
    grew = False
    for k, (_, text) in nt.items():
        if k[1] in tainted:
            continue
        base = k[1].split("#")[0]
        body = text[text.index(base) + len(base):]
        if any(re.search(rf"\b{re.escape(t)}\b", body) for t in tainted):
            tainted.add(k[1]); grew = True
print(f"types that are or contain a changed type: {sorted(tainted)}")

if len(sys.argv) > 3:
    names_new = collections.defaultdict(list)
    for k in nf:
        names_new[k[1].split("#")[0]].append(k)
    bad_names = {k[1].split("#")[0] for k in changed} | {k[1].split("#")[0] for k in added}
    call_re = re.compile(r"\b([a-z_][a-z0-9_]*)\s*(?:::<[^>()]*>)?\(")
    def callees(key):
        text = nf[key][1]
        body = text[text.index("{"):] if "{" in text else ""
        return {m.group(1) for m in call_re.finditer(body) if m.group(1) in names_new}
    def reach(name):
        seen, todo = set(), [name]
        while todo:
            x = todo.pop()
            for key in names_new.get(x, []):
                for c in callees(key):
                    if c not in seen:
                        seen.add(c); todo.append(c)
        return seen
    print("\n## Kernels of the list")
    print("| Label | Function | Own text | Reaches a changed/new function (by name) | Names a changed type (own text or a reached function) | New proof needed |")
    print("|---|---|---|---|---|---|")
    need = []
    for line in open(sys.argv[3]):
        f = line.rstrip("\n").split("\t")
        if len(f) < 2:
            continue
        label, name = f[0], f[1]
        keys = names_new.get(name, [])
        if not keys:
            print(f"| {label} | `{name}` | REMOVED | | | n/a |"); continue
        own_changed = any(k in changed or k in added for k in keys)
        own = "new" if all(k in added for k in keys) else ("changed" if own_changed else "same")
        r = reach(name)
        hits = sorted(r & bad_names)
        texts = [nf[k][1] for k in keys] + [nf[k][1] for x in r for k in names_new[x]]
        thit = sorted({t for t in tainted if any(re.search(rf"\b{re.escape(t)}\b", tx) for tx in texts)})
        own_thit = sorted({t for t in tainted if any(re.search(rf"\b{re.escape(t)}\b", nf[k][1]) for k in keys)})
        # a method that takes `self` of a type that is or contains a changed type is affected too
        for k in keys:
            sig = nf[k][1][:nf[k][1].index("{")] if "{" in nf[k][1] else nf[k][1]
            if re.search(r"\(\s*(&\s*(mut\s+)?)?(mut\s+)?self\b", sig):
                thit = sorted(set(thit) | {t for t in tainted if re.search(rf"\b{re.escape(t)}\b", k[0])})
        needs = own != "same" or bool(hits) or bool(thit)
        if needs:
            need.append(label)
        print(f"| {label} | `{name}` | {own} | {', '.join(hits) or 'no'} | {', '.join(thit) or 'no'}{' (own text: ' + ', '.join(own_thit) + ')' if own_thit and own_thit != thit else ''} | {'YES' if needs else 'no'} |")
    print(f"\nkernels that need a new proof: {len(need)}: {' '.join(need)}")
