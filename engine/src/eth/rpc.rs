use futures_core::Future;
use tracing::{debug, error, info};
use utilities::{context, make_periodic_tick};
use web3::{
	api::SubscriptionStream,
	signing::SecretKeyRef,
	types::{
		Block, BlockHeader, BlockNumber, Bytes, CallRequest, FeeHistory, Filter, Log,
		SignedTransaction, SyncState, Transaction, TransactionParameters, TransactionReceipt, H256,
		U256, U64,
	},
	Web3,
};
use web3_secp256k1::SecretKey;

use futures::FutureExt;

use anyhow::{anyhow, Context, Result};

use super::{redact_secret_eth_node_endpoint, TransportProtocol};
use crate::{
	constants::{ETH_HTTP_REQUEST_TIMEOUT, ETH_LOG_REQUEST_TIMEOUT, SYNC_POLL_INTERVAL},
	settings,
	witnesser::LatestBlockNumber,
};

use async_trait::async_trait;

#[cfg(test)]
use mockall::automock;

pub type EthHttpRpcClient = EthRpcClient<web3::transports::Http>;
pub type EthWsRpcClient = EthRpcClient<web3::transports::WebSocket>;

#[derive(Clone)]
pub struct EthRpcClient<T: EthTransport> {
	web3: Web3<T>,
}

impl<T: EthTransport> EthRpcClient<T> {
	async fn inner_new<F: futures::Future<Output = Result<T>>>(
		node_endpoint: &str,
		f: F,
	) -> Result<Self> {
		debug!(
			"Connecting new {} web3 client{}",
			T::transport_protocol(),
			match redact_secret_eth_node_endpoint(node_endpoint) {
				Ok(redacted_node_endpoint) => format!(" to {redacted_node_endpoint}"),
				Err(e) => {
					error!(
						"Could not redact secret in {} ETH node endpoint: {e}",
						T::transport_protocol(),
					);
					"".to_string()
				},
			}
		);

		Ok(Self { web3: Web3::new(f.await?) })
	}
}

pub trait EthTransport: web3::Transport {
	fn transport_protocol() -> TransportProtocol;
}

impl EthTransport for web3::transports::WebSocket {
	fn transport_protocol() -> TransportProtocol {
		TransportProtocol::Ws
	}
}

impl EthTransport for web3::transports::Http {
	fn transport_protocol() -> TransportProtocol {
		TransportProtocol::Http
	}
}

// We use a trait so we can inject a mock in the tests
#[cfg_attr(test, automock)]
#[async_trait]
pub trait EthRpcApi: Send + Sync {
	async fn estimate_gas(&self, req: CallRequest) -> Result<U256>;

	async fn sign_transaction(
		&self,
		tx: TransactionParameters,
		key: &SecretKey,
	) -> Result<SignedTransaction>;

	async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256>;

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

	async fn chain_id(&self) -> Result<U256>;

	async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt>;

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: U64) -> Result<Block<H256>>;

	async fn block_with_txs(&self, block_number: U64) -> Result<Block<Transaction>>;

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistory>;
}

async fn with_rpc_timeout<F: Future, O, Err>(request_future: F) -> Result<O>
where
	F: Future<Output = std::result::Result<O, Err>>,
	Err: Into<anyhow::Error>,
{
	tokio::time::timeout(ETH_HTTP_REQUEST_TIMEOUT, request_future)
		.await
		.context("HTTP RPC request timed out")?
		.map_err(|e| e.into())
}

#[async_trait]
impl<T> EthRpcApi for EthRpcClient<T>
where
	T: Send + Sync + EthTransport,
	T::Out: Send,
{
	async fn estimate_gas(&self, req: CallRequest) -> Result<U256> {
		with_rpc_timeout(self.web3.eth().estimate_gas(req, None))
			.await
			.context(format!("{} client: Failed to estimate gas", T::transport_protocol()))
	}

	async fn sign_transaction(
		&self,
		tx: TransactionParameters,
		key: &SecretKey,
	) -> Result<SignedTransaction> {
		with_rpc_timeout(self.web3.accounts().sign_transaction(tx, SecretKeyRef::from(key)))
			.await
			.context(format!("{} client: Failed to sign transaction", T::transport_protocol()))
	}

	async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
		with_rpc_timeout(self.web3.eth().send_raw_transaction(rlp))
			.await
			.context(format!("{} client: Failed to send raw transaction", T::transport_protocol()))
	}

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
		let request_fut = self.web3.eth().logs(filter);

		// NOTE: if this does time out we will most likely have a
		// "memory leak" associated with rust-web3's state for this
		// request not getting properly cleaned up
		tokio::time::timeout(ETH_LOG_REQUEST_TIMEOUT, request_fut)
			.await
			.context(format!("{} client: get_logs request timeout", T::transport_protocol()))?
			.context(format!("{} client: Failed to fetch ETH logs", T::transport_protocol()))
	}

	async fn chain_id(&self) -> Result<U256> {
		with_rpc_timeout(self.web3.eth().chain_id())
			.await
			.context(format!("{} client: Failed to fetch ETH ChainId", T::transport_protocol()))
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt> {
		with_rpc_timeout(self.web3.eth().transaction_receipt(tx_hash))
			.await
			.context(format!("{} client: Failed to fetch ETH transaction", T::transport_protocol()))
			.and_then(|opt_block| {
				opt_block.ok_or_else(|| {
					anyhow!(
						"{} client: Getting ETH transaction receipt with tx_hash {} returned None",
						T::transport_protocol(),
						tx_hash
					)
				})
			})
	}

	async fn block(&self, block_number: U64) -> Result<Block<H256>> {
		with_rpc_timeout(self.web3.eth().block(block_number.into()))
			.await
			.context(format!("{} client: Failed to fetch block", T::transport_protocol()))
			.and_then(|opt_block| {
				opt_block.ok_or_else(|| {
					anyhow!(
						"{} client: Getting ETH block for block number {} returned None",
						T::transport_protocol(),
						block_number,
					)
				})
			})
	}

	async fn block_with_txs(&self, block_number: U64) -> Result<Block<Transaction>> {
		with_rpc_timeout(self.web3.eth().block_with_txs(block_number.into()))
			.await
			.context(format!(
				"{} client: Failed to fetch block with transactions",
				T::transport_protocol()
			))
			.and_then(|opt_block| {
				opt_block.ok_or_else(|| {
					anyhow!(
						"{} client: Getting ETH block for block number {} returned None",
						T::transport_protocol(),
						block_number,
					)
				})
			})
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistory> {
		with_rpc_timeout(self.web3.eth().fee_history(
			block_count,
			newest_block,
			reward_percentiles.clone(),
		))
		.await
		.context(format!(
			"{} client: Call failed: fee_history({:?}, {:?}, {:?})",
			T::transport_protocol(),
			block_count,
			newest_block,
			reward_percentiles,
		))
	}
}

impl EthWsRpcClient {
	pub async fn new(
		eth_settings: &settings::Eth,
		// TODO: make this non-optional once we remove integration tests (PRO-414)
		expected_chain_id: Option<U256>,
	) -> Result<Self> {
		let client = Self::inner_new(&eth_settings.ws_node_endpoint, async {
			context!(web3::transports::WebSocket::new(&eth_settings.ws_node_endpoint).await)
		})
		.await?;

		if let Some(expected_chain_id) = expected_chain_id {
			validate_client_chain_id(&client, expected_chain_id).await?;
		}

		let mut poll_interval = make_periodic_tick(SYNC_POLL_INTERVAL, false);

		while let SyncState::Syncing(info) = client
			.web3
			.eth()
			.syncing()
			.await
			.context("Failure while syncing EthRpcClient client")?
		{
			info!(
				"Waiting for ETH node to sync. Sync state is: {info:?}. Checking again in {:?} ...",
				poll_interval.period(),
			);
			poll_interval.tick().await;
		}
		info!("ETH node is synced.");

		Ok(client)
	}
}

#[async_trait]
impl<T> LatestBlockNumber for EthRpcClient<T>
where
	T: Send + Sync + EthTransport,
	T::Out: Send,
{
	type BlockNumber = u64;

	async fn latest_block_number(&self) -> Result<Self::BlockNumber> {
		self.web3
			.eth()
			.block_number()
			.await
			.context("Failed to fetch block number with HTTP client")
			.map(|n| n.as_u64())
	}
}

#[async_trait]
pub trait EthWsRpcApi {
	async fn subscribe_new_heads(
		&self,
	) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>>;
}

#[async_trait]
impl EthWsRpcApi for EthWsRpcClient {
	async fn subscribe_new_heads(
		&self,
	) -> Result<SubscriptionStream<web3::transports::WebSocket, BlockHeader>> {
		self.web3
			.eth_subscribe()
			.subscribe_new_heads()
			.await
			.context("Failed to subscribe to new heads with WS Client")
	}
}

impl EthHttpRpcClient {
	pub async fn new(
		eth_settings: &settings::Eth,
		// TODO: make this non-optional once we remove integration tests (PRO-414)
		expected_chain_id: Option<U256>,
	) -> Result<Self> {
		let client = Self::inner_new(
			&eth_settings.http_node_endpoint,
			std::future::ready({
				context!(web3::transports::Http::new(&eth_settings.http_node_endpoint))
			}),
		)
		.now_or_never()
		.unwrap()?;

		if let Some(expected_chain_id) = expected_chain_id {
			validate_client_chain_id(&client, expected_chain_id).await?;
		}

		Ok(client)
	}
}

async fn validate_client_chain_id<T>(
	client: &EthRpcClient<T>,
	expected_chain_id: U256,
) -> anyhow::Result<()>
where
	T: Send + Sync + EthTransport,
	T::Out: Send,
{
	let chain_id = client
		.chain_id()
		.await
		.context(format!("Failed to fetch chain id via {}.", T::transport_protocol()))?;

	if chain_id != expected_chain_id {
		return Err(anyhow!(
			"Expected chain id {expected_chain_id}, {} client returned {chain_id}.",
			T::transport_protocol()
		))
	}

	Ok(())
}

#[cfg(test)]
pub mod mocks {
	use super::*;

	use mockall::mock;

	mock!(
		// becomes MockEthHttpRpcClient
		pub EthHttpRpcClient {}

		#[async_trait]
		impl EthRpcApi for EthHttpRpcClient {
			async fn estimate_gas(&self, req: CallRequest) -> Result<U256>;

			async fn sign_transaction(
				&self,
				tx: TransactionParameters,
				key: &SecretKey,
			) -> Result<SignedTransaction>;

			async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256>;

			async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

			async fn chain_id(&self) -> Result<U256>;

			async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt>;

			async fn block(&self, block_number: U64) -> Result<Block<H256>>;

			async fn block_with_txs(&self, block_number: U64) -> Result<Block<Transaction>>;

			async fn fee_history(
				&self,
				block_count: U256,
				newest_block: BlockNumber,
				reward_percentiles: Option<Vec<f64>>,
			) -> Result<FeeHistory>;
		}

		#[async_trait]
		impl LatestBlockNumber for EthHttpRpcClient {
			type BlockNumber = u64;

			async fn latest_block_number(&self) -> Result<u64>;
		}
	);
}
