use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use frame_support::RuntimeDebug;
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::prelude::*;

/// Errors occurring during a rotation
#[derive(RuntimeDebug, Encode, Decode, PartialEq)]
pub enum RotationError<ValidatorId> {
	/// An invalid request index
	InvalidRequestIndex,
	/// Empty validator set provided
	EmptyValidatorSet,
	/// A set of badly acting validators
	BadValidators(Vec<ValidatorId>),
	/// The key generation response failed
	KeyResponseFailed,
	/// Failed to construct a valid chain specific payload for rotation
	FailedToConstructPayload,
	/// Vault rotation completion failed
	VaultRotationCompletionFailed,
}

/// Determine if an index is valid
pub trait TryIndex {
	type Index: AtLeast32BitUnsigned;
	type Err;
	fn try_is_valid(idx: Self::Index) -> Result<(), Self::Err>;
}

/// A request/response trait
pub trait RequestResponse<Index: AtLeast32BitUnsigned, Req, Res, Err> {
	/// Try to make a request identified with an index
	fn try_request(index: Index, request: Req) -> Result<(), Err>;
	// Try to handle a response of a request identified with a index
	fn try_response(index: Index, response: Res) -> Result<(), Err>;
}

/// A vault for a chain
pub trait ChainVault {
	/// An index type we would associate requests with
	type Index: AtLeast32BitUnsigned;
	/// The type used for public keys
	type Bytes: Into<Vec<u8>>;
	/// An identifier for a validator involved in the rotation of the vault
	type ValidatorId;
	/// An error on rotating the vault
	type Err;
	/// A set of params for the chain for this vault
	fn chain_params() -> ChainParams;
	/// Start the vault rotation phase.  The chain would complete steps necessary for its chain
	/// for the rotation of the vault.
	/// When complete `ChainHandler::try_complete_vault_rotation()` would be used to notify to continue
	/// with the process.
	fn try_start_vault_rotation(
		index: Self::Index,
		new_public_key: Self::Bytes,
		validators: Vec<Self::ValidatorId>,
	) -> Result<(), Self::Err>;
	/// We have confirmation of the rotation back from `Vaults`
	fn vault_rotated(response: VaultRotationResponse<Self::Bytes>);
}

/// Events coming in from our chain.  This is used to callback from the request to complete the vault
/// rotation phase.  See `ChainVault::try_start_vault_rotation()` for more details.
pub trait ChainHandler {
	type Index: AtLeast32BitUnsigned;
	type ValidatorId;
	type Err;
	/// Initial vault rotation phase complete with a result describing the outcome of this phase
	/// Feedback is provided back on this step
	fn try_complete_vault_rotation(
		index: Self::Index,
		result: Result<VaultRotationRequest, RotationError<Self::ValidatorId>>,
	) -> Result<(), Self::Err>;
}

/// Description of some base types
pub trait ChainFlip {
	/// An amount for a bid
	type Amount: Member + Parameter + Default + Eq + Ord + Copy + AtLeast32BitUnsigned;
	/// An identity for a validator
	type ValidatorId: Member + Parameter;
}

/// Our different Chain's specific parameters
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum ChainParams {
	/// Ethereum blockchain
	///
	/// The value is the call data encoded for the final transaction
	/// to request the key rotation via `setAggKeyWithAggKey`
	Ethereum(Vec<u8>),
	/// This is a placeholder, not to be used in production
	Other(Vec<u8>),
}

/// A representation of a key generation request
/// This would be used for each supporting chain
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct KeygenRequest<ValidatorId> {
	/// A Chain's parameters
	pub(crate) chain: ChainParams,
	/// The set of validators from which we would like to generate the key
	pub(crate) validator_candidates: Vec<ValidatorId>,
}

/// A response for our KeygenRequest
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum KeygenResponse<ValidatorId, PublicKey: Into<Vec<u8>>> {
	/// The key generation ceremony has completed successfully with a new proposed public key
	Success(PublicKey),
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
pub struct VaultRotationResponse<Bytes: Into<Vec<u8>>> {
	pub old_key: Bytes,
	pub new_key: Bytes,
	pub tx: Bytes,
}
