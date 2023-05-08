//! This module contains data structs for multisig messages
//! version 1 (which did not support multiple payloads) as
//! well as conversion functions to the current version.
//! This can be removed when all nodes upgrade to version 2.

use std::collections::BTreeMap;

use anyhow::bail;

use super::{
	common::BroadcastVerificationMessage,
	signing::{Comm1, LocalSig3, SigningCommitment, VerifyComm2, VerifyLocalSig4},
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

impl<P: ECPoint> TryFrom<MultisigMessage<P>> for MultisigMessageV1<P> {
	type Error = anyhow::Error;

	fn try_from(value: MultisigMessage<P>) -> Result<Self, Self::Error> {
		Ok(MultisigMessageV1 { ceremony_id: value.ceremony_id, data: value.data.try_into()? })
	}
}

impl<P: ECPoint> TryFrom<MultisigData<P>> for MultisigDataV1<P> {
	type Error = anyhow::Error;

	fn try_from(value: MultisigData<P>) -> Result<Self, Self::Error> {
		Ok(match value {
			MultisigData::Keygen(data) => MultisigDataV1::Keygen(data),
			MultisigData::Signing(data) => MultisigDataV1::Signing(data.try_into()?),
		})
	}
}

impl<P: ECPoint> TryFrom<SigningData<P>> for SigningDataV1<P> {
	type Error = anyhow::Error;

	fn try_from(value: SigningData<P>) -> Result<Self, Self::Error> {
		match value {
			SigningData::CommStage1(x) => Ok(SigningDataV1::CommStage1(x.try_into()?)),
			SigningData::BroadcastVerificationStage2(x) =>
				Ok(SigningDataV1::BroadcastVerificationStage2(x.try_into()?)),
			SigningData::LocalSigStage3(x) => Ok(SigningDataV1::LocalSigStage3(x.try_into()?)),
			SigningData::VerifyLocalSigsStage4(x) =>
				Ok(SigningDataV1::VerifyLocalSigsStage4(x.try_into()?)),
		}
	}
}

impl<P: ECPoint> TryFrom<Comm1<P>> for Comm1V1<P> {
	type Error = anyhow::Error;

	fn try_from(value: Comm1<P>) -> Result<Self, Self::Error> {
		if value.0.len() != 1 {
			bail!("Comm1 must have exactly one element");
		}
		Ok(value.0[0].clone())
	}
}

impl<P: ECPoint> TryFrom<VerifyComm2<P>> for VerifyComm2V1<P> {
	type Error = anyhow::Error;

	fn try_from(value: VerifyComm2<P>) -> Result<Self, Self::Error> {
		let data: Result<_, _> = value
			.data
			.into_iter()
			.map(|(party_idx, x)| {
				let x: Option<Result<Comm1V1<P>, _>> = x.map(|x| x.try_into());
				match x {
					Some(Ok(x)) => Ok((party_idx, Some(x))),
					Some(Err(e)) => Err(e),
					None => Ok((party_idx, None)),
				}
			})
			.collect();
		Ok(VerifyComm2V1 { data: data? })
	}
}

impl<P: ECPoint> TryFrom<LocalSig3<P>> for LocalSig3V1<P> {
	type Error = anyhow::Error;

	fn try_from(value: LocalSig3<P>) -> Result<Self, Self::Error> {
		if value.responses.len() != 1 {
			bail!("LocalSig3 must have exactly one element");
		}
		Ok(LocalSig3V1 { response: value.responses.into_iter().next().unwrap() })
	}
}

impl<P: ECPoint> TryFrom<VerifyLocalSig4<P>> for VerifyLocalSig4V1<P> {
	type Error = anyhow::Error;

	fn try_from(value: VerifyLocalSig4<P>) -> Result<Self, Self::Error> {
		let data: Result<_, _> = value
			.data
			.into_iter()
			.map(|(party_idx, x)| {
				let x: Option<Result<LocalSig3V1<P>, _>> = x.map(|x| x.try_into());
				match x {
					Some(Ok(x)) => Ok((party_idx, Some(x))),
					Some(Err(e)) => Err(e),
					None => Ok((party_idx, None)),
				}
			})
			.collect();
		Ok(VerifyLocalSig4V1 { data: data? })
	}
}
