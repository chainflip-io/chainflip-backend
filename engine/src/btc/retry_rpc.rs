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

use bitcoin::{BlockHash, Txid};
use cf_utilities::task_scope::Scope;

use crate::{
	btc::rpc::MempoolInfo,
	retrier::{Attempt, RequestLog, RetrierClient, MAX_RPC_RETRY_DELAY},
	settings::{HttpBasicAuthEndpoint, NodeContainer},
	witness::common::chain_source::{ChainClient, Header},
};
use cf_chains::{btc::BitcoinNetwork, Bitcoin};
use core::time::Duration;

use anyhow::Result;
use cf_chains::btc::{BlockNumber, BtcAmount};

use super::rpc::{BlockHeader, BtcRpcApi, BtcRpcClient, MempoolTransaction, VerboseBlock};

#[derive(Clone)]
pub struct BtcRetryRpcClient {
	retry_client: RetrierClient<BtcRpcClient>,
}

const BITCOIN_RPC_TIMEOUT: Duration = Duration::from_millis(4 * 1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

const MAX_BROADCAST_RETRIES: Attempt = 2;

impl BtcRetryRpcClient {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<HttpBasicAuthEndpoint>,
		expected_btc_network: BitcoinNetwork,
	) -> Result<Self> {
		let rpc_client = BtcRpcClient::new(nodes.primary, Some(expected_btc_network))?;

		let backup_rpc_client = nodes
			.backup
			.map(|backup_endpoint| BtcRpcClient::new(backup_endpoint, Some(expected_btc_network)))
			.transpose()?;

		Ok(Self {
			retry_client: RetrierClient::new(
				scope,
				"btc_rpc",
				rpc_client,
				backup_rpc_client,
				BITCOIN_RPC_TIMEOUT,
				MAX_RPC_RETRY_DELAY,
				MAX_CONCURRENT_SUBMISSIONS,
			),
		})
	}
}

#[async_trait::async_trait]
impl BtcRpcApi for BtcRetryRpcClient {
	async fn block(&self, block_hash: BlockHash) -> Result<VerboseBlock> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("block".to_string(), Some(format!("{block_hash}"))),
				Box::pin(move |client| Box::pin(async move { client.block(block_hash).await })),
				2,
			)
			.await
	}

	async fn block_hash(&self, block_number: BlockNumber) -> Result<BlockHash> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("block_hash".to_string(), Some(format!("{block_number}"))),
				Box::pin(move |client| {
					Box::pin(async move { client.block_hash(block_number).await })
				}),
				2,
			)
			.await
	}

	async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> Result<Txid> {
		self.retry_client
			.request_with_limit(
				RequestLog::new(
					"send_raw_transaction".to_string(),
					Some(format!("{transaction_bytes:?}")),
				),
				Box::pin(move |client| {
					let transaction_bytes = transaction_bytes.clone();
					Box::pin(async move { client.send_raw_transaction(transaction_bytes).await })
				}),
				MAX_BROADCAST_RETRIES,
			)
			.await
	}

	async fn next_block_fee_rate(&self) -> Result<Option<BtcAmount>> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("next_block_fee_rate".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.next_block_fee_rate().await })),
				2,
			)
			.await
	}

	async fn average_block_fee_rate(&self, block_hash: BlockHash) -> Result<BtcAmount> {
		self.retry_client
			.request_with_limit(
				RequestLog::new(
					"average_block_fee_rate".to_string(),
					Some(format!("{block_hash}")),
				),
				Box::pin(move |client| {
					Box::pin(async move { client.average_block_fee_rate(block_hash).await })
				}),
				2,
			)
			.await
	}

	async fn best_block_hash(&self) -> Result<BlockHash> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("best_block_hash".to_string(), None),
				Box::pin(move |client| {
					Box::pin(async move {
						let best_block_hash = client.best_block_hash().await?;
						Ok(best_block_hash)
					})
				}),
				2,
			)
			.await
	}

	async fn block_header(&self, block_hash: BlockHash) -> Result<BlockHeader> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("block_header".to_string(), Some(block_hash.to_string())),
				Box::pin(move |client| {
					Box::pin(async move {
						let header = client.block_header(block_hash).await?;
						Ok(header)
					})
				}),
				2,
			)
			.await
	}

	async fn mempool_info(&self) -> anyhow::Result<MempoolInfo> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("mempool_info".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.mempool_info().await })),
				2,
			)
			.await
	}

	async fn raw_mempool(&self) -> anyhow::Result<Vec<Txid>> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("raw_mempool".to_string(), None),
				Box::pin(move |client| Box::pin(async move { client.raw_mempool().await })),
				2,
			)
			.await
	}

	async fn mempool_entries(&self, txids: Vec<Txid>) -> anyhow::Result<Vec<MempoolTransaction>> {
		self.retry_client
			.request_with_limit(
				RequestLog::new("raw_transaction".to_string(), Some(txids.len().to_string())),
				Box::pin(move |client| {
					let txids = txids.clone();
					Box::pin(async move { client.mempool_entries(txids).await })
				}),
				2,
			)
			.await
	}
}

#[async_trait::async_trait]
impl ChainClient for BtcRetryRpcClient {
	type Index = <Bitcoin as cf_chains::Chain>::ChainBlockNumber;
	type Hash = BlockHash;
	type Data = ();

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		self.retry_client
			.request(
				RequestLog::new("header_at_index".to_string(), Some(format!("{index}"))),
				Box::pin(move |client| {
					Box::pin(async move {
						let block_hash = client.block_hash(index).await?;
						let block_header = client.block_header(block_hash).await?;
						assert_eq!(block_header.height, index);

						Ok(Header {
							index,
							hash: block_hash,
							parent_hash: block_header.previous_block_hash,
							data: (),
						})
					})
				}),
			)
			.await
	}
}

#[cfg(test)]
pub mod mocks {

	use super::*;
	use mockall::mock;

	mock! {
		pub BtcRetryRpcClient {}

		impl Clone for BtcRetryRpcClient {
			fn clone(&self) -> Self;
		}

		#[async_trait::async_trait]
		impl BtcRpcApi for BtcRetryRpcClient {
			async fn block(&self, block_hash: BlockHash) -> anyhow::Result<VerboseBlock>;

			async fn block_hash(&self, block_number: cf_chains::btc::BlockNumber) -> anyhow::Result<BlockHash>;

			async fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> anyhow::Result<Txid>;

			async fn next_block_fee_rate(&self) -> anyhow::Result<Option<cf_chains::btc::BtcAmount>>;

			async fn average_block_fee_rate(&self, block_hash: BlockHash) -> anyhow::Result<cf_chains::btc::BtcAmount>;

			async fn block_header(
				&self,
				block_hash: BlockHash,
			) -> anyhow::Result<BlockHeader>;

			async fn best_block_hash(&self) -> anyhow::Result<BlockHash>;

			async fn mempool_info(&self) -> anyhow::Result<MempoolInfo>;

			async fn raw_mempool(&self) -> anyhow::Result<Vec<Txid>>;

			async fn mempool_entries(&self, txids: Vec<Txid>) -> anyhow::Result<Vec<MempoolTransaction>>;
		}
	}
}
