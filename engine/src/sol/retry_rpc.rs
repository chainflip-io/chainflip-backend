use crate::{
	retrier::{Attempt, RequestLog, RetrierClient},
	settings::{NodeContainer, WsHttpEndpoints},
	witness::common::chain_source::{ChainClient, Header},
};
use cf_chains::{
	sol::{SolAddress, SolHash, SolSignature},
	Solana,
};
use core::time::Duration;
use utilities::task_scope::Scope;

use anyhow::Result;
use std::str::FromStr;

use super::{
	commitment_config::CommitmentConfig,
	rpc::{SolRpcApi, SolRpcClient},
	rpc_client_api::*,
};

#[derive(Clone)]
pub struct SolRetryRpcClient {
	rpc_retry_client: RetrierClient<SolRpcClient>,
	witness_period: u64,
}

const SOLANA_RPC_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_CONCURRENT_SUBMISSIONS: u32 = 100;

#[allow(dead_code)]
const MAX_BROADCAST_RETRIES: Attempt = 10;

impl SolRetryRpcClient {
	pub async fn new(
		scope: &Scope<'_, anyhow::Error>,
		nodes: NodeContainer<WsHttpEndpoints>,
		expected_genesis_hash: Option<SolHash>,
		witness_period: u64,
	) -> Result<Self> {
		// Passing only the http_endpoint. Not using the ws for now
		let rpc_client = SolRpcClient::new(nodes.primary.http_endpoint, expected_genesis_hash)?;

		let backup_rpc_client = nodes
			.backup
			.map(|backup_endpoint| {
				SolRpcClient::new(backup_endpoint.http_endpoint, expected_genesis_hash)
			})
			.transpose()?;

		Ok(Self {
			rpc_retry_client: RetrierClient::new(
				scope,
				"sol_rpc",
				rpc_client,
				backup_rpc_client,
				SOLANA_RPC_TIMEOUT,
				MAX_CONCURRENT_SUBMISSIONS,
			),
			witness_period,
		})
	}
}

#[async_trait::async_trait]
pub trait SolRetryRpcApi: Clone {
	async fn get_block(&self, slot: u64, config: RpcBlockConfig) -> UiConfirmedBlock;
	async fn get_slot(&self, commitment: CommitmentConfig) -> u64; // Slot
	async fn get_recent_prioritization_fees(&self) -> Vec<RpcPrioritizationFee>;
	async fn get_multiple_accounts(
		&self,
		pubkeys: &[SolAddress],
		config: RpcAccountInfoConfig,
	) -> Response<Vec<Option<UiAccount>>>;
	async fn get_signature_statuses(
		&self,
		signatures: &[SolSignature],
		search_transaction_history: bool,
	) -> Response<Vec<Option<TransactionStatus>>>;

	async fn get_transaction(
		&self,
		signature: &SolSignature,
		config: RpcTransactionConfig,
	) -> EncodedConfirmedTransactionWithStatusMeta;

	async fn broadcast_transaction(&self, raw_transaction: Vec<u8>)
		-> anyhow::Result<SolSignature>;
}

#[async_trait::async_trait]
impl SolRetryRpcApi for SolRetryRpcClient {
	async fn get_block(&self, slot: u64, config: RpcBlockConfig) -> UiConfirmedBlock {
		self.rpc_retry_client
			.request(
				RequestLog::new("getBlock".to_string(), Some(format!("{slot:?}, {config:?}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_block(slot, config).await })
				}),
			)
			.await
	}

	async fn get_slot(&self, commitment: CommitmentConfig) -> u64 {
		self.rpc_retry_client
			.request(
				RequestLog::new("getSlot".to_string(), Some(format!("{commitment:?}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_slot(commitment).await })
				}),
			)
			.await
	}

	async fn get_recent_prioritization_fees(&self) -> Vec<RpcPrioritizationFee> {
		self.rpc_retry_client
			.request(
				RequestLog::new("getRecentPrioritizationFees".to_string(), None),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_recent_prioritization_fees().await })
				}),
			)
			.await
	}

	async fn get_multiple_accounts(
		&self,
		pubkeys: &[SolAddress],
		config: RpcAccountInfoConfig,
	) -> Response<Vec<Option<UiAccount>>> {
		let pubkeys = pubkeys.to_owned();
		let config = config.clone();
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"getMultipleAccounts".to_string(),
					Some(format!("{:?}, {:?}", pubkeys, config)),
				),
				Box::pin(move |client| {
					let pubkeys = pubkeys.clone();
					let config = config.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_multiple_accounts(&pubkeys, config).await })
				}),
			)
			.await
	}
	async fn get_signature_statuses(
		&self,
		signatures: &[SolSignature],
		search_transaction_history: bool,
	) -> Response<Vec<Option<TransactionStatus>>> {
		let signatures = signatures.to_owned();
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"getSignatureStatuses".to_string(),
					Some(format!("{:?}, {:?}", signatures, search_transaction_history)),
				),
				Box::pin(move |client| {
					let signatures = signatures.clone();
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						client.get_signature_statuses(&signatures, search_transaction_history).await
					})
				}),
			)
			.await
	}
	async fn get_transaction(
		&self,
		signature: &SolSignature,
		config: RpcTransactionConfig,
	) -> EncodedConfirmedTransactionWithStatusMeta {
		let signature = *signature;
		self.rpc_retry_client
			.request(
				RequestLog::new(
					"getTransaction".to_string(),
					Some(format!("{:?}, {:?}", signature, config)),
				),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move { client.get_transaction(&signature, config).await })
				}),
			)
			.await
	}
	async fn broadcast_transaction(&self, transaction: Vec<u8>) -> anyhow::Result<SolSignature> {
		let encoded_transaction = base64::encode(&transaction);
		let config = RpcSendTransactionConfig {
			skip_preflight: true,
			preflight_commitment: None,
			encoding: Some(UiTransactionEncoding::Base64),
			max_retries: None,
			min_context_slot: None,
		};

		self.rpc_retry_client
			.request_with_limit(
				RequestLog::new(
					"sendTransaction".to_string(),
					Some(format!("{:?}, {:?}", transaction, config)),
				),
				Box::pin(move |client| {
					let encoded_transaction = encoded_transaction.clone();
					// let config = config;
					#[allow(clippy::redundant_async_block)]
					Box::pin(
						async move { client.send_transaction(encoded_transaction, config).await },
					)
				}),
				MAX_BROADCAST_RETRIES,
			)
			.await
	}
}

#[async_trait::async_trait]
impl ChainClient for SolRetryRpcClient {
	type Index = <Solana as cf_chains::Chain>::ChainBlockNumber;
	type Hash = SolHash;
	type Data = ();

	async fn header_at_index(
		&self,
		index: Self::Index,
	) -> Header<Self::Index, Self::Hash, Self::Data> {
		use cf_chains::witness_period;

		let witness_period = self.witness_period;
		assert!(witness_period::is_block_witness_root(witness_period, index));
		self.rpc_retry_client
			.request(
				RequestLog::new("header_at_index".to_string(), Some(format!("{index}"))),
				Box::pin(move |client| {
					#[allow(clippy::redundant_async_block)]
					Box::pin(async move {
						let witness_range =
							witness_period::block_witness_range(witness_period, index);

						async fn get_block_details<Rpc: SolRpcApi>(
							client: &Rpc,
							index: u64,
						) -> anyhow::Result<(SolHash, Option<SolHash>)> {
							let block = client
								.get_block(
									index,
									RpcBlockConfig {
										encoding: Some(UiTransactionEncoding::JsonParsed),
										transaction_details: Some(TransactionDetails::None),
										rewards: Some(false),
										commitment: Some(CommitmentConfig::finalized()),
										max_supported_transaction_version: None,
									},
								)
								.await?;

							let block_hash = block.blockhash;
							Ok((
								SolHash::from_str(&block_hash).expect("Invalid block hash"),
								if index == 0 {
									None
								} else {
									Some(
										SolHash::from_str(&block.previous_blockhash)
											.expect("Invalid parent block hash"),
									)
								},
							))
						}

						let (block_hash, block_parent_hash) =
							get_block_details(&client, *witness_range.end()).await?;

						Ok(Header {
							index: witness_period::block_witness_root(witness_period, index),
							hash: block_hash,
							parent_hash: {
								if witness_range.end() == witness_range.start() {
									block_parent_hash
								} else {
									let (_, parent_block_hash) =
										get_block_details(&client, *witness_range.start()).await?;
									parent_block_hash
								}
							},
							data: (),
						})
					})
				}),
			)
			.await
	}
}

#[cfg(test)]
pub mod mocks {

	use super::*;
	use mockall::mock;

	mock! {
		pub SolRetryRpcClient {}

		impl Clone for SolRetryRpcClient {
			fn clone(&self) -> Self;
		}

		#[async_trait::async_trait]
		impl SolRetryRpcApi for SolRetryRpcClient {
			async fn get_block(&self, slot: u64, config: RpcBlockConfig) -> UiConfirmedBlock;
			async fn get_slot(&self, commitment: CommitmentConfig) -> u64; // Slot
			async fn get_recent_prioritization_fees(&self) -> Vec<RpcPrioritizationFee>;
			async fn get_multiple_accounts(
				&self,
				pubkeys: &[SolAddress],
				config: RpcAccountInfoConfig,
			) -> Response<Vec<Option<UiAccount>>>;
			async fn get_signature_statuses(
				&self,
				signatures: &[SolSignature],
				search_transaction_history: bool,
			) -> Response<Vec<Option<TransactionStatus>>>;

			async fn get_transaction(
				&self,
				signature: &SolSignature,
				config: RpcTransactionConfig,
			) -> EncodedConfirmedTransactionWithStatusMeta;

			async fn broadcast_transaction(&self, transaction: Vec<u8>)
				-> anyhow::Result<SolSignature>;
		}
	}
}

#[cfg(test)]
mod tests {
	use cf_chains::Chain;
	use futures::FutureExt;
	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	async fn test_sol_retry_rpc() {
		task_scope(|scope| {
			async move {
				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							ws_endpoint: "wss://api.testnet.solana.com".into(),
							http_endpoint: "https://api.testnet.solana.com".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let slot = retry_client.get_slot(CommitmentConfig::finalized()).await;
				println!("slot: {:?}", slot);

				let priority_fees = retry_client.get_recent_prioritization_fees().await;
				println!("priority_fees: {:?}", priority_fees[0]);

				let account_infos = retry_client
					.get_multiple_accounts(
						&[SolAddress::from_str("vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg")
							.unwrap()],
						RpcAccountInfoConfig {
							encoding: Some(UiAccountEncoding::JsonParsed),
							data_slice: None,
							commitment: Some(CommitmentConfig::finalized()),
							min_context_slot: None,
						},
					)
					.await;
				println!("account_info: {:?}", account_infos.value);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}

	#[tokio::test]
	async fn test_sol_get_transaction() {
		task_scope(|scope| {
			async move {
				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							ws_endpoint: "wss://api.devnet.solana.com".into(),
							http_endpoint: "https://api.devnet.solana.com".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let signature = SolSignature::from_str("4hWBYH3K7ia2q8Vfk9xCd1ovhRDQKaYKVUCsS9HKEEK2XTF2t2BP8q4AhbVihsqk7QyWiq4csXybBLVoJmMFo2Sf").unwrap();

				let transaction = retry_client
				.get_transaction(
					&signature,
					RpcTransactionConfig {
						encoding: Some(UiTransactionEncoding::JsonParsed),
						commitment: Some(CommitmentConfig::confirmed()),
						max_supported_transaction_version: Some(0),
					},
				)
				.await;
				println!("transaction: {:?}", transaction);

				let signature_status = retry_client
				.get_signature_statuses(
					&[signature],
					true
				).await;

				let confirmation_status = signature_status.value.first().and_then(Option::as_ref).and_then(|ts| ts.confirmation_status.as_ref()).expect("Expected confirmation_status to be Some");

				println!("Signature status: {:?}", signature_status);
				assert_eq!(confirmation_status, &TransactionConfirmationStatus::Finalized);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}

	#[tokio::test]
	#[ignore = "requires local node"]
	async fn test_sol_send_transaction() {
		task_scope(|scope| {
			async move {
				let retry_client = SolRetryRpcClient::new(
					scope,
					NodeContainer {
						primary: WsHttpEndpoints {
							ws_endpoint: "ws://localhost:8899".into(),
							http_endpoint: "http://localhost:8899".into(),
						},
						backup: None,
					},
					None,
					Solana::WITNESS_PERIOD,
				)
				.await
				.unwrap();

				let slot = retry_client.get_slot(CommitmentConfig::finalized()).await;
				println!("slot: {:?}", slot);

				// Transaction crafted after initializing localnet to get the blockchash/durable nonce.
				let signed_and_serialized_tx: Vec<u8> = hex::decode("018e2992131cfc9cb7efea3fcd2de3f026b9aa8ebc769bd241f969f1855c6b7fc1056e285e036128fbbf6ec334af4a5dfab0e4c59d7fd8de9a6cedd983ada0160b01000205a33ba00d6cffc2096e0cab5268ed0b692d36cc85bbf686bf1bd756c6221cf39017eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192603ca4135c27d0ea590412b95b436c0c288805dc88e49584cd3eb85da41e0f60000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000009fc96b14ae0b5ee9383cdacca044b8641f9509be9e1b71aa0e63825ed3a5ba210203030104000404000000030200020c0200000000ca9a3b00000000").expect("Decoding failed");

				// Checking that encoding and sending the transaction works.
				let tx_signature = retry_client
				.broadcast_transaction(signed_and_serialized_tx).await.unwrap();

				println!("tx_signature: {:?}", tx_signature);

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}
}
