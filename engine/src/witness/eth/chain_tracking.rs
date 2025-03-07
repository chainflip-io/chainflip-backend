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

use crate::{evm::retry_rpc::EvmRetryRpcApi, witness::common::chain_source::Header};
use cf_chains::eth::EthereumTrackedData;
use cf_utilities::context;
use ethers::types::Bloom;
use sp_core::U256;

use super::super::common::chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData;
use ethers::types::H256;

#[async_trait::async_trait]
impl<T: EvmRetryRpcApi + Send + Sync + Clone> GetTrackedData<cf_chains::Ethereum, H256, Bloom>
	for T
{
	async fn get_tracked_data(
		&self,
		header: &Header<<cf_chains::Ethereum as cf_chains::Chain>::ChainBlockNumber, H256, Bloom>,
	) -> Result<<cf_chains::Ethereum as cf_chains::Chain>::TrackedData, anyhow::Error> {
		const PRIORITY_FEE_PERCENTILE: f64 = 50.0;
		let fee_history = self
			.fee_history(U256::one(), header.index.into(), vec![PRIORITY_FEE_PERCENTILE])
			.await;

		Ok(EthereumTrackedData {
			base_fee: (*context!(fee_history.base_fee_per_gas.first())?)
				.try_into()
				.expect("Base fee should fit u128"),
			priority_fee: (*context!(context!(fee_history.reward.first())?.first())?)
				.try_into()
				.expect("Priority fee should fit u128"),
		})
	}
}
