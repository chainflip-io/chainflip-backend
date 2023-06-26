use ethers::{prelude::*, signers::Signer, types::transaction::eip2718::TypedTransaction};

use crate::settings;
use anyhow::{anyhow, Ok, Result};
use std::{str::FromStr, sync::Arc};
use utilities::read_clean_and_decode_hex_str_file;

#[cfg(test)]
use mockall::automock;

#[derive(Clone)]
pub struct EthersRpcClient {
	signer: SignerMiddleware<Provider<Http>, LocalWallet>,
	address_checker: AddressChecker<Provider<Http>>,
	vault: Vault<Provider<Http>>,
}

abigen!(AddressChecker, "eth-contract-abis/perseverance-rc17/IAddressChecker.json");
abigen!(Vault, "eth-contract-abis/perseverance-rc17/IVault.json");

impl EthersRpcClient {
	pub async fn new(
		eth_settings: &settings::Eth,
		vault_contract_address: H160,
		address_checker_address: H160,
	) -> Result<Self> {
		let provider = Provider::<Http>::try_from(eth_settings.http_node_endpoint.to_string())?;
		let wallet = read_clean_and_decode_hex_str_file(
			&eth_settings.private_key_file,
			"Ethereum Private Key",
			|key| ethers::signers::Wallet::from_str(key).map_err(anyhow::Error::new),
		)?;
		let chain_id = provider.get_chainid().await?;
		let signer =
			SignerMiddleware::new(provider.clone(), wallet.with_chain_id(chain_id.as_u64()));
		let provider = Arc::new(provider);
		let address_checker = AddressChecker::new(address_checker_address, provider.clone());
		let vault = Vault::new(vault_contract_address, provider);
		Ok(Self { signer, address_checker, vault })
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

	async fn fetched_native_events(&self, block_hash: H256) -> Result<Vec<FetchedNativeFilter>>;

	async fn address_states(
		&self,
		block_hash: H256,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>>;

	async fn balances(&self, block_hash: H256, addresses: Vec<H160>) -> Result<Vec<U256>>;
}

#[async_trait::async_trait]
impl EthersRpcApi for EthersRpcClient {
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

	async fn fetched_native_events(&self, block_hash: H256) -> Result<Vec<FetchedNativeFilter>> {
		let fetched_native_events =
			self.vault.event::<FetchedNativeFilter>().at_block_hash(block_hash);

		Ok(fetched_native_events.query().await?)
	}

	async fn address_states(
		&self,
		block_hash: H256,
		addresses: Vec<H160>,
	) -> Result<Vec<AddressState>> {
		Ok(self
			.address_checker
			.address_states(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
	}

	async fn balances(&self, block_hash: H256, addresses: Vec<H160>) -> Result<Vec<U256>> {
		Ok(self
			.address_checker
			.native_balances(addresses)
			.block(BlockId::Hash(block_hash))
			.call()
			.await?)
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
		let client = EthersRpcClient::new(
			&settings.eth,
			"B7A5bd0345EF1Cc5E66bf61BdeC17D2461fBd968".parse().unwrap(),
			H160::random(),
		)
		.await
		.unwrap();
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
