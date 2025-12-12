// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::AssetConverter;
use cf_chains::Chain;
use cf_primitives::{Asset, AssetAmount};
use frame_support::sp_runtime::traits::{UniqueSaturatedInto, Zero};

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
	) -> C::ChainAmount {
		let input_asset_generic: Asset = input_asset.into();

		C::ChainAmount::try_from(Self::calculate_input_for_desired_output(
			input_asset_generic,
			C::GAS_ASSET.into(),
			required_gas.into(),
			true,
			false,
		))
		.unwrap()
	}

	fn calculate_input_for_desired_output(
		input_asset: Asset,
		output_asset: Asset,
		desired_output_amount: AssetAmount,
		_with_network_fee: bool,
		_is_internal: bool,
	) -> AssetAmount {
		// The following check is copied from the implementation in the swapping pallet
		if desired_output_amount.is_zero() {
			return 0;
		}

		if input_asset == output_asset {
			return desired_output_amount;
		}

		// Note: the network fee is not taken into account.
		let required_input = Self::get_price(input_asset, output_asset)
			.map(|price| desired_output_amount * price)
			.unwrap_or_else(|| {
				panic!("Price must be set in the mock asset converter. {input_asset:?} to {output_asset:?}")
			});

		required_input.unique_saturated_into()
	}
}
