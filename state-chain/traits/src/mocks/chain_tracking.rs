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

use super::MockPallet;
use crate::mocks::MockPalletStorage;
use cf_chains::Chain;

use crate::{AdjustedFeeEstimationApi, GetBlockHeight};

use super::{
	block_height_provider::BlockHeightProvider, tracked_data_provider::TrackedDataProvider,
};

pub struct ChainTracker<C: Chain>(BlockHeightProvider<C>, TrackedDataProvider<C>);

impl<C: Chain> MockPallet for ChainTracker<C> {
	const PREFIX: &'static [u8] = b"MockChainTrackerProvider";
}

const TRACKED_FEE_KEY: &[u8] = b"TRACKED_FEE_DATA";

impl<C: Chain> ChainTracker<C> {
	pub fn set_fee(fee: C::ChainAmount) {
		Self::put_value(TRACKED_FEE_KEY, fee);
	}
}

impl<C: Chain> GetBlockHeight<C> for ChainTracker<C> {
	fn get_block_height() -> C::ChainBlockNumber {
		BlockHeightProvider::<C>::get_block_height()
	}
}

impl<C: Chain> AdjustedFeeEstimationApi<C> for ChainTracker<C> {
	fn estimate_ingress_fee(_asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value(TRACKED_FEE_KEY).unwrap_or_default()
	}

	fn estimate_ingress_fee_vault_swap() -> Option<C::ChainAmount> {
		Self::get_value(TRACKED_FEE_KEY)
	}

	fn estimate_egress_fee(_asset: C::ChainAsset) -> C::ChainAmount {
		Self::get_value(TRACKED_FEE_KEY).unwrap_or_default()
	}

	fn estimate_ccm_fee(
		_asset: C::ChainAsset,
		_gas_budget: cf_primitives::GasAmount,
		_message_length: usize,
	) -> Option<C::ChainAmount> {
		Self::get_value(TRACKED_FEE_KEY)
	}
}
