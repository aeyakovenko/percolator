
// Header pieces of class "final" (T*.header): the path structure, with every callee replaced.
//
// The ledger states each such piece as: if the header predicate (`validate_shape` of a default
// build) held before the entry point and it returns Ok, it holds of the state it leaves; the
// obligation is the path structure, "every Ok path ends in `validate_shape()` after its last
// market write". Each harness below checks that structure of the entry point's own body:
// - every method it calls on the market view is replaced by a stand-in: one that takes the view
//   by shared reference returns any value; one that takes it mutably may first write any bytes to
//   the header, the asset slots and the view's scratch, and any values to the fields of each
//   account it is given that these entry points read, then returns any value. Every behaviour of
//   the real callee, as far as the entry point can observe it, is among them;
// - its own `validate_shape()` calls are replaced by a recorder that returns either outcome and
//   remembers the header it was asked about and its answer, so the proof holds whatever the
//   predicate is, and so for a default build's predicate and any number of asset slots;
// - the account's own check `validate_with_market` (shared references only, so it writes nothing)
//   returns either outcome; pure helpers (the deltas, the token-flow records, the kernels, the
//   decoders) run as written, except a pure function a harness names in its stubs (`any_pure_*`):
//   it takes its inputs by value and writes nothing, and its stand-in returns any result;
// - the header and the slot are arbitrary bytes; of an account, the fields the entry point reads
//   itself are arbitrary (`arbitrary_read_fields`; the generator checks the list), the others are
//   read only by callees, which are replaced. Nothing is assumed of any of them.
// The assertion: an Ok return leaves the header bytes unchanged, or the last shape check passed on
// exactly the header the entry point leaves. Either way the predicate holds after if it held
// before. A callee that has no early Ok and ends in its own check is proved the stronger way (every
// Ok follows a passed check on the header it leaves) and is replaced in its callers by a stand-in
// with that property: any writes and any result, and a passed check recorded on every Ok. The
// generator checks that the entry points read no slot themselves, use an account only through the
// fields listed in `arbitrary_read_fields`, its own check and the replaced callees, and give `self`
// to nothing but the replaced callees.
#[cfg(all(kani, feature = "closure"))]
static mut LAST_SHAPE_CHECK: Option<(MarketGroupV16HeaderAccount, bool)> = None;

/// Stand-in for the view's `validate_shape`: either outcome, recorded with the header it was given.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn recorded_shape_check<'a, T>(market: &MarketGroupV16ViewMut<'a, T>) -> V16Result<()>
where
    'a: 'a,
{
    let passed: bool = kani::any();
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        LAST_SHAPE_CHECK = Some((*market.header, passed));
    }
    if passed {
        Ok(())
    } else {
        Err(kani::any())
    }
}

/// Records a passed check on the header the market view holds now.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn record_passed_shape_check<'a, T>(market: &MarketGroupV16ViewMut<'a, T>)
where
    'a: 'a,
{
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        LAST_SHAPE_CHECK = Some((*market.header, true));
    }
}
#[cfg(all(kani, feature = "closure"))]
fn havoc_market_view<'a, T>(market: &mut MarketGroupV16ViewMut<'a, T>)
where
    'a: 'a,
{
    let header: [u8; core::mem::size_of::<MarketGroupV16HeaderAccount>()] = kani::any();
    bytemuck::bytes_of_mut(&mut *market.header).copy_from_slice(&header);
    for slot in market.markets.iter_mut() {
        let engine: [u8; core::mem::size_of::<EngineAssetSlotV16Account>()] = kani::any();
        bytemuck::bytes_of_mut(&mut slot.engine).copy_from_slice(&engine);
    }
    market.protection_observed = kani::any();
}

/// Stand-in for an account's own check against the market: it takes both by shared reference, so
/// it writes nothing; either outcome.
#[cfg(all(kani, feature = "closure"))]
fn stand_in_account_check_any_outcome<'a, T>(
    _account: &PortfolioV16ViewMut<'a>,
    _market: &MarketGroupV16View<'_, T>,
) -> V16Result<()>
where
    'a: 'a,
{
    if kani::any() {
        Ok(())
    } else {
        Err(kani::any())
    }
}

/// Any values for the account fields that the entry points of this section read themselves.
#[cfg(all(kani, feature = "closure"))]
fn arbitrary_read_fields(account: &mut PortfolioAccountV16Account) {
    account.capital = V16PodU128::new(kani::any());
    account.pnl = V16PodI128::new(kani::any());
    account.reserved_pnl = V16PodU128::new(kani::any());
    account.fee_credits = V16PodI128::new(kani::any());
    account.cancel_deposit_escrow = V16PodU128::new(kani::any());
    account.last_fee_slot = V16PodU64::new(kani::any());
    let words: [u64; V16_ACTIVE_BITMAP_WORDS] = kani::any();
    account.active_bitmap = words.map(V16PodU64::new);
    let cert: [u8; core::mem::size_of::<HealthCertV16Account>()] = kani::any();
    bytemuck::bytes_of_mut(&mut account.health_cert).copy_from_slice(&cert);
    let close: [u8; core::mem::size_of::<CloseProgressLedgerV16Account>()] = kani::any();
    bytemuck::bytes_of_mut(&mut account.close_progress).copy_from_slice(&close);
}

#[cfg(all(kani, feature = "closure"))]
fn havoc_account(account: &mut PortfolioV16ViewMut<'_>) {
    arbitrary_read_fields(account.header);
}

#[cfg(all(kani, feature = "closure"))]
fn arbitrary_header_and_slot() -> (MarketGroupV16HeaderAccount, [Market<u64>; 1]) {
    let bytes: [u8; core::mem::size_of::<MarketGroupV16HeaderAccount>()] = kani::any();
    let mut header: MarketGroupV16HeaderAccount = bytemuck::Zeroable::zeroed();
    bytemuck::bytes_of_mut(&mut header).copy_from_slice(&bytes);
    let engine: [u8; core::mem::size_of::<EngineAssetSlotV16Account>()] = kani::any();
    let mut slot = EngineAssetSlotV16Account::default();
    bytemuck::bytes_of_mut(&mut slot).copy_from_slice(&engine);
    (header, [Market::new(kani::any(), slot)])
}

#[cfg(all(kani, feature = "closure"))]
fn arbitrary_account() -> PortfolioAccountV16Account {
    let mut account = PortfolioAccountV16Account::default();
    arbitrary_read_fields(&mut account);
    account
}

/// Ok leaves the header bytes unchanged, or the last shape check passed on exactly the header left.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn assert_ok_keeps_the_header_predicate<'a, T>(
    ok: bool,
    before: &MarketGroupV16HeaderAccount,
    market: &MarketGroupV16ViewMut<'a, T>,
) where
    'a: 'a,
{
    if ok {
        let now = bytemuck::bytes_of(&*market.header);
        let unchanged = now == bytemuck::bytes_of(before);
        // SAFETY: Kani runs one harness on one thread.
        let checked = match unsafe { LAST_SHAPE_CHECK } {
            Some((checked_header, passed)) => passed && bytemuck::bytes_of(&checked_header) == now,
            None => false,
        };
        assert!(unchanged || checked);
    }
}

/// Every Ok follows a passed check on exactly the header left (for a callee with no early Ok, whose
/// stand-in then records a passed check on every Ok).
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn assert_ok_follows_a_passed_check<'a, T>(ok: bool, market: &MarketGroupV16ViewMut<'a, T>)
where
    'a: 'a,
{
    if ok {
        // SAFETY: Kani runs one harness on one thread.
        let checked = match unsafe { LAST_SHAPE_CHECK } {
            Some((checked_header, passed)) => {
                passed && bytemuck::bytes_of(&checked_header) == bytemuck::bytes_of(&*market.header)
            }
            None => false,
        };
        assert!(checked);
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_reservation_encumbrance_proof() -> ReservationEncumbranceProofV16 {
    ReservationEncumbranceProofV16 {
        domain: kani::any(),
        exact_positive_claim_num: kani::any(),
        positive_claim_bound_num: kani::any(),
        source_fresh_reserved_backing_num: kani::any(),
        source_spent_backing_num: kani::any(),
        source_provider_receivable_num: kani::any(),
        bucket_fresh_unliened_backing_num: kani::any(),
        bucket_valid_liened_backing_num: kani::any(),
        bucket_consumed_liened_backing_num: kani::any(),
        source_valid_liened_backing_num: kani::any(),
        source_impaired_liened_backing_num: kani::any(),
        bucket_impaired_liened_backing_num: kani::any(),
        source_insurance_credit_reserved_num: kani::any(),
        reservation_insurance_credit_reserved_num: kani::any(),
        source_valid_liened_insurance_num: kani::any(),
        reservation_valid_liened_insurance_num: kani::any(),
        source_impaired_liened_insurance_num: kani::any(),
        reservation_impaired_liened_insurance_num: kani::any(),
        source_credit_rate_num: kani::any(),
    }
}

// Values of the engine's plain records that the stand-ins return or the harnesses pass: every field any value.
#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for TradeRequestV16 {
    fn any() -> Self {
        TradeRequestV16 {
            asset_index: kani::any(),
            size_q: kani::any(),
            exec_price: kani::any(),
            fee_bps: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for TradeOutcomeV16 {
    fn any() -> Self {
        TradeOutcomeV16 {
            fee_a: kani::any(),
            fee_b: kani::any(),
            notional: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for BatchTradeOutcomeV16 {
    fn any() -> Self {
        BatchTradeOutcomeV16 {
            fill_count: kani::any(),
            fee_a: kani::any(),
            fee_b: kani::any(),
            notional: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
impl kani::Arbitrary for SourceCreditConsumptionV16 {
    fn any() -> Self {
        SourceCreditConsumptionV16 {
            face_burn: kani::any(),
            counterparty_credit_consumed: kani::any(),
            insurance_credit_consumed: kani::any(),
        }
    }
}

#[cfg(all(kani, feature = "closure"))]
fn any_progress_outcome() -> PermissionlessProgressOutcomeV16 {
    match kani::any::<u8>() {
        0 => PermissionlessProgressOutcomeV16::AccountCurrent,
        1 => PermissionlessProgressOutcomeV16::AccountBChunk(AccountBSettlementChunkV16 {
            delta_b: kani::any(),
            loss: kani::any(),
            new_remainder: kani::any(),
            remaining_after: kani::any(),
        }),
        2 => PermissionlessProgressOutcomeV16::SourceBackingExpired {
            domain: kani::any(),
        },
        3 => PermissionlessProgressOutcomeV16::ResidualBooked(BResidualBookingOutcomeV16 {
            booked_loss: kani::any(),
            explicit_loss: kani::any(),
            delta_b: kani::any(),
            remaining_after: kani::any(),
        }),
        _ => PermissionlessProgressOutcomeV16::RecoveryDeclared(kani::any()),
    }
}
