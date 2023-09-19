use super::*;
use cf_chains::SetAggKeyWithAggKeyError;
use cf_runtime_utilities::log_or_panic;
use cf_traits::{CeremonyIdProvider, GetBlockHeight, VaultActivator};
use frame_support::sp_runtime::traits::BlockNumberProvider;

impl<T: Config<I>, I: 'static> VaultActivator<<T::Chain as Chain>::ChainCrypto> for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// Get the status of the current key generation
	fn status() -> AsyncResult<()> {
		if let Some(status_variant) = PendingVaultActivation::<T, I>::decode_variant() {
			match status_variant {
				VaultActivationStatusVariant::AwaitingActivation => AsyncResult::Pending,
				VaultActivationStatusVariant::Complete =>
					AsyncResult::Ready,
				
			}
		} else {
			AsyncResult::Void
		}
	}

	fn activate(new_public_key: AggKeyFor<T,I>) {
			if let Some(EpochKey { key, key_state, .. }) = Self::active_epoch_key() {
				match <T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
					Some(key),
					new_public_key,
				) {
					Ok(activation_call) => {
						let (_, threshold_request_id) =
							T::Broadcaster::threshold_sign_and_broadcast(activation_call);
						if <T::Chain as Chain>::OptimisticActivation::get() {
							// Optimistic activation means we don't need to wait for the activation
							// transaction to succeed before using the new key.
							Self::activate_new_key(
								new_public_key,
								T::ChainTracking::get_block_height(),
							);
						} else {
							debug_assert!(
								matches!(key_state, KeyState::Unlocked),
								"Current epoch key must be active to activate next key."
							);
							// The key needs to be locked until activation is complete.
							CurrentVaultEpochAndState::<T, I>::mutate(|epoch_end_key| {
								epoch_end_key
									.as_mut()
									.expect("Checked above at if let Some")
									.key_state
									.lock(threshold_request_id)
							});
							PendingVaultActivation::<T, I>::put(
								VaultActivationStatus::<T, I>::AwaitingActivation {
									new_public_key,
								},
							);
						}
					},
					Err(SetAggKeyWithAggKeyError::NotRequired) => {
						// This can happen if, for example, on a utxo chain there are no funds that
						// need to be swept.
						Self::activate_new_key(
							new_public_key,
							T::ChainTracking::get_block_height(),
						);
					},
					Err(SetAggKeyWithAggKeyError::Failed) => {
						log_or_panic!(
							"Unexpected failure during {} vault activation.",
							<T::Chain as cf_chains::Chain>::NAME,
						);
					},
				}
			} else {
				// No active key means we are bootstrapping the vault.
				PendingVaultActivation::<T, I>::put(
					VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key },
				);
				Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation { new_public_key });
			}
		} 
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<()>) {
		match outcome {
			AsyncResult::Pending => {
				PendingVaultActivation::<T, I>::put(
					VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key: Default::default() } ,
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
