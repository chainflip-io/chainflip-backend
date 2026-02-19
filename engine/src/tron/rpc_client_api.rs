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
use ethers::types::{H160, H256};
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
	pub number: BlockNumber,
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
	pub fee: Amount,
	#[serde(rename = "blockNumber")]
	pub block_number: BlockNumber,
	#[serde(rename = "blockTimeStamp")]
	pub block_time_stamp: i64,
	#[serde(rename = "contractResult")]
	pub contract_result: Option<Vec<String>>,
	pub contract_address: Option<String>,
	pub receipt: Value,
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
pub struct TronTransaction {
	#[serde(rename = "txID")]
	pub tx_id: H256,
	pub raw_data: RawData,
	pub raw_data_hex: String,
	pub ret: Option<Vec<TransactionRet>>,
	pub signature: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRet {
	#[serde(rename = "contractRet")]
	pub contract_ret: String,
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

pub struct TronTransactionRequest {
	pub owner_address: TronAddress,
	pub contract_address: TronAddress,
	pub function_selector: String,
	pub parameter: Vec<u8>,
	pub fee_limit: i64,
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
	pub transaction: TronTransaction,
	pub result: TransactionExtentionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionExtentionResult {
	pub result: bool,
}

/// Data returned from trigger_smart_contract containing unsigned transaction information
#[derive(Debug, Clone)]
pub struct UnsignedTronTransaction {
	pub tx_id: H256,
	pub raw_data_hex: String,
	pub raw_data: Value,
}
