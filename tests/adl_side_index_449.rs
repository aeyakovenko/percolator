//! percolator-prog#449, the fix: a side keeps the sum of its legs' raw basis
//! and derives its A index from it, and a same-side reduction is rounded
//! against the reducing leg.
//!
//! These tests exercise the two kernels on their own, without the engine
//! around them, and check the arithmetic facts the fix rests on with exact
//! integer comparisons (no floating point, no tolerance):
//!
//! - `kernel_side_adl_index`: `A = max(1, floor(ADL_ONE * OI / R))`, so the
//!   side's exact exposure `R * A / ADL_ONE` is never above `OI` and is short
//!   of it by less than `R / ADL_ONE` raw units.
//! - Every leg's effective quantity is at most `OI`, the legs' effective
//!   quantities cover `OI` and exceed it by less than one per leg, and a
//!   side's only leg has exactly `OI`.
//! - `kernel_raw_basis_after_adl_reduction`: the reducing leg's share of the
//!   side falls by at least the effective OI it removes, no other leg's share
//!   falls (except when the reducing leg is held at its last raw unit), and
//!   the reducing leg's new effective quantity is at most what it asked for.
//! - A full close removes at most one raw unit more than the leg's share.
//! - With the `audit-scan` feature: the audit scan's per-side predicate, which
//!   checks the index by multiplication, agrees with `kernel_side_adl_index`.
//!
//! Notation: `R` raw basis sum, `OI` effective OI, `r` a leg's raw basis,
//! share `s = r * OI / R` (a rational, compared by cross-multiplication),
//! effective quantity `e = ceil(r * A / ADL_ONE)`.
#![cfg(feature = "fuzz")]

use percolator::v16::{
    kani_adl_effective_quantity_ceil, kani_raw_basis_after_adl_reduction, kani_side_adl_index,
};
#[cfg(feature = "audit-scan")]
use percolator::{v16::kani_side_adl_totals_are_coherent, SideModeV16, MIN_A_SIDE};
use percolator::{V16Error, ADL_ONE, MAX_OI_SIDE_Q};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi`.
    fn range(&mut self, lo: u128, hi: u128) -> u128 {
        assert!(lo <= hi);
        let wide = (u128::from(self.next()) << 64) | u128::from(self.next());
        lo + wide % (hi - lo + 1)
    }

    /// Sizes spread over every order of magnitude up to `hi`.
    fn size(&mut self, hi: u128) -> u128 {
        let bits = 128 - hi.leading_zeros();
        let cap = (1u128 << self.range(1, u128::from(bits))) - 1;
        self.range(1, cap.min(hi).max(1))
    }
}

fn effective(raw: u128, a: u128) -> u128 {
    kani_adl_effective_quantity_ceil(raw, ADL_ONE, a).unwrap()
}

/// The contract of `kernel_side_adl_index` for one pair of totals.
fn check_index(oi: u128, raw: u128) -> u128 {
    let a = kani_side_adl_index(oi, raw, 12_345).unwrap();
    if raw == 0 {
        assert_eq!(a, 12_345, "no basis: the index keeps its value");
        return a;
    }
    assert!((1..=ADL_ONE).contains(&a), "oi={oi} raw={raw} a={a}");
    assert_eq!(a == ADL_ONE, oi == raw, "unit A exactly when OI == R");
    assert_eq!(a, (ADL_ONE * oi / raw).max(1));
    // within one index step below the exact ratio
    assert!(ADL_ONE * oi < (a + 1) * raw, "oi={oi} raw={raw} a={a}");
    if oi == 0 {
        assert_eq!(a, 1);
    } else {
        // never above the exact ratio: exposure R * A / ADL_ONE <= OI
        assert!(a * raw <= ADL_ONE * oi, "oi={oi} raw={raw} a={a}");
    }
    a
}

#[test]
fn side_index_matches_its_contract_on_every_small_pair_of_totals() {
    for raw in 0u128..=96 {
        for oi in 0..=raw {
            check_index(oi, raw);
        }
        if raw != 0 {
            assert_eq!(
                kani_side_adl_index(raw + 1, raw, ADL_ONE),
                Err(V16Error::InvalidLeg),
                "more effective OI than raw basis has no index"
            );
        }
    }
    // unowned OI on a side without basis keeps the index it had
    assert_eq!(kani_side_adl_index(7, 0, ADL_ONE / 3), Ok(ADL_ONE / 3));
}

#[test]
fn side_index_matches_its_contract_up_to_twice_the_side_cap() {
    let mut rng = Rng(0x449);
    // raw basis exceeds MAX_OI_SIDE_Q only inside a fill, before the other leg is applied
    let cap = 2 * MAX_OI_SIDE_Q;
    for raw in [1, 2, 3, 10, MAX_OI_SIDE_Q - 1, MAX_OI_SIDE_Q, cap - 1, cap] {
        for oi in [
            0,
            1,
            2,
            raw / 3,
            raw / 2,
            raw.saturating_sub(2),
            raw - 1,
            raw,
        ] {
            if oi <= raw {
                check_index(oi, raw);
            }
        }
    }
    for _ in 0..200_000 {
        let raw = rng.size(cap);
        let oi = match rng.range(0, 3) {
            0 => rng.range(0, raw),
            1 => raw - rng.size(raw).min(raw),
            2 => rng.size(raw).min(raw),
            _ => raw,
        };
        let a = check_index(oi, raw);
        // while any effective OI is left the index is at least 5 (10 under the cap itself)
        if oi != 0 {
            assert!(a >= 5);
        }
    }
}

/// The audit scan (`validate_shape` under test, Kani or the `audit-scan`
/// feature) checks a side's index by multiplication instead of calling the
/// kernel. This is the same predicate stated with the kernel; the two have to
/// agree on every side whose totals fit 64 bits, which every reachable side
/// does by a wide margin.
#[cfg(feature = "audit-scan")]
fn coherent_by_the_kernel(oi: u128, raw: u128, a: u128, weight: u128, mode: SideModeV16) -> bool {
    if raw > weight {
        return false;
    }
    if raw == 0 {
        return true;
    }
    oi <= raw
        && kani_side_adl_index(oi, raw, a) == Ok(a)
        && !(mode == SideModeV16::Normal && oi != 0 && a < MIN_A_SIDE)
}

#[cfg(feature = "audit-scan")]
#[test]
fn audit_scan_side_predicate_agrees_with_the_index_kernel() {
    let modes = [
        SideModeV16::Normal,
        SideModeV16::DrainOnly,
        SideModeV16::ResetPending,
    ];
    let mut accepted = 0u64;
    let mut rejected = 0u64;
    let mut check = |oi: u128, raw: u128, a: u128, weight: u128| {
        for mode in modes {
            let expected = coherent_by_the_kernel(oi, raw, a, weight, mode);
            assert_eq!(
                kani_side_adl_totals_are_coherent(oi, raw, a, weight, mode),
                expected,
                "oi={oi} raw={raw} a={a} weight={weight} mode={mode:?}"
            );
            if expected {
                accepted += 1;
            } else {
                rejected += 1;
            }
        }
    };
    // Index candidates: the derived one, its neighbours, and every boundary
    // the predicate branches on.
    let candidates = |oi: u128, raw: u128| -> [u128; 13] {
        let derived = if raw != 0 && oi <= raw {
            kani_side_adl_index(oi, raw, 7).unwrap()
        } else {
            7
        };
        [
            0,
            1,
            2,
            derived.saturating_sub(1),
            derived,
            derived + 1,
            MIN_A_SIDE - 1,
            MIN_A_SIDE,
            ADL_ONE - 1,
            ADL_ONE,
            ADL_ONE + 1,
            u128::from(u64::MAX),
            u128::from(u64::MAX) + 1,
        ]
    };
    // Every small pair of totals, with one unit of effective OI too many.
    for raw in 0u128..=64 {
        for oi in 0..=raw + 1 {
            for a in candidates(oi, raw) {
                for weight in [raw.saturating_sub(1), raw, raw + 1] {
                    check(oi, raw, a, weight);
                }
            }
        }
    }
    // Random sides at every scale up to twice the side cap.
    let mut rng = Rng(0x0449_a0d1);
    let cap = 2 * MAX_OI_SIDE_Q;
    for _ in 0..100_000 {
        let raw = rng.size(cap);
        let oi = match rng.range(0, 9) {
            0 => raw,
            1 => 0,
            2 => raw + 1,
            _ => rng.range(0, raw),
        };
        for a in candidates(oi, raw) {
            check(oi, raw, a, raw);
        }
        let a = rng.range(0, ADL_ONE + 1);
        let weight = raw + rng.range(0, 2) - 1;
        check(oi, raw, a, weight);
    }
    assert!(accepted > 100_000 && rejected > 100_000);

    // The one difference, far outside any reachable state: totals beyond 64
    // bits. The kernel still derives an index for them; the scan calls the
    // side incoherent unless it is at unit A.
    let wide = u128::from(u64::MAX) + 1;
    assert_eq!(kani_side_adl_index(wide / 2, wide, 0), Ok(ADL_ONE / 2));
    assert!(!kani_side_adl_totals_are_coherent(
        wide / 2,
        wide,
        ADL_ONE / 2,
        wide,
        SideModeV16::DrainOnly
    ));
    assert!(kani_side_adl_totals_are_coherent(
        wide,
        wide,
        ADL_ONE,
        wide,
        SideModeV16::Normal
    ));
}

/// A side of legs, kept the way the engine keeps it: the two totals, the A
/// index derived from them after every change, and the legs' raw basis.
#[derive(Clone, Debug)]
struct Side {
    legs: Vec<u128>,
    oi: u128,
    a: u128,
}

impl Side {
    fn raw_sum(&self) -> u128 {
        self.legs.iter().sum()
    }

    fn sync(&mut self) {
        self.a = kani_side_adl_index(self.oi, self.raw_sum(), self.a).unwrap();
    }

    fn eff(&self, i: usize) -> u128 {
        effective(self.legs[i], self.a)
    }

    /// The facts that hold for every haircut or unit side with legs.
    fn check(&self, what: &str) {
        let raw = self.raw_sum();
        let n = self.legs.len() as u128;
        assert!(self.legs.iter().all(|r| *r != 0), "{what}: {self:?}");
        if raw == 0 {
            return;
        }
        assert!(self.oi <= raw, "{what}: {self:?}");
        assert_eq!(self.a, (ADL_ONE * self.oi / raw).max(1), "{what}: {self:?}");
        let ceilings: u128 = (0..self.legs.len()).map(|i| self.eff(i)).sum();
        if self.oi == 0 {
            // exhausted: every leg shows one unit and owns nothing
            assert_eq!(self.a, 1, "{what}: {self:?}");
            assert_eq!(ceilings, n, "{what}: {self:?}");
            return;
        }
        // exact exposure R * A <= OI * ADL_ONE < R * A + R
        assert!(raw * self.a <= self.oi * ADL_ONE, "{what}: {self:?}");
        assert!(self.oi * ADL_ONE < raw * self.a + raw, "{what}: {self:?}");
        // ceilings cover the OI and exceed it by less than one per leg
        assert!(
            ceilings >= self.oi && ceilings < self.oi + n,
            "{what}: ceilings {ceilings} {self:?}"
        );
        for i in 0..self.legs.len() {
            assert!(
                self.eff(i) >= 1 && self.eff(i) <= self.oi,
                "{what}: leg {i} {self:?}"
            );
        }
        if n == 1 {
            assert_eq!(self.eff(0), self.oi, "{what}: the only leg carries the OI");
        }
    }
}

/// The full 256-bit product of two u128 values, as (high, low).
fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    const MASK: u128 = u64::MAX as u128;
    let (a1, a0) = (a >> 64, a & MASK);
    let (b1, b0) = (b >> 64, b & MASK);
    let low = a0 * b0;
    let mid1 = a1 * b0;
    let mid2 = a0 * b1;
    let high = a1 * b1;
    let carry = ((low >> 64) + (mid1 & MASK) + (mid2 & MASK)) >> 64;
    (
        high + (mid1 >> 64) + (mid2 >> 64) + carry,
        low.wrapping_add(mid1 << 64).wrapping_add(mid2 << 64),
    )
}

/// `a/b <= c/d` for positive denominators, exactly.
fn le_frac(a: u128, b: u128, c: u128, d: u128) -> bool {
    mul_wide(a, d) <= mul_wide(c, b)
}

#[test]
fn wide_product_used_by_these_tests_is_exact() {
    assert_eq!(mul_wide(0, u128::MAX), (0, 0));
    assert_eq!(mul_wide(u128::MAX, 1), (0, u128::MAX));
    assert_eq!(mul_wide(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
    assert_eq!(mul_wide(1 << 100, 1 << 100), (1 << 72, 0));
    assert_eq!(mul_wide(3 << 64, 5 << 64), (15, 0));
    let mut rng = Rng(7);
    for _ in 0..10_000 {
        let (a, b) = (
            rng.range(0, u64::MAX as u128),
            rng.range(0, u64::MAX as u128),
        );
        assert_eq!(mul_wide(a, b), (0, a * b));
        assert_eq!(mul_wide(a << 64, b), (a * b >> 64, (a * b) << 64));
    }
}

/// One same-side partial reduction of leg `i` to effective quantity
/// `next_effective`, with every sharing rule checked.
fn reduce(side: &mut Side, i: usize, next_effective: u128) {
    let before = side.clone();
    let (raw_sum, oi, r) = (before.raw_sum(), before.oi, before.legs[i]);
    let e = before.eff(i);
    assert!(next_effective >= 1 && next_effective < e);
    let d = e - next_effective;
    let next_raw = kani_raw_basis_after_adl_reduction(r, e, next_effective, oi, raw_sum).unwrap();
    assert!(
        next_raw >= 1 && next_raw < r,
        "{before:?} leg {i} -> {next_raw}"
    );
    side.legs[i] = next_raw;
    side.oi = oi - d;
    side.sync();
    side.check("after reduce");
    let (raw_after, oi_after) = (side.raw_sum(), side.oi);
    let other_raw = raw_sum - r;

    // the reducing leg's effective quantity does not end above what it asked for
    assert!(
        side.eff(i) <= next_effective,
        "{before:?} leg {i}: asked {next_effective}, has {}",
        side.eff(i)
    );
    if oi == raw_sum {
        // unit A: nothing is rounded
        assert_eq!(next_raw, next_effective);
        assert_eq!(side.a, ADL_ONE);
        return;
    }
    let held_for_the_side = next_raw + other_raw == oi_after;
    let held_at_one_unit = next_raw == 1;
    // the reducing leg's share falls by at least d:
    //   next_raw * oi_after / raw_after <= r * oi / raw_sum - d
    // (when the leg is held at its last raw unit it can keep up to that unit)
    let share_fell_by_d = r * oi >= d * raw_sum
        && le_frac(
            next_raw * oi_after,
            raw_after,
            r * oi - d * raw_sum,
            raw_sum,
        );
    assert!(
        share_fell_by_d || held_at_one_unit,
        "{before:?} leg {i} to {next_effective}: raw {next_raw}"
    );
    // no other leg's share falls: oi_after / raw_after >= oi / raw_sum
    let others_kept_their_share = le_frac(oi, raw_sum, oi_after, raw_after);
    assert!(
        others_kept_their_share || held_at_one_unit,
        "{before:?} leg {i} to {next_effective}: raw {next_raw}"
    );
    if held_for_the_side {
        assert_eq!(side.a, ADL_ONE, "raw basis equals OI: back at unit A");
    }
    // the result is the largest raw basis that satisfies both rules: one more
    // raw unit would break one of them (unless a floor is what set it)
    if !held_for_the_side && !held_at_one_unit && next_raw + 1 < r {
        let more = next_raw + 1;
        let share_rule = r * oi >= d * raw_sum
            && le_frac(
                more * oi_after,
                other_raw + more,
                r * oi - d * raw_sum,
                raw_sum,
            )
            // the kernel's own form of the share rule: give up ceil(d * R / OI)
            && r - more >= (d * raw_sum).div_ceil(oi);
        let target_rule = more * oi_after <= next_effective * (other_raw + more);
        assert!(
            !(share_rule && target_rule),
            "{before:?} leg {i} to {next_effective}: raw {next_raw} is not the largest"
        );
    }
}

/// A full close of leg `i`: the leg takes its whole effective quantity.
fn close(side: &mut Side, i: usize) {
    let before = side.clone();
    let (raw_sum, oi, r) = (before.raw_sum(), before.oi, before.legs[i]);
    let e = before.eff(i).min(oi);
    side.legs.remove(i);
    side.oi = oi - e;
    if side.legs.is_empty() {
        assert_eq!(side.oi, 0, "{before:?}: the last leg carries the whole OI");
        return;
    }
    side.sync();
    side.check("after close");
    if oi != 0 {
        // the close removes at most one raw unit more than the leg's share:
        //   e - r * oi / raw_sum < 1
        assert!(e * raw_sum < r * oi + raw_sum, "{before:?} leg {i}");
        // and never less than the leg's exposure at the integer index
        assert!(e * ADL_ONE >= r * before.a, "{before:?} leg {i}");
    }
}

/// A haircut: effective OI falls, raw basis stays.
fn haircut(side: &mut Side, q: u128) {
    let before = side.clone();
    side.oi -= q;
    side.sync();
    side.check("after haircut");
    // a haircut never raises any leg's effective quantity
    for i in 0..side.legs.len() {
        assert!(side.eff(i) <= before.eff(i), "{before:?} haircut {q}");
    }
}

fn random_history(seed: u64, max_legs: u128, max_size: u128, steps: usize) {
    let mut rng = Rng(seed);
    let n = rng.range(1, max_legs);
    let legs: Vec<u128> = (0..n).map(|_| rng.size(max_size)).collect();
    let oi = legs.iter().sum();
    let mut side = Side {
        legs,
        oi,
        a: ADL_ONE,
    };
    side.sync();
    side.check("open");
    for _ in 0..steps {
        if side.legs.is_empty() || side.oi == 0 {
            break;
        }
        match rng.range(0, 9) {
            // haircuts, small and deep
            0 | 1 => {
                let q = if rng.range(0, 3) == 0 {
                    rng.range(1, side.oi)
                } else {
                    rng.size(side.oi)
                };
                let q = q.min(side.oi);
                haircut(&mut side, q);
            }
            2 => {
                let i = rng.range(0, side.legs.len() as u128 - 1) as usize;
                close(&mut side, i);
            }
            _ => {
                let i = rng.range(0, side.legs.len() as u128 - 1) as usize;
                let e = side.eff(i);
                if e < 2 {
                    continue;
                }
                // one-unit steps, small steps and large steps
                let next = match rng.range(0, 2) {
                    0 => e - 1,
                    1 => e - rng.size(e - 1),
                    _ => rng.range(1, e - 1),
                };
                reduce(&mut side, i, next.clamp(1, e - 1));
            }
        }
    }
    // whatever is left can leave in any order: every close is exact
    while !side.legs.is_empty() {
        let i = rng.range(0, side.legs.len() as u128 - 1) as usize;
        close(&mut side, i);
        if side.oi == 0 {
            break;
        }
    }
}

#[test]
fn every_small_side_keeps_the_sharing_rules_through_every_single_step() {
    // every side of up to three legs of up to six raw units, every haircut
    // depth, then every single reduction and every single close
    let mut cases = 0u64;
    for l0 in 1u128..=6 {
        for l1 in 0u128..=6 {
            for l2 in 0u128..=6 {
                let legs: Vec<u128> = [l0, l1, l2].into_iter().filter(|r| *r != 0).collect();
                let raw: u128 = legs.iter().sum();
                for oi in 1..=raw {
                    let mut side = Side {
                        legs: legs.clone(),
                        oi,
                        a: ADL_ONE,
                    };
                    side.sync();
                    side.check("open");
                    for i in 0..side.legs.len() {
                        let e = side.eff(i);
                        for next in 1..e {
                            let mut s = side.clone();
                            reduce(&mut s, i, next);
                            cases += 1;
                        }
                        let mut s = side.clone();
                        close(&mut s, i);
                        cases += 1;
                    }
                    for q in 1..=oi {
                        let mut s = side.clone();
                        haircut(&mut s, q);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert!(cases > 10_000, "{cases}");
}

#[test]
fn random_histories_keep_the_sharing_rules_at_every_scale() {
    for seed in 0..4_000u64 {
        // dust sides, where one raw unit is most of a leg
        random_history(seed, 4, 12, 60);
    }
    for seed in 0..4_000u64 {
        // ordinary sizes
        random_history(0x1_0000 + seed, 8, 5_000_000, 80);
    }
    for seed in 0..2_000u64 {
        // up to the side cap, shared by a few legs
        random_history(0x2_0000 + seed, 4, MAX_OI_SIDE_Q / 4, 80);
    }
}

/// The history-free bound of the fix, stated on its own: however many
/// one-unit reductions a leg makes on a haircut side, the side's exact
/// exposure stays within `R / ADL_ONE` of its OI, and what the reducing leg
/// gives up beyond its trades goes to the other legs.
#[test]
fn repeated_one_unit_reductions_move_nothing_out_of_the_side() {
    let mut side = Side {
        legs: vec![8_777, 16_554],
        oi: 21_110,
        a: ADL_ONE,
    };
    side.sync();
    let other_before = side.eff(1);
    let mut traded = 0u128;
    while side.a != ADL_ONE && side.eff(0) >= 2 {
        let e = side.eff(0);
        reduce(&mut side, 0, e - 1);
        traded += 1;
    }
    // 4,221 raw units separated OI from raw basis; each one-unit reduction
    // gave up two raw units (ceil(25_331 / 21_110) = 2), one more than it
    // carried, until the two totals met
    assert_eq!(traded, 25_331 - 21_110);
    assert_eq!(side.a, ADL_ONE);
    assert_eq!(side.oi, 21_110 - traded);
    assert_eq!(side.legs, vec![8_777 - 2 * traded, 16_554]);
    // the untouched leg is back at its raw basis: it received what the
    // reducing leg gave up, never the other way round
    assert!(side.eff(1) > other_before);
    assert_eq!(side.eff(1), 16_554);
    assert_eq!(side.eff(0) + side.eff(1), side.oi);
}
