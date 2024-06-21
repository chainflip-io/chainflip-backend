use crate::sol::sol_tx_core::program::instruction::InstructionError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Reasons a transaction might be rejected.
#[derive(Error, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum TransactionError {
	/// An account is already being processed in another transaction in a way
	/// that does not support parallelism
	#[error("Account in use")]
	AccountInUse,

	/// A `SolPubkey` appears twice in the transaction's `account_keys`.  Instructions can
	/// reference `SolPubkey`s more than once but the message must contain a list with no duplicate
	/// keys
	#[error("Account loaded twice")]
	AccountLoadedTwice,

	/// Attempt to debit an account but found no record of a prior credit.
	#[error("Attempt to debit an account but found no record of a prior credit.")]
	AccountNotFound,

	/// Attempt to load a program that does not exist
	#[error("Attempt to load a program that does not exist")]
	ProgramAccountNotFound,

	/// The from `SolPubkey` does not have sufficient balance to pay the fee to schedule the
	/// transaction
	#[error("Insufficient funds for fee")]
	InsufficientFundsForFee,

	/// This account may not be used to pay transaction fees
	#[error("This account may not be used to pay transaction fees")]
	InvalidAccountForFee,

	/// The bank has seen this transaction before. This can occur under normal operation
	/// when a UDP packet is duplicated, as a user error from a client not updating
	/// its `recent_blockhash`, or as a double-spend attack.
	#[error("This transaction has already been processed")]
	AlreadyProcessed,

	/// The bank has not seen the given `recent_blockhash` or the transaction is too old and
	/// the `recent_blockhash` has been discarded.
	#[error("Blockhash not found")]
	BlockhashNotFound,

	/// An error occurred while processing an instruction. The first element of the tuple
	/// indicates the instruction index in which the error occurred.
	#[error("Error processing Instruction {0}: {1}")]
	InstructionError(u8, InstructionError),

	/// Loader call chain is too deep
	#[error("Loader call chain is too deep")]
	CallChainTooDeep,

	/// Transaction requires a fee but has no signature present
	#[error("Transaction requires a fee but has no signature present")]
	MissingSignatureForFee,

	/// Transaction contains an invalid account reference
	#[error("Transaction contains an invalid account reference")]
	InvalidAccountIndex,

	/// Transaction did not pass signature verification
	#[error("Transaction did not pass signature verification")]
	SignatureFailure,

	/// This program may not be used for executing instructions
	#[error("This program may not be used for executing instructions")]
	InvalidProgramForExecution,

	/// Transaction failed to sanitize accounts offsets correctly
	/// implies that account locks are not taken for this TX, and should
	/// not be unlocked.
	#[error("Transaction failed to sanitize accounts offsets correctly")]
	SanitizeFailure,

	#[error("Transactions are currently disabled due to cluster maintenance")]
	ClusterMaintenance,

	/// Transaction processing left an account with an outstanding borrowed reference
	#[error("Transaction processing left an account with an outstanding borrowed reference")]
	AccountBorrowOutstanding,

	/// Transaction would exceed max Block Cost Limit
	#[error("Transaction would exceed max Block Cost Limit")]
	WouldExceedMaxBlockCostLimit,

	/// Transaction version is unsupported
	#[error("Transaction version is unsupported")]
	UnsupportedVersion,

	/// Transaction loads a writable account that cannot be written
	#[error("Transaction loads a writable account that cannot be written")]
	InvalidWritableAccount,

	/// Transaction would exceed max account limit within the block
	#[error("Transaction would exceed max account limit within the block")]
	WouldExceedMaxAccountCostLimit,

	/// Transaction would exceed account data limit within the block
	#[error("Transaction would exceed account data limit within the block")]
	WouldExceedAccountDataBlockLimit,

	/// Transaction locked too many accounts
	#[error("Transaction locked too many accounts")]
	TooManyAccountLocks,

	/// Address lookup table not found
	#[error("Transaction loads an address table account that doesn't exist")]
	AddressLookupTableNotFound,

	/// Attempted to lookup addresses from an account owned by the wrong program
	#[error("Transaction loads an address table account with an invalid owner")]
	InvalidAddressLookupTableOwner,

	/// Attempted to lookup addresses from an invalid account
	#[error("Transaction loads an address table account with invalid data")]
	InvalidAddressLookupTableData,

	/// Address table lookup uses an invalid index
	#[error("Transaction address table lookup uses an invalid index")]
	InvalidAddressLookupTableIndex,

	/// Transaction leaves an account with a lower balance than rent-exempt minimum
	#[error("Transaction leaves an account with a lower balance than rent-exempt minimum")]
	InvalidRentPayingAccount,

	/// Transaction would exceed max Vote Cost Limit
	#[error("Transaction would exceed max Vote Cost Limit")]
	WouldExceedMaxVoteCostLimit,

	/// Transaction would exceed total account data limit
	#[error("Transaction would exceed total account data limit")]
	WouldExceedAccountDataTotalLimit,

	/// Transaction contains a duplicate instruction that is not allowed
	#[error("Transaction contains a duplicate instruction ({0}) that is not allowed")]
	DuplicateInstruction(u8),

	/// Transaction results in an account with insufficient funds for rent
	#[error(
		"Transaction results in an account ({account_index}) with insufficient funds for rent"
	)]
	InsufficientFundsForRent { account_index: u8 },

	/// Transaction exceeded max loaded accounts data size cap
	#[error("Transaction exceeded max loaded accounts data size cap")]
	MaxLoadedAccountsDataSizeExceeded,

	/// LoadedAccountsDataSizeLimit set for transaction must be greater than 0.
	#[error("LoadedAccountsDataSizeLimit set for transaction must be greater than 0.")]
	InvalidLoadedAccountsDataSizeLimit,

	/// Sanitized transaction differed before/after feature activiation. Needs to be resanitized.
	#[error("ResanitizationNeeded")]
	ResanitizationNeeded,

	/// Program execution is temporarily restricted on an account.
	#[error("Execution of the program referenced by account at index {account_index} is temporarily restricted.")]
	ProgramExecutionTemporarilyRestricted { account_index: u8 },

	/// The total balance before the transaction does not equal the total balance after the
	/// transaction
	#[error("Sum of account balances before and after transaction do not match")]
	UnbalancedTransaction,
}
