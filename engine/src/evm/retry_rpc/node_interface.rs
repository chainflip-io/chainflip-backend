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

use crate::evm::rpc::{node_interface::NodeInterfaceRpcApi, EvmRpcApi};

use super::EvmRetryRpcClient;

use crate::evm::retry_rpc::{RequestLog, MAX_RETRY_FOR_WITH_RESULT};

#[async_trait::async_trait]
pub trait NodeInterfaceRetryRpcApi {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> (u64, u64, U256, U256);
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + NodeInterfaceRpcApi> NodeInterfaceRetryRpcApi for EvmRetryRpcClient<Rpc> {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> (u64, u64, U256, U256) {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"gas_estimate_components".to_string(),
					Some(format!("{destination_address:?}, {contract_creation:?}")),
				),
				Box::pin(move |client| {
					let tx_data = tx_data.clone();
					Box::pin(async move {
						client
							.gas_estimate_components(
								destination_address,
								contract_creation,
								tx_data,
							)
							.await
					})
				}),
			)
			.await
	}
}

#[async_trait::async_trait]
pub trait NodeInterfaceRetryRpcApiWithResult {
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> anyhow::Result<(u64, u64, U256, U256)>;
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + NodeInterfaceRpcApi> NodeInterfaceRetryRpcApiWithResult
	for EvmRetryRpcClient<Rpc>
{
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> anyhow::Result<(u64, u64, U256, U256)> {
		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"gas_estimate_components".to_string(),
					Some(format!("{destination_address:?}, {contract_creation:?}")),
				),
				Box::pin(move |client| {
					let tx_data = tx_data.clone();
					Box::pin(async move {
						client
							.gas_estimate_components(
								destination_address,
								contract_creation,
								tx_data,
							)
							.await
					})
				}),
				MAX_RETRY_FOR_WITH_RESULT,
			)
			.await
	}
}
