use super::*;
use cf_chains::SetAggKeyWithAggKeyError;
use cf_runtime_utilities::{log_or_panic, StorageDecodeVariant};
use cf_traits::{FirstVault, GetBlockHeight, VaultActivator};

impl<T: Config<I>, I: 'static> VaultActivator<<T::Chain as Chain>::ChainCrypto> for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// Get the status of the current key generation
	fn status() -> AsyncResult<()> {
		if let Some(status_variant) = PendingVaultActivation::<T, I>::decode_variant() {
			match status_variant {
				VaultActivationStatusVariant::AwaitingActivation => AsyncResult::Pending,
				VaultActivationStatusVariant::Complete => AsyncResult::Ready(()),
			}
		} else {
			AsyncResult::Void
		}
	}

	fn activate(
		new_public_key: AggKeyFor<T, I>,
		maybe_old_public_key: Option<AggKeyFor<T, I>>,
	) -> FirstVault {
		// If this storage item exists, it means this chain is already active we will rotate
		// normally
		if VaultStartBlockNumbers::<T, I>::iter_keys().next().is_some() {
			match <T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
				maybe_old_public_key,
				new_public_key,
			) {
				Ok(activation_call) => {
					T::Broadcaster::threshold_sign_and_broadcast_rotation_tx(activation_call);
					Self::activate_new_key_for_chain(T::ChainTracking::get_block_height());
				},
				Err(SetAggKeyWithAggKeyError::NotRequired) => {
					// This can happen if, for example, on a utxo chain there are no funds that
					// need to be swept.
					Self::activate_new_key_for_chain(T::ChainTracking::get_block_height());
				},
				Err(SetAggKeyWithAggKeyError::Failed) => {
					log_or_panic!(
						"Unexpected failure during {} vault activation.",
						<T::Chain as cf_chains::Chain>::NAME,
					);
				},
			}
			FirstVault::False
		}
		// If the chain is not active yet, we check this flag to decide whether we want to activate
		// this chain during this epoch rotation
		else if ChainInitialized::<T, I>::get() {
			// VaultStartBlockNumbers being empty means we are bootstrapping the vault.
			PendingVaultActivation::<T, I>::put(
				VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key },
			);
			Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation { new_public_key });
			FirstVault::True
		}
		// The case where the ChainInitialized flag is not set, we skip activation for this chain
		// and complete rotation since this chain is still not ready to be activated yet.
		else {
			PendingVaultActivation::<T, I>::put(VaultActivationStatus::<T, I>::Complete);
			FirstVault::False
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<()>) {
		match outcome {
			AsyncResult::Pending => {
				PendingVaultActivation::<T, I>::put(
					VaultActivationStatus::<T, I>::AwaitingActivation {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(_) => {
				PendingVaultActivation::<T, I>::put(VaultActivationStatus::<T, I>::Complete);
			},
			AsyncResult::Void => {
				PendingVaultActivation::<T, I>::kill();
			},
		}
	}
}
