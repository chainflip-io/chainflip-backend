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

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::commitment_config::{CommitmentConfig, CommitmentLevel};
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
	pub transactions: Option<Vec<Value>>, // we should never get this
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub signatures: Option<Vec<String>>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub rewards: Option<Value>, // we should never get this
	pub block_time: Option<u64>, // unix_timestamp
	pub block_height: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionStatus {
	pub slot: u64,                    // slot
	pub confirmations: Option<usize>, // None = rooted
	pub status: Result<Value, Value>, // Not defined for simplification
	pub err: Option<Value>,           // Not defined for simplification
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlockhash {
	pub blockhash: String,
	pub last_valid_block_height: u64,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum InstructionError {
	/// Deprecated! Use CustomError instead!
	/// The program instruction returned an error
	GenericError,

	/// The arguments provided to a program were invalid
	InvalidArgument,

	/// An instruction's data contents were invalid
	InvalidInstructionData,

	/// An account's data contents was invalid
	InvalidAccountData,

	/// An account's data was too small
	AccountDataTooSmall,

	/// An account's balance was too small to complete the instruction
	InsufficientFunds,

	/// The account did not have the expected program id
	IncorrectProgramId,

	/// A signature was required but not found
	MissingRequiredSignature,

	/// An initialize instruction was sent to an account that has already been initialized.
	AccountAlreadyInitialized,

	/// An attempt to operate on an account that hasn't been initialized.
	UninitializedAccount,

	/// Program's instruction lamport balance does not equal the balance after the instruction
	UnbalancedInstruction,

	/// Program illegally modified an account's program id
	ModifiedProgramId,

	/// Program spent the lamports of an account that doesn't belong to it
	ExternalAccountLamportSpend,

	/// Program modified the data of an account that doesn't belong to it
	ExternalAccountDataModified,

	/// Read-only account's lamports modified
	ReadonlyLamportChange,

	/// Read-only account's data was modified
	ReadonlyDataModified,

	/// An account was referenced more than once in a single instruction
	// Deprecated, instructions can now contain duplicate accounts
	DuplicateAccountIndex,

	/// Executable bit on account changed, but shouldn't have
	ExecutableModified,

	/// Rent_epoch account changed, but shouldn't have
	RentEpochModified,

	/// The instruction expected additional account keys
	NotEnoughAccountKeys,

	/// Program other than the account's owner changed the size of the account data
	AccountDataSizeChanged,

	/// The instruction expected an executable account
	AccountNotExecutable,

	/// Failed to borrow a reference to account data, already borrowed
	AccountBorrowFailed,

	/// Account data has an outstanding reference after a program's execution
	AccountBorrowOutstanding,

	/// The same account was multiply passed to an on-chain program's entrypoint, but the program
	/// modified them differently.  A program can only modify one instance of the account because
	/// the runtime cannot determine which changes to pick or how to merge them if both are
	/// modified
	DuplicateAccountOutOfSync,

	/// Allows on-chain programs to implement program-specific error types and see them returned
	/// by the Solana runtime. A program-specific error may be any type that is represented as
	/// or serialized to a u32 integer.
	Custom(u32),

	/// The return value from the program was invalid.  Valid errors are either a defined builtin
	/// error value or a user-defined error in the lower 32 bits.
	InvalidError,

	/// Executable account's data was modified
	ExecutableDataModified,

	/// Executable account's lamports modified
	ExecutableLamportChange,

	/// Executable accounts must be rent exempt
	ExecutableAccountNotRentExempt,

	/// Unsupported program id
	UnsupportedProgramId,

	/// Cross-program invocation call depth too deep
	CallDepth,

	/// An account required by the instruction is missing
	MissingAccount,

	/// Cross-program invocation reentrancy not allowed for this instruction
	ReentrancyNotAllowed,

	/// Length of the seed is too long for address generation
	MaxSeedLengthExceeded,

	/// Provided seeds do not result in a valid address
	InvalidSeeds,

	/// Failed to reallocate account data of this length
	InvalidRealloc,

	/// Computational budget exceeded
	ComputationalBudgetExceeded,

	/// Cross-program invocation with unauthorized signer or writable account
	PrivilegeEscalation,

	/// Failed to create program execution environment
	ProgramEnvironmentSetupFailure,

	/// Program failed to complete
	ProgramFailedToComplete,

	/// Program failed to compile
	ProgramFailedToCompile,

	/// Account is immutable
	Immutable,

	/// Incorrect authority provided
	IncorrectAuthority,

	/// Failed to serialize or deserialize account data
	///
	/// Warning: This error should never be emitted by the runtime.
	///
	/// This error includes strings from the underlying 3rd party Borsh crate
	/// which can be dangerous because the error strings could change across
	/// Borsh versions. Only programs can use this error because they are
	/// consistent across Solana software versions.
	BorshIoError(String),

	/// An account does not have enough lamports to be rent-exempt
	AccountNotRentExempt,

	/// Invalid account owner
	InvalidAccountOwner,

	/// Program arithmetic overflowed
	ArithmeticOverflow,

	/// Unsupported sysvar
	UnsupportedSysvar,

	/// Illegal account owner
	IllegalOwner,

	/// Accounts data allocations exceeded the maximum allowed per transaction
	MaxAccountsDataAllocationsExceeded,

	/// Max accounts exceeded
	MaxAccountsExceeded,

	/// Max instruction trace length exceeded
	MaxInstructionTraceLengthExceeded,

	/// Builtin programs must consume compute units
	BuiltinProgramsMustConsumeComputeUnits,
	// Note: For any new error added here an equivalent ProgramError and its
	// conversions must also be added
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum TransactionError {
	/// An account is already being processed in another transaction in a way
	/// that does not support parallelism
	AccountInUse,

	/// A `Pubkey` appears twice in the transaction's `account_keys`.  Instructions can reference
	/// `Pubkey`s more than once but the message must contain a list with no duplicate keys
	AccountLoadedTwice,

	/// Attempt to debit an account but found no record of a prior credit.
	AccountNotFound,

	/// Attempt to load a program that does not exist
	ProgramAccountNotFound,

	/// The from `Pubkey` does not have sufficient balance to pay the fee to schedule the
	/// transaction
	InsufficientFundsForFee,

	/// This account may not be used to pay transaction fees
	InvalidAccountForFee,

	/// The bank has seen this transaction before. This can occur under normal operation
	/// when a UDP packet is duplicated, as a user error from a client not updating
	/// its `recent_blockhash`, or as a double-spend attack.
	AlreadyProcessed,

	/// The bank has not seen the given `recent_blockhash` or the transaction is too old and
	/// the `recent_blockhash` has been discarded.
	BlockhashNotFound,

	/// An error occurred while processing an instruction. The first element of the tuple
	/// indicates the instruction index in which the error occurred.
	InstructionError(u8, InstructionError),

	/// Loader call chain is too deep
	CallChainTooDeep,

	/// Transaction requires a fee but has no signature present
	MissingSignatureForFee,

	/// Transaction contains an invalid account reference
	InvalidAccountIndex,

	/// Transaction did not pass signature verification
	SignatureFailure,

	/// This program may not be used for executing instructions
	InvalidProgramForExecution,

	/// Transaction failed to sanitize accounts offsets correctly
	/// implies that account locks are not taken for this TX, and should
	/// not be unlocked.
	SanitizeFailure,

	ClusterMaintenance,

	/// Transaction processing left an account with an outstanding borrowed reference
	AccountBorrowOutstanding,

	/// Transaction would exceed max Block Cost Limit
	WouldExceedMaxBlockCostLimit,

	/// Transaction version is unsupported
	UnsupportedVersion,

	/// Transaction loads a writable account that cannot be written
	InvalidWritableAccount,

	/// Transaction would exceed max account limit within the block
	WouldExceedMaxAccountCostLimit,

	/// Transaction would exceed account data limit within the block
	WouldExceedAccountDataBlockLimit,

	/// Transaction locked too many accounts
	TooManyAccountLocks,

	/// Address lookup table not found
	AddressLookupTableNotFound,

	/// Attempted to lookup addresses from an account owned by the wrong program
	InvalidAddressLookupTableOwner,

	/// Attempted to lookup addresses from an invalid account
	InvalidAddressLookupTableData,

	/// Address table lookup uses an invalid index
	InvalidAddressLookupTableIndex,

	/// Transaction leaves an account with a lower balance than rent-exempt minimum
	InvalidRentPayingAccount,

	/// Transaction would exceed max Vote Cost Limit
	WouldExceedMaxVoteCostLimit,

	/// Transaction would exceed total account data limit
	WouldExceedAccountDataTotalLimit,

	/// Transaction contains a duplicate instruction that is not allowed
	DuplicateInstruction(u8),

	/// Transaction results in an account with insufficient funds for rent
	InsufficientFundsForRent {
		account_index: u8,
	},

	/// Transaction exceeded max loaded accounts data size cap
	MaxLoadedAccountsDataSizeExceeded,

	/// LoadedAccountsDataSizeLimit set for transaction must be greater than 0.
	InvalidLoadedAccountsDataSizeLimit,

	/// Sanitized transaction differed before/after feature activiation. Needs to be resanitized.
	ResanitizationNeeded,

	/// Program execution is temporarily restricted on an account.
	ProgramExecutionTemporarilyRestricted {
		account_index: u8,
	},

	/// The total balance before the transaction does not equal the total balance after the
	/// transaction
	UnbalancedTransaction,

	/// Program cache hit max limit.
	ProgramCacheHitMaxLimit,

	/// Commit cancelled internally.
	CommitCancelled,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcSimulateTransactionResult {
	pub err: Option<TransactionError>,
	pub logs: Option<Vec<String>>,
	pub accounts: Option<Vec<Option<UiAccount>>>,
	pub units_consumed: Option<u64>,
	pub return_data: Option<UiTransactionReturnData>,
	pub inner_instructions: Option<Vec<UiInnerInstructions>>,
	pub replacement_blockhash: Option<RpcBlockhash>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RpcSimulateTransactionAccountsConfig {
	pub encoding: Option<UiAccountEncoding>,
	pub addresses: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSimulateTransactionConfig {
	#[serde(default)]
	pub sig_verify: bool,
	#[serde(default)]
	pub replace_recent_blockhash: bool,
	#[serde(flatten)]
	pub commitment: Option<CommitmentConfig>,
	pub encoding: Option<UiTransactionEncoding>,
	pub accounts: Option<RpcSimulateTransactionAccountsConfig>,
	pub min_context_slot: Option<u64>, // slot
	#[serde(default)]
	pub inner_instructions: bool,
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
	Legacy(Value),
	Number(Value),
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
	Accounts(Vec<Value>),
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
	pub instructions: Vec<Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenAmount {
	pub ui_amount: Option<Value>,
	pub decimals: u8,
	pub amount: Value,
	pub ui_amount_string: Value,
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
	pub status: Result<Value, Value>, /* This field is deprecated.  See https://github.com/solana-labs/solana/issues/9302 */
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
	pub rewards: OptionSerializer<Vec<Value>>,
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
	pub account_keys: Vec<String>,
	pub recent_blockhash: String,
	pub instructions: Vec<Value>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub address_table_lookups: Option<Vec<Value>>,
}

/// A duplicate representation of a Message, in raw format, for pretty JSON serialization
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiRawMessage {
	pub header: MessageHeader,
	pub account_keys: Vec<String>,
	pub recent_blockhash: String,
	pub instructions: Vec<Value>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub address_table_lookups: Option<Vec<Value>>,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSendTransactionConfig {
	#[serde(default)]
	pub skip_preflight: bool,
	pub preflight_commitment: Option<CommitmentLevel>,
	pub encoding: Option<UiTransactionEncoding>,
	pub max_retries: Option<usize>,
	pub min_context_slot: Option<u64>, // slot
}
