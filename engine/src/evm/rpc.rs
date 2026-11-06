// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

pub mod address_checker;
pub mod node_interface;

use alloy_consensus::{SignableTransaction, TxEnvelope};
use alloy_eips::{eip2718::Encodable2718, BlockId, BlockNumberOrTag};
use alloy_network::{AnyRpcBlock, AnyRpcTransaction, AnyTransactionReceipt};
use alloy_primitives::{Bytes, TxKind, B256, U64};
use alloy_rpc_types_eth::{
	FeeHistory, Filter, FilterBlockOption, Log, TransactionInput, TransactionRequest,
};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolCall;
use cf_utilities::redact_endpoint_secret::SecretUrl;
use futures_core::Future;
use serde::de::DeserializeOwned;
use serde_json::json;
use sp_core::{H160, H256, U256};

use crate::{
	constants::RPC_RETRY_CONNECTION_INTERVAL,
	evm::conversions::{IntoAlloy, IntoSpCore},
	rpc_utils,
};
use anyhow::{anyhow, bail, Result};
use cf_utilities::make_periodic_tick;
use std::{path::PathBuf, str::FromStr, sync::Arc, time::Instant};
use tokio::sync::Mutex;

use cf_utilities::read_clean_and_decode_hex_str_file;

struct NonceInfo {
	next_nonce: u64,
	requested_at: std::time::Instant,
}

#[derive(Clone)]
pub struct EvmRpcClient {
	http_client: reqwest::Client,
	endpoint: SecretUrl,
	chain_name: &'static str,
}

impl EvmRpcClient {
	pub fn new(
		http_endpoint: SecretUrl,
		expected_chain_id: u64,
		chain_name: &'static str,
	) -> anyhow::Result<impl Future<Output = Self>> {
		// Fail fast on a malformed endpoint rather than retrying forever below.
		url::Url::parse(http_endpoint.as_ref())?;
		let client = EvmRpcClient {
			http_client: reqwest::Client::builder().build()?,
			endpoint: http_endpoint.clone(),
			chain_name,
		};

		Ok(async move {
			// We don't want to return an error here. Returning an error means that we'll exit the
			// CFE. So on client creation we wait until we can be successfully connected to this EVM
			// chain's node. So the other chains are unaffected
			let mut poll_interval = make_periodic_tick(RPC_RETRY_CONNECTION_INTERVAL, true);
			loop {
				poll_interval.tick().await;
				match client.chain_id().await {
					Ok(chain_id) if chain_id == expected_chain_id.into() => break client,
					Ok(chain_id) => {
						tracing::error!(
							"Connected to {chain_name} node but with incorrect chain_id {chain_id}, expected {expected_chain_id} from {http_endpoint}. \
								Please check your CFE configuration file...",
						);
					},
					Err(e) => tracing::error!(
						"Cannot connect to an {chain_name:?} node at {http_endpoint} with error: {e}. \
							Please check your CFE configuration file. Retrying in {:?}...",
						poll_interval.period()
					),
				}
			}
		})
	}

	async fn request<T: DeserializeOwned>(
		&self,
		method: &str,
		params: serde_json::Value,
	) -> Result<T> {
		Ok(serde_json::from_value(
			rpc_utils::call_rpc_raw(
				&self.http_client,
				self.endpoint.as_ref(),
				method,
				Some(params),
			)
			.await?,
		)?)
	}

	/// Makes an `eth_call` to a contract and decodes the result.
	async fn call_contract<C: SolCall>(
		&self,
		contract_address: H160,
		call: C,
		block: BlockId,
	) -> Result<C::Return> {
		let request = TransactionRequest {
			to: Some(TxKind::Call(contract_address.into_alloy())),
			input: data_only_input(call.abi_encode()),
			..Default::default()
		};
		let output: Bytes = self.request("eth_call", json!([request, block])).await?;
		Ok(C::abi_decode_returns(&output)?)
	}
}

/// Sets `data` rather than `input`, which not all nodes support.
pub(crate) fn data_only_input(data: impl Into<Bytes>) -> TransactionInput {
	TransactionInput { input: None, data: Some(data.into()) }
}

/// Witnessing attributes logs to a contract purely on the basis of the filter, which is applied by
/// the node. This catches a faulty node or proxy that doesn't honour the filter, in which case the
/// whole response is rejected. It is not a defence against a dishonest node, which can fabricate
/// logs that match: that relies on witnessing consensus.
fn ensure_logs_match_filter(filter: &Filter, logs: &[Log]) -> Result<()> {
	for log in logs {
		if !filter.address.matches(&log.address()) {
			bail!("log emitted by {:?} does not match the address filter", log.address());
		}
		if let FilterBlockOption::AtBlockHash(block_hash) = filter.block_option {
			if log.block_hash != Some(block_hash) {
				bail!("log from block {:?}, expected {block_hash:?}", log.block_hash);
			}
		}
		if log.removed {
			bail!("log in tx {:?} was removed by a reorg", log.transaction_hash);
		}
	}
	Ok(())
}

#[async_trait::async_trait]
impl EvmRpcApi for EvmRpcClient {
	async fn estimate_gas(&self, req: &TransactionRequest) -> Result<U256> {
		self.request("eth_estimateGas", json!([req])).await
	}

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
		let logs: Vec<Log> = self.request("eth_getLogs", json!([filter])).await?;
		ensure_logs_match_filter(&filter, &logs)
			.map_err(|e| anyhow!("Rejecting {} eth_getLogs response: {e}", self.chain_name))?;
		Ok(logs)
	}

	async fn chain_id(&self) -> Result<U256> {
		self.request("eth_chainId", json!([])).await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> Result<AnyTransactionReceipt> {
		self.request::<Option<_>>("eth_getTransactionReceipt", json!([tx_hash]))
			.await?
			.ok_or_else(|| {
				anyhow!(
					"Getting {} transaction receipt for tx hash {tx_hash} returned None",
					self.chain_name
				)
			})
	}

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: u64) -> Result<AnyRpcBlock> {
		self.request::<Option<_>>(
			"eth_getBlockByNumber",
			json!([BlockNumberOrTag::Number(block_number), false]),
		)
		.await?
		.ok_or_else(|| {
			anyhow!(
				"Getting {} block for block number {block_number} returned None",
				self.chain_name
			)
		})
	}

	async fn block_by_hash(&self, block_hash: H256) -> Result<AnyRpcBlock> {
		self.request::<Option<_>>("eth_getBlockByHash", json!([block_hash, false]))
			.await?
			.ok_or_else(|| {
				anyhow!(
					"Getting {} block for block hash {block_hash} returned None",
					self.chain_name
				)
			})
	}

	async fn block_with_txs(&self, block_number: u64) -> Result<AnyRpcBlock> {
		self.request::<Option<_>>(
			"eth_getBlockByNumber",
			json!([BlockNumberOrTag::Number(block_number), true]),
		)
		.await?
		.ok_or_else(|| {
			anyhow!(
				"Getting {} block with txs for block number {block_number} returned None",
				self.chain_name
			)
		})
	}

	async fn fee_history(
		&self,
		block_count: u64,
		newest_block: BlockNumberOrTag,
		reward_percentiles: &[f64],
	) -> Result<FeeHistory> {
		self.request(
			"eth_feeHistory",
			json!([U64::from(block_count), newest_block, reward_percentiles]),
		)
		.await
	}

	async fn get_transaction(&self, tx_hash: H256) -> Result<AnyRpcTransaction> {
		self.request::<Option<_>>("eth_getTransactionByHash", json!([tx_hash]))
			.await?
			.ok_or_else(|| {
				anyhow!(
					"Getting {} transaction for tx hash {tx_hash} returned None",
					self.chain_name
				)
			})
	}

	async fn get_block_number(&self) -> Result<u64> {
		Ok(self.request::<U64>("eth_blockNumber", json!([])).await?.to())
	}

	async fn get_finalized_block_number(&self) -> Result<u64> {
		Ok(self
			.request::<Option<AnyRpcBlock>>(
				"eth_getBlockByNumber",
				json!([BlockNumberOrTag::Finalized, false]),
			)
			.await?
			.ok_or_else(|| anyhow!("Getting {} finalized block returned None", self.chain_name))?
			.header
			.number)
	}
}

#[derive(Clone)]
pub struct EvmRpcSigningClient {
	signer: PrivateKeySigner,
	rpc_client: EvmRpcClient,
	nonce_info: Arc<Mutex<Option<NonceInfo>>>,
	chain_name: &'static str,
}

impl EvmRpcSigningClient {
	pub fn new(
		private_key_file: PathBuf,
		http_endpoint: SecretUrl,
		expected_chain_id: u64,
		chain_name: &'static str,
	) -> Result<impl Future<Output = Self>> {
		let rpc_client_fut = EvmRpcClient::new(http_endpoint, expected_chain_id, chain_name)?;

		let signer = read_clean_and_decode_hex_str_file(
			&private_key_file,
			format!("{chain_name} Private Key").as_str(),
			|key| PrivateKeySigner::from_str(key).map_err(anyhow::Error::new),
		)?;

		tracing::info!(
			"Loaded {chain_name} signing key with address {:?}",
			signer.address().into_sp_core()
		);

		Ok(async move {
			let rpc_client = rpc_client_fut.await;
			Self { signer, nonce_info: Arc::new(Mutex::new(None)), rpc_client, chain_name }
		})
	}

	async fn get_next_nonce(&self) -> Result<u64> {
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
					.rpc_client
					.request::<U64>(
						"eth_getTransactionCount",
						json!([self.signer.address(), BlockNumberOrTag::Pending]),
					)
					.await?
					.to();
				nonce_info_lock
					.insert(NonceInfo { next_nonce: tx_count, requested_at: Instant::now() })
			},
		};

		let result = nonce_info.next_nonce;
		nonce_info.next_nonce += 1;
		Ok(result)
	}
}

/// Signs an EIP-1559 transaction, returning its EIP-2718 encoding.
fn sign_transaction(signer: &PrivateKeySigner, tx: TransactionRequest) -> Result<Bytes> {
	let unsigned_tx = tx
		.build_1559()
		.map_err(|e| anyhow!("Failed to build EIP-1559 transaction: {e}"))?;
	let signature = signer.sign_hash_sync(&unsigned_tx.signature_hash())?;
	Ok(TxEnvelope::from(unsigned_tx.into_signed(signature)).encoded_2718().into())
}

#[async_trait::async_trait]
pub trait EvmRpcApi: Send + Sync + Clone + 'static {
	async fn estimate_gas(&self, req: &TransactionRequest) -> Result<U256>;

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>>;

	async fn chain_id(&self) -> Result<U256>;

	async fn transaction_receipt(&self, tx_hash: H256) -> Result<AnyTransactionReceipt>;

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: u64) -> Result<AnyRpcBlock>;

	async fn block_by_hash(&self, block_hash: H256) -> Result<AnyRpcBlock>;

	async fn block_with_txs(&self, block_number: u64) -> Result<AnyRpcBlock>;

	async fn fee_history(
		&self,
		block_count: u64,
		newest_block: BlockNumberOrTag,
		reward_percentiles: &[f64],
	) -> Result<FeeHistory>;

	async fn get_transaction(&self, tx_hash: H256) -> Result<AnyRpcTransaction>;
	async fn get_block_number(&self) -> Result<u64>;

	/// Errors if the endpoint doesn't support the `finalized` tag. Callers must not fall back to
	/// `latest`, since that would silently drop the finality guarantee.
	async fn get_finalized_block_number(&self) -> Result<u64>;
}

#[async_trait::async_trait]
pub trait EvmSigningRpcApi: EvmRpcApi {
	fn address(&self) -> H160;

	/// The client sets the nonce. All other fields of an EIP-1559 transaction must be set.
	async fn send_transaction(&self, tx: TransactionRequest) -> Result<H256>;
}

#[async_trait::async_trait]
impl EvmRpcApi for EvmRpcSigningClient {
	async fn estimate_gas(&self, req: &TransactionRequest) -> Result<U256> {
		self.rpc_client.estimate_gas(req).await
	}

	async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>> {
		self.rpc_client.get_logs(filter).await
	}

	async fn chain_id(&self) -> Result<U256> {
		self.rpc_client.chain_id().await
	}

	async fn transaction_receipt(&self, tx_hash: H256) -> Result<AnyTransactionReceipt> {
		self.rpc_client.transaction_receipt(tx_hash).await
	}

	/// Gets block, returning error when either:
	/// - Request fails
	/// - Request succeeds, but doesn't return a block
	async fn block(&self, block_number: u64) -> Result<AnyRpcBlock> {
		self.rpc_client.block(block_number).await
	}

	async fn block_by_hash(&self, block_hash: H256) -> Result<AnyRpcBlock> {
		self.rpc_client.block_by_hash(block_hash).await
	}

	async fn block_with_txs(&self, block_number: u64) -> Result<AnyRpcBlock> {
		self.rpc_client.block_with_txs(block_number).await
	}

	async fn fee_history(
		&self,
		block_count: u64,
		newest_block: BlockNumberOrTag,
		reward_percentiles: &[f64],
	) -> Result<FeeHistory> {
		self.rpc_client.fee_history(block_count, newest_block, reward_percentiles).await
	}

	async fn get_transaction(&self, tx_hash: H256) -> Result<AnyRpcTransaction> {
		self.rpc_client.get_transaction(tx_hash).await
	}

	async fn get_block_number(&self) -> Result<u64> {
		self.rpc_client.get_block_number().await
	}

	async fn get_finalized_block_number(&self) -> Result<u64> {
		self.rpc_client.get_finalized_block_number().await
	}
}

#[async_trait::async_trait]
impl EvmSigningRpcApi for EvmRpcSigningClient {
	fn address(&self) -> H160 {
		self.signer.address().into_sp_core()
	}

	async fn send_transaction(&self, mut tx: TransactionRequest) -> Result<H256> {
		tx.nonce = Some(self.get_next_nonce().await?);

		let res = async {
			let signed_tx = sign_transaction(&self.signer, tx)?;
			self.rpc_client
				.request::<B256>("eth_sendRawTransaction", json!([signed_tx]))
				.await
				.map(IntoSpCore::into_sp_core)
		}
		.await;
		if res.is_err() {
			// Reset the nonce just in case (it will be re-requested during next broadcast)
			tracing::warn!("Resetting {} broadcaster nonce due to error", self.chain_name);
			*self.nonce_info.lock().await = None;
		}

		res
	}
}

#[cfg(test)]
mod tests {

	use crate::settings::Settings;

	use super::*;
	use alloy_eips::eip2930::AccessList;

	#[test]
	fn logs_must_match_filter() {
		let (contract, block_hash) = (H160::repeat_byte(1), H256::repeat_byte(2));
		let filter = Filter::new()
			.address(contract.into_alloy())
			.at_block_hash(block_hash.into_alloy());
		let log: Log = Log {
			inner: alloy_primitives::Log {
				address: contract.into_alloy(),
				data: Default::default(),
			},
			block_hash: Some(block_hash.into_alloy()),
			..Default::default()
		};
		let with_address = |address: H160| Log {
			inner: alloy_primitives::Log { address: address.into_alloy(), ..log.inner.clone() },
			..log.clone()
		};

		assert!(ensure_logs_match_filter(&filter, &[]).is_ok());
		assert!(ensure_logs_match_filter(&filter, &[log.clone(), log.clone()]).is_ok());
		assert!(ensure_logs_match_filter(
			&Filter::new()
				.address(vec![H160::zero().into_alloy(), contract.into_alloy()])
				.from_block(1u64)
				.to_block(2u64),
			std::slice::from_ref(&log)
		)
		.is_ok());

		for invalid in [
			with_address(H160::repeat_byte(3)),
			Log { block_hash: Some(H256::repeat_byte(3).into_alloy()), ..log.clone() },
			Log { block_hash: None, ..log.clone() },
			Log { removed: true, ..log.clone() },
		] {
			assert!(ensure_logs_match_filter(&filter, &[log.clone(), invalid]).is_err());
		}
	}

	#[test]
	fn signed_transaction_is_unchanged() {
		// Expected values were produced with ethers.js for the same key and transaction.
		let signer = PrivateKeySigner::from_str(
			"4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318",
		)
		.unwrap();
		assert_eq!(
			signer.address().into_sp_core(),
			H160::from_str("0x2c7536E3605D9C16a7a3D7b1898e529396a65c23").unwrap()
		);

		let signed_tx = sign_transaction(
			&signer,
			TransactionRequest {
				to: Some(TxKind::Call(H160::repeat_byte(0x11).into_alloy())),
				input: data_only_input(vec![1u8, 2, 3, 4]),
				chain_id: Some(1),
				value: Some(U256::from(12345u64).into_alloy()),
				max_fee_per_gas: Some(30_000_000_000),
				max_priority_fee_per_gas: Some(1_000_000_000),
				gas: Some(100_000),
				access_list: Some(AccessList::default()),
				from: Some(signer.address()),
				nonce: Some(7),
				transaction_type: Some(2),
				..Default::default()
			},
		)
		.unwrap();

		assert_eq!(
			hex::encode(&signed_tx),
			"02f8720107843b9aca008506fc23ac00830186a09411111111111111111111111111111111111111118230398401020304c080a02232298af35a0d22071c3cbcf41414c7d4b4b805087e99aa9fdb5c9e01fc2949a0645b21691cf26e770d83492b2b0367e0e92270da11eca8ab4c996ed80848eb1f"
		);
		assert_eq!(
			H256(sp_crypto_hashing::keccak_256(&signed_tx)),
			H256::from_str("0x08aa583ee1a6e9e16c18d853cbe15bc346ecd675136cb1a16313e198b4246b9f")
				.unwrap()
		);
	}

	#[tokio::test]
	#[ignore = "Requires correct settings"]
	async fn eth_rpc_test() {
		let settings = Settings::new_test().unwrap();

		let client = EvmRpcSigningClient::new(
			settings.eth.private_key_file,
			settings.eth.nodes.primary.http_endpoint,
			2u64,
			"Ethereum",
		)
		.unwrap()
		.await;
		let chain_id = client.chain_id().await.unwrap();
		println!("{:?}", chain_id);

		let block = client.block(0).await.unwrap();
		println!("{:?}", block);

		let block_with_txs = client.block_with_txs(0).await.unwrap();
		println!("{:?}", block_with_txs);

		let fee_history = client
			.fee_history(10, BlockNumberOrTag::Latest, &[10.0, 50.0, 90.0])
			.await
			.unwrap();
		println!("{:?}", fee_history);
	}
}
