/// For reporting offences.
pub trait OffenceReporter {
	type ValidatorId;
	type Offence;

	/// Report an authority.
	fn report(offence: impl Into<Self::Offence>, validator_id: Self::ValidatorId) {
		Self::report_many(offence, &[validator_id]);
	}

	/// Report multiple authorities.
	fn report_many(offence: impl Into<Self::Offence>, authorities: &[Self::ValidatorId]);

	/// Forgive all authorities for an offence.
	fn forgive_all(offence: impl Into<Self::Offence>);
}
