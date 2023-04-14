use std::sync::Arc;

use bitcoincore_rpc::{
	bitcoin::{Block, BlockHash, Txid},
	bitcoincore_rpc_json::EstimateMode,
	Auth, Client, RpcApi,
};

use anyhow::Result;
use cf_chains::btc::{BlockNumber, BtcAmount};

use crate::{settings, witnesser::LatestBlockNumber};

#[cfg(test)]
use mockall::automock;

#[derive(Clone)]
pub struct BtcRpcClient {
	client: Arc<Client>,
}

impl BtcRpcClient {
	pub fn new(btc_settings: &settings::Btc) -> Result<Self> {
		Ok(Self {
			client: Arc::new(Client::new(
				&btc_settings.http_node_endpoint,
				Auth::UserPass(btc_settings.rpc_user.clone(), btc_settings.rpc_password.clone()),
			)?),
		})
	}
}

#[cfg_attr(test, automock)]
pub trait BtcRpcApi: Send + Sync {
	fn best_block_hash(&self) -> Result<BlockHash>;

	fn block(&self, block_hash: BlockHash) -> Result<Block>;

	fn block_hash(&self, block_number: BlockNumber) -> Result<BlockHash>;

	fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> Result<Txid>;

	/// Calculate the approx. fee rate to pay to get into the next block.
	/// It is possible we don't get a fee rate in times of low Bitcoin network usage (though this is
	/// very unlikely when using Bitcoin mainnet). In this case we return `None`.
	fn next_block_fee_rate(&self) -> Result<Option<BtcAmount>>;
}

impl BtcRpcApi for BtcRpcClient {
	fn best_block_hash(&self) -> Result<BlockHash> {
		Ok(self.client.get_best_block_hash()?)
	}

	fn block(&self, block_hash: BlockHash) -> Result<Block> {
		Ok(self.client.get_block(&block_hash)?)
	}

	fn block_hash(&self, block_number: BlockNumber) -> Result<BlockHash> {
		Ok(self.client.get_block_hash(block_number)?)
	}

	fn send_raw_transaction(&self, transaction_bytes: Vec<u8>) -> Result<Txid> {
		Ok(self.client.send_raw_transaction(&transaction_bytes)?)
	}

	fn next_block_fee_rate(&self) -> Result<Option<BtcAmount>> {
		Ok(self
			.client
			.estimate_smart_fee(1, Some(EstimateMode::Conservative))?
			.fee_rate
			.map(|fee| fee.to_sat()))
	}
}

#[async_trait::async_trait]
impl LatestBlockNumber for BtcRpcClient {
	type BlockNumber = BlockNumber;

	async fn latest_block_number(&self) -> Result<BlockNumber> {
		Ok(self.client.get_block_count()?)
	}
}
