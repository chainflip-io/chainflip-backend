use crate::{
	electoral_systems::{
		block_witnesser::state_machine::HookTypeFor,
		oracle_price::{
			price::{self, PriceUnit},
			primitives::{Aggregation, Apply, BasisPoints, Seconds, UnixTime},
		},
		state_machine::common_imports::*,
	},
	generic_tools::*,
};
use core::ops::RangeInclusive;
use enum_iterator::{all, Sequence};
use itertools::{Either, Itertools};

use crate::electoral_systems::state_machine::state_machine::{AbstractApi, Statemachine};
use sp_std::{
	ops::{Index, IndexMut},
	vec,
};

#[cfg(test)]
use proptest_derive::Arbitrary;

pub trait OPTypes: 'static + Sized + CommonTraits {
	type Price: PriceTrait + CommonTraits + Ord + Default + MaybeArbitrary;

	type AssetPair: AssetPairTrait + CommonTraits + Ord + Sequence + MaybeArbitrary;

	type Aggregation: Aggregation + CommonTraits + MaybeArbitrary;

	type GetTime: Hook<HookTypeFor<Self, GetTimeHook>> + CommonTraits;
}

pub trait AssetPairTrait {
	fn to_price_unit(&self) -> PriceUnit;
}

pub trait PriceTrait: Sized {
	fn to_price_range(&self, range: BasisPoints) -> RangeInclusive<Self>;
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
	#[derive(TypeInfo, Copy)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum PriceStaleness {
		UpToDate,
		MaybeStale,
		Stale
	}
}

def_derive! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct AssetState<T: OPTypes> {
		pub timestamp: Apply<T::Aggregation, UnixTime>,
		pub price: Apply<T::Aggregation, T::Price>,
		pub price_staleness: PriceStaleness,
		pub price_spiked: bool,
		pub minimal_price_deviation: BasisPoints
	}
}

impl<T: OPTypes> AssetState<T> {
	pub fn update(&mut self, response: AssetResponse<T>) {
		if T::Aggregation::canonical(&response.timestamp) >
			T::Aggregation::canonical(&self.timestamp)
		{
			self.timestamp = response.timestamp;
			self.price = response.price;
		}
	}
}

def_derive! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct AssetResponse<T: OPTypes> {
		pub timestamp: Apply<T::Aggregation, UnixTime>,
		pub price: Apply<T::Aggregation, T::Price>,
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
	conditions: &Vec<VotingCondition<T>>,
) -> bool {
	use VotingCondition::*;
	conditions.iter().all(|condition| match condition {
		PriceMoved { last_price, deviation } =>
			!last_price.to_price_range(*deviation).contains(&price),
		NewTimestamp { last_timestamp } => time > last_timestamp,
	})
}

pub fn get_price_status(
	price_timestamp: UnixTime,
	current_time: UnixTime,
	settings: &ExternalChainSettings,
) -> PriceStaleness {
	let up_to_date_until = price_timestamp + settings.up_to_date_timeout;
	let maybe_stale_until = up_to_date_until + settings.maybe_stale_timeout;

	use PriceStaleness::*;
	if current_time <= up_to_date_until {
		UpToDate
	} else if current_time <= maybe_stale_until {
		MaybeStale
	} else {
		Stale
	}
}

impl<T: OPTypes> ExternalChainState<T> {
	pub fn is_any_asset_price_not_up_to_date(&self) -> bool {
		all::<T::AssetPair>().any(|asset| {
			self.price
				.get(&asset)
				.map(|asset_state| asset_state.price_staleness != PriceStaleness::UpToDate)
				.unwrap_or(true)
		})
	}

	pub fn update_price_state(
		&mut self,
		current_time: &UnixTime,
		settings: &ExternalChainSettings,
	) {
		self.price.values_mut().for_each(|asset_state| {
			asset_state.price_staleness = get_price_status(
				T::Aggregation::canonical(&asset_state.timestamp),
				*current_time,
				settings,
			)
		});
	}

	pub fn get_query(&self) -> BTreeMap<T::AssetPair, Vec<VotingCondition<T>>> {
		use PriceStaleness::*;

		all::<T::AssetPair>()
			.map(|asset| {
				(
					asset.clone(),
					self.price
						.get(&asset)
						.map(|asset_state| match asset_state.price_staleness {
							UpToDate => vec![
								VotingCondition::NewTimestamp {
									last_timestamp: T::Aggregation::canonical(
										&asset_state.timestamp,
									),
								},
								VotingCondition::PriceMoved {
									last_price: T::Aggregation::canonical(&asset_state.price),
									deviation: asset_state.minimal_price_deviation,
								},
							],
							Stale => vec![VotingCondition::NewTimestamp {
								last_timestamp: T::Aggregation::canonical(&asset_state.timestamp),
							}],
							MaybeStale => vec![],
						})
						.unwrap_or(vec![]),
				)
			})
			.collect()
	}

	pub fn update(&mut self, response: BTreeMap<T::AssetPair, AssetResponse<T>>) {
		for (asset, response) in response {
			let entry = self.price.entry(asset).or_insert(AssetState {
				timestamp: T::Aggregation::single(&Default::default()),
				price: T::Aggregation::single(&Default::default()),
				price_staleness: PriceStaleness::Stale,
				price_spiked: false,
				minimal_price_deviation: Default::default(),
			});
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
	pub fn get_latest_prices(&self) -> BTreeMap<T::AssetPair, (T::Price, PriceStaleness)> {
		all::<T::AssetPair>()
			.filter_map(|asset| {
				all::<ExternalPriceChain>()
					.filter_map(|chain| self[chain].price.get(&asset))
					.max_by_key(|price_state| T::Aggregation::canonical(&price_state.timestamp))
					.map(|price_state| {
						(
							asset,
							(
								T::Aggregation::canonical(&price_state.price),
								price_state.price_staleness,
							),
						)
					})
			})
			.collect()
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
				state.chain_states[*chain].is_any_asset_price_not_up_to_date()
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
						if T::Aggregation::canonical(&consensus_asset_state.timestamp) >
							before.chain_states[query.chain]
								.price
								.get(&asset)
								.map(|asset| T::Aggregation::canonical(&asset.timestamp))
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
		oracle_price::{
			chainlink::{get_current_chainlink_prices, ChainlinkAssetPair, ChainlinkPrice},
			price::Fraction,
			primitives::*,
		},
		state_machine::core::{hook_test_utils::MockHook, *},
	};

	pub struct Mock;
	pub(crate) type MockTypes = TypesFor<Mock>;

	impl OPTypes for MockTypes {
		type Price = ChainlinkPrice;
		type AssetPair = ChainlinkAssetPair;
		type Aggregation = AggregatedF;
		type GetTime = MockHook<HookTypeFor<Self, GetTimeHook>>;
	}

	#[test]
	fn test_price_oracle_statemachine() {
		OraclePriceTracker::<MockTypes>::test(
			file!(),
			any::<OraclePriceTracker<MockTypes>>(),
			any::<OraclePriceSettings>(),
			|_| any::<BTreeMap<ChainlinkAssetPair, AssetResponse<MockTypes>>>().boxed(),
			|_| Just(()).boxed(),
			|state| {
				// verify that getting the prices doesn't panic
				let _ = get_current_chainlink_prices(state);
			},
		)
	}
}
