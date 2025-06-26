use cf_chains::witness_period::SaturatingStep;
use enum_iterator::{all, Sequence};
use itertools::{Either, Itertools};
use crate::electoral_systems::state_machine::common_imports::*;

use crate::electoral_systems::state_machine::state_machine::{AbstractApi, Statemachine};


pub trait OPTypes: 'static + Sized {
    /// The partial order decides when we accept to the price,
    /// namely when `old_aggregation < new_aggregation`. The default derived
    /// implementation of the `PartialOrd` trait is not useful for this,
    /// a manual, apprioprate implementation should be used.
    type PriceData: PartialOrd;

    type InternalTime: SaturatingStep + Ord;
}

// pub type SCBLOCK = u32;

// impl DurationBetween for TIMESTAMP {
//     fn duration_to(&self, other: &Self) -> Duration {
//         Duration::from_millis((self - other) as u64)
//     }
// }

// pub trait DurationBetween {
//     fn duration_to(&self, other: &Self) -> Duration;
// }



//---------------- the primitives ------------------


def_derive! {
    #[derive(Sequence, PartialOrd, Ord)]
    pub enum ExternalPriceChain {
        Solana,
        Ethereum
    }
}

def_derive! {
    pub enum ExternalChainBlockQueried {
        Solana(u64),
        Ethereum(u32)
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum PriceState {
    UpToDate,
    Stale
}



pub struct PriceEntry {
    price: PRICE,
    timestamp: TIMESTAMP,
    block: ExternalChainBlockQueried,
    state: PriceState,
    updated_at: SCBLOCK
}

impl PriceEntry {

    pub fn get_query(&self, ages: &Ages, settings: &OraclePriceTrackerSettings) -> QueryType {
        use PriceState::*;
        use QueryType::*;
        match self.state {
            UpToDate => {
                if self.timestamp < ages.old_age {
                    MaybeStaleRequestingLatest
                } else {
                    OnPriceMovement { 
                        last_block: self.block.clone(),
                        last_price: self.price.clone(),
                        minimal_deviation: settings.minimal_price_deviation 
                    }
                }
            },
            Stale => OnUpdate { 
                last_block: self.block.clone() 
            },
        }
    }
    pub fn update(&mut self, response: PriceResponse) {
        if response.time > self.timestamp
            && response.block > self.block
        {
            *self = PriceEntry {
                price: response.price,
                timestamp: response.time,
                block: response.block,
                state: PriceState::UpToDate,
            };
        } else {
            self.state = PriceState::Stale;
        }
    }
}

pub struct PriceHistory {
    prices: BTreeMap<ExternalPriceChain, PriceEntry>,
}

impl PriceHistory {
    pub fn get_queries(&self, ages: &Ages, settings: &OraclePriceTrackerSettings) -> Vec<PriceQuery> {
        use QueryType::*;

        all::<ExternalPriceChain>()
            .map(|chain| 
                self.prices
                    .get(&chain)
                    .map(|entry| 
                        (
                            entry.state,
                            PriceQuery { 
                                query_type: entry.get_query(ages, settings),
                                chain: chain.clone(), 
                            }))
                    .unwrap_or(
                        (
                            PriceState::Stale,
                            PriceQuery {
                                chain,
                                query_type: MaybeStaleRequestingLatest,
                            }
                        )
                    )
            )
            .take_while_inclusive(|(state, query)| *state == PriceState::Stale)
            .map(|(state, query)| query)
            .collect()
    }

    pub fn handle_response(&mut self, chain: ExternalPriceChain, response: PriceResponse) {
        if let Some(entry) = self.prices.get_mut(&chain) {
            entry.update(response);
        } else {
            self.prices.insert(chain, PriceEntry { 
                price: response.price,
                timestamp: response.time,
                block: response.block,
                state: PriceState::UpToDate 
            });
        }
    }

    pub fn mark_stale(&mut self, ages: &Ages) {
        for entry in self.prices.values_mut() {
            if entry.timestamp < ages.stale_age {
                entry.state = PriceState::Stale;
            }
        }
    }

    
}


//---------------- the api ------------------

def_derive! {
    pub enum QueryType {
        MaybeStaleRequestingLatest,
        OnPriceMovement {
            last_block: ExternalChainBlockQueried,
            last_price: PRICE,
            minimal_deviation: u16
        },
        OnUpdate {
            last_block: ExternalChainBlockQueried,
        }
    }
}

pub struct PriceQuery {
    chain: ExternalPriceChain,
    query_type: QueryType
}

pub struct PriceResponse {
    time: TIMESTAMP,
    block: ExternalChainBlockQueried,
    price: PRICE
}

pub enum PriceResponseError {
    SubmittedBlockTooOld,
    SubmittedTimestampTooOld,
    PriceWithinMinimalDeviation,
}

pub struct OraclePriceTrackerSettings {
    maximal_price_update_delay_seconds: u32,
    mark_stale_after_seconds: u32,
    minimal_price_deviation: u16,
    latest_election_timeout_seconds: u32,
}

impl OraclePriceTrackerSettings {
    fn compute_ages(&self, current_time: TIMESTAMP) -> Ages {
        let old_age = current_time.saturating_sub(self.maximal_price_update_delay_seconds as i64);
        let stale_age = old_age.saturating_sub(self.mark_stale_after_seconds as i64);
        Ages { 
            old_age, 
            stale_age 
        }
    }
    fn old_age(&self) -> Duration {
        Duration::from_millis((self.maximal_price_update_delay_seconds * 1000) as u64)
    }
    fn stale_age(&self) -> Duration {
        Duration::from_millis((self.maximal_price_update_delay_seconds + self.mark_stale_after_seconds * 1000) as u64)
    }
}

defx! {
    pub struct OraclePriceTracker[T: OPTypes] {
        prices_history: PriceHistory,
        current_ages: Ages<T>,
    }
    validate this (else OraclePriceTrackerError) {}
}

def_derive!{
    pub struct Ages<T: OPTypes> {
        old_age: T::InternalTime,
        stale_age: T::InternalTime,
    }
}

defx!{
    pub struct OraclePriceTrackerContext[T: OPTypes] {
        current_time: T::InternalTime,
    }
    validate this (else OraclePriceContextError) {}
}

impl<T: OPTypes> AbstractApi for OraclePriceTracker<T> {
    type Query = PriceQuery;
    type Response = PriceResponse;
    type Error = PriceResponseError;

    fn validate(query: &Self::Query, response: &Self::Response) -> Result<(), Self::Error> {
        todo!()
    }
}

impl<T: OPTypes> Statemachine for OraclePriceTracker<T> {
    type Context = OraclePriceTrackerContext<T>;
    type Settings = OraclePriceTrackerSettings;
    type Output = ();
    type State = OraclePriceTracker<T>;

    fn get_queries(state: &mut Self::State, settings: &Self::Settings) -> Vec<Self::Query> {
        state.prices_history.get_queries(&state.current_ages, settings)
    }

    fn step(
            state: &mut Self::State,
            input: crate::electoral_systems::state_machine::state_machine::InputOf<Self>,
            settings: &Self::Settings,
        ) -> Self::Output {

        match input {
            Either::Left(context) => {
                state.current_ages = settings.compute_ages(context.current_time);
            },
            Either::Right((query, response)) => {
                state.prices_history.handle_response(query.chain, response);
            },
        }

        state.prices_history.mark_stale(&state.current_ages);
    }
}