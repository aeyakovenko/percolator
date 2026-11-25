#!/usr/bin/env python3
"""account_rules.py <src/v16.rs> <fn> [<fn> ...]

Decides, for an engine entry point, whether its T*.account piece ("every portfolio account the entry point is given
passes `validate_with_market` against the market it leaves, if it did before and the call returns Ok") holds by a
strict local rule, or leaves it unclassified.

A0, no account is given. The function is a method of `impl<'a, T> MarketGroupV16ViewMut<'a, T>` or of
`impl MarketGroupV16HeaderAccount`, its receiver is `&mut self` or `&self`, and every other parameter has one of the
types in PLAIN: numbers, `bool`, `SideV16`, slices of number pairs or of `AccrualStepV16` (a struct of numbers,
checked), an asset slot by mutable reference (`&mut EngineAssetSlotV16Account`, or `&mut S` with the method's own
generic `S: MarketSlotV16ViewMut`). None of them is or holds a portfolio account, and the receiver is the market view
or the market header, so the statement quantifies over no account: it holds. What such an entry point can do to the
accounts it is not given is the frame piece G.other, which this rule does not touch.

Fails closed: a parameter type outside PLAIN, a generic other than that one, a function defined more than once or
outside those impl blocks is left unclassified.

A1 and A2, the checks come last. The function is `fn f(&mut self, ..) -> V16Result<..>` in
`impl<'a, T> MarketGroupV16ViewMut<'a, T>`, it is given at least one account, and, in its body with comments,
strings and characters removed (header_rules.py's reading, with its conditions on macros, attributes, items, `unsafe`,
block comments and raw strings):
  A1: the final expression is `self.validate_shape()`, the top-level statements before it end with a run of
      `ACCOUNT.validate_with_market(&self.as_view())?;` that names every account parameter, the token `Ok` does not
      occur and every `return` is `return Err(`;
  A2: the top-level statements end with a run of `ACCOUNT.validate_with_market(&self.as_view())?;` and
      `self.validate_shape()?;` that names every account parameter, the final expression is `Ok` of a local, the token
      `Ok` occurs once (there) and every `return` is `return Err(`.
A function returns a value only by a `return` expression, a `?` (an error) or its final expression, so every Ok it
returns comes after that run, and nothing in the run writes: `validate_with_market` takes the account and the market
by shared reference, `as_view` and `validate_shape` take `&self` (header_rules.crate_preconditions checks these, and
that the crate has no Drop impl, no interior mutability and no unsafe code but marker impls). So on every Ok each
account it is given passed its check on exactly the account and the market the function leaves.

Output: one TSV row per function: function, rule (A0, A1, A2 or -), reason. `selftest()` runs the must-accept and must-reject
cases (mutations of real signatures) and is called by make_ledger.py before it classifies anything.
"""
import os, re, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import header_rules as hr

IMPLS = ("impl<'a, T> MarketGroupV16ViewMut<'a, T> {", "impl MarketGroupV16HeaderAccount {")
PLAIN = {"usize", "u8", "u16", "u32", "u64", "u128", "i64", "i128", "bool", "SideV16",
         "&[AccrualStepV16]", "&[(usize, u128)]", "&[(usize, u64)]", "&mut EngineAssetSlotV16Account"}
SLOT_GENERIC = "<S: MarketSlotV16ViewMut>"


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
    return [p.strip() for p in out if p.strip()]


def signature(lines, name):
    """(impl line, normalized signature) of the one `fn name` at method indentation, or (None, reason)."""
    hits = [i for i, l in enumerate(lines) if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", l)]
    if len(hits) != 1:
        return None, f"{len(hits)} definitions"
    i = hits[0]
    k = i
    while not lines[k].startswith("impl"):
        k -= 1
    j = i
    while not lines[j].rstrip().endswith("{"):
        j += 1
    sig = re.sub(r"\s+", " ", " ".join(l.strip() for l in lines[i:j + 1]))
    sig = re.sub(r"\( ", "(", re.sub(r",? \)", ")", sig))
    return lines[k], sig


def classify_checks_last(lines, name):
    """A1, A2 or '-' with a reason (see the docstring)."""
    got, why = hr.method(lines, name)
    if got is None:
        return "-", why
    sig, raw = got
    if not re.search(r"\(\s*&mut self\b", sig) or not re.search(r"->\s*V16Result<", sig):
        return "-", "not `fn (&mut self, ..) -> V16Result<..>`"
    accounts = re.findall(r"\b([a-z_][a-z_0-9]*)\s*:\s*&(?:mut\s+)?PortfolioV16View(?:Mut)?<'_>", sig)
    if not accounts or "Portfolio" in re.sub(r"\b[a-z_][a-z_0-9]*\s*:\s*&(?:mut\s+)?PortfolioV16View(?:Mut)?<'_>", "", sig):
        return "-", "no account, or an account in another form"
    body = hr.strip(raw)
    if body is None:
        return "-", "a block comment or a raw string"
    stmts, tail = hr.top_level(body)
    if stmts is None:
        return "-", "unbalanced brackets"
    k, named = len(stmts), set()
    while k > 0:
        st = hr.squash(stmts[k - 1])
        m = re.fullmatch(r"([a-z_][a-z_0-9]*)\.validate_with_market\(&self\.as_view\(\)\)\?;", st)
        if m and m.group(1) in accounts:
            named.add(m.group(1))
        elif st != "self.validate_shape()?;":
            break
        k -= 1
    if named != set(accounts):
        return "-", f"the final run of checks names {sorted(named)}, not every account {sorted(accounts)}"
    run = [hr.squash(x) for x in stmts[k:]]
    if hr.squash(tail) == "self.validate_shape()":
        if any(x == "self.validate_shape()?;" for x in run):
            return "-", "A1 wants account checks only before the final `self.validate_shape()`"
        why = hr.tokens_ok(body, 0)
        return ("A1", f"{len(run)} account check(s), then the final expression `self.validate_shape()`") if not why \
            else ("-", why)
    if re.fullmatch(r"Ok\(\s*[a-z_][a-z_0-9]*\s*\)", tail):
        why = hr.tokens_ok(body, 1)
        return ("A2", f"the run `{' '.join(run)}`, then `{tail}`") if not why else ("-", why)
    return "-", "final expression `" + re.sub(r"\s+", " ", tail)[:60] + "`"


def classify_one(lines, name):
    a0 = classify_no_account(lines, name)
    if a0[0] == "A0":
        return a0
    a12 = classify_checks_last(lines, name)
    return a12 if a12[0] != "-" or "Portfolio" in a0[1] else a0


def classify_no_account(lines, name):
    impl, sig = signature(lines, name)
    if impl is None:
        return "-", sig
    if impl not in IMPLS:
        return "-", f"not in {' or '.join(f'`{x}`' for x in IMPLS)}"
    m = re.match(r"(?:pub(?:\([a-z]+\))? )?fn " + name + r"(<[^()]*>)?\((.*)\)\s*(?:->.*)?\{$", sig)
    if not m:
        return "-", f"signature not understood: {sig[:80]}"
    generics, params = m.group(1), split_params(m.group(2))
    if generics and generics != SLOT_GENERIC:
        return "-", f"generics `{generics}`"
    if not params or params[0] not in ("&mut self", "&self"):
        return "-", "no `&mut self` or `&self` receiver"
    kinds = []
    for p in params[1:]:
        if ":" not in p:
            return "-", f"parameter `{p}`"
        pn, pt = [x.strip() for x in p.split(":", 1)]
        if not re.fullmatch(r"[a-z_][a-z_0-9]*", pn):
            return "-", f"parameter pattern `{pn}`"
        if pt in PLAIN or (generics == SLOT_GENERIC and pt == "&mut S"):
            kinds.append(pt)
            continue
        return "-", f"parameter `{pn}: {pt}` is not of a type known to hold no portfolio account"
    return "A0", "its parameters besides the receiver: " + (", ".join(f"`{k}`" for k in kinds) if kinds else "none")


def crate_preconditions(src_path):
    """The types in PLAIN that are structs hold numbers only; header_rules.crate_preconditions (no Drop impl, no
    interior mutability, no unsafe code but marker impls, the signatures of `validate_with_market`, `as_view` and the
    view's one `validate_shape`)."""
    hr.crate_preconditions(src_path)
    v16 = open(src_path).read()
    m = re.search(r"\npub struct AccrualStepV16 \{([^}]*)\}", v16)
    assert m, "AccrualStepV16"
    fields = [f.strip() for f in m.group(1).split(",") if f.strip()]
    for f in fields:
        assert re.fullmatch(r"pub [a-z_][a-z_0-9]*: (?:u8|u16|u32|u64|u128|i64|i128|bool)", f), f"AccrualStepV16: {f}"
    assert re.search(r"\npub trait MarketSlotV16ViewMut: MarketSlotV16View \{\s*fn engine_slot_mut\(&mut self\) -> "
                     r"&mut EngineAssetSlotV16Account;\s*\}", v16), "MarketSlotV16ViewMut"
    m = re.search(r"\npub struct EngineAssetSlotV16Account \{(.*?)\n\}", v16, re.S)
    assert m and "Portfolio" not in m.group(1) and "&" not in m.group(1), "EngineAssetSlotV16Account"
    return v16.split("\n")


def classify(src_path, names):
    lines = crate_preconditions(src_path)
    return [(n,) + classify_one(lines, n) for n in names]


def _mutate(lines, name, old, new):
    """The source with `old` replaced by `new` once, in the signature of `name` or else in its body."""
    impl, sig = signature(lines, name)
    assert impl, name
    text = "\n".join(lines)
    i = text.index("fn " + name)
    j = text.index("{", i)
    head = text[i:j]
    if head.count(old) == 1:
        return (text[:i] + head.replace(old, new) + text[j:]).split("\n")
    k = text.index("\n    }\n", j)
    body = text[j:k]
    assert body.count(old) == 1, (name, old)
    return (text[:j] + body.replace(old, new) + text[k:]).split("\n")


# (function, rule it must get, mutation of its signature as (old, new) or None)
SELFTEST = [
    ("accrue_asset_path_to_not_atomic", "A0", None),
    ("advance_resolved_slot_not_atomic", "A0", None),
    ("activate_empty_market_slot_not_atomic", "A0", None),
    ("credit_domain_insurance_budgets_not_atomic", "A0", None),
    ("retire_terminal_unbudgeted_insurance_not_atomic", "A0", None),
    ("deposit_not_atomic", "-", None),                                  # given an account
    ("register_empty_materialized_portfolio_not_atomic", "-", None),   # given one by shared reference
    ("execute_trade_with_fee_loss_stale_scoped_not_atomic", "-", None),
    ("kernel_commit_declared_liquidation_recovery", "-", None),        # not a method of the view
    ("advance_resolved_slot_not_atomic", "-",
     ("authenticated_slot: u64", "authenticated_slot: u64, account: &PortfolioV16View<'_>")),
    ("advance_resolved_slot_not_atomic", "-",
     ("authenticated_slot: u64", "authenticated_slot: u64, accounts: &mut [PortfolioV16ViewMut<'_>]")),
    ("advance_resolved_slot_not_atomic", "-", ("authenticated_slot: u64", "authenticated_slot: u64, extra: Foo")),
    ("advance_resolved_slot_not_atomic", "-",
     ("fn advance_resolved_slot_not_atomic(", "fn advance_resolved_slot_not_atomic<A>(")),
    ("activate_empty_market_slot_not_atomic", "-", ("<S: MarketSlotV16ViewMut>", "<S>")),
    ("accrue_asset_path_to_not_atomic", "-", ("steps: &[AccrualStepV16]", "steps: &[PortfolioLegV16]")),
    ("advance_resolved_slot_not_atomic", "-", ("&mut self, ", "")),
    # A1 and A2 (mutations of bodies)
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "A2", None),
    ("claim_resolved_payout_topup_not_atomic", "A2", None),
    ("deposit_not_atomic", "-", None),                                  # an early `return Ok(())`
    ("deposit_not_atomic", "A1", ("        if amount == 0 {\n            return Ok(());\n        }\n", "")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-",
     ("        short_account.validate_with_market(&self.as_view())?;\n", "")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-",
     ("        short_account.validate_with_market(&self.as_view())?;\n",
      "        short_account.validate_with_market(&self.as_view())?;\n        short_account.header.pnl = V16PodI128::new(0);\n")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-",
     ("        short_account.validate_with_market(&self.as_view())?;\n",
      "        short_account.validate_with_market(&self.as_view())?;\n        self.header.c_tot = V16PodU128::new(0);\n")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-",
     ("        short_account.validate_with_market(&self.as_view())?;\n",
      "        long_account.validate_with_market(&self.as_view())?;\n")),
    ("claim_resolved_payout_topup_not_atomic", "-",
     ("        self.validate_shape()?;\n", "        if payout == 0 { return Ok(payout); }\n        self.validate_shape()?;\n")),
    ("claim_resolved_payout_topup_not_atomic", "-",
     ("        account.validate_with_market(&self.as_view())?;\n",
      "        account.validate_with_market(&self.as_view())?;\n        let _ = self.claim_resolved_payout_topup_core_not_atomic(account);\n")),
    ("claim_resolved_payout_topup_not_atomic", "-",
     ("        account.validate_with_market(&self.as_view())?;\n",
      "        account.validate_with_market(&self.as_view())?;\n        debug_log!(payout);\n")),
]


def selftest(src_path=None):
    """Every case of SELFTEST gets the expected verdict (called by make_ledger.py at every regen)."""
    src_path = src_path or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "v16.rs")
    base = crate_preconditions(src_path)
    for name, want, mut in SELFTEST:
        lines = _mutate(base, name, *mut) if mut else base
        got = classify_one(lines, name)[0]
        assert got == want, f"account_rules self-test: {name} {mut!r}: expected {want}, got {got}"
    return len(SELFTEST)


if __name__ == "__main__":
    print(f"self-test: {selftest(sys.argv[1])} cases", file=sys.stderr)
    for row in classify(sys.argv[1], sys.argv[2:]):
        print("\t".join(row))
