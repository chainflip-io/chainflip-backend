use crate::sol::sol_tx_core::program::instruction::InstructionError;
use serde::{Deserialize, Serialize};

/// Reasons a transaction might be rejected.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum TransactionError {
	/// An account is already being processed in another transaction in a way
	/// that does not support parallelism
	#[cfg_attr(feature = "std", error("Account in use"))]
	AccountInUse,

	/// A `SolPubkey` appears twice in the transaction's `account_keys`.  Instructions can
	/// reference `SolPubkey`s more than once but the message must contain a list with no duplicate
	/// keys
	#[cfg_attr(feature = "std", error("Account loaded twice"))]
	AccountLoadedTwice,

	/// Attempt to debit an account but found no record of a prior credit.
	#[cfg_attr(
		feature = "std",
		error("Attempt to debit an account but found no record of a prior credit.")
	)]
	AccountNotFound,

	/// Attempt to load a program that does not exist
	#[cfg_attr(feature = "std", error("Attempt to load a program that does not exist"))]
	ProgramAccountNotFound,

	/// The from `SolPubkey` does not have sufficient balance to pay the fee to schedule the
	/// transaction
	#[cfg_attr(feature = "std", error("Insufficient funds for fee"))]
	InsufficientFundsForFee,

	/// This account may not be used to pay transaction fees
	#[cfg_attr(feature = "std", error("This account may not be used to pay transaction fees"))]
	InvalidAccountForFee,

	/// The bank has seen this transaction before. This can occur under normal operation
	/// when a UDP packet is duplicated, as a user error from a client not updating
	/// its `recent_blockhash`, or as a double-spend attack.
	#[cfg_attr(feature = "std", error("This transaction has already been processed"))]
	AlreadyProcessed,

	/// The bank has not seen the given `recent_blockhash` or the transaction is too old and
	/// the `recent_blockhash` has been discarded.
	#[cfg_attr(feature = "std", error("Blockhash not found"))]
	BlockhashNotFound,

	/// An error occurred while processing an instruction. The first element of the tuple
	/// indicates the instruction index in which the error occurred.
	#[cfg_attr(feature = "std", error("Error processing Instruction {0}: {1}"))]
	InstructionError(u8, InstructionError),

	/// Loader call chain is too deep
	#[cfg_attr(feature = "std", error("Loader call chain is too deep"))]
	CallChainTooDeep,

	/// Transaction requires a fee but has no signature present
	#[cfg_attr(feature = "std", error("Transaction requires a fee but has no signature present"))]
	MissingSignatureForFee,

	/// Transaction contains an invalid account reference
	#[cfg_attr(feature = "std", error("Transaction contains an invalid account reference"))]
	InvalidAccountIndex,

	/// Transaction did not pass signature verification
	#[cfg_attr(feature = "std", error("Transaction did not pass signature verification"))]
	SignatureFailure,

	/// This program may not be used for executing instructions
	#[cfg_attr(feature = "std", error("This program may not be used for executing instructions"))]
	InvalidProgramForExecution,

	/// Transaction failed to sanitize accounts offsets correctly
	/// implies that account locks are not taken for this TX, and should
	/// not be unlocked.
	#[cfg_attr(
		feature = "std",
		error("Transaction failed to sanitize accounts offsets correctly")
	)]
	SanitizeFailure,

	#[cfg_attr(
		feature = "std",
		error("Transactions are currently disabled due to cluster maintenance")
	)]
	ClusterMaintenance,

	/// Transaction processing left an account with an outstanding borrowed reference
	#[cfg_attr(
		feature = "std",
		error("Transaction processing left an account with an outstanding borrowed reference")
	)]
	AccountBorrowOutstanding,

	/// Transaction would exceed max Block Cost Limit
	#[cfg_attr(feature = "std", error("Transaction would exceed max Block Cost Limit"))]
	WouldExceedMaxBlockCostLimit,

	/// Transaction version is unsupported
	#[cfg_attr(feature = "std", error("Transaction version is unsupported"))]
	UnsupportedVersion,

	/// Transaction loads a writable account that cannot be written
	#[cfg_attr(
		feature = "std",
		error("Transaction loads a writable account that cannot be written")
	)]
	InvalidWritableAccount,

	/// Transaction would exceed max account limit within the block
	#[cfg_attr(
		feature = "std",
		error("Transaction would exceed max account limit within the block")
	)]
	WouldExceedMaxAccountCostLimit,

	/// Transaction would exceed account data limit within the block
	#[cfg_attr(
		feature = "std",
		error("Transaction would exceed account data limit within the block")
	)]
	WouldExceedAccountDataBlockLimit,

	/// Transaction locked too many accounts
	#[cfg_attr(feature = "std", error("Transaction locked too many accounts"))]
	TooManyAccountLocks,

	/// Address lookup table not found
	#[cfg_attr(
		feature = "std",
		error("Transaction loads an address table account that doesn't exist")
	)]
	AddressLookupTableNotFound,

	/// Attempted to lookup addresses from an account owned by the wrong program
	#[cfg_attr(
		feature = "std",
		error("Transaction loads an address table account with an invalid owner")
	)]
	InvalidAddressLookupTableOwner,

	/// Attempted to lookup addresses from an invalid account
	#[cfg_attr(
		feature = "std",
		error("Transaction loads an address table account with invalid data")
	)]
	InvalidAddressLookupTableData,

	/// Address table lookup uses an invalid index
	#[cfg_attr(feature = "std", error("Transaction address table lookup uses an invalid index"))]
	InvalidAddressLookupTableIndex,

	/// Transaction leaves an account with a lower balance than rent-exempt minimum
	#[cfg_attr(
		feature = "std",
		error("Transaction leaves an account with a lower balance than rent-exempt minimum")
	)]
	InvalidRentPayingAccount,

	/// Transaction would exceed max Vote Cost Limit
	#[cfg_attr(feature = "std", error("Transaction would exceed max Vote Cost Limit"))]
	WouldExceedMaxVoteCostLimit,

	/// Transaction would exceed total account data limit
	#[cfg_attr(feature = "std", error("Transaction would exceed total account data limit"))]
	WouldExceedAccountDataTotalLimit,

	/// Transaction contains a duplicate instruction that is not allowed
	#[cfg_attr(
		feature = "std",
		error("Transaction contains a duplicate instruction ({0}) that is not allowed")
	)]
	DuplicateInstruction(u8),

	/// Transaction results in an account with insufficient funds for rent
	#[cfg_attr(
		feature = "std",
		error(
			"Transaction results in an account ({account_index}) with insufficient funds for rent"
		)
	)]
	InsufficientFundsForRent { account_index: u8 },

	/// Transaction exceeded max loaded accounts data size cap
	#[cfg_attr(feature = "std", error("Transaction exceeded max loaded accounts data size cap"))]
	MaxLoadedAccountsDataSizeExceeded,

	/// LoadedAccountsDataSizeLimit set for transaction must be greater than 0.
	#[cfg_attr(
		feature = "std",
		error("LoadedAccountsDataSizeLimit set for transaction must be greater than 0.")
	)]
	InvalidLoadedAccountsDataSizeLimit,

	/// Sanitized transaction differed before/after feature activiation. Needs to be resanitized.
	#[cfg_attr(feature = "std", error("ResanitizationNeeded"))]
	ResanitizationNeeded,

	/// Program execution is temporarily restricted on an account.
	#[cfg_attr(feature = "std", error("Execution of the program referenced by account at index {account_index} is temporarily restricted."))]
	ProgramExecutionTemporarilyRestricted { account_index: u8 },

	/// The total balance before the transaction does not equal the total balance after the
	/// transaction
	#[cfg_attr(
		feature = "std",
		error("Sum of account balances before and after transaction do not match")
	)]
	UnbalancedTransaction,
}
