use super::Config;
use cf_chains::ChainId;
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
	// pub keygen_request: KeygenRequest<T>,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct VaultRotationNew<T: Config> {
	pub rotation_id: CeremonyId,
	pub chain_id: ChainId,
	pub status: VaultRotationStatus<T>,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
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

/// The Vault's keys, public that is
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Vault<T: Config> {
	/// The current key
	pub current_key: T::PublicKey,
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
