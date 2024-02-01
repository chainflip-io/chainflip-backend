use crate::{mocks::MockPalletStorage, AsyncResult, KeyRotationStatusOuter, KeyRotator};
use cf_primitives::EpochIndex;
use sp_std::collections::btree_set::BTreeSet;

use super::MockPallet;

const ROTATION_OUTCOME: &[u8] = b"ROTATION_OUTCOME";

macro_rules! mock_key_rotator {
	($rotator_name:ident) => {
		pub struct $rotator_name;

		impl MockPallet for $rotator_name {
			const PREFIX: &'static [u8] = stringify!($rotator_name).as_bytes();
		}

		impl $rotator_name {
			pub fn keygen_success() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::KeygenComplete,
					),
				);
			}

			pub fn key_handover_success() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::KeyHandoverComplete,
					),
				);
			}

			pub fn keys_activated() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::RotationComplete,
					),
				);
			}

			pub fn failed<O: IntoIterator<Item = u64>>(offenders: O) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Ready(
						KeyRotationStatusOuter::Failed(offenders.into_iter().collect()),
					),
				);
			}

			pub fn pending() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				)
			}
		}

		impl KeyRotator for $rotator_name {
			type ValidatorId = u64;

			fn keygen(_candidates: BTreeSet<Self::ValidatorId>, _new_epoch_index: EpochIndex) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				);
			}

			fn key_handover(
				_old_participants: BTreeSet<Self::ValidatorId>,
				_new_candidates: BTreeSet<Self::ValidatorId>,
				_epoch_index: EpochIndex,
			) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				);
			}

			fn status() -> AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>> {
				Self::get_value(ROTATION_OUTCOME).unwrap_or_default()
			}

			fn activate_vaults() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<KeyRotationStatusOuter<u64>>::Pending,
				);
			}

			fn reset_key_rotation() {
				Self::put_value(ROTATION_OUTCOME, AsyncResult::<KeyRotationStatusOuter<u64>>::Void);
			}

			#[cfg(feature = "runtime-benchmarks")]
			fn set_status(_outcome: AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>>) {
				unimplemented!()
			}
		}
	};
}

mock_key_rotator!(MockKeyRotatorA);
mock_key_rotator!(MockKeyRotatorB);
mock_key_rotator!(MockKeyRotatorC);
mock_key_rotator!(MockKeyRotatorD);
