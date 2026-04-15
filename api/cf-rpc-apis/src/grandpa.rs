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

use cf_primitives::AccountId;
use jsonrpsee::{proc_macros::rpc, types::ErrorObjectOwned};
use serde::{Deserialize, Serialize};
use sp_core::ed25519;

/// The payload that the node's GRND delegate key signs to authorise a GRANDPA vote delegation.
/// Matches the tuple expected by `pallet_cf_validator::delegate_grandpa_vote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationPayload {
	pub account_id: AccountId,
	pub session_index: u32,
}

/// The delegate public key together with its signature over the SCALE-encoded
/// [`DelegationPayload`]. The signature is the `proof` field expected by
/// `pallet_cf_validator::delegate_grandpa_vote`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedDelegationProof {
	pub delegate_key: ed25519::Public,
	pub signature: ed25519::Signature,
}

#[rpc(server, client, namespace = "grandpa_ext")]
pub trait GrandpaExtApi {
	/// Sign a GRANDPA vote delegation payload with the node's GRND delegate key, creating the key
	/// if it does not yet exist. The payload is SCALE-encoded and signed internally so callers
	/// cannot ask the node to sign arbitrary bytes. Returns both the delegate public key and the
	/// signature.
	#[method(name = "signDelegationProof")]
	async fn sign_delegation_proof(
		&self,
		payload: DelegationPayload,
	) -> Result<SignedDelegationProof, ErrorObjectOwned>;
}
