#!/usr/bin/env python3
"""gen_header_final.py <engine worktree> <out.rs> <ledger tsv>
Generates the Kani harnesses of the T*.header pieces of class "final" and their delegates: for each entry point,
every method it calls on the market view is replaced by a stand-in (a read-only callee returns any value; a callee
that takes the view mutably may first write any bytes to the header, the asset slots and the view's scratch, and any
values to the account fields the entry points read), its own `validate_shape()` calls by a recorder (either outcome,
the header it was asked about remembered), and the harness asserts that an Ok return leaves the header bytes unchanged
or follows a passed check on exactly the header it leaves; a callee with no early Ok that ends in its own check is
proved the stronger way (every Ok follows a passed check) and stands in for itself with that property in its callers.
Pure functions run as written, except those named in PURE for one harness: each takes its inputs by value and writes
nothing, and its stand-in returns any result. Checked here: the entry point reads no slot, and uses an account or `self` only through the listed fields, its own
checks and the replaced callees. Writes the whole section that follows the T28
harness in src/v16_proofs.rs (shared helpers, stand-ins, harnesses) to <out.rs>; review it, then append it.

Fails on anything it does not know: a callee outside the mutable view's impl block, a method with generics of its
own, a parameter type it cannot build, a return type it cannot make arbitrary."""
import csv, os, re, sys, textwrap

wt, out, ledger = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(f"{wt}/src/v16.rs").read()
lines = src.split("\n")

# the mutable view's main impl block
start = lines.index("impl<'a, T> MarketGroupV16ViewMut<'a, T> {", 7300) + 1
end = start
while lines[end] != "}":
    end += 1
impl_text = "\n".join(lines[start - 1:end + 1])


def fn_span(name, text_lines, lo, hi):
    """(first line, last line) of `fn name` at method indentation in text_lines[lo:hi]."""
    for i in range(lo, hi):
        if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", text_lines[i]):
            j = i
            while text_lines[j] != "    }":
                j += 1
            return i, j
    return None


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


ARBITRARY = {"()", "u8", "u16", "u32", "u64", "u128", "i128", "usize", "bool", "SideV16", "AssetStateV16",
             "SourceCreditStateV16", "BackingBucketV16", "InsuranceCreditReservationV16", "PermissionlessRecoveryReasonV16",
             "AssetLifecycleV16", "SideModeV16", "BackingBucketStatusV16"}
CUSTOM = {"HealthCertV16": "any_health_cert()",
          "ReservationEncumbranceProofV16": "any_reservation_encumbrance_proof()",
          "PermissionlessProgressOutcomeV16": "any_progress_outcome()"}


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


# Callees whose own harness below proves the property: in a caller's harness their stand-in may write anything and
# return anything, but an Ok leaves the header unchanged or passing the predicate (assume-guarantee).
VERIFIED = {
    "cure_and_cancel_close_with_cert_not_atomic": "proof_v16_cure_and_cancel_close_with_cert_ok_keeps_the_header_predicate",
    "withdraw_fresh_counterparty_backing_not_atomic": "proof_v16_withdraw_fresh_counterparty_backing_ok_keeps_the_header_predicate",
}


def stand_in(name):
    params, ret, body = signature(name)
    assert params[0] in ("&self", "&mut self"), (name, params[0])
    mutating = params[0] == "&mut self"
    if name in VERIFIED:
        assert mutating and ret.startswith("V16Result<"), name
    ps = [f"market: &{'mut ' if mutating else ''}MarketGroupV16ViewMut<'a, T>" if mutating else
          "_market: &MarketGroupV16ViewMut<'a, T>"]
    havoc = ["havoc_market_view(market);"] if mutating else []
    for p in params[1:]:
        pn, pt = [x.strip() for x in p.split(":", 1)]
        if mutating and re.fullmatch(r"&mut PortfolioV16ViewMut<'_>", pt):
            ps.append(f"{pn}: {pt}")
            havoc.append(f"havoc_account({pn});")
        else:
            assert "&mut" not in pt, f"{name}: a mutable parameter {p} it does not know"
            ps.append(f"_{pn}: {pt}")
    prefix = "havoc" if mutating else "any"
    r = arbitrary_expr(ret)
    ret_sig = "" if ret == "()" else f" -> {ret}"
    body_lines = havoc + ([r] if r else [])
    if name in VERIFIED:
        prefix = "verified"
        body_lines = (havoc + [f"let result: {ret} = {r};",
                       "if result.is_ok() {",
                       "    record_passed_shape_check(market);",
                       "}", "result"])
        comment = "".join("// " + l + "\n" for l in textwrap.wrap(
            f"The stand-in of `{name}` has the property that `{VERIFIED[name]}` proves of it: any writes and any "
            "result, and every Ok follows a passed shape check on the header it leaves.", 97))
    else:
        comment = ""
    text = (comment + f"#[cfg(all(kani, feature = \"closure\"))]\n"
            f"fn {prefix}_{name}<'a, T>(\n" + "".join(f"    {p},\n" for p in ps) + f"){ret_sig}\nwhere\n    'a: 'a,\n{{\n"
            + "".join(f"    {b}\n" for b in body_lines) + "}\n")
    return f"{prefix}_{name}", text


# Pure functions replaced in one harness as well. Each is an associated function that takes its inputs by value and
# only returns a value, so it writes nothing, and a stand-in that returns any result has every behaviour of the real
# one. Used where the real one is too slow for the verifier on arbitrary inputs: the credit rate is a 256-bit
# multiply-divide, which the fresh-backing withdrawal runs twice (its recompute and its encumbrance check).
PURE = {"withdraw_fresh_counterparty_backing_not_atomic": [("V16Core", "expected_source_credit_rate_num_for_state")]}


def pure_stand_in(owner, name):
    hits = [i for i, l in enumerate(lines) if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", l)]
    assert len(hits) == 1, (name, hits)
    k = hits[0]
    while not lines[k].startswith("impl"):
        k -= 1
    assert lines[k] == f"impl {owner} {{", (name, lines[k])
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
        assert "&" not in pt and pn.isidentifier() and pn != "self", f"{name}: parameter {p} is not taken by value"
        ps.append(f"_{pn}: {pt}")
    ret = (m.group(3) or "()").strip()
    r = arbitrary_expr(ret)
    assert r, f"{name}: returns nothing"
    comment = "".join("// " + l + "\n" for l in textwrap.wrap(
        f"Stand-in for `{owner}::{name}`, which takes its inputs by value and writes nothing: any result, so every "
        "result of the real function is among its own.", 97))
    text = (comment + "#[cfg(all(kani, feature = \"closure\"))]\n"
            f"fn any_pure_{name}(" + ", ".join(ps) + f") -> {ret} {{\n    {r}\n}}\n")
    return f"any_pure_{name}", text


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


def check_uses(f, body, params):
    """An account parameter is used only as `.header.<field>` with the field in READ_FIELDS, through its own
    `validate_with_market(`, or inside the arguments of a replaced callee; `self` only as `self.header.`, its own
    `validate_shape()`, a replaced callee, or `self.as_view()` inside the arguments of a replaced call."""
    body = re.sub(r"//[^\n]*", "", body[body.index("{"):])  # comments are not uses
    stubbed = arg_spans(body, r"\bself\s*\.\s*(?!validate_shape\b|as_view\b|header\b)[a-z_0-9]+\s*\(") + \
        arg_spans(body, r"\.\s*validate_with_market\s*\(")
    inside = lambda p: any(a < p < b for a, b in stubbed)
    accounts = [p.split(":")[0].strip() for p in params[1:] if "PortfolioV16ViewMut" in p]
    for acc in accounts:
        for m in re.finditer(r"\b" + acc + r"\b", body):
            rest = body[m.end():]
            fm = re.match(r"\s*\.\s*header\s*\.\s*([a-z_0-9]+)", rest)
            ok = (fm and fm.group(1) in READ_FIELDS) or re.match(r"\s*\.\s*validate_with_market\s*\(", rest) \
                or inside(m.start())
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


# the account fields `arbitrary_read_fields` (header_final_helpers.rs) makes arbitrary
READ_FIELDS = {"capital", "pnl", "fee_credits", "cancel_deposit_escrow", "last_fee_slot", "active_bitmap",
               "health_cert", "close_progress"}
_helpers = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "header_final_helpers.rs")).read()
assert all(f"account.{x} =" in _helpers or f"account.{x})" in _helpers for x in READ_FIELDS), "helper and list differ"

rows = {r["target"]: r for r in csv.DictReader(open(ledger), delimiter="\t") if r["id"].endswith(".header")}

TARGETS = [  # (entry point, cover labels): straight-line entry points of class final
    "add_account_source_positive_pnl_not_atomic",
    "canonicalize_spent_backing_history_not_atomic",
    "charge_account_backing_fee_not_atomic",
    "claim_resolved_payout_topup_not_atomic",
    # not here: convert_released_pnl_to_capital_not_atomic (T18) returns Ok without a check when nothing was
    # converted, after its core ran; that needs the core's frame on a zero conversion, a property of its own.
    "credit_account_from_insurance_not_atomic",
    "credit_backing_provider_earnings_not_atomic",
    "cure_and_cancel_close_not_atomic",
    "deposit_domain_insurance_not_atomic",
    "deposit_fresh_counterparty_backing_not_atomic",
    "deposit_not_atomic",
    "force_asset_recovery_not_atomic",
    "mark_asset_drain_only_not_atomic",
    "recredit_terminal_claim_free_residual_for_asset_not_atomic",
    "reset_empty_asset_oracle_anchor_not_atomic",
    "resolve_market_not_atomic",
    "retire_empty_asset_not_atomic",
    "retire_terminal_unbudgeted_insurance_not_atomic",
    "sync_account_fee_to_slot_not_atomic",
    "withdraw_backing_provider_earnings_not_atomic",
    "withdraw_domain_insurance_not_atomic",
    "withdraw_fresh_counterparty_backing_not_atomic",
    "withdraw_not_atomic",
    "withdraw_resolved_counterparty_backing_surplus_not_atomic",
    "cure_and_cancel_close_with_cert_not_atomic",
]
AUX = {"cure_and_cancel_close_with_cert_not_atomic": "T25.header (its core)"}

stand_ins = {}
hmap = []
harnesses = []
report = []
for f in TARGETS:
    row = rows.get(f, {"id": AUX.get(f), "statement": "[aux]"})
    assert row["id"], f
    assert row["statement"].startswith(("[final]", "[delegates]", "[aux]")), (f, row["statement"][:20])
    body = entry_body(f)
    params, ret, _ = signature(f)
    assert params[0] == "&mut self"
    callees = sorted(set(re.findall(r"self\s*\.\s*([a-z_0-9]+)\s*\(", body)) - {"validate_shape", "as_view"})
    read = set(re.findall(r"\b\w*account\w*\s*\.\s*header\s*\.\s*([a-z_0-9]+)", body))
    assert read <= READ_FIELDS, f"{f} reads account fields arbitrary_read_fields does not set: {read - READ_FIELDS}"
    check_uses(f, body, params)
    direct_slot = re.findall(r"self\s*\.\s*markets\b", body)
    assert not direct_slot, f"{f} reads the slots directly"
    assert "as_view()." not in body.replace("self.as_view())", ""), f"{f} calls a read view method"
    stubs = ["#[kani::stub(MarketGroupV16ViewMut::validate_shape, recorded_shape_check)]"]
    if re.search(r"\baccount\w*\s*\.\s*validate_with_market\s*\(", body):
        stubs.append("#[kani::stub(\n    PortfolioV16ViewMut::validate_with_market,\n    stand_in_account_check_any_outcome\n)]")
    for c in callees:
        if c not in stand_ins:
            stand_ins[c] = stand_in(c)
        stubs.append(f"#[kani::stub(MarketGroupV16ViewMut::{c}, {stand_ins[c][0]})]")
    for owner, pf in PURE.get(f, []):
        key = f"{owner}::{pf}"
        if key not in stand_ins:
            stand_ins[key] = pure_stand_in(owner, pf)
        stubs.append(f"#[kani::stub({key}, {stand_ins[key][0]})]")
    setup, args = [], []
    for p in params[1:]:
        pn, pt = [x.strip() for x in p.split(":", 1)]
        if pt == "&mut PortfolioV16ViewMut<'_>":
            setup.append(f"let mut {pn}_header = arbitrary_account();")
            args.append(f"&mut PortfolioV16ViewMut::new(&mut {pn}_header)")
        elif pt in ("usize", "u64", "u128", "i128", "bool"):
            args.append("kani::any()")
        elif pt == "HealthCertV16":
            args.append("any_health_cert()")
        else:
            raise SystemExit(f"{f}: parameter {p} not handled")
    short = f[: -len("_not_atomic")]
    tid = row["id"]
    h = (f"// {tid}\n#[cfg(all(kani, feature = \"closure\"))]\n#[kani::proof]\n#[kani::solver(cadical)]\n"
         + "\n".join(stubs) + "\n"
         + f"fn proof_v16_{short}_ok_keeps_the_header_predicate() {{\n"
         + "    let (mut header, mut markets) = arbitrary_header_and_slot();\n"
         + ("" if f in VERIFIED else "    let before = header;\n")
         + "".join(f"    {s}\n" for s in setup)
         + "    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);\n"
         + f"    let result = market.{f}({', '.join(args)});\n"
         + ("    assert_ok_follows_a_passed_check(result.is_ok(), &market);\n" if f in VERIFIED else
            "    assert_ok_keeps_the_header_predicate(result.is_ok(), &before, &market);\n")
         + "    kani::cover!(result.is_ok(), \"Ok is reached\");\n"
         + "    kani::cover!(result.is_err(), \"an error is reached\");\n"
         + "}\n")
    harnesses.append(h)
    report.append(f"{tid}\t{f}\t{len(callees)} callees\t{','.join(callees)}"
                  + "".join(f"\tpure stand-in: {o}::{p}" for o, p in PURE.get(f, [])))
    if f not in AUX:
        hmap.append(f"{tid}\t{f}\tproof_v16_{short}_ok_keeps_the_header_predicate\t{','.join(callees)}")
    else:
        hmap.append(f"aux\t{f}\tproof_v16_{short}_ok_keeps_the_header_predicate\t{','.join(callees)}")

HELPERS = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "header_final_helpers.rs")).read()
text = HELPERS + "\n" + "\n".join(t for _, t in sorted(stand_ins.values())) + "\n" + "\n".join(harnesses)
open(out, "w").write(text)
open(out.replace(".rs", "_map.tsv"), "w").write("\n".join(hmap) + "\n")
print("\n".join(report))
print(f"{len(harnesses)} harnesses, {len(stand_ins)} stand-ins -> {out}")
