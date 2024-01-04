pub mod address_checker;

use anyhow::bail;
use ethers::{prelude::*, signers::Signer, types::transaction::eip2718::TypedTransaction};
use futures_core::Future;
use utilities::redact_endpoint_secret::SecretUrl;

use crate::constants::{RPC_RETRY_CONNECTION_INTERVAL, SYNC_POLL_INTERVAL};
use anyhow::{anyhow, Context, Result};
use std::{path::PathBuf, str::FromStr, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use utilities::make_periodic_tick;

use utilities::read_clean_and_decode_hex_str_file;

struct NonceInfo {
	next_nonce: U256,
	requested_at: std::time::Instant,
}

#[derive(Clone)]
pub struct EthRpcClient {
	provider: Arc<Provider<Http>>,
}

impl EthRpcClient {
	pub fn new(
		http_endpoint: SecretUrl,
		expected_chain_id: u64,
	) -> anyhow::Result<impl Future<Output = Self>> {
		let provider = Arc::new(Provider::<Http>::try_from(http_endpoint.as_ref())?);

		let client = EthRpcClient { provider };

		Ok(async move {
			// We don't want to return an error here. Returning an error means that we'll exit the
			// CFE. So on client creation we wait until we can be successfully connected to the ETH
			// node. So the other chains are unaffected
			let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
			loop {
				poll_interval.tick().await;
				match client.chain_id().await {
					Ok(chain_id) if chain_id == expected_chain_id.into() => break client,
					Ok(chain_id) => {
						tracing::error!(
								"Connected to Ethereum node but with incorrect chain_id {chain_id}, expected {expected_chain_id} from {http_endpoint}. \
								Please check your CFE configuration file...",
							);
					},
					Err(e) => tracing::error!(
						"Cannot connect to an Ethereum node at {http_endpoint} with error: {e}. \
							Please check your CFE configuration file. Retrying in {:?}...",
						poll_interval.period()
					),
				}
			}
		})
	}
}

#[async_trait::async_trait]
impl EthRpcApi for EthRpcClient {
	async fn estimate_gas(&self, req: &Eip1559TransactionRequest) -> Result<U256> {
		Ok(self
			.provider
			.estimate_gas(&TypedTransaction::Eip1559(req.clone()), None)
			.await?)
	}

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
		Ok(self.provider.get_logs(&filter).await?)
	}

	async fn chain_id(&self) -> Result<U256> {
		Ok(self.provider.get_chainid().await?)
	}

	async fn transaction_receipt(&self, tx_hash: TxHash) -> Result<TransactionReceipt> {
		self.provider.get_transaction_receipt(tx_hash).await?.ok_or_else(|| {
			anyhow!("Getting ETH transaction receipt for tx hash {tx_hash} returned None")
		})
	}

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: U64) -> Result<Block<H256>> {
		self.provider.get_block(block_number).await?.ok_or_else(|| {
			anyhow!("Getting ETH block for block number {block_number} returned None")
		})
	}

	async fn block_with_txs(&self, block_number: U64) -> Result<Block<Transaction>> {
		self.provider.get_block_with_txs(block_number).await?.ok_or_else(|| {
			anyhow!("Getting ETH block with txs for block number {block_number} returned None")
		})
	}

	async fn fee_history(
		&self,
		block_count: U256,
		last_block: BlockNumber,
		reward_percentiles: &[f64],
	) -> Result<FeeHistory> {
		Ok(self.provider.fee_history(block_count, last_block, reward_percentiles).await?)
	}

	async fn get_transaction(&self, tx_hash: H256) -> Result<Transaction> {
		self.provider
			.get_transaction(tx_hash)
			.await?
			.ok_or_else(|| anyhow!("Getting ETH transaction for tx hash {tx_hash} returned None"))
	}
}

#[derive(Clone)]
pub struct EthRpcSigningClient {
	signer: SignerMiddleware<Arc<Provider<Http>>, LocalWallet>,
	rpc_client: EthRpcClient,
	nonce_info: Arc<Mutex<Option<NonceInfo>>>,
}

impl EthRpcSigningClient {
	pub fn new(
		private_key_file: PathBuf,
		http_endpoint: SecretUrl,
		expected_chain_id: u64,
	) -> Result<impl Future<Output = Self>> {
		let rpc_client_fut = EthRpcClient::new(http_endpoint, expected_chain_id)?;

		let wallet =
			read_clean_and_decode_hex_str_file(&private_key_file, "Ethereum Private Key", |key| {
				ethers::signers::Wallet::from_str(key).map_err(anyhow::Error::new)
			})?;

		Ok(async move {
			let rpc_client = rpc_client_fut.await;

			let signer = SignerMiddleware::new(
				rpc_client.provider.clone(),
				wallet.with_chain_id(expected_chain_id),
			);
			Self { signer, nonce_info: Arc::new(Mutex::new(None)), rpc_client }
		})
	}

	async fn get_next_nonce(&self) -> Result<U256> {
		let mut nonce_info_lock = self.nonce_info.lock().await;

		const NONCE_LIFETIME: std::time::Duration = std::time::Duration::from_secs(120);

		// Reset nonce if too old to ensure that we never
		// get stuck with an incorrect nonce for some reason
		if nonce_info_lock.as_ref().is_some_and(|nonce| {
			Instant::now().checked_duration_since(nonce.requested_at).unwrap_or_default() >
				NONCE_LIFETIME
		}) {
			*nonce_info_lock = None;
		}

		// Re-request nonce if set to None
		let nonce_info = match nonce_info_lock.as_mut() {
			Some(nonce_info) => nonce_info,
			None => {
				let tx_count = self
					.signer
					.get_transaction_count(self.address(), Some(BlockNumber::Pending.into()))
					.await?;
				nonce_info_lock
					.insert(NonceInfo { next_nonce: tx_count, requested_at: Instant::now() })
			},
		};

		let result = nonce_info.next_nonce;
		nonce_info.next_nonce += U256::from(1);
		Ok(result)
	}
}

#[async_trait::async_trait]
pub trait EthRpcApi: Send + Sync + Clone + 'static {
	async fn estimate_gas(&self, req: &Eip1559TransactionRequest) -> Result<U256>;

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
		reward_percentiles: &[f64],
	) -> Result<FeeHistory>;

	async fn get_transaction(&self, tx_hash: H256) -> Result<Transaction>;
}

#[async_trait::async_trait]
pub trait EthSigningRpcApi: EthRpcApi {
	fn address(&self) -> H160;

	async fn send_transaction(&self, tx: Eip1559TransactionRequest) -> Result<TxHash>;
}

#[async_trait::async_trait]
impl EthRpcApi for EthRpcSigningClient {
	async fn estimate_gas(&self, req: &Eip1559TransactionRequest) -> Result<U256> {
		self.rpc_client.estimate_gas(req).await
	}

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
		self.rpc_client.get_logs(filter).await
	}

	async fn chain_id(&self) -> Result<U256> {
		self.rpc_client.chain_id().await
	}

	async fn transaction_receipt(&self, tx_hash: TxHash) -> Result<TransactionReceipt> {
		self.rpc_client.transaction_receipt(tx_hash).await
	}

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: U64) -> Result<Block<H256>> {
		self.rpc_client.block(block_number).await
	}

	async fn block_with_txs(&self, block_number: U64) -> Result<Block<Transaction>> {
		self.rpc_client.block_with_txs(block_number).await
	}

	async fn fee_history(
		&self,
		block_count: U256,
		last_block: BlockNumber,
		reward_percentiles: &[f64],
	) -> Result<FeeHistory> {
		self.rpc_client.fee_history(block_count, last_block, reward_percentiles).await
	}

	async fn get_transaction(&self, tx_hash: H256) -> Result<Transaction> {
		self.rpc_client.get_transaction(tx_hash).await
	}
}

#[async_trait::async_trait]
impl EthSigningRpcApi for EthRpcSigningClient {
	fn address(&self) -> H160 {
		self.signer.address()
	}

	async fn send_transaction(&self, mut tx: Eip1559TransactionRequest) -> Result<TxHash> {
		tx.nonce = Some(self.get_next_nonce().await?);

		let res = self.signer.send_transaction(tx, None).await;
		if res.is_err() {
			// Reset the nonce just in case (it will be re-requested during next broadcast)
			tracing::warn!("Resetting eth broadcaster nonce due to error");
			*self.nonce_info.lock().await = None;
		}

		Ok(res?.tx_hash())
	}
}

/// On each subscription this will create a new WS connection.
#[derive(Clone)]
pub struct ReconnectSubscriptionClient {
	ws_endpoint: SecretUrl,
	// This value comes from the SC.
	chain_id: web3::types::U256,
}

impl ReconnectSubscriptionClient {
	pub fn new(ws_endpoint: SecretUrl, chain_id: web3::types::U256) -> Self {
		Self { ws_endpoint, chain_id }
	}
}

#[async_trait::async_trait]
pub trait ReconnectSubscribeApi {
	async fn subscribe_blocks(&self) -> Result<ConscientiousEthWebsocketBlockHeaderStream>;
}

use crate::eth::ConscientiousEthWebsocketBlockHeaderStream;

#[async_trait::async_trait]
impl ReconnectSubscribeApi for ReconnectSubscriptionClient {
	async fn subscribe_blocks(&self) -> Result<ConscientiousEthWebsocketBlockHeaderStream> {
		let web3 =
			web3::Web3::new(web3::transports::WebSocket::new(self.ws_endpoint.as_ref()).await?);

		let mut poll_interval = make_periodic_tick(SYNC_POLL_INTERVAL, false);

		while let web3::types::SyncState::Syncing(info) =
			web3.eth().syncing().await.context("Failure while syncing WS Eth client")?
		{
			tracing::info!(
				"Waiting for ETH node to sync. Sync state is: {info:?}. Checking again in {:?} ...",
				poll_interval.period(),
			);
			poll_interval.tick().await;
		}

		let client_chain_id = web3.eth().chain_id().await.context("Failed to fetch chain id.")?;
		if self.chain_id != client_chain_id {
			bail!("Expected chain id {}, eth ws client returned {client_chain_id}.", self.chain_id)
		}

		Ok(ConscientiousEthWebsocketBlockHeaderStream {
			stream: Some(
				web3.eth_subscribe()
					.subscribe_new_heads()
					.await
					.context("Failed to subscribe to new heads with WS Client")?,
			),
		})
	}
}

#[cfg(test)]
mod tests {

	use crate::settings::Settings;

	use super::*;

	#[tokio::test]
	#[ignore = "Requires correct settings"]
	async fn eth_rpc_test() {
		let settings = Settings::new_test().unwrap();

		let client = EthRpcSigningClient::new(
			settings.eth.private_key_file,
			settings.eth.nodes.primary.http_endpoint,
			2u64,
		)
		.unwrap()
		.await;
		let chain_id = client.chain_id().await.unwrap();
		println!("{:?}", chain_id);

		let block = client.block(0.into()).await.unwrap();
		println!("{:?}", block);

		let block_with_txs = client.block_with_txs(0.into()).await.unwrap();
		println!("{:?}", block_with_txs);

		let fee_history = client
			.fee_history(10.into(), BlockNumber::Latest, &[10.0, 50.0, 90.0])
			.await
			.unwrap();
		println!("{:?}", fee_history);
	}
}
