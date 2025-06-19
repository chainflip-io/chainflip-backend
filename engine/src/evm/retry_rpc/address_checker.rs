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

use crate::evm::rpc::{
	address_checker::{AddressCheckerRpcApi, *},
	EvmRpcApi,
};

use super::EvmRetryRpcClient;

use crate::evm::retry_rpc::RequestLog;

#[async_trait::async_trait]
pub trait AddressCheckerRetryRpcApi {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<AddressState>;

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<U256>;

	async fn query_price_feeds(
		&self,
		contract_address: H160,
		aggregator_addresses: Vec<H160>,
	) -> (U256, U256, Vec<PriceFeedData>);
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + AddressCheckerRpcApi> AddressCheckerRetryRpcApi for EvmRetryRpcClient<Rpc> {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<AddressState> {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"address_states".to_string(),
					Some(format!("{block_hash:?}, {contract_address:?}")),
				),
				Box::pin(move |client| {
					let addresses = addresses.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.address_states(block_hash, contract_address, addresses).await
					})
				}),
			)
			.await
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> Vec<U256> {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"balances".to_string(),
					Some(format!("{block_hash:?}, {contract_address:?}")),
				),
				Box::pin(move |client| {
					let addresses = addresses.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.balances(block_hash, contract_address, addresses).await
					})
				}),
			)
			.await
	}

	async fn query_price_feeds(
		&self,
		contract_address: H160,
		aggregator_addresses: Vec<H160>,
	) -> (U256, U256, Vec<PriceFeedData>) {
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"query_price_feeds".to_string(),
					Some(format!("{contract_address:?}, {aggregator_addresses:?}")),
				),
				Box::pin(move |client| {
					let aggregator_addresses = aggregator_addresses.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.query_price_feeds(contract_address, aggregator_addresses).await
					})
				}),
			)
			.await
	}
}
