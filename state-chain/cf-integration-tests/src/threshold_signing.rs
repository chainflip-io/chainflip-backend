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

use arrayref::array_ref;
use cf_chains::{
	btc,
	dot::{EncodedPolkadotPayload, PolkadotPair, PolkadotPublicKey, PolkadotSignature},
	evm::{to_evm_address, AggKey, SchnorrVerificationComponents},
	sol::{
		signing_key::SolSigningKey, sol_tx_core::signer::Signer, SolAddress, SolSignature,
		SolVersionedMessage,
	},
};
use cf_primitives::{EpochIndex, GENESIS_EPOCH};
use libsecp256k1::{PublicKey, SecretKey};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::marker::PhantomData;

use crate::GENESIS_KEY_SEED;

#[derive(Clone)]
pub struct KeyComponents<SecretKey, AggKey> {
	seed: u64,
	secret: SecretKey,
	agg_key: AggKey,
	epoch_index: EpochIndex,
}

pub type EthKeyComponents = KeyComponents<SecretKey, AggKey>;

pub trait KeyUtils {
	type SigVerification;
	type AggKey;
	type Message: ?Sized;

	fn sign(&self, message: &Self::Message) -> Self::SigVerification;

	fn generate(seed: u64, epoch_index: EpochIndex) -> Self;

	fn generate_next(&self) -> Self;

	fn agg_key(&self) -> Self::AggKey;
}

impl KeyUtils for EthKeyComponents {
	type SigVerification = SchnorrVerificationComponents;
	type AggKey = AggKey;
	type Message = [u8];

	fn sign(&self, message: &Self::Message) -> Self::SigVerification {
		let message: &[u8; 32] = message.try_into().expect("Message for Ethereum is not 32 bytes");
		assert_eq!(self.agg_key, AggKey::from_private_key_bytes(self.secret.serialize()));

		// just use the same signature nonce for every ceremony in tests
		let k: [u8; 32] = StdRng::seed_from_u64(200).gen();
		let k = SecretKey::parse(&k).unwrap();
		let signature = self.agg_key.sign(message, &self.secret, &k);

		let k_times_g_address = to_evm_address(PublicKey::from_secret_key(&k)).to_fixed_bytes();
		SchnorrVerificationComponents { s: signature, k_times_g_address }
	}

	fn generate(seed: u64, epoch_index: EpochIndex) -> Self {
		let agg_key_priv: [u8; 32] = StdRng::seed_from_u64(seed).gen();
		let secret = SecretKey::parse(&agg_key_priv).unwrap();
		KeyComponents {
			seed,
			secret,
			agg_key: AggKey::from_pubkey_compressed(
				PublicKey::from_secret_key(&secret).serialize_compressed(),
			),
			epoch_index,
		}
	}

	fn generate_next(&self) -> Self {
		Self::generate(self.seed + 1, self.epoch_index + 1)
	}

	fn agg_key(&self) -> Self::AggKey {
		self.agg_key
	}
}

pub struct ThresholdSigner<KeyComponents, SigVerification> {
	previous_key_components: Option<KeyComponents>,
	key_components: KeyComponents,
	proposed_key_components: Option<KeyComponents>,
	_phantom: PhantomData<SigVerification>,
}

impl<KeyComponents, SigVerification, AggKey: Eq> ThresholdSigner<KeyComponents, SigVerification>
where
	KeyComponents: KeyUtils<SigVerification = SigVerification, AggKey = AggKey> + Clone,
{
	pub fn sign_with_key(&self, key: AggKey, message: &KeyComponents::Message) -> SigVerification {
		let current_key = self.key_components.agg_key();
		if key == current_key {
			return self.key_components.sign(message)
		}
		if self.previous_key_components.is_some() &&
			self.previous_key_components.as_ref().unwrap().agg_key() == key
		{
			return self.previous_key_components.as_ref().unwrap().sign(message)
		}
		if self.proposed_key_components.is_some() &&
			self.proposed_key_components.as_ref().unwrap().agg_key() == key
		{
			self.proposed_key_components.as_ref().unwrap().sign(message)
		} else {
			panic!("Unknown key");
		}
	}

	pub fn propose_new_key(&mut self) -> AggKey {
		let new_key = KeyComponents::generate_next(&self.key_components);
		let agg_key = new_key.agg_key();
		self.proposed_key_components = Some(new_key);
		agg_key
	}

	// Rotate to the current proposed key and clear the proposed key
	pub fn use_proposed_key(&mut self) {
		if self.proposed_key_components.is_some() {
			self.previous_key_components = Some(self.key_components.clone());
			self.key_components =
				self.proposed_key_components.as_ref().expect("No key has been proposed").clone();
			self.proposed_key_components = None;
		}
	}

	pub fn is_key_valid(&self, key: &AggKey) -> bool {
		let current_key = self.key_components.agg_key();
		if *key != current_key {
			if let Some(next_key_components) = self.proposed_key_components.as_ref() {
				*key == next_key_components.agg_key()
			} else {
				false
			}
		} else {
			true
		}
	}
}

pub type EthThresholdSigner = ThresholdSigner<EthKeyComponents, SchnorrVerificationComponents>;

impl Default for EthThresholdSigner {
	fn default() -> Self {
		ThresholdSigner {
			previous_key_components: None,
			key_components: EthKeyComponents::generate(GENESIS_KEY_SEED, GENESIS_EPOCH),
			proposed_key_components: None,
			_phantom: PhantomData,
		}
	}
}

pub type DotKeyComponents = KeyComponents<PolkadotPair, PolkadotPublicKey>;

pub type DotThresholdSigner = ThresholdSigner<DotKeyComponents, PolkadotSignature>;

impl Default for DotThresholdSigner {
	fn default() -> Self {
		Self {
			previous_key_components: None,
			key_components: DotKeyComponents::generate(GENESIS_KEY_SEED, GENESIS_EPOCH),
			proposed_key_components: None,
			_phantom: PhantomData,
		}
	}
}

impl KeyUtils for DotKeyComponents {
	type SigVerification = PolkadotSignature;
	type AggKey = PolkadotPublicKey;
	type Message = EncodedPolkadotPayload;

	fn sign(&self, message: &Self::Message) -> Self::SigVerification {
		self.secret.sign(message)
	}

	fn generate(seed: u64, epoch_index: EpochIndex) -> Self {
		let keypair = PolkadotPair::from_seed(&StdRng::seed_from_u64(seed).gen());
		KeyComponents { seed, agg_key: keypair.public_key(), secret: keypair, epoch_index }
	}

	fn generate_next(&self) -> Self {
		Self::generate(self.seed + 1, self.epoch_index + 1)
	}

	fn agg_key(&self) -> Self::AggKey {
		self.agg_key
	}
}

pub type BtcKeyComponents = KeyComponents<secp256k1::Keypair, cf_chains::btc::AggKey>;

pub type BtcThresholdSigner = ThresholdSigner<BtcKeyComponents, btc::Signature>;

impl Default for BtcThresholdSigner {
	fn default() -> Self {
		Self {
			previous_key_components: None,
			key_components: BtcKeyComponents::generate(GENESIS_KEY_SEED, GENESIS_EPOCH),
			proposed_key_components: None,
			_phantom: PhantomData,
		}
	}
}

impl KeyUtils for BtcKeyComponents {
	type SigVerification = btc::Signature;
	type AggKey = btc::AggKey;
	type Message = [u8];

	fn sign(&self, message: &Self::Message) -> Self::SigVerification {
		let secp = secp256k1::Secp256k1::new();
		let signature = secp
			.sign_schnorr(&secp256k1::Message::from_digest_slice(message).unwrap(), &self.secret);
		*array_ref!(signature[..], 0, 64)
	}

	fn generate(seed: u64, epoch_index: EpochIndex) -> Self {
		let priv_seed: [u8; 32] = StdRng::seed_from_u64(seed).gen();
		let secp = secp256k1::Secp256k1::new();
		let keypair = secp256k1::Keypair::from_seckey_slice(&secp, &priv_seed).unwrap();
		let pubkey_x = secp256k1::XOnlyPublicKey::from_keypair(&keypair).0.serialize();
		let agg_key = btc::AggKey { previous: None, current: pubkey_x };

		KeyComponents { seed, secret: keypair, agg_key, epoch_index }
	}

	fn generate_next(&self) -> Self {
		let prev_agg_key = self.agg_key.current;
		let mut generated = Self::generate(self.seed + 1, self.epoch_index + 1);
		generated.agg_key.previous = Some(prev_agg_key);
		generated
	}

	fn agg_key(&self) -> Self::AggKey {
		self.agg_key
	}
}

pub type SolKeyComponents = KeyComponents<SolSigningKey, SolAddress>;

pub type SolThresholdSigner = ThresholdSigner<SolKeyComponents, SolSignature>;

impl Default for SolThresholdSigner {
	fn default() -> Self {
		Self {
			previous_key_components: None,
			key_components: SolKeyComponents::generate(GENESIS_KEY_SEED, GENESIS_EPOCH),
			proposed_key_components: None,
			_phantom: PhantomData,
		}
	}
}

impl KeyUtils for SolKeyComponents {
	type SigVerification = SolSignature;
	type AggKey = SolAddress;
	type Message = SolVersionedMessage;

	fn sign(&self, message: &Self::Message) -> Self::SigVerification {
		self.secret.sign_message(message.serialize().as_slice())
	}

	fn generate(seed: u64, epoch_index: EpochIndex) -> Self {
		let signing_key = SolSigningKey::generate(&mut rand::rngs::StdRng::seed_from_u64(seed));
		KeyComponents {
			seed,
			agg_key: signing_key.pubkey().into(),
			secret: signing_key,
			epoch_index,
		}
	}

	fn generate_next(&self) -> Self {
		Self::generate(self.seed + 1, self.epoch_index + 1)
	}

	fn agg_key(&self) -> Self::AggKey {
		self.agg_key
	}
}
