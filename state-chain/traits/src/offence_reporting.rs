/// For reporting offences.
pub trait OffenceReporter {
	type ValidatorId;
	type Offence;

	/// Report a node.
	fn report(offence: impl Into<Self::Offence>, node: Self::ValidatorId) {
		Self::report_many(offence, &[node]);
	}

	/// Report multiple nodes
	fn report_many(offence: impl Into<Self::Offence>, nodes: &[Self::ValidatorId]);

	/// Forgive all nodes
	fn forgive_all(offence: impl Into<Self::Offence>);
}
