use crate::{mocks::MockPalletStorage, AsyncResult, MultiVaultRotator, VaultStatus};

use sp_std::collections::btree_set::BTreeSet;

use super::MockPallet;

const ROTATION_OUTCOME: &[u8] = b"ROTATION_OUTCOME";

pub struct MockMultiVaultRotator;

impl MockPallet for MockMultiVaultRotator {
	const PREFIX: &'static [u8] = b"MockMultiVaultRotator::";
}

impl MockMultiVaultRotator {
	pub fn keygen_success() {
		Self::put_value(
			ROTATION_OUTCOME,
			AsyncResult::<VaultStatus<u64>>::Ready(VaultStatus::KeygenVerificationComplete),
		);
	}

	pub fn rotated_externally() {
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
}

impl MultiVaultRotator for MockMultiVaultRotator {
	type ValidatorId = u64;

	fn start_all_vault_rotations(_candidates: BTreeSet<Self::ValidatorId>) {
		Self::put_value(ROTATION_OUTCOME, AsyncResult::<VaultStatus<u64>>::Pending);
	}

	fn multi_vault_rotation_outcome() -> AsyncResult<VaultStatus<Self::ValidatorId>> {
		Self::get_value(ROTATION_OUTCOME).unwrap_or_default()
	}

	fn rotate_all_externally() {
		Self::put_value(ROTATION_OUTCOME, AsyncResult::<VaultStatus<u64>>::Pending);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_all_vault_rotation_outcomes(_outcome: AsyncResult<VaultStatus<Self::ValidatorId>>) {
		unimplemented!()
	}
}
