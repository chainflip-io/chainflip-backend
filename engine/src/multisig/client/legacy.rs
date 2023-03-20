//! This module contains data structs for multisig messages
//! version 1 (which did not support multiple payloads) as
//! well as conversion functions to the current version.
//! This can be removed when all nodes upgrade to version 2.

use std::collections::BTreeMap;

use super::{
	common::BroadcastVerificationMessage,
	signing::{Comm1, LocalSig3, SigningCommitment},
	*,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MultisigMessageV1<P: ECPoint> {
	pub ceremony_id: CeremonyId,
	#[serde(bound = "")]
	pub data: MultisigDataV1<P>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MultisigDataV1<P: ECPoint> {
	#[serde(bound = "")]
	Keygen(KeygenData<P>),
	#[serde(bound = "")]
	Signing(SigningDataV1<P>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SigningDataV1<P: ECPoint> {
	#[serde(bound = "")]
	CommStage1(Comm1V1<P>),
	#[serde(bound = "")]
	BroadcastVerificationStage2(VerifyComm2V1<P>),
	#[serde(bound = "")]
	LocalSigStage3(LocalSig3V1<P>),
	#[serde(bound = "")]
	VerifyLocalSigsStage4(VerifyLocalSig4V1<P>),
}

pub type Comm1V1<P> = SigningCommitment<P>;

pub type VerifyComm2V1<P> = BroadcastVerificationMessage<Comm1V1<P>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalSig3V1<P: ECPoint> {
	pub response: P::Scalar,
}

pub type VerifyLocalSig4V1<P> = BroadcastVerificationMessage<LocalSig3V1<P>>;

impl<P: ECPoint> From<MultisigDataV1<P>> for MultisigData<P> {
	fn from(data: MultisigDataV1<P>) -> Self {
		match data {
			MultisigDataV1::Keygen(data) => MultisigData::Keygen(data),
			MultisigDataV1::Signing(data) => MultisigData::Signing(data.into()),
		}
	}
}

impl<P: ECPoint> From<SigningDataV1<P>> for SigningData<P> {
	fn from(value: SigningDataV1<P>) -> Self {
		match value {
			SigningDataV1::CommStage1(x) => SigningData::CommStage1(x.into()),
			SigningDataV1::BroadcastVerificationStage2(x) => {
				let data: BTreeMap<_, _> = x
					.data
					.into_iter()
					.map(|(party_idx, x)| (party_idx, x.map(|x| x.into())))
					.collect();
				SigningData::BroadcastVerificationStage2(BroadcastVerificationMessage { data })
			},
			SigningDataV1::LocalSigStage3(x) => SigningData::LocalSigStage3(x.into()),
			SigningDataV1::VerifyLocalSigsStage4(x) => {
				let data: BTreeMap<_, _> = x
					.data
					.into_iter()
					.map(|(party_idx, x)| (party_idx, x.map(|x| x.into())))
					.collect();
				SigningData::VerifyLocalSigsStage4(BroadcastVerificationMessage { data })
			},
		}
	}
}

impl<P: ECPoint> From<Comm1V1<P>> for Comm1<P> {
	fn from(value: Comm1V1<P>) -> Self {
		Comm1(vec![value])
	}
}

impl<P: ECPoint> From<LocalSig3V1<P>> for LocalSig3<P> {
	fn from(value: LocalSig3V1<P>) -> Self {
		LocalSig3 { responses: vec![value.response] }
	}
}
