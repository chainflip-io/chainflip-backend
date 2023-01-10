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

use futures::{
	future::{select, Either},
	FutureExt,
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
	common::format_iterator,
	constants::{ETH_DUAL_REQUEST_TIMEOUT, ETH_LOG_REQUEST_TIMEOUT, SYNC_POLL_INTERVAL},
	logging::COMPONENT_KEY,
	settings,
};

use super::{redact_secret_eth_node_endpoint, TransportProtocol};

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
		logger: &slog::Logger,
	) -> Result<Self> {
		slog::debug!(
			logger,
			"Connecting new {} web3 client{}",
			T::transport_protocol(),
			match redact_secret_eth_node_endpoint(node_endpoint) {
				Ok(redacted_node_endpoint) => format!(" to {redacted_node_endpoint}"),
				Err(e) => {
					slog::error!(
						logger,
						"Could not redact secret in {} ETH node endpoint: {}",
						T::transport_protocol(),
						e
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

	/// Get the latest block number.
	async fn block_number(&self) -> Result<U64>;
}

#[async_trait]
impl<T> EthRpcApi for EthRpcClient<T>
where
	T: Send + Sync + EthTransport,
	T::Out: Send,
{
	async fn estimate_gas(&self, req: CallRequest) -> Result<U256> {
		self.web3
			.eth()
			.estimate_gas(req, None)
			.await
			.context(format!("{} client: Failed to estimate gas", T::transport_protocol()))
	}

	async fn sign_transaction(
		&self,
		tx: TransactionParameters,
		key: &SecretKey,
	) -> Result<SignedTransaction> {
		self.web3
			.accounts()
			.sign_transaction(tx, SecretKeyRef::from(key))
			.await
			.context(format!("{} client: Failed to sign transaction", T::transport_protocol()))
	}

	async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
		self.web3
			.eth()
			.send_raw_transaction(rlp)
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
		self.web3
			.eth()
			.chain_id()
			.await
			.context(format!("{} client: Failed to fetch ETH ChainId", T::transport_protocol()))
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt> {
		self.web3
			.eth()
			.transaction_receipt(tx_hash)
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
		self.web3
			.eth()
			.block(block_number.into())
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
		self.web3
			.eth()
			.block_with_txs(block_number.into())
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
		self.web3
			.eth()
			.fee_history(block_count, newest_block, reward_percentiles.clone())
			.await
			.context(format!(
				"{} client: Call failed: fee_history({:?}, {:?}, {:?})",
				T::transport_protocol(),
				block_count,
				newest_block,
				reward_percentiles,
			))
	}

	async fn block_number(&self) -> Result<U64> {
		self.web3
			.eth()
			.block_number()
			.await
			.context("Failed to fetch block number with HTTP client")
	}
}

impl EthWsRpcClient {
	pub async fn new(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
		let client = Self::inner_new(
			&eth_settings.ws_node_endpoint,
			async {
				context!(web3::transports::WebSocket::new(&eth_settings.ws_node_endpoint).await)
			},
			logger,
		)
		.await?;

		let mut poll_interval = make_periodic_tick(SYNC_POLL_INTERVAL, false);

		while let SyncState::Syncing(info) = client
			.web3
			.eth()
			.syncing()
			.await
			.context("Failure while syncing EthRpcClient client")?
		{
			slog::info!(
				logger,
				"Waiting for ETH node to sync. Sync state is: {:?}. Checking again in {:?} ...",
				info,
				poll_interval.period(),
			);
			poll_interval.tick().await;
		}
		slog::info!(logger, "ETH node is synced.");

		Ok(client)
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
	pub fn new(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
		Self::inner_new(
			&eth_settings.http_node_endpoint,
			std::future::ready({
				context!(web3::transports::Http::new(&eth_settings.http_node_endpoint))
			}),
			logger,
		)
		.now_or_never()
		.unwrap()
	}
}

#[derive(Clone)]
pub struct EthDualRpcClient {
	pub ws_client: EthWsRpcClient,
	pub http_client: EthHttpRpcClient,
	logger: slog::Logger,
}

impl EthDualRpcClient {
	/// Create an Ethereum Rpc Client, containing a HTTP and a WS client.
	/// Passing a value to expected_chain_id ensures the endpoints provided in settings are pointing
	/// to nodes with the same `chain_id` as that provided.
	pub async fn new(
		eth_settings: &settings::Eth,
		expected_chain_id: U256,
		logger: &slog::Logger,
	) -> Result<Self> {
		let dual_rpc = Self::inner_new(eth_settings, logger).await?;

		pub async fn validate_client_chain_id<T>(
			client: &EthRpcClient<T>,
			expected_chain_id: U256,
		) -> anyhow::Result<()>
		where
			T: Send + Sync + EthTransport,
			T::Out: Send,
		{
			let chain_id = client.chain_id().await.context("Failed to fetch chain id")?;

			if chain_id != expected_chain_id {
				return Err(anyhow!(
					"Expected ETH chain id {}, received {} through {}.",
					expected_chain_id,
					chain_id,
					T::transport_protocol()
				))
			}

			Ok(())
		}

		let mut errors = [
			validate_client_chain_id(&dual_rpc.ws_client, expected_chain_id).await,
			validate_client_chain_id(&dual_rpc.http_client, expected_chain_id).await,
		]
		.into_iter()
		.filter_map(|res| res.err())
		.peekable();

		if errors.peek().is_some() {
			bail!("Inconsistent chain configuration. Terminating.{}", format_iterator(errors));
		}

		Ok(dual_rpc)
	}

	#[cfg(feature = "integration-test")]
	/// For tests we assume we're pointing to the correct chain_id.
	pub async fn new_test(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
		Self::inner_new(eth_settings, logger).await
	}

	async fn inner_new(eth_settings: &settings::Eth, logger: &slog::Logger) -> Result<Self> {
		let logger = logger.new(slog::o!(COMPONENT_KEY => "Eth-DualRpcClient"));

		let ws_client = EthWsRpcClient::new(eth_settings, &logger)
			.await
			.context("Failed to create EthWsRpcClient")?;

		let http_client = EthHttpRpcClient::new(eth_settings, &logger)
			.context("Failed to create EthHttpRpcClient")?;

		Ok(Self { ws_client, http_client, logger })
	}
}

async fn select_ok_or_both_errors<F, T, E>(
	f1: F,
	f2: F,
) -> Result<(T, Option<Either<E, E>>), (E, E)>
where
	F: futures::Future<Output = Result<T, E>> + Unpin,
{
	match select(f1, f2).await {
		Either::Left((Ok(ok), _)) | Either::Right((Ok(ok), _)) => Ok((ok, None)),
		Either::Left((Err(e_left), right)) => match right.await {
			Ok(ok) => Ok((ok, Some(Either::Left(e_left)))),
			Err(e_right) => Err((e_left, e_right)),
		},
		Either::Right((Err(e_right), left)) => match left.await {
			Ok(ok) => Ok((ok, Some(Either::Right(e_right)))),
			Err(e_left) => Err((e_left, e_right)),
		},
	}
}

macro_rules! dual_call_rpc {
    ($eth_dual:expr, $method:ident, $($arg:expr),*) => {
        {
            let ws_request = $eth_dual.ws_client.$method($($arg.clone()),*);
            let http_request = $eth_dual.http_client.$method($($arg),*);

            tokio::time::timeout(ETH_DUAL_REQUEST_TIMEOUT, select_ok_or_both_errors(ws_request, http_request))
                .await
                .context("ETH Dual RPC request timed out")?
                .map_err(|(e_ws, e_http)| {
                    anyhow!(
                        "ETH Dual RPC request failed: {:?} side: {:?}, {:?} side: {:?}",
                        TransportProtocol::Ws, e_ws, TransportProtocol::Http, e_http
                    )
                })
                .map(|(res, maybe_err)| {
                    if let Some(err) = maybe_err {
                        let (side, message) = match err {
                            Either::Left(e) => (TransportProtocol::Ws, e),
                            Either::Right(e) => (TransportProtocol::Http, e),
                        };
                        slog::warn!(
                            $eth_dual.logger,
                            "{:?} side of the ETH Dual RPC request failed (the other succeeded): {:?}",
                            side, message,
                        );
                    }
                    res
                })
        }
    };
}

#[async_trait]
impl EthRpcApi for EthDualRpcClient {
	async fn estimate_gas(&self, req: CallRequest) -> Result<U256> {
		dual_call_rpc!(self, estimate_gas, req)
	}

	async fn sign_transaction(
		&self,
		tx: TransactionParameters,
		key: &SecretKey,
	) -> Result<SignedTransaction> {
		// NB: This clippy allow applies file-wide, but we only need it for this borrow
		#![allow(clippy::needless_borrow)]
		dual_call_rpc!(self, sign_transaction, tx, &key)
	}

	async fn send_raw_transaction(&self, rlp: Bytes) -> Result<H256> {
		dual_call_rpc!(self, send_raw_transaction, rlp)
	}

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
		dual_call_rpc!(self, get_logs, filter)
	}

	async fn chain_id(&self) -> Result<U256> {
		dual_call_rpc!(self, chain_id,)
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> Result<TransactionReceipt> {
		dual_call_rpc!(self, transaction_receipt, tx_hash)
	}

	async fn block(&self, block_number: U64) -> Result<Block<H256>> {
		dual_call_rpc!(self, block, block_number)
	}

	async fn block_with_txs(&self, block_number: U64) -> Result<Block<Transaction>> {
		dual_call_rpc!(self, block_with_txs, block_number)
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistory> {
		dual_call_rpc!(self, fee_history, block_count, newest_block, reward_percentiles)
	}

	async fn block_number(&self) -> Result<U64> {
		dual_call_rpc!(self, block_number,)
	}
}

#[cfg(test)]
pub mod mocks {
	use super::*;

	use mockall::mock;
	// use web3::types::{Block, Bytes, Filter, Log, H256};

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

			async fn block_number(&self) -> Result<U64>;
		}
	);
}
