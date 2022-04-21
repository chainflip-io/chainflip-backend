/// For reporting offences.
pub trait OffenceReporter {
	type ValidatorId;
	type Offence;

	/// Report a validator.
	fn report(offence: impl Into<Self::Offence>, validator_id: Self::ValidatorId) {
		Self::report_many(offence, &[validator_id]);
	}

	/// Report multiple validators.
	fn report_many(offence: impl Into<Self::Offence>, validators: &[Self::ValidatorId]);

	/// Forgive all validators for an offence.
	fn forgive_all(offence: impl Into<Self::Offence>);
}
