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
	ops::{Add, Index, IndexMut},
	vec,
};

pub trait OPTypes: 'static + Sized + CommonTraits {
	type Price: CommonTraits + Ord + Default;

	fn price_range(price: &Self::Price, range: BasisPoints) -> RangeInclusive<Self::Price>;

	type Asset: CommonTraits + Ord + Sequence;

	type Aggregation: CommonTraits + Aggregation;

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
	pub enum ExternalPriceChain {
		Solana,
		Ethereum
	}
}

def_derive! {
	#[derive(PartialOrd, Ord, TypeInfo)]
	pub enum ExternalChainBlockQueried {
		Solana(u64),
		Ethereum(u32)
	}
}

impl ExternalChainBlockQueried {
	pub fn chain(&self) -> ExternalPriceChain {
		match self {
			ExternalChainBlockQueried::Solana(_) => ExternalPriceChain::Solana,
			ExternalChainBlockQueried::Ethereum(_) => ExternalPriceChain::Ethereum,
		}
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub enum PriceStatus {
		UpToDate,
		MaybeStale,
		Stale
	}
}

def_derive! {
	#[derive(TypeInfo)]
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
	pub struct AssetResponse<T: OPTypes> {
		pub timestamp: Apply<T::Aggregation, UnixTime>,
		pub price: Apply<T::Aggregation, T::Price>,
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct ExternalChainState<T: OPTypes> {
		pub price: BTreeMap<T::Asset, AssetState<T>>,
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct ExternalChainStateVote<T: OPTypes> {
		pub price: BTreeMap<T::Asset, (UnixTime, T::Price)>,
	}
}

pub fn should_vote_for_asset<T: OPTypes>(
	(time, price): &(UnixTime, T::Price),
	conditions: &Vec<VotingCondition<T>>,
) -> bool {
	use VotingCondition::*;
	conditions.iter().all(|condition| match condition {
		PriceMoved { last_price, deviation } =>
			!T::price_range(&last_price, *deviation).contains(&price),
		NewTimestamp { last_timestamp } => time > last_timestamp,
		Always => todo!(),
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

def_derive! {
	#[derive(TypeInfo)]
	pub enum ChainPriceStatus {
		AtLeastOnePriceStale,
		AllUpToDate
	}
}

impl<T: OPTypes> ExternalChainState<T> {
	pub fn is_stale(&self) -> bool {
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

impl<T: OPTypes> ExternalChainStates<T> {
	pub fn get_queries(&self) -> Vec<PriceQuery<T>> {
		all::<ExternalPriceChain>()
			.map(|chain| PriceQuery { chain, assets: self[chain].get_query() })
			.take_while_inclusive(|query| self[query.chain].is_stale())
			.collect()
	}
}

//---------------- the api ------------------

def_derive! {
	#[derive(TypeInfo)]
	pub enum VotingCondition<T: OPTypes> {
		PriceMoved {
			last_price: T::Price,
			deviation: BasisPoints
		},
		NewTimestamp {
			last_timestamp: UnixTime
		},
		Always
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct PriceQuery<T: OPTypes> {
		pub chain: ExternalPriceChain,
		pub assets: BTreeMap<T::Asset, Vec<VotingCondition<T>>>
	}
}

pub enum PriceResponseError {
	SubmittedBlockTooOld,
	SubmittedTimestampTooOld,
	PriceWithinMinimalDeviation,
}

def_derive! {
	#[derive(TypeInfo)]
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
	pub struct BasisPoints(pub u16);
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct ExternalChainSettings {
		pub up_to_date_timeout: Seconds,
		pub maybe_stale_timeout: Seconds,
		pub minimal_price_deviation: BasisPoints,
	}
}

def_derive! {
	#[derive(TypeInfo)]
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
	type Error = PriceResponseError;

	fn validate(query: &Self::Query, response: &Self::Response) -> Result<(), Self::Error> {
		Ok(())
	}
}

impl<T: OPTypes> Statemachine for OraclePriceTracker<T> {
	type Context = ();
	type Settings = OraclePriceSettings;
	type Output = Result<(), &'static str>;
	type State = OraclePriceTracker<T>;

	fn get_queries(state: &mut Self::State) -> Vec<Self::Query> {
		state.chain_states.get_queries()
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

		log::info!("Called step function for oracle at time {:?}", state.get_time.run(()));

		Ok(())
	}
}
