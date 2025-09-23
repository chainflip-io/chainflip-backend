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

use crate::{mocks::price_feed_api::MockPriceFeedApi, AssetConverter, PriceFeedApi};
use cf_amm::math::output_amount_ceil;
use cf_chains::Chain;
use cf_primitives::{Asset, AssetAmount};
use frame_support::sp_runtime::{
	helpers_128bit::multiply_by_rational_with_rounding,
	traits::{UniqueSaturatedInto, Zero},
	Rounding::Up,
};

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

		Self::calculate_input_for_desired_output(
			input_asset_generic,
			C::GAS_ASSET.into(),
			required_gas.into(),
			true,
		)
		.and_then(|amount| C::ChainAmount::try_from(amount).ok())
		.unwrap_or_else(|| {
			Self::input_asset_amount_using_reference_gas_asset_price::<C>(input_asset, required_gas)
		})
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

	fn input_asset_amount_using_reference_gas_asset_price<C: Chain>(
		input_asset: C::ChainAsset,
		required_gas: C::ChainAmount,
	) -> C::ChainAmount {
		if input_asset == C::GAS_ASSET {
			return required_gas;
		}
		match Into::<Asset>::into(input_asset) {
			Asset::ArbUsdc |
			Asset::SolUsdc |
			Asset::Usdt |
			Asset::Usdc |
			Asset::HubUsdc |
			Asset::HubUsdt => {
				if let Some(relative_price) =
					MockPriceFeedApi::get_relative_price(C::GAS_ASSET.into(), input_asset.into())
				{
					output_amount_ceil(required_gas.into(), relative_price.price)
						.try_into()
						.unwrap_or(0u32.into())
				} else {
					multiply_by_rational_with_rounding(
						required_gas.into(),
						C::NATIVE_TOKEN_PRICE_IN_FINE_USD.into(),
						C::SMALLEST_UNIT_PER_UNIT.into(),
						Up,
					)
					.and_then(|x| x.try_into().ok())
					.unwrap_or(0u32.into())
				}
			},
			Asset::Flip => multiply_by_rational_with_rounding(
				required_gas.into(),
				MockPriceFeedApi::get_price(C::GAS_ASSET.into())
					.and_then(|price| {
						output_amount_ceil(C::SMALLEST_UNIT_PER_UNIT.into(), price.price)
							.try_into()
							.ok()
					})
					.unwrap_or(C::NATIVE_TOKEN_PRICE_IN_FINE_USD.into()),
				cf_chains::eth::REFERENCE_FLIP_PRICE_IN_USD,
				Up,
			)
			.and_then(|x| x.try_into().ok())
			.unwrap_or(0u32.into()),
			_ => 0u32.into(),
		}
	}
}
