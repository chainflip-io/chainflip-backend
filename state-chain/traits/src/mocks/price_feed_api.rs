use cf_primitives::{Asset, Price};

use crate::{OraclePrice, PriceFeedApi};

use super::{MockPallet, MockPalletStorage};

pub struct MockPriceFeedApi {}

impl MockPriceFeedApi {}

impl MockPallet for MockPriceFeedApi {
	const PREFIX: &'static [u8] = b"MockPriceFeedApi";
}

const ORACLE_PRICE: &[u8] = b"ORACLE_PRICE";
const ORACLE_STALE: &[u8] = b"ORACLE_STALE";

impl MockPriceFeedApi {
	pub fn set_price(asset: cf_primitives::Asset, price: Option<Price>) {
		Self::put_storage(ORACLE_PRICE, asset, price);
	}

	pub fn set_stale(asset: cf_primitives::Asset, stale: bool) {
		Self::put_storage(ORACLE_STALE, asset, stale);
	}
}

impl PriceFeedApi for MockPriceFeedApi {
	fn get_price(asset: Asset) -> Option<OraclePrice> {
		let stale = Self::get_storage::<_, bool>(ORACLE_STALE, asset).unwrap_or_default();
		Self::get_storage::<_, Option<Price>>(ORACLE_PRICE, asset)
			.and_then(|price| price.map(|price| OraclePrice { price, stale }))
	}
}
