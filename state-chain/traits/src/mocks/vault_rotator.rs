use crate::{mocks::MockPalletStorage, AsyncResult, VaultRotationStatusOuter, VaultRotator};
use cf_primitives::EpochIndex;
use sp_std::collections::btree_set::BTreeSet;

use super::MockPallet;

const ROTATION_OUTCOME: &[u8] = b"ROTATION_OUTCOME";

macro_rules! mock_vault_rotator {
	($rotator_name:ident) => {
		pub struct $rotator_name;

		impl MockPallet for $rotator_name {
			const PREFIX: &'static [u8] = stringify!($rotator_name).as_bytes();
		}

		impl $rotator_name {
			pub fn keygen_success() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Ready(
						VaultRotationStatusOuter::KeygenComplete,
					),
				);
			}

			pub fn key_handover_success() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Ready(
						VaultRotationStatusOuter::KeyHandoverComplete,
					),
				);
			}

			pub fn keys_activated() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Ready(
						VaultRotationStatusOuter::RotationComplete,
					),
				);
			}

			pub fn failed<O: IntoIterator<Item = u64>>(offenders: O) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Ready(
						VaultRotationStatusOuter::Failed(offenders.into_iter().collect()),
					),
				);
			}

			pub fn pending() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Pending,
				)
			}
		}

		impl VaultRotator for $rotator_name {
			type ValidatorId = u64;

			fn keygen(_candidates: BTreeSet<Self::ValidatorId>, _new_epoch_index: EpochIndex) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Pending,
				);
			}

			fn key_handover(
				_old_participants: BTreeSet<Self::ValidatorId>,
				_new_candidates: BTreeSet<Self::ValidatorId>,
				_epoch_index: EpochIndex,
			) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Pending,
				);
			}

			fn status() -> AsyncResult<VaultRotationStatusOuter<Self::ValidatorId>> {
				Self::get_value(ROTATION_OUTCOME).unwrap_or_default()
			}

			fn activate_vaults() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Pending,
				);
			}

			fn reset_vault_rotation() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultRotationStatusOuter<u64>>::Void,
				);
			}

			#[cfg(feature = "runtime-benchmarks")]
			fn set_status(_outcome: AsyncResult<VaultRotationStatusOuter<Self::ValidatorId>>) {
				unimplemented!()
			}
		}
	};
}

mock_vault_rotator!(MockVaultRotatorA);
mock_vault_rotator!(MockVaultRotatorB);
mock_vault_rotator!(MockVaultRotatorC);
