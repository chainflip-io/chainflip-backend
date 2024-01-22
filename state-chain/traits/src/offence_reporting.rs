/// For reporting offences.
pub trait OffenceReporter {
	type ValidatorId: Clone;
	type Offence;

	/// Report a node.
	fn report(offence: impl Into<Self::Offence>, node: Self::ValidatorId) {
		Self::report_many(offence, [node]);
	}

	/// Report multiple nodes
	fn report_many(
		offence: impl Into<Self::Offence>,
		validators: impl IntoIterator<Item = Self::ValidatorId> + Clone,
	);

	/// Forgive all nodes
	fn forgive_all(offence: impl Into<Self::Offence>);
}
