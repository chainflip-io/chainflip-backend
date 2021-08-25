use cf_traits::RotationError;
use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::prelude::*;

/// Request index type
pub type RequestIndex = u64;

/// A request/response trait
pub trait RequestResponse<Index: AtLeast32BitUnsigned, Req, Res, Error> {
	/// Make a request identified with an index
	fn make_request(index: Index, request: Req) -> Result<(), Error>;
	// Handle a response of a request identified with a index
	fn handle_response(index: Index, response: Res) -> Result<(), Error>;
}

/// A vault for a chain
pub trait ChainVault {
	/// The type used for public keys
	type PublicKey: Into<Vec<u8>>;
	/// A transaction
	type Transaction: Into<Vec<u8>>;
	/// An identifier for a validator involved in the rotation of the vault
	type ValidatorId;
	/// An error on rotating the vault
	type Error;
	/// A set of params for the chain for this vault
	fn chain_params() -> ChainParams;
	/// Start the vault rotation phase.  The chain would complete steps necessary for its chain
	/// for the rotation of the vault.
	/// When complete `ChainHandler::try_complete_vault_rotation()` would be used to notify to continue
	/// with the process.
	fn start_vault_rotation(
		index: RequestIndex,
		new_public_key: Self::PublicKey,
		validators: Vec<Self::ValidatorId>,
	) -> Result<(), Self::Error>;
	/// We have confirmation of the rotation back from `Vaults`
	fn vault_rotated(response: VaultRotationResponse<Self::PublicKey, Self::Transaction>);
}

/// Events coming in from our chain.  This is used to callback from the request to complete the vault
/// rotation phase.  See `ChainVault::try_start_vault_rotation()` for more details.
pub trait ChainHandler {
	type ValidatorId;
	type Error;
	/// Request initial vault rotation phase complete with a result describing the outcome of this phase
	/// Feedback is provided back on this step
	fn request_vault_rotation(
		index: RequestIndex,
		result: Result<VaultRotationRequest, RotationError<Self::ValidatorId>>,
	) -> Result<(), Self::Error>;
}

pub type ChainIdentifier = u8;
/// Identifiers for chains supported
/// The Ethereum chain
pub const ETHEREUM_CHAIN: ChainIdentifier = 1;

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
pub struct VaultRotationResponse<PublicKey: Into<Vec<u8>>, Transaction: Into<Vec<u8>>> {
	pub old_key: PublicKey,
	pub new_key: PublicKey,
	pub tx: Transaction,
}

#[macro_export]
macro_rules! ensure_index {
	($index: expr) => {
		ensure!(
			VaultRotations::<T>::contains_key($index),
			RotationError::InvalidRequestIndex
		);
	};
}
