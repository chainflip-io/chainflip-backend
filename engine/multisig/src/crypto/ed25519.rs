use anyhow::Result;

use super::{
	curve25519::edwards::Point, ChainSigning, ChainTag, CryptoScheme, CryptoTag, ECPoint,
	SignatureToThresholdSignature,
};
use cf_chains::{sol::SolSignature, Chain, ChainCrypto, Solana};
use ed25519_dalek::{Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct SolSigning {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
	r: [u8; 32],
	s: [u8; 32],
}

impl Signature {
	pub fn to_bytes(&self) -> [u8; 64] {
		let mut bytes = [0u8; 64];
		bytes[..32].copy_from_slice(&self.r);
		bytes[32..].copy_from_slice(&self.s);
		bytes
	}
}

impl SignatureToThresholdSignature<<Solana as Chain>::ChainCrypto> for Vec<Signature> {
	fn to_threshold_signature(
		&self,
	) -> <<Solana as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature {
		self.iter()
			.map(|s| SolSignature(s.clone().to_bytes()))
			.next()
			.expect("Exactly one signature for Solana")
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
		if payload.is_empty() {
			anyhow::bail!("Invalid payload size");
		}
		Ok(SigningPayload(payload))
	}
}

#[derive(Clone, Debug, PartialEq)]
pub struct SolCryptoScheme;

impl ChainSigning for SolSigning {
	type CryptoScheme = SolCryptoScheme;
	// This scheme isn't implemented on the state chain.
	type ChainCrypto = <Solana as Chain>::ChainCrypto;

	const NAME: &'static str = "Solana";

	// TODO: Technically the same "scheme" can be used by
	// multiple chains, so we might want to decouple
	// "scheme" from "chain".
	const CHAIN_TAG: ChainTag = ChainTag::Solana;
}

impl CryptoScheme for SolCryptoScheme {
	type Point = super::curve25519::edwards::Point;

	type Signature = Signature;

	type PublicKey = VerifyingKey;

	type SigningPayload = SigningPayload;

	const CRYPTO_TAG: CryptoTag = CryptoTag::Solana;

	const NAME: &'static str = "Solana Crypto";

	fn build_signature(
		z: <Self::Point as super::ECPoint>::Scalar,
		group_commitment: Self::Point,
	) -> Self::Signature {
		Signature { r: group_commitment.as_bytes().into(), s: z.to_bytes() }
	}

	fn build_challenge(
		pubkey: Self::Point,
		nonce_commitment: Self::Point,
		payload: &Self::SigningPayload,
	) -> <Self::Point as super::ECPoint>::Scalar {
		use sha2::Digest;

		let hash = sha2::Sha512::default()
			.chain(nonce_commitment.as_bytes())
			.chain(pubkey.as_bytes())
			.chain(&payload.0);

		let mut output = [0u8; 64];
		output.copy_from_slice(hash.finalize().as_slice());

		use crate::crypto::curve25519::Scalar;

		Scalar(curve25519_dalek::scalar::Scalar::from_bytes_mod_order_wide(&output))
	}

	fn build_response(
		nonce: <Self::Point as super::ECPoint>::Scalar,
		_nonce_commitment: Self::Point,
		private_key: &<Self::Point as super::ECPoint>::Scalar,
		challenge: <Self::Point as super::ECPoint>::Scalar,
	) -> <Self::Point as super::ECPoint>::Scalar {
		challenge * private_key + nonce
	}

	fn is_party_response_valid(
		y_i: &Self::Point,
		lambda_i: &<Self::Point as super::ECPoint>::Scalar,
		commitment: &Self::Point,
		_group_commitment: &Self::Point,
		challenge: &<Self::Point as super::ECPoint>::Scalar,
		signature_response: &<Self::Point as super::ECPoint>::Scalar,
	) -> bool {
		Point::from_scalar(signature_response) == *commitment + (*y_i) * challenge * lambda_i
	}

	fn verify_signature(
		signature: &Self::Signature,
		public_key: &Self::PublicKey,
		payload: &Self::SigningPayload,
	) -> anyhow::Result<()> {
		let signature = ed25519_dalek::Signature::from_bytes(&signature.to_bytes());

		Ok(public_key.verify(&payload.0, &signature)?)
	}

	fn pubkey_from_point(pubkey_point: &Self::Point) -> Self::PublicKey {
		let bytes: [u8; 32] = pubkey_point.as_bytes().into();
		VerifyingKey::from_bytes(&bytes).expect("Invalid public key")
	}

	#[cfg(feature = "test")]
	fn signing_payload_for_test() -> Self::SigningPayload {
		SigningPayload([0u8; 32].to_vec())
	}
}

#[test]
fn test_signature_verification() {
	use crate::crypto::{curve25519::edwards::Point, ECScalar};
	use rand::{thread_rng, Rng};

	type Scalar = <Point as ECPoint>::Scalar;

	let message = b"payload";

	let secret_key = Scalar::from_bytes_mod_order(&thread_rng().gen());
	let public_key = Point::from_scalar(&secret_key);

	let payload = SigningPayload(message.to_vec());

	// Build a signature using the same primitives/operations as used in multisig ceremonies:
	let signature = {
		let nonce = Scalar::from_bytes_mod_order(&thread_rng().gen::<[u8; 32]>());
		let nonce_commitment = Point::from_scalar(&nonce);
		let challenge = SolCryptoScheme::build_challenge(public_key, nonce_commitment, &payload);

		let response =
			SolCryptoScheme::build_response(nonce, nonce_commitment, &secret_key, challenge);

		SolCryptoScheme::build_signature(response, nonce_commitment)
	};

	let verifying_key =
		ed25519_dalek::VerifyingKey::from_bytes(&public_key.as_bytes().into()).unwrap();

	// Verify the signature using the "reference" implementation (which in this case is
	// ed25519_dalek used by Solana):
	SolCryptoScheme::verify_signature(&signature, &verifying_key, &payload).unwrap();
}
