use state_chain_runtime::AccountId;
use tracing::warn;

use std::collections::BTreeSet;

use crate::{
	common::format_iterator,
	logging::{
		KEYGEN_CEREMONY_FAILED, KEYGEN_REQUEST_IGNORED, REQUEST_TO_SIGN_IGNORED,
		SIGNING_CEREMONY_FAILED, UNAUTHORIZED_KEYGEN_ABORTED, UNAUTHORIZED_SIGNING_ABORTED,
	},
};

use thiserror::Error;

use super::{KeygenStageName, SigningStageName};

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SigningFailureReason {
	#[error("Not participating in unauthorised ceremony")]
	NotParticipatingInUnauthorisedCeremony,
	#[error("Invalid Participants")]
	InvalidParticipants,
	#[error("Broadcast Failure ({0}) during {1} stage")]
	BroadcastFailure(BroadcastFailureReason, SigningStageName),
	#[error("Invalid Sig Share")]
	InvalidSigShare,
	#[error("Not Enough Signers")]
	NotEnoughSigners,
	#[error("Unknown Key")]
	UnknownKey,
	#[error("Invalid Number of Payloads")]
	InvalidNumberOfPayloads,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeygenFailureReason {
	#[error("Not participating in unauthorised ceremony")]
	NotParticipatingInUnauthorisedCeremony,
	#[error("Invalid Participants")]
	InvalidParticipants,
	#[error("Broadcast Failure ({0}) during {1} stage")]
	BroadcastFailure(BroadcastFailureReason, KeygenStageName),
	#[error("Invalid Commitment")]
	InvalidCommitment,
	#[error("Invalid secret share in a blame response")]
	InvalidBlameResponse,
	#[error("Invalid Complaint")]
	InvalidComplaint,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BroadcastFailureReason {
	/// Enough missing messages from broadcast + verification to stop consensus
	#[error("Insufficient Messages")]
	InsufficientMessages,
	/// Not enough broadcast verification messages received to continue verification
	#[error("Insufficient Verification Messages")]
	InsufficientVerificationMessages,
	/// Consensus could not be reached for one or more parties due to differing values
	#[error("Inconsistency")]
	Inconsistency,
}

const SIGNING_CEREMONY_FAILED_PREFIX: &str = "Signing ceremony failed";
const KEYGEN_CEREMONY_FAILED_PREFIX: &str = "Keygen ceremony failed";
const REQUEST_TO_SIGN_IGNORED_PREFIX: &str = "Signing request ignored";
const KEYGEN_REQUEST_IGNORED_PREFIX: &str = "Keygen request ignored";

pub trait CeremonyFailureReason {
	fn log(&self, reported_parties: &BTreeSet<AccountId>);
}

impl CeremonyFailureReason for SigningFailureReason {
	fn log(&self, reported_parties: &BTreeSet<AccountId>) {
		let reported_parties = format_iterator(reported_parties).to_string();
		match self {
			SigningFailureReason::BroadcastFailure(_, _) |
			SigningFailureReason::InvalidSigShare => {
				warn!(
					tag = SIGNING_CEREMONY_FAILED,
					reported_parties = reported_parties,
					"{SIGNING_CEREMONY_FAILED_PREFIX}: {self}",
				);
			},
			SigningFailureReason::NotParticipatingInUnauthorisedCeremony => {
				warn!(
					tag = UNAUTHORIZED_SIGNING_ABORTED,
					"{SIGNING_CEREMONY_FAILED_PREFIX}: {self}",
				);
			},
			SigningFailureReason::InvalidParticipants |
			SigningFailureReason::NotEnoughSigners |
			SigningFailureReason::UnknownKey => {
				warn!(tag = REQUEST_TO_SIGN_IGNORED, "{REQUEST_TO_SIGN_IGNORED_PREFIX}: {self}",);
			},
			SigningFailureReason::InvalidNumberOfPayloads => {
				warn!(reported_parties = reported_parties, "{self}");
			},
		}
	}
}

impl CeremonyFailureReason for KeygenFailureReason {
	fn log(&self, reported_parties: &BTreeSet<AccountId>) {
		let reported_parties = format_iterator(reported_parties).to_string();
		match self {
			KeygenFailureReason::BroadcastFailure(_, _) |
			KeygenFailureReason::InvalidBlameResponse |
			KeygenFailureReason::InvalidCommitment |
			KeygenFailureReason::InvalidComplaint => {
				warn!(
					tag = KEYGEN_CEREMONY_FAILED,
					reported_parties = reported_parties,
					"{KEYGEN_CEREMONY_FAILED_PREFIX}: {self}",
				);
			},
			KeygenFailureReason::NotParticipatingInUnauthorisedCeremony => {
				warn!(tag = UNAUTHORIZED_KEYGEN_ABORTED, "{KEYGEN_CEREMONY_FAILED_PREFIX}: {self}",);
			},
			KeygenFailureReason::InvalidParticipants => {
				warn!(tag = KEYGEN_REQUEST_IGNORED, "{KEYGEN_REQUEST_IGNORED_PREFIX}: {self}",);
			},
		}
	}
}
