use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::commitment_config::CommitmentConfig;
use cf_chains::sol::sol_tx_core::Pubkey;

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPrioritizationFee {
	pub slot: u64,
	pub prioritization_fee: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum UiAccountEncoding {
	Binary, // Legacy. Retained for RPC backwards compatibility
	Base58,
	Base64,
	JsonParsed,
	#[serde(rename = "base64+zstd")]
	Base64Zstd,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParsedAccount {
	pub program: String,
	pub parsed: Value,
	pub space: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum UiAccountData {
	LegacyBinary(String), // Legacy. Retained for RPC backwards compatibility
	Json(ParsedAccount),
	Binary(String, UiAccountEncoding),
}

/// An Account with data that is stored on chain
#[repr(C)]
// #[frozen_abi(digest = "HawRVHh7t4d3H3bitWHFt25WhhoDmbJMCfWdESQQoYEy")]
#[derive(Deserialize, PartialEq, Eq, Clone, Default /* ,  AbiExample */)]
#[serde(rename_all = "camelCase")]
pub struct Account {
	/// lamports in the account
	pub lamports: u64,
	/// data held in this account
	#[serde(with = "serde_bytes")]
	pub data: Vec<u8>,
	/// the program that owns this account. If executable, the program that loads this account.
	pub owner: Pubkey,
	/// this account's data contains a loaded program (and is now read-only)
	pub executable: bool,
	/// the epoch at which this account will next owe rent
	pub rent_epoch: u64,
}

/// A duplicate representation of an Account for pretty JSON serialization
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiAccount {
	pub lamports: u64,
	pub data: UiAccountData,
	pub owner: String,
	pub executable: bool,
	pub rent_epoch: u64,
	pub space: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDataSliceConfig {
	pub offset: usize,
	pub length: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcAccountInfoConfig {
	pub encoding: Option<UiAccountEncoding>,
	pub data_slice: Option<UiDataSliceConfig>,
	pub commitment: Option<CommitmentConfig>,
	pub min_context_slot: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcResponseContext {
	pub slot: u64,
	// simplified as a string for now
	#[serde(skip_serializing_if = "Option::is_none")]
	pub api_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response<T> {
	pub context: RpcResponseContext,
	pub value: T,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum UiTransactionEncoding {
	Binary, // Legacy. Retained for RPC backwards compatibility
	Base64,
	Base58,
	Json,
	JsonParsed,
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionDetails {
	Full,
	Signatures,
	None,
	Accounts,
}

impl Default for TransactionDetails {
	fn default() -> Self {
		Self::Full
	}
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlockConfig {
	pub encoding: Option<UiTransactionEncoding>,
	pub transaction_details: Option<TransactionDetails>,
	pub rewards: Option<bool>,
	#[serde(flatten)]
	pub commitment: Option<CommitmentConfig>,
	pub max_supported_transaction_version: Option<u8>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiConfirmedBlock {
	pub previous_blockhash: String,
	pub blockhash: String,
	pub parent_slot: u64,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub transactions: Option<Vec<()>>, // we should never get this
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub signatures: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rewards: Option<()>, // we should never get this
	pub block_time: Option<u64>, // unix_timestamp
	pub block_height: Option<u64>,
}
