use crate::{mocks::MockPalletStorage, AsyncResult, VaultRotator, VaultStatus};

use sp_std::collections::btree_set::BTreeSet;

use super::MockPallet;

const ROTATION_OUTCOME: &[u8] = b"ROTATION_OUTCOME";

pub struct MockVaultRotator;

impl MockPallet for MockVaultRotator {
	const PREFIX: &'static [u8] = b"MockVaultRotator::";
}

impl MockVaultRotator {
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
}

impl VaultRotator for MockVaultRotator {
	type ValidatorId = u64;

	fn start(_candidates: BTreeSet<Self::ValidatorId>) {
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
