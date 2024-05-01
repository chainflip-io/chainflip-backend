use crate::sol::{sol_tx_building_blocks::SolSignature, SolPubkey};
use ed25519_dalek::Signer as DalekSigner;
use rand0_7::{rngs::OsRng, CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A vanilla Ed25519 key pair
#[derive(Debug)]
pub struct Keypair(ed25519_dalek::Keypair);

impl Keypair {
	/// Can be used for generating a Keypair without a dependency on `rand` types
	pub const SECRET_KEY_LENGTH: usize = 32;

	/// Constructs a new, random `Keypair` using a caller-provided RNG
	pub fn generate<R>(csprng: &mut R) -> Self
	where
		R: CryptoRng + RngCore,
	{
		Self(ed25519_dalek::Keypair::generate(csprng))
	}

	/// Constructs a new, random `Keypair` using `OsRng`
	pub fn new() -> Self {
		let mut rng = OsRng;
		Self::generate(&mut rng)
	}

	/// Recovers a `Keypair` from a byte array
	pub fn from_bytes(bytes: &[u8]) -> Result<Self, ed25519_dalek::SignatureError> {
		let secret =
			ed25519_dalek::SecretKey::from_bytes(&bytes[..ed25519_dalek::SECRET_KEY_LENGTH])?;
		let public =
			ed25519_dalek::PublicKey::from_bytes(&bytes[ed25519_dalek::SECRET_KEY_LENGTH..])?;
		let expected_public = ed25519_dalek::PublicKey::from(&secret);
		(public == expected_public)
			.then_some(Self(ed25519_dalek::Keypair { secret, public }))
			.ok_or(ed25519_dalek::SignatureError::from_source(String::from(
				"keypair bytes do not specify same pubkey as derived from their secret key",
			)))
	}

	/// Returns this `Keypair` as a byte array
	pub fn to_bytes(&self) -> [u8; 64] {
		self.0.to_bytes()
	}

	/// Recovers a `Keypair` from a base58-encoded string
	pub fn from_base58_string(s: &str) -> Self {
		Self::from_bytes(&bs58::decode(s).into_vec().unwrap()).unwrap()
	}

	/// Returns this `Keypair` as a base58-encoded string
	pub fn to_base58_string(&self) -> String {
		bs58::encode(&self.0.to_bytes()).into_string()
	}

	/// Gets this `Keypair`'s SecretKey
	pub fn secret(&self) -> &ed25519_dalek::SecretKey {
		&self.0.secret
	}

	/// Allows Keypair cloning
	///
	/// Note that the `Clone` trait is intentionally unimplemented because making a
	/// second copy of sensitive secret keys in memory is usually a bad idea.
	///
	/// Only use this in tests or when strictly required. Consider using [`std::sync::Arc<Keypair>`]
	/// instead.
	pub fn insecure_clone(&self) -> Self {
		Self(ed25519_dalek::Keypair {
			// This will never error since self is a valid keypair
			secret: ed25519_dalek::SecretKey::from_bytes(self.0.secret.as_bytes()).unwrap(),
			public: self.0.public,
		})
	}
}

impl Signer for Keypair {
	#[inline]
	fn pubkey(&self) -> SolPubkey {
		SolPubkey::from(self.0.public.to_bytes())
	}

	fn try_pubkey(&self) -> Result<SolPubkey, SignerError> {
		Ok(self.pubkey())
	}

	fn sign_message(&self, message: &[u8]) -> SolSignature {
		SolSignature::from(self.0.sign(message).to_bytes())
	}

	fn try_sign_message(&self, message: &[u8]) -> Result<SolSignature, SignerError> {
		Ok(self.sign_message(message))
	}

	fn is_interactive(&self) -> bool {
		false
	}
}

/// The `Signer` trait declares operations that all digital signature providers
/// must support. It is the primary interface by which signers are specified in
/// `Transaction` signing interfaces
pub trait Signer {
	/// Infallibly gets the implementor's public key. Returns the all-zeros
	/// `SolPubkey` if the implementor has none.
	fn pubkey(&self) -> SolPubkey {
		self.try_pubkey().unwrap_or_default()
	}
	/// Fallibly gets the implementor's public key
	fn try_pubkey(&self) -> Result<SolPubkey, SignerError>;
	/// Infallibly produces an Ed25519 signature over the provided `message`
	/// bytes. Returns the all-zeros `Signature` if signing is not possible.
	fn sign_message(&self, message: &[u8]) -> SolSignature {
		self.try_sign_message(message).unwrap_or_default()
	}
	/// Fallibly produces an Ed25519 signature over the provided `message` bytes.
	fn try_sign_message(&self, message: &[u8]) -> Result<SolSignature, SignerError>;
	/// Whether the implementation requires user interaction to sign
	fn is_interactive(&self) -> bool;
}

/// Convenience trait for working with mixed collections of `Signer`s
pub trait Signers {
	fn pubkeys(&self) -> Vec<SolPubkey>;
	fn try_pubkeys(&self) -> Result<Vec<SolPubkey>, SignerError>;
	fn sign_message(&self, message: &[u8]) -> Vec<SolSignature>;
	fn try_sign_message(&self, message: &[u8]) -> Result<Vec<SolSignature>, SignerError>;
	fn is_interactive(&self) -> bool;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignerError {
	#[error("keypair-pubkey mismatch")]
	KeypairPubkeyMismatch,

	#[error("not enough signers")]
	NotEnoughSigners,

	#[error("transaction error")]
	TransactionError(#[from] TransactionError),

	#[error("custom error: {0}")]
	Custom(String),

	// Presigner-specific Errors
	#[error("presigner error")]
	PresignerError(#[from] PresignerError),

	// Remote Keypair-specific Errors
	#[error("connection error: {0}")]
	Connection(String),

	#[error("invalid input: {0}")]
	InvalidInput(String),

	#[error("no device found")]
	NoDeviceFound,

	#[error("{0}")]
	Protocol(String),

	#[error("{0}")]
	UserCancel(String),

	#[error("too many signers")]
	TooManySigners,
}

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PresignerError {
	#[error("pre-generated signature cannot verify data")]
	VerificationFailure,
}

#[derive(Serialize, Deserialize, Debug, Error, PartialEq, Eq, Clone)]
pub enum InstructionError {
	/// Deprecated! Use CustomError instead!
	/// The program instruction returned an error
	#[error("generic instruction error")]
	GenericError,

	/// The arguments provided to a program were invalid
	#[error("invalid program argument")]
	InvalidArgument,

	/// An instruction's data contents were invalid
	#[error("invalid instruction data")]
	InvalidInstructionData,

	/// An account's data contents was invalid
	#[error("invalid account data for instruction")]
	InvalidAccountData,

	/// An account's data was too small
	#[error("account data too small for instruction")]
	AccountDataTooSmall,

	/// An account's balance was too small to complete the instruction
	#[error("insufficient funds for instruction")]
	InsufficientFunds,

	/// The account did not have the expected program id
	#[error("incorrect program id for instruction")]
	IncorrectProgramId,

	/// A signature was required but not found
	#[error("missing required signature for instruction")]
	MissingRequiredSignature,

	/// An initialize instruction was sent to an account that has already been initialized.
	#[error("instruction requires an uninitialized account")]
	AccountAlreadyInitialized,

	/// An attempt to operate on an account that hasn't been initialized.
	#[error("instruction requires an initialized account")]
	UninitializedAccount,

	/// Program's instruction lamport balance does not equal the balance after the instruction
	#[error("sum of account balances before and after instruction do not match")]
	UnbalancedInstruction,

	/// Program illegally modified an account's program id
	#[error("instruction illegally modified the program id of an account")]
	ModifiedProgramId,

	/// Program spent the lamports of an account that doesn't belong to it
	#[error("instruction spent from the balance of an account it does not own")]
	ExternalAccountLamportSpend,

	/// Program modified the data of an account that doesn't belong to it
	#[error("instruction modified data of an account it does not own")]
	ExternalAccountDataModified,

	/// Read-only account's lamports modified
	#[error("instruction changed the balance of a read-only account")]
	ReadonlyLamportChange,

	/// Read-only account's data was modified
	#[error("instruction modified data of a read-only account")]
	ReadonlyDataModified,

	/// An account was referenced more than once in a single instruction
	// Deprecated, instructions can now contain duplicate accounts
	#[error("instruction contains duplicate accounts")]
	DuplicateAccountIndex,

	/// Executable bit on account changed, but shouldn't have
	#[error("instruction changed executable bit of an account")]
	ExecutableModified,

	/// Rent_epoch account changed, but shouldn't have
	#[error("instruction modified rent epoch of an account")]
	RentEpochModified,

	/// The instruction expected additional account keys
	#[error("insufficient account keys for instruction")]
	NotEnoughAccountKeys,

	/// Program other than the account's owner changed the size of the account data
	#[error("program other than the account's owner changed the size of the account data")]
	AccountDataSizeChanged,

	/// The instruction expected an executable account
	#[error("instruction expected an executable account")]
	AccountNotExecutable,

	/// Failed to borrow a reference to account data, already borrowed
	#[error("instruction tries to borrow reference for an account which is already borrowed")]
	AccountBorrowFailed,

	/// Account data has an outstanding reference after a program's execution
	#[error("instruction left account with an outstanding borrowed reference")]
	AccountBorrowOutstanding,

	/// The same account was multiply passed to an on-chain program's entrypoint, but the program
	/// modified them differently.  A program can only modify one instance of the account because
	/// the runtime cannot determine which changes to pick or how to merge them if both are
	/// modified
	#[error("instruction modifications of multiply-passed account differ")]
	DuplicateAccountOutOfSync,

	/// Allows on-chain programs to implement program-specific error types and see them returned
	/// by the Solana runtime. A program-specific error may be any type that is represented as
	/// or serialized to a u32 integer.
	#[error("custom program error: {0:#x}")]
	Custom(u32),

	/// The return value from the program was invalid.  Valid errors are either a defined builtin
	/// error value or a user-defined error in the lower 32 bits.
	#[error("program returned invalid error code")]
	InvalidError,

	/// Executable account's data was modified
	#[error("instruction changed executable accounts data")]
	ExecutableDataModified,

	/// Executable account's lamports modified
	#[error("instruction changed the balance of an executable account")]
	ExecutableLamportChange,

	/// Executable accounts must be rent exempt
	#[error("executable accounts must be rent exempt")]
	ExecutableAccountNotRentExempt,

	/// Unsupported program id
	#[error("Unsupported program id")]
	UnsupportedProgramId,

	/// Cross-program invocation call depth too deep
	#[error("Cross-program invocation call depth too deep")]
	CallDepth,

	/// An account required by the instruction is missing
	#[error("An account required by the instruction is missing")]
	MissingAccount,

	/// Cross-program invocation reentrancy not allowed for this instruction
	#[error("Cross-program invocation reentrancy not allowed for this instruction")]
	ReentrancyNotAllowed,

	/// Length of the seed is too long for address generation
	#[error("Length of the seed is too long for address generation")]
	MaxSeedLengthExceeded,

	/// Provided seeds do not result in a valid address
	#[error("Provided seeds do not result in a valid address")]
	InvalidSeeds,

	/// Failed to reallocate account data of this length
	#[error("Failed to reallocate account data")]
	InvalidRealloc,

	/// Computational budget exceeded
	#[error("Computational budget exceeded")]
	ComputationalBudgetExceeded,

	/// Cross-program invocation with unauthorized signer or writable account
	#[error("Cross-program invocation with unauthorized signer or writable account")]
	PrivilegeEscalation,

	/// Failed to create program execution environment
	#[error("Failed to create program execution environment")]
	ProgramEnvironmentSetupFailure,

	/// Program failed to complete
	#[error("Program failed to complete")]
	ProgramFailedToComplete,

	/// Program failed to compile
	#[error("Program failed to compile")]
	ProgramFailedToCompile,

	/// Account is immutable
	#[error("Account is immutable")]
	Immutable,

	/// Incorrect authority provided
	#[error("Incorrect authority provided")]
	IncorrectAuthority,

	/// Failed to serialize or deserialize account data
	///
	/// Warning: This error should never be emitted by the runtime.
	///
	/// This error includes strings from the underlying 3rd party Borsh crate
	/// which can be dangerous because the error strings could change across
	/// Borsh versions. Only programs can use this error because they are
	/// consistent across Solana software versions.
	#[error("Failed to serialize or deserialize account data: {0}")]
	BorshIoError(String),

	/// An account does not have enough lamports to be rent-exempt
	#[error("An account does not have enough lamports to be rent-exempt")]
	AccountNotRentExempt,

	/// Invalid account owner
	#[error("Invalid account owner")]
	InvalidAccountOwner,

	/// Program arithmetic overflowed
	#[error("Program arithmetic overflowed")]
	ArithmeticOverflow,

	/// Unsupported sysvar
	#[error("Unsupported sysvar")]
	UnsupportedSysvar,

	/// Illegal account owner
	#[error("Provided owner is not allowed")]
	IllegalOwner,

	/// Accounts data allocations exceeded the maximum allowed per transaction
	#[error("Accounts data allocations exceeded the maximum allowed per transaction")]
	MaxAccountsDataAllocationsExceeded,

	/// Max accounts exceeded
	#[error("Max accounts exceeded")]
	MaxAccountsExceeded,

	/// Max instruction trace length exceeded
	#[error("Max instruction trace length exceeded")]
	MaxInstructionTraceLengthExceeded,

	/// Builtin programs must consume compute units
	#[error("Builtin programs must consume compute units")]
	BuiltinProgramsMustConsumeComputeUnits,
	// Note: For any new error added here an equivalent ProgramError and its
	// conversions must also be added
}

macro_rules! default_keypairs_impl {
	() => {
		fn pubkeys(&self) -> Vec<SolPubkey> {
			self.iter().map(|keypair| keypair.pubkey()).collect()
		}

		fn try_pubkeys(&self) -> Result<Vec<SolPubkey>, SignerError> {
			let mut pubkeys = Vec::new();
			for keypair in self.iter() {
				pubkeys.push(keypair.try_pubkey()?);
			}
			Ok(pubkeys)
		}

		fn sign_message(&self, message: &[u8]) -> Vec<SolSignature> {
			self.iter().map(|keypair| keypair.sign_message(message)).collect()
		}

		fn try_sign_message(&self, message: &[u8]) -> Result<Vec<SolSignature>, SignerError> {
			let mut signatures = Vec::new();
			for keypair in self.iter() {
				signatures.push(keypair.try_sign_message(message)?);
			}
			Ok(signatures)
		}

		fn is_interactive(&self) -> bool {
			self.iter().any(|s| s.is_interactive())
		}
	};
}

impl<T: Signer> Signers for [&T; 1] {
	default_keypairs_impl!();
}
impl<T: Signer> Signers for [&T; 2] {
	default_keypairs_impl!();
}
