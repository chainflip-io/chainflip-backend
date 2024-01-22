use super::*;
use cf_runtime_utilities::{log_or_panic, StorageDecodeVariant};
use cf_traits::{
	CfeMultisigRequest, MapAsyncResultTo, VaultActivator, VaultRotationStatusOuter, VaultRotator,
};
use cfe_events::{KeyHandoverRequest, KeygenRequest};
use frame_support::{sp_runtime::traits::BlockNumberProvider, traits::PalletInfoAccess};

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
					Some(epoch_key) if T::TargetChainCrypto::key_handover_is_required() => {
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
	fn status() -> AsyncResult<VaultRotationStatusOuter<T::ValidatorId>> {
		if let Some(status_variant) = PendingVaultRotation::<T, I>::decode_variant() {
			match status_variant {
				VaultRotationStatusVariant::AwaitingKeygen => AsyncResult::Pending,
				VaultRotationStatusVariant::AwaitingKeygenVerification => AsyncResult::Pending,
				VaultRotationStatusVariant::AwaitingKeyHandoverVerification => AsyncResult::Pending,
				// It's at this point we want the vault to be considered ready to commit to. We
				// don't want to commit until the other vaults are ready
				VaultRotationStatusVariant::KeygenVerificationComplete =>
					AsyncResult::Ready(VaultRotationStatusOuter::KeygenComplete),
				VaultRotationStatusVariant::AwaitingKeyHandover => AsyncResult::Pending,
				VaultRotationStatusVariant::KeyHandoverComplete =>
					AsyncResult::Ready(VaultRotationStatusOuter::KeyHandoverComplete),
				VaultRotationStatusVariant::KeyHandoverFailed =>
					match PendingVaultRotation::<T, I>::get() {
						Some(VaultRotationStatus::KeyHandoverFailed { offenders, .. }) =>
							AsyncResult::Ready(VaultRotationStatusOuter::Failed(offenders)),
						_ => unreachable!(
							"Unreachable because we are in the branch for the Failed variant."
						),
					},
				VaultRotationStatusVariant::AwaitingActivation => {
					let new_public_key = match PendingVaultRotation::<T, I>::get() {
						Some(VaultRotationStatus::AwaitingActivation { new_public_key }) =>
							new_public_key,
						_ => unreachable!(
							"Unreachable because we are in the branch for the AwaitingActivation variant."
						),
					};

					let status = T::VaultActivator::status()
						.map_to(VaultRotationStatusOuter::RotationComplete);
					if status.is_ready() {
						Self::activate_new_key(new_public_key);
					}
					status
				},
				VaultRotationStatusVariant::Failed => match PendingVaultRotation::<T, I>::get() {
					Some(VaultRotationStatus::Failed { offenders }) =>
						AsyncResult::Ready(VaultRotationStatusOuter::Failed(offenders)),
					_ => unreachable!(
						"Unreachable because we are in the branch for the Failed variant."
					),
				},
				VaultRotationStatusVariant::Complete =>
					AsyncResult::Ready(VaultRotationStatusOuter::RotationComplete),
			}
		} else {
			AsyncResult::Void
		}
	}

	fn reset_vault_rotation() {
		PendingVaultRotation::<T, I>::kill();
		KeyHandoverResolutionPendingSince::<T, I>::kill();
		KeygenResolutionPendingSince::<T, I>::kill();
	}

	fn activate_vaults() {
		if let Some(VaultRotationStatus::<T, I>::KeyHandoverComplete { new_public_key }) =
			PendingVaultRotation::<T, I>::get()
		{
			let maybe_active_epoch_key = Self::active_epoch_key();
			let _activation_tx_broadcast_ids = T::VaultActivator::activate(
				new_public_key,
				maybe_active_epoch_key.map(|EpochKey { key, .. }| key),
			);

			if maybe_active_epoch_key.is_some() {
				Self::activate_new_key(new_public_key);
			} else {
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::AwaitingActivation { new_public_key },
				);
			}
		} else {
			log::error!("Vault activation called during wrong state.");
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<VaultRotationStatusOuter<Self::ValidatorId>>) {
		match outcome {
			AsyncResult::Pending => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingKeygen {
					ceremony_id: Default::default(),
					keygen_participants: Default::default(),
					response_status: KeygenResponseStatus::new(Default::default()),
					new_epoch_index: Default::default(),
				});
			},
			AsyncResult::Ready(VaultRotationStatusOuter::KeygenComplete) => {
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::KeygenVerificationComplete {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(VaultRotationStatusOuter::KeyHandoverComplete) => {
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::KeyHandoverComplete {
						new_public_key: Default::default(),
					},
				);
			},
			AsyncResult::Ready(VaultRotationStatusOuter::Failed(offenders)) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Failed {
					offenders,
				});
			},
			AsyncResult::Ready(VaultRotationStatusOuter::RotationComplete) => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete);
			},
			AsyncResult::Void => {
				PendingVaultRotation::<T, I>::kill();
			},
		}
	}
}
