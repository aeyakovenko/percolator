#!/usr/bin/env python3
"""header_writes.py <src/v16.rs> <rev> [--all] > specs/header_write_sets_<rev>.md

Splits the header half of layer T into (entry point x check) pieces.

`MarketGroupV16View::validate_shape` (default build) is a conjunction of checks H0..H17 over the
market header. For each public `*_not_atomic` entry point this script computes an OVER-APPROXIMATION
of the header fields a default-build execution can write (direct assignments in the function, plus
those of every function it can reach through a call graph resolved by callee name and, where the
call names it, receiver type). A check whose fields are all outside that set is a frame piece: the
entry point cannot change its truth. The other (entry point, check) pairs are the real pieces.

Fail-closed points:
- the read set of every check is written out below (CHECKS) and compared with the fields the two
  function bodies actually read; a difference stops the script;
- every assignment whose target field has the name of a header field must have a receiver this
  script knows (`self.header`, or `self` inside `impl MarketGroupV16HeaderAccount`, or one of the
  receivers listed in NOT_HEADER); anything else stops the script.
"""
import re, sys, collections
from rs_scan import scan

src_path, rev = sys.argv[1], sys.argv[2]
show_all = "--all" in sys.argv
src = open(src_path).read()

# ------------------------------------------------------------------ the checks of validate_shape
CHECKS = [
    ("H0", "`markets.len() == asset_slot_capacity >= config.max_market_slots`", {"asset_slot_capacity", "config"}),
    ("H1", "`config.try_to_runtime_shape()` is `Ok`", {"config"}),
    ("H2", "the four flags decode as booleans", {"bankruptcy_hlock_active", "threshold_stress_active", "loss_stale_active", "payout_snapshot_captured"}),
    ("H3", "`recovery_reason` decodes", {"recovery_reason"}),
    ("H4", "`resolved_payout_ledger` decodes", {"resolved_payout_ledger"}),
    ("H5", "`vault <= MAX_VAULT_TVL`", {"vault"}),
    ("H6", "`c_tot <= vault` and `insurance <= vault`", {"c_tot", "insurance", "vault"}),
    ("H7", "`pnl_matured_pos_tot <= pnl_pos_tot <= pnl_pos_bound_tot`", {"pnl_matured_pos_tot", "pnl_pos_tot", "pnl_pos_bound_tot"}),
    ("H8", "`slot_last <= current_slot`", {"slot_last", "current_slot"}),
    ("H9", "`next_market_id != 0`", {"next_market_id"}),
    ("H10", "`pnl_pos_bound_tot == amount_from_bound_num(pnl_pos_bound_tot_num)`", {"pnl_pos_bound_tot", "pnl_pos_bound_tot_num"}),
    ("H11", "`pnl_pos_bound_tot_num >= bound_num_from_amount(pnl_pos_tot)`", {"pnl_pos_bound_tot_num", "pnl_pos_tot"}),
    ("H12", "`payout_snapshot_captured == 0` implies `resolved_payout_ledger == EMPTY`", {"payout_snapshot_captured", "resolved_payout_ledger"}),
    ("H13", "`asset_activation_count == 0` implies `last_asset_activation_slot == 0`; otherwise `last_asset_activation_slot <= current_slot`",
     {"asset_activation_count", "last_asset_activation_slot", "current_slot"}),
    ("H14", "`c_tot + insurance + backing_provider_earnings_total <= vault`", {"c_tot", "insurance", "backing_provider_earnings_total", "vault"}),
    ("H15", "`c_tot + insurance + backing_provider_earnings_total + source_fresh_backing_total_num / BOUND_SCALE <= vault`",
     {"c_tot", "insurance", "backing_provider_earnings_total", "source_fresh_backing_total_num", "vault"}),
    ("H16", "`source_insurance_credit_reserved_total_atoms <= insurance` and `insurance_domain_budget_remaining_total <= insurance`",
     {"source_insurance_credit_reserved_total_atoms", "insurance_domain_budget_remaining_total", "insurance"}),
    ("H17", "`pnl_pos_bound_tot_num >= source_claim_bound_total_num`", {"pnl_pos_bound_tot_num", "source_claim_bound_total_num"}),
]
# receivers that carry a field of the same name but are not the market header
NOT_HEADER = {"asset": {"slot_last"}, "canonical_asset": {"slot_last"}, "totals": {"resolved_payout_blocker_count"}}
HEADER_TYPE = "MarketGroupV16HeaderAccount"

# ------------------------------------------------------------------ header fields
m = re.search(r"pub struct " + HEADER_TYPE + r"\s*\{(.*?)\n\}", src, re.S)
if not m:
    sys.exit("header struct not found")
FIELDS = [n for n, _ in re.findall(r"(?m)^\s*pub(?:\([a-z]+\))?\s+([a-z_0-9]+)\s*:\s*([^,\n]+),", m.group(1))]
FIELD_SET = set(FIELDS)

# ------------------------------------------------------------------ cfg evaluation for a default build
def cfg_holds(expr):
    """Evaluate a cfg(...) predicate for the deployed build: no test, no kani, no cargo feature."""
    expr = expr.strip()
    toks = re.findall(r'[A-Za-z_]\w*\s*=\s*"[^"]*"|[A-Za-z_]\w*|[(),]', expr)
    pos = 0
    def parse():
        nonlocal pos
        t = toks[pos]; pos += 1
        if t in ("any", "all", "not") and pos < len(toks) and toks[pos] == "(":
            pos += 1
            args = []
            while toks[pos] != ")":
                args.append(parse())
                if toks[pos] == ",":
                    pos += 1
            pos += 1
            return any(args) if t == "any" else all(args) if t == "all" else not args[0]
        if t.startswith("feature"):
            return False
        if t in ("test", "kani", "debug_assertions", "doc", "miri"):
            return False
        if t.startswith("target_") or t in ("unix",):
            return True
        sys.exit(f"cfg atom this script does not know: {t!r} in cfg({expr})")
    return parse()

def attr_cfgs(text):
    """cfg predicates of the attribute block that precedes an item's keyword."""
    head = re.split(r"\b(?:pub(?:\([a-z]+\))?\s+)?(?:(?:const|unsafe|async)\s+)*(?:fn|impl|mod|trait|struct|enum)\b", text, maxsplit=1)[0]
    out = []
    for mm in re.finditer(r"#\[cfg\(", head):
        i = mm.end(); depth = 1
        while depth:
            depth += {"(": 1, ")": -1}.get(head[i], 0); i += 1
        out.append(head[mm.end():i - 1])
    return out

def strip_comments(t):
    t = re.sub(r"//[^\n]*", "", t)
    t = re.sub(r"/\*.*?\*/", "", t, flags=re.S)
    # rustfmt breaks long method chains before each `.`; join them so `self\n.header\n.field` is one place
    return re.sub(r"\s*\n\s*\.(?!\.)", ".", t)

def impl_type(hdr):
    """Type name an impl block is for (`impl<..> Trait for Type<..>` -> Type)."""
    h = re.sub(r"#\[[^\]]*\]", "", hdr)
    mm = re.search(r"\bimpl\b(?:\s*<[^{]*?>)?\s*(.*)$", h)
    if not mm:
        return None
    rest = mm.group(1)
    if " for " in rest:
        rest = rest.split(" for ", 1)[1]
    mm = re.match(r"\s*(?:&\s*(?:'\w+\s+)?(?:mut\s+)?)?([A-Za-z_]\w*)", rest)
    return mm.group(1) if mm else None

# ------------------------------------------------------------------ functions of the default build
items = scan(src)
Fn = collections.namedtuple("Fn", "name type line text body writes calls")
fns = []
skipped = 0
for kind, name, hdrs, line, text in items:
    if kind != "fn":
        continue
    cfgs = attr_cfgs(text)
    for h in hdrs:
        cfgs += attr_cfgs(h)
    if not all(cfg_holds(c) for c in cfgs):
        skipped += 1
        continue
    if "{" not in text:
        continue            # a trait method declaration
    typ = impl_type(hdrs[-1]) if hdrs and re.search(r"\bimpl\b", hdrs[-1]) else None
    body = strip_comments(text[text.index("{"):])
    # cfg-gated blocks and statements inside the body that are not in a default build
    out, i = [], 0
    for mm in re.finditer(r"#\[cfg\(", body):
        if mm.start() < i:
            continue
        j = mm.end(); depth = 1
        while depth:
            depth += {"(": 1, ")": -1}.get(body[j], 0); j += 1
        pred = body[mm.end():j - 1]
        j = body.index("]", j) + 1
        if cfg_holds(pred):
            continue
        # drop the next block `{...}` or the next statement up to `;`
        k = j
        while body[k].isspace():
            k += 1
        if body[k] == "{":
            depth, e = 0, k
            while True:
                depth += {"{": 1, "}": -1}.get(body[e], 0); e += 1
                if depth == 0:
                    break
        else:
            depth, e = 0, k
            while True:
                c = body[e]
                depth += {"{": 1, "(": 1, "[": 1, "}": -1, ")": -1, "]": -1}.get(c, 0); e += 1
                if depth == 0 and c in ";}":
                    break
        out.append(body[i:mm.start()]); i = e
    out.append(body[i:])
    body = "".join(out)
    fns.append(Fn(name, typ, line, text, body, set(), set()))

by_name = collections.defaultdict(list)
for f in fns:
    by_name[f.name].append(f)

# ------------------------------------------------------------------ direct writes
ASSIGN = re.compile(r"([A-Za-z_]\w*(?:\.[A-Za-z_]\w*|\[[^\]\n]*\]|\(\))*)\.([a-z_0-9]+)((?:\.[a-z_0-9]+|\[[^\]\n]*\])*)\s*(=(?!=)|\+=|-=|\*=|/=|%=|\|=|&=|\^=|<<=|>>=)")
BORROW = re.compile(r"&mut\s+(\*?\s*self\.header|self)\.([a-z_0-9]+)")
WHOLE = re.compile(r"(\*\s*self\.header\s*=(?!=)|&mut\s+\*?\s*self\.header\b(?!\.))")
for f in fns:
    for mm in ASSIGN.finditer(f.body):
        root, field = mm.group(1), mm.group(2)
        if field not in FIELD_SET:
            continue
        if root == "self.header" or (root == "self" and f.type == HEADER_TYPE):
            f.writes.add(field)
        elif root in NOT_HEADER and field in NOT_HEADER[root]:
            pass
        elif root == "self" and f.type != HEADER_TYPE:
            sys.exit(f"assignment to self.{field} in `{f.name}` (line {f.line}), impl of {f.type}: is that the header?")
        else:
            sys.exit(f"unknown receiver `{root}` for an assignment to `.{field}` in `{f.name}` (line {f.line})")
    for mm in BORROW.finditer(f.body):
        root, field = mm.group(1), mm.group(2)
        if field in FIELD_SET and ("header" in root or f.type == HEADER_TYPE):
            f.writes.add(field)
    if WHOLE.search(f.body):
        sys.exit(f"`{f.name}` (line {f.line}) takes or assigns the whole header mutably; this script does not model that")

# ------------------------------------------------------------------ call graph
CALL = re.compile(r"(?:([A-Za-z_]\w*)\s*::\s*(?:<[^>()]*>\s*::\s*)?|(\.)\s*)?([a-z_][a-z_0-9]*)\s*(?:::\s*<[^>()]*>\s*)?\(")
names = set(by_name)
for f in fns:
    for mm in CALL.finditer(f.body):
        qual, dot, callee = mm.group(1), mm.group(2), mm.group(3)
        if callee not in names:
            continue
        cands = by_name[callee]
        if qual and qual not in ("Self", "self", "super", "crate"):
            typed = [c for c in cands if c.type == qual]
            if typed:
                cands = typed
        elif qual == "Self":
            typed = [c for c in cands if c.type == f.type]
            if typed:
                cands = typed
        elif dot:
            pre = f.body[max(0, mm.start() - 5):mm.start()]
            if pre.endswith("self"):
                typed = [c for c in cands if c.type == f.type]
                if typed:
                    cands = typed
        else:
            free = [c for c in cands if c.type is None]
            if free:
                cands = free
        for c in cands:
            f.calls.add(id(c))
by_id = {id(f): f for f in fns}

def reach(f):
    seen, todo = {id(f)}, [f]
    while todo:
        g = todo.pop()
        for cid in g.calls:
            if cid not in seen:
                seen.add(cid); todo.append(by_id[cid])
    return [by_id[i] for i in seen]

# ------------------------------------------------------------------ check the CHECKS table against the source
def body_of(name, typ_part):
    for f in fns:
        if f.name == name and f.type and typ_part in f.type:
            return f.body
    sys.exit(f"{name} not found")
vs = [f for f in fns if f.name == "validate_shape" and f.type == "MarketGroupV16View"]
if len(vs) != 1:
    sys.exit(f"expected one validate_shape on MarketGroupV16View, found {len(vs)}")
read_in_source = set(re.findall(r"self\.header\.([a-z_0-9]+)", vs[0].body + body_of("validate_header_aggregate_totals", "MarketGroupV16View")))
read_in_table = set().union(*(c[2] for c in CHECKS))
if read_in_source != read_in_table:
    sys.exit(f"CHECKS is stale: only in source {sorted(read_in_source - read_in_table)}, only in table {sorted(read_in_table - read_in_source)}")

# ------------------------------------------------------------------ entry points
sys.path.insert(0, ".")
import transition_kinds as tk
rows = tk.classify(src)
# classes of transition_kinds.REVIEWED with at least one Ok path that ends without a shape check
NO_OWN_CHECK = ("local", "frame", "real", "mixed", "dispatch")
print(f"# Header write sets and the (entry point x check) split of layer T (engine revision {rev})\n")
print(f"""`MarketGroupV16View::validate_shape` in a default build is the conjunction of the checks H0 to H17
below. The table after it gives, for each deployed entry point with at least one Ok path that ends
without a shape check (classes local, frame, real, mixed and dispatch of
`aristotle/transition_shape_spec.md`), an over-approximation of the header fields an execution can
write: the direct assignments of the entry point and of every function it can reach. Calls are resolved by callee name (and by
receiver type where the call names it), so the set can only be too large, never too small.
Code under `cfg(test)`, `cfg(kani)` or a cargo feature is left out ({skipped} functions).

A check that reads none of the written fields is a **frame** piece for that entry point: it holds
after the call because it held before. The remaining pairs are the real pieces; each names the
fields that have to be argued about.

The header has {len(FIELDS)} fields. `validate_shape` reads {len(read_in_table)} of them. Never read by it:
{", ".join("`" + n + "`" for n in FIELDS if n not in read_in_table)}.

## The checks

| Check | Statement | Header fields it reads |
|---|---|---|""")
for cid, text, reads in CHECKS:
    print(f"| {cid} | {text} | {', '.join('`' + r + '`' for r in sorted(reads))} |")
print("\nH0 also reads the length of the slice of market slots, which no engine function changes.\n")

def entry_fn(name):
    c = by_name.get(name, [])
    return c[0] if c else None

sel = []
for r in rows:
    if r["deployed"] != "yes":
        continue
    unchecked = r["cls"] in NO_OWN_CHECK
    if unchecked or show_all:
        sel.append((r, unchecked))
print("## Entry points with an Ok path that ends without a shape check\n" if not show_all else "## Every deployed entry point\n")
print("The class is the one of `aristotle/transition_shape_spec.md`. For a mixed entry point and for the dispatcher the set\n"
      "covers all of its paths, the checked ones included, so it is larger than what the unchecked paths write.\n")
print("| Label | Entry point | Class | Header fields it can write | Checks it can affect | Frame checks |\n|---|---|---|---|---|---|")
real_pairs, frame_pairs, detail = [], [], {}
for r, unchecked in sel:
    f = entry_fn(r["name"])
    if f is None:
        sys.exit(f"entry point {r['name']} not found among default-build functions")
    writers = collections.defaultdict(set)
    for g in reach(f):
        for w in g.writes:
            writers[w].add(g.name)
    w = set(writers)
    affected = [c for c in CHECKS if c[2] & w]
    frame = [c for c in CHECKS if not (c[2] & w)]
    detail[r["label"]] = (r, writers, affected)
    if unchecked:
        real_pairs += [(r["label"], c[0]) for c in affected]
        frame_pairs += [(r["label"], c[0]) for c in frame]
    print(f"| {r['label']} | `{r['name']}` | {r['cls']} | {', '.join('`' + x + '`' for x in sorted(w)) or 'none'} | "
          f"{', '.join(c[0] for c in affected) or 'none'} | {len(frame)} of {len(CHECKS)} |")
print(f"\nPairs for the entry points without a shape check: {len(real_pairs)} real, {len(frame_pairs)} frame "
      f"(of {len(real_pairs) + len(frame_pairs)}).\n")
# ---- the writers: one lemma per function that assigns a field some check reads
unchecked_labels = [l for l, (r, _, _) in detail.items() if r["cls"] in NO_OWN_CHECK]
reached_by = collections.defaultdict(list)
for label in unchecked_labels:
    r, writers, _ = detail[label]
    for fld, ws in writers.items():
        if fld in read_in_table:
            for wname in ws:
                if label not in reached_by[wname]:
                    reached_by[wname].append(label)
print("## The writers behind those entry points\n")
print("""Every header write of the entry points above happens in one of the functions below; they are the
places a proof has to look at. Some writers keep their checks on their own (the two accrual entry
points, the slot and activation writers, every flag write): for those the lemma is *if the checks
it can affect hold before the call and the call returns `Ok`, they hold after it*. Others only apply
a delta to a total that mirrors per-domain state (`update_source_credit_aggregate_totals`,
`update_backing_aggregate_totals`, `set_domain_insurance_spent_core`) or move value out of one total
(the `c_tot` and `insurance` writers): their checks are kept by the calling function together with
the change the delta mirrors, and H17 by a relation between the header and the accounts. For those
the unit of proof is the caller, and H alone is not an inductive invariant.

Per entry point the composition also needs that every writer's error is propagated (the caller of a
`*_not_atomic` function discards the state on `Err`). One place turns an error into `Ok`: the crank
commits a `RecoveryRequired` error when the same call declared Recovery
(`kernel_commit_declared_liquidation_recovery`); the declaration itself validates the shape.

| Lemma | Writer | Line | Fields it assigns that a check reads | Checks | Reached from |
|---|---|---|---|---|---|""")
wl = []
for f in sorted(fns, key=lambda f: f.line):
    if f.name in reached_by and f.writes & read_in_table:
        wl.append(f)
for n, f in enumerate(wl, 1):
    flds = sorted(f.writes & read_in_table)
    hs = [c[0] for c in CHECKS if c[2] & f.writes]
    print(f"| W{n:02d} | `{f.name}` | {f.line} | {', '.join('`' + x + '`' for x in flds)} | {', '.join(hs)} | {', '.join(reached_by[f.name])} |")
print(f"\n{len(wl)} writers.\n")
others = [f for f in sorted(fns, key=lambda f: f.line) if f.writes & read_in_table and f.name not in reached_by]
print("Functions that assign a checked field but are reached only from entry points that re-validate the shape "
      f"themselves ({len(others)}): " + ", ".join(f"`{f.name}`" for f in others) + ".\n")
print("## Which function writes each field, per entry point\n")
for label, (r, writers, affected) in detail.items():
    if not (r["cls"] in NO_OWN_CHECK):
        continue
    print(f"### {label} `{r['name']}`\n")
    if not writers:
        print("Writes no header field.\n")
        continue
    for fld in sorted(writers):
        hs = [c[0] for c in CHECKS if fld in c[2]]
        print(f"- `{fld}` ({', '.join(hs) if hs else 'read by no check'}): written in {', '.join('`' + n + '`' for n in sorted(writers[fld]))}")
    print()
