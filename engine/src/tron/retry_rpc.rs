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
	evm::rpc::EvmRpcApi,
	retrier::{RequestLog, RetrierClient, MAX_RPC_RETRY_DELAY},
	settings::NodeContainer,
};
use cf_utilities::{redact_endpoint_secret::SecretUrl, task_scope::Scope};
use core::time::Duration;
use ethers::types::U256;

use anyhow::Result;

use super::{
	rpc::{TronRpcApi, TronRpcClient},
	rpc_client_api::{BlockBalance, TransactionInfo},
};

#[derive(Clone)]
pub struct TronRetryRpcClient {
	rpc_retry_client: RetrierClient<TronRpcClient>,
	witness_period: u64,
}

const TRON_RPC_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

pub struct TronEndpoints {
	pub http_endpoint: SecretUrl,
	pub json_rpc_endpoint: SecretUrl,
}

impl TronRetryRpcClient {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<TronEndpoints>,
		expected_chain_id: u64,
		witness_period: u64,
	) -> Result<Self> {
		let rpc_client = TronRpcClient::new(
			nodes.primary.http_endpoint,
			nodes.primary.json_rpc_endpoint,
			expected_chain_id,
			"Tron",
		)?;

		let backup_rpc_client = nodes
			.backup
			.map(|backup_endpoint| {
				TronRpcClient::new(
					backup_endpoint.http_endpoint,
					backup_endpoint.json_rpc_endpoint,
					expected_chain_id,
					"Tron",
				)
			})
			.transpose()?;

		Ok(Self {
			rpc_retry_client: RetrierClient::new(
				scope,
				"tron_rpc",
				rpc_client,
				backup_rpc_client,
				TRON_RPC_TIMEOUT,
				MAX_RPC_RETRY_DELAY,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			witness_period,
		})
	}
}

#[async_trait::async_trait]
pub trait TronRetryRpcApi: Clone {
	async fn chain_id(&self) -> U256;
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> TransactionInfo;
	async fn get_block_balances(&self, block_number: u64, hash: &str) -> BlockBalance;
}

#[async_trait::async_trait]
impl TronRetryRpcApi for TronRetryRpcClient {
	async fn chain_id(&self) -> U256 {
		self.rpc_retry_client
			.request(
				RequestLog::new("eth_chainId".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.chain_id().await })),
			)
			.await
	}

	async fn get_transaction_info_by_id(&self, tx_id: &str) -> TransactionInfo {
		let tx_id = tx_id.to_owned();
		self.rpc_retry_client
			.request(
				RequestLog::new("getTransactionInfoById".to_string(), Some(format!("{:?}", tx_id))),
				Box::pin(move |client| {
					let tx_id = tx_id.clone();
					Box::pin(async move { client.get_transaction_info_by_id(&tx_id).await })
				}),
			)
			.await
	}

	async fn get_block_balances(&self, block_number: u64, hash: &str) -> BlockBalance {
		let hash = hash.to_owned();
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"getBlockBalance".to_string(),
					Some(format!("num: {}, hash: {}", block_number, hash)),
				),
				Box::pin(move |client| {
					let hash = hash.clone();
					Box::pin(async move { client.get_block_balances(block_number, &hash).await })
				}),
			)
			.await
	}
}
