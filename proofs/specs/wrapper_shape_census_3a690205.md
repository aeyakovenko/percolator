# Program instructions and the shape check after their engine calls (layer W, first census)

For each instruction of the program: its handler, the engine entry points the handler can reach, and
the classes those entry points have in `transition_shape_spec`. Reachability follows calls by name
inside the program source, so the set can only be too large. The last column counts
`validate_shape()` calls in the handler's own body and in the handler with its helpers; it does not
say whether a check follows the last engine call on every path. Where that was established by
reading the handler, the note says what was found.

The point of the table: an entry point of class final checks the shape itself, so an instruction
that reaches only such entry points commits a checked header. For the other classes the program
has to check after the call, or the engine's own preservation proof (layer T) is the only
safeguard. That is the case for PermissionlessCrank with unchanged positions and for
RebalanceReduce.

| Instruction | Handler | Engine entry points reached (class) | `validate_shape()` calls: handler body / with helpers | Found when read |
|---|---|---|---|---|
| InitMarket | `handle_init_market` | none | 0 / 1 |  |
| InitPortfolio | `handle_init_portfolio` | T47 (frame) | 0 / 0 | no check in the handler; the engine call is of class frame |
| Deposit | `handle_deposit` | T28 (final) | 0 / 0 |  |
| Withdraw | `handle_withdraw` | T23 (delegates), T59 (final), T63 (final) | 0 / 0 |  |
| PermissionlessCrank | `handle_permissionless_crank` | T01 (local), T02 (local), T21 (final), T23 (delegates), T44 (dispatch), T57 (delegates), T59 (final) | 0 / 5 | Live and Recovery: after the engine's auto crank the handler validates only `if position_changed`. A refresh, a B-chunk settlement, a step of a pending close or a Recovery refresh that leaves the position vector unchanged commits with no shape check by engine or program. Resolved: routed to the CloseResolved handler, which validates |
| TradeNoCpi | `handle_trade_nocpi` | T01 (local), T13 (final), T23 (delegates), T31 (delegates), T57 (delegates), T59 (final) | 0 / 2 |  |
| TradeCpi | `handle_trade_cpi` | T01 (local), T13 (final), T23 (delegates), T31 (delegates), T57 (delegates), T59 (final) | 0 / 3 |  |
| BatchTradeNoCpi | `handle_batch_trade_nocpi` | T01 (local), T23 (delegates), T24 (final), T30 (final), T58 (final), T59 (final) | 0 / 0 |  |
| BatchTradeCpi | `handle_batch_trade_cpi` | T01 (local), T23 (delegates), T24 (final), T30 (final), T58 (final), T59 (final) | 0 / 1 |  |
| SetMatcherConfig | `handle_set_matcher_config` | none | 0 / 0 |  |
| ClosePortfolio | `handle_close_portfolio` | T29 (frame) | 0 / 0 | no check in the handler; the engine call is of class frame |
| TopUpInsurance | `handle_top_up_insurance` | T26 (final) | 1 / 1 |  |
| TopUpInsuranceDomain | `handle_top_up_insurance` | T26 (final) | 1 / 1 |  |
| CloseSlab | `handle_close_slab` | T10 (mixed) | 0 / 0 | no check in the handler; the two unchecked paths of T10 perform only T09's write |
| ResolveMarket | `handle_resolve_market` | T53 (final) | 0 / 0 |  |
| TopUpBackingBucket | `handle_top_up_backing_bucket` | T27 (final) | 1 / 1 |  |
| WithdrawBackingBucket | `handle_withdraw_backing_bucket` | T62 (final), T64 (delegates) | 1 / 1 |  |
| ConvertReleasedPnl | `handle_convert_released_pnl` | T18 (final) | 0 / 1 | `wrapper_post_validate = true`: validated after the engine call |
| CloseResolved | `handle_close_resolved` | T23 (delegates), T44 (dispatch) | 1 / 1 | validates the shape after the auto crank returns, so the unchecked `ProgressOnly` paths of T15 are covered here |
| UpdateAuthority | `handle_update_authority` | none | 0 / 0 |  |
| UpdateAssetAuthority | `handle_update_asset_authority` | none | 0 / 0 |  |
| UpdateLiquidationFeePolicy | `handle_update_market_authority_policy` | none | 0 / 0 |  |
| UpdateMaintenanceFeePolicy | `handle_update_market_authority_policy` | none | 0 / 0 |  |
| UpdateBackingFeePolicy | `handle_update_backing_fee_policy` | none | 0 / 0 |  |
| UpdateTradeFeePolicy | `handle_update_trade_fee_policy` | none | 0 / 0 |  |
| UpdateFeeRedirectPolicy | `handle_update_market_authority_policy` | none | 0 / 0 |  |
| UpdateMarketInitFeePolicy | `handle_update_market_authority_policy` | none | 0 / 0 |  |
| WithdrawBackingBucketEarnings | `handle_withdraw_backing_bucket_earnings` | T60 (final) | 1 / 1 |  |
| SyncBackingDomainLedger | `handle_sync_backing_domain_ledger` | none | 0 / 0 |  |
| SyncInsuranceLedger | `handle_sync_insurance_ledger` | none | 0 / 0 |  |
| CanonicalizeSpentBackingHistory | `handle_canonicalize_spent_backing_history` | T12 (final) | 1 / 1 |  |
| ConfigurePermissionlessResolve | `handle_configure_permissionless_resolve` | none | 0 / 0 |  |
| ResolveStalePermissionless | `handle_resolve_stale_permissionless` | T53 (final) | 0 / 0 |  |
| ConfigureHybridOracle | `handle_configure_hybrid_oracle` | none | 1 / 1 |  |
| ConfigureEwmaMark | `handle_configure_managed_mark` | none | 1 / 1 |  |
| PushEwmaMark | `handle_push_managed_mark` | T57 (delegates) | 0 / 0 |  |
| ConfigureAuthMark | `handle_configure_managed_mark` | none | 1 / 1 |  |
| PushAuthMark | `handle_push_managed_mark` | T57 (delegates) | 0 / 0 |  |
| ForceCloseAbandonedAsset | `handle_force_close_abandoned_asset` | T23 (delegates), T35 (delegates), T36 (mixed), T59 (final) | 2 / 2 |  |
| RestartAssetOracle | `handle_restart_asset_oracle` | T54 (final) | 1 / 1 |  |
| UpdateAssetLifecycle | `handle_update_asset_lifecycle` | T02 (local), T03 (local), T05 (local), T26 (final), T34 (final), T38 (local), T43 (final), T55 (final) | 5 / 5 |  |
| WithdrawInsuranceAsset | `handle_withdraw_insurance_asset` | T46 (final), T61 (final) | 1 / 1 |  |
| CureAndCancelClose | `handle_cure_and_cancel_close` | T25 (final) | 0 / 0 |  |
| ForfeitRecoveryLeg | `handle_forfeit_recovery_leg` | T01 (local), T36 (mixed) | 0 / 1 | `wrapper_post_validate = true`: validated after the engine call |
| RebalanceReduce | `handle_rebalance_reduce` | T01 (local), T45 (real) | 0 / 1 | `with_one_portfolio_view(.., wrapper_post_validate = false, ..)`: no shape check after the engine call (the source comment gives the compute budget as the reason); the engine entry point checks only in audit builds |
| FinalizeResetSide | `handle_finalize_reset_side` | T33 (frame) | 0 / 0 | no check in the handler; the engine call is of class frame and validates the aggregate totals itself |
| ClaimResolvedPayoutTopup | `handle_claim_resolved_payout_topup` | T09 (local), T14 (final) | 0 / 0 | T09 is followed by T14, which ends in the check |
| SyncMaintenanceFee | `handle_sync_maintenance_fee` | T21 (final), T23 (delegates), T29 (frame), T59 (final) | 5 / 5 |  |
| UpdateBaseUnitMints | `handle_update_base_unit_mints` | none | 0 / 0 |  |
| SwapSecondaryForPrimary | `handle_swap_secondary_for_primary` | none | 0 / 0 |  |

Instructions by the weakest class of engine entry point they can reach: {'none': 18, 'frame': 4, 'final': 12, 'delegates': 4, 'dispatch': 2, 'local': 6, 'mixed': 3, 'real': 1}
