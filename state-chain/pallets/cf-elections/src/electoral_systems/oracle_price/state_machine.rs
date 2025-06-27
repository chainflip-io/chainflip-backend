use crate::electoral_systems::{
	block_witnesser::state_machine::HookTypeFor, state_machine::common_imports::*,
};
use enum_iterator::{all, Sequence};
use itertools::{Either, Itertools};

use crate::electoral_systems::state_machine::state_machine::{AbstractApi, Statemachine};
use sp_std::{
	ops::{Add, Index, IndexMut},
	time::Duration,
};

pub trait OPTypes: 'static + Sized + CommonTraits {
	/// The partial order decides when we accept to the price,
	/// namely when `old_aggregation < new_aggregation`. The default derived
	/// implementation of the `PartialOrd` trait is not useful for this,
	/// a manual, apprioprate implementation should be used.
	type Price: CommonTraits + PartialOrd;

	type AggregatedPrice: CommonTraits + PartialOrd;

	fn canonical_price(price: Self::AggregatedPrice) -> Self::Price;

	type Time: CommonTraits + Ord + Add<Duration, Output = Self::Time> + Validate;

	type GetTime: Hook<HookTypeFor<Self, GetTimeHook>> + CommonTraits;
}

pub struct GetTimeHook;
impl<T: OPTypes> HookType for HookTypeFor<T, GetTimeHook> {
	type Input = ();
	type Output = T::Time;
}

//---------------- the primitives ------------------

def_derive! {
	#[derive(Copy, Sequence, PartialOrd, Ord)]
	pub enum ExternalPriceChain {
		Solana,
		Ethereum
	}
}

def_derive! {
	#[derive(PartialOrd)]
	pub enum ExternalChainBlockQueried {
		Solana(u64),
		Ethereum(u32)
	}
}

def_derive! {
	pub enum PriceStatus {
		UpToDate,
		MaybeStale,
		Stale
	}
}

def_derive! {
	pub struct ExternalChainState<T: OPTypes> {
		block: ExternalChainBlockQueried,
		timestamp: T::Time,
		price: T::Price,
	}
}

impl<T: OPTypes> ExternalChainState<T> {
	pub fn get_status(
		&self,
		current_time: &T::Time,
		settings: &ExternalChainSettings,
	) -> PriceStatus {
		use PriceStatus::*;
		let up_to_date_until =
			self.timestamp.clone() + Duration::from_secs(settings.up_to_date_timeout_seconds);
		let maybe_stale_until =
			up_to_date_until.clone() + Duration::from_secs(settings.maybe_stale_timeout_blocks);
		if *current_time <= up_to_date_until {
			UpToDate
		} else if *current_time <= maybe_stale_until {
			MaybeStale
		} else {
			Stale
		}
	}

	pub fn get_query(
		&self,
		current_time: &T::Time,
		settings: &ExternalChainSettings,
	) -> QueryType<T> {
		use PriceStatus::*;
		use QueryType::*;
		match self.get_status(current_time, settings) {
			UpToDate => OnPriceDeviation {
				last_block: self.block.clone(),
				last_price: self.price.clone(),
				minimal_deviation: settings.minimal_price_deviation.clone(),
			},
			MaybeStale => todo!(),
			Stale => OnUpdate { last_block: self.block.clone() },
		}
	}

	pub fn update(&mut self, response: ExternalChainState<T>) {
		if response.timestamp > self.timestamp && response.block > self.block {
			*self = response;
		}
	}
}

def_derive! {
	pub struct ExternalChainStates<T: OPTypes> {
		solana: ExternalChainState<T>,
		ethereum: ExternalChainState<T>,
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
	pub fn get_queries(
		&self,
		current_time: &T::Time,
		settings: &OraclePriceSettings,
	) -> Vec<PriceQuery<T>> {
		use QueryType::*;

		all::<ExternalPriceChain>()
			.map(|chain| PriceQuery {
				chain,
				query_type: self[chain].get_query(current_time, &settings[chain]),
			})
			.take_while_inclusive(|query| matches!(query.query_type, LatestPrice | OnUpdate { .. }))
			.collect()
	}
}

//---------------- the api ------------------

def_derive! {
	pub enum QueryType<T: OPTypes> {
		LatestPrice,
		OnPriceDeviation {
			last_block: ExternalChainBlockQueried,
			last_price: T::Price,
			minimal_deviation: BasisPoints
		},
		OnUpdate {
			last_block: ExternalChainBlockQueried,
		}
	}
}

def_derive! {
	pub struct PriceQuery<T: OPTypes> {
		chain: ExternalPriceChain,
		query_type: QueryType<T>,
	}
}

pub enum PriceResponseError {
	SubmittedBlockTooOld,
	SubmittedTimestampTooOld,
	PriceWithinMinimalDeviation,
}

pub struct OraclePriceSettings {
	solana: ExternalChainSettings,
	ethereum: ExternalChainSettings,
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
	pub struct BasisPoints(u16);
}

pub struct ExternalChainSettings {
	up_to_date_timeout_seconds: u64,
	maybe_stale_timeout_blocks: u64,
	minimal_price_deviation: BasisPoints,
}

def_derive! {
	pub struct OraclePriceTracker<T: OPTypes> {
		chain_states: ExternalChainStates<T>,
		get_time: T::GetTime,
		queries: Vec<PriceQuery<T>>,
	}
}

impl<T: OPTypes> Validate for OraclePriceTracker<T> {
	type Error = ();

	fn is_valid(&self) -> Result<(), Self::Error> {
		Ok(())
	}
}

defx! {
	pub struct OraclePriceTrackerContext[T: OPTypes] {
		current_time: T::Time,
	}
	validate this (else OraclePriceContextError) {}
}

impl<T: OPTypes> AbstractApi for OraclePriceTracker<T> {
	type Query = PriceQuery<T>;
	type Response = ExternalChainState<T>;
	type Error = PriceResponseError;

	fn validate(query: &Self::Query, response: &Self::Response) -> Result<(), Self::Error> {
		todo!()
	}
}

impl<T: OPTypes> Statemachine for OraclePriceTracker<T> {
	type Context = ();
	type Settings = OraclePriceSettings;
	type Output = ();
	type State = OraclePriceTracker<T>;

	fn get_queries(state: &mut Self::State) -> Vec<Self::Query> {
		state.queries.clone()
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

		state.queries = state.chain_states.get_queries(&state.get_time.run(()), settings);
	}
}
