// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use cf_traits::{Hook, HookType, Validate};
use core::ops::RangeInclusive;
use enum_iterator::{all, Sequence};
use itertools::Either;
use sp_runtime::traits::AtLeast32BitUnsigned;

use crate::electoral_systems::oracle_price::primitives::{Seconds, UnixTime};

use crate::{
	electoral_systems::{
		block_witnesser::{primitives::SafeModeStatus, state_machine::HookTypeFor},
		oracle_price::{
			price::PriceUnit,
			primitives::{Aggregated, BasisPoints},
		},
		state_machine::{
			common_imports::*,
			state_machine::{AbstractApi, Statemachine},
		},
	},
	generic_tools::*,
};
use sp_std::{
	ops::{Index, IndexMut},
	vec,
};

#[cfg(test)]
use proptest_derive::Arbitrary;

//--------------- configuration trait -----------------

pub trait OPTypes: 'static + Sized + CommonTraits {
	type StateChainBlockNumber: CommonTraits + Default + MaybeArbitrary + AtLeast32BitUnsigned;

	type Price: PriceTrait + CommonTraits + Ord + Default + MaybeArbitrary;

	type AssetPair: AssetPairTrait + CommonTraits + Ord + Sequence + MaybeArbitrary;

	type GetTime: Hook<HookTypeFor<Self, GetTimeHook>> + CommonTraits;

	type GetStateChainBlockHeight: Hook<HookTypeFor<Self, GetStateChainBlockHeight>> + CommonTraits;

	type SafeModeEnabledHook: Hook<HookTypeFor<Self, SafeModeEnabledHook>> + CommonTraits;

	type EmitPricesUpdatedEventHook: Hook<HookTypeFor<Self, EmitPricesUpdatedEvent>> + CommonTraits;
}

pub trait AssetPairTrait {
	fn to_price_unit(&self) -> PriceUnit;
}

pub trait PriceTrait: Sized {
	fn to_price_range(&self, range: BasisPoints) -> Option<RangeInclusive<Self>>;
}

pub struct GetTimeHook;
impl<T: OPTypes> HookType for HookTypeFor<T, GetTimeHook> {
	type Input = ();
	type Output = UnixTime;
}

pub struct GetStateChainBlockHeight;
impl<T: OPTypes> HookType for HookTypeFor<T, GetStateChainBlockHeight> {
	type Input = ();
	type Output = T::StateChainBlockNumber;
}

pub struct SafeModeEnabledHook;
impl<T: OPTypes> HookType for HookTypeFor<T, SafeModeEnabledHook> {
	type Input = ();
	type Output = SafeModeStatus;
}

pub struct EmitPricesUpdatedEvent;
impl<T: OPTypes> HookType for HookTypeFor<T, EmitPricesUpdatedEvent> {
	type Input = Vec<(T::AssetPair, UnixTime, T::Price)>;
	type Output = ();
}

//---------------- the primitives ------------------

derive_common_traits! {
	#[derive(Copy, Sequence, PartialOrd, Ord, TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum ExternalPriceChain {
		Arbitrum,
		Ethereum
	}
}

derive_common_traits! {
	#[derive(TypeInfo, Copy, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum PriceStatus {
		UpToDate,
		MaybeStale,
		#[default]
		Stale
	}
}

derive_common_traits! {
	#[derive_where(Default;)]
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct AssetState<T: OPTypes> {
		pub timestamp: Aggregated<UnixTime>,
		pub price: Aggregated<T::Price>,
		pub price_status: PriceStatus,
		pub updated_at_statechain_block: T::StateChainBlockNumber,
		pub minimal_price_deviation: BasisPoints
	}
}

derive_common_traits! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct AssetResponse<T: OPTypes> {
		pub timestamp: Aggregated<UnixTime>,
		pub price: Aggregated<T::Price>,
	}
}

derive_common_traits! {
	#[derive_where(Default;)]
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainState<T: OPTypes> {
		pub price: BTreeMap<T::AssetPair, AssetState<T>>,
	}
}

derive_common_traits! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainStateVote<T: OPTypes> {
		pub price: BTreeMap<T::AssetPair, (UnixTime, T::Price)>,
	}
}

// This is defined here for the context, but it's used by the engines
// to decide whether they should vote with a given oracle result.
pub fn should_vote_for_asset<T: OPTypes>(
	(time, price): &(UnixTime, T::Price),
	conditions: &[VotingCondition<T>],
) -> bool {
	use VotingCondition::*;
	conditions.iter().all(|condition| match condition {
		PriceMoved { last_price, deviation } =>
		// Note, the `to_price_range` conversion might fail in extreme numeric situations,
		// in that case we treat this conditions as true
			last_price
				.to_price_range(*deviation)
				.map(|ignored_range| !ignored_range.contains(price))
				.unwrap_or(true),
		NewTimestamp { last_timestamp } => time > last_timestamp,
	})
}

impl<T: OPTypes> ExternalChainState<T> {
	pub fn update_price_state(
		&mut self,
		current_time: &UnixTime,
		settings: &ExternalChainSettings<T>,
	) {
		use PriceStatus::*;
		self.price.iter_mut().for_each(|(asset, asset_state)| {
			// update price deviation setting
			asset_state.minimal_price_deviation = settings.minimal_price_deviation;

			// update price status
			let up_to_date_until =
				asset_state.timestamp.median + settings.get_up_to_date_timeout(asset);
			let maybe_stale_until = up_to_date_until + settings.get_maybe_stale_timeout(asset);

			asset_state.price_status = if *current_time <= up_to_date_until {
				UpToDate
			} else if *current_time <= maybe_stale_until {
				MaybeStale
			} else {
				Stale
			};
		});
	}

	pub fn get_query(&self) -> BTreeMap<T::AssetPair, Vec<VotingCondition<T>>> {
		use PriceStatus::*;

		all::<T::AssetPair>()
			.map(|asset| {
				(
					asset.clone(),
					self.price
						.get(&asset)
						.map(|asset_state| match asset_state.price_status {
							UpToDate => vec![
								VotingCondition::NewTimestamp {
									last_timestamp: asset_state.timestamp.median,
								},
								VotingCondition::PriceMoved {
									last_price: asset_state.price.median.clone(),
									deviation: asset_state.minimal_price_deviation,
								},
							],
							Stale => vec![VotingCondition::NewTimestamp {
								last_timestamp: asset_state.timestamp.median,
							}],
							MaybeStale => vec![],
						})
						.unwrap_or_default(),
				)
			})
			.collect()
	}
}

derive_common_traits! {
	#[derive(TypeInfo, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainStates<T: OPTypes> {
		pub arbitrum: ExternalChainState<T>,
		pub ethereum: ExternalChainState<T>,
	}
}

impl<T: OPTypes> ExternalChainStates<T> {
	pub fn get_latest_asset_state(&self, asset: T::AssetPair) -> Option<&AssetState<T>> {
		all::<ExternalPriceChain>()
			.filter_map(|chain| self[chain].price.get(&asset))
			.max_by_key(|price_state| price_state.timestamp.median)
	}
}

impl<T: OPTypes> Validate for ExternalChainStates<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<T: OPTypes> Index<ExternalPriceChain> for ExternalChainStates<T> {
	type Output = ExternalChainState<T>;

	fn index(&self, index: ExternalPriceChain) -> &Self::Output {
		match index {
			ExternalPriceChain::Arbitrum => &self.arbitrum,
			ExternalPriceChain::Ethereum => &self.ethereum,
		}
	}
}

impl<T: OPTypes> IndexMut<ExternalPriceChain> for ExternalChainStates<T> {
	fn index_mut(&mut self, index: ExternalPriceChain) -> &mut Self::Output {
		match index {
			ExternalPriceChain::Arbitrum => &mut self.arbitrum,
			ExternalPriceChain::Ethereum => &mut self.ethereum,
		}
	}
}

//---------------- the api ------------------

derive_common_traits! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum VotingCondition<T: OPTypes> {
		PriceMoved {
			last_price: T::Price,
			deviation: BasisPoints
		},
		NewTimestamp {
			last_timestamp: UnixTime
		},
	}
}

derive_common_traits! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct PriceQuery<T: OPTypes> {
		pub chain: ExternalPriceChain,
		pub assets: BTreeMap<T::AssetPair, Vec<VotingCondition<T>>>
	}
}

derive_common_traits! {
	#[derive(TypeInfo, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct OraclePriceSettings<T: OPTypes> {
		pub arbitrum: ExternalChainSettings<T>,
		pub ethereum: ExternalChainSettings<T>,
	}
}

impl<T: OPTypes> Index<ExternalPriceChain> for OraclePriceSettings<T> {
	type Output = ExternalChainSettings<T>;

	fn index(&self, index: ExternalPriceChain) -> &Self::Output {
		match index {
			ExternalPriceChain::Arbitrum => &self.arbitrum,
			ExternalPriceChain::Ethereum => &self.ethereum,
		}
	}
}

derive_common_traits! {
	#[derive(TypeInfo)]
	#[derive_where(Default;)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainSettings<T: OPTypes> {
		pub up_to_date_timeout: Seconds,
		pub maybe_stale_timeout: Seconds,
		pub minimal_price_deviation: BasisPoints,
		pub up_to_date_timeout_overrides: BTreeMap<T::AssetPair, Seconds>,
		pub maybe_stale_timeout_overrides: BTreeMap<T::AssetPair, Seconds>,
	}
}

impl<T: OPTypes> ExternalChainSettings<T> {
	pub fn get_up_to_date_timeout(&self, asset: &T::AssetPair) -> Seconds {
		*self.up_to_date_timeout_overrides.get(asset).unwrap_or(&self.up_to_date_timeout)
	}
	pub fn get_maybe_stale_timeout(&self, asset: &T::AssetPair) -> Seconds {
		*self
			.maybe_stale_timeout_overrides
			.get(asset)
			.unwrap_or(&self.maybe_stale_timeout)
	}
}

derive_common_traits! {
	#[derive(TypeInfo, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct OraclePriceTracker<T: OPTypes> {
		pub chain_states: ExternalChainStates<T>,
		pub get_time: T::GetTime,
		pub get_statechain_block_height: T::GetStateChainBlockHeight,
		pub safe_mode_enabled: T::SafeModeEnabledHook,
		pub emit_oracle_price_event: T::EmitPricesUpdatedEventHook,
	}
}

impl<T: OPTypes> Validate for OraclePriceTracker<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<T: OPTypes> AbstractApi for OraclePriceTracker<T> {
	type Query = PriceQuery<T>;
	type Response = BTreeMap<T::AssetPair, AssetResponse<T>>;
	type Error = ();

	fn validate(_query: &Self::Query, _response: &Self::Response) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<T: OPTypes> Statemachine for OraclePriceTracker<T> {
	type Context = ();
	type Settings = OraclePriceSettings<T>;
	type Output = Result<(), &'static str>;
	type State = OraclePriceTracker<T>;

	fn get_queries(state: &mut Self::State) -> Vec<Self::Query> {
		if state.safe_mode_enabled.run(()) == SafeModeStatus::Disabled {
			all::<ExternalPriceChain>()
				.filter(|chain| {
					match chain {
						ExternalPriceChain::Arbitrum => true,
						ExternalPriceChain::Ethereum => {
							// only query every second block for ethereum
							state.get_statechain_block_height.run(()) % 2u32.into() == 0u32.into()
						},
					}
				})
				.map(|chain| PriceQuery { chain, assets: state.chain_states[chain].get_query() })
				.collect()
		} else {
			vec![]
		}
	}

	fn step(
		state: &mut Self::State,
		input: crate::electoral_systems::state_machine::state_machine::InputOf<Self>,
		settings: &Self::Settings,
	) -> Self::Output {
		match input {
			Either::Left(()) => {},
			Either::Right((query, response)) => {
				let current_statechain_block = state.get_statechain_block_height.run(());

				// try to write new prices into the chain state (only if they are newer)
				// and emit `OraclePricesUpdated` for the ones that have been updated
				let mut updated_prices = Vec::new();
				for (asset, response) in &response {
					// if the response timestamp is newer than the previously best timestamp for
					// this asset, we will emit this price as part of the `OraclePricesUpdated`
					// event.
					let previous_best_timestamp = state
						.chain_states
						.get_latest_asset_state(asset.clone())
						.map(|state| state.timestamp.median)
						.unwrap_or_default();
					if response.timestamp.median > previous_best_timestamp {
						updated_prices.push((
							asset.clone(),
							response.timestamp.median,
							response.price.median.clone(),
						));
					}

					// get the previous price state, or create a default one if none is available
					let entry =
						state.chain_states[query.chain].price.entry(asset.clone()).or_default();

					// update the price state if the response contains newer data
					if response.timestamp.median > entry.timestamp.median {
						entry.timestamp = response.timestamp.clone();
						entry.price = response.price.clone();
						entry.updated_at_statechain_block = current_statechain_block.clone();
					}
				}

				// emit event with all updated prices
				if !updated_prices.is_empty() {
					state.emit_oracle_price_event.run(updated_prices);
				}
			},
		}

		all::<ExternalPriceChain>().for_each(|chain| {
			state.chain_states[chain].update_price_state(&state.get_time.run(()), &settings[chain])
		});

		Ok(())
	}

	#[cfg(test)]
	fn step_specification(
		before: &mut Self::State,
		input: &crate::electoral_systems::state_machine::state_machine::InputOf<Self>,
		_output: &Self::Output,
		_settings: &Self::Settings,
		after: &Self::State,
	) {
		match input {
			Either::Left(()) => {
				// prices do not change if we don't have consensus
				for chain in all::<ExternalPriceChain>() {
					for asset in all::<T::AssetPair>() {
						assert_eq!(
							before.chain_states[chain].price.get(&asset).map(|asset| &asset.price),
							after.chain_states[chain].price.get(&asset).map(|asset| &asset.price)
						);
					}
				}
			},
			Either::Right((query, consensus)) => {
				for asset in all::<T::AssetPair>() {
					// prices are updated if they are newer
					if let Some(consensus_asset_state) = consensus.get(&asset) {
						if consensus_asset_state.timestamp.median >
							before.chain_states[query.chain]
								.price
								.get(&asset)
								.map(|asset| asset.timestamp.median)
								.unwrap_or_default()
						{
							assert_eq!(
								after.chain_states[query.chain].price.get(&asset).unwrap().price,
								consensus_asset_state.price
							);
						}
					}
				}
			},
		}
	}
}

#[cfg(test)]
pub mod tests {
	use crate::electoral_systems::state_machine::core::TypesFor;
	use proptest::prelude::{any, Just, Strategy};

	use super::*;
	use crate::electoral_systems::oracle_price::{
		chainlink::{
			get_all_latest_prices_with_statechain_encoding, ChainlinkAssetpair, ChainlinkPrice,
		},
		price::Fraction,
	};
	use cf_traits::hook_test_utils::MockHook;

	pub struct Mock;
	pub(crate) type MockTypes = TypesFor<Mock>;

	impl OPTypes for MockTypes {
		type StateChainBlockNumber = u32;
		type Price = ChainlinkPrice;
		type AssetPair = ChainlinkAssetpair;
		type GetTime = MockHook<HookTypeFor<Self, GetTimeHook>>;
		type GetStateChainBlockHeight = MockHook<HookTypeFor<Self, GetStateChainBlockHeight>>;
		type SafeModeEnabledHook = MockHook<HookTypeFor<Self, SafeModeEnabledHook>>;
		type EmitPricesUpdatedEventHook = MockHook<HookTypeFor<Self, EmitPricesUpdatedEvent>>;
	}

	#[test]
	fn test_price_oracle_statemachine() {
		OraclePriceTracker::<MockTypes>::test(
			file!(),
			any::<OraclePriceTracker<MockTypes>>(),
			any::<OraclePriceSettings<MockTypes>>(),
			|_| any::<BTreeMap<ChainlinkAssetpair, AssetResponse<MockTypes>>>().boxed(),
			|_| Just(()).boxed(),
			|state| {
				// verify that getting the prices doesn't panic
				let _ = get_all_latest_prices_with_statechain_encoding(state);
			},
		)
	}

	#[expect(clippy::bool_assert_comparison)]
	#[test]
	fn should_vote_sanity_check() {
		// the computation of voting conditions uses the `to_price_range()` method
		let price: ChainlinkPrice = Fraction::integer(100);
		let range = price.to_price_range(BasisPoints(1500)).unwrap();
		assert_eq!(range, Fraction::integer(85)..=Fraction::integer(115));

		// empty voting conditions mean that should vote is always true
		assert_eq!(
			should_vote_for_asset::<MockTypes>(&(UnixTime::default(), Fraction::integer(100)), &[]),
			true
		);

		// A newer timestamps means we should vote
		assert_eq!(
			should_vote_for_asset::<MockTypes>(
				&(UnixTime { seconds: 10 }, Fraction::integer(100)),
				&[VotingCondition::NewTimestamp { last_timestamp: UnixTime { seconds: 9 } }]
			),
			true
		);

		// The same timestamp means we shouldn't vote
		assert_eq!(
			should_vote_for_asset::<MockTypes>(
				&(UnixTime { seconds: 10 }, Fraction::integer(100)),
				&[VotingCondition::NewTimestamp { last_timestamp: UnixTime { seconds: 10 } }]
			),
			false
		);

		// Price changed, but less than the `deviation` parameter, so we shouldn't vote
		assert_eq!(
			should_vote_for_asset::<MockTypes>(
				&(UnixTime::default(), Fraction::integer(120)),
				&[VotingCondition::PriceMoved {
					last_price: Fraction::integer(100),
					deviation: BasisPoints(2300)
				}]
			),
			false
		);

		// Price changed more than deviation parameter, so we should vote
		assert_eq!(
			should_vote_for_asset::<MockTypes>(
				&(UnixTime::default(), Fraction::integer(125)),
				&[VotingCondition::PriceMoved {
					last_price: Fraction::integer(100),
					deviation: BasisPoints(2300)
				}]
			),
			true
		);
	}
}
