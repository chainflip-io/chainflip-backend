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
use cf_amm::math::Price;
use cf_primitives::Asset;

use crate::{PoolPrice, PoolPriceProvider};

use frame_support::sp_runtime::DispatchError;

use super::{MockPallet, MockPalletStorage};

pub struct MockPoolPriceApi {}

impl MockPallet for MockPoolPriceApi {
	const PREFIX: &'static [u8] = b"MockPoolPriceApi";
}

const POOL_PRICES: &[u8] = b"POOL_PRICES";

impl MockPoolPriceApi {
	pub fn set_pool_price(base_asset: Asset, quote_asset: Asset, price: Price) {
		Self::put_storage::<_, Price>(POOL_PRICES, (base_asset, quote_asset), price)
	}
}

impl PoolPriceProvider for MockPoolPriceApi {
	fn pool_price(base_asset: Asset, quote_asset: Asset) -> Result<PoolPrice, DispatchError> {
		let price = Self::get_storage::<_, Price>(POOL_PRICES, (base_asset, quote_asset))
			.ok_or(DispatchError::Other("MockPoolPriceApi: price not set"))?;
		Ok(PoolPrice { sell: price, buy: price })
	}
}
