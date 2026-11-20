# Invariants against Aristotle: what can be checked, and where

Status on 2026-11-19. "Stage" says which Aristotle run covers the invariant's checkable core:

- **A** arithmetic: `src/wide_math.rs` of the engine, 45 properties (`specs/wide_math_spec.md`).
- **B** engine kernels: the 58 contract-annotated functions of `src/v16.rs`
  (`specs/kernel_contracts_spec.md`, labels K01 to K58).
- **C** wrapper handlers: validation and routing functions of `src/v16_program.rs`
  (specification not written yet; needs the wrapper to build under Aristotle's compiler).
- **D** whole transitions: an invariant preserved by every public engine entry point
  (not attempted yet; depends on what A to C cost).
- **none**: outside what Aristotle does (liveness, termination, compute, or a statement about the
  verification process). These stay with the stateful fuzz, the bounded reachability model, Kani
  and the SVM tests.

A stage proves the kernel of an invariant, not the invariant as the charter states it: the charter
quantifies over every public route and every reachable state. Stage D is where that gap would
close.

| Invariant | Kind | Stage | What the stage checks |
|---|---|---|---|
| INV-001 market incarnation | safety | C | request handlers compare the stored market identity and reject a tombstone |
| INV-002 asset generation | safety | C | `asset_generation_binding_matches` and its call sites |
| INV-003 portfolio incarnation | safety | C | `expect_portfolio_sequence_binding`, `portfolio_close_binding_matches` |
| INV-004 position episode | safety | C, B | position-epoch checks; K32 to K35 leg kernels |
| INV-005 authority incarnation | safety | C | `require_authority_epoch_view`, `live_authority_matches` |
| INV-006 program / message binding | safety | none | rests on the Solana transaction envelope |
| INV-007 no ABA reuse | safety | C | tombstone and reinitialization guards |
| INV-008 intent uniqueness | safety | C | sequence advance on every successful economic handler |
| INV-009 partial fill accounting | safety | B, C | K10 batch accumulation; exact-fill checks |
| INV-010 out-of-order safety | history | none | permutation property over histories |
| INV-011 aggregate bounds | safety | B, C | K10, K21 guard summary; fee caps in trade handlers |
| INV-012 capability scope | safety | C | matcher config binding checks |
| INV-013 destructive-consent scope | safety | C | episode bindings on forfeit, close, resolve |
| INV-014 delayed policy | safety | C | signed fee ceilings against current policy |
| INV-015 account validity | safety | C | zero-copy view constructors reject wrong owner, length, tag |
| INV-016 canonical PDA | safety | C | `derive_vault_authority`, `canonical_vault_address` |
| INV-017 signer / writable / alias | safety | C | `expect_signer`, `expect_writable` and alias guards per handler |
| INV-018 mint, vault, token program | safety | C | `verify_withdrawable_token_accounts` (including the #477 rule), `verify_user_token_account`, `verify_vault_token_account`, `unpack_token_account` |
| INV-019 CPI return binding | safety | C | matcher return-data checks (the invoked program itself is assumed) |
| INV-020 authenticated clock | safety | C | `authenticated_slot_or_fallback` never moves time backward |
| INV-021 create / realloc / close | safety | C | `close_portfolio_account_to_market_slab`, storage growth |
| INV-022 instruction decoding | safety | C | the decoder is total and rejects trailing bytes |
| INV-023 caller-input confinement | safety | C, B | hints only select work: K19, K20, K23 |
| INV-024 attributed value conservation | safety | B, D | K11 to K15, K49 to K56 exact deltas; whole-route form is stage D |
| INV-025 stock reconciliation | safety | B, D | K50 `apply_total_delta`; global form is stage D |
| INV-026 reservations are labels | safety | B | K36 to K47 move no value |
| INV-027 principal seniority | safety | B, D | K11, K14, K29 |
| INV-028 realizability cap | safety | B | K04 `available_backing_num_for_source_credit_state` |
| INV-029 claim bounds | safety | B | K05, K06 |
| INV-030 credit rate | safety | B, A | rate formula over `wide_math` (K04 and its callers) |
| INV-031 no double use | safety | B | K36 to K47 |
| INV-032 lien lifecycle | safety | B | K36 to K43 |
| INV-033 insurance lien classification | safety | B | K44 to K47, K12 |
| INV-034 domain isolation | safety | B, D | per-domain kernels K52 to K56 |
| INV-035 local B | safety | B | K25 to K28, K30 |
| INV-036 fee destination | safety | B | `backing_domain_fee_split_for_lien_delta_num`, K51 |
| INV-037 residual partition | safety | B | K12, K26 to K28 |
| INV-038 rounding conservation | safety | A, B | spec section G; K14, K31 |
| INV-039 pending obligations | safety | B | K32, K33; `resolved_side_can_still_be_charged` |
| INV-040 no fee seniority | safety | B, D | fee kernels |
| INV-041 deterministic allocation | history | none | order independence over histories |
| INV-042 recovery fallback | n/a | none | reserved, no consumer on the public surface |
| INV-043 hedge credit | n/a | none | disabled in the deployed profile |
| INV-044 no phantom value | safety | B, D | index kernels change no stock: K30, K31 |
| INV-045 mark movement | safety | B | `canonical_accrual_price_step_v16` stays inside the envelope |
| INV-046 exit under unsafe prices | liveness | none | |
| INV-047 equivalent routes | history | none | metamorphic across routes |
| INV-048 OI coherence | safety | B, D | K16 to K18, K58 |
| INV-049 single net leg | safety | B | K34, K35 |
| INV-050 cross-zero | safety | B | K16, K18 |
| INV-051 ADL-effective quantity | safety | B | K07, K18 |
| INV-052 split / merge | history | none | |
| INV-053 health recertification | safety | B | K08, K09, K48 |
| INV-054 certificate epochs | safety | B | K22 `kernel_cert_is_current` |
| INV-055 state-indexed admission | safety | B, C | K17, mode checks per handler |
| INV-056 hints are discovery | safety | B | K19, K20 |
| INV-057 risk reduction available | liveness | none | |
| INV-058 cumulative limits | safety | B | K17 |
| INV-059 fee fragmentation | history | none | |
| INV-060 single-sided margin | safety | B | K08, K09, K48 |
| INV-061 bounded liquidation | safety + termination | B | determinism and no risk increase only |
| INV-062 no identity assumptions | history | none | |
| INV-063 expiry normalization | safety | B, D | bucket status kernels K37, K38 |
| INV-064 insurance withdrawal policy | safety | B, C | K54; one cap and cooldown in the handlers |
| INV-065 reset / recovery isolation | safety | B | K33 |
| INV-066 payout fairness | safety + history | B | K13 payout step; order independence is a history property |
| INV-067 exact-once settlement | safety + liveness | B | K13, K15 |
| INV-068 receipt monotonicity | safety | B, D | payout never exceeds entitlement |
| INV-069 terminal normalization | liveness | none | |
| INV-070 zero terminal residue | safety + liveness | B | K56 |
| INV-071 crank progress | liveness | none | K20, K23 give only plan selection |
| INV-072 order-robust crankability | liveness | none | |
| INV-073 no permanent lock | liveness | none | |
| INV-074 scope locality | safety | D | |
| INV-075 exclusive close | safety | B, D | K24 |
| INV-076 close drift, atomicity | safety | B, D | K27, K28 |
| INV-077 bounded compute | compute | none | |
| INV-078 permissionless recovery | liveness | none | |
| INV-079 reachability evidence | process | none | |
| INV-080 error propagation | safety | C | every engine error maps to an instruction error |
| INV-081 success-state validity | safety | D | the whole-route theorem |
| INV-082 liveness theorem | liveness | none | |
| INV-083 boundary completeness | process | A | stage A has no bound on values at all |
| INV-084 nonvacuous assumptions | process | B | `Kxx.nopanic` shows each precondition is satisfiable and sufficient |
| INV-085 proven arithmetic is deployed arithmetic | process | A | stage A proves the deployed `[u64; 4]` code, which Kani does not run |
| INV-086 reference model equivalence | process | none | |
| INV-087 no phantom controls | process | none | |
| INV-088 global summaries | safety | D | |
| INV-089 activation equivalence | safety | B, C | K57 `asset_restart_next_counters` |

## Counts

- In scope for some stage: 68 of 89.
- Outside Aristotle: 21.
  - Liveness (the family the charter assigns to the bounded reachability model): INV-046, 057,
    069, 071, 072, 073, 078, 082.
  - Compute: INV-077.
  - Properties of whole histories across many transactions: INV-010, 041, 047, 052, 059, 062.
  - The transaction envelope or the verification process: INV-006, 079, 086, 087.
  - Switched off in the deployed profile: INV-042, 043.
- The kernel-to-invariant assignment in the table is mine, made from the kernels' names and
  contracts. It is a plan for where to look, not a claim that a proved kernel proves the invariant.
