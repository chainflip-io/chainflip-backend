use crate::{mocks::MockPalletStorage, AsyncResult, VaultRotator, VaultStatus};
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
					AsyncResult::<VaultStatus<u64>>::Ready(VaultStatus::KeygenComplete),
				);
			}

			pub fn keys_activated() {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultStatus<u64>>::Ready(VaultStatus::RotationComplete),
				);
			}

			pub fn failed<O: IntoIterator<Item = u64>>(offenders: O) {
				Self::put_value(
					ROTATION_OUTCOME,
					AsyncResult::<VaultStatus<u64>>::Ready(VaultStatus::Failed(
						offenders.into_iter().collect(),
					)),
				);
			}

			pub fn pending() {
				Self::put_value(ROTATION_OUTCOME, AsyncResult::<VaultStatus<u64>>::Pending)
			}
		}

		impl VaultRotator for $rotator_name {
			type ValidatorId = u64;

			fn keygen(_candidates: BTreeSet<Self::ValidatorId>, _epoch_index: EpochIndex) {
				Self::put_value(ROTATION_OUTCOME, AsyncResult::<VaultStatus<u64>>::Pending);
			}

			fn status() -> AsyncResult<VaultStatus<Self::ValidatorId>> {
				Self::get_value(ROTATION_OUTCOME).unwrap_or_default()
			}

			fn activate() {
				Self::put_value(ROTATION_OUTCOME, AsyncResult::<VaultStatus<u64>>::Pending);
			}

			#[cfg(feature = "runtime-benchmarks")]
			fn set_status(_outcome: AsyncResult<VaultStatus<Self::ValidatorId>>) {
				unimplemented!()
			}
		}
	};
}

mock_vault_rotator!(MockVaultRotatorA);
mock_vault_rotator!(MockVaultRotatorB);
mock_vault_rotator!(MockVaultRotatorC);
