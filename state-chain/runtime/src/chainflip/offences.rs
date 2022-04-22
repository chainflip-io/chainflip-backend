use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use pallet_cf_reputation::{GetValidatorsExcludedFor, OffenceList};

use crate::Runtime;

/// Offences that can be reported in this runtime.
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
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
	/// An authority missed their authorship slot.
	MissedAuthorshipSlot,
	/// An authority has missed a heartbeat submission.
	MissedHeartbeat,
}

pub type ExclusionSetFor<L> = GetValidatorsExcludedFor<Runtime, L>;

pub struct KeygenOffences;

impl OffenceList<Runtime> for KeygenOffences {
	const OFFENCES: &'static [Offence] = &[Offence::ParticipateKeygenFailed];
}

pub struct SigningOffences;

impl OffenceList<Runtime> for SigningOffences {
	const OFFENCES: &'static [Offence] = &[
		Offence::ParticipateSigningFailed,
		Offence::MissedAuthorshipSlot,
		Offence::MissedHeartbeat,
	];
}

// Boilerplate
impl From<pallet_cf_broadcast::PalletOffence> for Offence {
	fn from(offences: pallet_cf_broadcast::PalletOffence) -> Self {
		match offences {
			pallet_cf_broadcast::PalletOffence::InvalidTransactionAuthored =>
				Self::InvalidTransactionAuthored,
			pallet_cf_broadcast::PalletOffence::TransactionFailedOnTransmission =>
				Self::TransactionFailedOnTransmission,
		}
	}
}

impl From<pallet_cf_reputation::PalletOffence> for Offence {
	fn from(offences: pallet_cf_reputation::PalletOffence) -> Self {
		match offences {
			pallet_cf_reputation::PalletOffence::MissedHeartbeat => Self::MissedHeartbeat,
		}
	}
}

impl From<pallet_cf_threshold_signature::PalletOffence> for Offence {
	fn from(offences: pallet_cf_threshold_signature::PalletOffence) -> Self {
		match offences {
			pallet_cf_threshold_signature::PalletOffence::ParticipateSigningFailed =>
				Self::ParticipateSigningFailed,
		}
	}
}

impl From<pallet_cf_vaults::PalletOffence> for Offence {
	fn from(offences: pallet_cf_vaults::PalletOffence) -> Self {
		match offences {
			pallet_cf_vaults::PalletOffence::ParticipateKeygenFailed =>
				Self::ParticipateKeygenFailed,
			pallet_cf_vaults::PalletOffence::SigningOffence => Self::ParticipateSigningFailed,
		}
	}
}

impl From<pallet_cf_validator::PalletOffence> for Offence {
	fn from(offences: pallet_cf_validator::PalletOffence) -> Self {
		match offences {
			pallet_cf_validator::PalletOffence::MissedAuthorshipSlot => Self::MissedAuthorshipSlot,
		}
	}
}
