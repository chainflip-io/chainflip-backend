use super::*;
use cf_chains::SetAggKeyWithAggKeyError;
use cf_runtime_utilities::log_or_panic;
use cf_traits::{CeremonyIdProvider, GetBlockHeight};
use frame_support::sp_runtime::traits::BlockNumberProvider;

impl<T: Config<I>, I: 'static> KeyRotator for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// # Panics
	/// - If an empty BTreeSet of candidates is provided
	/// - If a vault rotation outcome is already Pending (i.e. there's one already in progress)
	fn keygen(candidates: BTreeSet<Self::ValidatorId>, new_epoch_index: EpochIndex) {
		assert!(!candidates.is_empty());

		assert_ne!(Self::status(), AsyncResult::Pending);

		let ceremony_id = Self::increment_ceremony_id();

		PendingKeyRotation::<T, I>::put(KeyRotationStatus::AwaitingKeygen {
			ceremony_id,
			keygen_participants: candidates.clone(),
			response_status: KeygenResponseStatus::new(candidates.clone()),
			new_epoch_index,
		});

		// Start the timer for resolving Keygen - we check this in the on_initialise() hook each
		// block
		KeygenResolutionPendingSince::<T, I>::put(frame_system::Pallet::<T>::current_block_number());

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
		match PendingKeyRotation::<T, I>::get() {
			Some(KeyRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key }) |
			Some(KeyRotationStatus::<T, I>::KeyHandoverFailed { new_public_key, .. }) =>
				match Self::active_epoch_key() {
					Some(epoch_key) if <T::Chain as Chain>::KeyHandoverIsRequired::get() => {
						assert!(
							!sharing_participants.is_empty() && !receiving_participants.is_empty()
						);

						let ceremony_id = Self::increment_ceremony_id();

						// from the SC's perspective, we don't care what set they're in, they get
						// reported the same and each participant only gets one vote, like keygen.
						let all_participants =
							sharing_participants.union(&receiving_participants).cloned().collect();

						PendingKeyRotation::<T, I>::put(KeyRotationStatus::AwaitingKeyHandover {
							ceremony_id,
							response_status: KeyHandoverResponseStatus::new(all_participants),
							receiving_participants: receiving_participants.clone(),
							next_epoch: new_epoch_index,
							new_public_key,
						});

						KeyHandoverResolutionPendingSince::<T, I>::put(
							frame_system::Pallet::<T>::current_block_number(),
						);

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
						PendingKeyRotation::<T, I>::put(KeyRotationStatus::KeyHandoverComplete {
							new_public_key,
						});
						Self::deposit_event(Event::<T, I>::NoKeyHandover);
					},
				},
			_ => {
				debug_assert!(
					false,
					"We should have completed keygen verification before starting key handover."
				);
				log::error!("Key handover called during wrong wrong state.");
			},
		}
	}

	/// Get the status of the current key generation
	fn status() -> AsyncResult<KeyRotationStatus<T::ValidatorId>> {
		if let Some(status_variant) = PendingKeyRotation::<T, I>::decode_variant() {
			match status_variant {
				KeyRotationStatusVariant::AwaitingKeygen => AsyncResult::Pending,
				KeyRotationStatusVariant::AwaitingKeygenVerification => AsyncResult::Pending,
				KeyRotationStatusVariant::AwaitingKeyHandoverVerification => AsyncResult::Pending,
				// It's at this point we want the vault to be considered ready to commit to. We
				// don't want to commit until the other vaults are ready
				KeyRotationStatusVariant::KeygenVerificationComplete =>
					AsyncResult::Ready(KeyRotationStatus::KeygenComplete),
				KeyRotationStatusVariant::AwaitingKeyHandover => AsyncResult::Pending,
				KeyRotationStatusVariant::KeyHandoverComplete =>
					AsyncResult::Ready(KeyRotationStatus::KeyHandoverComplete),
				KeyRotationStatusVariant::KeyHandoverFailed =>
					match PendingKeyRotation::<T, I>::get() {
						Some(KeyRotationStatus::KeyHandoverFailed { offenders, .. }) =>
							AsyncResult::Ready(KeyRotationStatus::Failed(offenders)),
						_ => unreachable!(
							"Unreachable because we are in the branch for the Failed variant."
						),
					},
				KeyRotationStatusVariant::Failed => match PendingKeyRotation::<T, I>::get() {
					Some(KeyRotationStatus::Failed { offenders }) =>
						AsyncResult::Ready(KeyRotationStatus::Failed(offenders)),
					_ => unreachable!(
						"Unreachable because we are in the branch for the Failed variant."
					),
				},
			}
		} else {
			AsyncResult::Void
		}
	}

	fn reset_vault_rotation() {
		PendingKeyRotation::<T, I>::kill();
		KeyHandoverResolutionPendingSince::<T, I>::kill();
		KeygenResolutionPendingSince::<T, I>::kill();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<KeyRotationStatus<Self::ValidatorId>>) {
		match outcome {
			AsyncResult::Pending => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::AwaitingKeygen {
					ceremony_id: Default::default(),
					keygen_participants: Default::default(),
					response_status: KeygenResponseStatus::new(Default::default()),
					new_epoch_index: Default::default(),
				});
			},
			AsyncResult::Ready(KeyRotationStatus::KeygenComplete) => {
				PendingKeyRotation::<T, I>::put(
					KeyRotationStatus::<T, I>::KeygenVerificationComplete {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(KeyRotationStatus::KeyHandoverComplete) => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::KeyHandoverComplete {
					new_public_key: Default::default(),
				});
			},
			AsyncResult::Ready(KeyRotationStatus::Failed(offenders)) => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::Failed { offenders });
			},
			AsyncResult::Ready(KeyRotationStatus::RotationComplete) => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::Complete);
			},
			AsyncResult::Void => {
				PendingKeyRotation::<T, I>::kill();
			},
		}
	}
}
