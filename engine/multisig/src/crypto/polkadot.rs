use anyhow::{Context, Result};

use super::{
	curve25519::ristretto::Point, CanonicalEncoding, ChainSigning, ChainTag, CryptoScheme,
	CryptoTag, ECPoint, SignatureToThresholdSignature,
};
use cf_chains::{Chain, ChainCrypto, Polkadot};
use schnorrkel::context::{SigningContext, SigningTranscript};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct PolkadotSigning {}

// Polkadot seems to be using this generic "substrate" context for signing
const SIGNING_CTX: &[u8] = b"substrate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolkadotSignature(schnorrkel::Signature);

impl SignatureToThresholdSignature<<Polkadot as Chain>::ChainCrypto> for Vec<PolkadotSignature> {
	fn to_threshold_signature(
		&self,
	) -> <<Polkadot as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature {
		self.iter()
			.map(|s| s.clone().into())
			.next()
			.expect("Exactly one signature for Polkadot")
	}
}

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
		Self::from_aliased(cfe_sig.0.to_bytes())
	}
}

impl CanonicalEncoding for schnorrkel::PublicKey {
	fn encode_key(&self) -> Vec<u8> {
		self.to_bytes().to_vec()
	}
}
#[derive(Clone, Debug, PartialEq)]
pub struct PolkadotCryptoScheme;

impl ChainSigning for PolkadotSigning {
	type CryptoScheme = PolkadotCryptoScheme;
	type ChainCrypto = <Polkadot as Chain>::ChainCrypto;
	const NAME: &'static str = "Polkadot";
	const CHAIN_TAG: ChainTag = ChainTag::Polkadot;
}

impl CryptoScheme for PolkadotCryptoScheme {
	type Point = Point;
	type Signature = PolkadotSignature;
	type PublicKey = schnorrkel::PublicKey;
	type SigningPayload = SigningPayload;
	const CRYPTO_TAG: CryptoTag = CryptoTag::Polkadot;
	const NAME: &'static str = "Polkadot Crypto";

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
		_group_commitment: &Self::Point,
		challenge: &<Self::Point as ECPoint>::Scalar,
		signature_response: &<Self::Point as ECPoint>::Scalar,
	) -> bool {
		Point::from_scalar(signature_response) == *commitment + (*y_i) * challenge * lambda_i
	}

	fn verify_signature(
		signature: &Self::Signature,
		public_key: &Self::PublicKey,
		payload: &Self::SigningPayload,
	) -> anyhow::Result<()> {
		let context = schnorrkel::signing_context(SIGNING_CTX);

		public_key
			.verify(context.bytes(payload.0.as_slice()), &signature.0)
			.map_err(anyhow::Error::msg)
			.context("Failed to verify Polkadot signature.")
	}

	fn pubkey_from_point(pubkey_point: &Self::Point) -> Self::PublicKey {
		schnorrkel::PublicKey::from_point(pubkey_point.get_element())
	}

	fn build_response(
		nonce: <Self::Point as super::ECPoint>::Scalar,
		_nonce_commitment: Self::Point,
		private_key: &<Self::Point as super::ECPoint>::Scalar,
		challenge: <Self::Point as super::ECPoint>::Scalar,
	) -> <Self::Point as super::ECPoint>::Scalar {
		// "Response" is computed as done in schnorrkel
		challenge * private_key + nonce
	}

	#[cfg(feature = "test")]
	fn signing_payload_for_test() -> Self::SigningPayload {
		SigningPayload::new(vec![1_u8; 256]).unwrap()
	}
}

// Check that our signature generation results in
// signatures deemed valid by schnorrkel verification code
#[test]
fn signature_should_be_valid() {
	use super::{curve25519::Scalar, ECPoint, ECScalar};
	use crate::crypto::Rng;
	use rand::SeedableRng;
	use utilities::assert_ok;

	let mut rng = Rng::from_seed([0; 32]);

	// Generate a key pair
	let secret_key = Scalar::random(&mut rng);
	let public_key = Point::from_scalar(&secret_key);

	// Message to sign
	let payload = PolkadotCryptoScheme::signing_payload_for_test();

	let signature = {
		// Pick random nonce and commit to it
		let nonce = Scalar::random(&mut rng);
		let nonce_commitment = Point::from_scalar(&nonce);

		let challenge =
			PolkadotCryptoScheme::build_challenge(public_key, nonce_commitment, &payload);

		let response =
			PolkadotCryptoScheme::build_response(nonce, nonce_commitment, &secret_key, challenge);

		PolkadotCryptoScheme::build_signature(response, nonce_commitment)
	};

	assert_ok!(schnorrkel::PublicKey::from_point(public_key.get_element()).verify_simple(
		SIGNING_CTX,
		&payload.0,
		&signature.0
	));
}
