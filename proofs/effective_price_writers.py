#!/usr/bin/env python3
"""effective_price_writers.py <engine src/v16.rs> <program src/v16_program.rs> <expected tsv>
INV-045 clause C1: lists every site in the two files that can write an asset's effective price and checks the
deployed ones against the expected writer table (specs/inv045_effective_price_writers_<rev>.md, rows E1-E6 and
P1-P3, kept as a tsv). A site is
  - an assignment to a field named `effective_price` (`x.effective_price = ..`, `.set(..)`; a bare
    `effective_price = ..` or `let effective_price ..` is a local variable and is skipped),
  - a struct literal with an `effective_price:` field of the asset record (`AssetStateV16`,
    `AssetStateV16Account`, or `Self` in their impls); literals of other structs with a field of that name
    (requests, observations, path steps) are listed as "other struct" and not checked,
  - a fresh asset record or asset slot (`AssetStateV16::default()` / `EMPTY`, `AssetStateV16Account::default()`,
    `EngineAssetSlotV16Account::default()` / `zeroed()`),
  - a whole-slot assignment (`.engine = ..`, `markets[..] = ..`).
Each site is classified by the cfg of its function and enclosing blocks: deployed, host-only
(`not(target_os = "solana")`), or proof/test (`kani`, `test`, `fuzz`, `audit-scan`). Exit 1 if a deployed site
lies in a function the table does not name, or a function the table names has no site (the table is stale).
Second check (data flow the patterns cannot see): every engine call `set_asset_state(i, rec)` must write back a
record read from the same slot (`let [mut] rec = self.asset_state(i)?` in the same function) or be listed in the
table (label `writeback`, with the reading that shows the record is the slot's own, or a writer label E1-E6).
Over-approximates by design: a literal of another struct with the same field name is reported with its type,
and the table lists such functions with the reason they do not write an asset's price."""
import os, re, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rs_scan

PATTERNS = [
    ("assign", re.compile(r"\.\s*effective_price\s*=(?![=>])")),
    ("set", re.compile(r"\beffective_price\s*\.\s*set\s*\(")),
    ("field", re.compile(r"(?<![\w.])effective_price\s*:(?!:)")),
    ("fresh", re.compile(r"\b(AssetStateV16(?:Account)?\s*::\s*(?:default\s*\(\s*\)|EMPTY)|EngineAssetSlotV16Account\s*::\s*(?:default|zeroed|empty_for_market)\s*\()")),
    ("slot", re.compile(r"(\.engine\s*=(?![=>])|\bmarkets\s*\[[^\]]*\]\s*=(?![=>]))")),
]
PROOF_CFG = re.compile(r"cfg\s*\(\s*(?:all\s*\(\s*)?(?:kani|test|feature\s*=\s*\"(?:fuzz|audit-scan|contracts|closure)\")")
ANY_PROOF_CFG = re.compile(r"cfg\s*\(\s*any\s*\(([^)]*)\)\s*\)")
HOST_CFG = re.compile(r"cfg\s*\(\s*not\s*\(\s*target_os\s*=\s*\"solana\"\s*\)\s*\)")

def cfg_class(attr_texts):
    for t in attr_texts:
        # only the attributes in front of the item count, not the body
        head = t.split("{", 1)[0] if t.lstrip().startswith(("#", "///", "//", "pub", "fn", "impl", "mod", "const", "unsafe")) else t
        for line in head.splitlines():
            s = line.strip()
            if not s.startswith("#["):
                continue
            if HOST_CFG.search(s):
                return "host-only"
            if PROOF_CFG.search(s):
                return "proof/test"
            m = ANY_PROOF_CFG.search(s)
            if m and all(re.search(r"kani|test|feature\s*=\s*\"(fuzz|audit-scan|contracts|closure)\"", x)
                         for x in m.group(1).split(",") if x.strip()):
                return "proof/test"
    return "deployed"

def literal_type(body, pos):
    """Type of the struct literal whose field starts at body[pos]: the path before the innermost unclosed '{'."""
    depth = 0
    i = pos
    while i > 0:
        i -= 1
        c = body[i]
        if c == "}":
            depth += 1
        elif c == "{":
            if depth == 0:
                m = re.search(r"([A-Za-z_][\w:<>]*)\s*$", body[:i])
                return m.group(1) if m else "?"
            depth -= 1
    return "?"

def sites(path, label):
    src = open(path).read()
    out = []
    for kind, name, stack, start, text in rs_scan.scan(src):
        if kind != "fn":
            continue
        brace = text.find("{")
        if brace < 0:
            continue
        body = text[brace:]
        body_line0 = start + text[:brace].count("\n")
        cls = cfg_class([text] + list(stack))
        for pk, rx in PATTERNS:
            for m in rx.finditer(body):
                line = body_line0 + body[:m.start()].count("\n")
                ltxt = body[body.rfind("\n", 0, m.start()) + 1: body.find("\n", m.start())].strip()
                if ltxt.startswith("//"):
                    continue
                cls_here = cls
                if pk == "field":
                    if re.match(r"let\s+(mut\s+)?effective_price\s*:", ltxt):
                        continue  # a local variable's type ascription
                    extra = literal_type(body, m.start())
                    if extra == "Self":
                        impl = next((h for h in reversed(list(stack)) if re.search(r"\bimpl\b", h)), "")
                        mt = (re.search(r"\bfor\s+([A-Za-z_]\w*)", impl)
                              or re.search(r"\bimpl(?:<[^>]*>)?\s+([A-Za-z_]\w*)", impl))
                        extra = "Self = " + (mt.group(1) if mt else "?")
                    if not re.search(r"\bAssetStateV16(Account)?$", extra):
                        cls_here = "other struct"
                elif pk == "assign":
                    pre = body[max(0, m.start() - 40):m.start() + 1]
                    r = re.search(r"([\w\]\[\(\)]+(?:\.[\w\]\[\(\)]+)*)\.$", pre)
                    extra = r.group(1) if r else "?"
                else:
                    extra = m.group(0).strip()
                out.append(dict(file=label, fn=name, line=line, kind=pk, what=extra, cfg=cls_here,
                                text=ltxt[:110]))
    return out

def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    eng, prog, exp = sys.argv[1:]
    expected = {}
    for line in open(exp):
        if line.strip() and not line.startswith("#"):
            f, fn, label, reason = line.rstrip("\n").split("\t")
            expected[(f, fn)] = (label, reason)
    all_sites = sites(eng, "engine") + sites(prog, "program")
    bad = []
    seen = set()
    print("file\tfunction\tline\tkind\twhat\tcfg\tlabel")
    for s in all_sites:
        key = (s["file"], s["fn"])
        label = expected.get(key, ("", ""))[0]
        print(f"{s['file']}\t{s['fn']}\t{s['line']}\t{s['kind']}\t{s['what']}\t{s['cfg']}\t{label}")
        if s["cfg"] != "deployed":
            continue
        seen.add(key)
        if key not in expected:
            bad.append(s)
    # the write-backs: the record must be the slot's own
    src = open(eng).read()
    for kind, name, stack, start, text in rs_scan.scan(src):
        if kind != "fn" or name == "set_asset_state" or cfg_class([text] + list(stack)) != "deployed":
            continue
        calls = re.findall(r"set_asset_state\(\s*([^,]+?)\s*,\s*([^)]+?)\s*\)", text)
        if not calls:
            continue
        reads = re.findall(r"let\s+(?:mut\s+)?(\w+)\s*(?::\s*\w+\s*)?=\s*self\.asset_state\(\s*([^)]+?)\s*\)\s*\?", text)
        if all(any(v == rec and i == idx for v, i in reads) for idx, rec in calls):
            continue
        key = ("engine", name)
        seen.add(key)
        label = expected.get(key, ("", ""))[0]
        if not (label == "writeback" or re.fullmatch(r"E\d", label)):
            bad.append(dict(file="engine", fn=name, line=start, kind="writeback", what=str(calls),
                            text="set_asset_state with a record not read from the same slot in this function"))
    stale = [k for k in expected if k not in seen]
    print()
    print(f"{len(all_sites)} sites; {sum(1 for s in all_sites if s['cfg'] == 'deployed')} deployed, in "
          f"{len(seen)} deployed functions; table names {len(expected)} functions")
    for s in bad:
        print(f"NOT IN THE TABLE: {s['file']} {s['fn']} line {s['line']} ({s['kind']} {s['what']}): {s['text']}")
    for k in stale:
        print(f"TABLE NAMES A FUNCTION WITH NO DEPLOYED SITE: {k[0]} {k[1]}")
    print("WRITER LIST CHECKS" if not bad and not stale else "WRITER LIST DOES NOT CHECK")
    sys.exit(0 if not bad and not stale else 1)

if __name__ == "__main__":
    main()
