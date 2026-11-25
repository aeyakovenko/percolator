#!/usr/bin/env python3
"""gen_account_final.py <engine worktree> <out.rs> <ledger tsv>
Generates the Kani harnesses of the T*.account pieces of class "final": for each entry point, every method it calls on
the market view is replaced by a stand-in (a read-only callee returns any value; a callee that takes the view mutably
may first write any bytes to the header, the asset slots, the view's scratch and every account it is given), the
account's own check `validate_with_market` by a recorder (either outcome, remembered per account with the account, header
and slot bytes it was asked about), the view's `validate_shape` (a read) by a stand-in with either outcome, and the
harness asserts, for each account the entry point is given, that an Ok return leaves the account, the header and the
slot as they were, or follows a passed check of that account on exactly the account, header and slot it leaves.
A callee with no early Ok that ends in its accounts' checks is proved the stronger way (every Ok follows a passed check
of each of its accounts) and stands in for itself with that property in its callers.
The header, the slot and every account are arbitrary bytes. Pure functions run as written, except those named in PURE
for one harness: each takes its inputs by value or by shared reference and writes nothing, and its stand-in returns any
result. Checked here: the entry point reads no slot, uses `self` only through its header, its own checks, `as_view()`
handed to a replaced callee and the replaced callees, and uses an account only through its fields, its own check,
`as_view()` and the replaced callees. Writes the section (helpers, stand-ins, harnesses) to <out.rs>; review it, then
install it before the local header section.

Fails on anything it does not know: a callee outside the mutable view's impl block, a method with generics of its own,
a parameter type it cannot build, a return type it cannot make arbitrary."""
import csv, os, re, sys, textwrap

wt, out, ledger = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(f"{wt}/src/v16.rs").read()
lines = src.split("\n")

# the mutable view's main impl block
start = lines.index("impl<'a, T> MarketGroupV16ViewMut<'a, T> {", 7300) + 1
end = start
while lines[end] != "}":
    end += 1


def fn_span(name, text_lines, lo, hi):
    """(first line, last line) of `fn name` at method indentation in text_lines[lo:hi]."""
    for i in range(lo, hi):
        if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", text_lines[i]):
            j = i
            while text_lines[j] != "    }":
                j += 1
            return i, j
    return None


def split_params(s):
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    out.append(cur)
    return out


def signature(name):
    sp = fn_span(name, lines, start, end)
    assert sp, f"{name}: not a method of the mutable view's impl block"
    i, j = sp
    head = []
    k = i
    while True:
        head.append(lines[k])
        if lines[k].rstrip().endswith("{"):
            break
        k += 1
    h = re.sub(r"\s+", " ", " ".join(head)).strip()
    m = re.match(r"(?:pub(?:\([a-z]+\))? )?fn " + name + r"(<[^>]*>)?\s*\((.*)\)\s*(?:->\s*(.+?))?\s*\{$", h)
    assert m, h
    assert not m.group(1), f"{name}: generics of its own"
    params = [p.strip() for p in split_params(m.group(2)) if p.strip()]
    return params, (m.group(3) or "()").strip(), "\n".join(lines[i:j + 1])


ARBITRARY = {"()", "u8", "u16", "u32", "u64", "u128", "i128", "usize", "bool", "SideV16", "AssetStateV16",
             "SourceCreditStateV16", "BackingBucketV16", "InsuranceCreditReservationV16", "PermissionlessRecoveryReasonV16",
             "AssetLifecycleV16", "SideModeV16", "BackingBucketStatusV16", "PortfolioLegV16",
             # kani::Arbitrary implemented in the header-final section (every field any value)
             "TradeRequestV16", "TradeOutcomeV16", "BatchTradeOutcomeV16", "SourceCreditConsumptionV16"}
CUSTOM = {"HealthCertV16": "any_health_cert()",
          "ReservationEncumbranceProofV16": "any_reservation_encumbrance_proof()",
          "PermissionlessProgressOutcomeV16": "any_progress_outcome()"}
ACCOUNT_MUT = "&mut PortfolioV16ViewMut<'_>"
ACCOUNT_SHARED = "&PortfolioV16View<'_>"


def arbitrary_expr(ty):
    ty = ty.strip()
    m = re.fullmatch(r"V16Result<(.+)>", ty)
    if m:
        inner = m.group(1).strip()
        if inner in CUSTOM:
            return f"if kani::any() {{ Ok({CUSTOM[inner]}) }} else {{ Err(kani::any()) }}"
        check_arbitrary(inner)
        return "kani::any()"
    if ty == "()":
        return ""
    if ty in CUSTOM:
        return CUSTOM[ty]
    check_arbitrary(ty)
    return "kani::any()"


def check_arbitrary(ty):
    ty = ty.strip()
    if ty.startswith("(") and ty.endswith(")"):
        for t in split_params(ty[1:-1]):
            if t.strip():
                check_arbitrary(t)
        return
    m = re.fullmatch(r"Option<(.+)>", ty)
    if m:
        return check_arbitrary(m.group(1))
    assert ty in ARBITRARY, f"cannot make {ty} arbitrary"


# Callees whose own harness below proves that every Ok follows a passed check of each account they are given on
# exactly the state they leave: in a caller's harness their stand-in may write anything and return anything, but on
# Ok it records a passed check of each of those accounts (assume-guarantee).
VERIFIED = {
    "cure_and_cancel_close_with_cert_not_atomic": "proof_v16_cure_and_cancel_close_with_cert_ok_keeps_the_account_check",
    "execute_trade_with_fee_loss_stale_scoped_not_atomic":
        "proof_v16_execute_trade_with_fee_loss_stale_scoped_ok_keeps_the_account_check",
}


def stand_in(name):
    params, ret, body = signature(name)
    assert params[0] in ("&self", "&mut self"), (name, params[0])
    mutating = params[0] == "&mut self"
    if name in VERIFIED:
        assert mutating and ret.startswith("V16Result<"), name
    ps = [f"market: &mut MarketGroupV16ViewMut<'a, T>" if mutating else "_market: &MarketGroupV16ViewMut<'a, T>"]
    havoc = ["havoc_market_view(market);"] if mutating else []
    accounts = []
    for p in params[1:]:
        pn, pt = [x.strip() for x in p.split(":", 1)]
        if mutating and pt == ACCOUNT_MUT:
            ps.append(f"{pn}: {pt}")
            havoc.append(f"havoc_whole_account({pn});")
            accounts.append(pn)
        else:
            assert "&mut" not in pt, f"{name}: a mutable parameter {p} it does not know"
            ps.append(f"_{pn}: {pt}")
    prefix = "acct_havoc" if mutating else "acct_any"
    r = arbitrary_expr(ret)
    ret_sig = "" if ret == "()" else f" -> {ret}"
    body_lines = havoc + ([r] if r else [])
    comment = ""
    if name in VERIFIED:
        assert accounts, name
        prefix = "acct_verified"
        body_lines = (havoc + [f"let result: {ret} = {r};", "if result.is_ok() {"]
                      + [f"    record_passed_account_check({a}, market);" for a in accounts] + ["}", "result"])
        comment = "".join("// " + l + "\n" for l in textwrap.wrap(
            f"The stand-in of `{name}` has the property that `{VERIFIED[name]}` proves of it: any writes and any "
            "result, and every Ok follows a passed check of each account it is given on the state it leaves.", 97))
    text = (comment + f"#[cfg(all(kani, feature = \"closure\"))]\n"
            f"fn {prefix}_{name}<'a, T>(\n" + "".join(f"    {p},\n" for p in ps) + f"){ret_sig}\nwhere\n    'a: 'a,\n{{\n"
            + "".join(f"    {b}\n" for b in body_lines) + "}\n")
    return f"{prefix}_{name}", text


# Pure functions replaced in one harness as well (as in gen_header_final.py): each is an associated function that
# takes its inputs by value or by shared reference and only returns a value, so it writes nothing, and a stand-in that
# returns any result has every behaviour of the real one.
PURE = {
    "force_close_recovery_pair_not_atomic": [("MarketGroupV16ViewMut", "active_leg_for_asset"),
                                             ("V16Core", "effective_abs_quantity_for_leg")],
    "convert_released_pnl_to_capital_not_atomic": [("MarketGroupV16ViewMut", "account_has_source_claims"),
                                                   ("MarketGroupV16ViewMut", "account_has_source_liens")],
}
IMPL_LINE = {"V16Core": "impl V16Core {", "MarketGroupV16ViewMut": "impl<'a, T> MarketGroupV16ViewMut<'a, T> {"}


def pure_stand_in(owner, name):
    hits = [i for i, l in enumerate(lines) if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", l)]
    assert len(hits) == 1, (name, hits)
    k = hits[0]
    while not lines[k].startswith("impl"):
        k -= 1
    assert lines[k] == IMPL_LINE[owner], (name, lines[k])
    head, j = [], hits[0]
    while True:
        head.append(lines[j])
        if lines[j].rstrip().endswith("{"):
            break
        j += 1
    h = re.sub(r"\s+", " ", " ".join(head)).strip()
    m = re.match(r"(?:pub(?:\([a-z]+\))? )?fn " + name + r"(<[^>]*>)?\s*\((.*)\)\s*(?:->\s*(.+?))?\s*\{$", h)
    assert m and not m.group(1), h
    ps = []
    for p in (p.strip() for p in split_params(m.group(2)) if p.strip()):
        assert ":" in p, f"{name}: takes {p}, not an associated function"
        pn, pt = [x.strip() for x in p.split(":", 1)]
        assert "mut" not in pt.split() and "&mut" not in pt and pn.isidentifier() and pn != "self", \
            f"{name}: parameter {p} is not taken by value or by shared reference"
        assert "Cell" not in pt, f"{name}: parameter {p} could be written through a shared reference"
        ps.append(f"_{pn}: {pt}")
    ret = (m.group(3) or "()").strip()
    r = arbitrary_expr(ret)
    assert r, f"{name}: returns nothing"
    comment = "".join("// " + l + "\n" for l in textwrap.wrap(
        f"Stand-in for `{owner}::{name}`, which takes its inputs by value or by shared reference and writes nothing: "
        "any result, so every result of the real function is among its own.", 97))
    generic = owner == "MarketGroupV16ViewMut"
    text = (comment + "#[cfg(all(kani, feature = \"closure\"))]\n"
            f"fn acct_any_pure_{name}{'<' + chr(39) + 'a, T>' if generic else ''}(" + ", ".join(ps) + f") -> {ret}"
            + ("\nwhere\n    'a: 'a,\n" if generic else " ") + f"{{\n    {r}\n}}\n")
    return f"acct_any_pure_{name}", text


# Callees that run as written inside one harness (as in gen_header_final.py): the methods they call are replaced in
# turn, and their bodies are checked as the entry point's is.
INLINE = {
    "execute_trade_with_fee_loss_stale_scoped_not_atomic": ["execute_batch_with_fee_loss_stale_scoped_not_atomic"],
    "convert_released_pnl_to_capital_not_atomic": ["convert_released_pnl_to_capital_core_not_atomic",
                                                   "apply_released_pnl_conversion_core_not_atomic",
                                                   "apply_released_pnl_conversion_with_consumption_core_not_atomic"],
}


def arg_spans(body, opener_re):
    """(start, end) of the argument list of every call matching opener_re (which ends at its open paren)."""
    spans = []
    for m in re.finditer(opener_re, body):
        depth, i = 0, m.end() - 1
        assert body[i] == "("
        for j in range(i, len(body)):
            depth += {"(": 1, ")": -1}.get(body[j], 0)
            if depth == 0:
                spans.append((i, j))
                break
    return spans


def check_uses(f, body, params, pure_names=()):
    """An account parameter is used only through `.header.<field>`, its own `validate_with_market(`, `as_view()`, or
    inside the arguments of a replaced (or inlined) callee or of a pure function replaced in this harness; `self` only
    as `self.header.`, its own `validate_shape()`, a replaced or inlined callee, or `self.as_view()` inside the
    arguments of such a call or of the account's check."""
    body = re.sub(r"//[^\n]*", "", body[body.index("{"):])  # comments are not uses
    stubbed = arg_spans(body, r"\bself\s*\.\s*(?!validate_shape\b|as_view\b|header\b)[a-z_0-9]+\s*\(") + \
        arg_spans(body, r"\.\s*validate_with_market\s*\(")
    if pure_names:
        stubbed += arg_spans(body, r"\b(?:Self|V16Core|MarketGroupV16ViewMut)\s*::\s*(?:" + "|".join(pure_names)
                             + r")\s*\(")
    inside = lambda p: any(a < p < b for a, b in stubbed)
    accounts = [p.split(":")[0].strip() for p in params[1:] if ACCOUNT_MUT in p or ACCOUNT_SHARED in p]
    for acc in accounts:
        for m in re.finditer(r"\b" + acc + r"\b", body):
            rest = body[m.end():]
            ok = re.match(r"\s*\.\s*header\s*\.\s*[a-z_0-9]+", rest) or re.match(r"\s*\.\s*validate_with_market\s*\(", rest) \
                or re.match(r"\s*\.\s*as_view\s*\(\s*\)", rest) or inside(m.start())
            assert ok, f"{f}: `{acc}` used as `{body[m.start():m.start() + 60]!r}`"
    for m in re.finditer(r"(?<![A-Za-z_])self\b", body):
        rest = body[m.end():]
        ok = re.match(r"\s*\.\s*header\s*\.", rest) or re.match(r"\s*\.\s*validate_shape\s*\(\s*\)", rest) \
            or re.match(r"\s*\.\s*(?!validate_shape\b|as_view\b|header\b)[a-z_0-9]+\s*\(", rest) \
            or (re.match(r"\s*\.\s*as_view\s*\(\s*\)", rest) and inside(m.start()))
        assert ok, f"{f}: `self` used as `{body[m.start():m.start() + 60]!r}`"


def entry_body(name):
    sp = fn_span(name, lines, start, end)
    assert sp, name
    return "\n".join(lines[sp[0]:sp[1] + 1])


rows = {r["target"]: r for r in csv.DictReader(open(ledger), delimiter="\t") if re.fullmatch(r"T\d+\.account", r["id"])}

# Not here: T59 (the fee sync). Its kept-certificate path returns Ok after writing the account's capital, last fee slot
# and certificate and the market's c_tot, insurance and loss-stale flag, with no second account check; that rests on
# the check reading none of those fields and on the frames of the two callees on that path, not on the path structure.
TARGETS = [  # entry points given accounts whose Ok paths end in their accounts' checks (T14 and T30 are rule A2's)
    "add_account_source_positive_pnl_not_atomic",
    "charge_account_backing_fee_not_atomic",
    "convert_released_pnl_to_capital_not_atomic",
    "credit_account_from_insurance_not_atomic",
    "cure_and_cancel_close_not_atomic",
    "deposit_not_atomic",
    "execute_trade_with_fee_loss_stale_scoped_not_atomic",
    "force_close_recovery_pair_not_atomic",
    "withdraw_not_atomic",
    "cure_and_cancel_close_with_cert_not_atomic",
]
AUX = {"cure_and_cancel_close_with_cert_not_atomic": "T25.account (its core)"}
only = os.environ.get("ONLY", "").split(",") if os.environ.get("ONLY") else None

stand_ins = {}
hmap = []
harnesses = []
report = []
for f in TARGETS:
    if only and f not in only:
        continue
    row = rows.get(f, {"id": AUX.get(f)})
    assert row["id"], f
    params, ret, _ = signature(f)
    assert params[0] == "&mut self"
    inl = INLINE.get(f, [])
    pure_names = [pf for _, pf in PURE.get(f, [])]
    callees = set()
    for g in [f] + inl:  # the entry point and the callees that run as written: every check applies to each
        body = entry_body(g)
        gparams = signature(g)[0]
        assert gparams[0] == "&mut self", g
        callees |= set(re.findall(r"self\s*\.\s*([a-z_0-9]+)\s*\(", body)) - {"validate_shape", "as_view"}
        check_uses(g, body, gparams, pure_names)
        assert not re.findall(r"self\s*\.\s*markets\b", body), f"{g} reads the slots directly"
        assert "as_view()." not in body.replace("self.as_view())", "").replace(".as_view(),", ""), \
            f"{g} calls a read view method"
    assert set(inl) <= callees, (f, inl)
    if f == "execute_trade_with_fee_loss_stale_scoped_not_atomic":  # the batch it runs as written gets one trade
        assert re.search(r"self\.execute_batch_with_fee_loss_stale_scoped_not_atomic\(\s*long_account,\s*short_account,"
                         r"\s*core::slice::from_ref\(&request\),?\s*\)", entry_body(f)), "the batch's list of trades"
    callees = sorted(callees - set(inl))
    stubs = ["#[kani::stub(PortfolioV16ViewMut::validate_with_market, recorded_account_check)]",
             "#[kani::stub(MarketGroupV16ViewMut::validate_shape, acct_any_shape_check)]"]
    for c in callees:
        if c not in stand_ins:
            stand_ins[c] = stand_in(c)
        stubs.append(f"#[kani::stub(MarketGroupV16ViewMut::{c}, {stand_ins[c][0]})]")
    for owner, pf in PURE.get(f, []):
        key = f"{owner}::{pf}"
        if key not in stand_ins:
            stand_ins[key] = pure_stand_in(owner, pf)
        stubs.append(f"#[kani::stub({key}, {stand_ins[key][0]})]")
    setup, args, accounts = [], [], []
    for p in params[1:]:
        pn, pt = [x.strip() for x in p.split(":", 1)]
        if pt == ACCOUNT_MUT:
            setup.append(f"let mut {pn}_header = arbitrary_whole_account();")
            args.append(f"&mut PortfolioV16ViewMut {{ header: &mut {pn}_header }}")
            accounts.append(pn)
        elif pt in ARBITRARY:
            args.append("kani::any()")
        elif pt == "HealthCertV16":
            args.append("any_health_cert()")
        else:
            raise SystemExit(f"{f}: parameter {p} not handled")
    assert accounts, f
    short = f[: -len("_not_atomic")]
    tid = row["id"]
    befores = "".join(f"    let {a}_before = account_check_state_byte(&{a}_header, &header, &markets[0].engine, probe);\n"
                      for a in accounts)
    if f in VERIFIED:
        asserts = "".join(f"    assert_ok_follows_a_passed_account_check(result.is_ok(), &{a}_header, &market);\n"
                          for a in accounts)
    else:
        asserts = "".join(f"    assert_ok_keeps_the_account_check(result.is_ok(), &{a}_header, probe, {a}_before, &market);\n"
                          for a in accounts)
    h = (f"// {tid}\n#[cfg(all(kani, feature = \"closure\"))]\n#[kani::proof]\n#[kani::solver(cadical)]\n"
         + "\n".join(stubs) + "\n"
         + f"fn proof_v16_{short}_ok_keeps_the_account_check() {{\n"
         + "    let (mut header, mut markets) = arbitrary_header_and_slot();\n"
         + "".join(f"    {s}\n" for s in setup)
         + ("    let _ = choose_account_check_probes();\n" if f in VERIFIED
            else "    let probe = choose_account_check_probes();\n" + befores)
         + "    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);\n"
         + f"    let result = market.{f}({', '.join(args)});\n"
         + asserts
         + "    kani::cover!(result.is_ok(), \"Ok is reached\");\n"
         + "    kani::cover!(result.is_err(), \"an error is reached\");\n"
         + "}\n")
    harnesses.append(h)
    report.append(f"{tid}\t{f}\t{len(callees)} callees\t{','.join(callees)}"
                  + "".join(f"\truns as written: {g}" for g in inl)
                  + "".join(f"\tpure stand-in: {o}::{p}" for o, p in PURE.get(f, [])))
    hmap.append(f"{tid if f not in AUX else 'aux'}\t{f}\tproof_v16_{short}_ok_keeps_the_account_check\t"
                f"{','.join(callees)}\t{','.join(inl)}\t{','.join(o + '::' + p for o, p in PURE.get(f, []))}")

HELPERS = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "account_final_helpers.rs")).read()
text = HELPERS + "\n" + "\n".join(t for _, t in sorted(stand_ins.values())) + "\n" + "\n".join(harnesses)
open(out, "w").write(text)
open(out.replace(".rs", "_map.tsv"), "w").write("\n".join(hmap) + "\n")
print("\n".join(report))
print(f"{len(harnesses)} harnesses, {len(stand_ins)} stand-ins -> {out}")
