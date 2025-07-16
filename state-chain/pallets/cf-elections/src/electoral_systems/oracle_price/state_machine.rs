use crate::electoral_systems::{
	block_witnesser::state_machine::HookTypeFor,
	oracle_price::primitives::{Aggregation, Apply, Seconds, UnixTime},
	state_machine::common_imports::*,
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
	type Price: CommonTraits + Ord + Default + MaybeArbitrary;

	fn price_range(price: &Self::Price, range: BasisPoints) -> RangeInclusive<Self::Price>;

	type Asset: CommonTraits + Ord + Sequence + MaybeArbitrary;

	type Aggregation: CommonTraits + Aggregation + MaybeArbitrary;

	type GetTime: Hook<HookTypeFor<Self, GetTimeHook>> + CommonTraits;
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
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub enum PriceStatus {
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
		pub price_status: PriceStatus,
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
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainState<T: OPTypes> {
		pub price: BTreeMap<T::Asset, AssetState<T>>,
	}
}

def_derive! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainStateVote<T: OPTypes> {
		pub price: BTreeMap<T::Asset, (UnixTime, T::Price)>,
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
			!T::price_range(&last_price, *deviation).contains(&price),
		NewTimestamp { last_timestamp } => time > last_timestamp,
	})
}

pub fn get_price_status(
	price_timestamp: UnixTime,
	current_time: UnixTime,
	settings: &ExternalChainSettings,
) -> PriceStatus {
	let up_to_date_until = price_timestamp + settings.up_to_date_timeout;
	let maybe_stale_until = up_to_date_until + settings.maybe_stale_timeout;

	use PriceStatus::*;
	if current_time <= up_to_date_until {
		UpToDate
	} else if current_time <= maybe_stale_until {
		MaybeStale
	} else {
		Stale
	}
}

impl<T: OPTypes> ExternalChainState<T> {
	pub fn is_any_asset_price_stale(&self) -> bool {
		all::<T::Asset>().any(|asset| {
			self.price
				.get(&asset)
				.map(|asset_state| asset_state.price_status == PriceStatus::Stale)
				.unwrap_or(true)
		})
	}

	pub fn update_price_state(
		&mut self,
		current_time: &UnixTime,
		settings: &ExternalChainSettings,
	) {
		self.price.values_mut().for_each(|asset_state| {
			asset_state.price_status = get_price_status(
				T::Aggregation::canonical(&asset_state.timestamp),
				*current_time,
				settings,
			)
		});
	}

	pub fn get_query(&self) -> BTreeMap<T::Asset, Vec<VotingCondition<T>>> {
		use PriceStatus::*;

		all::<T::Asset>()
			.map(|asset| {
				(
					asset.clone(),
					self.price
						.get(&asset)
						.map(|asset_state| match asset_state.price_status {
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

	pub fn update(&mut self, response: BTreeMap<T::Asset, AssetResponse<T>>) {
		for (asset, response) in response {
			let entry = self.price.entry(asset).or_insert(AssetState {
				timestamp: T::Aggregation::single(&Default::default()),
				price: T::Aggregation::single(&Default::default()),
				price_status: PriceStatus::Stale,
				minimal_price_deviation: Default::default(),
			});
			entry.update(response);
		}
	}
}

def_derive! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainStates<T: OPTypes> {
		pub solana: ExternalChainState<T>,
		pub ethereum: ExternalChainState<T>,
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
		pub assets: BTreeMap<T::Asset, Vec<VotingCondition<T>>>
	}
}

def_derive! {
	#[derive(TypeInfo)]
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
	#[derive(TypeInfo, Copy, Default)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct BasisPoints(pub u16);
}

def_derive! {
	#[derive(TypeInfo)]
	#[cfg_attr(test, derive(Arbitrary))]
	pub struct ExternalChainSettings {
		pub up_to_date_timeout: Seconds,
		pub maybe_stale_timeout: Seconds,
		pub minimal_price_deviation: BasisPoints,
	}
}

def_derive! {
	#[derive(TypeInfo)]
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
	type Response = BTreeMap<T::Asset, AssetResponse<T>>;
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
			.take_while_inclusive(|chain| state.chain_states[*chain].is_any_asset_price_stale())
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
}

/*

#[cfg(test)]
mod tests {
	use proptest::prelude::{any, Just, Strategy};

	use super::*;
	use crate::electoral_systems::{
		oracle_price::primitives::*,
		state_machine::core::{hook_test_utils::MockHook, *},
	};

	struct Mock;
	type MockTypes = TypesFor<Mock>;

	impl OPTypes for MockTypes {
		type Price = u128;

		fn price_range(price: &Self::Price, range: BasisPoints) -> RangeInclusive<Self::Price> {
			todo!()
		}

		type Asset = ChainlinkAssetPair;
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
		)
	}
}

 */
