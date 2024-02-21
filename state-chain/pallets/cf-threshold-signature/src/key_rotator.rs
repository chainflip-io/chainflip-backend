use super::*;
use cf_runtime_utilities::{log_or_panic, StorageDecodeVariant};
use cf_traits::{CfeMultisigRequest, KeyRotationStatusOuter, KeyRotator, VaultActivator};
use cfe_events::{KeyHandoverRequest, KeygenRequest};
use frame_support::{sp_runtime::traits::BlockNumberProvider, traits::PalletInfoAccess};

impl<T: Config<I>, I: 'static> KeyRotator for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// # Panics
	/// - If an empty BTreeSet of candidates is provided
	/// - If a ley rotation outcome is already Pending (i.e. there's one already in progress)
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
		match PendingKeyRotation::<T, I>::get() {
			Some(KeyRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key }) |
			Some(KeyRotationStatus::<T, I>::KeyHandoverFailed { new_public_key, .. }) =>
				match Self::active_epoch_key() {
					Some(epoch_key) if T::TargetChainCrypto::key_handover_is_required() => {
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
						PendingKeyRotation::<T, I>::put(KeyRotationStatus::KeyHandoverComplete {
							new_public_key,
						});
						Self::deposit_event(Event::<T, I>::NoKeyHandover);
					},
				},
			Some(KeyRotationStatus::<T, I>::KeyHandoverComplete { .. }) => {
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
	fn status() -> AsyncResult<KeyRotationStatusOuter<T::ValidatorId>> {
		if let Some(status_variant) = PendingKeyRotation::<T, I>::decode_variant() {
			match status_variant {
				KeyRotationStatusVariant::AwaitingKeygen => AsyncResult::Pending,
				KeyRotationStatusVariant::AwaitingKeygenVerification => AsyncResult::Pending,
				KeyRotationStatusVariant::AwaitingKeyHandoverVerification => AsyncResult::Pending,
				// It's at this point we want the key to be considered ready to commit to. We
				// don't want to commit until the other keys are ready
				KeyRotationStatusVariant::KeygenVerificationComplete =>
					AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete),
				KeyRotationStatusVariant::AwaitingKeyHandover => AsyncResult::Pending,
				KeyRotationStatusVariant::KeyHandoverComplete =>
					AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete),
				KeyRotationStatusVariant::KeyHandoverFailed =>
					match PendingKeyRotation::<T, I>::get() {
						Some(KeyRotationStatus::KeyHandoverFailed { offenders, .. }) =>
							AsyncResult::Ready(KeyRotationStatusOuter::Failed(offenders)),
						_ => unreachable!(
							"Unreachable because we are in the branch for the Failed variant."
						),
					},
				KeyRotationStatusVariant::AwaitingActivation => {
					let (new_public_key, maybe_request_id) = match PendingKeyRotation::<T, I>::get()
					{
						Some(KeyRotationStatus::AwaitingActivation {
							request_id,
							new_public_key,
						}) => (new_public_key, request_id),
						_ => unreachable!(
							"Unreachable because we are in the branch for the AwaitingActivation variant."
						),
					};

					if let Some(request_id) = maybe_request_id {
						// After the ceremony completes, it is consumed and Void is left
						// behind. At this point we are sure the ceremony existed and we
						// completed it, we can activate the key
						if Signature::<T, I>::get(request_id) == AsyncResult::Void {
							T::VaultActivator::activate_key();
						};
					};

					let status = T::VaultActivator::status()
						.replace_inner(KeyRotationStatusOuter::RotationComplete);
					if status.is_ready() {
						Self::activate_new_key(new_public_key);
					}
					status
				},
				KeyRotationStatusVariant::Failed => match PendingKeyRotation::<T, I>::get() {
					Some(KeyRotationStatus::Failed { offenders }) =>
						AsyncResult::Ready(KeyRotationStatusOuter::Failed(offenders)),
					_ => unreachable!(
						"Unreachable because we are in the branch for the Failed variant."
					),
				},
				KeyRotationStatusVariant::Complete =>
					AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete),
			}
		} else {
			AsyncResult::Void
		}
	}

	fn reset_key_rotation() {
		PendingKeyRotation::<T, I>::kill();
		KeyHandoverResolutionPendingSince::<T, I>::kill();
		KeygenResolutionPendingSince::<T, I>::kill();
	}

	fn activate_keys() {
		if let Some(KeyRotationStatus::<T, I>::KeyHandoverComplete { new_public_key }) =
			PendingKeyRotation::<T, I>::get()
		{
			let maybe_active_epoch_key = Self::active_epoch_key();

			match T::VaultActivator::start_key_activation(
				new_public_key,
				maybe_active_epoch_key.map(|EpochKey { key, .. }| key),
			) {
				// If a request_id was returned we need to wait for the signing request to complete
				// before ending the rotation succesfully
				Some(request_id) => {
					PendingKeyRotation::<T, I>::put(
						KeyRotationStatus::<T, I>::AwaitingActivation {
							request_id: Some(request_id),
							new_public_key,
						},
					);
				},
				// if none was returned no ceremony is required and we can already complete the
				// rotation/wait for governance extrinsic to activate the vault
				None =>
					if maybe_active_epoch_key.is_some() {
						Self::activate_new_key(new_public_key);
					} else {
						PendingKeyRotation::<T, I>::put(
							KeyRotationStatus::<T, I>::AwaitingActivation {
								request_id: None,
								new_public_key,
							},
						);
					},
			}
		} else {
			log::error!("Vault activation called during wrong state.");
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>>) {
		match outcome {
			AsyncResult::Pending => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::AwaitingKeygen {
					ceremony_id: Default::default(),
					keygen_participants: Default::default(),
					response_status: KeygenResponseStatus::new(Default::default()),
					new_epoch_index: Default::default(),
				});
			},
			AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete) => {
				PendingKeyRotation::<T, I>::put(
					KeyRotationStatus::<T, I>::KeygenVerificationComplete {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete) => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::KeyHandoverComplete {
					new_public_key: Default::default(),
				});
			},
			AsyncResult::Ready(KeyRotationStatusOuter::Failed(offenders)) => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::Failed { offenders });
			},
			AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete) => {
				PendingKeyRotation::<T, I>::put(KeyRotationStatus::<T, I>::Complete);
			},
			AsyncResult::Void => {
				PendingKeyRotation::<T, I>::kill();
			},
		}
	}
}
