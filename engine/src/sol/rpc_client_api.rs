use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::commitment_config::CommitmentConfig;
use crate::sol::option_serializer::OptionSerializer;
use cf_chains::sol::SolAddress as Pubkey;

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
#[derive(Deserialize, PartialEq, Eq, Clone, Default)]
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
	pub transactions: Option<Vec<serde_json::Value>>, // we should never get this
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub signatures: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rewards: Option<serde_json::Value>, // we should never get this
	pub block_time: Option<u64>, // unix_timestamp
	pub block_height: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionStatus {
	pub slot: u64,                                            // slot
	pub confirmations: Option<usize>,                         // None = rooted
	pub status: Result<serde_json::Value, serde_json::Value>, // Not defined for simplification
	pub err: Option<serde_json::Value>,                       // Not defined for simplification
	pub confirmation_status: Option<TransactionConfirmationStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionConfirmationStatus {
	Processed,
	Confirmed,
	Finalized,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionConfig {
	pub encoding: Option<UiTransactionEncoding>,
	#[serde(flatten)]
	pub commitment: Option<CommitmentConfig>,
	pub max_supported_transaction_version: Option<u8>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedConfirmedTransactionWithStatusMeta {
	pub slot: u64, // slot
	#[serde(flatten)]
	pub transaction: EncodedTransactionWithStatusMeta,
	pub block_time: Option<u64>, // Unix Timestamp
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum TransactionVersion {
	Legacy(serde_json::Value),
	Number(serde_json::Value),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TransactionBinaryEncoding {
	Base58,
	Base64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum EncodedTransaction {
	LegacyBinary(String), /* Old way of expressing base-58, retained for RPC backwards
	                       * compatibility */
	Binary(String, TransactionBinaryEncoding),
	Json(UiTransaction),
	Accounts(Vec<serde_json::Value>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedTransactionWithStatusMeta {
	pub transaction: EncodedTransaction, // Not used
	pub meta: Option<UiTransactionStatusMeta>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub version: Option<TransactionVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiInnerInstructions {
	/// Transaction instruction index
	pub index: u8,
	/// List of inner instructions
	pub instructions: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenAmount {
	pub ui_amount: Option<serde_json::Value>,
	pub decimals: u8,
	pub amount: serde_json::Value,
	pub ui_amount_string: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTransactionTokenBalance {
	pub account_index: u8,
	pub mint: String,
	pub ui_token_amount: UiTokenAmount,
	#[serde(
		default = "OptionSerializer::skip",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub owner: OptionSerializer<String>,
	#[serde(
		default = "OptionSerializer::skip",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub program_id: OptionSerializer<String>,
}

/// A duplicate representation of TransactionStatusMeta with `err` field
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTransactionStatusMeta {
	pub err: Option<Value>,
	pub status: Result<serde_json::Value, serde_json::Value>, /* This field is deprecated.  See https://github.com/solana-labs/solana/issues/9302 */
	pub fee: u64,
	pub pre_balances: Vec<u64>,
	pub post_balances: Vec<u64>,
	#[serde(
		default = "OptionSerializer::none",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub inner_instructions: OptionSerializer<Vec<UiInnerInstructions>>,
	#[serde(
		default = "OptionSerializer::none",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub log_messages: OptionSerializer<Vec<String>>,
	#[serde(
		default = "OptionSerializer::none",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub pre_token_balances: OptionSerializer<Vec<UiTransactionTokenBalance>>,
	#[serde(
		default = "OptionSerializer::none",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub post_token_balances: OptionSerializer<Vec<UiTransactionTokenBalance>>,
	#[serde(
		default = "OptionSerializer::none",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub rewards: OptionSerializer<Vec<serde_json::Value>>,
	#[serde(
		default = "OptionSerializer::skip",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub loaded_addresses: OptionSerializer<UiLoadedAddresses>,
	#[serde(
		default = "OptionSerializer::skip",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub return_data: OptionSerializer<UiTransactionReturnData>,
	#[serde(
		default = "OptionSerializer::skip",
		skip_serializing_if = "OptionSerializer::should_skip"
	)]
	pub compute_units_consumed: OptionSerializer<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiTransactionReturnData {
	pub program_id: String,
	pub data: (String, UiReturnDataEncoding),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum UiReturnDataEncoding {
	Base64,
}

/// A duplicate representation of LoadedAddresses
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLoadedAddresses {
	pub writable: Vec<String>,
	pub readonly: Vec<String>,
}

/// A duplicate representation of a Transaction for pretty JSON serialization
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTransaction {
	pub signatures: Vec<String>,
	pub message: UiMessage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum UiMessage {
	Parsed(UiParsedMessage),
	Raw(UiRawMessage),
}

/// A duplicate representation of a Message, in parsed format, for pretty JSON serialization
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiParsedMessage {
	pub account_keys: Vec<serde_json::Value>,
	pub recent_blockhash: String,
	pub instructions: Vec<serde_json::Value>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub address_table_lookups: Option<Vec<serde_json::Value>>,
}

/// A duplicate representation of a Message, in raw format, for pretty JSON serialization
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiRawMessage {
	pub header: MessageHeader,
	pub account_keys: Vec<String>,
	pub recent_blockhash: String,
	pub instructions: Vec<serde_json::Value>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub address_table_lookups: Option<Vec<serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeader {
	/// The number of signatures required for this message to be considered
	/// valid. The signers of those signatures must match the first
	/// `num_required_signatures` of [`Message::account_keys`].
	// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
	pub num_required_signatures: u8,

	/// The last `num_readonly_signed_accounts` of the signed keys are read-only
	/// accounts.
	pub num_readonly_signed_accounts: u8,

	/// The last `num_readonly_unsigned_accounts` of the unsigned keys are
	/// read-only accounts.
	pub num_readonly_unsigned_accounts: u8,
}
