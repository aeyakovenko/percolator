// Account pieces of class "final" (T*.account): the path structure, with every callee replaced.
//
// The ledger states each such piece as: every portfolio account the entry point is given passes
// `validate_with_market` against the market it leaves, if it did before and the call returns Ok.
// The check reads the account, the market header and the asset slots, and nothing else (it takes
// the account and the market view by shared reference). Each harness below checks the path
// structure of the entry point's own body:
// - every method it calls on the market view is replaced by a stand-in: one that takes the view
//   by shared reference returns any value; one that takes it mutably may first write any bytes to
//   the header, the asset slots, the view's scratch and every account it is given, then returns
//   any value. Every behaviour of the real callee is among them;
// - the account's own check is replaced by a recorder that returns either outcome and remembers,
//   per account, its answer and what it was asked about, so the proof holds whatever the check
//   is; the view's `validate_shape` (a read) returns either outcome; pure helpers run as written,
//   except a pure function a harness names in its stubs (`acct_any_pure_*`): it takes its inputs
//   by value or by shared reference and writes nothing, and its stand-in returns any result;
// - the header, the slot and every account are arbitrary bytes. The entry point gets a view built
//   directly on the arbitrary account (`PortfolioV16ViewMut { header }`), not through
//   `PortfolioV16ViewMut::new`, which first sorts the account's source-domain table: every account
//   `new` can hand over is among the arbitrary ones, so no input is lost.
// The assertion, for each account the entry point is given: an Ok return leaves the account, the
// header and the slot as they were, or the account's last check passed on exactly the account,
// header and slot the entry point leaves. Either way the account passes the check after if it did
// before. A callee that has no early Ok and ends in its accounts' checks is proved the stronger way
// (every Ok follows a passed check of each of its accounts on the state it leaves) and is replaced
// in its callers by a stand-in with that property. The generator checks that the entry points read
// no slot themselves and give `self` and the accounts to nothing but their fields, their own
// checks and the replaced callees.
//
// Comparing whole states (an account, the header and a slot are several kilobytes) costs the
// verifier too much, so a harness compares one byte of each state, at positions it leaves
// arbitrary: `i` for "as they were" and `j` for "as checked" (a record keeps byte `j` of the state
// its check was asked about). The assertion `before[i] == now[i] || (passed && checked[j] ==
// now[j])` holding for every `i` and every `j` is the same as `before == now || (passed &&
// checked == now)`: if the state changed at some `i0` and differs from the checked one at some
// `j0`, the pair (`i0`, `j0`) fails it.

/// The bytes the account check reads, in one sequence: the account, the header and the one slot
/// of the harnesses.
#[cfg(all(kani, feature = "closure"))]
const ACCOUNT_CHECK_STATE_LEN: usize = core::mem::size_of::<PortfolioAccountV16Account>()
    + core::mem::size_of::<MarketGroupV16HeaderAccount>()
    + core::mem::size_of::<EngineAssetSlotV16Account>();

/// Byte `k` of that sequence.
#[cfg(all(kani, feature = "closure"))]
fn account_check_state_byte(
    account: &PortfolioAccountV16Account,
    header: &MarketGroupV16HeaderAccount,
    slot: &EngineAssetSlotV16Account,
    k: usize,
) -> u8 {
    let a = core::mem::size_of::<PortfolioAccountV16Account>();
    let h = core::mem::size_of::<MarketGroupV16HeaderAccount>();
    if k < a {
        bytemuck::bytes_of(account)[k]
    } else if k < a + h {
        bytemuck::bytes_of(header)[k - a]
    } else {
        bytemuck::bytes_of(slot)[k - a - h]
    }
}

/// `j`: the byte of the checked state that each record keeps.
#[cfg(all(kani, feature = "closure"))]
static mut ACCOUNT_CHECK_PROBE: usize = 0;

#[cfg(all(kani, feature = "closure"))]
#[derive(Clone, Copy)]
struct RecordedAccountCheck {
    account: *const PortfolioAccountV16Account,
    probed: u8,
    passed: bool,
}

#[cfg(all(kani, feature = "closure"))]
static mut ACCOUNT_CHECKS: [Option<RecordedAccountCheck>; 2] = [None, None];

/// Chooses the two positions: returns `i`, keeps `j` for the recorder.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn choose_account_check_probes() -> usize {
    let i: usize = kani::any();
    kani::assume(i < ACCOUNT_CHECK_STATE_LEN);
    let j: usize = kani::any();
    kani::assume(j < ACCOUNT_CHECK_STATE_LEN);
    // SAFETY: Kani runs one harness on one thread.
    unsafe {
        ACCOUNT_CHECK_PROBE = j;
    }
    i
}

/// Remembers a check of `account` against the market's header and its one slot (the harnesses have
/// one), per account: at most two accounts.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn record_account_check<T>(
    account: &PortfolioAccountV16Account,
    market: &MarketGroupV16View<'_, T>,
    passed: bool,
) {
    assert!(market.markets.len() == 1);
    // SAFETY: Kani runs one harness on one thread.
    let j = unsafe { ACCOUNT_CHECK_PROBE };
    let record = RecordedAccountCheck {
        account: account as *const PortfolioAccountV16Account,
        probed: account_check_state_byte(account, market.header, &market.markets[0].engine, j),
        passed,
    };
    // SAFETY: as above.
    let checks = unsafe { ACCOUNT_CHECKS };
    let k = match checks {
        [None, _] => 0,
        [Some(r), _] if r.account == record.account => 0,
        [_, None] => 1,
        [_, Some(r)] if r.account == record.account => 1,
        _ => panic!("more than two accounts"),
    };
    // SAFETY: as above.
    unsafe {
        ACCOUNT_CHECKS[k] = Some(record);
    }
}

/// Stand-in for an account's own check against the market: either outcome, recorded.
#[cfg(all(kani, feature = "closure"))]
fn recorded_account_check<'a, T>(
    account: &PortfolioV16ViewMut<'a>,
    market: &MarketGroupV16View<'_, T>,
) -> V16Result<()>
where
    'a: 'a,
{
    let passed: bool = kani::any();
    record_account_check(&*account.header, market, passed);
    if passed {
        Ok(())
    } else {
        Err(kani::any())
    }
}

/// Records a passed check of the account on the market the view holds now.
#[cfg(all(kani, feature = "closure"))]
fn record_passed_account_check<'a, T>(
    account: &PortfolioV16ViewMut<'_>,
    market: &MarketGroupV16ViewMut<'a, T>,
) where
    'a: 'a,
{
    record_account_check(&*account.header, &market.as_view(), true);
}

/// Stand-in for the view's `validate_shape`, a read: either outcome.
#[cfg(all(kani, feature = "closure"))]
fn acct_any_shape_check<'a, T>(_market: &MarketGroupV16ViewMut<'a, T>) -> V16Result<()>
where
    'a: 'a,
{
    if kani::any() {
        Ok(())
    } else {
        Err(kani::any())
    }
}

/// Any bytes in the whole account.
#[cfg(all(kani, feature = "closure"))]
fn havoc_whole_account(account: &mut PortfolioV16ViewMut<'_>) {
    let bytes: [u8; core::mem::size_of::<PortfolioAccountV16Account>()] = kani::any();
    bytemuck::bytes_of_mut(&mut *account.header).copy_from_slice(&bytes);
}

#[cfg(all(kani, feature = "closure"))]
fn arbitrary_whole_account() -> PortfolioAccountV16Account {
    let mut account = PortfolioAccountV16Account::default();
    let bytes: [u8; core::mem::size_of::<PortfolioAccountV16Account>()] = kani::any();
    bytemuck::bytes_of_mut(&mut account).copy_from_slice(&bytes);
    account
}

/// The last recorded check of `account` passed, and kept the byte `j` that the account and the
/// market left have.
#[cfg(all(kani, feature = "closure"))]
#[allow(unsafe_code)]
fn last_account_check_passed_on<'a, T>(
    account: &PortfolioAccountV16Account,
    market: &MarketGroupV16ViewMut<'a, T>,
) -> bool
where
    'a: 'a,
{
    // SAFETY: Kani runs one harness on one thread.
    let j = unsafe { ACCOUNT_CHECK_PROBE };
    let now = account_check_state_byte(account, market.header, &market.markets[0].engine, j);
    // SAFETY: as above.
    let checks = unsafe { ACCOUNT_CHECKS };
    let hit = |r: Option<RecordedAccountCheck>| match r {
        Some(r) => {
            r.account == account as *const PortfolioAccountV16Account && r.passed && r.probed == now
        }
        None => false,
    };
    hit(checks[0]) || hit(checks[1])
}

/// Ok leaves byte `i` of the account's state as it was, or the last check of the account passed
/// and kept byte `j` of the state left.
#[cfg(all(kani, feature = "closure"))]
fn assert_ok_keeps_the_account_check<'a, T>(
    ok: bool,
    account: &PortfolioAccountV16Account,
    i: usize,
    before: u8,
    market: &MarketGroupV16ViewMut<'a, T>,
) where
    'a: 'a,
{
    if ok {
        let account_unchanged =
            account_check_state_byte(account, market.header, &market.markets[0].engine, i) == before;
        let account_checked = last_account_check_passed_on(account, market);
        assert!(account_unchanged || account_checked);
    }
}

/// Every Ok follows a passed check of the account on the state left (for a callee with no early
/// Ok, whose stand-in then records a passed check on every Ok).
#[cfg(all(kani, feature = "closure"))]
fn assert_ok_follows_a_passed_account_check<'a, T>(
    ok: bool,
    account: &PortfolioAccountV16Account,
    market: &MarketGroupV16ViewMut<'a, T>,
) where
    'a: 'a,
{
    if ok {
        let account_checked = last_account_check_passed_on(account, market);
        assert!(account_checked);
    }
}
