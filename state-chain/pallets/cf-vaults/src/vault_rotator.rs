use super::*;
use cf_chains::SetAggKeyWithAggKeyError;
use cf_runtime_utilities::log_or_panic;
use cf_traits::{CeremonyIdProvider, CfeMultisigRequest, GetBlockHeight};
use cfe_events::{KeyHandoverRequest, KeygenRequest};
use frame_support::sp_runtime::traits::BlockNumberProvider;

impl<T: Config<I>, I: 'static> VaultRotator for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// # Panics
	/// - If an empty BTreeSet of candidates is provided
	/// - If a vault rotation outcome is already Pending (i.e. there's one already in progress)
	fn keygen(candidates: BTreeSet<Self::ValidatorId>, new_epoch_index: EpochIndex) {
		assert!(!candidates.is_empty());

		assert_ne!(Self::status(), AsyncResult::Pending);

		let ceremony_id = Self::increment_ceremony_id();

		PendingVaultRotation::<T, I>::put(VaultRotationStatus::AwaitingKeygen {
			ceremony_id,
			keygen_participants: candidates.clone(),
			response_status: KeygenResponseStatus::new(candidates.clone()),
			new_epoch_index,
		});

		// Start the timer for resolving Keygen - we check this in the on_initialise() hook each
		// block
		KeygenResolutionPendingSince::<T, I>::put(frame_system::Pallet::<T>::current_block_number());

		T::CfeMultisigRequest::keygen_request(KeygenRequest {
			ceremony_id,
			epoch_index: new_epoch_index,
			participants: candidates.clone(),
		});

		// TODO: consider deleting this
		Pallet::<T, I>::deposit_event(Event::KeygenRequest {
			ceremony_id,
			participants: candidates,
			epoch_index: new_epoch_index,
		});
	}

	/// Kicks off the key handover process
	fn key_handover(
		sharing_participants: BTreeSet<Self::ValidatorId>,
		receiving_participants: BTreeSet<Self::ValidatorId>,
		new_epoch_index: EpochIndex,
	) {
		assert_ne!(Self::status(), AsyncResult::Pending);
		match PendingVaultRotation::<T, I>::get() {
			Some(VaultRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key }) |
			Some(VaultRotationStatus::<T, I>::KeyHandoverFailed { new_public_key, .. }) =>
				match Self::active_epoch_key() {
					Some(epoch_key)
						if <T::Chain as Chain>::ChainCrypto::key_handover_is_required() =>
					{
						assert!(
							!sharing_participants.is_empty() && !receiving_participants.is_empty()
						);

						let ceremony_id = Self::increment_ceremony_id();

						// from the SC's perspective, we don't care what set they're in, they get
						// reported the same and each participant only gets one vote, like keygen.
						let all_participants =
							sharing_participants.union(&receiving_participants).cloned().collect();

						PendingVaultRotation::<T, I>::put(
							VaultRotationStatus::AwaitingKeyHandover {
								ceremony_id,
								response_status: KeyHandoverResponseStatus::new(all_participants),
								receiving_participants: receiving_participants.clone(),
								next_epoch: new_epoch_index,
								new_public_key,
							},
						);

						KeyHandoverResolutionPendingSince::<T, I>::put(
							frame_system::Pallet::<T>::current_block_number(),
						);

						T::CfeMultisigRequest::key_handover_request(KeyHandoverRequest {
							ceremony_id,
							from_epoch: epoch_key.epoch_index,
							to_epoch: new_epoch_index,
							key_to_share: epoch_key.key,
							sharing_participants: sharing_participants.clone(),
							receiving_participants: receiving_participants.clone(),
							new_key: new_public_key,
						});

						// TODO: consider removing this
						Self::deposit_event(Event::KeyHandoverRequest {
							ceremony_id,
							// The key we want to share is the key from the *previous/current*
							// epoch, not the newly generated key since we're handing it over to the
							// authorities of the new_epoch.
							from_epoch: epoch_key.epoch_index,
							key_to_share: epoch_key.key,
							sharing_participants,
							receiving_participants,
							new_key: new_public_key,
							to_epoch: new_epoch_index,
						});
					},
					_ => {
						// We don't do a handover if:
						// - We are not a chain that requires handover
						// - We are a chain that requires handover, but we are doing the first
						//   rotation
						PendingVaultRotation::<T, I>::put(
							VaultRotationStatus::KeyHandoverComplete { new_public_key },
						);
						Self::deposit_event(Event::<T, I>::NoKeyHandover);
					},
				},
			Some(VaultRotationStatus::<T, I>::KeyHandoverComplete { .. }) => {
				log::info!(
					target: Pallet::<T, I>::name(),
					"Key handover already complete."
				);
			},
			other => {
				log_or_panic!("Key handover initiated during invalid state {:?}.", other);
			},
		}
	}

	/// Get the status of the current key generation
	fn status() -> AsyncResult<VaultStatus<T::ValidatorId>> {
		if let Some(status_variant) = PendingVaultRotation::<T, I>::decode_variant() {
			match status_variant {
				VaultRotationStatusVariant::AwaitingKeygen => AsyncResult::Pending,
				VaultRotationStatusVariant::AwaitingKeygenVerification => AsyncResult::Pending,
				VaultRotationStatusVariant::AwaitingKeyHandoverVerification => AsyncResult::Pending,
				// It's at this point we want the vault to be considered ready to commit to. We
				// don't want to commit until the other vaults are ready
				VaultRotationStatusVariant::KeygenVerificationComplete =>
					AsyncResult::Ready(VaultStatus::KeygenComplete),
				VaultRotationStatusVariant::AwaitingKeyHandover => AsyncResult::Pending,
				VaultRotationStatusVariant::KeyHandoverComplete =>
					AsyncResult::Ready(VaultStatus::KeyHandoverComplete),
				VaultRotationStatusVariant::KeyHandoverFailed =>
					match PendingVaultRotation::<T, I>::get() {
						Some(VaultRotationStatus::KeyHandoverFailed { offenders, .. }) =>
							AsyncResult::Ready(VaultStatus::Failed(offenders)),
						_ => unreachable!(
							"Unreachable because we are in the branch for the Failed variant."
						),
					},
				VaultRotationStatusVariant::AwaitingActivation => AsyncResult::Pending,
				VaultRotationStatusVariant::Complete =>
					AsyncResult::Ready(VaultStatus::RotationComplete),
				VaultRotationStatusVariant::Failed => match PendingVaultRotation::<T, I>::get() {
					Some(VaultRotationStatus::Failed { offenders }) =>
						AsyncResult::Ready(VaultStatus::Failed(offenders)),
					_ => unreachable!(
						"Unreachable because we are in the branch for the Failed variant."
					),
				},
			}
		} else {
			AsyncResult::Void
		}
	}

	fn activate() {
		if let Some(VaultRotationStatus::<T, I>::KeyHandoverComplete { new_public_key }) =
			PendingVaultRotation::<T, I>::get()
		{
			if let Some(EpochKey { key, .. }) = Self::active_epoch_key() {
				match <T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
					Some(key),
					new_public_key,
				) {
					Ok(activation_call) => {
						T::Broadcaster::threshold_sign_and_broadcast_rotation_tx(activation_call);

						Self::activate_new_key(
							new_public_key,
							T::ChainTracking::get_block_height(),
						);
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
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::AwaitingActivation { new_public_key },
				);
				Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation { new_public_key });
			}
		} else {
			log_or_panic!("activate key called before key handover completed");
		}
	}

	fn reset_vault_rotation() {
		PendingVaultRotation::<T, I>::kill();
		KeyHandoverResolutionPendingSince::<T, I>::kill();
		KeygenResolutionPendingSince::<T, I>::kill();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<VaultStatus<Self::ValidatorId>>) {
		match outcome {
			AsyncResult::Pending => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingKeygen {
					ceremony_id: Default::default(),
					keygen_participants: Default::default(),
					response_status: KeygenResponseStatus::new(Default::default()),
					new_epoch_index: Default::default(),
				});
			},
			AsyncResult::Ready(VaultStatus::KeygenComplete) => {
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::KeygenVerificationComplete {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(VaultStatus::KeyHandoverComplete) => {
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::KeyHandoverComplete {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(VaultStatus::Failed(offenders)) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
					offenders,
				});
			},
			AsyncResult::Ready(VaultStatus::RotationComplete) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete);
			},
			AsyncResult::Void => {
				PendingVaultRotation::<T, I>::kill();
			},
		}
	}
}
