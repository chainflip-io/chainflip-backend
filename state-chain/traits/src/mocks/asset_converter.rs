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
	fn estimate_swap_input_for_desired_output<
		Amount: Into<AssetAmount> + AtLeast32BitUnsigned + Copy,
	>(
		input_asset: impl Into<Asset>,
		output_asset: impl Into<Asset>,
		desired_output_amount: Amount,
	) -> Option<Amount> {
		Self::get_price(output_asset.into(), input_asset.into())
			.map(|price| (price * desired_output_amount.into()).unique_saturated_into())
	}

	fn calculate_asset_conversion<Amount: Into<AssetAmount> + AtLeast32BitUnsigned + Copy>(
		input_asset: impl Into<Asset>,
		available_input_amount: Amount,
		output_asset: impl Into<Asset>,
		desired_output_amount: Amount,
	) -> Option<Amount> {
		// The following check is copied from the implementation in the pool pallet
		if desired_output_amount.is_zero() {
			return Some(Amount::zero())
		}
		if available_input_amount.is_zero() {
			return None
		}

		let input_asset = input_asset.into();
		let output_asset = output_asset.into();
		if input_asset == output_asset {
			return Some(available_input_amount.saturating_sub(desired_output_amount))
		}

		let required_input = Self::get_price(input_asset, output_asset)
			.map(|price| desired_output_amount.into() / price)?;

		if required_input > available_input_amount.into() {
			return Some(available_input_amount)
		}

		Some(required_input.unique_saturated_into())
	}
}
