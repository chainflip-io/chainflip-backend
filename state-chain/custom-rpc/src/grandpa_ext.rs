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

use cf_rpc_apis::grandpa::GrandpaExtApiServer;
use jsonrpsee::{core::async_trait, types::ErrorObjectOwned};
use sp_core::Bytes;
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
	async fn get_or_create_delegate_key(&self) -> Result<Bytes, ErrorObjectOwned> {
		let existing_keys = self.keystore.ed25519_public_keys(GRND_KEY_TYPE);

		let public_key = if let Some(key) = existing_keys.into_iter().next() {
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

		Ok(Bytes(public_key.0.to_vec()))
	}

	async fn sign_with_delegate_key(&self, payload: Bytes) -> Result<Bytes, ErrorObjectOwned> {
		let existing_keys = self.keystore.ed25519_public_keys(GRND_KEY_TYPE);
		let public_key = existing_keys.into_iter().next().ok_or_else(|| {
			ErrorObjectOwned::owned(
				-32001,
				"No GRND delegate key found in keystore. Call grandpa_ext_getOrCreateDelegateKey \
				 first.",
				None::<()>,
			)
		})?;

		let signature = self
			.keystore
			.ed25519_sign(GRND_KEY_TYPE, &public_key, &payload.0)
			.map_err(|e| {
				ErrorObjectOwned::owned(
					-32000,
					format!("Failed to sign with GRND delegate key: {e}"),
					None::<()>,
				)
			})?;

		let signature = signature.ok_or_else(|| {
			ErrorObjectOwned::owned(
				-32001,
				"GRND delegate key found in keystore but signing failed (key may be corrupt).",
				None::<()>,
			)
		})?;

		Ok(Bytes(signature.0.to_vec()))
	}
}
