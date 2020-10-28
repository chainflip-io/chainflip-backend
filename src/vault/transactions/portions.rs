use std::{collections::HashMap, convert::TryInto};

use crate::{
    common::*,
    utils::{self, primitives::U256},
};

use super::memory_provider::{PoolPortions, Portion, StakerOwnership, VaultPortions};

/// Calculate atomic amount for a given portion from total atomic amount
fn amount_from_portion(portion: Portion, total_amount: u128) -> u128 {
    // TODO: might be worth putting this in the constructor of some sort
    assert!(portion.0 <= Portion::MAX.0);

    let portion: U256 = portion.0.into();
    let total_amount: U256 = total_amount.into();

    let amount = portion.checked_mul(total_amount).expect("mul");

    let amount = amount.checked_div(Portion::MAX.0.into()).expect("div");

    amount.try_into().expect("overflow")
}

/// Calculate portion from a given amount and total amount in the pool
fn portion_from_amount(amount: u128, total_amount: u128) -> Portion {
    let amount: U256 = amount.into();
    let total_amount: U256 = total_amount.into();
    let max: U256 = Portion::MAX.0.into();

    let res = amount.checked_mul(max).expect("mul");
    let res = res.checked_div(total_amount).expect("div");

    Portion(res.try_into().expect("overflow"))
}

/// Calculate current ownership in atomic amounts
pub(crate) fn aggregate_current_portions(
    portions: &PoolPortions,
    liquidity: Liquidity,
) -> Vec<StakerOwnership> {
    portions
        .iter()
        .map(|(staker_id, portion)| {
            let loki = amount_from_portion(*portion, liquidity.loki_depth);
            let other = amount_from_portion(*portion, liquidity.depth);

            StakerOwnership {
                staker_id: staker_id.clone(),
                pool_type: PoolCoin::ETH,
                loki: LokiAmount::from_atomic(loki),
                other: GenericCoinAmount::from_atomic(Coin::ETH, other),
            }
        })
        .collect()
}

/// Pool change tx associated with staker id
#[derive(Debug)]
struct EffectiveStakeContribution {
    staker_id: StakerId,
    /// We only need to keep track of the loki amount because
    /// we know the other coin is contributed the equivalent
    /// amount after autoswapping (proportional to the ratio
    /// in the pool at the time of staking)
    loki_amount: LokiAmount,
}

/// Pool change tx associated with staker id
pub(crate) struct StakeContribution {
    staker_id: StakerId,
    loki_amount: LokiAmount,
    /// This amount is actually unused since we can always
    /// assume the contribute is symmetric at this point
    /// (due to autoswap)
    other_amount: GenericCoinAmount,
}

impl StakeContribution {
    /// Into which pool the stake is made
    pub fn coin(&self) -> PoolCoin {
        PoolCoin::from(self.other_amount.coin_type()).expect("invalid coin")
    }

    /// Create for fileds
    pub fn new(
        staker_id: StakerId,
        loki_amount: LokiAmount,
        other_amount: GenericCoinAmount,
    ) -> Self {
        StakeContribution {
            staker_id,
            loki_amount,
            other_amount,
        }
    }
}

fn adjust_portions_after_stake_for_coin(
    portions: &mut PoolPortions,
    liquidity: &Liquidity,
    contribution: &StakeContribution,
) {
    let staker_amounts = aggregate_current_portions(&portions, *liquidity);

    let contribution = compute_effective_contribution(&contribution, &liquidity);

    // Adjust portions for the existing staker ids

    let extra_loki = contribution.loki_amount.to_atomic();

    let new_total_loki = liquidity.loki_depth + extra_loki;

    let mut portions_sum = Portion(0);

    for entry in staker_amounts {
        let portion = portions
            .get_mut(&entry.staker_id)
            .expect("staker entry should exist");

        // Stakes are always symmetric (after auto-swapping any asymmetric stake),
        // so we can use any coin to compute new portions:

        let p = portion_from_amount(entry.loki.to_atomic(), new_total_loki);

        portions_sum = portions_sum.checked_add(p).expect("poritons overflow");

        *portion = p;
    }

    info!("portions sum: {:?}", portions_sum);

    // Give the new staker all remaining portions (to make sure we account for
    // all rounding errors)

    // TODO: add sanity check that the portions assigned to the new staker are
    // proportional to the actual amount contributed

    let portion_left = Portion::MAX
        .checked_sub(portions_sum)
        .expect("portions underflow");

    info!("portions left: {:?}", portion_left);

    let portion = portions
        .entry(contribution.staker_id.clone())
        .or_insert(Portion(0));

    *portion = portion
        .checked_add(portion_left)
        .expect("portions overflow");
}

/// Compute effective contribution, i.e. the contribution to the
/// pool after a potential autoswap
fn compute_effective_contribution(
    stake: &StakeContribution,
    liquidity: &Liquidity,
) -> EffectiveStakeContribution {
    let loki_amount = stake.loki_amount;
    let other_amount = stake.other_amount;

    let effective_loki = if liquidity.depth == 0 {
        info!(
            "First stake into {} pool, autoswap is not performed",
            other_amount.coin_type()
        );
        loki_amount
    } else {
        let (effective_loki, _d_other) =
            utils::autoswap::calc_autoswap_amount(loki_amount, other_amount, *liquidity)
                .expect("incorrect autoswap usage");
        effective_loki
    };

    EffectiveStakeContribution {
        staker_id: stake.staker_id.clone(),
        loki_amount: effective_loki,
    }
}

/// Need to make sure that stake transactions are processed before
/// Pool change transactions
// NOTE: the reference to `pools` doesn't really need to be mutable,
// but for now we need to make sure that liquidity is not `None`
pub(crate) fn adjust_portions_after_stake(
    portions: &mut VaultPortions,
    pools: &HashMap<PoolCoin, Liquidity>,
    contribution: &StakeContribution,
) {
    // For each staker compute their current ownership in atomic
    // amounts (before taking the new stake into account):

    let coin = contribution.coin();

    // TODO: make this work with other coins
    assert_eq!(coin, PoolCoin::ETH);

    let mut pool_portions = portions.entry(coin).or_insert(Default::default());

    let zero = Liquidity::zero();
    let liquidity = pools.get(&coin).unwrap_or(&zero);

    adjust_portions_after_stake_for_coin(&mut pool_portions, &liquidity, &contribution);
}

#[cfg(test)]
mod tests {

    use crate::utils::test_utils;

    use super::*;

    #[test]
    fn check_amount_from_portion() {
        let total_amount = LokiAmount::from_decimal_string("1000.0").to_atomic();

        assert_eq!(
            amount_from_portion(Portion::MAX, total_amount),
            total_amount
        );

        let portion = Portion(Portion::MAX.0 / 4);
        assert_eq!(amount_from_portion(portion, total_amount), total_amount / 4);

        assert_eq!(amount_from_portion(Portion(0), total_amount), 0);

        assert_eq!(amount_from_portion(Portion::MAX, u128::MAX), u128::MAX);
    }

    struct TestRunner {
        portions: PoolPortions,
        liquidity: Liquidity,
    }

    impl TestRunner {
        fn new() -> Self {
            let portions = PoolPortions::new();
            let liquidity = Liquidity::zero();

            TestRunner {
                portions,
                liquidity,
            }
        }

        fn add_stake(&mut self, staker_id: &StakerId, amount: LokiAmount) {
            // For convinience, eth amount is computed from loki amount:
            let factor = 1000;
            let other_amount =
                GenericCoinAmount::from_atomic(Coin::ETH, amount.to_atomic() * factor);

            self.add_asymmetric_stake(staker_id, amount, other_amount);
        }

        fn add_asymmetric_stake(
            &mut self,
            staker_id: &StakerId,
            loki_amount: LokiAmount,
            other_amount: GenericCoinAmount,
        ) {
            let stake = StakeContribution::new(staker_id.clone(), loki_amount, other_amount);

            adjust_portions_after_stake_for_coin(&mut self.portions, &self.liquidity, &stake);

            self.liquidity = Liquidity {
                depth: self.liquidity.depth + stake.other_amount.to_atomic(),
                loki_depth: self.liquidity.loki_depth + stake.loki_amount.to_atomic(),
            };
        }

        fn scale_liquidity(&mut self, factor: u32) {
            self.liquidity.loki_depth = self.liquidity.loki_depth * factor as u128;
            self.liquidity.depth = self.liquidity.depth * factor as u128;
        }
    }

    const HALF_PORTION: Portion = Portion(Portion::MAX.0 / 2);
    const QUATER_PORTION: Portion = Portion(Portion::MAX.0 / 4);
    const THREE_QUATERS_PORTION: Portion = Portion(3 * Portion::MAX.0 / 4);

    #[test]
    fn basic_portion_adjustment() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let alice = StakerId("Alice".to_owned());
        let bob = StakerId("Bob".to_owned());

        let amount1 = LokiAmount::from_decimal_string("100.0");

        // 1. First contribution from Alice

        runner.add_stake(&alice, amount1);

        assert_eq!(runner.portions.len(), 1);
        assert_eq!(runner.portions.get(&alice), Some(&Portion::MAX));

        // 2. Second equal contribution from Bob

        runner.add_stake(&bob, amount1);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&HALF_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&HALF_PORTION));

        // 3. Another contribution from Alice

        let amount2 = LokiAmount::from_decimal_string("200.0");

        runner.add_stake(&alice, amount2);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&THREE_QUATERS_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&QUATER_PORTION));
    }

    #[test]
    fn portion_adjustment_with_pool_changes() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let alice = StakerId("Alice".to_owned());
        let bob = StakerId("Bob".to_owned());

        let amount1 = LokiAmount::from_decimal_string("100.0");

        // 1. First contribution from Alice

        runner.add_stake(&alice, amount1);

        runner.scale_liquidity(3);

        // 2. Bob contributes after the liquidity changed

        runner.add_stake(&bob, amount1);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&THREE_QUATERS_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&QUATER_PORTION));
    }

    #[test]
    fn test_asymmetric_stake() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let alice = StakerId("Alice".to_owned());
        let bob = StakerId("Bob".to_owned());

        let amount = LokiAmount::from_decimal_string("100.0");

        let eth = GenericCoinAmount::from_atomic(Coin::ETH, 0);

        runner.add_stake(&alice, amount);
        runner.add_asymmetric_stake(&bob, amount, eth);

        assert_eq!(runner.portions.len(), 2);

        let a = runner.portions.get(&alice).unwrap().0;
        let b = runner.portions.get(&bob).unwrap().0;

        // Not only Bob contributes to only one side of the pool, but
        // he is also forced to autoswap (with low liquidity), resulting
        // in somewhat small portions:

        assert!(a > b * 2);
        assert!(a < b * 5);
    }
}
