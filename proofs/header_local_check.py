#!/usr/bin/env python3
"""header_local_check.py <engine src dir>

The facts the header harnesses of class "local" rest on (src/v16_proofs.rs, the section after the
marker "Header pieces of class \"local\""), checked on the source. Stops at the first that fails.

1. Each memo stand-in's key is the whole input of the function it replaces, and nothing else feeds
   that function:
   - `MarketGroupV16HeaderAccount::validate_dynamic_market_slots_len_static`, `V16Core::
     amount_from_bound_num` and `V16Core::bound_num_from_amount` take exactly the arguments the
     key holds, by value, and no `self`;
   - `V16ConfigAccount::try_to_runtime_shape` takes only `&self`, a `Pod` type (key: its bytes);
   - `V16Config::validate_public_user_fund_shape` takes only `&self`, whose `PartialEq` is derived
     (key: the value);
   - `MarketGroupV16View::validate_header_aggregate_totals` takes only `&self` and reads exactly
     the nine header fields the stand-in's key lists (in its order), and uses `self` no other way;
   - the deployed sources declare no `static` (no other state a function can read).
2. The audit-only scan: its `cfg(any(test, kani, feature = "audit-scan"))` block in
   `MarketGroupV16View::validate_shape` holds exactly `self.validate_shape_full_audit_scan()?;`, so
   a stand-in that returns `Ok(())` gives the default build's predicate; the stand-in does.
3. What runs as written differs in no way between a Kani build and a deployed build: cfg_reach.py,
   over each group of harnesses (its roots and its stand-ins), reports only the block of 2 where the
   group evaluates the predicate, and nothing else. The account stand-ins of T29 and T47 replace two
   functions that take the account and the market by shared reference (no interior mutability in
   the crate), so they write nothing.
4. The entry points read no slot of the view (no `.markets`), so the slot count enters the proof
   only through the memoized slot-count check. The two accruals use the slice only in their entry
   guard (`asset_index >= self.markets.len()`) and pass the index only to the replaced accessors
   and to their core, so the harness's one slot stands for any admitted index of any slot count.
   The path accrual (T01; these facts are checked when its harnesses are in the section) refuses,
   before any write, a slice of steps whose length is not the expected one, which is at most
   `V16_MAX_ACCRUAL_PATH_STEPS` (32): its harnesses, one per length from 0 to 32 (each length
   exactly once, with a slice of exactly that many arbitrary steps), cover every Ok. The replaced accrual kernels and the price step take their inputs by value. T33 (the side reset's finalization) reads no slot either, and passes the asset index only
   to `validate_configured_asset_index`, which runs as written and only refuses an index beyond the
   slice or the configured slots, to its pending-barrier read and to the asset accessors (both
   replaced), so the one slot stands for every slot count there too. The reads replaced by stand-ins
   with any result (`asset_state`, `require_asset_accruable`, `pending_domain_loss_barrier_count`)
   take the view by shared reference, so they write nothing.
5. The frame of `set_asset_state`, which the stand-in `frame_set_asset_state` gives the accruals and
   T33 (any bytes in every slot, any resolved-payout blocker count, any result), holds for every
   slot count, not only the one slot of its harness: after refusing an index beyond the configured
   slots or the slice, the function touches the slice only at `self.markets[asset_index]`, reads
   the header nowhere else, and writes it only through `update_resolved_payout_blocker_total`,
   which writes `resolved_payout_blocker_count` alone, from an associated function of the old
   count and its two arguments.
"""
import os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
VIEWMUT = "impl<'a, T> MarketGroupV16ViewMut<'a, T> {"
HEADER = "impl MarketGroupV16HeaderAccount {"
ENTRY = {"T09.header": ("advance_resolved_slot_not_atomic", VIEWMUT),
         "T05.header": ("activate_empty_market_slot_not_atomic", HEADER),
         "T03.header": ("activate_empty_asset_slot_not_atomic", HEADER),
         "T29.header": ("deregister_empty_materialized_portfolio_not_atomic", VIEWMUT),
         "T47.header": ("register_empty_materialized_portfolio_not_atomic", VIEWMUT),
         "T38.header": ("grow_asset_slot_capacity_not_atomic", HEADER),
         "T02.header": ("accrue_asset_to_not_atomic", VIEWMUT),
         "T01.header": ("accrue_asset_path_to_not_atomic", VIEWMUT),
         "T33.header": ("finalize_side_reset_not_atomic", VIEWMUT)}
# the accruals use the slice of slots in their entry guard only, and run a core as written
ACCRUAL = {"accrue_asset_to_not_atomic": "accrue_asset_to_admitted_not_atomic",
           "accrue_asset_path_to_not_atomic": "accrue_asset_path_to_admitted_not_atomic"}
ACCRUAL_STUBS = ["asset_state", "require_asset_accruable", "set_asset_state"]
PURE_STUBS = {"kernel_accrual_funding_num": "impl V16Core {", "kernel_accrual_index_update": "impl V16Core {",
              "kernel_accrual_price_move_within_cap": "impl V16Core {", "canonical_accrual_price_step_v16": None}
# T33 passes the asset index only to its index check (run as written) and to replaced callees
T33_INDEX_CALLEES = ["validate_configured_asset_index", "pending_domain_loss_barrier_count", "asset_state",
                     "set_asset_state"]
# reads replaced by stand-ins with any result
READ_STUBS = ["asset_state", "require_asset_accruable", "pending_domain_loss_barrier_count"]
MEMOS = ["validate_shape_full_audit_scan", "validate_dynamic_market_slots_len_static", "amount_from_bound_num",
         "bound_num_from_amount", "validate_header_aggregate_totals", "validate_public_user_fund_shape"]
# (roots, stubbed, the build differences allowed) of each group of harnesses
GROUPS = {
    "T09, T05, T03, T29, T47": (["validate_shape", "advance_resolved_slot_not_atomic",
                                 "activate_empty_market_slot_not_atomic", "activate_empty_asset_slot_not_atomic",
                                 "deregister_empty_materialized_portfolio_not_atomic",
                                 "register_empty_materialized_portfolio_not_atomic"],
                                MEMOS + ["try_to_runtime_shape", "validate_with_market",
                                         "is_empty_for_dematerialization"], 1),
    "T38 (1)": (["validate_shape", "grow_asset_slot_capacity_not_atomic"], MEMOS, 1),
    "T38 (2)": (["grow_asset_slot_capacity_not_atomic", "validate_dynamic_market_slots_len_static"],
                ["validate_public_user_fund_shape"], 0),
    "T02, T01": (["validate_shape", *ACCRUAL], MEMOS + ["try_to_runtime_shape", *ACCRUAL_STUBS, *PURE_STUBS], 1),
    "the frame of set_asset_state": (["set_asset_state"], [], 0),
    "T33": (["validate_shape", "finalize_side_reset_not_atomic"],
            MEMOS + ["try_to_runtime_shape", "asset_state", "set_asset_state", "pending_domain_loss_barrier_count"], 1),
}
MARKER = "\n// Header pieces of class \"local\" (T*.header): the real predicate before and after the call.\n"
T01_HARNESS = "fn t01_keeps_the_header_predicate_with<const N: usize>()"


def fn_text(lines, name, impl_line):
    hits = [i for i, l in enumerate(lines) if re.match(r"\s{4}(?:pub(?:\([a-z]+\))? )?fn " + name + r"\b", l)]
    out = []
    for i in hits:
        k = i
        while not lines[k].startswith("impl"):
            k -= 1
        if lines[k] == impl_line:
            j = i
            while lines[j] != "    }":
                j += 1
            out.append("\n".join(lines[i:j + 1]))
    assert len(out) == 1, (name, impl_line, len(out))
    return out[0]


def signature(text):
    head = text[:text.index("{")]
    return re.sub(r"\s+", " ", head).strip()


def main(d):
    v16 = open(os.path.join(d, "v16.rs")).read()
    lines = v16.split("\n")
    proofs = open(os.path.join(d, "v16_proofs.rs")).read()
    assert proofs.count(MARKER) == 1, "the local section"
    sec = proofs[proofs.index(MARKER):]
    t01 = T01_HARNESS in sec  # the path accrual's facts are checked when its harness is present
    path = "accrue_asset_path_to_not_atomic"
    # 1. keys
    sig = signature(fn_text(lines, "validate_dynamic_market_slots_len_static", "impl MarketGroupV16HeaderAccount {"))
    assert sig == ("fn validate_dynamic_market_slots_len_static( supplied_len: usize, capacity: usize, "
                   "configured_market_slots: usize, ) -> V16Result<()>"), sig
    assert "let key = (supplied_len, capacity, configured_market_slots);" in sec
    for name, arg in (("amount_from_bound_num", "bound_num"), ("bound_num_from_amount", "amount")):
        sig = signature(fn_text(lines, name, "impl V16Core {"))
        assert sig == f"fn {name}({arg}: u128) -> V16Result<u128>", sig
        assert f"Some((k, r)) if k == {arg} => r," in sec, name
    sig = signature(fn_text(lines, "try_to_runtime_shape", "impl V16ConfigAccount {"))
    assert sig == "pub fn try_to_runtime_shape(&self) -> V16Result<V16Config>", sig
    i = v16.index("pub struct V16ConfigAccount {")
    assert "bytemuck::Pod" in v16[v16.rindex("#[derive(", 0, i):i], "V16ConfigAccount is Pod"
    assert "key.copy_from_slice(bytemuck::bytes_of(config));" in sec
    sig = signature(fn_text(lines, "validate_public_user_fund_shape", "impl V16Config {"))
    assert sig == "fn validate_public_user_fund_shape(&self) -> V16Result<()>", sig
    i = v16.index("pub struct V16Config {")
    assert "PartialEq" in v16[v16.rindex("#[derive(", 0, i):i] and "PartialEq for V16Config" not in v16
    assert "Some((k, r)) if k == *config => return r," in sec
    # the account stand-ins of T29 and T47: both functions take the account (and the market) by shared reference
    pv = "impl<'a> PortfolioV16View<'a> {"
    sig = signature(fn_text(lines, "validate_with_market", pv))
    assert sig == "pub fn validate_with_market<T>(&self, market: &MarketGroupV16View<'_, T>) -> V16Result<()>", sig
    sig = signature(fn_text(lines, "is_empty_for_dematerialization", pv))
    assert sig == "fn is_empty_for_dematerialization(&self) -> V16Result<bool>", sig
    for f in ("v16.rs", "lib.rs", "wide_math.rs"):
        t = re.sub(r"//[^\n]*", "", open(os.path.join(d, f)).read())
        assert not re.search(r"\b(?:Cell|RefCell|UnsafeCell)\b", t), f"interior mutability in {f}"
    agg = fn_text(lines, "validate_header_aggregate_totals", "impl<'a, T> MarketGroupV16View<'a, T> {")
    assert signature(agg) == "fn validate_header_aggregate_totals(&self) -> V16Result<()>", signature(agg)
    body = re.sub(r"//[^\n]*", "", agg[agg.index("{") + 1:])
    reads = re.findall(r"\bself\s*\.\s*header\s*\.\s*([a-z_0-9]+)", body)
    assert len(re.findall(r"\bself\b", body)) == len(reads), "validate_header_aggregate_totals uses self otherwise"
    stand = sec[sec.index("fn memo_header_aggregate_totals"):]
    stand = stand[:stand.index("\n}\n")]
    key = re.findall(r"\bh\.([a-z_0-9]+)\.get\(\)", stand)
    assert len(key) == 9 and len(set(key)) == 9, key
    assert set(reads) == set(key), (sorted(set(reads) ^ set(key)))
    for f in ("v16.rs", "lib.rs", "wide_math.rs"):
        t = re.sub(r"//[^\n]*", "", open(os.path.join(d, f)).read())
        assert not re.search(r"(?<!')\bstatic\b", t), f"a static in {f}"
    # 2. the audit-only scan
    vs = fn_text(lines, "validate_shape", "impl<'a, T> MarketGroupV16View<'a, T> {")
    blocks = re.findall(r"#\[cfg\(([^\]]*)\)\]\s*\{([^{}]*)\}", vs)
    assert blocks == [('any(test, kani, feature = "audit-scan")', blocks[0][1])] if blocks else False, blocks
    assert re.sub(r"\s+", "", blocks[0][1]) == "self.validate_shape_full_audit_scan()?;", blocks[0][1]
    assert len(re.findall(r"#\[cfg|cfg!\(", vs)) == 1, "another cfg in validate_shape"
    st = sec[sec.index("fn default_build_has_no_audit_scan"):]
    st = st[st.index("{", st.index("where")) + 1:st.index("\n}\n")]
    assert re.sub(r"\s+", "", st) == "Ok(())", st
    # 3. build differences in what runs as written
    vs_line = lines.index(next(l for l in lines if l.startswith("    pub fn validate_shape(&self)"))) + 1
    audit = (f"  REPORT v16.rs:{vs_line} MarketGroupV16View::validate_shape: a cfg inside the body that differs "
             'between the builds: any(test, kani, feature = "audit-scan")')
    unresolved = []
    for g, (roots, stubbed, n_audit) in GROUPS.items():
        if not t01:
            roots = [r for r in roots if r != ACCRUAL[path]]
            stubbed = [x for x in stubbed if x != "canonical_accrual_price_step_v16"]
        r = subprocess.run([sys.executable, os.path.join(HERE, "cfg_reach.py"), d, *roots, "--stubbed", *stubbed],
                           capture_output=True, text=True)
        rep = [l for l in r.stdout.splitlines() if l.strip().startswith("REPORT")]
        assert rep == [audit] * n_audit, (g, rep)
        unresolved += [l for l in r.stdout.splitlines() if l.startswith("unresolved calls:")]
    # 4. the entry points read no slot of the view; the accruals use the slice only to refuse an index beyond it, and
    # pass the index only to the replaced accessors and to their core
    for pid, (fn, impl) in ENTRY.items():
        if fn == path and not t01:
            continue
        text = re.sub(r"//[^\n]*", "", fn_text(lines, fn, impl))
        if fn in ACCRUAL:
            core = re.sub(r"//[^\n]*", "", fn_text(lines, ACCRUAL[fn], VIEWMUT))
            assert text.count(".markets") == 1 and re.search(r"\|\|\s*asset_index >= self\.markets\.len\(\)", text), fn
            assert ".markets" not in core, ACCRUAL[fn]
            body = text[text.index("{") + 1:] + core[core.index("{") + 1:]
            uses = [body[max(0, m.start() - 50):m.end() + 45] for m in re.finditer(r"\basset_index\b", body)]
            for u in uses:
                assert re.search(r"asset_index >= (?:config\.max_market_slots as usize|self\.markets\.len\(\))", u) \
                    or re.search(r"self\.(?:" + "|".join(ACCRUAL_STUBS) + r")\(\s*asset_index\b", u) \
                    or re.search(r"(?:" + ACCRUAL[fn] + r"\(|config,)\s*asset_index,", u) \
                    or re.search(r"\basset_index: usize,", u), (fn, u)
        else:
            assert ".markets" not in text, f"{fn} reads a slot of the view"
        if fn == "finalize_side_reset_not_atomic":
            body = text[text.index("{") + 1:]
            ms = list(re.finditer(r"\basset_index\b", body))
            assert len(ms) == 4, (fn, len(ms))
            for m in ms:
                assert re.search(r"self\.(?:" + "|".join(T33_INDEX_CALLEES) + r")\(\s*$", body[:m.start()]), \
                    (fn, body[max(0, m.start() - 60):m.end()])
    vc = re.sub(r"//[^\n]*", "", fn_text(lines, "validate_configured_asset_index", VIEWMUT))
    assert re.sub(r"\s+", " ", vc).strip() == (
        "fn validate_configured_asset_index(&self, asset_index: usize) -> V16Result<()> { if asset_index >= "
        "self.header.config.max_market_slots.get() as usize || asset_index >= self.markets.len() { return "
        "Err(V16Error::InvalidLeg); } Ok(()) }"), vc
    for name in READ_STUBS:
        sig = signature(fn_text(lines, name, VIEWMUT))
        assert re.match(r"fn " + name + r"\(\s*&self,", sig), sig
    # the path accrual refuses a slice of any other length than the expected one, which is at most
    # V16_MAX_ACCRUAL_PATH_STEPS, before anything is written
    if t01:
        check_path_accrual(lines, v16)
        # one harness per length 0..=V16_MAX_ACCRUAL_PATH_STEPS, each calling the helper with that length
        inv = re.findall(r"t01_header_harness!\(\s*proof_v16_accrue_asset_path_to_keeps_the_header_predicate_(\d+)_steps,"
                         r"\s*(\d+)\s*\);", sec)
        assert sorted(int(n) for _, n in inv) == list(range(33)) and all(int(a) == int(n) for a, n in inv), inv
        assert sec.count("t01_header_harness!(") == 33 and sec.count("macro_rules! t01_header_harness {") == 1, \
            "the macro's definition and its 33 uses"
        helper = sec[sec.index(T01_HARNESS):]
        helper = helper[:helper.index("\n}\n")]
        assert "let steps: [AccrualStepV16; V16_MAX_ACCRUAL_PATH_STEPS] = kani::any();" in helper \
            and "&steps[..N]," in helper, helper
        assert re.search(r"\(\$name:ident, \$n:literal\) => \{[^}]*fn \$name\(\) \{\s*"
                         r"t01_keeps_the_header_predicate_with::<\$n>\(\);", sec, re.S), "the macro's body"
    # the replaced kernels and the price step take their inputs by value and write nothing
    for name, impl in PURE_STUBS.items():
        if impl:
            t = fn_text(lines, name, impl)
        elif t01:
            m = re.search(r"\npub fn " + name + r"\(", v16)
            t = v16[m.start():v16.index("\n}\n", m.start())]
        else:
            continue
        sig = signature(t)
        assert "self" not in sig and "&" not in sig, sig
    # 5. the frame of set_asset_state holds for every slot count
    sas = re.sub(r"//[^\n]*", "", fn_text(lines, "set_asset_state", VIEWMUT))
    assert signature(sas) == "fn set_asset_state(&mut self, asset_index: usize, asset: AssetStateV16) -> V16Result<()>", \
        signature(sas)
    body = sas[sas.index("{") + 1:]
    g = re.match(r"\s*if asset_index >= self\.header\.config\.max_market_slots\.get\(\) as usize\s*\|\|\s*"
                 r"asset_index >= self\.markets\.len\(\)\s*\{\s*return Err\(V16Error::InvalidLeg\);\s*\}", body)
    assert g, "set_asset_state: its guard"
    rest = body[g.end():]
    assert len(re.findall(r"\bself\b", rest)) == 3, "set_asset_state: another use of self"
    assert len(re.findall(r"\bself\.markets\[asset_index\]", rest)) == 2, "set_asset_state: the slice"
    assert re.findall(r"\bself\.([a-z_0-9]+)\s*\(", rest) == ["update_resolved_payout_blocker_total"], \
        "set_asset_state: a call on self"
    upd = re.sub(r"\s+", " ", re.sub(r"//[^\n]*", "", fn_text(lines, "update_resolved_payout_blocker_total", VIEWMUT)))
    assert upd.strip() == (
        "fn update_resolved_payout_blocker_total(&mut self, old: u64, new: u64) -> V16Result<()> { "
        "self.header.resolved_payout_blocker_count = V16PodU64::new(Self::apply_total_delta_u64( "
        "self.header.resolved_payout_blocker_count.get(), old, new, )?); Ok(()) }"), upd
    sig = signature(fn_text(lines, "apply_total_delta_u64", VIEWMUT))
    assert sig == "fn apply_total_delta_u64(total: u64, old: u64, new: u64) -> V16Result<u64>", sig
    return "\n".join(unresolved)


def check_path_accrual(lines, v16):
    core = re.sub(r"//[^\n]*", "", fn_text(lines, "accrue_asset_path_to_admitted_not_atomic", VIEWMUT))
    i = core.index("if steps.len() != expected_steps {")
    assert re.search(r"\.min\(V16_MAX_ACCRUAL_PATH_STEPS as u64\)", core[:i])
    assert not re.search(r"self\.(?:set_[a-z_]+|header\.[a-z_]+\s*=)", core[:i]), "a write before the length check"
    assert re.match(r"if steps\.len\(\) != expected_steps \{\s*return Err\(", core[i:])
    assert "pub const V16_MAX_ACCRUAL_PATH_STEPS: usize = 32;" in v16


if __name__ == "__main__":
    print(main(sys.argv[1]))
    print("header_local_check: every fact holds")
