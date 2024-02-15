use super::*;
use cf_chains::SetAggKeyWithAggKeyError;
use cf_primitives::ThresholdSignatureRequestId;
use cf_runtime_utilities::{log_or_panic, StorageDecodeVariant};
use cf_traits::{GetBlockHeight, VaultActivator};

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

	fn activate_key() {
		Self::activate_new_key_for_chain(T::ChainTracking::get_block_height());
	}

	fn start_key_activation(
		new_public_key: AggKeyFor<T, I>,
		maybe_old_public_key: Option<AggKeyFor<T, I>>,
	) -> Option<ThresholdSignatureRequestId> {
		if let Some(old_key) = maybe_old_public_key {
			match <T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
				Some(old_key),
				new_public_key,
			) {
				Ok(activation_call) => {
					// we need to sign and submit the rotation call
					// reporting back the request_id of the tss such that we can complete the
					// rotation when that request is completed
					let (_, tss_request_id) =
						T::Broadcaster::threshold_sign_and_broadcast_rotation_tx(activation_call);
					// since vaults are activated only when the tss completes we need to initiate
					// the activation
					PendingVaultActivation::<T, I>::put(
						VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key },
					);
					Some(tss_request_id)
				},
				Err(SetAggKeyWithAggKeyError::NotRequired) => {
					// This can happen if, for example, on a utxo chain there are no funds that
					// need to be swept.
					Self::activate_new_key_for_chain(T::ChainTracking::get_block_height());
					None
				},
				Err(SetAggKeyWithAggKeyError::Failed) => {
					log_or_panic!(
						"Unexpected failure during {} vault activation.",
						<T::Chain as cf_chains::Chain>::NAME,
					);
					None
				},
			}
		} else {
			// No active key means we are bootstrapping the vault.
			PendingVaultActivation::<T, I>::put(
				VaultActivationStatus::<T, I>::AwaitingActivation { new_public_key },
			);
			Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation { new_public_key });
			None
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
