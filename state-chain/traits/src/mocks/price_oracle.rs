use cf_primitives::{Asset, AssetAmount};

use crate::PriceOracle;

use super::{MockPallet, MockPalletStorage};

pub struct MockPriceOracle;

impl MockPallet for MockPriceOracle {
	const PREFIX: &'static [u8] = b"MockPriceOracle";
}

impl MockPriceOracle {
	pub fn set_price(source_asset: Asset, destination_asset: Asset, price: AssetAmount) {
		Self::put_storage(b"PRICES", (source_asset, destination_asset), price);
	}

	pub fn get_price(source_asset: Asset, destination_asset: Asset) -> Option<AssetAmount> {
		Self::get_storage::<_, AssetAmount>(b"PRICES", (source_asset, destination_asset))
	}
}

impl PriceOracle for MockPriceOracle {
	fn convert_asset_value(
		source_asset: impl Into<Asset>,
		destination_asset: impl Into<Asset>,
		source_amount: impl Into<AssetAmount>,
	) -> Option<AssetAmount> {
		Self::get_price(source_asset.into(), destination_asset.into())
			.map(|price| price * source_amount.into())
	}
}
