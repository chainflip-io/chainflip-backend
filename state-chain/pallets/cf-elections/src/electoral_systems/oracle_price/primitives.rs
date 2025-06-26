use core::ops::Range;
use std::collections::BTreeMap;

use cf_primitives::Asset;

use crate::electoral_systems::oracle_price::state_machine::ExternalChainBlockQueried;


pub type TIMESTAMP = i64;
pub type PRICE = Vec<u8>;


struct MeanPrice {
    interquartile_mean_price: PRICE
}

struct IQMean<A> {
    iq_mean: A,
    iq_range: Range<A>,
}

struct MeanPriceData {
    timestamp: IQMean<TIMESTAMP>,
    prices: BTreeMap<Asset, IQMean<PRICE>>,
    block: ExternalChainBlockQueried

}