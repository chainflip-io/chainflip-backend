#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)] // NOTE: This is stable as of rustc v1.54.0
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{
	eth::{set_agg_key_with_agg_key::SetAggKeyWithAggKey, AggKey},
	ChainId, Ethereum,
};
use cf_traits::{
	offline_conditions::{OfflineCondition, OfflineReporter},
	Chainflip, EpochIndex, EpochInfo, Nonce, NonceProvider, SigningContext, ThresholdSigner,
	VaultRotationHandler, VaultRotator,
};
use frame_support::{
	dispatch::{DispatchError, DispatchResult},
	pallet_prelude::*,
};
pub use pallet::*;
use sp_std::{convert::TryFrom, prelude::*};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// Id type used for the KeyGen ceremony.
pub type CeremonyId = u64;

/// The current status of a vault rotation.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub enum VaultRotationStatus<T: Config> {
	AwaitingKeygen {
		keygen_ceremony_id: CeremonyId,
		candidates: Vec<T::ValidatorId>,
	},
	AwaitingRotation {
		new_public_key: Vec<u8>,
	},
	Complete {
		tx_hash: Vec<u8>,
	},
}

type BlockHeight = u64;

/// A single vault.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Vault {
	/// The vault's public key.
	pub public_key: Vec<u8>,
	/// At which block height this key was rotated to
	pub block_height: BlockHeight,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	/// This is roughly the number of Ethereum blocks in 14 days.
	pub const ETHEREUM_LEEWAY_IN_BLOCKS: u64 = 80_000;

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Rotation handler.
		type RotationHandler: VaultRotationHandler<ValidatorId = Self::ValidatorId>;

		/// Epoch info.
		type EpochInfo: EpochInfo<ValidatorId = Self::ValidatorId>;

		/// For reporting misbehaving validators.
		type OfflineReporter: OfflineReporter<ValidatorId = Self::ValidatorId>;

		/// Top-level Ethereum signing context needs to support `SetAggKeyWithAggKey`.
		type SigningContext: From<SetAggKeyWithAggKey> + SigningContext<Self, Chain = Ethereum>;

		/// Threshold signer.
		type ThresholdSigner: ThresholdSigner<Self, Context = Self::SigningContext>;
	}

	/// Pallet implements [`Hooks`] trait
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	/// Counter for generating unique ceremony ids for the keygen ceremony.
	#[pallet::storage]
	#[pallet::getter(fn keygen_ceremony_id_counter)]
	pub(super) type KeygenCeremonyIdCounter<T: Config> = StorageValue<_, CeremonyId, ValueQuery>;

	/// A map of vaults by epoch and chain
	#[pallet::storage]
	#[pallet::getter(fn vaults)]
	pub(super) type Vaults<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, EpochIndex, Blake2_128Concat, ChainId, Vault>;

	/// Vault rotation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn pending_vault_rotations)]
	pub(super) type PendingVaultRotations<T: Config> =
		StorageMap<_, Blake2_128Concat, ChainId, VaultRotationStatus<T>>;

	/// Threshold key nonces for each chain.
	#[pallet::storage]
	#[pallet::getter(fn chain_nonces)]
	pub(super) type ChainNonces<T: Config> =
		StorageMap<_, Blake2_128Concat, ChainId, Nonce, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Request a key generation \[ceremony_id, chain_id, participants\]
		KeygenRequest(CeremonyId, ChainId, Vec<T::ValidatorId>),
		/// The vault for the request has rotated \[chain_id\]
		VaultRotationCompleted(ChainId),
		/// All KeyGen ceremonies have been aborted \[chain_ids\]
		KeygenAborted(Vec<ChainId>),
		/// A complete set of vaults have been rotated
		VaultsRotated,
		/// UnexpectedPubkeyWitnessed \[chain_id, key\]
		UnexpectedPubkeyWitnessed(ChainId, Vec<u8>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid ceremony id
		InvalidCeremonyId,
		/// We have an empty validator set
		EmptyValidatorSet,
		/// The rotation has not been confirmed
		NotConfirmed,
		/// There is currently no vault rotation in progress for this chain.
		NoActiveRotation,
		/// The specified chain is not supported.
		UnsupportedChain,
		/// The requested call is invalid based on the current rotation state.
		InvalidRotationStatus,
		/// The generated key is not a valid public key.
		InvalidPublicKey,
		/// A rotation for the requested ChainId is already underway.
		DuplicateRotationRequest,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A key generation succeeded. Update the state of the rotation and attempt to broadcast the setAggKey
		/// transaction.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		/// - [InvalidCeremonyId](Error::InvalidCeremonyId)
		/// - [InvalidPublicKey](Error::InvalidPublicKey)
		///
		/// ## Dependencies
		///
		/// - [Threshold Signer Trait](ThresholdSigner)
		#[pallet::weight(10_000)]
		pub fn keygen_success(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			chain_id: ChainId,
			new_public_key: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let rotation =
				PendingVaultRotations::<T>::get(chain_id).ok_or(Error::<T>::NoActiveRotation)?;
			let pending_ceremony_id = ensure_variant!(
				VaultRotationStatus::<T>::AwaitingKeygen { keygen_ceremony_id, .. } => keygen_ceremony_id,
				rotation,
				Error::<T>::InvalidRotationStatus,
			);
			ensure!(
				pending_ceremony_id == ceremony_id,
				Error::<T>::InvalidCeremonyId
			);
			let agg_key = AggKey::try_from(&new_public_key[..]).map_err(|e| {
				frame_support::debug::error!(
					"Unable to decode new public key {:?}: {:?}",
					new_public_key,
					e
				);
				Error::<T>::InvalidPublicKey
			})?;

			PendingVaultRotations::<T>::insert(
				chain_id,
				VaultRotationStatus::<T>::AwaitingRotation { new_public_key },
			);

			// TODO: 1. We only want to do this once *all* of the keygen ceremonies have succeeded so we might need an
			//          intermediate VaultRotationStatus::AwaitingOtherKeygens.
			//       2. This also implicitly broadcasts the transaction - could be made clearer.
			//       3. This is eth-specific, should be chain-agnostic.
			T::ThresholdSigner::request_transaction_signature(SetAggKeyWithAggKey::new_unsigned(
				<Self as NonceProvider<Ethereum>>::next_nonce(),
				agg_key,
			));

			Ok(().into())
		}

		/// Key generation failed. We report the guilty parties and abort all pending keygen ceremonies.
		///
		/// If key generation fails for *any* chain we need to abort *all* chains.
		///
		/// ## Events
		///
		/// - [KeygenAborted](Event::KeygenAborted)
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		/// - [InvalidCeremonyId](Error::InvalidCeremonyId)
		///
		/// ## Dependencies
		///
		/// - [Offline Reporter Trait](OfflineReporter)
		/// - [Threshold Signer Trait](ThresholdSigner)
		#[pallet::weight(10_000)]
		pub fn keygen_failure(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			chain_id: ChainId,
			guilty_validators: Vec<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let rotation =
				PendingVaultRotations::<T>::get(chain_id).ok_or(Error::<T>::NoActiveRotation)?;
			let pending_ceremony_id = ensure_variant!(
				VaultRotationStatus::<T>::AwaitingKeygen { keygen_ceremony_id, .. } => keygen_ceremony_id,
				rotation,
				Error::<T>::InvalidRotationStatus,
			);
			ensure!(
				pending_ceremony_id == ceremony_id,
				Error::<T>::InvalidCeremonyId
			);

			// TODO:
			// - Centralise penalty points.
			// - Define offline condition(s) for keygen failures.
			const PENALTY: i32 = 15;
			for offender in guilty_validators {
				T::OfflineReporter::report(
					OfflineCondition::ParticipateSigningFailed,
					PENALTY,
					&offender,
				)
				.unwrap_or_else(|e| {
					frame_support::debug::error!(
						"Unable to report ParticipateSigningFailed for signer {:?}: {:?}",
						offender,
						e
					);
					0
				});
			}
			Pallet::<T>::abort_rotation();
			Ok(().into())
		}

		/// A vault rotation event has been witnessed, we update the vault with the new key.
		///
		/// ## Events
		///
		/// - [UnexpectedPubkeyWitnessed](Event::UnexpectedPubkeyWitnessed)
		/// - [VaultRotationCompleted](Event::VaultRotationCompleted)
		///
		/// ## Errors
		///
		/// - [NoActiveRotation](Error::NoActiveRotation)
		/// - [InvalidRotationStatus](Error::InvalidRotationStatus)
		/// - [UnsupportedChain](Error::UnsupportedChain)
		/// - [InvalidPublicKey](Error::InvalidPublicKey)
		///
		/// ## Dependencies
		///
		/// - [Epoch Info Trait](EpochInfo)
		#[pallet::weight(10_000)]
		pub fn vault_key_rotated(
			origin: OriginFor<T>,
			chain_id: ChainId,
			new_public_key: Vec<u8>,
			block_number: u64,
			tx_hash: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let rotation =
				PendingVaultRotations::<T>::get(chain_id).ok_or(Error::<T>::NoActiveRotation)?;

			let expected_new_key = ensure_variant!(
				VaultRotationStatus::<T>::AwaitingRotation { new_public_key } => new_public_key,
				rotation,
				Error::<T>::InvalidRotationStatus
			);

			// If the keys don't match, we don't have much choice but to trust the witnessed one over the one
			// we expected, but we should log the issue nonetheless.
			if new_public_key != expected_new_key {
				frame_support::debug::error!(
					"Unexpected new agg key witnessed for {:?}. Expected {:?}, got {:?}.",
					chain_id,
					expected_new_key,
					new_public_key,
				);
				Self::deposit_event(Event::<T>::UnexpectedPubkeyWitnessed(
					chain_id,
					new_public_key.clone(),
				));
			}

			PendingVaultRotations::<T>::insert(
				chain_id,
				VaultRotationStatus::<T>::Complete { tx_hash },
			);

			// For the new epoch we create a new vault with the new public key and the block height
			Vaults::<T>::insert(
				T::EpochInfo::epoch_index().saturating_add(1),
				ChainId::Ethereum,
				Vault {
					public_key: new_public_key,
					block_height: block_number,
				},
			);

			Pallet::<T>::deposit_event(Event::VaultRotationCompleted(chain_id));

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		/// The Vault key should be a 33-byte compressed key in `[y; x]` order, where is `2` (even) or `3` (odd).
		///
		/// Requires `Serialize` and `Deserialize` which isn't implemented for `[u8; 33]` otherwise we could use
		/// that instead of `Vec`...
		pub ethereum_vault_key: Vec<u8>,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {
				ethereum_vault_key: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			let _ = AggKey::try_from(&self.ethereum_vault_key[..])
				.expect("Can't build genesis without a valid ethereum vault key.");

			Vaults::<T>::insert(
				T::EpochInfo::epoch_index(),
				ChainId::Ethereum,
				Vault {
					public_key: self.ethereum_vault_key.clone(),
					block_height: BlockHeight::default(),
				},
			);
		}
	}
}

impl<T: Config> NonceProvider<Ethereum> for Pallet<T> {
	fn next_nonce() -> Nonce {
		ChainNonces::<T>::mutate(ChainId::Ethereum, |nonce| {
			let new_nonce = nonce.saturating_add(1);
			*nonce = new_nonce;
			new_nonce
		})
	}
}

impl<T: Config> Pallet<T> {
	/// Abort all pending rotations and notify the `VaultRotationHandler` trait of our decision to abort.
	fn abort_rotation() {
		// TODO: Should disallow aborting if we have passed the keygen stage.
		// TODO: Should also notify of the ceremony id for each aborted ceremony.
		Self::deposit_event(Event::KeygenAborted(
			PendingVaultRotations::<T>::iter().map(|(c, _)| c).collect(),
		));
		PendingVaultRotations::<T>::remove_all();
		T::RotationHandler::vault_rotation_aborted();
	}

	fn no_active_chain_vault_rotations() -> bool {
		// Returns true if the iterator is empty or if all rotations are complete.
		PendingVaultRotations::<T>::iter()
			.all(|(_, status)| matches!(status, VaultRotationStatus::Complete { .. }))
	}

	fn start_vault_rotation_for_chain(
		candidates: Vec<T::ValidatorId>,
		chain_id: ChainId,
	) -> DispatchResult {
		// Main entry point for the pallet
		ensure!(!candidates.is_empty(), Error::<T>::EmptyValidatorSet);
		ensure!(
			!PendingVaultRotations::<T>::contains_key(chain_id),
			Error::<T>::DuplicateRotationRequest
		);

		let ceremony_id = KeygenCeremonyIdCounter::<T>::mutate(|id| {
			*id += 1;
			*id
		});

		PendingVaultRotations::<T>::insert(
			chain_id,
			VaultRotationStatus::<T>::AwaitingKeygen {
				keygen_ceremony_id: ceremony_id,
				candidates: candidates.clone(),
			},
		);
		Pallet::<T>::deposit_event(Event::KeygenRequest(ceremony_id, chain_id, candidates));

		Ok(())
	}
}

impl<T: Config> VaultRotator for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type RotationError = DispatchError;

	fn start_vault_rotation(candidates: Vec<Self::ValidatorId>) -> Result<(), Self::RotationError> {
		// We only support Ethereum for now.
		Self::start_vault_rotation_for_chain(candidates, ChainId::Ethereum)
	}

	fn finalize_rotation() -> Result<(), Self::RotationError> {
		if Pallet::<T>::no_active_chain_vault_rotations() {
			// The 'exit' point for the pallet, no rotations left to process
			PendingVaultRotations::<T>::remove_all();
			Self::deposit_event(Event::VaultsRotated);
			Ok(())
		} else {
			// Wait on confirmation
			Err(Error::<T>::NotConfirmed.into())
		}
	}
}

/// Takes three arguments: a pattern, a variable expression and an error literal.
///
/// If the variable matches the pattern, returns it, otherwise returns an error. The pattern may optionally have an
/// expression attached to process and return inner arguments.
///
/// ## Example
///
/// let x = ensure_variant!(Some(..), optional_value, Error::<T>::ValueIsNone);
///
/// let 2x = ensure_variant!(Some(x) => { 2 * x }, optional_value, Error::<T>::ValueIsNone);
///
#[macro_export]
macro_rules! ensure_variant {
	( $variant:pat => $varexp:expr, $var:expr, $err:expr $(,)? ) => {
		if let $variant = $var {
			$varexp
		} else {
			frame_support::fail!($err)
		}
	};
	( $variant:pat, $var:expr, $err:expr $(,)? ) => {
		ensure_variant!($variant => { $var }, $var, $err)
	};
}
