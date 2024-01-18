use crate::Runtime;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::RuntimeDebug;
use pallet_cf_reputation::OffenceList;
use pallet_grandpa::EquivocationOffence;
use scale_info::TypeInfo;

/// Offences that can be reported in this runtime.
#[derive(
	serde::Serialize,
	serde::Deserialize,
	Clone,
	Copy,
	PartialEq,
	Eq,
	RuntimeDebug,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
)]
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
	/// A node failed to participate in key handover.
	ParticipateKeyHandoverFailed,
}

/// Nodes should be excluded from keygen if they have been reported for any of the offences in this
/// struct's implementation of [OffenceList].
pub struct KeygenExclusionOffences;

impl OffenceList<Runtime> for KeygenExclusionOffences {
	const OFFENCES: &'static [Offence] =
		&[Offence::MissedAuthorshipSlot, Offence::GrandpaEquivocation];
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
			pallet_cf_vaults::PalletOffence::FailedKeygen => Self::ParticipateKeygenFailed,
			pallet_cf_vaults::PalletOffence::FailedKeyHandover =>
				Self::ParticipateKeyHandoverFailed,
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

impl<T> From<EquivocationOffence<T>> for Offence {
	fn from(_: EquivocationOffence<T>) -> Self {
		Self::GrandpaEquivocation
	}
}
