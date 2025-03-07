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

mod keygen_data;
mod keygen_detail;
mod keygen_stages;

#[cfg(test)]
mod tests;

#[cfg(feature = "test")]
pub use keygen_detail::{
	generate_shares_and_commitment, get_key_data_for_test, DKGUnverifiedCommitment, OutgoingShares,
	SharingParameters,
};

#[cfg(test)]
pub use keygen_data::{gen_keygen_data_hash_comm1, gen_keygen_data_verify_hash_comm2};

pub use keygen_data::{
	BlameResponse8, CoeffComm3, Complaints6, HashComm1, KeygenData, PubkeyShares0, SecretShare5,
	VerifyBlameResponses9, VerifyCoeffComm4, VerifyComplaints7, VerifyHashComm2,
};

pub use keygen_detail::{
	genesis::{generate_key_data, generate_key_data_with_initial_incompatibility},
	HashContext,
};

pub use keygen_stages::{
	HashCommitments1, KeygenCommon, PubkeySharesStage0, VerifyHashCommitmentsBroadcast2,
};
