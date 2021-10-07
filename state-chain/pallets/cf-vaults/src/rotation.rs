use cf_chains::{ChainId};
use super::Config;
use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use sp_std::prelude::*;

/// CeremonyId type
pub type CeremonyId = u64;

/// State of a vault rotation
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct VaultRotation<T: Config> {
	/// Proposed new public key. Is None before keygen_response is returned
	pub new_public_key: Option<T::PublicKey>,
	pub keygen_request: KeygenRequest<T::ValidatorId>,
}

pub struct VaultRotationNew<T: Config> {
	rotation_id: CeremonyId,
	status: VaultRotationStatus<T>,
}

pub enum VaultRotationStatus<T: Config> {
	AwaitingKeygen {
		keygen_ceremony_id: CeremonyId,
		candidates: Vec<T::ValidatorId>,
	},
	AwaitingRotation {
		new_public_key: T::PublicKey,
	},
	Complete {
		tx_hash: T::TransactionHash,
	},
}

/// A representation of a key generation request
/// This would be used for each supporting chain
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct KeygenRequest<ValidatorId> {
	/// A Chain's Id.
	pub(crate) chain: ChainId,
	/// The set of validators from which we would like to generate the key
	pub validator_candidates: Vec<ValidatorId>,
}

/// A response for our KeygenRequest
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum KeygenResponse<ValidatorId, PublicKey: Into<Vec<u8>>> {
	/// The key generation ceremony has completed successfully with a new proposed public key
	Success(PublicKey),
	/// Something went wrong and it failed.
	Error(Vec<ValidatorId>),
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
	Success {
		tx_hash: TransactionHash,
		block_number: u64,
	},
	Error,
}

#[macro_export]
macro_rules! ensure_index {
	($index: expr) => {
		ensure!(
			ActiveChainVaultRotations::<T>::contains_key($index),
			Error::<T>::InvalidCeremonyId
		);
	};
}
