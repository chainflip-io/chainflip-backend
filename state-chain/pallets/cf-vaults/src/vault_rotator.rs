use super::*;
use cf_traits::{CeremonyIdProvider, GetBlockHeight};
use sp_runtime::traits::BlockNumberProvider;

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
		if let Some(VaultRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key }) =
			PendingVaultRotation::<T, I>::get()
		{
			match Self::current_epoch_key() {
				Some(epoch_key) if <T::Chain as Chain>::KeyHandoverIsRequired::get() => {
					assert!(!sharing_participants.is_empty() && !receiving_participants.is_empty());

					assert_ne!(Self::status(), AsyncResult::Pending);

					let ceremony_id = Self::increment_ceremony_id();

					// from the SC's perspective, we don't care what set they're in, they get
					// reported the same and each participant only gets one vote, like keygen.
					let all_participants =
						sharing_participants.union(&receiving_participants).cloned().collect();

					PendingVaultRotation::<T, I>::put(VaultRotationStatus::AwaitingKeyHandover {
						ceremony_id,
						response_status: KeyHandoverResponseStatus::new(all_participants),
						new_public_key,
					});

					KeyHandoverResolutionPendingSince::<T, I>::put(
						frame_system::Pallet::<T>::current_block_number(),
					);

					Self::deposit_event(Event::KeyHandoverRequest {
						ceremony_id,
						// The key we want to share is the key from the *previous/current* epoch,
						// not the newly generated key since we're handing it over to the
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
					// - We are a chain that requires handover, but we are doing the first rotation
					PendingVaultRotation::<T, I>::put(VaultRotationStatus::KeyHandoverComplete {
						new_public_key,
					});
					Self::deposit_event(Event::<T, I>::NoKeyHandover);
				},
			}
		} else {
			debug_assert!(
				false,
				"We should have completed keygen verification before starting key handover."
			)
		}
	}

	/// Get the status of the current key generation
	fn status() -> AsyncResult<VaultStatus<T::ValidatorId>> {
		if let Some(status_variant) = PendingVaultRotation::<T, I>::decode_variant() {
			match status_variant {
				VaultRotationStatusVariant::AwaitingKeygen => AsyncResult::Pending,
				VaultRotationStatusVariant::AwaitingKeygenVerification => AsyncResult::Pending,
				// It's at this point we want the vault to be considered ready to commit to. We
				// don't want to commit until the other vaults are ready
				VaultRotationStatusVariant::KeygenVerificationComplete =>
					AsyncResult::Ready(VaultStatus::KeygenComplete),
				VaultRotationStatusVariant::AwaitingKeyHandover => AsyncResult::Pending,
				VaultRotationStatusVariant::KeyHandoverComplete =>
					AsyncResult::Ready(VaultStatus::KeyHandoverComplete),
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
			if let Some((rotation_call, key_state, epoch_index)) = Self::current_epoch_key()
				.and_then(|EpochKey { key, epoch_index, key_state }| {
					<T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
						Some(key),
						new_public_key,
					)
					.ok()
					.map(|call| (call, key_state, epoch_index))
				}) {
				let (_, threshold_request_id) =
					T::Broadcaster::threshold_sign_and_broadcast(rotation_call);
				if <T::Chain as Chain>::OptimisticActivation::get() {
					Self::set_next_vault(new_public_key, T::ChainTracking::get_block_height());
					Pallet::<T, I>::deposit_event(Event::VaultRotationCompleted);
					PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete);
				} else {
					debug_assert!(
						matches!(key_state, KeyState::Unlocked),
						"Current epoch key must be active to activate next key."
					);
					CurrentVaultEpochAndState::<T, I>::put(VaultEpochAndState {
						epoch_index,
						key_state: KeyState::Locked(threshold_request_id),
					});
					PendingVaultRotation::<T, I>::put(
						VaultRotationStatus::<T, I>::AwaitingActivation { new_public_key },
					);
				}
			} else {
				// The block number value 1, which the vault is being set with is a dummy value
				// and doesn't mean anything. It will be later modified to the real value when
				// we witness the vault rotation manually via governance
				Self::set_vault_for_next_epoch(new_public_key, 1_u32.into());
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::AwaitingActivation { new_public_key },
				);
				Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation { new_public_key });
			}
		} else {
			#[cfg(not(test))]
			log::error!("activate key called before key handover completed");
			#[cfg(test)]
			panic!("activate key called before keygen handover completed");
		}
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
