#!/usr/bin/env python3
"""write_sets.py <engine src/v16.rs> name [name ...]
Over-approximated write sets of engine methods, from the call graph of header_writes.py (callees by
receiver type where the text allows, else by name): the market-header fields and the account fields
(`account.header.<field> = ..`, or `self.header.<field>` inside the account view) the method or
anything it can reach assigns. Used to check a frame contract before handing it to the prover."""
import sys, re, os
here = os.path.dirname(os.path.abspath(__file__))
src_text = open(os.path.join(here, "header_writes.py")).read()
cut = src_text.index("# ------------------------------------------------------------------ check the CHECKS table against the source")
argv = sys.argv; sys.argv = ["header_writes.py", argv[1], "x"]
g = {"__name__": "hw"}; exec(compile(src_text[:cut], "header_writes.py", "exec"), g); sys.argv = argv
by_name, reach = g["by_name"], g["reach"]
ACC = re.compile(r"\b([a-z_]+)\.header\.([a-z_0-9]+)((?:\.[a-z_0-9]+|\[[^\]\n]*\])*)\s*(=(?!=)|\+=|-=|\|=|&=)")
ACCT_TYPES = ("PortfolioV16ViewMut", "PortfolioV16Account", "PortfolioAccountV16Account")
def acct_writes(R):
    w = set()
    for f in R:
        for mm in ACC.finditer(f.body):
            root, field = mm.group(1), mm.group(2)
            if root == "self" and f.type not in ACCT_TYPES: continue
            w.add(field)
        for mm in re.finditer(r"&mut\s+([a-z_]+)\.header\.([a-z_0-9]+)", f.body):
            if mm.group(1) == "self" and f.type not in ACCT_TYPES: continue
            w.add(mm.group(2))
    return w
for name in argv[2:]:
    for f in by_name.get(name, []):
        R = reach(f); hw = set()
        for x in R: hw |= x.writes
        print(f"{name} ({f.type}, line {f.line}): reaches {len(R) - 1} functions")
        print("   market header:", ", ".join(sorted(hw)) or "(none)")
        print("   account:", ", ".join(sorted(acct_writes(R))) or "(none)")
