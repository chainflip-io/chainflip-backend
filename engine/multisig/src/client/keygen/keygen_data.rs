// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use cf_primitives::AuthorityCount;
use serde::{Deserialize, Serialize};
use state_chain_runtime::constants::common::MAX_AUTHORITIES;

use crate::{
	client::common::{
		BroadcastVerificationMessage, DelayDeserialization, KeygenStageName,
		PreProcessStageDataCheck,
	},
	crypto::ECPoint,
	ChainSigning,
};

use super::keygen_detail::{ShamirShare, MAX_COEFF_COMM_3_SIZE};

#[cfg(test)]
pub use tests::{gen_keygen_data_hash_comm1, gen_keygen_data_verify_hash_comm2};

/// Data sent between parties over p2p for a keygen ceremony
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeygenData<P: ECPoint> {
	#[serde(bound = "")]
	PubkeyShares0(PubkeyShares0<P>),
	HashComm1(HashComm1),
	VerifyHashComm2(VerifyHashComm2),
	#[serde(bound = "")] // see https://github.com/serde-rs/serde/issues/1296
	CoeffComm3(CoeffComm3<P>),
	#[serde(bound = "")]
	VerifyCoeffComm4(VerifyCoeffComm4<P>),
	#[serde(bound = "")]
	SecretShares5(SecretShare5<P>),
	Complaints6(Complaints6),
	VerifyComplaints7(VerifyComplaints7),
	#[serde(bound = "")]
	BlameResponse8(BlameResponse8<P>),
	#[serde(bound = "")]
	VerifyBlameResponses9(VerifyBlameResponses9<P>),
}

impl<P: ECPoint> std::fmt::Display for KeygenData<P> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let inner = match self {
			KeygenData::PubkeyShares0(inner) => inner.to_string(),
			KeygenData::HashComm1(inner) => inner.to_string(),
			KeygenData::VerifyHashComm2(inner) => inner.to_string(),
			KeygenData::CoeffComm3(inner) => inner.to_string(),
			KeygenData::VerifyCoeffComm4(inner) => inner.to_string(),
			KeygenData::SecretShares5(inner) => inner.to_string(),
			KeygenData::Complaints6(inner) => inner.to_string(),
			KeygenData::VerifyComplaints7(inner) => inner.to_string(),
			KeygenData::BlameResponse8(inner) => inner.to_string(),
			KeygenData::VerifyBlameResponses9(inner) => inner.to_string(),
		};
		write!(f, "KeygenData({inner})")
	}
}

impl<P: ECPoint> PreProcessStageDataCheck<KeygenStageName> for KeygenData<P> {
	fn is_data_size_valid<Chain: ChainSigning>(
		&self,
		num_of_parties: AuthorityCount,
		_num_of_payloads: Option<usize>,
	) -> bool {
		let num_of_parties = num_of_parties as usize;
		match self {
			KeygenData::PubkeyShares0(_) | KeygenData::HashComm1(_) =>
				self.is_initial_stage_data_size_valid::<Chain>(),
			KeygenData::VerifyHashComm2(message) => message.data.len() == num_of_parties,
			KeygenData::CoeffComm3(message) => message.payload.len() <= MAX_COEFF_COMM_3_SIZE,
			KeygenData::VerifyCoeffComm4(message) =>
				message.is_data_size_valid(num_of_parties, MAX_COEFF_COMM_3_SIZE),
			KeygenData::SecretShares5(_) => true,
			KeygenData::Complaints6(complaints) => {
				// The complaints are optional, so we just check the max length
				complaints.0.len() <= num_of_parties
			},
			KeygenData::VerifyComplaints7(message) =>
				message.data.len() == num_of_parties &&
					!message
						.data
						.values()
						.flatten()
						.any(|complaints| complaints.0.len() > num_of_parties),
			KeygenData::BlameResponse8(blame_response) => {
				// The blame response will only contain a subset, so we just check the max length
				blame_response.0.len() <= num_of_parties
			},
			KeygenData::VerifyBlameResponses9(message) =>
				message.data.len() == num_of_parties &&
					!message
						.data
						.values()
						.flatten()
						.any(|blame_response| blame_response.0.len() > num_of_parties),
		}
	}

	fn is_initial_stage_data_size_valid<Chain: ChainSigning>(&self) -> bool {
		match self {
			KeygenData::PubkeyShares0(message) => message.0.len() <= MAX_AUTHORITIES as usize,
			KeygenData::HashComm1(_) => true,
			_ => panic!("unexpected stage"),
		}
	}

	fn should_delay_unauthorised(&self) -> bool {
		matches!(self, KeygenData::PubkeyShares0(_) | KeygenData::HashComm1(_))
	}

	fn should_delay(stage_name: KeygenStageName, message: &Self) -> bool {
		match stage_name {
			KeygenStageName::PubkeyShares0 => {
				matches!(message, KeygenData::HashComm1(_))
			},
			KeygenStageName::HashCommitments1 => {
				matches!(message, KeygenData::VerifyHashComm2(_))
			},
			KeygenStageName::VerifyHashCommitmentsBroadcast2 => {
				matches!(message, KeygenData::CoeffComm3(_))
			},
			KeygenStageName::CoefficientCommitments3 => {
				matches!(message, KeygenData::VerifyCoeffComm4(_))
			},
			KeygenStageName::VerifyCommitmentsBroadcast4 => {
				matches!(message, KeygenData::SecretShares5(_))
			},
			KeygenStageName::SecretSharesStage5 => {
				matches!(message, KeygenData::Complaints6(_))
			},
			KeygenStageName::ComplaintsStage6 => {
				matches!(message, KeygenData::VerifyComplaints7(_))
			},
			KeygenStageName::VerifyComplaintsBroadcastStage7 => {
				matches!(message, KeygenData::BlameResponse8(_))
			},
			KeygenStageName::BlameResponsesStage8 => {
				matches!(message, KeygenData::VerifyBlameResponses9(_))
			},
			KeygenStageName::VerifyBlameResponsesBroadcastStage9 => {
				// Last stage, nothing to delay
				false
			},
		}
	}
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct HashComm1(pub sp_core::H256);

#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct PubkeyShares0<P: ECPoint>(#[serde(bound = "")] pub BTreeMap<AuthorityCount, P>);

pub type VerifyHashComm2 = BroadcastVerificationMessage<HashComm1>;

pub type CoeffComm3<P> = DelayDeserialization<super::keygen_detail::DKGUnverifiedCommitment<P>>;

pub type VerifyCoeffComm4<P> = BroadcastVerificationMessage<CoeffComm3<P>>;

/// Secret share of our locally generated secret calculated separately
/// for each party as the result of evaluating sharing polynomial (generated
/// during stage 1) at the corresponding signer's index
pub type SecretShare5<P> = ShamirShare<P>;

/// List of parties blamed for sending invalid secret shares
#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct Complaints6(pub BTreeSet<AuthorityCount>);

pub type VerifyComplaints7 = BroadcastVerificationMessage<Complaints6>;

/// For each party blaming a node, it responds with the corresponding (valid)
/// secret share. Unlike secret shares sent at the earlier stage, these shares
/// are verifiably broadcast, so sending an invalid share would result in the
/// node being slashed. Although the shares are meant to be secret, it is safe
/// to reveal/broadcast some them at this stage: a node's long-term secret can
/// only be recovered by collecting shares from all (N-1) nodes, which would
/// require collusion of N-1 nodes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct BlameResponse8<P: ECPoint>(
	#[serde(bound = "")] pub BTreeMap<AuthorityCount, ShamirShare<P>>,
);

pub type VerifyBlameResponses9<P> = BroadcastVerificationMessage<BlameResponse8<P>>;

derive_impls_for_enum_variants!(impl<P: ECPoint> for PubkeyShares0<P>, KeygenData::PubkeyShares0, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for HashComm1, KeygenData::HashComm1, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyHashComm2, KeygenData::VerifyHashComm2, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for CoeffComm3<P>, KeygenData::CoeffComm3, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyCoeffComm4<P>, KeygenData::VerifyCoeffComm4, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for SecretShare5<P>, KeygenData::SecretShares5, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for Complaints6, KeygenData::Complaints6, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyComplaints7, KeygenData::VerifyComplaints7, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for BlameResponse8<P>, KeygenData::BlameResponse8, KeygenData<P>);
derive_impls_for_enum_variants!(impl<P: ECPoint> for VerifyBlameResponses9<P>, KeygenData::VerifyBlameResponses9, KeygenData<P>);

// TODO: combine these with derive_impls_for_enum_variants! macro?
derive_display_as_type_name!(PubkeyShares0<P: ECPoint>);
derive_display_as_type_name!(HashComm1);
derive_display_as_type_name!(VerifyHashComm2);
derive_display_as_type_name!(CoeffComm3<P: ECPoint>);
derive_display_as_type_name!(VerifyCoeffComm4<P: ECPoint>);
derive_display_as_type_name!(SecretShare5<P: ECPoint>);
derive_display_as_type_name!(Complaints6);
derive_display_as_type_name!(VerifyComplaints7);
derive_display_as_type_name!(BlameResponse8<P: ECPoint>);
derive_display_as_type_name!(VerifyBlameResponses9<P: ECPoint>);
