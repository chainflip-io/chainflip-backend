use async_trait::async_trait;
use cf_chains::dot::{FeeDetails, InclusionFee, PolkadotBalance, PolkadotHash};
use sp_rpc::number::NumberOrHex;
use subxt::{ext::sp_core::Bytes, rpc_params, OnlineClient, PolkadotConfig};

use anyhow::{anyhow, Result};

use serde::{Deserialize, Serialize};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait DotRpcApi: Send + Sync {
	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash>;

	async fn query_fee_details(&self, extrinsic: Bytes, at: PolkadotHash) -> Result<FeeDetails>;
}

pub struct DotRpcClient {
	online_client: OnlineClient<PolkadotConfig>,
}

impl DotRpcClient {
	pub fn new(online_client: OnlineClient<PolkadotConfig>) -> Self {
		Self { online_client }
	}
}

// Helper struct that we can deserialize the NumberOrHex fields into before
// converting to `InclusionFee` which has `PolkadotBalance` fields.
#[derive(PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InclusionFeePre {
	pub base_fee: NumberOrHex,
	pub len_fee: NumberOrHex,
	pub adjusted_weight_fee: NumberOrHex,
}

// Helper struct that we can deserialize the NumberOrHex inclusion fee into before
// we converting into `FeeDetails`
#[derive(PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeeDetailsPre {
	pub inclusion_fee: Option<InclusionFeePre>,
	// // Do not serialize and deserialize `tip` as we actually can not pass any tip to the
	// RPC.
	#[serde(skip)]
	pub tip: PolkadotBalance,
}

impl From<InclusionFeePre> for InclusionFee {
	fn from(inclusion_fee_pre: InclusionFeePre) -> Self {
		let num_or_hex_to_balance = |num_or_hex| {
			PolkadotBalance::try_from(num_or_hex).map_err(|_| ()).expect(
				"Unable to convert NumberOrHex value, that should be a balance into a Balance",
			)
		};

		Self {
			base_fee: num_or_hex_to_balance(inclusion_fee_pre.base_fee),
			len_fee: num_or_hex_to_balance(inclusion_fee_pre.len_fee),
			adjusted_weight_fee: num_or_hex_to_balance(inclusion_fee_pre.adjusted_weight_fee),
		}
	}
}

impl From<FeeDetailsPre> for FeeDetails {
	fn from(fee_details_pre: FeeDetailsPre) -> Self {
		Self { inclusion_fee: fee_details_pre.inclusion_fee.map(|e| e.into()) }
	}
}

#[async_trait]
impl DotRpcApi for DotRpcClient {
	async fn submit_raw_encoded_extrinsic(&self, encoded_bytes: Vec<u8>) -> Result<PolkadotHash> {
		let encoded_bytes: Bytes = encoded_bytes.into();
		self.online_client
			.rpc()
			.request::<PolkadotHash>("author_submitExtrinsic", rpc_params![encoded_bytes])
			.await
			.map_err(|error| {
				anyhow!(
					"Raw Polkadot extrinsic submission failed with error: {:?}",
					error.to_string()
				)
			})
	}

	async fn query_fee_details(&self, extrinsic: Bytes, at: PolkadotHash) -> Result<FeeDetails> {
		Ok(self
			.online_client
			.rpc()
			.request::<FeeDetailsPre>("payment_queryFeeDetails", rpc_params![extrinsic, at])
			.await
			.map_err(|error| {
				anyhow!("Querying fee details failed with error: {:?}", error.to_string())
			})?
			.into())
	}
}

#[cfg(test)]
mod tests {
	use std::str::FromStr;

	use cf_chains::dot::PolkadotHash;
	use subxt::{ext::sp_core::Bytes, OnlineClient, PolkadotConfig};

	use crate::dot::{rpc::DotRpcClient, DotBroadcaster};

	use super::DotRpcApi;

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

	#[tokio::test]
	#[ignore = "helpful for testing a query fee details to a live network"]
	async fn fee_details_test() {
		tracing_subscriber::FmtSubscriber::builder()
			.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
			.try_init()
			.expect("setting default subscriber failed");

		let polkadot_network_ws_url = "url";

		let dot_rpc = DotRpcClient::new(
			OnlineClient::<PolkadotConfig>::from_url(polkadot_network_ws_url).await.unwrap(),
		);

		let extrinsic: Bytes = hex::decode("490284001cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c019ca0f159e74252a572f6d0556268bd7b813cbf3280758c461785b13d371b821011726e33b3a183071beb1e8f895ecf1214f73c79ca9578b36a2cac07eddd358325031c0005000090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe220f0080c6a47e8d03").unwrap().into();

		let fee_details = dot_rpc
			.query_fee_details(
				extrinsic,
				PolkadotHash::from_str(
					"0xb77bb8efbf352c6d732f7191035a7aa6bddceb1b6075122fabd8a0eccf5a55dc",
				)
				.unwrap(),
			)
			.await
			.unwrap();

		println!("Fee details: {:?}", fee_details);
	}
}
