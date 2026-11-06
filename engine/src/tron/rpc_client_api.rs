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

use crate::evm::conversions::IntoAlloy;
use alloy_network::{AnyHeader, AnyRpcBlock};
use alloy_primitives::{Bloom, Bytes, B256, B64, U64};
use alloy_rpc_types_eth::{BlockTransactions, Header};
use alloy_serde::WithOtherFields;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sp_core::{H160, H256, U256};

pub use cf_chains::tron::TronAddress;

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
	pub hash: H256,
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
	pub result: Option<bool>,
	pub code: Option<Value>,
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
/// Deserialized leniently because some TRON-specific values break the standard block
/// deserialization.
/// - State root is returned as "0x" which is not a valid H256, so we deserialize it as an
///   Option<String> and default to H256::zero() in the conversion.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TronBlockRpc {
	pub hash: Option<H256>,
	#[serde(default, rename = "parentHash")]
	pub parent_hash: H256,
	#[serde(default, rename = "sha3Uncles")]
	pub uncles_hash: H256,
	#[serde(default, rename = "miner")]
	pub author: Option<H160>,
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
	#[serde(default)]
	pub transactions: Vec<H256>,
	pub size: Option<U256>,
	#[serde(rename = "mixHash")]
	pub mix_hash: Option<H256>,
	pub nonce: Option<B64>,
	#[serde(rename = "baseFeePerGas")]
	pub base_fee_per_gas: Option<U256>,
}

impl TronBlockRpc {
	/// Converts into a standard block, defaulting `state_root` to zero.
	pub fn into_alloy_block(self) -> anyhow::Result<AnyRpcBlock> {
		let saturating_u64 = |value: U256| value.into_alloy().saturating_to::<u64>();
		Ok(AnyRpcBlock::new(WithOtherFields::new(alloy_rpc_types_eth::Block {
			header: Header {
				hash: self.hash.ok_or_else(|| anyhow::anyhow!("No block hash"))?.into_alloy(),
				inner: AnyHeader {
					parent_hash: self.parent_hash.into_alloy(),
					ommers_hash: self.uncles_hash.into_alloy(),
					beneficiary: self.author.unwrap_or_default().into_alloy(),
					state_root: B256::ZERO,
					transactions_root: self.transactions_root.into_alloy(),
					receipts_root: self.receipts_root.into_alloy(),
					// Without a bloom, logs must always be fetched.
					logs_bloom: self.logs_bloom.unwrap_or(Bloom::repeat_byte(0xFF)),
					difficulty: self.difficulty.into_alloy(),
					number: self.number.ok_or_else(|| anyhow::anyhow!("No block number"))?.to(),
					gas_limit: saturating_u64(self.gas_limit),
					gas_used: saturating_u64(self.gas_used),
					timestamp: saturating_u64(self.timestamp),
					extra_data: self.extra_data,
					mix_hash: self.mix_hash.map(IntoAlloy::into_alloy),
					nonce: self.nonce,
					base_fee_per_gas: self.base_fee_per_gas.map(saturating_u64),
					withdrawals_root: None,
					blob_gas_used: None,
					excess_blob_gas: None,
					parent_beacon_block_root: None,
					requests_hash: None,
				},
				total_difficulty: self.total_difficulty.map(IntoAlloy::into_alloy),
				size: self.size.map(IntoAlloy::into_alloy),
			},
			uncles: self.uncles.into_alloy(),
			transactions: BlockTransactions::Hashes(self.transactions.into_alloy()),
			withdrawals: None,
		})))
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::evm::conversions::IntoSpCore;
	use alloy_network::AnyTransactionReceipt;
	use std::str::FromStr;

	// Responses from the Nile testnet JSON-RPC endpoint.
	const RECEIPT: &str = r#"{
			"blockHash": "0x0000000003dff370907b4e8a8c49cda42c2aa2940170eee61aeb8782cf508210",
			"blockNumber": "0x3dff370",
			"contractAddress": null,
			"cumulativeGasUsed": "0x3124b",
			"effectiveGasPrice": "0x64",
			"from": "0xf10b2b8efd89cb89c3c43b54d628b4f15302233e",
			"gasUsed": "0x393a",
			"logs": [
				{
					"address": "0xeca9bc828a3005b9a3b909f2cc5c2a54794de05f",
					"blockHash": "0x0000000003dff370907b4e8a8c49cda42c2aa2940170eee61aeb8782cf508210",
					"blockNumber": "0x3dff370",
					"blockTimestamp": "0x6996c336",
					"data": "0x0000000000000000000000000000000000000000000000000000000000000032",
					"logIndex": "0x1",
					"removed": false,
					"topics": [
						"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
						"0x000000000000000000000000f10b2b8efd89cb89c3c43b54d628b4f15302233e",
						"0x00000000000000000000000015208ef33a926919ed270e2fa61367b2da3753da"
					],
					"transactionHash": "0xbd17efdc7bd30e3887a3af59454bbe219ba53a4d91040aae4c0948edda586c0f",
					"transactionIndex": "0x2"
				}
			],
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			"status": "0x1",
			"to": "0xeca9bc828a3005b9a3b909f2cc5c2a54794de05f",
			"transactionHash": "0xbd17efdc7bd30e3887a3af59454bbe219ba53a4d91040aae4c0948edda586c0f",
			"transactionIndex": "0x2",
			"type": "0x0"
		}"#;

	const BLOCK: &str = r#"{
			"baseFeePerGas": "0x0",
			"difficulty": "0x0",
			"extraData": "0x",
			"gasLimit": "0x77359400",
			"gasUsed": "0x3124b",
			"hash": "0x0000000003dff370907b4e8a8c49cda42c2aa2940170eee61aeb8782cf508210",
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			"miner": "0x486201c73390eccebb20341053e1b2d91af40e79",
			"mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"nonce": "0x0000000000000000",
			"number": "0x3dff370",
			"parentHash": "0x0000000003dff36f4123c6f9008e8b1e5ea2cba110b558ca1e2f54565ff73294",
			"receiptsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"sha3Uncles": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"size": "0x668",
			"stateRoot": "0x",
			"timestamp": "0x6996c336",
			"totalDifficulty": "0x0",
			"transactions": [
				"0xf4b015e1a563e7512a841a9a082746f6daae6db4f61be77ec3293e56d41b3caf",
				"0x18414573959a019e5a57beee6fb8b284bb196ddee10be4009f09c633147c4fa9",
				"0xbd17efdc7bd30e3887a3af59454bbe219ba53a4d91040aae4c0948edda586c0f"
			],
			"transactionsRoot": "0xc1c888c8503210fa31a301e271a100c68ae0758dee9756c63391e7e6f64e571f",
			"uncles": []
		}"#;

	#[test]
	fn tron_json_rpc_responses_deserialize() {
		let receipt: AnyTransactionReceipt = serde_json::from_str(RECEIPT).unwrap();
		assert_eq!(
			receipt.from.into_sp_core(),
			H160::from_str("0xf10b2b8efd89cb89c3c43b54d628b4f15302233e").unwrap()
		);
		assert_eq!(
			receipt.to.unwrap().into_sp_core(),
			H160::from_str("0xeca9bc828a3005b9a3b909f2cc5c2a54794de05f").unwrap()
		);
		assert_eq!(receipt.inner.logs().len(), 1);

		let block =
			serde_json::from_str::<TronBlockRpc>(BLOCK).unwrap().into_alloy_block().unwrap();
		assert_eq!(block.header.number, 0x3dff370);
		assert_eq!(
			block.header.hash.into_sp_core(),
			H256::from_str("0x0000000003dff370907b4e8a8c49cda42c2aa2940170eee61aeb8782cf508210")
				.unwrap()
		);
		assert_eq!(
			block.header.parent_hash.into_sp_core(),
			H256::from_str("0x0000000003dff36f4123c6f9008e8b1e5ea2cba110b558ca1e2f54565ff73294")
				.unwrap()
		);
		assert_eq!(block.transactions.len(), 3);
	}
}
