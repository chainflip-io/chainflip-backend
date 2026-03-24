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

use jsonrpsee::{proc_macros::rpc, types::ErrorObjectOwned};
use sp_core::Bytes;

#[rpc(server, client, namespace = "grandpa_ext")]
pub trait GrandpaExtApi {
	/// Get or create a GRND delegate key in the node's keystore. Returns the public key bytes.
	/// If a GRND key already exists in the keystore, returns it.
	/// If none exists, generates a new one, inserts it, and returns it.
	#[method(name = "getOrCreateDelegateKey")]
	async fn get_or_create_delegate_key(&self) -> Result<Bytes, ErrorObjectOwned>;

	/// Sign a payload with the GRND delegate key.
	/// Used to create the proof for delegate_grandpa_vote.
	#[method(name = "signWithDelegateKey")]
	async fn sign_with_delegate_key(&self, payload: Bytes) -> Result<Bytes, ErrorObjectOwned>;
}
