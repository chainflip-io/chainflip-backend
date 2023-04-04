#![cfg_attr(not(feature = "std"), no_std)]

use cf_amm::{
	common::{Amount, SqrtPriceQ64F96, Tick},
	range_orders::Liquidity,
};
use cf_primitives::{AccountId, Asset};
use sp_std::vec::Vec;

use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	pub trait PoolsApi {
		fn cf_pool_sqrt_price(asset: Asset) -> Option<SqrtPriceQ64F96>;
		fn cf_pool_price(asset: Asset) -> Option<f64>;
		fn cf_pool_range_orders(
			lp: AccountId,
			asset: Asset,
		) -> Vec<(core::ops::Range<Tick>, Liquidity)>;
		fn cf_pool_limit_orders(lp: AccountId, asset: Asset) -> Vec<(Tick, Amount)>;
	}
);
