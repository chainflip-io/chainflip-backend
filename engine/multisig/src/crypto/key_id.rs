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

use cf_primitives::EpochIndex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct KeyId {
	epoch_index: EpochIndex,
	public_key_bytes: Vec<u8>,
}

/// Defines the commonly agreed-upon byte-encoding used for public keys.
pub trait CanonicalEncoding {
	fn encode_key(&self) -> Vec<u8>;
}

impl KeyId {
	pub fn new<Key: CanonicalEncoding>(epoch_index: EpochIndex, key: Key) -> Self {
		KeyId { epoch_index, public_key_bytes: key.encode_key() }
	}
}

impl CanonicalEncoding for cf_chains::dot::PolkadotPublicKey {
	fn encode_key(&self) -> Vec<u8> {
		self.aliased_ref().to_vec()
	}
}

impl CanonicalEncoding for cf_chains::evm::AggKey {
	fn encode_key(&self) -> Vec<u8> {
		self.to_pubkey_compressed().to_vec()
	}
}

impl CanonicalEncoding for secp256k1::XOnlyPublicKey {
	fn encode_key(&self) -> Vec<u8> {
		self.serialize().to_vec()
	}
}

impl CanonicalEncoding for cf_chains::sol::SolAddress {
	fn encode_key(&self) -> Vec<u8> {
		self.0.to_vec()
	}
}

impl CanonicalEncoding for ed25519_dalek::VerifyingKey {
	fn encode_key(&self) -> Vec<u8> {
		self.to_bytes().to_vec()
	}
}

impl<const S: usize> CanonicalEncoding for [u8; S] {
	fn encode_key(&self) -> Vec<u8> {
		self.to_vec()
	}
}

impl core::fmt::Display for KeyId {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		#[cfg(feature = "std")]
		{
			write!(
				f,
				"KeyId(epoch_index: {}, public_key_bytes: {})",
				self.epoch_index,
				hex::encode(self.public_key_bytes.clone())
			)
		}
		#[cfg(not(feature = "std"))]
		{
			write!(
				f,
				"KeyId(epoch_index: {}, public_key_bytes: {:?})",
				self.epoch_index, self.public_key_bytes
			)
		}
	}
}

#[cfg(test)]
mod test_super {
	use super::*;

	#[test]
	fn key_id_encoding_is_stable() {
		let key_id = KeyId {
			epoch_index: 29,
			public_key_bytes: vec![
				0xa,
				93,
				141,
				u8::MAX,
				0,
				82,
				2,
				39,
				144,
				241,
				29,
				91,
				3,
				241,
				120,
				194,
			],
		};
		// We check this because if this form changes then there will be an impact to how keys
		// should be loaded from the db on the CFE. Thus, we want to be notified if this changes.
		let expected_bytes = vec![
			29, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 10, 93, 141, 255, 0, 82, 2, 39, 144, 241, 29, 91,
			3, 241, 120, 194,
		];
		assert_eq!(expected_bytes, bincode::serialize(&key_id).unwrap());
		assert_eq!(key_id, bincode::deserialize::<KeyId>(&expected_bytes).unwrap());
	}
}
