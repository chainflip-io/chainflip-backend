use codec::{Encode, Decode};
use frame_support::RuntimeDebug;
use std::ops::Add;
use frame_support::pallet_prelude::*;
use sp_runtime::traits::AtLeast32BitUnsigned;
use cf_traits::{AuctionConfirmation, AuctionEvents, AuctionPenalty};
use sp_runtime::DispatchResult;

/// The new public key
pub type NewPublicKey = Vec<u8>;
/// Request index type
pub type RequestIndex = u32;
/// List of request indexes
pub type RequestIndexes = Vec<RequestIndex>;

// The things that can go wrong
pub enum RotationError<ValidatorId> {
	/// Empty validator set provided
	EmptyValidatorSet,
	/// An invalid set of validators
	InvalidValidators,
	/// A set of badly acting validators
	BadValidators(Vec<ValidatorId>),
	/// Failed to construct a valid chain specific payload for rotation
	FailedToConstructPayload,
	/// Vault rotation completion failed
	VaultRotationCompletionFailed,
}

/// An index scheme which manages a number of index references
pub trait Index<T: Add> {
	/// Provide the next index
	fn next() -> T;
	/// Invalidate this index if it exists
	fn invalidate(idx: T);
	/// Do we have an indexes
	fn is_empty() -> bool;
	/// Is this a valid index
	fn is_valid(idx: T) -> bool;
}

/// Try to determine if an index is valid
pub trait TryIndex<T: Add> : Index<T> {
	fn try_is_valid(idx: T) -> DispatchResult;
}

/// A request/response trait
pub trait RequestResponse<I, Req, Res, Err> {
	/// Try to make a request identified with an index
	fn try_request(index: I, request: Req) -> Result<(), Err>;
	// Try to handle a response of a request identified with a index
	fn try_response(index: I, response: Res) -> Result<(), Err>;
}

/// A vault for a chain
pub trait ChainVault<I, ValidatorId, Err> {
	/// A set of params for the chain for this vault
	fn chain_params() -> ChainParams;
	/// Start the vault rotation phase.  The chain would construct a `VaultRotationRequest`.
	/// When complete `ChainEvents::try_complete_vault_rotation()` would be used to notify to continue
	/// with the process.
	fn try_start_vault_rotation(index: I, new_public_key: NewPublicKey, validators: Vec<ValidatorId>) -> Result<(), Err>;
	/// We have confirmation of the rotation
	fn vault_rotated(response: VaultRotationResponse);
}

/// Events coming in from our chains.  This is used to callback from the request to complete the vault
/// rotation phase
pub trait ChainEvents<I, ValidatorId, Err> {
	/// Initial vault rotation phase complete with a result describing the outcome of this phase
	/// Feedback is provided back on this step
	fn try_complete_vault_rotation(index: I, result: Result<VaultRotationRequest, RotationError<ValidatorId>>) -> Result<(), Err>;
}

/// Description of some base types
pub trait ChainFlip {
	/// An amount for a bid
	type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
	/// An identity for a validator
	type ValidatorId: Member + Parameter;
}

/// Grouping the management(traits) of a ChainFlip auction.
pub trait AuctionManager<ValidatorId, Amount> {
	type Penalty: AuctionPenalty<ValidatorId>;
	type Confirmation: AuctionConfirmation;
	type Events: AuctionEvents<ValidatorId, Amount>;
}

/// Our different Chain's specific parameters
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum ChainParams {
	// Ethereum blockchain
	//
	// The value is the call data encoded for the final transaction
	// to request the key rotation via `setAggKeyWithAggKey`
	Ethereum(Vec<u8>),
	// This is a placeholder, not to be used in production
	Other(Vec<u8>),
}

/// A representation of a key generation request
/// This would be constructing for each supporting chain
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct KeygenRequest<ValidatorId> {
	/// Chain's parameters
	pub(crate) chain: ChainParams,
	/// The set of validators from which we would like to generate the key
	pub(crate) validator_candidates: Vec<ValidatorId>,
}

/// A response for our KeygenRequest
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum KeygenResponse<ValidatorId> {
	/// The key generation ceremony has completed successfully with a new proposed public key
	Success(NewPublicKey),
	/// Something went wrong and it failed.
	Failure(Vec<ValidatorId>),
}

/// The vault rotation request
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct VaultRotationRequest {
	pub(crate) chain: ChainParams,
}
/// From chain to request
impl From<ChainParams> for VaultRotationRequest {
	fn from(chain: ChainParams) -> Self {
		VaultRotationRequest { chain }
	}
}
/// A response of our request to rotate the vault
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Default)]
pub struct VaultRotationResponse {
	old_key: Vec<u8>,
	new_key: Vec<u8>,
	tx: Vec<u8>
}

/// The state of a vault rotation
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct VaultRotation<Index, ValidatorId> {
	/// Index for this rotation
	id: Index,
	/// The initial key generation request
	pub(crate) keygen_request: KeygenRequest<ValidatorId>,
	/// The proposed key
	pub(crate) new_public_key: NewPublicKey,
}

impl<Index, ValidatorId> VaultRotation<Index, ValidatorId> {
	pub fn new(id: Index, keygen_request: KeygenRequest<ValidatorId>) -> Self {
		VaultRotation {
			id,
			keygen_request,
			new_public_key: vec![],
		}
	}

	pub fn candidate_validators(&self) -> &Vec<ValidatorId> {
		&self.keygen_request.validator_candidates
	}
}