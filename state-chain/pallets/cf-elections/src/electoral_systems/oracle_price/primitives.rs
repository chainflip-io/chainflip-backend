use core::ops::Range;

use cf_primitives::Asset;

use crate::electoral_systems::oracle_price::state_machine::ExternalChainBlockQueried;

// struct Aggregated<A> {
// 	median: A,
// 	iq_range: Range<A>,
// }

// struct MeanPriceData {
// 	timestamp: Aggregated<TIMESTAMP>,
// 	prices: BTreeMap<Asset, Aggregated<PRICE>>,
// 	block: ExternalChainBlockQueried,
// }
