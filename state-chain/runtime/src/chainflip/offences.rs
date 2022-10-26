use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::RuntimeDebug;
use pallet_cf_reputation::{GetValidatorsExcludedFor, OffenceList};
use pallet_grandpa::GrandpaEquivocationOffence;
use scale_info::TypeInfo;

use crate::Runtime;

/// Offences that can be reported in this runtime.
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, PartialEq, Eq, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum Offence {
	/// There was a failure in participation during a signing.
	ParticipateSigningFailed,
	/// There was a failure in participation during a key generation ceremony.
	ParticipateKeygenFailed,
	/// An authority did not broadcast a transaction.
	FailedToBroadcastTransaction,
	/// An authority missed their authorship slot.
	MissedAuthorshipSlot,
	/// A node has missed a heartbeat submission.
	MissedHeartbeat,
	/// Grandpa equivocation detected.
	GrandpaEquivocation,
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
			pallet_cf_broadcast::PalletOffence::FailedToBroadcastTransaction =>
				Self::FailedToBroadcastTransaction,
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
			// Failing keygen should carry the same consequences as failing a signing ceremony.
			pallet_cf_vaults::PalletOffence::FailedKeygen => Self::ParticipateSigningFailed,
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

impl<T> From<GrandpaEquivocationOffence<T>> for Offence {
	fn from(_: GrandpaEquivocationOffence<T>) -> Self {
		Self::GrandpaEquivocation
	}
}
