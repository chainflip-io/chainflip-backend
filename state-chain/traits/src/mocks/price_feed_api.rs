use cf_primitives::{Asset, Price};

use crate::{OraclePrice, PriceFeedApi};

use super::{MockPallet, MockPalletStorage};

pub struct MockPriceFeedApi {}

impl MockPriceFeedApi {}

impl MockPallet for MockPriceFeedApi {
	const PREFIX: &'static [u8] = b"MockPriceFeedApi";
}

const ORACLE_PRICE: &[u8] = b"ORACLE_PRICE";

impl MockPriceFeedApi {
	pub fn set_price(asset: cf_primitives::Asset, price: Option<Price>) {
		Self::put_storage(ORACLE_PRICE, asset, price);
	}
}

impl PriceFeedApi for MockPriceFeedApi {
	fn get_price(asset: Asset) -> Option<OraclePrice> {
		Self::get_storage(ORACLE_PRICE, asset).map(|price| OraclePrice { price, stale: false })
	}
}
