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

use sp_std::marker::PhantomData;

use cf_chains::{Chain, FeeEstimationApi};

use super::MockPallet;
use crate::mocks::MockPalletStorage;

pub struct TrackedDataProvider<C: Chain>(PhantomData<C>);

impl<C: Chain> MockPallet for TrackedDataProvider<C> {
	const PREFIX: &'static [u8] = b"MockTrackedDataProvider";
}

const TRACKED_DATA_KEY: &[u8] = b"TRACKED_DATA";

impl<C: Chain> TrackedDataProvider<C> {
	pub fn set_tracked_data(height: C::TrackedData) {
		Self::put_value(TRACKED_DATA_KEY, height);
	}
}

impl<C: Chain> FeeEstimationApi<C> for TrackedDataProvider<C> {
	fn estimate_ingress_fee(&self, asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_ingress_fee(asset)
	}

	fn estimate_ingress_fee_vault_swap(&self) -> Option<<C as Chain>::ChainAmount> {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_ingress_fee_vault_swap()
	}

	fn estimate_egress_fee(&self, asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_egress_fee(asset)
	}

	fn estimate_ccm_fee(
		&self,
		asset: <C as Chain>::ChainAsset,
		gas_budget: cf_primitives::GasAmount,
		message_length: usize,
	) -> Option<<C as Chain>::ChainAmount> {
		Self::get_value::<C::TrackedData>(TRACKED_DATA_KEY)
			.expect("TrackedData must be set explicitly in mocks")
			.estimate_ccm_fee(asset, gas_budget, message_length)
	}
}
