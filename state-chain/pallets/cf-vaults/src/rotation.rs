use codec::{Encode, Decode};
use frame_support::RuntimeDebug;
use std::ops::Add;
use frame_system::pallet_prelude::*;
use frame_support::pallet_prelude::*;
use sp_runtime::traits::AtLeast32BitUnsigned;
use cf_traits::AuctionConfirmation;

pub type NewPublicKey = Vec<u8>;
pub type BadValidators<ValidatorId> = Vec<ValidatorId>;
pub type RequestIndex = u32;

pub trait Index<T: Add> {
	fn is_valid(idx: T) -> bool;
	fn next() -> T;
	fn clear(idx: T);
}

pub trait RequestResponse<Index, Request, Response> {
	fn process_request(index: Index, request: Request);
	fn process_response(index: Index, response: Response);
}

pub trait Construct<Index, ValidatorId> {
	type Manager: ConstructionManager<Index>;
	// Start the construction phase.  When complete `ConstructionHandler::on_completion()`
	// would be used to notify that this is complete
	fn start_construction_phase(index: Index, response: KeygenResponse<ValidatorId>);
}

pub trait ConstructionManager<Index> {
	// Construction phase complete
	fn on_completion(index: Index, result: Result<ValidatorRotationRequest, ValidatorRotationError>);
}

pub trait AuctionPenalty<ValidatorId> {
	fn penalise(bad_validators: BadValidators<ValidatorId>);
}

// A trait covering those things we find dearly in ChainFlip
pub trait ChainFlip {
	/// An amount for a bid
	type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
	/// An identity for a validator
	type ValidatorId: Member + Parameter;
}

pub trait AuctionManager<ValidatorId> {
	type AuctionPenalty: AuctionPenalty<ValidatorId>;
	type AuctionConfirmation: AuctionConfirmation;
}

// TODO - should this be broken down into its own trait as opposed in the pallet?
// pub trait KeyRotation<ValidatorId> {
// 	type AuctionPenalty: AuctionPenalty<ValidatorId>;
// 	type KeyGeneration: RequestResponse<KeygenRequest<ValidatorId>, KeygenResponse<ValidatorId>>;
// 	type Construct: Construct<KeygenResponse<ValidatorId>>;
// 	type ConstructionManager: ConstructionManager;
// 	type Rotation: RequestResponse<ValidatorRotationRequest, ValidatorRotationResponse>;
// }

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
	chain: ChainParams,
	// validator_candidates - the set from which we would like to generate the key
	validator_candidates: Vec<ValidatorId>,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum KeygenResponse<ValidatorId> {
	// The KGC has completed successfully with a new public key
	Success(NewPublicKey),
	// Something went wrong and it has failed.
	// Re-run the auction minus the bad validators
	Failure(BadValidators<ValidatorId>),
}

pub enum ValidatorRotationError {

}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct ValidatorRotationRequest {
	chain: ChainParams,
}

impl ValidatorRotationRequest {
	pub fn new(chain: ChainParams) -> ValidatorRotationRequest {
		Self {
			chain
		}
	}
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct ValidatorRotationResponse {
	old_key: Vec<u8>,
	new_key: Vec<u8>,
	tx: Vec<u8>
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct VaultRotation<Index, ValidatorId> {
	id: Index,
	keygen_response: Option<KeygenResponse<ValidatorId>>,
	// completed_construct: CompletedConstruct,
	// validator_rotation_response: ValidatorRotationResponse,
}

impl<Index, ValidatorId> VaultRotation<Index, ValidatorId> {
	pub fn new(id: Index) -> Self {
		VaultRotation {
			id,
			keygen_response: None,
		}
	}
}