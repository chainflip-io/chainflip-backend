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
	pub keygen_request: KeygenRequest<T>,
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
pub struct KeygenRequest<T: Config> {
	/// A Chain's Id.
	pub(crate) chain: ChainId,
	/// The set of validators from which we would like to generate the key
	pub validator_candidates: Vec<T::ValidatorId>,
}

/// A response for our KeygenRequest
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum KeygenResponse<T: Config> {
	/// The key generation ceremony has completed successfully with a new proposed public key
	Success(T::PublicKey),
	/// Something went wrong and it failed.
	Error(Vec<T::ValidatorId>),
}

/// The Vault's keys, public that is
#[derive(Default, PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Vault<T: Config> {
	/// The previous key
	pub previous_key: T::PublicKey,
	/// The current key
	pub current_key: T::PublicKey,
	/// The transaction hash for the vault rotation to the current key
	pub tx_hash: T::TransactionHash,
}

/// A response of our request to rotate the vault
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum VaultRotationResponse<T: Config> {
	Success { tx_hash: T::TransactionHash },
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
