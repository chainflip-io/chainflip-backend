use crate::electoral_systems::{
	block_witnesser::state_machine::HookTypeFor,
	oracle_price::primitives::{Aggregation, Apply, Seconds, UnixTime},
	state_machine::common_imports::*,
};
use enum_iterator::{all, Sequence};
use itertools::{Either, Itertools};

use crate::electoral_systems::state_machine::state_machine::{AbstractApi, Statemachine};
use sp_std::ops::{Add, Index, IndexMut};

pub trait OPTypes: 'static + Sized + CommonTraits {
	type Price: CommonTraits + Ord;

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
	pub enum PriceStatus {
		UpToDate,
		MaybeStale,
		Stale
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct ExternalChainState<T: OPTypes> {
		pub block: ExternalChainBlockQueried,
		pub timestamp: Apply<T::Aggregation, UnixTime>,
		pub price: BTreeMap<T::Asset, Apply<T::Aggregation, T::Price>>,
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct ExternalChainStateVote<T: OPTypes> {
		pub block: ExternalChainBlockQueried,
		pub timestamp: UnixTime,
		pub price: BTreeMap<T::Asset, T::Price>,
	}
}

impl<T: OPTypes> ExternalChainState<T> {
	pub fn get_status(
		&self,
		current_time: &UnixTime,
		settings: &ExternalChainSettings,
	) -> PriceStatus {
		use PriceStatus::*;
		let up_to_date_until =
			T::Aggregation::canonical(&self.timestamp) + settings.up_to_date_timeout;
		let maybe_stale_until = up_to_date_until.clone() + settings.maybe_stale_timeout;
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
		current_time: &UnixTime,
		settings: &ExternalChainSettings,
	) -> QueryType<T> {
		use PriceStatus::*;
		use QueryType::*;
		match self.get_status(current_time, settings) {
			UpToDate => OnPriceDeviation {
				last_block: self.block.clone(),
				last_price: self
					.price
					.iter()
					.map(|(asset, price)| (asset.clone(), T::Aggregation::canonical(price)))
					.collect(),
				minimal_deviation: settings.minimal_price_deviation.clone(),
			},
			MaybeStale => LatestPrice,
			Stale => OnUpdate { last_block: self.block.clone() },
		}
	}

	pub fn update(&mut self, response: ExternalChainState<T>) {
		if T::Aggregation::canonical(&response.timestamp) >
			T::Aggregation::canonical(&self.timestamp) &&
			response.block > self.block
		{
			*self = response;
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
	pub fn get_queries(
		&self,
		current_time: &UnixTime,
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
	#[derive(TypeInfo)]
	pub enum QueryType<T: OPTypes> {
		LatestPrice,
		OnPriceDeviation {
			last_block: ExternalChainBlockQueried,
			last_price: BTreeMap<T::Asset, T::Price>,
			minimal_deviation: BasisPoints
		},
		OnUpdate {
			last_block: ExternalChainBlockQueried,
		}
	}
}

def_derive! {
	#[derive(TypeInfo)]
	pub struct PriceQuery<T: OPTypes> {
		pub chain: ExternalPriceChain,
		pub query_type: QueryType<T>,
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
	#[derive(TypeInfo)]
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
		pub queries: Vec<PriceQuery<T>>,
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
	type Response = ExternalChainState<T>;
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

		log::info!("Called step function for oracle at time {:?}", state.get_time.run(()));

		Ok(())
	}
}

//-------------- partial voter implementation -----------
//
// The voter needs access to some RPC to query for data.
//
// What would actually be nice is if we could
