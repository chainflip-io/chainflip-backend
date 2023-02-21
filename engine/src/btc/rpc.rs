use bitcoincore_rpc::{
	bitcoin::{BlockHash, BlockHeader},
	Auth, Client, RpcApi,
};

use anyhow::Result;

use crate::settings;

pub struct BtcRpcClient {
	client: Client,
}

impl BtcRpcClient {
	pub fn new(btc_settings: &settings::Btc) -> Result<Self> {
		let auth = Auth::UserPass(btc_settings.rpc_user.clone(), btc_settings.rpc_password.clone());
		let client = Client::new(&btc_settings.http_node_endpoint, auth)?;
		Ok(Self { client })
	}
}

pub trait BtcRpcApi: Send + Sync {
	fn best_block_hash(&self) -> Result<BlockHash>;

	fn best_block_number(&self) -> Result<u64>;

	fn best_block_header(&self) -> Result<BlockHeader>;
}

impl BtcRpcApi for BtcRpcClient {
	fn best_block_hash(&self) -> Result<BlockHash> {
		Ok(self.client.get_best_block_hash()?)
	}

	fn best_block_number(&self) -> Result<u64> {
		Ok(self.client.get_block_count()?)
	}

	fn best_block_header(&self) -> Result<BlockHeader> {
		Ok(self.client.get_block_header(&self.best_block_hash()?)?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn btc_settings() -> settings::Btc {
		settings::Btc {
			http_node_endpoint: "http://127.0.0.1:18443".to_string(),
			rpc_user: "kyle".to_string(),
			rpc_password: "password".to_string(),
		}
	}

	#[tokio::test]
	async fn my_test() {
		let rpc = BtcRpcClient::new(&btc_settings()).unwrap();

		println!("Best block header: {:?}", rpc.best_block_header().unwrap());
	}
}
