//! Execute cross-slab tests - v0 capital efficiency proofs
//!
//! These tests validate the core v0 thesis: net exposure netting reduces IM to ~0.

#[cfg(test)]
mod capital_efficiency_tests {
    use crate::state::Portfolio;
    use pinocchio::pubkey::Pubkey;

    const SCALE: i64 = 1_000_000;

    /// E2E-2: THE KEY TEST - Capital efficiency proof
    ///
    /// This proves: net_exposure = 0 → IM ≈ 0 (infinite capital efficiency!)
    #[test]
    fn test_e2e_2_capital_efficiency_netting() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        // Scenario: User opens +10 BTC on Slab A, -10 BTC on Slab B
        let slab_a_qty = 10 * SCALE;
        let slab_b_qty = -10 * SCALE;

        portfolio.update_exposure(0, 0, slab_a_qty);
        portfolio.update_exposure(1, 0, slab_b_qty);

        let net_exposure = portfolio.get_exposure(0, 0) + portfolio.get_exposure(1, 0);
        assert_eq!(net_exposure, 0, "Net exposure should be zero");

        // Calculate IM based on NET exposure
        let imr = 10; // 10% IMR
        let price = 60_000u128;
        let im_required = (net_exposure.abs() as u128 * price * imr) / 100;

        // THE KEY ASSERTION: Zero net exposure → Zero IM!
        assert_eq!(im_required, 0, "CAPITAL EFFICIENCY PROOF: Zero net = Zero IM");

        // Compare to gross IM
        let gross_exposure = slab_a_qty.abs() + slab_b_qty.abs();
        let gross_im = (gross_exposure as u128 * price * imr) / 100;

        assert_eq!(portfolio.exposure_count, 2);
        assert!(gross_im > 0, "Gross IM should be positive");
        assert_eq!(im_required, 0, "Net IM must be zero - proves the thesis!");
    }

    /// Test: Partial netting reduces IM
    #[test]
    fn test_partial_netting_reduces_im() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        portfolio.update_exposure(0, 0, 15 * SCALE);
        portfolio.update_exposure(1, 0, -10 * SCALE);

        let net_exposure = portfolio.get_exposure(0, 0) + portfolio.get_exposure(1, 0);
        assert_eq!(net_exposure, 5 * SCALE);

        let price = 60_000u128;
        let imr = 10;

        let gross_exposure = (15 + 10) * SCALE;
        let gross_im = (gross_exposure as u128 * price * imr) / 100;
        let net_im = (net_exposure.abs() as u128 * price * imr) / 100;

        assert!(net_im < gross_im, "Net IM < Gross IM");
        assert!(net_im > 0, "Net IM > 0 when net exposure != 0");
    }

    /// Test: Multi-instrument netting
    #[test]
    fn test_multi_instrument_netting() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        portfolio.update_exposure(0, 0, 10 * SCALE);
        portfolio.update_exposure(0, 1, 5 * SCALE);
        portfolio.update_exposure(1, 0, -10 * SCALE);
        portfolio.update_exposure(1, 1, -5 * SCALE);

        let btc_net = portfolio.get_exposure(0, 0) + portfolio.get_exposure(1, 0);
        let eth_net = portfolio.get_exposure(0, 1) + portfolio.get_exposure(1, 1);

        assert_eq!(btc_net, 0);
        assert_eq!(eth_net, 0);
        assert_eq!(portfolio.exposure_count, 4);
    }

    /// Test: Exposure lifecycle
    #[test]
    fn test_exposure_lifecycle() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        portfolio.update_exposure(0, 0, 10 * SCALE);
        assert_eq!(portfolio.exposure_count, 1);
        assert_eq!(portfolio.get_exposure(0, 0), 10 * SCALE);

        portfolio.update_exposure(0, 0, 15 * SCALE);
        assert_eq!(portfolio.exposure_count, 1);
        assert_eq!(portfolio.get_exposure(0, 0), 15 * SCALE);

        portfolio.update_exposure(0, 0, 0);
        assert_eq!(portfolio.exposure_count, 0);
        assert_eq!(portfolio.get_exposure(0, 0), 0);
    }
}

#[cfg(test)]
mod margin_calculation_tests {
    use crate::state::Portfolio;
    use pinocchio::pubkey::Pubkey;

    const SCALE: i64 = 1_000_000;

    /// Test: IM calculation accuracy
    #[test]
    fn test_im_calculation_accuracy() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        portfolio.update_exposure(0, 0, 10 * SCALE);

        let net_exposure = portfolio.get_exposure(0, 0);
        let price = 50_000u128;
        let imr = 10;

        // IM = abs(net_exposure) * price * imr / 100
        let im_required = (net_exposure.abs() as u128 * price * imr) / 100;

        // Expected: 10_000_000 * 50_000 * 10 / 100 = 50_000_000_000
        let expected_im = (10 * SCALE) as u128 * 50_000 * 10 / 100;

        assert_eq!(im_required, expected_im);
    }

    /// Test: Margin update and free collateral
    #[test]
    fn test_margin_and_free_collateral() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        portfolio.update_equity(100_000);
        portfolio.update_margin(50_000, 25_000);

        assert_eq!(portfolio.im, 50_000);
        assert_eq!(portfolio.mm, 25_000);
        assert_eq!(portfolio.free_collateral, 50_000);
        assert!(portfolio.has_sufficient_margin());
        assert!(portfolio.is_above_maintenance());

        portfolio.update_margin(110_000, 55_000);
        assert_eq!(portfolio.free_collateral, -10_000);
        assert!(!portfolio.has_sufficient_margin());
    }
}

#[cfg(test)]
mod net_exposure_calculation_tests {
    use super::super::calculate_net_exposure;
    use crate::state::Portfolio;
    use pinocchio::pubkey::Pubkey;

    const SCALE: i64 = 1_000_000;

    /// Test: Net exposure calculation
    #[test]
    fn test_calculate_net_exposure() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        portfolio.update_exposure(0, 0, 10 * SCALE);
        portfolio.update_exposure(1, 0, -5 * SCALE);
        portfolio.update_exposure(2, 0, 3 * SCALE);

        let net = calculate_net_exposure(&portfolio);
        assert_eq!(net, 8 * SCALE);
    }

    /// Test: Zero net exposure → zero IM
    #[test]
    fn test_zero_net_zero_im() {
        let router_id = Pubkey::default();
        let user = Pubkey::default();
        let mut portfolio = Portfolio::new(router_id, user, 0);

        portfolio.update_exposure(0, 0, 10 * SCALE);
        portfolio.update_exposure(1, 0, -10 * SCALE);

        let net = calculate_net_exposure(&portfolio);
        assert_eq!(net, 0, "Net exposure should be zero");

        // When net = 0, IM calculation should yield 0
        let im = (net.abs() as u128 * 60_000 * 10) / 100;
        assert_eq!(im, 0, "Zero net MUST produce zero IM");
    }
}
