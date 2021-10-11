#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)] // NOTE: This is stable as of rustc v1.54.0
#![doc = include_str!("../README.md")]

use frame_support::pallet_prelude::*;
use sp_std::prelude::*;

use cf_chains::{
	eth::{self, set_agg_key_with_agg_key::SetAggKeyWithAggKey, ChainflipKey},
	ChainId, Ethereum,
};
use cf_traits::{
	offline_conditions::{OfflineCondition, OfflineReporter},
	Chainflip, Nonce, NonceProvider, SigningContext, ThresholdSigner, VaultRotationHandler,
	VaultRotator,
};
pub use pallet::*;

pub use crate::rotation::VaultRotationResponse;
pub use crate::rotation::*;
use sp_runtime::traits::One;

pub mod rotation;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

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
			+ Into<ChainflipKey>
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

	/// The Vault for this instance
	#[pallet::storage]
	#[pallet::getter(fn eth_vault)]
	pub(super) type Vaults<T: Config> = StorageMap<_, Blake2_128Concat, ChainId, Vault<T>>;

	/// A map acting as a list of our current vault rotations
	#[pallet::storage]
	#[pallet::getter(fn active_chain_vault_rotations)]
	pub(super) type ActiveChainVaultRotations<T: Config> =
		StorageMap<_, Blake2_128Concat, CeremonyId, VaultRotation<T>>;

	/// A map of Nonces for chains supported
	#[pallet::storage]
	#[pallet::getter(fn chain_nonces)]
	pub(super) type ChainNonces<T: Config> =
		StorageMap<_, Blake2_128Concat, ChainId, Nonce>;

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
		/// Request a rotation of the vault for this chain \[ceremony_id, request\]
		VaultRotationRequest(CeremonyId),
		/// The vault for the request has rotated \[ceremony_id\]
		VaultRotationCompleted(CeremonyId),
		/// A rotation of vaults has been aborted \[ceremony_ides\]
		RotationAborted(Vec<CeremonyId>),
		/// A complete set of vaults have been rotated
		VaultsRotated,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An invalid ceremony id
		InvalidCeremonyId,
		/// We have an empty validator set
		EmptyValidatorSet,
		/// The key in the response is not different to the current key
		KeyUnchanged,
		/// A vault rotation has failed
		VaultRotationCompletionFailed,
		/// A key generation response has failed
		KeygenResponseFailed,
		/// A vault rotation has failed
		VaultRotationFailed,
		/// A set of badly acting validators
		BadValidators,
		/// Failed to construct a valid chain specific payload for rotation
		FailedToConstructPayload,
		/// The rotation has not been confirmed
		NotConfirmed,
		/// Failed to make a key generation request
		FailedToMakeKeygenRequest,
		/// New public key not set by keygen_response
		NewPublicKeyNotSet,
		///
		NoActiveRotation,
		/// The specified chain is not supported.
		UnsupportedChain,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A key generation succeeded. Update the state of the rotation and attempt to
		#[pallet::weight(10_000)]
		pub fn keygen_success(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			chain_id: ChainId,
			new_public_key: T::PublicKey,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure_index!(ceremony_id);

			let vault = Vaults::<T>::get(chain_id).ok_or(Error::<T>::UnsupportedChain)?;
			if vault.current_key == new_public_key {
				// TODO: we probably shouldn't be updating the state if we're returning an error.
				Pallet::<T>::abort_rotation();
				return Err(Error::<T>::KeyUnchanged.into());
			}

			// TODO: we only want to do this once *all* of the keygen ceremonies have succeeded.
			ActiveChainVaultRotations::<T>::mutate(ceremony_id, |maybe_vault_rotation| {
				*maybe_vault_rotation = Some(VaultRotation {
					new_public_key: Some(new_public_key.clone()),
				});
			});

			// TODO: This is implicitly also broadcasts the transaction - could be made clearer.
			T::ThresholdSigner::request_transaction_signature(SetAggKeyWithAggKey::new_unsigned(
				<Self as NonceProvider<Ethereum>>::next_nonce(),
				new_public_key,
			));

			Ok(().into())
		}

		/// Key generation failed. We report the guilty parties and abort.
		#[pallet::weight(10_000)]
		pub fn keygen_failure(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			guilty_validators: Vec<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure_index!(ceremony_id);

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

		/// A vault rotation transaction succeeeded.
		#[pallet::weight(10_000)]
		pub fn vault_rotation_success(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			chain_id: ChainId,
			_tx_hash: T::TransactionHash,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let new_key = ActiveChainVaultRotations::<T>::get(ceremony_id)
				.ok_or(Error::<T>::InvalidCeremonyId)?
				.new_public_key
				.ok_or(Error::<T>::NewPublicKeyNotSet)?;

			Vaults::<T>::try_mutate_exists(chain_id, |maybe_vault| {
				if let Some(mut vault) = maybe_vault.as_mut() {
					vault.current_key = new_key;
					Ok(())
				} else {
					Err(Error::<T>::InvalidCeremonyId)
				}
			})?;

			Pallet::<T>::deposit_event(Event::VaultRotationCompleted(ceremony_id));

			Ok(().into())
		}

		/// A vault rotation response received from a vault rotation request and handled
		/// by [VaultRotationRequestResponse::handle_response]
		#[pallet::weight(10_000)]
		pub fn vault_rotation_abort(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;
			ensure_index!(ceremony_id);
			Pallet::<T>::abort_rotation();
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
			Vaults::<T>::insert(ChainId::Ethereum, Vault {
				current_key: self.ethereum_vault_key.clone(),
			});
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
			ActiveChainVaultRotations::<T>::iter()
				.map(|(k, _)| k)
				.collect(),
		));
		ActiveChainVaultRotations::<T>::remove_all();
		T::RotationHandler::abort();
	}

	fn no_active_chain_vault_rotations() -> bool {
		ActiveChainVaultRotations::<T>::iter().count() == 0
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

		Pallet::<T>::deposit_event(Event::KeygenRequest(ceremony_id, ChainId::Ethereum, candidates.clone()));
		ActiveChainVaultRotations::<T>::insert(
			ceremony_id,
			VaultRotation {
				new_public_key: None,
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
