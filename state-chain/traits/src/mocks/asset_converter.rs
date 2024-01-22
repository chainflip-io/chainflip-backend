use cf_primitives::{Asset, AssetAmount};
use frame_support::sp_runtime::traits::UniqueSaturatedInto;
use sp_runtime::traits::AtLeast32BitUnsigned;

use crate::AssetConverter;

use super::{MockPallet, MockPalletStorage};

pub struct MockAssetConverter;

impl MockPallet for MockAssetConverter {
	const PREFIX: &'static [u8] = b"MockAssetConverter";
}

impl MockAssetConverter {
	pub fn set_price(source_asset: Asset, destination_asset: Asset, price: AssetAmount) {
		Self::put_storage(b"PRICES", (source_asset, destination_asset), price);
	}

	pub fn get_price(source_asset: Asset, destination_asset: Asset) -> Option<AssetAmount> {
		Self::get_storage::<_, AssetAmount>(b"PRICES", (source_asset, destination_asset))
	}
}

impl AssetConverter for MockAssetConverter {
	fn convert_asset_to_approximate_output<
		Amount: Into<AssetAmount> + AtLeast32BitUnsigned + Copy,
	>(
		input_asset: impl Into<Asset>,
		available_input_amount: Amount,
		output_asset: impl Into<Asset>,
		desired_output_amount: Amount,
	) -> Option<(Amount, Amount)> {
		let input_asset = input_asset.into();
		let output_asset = output_asset.into();

		let available_input_amount = available_input_amount.into();
		let desired_output_amount = desired_output_amount.into();

		let input = Self::get_price(output_asset, input_asset)
			.map(|price| price * desired_output_amount)?;

		if input > available_input_amount {
			return None
		}

		let fee_estimate = Self::get_price(input_asset, output_asset).map(|price| price * input)?;

		Some((
			available_input_amount.saturating_sub(fee_estimate).unique_saturated_into(),
			fee_estimate.unique_saturated_into(),
		))
	}
}
