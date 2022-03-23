use super::*;
pub type ReputationPoints = i32;

/// Conditions that cause a validator to be docked reputation points
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum Offence {
	/// There was a failure in participation during a signing
	ParticipateSigningFailed,
	/// There was a failure in participation during a key generation ceremony
	ParticipateKeygenFailed,
	/// An invalid transaction was authored
	InvalidTransactionAuthored,
	/// A transaction failed on transmission
	TransactionFailedOnTransmission,
	/// A validator missed their authorship slot.
	MissedAuthorshipSlot,
}

pub trait OffencePenalty {
	fn penalty(condition: &Offence) -> (ReputationPoints, bool);
}

/// For reporting offences.
pub trait OffenceReporter {
	type ValidatorId;
	type Penalty: OffencePenalty;

	/// Report the condition for validator
	/// Returns `Ok(Weight)` else an error if the validator isn't valid
	fn report(condition: Offence, validator_id: &Self::ValidatorId);
}

/// We report on nodes that should be banned
pub trait Banned {
	type ValidatorId;
	/// A validator to be banned
	fn ban(validator_id: &Self::ValidatorId);
}
