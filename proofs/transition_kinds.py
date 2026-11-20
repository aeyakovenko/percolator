#!/usr/bin/env python3
"""transition_kinds.py: classify every public `*_not_atomic` entry point of an engine `src/v16.rs` by
whether its body re-validates the market shape. Used by gen_transition_spec.py and make_ledger.py so the
spec file and the ledger cannot disagree.

    classify(src_text) -> list of dicts: label, name, line, kind, deployed ("yes" or "no: `cfg(..)`"), first_task,
                          cls, note

`kind` is the result of a text scan and is only a first guess: it says "self-checking" as soon as the
body contains `validate_shape()` anywhere. `cls` and `note` are the classification from READING every
Ok path of every deployed entry point (2026-11-20, engine c030aa40), kept in REVIEWED below with a
fingerprint of the function body. classify() stops with an error when a deployed entry point is not
in REVIEWED or its body no longer has the reviewed fingerprint, so a changed function is read again
before anything is generated from this table.

Classes:
  final      every Ok path ends in `validate_shape()` after its last market write (an early `Ok` writes nothing)
  delegates  the body is a call to another entry point of class `final` (plus read-only checks)
  local      no shape check; the property follows from guards inside the function, for every state
  frame      no shape check; the function writes no header field that `validate_shape` reads
  real       no shape check in a default build; needs kernel contracts and invariants that relate the
             header to the market slots and the accounts
  mixed      `final` on most paths, with the listed Ok paths that end without a check
  dispatch   chooses and calls other functions; each branch is listed
"""
import re, hashlib

# name -> (class, body fingerprint, note). Fingerprint: first 12 hex digits of sha256 of the body text.
REVIEWED = {
    "accrue_asset_path_to_not_atomic": ("local", "16113c661a32",
        "writes `current_slot = now_slot` after refusing `now_slot < current_slot`, and `slot_last = asset.slot_last <= now_slot`; "
        "`loss_stale_active` from `encode_bool`. Ends in the audit-only scan"),
    "accrue_asset_to_not_atomic": ("local", "583d2cb21547",
        "writes `current_slot = max(current_slot, now_slot)` and `slot_last = asset.slot_last <= now_slot`; `loss_stale_active` from "
        "`encode_bool`. Ends in the audit-only scan"),
    "activate_empty_asset_slot_not_atomic": ("local", "5696ea1a3a3b", "a call to T05"),
    "activate_empty_market_slot_not_atomic": ("local", "ca33cea3425e",
        "header-account method: writes `next_market_id`, `current_slot`, `asset_activation_count`, `last_asset_activation_slot` and two epochs"),
    "add_account_source_positive_pnl_not_atomic": ("final", "6d9d4fe0cf47", "early `Ok` for `amount == 0` before any write"),
    "advance_resolved_slot_not_atomic": ("local", "98c39ed319d6", "one write, `current_slot = kernel_advance_resolved_slot(..)`"),
    "advance_terminal_slab_not_atomic": ("mixed", "13250b15ba14",
        "validates at entry, then calls T09. The two `ScanProgress` returns end without a check after T09's write only, so they reduce "
        "to T09; the other three Ok paths end in the check"),
    "canonicalize_retired_empty_asset_slot_not_atomic": ("final", "8db3b50b5012", ""),
    "canonicalize_spent_backing_history_not_atomic": ("final", "e28dac01cf9e",
        "first applies retirement's normalization to the asset's two buckets (a bucket write each, no header field); every Ok path still ends in the check"),
    "charge_account_backing_fee_not_atomic": ("final", "9069fc18e03a", "early `Ok(0)` for a zero fee before any write"),
    "claim_resolved_payout_topup_not_atomic": ("final", "280710e74c82", ""),
    "close_resolved_account_not_atomic": ("mixed", "f57730fddb08",
        "seven Ok paths end in the check, and the return after a source preparation is checked inside the callee. The two "
        "`ProgressOnly` returns after `settle_negative_pnl_from_principal_not_atomic`, `settle_resolved_bankruptcy_negative_pnl` and "
        "`detach_solvent_active_legs_for_resolved_close` end without a check: class real. The program's CloseResolved handler "
        "validates the shape after the call"),
    "convert_released_pnl_to_capital_not_atomic": ("final", "55c8375600d7",
        "the check is skipped only for `converted == 0`, which the core returns only before any write (`released == 0`)"),
    "credit_account_from_insurance_not_atomic": ("final", "660ac90ae842", "early `Ok` for `amount == 0`"),
    "credit_backing_provider_earnings_not_atomic": ("final", "e9ff578eb854", "early `Ok` for `amount == 0`"),
    "credit_domain_insurance_budget_not_atomic": ("delegates", "99e36eb71f1a", "a call to T24"),
    "credit_domain_insurance_budgets_not_atomic": ("final", "96997b364aec", ""),
    "cure_and_cancel_close_not_atomic": ("final", "67b938a28806",
        "a read-only preflight, T37 (class real, no check of its own), then `cure_and_cancel_close_with_cert_not_atomic`, whose only "
        "Ok path ends in the check; that check sees T37's writes too"),
    "deposit_domain_insurance_not_atomic": ("final", "9d3859a5f601", ""),
    "deposit_fresh_counterparty_backing_not_atomic": ("final", "c0053e270b72",
        "after the principal adjustment it shifts the domain's provider-protection floor (a slot field); ends in the check"),
    "deposit_not_atomic": ("final", "90772fe720ee", "early `Ok` for `amount == 0`"),
    "deregister_empty_materialized_portfolio_not_atomic": ("frame", "900524d24017", "writes only `materialized_portfolio_count`"),
    "execute_batch_with_fee_loss_stale_scoped_not_atomic": ("final", "5c7e3cfd6804", "the check follows the restore of `loss_stale_active`"),
    "execute_trade_with_fee_loss_stale_scoped_not_atomic": ("delegates", "9416a18706fc", "a call to T30"),
    "expire_source_backing_bucket_not_atomic": ("real", "f33f98d0ee17",
        "writes the backing and source-credit totals and, after the payout snapshot, the payout ledger"),
    "finalize_side_reset_not_atomic": ("frame", "8e20bfee36c7",
        "early `Ok` for a side that is already Normal, before any write; otherwise writes one asset record, "
        "`resolved_payout_blocker_count` and `risk_epoch`, and calls `validate_header_aggregate_totals` (H14 to H17) in every build"),
    "force_asset_recovery_not_atomic": ("final", "3353d795ee2e", "both Ok arms end in the check"),
    "force_close_recovery_pair_not_atomic": ("delegates", "0768dc5a89c3", "every Ok path ends in one of two calls to T31"),
    "forfeit_recovery_leg_not_atomic": ("mixed", "f3a71ac74fd6",
        "three Ok paths end in the check. The return for a leg with nothing left to settle ends without one, after "
        "`begin_full_drain_reset_inner`, then `retain_leg_as_pending_obligation` or `clear_forfeited_leg` and "
        "`begin_zero_oi_residue_resets`: class frame, to be confirmed from the write sets of those four functions"),
    "full_account_refresh_not_atomic": ("real", "7c033a258c1d", "a call to `refresh_account_and_certify_not_atomic`, which ends in the audit-only scan"),
    "grow_asset_slot_capacity_not_atomic": ("local", "b54736755044", "header-account method: writes `asset_slot_capacity`, `config` and two epochs"),
    "liquidate_account_not_atomic": ("real", "21aa120f11e4", "both Ok paths end in the audit-only scan"),
    "mark_asset_drain_only_not_atomic": ("final", "25fe4203a364", "`Ok` for an asset that is already DrainOnly, without a write"),
    "permissionless_auto_crank_not_atomic": ("dispatch", "9caad715062a",
        "Recovery: T36 then the audit-only scan, or T53. Released obligation: T36, or `clear_leg` and `begin_zero_oi_residue_resets`, "
        "then the audit-only scan. Recovery leg: `refresh_recovery_account_from_committed_state_not_atomic` (audit-only). "
        "ReleaseSourceLiens: its core ends in the check. Refresh: T02 then `refresh_account_and_certify_not_atomic` (audit-only). "
        "SettleBChunk: `settle_account_b_chunk` (no market check). Liquidate: T42, then T02. AdvanceClose: "
        "`advance_pending_close_residual_not_atomic` (audit-only); its `RecoveryRequired` error is turned into `Ok` when the call "
        "declared Recovery, which validates the shape at the declaration. DeclareRecovery: validates. CloseResolved: T09, then T32 "
        "and the check, or T15. The program's crank handler validates the shape afterwards only when the position vector changed"),
    "rebalance_reduce_position_not_atomic": ("real", "71da8efbda88",
        "ends in the audit-only scan; the program's RebalanceReduce handler does not validate afterwards either (compute budget)"),
    "recredit_terminal_claim_free_residual_for_asset_not_atomic": ("final", "7254982763d3", "validates at entry and at exit"),
    "register_empty_materialized_portfolio_not_atomic": ("frame", "7e3a23af0248", "writes only `materialized_portfolio_count`"),
    "reset_empty_asset_oracle_anchor_not_atomic": ("final", "c36e072042b1", ""),
    "resolve_market_not_atomic": ("final", "0063ab0c7a50", ""),
    "restart_empty_asset_preserving_insurance_budget_not_atomic": ("final", "69d43840afb3", ""),
    "retire_empty_asset_not_atomic": ("final", "bb587280a000", "both Ok arms end in the check"),
    "retire_terminal_unbudgeted_insurance_not_atomic": ("final", "33d2c6cd5d9a", "validates at entry and at exit"),
    "set_asset_raw_oracle_target_not_atomic": ("delegates", "61e861e02c69", "a call to T58"),
    "set_asset_raw_oracle_targets_not_atomic": ("final", "44191266e35e", ""),
    "sync_account_fee_to_slot_not_atomic": ("final", "25c18f09a3f6", "early `Ok(0)` when no fee is due, before any write"),
    "withdraw_backing_provider_earnings_not_atomic": ("final", "3276d1ee20fd", "early `Ok` for `amount == 0`"),
    "withdraw_domain_insurance_not_atomic": ("final", "55e4683b6b0a", ""),
    "withdraw_fresh_counterparty_backing_not_atomic": ("final", "707136688bfe",
        "after the principal release it shifts the domain's provider-protection floor (a slot field); ends in the check"),
    "withdraw_not_atomic": ("final", "5a3c2f432311", "early `Ok` for `amount == 0`"),
    "withdraw_resolved_counterparty_backing_surplus_not_atomic": ("delegates", "99aaad13ab8e", "a read-only check, then a call to T62"),
}

def classify(src):
    rows = []
    for m in re.finditer(r"(?m)^    pub fn ([a-z_0-9]+_not_atomic)", src):
        name = m.group(1)
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
        rows.append((name, src.count("\n", 0, m.start()) + 1, src[i:j + 1]))
    seen = {}
    for name, line, body in rows:
        seen.setdefault(name, (line, body))
    names = sorted(seen)
    LINES = src.split("\n")
    def cfg_gate(name):
        """The `#[cfg(..)]` directly above the entry point (above its attributes and doc comment), if any."""
        j = seen[name][0] - 2
        while j >= 0:
            t = LINES[j].strip()
            if re.match(r"(pub(\([a-z]+\))? )?(const )?fn |\}$|impl ", t) or t == "":
                return None
            m = re.match(r"#\[cfg\((.*)\)\]$", t)
            if m:
                return m.group(1)
            j -= 1
        return None
    def deployed(name):
        g = cfg_gate(name)
        return "yes" if not g else f"no: `cfg({g})`"
    def fn_body(fname):
        """Body of the first `fn fname(` in the file (public or private), or None."""
        m = re.search(r"(?m)^\s*(?:pub(?:\([a-z]+\))? )?fn %s\b" % re.escape(fname), src)
        if not m:
            return None
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
        return src[i:j + 1]
    def wrapped(name):
        body = seen[name][1]
        return [o for o in names if o != name and (f"self.{o}(" in body or f"Self::{o}(" in body)]
    def kind(name):
        body = seen[name][1]
        if "validate_shape()" in body:
            last = body.rindex("validate_shape()")
            early = any(m.start() < last for m in re.finditer(r"return Ok\(", body))
            conditional = re.search(r"if [^{]*\{\s*self\.validate_shape\(\)\?", body) is not None
            if early or conditional:
                return "self-checking, with an early or conditional Ok path"
            return "self-checking"
        # the body's final expression is a call to a private function that ends in validate_shape()
        tail = re.search(r"self\.([a-z_0-9]+)\([^;{}]*\)\s*\}\s*$", body)
        if tail and tail.group(1) not in names:
            inner = fn_body(tail.group(1))
            if inner and re.search(r"self\.validate_shape\(\)\s*\}\s*$", inner):
                return f"self-checking through its final call, to `{tail.group(1)}`"
        w = wrapped(name)
        if w:
            tags = []
            for o in w:
                k = kind(o)
                tags.append(f"{LABEL[o]}" + (" (which is unchecked)" if k.startswith("unchecked") else ""))
            return "delegating to " + ", ".join(tags)
        if "validate_shape_audit_scan()" in body:
            return "unchecked in production"
        return "unchecked"
    LABEL = {name: f"T{i:02d}" for i, name in enumerate(names, 1)}
    def reviewed(name):
        if deployed(name) != "yes":
            return ("not deployed", "")
        if name not in REVIEWED:
            raise SystemExit(f"transition_kinds: deployed entry point `{name}` has not been reviewed; read its Ok paths and add it to REVIEWED")
        cls, fp, note = REVIEWED[name]
        now = hashlib.sha256(seen[name][1].encode()).hexdigest()[:12]
        if now != fp:
            raise SystemExit(f"transition_kinds: the body of `{name}` changed since it was reviewed ({fp} -> {now}); read it again and update REVIEWED")
        return (cls, note)
    def first_task(name):
        """Entry points whose property is structural: every Ok path ends in the runtime check."""
        return reviewed(name)[0] in ("final", "delegates")
    return [dict(label=LABEL[n], name=n, line=seen[n][0], kind=kind(n), deployed=deployed(n), first_task=first_task(n),
                 cls=reviewed(n)[0], note=reviewed(n)[1])
            for n in names]
