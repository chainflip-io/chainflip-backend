use anyhow::Result;

use super::{curve25519::ristretto::Point, ChainTag, CryptoScheme, ECPoint};
use cf_chains::dot::PolkadotPublicKey;
use schnorrkel::context::{SigningContext, SigningTranscript};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct PolkadotSigning {}

// Polkadot seems to be using this generic "substrate" context for signing
const SIGNING_CTX: &[u8] = b"substrate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolkadotSignature(schnorrkel::Signature);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct SigningPayload(Vec<u8>);

impl std::fmt::Display for SigningPayload {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", hex::encode(&self.0))
	}
}

impl AsRef<[u8]> for SigningPayload {
	fn as_ref(&self) -> &[u8] {
		self.0.as_ref()
	}
}

impl SigningPayload {
	pub fn new(payload: Vec<u8>) -> Result<Self> {
		if payload.is_empty() || payload.len() > 256 {
			anyhow::bail!("Invalid payload size");
		}
		Ok(SigningPayload(payload))
	}
}

impl From<PolkadotSignature> for cf_chains::dot::PolkadotSignature {
	fn from(cfe_sig: PolkadotSignature) -> Self {
		sp_core::sr25519::Signature(cfe_sig.0.to_bytes())
	}
}

impl CryptoScheme for PolkadotSigning {
	type Point = Point;
	type Signature = PolkadotSignature;
	type AggKey = cf_chains::dot::PolkadotPublicKey;
	type SigningPayload = SigningPayload;

	const NAME: &'static str = "Polkadot";
	const CHAIN_TAG: ChainTag = ChainTag::Polkadot;

	fn build_signature(
		z: <Self::Point as super::ECPoint>::Scalar,
		group_commitment: Self::Point,
	) -> Self::Signature {
		// First, serialize the signature the way defined in schnorrkel
		let mut bytes: [u8; 64] = [0u8; 64];

		bytes[..32].copy_from_slice(&group_commitment.get_element().compress().to_bytes());
		bytes[32..].copy_from_slice(&z.to_bytes());
		bytes[63] |= 128;

		// Then parse the bytes into the schnorrkel type
		// NOTE: it should be safe to unwrap because it should be valid by construction
		PolkadotSignature(schnorrkel::Signature::from_bytes(&bytes).unwrap())
	}

	fn build_challenge(
		pubkey: Self::Point,
		nonce_commitment: Self::Point,
		payload: &Self::SigningPayload,
	) -> <Self::Point as super::ECPoint>::Scalar {
		// NOTE: This computation is copied from schnorrkel's
		// source code (since it is the "source of truth")
		// (see https://docs.rs/schnorrkel/0.9.1/src/schnorrkel/sign.rs.html#171)

		// Is the message not expected to be already hashed?
		let mut t = SigningContext::new(SIGNING_CTX).bytes(&payload.0);
		t.proto_name(b"Schnorr-sig");
		// TODO: see how expensive this compression is and whether we should
		// always keep both compressed and uncompressed in memory the way schnorrkel does
		t.commit_point(b"sign:pk", &pubkey.get_element().compress());
		t.commit_point(b"sign:R", &nonce_commitment.get_element().compress());

		t.challenge_scalar(b"sign:c").into()
	}

	fn is_party_response_valid(
		y_i: &Self::Point,
		lambda_i: &<Self::Point as ECPoint>::Scalar,
		commitment: &Self::Point,
		challenge: &<Self::Point as ECPoint>::Scalar,
		signature_response: &<Self::Point as ECPoint>::Scalar,
	) -> bool {
		Point::from_scalar(signature_response) == *commitment + (*y_i) * challenge * lambda_i
	}

	fn verify_signature(
		signature: &Self::Signature,
		key_id: &crate::multisig::KeyId,
		payload: &Self::SigningPayload,
	) -> anyhow::Result<()> {
		let public_key = schnorrkel::PublicKey::from_bytes(&key_id.0).expect("invalid public key");

		let context = schnorrkel::signing_context(SIGNING_CTX);

		public_key
			.verify(context.bytes(payload.0.as_slice()), &signature.0)
			.map_err(anyhow::Error::msg)
	}

	fn agg_key(pubkey: &Self::Point) -> Self::AggKey {
		PolkadotPublicKey(sp_core::sr25519::Public::from_raw(
			pubkey.get_element().compress().to_bytes(),
		))
	}

	fn build_response(
		nonce: <Self::Point as super::ECPoint>::Scalar,
		private_key: &<Self::Point as super::ECPoint>::Scalar,
		challenge: <Self::Point as super::ECPoint>::Scalar,
	) -> <Self::Point as super::ECPoint>::Scalar {
		// "Response" is computed as done in schnorrkel
		challenge * private_key + nonce
	}

	#[cfg(test)]
	fn signing_payload_for_test() -> Self::SigningPayload {
		SigningPayload::new(vec![1_u8; 256]).unwrap()
	}
}

// Check that our signature generation results in
// signatures deemed valid by schnorrkel verification code
#[test]
fn signature_should_be_valid() {
	use super::{curve25519::Scalar, ECPoint, ECScalar};
	use crate::multisig::crypto::Rng;
	use rand_legacy::SeedableRng;
	use utilities::assert_ok;

	let mut rng = Rng::from_seed([0; 32]);

	// Generate a key pair
	let secret_key = Scalar::random(&mut rng);
	let public_key = Point::from_scalar(&secret_key);

	// Message to sign
	let payload = PolkadotSigning::signing_payload_for_test();

	let signature = {
		// Pick random nonce and commit to it
		let nonce = Scalar::random(&mut rng);
		let nonce_commitment = Point::from_scalar(&nonce);

		let challenge = PolkadotSigning::build_challenge(public_key, nonce_commitment, &payload);

		let response = PolkadotSigning::build_response(nonce, &secret_key, challenge);

		PolkadotSigning::build_signature(response, nonce_commitment)
	};

	assert_ok!(schnorrkel::PublicKey::from_point(public_key.get_element()).verify_simple(
		SIGNING_CTX,
		&payload.0,
		&signature.0
	));
}
