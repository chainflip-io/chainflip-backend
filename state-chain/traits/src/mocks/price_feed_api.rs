use cf_amm::math::Price;
use cf_primitives::{Asset, AssetAmount};

use super::{MockPallet, MockPalletStorage};
use crate::{OraclePrice, PriceFeedApi};
pub struct MockPriceFeedApi {}

impl MockPriceFeedApi {}

impl MockPallet for MockPriceFeedApi {
	const PREFIX: &'static [u8] = b"MockPriceFeedApi";
}

const ORACLE_PRICE: &[u8] = b"ORACLE_PRICE";
const ORACLE_STALE: &[u8] = b"ORACLE_STALE";

impl MockPriceFeedApi {
	pub fn set_price(asset: Asset, price: Option<Price>) {
		Self::put_storage(ORACLE_PRICE, asset, price);
	}

	pub fn set_stale(asset: cf_primitives::Asset, stale: bool) {
		Self::put_storage(ORACLE_STALE, asset, stale);
	}

	/// A helper function to update asset prices (in atomic USD units)
	pub fn set_price_usd_fine(asset: cf_primitives::Asset, price_usd_fine: AssetAmount) {
		todo!()
		// Self::set_price(asset, Some(Price::from_usd_fine_amount(price_usd_fine)));
	}

	/// Sets the price of an asset in whole USD units, automatically scaling to the asset's
	/// decimals.
	pub fn set_price_usd(asset: cf_primitives::Asset, dollar_amount: u32) {
		todo!()
		// Self::set_price(asset, Some(Price::from_usd(asset, dollar_amount)));
	}
}

impl PriceFeedApi for MockPriceFeedApi {
	fn get_price(asset: Asset) -> Option<OraclePrice> {
		let stale = Self::get_storage::<_, bool>(ORACLE_STALE, asset).unwrap_or_default();
		Self::get_storage::<_, Option<Price>>(ORACLE_PRICE, asset)
			.and_then(|price| price.map(|price| OraclePrice { price, stale }))
	}

	#[cfg(any(feature = "runtime-benchmarks", feature = "runtime-integration-tests"))]
	fn set_price(asset: cf_primitives::Asset, price: Price) {
		Self::set_price(asset, Some(price));
	}
}
