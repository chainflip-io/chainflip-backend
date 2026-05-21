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

use cf_primitives::Asset;
use sp_runtime::Permill;

use crate::{mocks::MockPalletStorage, NetworkFeeApi};

use super::MockPallet;

pub struct MockNetworkFeeApi;

impl MockPallet for MockNetworkFeeApi {
	const PREFIX: &'static [u8] = b"MockNetworkFeeApi";
}

const NETWORK_FEE_RATE: &[u8] = b"NETWORK_FEE_RATE";

impl MockNetworkFeeApi {
	pub fn set_network_fee_rate(rate: Permill) {
		Self::put_value(NETWORK_FEE_RATE, rate);
	}
}

impl NetworkFeeApi for MockNetworkFeeApi {
	fn get_network_fee_rate(
		_input_asset: Asset,
		_output_asset: Asset,
		_is_internal_swap: bool,
	) -> Permill {
		Self::get_value::<Permill>(NETWORK_FEE_RATE).unwrap_or_default()
	}
}
