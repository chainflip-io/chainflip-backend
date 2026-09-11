// Copyright 2026 Chainflip Labs GmbH
// SPDX-License-Identifier: Apache-2.0

use super::*;
use sp_runtime::traits::Hash;
use sp_std::collections::btree_map::BTreeMap;

// Keep the numeric configuration IDs in sync with benchmark-swaps.py.
pub(super) struct Fixture<T: Config> {
	pub from: Asset,
	pub to: Asset,
	pub input: AssetAmount,
	pair: AssetPair,
	side: Side,
	n: u32,
	configuration: u32,
	liquidity_before: Amount,
	pool_hash: T::Hash,
}

impl<T: Config> Fixture<T> {
	pub fn new(n: u32, configuration: u32, asset: u32, dollars: u32) -> Self {
		// CLI range overrides permit larger stress runs than the default benchmark range.
		assert!((1..=100_000).contains(&n));
		assert!(configuration <= 10 && asset <= 3 && (1..=1_000_000).contains(&dollars));
		frame_system::Pallet::<T>::set_block_number(1u32.into());
		let (base, tick) = if asset < 2 { (Asset::Btc, 69_082) } else { (Asset::Eth, -200_000) };
		let side = if asset % 2 == 0 { Side::Sell } else { Side::Buy };
		let (from, to, tick_step) = match side {
			Side::Sell => (STABLE_ASSET, base, 1),
			Side::Buy => (base, STABLE_ASSET, -1),
		};
		let pair = AssetPair::new(base, STABLE_ASSET).unwrap();
		let accounts = T::AccountRoleRegistry::generate_whitelisted_callers_with_role(
			AccountRole::LiquidityProvider,
			n + 1,
		)
		.unwrap();
		let mut pool = Pool::<T> {
			range_orders_cache: Default::default(),
			limit_orders_cache: Default::default(),
			pool_state: PoolState::new(0, Price::from_tick(tick).unwrap()).unwrap(),
		};
		let quote_amount = AssetAmount::from(dollars) * 1_000_000;
		let amount_at = |side, tick| match side {
			Side::Sell =>
				Price::from(SqrtPrice::from_tick(tick)).input_amount_ceil(quote_amount).unwrap(),
			Side::Buy => Amount::from(quote_amount),
		};
		let input_for = |amount, tick| {
			let price = Price::from(SqrtPrice::from_tick(tick));
			match side {
				Side::Sell => price.output_amount_ceil(amount).unwrap(),
				Side::Buy => price.input_amount_ceil(amount).unwrap(),
			}
		};
		let mut depth = BTreeMap::<Tick, Amount>::new();
		// Construct once in memory: repeated extrinsics would make setup quadratic in book size.
		for i in 0..n {
			let order_tick = tick +
				tick_step *
					match configuration {
						3 | 7 => (i / 12) as Tick,
						4 => 1 + i as Tick,
						9 => i as Tick,
						_ => 0,
					};
			let order_side = if configuration == 5 { !side } else { side };
			let lp = &accounts[if configuration == 1 { 0 } else { i as usize }];
			let amount = amount_at(order_side, order_tick);
			pool.pool_state
				.mint_limit_order(&(lp.clone(), i as u64), order_side, order_tick, amount)
				.unwrap();
			Pallet::<T>::update_limit_orders_cache(
				&mut pool, lp, order_side, i as u64, order_tick, amount,
			);
			if order_side == side {
				*depth.entry(order_tick).or_default() += amount;
			}
		}
		// A remaining quote is required by swap_single_leg's post-swap price check. For the
		// untouched-book cases this extra order is instead the only executable liquidity.
		let guard_tick = if matches!(configuration, 4 | 5) {
			tick
		} else {
			tick + tick_step *
				(1_000 +
					match configuration {
						3 | 7 => n.div_ceil(12) as Tick,
						9 => n as Tick,
						_ => 0,
					})
		};
		let guard_amount = amount_at(side, guard_tick);
		let guard_lp = &accounts[n as usize];
		pool.pool_state
			.mint_limit_order(&(guard_lp.clone(), n as u64), side, guard_tick, guard_amount)
			.unwrap();
		Pallet::<T>::update_limit_orders_cache(
			&mut pool,
			guard_lp,
			side,
			n as u64,
			guard_tick,
			guard_amount,
		);

		let input = match configuration {
			2 | 3 | 7 | 9 => {
				let full_input = depth
					.iter()
					.fold(Amount::zero(), |sum, (&tick, &amount)| sum + input_for(amount, tick));
				if configuration == 7 {
					full_input + input_for(guard_amount / 3, guard_tick)
				} else {
					full_input
				}
			},
			4 | 5 => input_for(guard_amount / 3, guard_tick),
			6 => Amount::one(),
			10 => input_for(amount_at(side, tick) / 2, tick),
			_ => input_for(depth[&tick] / 3, tick),
		};
		if configuration == 8 {
			for lp in &accounts {
				T::LpBalance::credit_account(lp, from, 1);
				T::LpStats::on_limit_order_filled(lp, &from, 1);
			}
		}
		MaximumPriceImpact::<T>::remove(pair);
		if configuration == 7 {
			MaximumPriceImpact::<T>::insert(pair, 0);
		}
		let liquidity_before = pool
			.pool_state
			.limit_orders(side)
			.fold(Amount::zero(), |sum, (_, _, position)| sum + position.amount);
		let pool_hash = T::Hashing::hash_of(&pool);
		Pools::<T>::insert(pair, pool);
		frame_system::Pallet::<T>::reset_events();
		Self {
			from,
			to,
			input: input.try_into().unwrap(),
			pair,
			side,
			n,
			configuration,
			liquidity_before,
			pool_hash,
		}
	}

	pub fn verify(self, result: Result<AssetAmount, DispatchError>) {
		let pool = Pools::<T>::get(self.pair).unwrap();
		if self.configuration == 7 {
			assert_eq!(result, Err(Error::<T>::PriceImpactLimitExceeded.into()));
			assert_eq!(T::Hashing::hash_of(&pool), self.pool_hash);
			assert!(frame_system::Pallet::<T>::events().is_empty());
			return;
		}
		let output = result.unwrap();
		assert_eq!(
			pool.pool_state.limit_orders(!self.side).count(),
			if self.configuration == 5 { self.n as usize } else { 0 }
		);
		if self.configuration != 6 {
			assert!(output > 0);
		}
		let orders: Vec<_> = pool.pool_state.limit_orders(self.side).collect();
		let remaining = orders
			.iter()
			.fold(Amount::zero(), |sum, (_, _, position)| sum + position.amount);
		assert_eq!(self.liquidity_before - remaining, Amount::from(output));
		let before_count = if self.configuration == 5 { 1 } else { self.n + 1 };
		let changed = orders
			.iter()
			.filter(|(_, _, position)| position.amount != position.original_amount)
			.count();
		let filled = before_count as usize - orders.len() + changed;
		match self.configuration {
			0 | 1 | 8 => assert_eq!(filled, self.n as usize),
			2 | 3 | 9 => {
				assert_eq!(filled, self.n as usize);
				assert_eq!(orders.len(), 1);
			},
			4 | 5 => assert_eq!(filled, 1),
			6 | 10 => assert_eq!(filled == 0, output == 0),
			_ => unreachable!(),
		}
		let cached = pool.limit_orders_cache[self.side.to_sold_pair()]
			.values()
			.map(|orders| orders.len())
			.sum::<usize>();
		assert_eq!(cached, orders.len());
		frame_system::Pallet::<T>::assert_last_event(
			Event::<T>::AssetSwapped {
				from: self.from,
				to: self.to,
				input_amount: self.input,
				output_amount: output,
			}
			.into(),
		);
	}
}
