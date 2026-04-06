use cf_amm::math::Price;
use cf_primitives::Asset;

use super::{MockPallet, MockPalletStorage};
use crate::{PoolPrice, PoolPriceProvider};

use frame_support::sp_runtime::DispatchError;

pub struct MockPoolPriceApi {}

impl MockPallet for MockPoolPriceApi {
	const PREFIX: &'static [u8] = b"MockPoolPriceApi";
}

const POOL_PRICES: &[u8] = b"POOL_PRICES";

impl MockPoolPriceApi {
	pub fn set_pool_price(base_asset: Asset, quote_asset: Asset, price: Price) {
		Self::put_storage::<_, Price>(POOL_PRICES, (base_asset, quote_asset), price)
	}
}

impl PoolPriceProvider for MockPoolPriceApi {
	fn pool_price(base_asset: Asset, quote_asset: Asset) -> Result<PoolPrice, DispatchError> {
		let price = Self::get_storage::<_, Price>(POOL_PRICES, (base_asset, quote_asset))
			.unwrap_or_else(|| {
				panic!(
					"price should have been set for assets: {:?} -> {:?}",
					base_asset, quote_asset
				)
			});
		Ok(PoolPrice { sell: price, buy: price })
	}
}
