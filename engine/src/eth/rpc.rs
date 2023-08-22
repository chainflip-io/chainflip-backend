pub mod address_checker;

use ethers::{prelude::*, signers::Signer, types::transaction::eip2718::TypedTransaction};

use crate::{
	constants::{ETH_AVERAGE_BLOCK_TIME, SYNC_POLL_INTERVAL},
	settings,
};
use anyhow::{anyhow, Context, Result};
use std::{str::FromStr, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use utilities::make_periodic_tick;

use utilities::read_clean_and_decode_hex_str_file;

#[cfg(test)]
use mockall::automock;

struct NonceInfo {
	next_nonce: U256,
	requested_at: std::time::Instant,
}

#[derive(Clone)]
pub struct EthRpcClient {
	signer: SignerMiddleware<Arc<Provider<Http>>, LocalWallet>,
	nonce_info: Arc<Mutex<Option<NonceInfo>>>,
}

impl EthRpcClient {
	pub async fn new(eth_settings: settings::Eth, expected_chain_id: u64) -> Result<Self> {
		let provider =
			Arc::new(Provider::<Http>::try_from(eth_settings.http_node_endpoint.to_string())?);
		let wallet = read_clean_and_decode_hex_str_file(
			&eth_settings.private_key_file,
			"Ethereum Private Key",
			|key| ethers::signers::Wallet::from_str(key).map_err(anyhow::Error::new),
		)?;
		let signer = SignerMiddleware::new(provider, wallet.with_chain_id(expected_chain_id));

		let client = Self { signer, nonce_info: Arc::new(Mutex::new(None)) };

		// We don't want to return an error here. Returning an error means that we'll exit the CFE.
		// So on client creation we wait until we can be successfully connected to the ETH node. So
		// the other chains are unaffected
		let mut poll_interval = make_periodic_tick(ETH_AVERAGE_BLOCK_TIME, true);
		loop {
			poll_interval.tick().await;
			match client.chain_id().await {
				Ok(chain_id) if chain_id == expected_chain_id.into() => break,
				Ok(chain_id) => {
					tracing::warn!(
						"Connected to Ethereum node but with chain_id {}, expected {}. Please check your CFE
						configuration file...", 				
						chain_id,
						expected_chain_id
					);
				},
				Err(e) => tracing::error!(
					"Cannot connect to an Ethereum node at {} with error: {e}. Please check your CFE
					configuration file. Retrying...",
					eth_settings.http_node_endpoint
				),
			}
		}

		Ok(client)
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

#[cfg_attr(test, automock)]
#[async_trait::async_trait]
pub trait EthRpcApi: Send {
	fn address(&self) -> H160;

	async fn estimate_gas(&self, req: &TypedTransaction) -> Result<U256>;

	async fn send_transaction(&self, tx: TransactionRequest) -> Result<TxHash>;

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
}

#[async_trait::async_trait]
impl EthRpcApi for EthRpcClient {
	fn address(&self) -> H160 {
		self.signer.address()
	}

	async fn estimate_gas(&self, req: &TypedTransaction) -> Result<U256> {
		Ok(self.signer.estimate_gas(req, None).await?)
	}

	async fn send_transaction(&self, mut tx: TransactionRequest) -> Result<TxHash> {
		tx.nonce = Some(self.get_next_nonce().await?);

		let res = self.signer.send_transaction(tx, None).await;

		if res.is_err() {
			// Reset the nonce just in case (it will be re-requested during next broadcast)
			tracing::warn!("Resetting eth broadcaster nonce due to error");
			*self.nonce_info.lock().await = None;
		}

		Ok(res?.tx_hash())
	}

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
		Ok(self.signer.get_logs(&filter).await?)
	}

	async fn chain_id(&self) -> Result<U256> {
		Ok(self.signer.get_chainid().await?)
	}

	async fn transaction_receipt(&self, tx_hash: TxHash) -> Result<TransactionReceipt> {
		Ok(self.signer.get_transaction_receipt(tx_hash).await?.unwrap())
	}

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: U64) -> Result<Block<H256>> {
		self.signer.get_block(block_number).await?.ok_or_else(|| {
			anyhow!("Getting ETH block for block number {} returned None", block_number)
		})
	}

	async fn block_with_txs(&self, block_number: U64) -> Result<Block<Transaction>> {
		self.signer.get_block_with_txs(block_number).await?.ok_or_else(|| {
			anyhow!("Getting ETH block with txs for block number {} returned None", block_number)
		})
	}

	async fn fee_history(
		&self,
		block_count: U256,
		last_block: BlockNumber,
		reward_percentiles: &[f64],
	) -> Result<FeeHistory> {
		Ok(self.signer.fee_history(block_count, last_block, reward_percentiles).await?)
	}
}

/// On each subscription this will create a new WS connection.
#[derive(Clone)]
pub struct ReconnectSubscriptionClient {
	ws_node_endpoint: String,
	// This value comes from the SC.
	chain_id: web3::types::U256,
}

impl ReconnectSubscriptionClient {
	pub fn new(ws_node_endpoint: String, chain_id: web3::types::U256) -> Self {
		Self { ws_node_endpoint, chain_id }
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
		let web3 = web3::Web3::new(web3::transports::WebSocket::new(&self.ws_node_endpoint).await?);

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
			Err(anyhow!(
				"Expected chain id {}, eth ws client returned {client_chain_id}.",
				self.chain_id
			))
		} else {
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
}

#[cfg(test)]
mod tests {

	use crate::settings::Settings;

	use super::*;

	#[tokio::test]
	#[ignore = "Requires correct settings"]
	async fn eth_rpc_test() {
		let settings = Settings::new_test().unwrap();

		let client = EthRpcClient::new(&settings.eth, 2u64).await.unwrap();
		let chain_id = client.chain_id().await.unwrap();
		println!("{:?}", chain_id);

		let block = client.block(0.into()).await.unwrap();
		println!("{:?}", block);

		let block_with_txs = client.block_with_txs(0.into()).await.unwrap();
		println!("{:?}", block_with_txs);

		let fee_history = client
			.fee_history(10.into(), BlockNumber::Latest, &[0.1, 0.5, 0.9])
			.await
			.unwrap();
		println!("{:?}", fee_history);
	}
}
