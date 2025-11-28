use std::ops::RangeInclusive;

use crate::{
	caching_request::CachingRequest,
	evm::{
		retry_rpc::{
			node_interface::NodeInterfaceRetryRpcApiWithResult, EvmRetryRpcClient,
			EvmRetrySigningRpcApi,
		},
		rpc::{
			address_checker::{AddressCheckerRpcApi, AddressState},
			node_interface::NodeInterfaceRpcApi,
			EvmRpcApi, EvmSigningRpcApi,
		},
	},
};
use cf_utilities::task_scope::Scope;
use ethers::{prelude::*, types::TransactionReceipt};
use tokio::sync::mpsc;

use crate::evm::rpc::address_checker::PriceFeedData;

/// Tmp trait defined to allow having a finite retry impl (2 retries, one with main endpoint and
/// one with backup), the infinite retry will be removed once ARB is migrated to election based
/// witnessing
#[async_trait::async_trait]
pub trait EvmRetryRpcApiWithResult: Clone {
	async fn get_logs_range(
		&self,
		range: std::ops::RangeInclusive<u64>,
		contract_address: H160,
	) -> anyhow::Result<Vec<Log>>;

	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> anyhow::Result<Vec<Log>>;

	async fn chain_id(&self) -> anyhow::Result<U256>;

	async fn transaction_receipt(&self, tx_hash: H256) -> anyhow::Result<TransactionReceipt>;

	async fn block(&self, block_number: U64) -> anyhow::Result<Block<H256>>;

	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<Block<H256>>;

	async fn block_with_txs(&self, block_number: U64) -> anyhow::Result<Block<Transaction>>;

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> anyhow::Result<FeeHistory>;

	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<Transaction>;

	async fn get_block_number(&self) -> anyhow::Result<U64>;
}

/// Tmp trait defined to allow having a finite retry impl (2 retries, one with main endpoint and
/// one with backup), the infinite retry will be removed once ARB is migrated to election based
/// witnessing
#[async_trait::async_trait]
pub trait AddressCheckerRetryRpcApiWithResult {
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> anyhow::Result<Vec<AddressState>>;

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> anyhow::Result<Vec<U256>>;

	async fn query_price_feeds(
		&self,
		contract_address: H160,
		aggregator_addresses: Vec<H160>,
	) -> anyhow::Result<(U256, U256, Vec<PriceFeedData>)>;
}

#[derive(Clone)]
pub struct EvmCachingClient<Rpc: EvmRpcApi> {
	retry_client: EvmRetryRpcClient<Rpc>,
	get_logs: CachingRequest<(H256, H160), Vec<Log>, EvmRetryRpcClient<Rpc>>,
	chain_id: CachingRequest<(), U256, EvmRetryRpcClient<Rpc>>,
	transaction_receipt: CachingRequest<H256, TransactionReceipt, EvmRetryRpcClient<Rpc>>,
	block: CachingRequest<U64, Block<H256>, EvmRetryRpcClient<Rpc>>,
	block_by_hash: CachingRequest<H256, Block<H256>, EvmRetryRpcClient<Rpc>>,
	block_with_txs: CachingRequest<U64, Block<Transaction>, EvmRetryRpcClient<Rpc>>,
	fee_history: CachingRequest<(U256, BlockNumber), FeeHistory, EvmRetryRpcClient<Rpc>>,
	get_transaction: CachingRequest<H256, Transaction, EvmRetryRpcClient<Rpc>>,
	get_logs_range: CachingRequest<(RangeInclusive<u64>, H160), Vec<Log>, EvmRetryRpcClient<Rpc>>,
	address_states:
		CachingRequest<(H256, H160, Vec<H160>), Vec<AddressState>, EvmRetryRpcClient<Rpc>>,
	balances: CachingRequest<(H256, H160, Vec<H160>), Vec<U256>, EvmRetryRpcClient<Rpc>>,
	query_price_feeds:
		CachingRequest<(H160, Vec<H160>), (U256, U256, Vec<PriceFeedData>), EvmRetryRpcClient<Rpc>>,
	get_block_number: CachingRequest<(), U64, EvmRetryRpcClient<Rpc>>,
	gas_estimate_components: CachingRequest<(), (u64, u64, U256, U256), EvmRetryRpcClient<Rpc>>,

	pub cache_invalidation_senders: Vec<mpsc::Sender<()>>,
}

impl<Rpc: EvmRpcApi> EvmCachingClient<Rpc> {
	pub(crate) fn new(scope: &Scope<'_, anyhow::Error>, client: EvmRetryRpcClient<Rpc>) -> Self {
		let (get_logs, get_logs_cache) = CachingRequest::<
			(H256, H160),
			Vec<Log>,
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (chain_id, chain_id_cache) =
			CachingRequest::<(), U256, EvmRetryRpcClient<Rpc>>::new(scope, client.clone());
		let (transaction_receipt, transaction_receipt_cache) = CachingRequest::<
			H256,
			TransactionReceipt,
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (block, block_cache) =
			CachingRequest::<U64, Block<H256>, EvmRetryRpcClient<Rpc>>::new(scope, client.clone());
		let (block_by_hash, block_by_hash_cache) =
			CachingRequest::<H256, Block<H256>, EvmRetryRpcClient<Rpc>>::new(scope, client.clone());
		let (block_with_txs, block_with_txs_cache) = CachingRequest::<
			U64,
			Block<Transaction>,
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (fee_history, fee_history_cache) = CachingRequest::<
			(U256, BlockNumber),
			FeeHistory,
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (get_transaction, get_transaction_cache) =
			CachingRequest::<H256, Transaction, EvmRetryRpcClient<Rpc>>::new(scope, client.clone());
		let (get_logs_range, get_logs_range_cache) = CachingRequest::<
			(RangeInclusive<u64>, H160),
			Vec<Log>,
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (address_states, address_states_cache) = CachingRequest::<
			(H256, H160, Vec<H160>),
			Vec<AddressState>,
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (balances, balances_cache) = CachingRequest::<
			(H256, H160, Vec<H160>),
			Vec<U256>,
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (query_price_feeds, query_price_feeds_cache) = CachingRequest::<
			(H160, Vec<H160>),
			(U256, U256, Vec<PriceFeedData>),
			EvmRetryRpcClient<Rpc>,
		>::new(scope, client.clone());
		let (get_block_number, get_block_number_cache) =
			CachingRequest::<(), U64, EvmRetryRpcClient<Rpc>>::new(scope, client.clone());
		let (gas_estimate_components, gas_estimate_components_cache) =
			CachingRequest::<(), (u64, u64, U256, U256), EvmRetryRpcClient<Rpc>>::new(
				scope,
				client.clone(),
			);
		EvmCachingClient {
			retry_client: client,
			get_logs,
			chain_id,
			transaction_receipt,
			block,
			block_by_hash,
			block_with_txs,
			fee_history,
			get_transaction,
			get_logs_range,
			address_states,
			balances,
			query_price_feeds,
			get_block_number,
			gas_estimate_components,
			cache_invalidation_senders: vec![
				get_logs_cache,
				chain_id_cache,
				transaction_receipt_cache,
				block_cache,
				block_by_hash_cache,
				block_with_txs_cache,
				fee_history_cache,
				get_transaction_cache,
				get_logs_range_cache,
				address_states_cache,
				balances_cache,
				query_price_feeds_cache,
				get_block_number_cache,
				gas_estimate_components_cache,
			],
		}
	}
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi> EvmRetryRpcApiWithResult for EvmCachingClient<Rpc> {
	async fn get_logs(&self, block_hash: H256, contract_address: H160) -> anyhow::Result<Vec<Log>> {
		self.get_logs
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_logs(block_hash, contract_address).await })
				}),
				(block_hash, contract_address),
			)
			.await
	}

	async fn chain_id(&self) -> anyhow::Result<U256> {
		self.chain_id
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.chain_id().await })
				}),
				(),
			)
			.await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> anyhow::Result<TransactionReceipt> {
		self.transaction_receipt
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.transaction_receipt(tx_hash).await })
				}),
				tx_hash,
			)
			.await
	}

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: U64) -> anyhow::Result<Block<H256>> {
		self.block
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block(block_number).await })
				}),
				block_number,
			)
			.await
	}

	async fn block_by_hash(&self, block_hash: H256) -> anyhow::Result<Block<H256>> {
		self.block_by_hash
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_by_hash(block_hash).await })
				}),
				block_hash,
			)
			.await
	}

	async fn block_with_txs(&self, block_number: U64) -> anyhow::Result<Block<Transaction>> {
		self.block_with_txs
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.block_with_txs(block_number).await })
				}),
				block_number,
			)
			.await
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> anyhow::Result<FeeHistory> {
		self.fee_history
			.get_or_fetch(
				Box::pin(move |client| {
					let reward_percentiles = reward_percentiles.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.fee_history(block_count, newest_block, reward_percentiles).await
					})
				}),
				(block_count, newest_block),
			)
			.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> anyhow::Result<Transaction> {
		self.get_transaction
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_transaction(tx_hash).await })
				}),
				tx_hash,
			)
			.await
	}

	async fn get_logs_range(
		&self,
		range: RangeInclusive<u64>,
		contract_address: H160,
	) -> anyhow::Result<Vec<Log>> {
		let rangee = range.clone();
		self.get_logs_range
			.get_or_fetch(
				Box::pin(move |client| {
					let range = range.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_logs_range(range, contract_address).await })
				}),
				(rangee, contract_address),
			)
			.await
	}
	async fn get_block_number(&self) -> anyhow::Result<U64> {
		self.get_block_number
			.get_or_fetch(
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_block_number().await })
				}),
				(),
			)
			.await
	}
}

#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + AddressCheckerRpcApi> AddressCheckerRetryRpcApiWithResult
	for EvmCachingClient<Rpc>
{
	async fn address_states(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> anyhow::Result<Vec<AddressState>> {
		let addressess = addresses.clone();
		self.address_states
			.get_or_fetch(
				Box::pin(move |client| {
					let addresses = addresses.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.address_states(block_hash, contract_address, addresses).await
					})
				}),
				(block_hash, contract_address, addressess),
			)
			.await
	}

	async fn balances(
		&self,
		block_hash: H256,
		contract_address: H160,
		addresses: Vec<H160>,
	) -> anyhow::Result<Vec<U256>> {
		let addressess = addresses.clone();
		self.balances
			.get_or_fetch(
				Box::pin(move |client| {
					let addresses = addresses.clone();

					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						AddressCheckerRetryRpcApiWithResult::balances(
							&client,
							block_hash,
							contract_address,
							addresses,
						)
						.await
					})
				}),
				(block_hash, contract_address, addressess),
			)
			.await
	}

	async fn query_price_feeds(
		&self,
		contract_address: H160,
		aggregator_addresses: Vec<H160>,
	) -> anyhow::Result<(U256, U256, Vec<PriceFeedData>)> {
		let aggregator_addressess = aggregator_addresses.clone();
		self.query_price_feeds
			.get_or_fetch(
				Box::pin(move |client| {
					let aggregator_addresses = aggregator_addresses.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						AddressCheckerRetryRpcApiWithResult::query_price_feeds(
							&client,
							contract_address,
							aggregator_addresses,
						)
						.await
					})
				}),
				(contract_address, aggregator_addressess),
			)
			.await
	}
}
#[async_trait::async_trait]
impl<Rpc: EvmRpcApi + NodeInterfaceRpcApi> NodeInterfaceRetryRpcApiWithResult
	for EvmCachingClient<Rpc>
{
	async fn gas_estimate_components(
		&self,
		destination_address: H160,
		contract_creation: bool,
		tx_data: Bytes,
	) -> anyhow::Result<(u64, u64, U256, U256)> {
		self.gas_estimate_components
			.get_or_fetch(
				Box::pin(move |client| {
					let tx_data = tx_data.clone();
					#[allow(clippy::redundant_async_block)]
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
				(),
			)
			.await
	}
}

#[async_trait::async_trait]
impl<Rpc: EvmSigningRpcApi> EvmRetrySigningRpcApi for EvmCachingClient<Rpc> {
	/// Estimates gas and then sends the transaction to the network.
	async fn broadcast_transaction(
		&self,
		tx: cf_chains::evm::Transaction,
	) -> anyhow::Result<TxHash> {
		self.retry_client.broadcast_transaction(tx).await
	}
}
