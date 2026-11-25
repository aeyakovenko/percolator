#!/usr/bin/env python3
"""gen_po.py <engine worktree> <out.rs> <ledger tsv> [<census tsv>]
Generates one Kani harness per open PO piece whose function is a method of the mutable market view: the function runs as
written on an arbitrary header, one arbitrary slot and wholly arbitrary accounts, with arbitrary arguments, and every
method it calls on the view replaced by a stand-in (one that takes the view by shared reference returns any value; one
that takes it mutably may first write any bytes to the header, the slots, the view's scratch and every account it is
given); the view's scratch starts arbitrary too. The harness asserts nothing itself: Kani checks every `+`, `-` and `*` it runs for overflow. The piece holds if
the overflow check of every operation the census lists for the function is reached and passes (po_kani_results.py reads
that from Kani's per-check report). Pure functions run as written. Writes the harnesses and their stand-ins to <out.rs>
and a map (piece, function, harness, listed lines) next to it; a function with a parameter it cannot build is reported
and skipped."""
import csv, os, re, sys, textwrap

wt, out, ledger = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(f"{wt}/src/v16.rs").read()
lines = src.split("\n")
start = lines.index("impl<'a, T> MarketGroupV16ViewMut<'a, T> {", 7300) + 1
end = start
while lines[end] != "}":
    end += 1


def fn_span(name):
    for i in range(start, end):
        if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", lines[i]):
            j = i
            while lines[j] != "    }":
                j += 1
            return i, j
    return None


def split_params(s):
    out_, depth, cur = [], 0, ""
    for ch in s:
        if ch in "<([":
            depth += 1
        elif ch in ">)]":
            depth -= 1
        if ch == "," and depth == 0:
            out_.append(cur)
            cur = ""
        else:
            cur += ch
    out_.append(cur)
    return [p.strip() for p in out_ if p.strip()]


def signature(name):
    sp = fn_span(name)
    if not sp:
        return None
    i, j = sp
    head, k = [], i
    while True:
        head.append(lines[k])
        if lines[k].rstrip().endswith("{"):
            break
        k += 1
    h = re.sub(r"\s+", " ", " ".join(head)).strip()
    m = re.match(r"(?:pub(?:\([a-z]+\))? )?fn " + name + r"(<[^>]*>)?\s*\((.*)\)\s*(?:->\s*(.+?))?\s*\{$", h)
    assert m, h
    return m.group(1), split_params(m.group(2)), (m.group(3) or "()").strip(), "\n".join(lines[i:j + 1])


ARBITRARY = {"()", "u8", "u16", "u32", "u64", "u128", "i128", "usize", "bool", "SideV16", "AssetStateV16",
             "SourceCreditStateV16", "BackingBucketV16", "InsuranceCreditReservationV16", "PermissionlessRecoveryReasonV16",
             "AssetLifecycleV16", "SideModeV16", "BackingBucketStatusV16", "PortfolioLegV16",
             "TradeRequestV16", "TradeOutcomeV16", "BatchTradeOutcomeV16", "SourceCreditConsumptionV16"}
CUSTOM = {"HealthCertV16": "any_health_cert()", "ReservationEncumbranceProofV16": "any_reservation_encumbrance_proof()",
          "PermissionlessProgressOutcomeV16": "any_progress_outcome()", "V16Config": "any_v16_config_for_overflow()"}
STRUCTS = ["ParNettingBatchV16", "KfLossDomainCreditV16", "KfLossLienPaymentV16", "TradeApplyOutcomeV16",
           "AccountBSettlementChunkV16"]
FIELD_OK = re.compile(r"(?:u8|u16|u32|u64|u128|i64|i128|usize|bool|Option<bool>|Option<usize>|Option<u64>)")


def struct_arbitrary(name):
    m = re.search(r"\n(?:pub(?:\([a-z]+\))? )?struct " + name + r" \{(.*?)\n\}", src, re.S)
    assert m, name
    body = re.sub(r"//[^\n]*", "", m.group(1))
    fields = [f.strip() for f in body.split(",") if f.strip()]
    names = []
    for f in fields:
        fm = re.fullmatch(r"(?:pub(?:\([a-z]+\))? )?([a-z_][a-z_0-9]*)\s*:\s*(.+)", f)
        assert fm and FIELD_OK.fullmatch(fm.group(2).strip()), (name, f)
        names.append(fm.group(1))
    return (f"#[cfg(all(kani, feature = \"closure\"))]\nimpl kani::Arbitrary for {name} {{\n    fn any() -> Self {{\n"
            f"        {name} {{\n" + "".join(f"            {n}: kani::any(),\n" for n in names) + "        }\n    }\n}\n")


ARBITRARY |= set(STRUCTS)
ACCOUNT_MUT = "&mut PortfolioV16ViewMut<'_>"
ACCOUNT_SHARED = "&PortfolioV16View<'_>"


class Unknown(Exception):
    pass


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
            check_arbitrary(t)
        return
    m = re.fullmatch(r"Option<(.+)>", ty)
    if m:
        return check_arbitrary(m.group(1))
    m = re.fullmatch(r"\[(.+);\s*([A-Z_0-9]+)\]", ty)
    if m:
        return check_arbitrary(m.group(1))
    if ty not in ARBITRARY:
        raise Unknown(f"cannot make {ty} arbitrary")


def stand_in(name):
    sig = signature(name)
    if not sig:
        raise Unknown(f"callee {name} is not a method of the view")
    gen, params, ret, _ = sig
    if gen:
        raise Unknown(f"callee {name} has generics of its own")
    mutating = params[0] == "&mut self"
    ps = ["market: &mut MarketGroupV16ViewMut<'a, T>" if mutating else "_market: &MarketGroupV16ViewMut<'a, T>"]
    havoc = ["havoc_market_view(market);"] if mutating else []
    for p in params[1:]:
        pn, pt = [x.strip() for x in p.split(":", 1)]
        pn = re.sub(r"^mut\s+", "", pn)
        if pt == ACCOUNT_MUT:  # a callee that takes the view by shared reference can still write an account
            ps.append(f"{pn}: {pt}")
            havoc.append(f"havoc_whole_account({pn});")
        elif re.fullmatch(r"&mut ([A-Za-z0-9_]+)", pt):
            check_arbitrary(re.fullmatch(r"&mut ([A-Za-z0-9_]+)", pt).group(1))
            ps.append(f"{pn}: {pt}")
            havoc.append(f"*{pn} = kani::any();")
        elif re.fullmatch(r"Option<&mut ([A-Za-z0-9_]+)>", pt):
            check_arbitrary(re.fullmatch(r"Option<&mut ([A-Za-z0-9_]+)>", pt).group(1))
            ps.append(f"{pn}: {pt}")
            havoc.append(f"if let Some(v) = {pn} {{\n        *v = kani::any();\n    }}")
        elif "&mut" in pt:
            raise Unknown(f"callee {name}: a mutable parameter {p}")
        else:
            ps.append(f"_{pn}: {pt}")
    r = arbitrary_expr(ret)
    body = havoc + ([r] if r else [])
    prefix = "po_havoc" if mutating else "po_any"
    text = (f"#[cfg(all(kani, feature = \"closure\"))]\nfn {prefix}_{name}<'a, T>(\n" + "".join(f"    {p},\n" for p in ps)
            + (f") -> {ret}\n" if ret != "()" else ")\n") + "where\n    'a: 'a,\n{\n" + "".join(f"    {b}\n" for b in body)
            + "}\n")
    return f"{prefix}_{name}", text


rows = {r["id"][3:]: r for r in csv.DictReader(open(ledger), delimiter="\t")
        if r["id"].startswith("PO.MarketGroupV16ViewMut::") and r["status"] in ("open", "attempted, open")}
PURE_HELPERS = {  # function -> [(owner, helper)]: pure helpers whose call graph differs between builds, replaced
    "accrue_asset_to_admitted_not_atomic": [("V16Core", "kernel_accrual_index_update")],
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
    for p in split_params(m.group(2)):
        pn, pt = [x.strip() for x in p.split(":", 1)]
        assert "&mut" not in pt and pn != "self" and pn.isidentifier(), f"{name}: parameter {p} is not taken by value"
        ps.append(f"_{pn}: {pt}")
    ret = (m.group(3) or "()").strip()
    r = arbitrary_expr(ret)
    generic = owner == "MarketGroupV16ViewMut"
    text = (f"// Stand-in for `{owner}::{name}` (inputs by value or by shared reference, writes nothing): any result.\n"
            "#[cfg(all(kani, feature = \"closure\"))]\n"
            f"fn po_any_pure_{name}{'<' + chr(39) + 'a, T>' if generic else ''}(" + ", ".join(ps) + f") -> {ret}"
            + ("\nwhere\n    'a: 'a,\n" if generic else " ") + f"{{\n    {r}\n}}\n")
    return f"po_any_pure_{name}", text


stand_ins, harnesses, hmap, report = {}, [], [], []
only = set(os.environ["ONLY"].split(",")) if os.environ.get("ONLY") else None  # the functions to keep, when given
for qual, row in sorted(rows.items()):
    fn = qual.split("::")[1]
    if only is not None and fn not in only:
        continue
    m = re.search(r"\((\d+) at lines? ([\d, ]+)\)", row["statement"])
    listed = [int(x) for x in re.findall(r"\d+", m.group(2))]
    try:
        sig = signature(fn)
        if not sig:
            raise Unknown("not in the view's main impl block")
        gen, params, ret, body = sig
        if gen:
            raise Unknown(f"generics {gen}")
        callees = sorted(set(re.findall(r"self\s*\.\s*([a-z_0-9]+)\s*\(", body)) - {"validate_shape", "as_view"} - {fn})
        stubs = []
        for c in callees:
            if c not in stand_ins:
                stand_ins[c] = stand_in(c)
            stubs.append(f"#[kani::stub(MarketGroupV16ViewMut::{c}, {stand_ins[c][0]})]")
        if re.search(r"\.\s*validate_with_market\s*\(", body):
            stubs.append("#[kani::stub(PortfolioV16ViewMut::validate_with_market, stand_in_account_check_any_outcome)]")
            stubs.append("#[kani::stub(PortfolioV16View::validate_with_market, stand_in_view_account_check_any_outcome)]")
        for owner, helper in PURE_HELPERS.get(fn, []):
            key = f"{owner}::{helper}"
            if key not in stand_ins:
                stand_ins[key] = pure_stand_in(owner, helper)
            stubs.append(f"#[kani::stub({key}, {stand_ins[key][0]})]")
        if re.search(r"self\s*\.\s*validate_shape\s*\(", body):
            stubs.append("#[kani::stub(MarketGroupV16ViewMut::validate_shape, acct_any_shape_check)]")
        setup, args = [], []
        for p in params[1:]:
            pn, pt = [x.strip() for x in p.split(":", 1)]
            pn = re.sub(r"^mut\s+", "", pn)
            if pt == ACCOUNT_MUT:
                setup.append(f"let mut {pn}_header = arbitrary_whole_account();")
                args.append(f"&mut PortfolioV16ViewMut {{ header: &mut {pn}_header }}")
            elif pt == ACCOUNT_SHARED:
                setup.append(f"let {pn}_header = arbitrary_whole_account();")
                args.append(f"&PortfolioV16View {{ header: &{pn}_header }}")
            elif pt == "&PortfolioV16ViewMut<'_>":
                setup.append(f"let mut {pn}_header = arbitrary_whole_account();")
                args.append(f"&PortfolioV16ViewMut {{ header: &mut {pn}_header }}")
            elif re.fullmatch(r"&mut ([A-Za-z0-9_]+)", pt):
                inner = re.fullmatch(r"&mut ([A-Za-z0-9_]+)", pt).group(1)
                check_arbitrary(inner)
                setup.append(f"let mut {pn}_value: {inner} = kani::any();")
                args.append(f"&mut {pn}_value")
            elif re.fullmatch(r"Option<&mut ([A-Za-z0-9_]+)>", pt):
                inner = re.fullmatch(r"Option<&mut ([A-Za-z0-9_]+)>", pt).group(1)
                check_arbitrary(inner)
                setup.append(f"let mut {pn}_value: {inner} = kani::any();")
                args.append(f"if kani::any() {{ Some(&mut {pn}_value) }} else {{ None }}")
            else:
                args.append(arbitrary_expr(pt) or "()")
        self_kind = params[0]
        h = (f"// {row['id']}\n#[cfg(all(kani, feature = \"closure\"))]\n#[kani::proof]\n#[kani::solver(cadical)]\n"
             + "".join(f"{s}\n" for s in stubs)
             + f"fn proof_po_{fn}() {{\n"
             + "    let (mut header, mut markets) = arbitrary_header_and_slot();\n"
             + "".join(f"    {s}\n" for s in setup)
             + "    let mut market = MarketGroupV16ViewMut::new(&mut header, &mut markets);\n"
             + "    market.protection_observed = kani::any(); // the view's scratch, as a view in use may hold it\n"
             + f"    let _ = market.{fn}({', '.join(args)});\n}}\n")
        harnesses.append(h)
        hmap.append(f"{row['id']}\t{fn}\tproof_po_{fn}\t{','.join(map(str, listed))}")
        report.append(f"{row['id']}\t{len(callees)} callees\tlines {listed}")
    except Unknown as e:
        report.append(f"{row['id']}\tSKIPPED: {e}")
text = ("// Overflow pieces (PO.*) of methods of the mutable market view: each function runs as written on arbitrary\n"
        "// inputs with its callees replaced, and Kani checks its arithmetic (see proofs/gen_po.py).\n\n"
        + "\n".join(struct_arbitrary(t) for t in STRUCTS) + "\n"
        + "\n".join(t for _, t in sorted(stand_ins.values())) + "\n" + "\n".join(harnesses))
open(out, "w").write(text)
open(out.replace(".rs", "_map.tsv"), "w").write("\n".join(hmap) + "\n")
print("\n".join(report))
print(f"{len(harnesses)} harnesses, {len(stand_ins)} stand-ins -> {out}")
