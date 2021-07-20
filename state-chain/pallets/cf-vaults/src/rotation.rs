use codec::{Encode, Decode};
use frame_support::RuntimeDebug;
use std::ops::Add;
use frame_support::pallet_prelude::*;
use sp_runtime::traits::AtLeast32BitUnsigned;
use cf_traits::{AuctionConfirmation, AuctionEvents, AuctionPenalty};
use sp_runtime::DispatchResult;

pub type NewPublicKey = Vec<u8>;
pub type RequestIndex = u32;
pub type RequestIndexes = Vec<RequestIndex>;

// The state of a rotation, where we have one rotation for all vaults
pub enum State {
	Invalid,
	InProcess,
	Completed,
}

// The things that can go wrong
pub enum RotationError<ValidatorId> {
	EmptyValidatorSet,
	InvalidValidators,
	BadValidators(Vec<ValidatorId>),
	FailedConstruct,
	FailedToComplete,
	KeygenResponseFailed,
	VaultRotationCompletionFailed,
}

pub trait TryIndex<T> {
	fn try_is_valid(idx: T) -> DispatchResult;
}

pub trait Index<T: Add> {
	fn next() -> T;
	fn clear(idx: T);
	fn is_empty() -> bool;
	fn is_valid(idx: T) -> bool;
}

pub trait RequestResponse<Index, Request, Response, Error> {
	fn try_request(index: Index, request: Request) -> Result<(), Error>;
	fn try_response(index: Index, response: Response) -> Result<(), Error>;
}

pub trait Chain<Index, ValidatorId, Error> {
	fn chain_params() -> ChainParams;
	// Start the construction phase.  When complete `ConstructionHandler::on_completion()`
	// would be used to notify that this is complete
	fn try_start_construction_phase(index: Index, new_public_key: NewPublicKey, validators: Vec<ValidatorId>) -> Result<(), Error>;
}

pub trait ChainEvents<Index, ValidatorId, Error> {
	// Construction phase complete
	fn try_on_completion(index: Index, result: Result<ValidatorRotationRequest, ValidatorRotationError<ValidatorId>>) -> Result<(), Error>;
}

// A trait covering those things we find dearly in ChainFlip
pub trait ChainFlip {
	/// An amount for a bid
	type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
	/// An identity for a validator
	type ValidatorId: Member + Parameter;
}

pub trait AuctionManager<ValidatorId, Amount> {
	type Penalty: AuctionPenalty<ValidatorId>;
	type Confirmation: AuctionConfirmation;
	type Events: AuctionEvents<ValidatorId, Amount>;
}

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

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct KeygenRequest<ValidatorId> {
	// Chain
	pub(crate) chain: ChainParams,
	// validator_candidates - the set from which we would like to generate the key
	pub(crate) validator_candidates: Vec<ValidatorId>,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum KeygenResponse<ValidatorId> {
	// The KGC has completed successfully with a new public key
	Success(NewPublicKey),
	// Something went wrong and it has failed.
	// Re-run the auction minus the bad validators
	Failure(Vec<ValidatorId>),
}

pub enum ValidatorRotationError<ValidatorId> {
	BadValidators(Vec<ValidatorId>),
	FailedConstruct,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct ValidatorRotationRequest {
	pub(crate) chain: ChainParams,
}

impl ValidatorRotationRequest {
	pub fn new(chain: ChainParams) -> ValidatorRotationRequest {
		Self {
			chain
		}
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Default)]
pub struct ValidatorRotationResponse {
	old_key: Vec<u8>,
	new_key: Vec<u8>,
	tx: Vec<u8>
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct VaultRotation<Index, ValidatorId> {
	id: Index,
	pub(crate) keygen_request: KeygenRequest<ValidatorId>,
	pub(crate) new_public_key: NewPublicKey,
	// completed_construct: CompletedConstruct,
	// validator_rotation_response: ValidatorRotationResponse,
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