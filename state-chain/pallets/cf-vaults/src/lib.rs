#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)] // NOTE: This is stable as of rustc v1.54.0
#![doc = include_str!("../README.md")]

use cf_chains::{
	eth::{self, set_agg_key_with_agg_key::SetAggKeyWithAggKey, AggKey},
	ChainId, Ethereum,
};
use cf_traits::{
	offline_conditions::{OfflineCondition, OfflineReporter},
	Chainflip, Nonce, NonceProvider, SigningContext, ThresholdSigner, VaultRotationHandler,
	VaultRotator,
};
use frame_support::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::One;
use sp_std::prelude::*;

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
		new_public_key: T::PublicKey,
	},
	Complete {
		tx_hash: T::TransactionHash,
	},
}

/// A single vault.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug)]
pub struct Vault<T: Config> {
	/// The current key
	pub current_key: T::PublicKey,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, Default)]
	pub struct BlockHeightWindow {
		pub from: u64,
		pub to: Option<u64>,
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// The event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// A public key
		type PublicKey: Member
			+ Parameter
			+ Into<AggKey>
			+ Default
			+ MaybeSerializeDeserialize;

		/// A transaction
		type TransactionHash: Member + Parameter + Into<eth::TxHash> + Default;

		/// Rotation handler
		type RotationHandler: VaultRotationHandler<ValidatorId = Self::ValidatorId>;

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

	/// Current request index used in request/response
	#[pallet::storage]
	#[pallet::getter(fn current_request)]
	pub(super) type CurrentRequest<T: Config> = StorageValue<_, CeremonyId, ValueQuery>;

	/// The active vaults for the current epoch.
	#[pallet::storage]
	#[pallet::getter(fn vaults)]
	pub(super) type Vaults<T: Config> = StorageMap<_, Blake2_128Concat, ChainId, Vault<T>>;

	/// Vault rotation statuses for the current epoch rotation.
	#[pallet::storage]
	#[pallet::getter(fn pending_vault_rotations)]
	pub(super) type PendingVaultRotations<T: Config> =
		StorageMap<_, Blake2_128Concat, ChainId, VaultRotationStatus<T>>;

	/// Threshold key nonces for each chain.
	#[pallet::storage]
	#[pallet::getter(fn chain_nonces)]
	pub(super) type ChainNonces<T: Config> = StorageMap<_, Blake2_128Concat, ChainId, Nonce>;

	#[pallet::storage]
	#[pallet::getter(fn active_windows)]
	pub(super) type ActiveWindows<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		<T::EpochInfo as EpochInfo>::EpochIndex,
		Blake2_128Concat,
		Chain,
		BlockHeightWindow,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Request a key generation \[ceremony_id, chain_id, participants\]
		KeygenRequest(CeremonyId, ChainId, Vec<T::ValidatorId>),
		/// The vault for the request has rotated \[chain_id\]
		VaultRotationCompleted(ChainId),
		/// A rotation of vaults has been aborted \[ceremony_ides\]
		RotationAborted(Vec<ChainId>),
		/// A complete set of vaults have been rotated
		VaultsRotated,
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
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A key generation succeeded. Update the state of the rotation and attempt to broadcast the setAggKey
		/// transaction.
		#[pallet::weight(10_000)]
		pub fn keygen_success(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			chain_id: ChainId,
			new_public_key: T::PublicKey,
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

			PendingVaultRotations::<T>::insert(
				chain_id,
				VaultRotationStatus::<T>::AwaitingRotation {
					new_public_key: new_public_key.clone(),
				},
			);

			// TODO: 1. We only want to do this once *all* of the keygen ceremonies have succeeded so we might need an
			//          intermediate VaultRotationStatus::AwaitingOtherKeygens.
			//       2. This is implicitly also broadcasts the transaction - could be made clearer.
			T::ThresholdSigner::request_transaction_signature(SetAggKeyWithAggKey::new_unsigned(
				<Self as NonceProvider<Ethereum>>::next_nonce(),
				new_public_key,
			));

			Ok(().into())
		}

		/// Key generation failed. We report the guilty parties and abort.
		///
		/// If key generation fails for *any* chain we need to abort *all* chains.
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
		#[pallet::weight(10_000)]
		pub fn vault_key_rotated(
			origin: OriginFor<T>,
			chain_id: ChainId,
			new_public_key: T::PublicKey,
			_tx_hash: T::TransactionHash,
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
				frame_support::debug::warn!(
					"Unexpected new agg key witnessed for {:?}. Expected {:?}, got {:?}.",
					chain_id,
					expected_new_key,
					new_public_key,
				)
			}

			Vaults::<T>::try_mutate_exists(chain_id, |maybe_vault| {
				if let Some(mut vault) = maybe_vault.as_mut() {
					vault.current_key = new_public_key;
					Ok(())
				} else {
					Err(Error::<T>::UnsupportedChain)
				}
			})?;

			Pallet::<T>::deposit_event(Event::VaultRotationCompleted(chain_id));

			Ok(().into())
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub ethereum_vault_key: T::PublicKey,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				ethereum_vault_key: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			Vaults::<T>::insert(
				ChainId::Ethereum,
				Vault {
					current_key: self.ethereum_vault_key.clone(),
				},
			);
		}
	}
}

impl<T: Config> NonceProvider<Ethereum> for Pallet<T> {
	fn next_nonce() -> Nonce {
		ChainNonces::<T>::mutate(ChainId::Ethereum, |nonce| {
			let new_nonce = nonce.unwrap_or_default().saturating_add(One::one());
			*nonce = Some(new_nonce);
			new_nonce
		})
	}
}

impl<T: Config> Pallet<T> {
	/// Abort all rotations registered and notify the `VaultRotationHandler` trait of our decision to abort.
	fn abort_rotation() {
		Self::deposit_event(Event::RotationAborted(
			PendingVaultRotations::<T>::iter().map(|(c, _)| c).collect(),
		));
		PendingVaultRotations::<T>::remove_all();
		T::RotationHandler::abort();
	}

	fn no_active_chain_vault_rotations() -> bool {
		PendingVaultRotations::<T>::iter().count() == 0
	}
}

impl<T: Config> VaultRotator for Pallet<T> {
	type ValidatorId = T::ValidatorId;
	type RotationError = Error<T>;

	fn start_vault_rotation(candidates: Vec<Self::ValidatorId>) -> Result<(), Self::RotationError> {
		// Main entry point for the pallet
		ensure!(!candidates.is_empty(), Error::<T>::EmptyValidatorSet);

		let ceremony_id = CurrentRequest::<T>::mutate(|id| {
			*id += 1;
			*id
		});

		Pallet::<T>::deposit_event(Event::KeygenRequest(
			ceremony_id,
			ChainId::Ethereum,
			candidates.clone(),
		));
		PendingVaultRotations::<T>::insert(
			ChainId::Ethereum,
			VaultRotationStatus::<T>::AwaitingKeygen {
				keygen_ceremony_id: ceremony_id,
				candidates,
			},
		);

		Ok(())
	}
	
	fn finalize_rotation() -> Result<(), Self::RotationError> {
		// The 'exit' point for the pallet, no rotations left to process
		if Pallet::<T>::no_active_chain_vault_rotations() {
			// The process has completed successfully
			Self::deposit_event(Event::VaultsRotated);
			Ok(())
		} else {
			// Wait on confirmation
			Err(Error::<T>::NotConfirmed)
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
