use super::*;
use cf_traits::EpochInfo;
use sp_runtime::traits::BlockNumberProvider;

impl<T: Config<I>, I: 'static> VaultRotator for Pallet<T, I> {
	type ValidatorId = T::ValidatorId;

	/// # Panics
	/// - If an empty BTreeSet of candidates is provided
	/// - If a vault rotation outcome is already Pending (i.e. there's one already in progress)
	fn keygen(candidates: BTreeSet<Self::ValidatorId>) {
		assert!(!candidates.is_empty());

		assert_ne!(Self::status(), AsyncResult::Pending);

		let ceremony_id = T::CeremonyIdProvider::increment_ceremony_id();

		PendingVaultRotation::<T, I>::put(VaultRotationStatus::AwaitingKeygen {
			keygen_ceremony_id: ceremony_id,
			keygen_participants: candidates.clone(),
			response_status: KeygenResponseStatus::new(candidates.clone()),
		});

		// Start the timer for resolving Keygen - we check this in the on_initialise() hook each
		// block
		KeygenResolutionPendingSince::<T, I>::put(frame_system::Pallet::<T>::current_block_number());

		Pallet::<T, I>::deposit_event(Event::KeygenRequest {
			ceremony_id,
			participants: candidates,
			epoch_index: <T as Chainflip>::EpochInfo::epoch_index() + 1,
		});
	}

	/// Get the status of the current key generation
	fn status() -> AsyncResult<VaultStatus<T::ValidatorId>> {
		match PendingVaultRotation::<T, I>::decode_variant() {
			Some(VaultRotationStatusVariant::AwaitingKeygen) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::AwaitingKeygenVerification) => AsyncResult::Pending,
			// It's at this point we want the vault to be considered ready to commit to. We don't
			// want to commit until the other vaults are ready
			Some(VaultRotationStatusVariant::KeygenVerificationComplete) =>
				AsyncResult::Ready(VaultStatus::KeygenComplete),
			Some(VaultRotationStatusVariant::AwaitingRotation) => AsyncResult::Pending,
			Some(VaultRotationStatusVariant::Complete) =>
				AsyncResult::Ready(VaultStatus::RotationComplete),
			Some(VaultRotationStatusVariant::Failed) => match PendingVaultRotation::<T, I>::get() {
				Some(VaultRotationStatus::Failed { offenders }) =>
					AsyncResult::Ready(VaultStatus::Failed(offenders)),
				_ =>
					unreachable!("Unreachable because we are in the branch for the Failed variant."),
			},
			None => AsyncResult::Void,
		}
	}

	fn activate() {
		if let Some(VaultRotationStatus::<T, I>::KeygenVerificationComplete { new_public_key }) =
			PendingVaultRotation::<T, I>::get()
		{
			if let Some(EpochKey { key, epoch_index, key_state }) = Self::current_epoch_key() {
				if let Ok(rotation_call) =
					<T::SetAggKeyWithAggKey as SetAggKeyWithAggKey<_>>::new_unsigned(
						Some(key),
						new_public_key,
					) {
					let (_, threshold_request_id) =
						T::Broadcaster::threshold_sign_and_broadcast(rotation_call);
					debug_assert!(
						matches!(key_state, KeyState::Unlocked),
						"Current epoch key must be active to activate next key."
					);
					CurrentVaultEpochAndState::<T, I>::put(VaultEpochAndState {
						epoch_index,
						key_state: KeyState::Locked(threshold_request_id),
					});
				} else {
					// TODO: Fix the integration tests and/or SetAggKeyWithAggKey impls so that this
					// is actually unreachable.
					log::error!(
						"Unable to create unsigned transaction to set new vault key. This should not be possible."
					);
				}
			} else {
				// The block number value 1, which the vault is being set with is a dummy value
				// and doesn't mean anything. It will be later modified to the real value when
				// we witness the vault rotation manually via governance
				Self::set_vault_for_next_epoch(new_public_key, 1_u32.into());
				Self::deposit_event(Event::<T, I>::AwaitingGovernanceActivation { new_public_key })
			}

			PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingRotation {
				new_public_key,
			});
		} else {
			#[cfg(not(test))]
			log::error!("activate key called before keygen verification completed");
			#[cfg(test)]
			panic!("activate key called before keygen verification completed");
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<VaultStatus<Self::ValidatorId>>) {
		use cf_chains::benchmarking_value::BenchmarkValue;

		match outcome {
			AsyncResult::Pending => {
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::AwaitingKeygen {
					keygen_ceremony_id: Default::default(),
					keygen_participants: Default::default(),
					response_status: KeygenResponseStatus::new(Default::default()),
				});
			},
			AsyncResult::Ready(VaultStatus::KeygenComplete) => {
				PendingVaultRotation::<T, I>::put(
					VaultRotationStatus::<T, I>::KeygenVerificationComplete {
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
				PendingVaultRotation::<T, I>::put(VaultRotationStatus::<T, I>::Complete {
					tx_id: BenchmarkValue::benchmark_value(),
				});
			},
			AsyncResult::Void => {
				PendingVaultRotation::<T, I>::kill();
			},
		}
	}
}
