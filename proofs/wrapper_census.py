#!/usr/bin/env python3
"""wrapper_census.py <wrapper src/v16_program.rs> <engine src/v16.rs>
For each instruction of the program: its handler, the engine entry points the handler can reach
(call graph by callee name inside the wrapper source, an over-approximation), the classes of those
entry points (transition_kinds.REVIEWED), and how many `validate_shape()` calls the handler's own
body and its helpers contain. Prints a table; the order of check and engine call on each path is
NOT decided here (see the notes column in the spec, filled by reading)."""
import re, sys, os, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rs_scan import scan
import transition_kinds as tk
wsrc = open(sys.argv[1]).read()
esrc = open(sys.argv[2]).read()
rows = {r["name"]: r for r in tk.classify(esrc)}
fns = {}
for kind, name, hdrs, line, text in scan(wsrc):
    if kind == "fn" and "{" in text:
        body = re.sub(r"//[^\n]*", "", text[text.index("{"):])
        fns.setdefault(name, []).append((line, body))
names = set(fns)
def calls(body):
    return {m for m in re.findall(r"\b([a-z_][a-z_0-9]*)\s*(?:::\s*<[^>()]*>\s*)?\(", body) if m in names}
def reach(name):
    seen, todo = {name}, [name]
    while todo:
        n = todo.pop()
        for line, body in fns.get(n, []):
            for c in calls(body):
                if c not in seen:
                    seen.add(c); todo.append(c)
    return seen
# dispatch: Instruction::Variant ... => handle_x(  (first handler call after the variant, before the next variant)
start = wsrc.index("pub enum Instruction")
variants = re.findall(r"(?m)^        ([A-Z][A-Za-z0-9]*)\s*[{,(]?", wsrc[start:wsrc.index("\n    }\n", start)])
disp = {}
for m in re.finditer(r"Instruction::([A-Za-z0-9]+)\b", wsrc):
    v = m.group(1)
    if v in disp or v not in variants:
        continue
    seg = wsrc[m.end():m.end() + 1500]
    nxt = re.search(r"\n\s{12}Instruction::[A-Za-z0-9]+", seg)
    if nxt:
        seg = seg[:nxt.start()]
    h = re.search(r"=>\s*\{?\s*(handle_[a-z_0-9]+)\s*\(", seg)
    if h:
        disp[v] = h.group(1)
# What reading the handler showed about the order of engine call and shape check (program main 3a690205).
NOTES = {
    "PermissionlessCrank": "Live and Recovery: each of the handler's four `Ok` exits is preceded by `group.validate_shape()` (program "
        "#485; read at 78ad2c53), so a refresh, a B-chunk settlement, a step of a pending close or a Recovery refresh commits a "
        "checked header. After `NonProgress` from the auto crank the handler commits only if it accrued the market itself and "
        "`insurance` and the account's position vector are unchanged (X3). Resolved: routed to the CloseResolved handler, which validates",
    "RebalanceReduce": "`with_one_portfolio_view` validates the market header for every caller (program #485); "
        "`wrapper_post_validate = false` gates only the portfolio audit, which a maximum-shape exit cannot afford",
    "CloseResolved": "validates the shape after the auto crank returns, so the unchecked `ProgressOnly` paths of T15 are covered here",
    "ForfeitRecoveryLeg": "`wrapper_post_validate = true`: validated after the engine call",
    "ConvertReleasedPnl": "`wrapper_post_validate = true`: validated after the engine call",
    "FinalizeResetSide": "no check in the handler; the engine call is of class frame and validates the aggregate totals itself",
    "InitPortfolio": "no check in the handler; the engine call is of class frame",
    "ClosePortfolio": "no check in the handler; the engine call is of class frame",
    "CloseSlab": "no check in the handler; the two unchecked paths of T10 perform only T09's write",
    "ClaimResolvedPayoutTopup": "T09 is followed by T14, which ends in the check",
}
print(f"""# Program instructions and the shape check after their engine calls (layer W, first census)

For each instruction of the program: its handler, the engine entry points the handler can reach, and
the classes those entry points have in `transition_shape_spec`. Reachability follows calls by name
inside the program source, so the set can only be too large. The last column counts
`validate_shape()` calls in the handler's own body and in the handler with its helpers; it does not
say whether a check follows the last engine call on every path. Where that was established by
reading the handler, the note says what was found.

The point of the table: an entry point of class final checks the shape itself, so an instruction
that reaches only such entry points commits a checked header. For the other classes the program
has to check after the call, or the engine's own preservation proof (layer T) is the only
safeguard. That was the case for PermissionlessCrank with unchanged positions and for
RebalanceReduce; since program #485 both validate the header before they commit.
""")
print("| Instruction | Handler | Engine entry points reached (class) | `validate_shape()` calls: handler body / with helpers | Found when read |")
print("|---|---|---|---|---|")
summary = collections.Counter()
for v in variants:
    h = disp.get(v)
    if not h:
        print(f"| {v} | not found in the dispatch | | | |"); continue
    R = reach(h)
    eng = set()
    for n in R:
        for line, body in fns.get(n, []):
            eng |= set(re.findall(r"\.\s*([a-z_0-9]+_not_atomic)\s*\(", body))
    eng = sorted(e for e in eng if e in rows)
    own = sum(len(re.findall(r"\.validate_shape\(\)", body)) for line, body in fns.get(h, []))
    allv = sum(len(re.findall(r"\.validate_shape\(\)", body)) for n in R for line, body in fns.get(n, []))
    classes = {rows[e]["cls"] for e in eng}
    worst = next((c for c in ("dispatch", "real", "mixed", "local", "frame", "delegates", "final") if c in classes), "none")
    summary[worst] += 1
    cell = ", ".join(f"{rows[e]['label']} ({rows[e]['cls']})" for e in eng) or "none"
    print(f"| {v} | `{h}` | {cell} | {own} / {allv} | {NOTES.get(v, '')} |")
print()
print("Instructions by the weakest class of engine entry point they can reach:", dict(summary))
