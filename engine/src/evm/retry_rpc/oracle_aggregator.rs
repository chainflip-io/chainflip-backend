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

use ethers::prelude::*;

use crate::evm::rpc::{oracle_aggregator::AggregatorV3InterfaceRpcApi, EvmRpcApi};

use super::EvmRetryRpcClient;

use crate::evm::retry_rpc::RequestLog;

#[async_trait::async_trait]
pub trait AggregatorV3InterfaceRetryRpcApi {
	async fn latest_round_data(&self, aggregator_address: H160) -> (u128, I256, U256, U256, u128);
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + AggregatorV3InterfaceRpcApi> AggregatorV3InterfaceRetryRpcApi
	for EvmRetryRpcClient<Rpc>
{
	async fn latest_round_data(&self, aggregator_address: H160) -> (u128, I256, U256, U256, u128) {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"latest_round_data".to_string(),
					Some(format!("{aggregator_address:?}")),
				),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.latest_round_data(aggregator_address).await })
				}),
			)
			.await
	}
}
