use async_trait::async_trait;
use subxt::{ext::sp_core::Bytes, rpc_params, Config, OnlineClient, PolkadotConfig};

use anyhow::{anyhow, Result};

pub type PolkadotHash = <PolkadotConfig as Config>::Hash;

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait DotRpcApi: Send + Sync {
	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash>;
}

pub struct DotRpcClient {
	online_client: OnlineClient<PolkadotConfig>,
}

impl DotRpcClient {
	pub fn new(online_client: OnlineClient<PolkadotConfig>) -> Self {
		Self { online_client }
	}
}

#[async_trait]
impl DotRpcApi for DotRpcClient {
	async fn submit_raw_encoded_extrinsic(
		&self,
		encoded_bytes: Vec<u8>,
	) -> Result<<PolkadotConfig as Config>::Hash> {
		let encoded_bytes: Bytes = encoded_bytes.into();
		self.online_client
			.rpc()
			.request::<<PolkadotConfig as Config>::Hash>(
				"author_submitExtrinsic",
				rpc_params![encoded_bytes],
			)
			.await
			.map_err(|error| {
				anyhow!(
					"Raw Polkadot extrinsic submission failed with error: {:?}",
					error.to_string()
				)
			})
	}
}

#[cfg(test)]
mod tests {
	use subxt::{OnlineClient, PolkadotConfig};

	use crate::dot::{rpc::DotRpcClient, DotBroadcaster};

	#[tokio::test]
	#[ignore = "Testing raw broadcast to live network"]
	async fn broadcast_tx() {
		let dot_broadcaster = DotBroadcaster::new(DotRpcClient::new(
			OnlineClient::<PolkadotConfig>::from_url("URL").await.unwrap(),
		));

		// Can get these bytes from the `create_test_extrinsic()` in state-chain/chains/src/dot.rs
		// Will have to ensure the nonce for the account is correct and westend versions are correct
		// for the transaction to be valid
		let balances_signed_encoded_bytes: Vec<u8> = vec![
			61, 2, 132, 0, 86, 204, 74, 248, 255, 159, 185, 124, 96, 50, 10, 228, 61, 53, 189, 131,
			27, 20, 240, 183, 6, 95, 51, 133, 219, 13, 191, 76, 181, 216, 118, 111, 1, 248, 49, 73,
			4, 246, 220, 141, 169, 139, 169, 179, 156, 141, 168, 221, 129, 139, 217, 69, 138, 202,
			21, 226, 229, 249, 205, 183, 253, 121, 63, 133, 124, 0, 52, 146, 100, 192, 219, 76,
			144, 138, 123, 47, 117, 101, 73, 139, 71, 255, 94, 99, 144, 186, 185, 34, 46, 165, 13,
			183, 107, 235, 223, 12, 139, 0, 48, 0, 4, 0, 0, 190, 185, 195, 240, 174, 91, 218, 121,
			141, 211, 182, 95, 227, 69, 253, 249, 3, 25, 70, 132, 157, 137, 37, 174, 123, 231, 62,
			233, 64, 124, 103, 55, 7, 0, 158, 41, 38, 8,
		];

		let tx_hash = dot_broadcaster.send(balances_signed_encoded_bytes).await.unwrap();
		println!("Tx hash: {:?}", tx_hash);
	}
}
