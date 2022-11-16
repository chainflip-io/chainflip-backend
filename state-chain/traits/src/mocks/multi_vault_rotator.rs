use crate::{AsyncResult, MultiVaultRotator, VaultStatus};

use sp_std::collections::btree_set::BTreeSet;

use super::MockPallet;

pub struct MockMultiVaultRotator;

impl MockPallet for MockMultiVaultRotator {
	const PREFIX: &'static [u8] = b"MockMultiVaultRotator::";
}

impl MultiVaultRotator for MockMultiVaultRotator {
	type ValidatorId = u64;

	fn start_all_vault_rotations(_candidates: BTreeSet<Self::ValidatorId>) {
		todo!()
	}

	fn multi_vault_rotation_outcome() -> AsyncResult<VaultStatus<Self::ValidatorId>> {
		todo!()
	}

	fn rotate_all_externally() {
		todo!()
	}
}
