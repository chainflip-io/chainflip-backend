use super::{MockPallet, MockPalletStorage};
use crate::{AsyncResult, SuccessOrFailure, VaultRotator};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MockVaultRotator;

impl MockPallet for MockVaultRotator {
	const PREFIX: &'static [u8] = b"MockVaultRotator::";
}

impl MockVaultRotator {
	pub fn set_error_on_start(e: bool) {
		Self::put_storage(b"ERROR_ON_START", b"", e);
	}

	pub fn succeed() {
		Self::set_vault_rotation_outcome(SuccessOrFailure::Success);
	}

	pub fn fail() {
		Self::set_vault_rotation_outcome(SuccessOrFailure::Failure);
	}

	fn get_error_on_start() -> bool {
		Self::get_storage(b"ERROR_ON_START", b"").unwrap_or(false)
	}

	fn initialise() {
		Self::put_storage(b"ROTATION_OUTCOME", b"", AsyncResult::<SuccessOrFailure>::Pending);
	}

	fn set_vault_rotation_outcome(o: SuccessOrFailure) {
		Self::put_storage(b"ROTATION_OUTCOME", b"", AsyncResult::Ready(o));
	}

	fn get_vault_rotation_outcome() -> AsyncResult<SuccessOrFailure> {
		Self::get_storage(b"ROTATION_OUTCOME", b"").unwrap_or_default()
	}
}

impl VaultRotator for MockVaultRotator {
	type ValidatorId = u64;
	type RotationError = &'static str;

	fn start_vault_rotation(
		_candidates: Vec<Self::ValidatorId>,
	) -> Result<(), Self::RotationError> {
		if Self::get_error_on_start() {
			return Err("failure")
		}

		Self::initialise();
		Ok(())
	}

	fn get_vault_rotation_outcome() -> AsyncResult<SuccessOrFailure> {
		Self::get_vault_rotation_outcome()
	}
}

#[test]
fn test_mock() {
	sp_io::TestExternalities::new_empty().execute_with(|| {
		<MockVaultRotator as VaultRotator>::start_vault_rotation(vec![]).unwrap();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::<SuccessOrFailure>::Pending
		);
		MockVaultRotator::succeed();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(SuccessOrFailure::Success)
		);
		MockVaultRotator::fail();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(SuccessOrFailure::Failure)
		);
		MockVaultRotator::set_error_on_start(true);
		<MockVaultRotator as VaultRotator>::start_vault_rotation(vec![]).expect_err("should error");
	})
}
