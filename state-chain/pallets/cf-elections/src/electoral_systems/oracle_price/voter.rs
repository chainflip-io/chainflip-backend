use crate::electoral_systems::oracle_price::state_machine::{
	ExternalChainStateVote, OPTypes, PriceQuery,
};

use super::super::state_machine::common_imports::*;

// pub fn check_oracle_price_vote_required<T: OPTypes>(
// 	properties: PriceQuery<T>,
// 	vote: ExternalChainStateVote<T>,
// ) -> bool {
// 	match properties.query_type {
// 		super::state_machine::QueryType::LatestPrice => true,
// 		super::state_machine::QueryType::OnPriceDeviation {
// 			last_block,
// 			last_price,
// 			minimal_deviation,
// 		} => todo!(),
// 		super::state_machine::QueryType::OnUpdate { last_block } => todo!(),
// 	}
// }
