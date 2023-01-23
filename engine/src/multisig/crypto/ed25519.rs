use super::{curve25519::edwards::Point, ChainTag, CryptoScheme, ECPoint};
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

pub struct AggKey(VerificationKeyBytes);

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

impl CryptoScheme for Ed25519Signing {
	type Point = super::curve25519::edwards::Point;

	type Signature = Signature;

	type AggKey = AggKey;

	type SigningPayload = SigningPayload;

	const NAME: &'static str = "Ed25519";

	// TODO: Technically the same "scheme" can be used by
	// multiple chains, so we might want to decouple
	// "scheme" from "chain".
	const CHAIN_TAG: ChainTag = ChainTag::Sui;

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

		use crate::multisig::crypto::curve25519::Scalar;

		Scalar(curve25519_dalek::scalar::Scalar::from_bytes_mod_order_wide(&output))
	}

	fn build_response(
		nonce: <Self::Point as super::ECPoint>::Scalar,
		private_key: &<Self::Point as super::ECPoint>::Scalar,
		challenge: <Self::Point as super::ECPoint>::Scalar,
	) -> <Self::Point as super::ECPoint>::Scalar {
		challenge * private_key + nonce
	}

	fn is_party_response_valid(
		y_i: &Self::Point,
		lambda_i: &<Self::Point as super::ECPoint>::Scalar,
		commitment: &Self::Point,
		challenge: &<Self::Point as super::ECPoint>::Scalar,
		signature_response: &<Self::Point as super::ECPoint>::Scalar,
	) -> bool {
		Point::from_scalar(signature_response) == *commitment + (*y_i) * challenge * lambda_i
	}

	fn verify_signature(
		signature: &Self::Signature,
		key_id: &crate::multisig::KeyId,
		payload: &Self::SigningPayload,
	) -> anyhow::Result<()> {
		use anyhow::anyhow;
		use ed25519_consensus::VerificationKey;

		let key_bytes: [u8; 32] = key_id
			.0
			.clone()
			.try_into()
			.map_err(|_| anyhow!("Invalid Key length: {}", key_id.0.len()))?;

		let key = VerificationKey::try_from(key_bytes)
			.map_err(|_| anyhow::anyhow!("Invalid key encoding"))?;

		let signature = ed25519_consensus::Signature::from(signature.to_bytes());

		key.verify(&signature, &payload.0).map_err(|_| anyhow!("Invalid signature"))
	}

	fn agg_key(pubkey: &Self::Point) -> Self::AggKey {
		let bytes: [u8; 32] = pubkey.as_bytes().into();
		AggKey(VerificationKeyBytes::from(bytes))
	}

	#[cfg(test)]
	fn signing_payload_for_test() -> Self::SigningPayload {
		SigningPayload([0u8; 32].to_vec())
	}
}
