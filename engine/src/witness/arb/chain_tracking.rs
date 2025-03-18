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

use crate::{
	evm::retry_rpc::{node_interface::NodeInterfaceRetryRpcApi, EvmRetryRpcApi},
	witness::common::chain_source::Header,
};

use cf_chains::arb::ArbitrumTrackedData;
use ethers::types::Bloom;

use super::super::common::chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData;
use ethers::types::{Bytes, H256};
use sp_core::H160;

#[async_trait::async_trait]
impl<T: EvmRetryRpcApi + NodeInterfaceRetryRpcApi + Send + Sync + Clone>
	GetTrackedData<cf_chains::Arbitrum, H256, Bloom> for T
{
	async fn get_tracked_data(
		&self,
		_header: &Header<<cf_chains::Arbitrum as cf_chains::Chain>::ChainBlockNumber, H256, Bloom>,
	) -> Result<<cf_chains::Arbitrum as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let (_, _, l2_base_fee, l1_base_fee_estimate) = self
			.gas_estimate_components(
				// Using zero address as a proxy destination address for the gas estimation.
				H160::default(),
				false,
				// Using empty data for the gas estimation
				Bytes::default(),
			)
			.await;

		Ok(ArbitrumTrackedData {
			base_fee: l2_base_fee.try_into().expect("Base fee should fit u128"),
			l1_base_fee_estimate: l1_base_fee_estimate
				.try_into()
				.expect("L1 Base fee should fit u128"),
		})
	}
}
