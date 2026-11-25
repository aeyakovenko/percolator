#!/usr/bin/env python3
"""cfg_reach.py <src dir> <root fn> [<root fn> ...] [--stubbed <fn> ...]

Which code a Kani harness runs that differs between a Kani build and a deployed build.

The call graph covers the functions of src/v16.rs, src/lib.rs and src/wide_math.rs (as rs_scan.py
finds them). A call is resolved by its receiver's type where the text names it, and by name
otherwise, so the graph OVER-approximates what runs:
- `Type::name(` and `Self::name(`: the functions `name` of the impls of that type (if it has none
  among the scanned functions, the call is listed as unresolved: a primitive, a library or a
  macro-generated function);
- `self.name(`: the caller's own impl first, then any function of that name;
- `x.field.name(` (any chain ending in `.field.name(`): the functions `name` of the impls of every
  type a scanned struct gives a field of that name; if no scanned struct has the field, any
  function of that name;
- any other `.name(` or `name(`: every function of that name, except that a method call reaches
  src/wide_math.rs only from code that names U256, I256 or wide_math.
The roots are the functions the harness calls; the stubbed functions are replaced in the harness
and contribute nothing.

Reported for the reachable set: every `#[cfg(..)]`/`cfg!(..)` inside a body whose predicate has a
different value in the Kani run (`kani`, features fuzz and closure) and in the deployed build (no
`kani`, no `test`, no features), and any `cfg_attr` inside a body; every reference to an item defined differently under `cfg(kani)` and `cfg(not(kani))`
(a constant, a type); every reachable function of src/wide_math.rs (its U256 and I256 have
different representations in the two builds). Exit status 1 if anything is reported. The
unresolved calls are listed after the report.
"""
import os, re, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rs_scan import scan

FILES = ("v16.rs", "lib.rs", "wide_math.rs")


def strip_comments(t):
    t = re.sub(r"//[^\n]*", "", t)
    return re.sub(r'"(?:\\.|[^"\\])*"', '""', t)


def impl_type(hdr):
    if not hdr:
        return None
    h = hdr[-1].strip()
    if not h.startswith("impl"):
        return None
    h = h[4:]
    if h.startswith("<"):  # the impl's own generics
        depth, i = 0, 0
        for i, c in enumerate(h):
            depth += {"<": 1, ">": -1}.get(c, 0)
            if depth == 0:
                break
        h = h[i + 1:]
    h = h.strip()
    m = re.search(r"\bfor\s+&?(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)", h)
    if m:
        return m.group(1)
    m = re.match(r"&?(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)", h)
    return m.group(1) if m else None


def struct_fields(d):
    """field name -> set of base type names, over every `struct Name { field: Type, .. }`."""
    out = {}
    for f in FILES:
        src = strip_comments(open(os.path.join(d, f)).read())
        for m in re.finditer(r"\bstruct\s+[A-Za-z_][A-Za-z0-9_]*(?:<[^>{]*>)?\s*\{([^}]*)\}", src):
            for fm in re.finditer(r"(?:pub(?:\([a-z]+\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*&?(?:'[a-z_]+\s+)?(?:mut\s+)?\[?([A-Za-z_][A-Za-z0-9_]*)",
                                  m.group(1)):
                out.setdefault(fm.group(1), set()).add(fm.group(2))
    return out


def differing_items(d):
    """Names defined under cfg(kani) and cfg(not(kani)) with different text, or under one only."""
    out = {}
    for f in FILES:
        L = open(os.path.join(d, f)).read().split("\n")
        for i, l in enumerate(L):
            m = re.match(r"\s*#\[cfg\((not\()?kani\)?\)\]\s*$", l)
            if not m:
                continue
            j = i + 1
            while L[j].strip().startswith("#[") or L[j].strip().startswith("//"):
                j += 1
            nm = re.match(r"\s*(?:pub(?:\([a-z]+\))? )?(?:const|static|type|struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)", L[j])
            if not nm or nm.group(1) == "_":
                continue
            body, k, depth = "", j, 0
            while True:
                body += L[k] + "\n"
                depth += L[k].count("{") - L[k].count("}")
                if (depth <= 0 and ("{" in body or L[k].rstrip().endswith(";"))) or k > j + 400:
                    break
                k += 1
            body = re.sub(r"\bpub(\([a-z]+\))? ", "", body).strip()
            out.setdefault(nm.group(1), {})["not" if m.group(1) else "kani"] = body
    return sorted(n for n, v in out.items() if len(v) == 1 or len(set(v.values())) > 1)


KANI_RUN = ({"kani"}, {"fuzz", "closure"})      # the closure layer: cargo kani --features fuzz,closure
DEPLOYED = (set(), set())                       # the program's build of the engine: default features (none)


def cfg_value(expr, build):
    """Evaluate a cfg predicate (any/all/not, bare names, feature = "x") in a build (names, features)."""
    names, feats = build
    toks = re.findall(r'any|all|not|feature\s*=\s*"[^"]*"|[A-Za-z_][A-Za-z0-9_]*|[(),]', expr)
    pos = 0

    def parse():
        nonlocal pos
        t = toks[pos]
        pos += 1
        if t in ("any", "all", "not"):
            assert toks[pos] == "("
            pos += 1
            vals = []
            while toks[pos] != ")":
                vals.append(parse())
                if toks[pos] == ",":
                    pos += 1
            pos += 1
            return any(vals) if t == "any" else all(vals) if t == "all" else not vals[0]
        if t.startswith("feature"):
            return re.search(r'"([^"]*)"', t).group(1) in feats
        return t in names
    v = parse()
    assert pos == len(toks), expr
    return v


def cfg_differs(body):
    """The cfg predicates in a body whose value differs between the Kani run and the deployed build."""
    out = []
    for m in re.finditer(r"#\[cfg\((.*?)\)\]\s*$|cfg!\((.*?)\)", body, re.M):
        e = m.group(1) or m.group(2)
        if cfg_value(e, KANI_RUN) != cfg_value(e, DEPLOYED):
            out.append(e)
    if re.search(r"cfg_attr", body):
        out.append("cfg_attr")
    return out


def main(argv):
    d = argv[1]
    args = argv[2:]
    stubbed = set()
    if "--stubbed" in args:
        k = args.index("--stubbed")
        stubbed = set(args[k + 1:])
        args = args[:k]
    roots = args
    fns = []  # (file, name, line, impl type, text)
    raw = {}
    for f in FILES:
        src = open(os.path.join(d, f)).read()
        for kind, name, hdr, line, text in scan(src):
            if kind == "fn":
                fns.append((f, name, line, impl_type(hdr), strip_comments(text)))
                raw[(f, line)] = re.sub(r"//[^\n]*", "", text)  # strings kept: cfg predicates name features
    by_name, by_type = {}, {}
    for x in fns:
        by_name.setdefault(x[1], []).append(x)
        by_type.setdefault((x[3], x[1]), []).append(x)
    fields = struct_fields(d)
    for r in roots:
        assert r in by_name, f"no fn {r}"
    seen = set()            # ids of reachable functions
    todo = [x for r in roots for x in by_name[r]]
    unresolved = set()
    while todo:
        x = todo.pop()
        if id(x) in seen or x[1] in stubbed:
            continue
        seen.add(id(x))
        f, name, line, typ, text = x
        body = text[text.index("{") + 1:] if "{" in text else ""
        wide = f == "wide_math.rs" or re.search(r"\b(?:U256|I256|wide_math)\b", body)
        for m in re.finditer(r"(?:(\bSelf|\b[A-Z][A-Za-z0-9_]*)\s*::\s*|(\bself\s*\.\s*)|(\.\s*([a-z_][a-z0-9_]*)\s*\.\s*)|(\.\s*))?"
                             r"\b([a-z_][a-z0-9_]*)\s*(?:::<[^>]*>)?\s*\(", body):
                path, on_self, on_field, field, dot, c = m.groups()
                if path:
                    t = typ if path == "Self" else path
                    cands = by_type.get((t, c), [])
                    if not cands:
                        unresolved.add(f"{t}::{c}")
                elif on_self:
                    cands = by_type.get((typ, c), []) or by_name.get(c, [])
                elif on_field:
                    ts = fields.get(field)
                    cands = [y for t in ts for y in by_type.get((t, c), [])] if ts else by_name.get(c, [])
                    if ts and not cands:
                        unresolved.add(f"{'|'.join(sorted(ts))}::{c}")
                else:
                    cands = by_name.get(c, [])
                    if dot and not wide:
                        cands = [y for y in cands if y[0] != "wide_math.rs"]
                todo.extend(y for y in cands if id(y) not in seen)
    differ = differing_items(d)
    report = []
    reach = [x for x in fns if id(x) in seen]
    for f, name, line, typ, text in sorted(reach, key=lambda x: (x[0], x[2])):
        body = text[text.index("{") + 1:] if "{" in text else ""
        rt = raw[(f, line)]
        for e in cfg_differs(rt[rt.index("{") + 1:] if "{" in rt else ""):
            report.append(f"{f}:{line} {typ}::{name}: a cfg inside the body that differs between the builds: {e}")
        for it in differ:
            if re.search(r"\b" + it + r"\b", body):
                report.append(f"{f}:{line} {typ}::{name}: uses {it}, defined differently under cfg(kani)")
        if f == "wide_math.rs":
            report.append(f"{f}:{line} {typ}::{name}: in wide_math.rs (U256/I256 differ between builds)")
    print(f"{len(reach)} functions reachable from {', '.join(roots)}"
          + (f" (stubbed: {', '.join(sorted(stubbed))})" if stubbed else ""))
    print("items defined differently under cfg(kani):", ", ".join(differ))
    for r in report:
        print("  REPORT " + r)
    if "--list" in argv:
        for f, name, line, typ, text in sorted(reach, key=lambda x: (x[3] or "", x[1])):
            print(f"  reach {typ}::{name} ({f}:{line})")
    print("unresolved calls:", ", ".join(sorted(unresolved)))
    return 1 if report else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
