use super::{MockPallet, MockPalletStorage};
use crate::{AsyncResult, SuccessOrFailure, VaultRotator};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MockVaultRotator;

impl MockPallet for MockVaultRotator {
	const PREFIX: &'static [u8] = b"MockVaultRotator::";
}

const BEHAVIOUR: &[u8] = b"BEHAVIOUR";
const ROTATION_OUTCOME: &[u8] = b"ROTATION_OUTCOME";
const ERROR_ON_START: &[u8] = b"ERROR_ON_START";

impl MockVaultRotator {
	pub fn set_error_on_start(e: bool) {
		Self::put_storage(ERROR_ON_START, b"", e);
	}

	pub fn succeeding() {
		Self::put_storage(BEHAVIOUR, b"", SuccessOrFailure::Success);
	}

	pub fn failing() {
		Self::put_storage(BEHAVIOUR, b"", SuccessOrFailure::Failure);
	}

	fn get_error_on_start() -> bool {
		Self::get_storage(ERROR_ON_START, b"").unwrap_or(false)
	}

	fn initialise() {
		Self::put_storage(ROTATION_OUTCOME, b"", AsyncResult::<SuccessOrFailure>::Pending);
	}

	fn get_vault_rotation_outcome() -> AsyncResult<SuccessOrFailure> {
		Self::get_storage(ROTATION_OUTCOME, b"").unwrap_or_default()
	}

	/// Call this to simulate the on_initialise pallet hook.
	pub fn on_initialise() {
		// default to success
		let s = Self::get_storage(BEHAVIOUR, b"").unwrap_or(SuccessOrFailure::Success);
		Self::put_storage(
			ROTATION_OUTCOME,
			b"",
			match Self::get_vault_rotation_outcome() {
				AsyncResult::Pending => AsyncResult::Ready(s),
				other => other,
			},
		)
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
		MockVaultRotator::succeeding();
		MockVaultRotator::on_initialise();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(SuccessOrFailure::Success)
		);
		<MockVaultRotator as VaultRotator>::start_vault_rotation(vec![]).unwrap();
		MockVaultRotator::failing();
		MockVaultRotator::on_initialise();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(SuccessOrFailure::Failure)
		);
		MockVaultRotator::set_error_on_start(true);
		<MockVaultRotator as VaultRotator>::start_vault_rotation(vec![]).expect_err("should error");
	})
}
