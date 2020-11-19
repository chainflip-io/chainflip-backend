use std::{collections::HashMap, convert::TryInto};

use rand::{prelude::StdRng, seq::SliceRandom, SeedableRng};

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
    if amount == 0 && total_amount == 0 {
        return Portion(0);
    }

    let amount: U256 = amount.into();
    let total_amount: U256 = total_amount.into();
    let max: U256 = Portion::MAX.0.into();

    let res = amount.checked_mul(max).expect("mul");
    let res = res.checked_div(total_amount).expect("div");

    let res = res.try_into().expect("overflow");

    Portion(res)
}

/// Calculate current ownership in atomic amounts
pub(crate) fn aggregate_current_portions(
    portions: &PoolPortions,
    liquidity: Liquidity,
    pool_coin: PoolCoin,
) -> Vec<StakerOwnership> {
    portions
        .iter()
        .map(|(staker_id, portion)| {
            let loki = amount_from_portion(*portion, liquidity.loki_depth);
            let other = amount_from_portion(*portion, liquidity.depth);

            StakerOwnership {
                staker_id: staker_id.clone(),
                pool_type: pool_coin,
                loki: LokiAmount::from_atomic(loki),
                other: GenericCoinAmount::from_atomic(pool_coin.into(), other),
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

/// How much (`fraction`) `staker_id` withdrew from `pool`
pub struct UnstakeWithdrawal {
    pub staker_id: StakerId,
    pub fraction: UnstakeFraction,
    pub pool: PoolCoin,
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

/// Calculate fraction of a portion of the total amount
fn amount_from_fraction_and_portion(
    fraction: UnstakeFraction,
    portion: Portion,
    total: u128,
) -> u128 {
    let loki_owned = amount_from_portion(portion, total);

    let loki_owned: U256 = loki_owned.into();
    let fraction: U256 = fraction.0.into();
    let max_fraction: U256 = UnstakeFraction::MAX.0.into();

    let loki: U256 = loki_owned * fraction / max_fraction;

    let loki: u128 = loki.try_into().expect("Unexpected overflow");
    loki
}

/// Adjust portions taking `unstake` into account
fn adjust_portions_after_unstake_for_coin(
    portions: &mut PoolPortions,
    liquidity: &Liquidity,
    unstake: UnstakeWithdrawal,
) {
    let pool = unstake.pool;

    let fraction = UnstakeFraction::MAX;

    let portion = *portions
        .get(&unstake.staker_id)
        .expect("Staker id must exist");

    let loki = amount_from_fraction_and_portion(fraction, portion, liquidity.loki_depth);

    let staker_amounts = aggregate_current_portions(&portions, *liquidity, pool);

    // Check how much we can unstake:
    let (mut unstaker_entries, mut other_entries): (Vec<StakerOwnership>, Vec<StakerOwnership>) =
        staker_amounts
            .into_iter()
            .partition(|entry| entry.staker_id == unstake.staker_id);

    assert_eq!(
        unstaker_entries.len(),
        1,
        "There must be exactly one entry for unstaker id"
    );

    let mut unstaker_entry = unstaker_entries.pop().unwrap();

    let loki_withdrawn = unstaker_entry.loki.to_atomic().min(loki);
    unstaker_entry.loki = unstaker_entry
        .loki
        .checked_sub(&LokiAmount::from_atomic(loki_withdrawn))
        .expect("underflow");

    let new_total_loki = liquidity.loki_depth.saturating_sub(loki_withdrawn);

    // Adjust everyone's portions according to the new total amount

    // Note: we need to keep an invariant that all portions add up to Portion::MAX,
    // so we assign all remaining portions to the last in a shuffled list (the unstaker
    // does not participate in this, because if they unstake all, we don't want to keep
    // a trivial amount under their entry)

    let mut rng = StdRng::seed_from_u64(0);
    other_entries.shuffle(&mut rng);

    other_entries.push(unstaker_entry);

    let all_entries = other_entries;

    let mut dust_left_from_portions = Portion::MAX;

    for entry in &all_entries {
        let portion = portions
            .get_mut(&entry.staker_id)
            .expect("staker entry must exist");

        let owned = entry.loki.to_atomic();

        let p = portion_from_amount(owned, new_total_loki);
        *portion = p;

        dust_left_from_portions = dust_left_from_portions.checked_sub(p).expect("underflow");
    }

    if all_entries.len() > 1 {
        if let Some(entry) = all_entries.get(0) {
            let portion = portions
                .get_mut(&entry.staker_id)
                .expect("staker entry must exist");

            *portion = portion
                .checked_add(dust_left_from_portions)
                .expect("overflow");
        }
    }

    // Remove all 0 portions entries
    for entry in &all_entries {
        let portion = portions
            .get_mut(&entry.staker_id)
            .expect("staker entry must exist");

        if portion.0 == 0 {
            portions
                .remove(&entry.staker_id)
                .expect("staker entry must exist");
        }
    }
}

fn adjust_portions_after_stake_for_coin(
    portions: &mut PoolPortions,
    liquidity: &Liquidity,
    contribution: &StakeContribution,
) {
    let pool_coin = contribution.coin();
    let staker_amounts = aggregate_current_portions(&portions, *liquidity, pool_coin);

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
        let (effective_loki, other) =
            utils::autoswap::calc_autoswap_amount(loki_amount, other_amount, *liquidity)
                .expect("incorrect autoswap usage");
        info!(
            "Autoswapped from ({:?}, {:?}) to ({:?}, {:?})",
            loki_amount, other_amount, effective_loki, other
        );
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

    let mut pool_portions = portions.entry(coin).or_insert(Default::default());

    let zero = Liquidity::zero();
    let liquidity = pools.get(&coin).unwrap_or(&zero);

    adjust_portions_after_stake_for_coin(&mut pool_portions, &liquidity, &contribution);
}

/// Adjust portions taking `unstake` into account
pub(crate) fn adjust_portions_after_unstake(
    portions: &mut VaultPortions,
    liquidity: &Liquidity,
    unstake: UnstakeWithdrawal,
) {
    let coin = &unstake.pool;

    let mut pool_portions = portions.entry(*coin).or_insert(Default::default());

    adjust_portions_after_unstake_for_coin(&mut pool_portions, liquidity, unstake);
}

#[cfg(test)]
mod tests {

    use crate::{transactions::signatures::get_random_staker, utils::test_utils};

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

        fn add_stake_eth(&mut self, staker_id: &StakerId, amount: LokiAmount) {
            // For convinience, eth amount is computed from loki amount:
            let factor = 1000;
            let other_amount =
                GenericCoinAmount::from_atomic(Coin::ETH, amount.to_atomic() * factor);

            self.add_asymmetric_stake(staker_id, amount, other_amount);
        }

        fn add_stake_btc(&mut self, staker_id: &StakerId, amount: LokiAmount) {
            let factor = 1000;
            let other_amount =
                GenericCoinAmount::from_atomic(Coin::BTC, amount.to_atomic() * factor);

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

        /// Unstake, other amount is derived from loki_amount
        fn unstake_symmetric(
            &mut self,
            staker_id: &StakerId,
            fraction: UnstakeFraction,
            pool: PoolCoin,
        ) {
            let portion = *self
                .portions
                .get(staker_id)
                .expect("Staker id is expected to have portions");
            let loki_amount =
                amount_from_fraction_and_portion(fraction, portion, self.liquidity.loki_depth);

            let other_amount = loki_amount * self.liquidity.depth / self.liquidity.loki_depth;

            let unstake = UnstakeWithdrawal {
                staker_id: staker_id.to_owned(),
                fraction,
                pool,
            };

            adjust_portions_after_unstake_for_coin(&mut self.portions, &self.liquidity, unstake);

            self.liquidity = Liquidity {
                depth: self.liquidity.depth + other_amount,
                loki_depth: self
                    .liquidity
                    .loki_depth
                    .checked_sub(loki_amount)
                    .expect("underflow"),
            }
        }

        fn scale_liquidity(&mut self, factor: u32) {
            self.liquidity.loki_depth = self.liquidity.loki_depth * factor as u128;
            self.liquidity.depth = self.liquidity.depth * factor as u128;
        }
    }

    const HALF_PORTION: Portion = Portion(Portion::MAX.0 / 2);
    const ONE_THIRD_PORTION: Portion = Portion(Portion::MAX.0 / 3);
    const QUATER_PORTION: Portion = Portion(Portion::MAX.0 / 4);
    const THREE_QUATERS_PORTION: Portion = Portion(3 * Portion::MAX.0 / 4);

    #[test]
    fn basic_portion_adjustment() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let alice = get_random_staker().id();
        let bob = get_random_staker().id();

        let amount1 = LokiAmount::from_decimal_string("100.0");

        // 1. First contribution from Alice

        runner.add_stake_eth(&alice, amount1);

        assert_eq!(runner.portions.len(), 1);
        assert_eq!(runner.portions.get(&alice), Some(&Portion::MAX));

        // 2. Second equal contribution from Bob

        runner.add_stake_eth(&bob, amount1);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&HALF_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&HALF_PORTION));

        // 3. Another contribution from Alice

        let amount2 = LokiAmount::from_decimal_string("200.0");

        runner.add_stake_eth(&alice, amount2);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&THREE_QUATERS_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&QUATER_PORTION));
    }

    #[test]
    fn portion_adjustment_with_pool_changes() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let alice = get_random_staker().id();
        let bob = get_random_staker().id();

        let amount1 = LokiAmount::from_decimal_string("100.0");

        // 1. First contribution from Alice

        runner.add_stake_eth(&alice, amount1);

        runner.scale_liquidity(3);

        // 2. Bob contributes after the liquidity changed

        runner.add_stake_eth(&bob, amount1);

        assert_eq!(runner.portions.len(), 2);
        assert_eq!(runner.portions.get(&alice), Some(&THREE_QUATERS_PORTION));
        assert_eq!(runner.portions.get(&bob), Some(&QUATER_PORTION));
    }

    #[test]
    fn test_asymmetric_stake_eth() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let alice = get_random_staker().id();
        let bob = get_random_staker().id();

        let amount = LokiAmount::from_decimal_string("100.0");

        let eth = GenericCoinAmount::from_atomic(Coin::ETH, 0);

        runner.add_stake_eth(&alice, amount);
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

    #[test]
    fn test_asymmetric_stake_btc() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let alice = get_random_staker().id();
        let bob = get_random_staker().id();

        let amount = LokiAmount::from_decimal_string("100.0");

        let btc = GenericCoinAmount::from_atomic(Coin::BTC, 0);

        runner.add_stake_btc(&alice, amount);
        runner.add_asymmetric_stake(&bob, amount, btc);

        assert_eq!(runner.portions.len(), 2);

        let a = runner.portions.get(&alice).unwrap().0;
        let b = runner.portions.get(&bob).unwrap().0;

        // Not only Bob contributes to only one side of the pool, but
        // he is also forced to autoswap (with low liquidity), resulting
        // in somewhat small portions:

        assert!(a > b * 2);
        assert!(a < b * 5);
    }

    #[test]
    fn stake_unstake_all() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        // 1. Alice stakes as a sole staker

        let alice = get_random_staker().id();

        let amount = LokiAmount::from_decimal_string("100.0");

        runner.add_stake_eth(&alice, amount);

        let a = runner.portions.get(&alice).unwrap().0;

        assert_eq!(a, Portion::MAX.0);

        // 2. Alice unstakes everything

        runner.unstake_symmetric(&alice, UnstakeFraction::MAX, PoolCoin::ETH);

        assert!(runner.portions.get(&alice).is_none());
    }

    #[test]
    fn two_stakers_one_unstakes_all() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        // 1. Alice and Bob stake equal amounts

        let alice = get_random_staker().id();
        let bob = get_random_staker().id();

        let amount = LokiAmount::from_decimal_string("100.0");

        runner.add_stake_eth(&alice, amount);
        runner.add_stake_eth(&bob, amount);

        let a = runner.portions.get(&alice).unwrap();
        let b = runner.portions.get(&alice).unwrap();

        assert_eq!(*a, HALF_PORTION);
        assert_eq!(*b, HALF_PORTION);

        // 2. Alice unstakes everything

        runner.unstake_symmetric(&alice, UnstakeFraction::MAX, PoolCoin::ETH);

        assert!(runner.portions.get(&alice).is_none());
        let b = runner.portions.get(&bob).unwrap();
        assert_eq!(*b, Portion::MAX);
    }

    #[test]
    fn three_stakers_two_unstake() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        // 1. Alice, Bob, and Charlie stake equal amounts

        let alice = get_random_staker().id();
        let bob = get_random_staker().id();
        let charlie = get_random_staker().id();

        let amount = LokiAmount::from_decimal_string("100.0");

        runner.add_stake_eth(&alice, amount);
        runner.add_stake_eth(&bob, amount);
        runner.add_stake_eth(&charlie, amount);

        let a = *runner.portions.get(&alice).unwrap();
        let b = *runner.portions.get(&alice).unwrap();
        let c = *runner.portions.get(&alice).unwrap();

        assert_eq!(a, ONE_THIRD_PORTION);
        assert_eq!(b, ONE_THIRD_PORTION);
        assert_eq!(c, ONE_THIRD_PORTION);

        // 2. Alice and Bob unstakes everything

        runner.unstake_symmetric(&alice, UnstakeFraction::MAX, PoolCoin::ETH);
        runner.unstake_symmetric(&bob, UnstakeFraction::MAX, PoolCoin::ETH);

        assert!(runner.portions.get(&alice).is_none());
        assert!(runner.portions.get(&bob).is_none());
        let c = runner.portions.get(&charlie).unwrap();
        assert_eq!(*c, Portion::MAX);
    }

    #[test]
    fn staking_marginal_extra_btc() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        // 1. No autoswap on the first stake: it creates initial liquidity
        let loki = LokiAmount::from_decimal_string("500.0");
        let btc = GenericCoinAmount::from_decimal_string(Coin::BTC, "0.02");

        let alice = get_random_staker().id();

        runner.add_asymmetric_stake(&alice, loki, btc);

        // 2. Bob contributes half of what Alice contributed in Loki, but
        // a larger amount of BTC, which should result in autoswap (and a
        // higher that 33.3% portion for Bob)

        let bob = get_random_staker().id();

        let loki = LokiAmount::from_decimal_string("250.0");
        let btc = GenericCoinAmount::from_decimal_string(Coin::BTC, "0.028");

        runner.add_asymmetric_stake(&bob, loki, btc);

        let a = runner
            .portions
            .get(&alice)
            .expect("Alice should have portions");
        let b = runner.portions.get(&bob).expect("Bob should have portions");

        assert_eq!(a.0, 5952304034);
        assert_eq!(b.0, 4047695966);
    }

    #[test]
    fn test_amount_from_fraction() {
        let portion = Portion::MAX;

        assert_eq!(
            amount_from_fraction_and_portion(UnstakeFraction::MAX, portion, 1000),
            1000
        );

        let half_fraction = UnstakeFraction::new(UnstakeFraction::MAX.0 / 2).unwrap();
        assert_eq!(
            amount_from_fraction_and_portion(half_fraction, HALF_PORTION, 1000),
            250
        );
    }
}
