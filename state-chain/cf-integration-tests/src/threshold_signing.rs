use std::marker::PhantomData;

use cf_chains::eth::{to_ethereum_address, AggKey, SchnorrVerificationComponents};
use cf_primitives::KeyId;
use libsecp256k1::{PublicKey, SecretKey};
use rand::{rngs::StdRng, Rng, SeedableRng};

use crate::GENESIS_KEY_SEED;

#[derive(Clone)]
pub struct KeyComponents<SecretKey, PublicKey, AggKey> {
	pub seed: u64,
	pub secret: SecretKey,
	pub public_key: PublicKey,
	pub agg_key: AggKey,
}

pub type EthKeyComponents = KeyComponents<SecretKey, PublicKey, AggKey>;

pub trait KeyUtils {
	type SigVerification;
	type AggKey;

	fn sign(&self, message: &[u8; 32]) -> Self::SigVerification;

	fn key_id(&self) -> KeyId;

	fn generate_keypair(seed: u64) -> Self;

	fn generate_next_key_pair(&self) -> Self;

	fn agg_key(&self) -> Self::AggKey;
}

impl KeyUtils for EthKeyComponents {
	type SigVerification = SchnorrVerificationComponents;
	type AggKey = AggKey;

	fn sign(&self, message: &[u8; 32]) -> Self::SigVerification {
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

	// Generate a keypair with seed
	fn generate_keypair(seed: u64) -> Self {
		let agg_key_priv: [u8; 32] = StdRng::seed_from_u64(seed).gen();
		let secret = SecretKey::parse(&agg_key_priv).unwrap();
		let public_key = PublicKey::from_secret_key(&secret);
		KeyComponents {
			seed,
			secret,
			public_key,
			agg_key: AggKey::from_pubkey_compressed(public_key.serialize_compressed()),
		}
	}

	fn generate_next_key_pair(&self) -> Self {
		let next_seed = self.seed + 1;
		Self::generate_keypair(next_seed)
	}

	fn agg_key(&self) -> Self::AggKey {
		self.agg_key
	}
}

pub struct ThresholdSigner<KeyComponents, SigVerification> {
	pub key_components: KeyComponents,
	pub proposed_key_components: Option<KeyComponents>,
	_phantom: PhantomData<SigVerification>,
}

impl<KeyComponents, SigVerification, AggKey> ThresholdSigner<KeyComponents, SigVerification>
where
	KeyComponents: KeyUtils<SigVerification = SigVerification, AggKey = AggKey> + Clone,
{
	pub fn sign_with_key(&self, key_id: KeyId, message: &[u8; 32]) -> SigVerification {
		let curr_key_id = self.key_components.key_id();
		if key_id == curr_key_id {
			println!("Signing with current key");
			return self.key_components.sign(message)
		}
		let next_key_id = self.proposed_key_components.as_ref().unwrap().key_id();
		if key_id == next_key_id {
			println!("Signing with proposed key");
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

	pub fn propose_new_public_key(&mut self) -> AggKey {
		self.proposed_key_components =
			Some(KeyComponents::generate_next_key_pair(&self.key_components));
		self.proposed_public_key()
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
			key_components: EthKeyComponents::generate_keypair(GENESIS_KEY_SEED),
			proposed_key_components: None,
			_phantom: PhantomData,
		}
	}
}
