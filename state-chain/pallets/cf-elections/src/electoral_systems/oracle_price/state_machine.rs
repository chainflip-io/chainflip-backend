use crate::{
	electoral_systems::{
		block_witnesser::state_machine::HookTypeFor,
		oracle_price::{
			price::PriceUnit,
			primitives::{BasisPoints, Seconds, UnixTime},
		},
		state_machine::common_imports::*,
	},
	generic_tools::*,
};
use core::ops::RangeInclusive;
use enum_iterator::{all, Sequence};
use itertools::{Either, Itertools};

use crate::electoral_systems::{
	oracle_price::primitives::Aggregated,
	state_machine::state_machine::{AbstractApi, Statemachine},
};
use sp_std::{
	ops::{Index, IndexMut},
	vec,
};

#[cfg(test)]
use proptest_derive::Arbitrary;

//--------------- configuration trait -----------------

pub trait OPTypes: 'static + Sized + CommonTraits {
	type Price: PriceTrait + CommonTraits + Ord + Default + MaybeArbitrary;

	type AssetPair: AssetPairTrait + CommonTraits + Ord + Sequence + MaybeArbitrary;

	type GetTime: Hook<HookTypeFor<Self, GetTimeHook>> + CommonTraits;
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

//---------------- the primitives ------------------

def_derive! {
	#[derive(Copy, Sequence, PartialOrd, Ord, TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum ExternalPriceChain {
		Solana,
		Ethereum
	}
}

def_derive! {
	#[derive(TypeInfo, Copy, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum PriceStatus {
		UpToDate,
		MaybeStale,
		#[default]
		Stale
	}
}

def_derive! {
	#[derive_where(Default;)]
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct AssetState<T: OPTypes> {
		pub timestamp: Aggregated<UnixTime>,
		pub price: Aggregated<T::Price>,
		pub price_status: PriceStatus,
		pub price_spiked: bool,
		pub minimal_price_deviation: BasisPoints
	}
}

impl<T: OPTypes> AssetState<T> {
	pub fn update(&mut self, response: AssetResponse<T>) {
		if response.timestamp.median > self.timestamp.median {
			self.timestamp = response.timestamp;
			self.price = response.price;
		}
	}
}

def_derive! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct AssetResponse<T: OPTypes> {
		pub timestamp: Aggregated<UnixTime>,
		pub price: Aggregated<T::Price>,
	}
}

def_derive! {
	#[derive_where(Default;)]
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainState<T: OPTypes> {
		pub price: BTreeMap<T::AssetPair, AssetState<T>>,
	}
}

def_derive! {
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
		settings: &ExternalChainSettings,
	) {
		use PriceStatus::*;
		self.price.values_mut().for_each(|asset_state| {
			let up_to_date_until = asset_state.timestamp.median + settings.up_to_date_timeout;
			let maybe_stale_until = up_to_date_until + settings.maybe_stale_timeout;

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

	pub fn update(&mut self, response: BTreeMap<T::AssetPair, AssetResponse<T>>) {
		for (asset, response) in response {
			let entry = self.price.entry(asset).or_default();
			entry.update(response);
		}
	}
}

def_derive! {
	#[derive(TypeInfo, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainStates<T: OPTypes> {
		pub solana: ExternalChainState<T>,
		pub ethereum: ExternalChainState<T>,
	}
}

impl<T: OPTypes> ExternalChainStates<T> {
	pub fn get_latest_price(&self, asset: T::AssetPair) -> Option<(T::Price, PriceStatus)> {
		all::<ExternalPriceChain>()
			.filter_map(|chain| self[chain].price.get(&asset))
			.max_by_key(|price_state| price_state.timestamp.median)
			.map(|price_state| (price_state.price.median.clone(), price_state.price_status))
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
			ExternalPriceChain::Solana => &self.solana,
			ExternalPriceChain::Ethereum => &self.ethereum,
		}
	}
}

impl<T: OPTypes> IndexMut<ExternalPriceChain> for ExternalChainStates<T> {
	fn index_mut(&mut self, index: ExternalPriceChain) -> &mut Self::Output {
		match index {
			ExternalPriceChain::Solana => &mut self.solana,
			ExternalPriceChain::Ethereum => &mut self.ethereum,
		}
	}
}

//---------------- the api ------------------

def_derive! {
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

def_derive! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct PriceQuery<T: OPTypes> {
		pub chain: ExternalPriceChain,
		pub assets: BTreeMap<T::AssetPair, Vec<VotingCondition<T>>>
	}
}

def_derive! {
	#[derive(TypeInfo, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct OraclePriceSettings {
		pub solana: ExternalChainSettings,
		pub ethereum: ExternalChainSettings,
	}
}

impl Index<ExternalPriceChain> for OraclePriceSettings {
	type Output = ExternalChainSettings;

	fn index(&self, index: ExternalPriceChain) -> &Self::Output {
		match index {
			ExternalPriceChain::Solana => &self.solana,
			ExternalPriceChain::Ethereum => &self.ethereum,
		}
	}
}

def_derive! {
	#[derive(TypeInfo, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainSettings {
		pub up_to_date_timeout: Seconds,
		pub maybe_stale_timeout: Seconds,
		pub minimal_price_deviation: BasisPoints,
	}
}

def_derive! {
	#[derive(TypeInfo, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct OraclePriceTracker<T: OPTypes> {
		pub chain_states: ExternalChainStates<T>,
		pub get_time: T::GetTime,
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
	type Settings = OraclePriceSettings;
	type Output = Result<(), &'static str>;
	type State = OraclePriceTracker<T>;

	fn get_queries(state: &mut Self::State) -> Vec<Self::Query> {
		all::<ExternalPriceChain>()
			.take_while_inclusive(|chain| {
				// return true if at least one asset does not exist OR is not `UpToDate`
				all::<T::AssetPair>().any(|asset| {
					state.chain_states[*chain]
						.price
						.get(&asset)
						.map(|asset_state| asset_state.price_status != PriceStatus::UpToDate)
						.unwrap_or(true)
				})
			})
			.map(|chain| PriceQuery { chain, assets: state.chain_states[chain].get_query() })
			.collect()
	}

	fn step(
		state: &mut Self::State,
		input: crate::electoral_systems::state_machine::state_machine::InputOf<Self>,
		settings: &Self::Settings,
	) -> Self::Output {
		match input {
			Either::Left(()) => {},
			Either::Right((query, response)) => {
				state.chain_states[query.chain].update(response);
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
	use proptest::prelude::{any, Just, Strategy};

	use super::*;
	use crate::electoral_systems::{
		oracle_price::chainlink::{
			get_all_latest_prices_with_statechain_encoding, ChainlinkAssetpair, ChainlinkPrice,
		},
		state_machine::core::hook_test_utils::MockHook,
	};

	pub struct Mock;
	pub(crate) type MockTypes = TypesFor<Mock>;

	impl OPTypes for MockTypes {
		type Price = ChainlinkPrice;
		type AssetPair = ChainlinkAssetpair;
		type GetTime = MockHook<HookTypeFor<Self, GetTimeHook>>;
	}

	#[test]
	fn test_price_oracle_statemachine() {
		OraclePriceTracker::<MockTypes>::test(
			file!(),
			any::<OraclePriceTracker<MockTypes>>(),
			any::<OraclePriceSettings>(),
			|_| any::<BTreeMap<ChainlinkAssetpair, AssetResponse<MockTypes>>>().boxed(),
			|_| Just(()).boxed(),
			|state| {
				// verify that getting the prices doesn't panic
				let _ = get_all_latest_prices_with_statechain_encoding(state);
			},
		)
	}
}
