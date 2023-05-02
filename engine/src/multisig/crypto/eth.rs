use crate::multisig::crypto::ECScalar;

use super::{CanonicalEncoding, ChainTag, CryptoScheme, ECPoint, SignatureToThresholdSignature};

// NOTE: for now, we re-export these to make it
// clear that these a the primitives used by ethereum.
// TODO: we probably want to change the "clients" to
// solely use "CryptoScheme" as generic parameter instead.
pub use super::secp256k1::{Point, Scalar};
use cf_chains::{ChainCrypto, Ethereum};
use cf_primitives::{EpochIndex, KeyId};
use num_bigint::BigUint;
use secp256k1::constants::CURVE_ORDER;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthSchnorrSignature {
	/// Scalar component
	pub s: [u8; 32],
	/// Point component (commitment)
	pub r: secp256k1::PublicKey,
}

impl From<EthSchnorrSignature> for cf_chains::eth::SchnorrVerificationComponents {
	fn from(cfe_sig: EthSchnorrSignature) -> Self {
		use crate::eth::utils::pubkey_to_eth_addr;

		Self { s: cfe_sig.s, k_times_g_address: pubkey_to_eth_addr(cfe_sig.r) }
	}
}

impl SignatureToThresholdSignature<Ethereum> for Vec<EthSchnorrSignature> {
	fn to_threshold_signature(&self) -> <Ethereum as ChainCrypto>::ThresholdSignature {
		self.iter()
			.map(|s| s.clone().into())
			.next()
			.expect("Exactly one signature for Ethereum")
	}
}

/// Ethereum crypto scheme (as defined by the Key Manager contract)
#[derive(Clone, Debug, PartialEq)]
pub struct EthSigning {}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct SigningPayload(pub [u8; 32]);

impl std::fmt::Display for SigningPayload {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", hex::encode(self.0))
	}
}

impl AsRef<[u8]> for SigningPayload {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

impl CanonicalEncoding for AggKey {
	fn encode_key(&self) -> Vec<u8> {
		let mut res = Vec::with_capacity(33);
		res.extend_from_slice(&self.to_pubkey_compressed());
		res
	}
}

impl CryptoScheme for EthSigning {
	type Point = Point;
	type Signature = EthSchnorrSignature;
	type PublicKey = cf_chains::eth::AggKey;
	type SigningPayload = SigningPayload;
	type Chain = cf_chains::Ethereum;

	const NAME: &'static str = "Ethereum";
	const CHAIN_TAG: ChainTag = ChainTag::Ethereum;

	fn build_signature(z: Scalar, group_commitment: Self::Point) -> Self::Signature {
		EthSchnorrSignature { s: *z.as_bytes(), r: group_commitment.get_element() }
	}

	/// Assembles and hashes the challenge in the correct order for the KeyManager Contract
	fn build_challenge(
		pubkey: Self::Point,
		nonce_commitment: Self::Point,
		payload: &Self::SigningPayload,
	) -> Scalar {
		use crate::eth::utils::pubkey_to_eth_addr;
		use cf_chains::eth::AggKey;

		let e = AggKey::from_pubkey_compressed(pubkey.get_element().serialize())
			.message_challenge(&payload.0, &pubkey_to_eth_addr(nonce_commitment.get_element()));

		Scalar::from_bytes_mod_order(&e)
	}

	fn build_response(
		nonce: <Self::Point as ECPoint>::Scalar,
		_nonce_commitment: Self::Point,
		private_key: &<Self::Point as ECPoint>::Scalar,
		challenge: <Self::Point as ECPoint>::Scalar,
	) -> <Self::Point as ECPoint>::Scalar {
		nonce - challenge * private_key
	}

	fn is_party_response_valid(
		y_i: &Self::Point,
		lambda_i: &<Self::Point as ECPoint>::Scalar,
		commitment: &Self::Point,
		_group_commitment: &Self::Point,
		challenge: &<Self::Point as ECPoint>::Scalar,
		signature_response: &<Self::Point as ECPoint>::Scalar,
	) -> bool {
		Point::from_scalar(signature_response) == *commitment - (*y_i) * challenge * lambda_i
	}

	fn verify_signature(
		signature: &Self::Signature,
		public_key: &Self::PublicKey,
		payload: &Self::SigningPayload,
	) -> anyhow::Result<()> {
		let x = BigUint::from_bytes_be(&public_key.pub_key_x);
		let half_order = BigUint::from_bytes_be(&CURVE_ORDER) / 2u32 + 1u32;
		assert!(x < half_order);

		public_key
			.verify(&payload.0, &signature.clone().into())
			.map_err(|e| anyhow::anyhow!("Failed to verify signature: {:?}", e))?;

		Ok(())
	}

	/// Check if the public key's x coordinate is smaller than "half secp256k1's order",
	/// which is a requirement imposed by the Key Manager contract.
	fn is_pubkey_compatible(pubkey: &Self::Point) -> bool {
		let x = BigUint::from_bytes_be(&pubkey.x_bytes());
		let half_order = BigUint::from_bytes_be(&CURVE_ORDER) / 2u32 + 1u32;

		x < half_order
	}

	fn pubkey_from_point(point: Self::Point) -> Self::PublicKey {
		cf_chains::eth::AggKey {
			pub_key_x: point.x_bytes(),
			pub_key_y_parity: if point.is_even_y() {
				cf_chains::eth::ParityBit::Even
			} else {
				cf_chains::eth::ParityBit::Odd
			},
		}
	}

	#[cfg(test)]
	fn signing_payload_for_test() -> Self::SigningPayload {
		SigningPayload("Chainflip:Chainflip:Chainflip:01".as_bytes().try_into().unwrap())
	}
}
