use codec::{Decode, Encode};
use frame_support::RuntimeDebug;

/// Offences that can be reported in this runtime.
#[derive(Clone, Copy, PartialEq, Eq, RuntimeDebug, Encode, Decode)]
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
	/// A validator has missed a heartbeat submission.
	MissedHeartbeat,
}
