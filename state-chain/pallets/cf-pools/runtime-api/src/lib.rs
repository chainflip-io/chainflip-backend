#![cfg_attr(not(feature = "std"), no_std)]

use cf_primitives::{Asset, Tick};

use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	pub trait PoolsApi {
		fn cf_pool_tick_price(asset: Asset) -> Option<Tick>;
	}
);
