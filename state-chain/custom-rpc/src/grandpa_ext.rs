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

use cf_rpc_apis::grandpa::{DelegationPayload, GrandpaExtApiServer, SignedDelegationProof};
use codec::Encode;
use jsonrpsee::{core::async_trait, types::ErrorObjectOwned};
use sp_keystore::KeystorePtr;

/// Key type identifier for GRANDPA delegate keys stored in the keystore.
const GRND_KEY_TYPE: sp_application_crypto::KeyTypeId = sp_application_crypto::KeyTypeId(*b"grnd");

pub struct GrandpaExtRpc {
	keystore: KeystorePtr,
}

impl GrandpaExtRpc {
	pub fn new(keystore: KeystorePtr) -> Self {
		Self { keystore }
	}
}

#[async_trait]
impl GrandpaExtApiServer for GrandpaExtRpc {
	async fn sign_delegation_proof(
		&self,
		payload: DelegationPayload,
	) -> Result<SignedDelegationProof, ErrorObjectOwned> {
		let delegate_key = if let Some(key) =
			self.keystore.ed25519_public_keys(GRND_KEY_TYPE).into_iter().next()
		{
			key
		} else {
			self.keystore.ed25519_generate_new(GRND_KEY_TYPE, None).map_err(|e| {
				ErrorObjectOwned::owned(
					-32000,
					format!("Failed to generate GRND delegate key: {e}"),
					None::<()>,
				)
			})?
		};

		let encoded = (payload.account_id, payload.session_index).encode();
		let signature = self
			.keystore
			.ed25519_sign(GRND_KEY_TYPE, &delegate_key, &encoded)
			.map_err(|e| {
				ErrorObjectOwned::owned(
					-32000,
					format!("Failed to sign with GRND delegate key: {e}"),
					None::<()>,
				)
			})?
			.ok_or_else(|| {
				ErrorObjectOwned::owned(
					-32001,
					"GRND delegate key found in keystore but signing failed (key may be corrupt).",
					None::<()>,
				)
			})?;

		Ok(SignedDelegationProof { delegate_key, signature })
	}
}
