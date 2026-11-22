#!/usr/bin/env python3
"""po_rules.py <src/v16.rs> <specs/engine_wrapping_arithmetic_<rev>.tsv> [--open-only <proof_ledger.tsv>]

Classifies every unchecked engine operation listed in the census (one row per function, with its source lines) by
one strict local rule, or leaves it unclassified. Only R1 is decided mechanically; every other site is listed with
its line for a proof or a reading.

R1, loop counter. The operation is the statement `IDENT += 1;` and:
  (a) the nearest enclosing loop is `while IDENT < BOUND {` or `while IDENT < BOUND && ... {`, with no `||` in the
      guard (any BOUND: both sides of `<` have IDENT's type, so IDENT < BOUND <= MAX of that type, and IDENT + 1
      cannot overflow);
  (b) inside that loop body, IDENT is written only by statements `IDENT += 1;` (no other assignment, no `&mut IDENT`,
      no shadowing `let IDENT`);
  (c) no `IDENT += 1;` of that loop sits inside a nested loop (it would run more than once per guard check);
  (d) each `IDENT += 1;` is either the last statement of the loop body or directly followed by `continue;`, so at
      most one increment runs per iteration, after the guard was checked;
  (e) nowhere in the function is IDENT borrowed `&mut` or named on a line with a closure.
Then on every execution the increment runs with IDENT < BOUND, so it does not overflow.

Output: one TSV row per operation site: function, line, the source line, rule (R1 or -), and the reason a site is
not R1. Exit status 0. With --open-only, only functions whose PO piece is open in the ledger.
"""
import csv, re, sys

src = []


def strip_comment(line):
    """The line without string and char literals (their contents could hold braces or `//`) and without a `//`
    comment. Block comments and raw strings do not occur on the lines this is used for (checked in main)."""
    out = []
    i, n = 0, len(line)
    while i < n:
        c = line[i]
        if c == '"':
            j = i + 1
            while j < n and line[j] != '"':
                j += 2 if line[j] == "\\" else 1
            out.append('""')
            i = j + 1
            continue
        if c == "'" and i + 2 < n and (line[i + 2] == "'" or (line[i + 1] == "\\" and i + 3 < n and line[i + 3] == "'")):
            out.append("' '")
            i += 3 if line[i + 2] == "'" else 4
            continue
        if line.startswith("//", i):
            break
        out.append(c)
        i += 1
    return "".join(out)


def enclosing_openers(idx):
    """The lines that open the blocks enclosing line idx, innermost first (by brace depth, literals stripped)."""
    out = []
    depth = 0
    for j in range(idx - 1, -1, -1):
        line = strip_comment(src[j])
        for ch in reversed(line):
            if ch == "}":
                depth += 1
            elif ch == "{":
                if depth == 0:
                    out.append(j)
                else:
                    depth -= 1
    return out


# a block between the loop head and the increment may only be one of these (anything else, a nested loop, a
# labelled loop, a loop header split over lines, a closure, an `unsafe` or a plain block, makes the site unclassified)
_BRANCH = re.compile(r"\s*(\}\s*)?(if\b|else\b|match\b|[^{}]*=>\s*\{\s*$)")


def find_enclosing_loop(idx):
    """Line of `while IDENT < ..` that encloses idx through branch blocks only, or None (with the first other opener)."""
    for j in enclosing_openers(idx):
        line = strip_comment(src[j]).strip()
        if re.match(r"while\b", line):
            return j, None
        if not _BRANCH.match(strip_comment(src[j])):
            return None, line
    return None, "no enclosing block"


def block_end(start):
    """Index of the line closing the first block that opens on or after line start (a signature may span lines)."""
    depth = 0
    opened = False
    for j in range(start, len(src)):
        line = strip_comment(src[j])
        for ch in line:
            if ch == "{":
                depth += 1
                opened = True
            elif ch == "}":
                depth -= 1
                if opened and depth == 0:
                    return j
    return None


def r1(idx, fn_start):
    line = strip_comment(src[idx]).strip()
    m = re.fullmatch(r"([A-Za-z_][A-Za-z0-9_]*)\s*\+=\s*1\s*;", line)
    if not m:
        return None, "not `IDENT += 1;`"
    ident = m.group(1)
    head, other = find_enclosing_loop(idx)
    if head is None:
        return None, f"enclosed by a block that is not a branch: {(other or '')[:60]}"
    hl = strip_comment(src[head]).strip()
    if not re.fullmatch(r"while\s+" + re.escape(ident) + r"\s*<(?![=<])\s*[^{}]+\{", hl):
        return None, f"enclosing loop is not `while {ident} < ...`: {hl[:60]}"
    if "||" in hl:
        # `IDENT < BOUND && x` still implies IDENT < BOUND in the body; `||` does not
        return None, f"loop guard has `||`: {hl[:60]}"
    end = block_end(head)
    if end is None:
        return None, "unbalanced braces"
    # (e) nothing in the whole function can write IDENT out of sight: no `&mut IDENT`, no closure that names it
    fn_end = block_end(fn_start)
    for j in range(fn_start, (fn_end or end) + 1):
        l = strip_comment(src[j])
        if re.search(r"&\s*mut\s+" + re.escape(ident) + r"\b", l):
            return None, f"`&mut {ident}` at line {j + 1}"
        if re.search(r"\|[^|]*\|", l) and re.search(r"\b" + re.escape(ident) + r"\b", l):
            return None, f"a closure names {ident} at line {j + 1}"
    body = list(range(head + 1, end))
    incs = []
    for j in body:
        l = strip_comment(src[j])
        if re.search(r"\b" + re.escape(ident) + r"\b", l):
            s = l.strip()
            if re.fullmatch(re.escape(ident) + r"\s*\+=\s*1\s*;", s):
                incs.append(j)
                continue
            # reads are fine; writes are not
            if re.search(r"\b" + re.escape(ident) + r"\s*(\+|-|\*|/|%|<<|>>|&|\||\^)?=(?!=)", l) or \
               re.search(r"&\s*mut\s+" + re.escape(ident) + r"\b", l) or \
               re.search(r"\blet\s+(mut\s+)?" + re.escape(ident) + r"\b", l):
                return None, f"{ident} written at line {j + 1}: {s[:60]}"
    # (c) not inside a nested loop of the body; (d) last statement or followed by continue
    for j in incs:
        inner, _ = find_enclosing_loop(j)
        if inner != head:
            return None, f"increment at line {j + 1} is not directly in the loop's branches"
        nxt = None
        for k in range(j + 1, end + 1):
            t = strip_comment(src[k]).strip()
            if t:
                nxt = (k, t)
                break
        if nxt is None:
            return None, "no statement after the increment"
        k, t = nxt
        if t.startswith("continue"):
            continue
        if k == end and t.startswith("}"):
            continue
        # the increment closes an inner block (if/else) that is itself the last statement: accept only if every
        # following line up to the loop end is a closing brace
        if all(strip_comment(src[q]).strip() in ("}", "") for q in range(k, end + 1)):
            continue
        return None, f"increment at line {j + 1} is followed by `{t[:40]}` (not the end of the body, not continue)"
    return "R1", f"guard `{hl}` at line {head + 1}"


def classify(src_path, census_path, functions=None):
    """One dict per operation site of the census rows (all, or those whose function is in `functions`)."""
    global src
    text = open(src_path).read()
    # strip_comment handles `//` comments and plain string and char literals only
    assert "/*" not in text and not re.search(r'\br#*"', text), "block comments or raw strings: extend strip_comment"
    src = text.split("\n")
    out = []
    for r in csv.DictReader(open(census_path), delimiter="\t"):
        if functions is not None and r["function"] not in functions:
            continue
        fn_start = int(r["line"]) - 1
        for ln in re.findall(r"\d+", r["lines"]):
            idx = int(ln) - 1
            rule, why = r1(idx, fn_start)
            out.append({"function": r["function"], "line": ln, "source": src[idx].strip()[:100],
                        "rule": rule or "-", "reason": why})
    return out


# Each case: a function whose one listed operation is the line holding `i += 1;` marked `// <-`; must R1 accept it?
SELFTEST = [
    (True, "fn f(n: usize) {\n    let mut i = 0;\n    while i < n {\n        i += 1; // <-\n    }\n}"),
    (True, "fn f(n: usize, x: bool) {\n    let mut i = 0;\n    while i < n && x {\n        if x {\n            i += 1; // <-\n            continue;\n        }\n        i += 1;\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    while i <= n {\n        i += 1; // <-\n    }\n}"),
    (False, "fn f(n: usize, x: bool) {\n    let mut i = 0;\n    while i < n || x {\n        i += 1; // <-\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    while i\n        < n\n    {\n        i += 1; // <-\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    'outer: while i < n {\n        i += 1; // <-\n    }\n}"),
    (False, "fn f(n: usize, v: &[u8]) {\n    let mut i = 0;\n    while i < n {\n        for _ in v {\n            i += 1; // <-\n        }\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    while i < n {\n        let g = |k: usize| {\n            i += 1; // <-\n        };\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    while i < n {\n        i += 1; // <-\n        g();\n        i += 1;\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    while i < n {\n        i = n - 1;\n        i += 1; // <-\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    bump(&mut i);\n    while i < n {\n        i += 1; // <-\n    }\n}"),
    (False, "fn f(n: usize) {\n    let mut i = 0;\n    while i < n {\n        unsafe {\n            i += 1; // <-\n        }\n    }\n}"),
]


def selftest():
    """Every case of SELFTEST gets the expected verdict (called by make_ledger.py at every regen)."""
    global src
    saved = src
    try:
        for want, text in SELFTEST:
            src = text.split("\n")
            line = next(k for k, l in enumerate(src) if "// <-" in l)
            src[line] = src[line].replace(" // <-", "")
            got = r1(line, 0)[0] == "R1"
            assert got == want, f"po_rules self-test: expected {'accept' if want else 'reject'}:\n{text}"
    finally:
        src = saved
    return len(SELFTEST)


if __name__ == "__main__":
    print(f"self-test: {selftest()} cases", file=sys.stderr)
    open_only = None
    if "--open-only" in sys.argv:
        ledger = sys.argv[sys.argv.index("--open-only") + 1]
        open_only = {l.split("\t")[0][3:] for l in open(ledger) if l.startswith("PO.") and "\topen\t" in l}
    w = csv.writer(sys.stdout, delimiter="\t", lineterminator="\n")
    w.writerow(["function", "line", "source", "rule", "reason"])
    for r in classify(sys.argv[1], sys.argv[2], open_only):
        w.writerow([r["function"], r["line"], r["source"], r["rule"], r["reason"]])
