#![cfg_attr(not(feature = "std"), no_std)]

use cf_primitives::{Asset, AssetAmount, ExchangeRate};

use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	pub trait PoolsApi {
		fn cf_swap_rate(asset: &Asset, input_amount: AssetAmount) -> ExchangeRate;
	}
);
