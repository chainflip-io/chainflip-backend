#[macro_export]
macro_rules! impl_mock_offence_reporting {
	($account_id:ty) => {
		thread_local! {
			pub static REPORTED: std::cell::RefCell<Vec<$account_id>> = Default::default();
		}

		pub struct MockOffenceReporter;

		impl MockOffenceReporter {
			pub fn get_reported() -> Vec<$account_id> {
				REPORTED.with(|cell| cell.borrow().clone())
			}
		}

		pub struct MockOffencePenalty;
		impl $crate::offence_reporting::OffencePenalty for MockOffencePenalty {
			fn penalty(
				condition: &$crate::offence_reporting::Offence,
			) -> ($crate::offence_reporting::ReputationPoints, bool) {
				match condition {
					$crate::offence_reporting::Offence::ParticipateSigningFailed => (15, true),
					$crate::offence_reporting::Offence::ParticipateKeygenFailed => (15, true),
					$crate::offence_reporting::Offence::InvalidTransactionAuthored => (15, false),
					$crate::offence_reporting::Offence::TransactionFailedOnTransmission =>
						(15, false),
					$crate::offence_reporting::Offence::MissedAuthorshipSlot => (15, true),
				}
			}
		}

		impl $crate::offence_reporting::OffenceReporter for MockOffenceReporter {
			type ValidatorId = $account_id;
			type Penalty = MockOffencePenalty;

			fn report(
				_condition: $crate::offence_reporting::Offence,
				validator_id: &Self::ValidatorId,
			) {
				REPORTED.with(|cell| cell.borrow_mut().push(validator_id.clone()));
			}
		}
	};
}
