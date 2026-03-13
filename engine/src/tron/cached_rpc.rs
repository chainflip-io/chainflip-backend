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
	caching_request::CachingRequest,
	evm::rpc::EvmRpcApi,
	retrier::Attempt,
	tron::{
		retry_rpc::{TronRetryRpcClient, TronRetrySigningRpcApi},
		rpc::{TronRpcApi, TronSigningRpcApi},
		rpc_client_api::{
			BlockBalance, BlockNumber, Transaction as TronTransaction, TransactionInfo,
		},
	},
};
use cf_utilities::task_scope::Scope;
use ethers::types::{Block, Log, TransactionReceipt, H160, H256, U256, U64};
use tokio::sync::mpsc;

pub const MAX_RETRY_FOR_WITH_RESULT: Attempt = 2;

/// Finite-retry version of the Tron RPC API — methods return `anyhow::Result<T>` so callers can
/// handle errors instead of looping forever. Used by the caching client and for election-based
/// witnessing where a single failed attempt should be observable.
#[async_trait::async_trait]
pub trait TronRetryRpcApiWithResult: Clone {
	// Tron HTTP API
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo>;
	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<TronTransaction>;
	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance>;

	// EVM-compatible JSON-RPC (via Tron's JSON-RPC endpoint)
	async fn chain_id(&self) -> anyhow::Result<U256>;
	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> anyhow::Result<Vec<Log>>;
	async fn transaction_receipt(&self, tx_hash: H256) -> anyhow::Result<TransactionReceipt>;
	async fn block(&self, block_number: U64) -> anyhow::Result<Block<H256>>;
	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<Block<H256>>;
	async fn block_with_txs(
		&self,
		block_number: U64,
	) -> anyhow::Result<Block<ethers::types::Transaction>>;
	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<ethers::types::Transaction>;
	async fn get_block_number(&self) -> anyhow::Result<U64>;
}

/// Wraps a [`TronRetryRpcClient`] and deduplicates in-flight requests with the same key,
/// returning a cached result for subsequent callers until the cache is invalidated.
#[derive(Clone)]
pub struct TronCachingClient<Rpc: TronRpcApi> {
	retry_client: TronRetryRpcClient<Rpc>,
	get_transaction_info_by_id: CachingRequest<String, TransactionInfo, TronRetryRpcClient<Rpc>>,
	get_transaction_by_id: CachingRequest<String, TronTransaction, TronRetryRpcClient<Rpc>>,
	get_block_balances:
		CachingRequest<(BlockNumber, String), BlockBalance, TronRetryRpcClient<Rpc>>,
	chain_id: CachingRequest<(), U256, TronRetryRpcClient<Rpc>>,
	get_logs: CachingRequest<(H256, H160), Vec<Log>, TronRetryRpcClient<Rpc>>,
	transaction_receipt: CachingRequest<H256, TransactionReceipt, TronRetryRpcClient<Rpc>>,
	block: CachingRequest<U64, Block<H256>, TronRetryRpcClient<Rpc>>,
	block_by_hash: CachingRequest<H256, Block<H256>, TronRetryRpcClient<Rpc>>,
	block_with_txs: CachingRequest<U64, Block<ethers::types::Transaction>, TronRetryRpcClient<Rpc>>,
	get_transaction: CachingRequest<H256, ethers::types::Transaction, TronRetryRpcClient<Rpc>>,
	get_block_number: CachingRequest<(), U64, TronRetryRpcClient<Rpc>>,
	pub cache_invalidation_senders: Vec<mpsc::Sender<()>>,
}

impl<Rpc: TronRpcApi + EvmRpcApi> TronCachingClient<Rpc> {
	pub fn new(scope: &Scope<'_, anyhow::Error>, client: TronRetryRpcClient<Rpc>) -> Self {
		let (get_transaction_info_by_id, get_transaction_info_by_id_cache) =
			CachingRequest::<String, TransactionInfo, TronRetryRpcClient<Rpc>>::new(
				scope,
				client.clone(),
			);
		let (get_transaction_by_id, get_transaction_by_id_cache) = CachingRequest::<
			String,
			TronTransaction,
			TronRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (get_block_balances, get_block_balances_cache) = CachingRequest::<
			(BlockNumber, String),
			BlockBalance,
			TronRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (chain_id, chain_id_cache) =
			CachingRequest::<(), U256, TronRetryRpcClient<Rpc>>::new(scope, client.clone());
		let (get_logs, get_logs_cache) = CachingRequest::<
			(H256, H160),
			Vec<Log>,
			TronRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (transaction_receipt, transaction_receipt_cache) = CachingRequest::<
			H256,
			TransactionReceipt,
			TronRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (block, block_cache) =
			CachingRequest::<U64, Block<H256>, TronRetryRpcClient<Rpc>>::new(scope, client.clone());
		let (block_by_hash, block_by_hash_cache) = CachingRequest::<
			H256,
			Block<H256>,
			TronRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (block_with_txs, block_with_txs_cache) = CachingRequest::<
			U64,
			Block<ethers::types::Transaction>,
			TronRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (get_transaction, get_transaction_cache) = CachingRequest::<
			H256,
			ethers::types::Transaction,
			TronRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (get_block_number, get_block_number_cache) =
			CachingRequest::<(), U64, TronRetryRpcClient<Rpc>>::new(scope, client.clone());

		TronCachingClient {
			retry_client: client,
			get_transaction_info_by_id,
			get_transaction_by_id,
			get_block_balances,
			chain_id,
			get_logs,
			transaction_receipt,
			block,
			block_by_hash,
			block_with_txs,
			get_transaction,
			get_block_number,
			cache_invalidation_senders: vec![
				get_transaction_info_by_id_cache,
				get_transaction_by_id_cache,
				get_block_balances_cache,
				chain_id_cache,
				get_logs_cache,
				transaction_receipt_cache,
				block_cache,
				block_by_hash_cache,
				block_with_txs_cache,
				get_transaction_cache,
				get_block_number_cache,
			],
		}
	}
}

#[async_trait::async_trait]
impl<Rpc: TronRpcApi + EvmRpcApi> TronRetryRpcApiWithResult for TronCachingClient<Rpc> {
	async fn get_transaction_info_by_id(&self, tx_id: &str) -> anyhow::Result<TransactionInfo> {
		let tx_id = tx_id.to_owned();
		self.get_transaction_info_by_id
			.get_or_fetch(
				{
					let tx_id = tx_id.clone();
					Box::pin(move |client| {
						let tx_id = tx_id.clone();
						Box::pin(async move { client.get_transaction_info_by_id(&tx_id).await })
					})
				},
				tx_id,
			)
			.await
	}

	async fn get_transaction_by_id(&self, tx_id: &str) -> anyhow::Result<TronTransaction> {
		let tx_id = tx_id.to_owned();
		self.get_transaction_by_id
			.get_or_fetch(
				{
					let tx_id = tx_id.clone();
					Box::pin(move |client| {
						let tx_id = tx_id.clone();
						Box::pin(async move { client.get_transaction_by_id(&tx_id).await })
					})
				},
				tx_id,
			)
			.await
	}

	async fn get_block_balances(
		&self,
		block_number: BlockNumber,
		hash: &str,
	) -> anyhow::Result<BlockBalance> {
		let hash = hash.to_owned();
		self.get_block_balances
			.get_or_fetch(
				{
					let hash = hash.clone();
					Box::pin(move |client| {
						let hash = hash.clone();
						Box::pin(
							async move { client.get_block_balances(block_number, &hash).await },
						)
					})
				},
				(block_number, hash),
			)
			.await
	}

	async fn chain_id(&self) -> anyhow::Result<U256> {
		self.chain_id
			.get_or_fetch(
				Box::pin(move |client| Box::pin(async move { client.chain_id().await })),
				(),
			)
			.await
	}

	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> anyhow::Result<Vec<Log>> {
		self.get_logs
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move { client.get_logs(block_hash, contract_address).await })
				}),
				(block_hash, contract_address),
			)
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> anyhow::Result<TransactionReceipt> {
		self.transaction_receipt
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move { client.transaction_receipt(tx_hash).await })
				}),
				tx_hash,
			)
			.await
	}

	async fn block(&self, block_number: U64) -> anyhow::Result<Block<H256>> {
		self.block
			.get_or_fetch(
				Box::pin(move |client| Box::pin(async move { client.block(block_number).await })),
				block_number,
			)
			.await
	}

	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<Block<H256>> {
		self.block_by_hash
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move { client.block_by_hash(block_hash).await })
				}),
				block_hash,
			)
			.await
	}

	async fn block_with_txs(
		&self,
		block_number: U64,
	) -> anyhow::Result<Block<ethers::types::Transaction>> {
		self.block_with_txs
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move { client.block_with_txs(block_number).await })
				}),
				block_number,
			)
			.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<ethers::types::Transaction> {
		self.get_transaction
			.get_or_fetch(
				Box::pin(move |client| {
					Box::pin(async move { client.get_transaction(tx_hash).await })
				}),
				tx_hash,
			)
			.await
	}

	async fn get_block_number(&self) -> anyhow::Result<U64> {
		self.get_block_number
			.get_or_fetch(
				Box::pin(move |client| Box::pin(async move { client.get_block_number().await })),
				(),
			)
			.await
	}
}

/// Broadcast is not cached — it is a write operation that always goes through the retry client.
#[async_trait::async_trait]
impl<Rpc: TronSigningRpcApi + EvmRpcApi> TronRetrySigningRpcApi for TronCachingClient<Rpc> {
	async fn broadcast_transaction(
		&self,
		tx: cf_chains::tron::TronTransaction,
	) -> anyhow::Result<H256> {
		self.retry_client.broadcast_transaction(tx).await
	}
}
