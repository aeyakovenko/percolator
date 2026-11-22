#!/usr/bin/env python3
"""Build the proof ledger: every charter invariant split into provable pieces, with a status each.

  make_ledger.py            reads the live Aristotle property list (needs ~/.aristotle)
  make_ledger.py --offline  uses the last saved list (properties.json)

Inputs (all in this directory unless noted):
  coverage_matrix.md                 invariant -> stage and the kernels that carry it
  specs/kernel_contracts_list.tsv    the engine's contract-annotated kernels (K01..K58)
  specs/wide_math_spec.md            the arithmetic properties (A1..G5)
  engine source (ENGINE_SRC)         public entry points
  wrapper source (WRAPPER_SRC)       instruction variants
Outputs: proof_ledger.md, proof_ledger.tsv, properties.json
"""
import json, os, re, subprocess, sys, datetime

HERE = os.path.dirname(os.path.abspath(__file__))
# Frozen copies of the landed revisions (LANDED_REV, WRAPPER_REV). The worktrees move on with work in progress.
# In the engine repository this script and the data it reads and writes live in proofs/,
# and the engine source is the checkout itself (which must be at LANDED_REV); the program source comes
# from a checkout of percolator-prog at WRAPPER_REV, named by LEDGER_WRAPPER_SRC.
_here = os.path.dirname(os.path.abspath(__file__))
_frozen = os.path.join(_here, "src_80f8f0b4/v16.rs")
ENGINE_SRC = os.environ.get("LEDGER_ENGINE_SRC") or (_frozen if os.path.exists(_frozen) else os.path.join(_here, "..", "src", "v16.rs"))
WRAPPER_SRC = os.environ.get("LEDGER_WRAPPER_SRC") or os.path.join(_here, "wrapper_ebcded58/v16_program.rs")
if not os.path.exists(WRAPPER_SRC):
    sys.exit("make_ledger.py: set LEDGER_WRAPPER_SRC to src/v16_program.rs of a percolator-prog checkout at the pinned revision")
# The Aristotle project id is account data and is not kept in this file: set ARISTOTLE_PROJECT, or put
# it in a file `.aristotle_project` next to this script (not published).
_pf = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".aristotle_project")
PROJECT = os.environ.get("ARISTOTLE_PROJECT") or (open(_pf).read().strip() if os.path.exists(_pf) else "")
PROVED_AT = "engine add1d027"  # the revision the proofs before the #449 engine change were made against
NEW_REV = "c030aa40"            # the #449 engine change, master from 2026-11-20 06:58 to the landing of LANDED_REV
# The engine revision this ledger describes: the provider-protection floor (percolator#271). ENGINE_SRC
# is this revision. Nothing proved for NEW_REV in layers A and K has to be proved again for it:
# specs/changed_since_c030aa40_at_d1fa39f6.md (changed_fns.py) finds no listed kernel with changed text,
# a changed or new callee, or a changed type, and src/wide_math.rs is identical. The entry-point
# pieces proved for NEW_REV do have to be established again (REESTABLISH): their text is unchanged,
# but they are methods over, or build, a type that gained fields.
# Rolled 2026-11-21 to cb78f1f0, the first two batches of the refactor that moves the logic of the
# engine's methods into pure kernels (specs/changed_since_c030aa40_at_cb78f1f0.md and
# specs/changed_since_d1fa39f6_at_cb78f1f0.md: no listed kernel changed; only private methods changed,
# and ten kernels K64 to K73 are new, each verified by Kani for every input).
PREV_LANDED_REV = "d1fa39f6"   # the revision K61-K63 are new in
CB_REV = "cb78f1f0"            # refactor batches 1-2: K64-K73
# Rolled 2026-11-21 to 80f8f0b4: refactor batches 3-9 (K74-K93, of which fourteen are verified by Kani for every
# input and six only for bounded inputs, with their full-width proofs by Aristotle: K91 task k1, K93 task k2, K88
# task k3, K76-K78 task k4) and config admission refusing a public B chunk above MAX_VAULT_TVL.
# specs/changed_since_cb78f1f0_at_80f8f0b4.md: no listed kernel changed; its flag on K35 is a false positive (the
# callee add_open_interest_for_new_position gained only a cfg(kani) contract attribute; the deployed text is equal).
LANDED_REV = "80f8f0b4"
REESTABLISH = {"T09.header", "T38.header"}
# Kernels whose proof has to be made again for NEW_REV: specs/changed_since_add1d027_at_c030aa40.md
# (changed_fns.py: own text, callees by name, changed types). Every other kernel, its callees and its
# types are textually identical in both revisions, and src/wide_math.rs is identical, so those proofs
# hold for NEW_REV as they stand.
REPROVE = {"K02", "K03", "K07", "K26", "K32", "K34", "K35", "K59", "K60"}
# UTC time the task for NEW_REV was submitted (written by the submission step); a row of a REPROVE
# kernel counts for NEW_REV only when Aristotle touched it after that.
_since = os.path.join(HERE, "b6_submitted_at.txt")
REPROVE_SINCE = open(_since).read().strip() if os.path.exists(_since) else None

def read(path):
    return open(os.path.join(HERE, path) if not path.startswith("/") else path).read()

# ---------------------------------------------------------------- Aristotle statuses
def aristotle_properties(offline):
    """Every declaration of the project: name, status, and whether its final check ran."""
    cache = os.path.join(HERE, "properties.json")
    if not offline:
        env = dict(os.environ)
        env["ARISTOTLE_API_KEY"] = os.environ.get("ARISTOTLE_API_KEY") or open(os.path.expanduser("~/.aristotle")).read().strip()
        import shutil
        out = subprocess.run([shutil.which("aristotle") or os.path.expanduser("~/.local/bin/aristotle"), "properties", PROJECT,
                              "--status", "ALL", "--json"], capture_output=True, text=True, env=env).stdout
        try:
            decls = json.loads(out)["declarations"]
        except Exception:
            decls = []
        if decls:
            json.dump(decls, open(cache, "w"), indent=1)
    rows = []
    for d in json.load(open(cache)):
        status = d.get("status_name") or ""
        if status == "PROVED" and d.get("unchecked"):
            status = "PROVED (unchecked)"
        rows.append({"name": d["name"], "status": status, "description": d.get("description") or "",
                     "last_updated": d.get("last_updated") or ""})
    return rows

# ---------------------------------------------------------------- which code is in the deployed build
SRC_LINES = read(ENGINE_SRC).split("\n")
def cfg_gate(fn_name):
    """The `#[cfg(..)]` directly above `fn_name` (above its attributes and doc comment), if any.
    Functions under `cfg(any(kani, feature = "fuzz"))` are not compiled into the deployed program."""
    for i, line in enumerate(SRC_LINES):
        if re.match(r"\s*(pub(\([a-z]+\))? )?(const )?fn " + re.escape(fn_name) + r"\b", line):
            j = i - 1
            while j >= 0:
                t = SRC_LINES[j].strip()
                if re.match(r"(pub(\([a-z]+\))? )?(const )?fn |\}$|impl ", t) or t == "":
                    break
                m = re.match(r"#\[cfg\((.*)\)\]$", t)
                if m:
                    return m.group(1)
                j -= 1
            return None
    return None
NOT_DEPLOYED = "not in the deployed build"

# ---------------------------------------------------------------- pieces
pieces = {}   # id -> dict(layer, target, statement, method, status, evidence)
def piece(pid, layer, target, statement, method, status="open", evidence=""):
    pieces[pid] = dict(id=pid, layer=layer, target=target, statement=statement, method=method,
                       status=status, evidence=evidence)

# A: arithmetic
spec = read("specs/wide_math_spec.md")
for m in re.finditer(r"(?m)^- ([A-G]\d+b?)\. (.*(?:\n  .*)*)", spec):
    text = " ".join(m.group(2).split())
    piece(m.group(1), "A arithmetic", "wide_math.rs", text[:150] + ("..." if len(text) > 150 else ""), "Aristotle")

# K: kernels
kernels = {}
for line in read("specs/kernel_contracts_list.tsv").splitlines():
    f = line.split("\t")
    if len(f) >= 2:
        kernels[f[0]] = f[1]
kernels["K59"] = "kernel_side_adl_index"                    # new in the #449 engine change
kernels["K60"] = "kernel_raw_basis_after_adl_reduction"     # new in the #449 engine change
kernels["K61"] = "kernel_provider_protection_num"            # new in LANDED_REV (provider-protection floor)
kernels["K62"] = "kernel_provider_protection_floor_observe"  # new in LANDED_REV
kernels["K63"] = "kernel_provider_protection_floor_shift"    # new in d1fa39f6
# new in cb78f1f0 (refactor batches 1-2, K64-K73) and in 80f8f0b4 (batches 3-9, K74-K93)
REFACTOR_KERNELS = {}
# Kani checks these only for bounded inputs (specs/kernel_contracts_spec_80f8f0b4.md); the prover proves them at full width
BOUNDED_KERNELS = {"K76", "K77", "K78", "K88", "K91", "K93"}
for _line in read("specs/kernel_contracts_list_80f8f0b4.tsv").splitlines():
    _f = _line.split("\t")
    if len(_f) >= 2 and int(_f[0][1:]) >= 64:
        REFACTOR_KERNELS[_f[0]] = _f[1]
        kernels[_f[0]] = _f[1]
changed_by_449 = {"K07", "K32", "K34", "K35"}
for k, fn in kernels.items():
    note = ""
    if k in changed_by_449:
        note = " (contract extended by the #449 engine change: prove against the new revision)"
    if k in ("K59", "K60"):
        note = " (new in the #449 engine change; Kani covers an 8-bit grid only)"
    if k in ("K61", "K62", "K63"):
        note = f" (new in engine {PREV_LANDED_REV}; its Kani contract check verifies for all inputs)"
    if k in REFACTOR_KERNELS:
        _rev = CB_REV if int(k[1:]) <= 73 else LANDED_REV
        if k in BOUNDED_KERNELS:
            note = f" (new in engine {_rev}; its Kani contract check covers bounded inputs only, the full-width proof is the prover's)"
        else:
            note = f" (new in engine {_rev}; its Kani contract check verifies for all inputs)"
    gate = cfg_gate(fn)
    piece(f"{k}.contract", "K kernel", fn, f"`{fn}` satisfies its requires/ensures contract for every input{note}", "Aristotle")
    piece(f"{k}.nopanic", "K kernel", fn, f"no execution of `{fn}` panics under its requires", "Aristotle")
    if gate and "kani" in gate:
        for suffix in ("contract", "nopanic"):
            pieces[f"{k}.{suffix}"]["status"] = NOT_DEPLOYED
            pieces[f"{k}.{suffix}"]["evidence"] = f"compiled only under cfg({gate}); it serves a test-only entry point"
# Two named consequences of the new kernels (last section of specs/kernel_contracts_spec_<rev>.md).
piece("K59.exposure", "K kernel", "kernel_side_adl_index",
      "when `kernel_side_adl_index(oi, raw_sum, a)` returns `Ok(A)` with `raw_sum > 0` and `oi > 0`: "
      "`A * raw_sum <= ADL_ONE * oi < (A + 1) * raw_sum`, so the legs' exact exposure never exceeds the side's "
      "effective OI and is short of it by less than `raw_sum / ADL_ONE` (the #449 bound)", "Aristotle")
piece("K60.progress", "K kernel", "kernel_raw_basis_after_adl_reduction",
      "when `kernel_raw_basis_after_adl_reduction(raw, e, e2, oi, raw_sum)` returns `Ok(next_raw)`: "
      "`1 <= next_raw < raw`, `oi - (e - e2) <= (raw_sum - raw) + next_raw`, and `next_raw == e2` when `oi == raw_sum`",
      "Aristotle")
# Four named consequences of the provider-protection kernels (last section of specs/kernel_contracts_spec_d1fa39f6.md).
piece("K61.realization", "K kernel", "kernel_provider_protection_num",
      "consuming `x` of backing together with burning `x` of claims leaves the protection unchanged: "
      "`protection(p, r - x, c - x) == protection(p, r, c)` for `x <= r` and `x <= c`", "Aristotle")
piece("K61.monotone", "K kernel", "kernel_provider_protection_num",
      "protection does not fall when the reserved backing rises or the claims fall, and does not rise when a claim is added",
      "Aristotle")
piece("K62.minimum", "K kernel", "kernel_provider_protection_floor_observe",
      "observing a sequence of protections in any order leaves the minimum of the starting floor and all of them", "Aristotle")
piece("K63.inverse", "K kernel", "kernel_provider_protection_floor_shift",
      "a deposit of `a` followed by a withdrawal of `a` leaves the floor unchanged unless the deposit saturated; with "
      "`floor <= principal`, shifting both down by `a <= principal` keeps `principal - floor` unchanged exactly when `a <= floor`",
      "Aristotle")
EXTRA_K = {("kernel_side_adl_index", "exposure"): "K59.exposure",
           ("kernel_raw_basis_after_adl_reduction", "progress"): "K60.progress",
           ("kernel_provider_protection_num", "realization"): "K61.realization",
           ("kernel_provider_protection_num", "monotone"): "K61.monotone",
           ("kernel_provider_protection_floor_observe", "minimum"): "K62.minimum",
           ("kernel_provider_protection_floor_shift", "inverse"): "K63.inverse"}

# T: transitions (one per public engine entry point)
sys.path.insert(0, HERE)
src = read(ENGINE_SRC)
def entry_bodies(src):
    """name -> body text of each public `*_not_atomic` entry point (brace matched)."""
    out = {}
    for m in re.finditer(r"(?m)^    pub fn ([a-z_0-9]+_not_atomic)", src):
        i = src.index("{", src.index(")", m.end()))
        depth, j = 0, i
        while True:
            c = src[j]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        out.setdefault(m.group(1), src[i:j + 1])
    return out
bodies = entry_bodies(src)
entries = sorted(bodies)
from transition_kinds import classify
T_ROWS = {r["name"]: r for r in classify(src)}
assert sorted(T_ROWS) == entries, "transition_kinds.py and this file see different entry points"
t_kind = {}
for i, e in enumerate(entries, 1):
    assert T_ROWS[e]["label"] == f"T{i:02d}"
    cls, note = T_ROWS[e]["cls"], T_ROWS[e]["note"]
    how = {
        "final": "every Ok path ends in `validate_shape()` after its last market write, so the predicate holds by a runtime "
                 "check; the obligation is that path structure (Aristotle, small)",
        "delegates": "a call to an entry point of class final; inherits its obligation (Aristotle, small)",
        "local": "no shape check; follows from guards inside the function for every state (Aristotle, small real proof)",
        "frame": "no shape check; the function writes no header field a check reads (Aristotle, from its write set)",
        "mixed": "final on most Ok paths; the paths named in the statement end without a check (Aristotle: structural part "
                 "small, the named paths as their class says)",
        "real": "no shape check in a default build; needs kernel contracts and invariants relating the header to slots and "
                "accounts (Aristotle, large; split by writer in specs/header_write_sets_<rev>.md); today: Kani closure and "
                "composition harnesses and audit-scan fuzz",
        "dispatch": "the conjunction of the obligations of the functions its branches call, four of them internal and of "
                    "class real (Aristotle, after those)",
        "not deployed": "not in the deployed build",
    }[cls]
    t_kind[e] = cls
    piece(f"T{i:02d}.header", "T transition", e,
          f"[{cls}] if the header predicate (`validate_shape` of a default build) held before `{e}` and it returns Ok, it holds "
          f"of the state it leaves" + (f". Found when read: {note}" if note else ""),
          how)
    piece(f"T{i:02d}.scan", "T transition", e,
          f"if the per-asset scan predicate (`validate_shape_full_audit_scan`) held before `{e}` and it returns Ok, it holds after",
          "Aristotle from the kernel contracts (frame for the slots it does not write, kernel deltas for the ones it does); "
          "today: audit-scan fuzz, Kani closure and composition harnesses")
    piece(f"T{i:02d}.account", "T transition", e,
          f"every portfolio account `{e}` is given passes `validate_with_market` against the market it leaves, if it did before and the call returns Ok",
          "Aristotle (the entry points that end in `account.validate_with_market(..)` are structural, like the header half); "
          "today: audit-scan fuzz and the account audits of the program")
    piece(f"T{i:02d}.mirror", "T transition", e,
          f"`{e}` keeps every market total in step with the accounts it is given: for each mirrored total (capital, positive PnL and "
          "its bound, source claims per domain, stored positions, raw basis and loss weight per side, stale and negative-PnL counts) "
          "the total minus the given accounts' contribution is unchanged (specs: global invariant, part M)",
          "Aristotle from the kernel contracts; today: the program's reference-model census and the stateful fuzz")
    gate = cfg_gate(e)
    if gate and "kani" in gate:
        for suffix in ("header", "scan", "account", "mirror"):
            pieces[f"T{i:02d}.{suffix}"]["status"] = NOT_DEPLOYED
            pieces[f"T{i:02d}.{suffix}"]["evidence"] = f"compiled only under cfg({gate}): a test and proof API, absent from the program"

# P: panic sites, the calls from src/v16.rs into src/wide_math.rs (gen_callsite_spec.py)
P_SPEC = f"specs/wide_math_callsites_spec_{LANDED_REV}.md"
p_none = []
if os.path.exists(os.path.join(HERE, P_SPEC)):
    for m in re.finditer(r"(?m)^\| (P\d\d) \| (\d+) \| `(\w+)` \| `(\w+)` \((\w+)\) \| (\w+) \| ([^|]*) \|", read(P_SPEC)):
        pid, line, fn, callee, label, cls, why = m.groups()
        piece(pid, "P panic sites", fn,
              f"the call to `{callee}` at line {line} cannot meet its panic condition ({label}); class {cls}: {why.strip()}",
              "Aristotle, from the exact panic conditions of the arithmetic layer" if cls != "none" else f"follows from {label}")
        if cls == "none":
            p_none.append((pid, label))
    for pid, target, text in (
        ("PL1", "PortfolioLegV16Account::try_to_runtime", "an active leg decoded from account data passed `validate_active_leg`"),
        ("PL2", "leg_kf_delta_components_for_settlement_from_asset", "every caller in a default build passes an active leg it decoded with `try_to_runtime`"),
        ("PR1", "kernel_recompute_resolved_payout_rate", "the recomputed payout rate has `1 <= den` and `num <= den`"),
        ("PR2", "writers of header.resolved_payout_ledger", "every writer of the payout ledger leaves the rate unchanged or ends with a recompute"),
        ("PR3", "payout_snapshot_captured", "the snapshot flag is set only by the function that initializes the ledger and recomputes the rate"),
        ("PR4", "resolved payout receipts", "a present receipt is written only after the ledger was initialized; callers pass the header's ledger"),
    ):
        piece(pid, "P panic sites", target, text, "Aristotle (PL: leg validity; PR: the ledger-rate invariant)")

# P.surface: what src/v16.rs can reach in wide_math.rs at all (compiler census, census_wide_surface.sh)
_census = os.path.join(HERE, "specs", f"wide_surface_census_{LANDED_REV}.txt")
_census_ok = os.path.exists(_census) and open(_census).read().startswith(f"OK {LANDED_REV}")
piece("P.surface", "P panic sites", "src/v16.rs",
      "`src/v16.rs` reaches `wide_math.rs` only through five free functions and these `U256` items: `ONE`, `from_u128`, "
      "`from_u64`, `is_zero`, `try_into_u128`, `checked_add/sub/mul/div/rem`, equality and ordering; each of those items "
      "never panics (A1 to A4, B1 to B4)",
      "compiler census (the file builds against a stand-in module exposing only these items) plus the arithmetic layer",
      status="proved" if _census_ok else "open",
      evidence=(f"specs/wide_surface_census_{LANDED_REV}.txt (cargo check --lib, default features) with A1-A4, B1-B4; the comparison "
                "operators are the core library's provided methods over the proved `partial_cmp`") if _census_ok else "")

# TR: the real class of layer T, split into statements local to the header and one account
for pid, text in (
    ("TR.a", "H2, H3: every flag write is 0, 1 or `encode_bool(..)`; `recovery_reason` is written only by `from_runtime(Some(reason))`"),
    ("TR.b", "H5, H6, H14, H15: `c_tot + insurance + backing_provider_earnings_total + source_fresh_backing_total_num / BOUND_SCALE` "
             "rises by at most the change of `vault` (for the real entry points: it does not rise)"),
    ("TR.c", "H16: `insurance - insurance_domain_budget_remaining_total` does not fall, and the reserved insurance credit total stays "
             "within the remaining budget total"),
    ("TR.d", "H7, H10, H11: the positive-PnL totals are written only by `set_account_pnl_inner`, which keeps the bound relations by construction"),
    ("TR.e", "H17: with `U(account)` the account's positive-PnL bound minus its source claims, `U(account) >= 0` is preserved and "
             "`(pnl_pos_bound_tot_num - source_claim_bound_total_num) - U(account)` does not fall"),
    ("TR.f", "H4, H12: the payout ledger is written only through `from_runtime`, and only after the snapshot flag is set"),
):
    piece(pid, "T transition", "writers reached by T32, T37, T42, T45 and the crank's internal functions",
          text + " (specs/transition_shape_spec_<rev>.md, section on the real class)",
          "Aristotle from the kernel contracts; the header pieces of the real entry points are the conjunction of TR.a to TR.f")

# G: the global invariant every reachable state has to satisfy, and the induction that gives it.
# Three of its five parts are functions of the code and need no statement of their own: H = `validate_shape` of a default
# build, S = `validate_shape_full_audit_scan`, V = `validate_with_market` for every portfolio account of the market.
for pid, target, text, how in (
    ("G.M", "market and all portfolio accounts", "part M: each market total that mirrors account state equals the sum over all portfolio "
     "accounts of the market (the engine cannot check this at run time: it never sees all accounts). To be written as a formal statement, "
     "one line per mirrored total", "formal statement, then reviewed"),
    ("G.C", "vault token accounts and market header", "part C: custody. The program's vault token accounts hold exactly what the header's "
     "`vault` says, in the configured mints, under the program's authority. To be written as a formal statement", "formal statement, then reviewed"),
    ("G.other", "portfolio accounts an instruction does not name",
     "frame: an instruction leaves every account it does not name valid against the market it writes (a market write such as an asset "
     "retirement or a new market id must not invalidate a leg of an account that is not present)", "Aristotle, per kind of market write"),
    ("G.init", "InitMarket", "the initial state satisfies H, S, V, M and C", "Aristotle"),
    ("G.reach", "every instruction", "induction: H, S, V, M and C hold in every state reachable from InitMarket by successful instructions; "
     "the conjunction of the instruction pieces of layer W", "Aristotle, composition of layer W"),
):
    piece(pid, "G global invariant", target, text, how)

# Y: what the proofs assume and what is established by evidence instead of proof
for pid, target, text in (
    ("Y.runtime", "Solana runtime", "assumed: transaction atomicity (a failed instruction changes nothing), account ownership and signer "
     "checks, rent and reallocation rules, the Clock sysvar, the compute meter"),
    ("Y.spl", "SPL Token program", "assumed: a transfer moves exactly the amount between the named accounts or fails; mint and owner fields "
     "are as the token program wrote them"),
    ("Y.oracle", "oracle accounts", "assumed: the layouts the program parses (Pyth, Switchboard, Chainlink) are the publishers' layouts. "
     "Nothing is assumed about the values: what an oracle reports and what any authority signs are adversarial inputs. The charter "
     "already bounds what they can do to a user (INV-005 with its role matrix, INV-014, INV-045, INV-046, INV-064, INV-018, the "
     "isolation invariants and the exit rule), and those invariants are proved with the oracle and the authorities quantified over"),
    ("Y.tool", "Aristotle", "trusted: its model of Rust and of the core library, with the library behaviours each run summary lists as "
     "assumed, and the bound on object sizes (2^56 bytes)"),
    ("Y.compute", "compute budget (INV-077)", "evidence, not proof: every instruction fits the compute limit at the maximum shape "
     "(measured by the program's max-shape tests)"),
    ("Y.terminates", "loops", "evidence, not proof: every loop is bounded by a compile-time constant or by an input length that the "
     "compute meter bounds (Aristotle proves no termination)"),
    ("Y.statements", "the formal statements", "an independent reader has checked that each formal statement says what the charter "
     "invariant means; a proof of the wrong statement proves nothing"),
):
    piece(pid, "Y assumptions and evidence", target, text, "written down and signed off; not a proof")

# S: the settlement and bookkeeping methods between the kernels and the entry points. The owner's
# priority (2026-11-20): Aristotle goes where Kani has gaps, business logic before easily tested math.
# The gap is measured by the call graph (kani_reach.py, specs/kani_gap_map_<rev>.md): 62 deployed
# functions and 14 of the 50 deployed entry points are reached by no harness: trade execution,
# recovery and forfeit, three crank steps and the terminal routes. Those come first (the trade pilot
# below: TF, TA, TB with the contracts TC1 to TC9; prompts/stage_s2_trade_pilot.txt). The two
# methods S1 and S2 were a first pilot, submitted on a count by NAME that overstated the gap;
# existing harnesses reach both (8 and 19 harnesses), so the task was cancelled and the pieces wait
# behind the certain gaps. Each method is proved from its own body and from contracts of the
# methods it calls; a property is closed only when the contracts it names are.
TRADE_EVIDENCE = ("Aristotle. The trade pilot (task s2, engine d1fa39f6) stated it and stopped by itself with this piece open: the "
                  "walk of the compiled body was too expensive (the byte-level model of the 9,419-byte account, the number of `?` exits, "
                  "and shared borrows that must be given back on every exit). Kani proves the gates of the trade entry point, not its "
                  "body, so today the evidence is the engine's tests and fuzz and the program's stateful suites")
for pid, target, text in (
    ("TF.margin", "finish_trade_checks_not_atomic",
     "no trade that needed initial margin from an account ends with that account below it: its certificate is valid and "
     "`certified_equity >= certified_initial_req` at the return (rests on TC9)"),
    ("TF.locked", "finish_trade_checks_not_atomic",
     "in the locked lane the account also meets its initial requirement from capital and negative PnL alone, without positive credit"),
    ("TF.frame", "finish_trade_checks_not_atomic",
     "writes nothing unless the trade increased risk outside the locked lane; then only the fields a lien creation writes (rests on TC9)"),
    ("TF.order", "finish_trade_checks_not_atomic",
     "the long account's stored certificate can overstate its equity when its margin is checked, because the short account's margin "
     "lien was created after it was written (answered from the source by the pilot and by a second reading, not proved); the "
     "margin rule does not rest on that number: see TF.reserved"),
    ("TF.reserved", "create_initial_margin_source_lien_if_needed",
     "when it returns Ok, the account's initial requirement is covered by capital less its negative PnL and fee debt plus the "
     "credit its own valid liens reserve (`incremental_initial_margin_source_credit_needed == 0`), and no other account's lien "
     "can take reserved backing; so a risk-increasing trade never leaves an account with source claims below its initial "
     "requirement, whatever its certificate says"),
    ("TA.fee", "apply_trade_after_refresh_not_atomic",
     "each account pays `min(fee, capital)`, or nothing when its PnL is negative; the fee and the notional are the stated formulas (rests on TC1)"),
    ("TA.conservation", "apply_trade_after_refresh_not_atomic",
     "applying a fill moves no token value and no PnL: `vault` and both accounts' PnL are unchanged, and each fee moves from that "
     "account's capital (and `c_tot`) to `insurance` (rests on TC1 to TC7)"),
    ("TA.positions", "apply_trade_after_refresh_not_atomic",
     "each account's position on the traded asset becomes the planned one and its other legs are unchanged, for a lookup whose "
     "current position is the stored one (the pilot found the first wording false without that: a lookup with equal current and "
     "next positions changes nothing and is not compared with the stored leg; the engine builds lookups from the account) "
     "(rests on TC2, TC5)"),
    ("TA.gate", "apply_trade_after_refresh_not_atomic", "a side the fill grew does not end above the side cap (rests on TC8)"),
    ("TA.frame", "apply_trade_after_refresh_not_atomic",
     "writes four header fields, eight account fields and only the traded asset's slot (rests on TC1 to TC7)"),
    ("TA.nopanic", "apply_trade_after_refresh_not_atomic", "no execution of the body panics outside its named callees"),
    ("TB.flag", "execute_batch_with_fee_loss_stale_scoped_not_atomic",
     "the scoped suspension of `loss_stale_active` never leaks: on every return after the inner call, Ok or Err, the flag has its entry value"),
    ("TB.checked", "execute_batch_with_fee_loss_stale_scoped_not_atomic",
     "every Ok return follows a passing `validate_shape` and `validate_with_market` of both accounts on the final state"),
    ("TC1", "charge_account_fee_current_not_atomic",
     "contract: charges `min(requested, capital)` or nothing when PnL is negative; capital and `c_tot` fall and `insurance` rises by that amount; `vault` is not written"),
    ("TC2", "validate_trade_position_preflight", "contract: writes nothing; returns the lookups it was given and the risk flags computed from them"),
    ("TC3", "require_asset_risk_change_allowed", "contract: writes nothing; a risk increase needs both side indices at `ADL_ONE`"),
    ("TC4", "begin_terminal_trade_residual_if_needed", "contract: writes only the account's close ledger, one header counter and one field of the traded asset's slot, and nothing unless a final fill closes a losing single-leg account"),
    ("TC5", "apply_current_position_delta_with_lookup",
     "contract: the leg becomes the planned position, other legs are unchanged, none of capital, PnL, `c_tot`, `insurance`, `vault` is written, and nothing at all is written for a zero delta"),
    ("TC6", "transfer_trade_residual_reward_credit", "contract: writes only the two accounts' residual reward counters"),
    ("TC7", "recertify_account_after_trade_delta", "contract: writes only the account's certificate"),
    ("TC8", "kernel_trade_side_oi_cap_gate", "contract: the exact return condition of the side cap gate"),
    ("TC9", "create_initial_margin_source_lien_if_needed",
     "contract: on Ok the certificate is valid; writes only capital, certificate and source domains of that account and six header fields"),
):
    piece(pid, "S settlement glue", target, text, TRADE_EVIDENCE)
for pid, target, text in (
    ("S0.C1", "fresh_counterparty_backing_expiry_slot", "contract: writes nothing"),
    ("S0.C2", "credit_post_snapshot_residual_not_atomic",
     "contract: writes only `payout_snapshot` and `resolved_payout_ledger`, and nothing when the amount is zero or no snapshot is captured"),
    ("S0.C3.bound", "book_realized_kf_loss_for_source_domain_not_atomic", "contract: the returned junior amount is at most the amount booked"),
    ("S0.C3.frame", "book_realized_kf_loss_for_source_domain_not_atomic",
     "contract: writes neither `vault` nor `c_tot`, and no header field outside seven named ones"),
    ("S0.C3.value", "book_realized_kf_loss_for_source_domain_not_atomic",
     "the exact relation between the amount booked, the part given to the domain's backing and the junior amount (the junior part is "
     "rounded up)"),
    ("S0.C4", "burn_account_source_claim_bound_num",
     "contract: writes none of the header's positive-PnL totals or its negative-PnL count, and none of the account's PnL, lock or active bitmap"),
    ("S0.C5", "prepare_source_credit_domain_recompute_for_epoch", "contract: changes only `credit_rate_num` and `credit_epoch`; both epochs rise by one"),
    ("S0.C6", "source_credit_for_domain / set_source_credit_for_domain",
     "contract: the exact write set of the setter (the record, three header totals, and the protection floor of the same domain, which can only fall)"),
    ("S0.C7", "ensure_account_source_claim_market_id", "contract: finds or creates the account's entry for the domain and tags it with the asset's market id; nothing else changes"),
    ("S0.C8", "source_domain_mut_or_insert", "contract: after S0.C7 it returns the same entry and writes nothing"),
    ("S1.amount", "pay_kf_loss_from_free_capital_not_atomic", "pays `min(loss, capital)`; a zero payment writes nothing"),
    ("S1.conservation", "pay_kf_loss_from_free_capital_not_atomic",
     "the account's capital and the market's `c_tot` fall by exactly the amount paid and `vault` is unchanged (rests on S0.C1, S0.C2, S0.C3.frame)"),
    ("S1.live", "pay_kf_loss_from_free_capital_not_atomic",
     "Live branch: nothing else is written in the market; the pending credit for the loss domain grows by exactly the amount paid"),
    ("S1.resolved", "pay_kf_loss_from_free_capital_not_atomic",
     "Resolved branch: the pending credit is not written and no header field outside eight named ones is (rests on S0.C2, S0.C3.frame)"),
    ("S1.nopanic", "pay_kf_loss_from_free_capital_not_atomic", "no call panics"),
    ("S2.account", "set_account_pnl_inner", "the account's PnL becomes `new_pnl` and its certificate is invalidated; `i128::MIN` is refused"),
    ("S2.mirror", "set_account_pnl_inner", "the market's `pnl_pos_tot` moves by exactly the change of the account's positive PnL (a decrease rests on S0.C4)"),
    ("S2.count", "set_account_pnl_inner", "`negative_pnl_account_count` follows the sign of the account's PnL"),
    ("S2.source", "set_account_pnl_inner",
     "every atom of positive PnL that appears is registered as a claim on exactly one named source domain (account entry, bound and "
     "exact claims of that domain, and the market total); in Live an increase without a domain is refused (rests on S0.C5 to S0.C8)"),
    ("S2.totals", "set_account_pnl_inner", "header checks H7, H10 and H11 are preserved"),
    ("S2.claims", "set_account_pnl_inner",
     "whether H17 (`pnl_pos_bound_tot_num >= source_claim_bound_total_num`) is preserved, and under which precondition on the "
     "caller's pre-burned claims"),
    ("S2.frame", "set_account_pnl_inner", "the exact header, slot and account fields an Ok call can write"),
    ("S2.nopanic", "set_account_pnl_inner", "no call panics"),
):
    piece(pid, "S settlement glue", target, text,
          "Aristotle, using the kernel contracts already proved (first pilot s1, cancelled 2026-11-20: Kani harnesses reach this "
          "method through other entry points, so it waits behind the methods no harness reaches); what those harnesses assert "
          "about it has not been checked")

# X: states committed after an error (labels X.., because E1..E6 are arithmetic labels) (specs/error_commit_paths_<rev>.md, gen_error_commit_spec.py)
for pid, target, text in (
    ("X1.header", "kernel_commit_declared_liquidation_recovery",
     "on every path that returns `RecoveryRequired` after this call declared Recovery, nothing is written to the market after the "
     "`validate_shape()` inside `declare_permissionless_recovery`"),
    ("X1.account", "kernel_commit_declared_liquidation_recovery",
     "on those paths the account passes `validate_with_market` at the point of return"),
    ("X2", "clear_leg_at_slot_inner",
     "the writes before a `Stale` return (`liquidation_lock = true`, `health_cert.valid = 0`) leave a state the engine accepts and "
     "that `sync_unattributed_loss_lock` would produce as well"),
    ("X4", "deregister_empty_materialized_portfolio_not_atomic",
     "it returns `LockActive` only before its single write; the program's SyncMaintenanceFee continues after that error"),
    ("X3.before", "permissionless_auto_crank_not_atomic",
     "the listed `NonProgress` returns come before any write"),
    ("X3.refresh", "liquidate_account_not_atomic",
     "at the two `NonProgress` returns after its refresh the state is that of a completed refresh with a valid certificate"),
    ("X3.guarded", "permissionless_auto_crank_not_atomic",
     "at each of the four `NonProgress` returns that follow further writes (N1c, N2, N3, N4), in every execution in which "
     "`header.insurance` and the account's position vector are what they were before the call: the state passes the per-asset "
     "scan and `validate_with_market`, and differs from the state before the call only by writes a successful step of the same "
     "kind makes and that debit no other account. The program commits after `NonProgress` only when it accrued the market in the "
     "same instruction and that guard holds (program #485), and it checks the header before every crank commit. Split by a "
     "reading of 2026-11-20 (specs/x3_guarded_reading_<rev>.md) into the three lemmas below"),
    ("X3.R1", "reduce_position",
     "an Ok return with `close_q >= 1` leaves the leg's active flag or raw basis different from before the call; with it the "
     "guard refuses every execution that reaches N1c or N4 (both follow a reduction)"),
    ("X3.R2", "advance_pending_close_residual_not_atomic",
     "at N2 nothing was booked onto the other side and no insurance was spent: the ledger's remaining residual falls by exactly "
     "what is spent or booked, and in Live mode a residual that cannot be booked returns `RecoveryRequired`"),
    ("X3.R3", "release_account_source_credit_liens_if_unneeded_core_not_atomic",
     "at N3 the loop has written nothing, and the settlement of a flat account's side effects before it debits no other account"),
):
    piece(pid, "X committed after an error", target, text,
          "Aristotle (for N1c: what a liquidation step that reduced nothing can still have written); today: a reading of the source, and, on "
          "engine c030aa40 and program 3a690205 (before the guard), an engine copy that stops at N1c, N2, N3 and N4, at a failing "
          "header after every unchecked exit and at a loss booked after the payout snapshot: nothing fired in the engine suites "
          "(430 and 515 tests), in 2.3 million random action sequences, or in the program's suites (stateful 339, fuzz "
          "regressions 148, compute suite 1,592; its 14 failures were the funding-cadence test and 13 compute effects of the "
          "added checks). Not repeated on the revisions of this ledger")

# W: wrapper instruction handlers
wsrc = read(WRAPPER_SRC)
start = wsrc.index("pub enum Instruction")
body = wsrc[start:wsrc.index("\n    }\n", start)]
variants = re.findall(r"(?m)^        ([A-Z][A-Za-z0-9]*)\s*[{,(]?", body)
# what the instruction census found per instruction (wrapper_census.py -> specs/wrapper_shape_census_<rev>.md)
WRAPPER_REV = "ebcded58"
_census_rows = {}
_cpath = os.path.join(HERE, "specs", f"wrapper_shape_census_{WRAPPER_REV}.md")
if os.path.exists(_cpath):
    for line in open(_cpath):
        c = [x.strip() for x in line.strip().strip("|").split("|")]
        if len(c) == 5 and c[0] not in ("Instruction", "---") and not set(c[0]) <= set("-"):
            _census_rows[c[0]] = c
ERROR_EXCEPTIONS = {
    "PermissionlessCrank": "an engine error aborts the instruction, except `NonProgress` from the auto crank after the handler accrued the market, and then only if `insurance` and the account's positions are unchanged (X3)",
    "SyncMaintenanceFee": "an engine error aborts the instruction, except `LockActive` from the portfolio deregistration (X4)",
}
for i, v in enumerate(variants, 1):
    row = _census_rows.get(v)
    reach = ""
    if row:
        reach = f". Engine entry points its handler can reach: {row[2]}" + (f". Found when read: {row[4]}" if row[4] else "")
    piece(f"W{i:02d}", "W wrapper", v,
          f"`{v}`: accounts, signers and bindings are checked before any state change; token movement equals the engine's booked delta; "
          "if the global invariant (H, S, V, M, C) held before and the instruction succeeds, it holds after; "
          + ERROR_EXCEPTIONS.get(v, "an engine error aborts the instruction") + reach,
          "Aristotle (needs the wrapper to build under its compiler); today: SVM tests and source censuses")

# L: liveness, split into safety pieces
plans = ["NoAction", "RefreshAccount", "SettleBChunk", "Liquidate", "ReleaseSourceLiens",
         "AdvancePendingClose", "ExpireSourceBacking", "ResolvedClose", "RecoveryForfeit", "FinalizeReset"]
for i, p in enumerate(plans, 1):
    piece(f"L{i:02d}.rank", "L liveness", p,
          f"a successful `{p}` step strictly lowers a fixed rank of (market, account) state",
          "Aristotle (safety form of progress); today: Kani rank proofs and reachability model")
piece("L.enabled", "L liveness", "select_auto_crank_plan",
      "whenever the actionable summary is nonzero the selector returns a plan other than NoAction (K20, K23)",
      "Aristotle via K20/K23")
piece("L.terminates", "L liveness", "all loops", "every loop is bounded by a compile-time constant",
      "outside Aristotle (it proves no termination); bounded-loop audit and compute tests")

# ---------------------------------------------------------------- statuses from Aristotle
props = aristotle_properties("--offline" in sys.argv)
fn_to_k = {fn: k for k, fn in kernels.items()}
arith = {"widening_mul_u128_spec": ("D1", "proved"), "mul_u256_spec": ("D2", "proved"),
         "div_rem_u256_zero_spec": ("C1", "proved in part: the zero-divisor half"),
         "checked_div_rem_by_u256_zero_spec": ("D3", "proved in part: the zero-divisor half")}
arith_rows = {}
# Aristotle records the proofs made on NEW_REV in the namespace Spec.KernelC and keeps the rows of
# the earlier revision in Spec.Kernel. The new revision's rows are read last, so they decide.
props = sorted(props, key=lambda p: p["name"].startswith("Spec.KernelC."))
for p in props:
    name, st = p["name"], p["status"]
    on_new_rev = name.startswith("Spec.KernelC.")
    if on_new_rev:
        name_k = "Spec.Kernel." + name[len("Spec.KernelC."):]
        mx = re.match(r"Spec\.Kernel\.(\w+)_(exposure|progress|realization|monotone|minimum|inverse)$", name_k)
        if mx and (mx.group(1), mx.group(2)) in EXTRA_K:
            pid = EXTRA_K[(mx.group(1), mx.group(2))]
            if st.startswith("PROVED"):
                pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
                pieces[pid]["evidence"] = f"{name} (engine {NEW_REV})"
            elif st.startswith("REFUTED"):
                pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
            elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
                pieces[pid]["status"] = "in progress"
            elif st:
                pieces[pid]["status"] = "attempted, open"; pieces[pid]["evidence"] = name
            continue
    else:
        name_k = name
    m = re.match(r"Spec\.Kernel\.(\w+)_(contract|nopanic)$", name_k)
    generic = ""
    if m and m.group(1) not in fn_to_k and m.group(1).endswith("_u64") and m.group(1)[:-4] in fn_to_k:
        # a method of the generic market view, proved for the instantiation T = u64
        generic = "; proved for the instantiation T = u64 of the generic market view, the body does not use T"
        m = re.match(r"Spec\.Kernel\.(\w+)_u64_(contract|nopanic)$", name_k)
    elif m and m.group(1) not in fn_to_k and m.group(1).endswith("_generic") and m.group(1)[:-8] in fn_to_k:
        # the every-T statement of a method of the generic market view (b5 summary, K50 to K56)
        generic = ("; this row is the statement for every T, which assumes the instantiating crate's layouts and the "
                   "documented behaviour of the library calls; the b5 summary (b5_dl/output_summary.md) states the "
                   "result at T = u64 with no assumptions")
        m = re.match(r"Spec\.Kernel\.(\w+)_generic_(contract|nopanic)$", name_k)
    if m and m.group(1) in fn_to_k:
        pid = f"{fn_to_k[m.group(1)]}.{m.group(2)}"
        if pieces[pid]["status"] == NOT_DEPLOYED:
            pieces[pid]["evidence"] += "; Aristotle proved it on the fuzz-feature build (b4 summary)"
            continue
        if st.startswith("PROVED") and pid.split(".")[0] in REPROVE and not on_new_rev and not (REPROVE_SINCE and p["last_updated"] >= REPROVE_SINCE):
            pieces[pid]["status"] = f"proved for {PROVED_AT}; to be established again for {NEW_REV}"
            pieces[pid]["evidence"] = (f"{name} ({PROVED_AT}); the function's text is unchanged but it takes the asset record, "
                                       f"which gained two fields in {NEW_REV}")
        elif st.startswith("PROVED"):
            # A proof recorded by a task that was cancelled before its closing check is listed as
            # "unchecked" until a later task re-stamps it. It is not counted as proved here.
            at = f"engine {NEW_REV}" if (on_new_rev or pid.split(".")[0] in REPROVE) else f"{PROVED_AT}, unchanged in {NEW_REV}"
            pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
            pieces[pid]["evidence"] = f"{name} ({at}{generic})"
        elif st.startswith("REFUTED"):
            pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
        elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
            pieces[pid]["status"] = "in progress"
        elif st:
            pieces[pid]["status"] = "attempted, open"; pieces[pid]["evidence"] = name
    m = re.match(r"Spec\.WideMath\.(\w+)$", name)
    if m and m.group(1) in arith and st.startswith("PROVED"):
        pid, how = arith[m.group(1)]
        pieces[pid]["status"] = how; pieces[pid]["evidence"] = f"{name} ({PROVED_AT}; src/wide_math.rs is identical in {NEW_REV})"
    # rows named after a label of the arithmetic spec, e.g. Spec.WideMath.B3_checked_mul
    m = re.match(r"Spec\.WideMath\.([A-G]\d+b?)_\w+$", name)
    if m and m.group(1) in pieces:
        arith_rows.setdefault(m.group(1), []).append((name, st))

# What a label's rows establish when they do not establish the whole label (from the a2 summary),
# and the row of the later division task (a3) that completes each of them.
ARITH_PARTIAL = {
    "A2": ("proved in part: is_zero; the values of the three constants are open (they have no code of their own in the deployed build)",
           "Spec.WideMath.A2_u256_constants"),
    "A6": ("proved in part: is_zero, is_negative, is_positive, signum; the values of the five constants are open",
           "Spec.WideMath.A6_i256_constants"),
    "B9": ("proved in part: + - * << >> & | += -= with both panic directions; / and % belong to the division task",
           "Spec.WideMath.B9_div_rem_operators"),
}
ARITH_CORRECTED = {
    "A7": "the label as first written was wrong at I256::MIN: abs_u256 asserts self != MIN, so every execution there panics "
          "(proved); for every other x no execution panics and the result is |sval(x)| (proved)",
    "A2": "the constants are proved through a harness that compiles src/wide_math.rs unchanged and returns each constant (a3 summary)",
    "A6": "the constants are proved through a harness that compiles src/wide_math.rs unchanged and returns each constant (a3 summary)",
}
for pid, rows_ in arith_rows.items():
    if not all(st.startswith("PROVED") and "unchecked" not in st for _, st in rows_):
        pieces[pid]["status"] = "attempted, open"
        pieces[pid]["evidence"] = ", ".join(n for n, _ in rows_)
        continue
    earlier = pieces[pid]["evidence"].split(" (")[0] if pieces[pid]["evidence"].startswith("Spec.WideMath.") else ""
    ev = ", ".join(([earlier] if earlier and earlier not in [n for n, _ in rows_] else []) + [n for n, _ in rows_]) \
        + f" ({PROVED_AT}; src/wide_math.rs is identical in {NEW_REV})"
    if pid in ARITH_PARTIAL and ARITH_PARTIAL[pid][1] not in [n for n, _ in rows_]:
        pieces[pid]["status"] = ARITH_PARTIAL[pid][0]
    else:
        pieces[pid]["status"] = "proved"
        if pid in ARITH_CORRECTED:
            ev += "; " + ARITH_CORRECTED[pid]
    pieces[pid]["evidence"] = ev

# E4b is a question, not a claim: the spec file expects "does not return when F >= 2^255" to be false.
# It is settled when Aristotle refutes that statement with a counterexample AND proves what the function
# does instead (the E4 rows: outside its exact panic condition the result is F wrapped into I256).
e4b_refuted = any(p["name"] == "wide_signed_mul_div_floor.no_return_when_f_ge_2_255" and p["status"].startswith("REFUTED") for p in props)
if "E4b" in pieces and e4b_refuted and pieces.get("E4", {}).get("status") == "proved":
    pieces["E4b"]["status"] = "proved"
    pieces["E4b"]["statement"] = ("(settled as expected) `wide_signed_mul_div_floor` DOES return when F >= 2^255: outside its exact panic "
                                  "condition the result is F wrapped into I256, e.g. (2^255, 1, 1) returns I256::MIN; the engine does not call it")
    pieces["E4b"]["evidence"] = ("refuted with a counterexample: wide_signed_mul_div_floor.no_return_when_f_ge_2_255; the behaviour is proved "
                                 f"under E4 ({PROVED_AT}; a3 summary, reproducer sim/e4b_wide_signed_mul_div_floor)")

for pid, label in p_none:
    if pieces[label]["status"] == "proved":
        pieces[pid]["status"] = "proved"
        pieces[pid]["evidence"] = f"the callee never panics: {label} ({pieces[label]['evidence'].split(' (')[0]})"

# A proof made by a task that was cancelled keeps an "unchecked" flag in the listing even after a
# later task has run its final check. confirmed_final_checks.tsv names the pieces for which a
# later task's summary states that the final check passed, with the file that says so.
confirmed = {}
cf = os.path.join(HERE, "confirmed_final_checks.tsv")
if os.path.exists(cf):
    for line in open(cf):
        if line.strip() and not line.startswith("#"):
            pid, src_note = line.rstrip("\n").split("\t")
            confirmed[pid] = src_note
for pid, src_note in confirmed.items():
    if pid in pieces and pieces[pid]["status"] == "proved, final check pending":
        pieces[pid]["status"] = "proved"
        pieces[pid]["evidence"] += f"; final check confirmed in {src_note}"

# Layer T: `Spec.LayerT.tNN_header` is the statement of TNN.header (the entry point preserves the
# header checks). Batch t1 (task 18dfda03, engine c030aa40) proved T09 and T38 and was stopped at its
# spending cap with the other nine in progress; it wrote no summary, so a proved row whose final
# check is not recorded stays "final check pending".
for p in props:
    mt = re.match(r"Spec\.LayerT\.t(\d+)_header$", p["name"])
    if not mt:
        continue
    pid = f"T{int(mt.group(1)):02d}.header"
    if pid not in pieces or pieces[pid]["status"] == NOT_DEPLOYED:
        continue
    st = p["status"]
    if st.startswith("PROVED"):
        pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
        pieces[pid]["evidence"] = f"{p['name']} (engine {NEW_REV}, batch t1)"
    elif st.startswith("REFUTED"):
        pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = p["name"]
    elif st:
        # The service lists a property that a stopped task left unfinished as IN_PROGRESS again as soon
        # as any later task starts. No task after t1 was asked for these.
        pieces[pid]["status"] = "attempted, open"
        pieces[pid]["evidence"] = f"{p['name']}: batch t1 was stopped before this one finished"

# Kernels of the refactor that Kani checks only for bounded inputs and Aristotle proved at full width
# (tasks k1 on a3efa566, k2 on f24a561c, k3 on bfd4d0c4; the functions are textually unchanged in LANDED_REV).
BOUNDED_ROWS = {
    "Spec.KernelB.kernel_b_settlement_chunk_contract": ("kernel_b_settlement_chunk", "contract", "k1", "a3efa566", None),
    "Spec.KernelB.kernel_b_settlement_chunk_nopanic": ("kernel_b_settlement_chunk", "nopanic", "k1", "a3efa566", None),
    "Spec.KernelB.kernel_b_settlement_chunk_progress": ("kernel_b_settlement_chunk", "progress", "k1", "a3efa566",
        "a step with something to settle makes progress"),
    "Spec.KernelB.kernel_b_settlement_chunk_refusals": ("kernel_b_settlement_chunk", "refusals", "k1", "a3efa566",
        "the exact conditions of its RecoveryRequired and ArithmeticOverflow refusals"),
    "Spec.Accrual.canonical_accrual_price_step_v16_contract": ("canonical_accrual_price_step_v16", "contract", "k2", "f24a561c", None),
    "Spec.Accrual.canonical_accrual_price_step_v16_nopanic": ("canonical_accrual_price_step_v16", "nopanic", "k2", "f24a561c", None),
    "Spec.Accrual.canonical_accrual_price_step_v16_envelope": ("canonical_accrual_price_step_v16", "envelope", "k2", "f24a561c",
        "over any chain of steps toward a fixed target, the price moves at most n * cap_anchor * max_change_bps + r0 basis-point units "
        "and never past the target"),
    "Spec.Accrual.canonical_accrual_price_step_v16_reaches_of_valid": ("canonical_accrual_price_step_v16", "reaches", "k2", "f24a561c",
        "the price reaches the target once the allowance covers the distance"),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_contract": ("kernel_bankruptcy_residual_step_capacity", "contract", "k3", "bfd4d0c4", None),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_nopanic": ("kernel_bankruptcy_residual_step_capacity", "nopanic", "k3", "bfd4d0c4", None),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_fits": ("kernel_bankruptcy_residual_step_capacity", "fits", "k3", "bfd4d0c4",
        "booking the returned step never overflows the B index"),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_zero": ("kernel_bankruptcy_residual_step_capacity", "zero", "k3", "bfd4d0c4",
        "the step is 0 exactly when booking one atom would overflow the B index"),
    "Spec.KR.kernel_bankruptcy_residual_step_capacity_books": ("kernel_bankruptcy_residual_step_capacity", "books", "k3", "bfd4d0c4",
        "under the shape facts and the chunk bound, a nonzero step is always booked (no Recovery while the index has room)"),
}
FUNDING_ROWS = {  # task k3: the funding snapshot carried between settlements (settlement glue, not a listed kernel)
    "Spec.KF.scaled_adl_delta_fast_delta": ("KF.delta", "scaled_adl_delta_fast",
        "the fast funding delta is the exact floor of abs_basis * (now - then) / (a_basis * POS_SCALE)"),
    "Spec.KF.f_snap_after_settlement_nopanic": ("KF.nopanic", "f_snap_after_settlement", "no execution panics"),
    "Spec.KF.f_snap_after_settlement_carry_of_pos": ("KF.carry", "f_snap_after_settlement",
        "with the exact delta, the unpaid remainder is worth less than one atom and is never a debt"),
    "Spec.KF.f_snap_after_settlement_ok_exact": ("KF.ok", "f_snap_after_settlement",
        "with the exact delta, the exact set of inputs it refuses (its self-check never fires)"),
    "Spec.KF.f_snap_after_settlement_cadence": ("KF.cadence", "f_snap_after_settlement",
        "settling in n steps never credits an account more or charges it less than settling once, and the difference is "
        "bounded by the rounding of each step (exact when abs_basis divides den)"),
}
_k_of = {fn: k for k, fn in kernels.items()}
for p in props:
    row = BOUNDED_ROWS.get(p["name"])
    if row is not None and row[0] in _k_of:
        fn, suffix, task, rev, statement = row
        pid = f"{_k_of[fn]}.{suffix}"
        if pid not in pieces:
            piece(pid, "K kernel", fn, f"`{fn}`: {statement}, for every input", "Aristotle")
        if p["status"].startswith("PROVED"):
            pieces[pid]["status"] = "proved, final check pending" if "unchecked" in p["status"] else "proved"
            pieces[pid]["evidence"] = f"{p['name']} (engine {rev}, task {task}; the function is unchanged in {LANDED_REV})"
        continue
    frow = FUNDING_ROWS.get(p["name"])
    if frow is not None:
        pid, fn, statement = frow
        if pid not in pieces:
            piece(pid, "S settlement glue", fn, f"`{fn}`: {statement}", "Aristotle")
        if p["status"].startswith("PROVED"):
            pieces[pid]["status"] = "proved, final check pending" if "unchecked" in p["status"] else "proved"
            pieces[pid]["evidence"] = f"{p['name']} (engine bfd4d0c4, task k3; the function is unchanged in {LANDED_REV})"
# A closing task's final check of these pieces (the listing keeps the cancelled task's "unchecked" flag):
# confirmed_final_checks.tsv is read above, before these pieces exist, so apply it again here.
for _pid, _src in confirmed.items():
    if _pid in pieces and pieces[_pid]["status"] == "proved, final check pending":
        pieces[_pid]["status"] = "proved"
        pieces[_pid]["evidence"] += f"; final check confirmed in {_src}"
# Config admission keeps every B-settlement step clear of its overflow refusal (Kani, every chunk and budget),
# which with K91.refusals means no admitted market can freeze positions that way.
if "kernel_b_settlement_chunk" in _k_of:
    _pid = f"{_k_of['kernel_b_settlement_chunk']}.admission"
    piece(_pid, "K kernel", "validate_public_user_fund_shape",
          "every public B chunk that config admission accepts keeps the B-settlement step's largest numerator inside u128, so "
          "(with the exact refusal conditions) no admitted market's B settlement fails with ArithmeticOverflow", "Kani")
    pieces[_pid]["status"] = "verified by Kani"
    pieces[_pid]["evidence"] = f"proof_admitted_public_b_chunk_keeps_b_settlement_free_of_overflow (Kani, every chunk and budget), engine {LANDED_REV}"
# INV-045 (reworded in program a406ce3b): what one accrual segment does with and without effective open
# interest. Kani lemmas in engine c65bb9a1 (src/v16_proofs.rs, refactor batch 10: proofs only, the deployed code is
# LANDED_REV's); the effective-price writers are listed in specs/inv045_effective_price_writers_80f8f0b4.md.
for _pid, _target, _text, _proof, _over in [
    ("AC.unexposed", "accrual_activity_for_asset_segment",
     "with no effective open interest on either side, a segment of either accrual path is neither price-move nor "
     "funding active, and the funding amount it books is zero",
     "proof_unexposed_accrual_segment_moves_no_equity", "every asset state, segment length, price and rate"),
    ("AC.exposed", "accrual_activity_for_asset_segment",
     "with effective open interest on either side, a segment that moves the effective price is equity active, so "
     "the single-segment accrual runs the price-move cap (K76) before it books the move",
     "proof_exposed_price_move_is_equity_active", "every asset state, segment length, price and rate"),
    ("AC.nofunding", "kernel_accrual_index_update",
     "a zero funding amount leaves both funding indices unchanged; only the K indices can move",
     "proof_index_update_without_funding_leaves_funding_indices",
     "closure layer, every K and F index, A index and price move"),
    ("AC.residue", "side_adl_totals_are_coherent",
     "a side the per-asset scan admits with raw basis and no effective open interest has A index 1, so its legs "
     "carry at most raw_basis_sum / ADL_ONE quantity units (the residue of engine issue #132)",
     "proof_scan_side_with_basis_and_no_open_interest_has_unit_a_index", "every raw basis, A index, weight and mode"),
]:
    piece(_pid, "S settlement glue", _target, f"`{_target}`: {_text}", "Kani")
    pieces[_pid]["status"] = "verified by Kani"
    pieces[_pid]["evidence"] = f"{_proof} (Kani, {_over}), engine c65bb9a1"
piece("AC.clamp", "W wrapper", "oracle_v16::clamp_toward_engine_dt",
      "`oracle_v16::effective_price_from_target` / `clamp_toward_engine_dt` (program): the crank's price is the clamp "
      "when the asset has effective open interest and the target when it has none; with a nonzero anchor and target "
      "the clamp lies between them and moves at most anchor * cap * dt / 10000, the bound the engine's price-move cap "
      "enforces; a trade-reported price goes through the same clamp", "Aristotle")
pieces["AC.clamp"]["evidence"] = ("queued: Aristotle task k4 Part 4 (KC.nopanic, KC.envelope, KC.reaches, KC.select) on the "
                                  f"verbatim functions of program {WRAPPER_REV}; model checking gave no verdict, even for "
                                  "16-bit caps and segment lengths")
# C7 of the closure map: one jump and any chain of steps to the same price move the K indices alike
_k78 = _k_of.get("kernel_accrual_index_update")
if _k78 is not None:
    piece(f"{_k78}.path", "K kernel", "kernel_accrual_index_update",
          "`kernel_accrual_index_update`: on Ok the K indices move by exactly price_delta * a_long and "
          "-(price_delta * a_short), and two successive updates with the same A indices move them by the sum of the "
          "price deltas, so a jump and any chain of steps to the same price leave the same K, for every input", "Aristotle")
    pieces[f"{_k78}.path"]["evidence"] = "queued: Aristotle task k4 (KI.path)"
# C0 of the closure map: the price range both accrual entry points accept
piece("AC.price_range", "S settlement glue", "accrue_asset_to_not_atomic",
      "`accrue_asset_to_not_atomic` and `accrue_asset_path_to_not_atomic` refuse the prices 0, MAX_ORACLE_PRICE + 1 and "
      "u64::MAX with InvalidConfig and leave the header and the asset slot unchanged (so a raw target of 0 is never "
      "accepted as a mark); the guard is the single comparison `price == 0 || price > MAX_ORACLE_PRICE`, so every "
      "price above the range takes the same branch", "Kani")
# bounded evidence (three concrete prices, one fixture): by the ledger's rule it stays open
pieces["AC.price_range"]["evidence"] = ("bounded Kani check proof_v16_public_accrual_refuses_a_price_outside_the_oracle_range_and_writes_nothing "
                                        "(the prices 0, MAX_ORACLE_PRICE + 1 and u64::MAX; every slot, funding rate, flag and "
                                        "step slice of up to one step; one-market fixture), engine 3f56c204; every price and "
                                        "every market state is not machine-checked")

# The trade pilot (task s2, engine d1fa39f6): the prover's names for the pieces TF, TA, TB and TC1-TC9.
TRADE_ROWS = {
    "Spec.Trade.tf_margin": "TF.margin", "Spec.Trade.tf_locked": "TF.locked", "Spec.Trade.tf_frame": "TF.frame",
    "finish_trade_checks_not_atomic.order": "TF.order",
    "apply_trade_after_refresh_not_atomic.fee": "TA.fee", "apply_trade_after_refresh_not_atomic.conservation": "TA.conservation",
    "apply_trade_after_refresh_not_atomic.positions": "TA.positions", "apply_trade_after_refresh_not_atomic.gate": "TA.gate",
    "apply_trade_after_refresh_not_atomic.frame": "TA.frame", "apply_trade_after_refresh_not_atomic.nopanic": "TA.nopanic",
    "Spec.Trade.tb_flag": "TB.flag", "Spec.Trade.tb_checked": "TB.checked",
    "Spec.Trade.tc1_charge_fee": "TC1", "Spec.Trade.tc2_preflight": "TC2", "Spec.Trade.tc3_risk_change": "TC3",
    "Spec.Trade.tc4_terminal_residual": "TC4", "Spec.Trade.tc5_position_delta": "TC5",
    "transfer_trade_residual_reward_credit.frame": "TC6", "Spec.Trade.tc7_recertify": "TC7",
    "Spec.Trade.tc8_oi_cap_gate": "TC8", "Spec.Trade.tc9_im_lien": "TC9",
}
_trade_unmapped = []
for p in props:
    name, st = p["name"], p["status"]
    pid = TRADE_ROWS.get(name)
    if pid is None:
        if name.startswith("Spec.Trade.") or name.startswith(("apply_trade_after_refresh_not_atomic.", "finish_trade_checks_not_atomic.",
                                                               "execute_batch_with_fee_loss_stale_scoped_not_atomic.")):
            _trade_unmapped.append(name)
        continue
    if st.startswith("PROVED"):
        pieces[pid]["status"] = "proved, final check pending" if "unchecked" in st else "proved"
        pieces[pid]["evidence"] = f"{name} (engine {LANDED_REV}, task s2)"
    elif st.startswith("REFUTED"):
        pieces[pid]["status"] = "REFUTED"; pieces[pid]["evidence"] = name
    elif st.startswith("IN_PROGRESS") or st.startswith("IN PROGRESS"):
        pieces[pid]["status"] = "in progress"
    elif st:
        pieces[pid]["status"] = "attempted, open"
        pieces[pid]["evidence"] = f"{name}: stated, not proved in task s2 (aristotle/summaries/s2.md says how far each got)"
if _trade_unmapped:
    print("trade rows without a ledger piece:", ", ".join(sorted(_trade_unmapped)), file=sys.stderr)

# The three provider-protection kernels are small scalar functions that Kani verifies for all inputs.
# They are not queued for Aristotle (the owner's priority: Aristotle goes where Kani has gaps).
KANI_FULL_WIDTH = {
    "K61.contract": "contract_check_kernel_provider_protection_num (Kani, every u128 input)",
    "K61.nopanic": "contract_check_kernel_provider_protection_num (Kani, every u128 input; a panic would fail it)",
    "K62.contract": "contract_check_kernel_provider_protection_floor_observe (Kani, every u128 input)",
    "K62.nopanic": "contract_check_kernel_provider_protection_floor_observe (Kani, every u128 input; a panic would fail it)",
    "K63.contract": "contract_check_kernel_provider_protection_floor_shift (Kani, every input)",
    "K63.nopanic": "contract_check_kernel_provider_protection_floor_shift (Kani, every input; a panic would fail it)",
    "K61.realization": "proof_v16_provider_protection_kernel_laws (Kani, every u128 input)",
    "K61.monotone": "proof_v16_provider_protection_kernel_laws (Kani, every u128 input)",
    "K62.minimum": "proof_v16_provider_protection_floor_is_a_running_minimum (Kani, every u128 input, two observations in both "
                   "orders; any longer sequence follows by induction)",
    "K63.inverse": "proof_v16_provider_flows_move_the_floor_with_the_principal (Kani, 64-bit amounts)",
}
for _k, _fn in REFACTOR_KERNELS.items():
    if _k in BOUNDED_KERNELS:
        continue
    KANI_FULL_WIDTH[f"{_k}.contract"] = f"contract_check_{_fn} (Kani, every input)"
    KANI_FULL_WIDTH[f"{_k}.nopanic"] = f"contract_check_{_fn} (Kani, every input; a panic would fail it)"
for pid, ev in KANI_FULL_WIDTH.items():
    if pieces[pid]["status"] == "open":
        pieces[pid]["status"] = "verified by Kani"
        pieces[pid]["evidence"] = ev + f", engine {LANDED_REV}"

# Carry-over to LANDED_REV. Layers A and K: nothing has to be proved again (see the note at LANDED_REV).
# The entry-point pieces of REESTABLISH were proved for NEW_REV and are not counted for LANDED_REV.
for pid, p in pieces.items():
    st = p["status"]
    if not st.startswith("proved"):
        continue
    if pid in REESTABLISH:
        p["status"] = f"proved for {NEW_REV}; to be established again for {LANDED_REV}"
        p["evidence"] += (f"; the function's text is unchanged in {LANDED_REV}, but it is a method over, or builds, a type that "
                          f"gained fields there (specs/changed_since_{NEW_REV}_at_{LANDED_REV}.md)")
    elif p["layer"].startswith("K") and "Spec.Kernel" in p["evidence"]:
        p["evidence"] += f"; unchanged in {LANDED_REV} (specs/changed_since_{NEW_REV}_at_{LANDED_REV}.md)"
    elif p["layer"].startswith("A") and "src/wide_math.rs is identical" in p["evidence"]:
        p["evidence"] += f" and in {LANDED_REV}"

# ---------------------------------------------------------------- invariants -> pieces
def expand_k(text):
    out = []
    for a, b in re.findall(r"K(\d+)(?: to K(\d+))?", text):
        lo, hi = int(a), int(b or a)
        out += [f"K{n:02d}" for n in range(lo, hi + 1)]
    return sorted(set(out))

arith_ids = [p for p in pieces if pieces[p]["layer"].startswith("A")]
group = lambda letters: [p for p in arith_ids if p[0] in letters]
extra = {  # pieces the coverage matrix names only in words
    "INV-030": group("D"), "INV-038": group("G"), "INV-083": group("ABCDEF"), "INV-085": group("ABCDEF"),
    "INV-048": ["K59.contract", "K59.nopanic"], "INV-051": ["K59.contract", "K59.nopanic", "K60.contract", "K60.nopanic"],
}
for _inv, _ids in {"INV-045": ["K76.contract", "K76.nopanic", "K93.contract", "K93.nopanic", "K93.envelope", "K93.reaches"],
                   "INV-052": ["KF.cadence", "KF.carry", "KF.delta"],
                   "INV-071": ["K88.contract", "K88.zero", "K88.books"],
                   "INV-073": ["K88.books", "K91.refusals", "K91.admission"]}.items():
    extra[_inv] = extra.get(_inv, []) + [i for i in _ids if i in pieces]
extra["INV-045"] = extra.get("INV-045", []) + [i for i in ("AC.unexposed", "AC.exposed", "AC.nofunding", "AC.residue",
                                                         "AC.clamp", "AC.price_range") if i in pieces]
if "kernel_accrual_index_update" in _k_of:
    extra["INV-045"] += [i for i in (f"{_k_of['kernel_accrual_index_update']}.contract",
                                     f"{_k_of['kernel_accrual_index_update']}.path") if i in pieces]
liveness = {"INV-046", "INV-057", "INV-069", "INV-071", "INV-072", "INV-073", "INV-078", "INV-082"}
invariants = []
for line in read("coverage_matrix.md").splitlines():
    m = re.match(r"\| (INV-\d+) ([^|]*)\| ([^|]*)\| ([^|]*)\| ([^|]*)\|", line)
    if not m:
        continue
    inv, name, kind, stage, what = (x.strip() for x in m.groups())
    ids = []
    for k in expand_k(what):
        ids += [f"{k}.contract", f"{k}.nopanic"]
    ids += extra.get(inv, [])
    stages = [s.strip() for s in stage.split(",")]
    if "D" in stages:
        ids.append("T*")
    if "C" in stages:
        ids.append("W*")
    if inv in liveness:
        ids += ["L*"]
    invariants.append(dict(inv=inv, name=name, kind=kind, stage=stage, what=what, ids=ids))

CLOSE_HOW = {
    "safety": "write the invariant as a predicate on states or on single transitions; derive it from the global invariant (layer G) and "
              "the pieces listed for it (Aristotle)",
    "history": "write the invariant as a statement about two executions (two orders, two routes, a split and its whole); prove it "
               "from the kernels' algebra (Aristotle)",
    "liveness": "write the invariant as: a rank that every successful step lowers, and a step that is enabled in every non-terminal "
                "state (Aristotle); termination itself is evidence (Y.terminates, Y.compute)",
    "compute": "evidence: max-shape compute measurements (Y.compute)",
    "process": "evidence about the verification itself; closed when the ledger shows it (no theorem)",
    "n/a": "prove that the feature has no reachable consumer in the deployed program (Aristotle)",
}
def close_how(kind):
    parts = [k.strip() for k in kind.replace(" + ", ",").split(",")]
    return "; ".join(CLOSE_HOW.get("liveness" if k == "termination" else k, CLOSE_HOW["safety"]) for k in parts)
for v in invariants:
    num = v["inv"].split("-")[1]
    needs = ", ".join(sorted({i.split(".")[0] for i in v["ids"]})) or "no lower piece named yet"
    piece(f"I{num}", "I invariants closed", f"{v['inv']} {v['name']}",
          f"{v['inv']} ({v['kind']}) holds for the deployed program, as a formal statement that an independent reader has checked "
          f"against the charter text. Rests on: {needs}",
          close_how(v["kind"]))

def counts(ids):
    concrete = [i for i in ids if not i.endswith("*")]
    done = sum(1 for i in concrete if pieces[i]["status"] in ("proved", "verified by Kani"))  # as the roadmap's DONE
    return done, len(concrete)

# ---------------------------------------------------------------- write
by_layer = {}
for p in pieces.values():
    by_layer.setdefault(p["layer"], []).append(p)
total_proved = sum(1 for p in pieces.values() if p["status"] == "proved")
total_pending = sum(1 for p in pieces.values() if p["status"] == "proved, final check pending")
now = datetime.datetime.utcnow().strftime("%Y-%m-%d")  # the date only: a publish with no change leaves no diff
md = []
md.append("# Proof ledger: the charter's invariants as provable pieces\n")
md.append(f"Generated {now} by `make_ledger.py` from the live Aristotle property list. "
          f"**{total_proved} of {len(pieces)} pieces are proved and checked; {total_pending} more are proved "
          f"with the final check pending** (their task was stopped before its closing check).\n")
md.append("## The end goal\n")
md.append("""Every one of the charter's 89 invariants is **closed** for the revision that is deployed (layer I): it has a
formal statement that an independent reader has checked against the charter text, and that statement
is proved from the layers below it, or, for the few that are about compute or about the verification
itself, is established by the evidence named for it. The layers below are what such a proof needs:

- **G, the global invariant.** H (the header checks), S (the per-asset scan), V (every portfolio
  account valid against the market), M (market totals equal the sums over all accounts) and C
  (custody: the vault token accounts hold what the header says). It holds initially and every
  successful instruction preserves it.
- **W, the program's instructions** preserve G and check authority; **T, the engine's entry points**
  do the same one level down, in four halves (header, scan, account, mirror); **X** covers the
  places where state is kept after an error; **K, A and P** are the kernels, the arithmetic and the
  absence of panics under them.
- **L, liveness**, as ranks and enabledness.
- **Y, what is assumed** (the Solana runtime, the token program, the oracle formats, the compiler,
  the prover) **or shown by evidence** (compute bounds, termination, the review of the statements).

The count at the top of this file is against that whole list. See `ROADMAP.md` for the order.
""")
md.append("## How an invariant is split\n")
md.append("""1. **State predicate.** A safety invariant is a predicate on engine state. The engine already carries
   most of them as executable checks (`validate_shape`, `validate_with_market`, the audit scans).
2. **One obligation per public entry point (layer T).** If the predicate held before the call and the
   call returns Ok, it holds after. The engine has %d public entry points, so each invariant becomes at
   most %d obligations, most of them "this call does not write those fields".
3. **Kernel contracts (layer K).** Each entry point does its arithmetic and bookkeeping through small
   pure kernels with a written contract. An obligation of layer T follows from the contracts of the
   kernels it calls. Two properties per kernel: its contract, and no panic.
4. **Arithmetic (layer A).** Kernels that multiply and divide rest on `wide_math.rs`.
5. **Wrapper (layer W).** Each Solana instruction checks its accounts, calls the engine and moves
   tokens by exactly what the engine booked.
6. **Liveness (layer L).** Aristotle proves no termination, so a liveness invariant is split into two
   safety pieces it can take (every successful step lowers a rank; a step is enabled whenever the state
   is not terminal) and one it cannot (loops are bounded), which stays with the bounded-loop audit.

A piece is **proved** only when Aristotle reports it proved and checked against the deployed function
body. Kernel proofs name every library function whose documented behaviour they assume; those are
listed in the run summaries (`b1f_extract`, `b2_dl`, ...). Two kinds of piece are counted as proved
on other evidence, and say so in their row: a call site whose callee Aristotle proved never to panic
(nothing is left to prove at the site), and `P.surface`, which the Rust compiler establishes (the
engine builds against a stand-in module that exposes only the listed items).

**Revisions.** The ledger describes engine %s (the provider-protection floor, percolator#271) and
program %s, which pins an engine commit with the same `src/`. Proofs were made against two earlier
engine revisions, %s and %s (the #449 change). A proof holds for a later revision wherever the
function, everything it calls and every type it names are textually identical; `changed_fns.py`
checks that function by function, and `src/wide_math.rs` is identical in all three.

- From %s to %s (`specs/changed_since_add1d027_at_c030aa40.md`): nine kernels do not pass the check
  (two are new, four changed, three take the asset record, which grew) and are counted as proved only
  once Aristotle has proved them on %s.
- From %s to %s (`specs/changed_since_c030aa40_at_d1fa39f6.md`): every kernel passes. Three kernels
  are new (K61 to K63, the provider-protection floor). The two entry-point pieces proved on %s
  (T09.header and T38.header) do not pass, because the market view and the per-asset slot gained
  fields; they are listed as proved for %s and to be established again.
""" % (len(entries), len(entries), LANDED_REV, WRAPPER_REV, PROVED_AT, NEW_REV, PROVED_AT, NEW_REV, NEW_REV, NEW_REV, LANDED_REV, NEW_REV, NEW_REV))
md.append("## Totals\n")
md.append("| Layer | Pieces | Not in the deployed build | Proved and checked | Proved, final check pending | In progress or attempted | Open |\n|---|---|---|---|---|---|---|")
for layer in sorted(by_layer):
    ps = by_layer[layer]
    nd = sum(1 for p in ps if p["status"] == NOT_DEPLOYED)
    pr = sum(1 for p in ps if p["status"] == "proved")
    pe = sum(1 for p in ps if p["status"] == "proved, final check pending")
    op = sum(1 for p in ps if p["status"] == "open")
    md.append(f"| {layer} | {len(ps)} | {nd} | {pr} | {pe} | {len(ps) - nd - pr - pe - op} | {op} |")
md.append("")
kinds = {}
for e, k in t_kind.items():
    kinds.setdefault(k, []).append(e)
md.append("Layer T has two pieces per entry point. `.header`: the production shape check (header totals and "
          "flags). Each deployed entry point was read path by path (classes and what was found are in "
          "`specs/transition_shape_spec_<rev>.md`). By class: " + "; ".join(f"{k}: {len(v)}" for k, v in sorted(kinds.items()))
          + ". Classes final and delegates reduce to \"every Ok path ends in the runtime check\"; local and frame are "
          "small proofs; real, the unchecked paths of the mixed ones and the dispatcher carry the proofs that matter. "
          "Since program #485 the program checks the header before every commit of a crank and of a RebalanceReduce, "
          "so for those two instructions the header half also holds by a runtime check. `.scan`: the per-asset scan "
          "that only audit builds run; every entry point carries a real proof for it.\n")
md.append("## Each invariant and its pieces\n")
md.append("`T*`, `W*` and `L*` mean every piece of that layer applies (frame obligations included).\n")
md.append("| Invariant | Kind | Pieces | Pieces done (proved, or verified by Kani) | Closed |\n|---|---|---|---|---|")
for v in invariants:
    done, n = counts(v["ids"])
    ids = v["ids"]
    short = ", ".join(sorted({i.split(".")[0] for i in ids})) if ids else "none yet: " + v["what"]
    closed = pieces["I" + v["inv"].split("-")[1]]["status"]
    md.append(f"| {v['inv']} {v['name']} | {v['kind']} | {short} | {done} of {n} | {'yes' if closed == 'proved' else 'no'} |")
md.append("")
for layer in sorted(by_layer):
    md.append(f"## Layer {layer}\n")
    md.append("| Piece | Target | Statement | Status | Evidence |\n|---|---|---|---|---|")
    for p in by_layer[layer]:
        md.append(f"| {p['id']} | `{p['target']}` | {p['statement']} | {p['status']} | {p['evidence']} |")
    md.append("")
open(os.path.join(HERE, "proof_ledger.md"), "w").write("\n".join(md) + "\n")
with open(os.path.join(HERE, "proof_ledger.tsv"), "w") as f:
    f.write("id\tlayer\ttarget\tstatus\tevidence\tmethod\tstatement\n")
    for p in pieces.values():
        f.write("\t".join(p[k] for k in ("id", "layer", "target", "status", "evidence", "method", "statement")) + "\n")
total_nd = sum(1 for p in pieces.values() if p["status"] == NOT_DEPLOYED)
print(f"{len(pieces)} pieces ({total_nd} not in the deployed build), {total_proved} proved and checked, {total_pending} proved with final check pending; {len(invariants)} invariants; "
      f"{len(entries)} entry points, {len(variants)} instructions")
for layer in sorted(by_layer):
    ps = by_layer[layer]
    print(f"  {layer}: {sum(1 for p in ps if p['status']=='proved')}/{len(ps)} proved")
