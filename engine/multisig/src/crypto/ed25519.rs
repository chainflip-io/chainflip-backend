use super::{
	curve25519::edwards::Point, CanonicalEncoding, ChainSigning, ChainTag, CryptoScheme, CryptoTag,
	ECPoint,
};
use cf_chains::Chain;
use ed25519_consensus::VerificationKeyBytes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct Ed25519Signing {}

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

impl CanonicalEncoding for VerificationKeyBytes {
	fn encode_key(&self) -> Vec<u8> {
		self.to_bytes().to_vec()
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
#[derive(Clone, Debug, PartialEq)]
pub struct Ed25519CryptoScheme;

impl ChainSigning for Ed25519Signing {
	type CryptoScheme = Ed25519CryptoScheme;
	// This scheme isn't implemented on the state chain.
	type ChainCrypto = <cf_chains::none::NoneChain as Chain>::ChainCrypto;

	const NAME: &'static str = "Ed25519";

	// TODO: Technically the same "scheme" can be used by
	// multiple chains, so we might want to decouple
	// "scheme" from "chain".
	const CHAIN_TAG: ChainTag = ChainTag::Ed25519;
}

impl CryptoScheme for Ed25519CryptoScheme {
	type Point = super::curve25519::edwards::Point;

	type Signature = Signature;

	type PublicKey = VerificationKeyBytes;

	type SigningPayload = SigningPayload;

	const CRYPTO_TAG: CryptoTag = CryptoTag::Ed25519;

	const NAME: &'static str = "Ed25519 Crypto";

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
		use ed25519_consensus::VerificationKey;

		let signature = ed25519_consensus::Signature::from(signature.to_bytes());

		Ok(VerificationKey::try_from(*public_key)
			.and_then(|vk| vk.verify(&signature, &payload.0))?)
	}

	fn pubkey_from_point(pubkey_point: &Self::Point) -> Self::PublicKey {
		let bytes: [u8; 32] = pubkey_point.as_bytes().into();
		VerificationKeyBytes::from(bytes)
	}

	#[cfg(feature = "test")]
	fn signing_payload_for_test() -> Self::SigningPayload {
		SigningPayload([0u8; 32].to_vec())
	}
}
