use crate::GetMinimumFunding;
use cf_primitives::AssetAmount;

pub struct MockMinimumFundingProvider;
const MINIMUM_FUNDING: u128 = 100;

impl GetMinimumFunding for MockMinimumFundingProvider {
	fn get_min_funding_amount() -> AssetAmount {
		MINIMUM_FUNDING
	}
}
