use super::{MockPallet, MockPalletStorage};
use crate::{AsyncResult, RotationError, VaultRotator};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct MockVaultRotator;

impl MockPallet for MockVaultRotator {
	const PREFIX: &'static [u8] = b"MockVaultRotator::";
}

const BEHAVIOUR: &[u8] = b"BEHAVIOUR";
const ROTATION_OUTCOME: &[u8] = b"ROTATION_OUTCOME";
const ERROR_ON_START: &[u8] = b"ERROR_ON_START";

type MockVaultOutcome = Result<(), Vec<u64>>;

impl MockVaultRotator {
	pub fn set_error_on_start(e: bool) {
		Self::put_storage(ERROR_ON_START, b"", e);
	}

	pub fn succeeding() {
		Self::put_storage(BEHAVIOUR, b"", MockVaultOutcome::Ok(()));
	}

	pub fn failing(offenders: Vec<u64>) {
		Self::put_storage(BEHAVIOUR, b"", MockVaultOutcome::Err(offenders));
	}

	fn get_error_on_start() -> bool {
		Self::get_storage(ERROR_ON_START, b"").unwrap_or(false)
	}

	/// Call this to simulate the on_initialise pallet hook.
	pub fn on_initialise() {
		// default to success
		let outcome = Self::get_storage(BEHAVIOUR, b"").unwrap_or(Ok(()));
		Self::put_storage(
			ROTATION_OUTCOME,
			b"",
			match Self::get_vault_rotation_outcome() {
				AsyncResult::Pending => AsyncResult::Ready(outcome),
				other => other,
			},
		)
	}
}

impl VaultRotator for MockVaultRotator {
	type ValidatorId = u64;

	fn start_vault_rotation(_candidates: Vec<Self::ValidatorId>) -> Result<(), RotationError> {
		if Self::get_error_on_start() {
			return Err(RotationError::RotationInProgress)
		}

		Self::put_storage(ROTATION_OUTCOME, b"", AsyncResult::<MockVaultOutcome>::Pending);
		Ok(())
	}

	fn get_vault_rotation_outcome() -> AsyncResult<MockVaultOutcome> {
		Self::get_storage(ROTATION_OUTCOME, b"").unwrap_or_default()
	}
}

#[test]
fn test_mock() {
	sp_io::TestExternalities::new_empty().execute_with(|| {
		<MockVaultRotator as VaultRotator>::start_vault_rotation(vec![]).unwrap();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::<MockVaultOutcome>::Pending
		);
		MockVaultRotator::succeeding();
		MockVaultRotator::on_initialise();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(MockVaultOutcome::Ok(()))
		);
		<MockVaultRotator as VaultRotator>::start_vault_rotation(vec![]).unwrap();
		MockVaultRotator::failing(vec![42]);
		MockVaultRotator::on_initialise();
		assert_eq!(
			<MockVaultRotator as VaultRotator>::get_vault_rotation_outcome(),
			AsyncResult::Ready(MockVaultOutcome::Err(vec![42]))
		);
		MockVaultRotator::set_error_on_start(true);
		<MockVaultRotator as VaultRotator>::start_vault_rotation(vec![]).expect_err("should error");
	})
}
