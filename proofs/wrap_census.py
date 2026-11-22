#!/usr/bin/env python3
"""wrap_census.py <engine src/v16.rs> <engine rev> <program rev> <clippy sites json> <kernel list tsv> <ledger tsv> <out prefix>
The engine's plain arithmetic that the deployed build does not check, by function: writes <out prefix>.tsv and
<out prefix>.md (normally specs/engine_wrapping_arithmetic_<engine rev>).

Why it is needed. Cargo applies only the root manifest's profiles, so the program's manifest decides how the engine
is compiled into the deployed program. Since <program rev> it checks the program's own arithmetic and leaves every
dependency, the engine included, unchecked: an overflowing `+`, `-`, `*`, shift or negation in the engine
continues with a wrapped value. Every proof of this ledger was made with overflow checks on (Kani always checks;
the Aristotle specs said so), where the same operation stops the call. A no-panic proof therefore also rules out
wrapping, but a statement of the form "if the call returns, then ..." does not: on an input where the checked
model panics, the deployed engine may return a wrong value.

<clippy sites json> holds clippy's `arithmetic_side_effects` sites of the engine ({"engine": [[file, line, text],
...]}), from `cargo clippy --release --lib --message-format=json -- -A clippy::all -W
clippy::arithmetic_side_effects` on the engine at <engine rev>. Each site is one operation. Division and remainder
are not counted here: Rust checks them in every build (a zero divisor or `MIN / -1` panics either way), so the proofs'
model and the deployed build agree on them. `src/wide_math.rs` is not counted either: the arithmetic layer proved the
exact panic condition of each of its functions, and the P pieces state that no call site from `src/v16.rs` meets it.

A function's operations are covered when the function is a listed kernel whose own no-panic piece is proved or
verified. Every other deployed function with such operations is one open piece. The call graph and the deployed
filter (no test, kani or feature code, no host-only items) are header_writes.py's."""
import json, os, re, sys, collections
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, here)
src_path, ENG, PROG, sites_path, klist, ledger, out = sys.argv[1:8]
t = open(os.path.join(here, "header_writes.py")).read()
cut = t.index("# ------------------------------------------------------------------ check the CHECKS table against the source")
_argv = sys.argv; sys.argv = ["header_writes.py", src_path, "x"]
g = {"__name__": "hw"}; exec(compile(t[:cut], "header_writes.py", "exec"), g); sys.argv = _argv
fns = g["fns"]
src_text = open(src_path).read(); src_lines = src_text.split("\n")
# clippy ran on the host: it sees every deployed function only if the engine has no target-specific code
assert "target_os" not in src_text, "the engine has target-specific code: clippy on the host does not see all of it"

PREC = {"<<=": 0, ">>=": 0, "+=": 0, "-=": 0, "*=": 0, "/=": 0, "%=": 0, "<<": 3, ">>": 3, "+": 4, "-": 4,
        "*": 5, "/": 5, "%": 5}
OPS = ("<<=", ">>=", "+=", "-=", "*=", "/=", "%=", "<<", ">>", "+", "-", "*", "/", "%")
def outer_op(e):
    """The operator of one site: the lowest-precedence operator outside brackets (clippy reports each operation
    with its own span, so that is the operation itself)."""
    e = e.strip()
    while e.startswith("(") and e.endswith(")"):
        d = 0
        for i, c in enumerate(e):
            d += c == "("; d -= c == ")"
            if d == 0 and i < len(e) - 1:
                break
        else:
            e = e[1:-1].strip(); continue
        break
    if e.startswith("-") and not e.startswith("-="):
        return "neg"
    d, best, i = 0, None, 0
    while i < len(e):
        c = e[i]
        if c in "([{":
            d += 1
        elif c in ")]}":
            d -= 1
        elif d == 0:
            for op in OPS:
                if e.startswith(op, i):
                    if op in ("-", "*") and (i == 0 or e[i - 1] in "(*/+-%<>=&|!,"):
                        break                       # a unary minus or a dereference, not the operation
                    if best is None or PREC[op] <= PREC[best]:
                        best = op
                    i += len(op) - 1
                    break
        i += 1
    return best
CHECKED_IN_EVERY_BUILD = {"/", "%", "/=", "%="}

def end_line(f):
    return f.line + f.text.count("\n")
qual_count = collections.Counter((f.type, f.name) for f in fns)
def qual(f):
    q = f"{f.type}::{f.name}" if f.type else f.name
    return q if qual_count[(f.type, f.name)] == 1 else f"{q}@{f.line}"

kern = {}
for row in open(klist):
    r = row.rstrip("\n").split("\t")
    if len(r) > 1 and re.fullmatch(r"K\d+", r[0]):
        kern[r[1]] = r[0]
status = {}
for row in open(ledger):
    r = row.rstrip("\n").split("\t")
    if len(r) > 3:
        status[r[0]] = r[3]

sites = json.load(open(sites_path))["engine"]
per = collections.OrderedDict(); div_sites = 0; outside = 0; unsure = []
for fname, line, text in sites:
    if fname != "v16.rs":
        continue
    owners = [f for f in fns if f.line <= line <= end_line(f)]
    if not owners:
        outside += 1                               # not in a deployed function: test, kani or host-only code
        continue
    f = max(owners, key=lambda f: f.line)
    op = outer_op(text)
    if op in CHECKED_IN_EVERY_BUILD:
        div_sites += 1
        continue
    if op is None:                                 # the span is cut (a multi-line expression): count it, conservatively
        unsure.append(line)
        src_line = src_lines[line - 1].strip()
        op = "?"
        text = src_line
    per.setdefault(id(f), (f, []))[1].append((line, op, re.sub(r"\s+", " ", text.strip())))

rows = []
for f, ss in sorted(per.values(), key=lambda v: v[0].line):
    k = kern.get(f.name)
    cov = ""
    if k and status.get(f"{k}.nopanic", "") in ("proved", "verified by Kani"):
        # the kernel list names functions by bare name: the name must be one deployed function
        assert sum(1 for h in fns if h.name == f.name) == 1, f"kernel name {f.name} is not unique among deployed functions"
        cov = f"{k}.nopanic"
    rows.append((qual(f), f.line, ss, cov))

with open(out + ".tsv", "w") as o:
    o.write("function\tline\toperations\tlines\toperators\tcovered_by\n")
    for q, line, ss, cov in rows:
        ops = collections.Counter(op for _, op, _ in ss)
        o.write("\t".join([q, str(line), str(len(ss)), ",".join(str(l) for l, _, _ in ss),
                           " ".join(f"{op}x{n}" for op, n in sorted(ops.items())), cov or "-"]) + "\n")

n_ops = sum(len(r[2]) for r in rows); cov_rows = [r for r in rows if r[3]]
open_rows = [r for r in rows if not r[3]]
md = []
md.append(f"# The engine's unchecked arithmetic in the deployed build (engine {ENG}, program {PROG})\n")
md.append("Generated by `wrap_census.py`. Cargo applies only the root manifest's profiles, so the program's manifest "
          f"decides how the engine is compiled into the deployed program. Since program `cd8a446a` its release profile "
          "checks the program's own arithmetic (an overflow aborts the transaction) and leaves every dependency, the "
          "engine included, unchecked (`[profile.release.package.\"*\"] overflow-checks = false`): with the engine "
          "checked, the 14-leg atomic exit of INV-073 exceeds the 1.4M CU limit. Before that commit no crate was "
          "checked.\n")
md.append("Every proof of this ledger was made with overflow checks on: Kani always checks, and the Aristotle specs "
          "said so. With checks on, an overflowing operation stops the call; in the deployed engine it continues with "
          "a wrapped value. So a no-panic proof also rules out wrapping and carries to the deployed build, but a "
          "statement of the form \"if the call returns, then ...\" carries only where the operations it runs cannot "
          "overflow. The pieces below are those obligations: one per deployed engine function with an unchecked "
          "`+`, `-`, `*`, shift or negation that no proved no-panic piece of the function itself covers.\n")
md.append(f"**Count.** {n_ops} such operations in {len(rows)} deployed functions of `src/v16.rs`; "
          f"{sum(len(r[2]) for r in cov_rows)} of them in {len(cov_rows)} kernels whose no-panic piece is proved or "
          f"verified; **{sum(len(r[2]) for r in open_rows)} in {len(open_rows)} functions are pieces of the ledger (PO.*)**, "
          "open until a proof closes them (the ledger has each one's status). "
          f"Not counted: {div_sites} divisions and remainders (checked in every build, so the proofs' model and the "
          f"deployed build agree on them), {outside} operations outside deployed functions, and `src/wide_math.rs` "
          "(the arithmetic layer proved the exact panic condition of each of its functions; the P pieces state that no "
          "call from `src/v16.rs` meets it).\n")
md.append("Clippy ran on the host build. The engine has no target-specific code (`target_os` appears nowhere in "
          "`src/v16.rs`), so it saw every function the deployed build compiles.\n")
if unsure:
    md.append(f"{len(unsure)} operations span more than one line; clippy's text was cut, so they are listed with "
              f"operator `?` and counted (lines {', '.join(map(str, unsure))}).\n")
md.append("**How a piece closes.** A no-panic proof of the function for the inputs its deployed callers can pass "
          "(Kani, or Aristotle with overflow checks on), or a caller's no-panic proof that covers every call, or "
          "replacing the operation with checked arithmetic that returns an error (a code change: it costs compute "
          "on paths near the limit). Many are loop counters bounded by a slice length or differences guarded by a "
          "comparison just before them; the proof still has to say so.\n")
md.append("**What is already proved and depends on this.** A proved \"if it returns\" statement about a function "
          "carries to the deployed build when the no-panic pieces it needs are done. `make_ledger.py` checks that for "
          "every proved piece at each generation and lists the exceptions in the ledger (\"Which proved pieces carry "
          "to the deployed build\").\n")
md.append("**The program.** Its own arithmetic is checked in the deployed build: an overflow aborts the "
          "transaction, which no safety statement has to exclude. A liveness statement (an instruction that must "
          "succeed) has to show it does not happen, as it shows for every other error.\n")
md.append("## The pieces (status in the ledger)\n\n| Piece | Line | Operations | Operators |\n|---|---|---|---|")
for q, line, ss, cov in open_rows:
    ops = collections.Counter(op for _, op, _ in ss)
    md.append(f"| `PO.{q}` | {line} | {len(ss)} (lines {', '.join(str(l) for l, _, _ in ss)}) | "
              + " ".join(f"`{op}` x{n}" for op, n in sorted(ops.items())) + " |")
md.append("\n## Covered by a proved no-panic piece\n\n| Function | Line | Operations | Covered by |\n|---|---|---|---|")
for q, line, ss, cov in cov_rows:
    md.append(f"| `{q}` | {line} | {len(ss)} | {cov} |")
open(out + ".md", "w").write("\n".join(md) + "\n")
print(f"{n_ops} unchecked operations in {len(rows)} functions; {len(open_rows)} open pieces; {div_sites} div/rem not "
      f"counted; {outside} outside deployed functions; {len(unsure)} cut spans")
