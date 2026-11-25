#!/usr/bin/env python3
"""header_rules.py <src/v16.rs> <fn> [<fn> ...]

Decides, for an entry point of the engine's mutable market view, whether every Ok it returns follows a passed
`validate_shape()` on exactly the state it leaves, by one of two strict local rules, or leaves it unclassified.
That is the T*.header obligation ("if the header predicate held before the call and it returns Ok, it holds of the
state it leaves"), without even needing the "held before".

A function returns a value in three ways only: a `return` expression, a `?` (which returns `Err(From::from(e))`, never
Ok), or its body's final expression. Panics return nothing. So:

H1, the check is the final expression. The function is `fn f(&mut self, ..) -> V16Result<..>` in
`impl<'a, T> MarketGroupV16ViewMut<'a, T>`, and, in its body with comments, strings and characters removed:
  (a) the final expression of the body is exactly `self.validate_shape()`;
  (b) every `return` is followed by `Err(`;
  (c) the token `Ok` does not occur;
  (d) no macro other than `matches!`, no attribute (`#[`), no item or `use` inside the body (`fn`, `macro_rules`,
      `impl`, `struct`, `enum`, `trait`, `mod`), no `unsafe`, `async` or `await`, no block comment or raw string (so (a)
      to (c) read every token the compiler reads).
Then the function returns Ok only as the value of that final call, which is the check of the state it leaves:
nothing runs after it (the crate has no `Drop` impl, checked below).

H2, the check and then reads. As H1, except:
  (a) the body's top-level statements end with `self.validate_shape()?;`, then zero or more statements
      `IDENT.validate_with_market(&self.as_view())?;`, then the final expression `Ok(IDENT)`;
  (c) the token `Ok` occurs once, in that final expression.
Every `validate_with_market` in the crate takes `&self` and the market as `&MarketGroupV16View`, and `as_view` takes
`&self` (checked below): they write nothing, so the state the function leaves is the one the check passed on.

Checked of the crate before any rule is applied: no `impl .. Drop for`, `#![deny(unsafe_code)]` in lib.rs and no
`unsafe` in src/v16.rs other than marker traits and impls with empty bodies (bytemuck's `Pod` and `Zeroable` for the
slot type), no `Cell`, `RefCell` or `UnsafeCell` (nothing writes through a shared reference), and the
view's own `validate_shape(&self)` is the one inherent method of that name in `impl<'a, T> MarketGroupV16ViewMut<'a, T>`.

Output: one TSV row per function: function, rule (H1, H2 or -), reason. `selftest()` runs the must-accept and
must-reject cases (mutations of real bodies) and is called by make_ledger.py before it classifies anything.
"""
import os, re, sys

VIEW_IMPL = "impl<'a, T> MarketGroupV16ViewMut<'a, T> {"
KEYWORDS = {"if", "while", "for", "match", "return", "let", "in", "else", "loop", "break", "continue", "as", "mut",
            "ref", "move", "where", "impl", "self", "Self", "true", "false"}


def strip(text):
    """The text with line comments, string literals and character literals replaced by spaces or empty literals.
    Returns None if it has a block comment or a raw string (not handled: fail closed)."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("/*", i):
            return None
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "r" and re.match(r"r#*\"", text[i:]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            return None
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append('""')
            i = j + 1
            continue
        if c == "'":
            m = re.match(r"'(?:\\.|[^\\'])'", text[i:])
            if m:
                out.append("' '")
                i += m.end()
                continue
        out.append(c)
        i += 1
    return "".join(out)


def method(lines, name):
    """(signature text, body text between the outer braces) of `fn name` in the mutable view's impl blocks."""
    hits = [i for i, l in enumerate(lines) if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", l)]
    if len(hits) != 1:
        return None, f"{len(hits)} definitions"
    i = hits[0]
    k = i
    while not lines[k].startswith("impl"):
        k -= 1
    if lines[k] != VIEW_IMPL:
        return None, f"not in `{VIEW_IMPL}`"
    j = i
    while not lines[j].rstrip().endswith("{"):
        j += 1
    sig = " ".join(l.strip() for l in lines[i:j + 1])
    e = j + 1
    while lines[e] != "    }":
        e += 1
    return (sig, "\n".join(lines[j + 1:e])), ""


def top_level(body):
    """(statements, final expression) of a stripped body: a statement ends at a top-level `;`, or at a top-level `}`
    not followed by `else`, `;`, `.`, `?` or an operator (a block-like expression statement)."""
    stmts, cur, depth, i, n = [], "", 0, 0, len(body)
    while i < n:
        c = body[i]
        cur += c
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth < 0:
                return None, None
            if c == "}" and depth == 0:
                rest = body[i + 1:].lstrip()
                if not re.match(r"(else\b|[;.?)\],=+\-*/%&|^<>!])", rest):
                    stmts.append(cur.strip())
                    cur = ""
        elif c == ";" and depth == 0:
            stmts.append(cur.strip())
            cur = ""
        i += 1
    if depth != 0:
        return None, None
    return [s for s in stmts if s], cur.strip()


def squash(s):
    return re.sub(r"\s+", "", s)


def tokens_ok(body, ok_count):
    """(d), (b) and (c) of the rules; returns a reason or ''."""
    if re.search(r"#!?\[", body):
        return "an attribute in the body"
    for kw in ("fn", "unsafe", "async", "await", "macro_rules", "use", "impl", "struct", "enum", "trait", "mod"):
        if re.search(r"\b" + kw + r"\b", body):
            return f"`{kw}` in the body"
    for m in re.finditer(r"\b([A-Za-z_][A-Za-z_0-9]*)!\s*[\(\[\{]", body):
        if m.group(1) != "matches":
            return f"macro `{m.group(1)}!`"
    for m in re.finditer(r"\breturn\b", body):
        if not re.match(r"\s*Err\s*\(", body[m.end():]):
            return "a `return` that is not `return Err(`"
    k = len(re.findall(r"\bOk\b", body))
    if k != ok_count:
        return f"{k} `Ok` tokens"
    return ""


def classify_one(lines, name):
    got, why = method(lines, name)
    if got is None:
        return "-", why
    sig, raw = got
    if not re.search(r"\(\s*&mut self\b", sig) or not re.search(r"->\s*V16Result<", sig):
        return "-", "not `fn (&mut self, ..) -> V16Result<..>`"
    body = strip(raw)
    if body is None:
        return "-", "a block comment or a raw string"
    stmts, tail = top_level(body)
    if stmts is None:
        return "-", "unbalanced brackets"
    if squash(tail) == "self.validate_shape()":
        why = tokens_ok(body, 0)
        return ("H1", "the final expression is `self.validate_shape()`") if not why else ("-", why)
    m = re.fullmatch(r"Ok\(\s*([a-z_][a-z_0-9]*)\s*\)", tail)
    if m:
        k = len(stmts)
        while k > 0 and re.fullmatch(r"[a-z_][a-z_0-9]*\.validate_with_market\(&self\.as_view\(\)\)\?;",
                                     squash(stmts[k - 1])):
            k -= 1
        if k == 0 or squash(stmts[k - 1]) != "self.validate_shape()?;":
            return "-", "the final `Ok` does not follow `self.validate_shape()?;` and account checks only"
        why = tokens_ok(body, 1)
        return ("H2", f"`self.validate_shape()?;`, {len(stmts) - k} account check(s), `{tail}`") if not why \
            else ("-", why)
    return "-", "final expression `" + re.sub(r"\s+", " ", tail)[:60] + "`"


def crate_preconditions(src_path):
    d = os.path.dirname(src_path)
    lib = open(os.path.join(d, "lib.rs")).read()
    assert "#![deny(unsafe_code)]" in lib, "lib.rs does not deny unsafe code"
    for f in sorted(os.listdir(d)):
        if f.endswith(".rs"):
            t = strip(open(os.path.join(d, f)).read()) or ""
            assert not re.search(r"\bimpl\b[^{;]*\bDrop\s+for\b", t), f"a Drop impl in {f}"
            assert not re.search(r"\b(?:Cell|RefCell|UnsafeCell)\b", t), f"interior mutability in {f}"
            assert not re.search(r"\bmacro_rules!\s*matches\b", t), f"`matches!` redefined in {f}"
            assert not re.search(r"\b(?:fn|struct|enum|const|static|type|as)\s+(?:Ok|Err)\b", t), f"`Ok` or `Err` redefined in {f}"
    v16 = open(src_path).read()
    lines = v16.split("\n")
    sv = strip(v16)
    assert sv is not None, "src/v16.rs has a block comment or a raw string"
    for m in re.finditer(r"\bunsafe\b", sv):  # only marker declarations with empty bodies (bytemuck's Pod/Zeroable)
        assert re.match(r"\s*(?:impl\b[^{]*|trait\b[^{]*)\{\s*\}", sv[m.end():]), \
            f"unsafe code in src/v16.rs: {sv[m.start():m.start() + 60]!r}"
    vs = [i for i, l in enumerate(lines) if re.match(r"\s{4}pub fn validate_shape\(&self\) -> V16Result<\(\)> \{", l)]
    owners = []
    for i in vs:
        k = i
        while not lines[k].startswith("impl"):
            k -= 1
        owners.append(lines[k])
    assert owners.count(VIEW_IMPL) == 1, owners
    for i, l in enumerate(lines):
        if re.match(r"\s{4}pub fn validate_with_market\b", l):
            assert re.match(r"\s{4}pub fn validate_with_market<T>\(&self, market: &MarketGroupV16View<'_, T>\) -> ", l), l
        if re.match(r"\s{4}pub fn as_view\b", l):
            assert re.match(r"\s{4}pub fn as_view\(&self\) -> ", l), l
    return lines


def classify(src_path, names):
    lines = crate_preconditions(src_path)
    return [(n,) + classify_one(lines, n) for n in names]


def _mutate(lines, name, old, new):
    got, _ = method(lines, name)
    assert got, name
    text = "\n".join(lines)
    _, body = got
    assert body.count(old) == 1, (name, old)
    return text.replace(body, body.replace(old, new), 1).split("\n")


# (function, rule it must get, mutation as (old, new) or None); the mutations are of real bodies
SELFTEST = [
    ("withdraw_domain_insurance_not_atomic", "H1", None),
    ("credit_domain_insurance_budgets_not_atomic", "H1", None),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "H2", None),
    ("deposit_not_atomic", "-", None),                                   # an early `return Ok(())`
    ("convert_released_pnl_to_capital_not_atomic", "-", None),           # `Ok(converted)` after a conditional check
    ("credit_domain_insurance_budgets_not_atomic", "-", ("        self.validate_shape()", "        Ok(())")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("            self.validate_source_domain_ledger(domain)?;",
      "            if amount == 0 { return Ok(()); }\n            self.validate_source_domain_ledger(domain)?;")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        if credits.is_empty() {", "        let r = self.validate_shape();\n        if r.is_ok() { return r; }\n"
                                       "        if credits.is_empty() {")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        if credits.is_empty() {", "        if credits.len() > 9 { return self.validate_shape(); }\n"
                                       "        if credits.is_empty() {")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        self.validate_shape()", "        self.validate_shape().or(Err(V16Error::LockActive))")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        self.validate_shape()", "        if credits.len() > 1 { self.validate_shape() } else { Err(V16Error::LockActive) }")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        self.validate_shape()", "        #[cfg(kani)]\n        self.header.c_tot = V16PodU128::new(0);\n"
                                     "        self.validate_shape()")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        self.validate_shape()", "        debug_print!(credits);\n        self.validate_shape()")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        self.validate_shape()", "        Err(V16Error::LockActive) // self.validate_shape()")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        if credits.is_empty() {", "        let _g = |x: V16Result<()>| { return x; };\n"
                                       "        if credits.is_empty() {")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-",
     ("        self.validate_shape()?;\n", "        self.validate_shape()?;\n"
                                         "        self.header.loss_stale_active = restore_loss_stale_active;\n")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-",
     ("        self.validate_shape()?;\n", "        self.validate_shape()?;\n"
                                         "        self.validate_unconfigured_market_tail()?;\n")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-", ("        self.validate_shape()?;\n", "")),
    ("execute_batch_with_fee_loss_stale_scoped_not_atomic", "-",
     ("        let outcome = result?;", "        let outcome = result?;\n"
                                      "        if requests.is_empty() { return Ok(outcome); }")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        if credits.is_empty() {", "        macro_rules! done { () => { return Err(V16Error::LockActive) }; }\n"
                                       "        if credits.is_empty() {")),
    ("credit_domain_insurance_budgets_not_atomic", "-",
     ("        if credits.is_empty() {", "        use core::result::Result::Ok as Fine;\n        if credits.is_empty() {")),
]


def selftest(src_path=None):
    """Every case of SELFTEST gets the expected verdict (called by make_ledger.py at every regen)."""
    src_path = src_path or os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src", "v16.rs")
    base = crate_preconditions(src_path)
    for name, want, mut in SELFTEST:
        lines = _mutate(base, name, *mut) if mut else base
        got = classify_one(lines, name)[0]
        assert got == want, f"header_rules self-test: {name} {mut!r}: expected {want}, got {got}"
    return len(SELFTEST)


if __name__ == "__main__":
    print(f"self-test: {selftest(sys.argv[1])} cases", file=sys.stderr)
    for row in classify(sys.argv[1], sys.argv[2:]):
        print("\t".join(row))
