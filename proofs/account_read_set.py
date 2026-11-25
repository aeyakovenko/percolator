#!/usr/bin/env python3
"""account_read_set.py <engine src dir> <field> [<field> ...]

Checks on the source that the account check `validate_with_market` (of `PortfolioV16View` and of
`PortfolioV16ViewMut`) cannot read any of the named fields, of the account or of the market:
- no function the check can reach, by cfg_reach.py's call graph (an over-approximation: a call is
  resolved by its receiver's type where the text names it and by name otherwise), names any of the
  fields anywhere in its body (comments and strings removed);
- none of them reads an account or a market header whole: no `bytemuck`, no `transmute`, no
  `.clone()`, no header compared with `==`, and no header handed to a call (the one place a
  header is named alone is `as_view`'s `PortfolioV16View { header: self.header }`, which builds a
  view and reads nothing);
- every call the graph leaves unresolved (a library or primitive method) is on a number, a 256-bit
  number, a pod number wrapper or an array of them, or is the slot slice's `len`: none of them takes
  a header.
The crate has no unsafe code but marker impls and no interior mutability (header_rules.py checks
both), so a field can be read only by naming it or by reading its struct whole. If every check
holds, two states that differ only in the named fields give the check the same inputs, and so the
same answer.

Exit status 1 and the violations if a check fails. `selftest()` runs must-reject mutations of real
bodies and is called by make_ledger.py.
"""
import os, re, shutil, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from rs_scan import scan

FILES = ("v16.rs", "lib.rs", "wide_math.rs")
UNRESOLVED_OK = re.compile(r"(?:I256|U256|U512|Market|V16ActiveBitmap\|V16PodU64|V16PodU64|V16PodU32|V16PodU128|"
                           r"V16PodI128|V16PodI128\|i128|V16PodU128\|u128|V16PodU64\|u64|V16PodU32\|u32|u8|u16|u32|"
                           r"u64|u128|i64|i128|usize)::[a-z_0-9]+")
WHOLE = [r"\bbytemuck\b", r"\btransmute\b", r"\.\s*clone\s*\(\s*\)", r"==\s*\*?\s*(?:[a-z_]+\s*\.\s*)?header\b(?!\s*\.)",
         r"\bheader\b(?!\s*\.)\s*==", r"\bheader\s*[,)]"]
AS_VIEW = "header: self.header,"


def strip(text):
    text = re.sub(r"//[^\n]*", "", text)
    return re.sub(r'"(?:\\.|[^"\\])*"', '""', text)


def reachable(src_dir):
    r = subprocess.run([sys.executable, os.path.join(HERE, "cfg_reach.py"), src_dir, "validate_with_market",
                        "--stubbed", "--list"], capture_output=True, text=True)
    reach = set()
    unresolved = []
    for l in r.stdout.splitlines():
        if l.startswith("  reach "):
            _, t, loc = l.split()
            reach.add((t.split("::")[-1], loc.strip("()")))
        elif l.startswith("unresolved calls:"):
            unresolved = [x.strip() for x in l.split(":", 1)[1].split(",") if x.strip()]
    assert reach, r.stdout + r.stderr
    return reach, unresolved


def check(src_dir, fields):
    reach, unresolved = reachable(src_dir)
    bad = []
    n = 0
    for f in FILES:
        for kind, name, hdr, line, text in scan(open(os.path.join(src_dir, f)).read()):
            if kind != "fn" or (name, f"{f}:{line}") not in reach:
                continue
            n += 1
            body = strip(text)
            body = body[body.index("{"):] if "{" in body else ""
            for fl in fields:
                if re.search(r"\b" + re.escape(fl) + r"\b", body):
                    bad.append(f"{f}:{line} {name} names `{fl}`")
            for p in WHOLE:
                for m in re.finditer(p, body):
                    if p == r"\bheader\s*[,)]" and body[max(0, m.start() - 13):m.end()].endswith(AS_VIEW) \
                            and name == "as_view":
                        continue
                    bad.append(f"{f}:{line} {name} reads a header whole: {body[max(0, m.start() - 40):m.end() + 20]!r}")
    assert n == len(reach), (n, len(reach))
    for u in unresolved:
        if not UNRESOLVED_OK.fullmatch(u):
            bad.append(f"unresolved call `{u}` is not on a number")
    return n, bad


def _mutated(src_dir, fn, old, new):
    t = tempfile.mkdtemp()
    for f in FILES:
        shutil.copy(os.path.join(src_dir, f), t)
    p = os.path.join(t, "v16.rs")
    s = open(p).read()
    i = s.index(fn)
    j = s.index("\n    }\n", i)
    body = s[i:j]
    assert body.count(old) == 1, (fn, old)
    open(p, "w").write(s[:i] + body.replace(old, new) + s[j:])
    return t


SELFTEST = [  # (function head, old, new, a field list, expected violation substring)
    ("    pub fn validate_with_market<T>(&self, market: &MarketGroupV16View<'_, T>) -> V16Result<()> {\n        let config",
     "        let config = market.header.config.try_to_runtime_shape()?;",
     "        let config = market.header.config.try_to_runtime_shape()?;\n"
     "        let _m = market.header.materialized_portfolio_count.get();",
     ["materialized_portfolio_count"], "names `materialized_portfolio_count`"),
    ("    pub fn validate_with_market<T>(&self, market: &MarketGroupV16View<'_, T>) -> V16Result<()> {\n        let config",
     "        let config = market.header.config.try_to_runtime_shape()?;",
     "        let config = market.header.config.try_to_runtime_shape()?;\n        let _b = bytemuck::bytes_of(market.header);",
     ["materialized_portfolio_count"], "reads a header whole"),
    ("    pub fn validate_with_market<T>(&self, market: &MarketGroupV16View<'_, T>) -> V16Result<()> {\n        let config",
     "        let config = market.header.config.try_to_runtime_shape()?;",
     "        let config = market.header.config.try_to_runtime_shape()?;\n        let _h = *market.header == *market.header;",
     ["materialized_portfolio_count"], "reads a header whole"),
    ("    fn validate_source_credit_shape_with_market<T>(",
     "    ) -> V16Result<()> {",
     "    ) -> V16Result<()> {\n        let _c = self.header.capital.get();",
     ["capital", "last_fee_slot"], "names `capital`"),
]


def selftest(src_dir=None):
    src_dir = src_dir or os.path.join(HERE, "..", "src")
    n, bad = check(src_dir, ["materialized_portfolio_count"])
    assert not bad, bad
    for fn, old, new, fields, want in SELFTEST:
        t = _mutated(src_dir, fn, old, new)
        try:
            _, bad = check(t, fields)
        finally:
            shutil.rmtree(t)
        assert any(want in b for b in bad), (fn, new, bad)
    return len(SELFTEST)


if __name__ == "__main__":
    d, fields = sys.argv[1], sys.argv[2:]
    print(f"self-test: {selftest(d)} cases", file=sys.stderr)
    n, bad = check(d, fields)
    for b in bad:
        print("  " + b)
    print(f"{n} functions reachable from validate_with_market; " + ("none reads " + ", ".join(fields) if not bad
                                                                     else f"{len(bad)} violations"))
    sys.exit(1 if bad else 0)
