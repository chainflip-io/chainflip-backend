use cf_traits::RotationError;
use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::prelude::*;

/// CeremonyId type
pub type CeremonyId = u64;

// TODO: Use the same type as the CFE
/// Schnorr Signature type
#[derive(PartialEq, Decode, Encode, Eq, Clone, RuntimeDebug, Copy, Default)]
pub struct SchnorrSignature {
	/// Scalar component
	// s: secp256k1::SecretKey,
	pub s: [u8; 32],

	// TODO: This is not technically "r" so this should be renamed
	/// Public key hashed and truncated to an ethereum address
	pub r: [u8; 20],
}

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
	/// A transaction hash
	type TransactionHash: Into<Vec<u8>>;
	/// An identifier for a validator involved in the rotation of the vault
	type ValidatorId;
	/// An error on rotating the vault
	type Error;
	/// Start the vault rotation phase.  The chain would complete steps necessary for its chain
	/// for the rotation of the vault.
	fn rotate_vault(
		ceremony_id: CeremonyId,
		new_public_key: Self::PublicKey,
		validators: Vec<Self::ValidatorId>,
	) -> Result<(), Self::Error>;
	/// We have confirmation of the rotation back from `Vaults`
	fn vault_rotated(new_public_key: Self::PublicKey, tx_hash: Self::TransactionHash);
}

/// Events coming in from our chain.  This is used to callback from the request to complete the vault
/// rotation phase.  See `ChainVault::try_start_vault_rotation()` for more details.
pub trait ChainHandler {
	type ValidatorId;
	type Error;
	/// Request initial vault rotation phase complete with a result describing the outcome of this phase
	/// Feedback is provided back on this step
	fn request_vault_rotation(
		index: CeremonyId,
		result: Result<VaultRotationRequest, RotationError<Self::ValidatorId>>,
	) -> Result<(), Self::Error>;
}

/// Chain types supported
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum ChainType {
	/// Ethereum type blockchain
	Ethereum,
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

/// State of a vault rotation
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct VaultRotation<ValidatorId, PublicKey> {
	/// Proposed new public key
	pub new_public_key: PublicKey,
	pub keygen_request: KeygenRequest<ValidatorId>,
}

/// A representation of a key generation request
/// This would be used for each supporting chain
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct KeygenRequest<ValidatorId> {
	/// The chain type
	pub(crate) chain_type: ChainType,
	/// The set of validators from which we would like to generate the key
	pub validator_candidates: Vec<ValidatorId>,
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
	pub chain: ChainParams,
}

/// From chain to request
impl From<ChainParams> for VaultRotationRequest {
	fn from(chain: ChainParams) -> Self {
		VaultRotationRequest { chain }
	}
}

/// The Vault's keys, public that is
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Default)]
pub struct Vault<PublicKey: Into<Vec<u8>>, TransactionHash: Into<Vec<u8>>> {
	/// The previous key
	pub previous_key: PublicKey,
	/// The current key
	pub current_key: PublicKey,
	/// The transaction hash for the vault rotation to the current key
	pub tx_hash: TransactionHash,
}

/// A response of our request to rotate the vault
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum VaultRotationResponse<TransactionHash: Into<Vec<u8>>> {
	Success { tx_hash: TransactionHash },
	Failure,
}

/// A signing request
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct ThresholdSignatureRequest<PublicKey: Into<Vec<u8>>, ValidatorId> {
	/// Payload to be signed over
	pub payload: Vec<u8>,
	/// The public key of the key to be used to sign with
	pub public_key: PublicKey,
	/// Those validators to sign
	pub validators: Vec<ValidatorId>,
}

/// A response back with our signature else a list of bad validators
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum ThresholdSignatureResponse<ValidatorId, Signature> {
	// Signature
	Success(Signature),
	// Bad validators
	Error(Vec<ValidatorId>),
}

#[macro_export]
macro_rules! ensure_index {
	($index: expr) => {
		ensure!(
			VaultRotations::<T>::contains_key($index),
			RotationError::InvalidCeremonyId
		);
	};
}
