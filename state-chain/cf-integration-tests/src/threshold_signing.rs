use std::marker::PhantomData;

use cf_chains::{
	dot::{PolkadotPublicKey, PolkadotSignature},
	eth::{to_ethereum_address, AggKey, SchnorrVerificationComponents},
};
use cf_primitives::KeyId;
use libsecp256k1::{PublicKey, SecretKey};
use rand::{rngs::StdRng, Rng, SeedableRng};
use sp_core::{
	crypto::Pair as TraitPair,
	sr25519::{self, Pair},
};

use crate::GENESIS_KEY_SEED;

#[derive(Clone)]
pub struct KeyComponents<SecretKey, AggKey> {
	seed: u64,
	secret: SecretKey,
	agg_key: AggKey,
}

pub type EthKeyComponents = KeyComponents<SecretKey, AggKey>;

pub trait KeyUtils {
	type SigVerification;
	type AggKey;

	fn sign(&self, message: &[u8]) -> Self::SigVerification;

	fn key_id(&self) -> KeyId;

	fn generate(seed: u64) -> Self;

	fn generate_next(&self) -> Self;

	fn agg_key(&self) -> Self::AggKey;
}

impl KeyUtils for EthKeyComponents {
	type SigVerification = SchnorrVerificationComponents;
	type AggKey = AggKey;

	fn sign(&self, message: &[u8]) -> Self::SigVerification {
		let message: &[u8; 32] = message.try_into().expect("Message for Ethereum is not 32 bytes");
		assert_eq!(self.agg_key, AggKey::from_private_key_bytes(self.secret.serialize()));

		// just use the same signature nonce for every ceremony in tests
		let k: [u8; 32] = StdRng::seed_from_u64(200).gen();
		let k = SecretKey::parse(&k).unwrap();
		let signature = self.agg_key.sign(message, &self.secret, &k);

		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));
		SchnorrVerificationComponents { s: signature, k_times_g_address }
	}

	fn key_id(&self) -> KeyId {
		self.agg_key.to_pubkey_compressed().to_vec()
	}

	fn generate(seed: u64) -> Self {
		let agg_key_priv: [u8; 32] = StdRng::seed_from_u64(seed).gen();
		let secret = SecretKey::parse(&agg_key_priv).unwrap();
		KeyComponents {
			seed,
			secret,
			agg_key: AggKey::from_pubkey_compressed(
				PublicKey::from_secret_key(&secret).serialize_compressed(),
			),
		}
	}

	fn generate_next(&self) -> Self {
		let next_seed = self.seed + 1;
		Self::generate(next_seed)
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

impl<KeyComponents, SigVerification, AggKey> ThresholdSigner<KeyComponents, SigVerification>
where
	KeyComponents: KeyUtils<SigVerification = SigVerification, AggKey = AggKey> + Clone,
{
	pub fn sign_with_key(&self, key_id: KeyId, message: &[u8]) -> SigVerification {
		let curr_key_id = self.key_components.key_id();
		if key_id == curr_key_id {
			return self.key_components.sign(message)
		}
		let next_key_id = self.proposed_key_components.as_ref().unwrap().key_id();
		if key_id == next_key_id {
			self.proposed_key_components.as_ref().unwrap().sign(message)
		} else {
			panic!("Unknown key");
		}
	}

	pub fn proposed_public_key(&self) -> AggKey {
		self.proposed_key_components
			.as_ref()
			.expect("should have proposed key")
			.agg_key()
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
			key_components: EthKeyComponents::generate(GENESIS_KEY_SEED),
			proposed_key_components: None,
			_phantom: PhantomData,
		}
	}
}

pub type DotKeyComponents = KeyComponents<Pair, sr25519::Public>;

pub type DotThresholdSigner = ThresholdSigner<DotKeyComponents, PolkadotSignature>;

impl Default for DotThresholdSigner {
	fn default() -> Self {
		Self {
			key_components: DotKeyComponents::generate(GENESIS_KEY_SEED),
			proposed_key_components: None,
			_phantom: PhantomData,
		}
	}
}

impl KeyUtils for DotKeyComponents {
	type SigVerification = PolkadotSignature;

	type AggKey = PolkadotPublicKey;

	fn sign(&self, message: &[u8]) -> Self::SigVerification {
		self.secret.sign(message)
	}

	fn key_id(&self) -> KeyId {
		self.agg_key().0.to_vec()
	}

	fn generate(seed: u64) -> Self {
		let priv_seed: [u8; 32] = StdRng::seed_from_u64(seed).gen();
		let keypair: Pair = <Pair as TraitPair>::from_seed(&priv_seed);
		let agg_key = keypair.public();

		KeyComponents { seed, secret: keypair, agg_key }
	}

	fn generate_next(&self) -> Self {
		let next_seed = self.seed + 1;
		Self::generate(next_seed)
	}

	fn agg_key(&self) -> Self::AggKey {
		cf_chains::dot::PolkadotPublicKey(self.agg_key)
	}
}
