use crate::Runtime;
use cf_chains::{ForeignChain, Get};
use cf_traits::{offence_reporting::OffenceReporter, Chainflip};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::RuntimeDebug;
use pallet_cf_elections::electoral_systems::liveness::OnCheckComplete;
use pallet_cf_reputation::OffenceList;
use pallet_grandpa::EquivocationOffence;
use scale_info::TypeInfo;
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData};

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
	/// A authority failed to Witness a call in time.
	FailedToWitnessInTime,
	/// Failed to Complete Liveness Check for chain.
	FailedLivenessCheck(ForeignChain),
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
			pallet_cf_threshold_signature::PalletOffence::FailedKeygen =>
				Self::ParticipateKeygenFailed,
			pallet_cf_threshold_signature::PalletOffence::FailedKeyHandover =>
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

impl From<pallet_cf_witnesser::PalletOffence> for Offence {
	fn from(offences: pallet_cf_witnesser::PalletOffence) -> Self {
		match offences {
			pallet_cf_witnesser::PalletOffence::FailedToWitnessInTime =>
				Self::FailedToWitnessInTime,
		}
	}
}

impl<T> From<EquivocationOffence<T>> for Offence {
	fn from(_: EquivocationOffence<T>) -> Self {
		Self::GrandpaEquivocation
	}
}

pub struct ReportFailedLivenessCheck<C> {
	phantom_data: PhantomData<C>,
}
impl<C: Get<ForeignChain>> OnCheckComplete<<Runtime as Chainflip>::ValidatorId>
	for ReportFailedLivenessCheck<C>
{
	fn on_check_complete(validator_ids: BTreeSet<<Runtime as Chainflip>::ValidatorId>) {
		<crate::Reputation as OffenceReporter>::report_many(
			Offence::FailedLivenessCheck(C::get()),
			validator_ids,
		);
	}
}
