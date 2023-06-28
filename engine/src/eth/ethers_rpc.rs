use ethers::{prelude::*, signers::Signer, types::transaction::eip2718::TypedTransaction};

use crate::settings;
use anyhow::{anyhow, Ok, Result};
use std::{str::FromStr, sync::Arc};
use utilities::read_clean_and_decode_hex_str_file;

#[cfg(test)]
use mockall::automock;

#[derive(Clone)]
pub struct EthersRpcClient<T: JsonRpcClient> {
	signer: SignerMiddleware<Arc<Provider<T>>, LocalWallet>,
}

impl<T: JsonRpcClient + 'static> EthersRpcClient<T> {
	pub async fn new(provider: Arc<Provider<T>>, eth_settings: &settings::Eth) -> Result<Self> {
		let wallet = read_clean_and_decode_hex_str_file(
			&eth_settings.private_key_file,
			"Ethereum Private Key",
			|key| ethers::signers::Wallet::from_str(key).map_err(anyhow::Error::new),
		)?;
		let chain_id = provider.get_chainid().await?;
		let signer = SignerMiddleware::new(provider, wallet.with_chain_id(chain_id.as_u64()));
		Ok(Self { signer })
	}
}

#[cfg_attr(test, automock)]
#[async_trait::async_trait]
pub trait EthersRpcApi: Send + Sync {
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
impl<T: JsonRpcClient + 'static> EthersRpcApi for EthersRpcClient<T> {
	fn address(&self) -> H160 {
		self.signer.address()
	}

	async fn estimate_gas(&self, req: &TypedTransaction) -> Result<U256> {
		Ok(self.signer.estimate_gas(req, None).await?)
	}

	async fn send_transaction(&self, tx: TransactionRequest) -> Result<TxHash> {
		Ok(self.signer.send_transaction(tx, None).await?.tx_hash())
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

#[derive(Clone)]
pub struct EthersSubscriptionClient {
	signer: SignerMiddleware<Provider<Ws>, LocalWallet>,
}

#[async_trait::async_trait]
pub trait EthersSubscribeApi {
	async fn subscribe_blocks(&self) -> Result<SubscriptionStream<Ws, Block<H256>>>;
}

#[async_trait::async_trait]
impl EthersSubscribeApi for EthersSubscriptionClient {
	async fn subscribe_blocks(&self) -> Result<SubscriptionStream<Ws, Block<H256>>> {
		Ok(self.signer.subscribe_blocks().await?)
	}
}

#[cfg(test)]
mod tests {

	use crate::settings::Settings;

	use super::*;

	#[tokio::test]
	#[ignore = "Requires correct settings"]
	async fn ethers_rpc_test() {
		let settings = Settings::new_test().unwrap();

		let provider =
			Provider::<Http>::try_from(settings.eth.http_node_endpoint.to_string()).unwrap();
		let client = EthersRpcClient::new(Arc::new(provider), &settings.eth).await.unwrap();
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
