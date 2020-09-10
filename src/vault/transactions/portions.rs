use std::{collections::HashMap, convert::TryInto};

use crate::{
    common::{coins::GenericCoinAmount, coins::PoolCoin, Coin, LokiAmount},
    transactions::PoolChangeTx,
    utils::primitives::U256,
};

use super::{
    memory_provider::StakerId,
    memory_provider::VaultPortions,
    memory_provider::{PoolPortions, Portion, StakerOwnership},
    Liquidity,
};

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
pub fn aggregate_current_portions(
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
pub struct StakeContribution {
    staker_id: StakerId,
    pool_change_tx: PoolChangeTx,
}

impl StakeContribution {
    /// Into which pool the stake is made
    pub fn coin(&self) -> PoolCoin {
        self.pool_change_tx.coin
    }
}

fn adjust_portions_after_stake_for_coin(
    portions: &mut PoolPortions,
    liquidity: &Liquidity,
    tx: &StakeContribution,
) {
    let staker_amounts = aggregate_current_portions(&portions, *liquidity);

    // Adjust portions for the existing staker ids

    let extra_loki: u128 = tx
        .pool_change_tx
        .loki_depth_change
        .try_into()
        .expect("negative amount staked");

    let new_total_loki = liquidity.loki_depth + extra_loki;

    let mut portions_sum = Portion(0);

    for entry in staker_amounts {
        let portion = portions
            .get_mut(&entry.staker_id)
            .expect("staker entry should exist");

        // Stakes are always symmetric (after auto-swapping any assymetric stake),
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

    let portion = portions.entry(tx.staker_id.clone()).or_insert(Portion(0));

    *portion = portion
        .checked_add(portion_left)
        .expect("portions overflow");
}

/// Need to make sure that stake transactions are processed before
/// Pool change transactions
// NOTE: the reference to `pools` doesn't really need to be mutable,
// but for now we need to make sure that liquidity is not `None`
pub fn adjust_portions_after_stake(
    portions: &mut VaultPortions,
    pools: &mut HashMap<PoolCoin, Liquidity>,
    tx: &StakeContribution,
) {
    // For each staker compute their current ownership in atomic
    // amounts (before taking the new stake into account):

    // TODO: make this work with other coins
    assert_eq!(tx.coin(), PoolCoin::ETH);

    let mut pool_portions = portions.entry(tx.coin()).or_insert(Default::default());

    let liquidity = pools.entry(tx.coin()).or_insert(Liquidity::new());

    adjust_portions_after_stake_for_coin(&mut pool_portions, &liquidity, &tx);
}

#[cfg(test)]
mod tests {

    use crate::transactions::PoolChangeTx;

    use super::*;

    #[test]
    fn check_amount_from_portion() {
        let total_amount = LokiAmount::from_decimal(1000.0).to_atomic();

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
            let liquidity = Liquidity::new();

            TestRunner {
                portions,
                liquidity,
            }
        }

        fn add_stake(&mut self, staker_id: &StakerId, amount: LokiAmount) {
            // In this test all stakes are "symmetrical"
            let factor = 1000;

            let stake = StakeContribution {
                staker_id: staker_id.clone(),
                pool_change_tx: PoolChangeTx::new(
                    PoolCoin::ETH,
                    amount.to_atomic() as i128,
                    amount.to_atomic() as i128 * factor,
                ),
            };

            adjust_portions_after_stake_for_coin(&mut self.portions, &self.liquidity, &stake);

            self.liquidity = Liquidity {
                depth: self.liquidity.depth + stake.pool_change_tx.depth_change as u128,
                loki_depth: self.liquidity.loki_depth
                    + stake.pool_change_tx.loki_depth_change as u128,
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
        env_logger::builder()
            .format_timestamp(None)
            .format_module_path(false)
            .init();

        let mut runner = TestRunner::new();

        let alice = "Alice".to_owned();
        let bob = "Bob".to_owned();

        let amount1 = LokiAmount::from_decimal(100.0);

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

        let amount2 = LokiAmount::from_decimal(200.0);

        runner.add_stake(&alice, amount2);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&THREE_QUATERS_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&QUATER_PORTION));
    }

    #[test]
    fn portion_adjustment_with_pool_changes() {
        env_logger::builder()
            .format_timestamp(None)
            .format_module_path(false)
            .init();

        let mut runner = TestRunner::new();

        let alice = "Alice".to_owned();
        let bob = "Bob".to_owned();

        let amount1 = LokiAmount::from_decimal(100.0);

        // 1. First contribution from Alice

        runner.add_stake(&alice, amount1);

        runner.scale_liquidity(3);

        // 2. Bob contributes after the liquidity changed

        runner.add_stake(&bob, amount1);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&THREE_QUATERS_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&QUATER_PORTION));
    }
}
