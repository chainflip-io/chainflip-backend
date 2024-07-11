use crate::{
	mocks::{MockPallet, MockPalletStorage},
	AssetWithholding,
};
use cf_primitives::{Asset, AssetAmount};
use sp_runtime::Saturating;

pub struct MockAssetWithholding;

impl MockPallet for MockAssetWithholding {
	const PREFIX: &'static [u8] = b"MockAssetWithholding";
}

impl MockAssetWithholding {
	pub fn withheld_assets(asset: Asset) -> AssetAmount {
		Self::get_storage::<Asset, AssetAmount>(WITHHELD_ASSETS, asset).unwrap_or_default()
	}
}

const WITHHELD_ASSETS: &[u8] = b"WITHHELD_ASSETS";

impl AssetWithholding for MockAssetWithholding {
	fn withhold_assets(asset: Asset, amount: AssetAmount) {
		Self::mutate_storage::<Asset, _, AssetAmount, _, _>(
			WITHHELD_ASSETS,
			&asset,
			|value: &mut Option<AssetAmount>| {
				value.get_or_insert_default().saturating_accrue(amount);
			},
		);
	}

	#[cfg(feature = "try-runtime")]
	fn withheld_assets(asset: Asset) -> AssetAmount {
		Self::withheld_assets(asset)
	}
}
