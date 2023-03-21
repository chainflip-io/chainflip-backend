#![cfg_attr(not(feature = "std"), no_std)]

use cf_primitives::{Asset, Liquidity, Tick};
use sp_runtime::AccountId32;
use sp_std::vec::Vec;

use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	pub trait PoolsApi {
		fn cf_pool_tick_price(asset: Asset) -> Option<Tick>;
		fn cf_pool_minted_positions(lp: AccountId32, asset: Asset) -> Vec<(Tick, Tick, Liquidity)>;
	}
);
