use cf_chains::Chain;
use cf_primitives::{Asset, AssetAmount};
use frame_support::sp_runtime::traits::{UniqueSaturatedInto, Zero};

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
	fn estimate_swap_input_for_desired_output<C: Chain>(
		input_asset: C::ChainAsset,
		output_asset: C::ChainAsset,
		desired_output_amount: C::ChainAmount,
	) -> Option<C::ChainAmount> {
		Self::get_price(output_asset.into(), input_asset.into())
			.map(|price| (price * desired_output_amount.into()).unique_saturated_into())
	}

	fn calculate_input_for_gas_output<C: Chain>(
		input_asset: C::ChainAsset,
		desired_output_amount: C::ChainAmount,
	) -> Option<C::ChainAmount> {
		// The following check is copied from the implementation in the pool pallet
		if desired_output_amount.is_zero() {
			return Some(Zero::zero())
		}

		let input_asset = input_asset.into();
		let output_asset = C::GAS_ASSET.into();

		if input_asset == output_asset {
			return Some(desired_output_amount)
		}

		let required_input = Self::get_price(input_asset, output_asset)
			.map(|price| desired_output_amount.into() * price)?;

		Some(required_input.unique_saturated_into())
	}
}
