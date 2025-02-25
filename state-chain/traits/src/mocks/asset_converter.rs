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
	fn calculate_input_for_gas_output<C: Chain>(
		input_asset: C::ChainAsset,
		required_gas: C::ChainAmount,
	) -> Option<C::ChainAmount> {
		Self::calculate_input_for_desired_output(
			input_asset.into(),
			C::GAS_ASSET.into(),
			required_gas.into(),
			true,
		)
		.map(|amount| C::ChainAmount::try_from(amount).expect("Asset amount is for this chain"))
	}

	fn calculate_input_for_desired_output(
		input_asset: Asset,
		output_asset: Asset,
		desired_output_amount: AssetAmount,
		_with_network_fee: bool,
	) -> Option<AssetAmount> {
		// The following check is copied from the implementation in the swapping pallet
		if desired_output_amount.is_zero() {
			return Some(Zero::zero())
		}

		if input_asset == output_asset {
			return Some(desired_output_amount)
		}

		// Note: the network fee is not taken into account.
		let required_input = Self::get_price(input_asset, output_asset)
			.map(|price| desired_output_amount * price)?;

		Some(required_input.unique_saturated_into())
	}
}
