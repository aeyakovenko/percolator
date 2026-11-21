use percolator::{MarketGroupV16ViewMut, PortfolioV16ViewMut};

// The runner materializes Wrapper from its list of concrete market wrapper types.
include!("wrapper.rs");

#[no_mangle]
pub fn positive_counterexample(
    market: &mut MarketGroupV16ViewMut<'_, Wrapper>,
    portfolio: &mut PortfolioV16ViewMut<'_>,
    amount: u128,
) -> bool {
    if amount == 0 || market.deposit_not_atomic(portfolio, amount).is_err() {
        return false;
    }
    #[cfg(feature = "corrupt-header")]
    {
        market.header.next_market_id = percolator::V16PodU64::new(0);
    }
    market.validate_shape().is_err()
}

#[no_mangle]
pub fn zero_counterexample(
    market: &mut MarketGroupV16ViewMut<'_, Wrapper>,
    portfolio: &mut PortfolioV16ViewMut<'_>,
) -> bool {
    if market.validate_shape().is_err() || market.deposit_not_atomic(portfolio, 0).is_err() {
        return false;
    }
    #[cfg(feature = "corrupt-header")]
    {
        market.header.next_market_id = percolator::V16PodU64::new(0);
    }
    market.validate_shape().is_err()
}

#[no_mangle]
pub fn header_ok(market: &MarketGroupV16ViewMut<'_, Wrapper>) -> bool {
    market.validate_shape().is_ok()
}
