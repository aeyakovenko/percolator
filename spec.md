# Risk Engine Spec (Source of Truth) — v16.9.0 Realizable Full Shared Cross-Margin

**Design:** protected principal + full instance-local cross-margin + source-domain realizable PnL credit + source-credit liens + insurance-credit reservations + exact counterparty/insurance lien lifecycle + single-category residual-cure accounting + quote-value flow proof + reservation encumbrance proof + stock reconciliation + explicit rounding-residue sink + reserved recovery-fallback envelope (mechanism reserved; see req 31) + expiry-reconciled backing buckets + non-double-counted insurance capacity + single-sided margin penalties + exclusive per-domain close serialization + local market-side bankruptcy domains + mutable asset lifecycle + domain-serialized bankrupt close + durable close-progress ledger + pending-loss obligations + instance isolation.  
**Scope:** one Percolator market-group instance for one quote-token vault, with up to `N` configured asset slots per `PortfolioAccount` and unbounded global account count. A UI MAY aggregate multiple instances, but each instance is an independent vault, solvency, credit, insurance, B, PnL, payout, and recovery domain.  
**Status:** normative source of truth. Terms **MUST**, **MUST NOT**, **SHOULD**, and **MAY** are normative.

This revision (v16.9.0) supersedes v16.8.11 by reconciling three specified-but-never-implemented mechanisms with the PROVEN implementation (formal-verification campaign, ~265 certified Kani artifacts: conservation lattice, function contracts, inductive closure, exact whole-state frames, no-steal composition in `scripts/no-steal-theorem.md`): (1) requirement 21's priority-preemption (`ClosePriority` tuple) is replaced by the implemented and machine-proven EXCLUSIVE close serialization — one active close per domain via the pending-domain-loss barrier (occupied-domain begin rejects before mutation), one active close per account, strictly monotone `close_id`, liveness via the immutable `max_close_slot` lifetime; hold-and-wait and livelock are impossible by construction rather than by comparison; (2) requirement 23's close-drift reserve is replaced by the implemented bounded-lifetime rule — `drift_consumed` remains a reserved, always-zero partition category (no engine path funds a drift reserve) so the ledger shape is forward-compatible; (3) requirement 31's recovery fallback pricing is marked RESERVED — the config knobs exist and are bound-validated but no code path may synthesize fallback prices until the mechanism lands with its own envelope proofs; recovery operates on the last authenticated effective price with proven accounting-neutral transitions. This revision supersedes v16.8.10 with reviewer-driven wording precision (no engine change): (1) realization is RELEASE pre-existing liens then CREATE-AND-CONSUME fresh backing — it never consumes a Released lien (the prior "release followed by consumption" wording mis-described the lien state machine); (2) flow-proof obligation is keyed on the FUNDED-STOCK DEBIT, not the destination — counterparty cure (no funded debit, pure reclassification to the derived junior pool) is exempt like forfeit, insurance cure (debits I) keeps its CloseInsuranceSpent flow proof, resolving the v16.8.10 exemption-predicate-vs-req-12 inconsistency; (3) reserve crystallizes on a REALIZED-loss settlement with basis rebase, not on mark-to-market refresh (no unrealized-drawdown ratchet, no double-charge); (4) the realization payout is characterized as realizable-limited and conservation-exact (proven across the full backing range) rather than monotone — the unbacked shortfall is refined back to the junior pool, and snapshot order-independence is stated precisely. v16.8.10 superseded v16.8.9 with one typing clarification: forfeit moves (impair/expiry) whose only credit is the derived closing class `junior_residual_pool` are proven by their `ReservationEncumbranceProof` plus the lockstepped `counterparty_backing_principal` aggregate delta and the residual identity — not by a `TokenValueFlowProof` credit entry, which would double-book a derived class; all moves with non-derived destinations keep their balanced flow proofs. v16.8.9 superseded v16.8.8 by closing the `counterparty_backing_principal` lifecycle over ALL transitions that move `Σ fresh_reserved_backing_num` (a v16.8.8 review showed conservation was stated only for reserve/consume): (1) the stock equality gains its defined CLOSING class `junior_residual_pool` (and `backing_provider_earnings`), so impairment and expiry are not orphanings — the forfeited principal lands atom-for-atom in the junior pool with `V` flat (engine-proven: the impair/expiry witnesses are value-neutral on `V/C_tot/I` and raise residual by exactly the forfeit); (2) the phantom "release (claim no longer owed): principal −X; C_tot += X" lifecycle line is REMOVED — reserve crystallizes a realized loss, so no internal backing→capital teardown exists or may be synthesized, and the only "release" is the lien un-pledge encumbrance relabel (no stock movement); (3) the lifecycle table now lists every transition with its lockstepped stock movement (external deposit/withdrawal, reserve, un-pledge, realize, cure, forfeit) and clarifies the reserve flow-proof transit label (`ExplicitBackedLoss`) does not name the destination class; (4) proof item 24's partition→class map is scoped per cure transition (transit classes net to zero at reconciliation points; no persistent sub-decomposition of `support_consumed` is required). The v16.8.8 corrections (below) stand. This revision supersedes v16.8.7 with stock-class typing closures for the terminal-realization mechanism: (1) `counterparty_backing_principal` is named as a FIRST-CLASS persistent stock class in the §5.1.1 equality (not merely a senior-stack inequality term), so the conservation of counterparty-backed realization is explicit — `C_tot += X` is funded by `counterparty_backing_principal -= X`, with the full reserve→realize→release lifecycle and its `TokenValueFlowProof` transit classes (`ExplicitBackedLoss`, `CloseCounterpartyCreditConsumed`) spelled out; (2) requirement 13's flow-proof ban is scoped to the `BackingBucket` encumbrance counters only, explicitly NOT the `counterparty_backing_principal` stock class or the transit value classes; (3) realization EXTENT is defined (consume the backed face, credit `floor(r·F)`, burn the consumed face so it never also enters the receipt pool; only non-source-backed PnL survives into the pool) with `total_paid ≤ face` and idempotence; (4) §14 proof items 22/24 are rewritten to the single-category (`support_consumed`) model with a pinned partition→close-flow-class map so per-class reconciliation (proof 102) is well-defined. The earlier v16.8.7 corrections (below) stand. This revision supersedes v16.8.6 with two corrections. (1) The residual partition no longer lists `consumed_counterparty_credit_lien_backing` as a separate subtrahend: ALL source-credit lien consumption used for residual cure (counterparty- or insurance-backed) is recorded exactly once as `support_consumed`, `insurance_spent` records direct insurance allocation only, and partition categories are pairwise disjoint — the prior text put counterparty-backed liens inside `SupportPool` while also requiring a standalone counterparty subtrahend, so one conforming reading subtracted the same cure twice and finalized closes with uncovered loss. (2) The v16.8.6 senior classification of recoverable counterparty backing principal is completed with a terminal-realization rule: at resolved close, an account's outstanding source-credit claims MUST be realized against their domain backing at the current credit rate before the claim face enters the resolved payout receipt pool; otherwise resolution would strip claims that were realizable in Live (the wind-down releases the backing to the provider while the winner is haircut from a pool that excludes it). v16.8.6 classified recoverable counterparty backing principal as senior-side vault stock (`C_tot + I + backing_provider_earnings + counterparty_backing_principal <= V`, excluded from the junior residual pool and the resolved payout snapshot); v16.8.5 superseded v16.8.4 for the product goal of Hyperliquid-like cross-margin UX in permissionless accounts while containing oracle/market failure by limiting usable PnL to realizable source-domain backing.

```text
Inside one trusted instance:
    all Active assets are Tier-4 / support weight 1.0;
    positive PnL from one leg may support another leg immediately;
    but source-domain positive credit is capped by realizable, reserved counterparty backing.

Across instances:
    no shared health, collateral, PnL, insurance, B, haircut, payout, or recovery.
```

The core invariant is:

```text
usable_positive_credit_from_source_domain
    <= realizable_counterparty_backing_reserved_for_that_domain
```

If an attacker manipulates asset A, positive PnL on A is usable only up to what the A losing side can conservatively pay or has actually reserved. If A counterparties bankrupt, A-source credit falls, credit liens become impaired, and dependent portfolios must deleverage, liquidate, or ADL. The attacker cannot transform uncollectible A paper profit into global B-asset purchasing power.

Every top-level instruction is atomic. Any failed precondition, checked arithmetic guard, missing authenticated proof, context-capacity overflow, stale close snapshot, invalid credit bound, invalid credit lien, invalid insurance-credit reservation, invalid insurance-capacity accounting, invalid backing-expiry reconciliation, invalid lien lifecycle accounting, dual-classified lien accounting, invalid quote-value flow proof, invalid reservation encumbrance proof, invalid stock reconciliation, unattributed rounding residue, invalid recovery configuration, margin penalty double-counting, invalid hedge envelope, close-serialization conflict, unresolved pending loss, cross-instance netting attempt, or conservative-failure condition MUST roll back every mutation performed by that instruction. Before commit, every successful instruction MUST leave all global, asset, account, certificate, credit, lien, close-state, insurance, payout, obligation, and attribution invariants true.

-------------------------------------------------------------------------------
0. Non-negotiable requirements
-------------------------------------------------------------------------------

1. **Full shared account solvency:** every Active asset inside one instance has support weight `1.0`. Eligible positive PnL from any leg may support maintenance and risk approval for any other leg in the same account.
2. **Source-domain realizability cap:** positive PnL from a leg is usable only through its source domain `(asset, opposing_side)` and only up to that source domain's realizable counterparty backing.
3. **No identity assumptions:** the engine MUST NOT rely on detecting self-trading, common ownership, or account linkage. All protections are economic and source-domain based.
4. **Instance boundary is absolute:** no health, collateral, PnL, lien, insurance, B, payout, or recovery state may cross instances. UI aggregation is display only.
5. **No global B pool:** bankruptcy residual is charged only to the asset-side domain whose exposure generated it.
6. **Protected principal is senior:** junior positive PnL is usable only when backed by reserved counterparty value or conservative source-domain rate. It MUST NOT outrank senior capital, insurance, or durable loss recognition.
7. **Healthy-market UX:** when a source domain's conservative claim bound is fully backed, its credit rate is `1.0`, so spread and portfolio PnL behave as fungible within the account.
8. **Oracle/manipulation containment:** if a source domain's counterparty backing is insufficient or stale, credit from that domain is haircut, lien-impaired, and cannot be withdrawn or used for new risk beyond the backing cap.
9. **Credit liens for durable use:** withdrawals, conversions, fee payment from PnL, residual curing, and risk-increasing trades that depend on positive PnL MUST reserve or consume a source-domain credit lien. Maintenance-only credit MAY be soft but must revalidate on every favorable action.
10. **No double use of credit or insurance:** the same source-domain claim, backing, or insurance atom cannot support two accounts, two domains, two instances, or two risk increases at once. Face claim, backing reservation, and insurance reservation are tracked by a single canonical ledger and released only by deterministic rules.
11. **Insurance-backed lien lifecycle is explicit:** creating, consuming, releasing, impairing, or recovering an insurance-backed source-credit lien MUST update `valid_liened_insurance_num`, `impaired_liened_insurance_num`, `insurance_credit_reserved_num`, insurance spend/reservation counters, and vault/insurance balances exactly once.
12. **Source-credit lien cures are counted once:** a source-credit lien consumed for residual cure — counterparty-backed or insurance-backed — MUST reduce the residual partition through exactly one category (`support_consumed`); no second partition category may subtract the same cure. An insurance-backed lien cure MUST additionally debit the insurance stock and the domain insurance budget exactly once through a balanced `TokenValueFlowProof` (`CloseInsuranceSpent`); that funding debit is stock accounting, not a residual-partition subtraction, and MUST NOT also be booked as direct `insurance_spent`.
13. **Quote-value conservation is mandatory:** every instruction that moves quote-token value internally or externally MUST produce a balanced `TokenValueFlowProof` over quote atoms only. The BANNED labels are the `BackingBucket`/reservation ENCUMBRANCE counters (`fresh_unliened_backing_num`, `valid_liened_backing_num`, `insurance_credit_reserved_num`, …) and the lien lifecycle relabelings — these are encumbrance over already-counted value, are not value classes, and MUST be proven by a separate `ReservationEncumbranceProof`. This ban does NOT extend to the quote-atom stock class `counterparty_backing_principal` or the transit value classes (`CloseSupportConsumed`, `CloseCounterpartyCreditConsumed`, `CloseInsuranceSpent`, `ExplicitBackedLoss`, `BResidualBooked`): those ARE declared value classes that appear in `TokenValueFlowProof` and stock reconciliation (see §5.1.1). The encumbrance proof and the value-flow proof move in lockstep but are separately typed.
14. **Rounding residue has an explicit sink:** every quote-atom settlement/allocation residue caused by conservative rounding MUST be credited to `SettlementRoundingResidue` or `UnallocatedProtocolSurplus` in the same `TokenValueFlowProof`. Rounding residue MUST NOT create account health, source-credit backing, insurance credit, payout entitlement, or senior capital unless later moved through an explicit balanced transition.
15. **No open unbacked loss curing:** open positive PnL may support health, but it MUST NOT cure a bankruptcy residual unless a source-domain backing lien is consumed and the supporting face claim is locked/burned.
16. **Stale backing fails closed:** stale, expired, unrefreshed, or unverifiable counterparty backing contributes zero to source-domain usable credit. Expiry, lien consumption, lien release, and lien impairment MUST update both bucket-local and source-domain aggregates exactly once and MUST NOT increase `available_backing_num` or credit rate unless new independently proven backing is added.
17. **Claim bounds are exact scaled bounds:** source-domain positive claim bounds are computed from exact claims plus formulaic, replaceable bucket terms in `BOUND_SCALE` units. They MUST never understate.
18. **Credit rates are deterministic:** source-domain rates are derived from claim bounds and reserved backing; they are not caller supplied and cannot be made favorable by stale certificates.
19. **Pending loss obligations survive exit:** a participant reducing/clearing weight while exposed to pending residual must escrow, settle, or pull forward its obligation, including drift share, before weight removal.
20. **Penalty accounting is single-sided:** for every health, initial, trade, and withdrawal check, each penalty, pending obligation, impaired lien, or reserve MUST appear either as an equity deduction or as a requirement add-on, never both.
21. **Exclusive close ownership:** bankrupt close ownership is deterministic and exclusive: at most one active close per domain (enforced by the pending-domain-loss barrier; a second close beginning in an occupied domain MUST reject before mutation) and at most one active close per account. Each close holds exactly one domain, so hold-and-wait cycles are impossible by construction; contention rejects rather than compares, so livelock is impossible. Liveness is guaranteed by the immutable `max_close_slot` lifetime bound: an expired close routes to recovery rather than holding its domain. *(v16.9.0: replaces the earlier priority-preemption design, which was never implemented; the exclusive-barrier mechanism is the proven implementation.)*
22. **Immutable close lifecycle:** `close_id`, `gross_loss_at_close_start`, `drift_reference_slot`, and `max_close_slot` persist across restart and recovery until finalized or safely canceled. `close_id` is strictly monotone per account: a new close never reuses an id.
23. **Bounded close lifetime:** every close is bounded by the immutable `max_close_slot` (`drift_reference_slot + cfg_max_bankrupt_close_lifetime_slots`); an expired close MUST route to recovery instead of continuing. `drift_consumed` is a reserved residual-partition category and MUST remain zero in v16.9 (no engine path funds a drift reserve); the partition equation carries the column so a future drift-reserve mechanism can be added without re-shaping the ledger.
24. **Residual durability before exposure clear:** basis, OI, PnL, and side weights for bankrupt close MUST NOT be freed until residuals are booked, backed, explicitly assigned, or recovered.
25. **No ADL/finalization split:** quantity ADL, closing-account exposure clear, and ledger advancement MUST be atomic or protected by a non-preemptible finalization barrier.
26. **No fee seniority:** uncollectible protocol/liquidation fees are dropped or forgiven, never paid from insurance or socialized through B.
27. **Deterministic residual attribution:** liquidation order, support allocation, insurance allocation, credit-lien consumption, and residual attribution MUST be deterministic and independent of caller ordering.
28. **No arbitrary correlation trust:** hedge credit is allowed only under deterministic buckets and exact conservative portfolio envelopes proving worst-case combined loss is covered after the credit.
29. **Mutable asset lifecycle is fail-closed:** activation requires full envelope proofs, bounded rate limits, support weight `1.0`, fresh source-domain credit ledgers, and certificate fail-closed handling.
30. **Dead-leg exit:** public markets MUST expose bounded owner-callable dead-leg forfeit/detach for terminal/recovery assets.
31. **Recovery fallback pricing is reserved (not implemented):** v16.9 implements no synthetic fallback price computation. Recovery operates on the last authenticated effective price and the proven accounting-neutral recovery transitions; states where bounded progress cannot continue route to terminal recovery / resolution. The configuration knobs (`cfg_recovery_fallback_price_enabled`, `cfg_max_recovery_fallback_deviation_bps`, `cfg_recovery_fallback_envelope_enabled`) are bound-validated and RESERVED for a future fallback mechanism; until such a mechanism exists with its own envelope proofs, no code path may consume them to synthesize prices.
32. **Hints are discovery only:** omitted or stale positions MUST NOT improve account health.
33. **Full account refresh is bounded by `N`:** every user-favorable operation MUST refresh the full active portfolio first.
34. **No full-market atomic work:** public instructions MUST NOT scan all accounts or all opposing accounts.
35. **Crank-forward public markets:** any state that only a privileged actor can advance is non-compliant.
36. **Canonical per-asset leg:** each account has at most one canonical signed net leg per configured asset.
37. **Verified maker exemption is bounded:** maker/liquidator refresh exemption is allowed only with an engine-verified post-trade health certificate covering the exact candidate trade.
-------------------------------------------------------------------------------
1. Units, bounds, and configuration
-------------------------------------------------------------------------------

Persistent quantities use `u128` or `i128`; persistent signed fields MUST NOT equal `i128::MIN`. Transient products involving price, position, A/K/F/B, credit, liens, bounds, fees, insurance, obligations, and remainders MUST use an exact domain at least 256 bits wide. All divisions round against the account unless explicitly stated.

```text
POS_SCALE                    = 1_000_000
ADL_ONE                      = 1_000_000_000_000_000
FUNDING_DEN                  = 1_000_000_000
SOCIAL_WEIGHT_SCALE          = ADL_ONE
SOCIAL_LOSS_DEN              = 1_000_000_000_000_000_000_000
SUPPORT_WEIGHT_SCALE         = 1_000_000
FULL_SUPPORT_WEIGHT          = SUPPORT_WEIGHT_SCALE
BOUND_SCALE                  = 1_000_000_000_000
CREDIT_RATE_SCALE            = 1_000_000_000_000
MAX_BPS                      = 10_000
```

Every live, resolved, raw target, effective engine, and recovery price (and any future fallback price) MUST satisfy:

```text
0 < price <= MAX_ORACLE_PRICE
```

```text
RiskNotional(asset, account) =
    0 if effective_pos_q == 0
    else ceil(abs(effective_pos_q) * conservative_effective_price / POS_SCALE)

trade_notional =
    floor(abs(size_q) * exec_price / POS_SCALE)
```

### 1.1 Hard bounds

```text
MAX_VAULT_TVL                         = 10_000_000_000_000_000
MAX_ORACLE_PRICE                      = 1_000_000_000_000
MAX_POSITION_ABS_Q_PER_ASSET          = 100_000_000_000_000
MAX_TRADE_SIZE_Q                      = MAX_POSITION_ABS_Q_PER_ASSET
MAX_OI_SIDE_Q_PER_ASSET               = 100_000_000_000_000
MAX_ACCOUNT_NOTIONAL_PER_ASSET        = 100_000_000_000_000_000_000
MAX_PORTFOLIO_ASSETS_N                = implementation/config bounded
MAX_PROTOCOL_FEE_ABS                  = 1_000_000_000_000_000_000_000_000_000_000_000_000
GLOBAL_MAX_ABS_FUNDING_E9_PER_SLOT   = 10_000
MAX_WARMUP_SLOTS                      = u64::MAX
MAX_RESOLVE_PRICE_DEVIATION_BPS       = 10_000
MAX_RECOVERY_FALLBACK_DEVIATION_BPS    = MAX_RESOLVE_PRICE_DEVIATION_BPS
MIN_A_SIDE                            = 100_000_000_000_000
MAX_CLAIM_BOUND_BUCKETS_PER_SIDE      = implementation/config bounded
MAX_BACKING_EXPIRY_BUCKETS            = implementation/config bounded
```

`N`, claim-bound buckets, and backing-expiry buckets MUST be small enough that full refresh, credit-rate recomputation, lien creation/release, liquidation validation, close-vector re-aging, pending-obligation netting, and proof packing fit runtime limits.

### 1.2 Public-market configuration

Public user-fund markets MUST satisfy:

```text
cfg_margin_mode == RealizableFullSharedCrossMargin
cfg_asset_support_weight(asset) == FULL_SUPPORT_WEIGHT for every Active asset
cfg_source_credit_mode == CounterpartyRealizableBackingCapped
cfg_credit_lien_mode == RequiredForRiskAndSettlementUse
cfg_claim_bound_mode == ExactScaledDecomposedReplaceable
cfg_backing_mode == ReservedFreshCounterpartyBacking
cfg_bankruptcy_mode == LegAttributedMarketSideB
cfg_insurance_mode in {DomainBudgeted, GlobalProtocolFirstLossWithCaps}
cfg_asset_set_lifecycle == MutableWithActivationProofs
cfg_instance_isolation == true
cfg_public_liveness_profile == CrankForward
cfg_permissionless_recovery_enabled == true
cfg_recovery_fallback_price_enabled == true       # RESERVED (v16.9.0): see req 31
cfg_max_recovery_fallback_deviation_bps <= MAX_RECOVERY_FALLBACK_DEVIATION_BPS
cfg_recovery_fallback_envelope_enabled == true    # RESERVED (v16.9.0): see req 31
cfg_owner_dead_leg_forfeit_enabled == true
cfg_full_refresh_required_for_favorable_actions == true
cfg_stale_certificate_penalty_enabled == true
cfg_deterministic_portfolio_liquidation_enabled == true
cfg_close_state_scope == AccountLocalWithPreemptibleDomainLocks
cfg_close_conflict_policy == DeterministicPreemptivePriority
cfg_no_global_B_index == true
cfg_no_cross_instance_socialization == true
cfg_asset_activation_cooldown_slots >= cfg_min_public_refresh_grace_slots
cfg_public_b_chunk_atoms > 0
cfg_max_account_b_settlement_chunks > 0
cfg_max_bankrupt_close_chunks > 0
cfg_max_bankrupt_close_lifetime_slots > 0
cfg_credit_lien_revalidation_required == true
cfg_backing_freshness_buckets == 1
cfg_pending_obligation_settlement_chunks > 0
cfg_close_drift_reserve_enabled == true   # RESERVED (v16.9.0): no drift-reserve mechanism exists; knob validated, unused
cfg_close_drift_anchor_mode == ImmutableReferenceSlot
```

Activation and initialization validate all fee, price, funding, margin, OI, B-headroom, source-credit, close-progress, and portfolio envelopes. Assets that should not fully share account solvency SHOULD be deployed in a separate instance.

If global protocol first-loss insurance is enabled:

```text
permitted_global_protocol_first_loss_for_domain =
    min(domain_global_cap - domain_global_spent,
        global_protocol_budget - global_protocol_spent)
```

### 1.3 Solvency and close-progress envelopes

For each Active or activating asset:

```text
ADL_ONE * MAX_ORACLE_PRICE * cfg_max_abs_funding_e9_per_slot * cfg_max_accrual_dt_slots <= i128::MAX
cfg_min_funding_lifetime_slots >= cfg_max_accrual_dt_slots
ADL_ONE * MAX_ORACLE_PRICE * cfg_max_abs_funding_e9_per_slot * cfg_min_funding_lifetime_slots <= i128::MAX
```

For every `1 <= X <= MAX_ACCOUNT_NOTIONAL_PER_ASSET`:

```text
price_budget_bps      = cfg_max_price_move_bps_per_slot * cfg_max_accrual_dt_slots
funding_budget_num    = cfg_max_abs_funding_e9_per_slot * cfg_max_accrual_dt_slots * 10_000
loss_budget_num       = price_budget_bps * FUNDING_DEN + funding_budget_num
price_funding_loss_X  = ceil(X * loss_budget_num / (10_000 * FUNDING_DEN))
worst_liq_notional_X  = ceil(X * (10_000 + price_budget_bps) / 10_000)
liq_fee_raw_X         = ceil(worst_liq_notional_X * cfg_liquidation_fee_bps / 10_000)
liq_fee_X             = min(max(liq_fee_raw_X, cfg_min_liquidation_abs), cfg_liquidation_fee_cap)
mm_req_X              = max(floor(X * cfg_maintenance_bps / 10_000), cfg_min_nonzero_mm_req)
require price_funding_loss_X + liq_fee_X <= mm_req_X
```

Close-progress envelope MUST cover every allowed portfolio and close domain set, not merely per asset.

RESERVED (v16.9.0): the recovery-fallback price mechanism below is specified for a FUTURE revision and is NOT implemented; no engine path synthesizes fallback prices, and activation does not run this validation. The envelope is retained as the normative design any future implementation MUST satisfy before the reserved config knobs may be consumed. For each Active or activating asset (future mechanism):

```text
RecoveryReferencePrice(asset) =
    last authenticated valid effective price accepted under oracle policy

FallbackRecoveryPrice(asset) =
    deterministic immutable configured function of RecoveryReferencePrice(asset)

require 0 < RecoveryReferencePrice <= MAX_ORACLE_PRICE
require 0 < FallbackRecoveryPrice <= MAX_ORACLE_PRICE
require abs(FallbackRecoveryPrice - RecoveryReferencePrice) * 10_000
        <= cfg_max_recovery_fallback_deviation_bps * RecoveryReferencePrice
```

For every allowed portfolio and every possible recovery using fallback prices, initialization/activation MUST prove in exact wide arithmetic:

```text
recovery_value_transfer_bound(account) =
    sum_over_legs ceil(abs(pos_q_leg)
        * abs(FallbackRecoveryPrice(asset) - RecoveryReferencePrice(asset))
        / POS_SCALE)

require recovery_value_transfer_bound(account)
        <= ceil(total_abs_recovery_notional(account)
                * cfg_max_recovery_fallback_deviation_bps / 10_000)
```

The bound is a user-facing value-transfer cap, not a solvency claim. In v16.9.0 (mechanism not implemented), recovery uses only the last authenticated effective price; states where bounded progress cannot continue route to dead-leg forfeiture or terminal recovery/resolution, all proven accounting-neutral. Any future fallback implementation MUST satisfy the envelope above before activation may rely on it; a fallback that would exceed the envelope MUST make no positive junior payout and MUST route to authenticated recovery pricing, dead-leg forfeiture, or terminal recovery preserving senior invariants.

-------------------------------------------------------------------------------
2. Core source-domain credit model
-------------------------------------------------------------------------------

For a profitable leg, its source domain is the opposing side of the same asset:

```text
source_domain(long asset A profit)  = (A, Short)
source_domain(short asset A profit) = (A, Long)
```

Each source domain `D` maintains:

```text
SourceCreditState[D] {
    positive_claim_bound_num            // scaled upper bound of claims owed by D
    exact_positive_claim_num

    fresh_reserved_backing_num          // sum over Fresh buckets of fresh_unliened + valid_liened
    spent_backing_num                   // cumulative audit/cap counter; not a live subtrahend
    provider_receivable_num             // outstanding consumed counterparty backing to be refilled
    valid_liened_backing_num            // sum over Fresh buckets of valid_liened
    impaired_liened_backing_num         // expired/stale lien backing, not usable for new credit

    insurance_credit_reserved_num       // canonical reservation view from InsuranceLedger
    valid_liened_insurance_num          // insurance backing already liened
    impaired_liened_insurance_num

    credit_rate_num                     // in [0, CREDIT_RATE_SCALE]
    credit_epoch
}
```

```text
available_backing_num =
    (fresh_reserved_backing_num - valid_liened_backing_num)
  + (insurance_credit_reserved_num
     - valid_liened_insurance_num
     - impaired_liened_insurance_num)

require available_backing_num >= 0
require 0 <= credit_rate_num <= CREDIT_RATE_SCALE

if positive_claim_bound_num == 0:
    credit_rate_num = CREDIT_RATE_SCALE only if the engine proves
        no exact, bucketed, pending, unresolved, or recovery claim exists for D;
    otherwise credit_rate_num = 0
else:
    credit_rate_num =
        min(CREDIT_RATE_SCALE,
            floor(available_backing_num * CREDIT_RATE_SCALE
                  / positive_claim_bound_num))
```

`fresh_reserved_backing_num` is actual account equity already locked for that source domain and still inside Fresh backing buckets. It is not an optimistic estimate. It cannot be withdrawn, pledged to another domain, used by another instance, or released without a full account refresh and source-credit recomputation. Expired, impaired, or consumed backing is not available for new credit.

`spent_backing_num` is cumulative audit state paired with consumed liens and reduced claims. It MUST NOT be subtracted from `available_backing_num` because consumed backing is already removed from `fresh_reserved_backing_num`. Counting spent backing as both removed from fresh backing and as a live subtrahend is forbidden.

`provider_receivable_num` is the outstanding principal refill owed to consumed counterparty backing for the source domain. It is not usable credit by itself and is not added to `available_backing_num`. Any future source-domain counterparty backing inflow MUST atomically reduce `provider_receivable_num` and the matching bucket `consumed_liened_backing_num` before the inflow is treated as excess new backing. The cumulative `spent_backing_num` is not reduced by refill.

At all times:
```text
fresh_reserved_backing_num = sum_FreshBuckets(fresh_unliened_backing_num + valid_liened_backing_num)
valid_liened_backing_num   = sum_FreshBuckets(valid_liened_backing_num)
provider_receivable_num    = sum_Buckets(consumed_liened_backing_num)
fresh_reserved_backing_num >= valid_liened_backing_num
spent_backing_num >= provider_receivable_num
insurance_credit_reserved_num >= valid_liened_insurance_num + impaired_liened_insurance_num
```

Any transition that changes bucket freshness, lien state, or backing consumption MUST update bucket-local and source-domain aggregates in the same atomic instruction or recompute them from bucket state. Bucket expiry, lien consumption, lien release, and lien impairment MUST NOT increase `available_backing_num`, `credit_rate_num`, or lien creation capacity unless new independently proven backing is added.

### 2.1 Claim bounds

Source-domain positive claim bounds use exact scaled bucket arithmetic:

```text
positive_claim_bound_num[D] =
    exact_positive_claim_num[D]
  + account_base_bound_sum_num[D]
  + sum(bucket.current_upper_bound_num[D])
  + unresolved_recovery_bound_num[D]
```

A bucket upper bound uses the safe per-unit construction below:

```text
unit_profit_bound_num =
    ceil((max(0, favorable_upper_AKF_delta) + uncertainty)
         * BOUND_SCALE / POS_SCALE)

bucket.current_upper_bound_num =
    sum_abs_pos_q * unit_profit_bound_num
  + sum_funding_weight * unit_funding_bound_num
  + stale_uncertainty_bound * BOUND_SCALE
```

For long-profit claims, use long-side best-case price/basis; for short-profit claims, use short-side best-case price/basis. B loss is nonnegative and excluded from positive upper bounds. Bucket members must remain inside stored basis/K/F ranges; otherwise the bucket is recomputed, split, hard-maxed, or the market fails closed. The claim bound MUST never understate true positive claims owed by the source domain.

### 2.2 Counterparty backing and insurance-credit reservations

A full account refresh computes, for every domain where the account currently owes loss, a deterministic `BackingReservationPlan`. Reservation crystallization fires on a REALIZED-loss settlement event (K/F leg settlement, close, bankruptcy booking), not on every mark-to-market refresh: the settlement that reserves the loss also rebases the leg basis (`k_snap`, `f_snap` advanced to the settled point), so the same loss is never re-derived and re-reserved on a later settlement — there is no unrealized-drawdown ratchet and no double-charge. A round-trip that recovers the mark does NOT un-reserve backing (reserve is irreversible per §5.1.1); the recovery instead settles as fresh positive PnL — a new source-attributed claim that realizes against the domain's backing at terminal like any other, so the recovered value returns to the account through realization, not through a (nonexistent) reservation teardown.

A backing reservation may be funded only by:
- senior capital `C_i`;
- already realized nonjunior quote gains;
- released cancel/deposit escrow;
- settled pending-obligation rebate;
- settlement-quality source credit from another domain whose backing is consumed or liened atomically.

Open positive PnL that is not converted into a source-credit lien is not backing. Circular backing is forbidden: a reservation chain MUST strictly consume or lien already available backing and MUST NOT return to a previously visited source domain without external senior capital.

Backing freshness is maintained with bounded buckets. The v16.8 public profile uses exactly one freshness bucket per source domain. Any future multi-bucket profile MUST either refill the bucket holding the consumed receivable or recompute the source aggregate before admitting new backing; it MUST NOT compare a source-wide receivable against an unrelated empty bucket.

```text
BackingBucket[D, expiry_bucket] {
    fresh_unliened_backing_num
    valid_liened_backing_num
    consumed_liened_backing_num       // outstanding consumed principal to be refilled
    impaired_liened_backing_num
    status in {Fresh, Expired, Impaired}
}
```


Lien lifecycle arithmetic:

```text
create_lien_from_counterparty_backing(bucket, amount):
    require bucket.status == Fresh
    require bucket.fresh_unliened_backing_num >= amount
    bucket.fresh_unliened_backing_num -= amount
    bucket.valid_liened_backing_num   += amount
    SourceCreditState.valid_liened_backing_num += amount
    // fresh_reserved_backing_num unchanged

consume_lien_backing(bucket, amount):
    require bucket.valid_liened_backing_num >= amount
    require amount % BOUND_SCALE == 0 when consumed for any quote-atom close/support ledger
    cure_atoms = amount / BOUND_SCALE
    bucket.valid_liened_backing_num     -= amount
    bucket.consumed_liened_backing_num  += amount
    SourceCreditState.valid_liened_backing_num -= amount
    SourceCreditState.fresh_reserved_backing_num -= amount
    SourceCreditState.spent_backing_num += amount
    SourceCreditState.provider_receivable_num += amount
    reduce or finalize the locked source-domain claim in the same atomic step
    record only cure_atoms, never amount, in quote-atom close/support ledgers

add_fresh_counterparty_backing(bucket, amount):
    require amount > 0 and bucket accepts the target freshness epoch
    refill = min(amount, SourceCreditState.provider_receivable_num)
    require refill <= bucket.consumed_liened_backing_num
    bucket.consumed_liened_backing_num -= refill
    SourceCreditState.provider_receivable_num -= refill
    bucket.fresh_unliened_backing_num += amount
    SourceCreditState.fresh_reserved_backing_num += amount
    // spent_backing_num unchanged: it remains cumulative audit state

release_lien_backing(bucket, amount):
    require bucket.valid_liened_backing_num >= amount
    bucket.valid_liened_backing_num   -= amount
    bucket.fresh_unliened_backing_num += amount
    SourceCreditState.valid_liened_backing_num -= amount
    // fresh_reserved_backing_num unchanged

impair_lien_backing(bucket, amount):
    require bucket.valid_liened_backing_num >= amount
    bucket.valid_liened_backing_num    -= amount
    bucket.impaired_liened_backing_num += amount
    SourceCreditState.valid_liened_backing_num -= amount
    SourceCreditState.fresh_reserved_backing_num -= amount
    SourceCreditState.impaired_liened_backing_num += amount
```

A consumed lien MUST NOT remain in `bucket.valid_liened_backing_num`. A released lien MUST NOT remain in `SourceCreditState.valid_liened_backing_num`. An impaired lien MUST NOT remain in fresh backing. These equalities are load-bearing invariants, not implementation suggestions.

Consumed counterparty backing is recoverable principal, not a fee. Refill MUST be deterministic and source-domain local: a future inflow for domain `D` refills `D`'s outstanding consumed backing before it can be interpreted as excess new backing for another domain or another instance. Refill does not resurrect a consumed claim, does not reduce cumulative `spent_backing_num`, and does not create token value; it only moves independently locked backing back into the Fresh bucket while lowering the outstanding receivable.

Backing fee schedules are wrapper/product policy: the wrapper may choose a fee rate from time, utilization, market profile, or provider terms. Any fee that changes account capital, provider accounting, insurance, vault stock, source credit, or backing availability MUST be charged through an engine transition with a balanced `TokenValueFlowProof` and the same source-domain freshness/lien checks. The wrapper MUST NOT hand-edit backing-fee ledger state outside the engine.

Unliened backing in an expired bucket contributes zero. Liened backing in an expiring bucket MUST NOT cause `available_backing_num` underflow or inflation. On expiry, the engine MUST do one of the following before any credit-rate read:
1. refresh and roll the bucket forward with a full account proof;
2. atomically expire the bucket:
   ```text
   expired_unliened = bucket.fresh_unliened_backing_num
   expired_liened   = bucket.valid_liened_backing_num
   expired_total    = expired_unliened + expired_liened

   SourceCreditState.fresh_reserved_backing_num -= expired_total
   SourceCreditState.valid_liened_backing_num   -= expired_liened
   SourceCreditState.impaired_liened_backing_num += expired_liened

   bucket.fresh_unliened_backing_num = 0
   bucket.valid_liened_backing_num = 0
   bucket.impaired_liened_backing_num += expired_liened
   bucket.status = Impaired if expired_liened > 0 else Expired

   // consumed_liened_backing_num is unchanged audit state and MUST already have
   // been removed from source fresh/valid aggregates at consumption time.
   ```
3. route the source domain to recovery.

The expiry/impairment transition is aggregate and bounded by bucket id. Individual liens referencing an impaired bucket become impaired by bucket status and settle later through bounded cranks. Impaired lien backing is not available for new credit and does not count toward `available_backing_num`.

The instruction MUST prove after expiry by independent recomputation from bucket state:
```text
fresh_reserved_backing_num_after = sum_FreshBuckets(fresh_unliened + valid_liened)
valid_liened_backing_num_after   = sum_FreshBuckets(valid_liened)
available_backing_num_after      = recomputed_fresh_unliened + available_insurance_credit
available_backing_num_after <= available_backing_num_before
credit_rate_num_after <= credit_rate_num_before unless positive_claim_bound_num also decreased by an independently valid bounded recomputation
```
A pure expiry transition that increases available backing or credit rate is invalid and MUST revert or recover. A before/after inequality alone is insufficient unless the recomputed aggregate equalities also hold.

Insurance may contribute to source credit only through a canonical live insurance-credit reservation recorded in `InsuranceLedger.source_credit_reserved_num[D]`. `SourceCreditState[D].insurance_credit_reserved_num` is a derived view of that canonical ledger entry, not a second writer.

```text
amount_from_bound_num_up(x_num) = ceil(x_num / BOUND_SCALE)

InsuranceCreditReservation[D] {
    insurance_credit_reserved_num       // canonical live reservation, scaled
    valid_liened_insurance_num          // valid liens funded from this reservation
    impaired_liened_insurance_num       // impaired liens still encumbering this reservation
    consumed_insurance_num              // cumulative audit, no longer live
    source_credit_epoch
}
```

Granting or increasing `insurance_credit_reserved_num[D]` MUST atomically reserve from the domain's unspent current insurance capacity. Source-credit insurance reservations MUST NOT be drawn from global protocol first-loss capacity unless the reservation is explicitly recorded in a separate global reservation field and included in the same live-encumbrance invariant. The same insurance atom cannot simultaneously be:
- a source-credit insurance reservation;
- staged residual insurance;
- spent residual insurance;
- available global protocol first-loss budget; or
- available domain insurance budget.

`insurance_ledger.total_available` is current unspent insurance capital in the vault. Cumulative spent insurance is reflected by a lower `I`/`total_available`; it MUST NOT also be counted as a live encumbrance.

Live insurance encumbrance:

```text
live_source_credit_insurance =
    sum_D amount_from_bound_num_up(source_credit_reserved_num[D])

live_domain_staged =
    sum_D staged_domain_insurance_debits[D]

live_global_staged =
    global_protocol_staged_debits

live_source_credit_insurance + live_domain_staged + live_global_staged
    <= insurance_ledger.total_available
```

Per-domain cap:

```text
domain_spent[D]
  + staged_domain_insurance_debits[D]
  + amount_from_bound_num_up(source_credit_reserved_num[D])
  <= domain_budget[D]
```

Insurance-backed lien lifecycle arithmetic mirrors counterparty-backed lien arithmetic. All amounts below are scaled insurance reservation numerators and MUST be integer multiples of `BOUND_SCALE` whenever they move or release quote-atom value.

```text
create_lien_from_insurance(reservation, amount):
    require amount % BOUND_SCALE == 0
    require reservation.insurance_credit_reserved_num
        >= reservation.valid_liened_insurance_num
         + reservation.impaired_liened_insurance_num
         + amount
    reservation.valid_liened_insurance_num += amount
    SourceCreditState.valid_liened_insurance_num += amount
    // insurance_credit_reserved_num unchanged; available insurance credit decreases by amount

consume_lien_from_insurance(reservation, amount):
    require reservation.valid_liened_insurance_num >= amount
    require amount % BOUND_SCALE == 0
    spend_atoms = amount / BOUND_SCALE

    reservation.valid_liened_insurance_num -= amount
    reservation.insurance_credit_reserved_num -= amount
    reservation.consumed_insurance_num += amount

    InsuranceLedger.source_credit_reserved_num[D] -= amount
    SourceCreditState.valid_liened_insurance_num -= amount
    // SourceCreditState.insurance_credit_reserved_num view decreases with the canonical ledger

    InsuranceLedger.domain_spent[D] += spend_atoms
    InsuranceLedger.total_available -= spend_atoms
    I -= spend_atoms
    if the consume instruction pays external quote tokens:
        V -= spend_atoms
        record external_insurance_payout in the TokenValueFlowProof
    else:
        record exactly one internal quote-value credit in the TokenValueFlowProof and close/payout state:
            - CloseProgressLedger.support_consumed for source-credit-lien residual cure
              (the insurance funding side is the I/insurance-budget debit recorded
              by the CloseInsuranceSpent flow class); or
            - staged_domain_insurance_debit for staged close insurance; or
            - ResolvedPayoutLedger.paid_effective for resolved/recovery payout.
        The same consume MUST NOT increment more than one residual-partition
        category, and MUST NOT also be booked as direct insurance_spent.

    reduce or finalize the locked source-domain claim in the same atomic step
    require all senior and quote-value and reservation-conservation invariants hold after the debit

release_lien_from_insurance(reservation, amount):
    require reservation.valid_liened_insurance_num >= amount
    require amount % BOUND_SCALE == 0
    reservation.valid_liened_insurance_num -= amount
    SourceCreditState.valid_liened_insurance_num -= amount
    // insurance_credit_reserved_num unchanged; available insurance credit increases by amount

impair_lien_from_insurance(reservation, amount):
    require reservation.valid_liened_insurance_num >= amount
    require amount % BOUND_SCALE == 0
    reservation.valid_liened_insurance_num -= amount
    reservation.impaired_liened_insurance_num += amount
    SourceCreditState.valid_liened_insurance_num -= amount
    SourceCreditState.impaired_liened_insurance_num += amount
    // insurance_credit_reserved_num unchanged; impaired amount remains encumbered and unavailable

recover_or_reconcile_impaired_insurance_lien(reservation, amount, outcome):
    require reservation.impaired_liened_insurance_num >= amount
    require amount % BOUND_SCALE == 0
    if outcome == Released:
        reservation.impaired_liened_insurance_num -= amount
        reservation.insurance_credit_reserved_num -= amount
        InsuranceLedger.source_credit_reserved_num[D] -= amount
        SourceCreditState.impaired_liened_insurance_num -= amount
    if outcome == Consumed:
        reservation.impaired_liened_insurance_num -= amount
        reservation.insurance_credit_reserved_num -= amount
        InsuranceLedger.source_credit_reserved_num[D] -= amount
        SourceCreditState.impaired_liened_insurance_num -= amount
        spend_atoms = amount / BOUND_SCALE
        InsuranceLedger.domain_spent[D] += spend_atoms
        InsuranceLedger.total_available -= spend_atoms
        I -= spend_atoms
        if the recovery/settlement transfer pays external quote tokens:
            V -= spend_atoms
            record external_insurance_payout in the TokenValueFlowProof
        else:
            record exactly one internal recovery/close quote-value credit in the TokenValueFlowProof
        preserve senior and quote-value and reservation-conservation invariants
```

At all times:

```text
insurance_credit_reserved_num
    >= valid_liened_insurance_num + impaired_liened_insurance_num

InsuranceLedger.source_credit_reserved_num[D]
    == InsuranceCreditReservation[D].insurance_credit_reserved_num
    == SourceCreditState[D].insurance_credit_reserved_num
```

`impaired_liened_insurance_num` is a live encumbrance and MUST be subtracted from available insurance credit. It is unavailable for new liens until explicitly released or consumed by recovery. A transition that moves an insurance-backed lien between valid, impaired, consumed, and released states MUST independently recompute:

```text
available_insurance_credit_num =
    insurance_credit_reserved_num
  - valid_liened_insurance_num
  - impaired_liened_insurance_num
```

and MUST NOT increase available insurance credit or credit rate unless the transition is a genuine release or a new insurance reservation is added.

`SourceCreditLien` records the backing source:

```text
SourceCreditLien {
    account_id
    source_domain
    face_claim_locked
    effective_credit_reserved
    backing_reserved
    backing_source in {CounterpartyBucket, InsuranceReservation}
    backing_bucket_id optional
    insurance_reservation_id optional
    credit_rate_num_at_creation
    credit_epoch
    status in {Valid, Impaired, Consumed, Released}
    purpose in {Risk, Withdrawal, Conversion, Fee, ResidualCure, Payout}
}
```

Creating a lien atomically:
1. verifies the account has un-liened positive claim face in that source domain;
2. computes required face and backing;
3. requires `credit_rate_num > 0`;
4. requires `required_backing <= available_backing_num`;
5. locks face claim so it cannot be reused for soft credit, another lien, or another instance;
6. chooses a deterministic backing source:
   - if `CounterpartyBucket`, call `create_lien_from_counterparty_backing` and record `backing_bucket_id`;
   - if `InsuranceReservation`, call `create_lien_from_insurance` and record `insurance_reservation_id`;
7. records credit epoch and purpose.

For effective credit `E` measured in quote atoms:

```text
required_face_num = ceil(E * BOUND_SCALE * CREDIT_RATE_SCALE / credit_rate_num)
required_backing_num = E * BOUND_SCALE
```

A lien can be released only by reversing the dependent risk, consuming it into settlement, or recovery reconciliation. If a counterparty backing bucket expires or insurance backing becomes impaired, the lien becomes `Impaired`. Insurance backing has no time-expiry bucket; it becomes impaired only by deterministic events: source-domain Recovery, market-group Recovery, insurance-reservation invariant failure, domain/global insurance cap exhaustion affecting the reservation, or governance-declared insurance impairment routed through recovery. Recovery MUST call `impair_lien_from_insurance` or `recover_or_reconcile_impaired_insurance_lien` for affected insurance-backed liens before any favorable action can use that source domain. An impaired lien cannot support new risk or payout and adds an impaired-lien penalty to the owning account until it deleverages, liquidates, ADLs, refreshes with new backing, or recovers.

Locked face claim MUST be excluded from soft maintenance credit and from any further lien calculation. This prevents the same positive PnL from being counted once as soft equity and again as liened equity.

Close/support classification for source-credit liens:

```text
if backing_source == CounterpartyBucket and purpose == ResidualCure:
    consumed scaled backing amount is converted to cure_atoms = amount / BOUND_SCALE
    cure_atoms is recorded EXACTLY ONCE as source-credit support (support_consumed)
    and MUST NOT be recorded as insurance_spent or as any additional
    counterparty-specific partition subtrahend.

if backing_source == CounterpartyBucket and purpose in {Withdrawal, Conversion, Fee, Payout}:
    consumed scaled backing amount is converted to support_atoms = amount / BOUND_SCALE
    support_atoms is recorded as counterparty source-credit support for the exact
    account-capital, fee, or payout credit being created.
    This support MUST be matched by a prior or same-instruction account-capital
    to realized-loss/backing reservation for the same source domain. Consuming
    the lien removes already-reserved backing, increments the source-domain
    provider receivable, and proves the supported credit as
    CloseCounterpartyCreditConsumed -> AccountCapital or an exactly equivalent
    value-flow class. It MUST NOT debit V, I, insurance_spent, or any insurance
    ledger. Any later external token payout MUST use the ordinary account-capital
    or payout-ledger debit and vault debit path; counterparty backing consumption
    alone never sends quote tokens out of the vault.

if backing_source == InsuranceReservation and purpose == ResidualCure:
    cure_atoms is recorded EXACTLY ONCE as source-credit support (support_consumed);
    the insurance funding is debited from I and the domain insurance budget through
    the CloseInsuranceSpent flow class and MUST NOT also be booked as direct
    insurance_spent or any second partition category.

if backing_source == InsuranceReservation and purpose in {Withdrawal, Conversion, Fee, Payout}:
    consumed value is recorded as external insurance-backed payout/spend
    with the matching V/I/insurance-ledger debit.
```

Every consumed lien MUST be classified by `backing_source` before it mutates any close ledger. A lien consumption that would increment two residual-cure categories, or none, is an invariant failure and MUST revert or route to recovery.

Insurance-backed lien impairment triggers:
- source domain enters `Recovery` or `DrainOnly` with the reservation not proven usable;
- the insurance reservation is invalidated, suspended, or no longer within domain/global caps;
- market group enters `Recovery` and the reservation is not explicitly preserved;
- recovery marks the backing unavailable.

Insurance does not expire by time unless an explicit configured expiry policy exists. If such a policy exists, expiry MUST call `impair_lien_from_insurance` or release/consume the lien in the same bounded step.

### 2.4 Soft maintenance credit

Maintenance may use soft source credit without reserving a lien:

```text
soft_leg_credit =
    floor(leg_local_positive_value * credit_rate_num[source_domain]
          / CREDIT_RATE_SCALE)
```

Soft credit is recomputed on every full refresh and every favorable action. It creates no payout right and no durable support. If the source rate falls, health falls immediately.

Trade approval that increases risk MUST create liens for any positive credit beyond no-positive-credit equity. Purely risk-reducing trades may use soft credit only for validation.

-------------------------------------------------------------------------------
3. Asset lifecycle
-------------------------------------------------------------------------------

Asset slots are bounded by `N`:

```text
Disabled -> PendingActivation -> Active -> DrainOnly -> Retired
                                      \-> Recovery -> Retired
```

Activation requires:
- slot Disabled or Retired;
- no remaining OI, weights, B, K/F, claims, backing, liens, pending barriers, pending obligations, close ledgers, or stale accounts in the slot;
- oracle, price, funding, B-headroom, claim-bound, backing, close-progress, and portfolio-envelope proofs pass for the whole instance;
- support weight exactly `FULL_SUPPORT_WEIGHT`;
- activation cooldown satisfied;
- `config_hash`, `risk_epoch`, and `asset_set_epoch` incremented;
- certificates fail closed unless their schema explicitly excludes the new asset.

DrainOnly blocks risk increase and new attaches. Retired requires zero OI, zero stored positions, no pending barriers, no obligations, no liens, all close ledgers finalized/canceled, and all prior-epoch stale accounts settled/migrated/recovered. A `ResetPending` side cannot reset again until all prior-epoch stale accounts are settled, migrated, or recovered.

-------------------------------------------------------------------------------
4. State
-------------------------------------------------------------------------------

```text
MarketGroup {
    instance_id
    V, I, C_tot
    materialized_portfolio_count_unbounded_counter

    risk_epoch
    oracle_epoch
    funding_epoch
    asset_set_epoch
    current_slot

    assets[0..N)
    source_credit_ledger[(asset, side)]
    source_credit_liens
    domain_locks[(asset, side)]
    insurance_ledger
    close_progress_ledger
    pending_domain_loss_barriers[(asset, side)]
    pending_obligation_aggregates[(barrier_id)]
    pending_obligation_ledger
    resolved_payout_ledger optional
    global_stale_penalty_params
    mode in {Live, Resolved, Recovery}
}
```


```text
InsuranceLedger {
    total_available                         // current unspent insurance capital in the vault
    domain_budget[(asset, side)]            // per-domain cap
    domain_spent[(asset, side)]             // cumulative spent for cap/audit only
    domain_global_cap[(asset, side)]
    domain_global_spent[(asset, side)]      // cumulative global first-loss spend by domain
    staged_domain_insurance_debits[(asset, side)]
    global_protocol_budget
    global_protocol_spent                   // cumulative spent for cap/audit only
    global_protocol_staged_debits
    source_credit_reserved_num[(asset, side)]   // canonical live source-credit insurance reservation
}
```

Insurance-credit invariants:

```text
live_source_credit_insurance =
    sum_D amount_from_bound_num_up(source_credit_reserved_num[D])

live_domain_staged =
    sum_D staged_domain_insurance_debits[D]

live_source_credit_insurance + live_domain_staged + global_protocol_staged_debits
    <= total_available

for every D:
    domain_spent[D]
  + staged_domain_insurance_debits[D]
  + amount_from_bound_num_up(source_credit_reserved_num[D])
  <= domain_budget[D]

global_protocol_spent + global_protocol_staged_debits
    <= global_protocol_budget
```

`InsuranceLedger.source_credit_reserved_num[D]` is canonical. `SourceCreditState[D].insurance_credit_reserved_num` MUST be read as a derived view or updated only by the same helper that mutates the insurance ledger. A desynchronized duplicate value is an invariant failure.


```text
Asset {
    lifecycle
    raw_oracle_target_price
    effective_price
    fund_px_last
    slot_last

    A_long, A_short
    K_long, K_short
    F_long_num, F_short_num

    B_long_num, B_short_num
    B_epoch_start_long_num, B_epoch_start_short_num
    K_epoch_start_long, K_epoch_start_short
    F_epoch_start_long_num, F_epoch_start_short_num
    A_epoch_start_long, A_epoch_start_short

    OI_eff_long, OI_eff_short
    stored_pos_count_long, stored_pos_count_short
    stale_account_count_long, stale_account_count_short

    loss_weight_sum_long, loss_weight_sum_short
    social_loss_remainder_long_num, social_loss_remainder_short_num
    social_loss_dust_long_num, social_loss_dust_short_num
    explicit_unallocated_loss_long, explicit_unallocated_loss_short

    support_weight = FULL_SUPPORT_WEIGHT when Active
    recovery_reference_price
    fallback_recovery_price
    recovery_fallback_deviation_bps
    epoch_long, epoch_short
    mode_long, mode_short in {Normal, DrainOnly, ResetPending}
}
```

```text
PortfolioAccount {
    owner
    instance_id
    market_group_id
    config_hash_at_open

    C_i
    PNL_i
    R_i                         // live released positive PnL face
    fee_credits_i <= 0 and != i128::MIN

    active_bitmap
    legs[0..N)
    account_claim_bound_contributions
    source_credit_lien_keys[0..bounded]

    health_cert
    stale_state
    positive_credit_lock
    rebalance_lock
    liquidation_lock
    cancel_deposit_escrow
    portfolio_close_state optional
}
```

Each account has at most one canonical signed net leg per asset. Same-asset opposite exposure MUST net into that leg.

-------------------------------------------------------------------------------
5. Global invariants
-------------------------------------------------------------------------------

```text
C_tot <= V <= MAX_VAULT_TVL
I <= V
V >= C_tot + I
0 < effective_price(asset) <= MAX_ORACLE_PRICE for Active/DrainOnly/Recovery assets
0 < fund_px_last(asset) <= MAX_ORACLE_PRICE for Active/DrainOnly/Recovery assets
asset.slot_last <= current_slot
insurance_ledger.total_available <= I
insurance_ledger.total_available is current unspent insurance capital, not original insurance principal
```

For every source domain:

```text
positive_claim_bound_num >= true positive claims owed by source domain * BOUND_SCALE
fresh_reserved_backing_num = sum_FreshBuckets(fresh_unliened_backing_num + valid_liened_backing_num)
valid_liened_backing_num   = sum_FreshBuckets(valid_liened_backing_num)
provider_receivable_num    = sum_Buckets(consumed_liened_backing_num)
fresh_reserved_backing_num >= valid_liened_backing_num
spent_backing_num >= provider_receivable_num
spent_backing_num is cumulative audit state and is not a live available-backing subtrahend
insurance_credit_reserved_num is backed by a unique canonical insurance-credit reservation
valid_liened_insurance_num + impaired_liened_insurance_num <= insurance_credit_reserved_num
amount_from_bound_num_up(insurance_credit_reserved_num) is included in live insurance encumbrance
available insurance credit equals insurance_credit_reserved_num - valid_liened_insurance_num - impaired_liened_insurance_num
0 <= credit_rate_num <= CREDIT_RATE_SCALE
available_backing_num =
    (fresh_reserved_backing_num - valid_liened_backing_num)
  + (insurance_credit_reserved_num
     - valid_liened_insurance_num
     - impaired_liened_insurance_num)
available_backing_num >= 0
impaired_liened_backing_num and impaired_liened_insurance_num are not counted in available_backing_num
insurance-backed liens satisfy the same create/consume/release/impair conservation as counterparty-backed liens
valid_liened_insurance_num and impaired_liened_insurance_num are backed by canonical insurance reservations
all non-expired backing buckets correspond to account reservations that cannot withdraw elsewhere
expired buckets are removed from fresh_reserved_backing_num if unliened or moved from fresh/valid liened backing to impaired liened backing if liened
consumed liens are removed from bucket.valid_liened_backing_num before any bucket expiry
pure expiry never increases available_backing_num or credit_rate_num
```

For every Active/DrainOnly/Recovery asset side:

```text
0 < A_side <= ADL_ONE
if side is Normal and has current-epoch stored positions: A_side >= MIN_A_SIDE
0 <= OI_eff_side <= MAX_OI_SIDE_Q_PER_ASSET
if Live: OI_eff_long == OI_eff_short
if OI_eff_side > 0 and side is not ResetPending: loss_weight_sum_side > 0
if loss_weight_sum_side == 0: residual may clear only via fully backed protocol-owned explicit loss
0 <= loss_weight_sum_side <= SOCIAL_LOSS_DEN
social_loss_remainder_side_num < SOCIAL_LOSS_DEN
social_loss_dust_side_num < SOCIAL_LOSS_DEN
```

```text
abs(K_side) + A_side * MAX_ORACLE_PRICE <= i128::MAX
abs(F_side_num) + A_side * MAX_ORACLE_PRICE * cfg_max_abs_funding_e9_per_slot * cfg_max_accrual_dt_slots <= i128::MAX
B_side_num <= u128::MAX
```

No lien, backing reservation, claim, certificate, or pending obligation may reference another instance.

-------------------------------------------------------------------------------
5.1 Quote-value flow, reservation encumbrance, and stock reconciliation
-------------------------------------------------------------------------------

Every top-level instruction that moves quote-token value internally or externally MUST produce a `TokenValueFlowProof` over exact quote atoms. The proof is a double-entry value ledger, not an encumbrance ledger.

```text
external_quote_in - external_quote_out = ΔV
token_value_debits == token_value_credits
```

Token value classes include only quote-token value or claim buckets backed by quote-token value, for example:

```text
TokenVault
SeniorCapital
InsuranceCapital
AccountCapital
CloseSupportConsumed
CloseInsuranceSpent
CloseCounterpartyCreditConsumed
BResidualBooked
PendingObligationEscrow
PendingObligationCredit
ExplicitBackedLoss
SettlementRoundingResidue
CancelDepositEscrow
ResolvedPayoutPaid
ProtocolFeePaid
FeesForgiven
RecoverySettlement
ExternalQuoteIn
ExternalQuoteOut
UnallocatedProtocolSurplus
```

The following are **not** token value classes and MUST NOT appear in `TokenValueFlowProof`:

```text
SourceCounterpartyBacking
SourceInsuranceReservation
SourceCreditLienCounterparty
SourceCreditLienInsurance
BackingBucketFresh
BackingBucketLiened
BackingBucketImpaired
ClaimBoundBucket
```

Those labels are encumbrance, reservation, or bound state over already-counted quote-token value. Lien creation, lien release, bucket refresh, claim-bound update, and reservation relabeling usually move zero quote-token value. They MUST produce a `ReservationEncumbranceProof`, not a token value debit/credit.

`ReservationEncumbranceProof` is checked in the native units of each ledger and MUST NOT mix quote atoms with `BOUND_SCALE` numerators. It proves, for example:

```text
fresh_reserved_backing_num == sum(Fresh bucket fresh_unliened + valid_liened)
valid_liened_backing_num == sum(Fresh bucket valid_liened)
provider_receivable_num == sum(bucket consumed_liened_backing_num)
spent_backing_num >= provider_receivable_num
impaired_liened_backing_num == sum(Impaired bucket impaired_liened)
insurance_credit_reserved_num >= valid_liened_insurance_num + impaired_liened_insurance_num
source_credit_reserved_num is unique and canonical in InsuranceLedger
claim_bound_bucket sums match their scaled formula terms
```

A transition touching both value and encumbrance state MUST provide both proofs. For example:

```text
create_lien_from_counterparty_backing:
    TokenValueFlowProof: no token value moved
    ReservationEncumbranceProof: backing relabeled from fresh_unliened to valid_liened

consume_lien_from_insurance for external payout:
    TokenValueFlowProof: InsuranceCapital or TokenVault debited, ExternalQuoteOut credited
    ReservationEncumbranceProof: insurance reservation/lien counters reduced consistently

consume_lien_from_insurance for residual cure:
    TokenValueFlowProof: InsuranceCapital debited, CloseInsuranceSpent credited
    ReservationEncumbranceProof: insurance reservation/lien counters reduced consistently
```

Rounding residue rule:

```text
For any exact quote-token amount X split into rounded allocations A_1..A_k:
    require each A_j is rounded against the account or claimant receiving value
    residue = X - sum(A_j)
    require residue >= 0
    TokenValueFlowProof credits residue to SettlementRoundingResidue or UnallocatedProtocolSurplus
```

If a rounding method would produce `sum(A_j) > X`, the payer MUST be charged the exact larger amount under conservative rounding or the instruction MUST revert. Silent negative residue is forbidden.

`SettlementRoundingResidue` is protocol-owned dust. It may be swept only into `UnallocatedProtocolSurplus` through a balanced `TokenValueFlowProof`. It MUST NOT be used as account health, source-credit backing, insurance credit, resolved payout entitlement, hedge collateral, or senior capital. This is the only sanctioned non-fee source of `unallocated_protocol_surplus` growth.


For every close ledger entry, residual partition is an equality over quote atoms:

```text
remaining_residual =
    gross_loss_at_close_start
  + total_adverse_drift_from(drift_reference_slot, now)
  - support_consumed
  - insurance_spent
  - b_loss_booked
  - explicit_loss_assigned
  - pending_obligation_credits
```

Every cured quote atom appears in exactly one partition category. `support_consumed` includes ALL source-credit lien consumption used for residual cure — counterparty-backed and insurance-backed alike — each consumed lien recorded exactly once; there is no separate `consumed_counterparty_credit_lien_backing` subtrahend (a partition listing one would double-subtract every counterparty-lien cure that also flows through the support pool). `insurance_spent` records direct insurance allocation only. The categories are pairwise disjoint: a residual-cure instruction that books one consumed lien into two partition categories, or into none, MUST revert. The insurance-funding side of an insurance-backed lien cure is recorded exactly once by the insurance stock debit, the domain insurance budget ledger, and the `CloseInsuranceSpent` value-flow class — never as a second residual-partition subtraction. Structurally, the total residual reduction across categories MUST equal the value the close's `TokenValueFlowProof`s actually moved or recognized; subtracting more from residual than the flow proofs account for is an invariant failure.

Accrual and mark updates that only change unrealized PnL, K/F indexes, effective prices, claim bounds, or credit rates normally move no quote-token value. They MUST provide the relevant reservation/bound proofs and may have an empty zero-value `TokenValueFlowProof`.

### 5.1.1 Stock reconciliation

At genesis, asset activation, mode transition, recovery entry/exit, resolved-payout initialization, and any instruction that touches insurance, external quote flow, close finalization, or recovery settlement, the engine MUST also check direct stock reconciliation.

```text
V =
    C_tot
  + I
  + counterparty_backing_principal
  + backing_provider_earnings
  + cancel_deposit_escrow_total
  + pending_obligation_escrow_total
  + close_staged_quote_reserve_total
  + resolved_payout_escrow_total
  + explicit_backed_loss_reserve_total
  + settlement_rounding_residue_total
  + protocol_fee_payable_total
  + unallocated_protocol_surplus
  + junior_residual_pool
```

`junior_residual_pool` is the defined CLOSING class of the equality: the vault value that funds junior positive-PnL payouts and haircut support. It has no independent counter — its O(1) ledger IS the residual computation `V − (all other configured classes)`, and per-class reconciliation (proof 102) derives it rather than cross-checking it. Naming it makes the equality total: every transition that removes atoms from another class without moving external tokens credits the junior pool implicitly and exactly, so "no listed class rose" is never an orphaning. `backing_provider_earnings` follows the same lockstep discipline: crediting provider earnings draws `junior_residual_pool -= X ; backing_provider_earnings += X` (vault flat, gated on the pool actually covering it), and earnings withdrawal is external (`backing_provider_earnings -= X ; V -= X`).

The exact classes included in stock reconciliation are implementation-configured but immutable after initialization. Every quote atom in `V` MUST appear in exactly one stock class. A configuration whose close paths credit internal transit value (`CloseSupportConsumed`, `CloseInsuranceSpent`, `CloseCounterpartyCreditConsumed`, `BResidualBooked`) MUST include those transit classes in its configured set (or prove them zero at every reconciliation point).

`counterparty_backing_principal` is a FIRST-CLASS persistent quote-atom stock class — the equation above is authoritative, not just a senior-stack inequality. It holds the quote atoms physically resting behind every Fresh counterparty backing bucket (its quantity equals `Σ fresh_reserved_backing_num / BOUND_SCALE`; impaired and consumed encumbrance counters are NOT in this class — their atoms have already moved to another class per the lifecycle below). This resolves the apparent conservation gap in counterparty-backed realization: when realization credits `C_tot` by `X` with `V` flat, the funding debit is named — `counterparty_backing_principal` drops by exactly `X` — so the equality is preserved by an intra-stock transfer, not by minting senior capital. EVERY transition that changes `Σ fresh_reserved_backing_num` has exactly one lockstepped stock movement; the lifecycle is closed and conservative:

```text
external deposit (provider):    V += X ; counterparty_backing_principal += X
reserve (capital-backed loss):  C_tot -= X ; counterparty_backing_principal += X ; V flat
                                (the persistent stock movement; the flow proof's
                                 transit label for the capital debit is
                                 ExplicitBackedLoss — the transit label does NOT
                                 name the destination class, which is always
                                 counterparty_backing_principal at reserve)
lien create / lien un-pledge:   NO stock movement (encumbrance relabel between
                                fresh_unliened and valid_liened inside the same
                                class; ReservationEncumbranceProof only — this is
                                the helper named release_lien, and it must not be
                                confused with any teardown)
consume - realize:              counterparty_backing_principal -= X ; C_tot += X ; V flat
                                (transit class: CloseCounterpartyCreditConsumed)
consume - counterparty cure:    counterparty_backing_principal -= X ;
                                junior_residual_pool += X (the cured loss no longer
                                drains the pool) ; V flat ; partition category:
                                support_consumed. NO funded-stock debit -> NO
                                TokenValueFlowProof (pure reclassification, same
                                proof family as forfeit below)
consume - insurance cure:       I -= X ; junior_residual_pool += X ; V flat ;
                                partition category: support_consumed ; funded by
                                an insurance-stock debit -> balanced
                                TokenValueFlowProof, transit CloseInsuranceSpent
forfeit (impair or expiry):     counterparty_backing_principal -= X ;
                                junior_residual_pool += X ; V flat
                                (impaired/expired encumbrance counters keep audit
                                 history but hold no stock)
external withdrawal (provider): counterparty_backing_principal -= X ; V -= X
```

There is NO internal teardown that returns reserved backing to the loser's `C_tot`. Reserve crystallizes a realized loss — the loser's negative PnL is offset at reserve time, so the principal thereafter belongs to the claim side until consumed, forfeited, or externally withdrawn. A "release reservation to capital" operation does not exist and MUST NOT be synthesized; the only helper named "release" is the lien un-pledge relabel above, which moves no stock.

Across reserve→realize the loser's senior capital funds the winner's payout: `C_tot` nets zero, `counterparty_backing_principal` nets zero, `V` is flat throughout. Across impair/expiry the forfeited principal lands, atom for atom, in `junior_residual_pool` (proven: `counterparty_lien_impair`/`backing_expiry` are value-neutral on `V/C_tot/I` and raise the residual pool by exactly the forfeit). `counterparty_backing_principal` MUST NOT be counted in the junior residual pool used for haircut support or the resolved payout snapshot — atoms are in exactly one of the two classes at any time, and only forfeit/cure transitions move them from principal to pool.

Flow-proof obligation is keyed on the FUNDED-STOCK DEBIT, not on the destination. A transition that debits NO funded stock class (no `V`, `I`, or `C_tot` movement) — the impair/expiry forfeits AND counterparty residual cure — does not emit a `TokenValueFlowProof`: it only reclassifies `counterparty_backing_principal` into the derived closing class `junior_residual_pool` (which has no independent counter, so a flow credit to it would be double bookkeeping). These moves are proven by their `ReservationEncumbranceProof`, the lockstepped O(1) aggregate delta on `counterparty_backing_principal`, and the residual identity re-checked at the same transition (the impair/expiry witnesses assert all three; the counterparty-cure ledger advance asserts the partition equality). Every transition that DOES debit a funded stock class still requires its balanced `TokenValueFlowProof`: reserve (`C_tot` debit, transit `ExplicitBackedLoss`), realize and external withdrawal (`C_tot`/`V`), insurance cure (`I` debit, transit `CloseInsuranceSpent`), and external deposit. This is why requirement 12's "MUST … through a balanced `TokenValueFlowProof`" applies to the insurance-backed cure (it debits `I`) but not to the counterparty cure (pure reclassification): the two cure branches differ precisely in whether a funded stock class moves.

Note the typing distinction that makes this legal under requirement 13's flow-proof ban: the BANNED labels are the `BackingBucket` ENCUMBRANCE counters (`fresh_unliened_backing_num`, `valid_liened_backing_num`, …), which are an encumbrance VIEW proven only by `ReservationEncumbranceProof` and never appear in a `TokenValueFlowProof`. The `counterparty_backing_principal` STOCK class and the `CloseCounterpartyCreditConsumed` / `ExplicitBackedLoss` TRANSIT classes are quote-atom value classes and DO appear in flow proofs and stock reconciliation. Backing realization therefore moves real quote value through declared value classes, while the encumbrance bookkeeping moves in lockstep through the separate reservation proof.

The resolved-payout exclusion of `counterparty_backing_principal` from the junior pool is sound only because outstanding source-credit claims realize against their backing at terminal: at resolved close, BEFORE an account's positive claim face enters the resolved payout receipt pool, the engine MUST realize the account's source-credit claims against their domain backing at the current credit rate. The realization sequence is: (1) RELEASE the account's pre-existing liens (un-pledge `valid_liened -> fresh_unliened`, returning the backing to Fresh); (2) CREATE-AND-CONSUME a fresh lien against that now-unliened backing for the realizable face, atomically, crediting account capital. It does NOT consume a released lien (the lien state machine forbids consuming a Released lien); release and create-and-consume act on different liens. The un-pledge step exists only so the realizable estimate matches what fresh consumption can actually deliver (a still-pledged lien would over-estimate and dead-lock the close). The realization is conservation-exact (`counterparty_backing_principal -= X`, `C_tot += X`, `V` unchanged), so the payout snapshot does not depend on realization order, and only backing net of realizable claims is provider-recoverable. The insurance-backed branch balances symmetrically (`I -= X`, `C_tot += X`, transit `CloseInsuranceSpent`).

Realization EXTENT and no-double-pay are exact. For a domain claim of face `F` backed at credit rate `r`, realization consumes the backed face and credits realizable value `floor(r·F)` to account capital; the consumed face is burned from the account's positive PnL in the SAME step, so it can never also enter the receipt pool. The unbacked shortfall `F − floor(r·F)` is NOT paid to this winner — the realizable-limited thesis holds that unbacked positive PnL is not the winner's value. It is NOT stranded either: the realized face is refined out of the junior bound (`refine_resolved_unreceipted_bound`), so the shortfall returns to `junior_residual_pool` and is redistributed to the remaining junior claimants through the higher post-refine haircut rate. Conservation is exact (verified across the full backing range, including the dust regime, by `backed_winner_close_conserves_across_all_backing_levels`): `total_paid(account) ≤ original_face(account)`, the vault never over-drains or strands, and the close always finalizes (no DoS). Realization is idempotent: a second realization finds the source claims consumed and the backing spent, so it credits zero.

DISTRIBUTION CHARACTERIZATION (not a solvency property; a deliberate design choice): payout is realizable-limited, so it is NOT monotone in backing at the boundary. A claim with zero backing realizes nothing (rate 0 → realize is skipped) and its full face enters the receipt pool, drawing the junior haircut; a claim with dust backing realizes the dust and refines the rest back to the pool, so its OWN payout is ~dust while the rest of its face benefits the other junior claimants. This is the intended consequence of "usable PnL is limited to realizable source-domain backing" — it is conservation-safe and griefer-funded (raising a domain's backing requires the griefer to deposit real atoms via `deposit_fresh_counterparty_backing`, which fund the very payout), not a loss-of-funds or DoS. Order-independence holds for the SNAPSHOT (residual and bound at capture), not for the exact per-account payment, which can depend on whether a lazy receipt realizes before or after a bucket expiry; the snapshot invariance is what the conservation proofs assert. Stripping a claim that was realizable in Live by resolving the market is forbidden. If a deployment instead keeps backing principal inside `C_tot`, it MUST rewrite both the senior-stack relation and the realization steps as intra-`C_tot` transfers (loser `−X`, winner `+X`) and drop `counterparty_backing_principal` as a separate class — but it MUST NOT mix the two models. `settlement_rounding_residue_total` and `unallocated_protocol_surplus` are protocol-owned, non-user-claim value and MUST NOT be used as proof of account health, source-credit backing, insurance credit, hedge collateral, or payout entitlement unless explicitly moved through a balanced `TokenValueFlowProof` and all senior invariants remain true.

Public instructions that do not touch quote-token stock classes MAY rely on a valid prior stock reconciliation plus a balanced zero-value flow proof. Recovery MUST perform direct stock reconciliation before clearing recovery state. A balanced flow proof alone is insufficient if stock classes are not already reconciled.

This equality is checked before commit together with global invariants. Recovery MUST preserve or reconcile both proof families; it cannot repair an unbalanced transition by silently dropping value.

Where an O(1) source-of-truth ledger exists, the stock class total MUST also reconcile to that ledger. Examples:

```text
I == InsuranceLedger.total_available + insurance spent/committed classes as configured
cancel_deposit_escrow_total == sum(cancel escrow ledger)
pending_obligation_escrow_total == sum(pending obligation escrow ledger)
close_staged_quote_reserve_total == sum(funded close reserve ledger)
settlement_rounding_residue_total == sum(rounding residue ledger)
```

`C_tot == sum(C_i)` is maintained by aggregate-updating helpers and targeted aggregate proofs because global account count is unbounded. Any helper that mutates `C_i` MUST mutate `C_tot` in the same atomic step.

`CloseDriftReserve` is a loss-capacity proof, not automatically a token stock class. If it is funded by a quote-token escrow, it is included in `close_staged_quote_reserve_total`. If it is backed by insurance capacity, B-booking capacity, source-credit lien backing, or recovery capacity, it is proven by the relevant reservation/capacity ledger and MUST NOT be double-counted as token stock.
-------------------------------------------------------------------------------
6. Equity and credit lanes
-------------------------------------------------------------------------------

```text
FeeDebt_i = max(-fee_credits_i, 0)
ReleasedPos_i = max(max(PNL_i,0) - R_i, 0)
ordinary_positive_withdraw_enabled =
    MarketGroup.mode == Live && ResolvedPayoutLedger is not initialized
```

For each positive leg:

```text
leg_local_factor =
    min(maturity_or_warmup_factor,
        oracle_confidence_factor,
        target_effective_dual_price_factor,
        thin_market_factor,
        domain_lock_factor,
        pending_loss_factor,
        recovery_factor,
        configured_leg_credit_cap)

leg_local_positive_value =
    floor(positive_pnl_current * leg_local_factor / SUPPORT_WEIGHT_SCALE)

source_credit_value =
    floor(leg_local_positive_value * credit_rate_num[source_domain]
          / CREDIT_RATE_SCALE)
```

```text
Eq_maint_i =
    C_i
  + conservative_negative_leg_pnl
  + sum(eligible soft source_credit_value from un-liened claims)
  + sum(valid Risk lien effective_credit_reserved)
  - FeeDebt_i
  - equity_side_penalties_maint

Eq_initial_i / Eq_trade_i =
    C_i
  + conservative_negative_leg_pnl
  + sum(valid liened credit or lien-creatable source_credit_value from un-liened claims)
  - FeeDebt_i
  - equity_side_penalties_initial_or_trade

Eq_withdraw_i =
    if ordinary_positive_withdraw_enabled:
        C_i + conservative_sum_negative_leg_pnl
            + sum(withdraw-lien-creatable source_credit_value from un-liened claims)
            - FeeDebt_i - equity_side_penalties_withdraw - withdraw_pending_loss_reserve
    else:
        C_i + conservative_sum_negative_leg_pnl - FeeDebt_i - equity_side_penalties_withdraw

Eq_no_positive_credit_i =
    C_i + conservative_sum_negative_leg_pnl - FeeDebt_i - equity_side_penalties_no_positive
```

`lien-creatable source_credit_value` means the engine can create the exact lien required for that lane in the same instruction or an already-valid lien exists. It requires:

```text
un_liened_face_claim >= required_face
credit_epoch current
credit_rate_num > 0
required_backing <= available_backing_num
and one of:
    deterministic Fresh counterparty bucket has fresh_unliened_backing_num >= required_backing;
    canonical insurance reservation has
        insurance_credit_reserved_num
      - valid_liened_insurance_num
      - impaired_liened_insurance_num >= required_backing;
    existing valid lien covers the same purpose and amount.
```

Aggregate `available_backing_num` alone is not sufficient if no actual bucket or insurance reservation can satisfy the lien lifecycle preconditions.

For every check, equity-side penalties and requirement-side terms are disjoint. A named requirement-side term such as `maintenance_pending_loss_penalty`, `pending_obligation_exposure`, `impaired_lien_penalty`, `concentration_penalty`, `unsettled_loss_penalty`, or `target_effective_lag_penalty` MUST NOT also be included inside any `equity_side_penalties_*` used for the same check. A penalty may move sides only by an explicit formula proving identical or more conservative single counting.

A PositiveCreditAction is any action whose approval, payout, withdrawal, release, transfer, conversion, fee payment, residual reduction, or risk increase depends on positive PnL. It MUST either:
- use a valid source-credit lien; or
- use `Eq_no_positive_credit_i`.

Positive PnL use is forbidden or zero if the contributing leg is stale, B-stale, loss-stale, partial, locked, pending-loss-exposed without reserve, pending-obligation-exposed, recovery-mode, target/effective-lagged without dual-price pass, thin-market locked, or hmax/stress locked.

If a lien's source rate drops, backing expires, or source domain enters recovery, the lien is impaired. Impaired liens cannot support new risk and must be resolved by deleveraging, liquidation, ADL, backing replenishment, or recovery.

-------------------------------------------------------------------------------
7. Portfolio health and staleness
-------------------------------------------------------------------------------

A full portfolio refresh computes a health certificate from the current v16
public-profile requirement formula:

```text
portfolio_maintenance_req =
    gross_mm
    + target_effective_lag_penalty

portfolio_initial_req =
    gross_im + target_effective_lag_penalty
```

The current v16 public profile does not enable numeric hedge credit,
concentration penalties, thin-market penalties, stale penalties, domain-lock
penalties, maintenance-pending-loss penalties, pending-obligation exposure
adders, or impaired-lien adders. Those terms are zero unless a future profile
adds explicit config, exact formulas, and proofs. In the current profile their
safety effect is represented by one of these mechanisms instead:

- negative and unsettled account PnL is included on the equity side by refresh,
  settlement, and conservative haircut equity;
- stale, B-stale, loss-stale, hmax/stress, recovery, target/effective-lagged
  without the approved lane, active close barrier, pending-domain-loss barrier,
  pending obligation, and impaired-lien states are hard locks or
  progress-only/reduction-only states for favorable actions;
- unsupported hedge/concentration/thin-market profiles are not production-legal
  merely by naming the term in this section.

For every check, equity-side penalties and requirement-side terms remain
disjoint. If a future profile moves any hard-lock/zero term into the numeric
requirement formula, it MUST define the exact formula and prove equivalent or
more conservative single counting. Pending obligation and impaired lien exposure
MUST be counted exactly once: in current v16 they are counted by lock/progress
rules, not by positive numeric maintenance adders.

Hedge credit is optional, disabled in the current v16 public profile, and
deterministic when enabled by a future profile:

```text
hedge_credit <= min(offset_leg_risks) * cfg_max_offset_bps / 10_000
```

Allowed only for configured buckets with current epochs and no unsettled B, stale cert, target/effective lag without dual-price pass, recovery, active close barrier, pending-domain-loss barrier, pending obligation, or impaired lien.

For every hedge bucket and every allowed portfolio at initialization or activation, the engine MUST prove in exact wide arithmetic:
```text
worst_combined_price_funding_loss
  + combined_liquidation_costs
  + configured_basis_or_depeg_gap_loss
  + stale_or_oracle_uncertainty
  <= gross_mm - hedge_credit
```
For initial-margin approval, the analogous initial envelope MUST hold using
`gross_im - initial_hedge_credit`. No hedge-credit configuration is
production-legal unless this reduced-requirement envelope holds for all allowed
portfolios. Until that configuration and proof exist, `hedge_credit` and
`initial_hedge_credit` are exactly zero.

A certificate is fresh only if instance id, market group, config hash, asset-set epoch, epochs, active bitmap, asset slots, source-credit epochs, lien states, and effective prices remain valid.

Stale accounts cannot perform favorable actions, create liens, use hedge credit, use positive PnL for approval/support, or receive resolved positive payout. They may refresh, rebalance defensively, liquidate, recover, or forfeit/detach a dead leg.

-------------------------------------------------------------------------------
8. Settlement helpers
-------------------------------------------------------------------------------

Every `C_i`, `PNL_i`, position, B, fee, source-credit, lien, claim-bound, close-state, insurance, support, pending obligation, and ledger mutation MUST use aggregate-updating helpers.

`attach_leg` requires old effects settled, side mode permitting attach, full account refresh, asset Active, no same-asset opposite nonzero leg, and no active close/pending loss/pending obligation conflict.

`clear_leg` requires A/K/F/B settled and all source-credit liens or pending obligations touching the leg released, consumed, escrowed, or pulled forward. It quarantines remainder, transfers local `b_rem` to dust, subtracts weight only after pending obligations are handled, clears local fields, and mutates OI only through a transition proving matching OI change.

For a nonzero leg:

```text
B_target = current B_side_num if current epoch else B_epoch_start_side_num under ResetPending
ΔB = B_target - b_snap
num = b_rem + loss_weight * ΔB
B_loss = floor(num / SOCIAL_LOSS_DEN)
b_rem_new = num % SOCIAL_LOSS_DEN
KF_pnl_delta = exact signed-floor A/K/F settlement
net_pnl_delta = KF_pnl_delta - B_loss
```

If full B settlement is too large, partial settlement is allowed. While `B_remaining > 0`, no user-favorable action may continue.

-------------------------------------------------------------------------------
9. A/K/F/B accrual and source-credit updates
-------------------------------------------------------------------------------

`accrue_asset_to(asset, now_slot, effective_price, funding_rate)` requires Active/DrainOnly live mode, authenticated time, valid price, and bounded funding rate. Domain locks do not block K/F/price/time accrual. Accrual MUST NOT mutate B, A, OI, weights, staged residuals, staged insurance, ADL, pending barriers, pending obligations, or exposure-clear state for a locked domain unless held by the close/recovery path.

Before any accrual/effective-price/K/F write is usable by a favorable action:
1. affected source-domain claim-bound buckets MUST be recomputed conservatively;
2. affected backing freshness buckets MUST be applied;
3. source credit rates and epochs MUST update;
4. accounts using stale or impaired credit liens fail closed until refreshed or deleveraged.

B residual booking:

```text
H = u128::MAX - B_side_num
W = loss_weight_sum_side
R = social_loss_remainder_side_num

max_scaled = (H + 1) * W - 1
if R > max_scaled: max_chunk_by_B = 0
else:              max_chunk_by_B = floor((max_scaled - R) / SOCIAL_LOSS_DEN)

engine_chunk = min(residual_remaining, max_chunk_by_B, cfg_public_b_chunk_atoms)
delta_B = floor((engine_chunk * SOCIAL_LOSS_DEN + R) / W)
new_remainder = (engine_chunk * SOCIAL_LOSS_DEN + R) % W
```

Successful B booking requires `W > 0`, positive chunk/delta, and B headroom. Before booking, aggregate pending obligation credits due through the booking slot are applied in O(1). The same atomic step increments `b_loss_booked`, reduces residual, consumes matching obligations/escrow, and updates barriers.

After any B residual booking, the affected source-domain claim bounds, backing availability, and credit rates MUST be recomputed, conservatively lowered, or hard-maxed before any favorable action reads them. B booking can reduce winners' net PnL and source-domain claim bounds; stale higher bounds are conservative, but stale credit epochs MUST block favorable actions until updated or explicitly proven safe.

If `W == 0`, residual clears only by reserved insurance or explicit protocol-owned backing preserving senior invariants; otherwise route to recovery.

Quantity ADL applies exactly once after residual durability and is atomic with closing exposure clear/finalization or protected by a non-preemptible finalization barrier.

`begin_full_drain_reset(asset, side)` requires zero OI, no close/pending barrier, no pending obligations, no liens, and side not already `ResetPending`.

-------------------------------------------------------------------------------
10. Liquidation and bankrupt close
-------------------------------------------------------------------------------

Liquidation is triggered by:

```text
certified_equity_maint < certified_maintenance_req
```

A liquidation instruction refreshes the full account and builds a deterministic plan from all active legs and source-credit liens. Plan order is deterministic: highest risk contribution, largest deficit, asset id ascending, Long before Short.

Cross-close contention is resolved by EXCLUSIVE serialization, not priority comparison (v16.9.0, matching the proven implementation):
```text
At most one active close per domain:
    begin_close in an occupied domain (pending-domain-loss barrier != 0)
    MUST reject with LockActive BEFORE any mutation.
At most one active close per account.
close_id is strictly monotone per account: a new close never reuses an id;
    an instruction carrying the active close_id is a continuation, not a conflict.
Liveness: an active close past its immutable max_close_slot MUST route to
    recovery instead of holding its domain.
```
Each close holds exactly one domain barrier, so hold-and-wait cycles are impossible by construction. Contention rejects rather than compares, so equal-priority livelock cannot exist. (Proven: occupied-domain rejection-before-mutation, per-account exclusion, monotone identity stamping, and the bounded-lifetime expiry route.)

Initial close sets immutable:

```text
DriftReferenceSlot = snapshot_slot
MaxCloseSlot = DriftReferenceSlot + cfg_max_bankrupt_close_lifetime_slots
drift_consumed = 0   # RESERVED partition category (v16.9.0): no engine path
                     # funds a drift reserve; the column is carried so a future
                     # drift-reserve mechanism can land without re-shaping the
                     # ledger. Until then adverse drift is bounded by the
                     # lifetime rule: continuation past MaxCloseSlot MUST route
                     # to recovery.
```

Before any account close, finalization, or weight reduction, `settle_pending_obligations(account)` handles all pending barriers by escrow, settlement, pulled-forward loss, or recovery.

Bankrupt close phases:

```text
Touched
SourceCreditLiensSettledOrImpaired
PendingObligationsSettledOrPulledForward
FullPortfolioSideEffectsPartiallySettled
PortfolioLossVectorComputed
SupportPoolComputed
SupportAllocated
InsuranceAllocated
ResidualsPartiallyBooked
ResidualsBooked
QuantityADLApplied
AccountFinalized
CanceledIfCured
```

`SupportPool` for residual curing may include only:
- senior capital;
- durable realized nonjunior gains;
- settlement-quality source-credit liens (counterparty- or insurance-backed) whose backing is consumed atomically;
- legs being closed/finalized with matching source-domain loss recognition;
- finalized pending-obligation surplus.

Every source-credit lien consumed through SupportPool reduces the residual partition exactly once, as `support_consumed`. An insurance-backed lien consumed this way additionally debits I and the domain insurance budget exactly once (`CloseInsuranceSpent`); direct insurance allocation (`InsuranceAllocated`) is the separate `insurance_spent` partition category and never overlaps with lien-funded support.

Open non-candidate positive PnL and soft maintenance credit are excluded from residual curing.

For each losing candidate:

```text
LegLoss_j = max(0, loss_to_close_leg_j + liquidation_cost_j + side_effect_loss_j + impaired_lien_shortfall_j)
Domain_j  = (asset_j, opposing_side_j)
```

Pulled-forward pending-obligation shortfalls retain their original barrier domain and become a liability of the exiting/finalizing account. The origin residual is credited exactly once before the obligation is pulled forward. If the exiting account cannot pay, its own close books the shortfall to the original barrier domain; the origin close MUST NOT also socialize that share over remaining stayers. Support, insurance, and residual allocation are deterministic. Residuals may only book to `Domain_j`, never to unrelated assets, all shorts, all profitable accounts, or a global B index.

Remaining residual in close ledger:

```text
remaining_residual =
    gross_loss_at_close_start
  + total_adverse_drift_from(drift_reference_slot, now)
  - support_consumed
  - insurance_spent
  - b_loss_booked
  - explicit_loss_assigned
  - pending_obligation_credits
```

Every continuation first checks owner cure-and-cancel after any cancel escrow. New deposits intended for cancel MUST NOT be consumed as support before this check. Continuation must strictly reduce close progress after worst-case drift; otherwise route to recovery.

Cure-and-cancel is allowed only if the account is initial-healthy after all reserves and no irreversible B booking, ADL, insurance spend, explicit loss, support consumption, pulled-forward obligation, or consumed credit lien has occurred.

-------------------------------------------------------------------------------
11. User operations
-------------------------------------------------------------------------------

A user-favorable operation MUST:
1. authenticate owner/authority;
2. validate clock, oracle target, effective price, admission, and inputs;
3. refresh the full active portfolio;
4. update source-domain claim bounds, backing freshness, credit rates, and lien validity;
5. continue conflicting close, recover, cure-and-cancel, detach/forfeit a dead leg, or fail before unrelated mutation;
6. settle A/K/F/B for touched legs;
7. settle losses before fees;
8. recompute `HealthCert`;
9. run candidate checks under final stale/B/loss-stale/domain-lock/pending-loss/pending-obligation/source-credit-lien/recovery state;
10. commit only if all invariants hold.

Deposits are pure capital. Deposits into cancelable closes may be placed in cancel escrow and must receive cancel consideration before being consumed as close support. Other deposits into stale/B-stale/locked accounts are loss-curing only until refresh clears locks.

Withdrawals require current source-credit rates and liens for any positive PnL component. Cross-instance transfers withdraw actual quote tokens only, then deposit actual received tokens into the destination instance.

Trades require:
- full portfolio refresh for both counterparties, or an engine-verified post-trade health certificate covering candidate size, price, active bitmap, source-credit epochs, liens, locks, barriers, and all existing legs;
- loss-current market state;
- current B/K/F settlement for touched legs;
- side-mode gating;
- OI/position bounds;
- candidate-slippage neutralization;
- lien creation for positive credit used beyond no-positive-credit equity;
- matched-side loss recognition before gain extractability;
- exact fee enforcement.

-------------------------------------------------------------------------------
12. Recovery, resolution, and payout
-------------------------------------------------------------------------------

A public `CrankForward` market MUST expose permissionless terminal recovery for any state where bounded progress cannot continue, including account B settlement failure, B-index exhaustion, source-credit underbacking, lien impairment, backing-expiry failure, active close failure, domain lock/barrier/obligation failure, insurance budget exhaustion, snapshot re-aging failure, close drift expiration, oracle/target unavailability, asset lifecycle failure, claim-bound failure, and payout-lane conflict.

Recovery price is deterministic. The engine uses an authenticated recovery price when available and representable; otherwise it may use the immutable configured fallback only if the recovery fallback envelope is valid for the asset and mode transition. Caller cannot choose recovery price.

Fallback recovery is allowed only under the numeric envelope from §1.3:

```text
P_ref = RecoveryReferencePrice(asset)
P_fb  = FallbackRecoveryPrice(asset)

require abs(P_fb - P_ref) * 10_000
        <= cfg_max_recovery_fallback_deviation_bps * P_ref

recovery_value_transfer_leg =
    ceil(abs(pos_q_leg) * abs(P_fb - P_ref) / POS_SCALE)
```

For every recovered account, the engine MUST compute `recovery_value_transfer_bound(account)` and include it in the recovery receipt or settlement proof. For every recovered source domain, the engine MUST compute the aggregate domain transfer bound from touched accounts and domain obligations. Recovery may overcharge the loss domain or underpay profitable legs only within these checked bounds. If the bound cannot be computed, exceeds the configured envelope, or uses a stale/unverified reference, fallback recovery MUST NOT pay positive junior value and MUST route to authenticated recovery pricing, dead-leg forfeiture, or terminal recovery preserving senior invariants.

Recovery preserves and reconciles `SourceCreditState`, insurance-credit reservations, lien buckets including impaired buckets, liens, close ledgers, pending obligations, B/ADL/support progress, and payout ledgers. It cannot erase ledgered progress and recompute gross loss. Recovery direct positive payout must use resolved receipts or pay no positive junior value.

Resolved payout is progressive and source-domain based:
1. initialize a `ResolvedPayoutLedger` after terminal losses, insurance, source-credit liens, barriers, and obligations are settled/reserved;
2. disable ordinary positive-PnL withdrawals/releases;
3. allow bounded exact receipts by account refresh;
4. require exact receipt claim `* BOUND_SCALE <= prior_bound_contribution_num`;
5. replace unreceipted bound with exact claim;
6. compute non-decreasing payout rate per source domain or a conservative aggregate rate;
7. pay only bounded top-ups.

Receipt underbound halts payouts and routes to recovery/bound repair before the invalid receipt affects rate.

Dead-leg forfeit/detach is bounded and owner-callable for terminal/recovery/dead assets. It refreshes the full account, settles or over-reserves losses, values positive PnL at zero unless source-domain backing is consumed, values negative PnL at conservative fallback/recovery loss within the §1.3 recovery fallback envelope, books residual only to `(asset, opposing_side)`, clears only after residual durability, and leaves unrelated legs usable once healthy.

-------------------------------------------------------------------------------
13. Instance isolation and wrappers
-------------------------------------------------------------------------------

A cross-instance transfer is not cross margin:

```text
source instance:
    full refresh
    settle losses
    validate source-credit liens and pending obligations
    withdraw value up to the senior claim as actual quote tokens

destination instance:
    deposit actual received quote tokens as new capital
```

The same collateral, source credit, backing reservation, PnL, claim, certificate, or insurance value MUST NOT be counted in two instances.

Wrappers own authorization, oracle normalization, raw target storage, effective-price staircase policy, account proof packing, anti-spam economics, hint markets, thin-market guardrails, credit-lien UX, resolved receipt incentives, pending-obligation settlement incentives, and MEV-aware cancel routing.

Public wrappers MUST NOT expose caller-controlled:
- admission/funding/threshold/future slot;
- asset lifecycle changes;
- B chunk sizes;
- claim-bound bucket membership/formula inputs;
- backing bucket inclusion or freshness;
- source-credit rate or lien interpretation;
- support/insurance allocation;
- residual attribution;
- domain lock order or preemption priority;
- recovery fallback price, recovery reference price, fallback deviation cap, or recovery value-transfer bound;
- cross-instance netting or merged health.

Wrappers MUST expose full refresh, hinted crank, bounded catchup, active close continuation, account-B settlement, source-credit/lien revalidation, domain-lock/pending-loss/pending-obligation continuation, permissionless recovery, cure-and-cancel, dead-leg forfeit/detach, resolved claim receipt, and rebalance-on-touch.

-------------------------------------------------------------------------------
14. Required proof and TDD coverage
-------------------------------------------------------------------------------

1. `source_domain_positive_credit_capped_by_realizable_backing`.
2. `token_value_flow_proof_every_quote_atom_has_one_debit_and_one_credit`.
3. `reservation_encumbrance_proof_excludes_non_value_labels_from_token_flow`.
4. `source_credit_lien_creation_moves_no_quote_value`.
5. `stock_reconciliation_holds_at_genesis_activation_mode_transition_and_recovery`.
6. `oracle_pump_credit_limited_by_opposing_reserved_backing`.
7. `source_credit_rate_zero_when_backing_stale_or_exhausted`.
8. `risk_increasing_trade_requires_source_credit_lien`.
9. `source_credit_lien_prevents_double_use_of_same_claim_and_backing`.
10. `source_credit_lien_impairment_forces_deleverage_liquidation_or_recovery`.
11. `backing_reservation_is_actual_locked_equity_not_optimistic_certificate`.
12. `backing_expiry_buckets_exclude_stale_contributions_without_full_scan`.
13. `insurance_credit_reservation_globally_conserved`.
14. `insurance_spend_not_double_counted_as_live_encumbrance`.
15. `amount_from_bound_num_up_rounds_up_for_insurance_credit`.
16. `source_credit_insurance_reservation_single_canonical_writer`.
17. `source_credit_insurance_cannot_be_double_reserved_or_double_spent`.
18. `insurance_backed_lien_creation_increments_valid_liened_insurance_not_counterparty_backing`.
19. `insurance_backed_lien_consume_release_impair_conserves_canonical_ledger`.
20. `insurance_backed_lien_consumption_decrements_source_credit_reservation_and_total_available_once`.
21. `impaired_insurance_lien_remains_reserved_and_unavailable_until_reconciled`.
22. `source_credit_lien_cure_counts_once_as_support_consumed_with_one_funding_flow_class` (a residual cure by a source-credit lien — counterparty- OR insurance-backed — reduces the residual partition through exactly one category, `support_consumed`; the insurance-backed branch additionally debits `I`/domain budget through exactly one funding flow class, `CloseInsuranceSpent`, and MUST NOT also book direct `insurance_spent`; the counterparty branch's funding transit class is `CloseCounterpartyCreditConsumed`).
23. `insurance_backed_lien_never_counts_as_both_support_and_insurance`.
24. `close_residual_partition_sum_equals_close_flow_class_sum_under_pinned_partition_to_class_map` (every cured atom lands in exactly one residual-partition category and exactly one close flow class under the fixed map: `support_consumed → {CloseSupportConsumed for non-lien support, CloseCounterpartyCreditConsumed for counterparty-lien cure, CloseInsuranceSpent for insurance-lien cure}`, direct `insurance_spent → CloseInsuranceSpent`, `b_loss_booked → BResidualBooked`, `explicit_loss_assigned → ExplicitBackedLoss`. The map is checked PER RESIDUAL-CURE TRANSITION by that transition's balanced `TokenValueFlowProof` — the close ledger persists only the partition category totals, and the transit flow classes net to zero at every reconciliation point, so proof 102's per-class O(1) reconciliation applies to persistent classes only and needs no persistent three-way sub-decomposition of `support_consumed`; `CloseCounterpartyCreditConsumed` is the pinned counterparty-cure/realization transit counterpart).
25. `token_value_flow_proof_balances_internal_insurance_transfers`.
26. `internal_insurance_transfer_requires_exactly_one_credit_entry`.
27. `recovery_consumed_insurance_lien_decrements_v_on_external_payout`.
28. `lien_creatable_predicate_requires_actual_bucket_or_insurance_reservation_capacity`.
29. `lien_creatable_matches_actual_bucket_and_insurance_lifecycle_helpers`.
30. `insurance_backed_lien_create_consume_release_impair_conserves_canonical_ledger`.
31. `insurance_impaired_lien_remains_encumbered_until_release_or_consume`.
32. `lien_creation_increments_correct_aggregate_for_backing_source`.
33. `lien_creatable_predicate_matches_actual_bucket_or_insurance_lifecycle`.
34. `backing_bucket_expiry_does_not_underflow_available_backing`.
35. `backing_bucket_expiry_does_not_increase_available_backing_or_credit_rate`.
36. `backing_bucket_expiry_after_partial_lien_consumption_does_not_inflate_available`.
37. `lien_consumption_decrements_bucket_valid_liened_and_source_valid_liened_once`.
38. `lien_consumption_removes_backing_from_fresh_reserved_and_claim_bound`.
39. `lien_release_moves_valid_liened_to_fresh_unliened_without_changing_fresh_reserved`.
40. `source_available_backing_recomputes_from_bucket_sums`.
41. `lien_consumption_creates_provider_receivable_matching_bucket_consumed`.
42. `future_source_backing_refills_provider_receivable_before_excess_new_backing`.
43. `backing_refill_preserves_reservation_encumbrance_proof`.
44. `expired_liened_bucket_marks_liens_impaired_in_bounded_work`.
45. `credit_rate_num_bounded_below_and_above`.
46. `lien_creation_requires_required_backing_le_available_backing`.
47. `locked_face_claim_excluded_from_soft_credit`.
48. `withdrawal_uses_conservative_sum_negative_leg_pnl_not_aggregate_min`.
49. `close_past_max_close_slot_routes_to_recovery_and_drift_consumed_is_zero` *(v16.9.0: replaces the drift-reserve capacity item; the reserve mechanism is reserved/unimplemented)*.
50. `pulled_forward_obligation_credit_not_socialized_again`.
51. `claim_bound_bucket_formula_never_understates_source_domain_claims`.
52. `claim_bound_bucket_out_of_range_fails_closed_or_rebuckets`.
53. `credit_rate_recomputation_is_bounded_by_domain_count_and_bucket_count`.
54. `no_circular_credit_without_external_senior_backing`.
55. `soft_maintenance_credit_does_not_create_payout_or_residual_cure`.
56. `settlement_quality_credit_consumes_backing_and_locks_face_claim`.
57. `fake_asset_profit_cannot_buy_unbacked_other_asset_risk`.
58. `backing_consumption_reduces_loser_capital_and_preserves_senior_invariants`.
59. `residuals_charged_only_to_asset_opposing_side_domain`.
60. `no_global_B_index`.
61. `cross_instance_ui_aggregation_not_health_or_collateral_proof`.
62. `mutable_asset_activation_requires_full_envelope_proofs`.
63. `asset_cannot_activate_with_nonzero_or_unreconciled_state`.
64. `activation_invalidates_or_scopes_certs_fail_closed`.
65. `full_account_refresh_required_for_favorable_actions`.
66. `verified_maker_exemption_requires_engine_verified_post_trade_health_cert`.
67. `pending_obligation_credit_decrements_origin_residual_once`.
68. `aggregate_due_drift_credit_is_O_1_before_b_booking`.
69. `participant_finalization_pulls_forward_pending_obligation`.
70. `phantom_weight_without_backing_reverts`.
71. `close_begin_rejects_occupied_domain_and_second_account_close_before_mutation` *(v16.9.0: exclusion replaces preemption; proven)*.
72. `close_id_monotone_and_canceled_ledger_with_partial_booking_rejected` *(v16.9.0: restart identity + dropped-residual rejection; proven)*.
73. `close_id_and_drift_anchors_immutable`.
74. `bankrupt_close_progress_decreases_net_of_close_drift`.
75. `cure_and_cancel_checks_before_consuming_new_deposit`.
76. `quantity_adl_and_account_finalization_atomic_or_barriered`.
77. `domain_lock_does_not_block_asset_wide_kf_accrual`.
78. `B_booking_exact_remainder_conservation`.
79. `zero_weight_domain_residual_cannot_clear_without_backing`.
80. `uncollectible_fees_forgiven_not_socialized`.
81. `resolved_payout_uses_source_domain_or_conservative_aggregate_rates`.
82. `resolved_receipt_underbound_halts_payout_or_recovers`.
83. `recovery_fallback_price_within_configured_deviation_envelope`.
84. `recovery_fallback_value_transfer_bound_computed_per_account_and_domain`.
85. `fallback_recovery_rejects_unverified_or_out_of_envelope_reference_price`.
86. `dead_leg_forfeit_uses_bounded_fallback_or_zero_positive_payout`.
87. `dead_leg_forfeit_books_to_bankruptcy_domain`.
88. `no_single_instruction_full_market_scan_required`.
89. `global_accumulator_not_account_health_proof`.
90. `canonical_single_leg_per_asset`.
91. `N_too_large_rejects_public_initialization_or_activation`.
92. `pending_obligation_exposure_counted_exactly_once_by_lock_or_health_test`.
93. `equity_side_penalties_disjoint_from_requirement_side_penalties`.
94. `hedge_credit_zero_unless_reduced_requirement_envelope_is_proven`.
95. `cross_close_priority_is_strict_total_order`.
96. `equal_priority_livelock_impossible`.
97. `B_booking_triggers_source_claim_bound_and_credit_rate_recompute_or_conservative_lowering`.
98. `settlement_rounding_residue_credits_unallocated_surplus_and_flow_proof_balances`.
99. `rounding_residue_never_used_for_health_backing_insurance_or_payout`.
100. `stock_reconciliation_includes_settlement_rounding_residue_total`.
101. `drift_consumed_partition_category_is_reserved_and_zero` *(v16.9.0)*.
102. `per_class_stock_reconciliation_matches_o1_ledgers_where_available`.
-------------------------------------------------------------------------------
15. Audit summary and intended tradeoff
-------------------------------------------------------------------------------

[FIXED] Recovery fallback envelope.
    v16.8 replaces the word-only “bounded recovery risk” with an activation-validated numeric envelope. Fallback recovery prices are deterministic functions of the last authenticated recovery reference price and must stay within `cfg_max_recovery_fallback_deviation_bps`.

[FIXED] Recovery user-transfer bound.
    Every fallback recovery computes a per-leg, per-account, and per-domain value-transfer bound. Fallback recovery cannot overcharge a loss domain or underpay a profitable leg outside the checked envelope.

[KEPT] Rounding residue in quote-value flow proofs.
    Settlement/allocation rounding residue remains assigned to `SettlementRoundingResidue` or `UnallocatedProtocolSurplus` and cannot become user health, backing, insurance, or payout value.

[KEPT] Conservation layer.
    Token-value flow proofs, reservation encumbrance proofs, stock reconciliation, and per-class O(1) reconciliation remain separate and well-typed.

[KEPT] Realizable full shared cross-margin.
    In healthy, fully backed domains credit rate is 1.0; in manipulated, stale, expired, or underbacked domains positive PnL cannot become global purchasing power.

v16.8 guarantee:

```text
one honest crank with a valid account hint can force bounded progress on that account;
inside one market-group instance, all Active assets share full cross-margin solvency;
positive PnL is live-usable only to the extent its source domain has fresh reserved counterparty or insurance backing;
fallback recovery can transfer value only within a numeric, activation-proven envelope;
rounding residue is always assigned to a non-user, non-backing stock class;
backing expiry cannot inflate usable credit;
insurance spend cannot freeze remaining valid insurance through double counting;
oracle-manipulated or uncollectible source PnL cannot become global purchasing power;
bankruptcy residuals remain market-side local;
separate instances are isolated even if a UI aggregates them.
```
