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

use anyhow::anyhow;
use ethers::types::{Address, Bloom, Bytes, H160, H256, U256, U64};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tron address is 21 bytes: 0x41 prefix + 20 bytes (EVM address)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TronAddress(pub [u8; 21]);

impl TronAddress {
	// Convert EVM address (H160/20 bytes) to Tron address (21 bytes)
	// by prepending 0x41
	pub fn from_evm_address(evm_address: H160) -> Self {
		let mut tron_address = [0u8; 21];
		tron_address[0] = 0x41;
		tron_address[1..].copy_from_slice(evm_address.as_bytes());
		TronAddress(tron_address)
	}

	// Convert Tron address (21 bytes) to EVM address (H160/20 bytes)
	// by removing the 0x41 prefix
	pub fn to_evm_address(&self) -> anyhow::Result<H160> {
		if self.0[0] == 0x41 {
			Ok(H160::from_slice(&self.0[1..]))
		} else {
			Err(anyhow!("Invalid Tron address: expected 0x41 prefix"))
		}
	}
}

// Serialize as hex string for JSON (without 0x prefix)
impl serde::Serialize for TronAddress {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		serializer.serialize_str(&hex::encode(self.0))
	}
}

// Deserialize from hex string (expects 42 characters: "41" + 40 hex chars)
impl<'de> serde::Deserialize<'de> for TronAddress {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		let hex_str = s.strip_prefix("0x").unwrap_or(&s);

		if hex_str.len() != 42 {
			return Err(serde::de::Error::custom(format!(
				"Invalid hex length: expected 42, got {}",
				hex_str.len()
			)));
		}

		let bytes = hex::decode(hex_str)
			.map_err(|e| serde::de::Error::custom(format!("Failed to decode hex: {}", e)))?;

		let mut address = [0u8; 21];
		address.copy_from_slice(&bytes);
		Ok(TronAddress(address))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBalanceTrace {
	pub operation_identifier: i64,
	pub address: TronAddress,
	pub amount: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockBalance {
	pub timestamp: i64,
	pub block_identifier: BlockIdentifier,
	pub transaction_balance_trace: Vec<TransactionBalanceTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIdentifier {
	pub hash: String,
	pub number: Option<BlockNumber>,
}

pub type BlockNumber = i64;
pub type Amount = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionBalanceTrace {
	pub transaction_identifier: H256,
	pub operation: Vec<BlockBalanceTrace>,
	#[serde(rename = "type")]
	pub type_field: String,
	pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
	pub id: String,
	pub fee: Option<Amount>,
	#[serde(rename = "blockNumber")]
	pub block_number: BlockNumber,
	#[serde(rename = "blockTimeStamp")]
	pub block_time_stamp: i64,
	#[serde(rename = "contractResult")]
	pub contract_result: Option<Vec<String>>,
	pub contract_address: Option<String>,
	pub receipt: ResourceReceipt,
	pub log: Option<Vec<Value>>,
	pub result: Option<String>,
	#[serde(rename = "resMessage")]
	pub res_message: Option<String>,
	#[serde(rename = "assetIssueID")]
	pub asset_issue_id: Option<String>,
	pub withdraw_amount: Option<Amount>,
	pub unfreeze_amount: Option<Amount>,
	pub internal_transactions: Option<Vec<Value>>,
	pub withdraw_expire_amount: Option<Amount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransactionResultSimulation {
	Ret { ret: String },
	Empty(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TransactionResult {
	ContractRet {
		#[serde(rename = "contractRet")]
		contract_ret: String,
	},
	Empty(Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction<TransactionResultType = TransactionResult> {
	#[serde(rename = "txID")]
	pub tx_id: H256,
	pub raw_data: RawData,
	pub raw_data_hex: String,
	pub ret: Option<Vec<TransactionResultType>>,
	pub signature: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionResultStatus {
	Success,
	Failure,
	Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawData {
	pub data: Option<String>,
	pub contract: Vec<Value>,
	pub ref_block_bytes: String,
	pub ref_block_hash: String,
	pub expiration: i64,
	pub timestamp: i64,
	pub fee_limit: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BroadcastResponse {
	pub result: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub txid: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub message: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConstantContractRequest {
	pub owner_address: TronAddress,
	pub contract_address: TronAddress,
	pub function_selector: String,
	pub parameter: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerSmartContractRequest {
	pub owner_address: TronAddress,
	pub contract_address: TronAddress,
	pub function_selector: String,
	pub parameter: Vec<u8>,
	pub fee_limit: i64,
}

/// Response from triggersmartcontract - contains unsigned transaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionExtention {
	pub transaction: Transaction,
	pub result: TransactionExtentionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConstantContractResult {
	pub result: TransactionExtentionResult,
	pub energy_used: i64,
	pub energy_penalty: Option<i64>,
	pub constant_result: Vec<String>,
	pub transaction: Transaction<TransactionResultSimulation>,
}

impl Transaction<TransactionResultSimulation> {
	// Only return success if all results are successful.
	pub fn status(&self) -> TransactionResultStatus {
		match &self.ret {
			Some(results) => {
				for result in results {
					match result {
						TransactionResultSimulation::Ret { ret } =>
							if ret != "SUCCESS" {
								return TransactionResultStatus::Failure;
							},
						// Empty is equivalent to success for simulations
						TransactionResultSimulation::Empty(_) => {
							continue;
						},
					}
				}
				TransactionResultStatus::Success
			},
			None => TransactionResultStatus::Unknown,
		}
	}
}

impl Transaction<TransactionResult> {
	// Only return success if all results are successful.
	pub fn status(&self) -> TransactionResultStatus {
		match &self.ret {
			Some(results) => {
				for result in results {
					match result {
						TransactionResult::ContractRet { contract_ret } => {
							if contract_ret != "SUCCESS" {
								return TransactionResultStatus::Failure;
							}
						},
						// This is considered a Failure for TransactionResult
						TransactionResult::Empty(_) => {
							return TransactionResultStatus::Failure;
						},
					}
				}
				TransactionResultStatus::Success
			},
			None => TransactionResultStatus::Unknown,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionExtentionResult {
	pub result: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub message: Option<String>,
}

impl TransactionExtentionResult {
	pub fn ensure_success(&self, context: &str) -> anyhow::Result<()> {
		if self.result {
			return Ok(());
		}
		let mut details = Vec::new();
		if let Some(code) = &self.code {
			details.push(format!("code: {code}"));
		}
		if let Some(message) = &self.message {
			details.push(format!("message: {message}"));
		}
		if details.is_empty() {
			Err(anyhow::anyhow!("{context}"))
		} else {
			Err(anyhow::anyhow!("{context} ({})", details.join(", ")))
		}
	}
}

/// Response from wallet/estimateenergy. We cannot use
/// TransactionExtentionResult because when it fails the
/// result bool is not existent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionExtentionEnergyResult {
	// #[serde(skip_serializing_if = "Option::is_none")]
	pub result: Option<bool>,
	// #[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<Value>,
	// #[serde(skip_serializing_if = "Option::is_none")]
	pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimateEnergyResult {
	pub result: TransactionExtentionEnergyResult,
	pub energy_required: Option<i64>,
}

/// Tron block as returned by the HTTP API (e.g. `/getblockbynum`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TronBlock {
	#[serde(rename = "blockID")]
	pub block_id: H256,
	pub block_header: TronBlockHeader,
	#[serde(default)]
	pub transactions: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TronBlockHeader {
	pub raw_data: TronBlockHeaderRawData,
	pub witness_signature: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TronBlockHeaderRawData {
	pub number: BlockNumber,
	#[serde(rename = "parentHash")]
	pub parent_hash: H256,
	pub timestamp: i64,
	#[serde(rename = "txTrieRoot")]
	pub tx_trie_root: String,
	pub version: i64,
	pub witness_address: String,
}

/// Tron block as returned by the JSON-RPC endpoint.
///
/// Mirrors ethers' `Block<TX>` but with some small changes because some
/// specific TRON parameters break the ethers deserialization.
/// - State root is returned as "0x" which is not a valid H256, so we deserialize it as an
///   Option<String> and default to H256::zero() in the conversion.
/// - Consider making gas stuff and other ethers specific stuff optional?
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TronBlockRpc<TX> {
	pub hash: Option<H256>,
	#[serde(default, rename = "parentHash")]
	pub parent_hash: H256,
	#[serde(default, rename = "sha3Uncles")]
	pub uncles_hash: H256,
	#[serde(default, rename = "miner")]
	pub author: Option<Address>,
	/// Tron returns `"0x"` here which is not a valid H256.
	#[serde(default, rename = "stateRoot")]
	pub state_root: Option<String>,
	#[serde(default, rename = "transactionsRoot")]
	pub transactions_root: H256,
	#[serde(default, rename = "receiptsRoot")]
	pub receipts_root: H256,
	pub number: Option<U64>,
	#[serde(default, rename = "gasUsed")]
	pub gas_used: U256,
	#[serde(default, rename = "gasLimit")]
	pub gas_limit: U256,
	#[serde(default, rename = "extraData")]
	pub extra_data: Bytes,
	#[serde(rename = "logsBloom")]
	pub logs_bloom: Option<Bloom>,
	#[serde(default)]
	pub timestamp: U256,
	#[serde(default)]
	pub difficulty: U256,
	#[serde(rename = "totalDifficulty")]
	pub total_difficulty: Option<U256>,
	#[serde(default)]
	pub uncles: Vec<H256>,
	#[serde(bound = "TX: Serialize + serde::de::DeserializeOwned", default)]
	pub transactions: Vec<TX>,
	pub size: Option<U256>,
	#[serde(rename = "mixHash")]
	pub mix_hash: Option<H256>,
	pub nonce: Option<ethers::types::H64>,
	#[serde(rename = "baseFeePerGas")]
	pub base_fee_per_gas: Option<U256>,
}

impl<TX> TronBlockRpc<TX> {
	/// Convert into an ethers `Block<TX>`, defaulting `state_root` to `H256::zero()`.
	pub fn into_ethers_block(self) -> ethers::types::Block<TX> {
		ethers::types::Block {
			hash: self.hash,
			parent_hash: self.parent_hash,
			uncles_hash: self.uncles_hash,
			author: self.author,
			state_root: H256::zero(),
			transactions_root: self.transactions_root,
			receipts_root: self.receipts_root,
			number: self.number,
			gas_used: self.gas_used,
			gas_limit: self.gas_limit,
			extra_data: self.extra_data,
			logs_bloom: self.logs_bloom,
			timestamp: self.timestamp,
			difficulty: self.difficulty,
			total_difficulty: self.total_difficulty,
			seal_fields: vec![],
			uncles: self.uncles,
			transactions: self.transactions,
			size: self.size,
			mix_hash: self.mix_hash,
			nonce: self.nonce,
			base_fee_per_gas: self.base_fee_per_gas,
			blob_gas_used: None,
			excess_blob_gas: None,
			withdrawals_root: None,
			withdrawals: None,
			parent_beacon_block_root: None,
			other: Default::default(),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReceipt {
	pub energy_usage: Option<i64>,
	pub energy_fee: Option<i64>,
	pub origin_energy_usage: Option<i64>,
	pub energy_usage_total: Option<i64>,
	pub net_usage: Option<i64>,
	pub net_fee: Option<i64>,
	pub result: Option<String>,
	pub energy_penalty_total: Option<i64>,
}
