use arrayref::array_ref;
use cf_chains::{
	btc,
	dot::{EncodedPolkadotPayload, PolkadotPair, PolkadotPublicKey, PolkadotSignature},
	eth::{to_ethereum_address, AggKey, SchnorrVerificationComponents},
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

		let k_times_g_address =
			to_ethereum_address(PublicKey::from_secret_key(&k)).to_fixed_bytes();
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
	key_components: KeyComponents,
	proposed_key_components: Option<KeyComponents>,
	_phantom: PhantomData<SigVerification>,
}

impl<KeyComponents, SigVerification, AggKey: Eq> ThresholdSigner<KeyComponents, SigVerification>
where
	KeyComponents: KeyUtils<SigVerification = SigVerification, AggKey = AggKey> + Clone,
{
	pub fn sign_with_key(&self, key: AggKey, message: &KeyComponents::Message) -> SigVerification {
		let curr_key = self.key_components.agg_key();
		if key == curr_key {
			return self.key_components.sign(message)
		}
		let next_key = self.proposed_key_components.as_ref().unwrap().agg_key();
		if key == next_key {
			self.proposed_key_components.as_ref().unwrap().sign(message)
		} else {
			panic!("Unknown key");
		}
	}

	pub fn current_agg_key(&self) -> AggKey {
		self.key_components.agg_key()
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
			self.key_components =
				self.proposed_key_components.as_ref().expect("No key has been proposed").clone();
			self.proposed_key_components = None;
		}
	}
}

pub type EthThresholdSigner = ThresholdSigner<EthKeyComponents, SchnorrVerificationComponents>;

impl Default for EthThresholdSigner {
	fn default() -> Self {
		ThresholdSigner {
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

pub type BtcKeyComponents = KeyComponents<secp256k1::KeyPair, cf_chains::btc::AggKey>;

pub type BtcThresholdSigner = ThresholdSigner<BtcKeyComponents, btc::Signature>;

impl Default for BtcThresholdSigner {
	fn default() -> Self {
		Self {
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
		let signature =
			secp.sign_schnorr(&secp256k1::Message::from_slice(message).unwrap(), &self.secret);
		*array_ref!(signature[..], 0, 64)
	}

	fn generate(seed: u64, epoch_index: EpochIndex) -> Self {
		let priv_seed: [u8; 32] = StdRng::seed_from_u64(seed).gen();
		let secp = secp256k1::Secp256k1::new();
		let keypair = secp256k1::KeyPair::from_seckey_slice(&secp, &priv_seed).unwrap();
		let pubkey_x = secp256k1::XOnlyPublicKey::from_keypair(&keypair).0.serialize();
		let agg_key = btc::AggKey { previous: None, current: pubkey_x };

		KeyComponents { seed, secret: keypair, agg_key, epoch_index }
	}

	fn generate_next(&self) -> Self {
		Self::generate(self.seed + 1, self.epoch_index + 1)
	}

	fn agg_key(&self) -> Self::AggKey {
		self.agg_key
	}
}
