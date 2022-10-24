pub mod broadcast;
mod broadcast_verification;
mod ceremony_stage;
mod failure_reason;

pub use ceremony_stage::{
	CeremonyCommon, CeremonyStage, PreProcessStageDataCheck, ProcessMessageResult, StageResult,
};

pub use broadcast_verification::BroadcastVerificationMessage;

pub use failure_reason::{
	BroadcastFailureReason, CeremonyFailureReason, KeygenFailureReason, SigningFailureReason,
};

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use thiserror::Error;

use crate::multisig::crypto::{ECPoint, KeyShare};

use super::{utils::PartyIdxMapping, ThresholdParameters};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResult<P: ECPoint> {
	#[serde(bound = "")]
	pub key_share: KeyShare<P>,
	#[serde(bound = "")]
	pub party_public_keys: Vec<P>,
}

impl<P: ECPoint> KeygenResult<P> {
	pub fn get_public_key(&self) -> P {
		self.key_share.y
	}

	/// Gets the serialized compressed public key (33 bytes - 32 bytes + a y parity byte)
	pub fn get_public_key_bytes(&self) -> Vec<u8> {
		self.key_share.y.as_bytes().as_ref().into()
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KeygenResultInfo<P: ECPoint> {
	#[serde(bound = "")]
	pub key: Arc<KeygenResult<P>>,
	pub validator_mapping: Arc<PartyIdxMapping>,
	pub params: ThresholdParameters,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum KeygenStageName {
	#[error("Hash Commitments")]
	HashCommitments1,
	#[error("Verify Hash Commitments")]
	VerifyHashCommitmentsBroadcast2,
	#[error("Coefficient Commitments")]
	CoefficientCommitments3,
	#[error("Verify Coefficient Commitments")]
	VerifyCommitmentsBroadcast4,
	#[error("Secret Shares")]
	SecretSharesStage5,
	#[error("Complaints")]
	ComplaintsStage6,
	#[error("Verify Complaints")]
	VerifyComplaintsBroadcastStage7,
	#[error("Blame Responses")]
	BlameResponsesStage8,
	#[error("Verify Blame Responses")]
	VerifyBlameResponsesBroadcastStage9,
}

#[derive(Error, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum SigningStageName {
	#[error("Commitments")]
	AwaitCommitments1,
	#[error("Verify Commitments")]
	VerifyCommitmentsBroadcast2,
	#[error("Local Signatures")]
	LocalSigStage3,
	#[error("Verify Local Signatures")]
	VerifyLocalSigsBroadcastStage4,
}
