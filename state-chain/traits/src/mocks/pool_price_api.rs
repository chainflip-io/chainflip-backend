use cf_primitives::{Asset, Price};

use crate::{PoolPrice, PoolPriceProvider};

use frame_support::sp_runtime::DispatchError;

use super::{MockPallet, MockPalletStorage};

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
			.expect("price should have been set");
		Ok(PoolPrice { sell: price, buy: price })
	}
}
