pub use super::secp256k1::{Point, Scalar};
use super::{
	ChainSigning, ChainTag, CryptoScheme, CryptoTag, ECPoint, SignatureToThresholdSignature,
};
use crate::crypto::ECScalar;
use anyhow::Context;
use cf_chains::{Bitcoin, Chain, ChainCrypto};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// SHA256("BIP0340/challenge")
// See also https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki
const CHALLENGE_TAG: [u8; 32] = [
	0x7b, 0xb5, 0x2d, 0x7a, 0x9f, 0xef, 0x58, 0x32, 0x3e, 0xb1, 0xbf, 0x7a, 0x40, 0x7d, 0xb3, 0x82,
	0xd2, 0xf3, 0xf2, 0xd8, 0x1b, 0xb1, 0x22, 0x4f, 0x49, 0xfe, 0x51, 0x8f, 0x6d, 0x48, 0xd3, 0x7c,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtcSchnorrSignature {
	pub r: Point,
	pub s: Scalar,
}

impl BtcSchnorrSignature {
	// Bitcoin represents Schnorr signatures as a raw set of 64 bytes
	// The first 32 are the x-component of R, the next 32 are used for s.
	fn to_raw(&self) -> [u8; 64] {
		let mut result: [u8; 64] = [0; 64];
		result[..32].copy_from_slice(&self.r.x_bytes());
		result[32..].copy_from_slice(self.s.as_bytes());
		result
	}
}

impl SignatureToThresholdSignature<<Bitcoin as Chain>::ChainCrypto> for Vec<BtcSchnorrSignature> {
	fn to_threshold_signature(
		&self,
	) -> <<Bitcoin as Chain>::ChainCrypto as ChainCrypto>::ThresholdSignature {
		self.iter().map(|s| s.to_raw()).collect()
	}
}

#[derive(Clone, Debug, PartialEq)]
pub struct BtcSigning {}

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

#[derive(Clone, Debug, PartialEq)]
/// Bitcoin crypto scheme (as defined by BIP 340)
pub struct BtcCryptoScheme;

impl ChainSigning for BtcSigning {
	type CryptoScheme = BtcCryptoScheme;
	type ChainCrypto = <Bitcoin as Chain>::ChainCrypto;
	const NAME: &'static str = "Bitcoin";
	const CHAIN_TAG: ChainTag = ChainTag::Bitcoin;

	/// The window is smaller for bitcoin because its block time is a lot longer and it supports
	/// multiple signing payloads
	const CEREMONY_ID_WINDOW: u64 = 50;
}

impl CryptoScheme for BtcCryptoScheme {
	type Point = Point;
	type Signature = BtcSchnorrSignature;
	type PublicKey = secp256k1::XOnlyPublicKey;
	type SigningPayload = SigningPayload;
	const CRYPTO_TAG: CryptoTag = CryptoTag::Bitcoin;
	const NAME: &'static str = "Bitcoin Crypto";

	fn build_signature(z: Scalar, group_commitment: Self::Point) -> Self::Signature {
		BtcSchnorrSignature { s: z, r: group_commitment }
	}

	fn build_challenge(
		pubkey: Self::Point,
		nonce_commitment: Self::Point,
		payload: &Self::SigningPayload,
	) -> Scalar {
		let mut hasher = Sha256::new();
		hasher.update(CHALLENGE_TAG);
		hasher.update(CHALLENGE_TAG);
		hasher.update(nonce_commitment.x_bytes());
		hasher.update(pubkey.x_bytes());
		hasher.update(payload.0);
		ECScalar::from_bytes_mod_order(&hasher.finalize().into())
	}

	fn build_response(
		nonce: <Self::Point as ECPoint>::Scalar,
		nonce_commitment: Self::Point,
		private_key: &<Self::Point as ECPoint>::Scalar,
		challenge: <Self::Point as ECPoint>::Scalar,
	) -> <Self::Point as ECPoint>::Scalar {
		if nonce_commitment.is_even_y() {
			private_key * &challenge + nonce
		} else {
			private_key * &challenge - nonce
		}
	}

	fn is_party_response_valid(
		y_i: &Self::Point,
		lambda_i: &<Self::Point as ECPoint>::Scalar,
		commitment: &Self::Point,
		group_commitment: &Self::Point,
		challenge: &<Self::Point as ECPoint>::Scalar,
		signature_response: &<Self::Point as ECPoint>::Scalar,
	) -> bool {
		if group_commitment.is_even_y() {
			Point::from_scalar(signature_response) == (*y_i) * challenge * lambda_i + *commitment
		} else {
			Point::from_scalar(signature_response) == (*y_i) * challenge * lambda_i - *commitment
		}
	}

	fn verify_signature(
		signature: &Self::Signature,
		public_key: &Self::PublicKey,
		payload: &Self::SigningPayload,
	) -> anyhow::Result<()> {
		let secp = secp256k1::Secp256k1::new();
		let raw_sig = secp256k1::schnorr::Signature::from_slice(&signature.to_raw()).unwrap();
		let raw_msg = secp256k1::Message::from_slice(&payload.0).unwrap();

		secp.verify_schnorr(&raw_sig, &raw_msg, public_key)
			.context("Failed to verify Bitcoin signature")?;
		Ok(())
	}

	fn pubkey_from_point(pubkey_point: &Self::Point) -> Self::PublicKey {
		secp256k1::XOnlyPublicKey::from_slice(&pubkey_point.x_bytes())
			.expect("from_slice expects 32 byte x coordinate.")
	}

	fn is_pubkey_compatible(pubkey: &Self::Point) -> bool {
		pubkey.is_even_y()
	}

	#[cfg(feature = "test")]
	fn signing_payload_for_test() -> Self::SigningPayload {
		SigningPayload(Sha256::digest(b"Chainflip:Chainflip:Chainflip:01").into())
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_sig_verification() {
		// These are some random values fed through a reference implementation for bitcoin signing
		// to test that our verification works correctly
		type BtcCryptoScheme = <BtcSigning as ChainSigning>::CryptoScheme;
		let r = Point::from_scalar(&Scalar::from_hex(
			"8F78522655F02F46F55103BC6EE2242E04553DAA65BF18D0E329EC6B49FD3788",
		));
		let s =
			Scalar::from_hex("ED7A468DBE45823D91CC1276F9E9F1DD3A1DB8E4C9EFE8F5DBA43B63E4C02FAD");
		let signature = BtcCryptoScheme::build_signature(s, r);
		let pubkey = secp256k1::XOnlyPublicKey::from_slice(
			hex::decode("59B2B46FB182A6D4B39FFB7A29D0B67851DDE2433683BE6D46623A7960D2799E")
				.unwrap()
				.as_slice(),
		)
		.unwrap();
		assert!(BtcCryptoScheme::verify_signature(
			&signature,
			&pubkey,
			&BtcCryptoScheme::signing_payload_for_test()
		)
		.is_ok());
	}

	#[test]
	fn test_btcsig_to_raw() {
		// These are some random values that we can use to see that the "sig.to_raw()" function
		// works as expected
		let s =
			Scalar::from_hex("626FC96FF3678D4FA2DE960B2C39D199747D3F47F01508FBBE24825C4D11B543");
		let r = Point::from_scalar(&s);
		let sig = BtcCryptoScheme::build_signature(s, r);
		assert_eq!(
			sig.to_raw(),
			[
				0x59, 0xb2, 0xb4, 0x6f, 0xb1, 0x82, 0xa6, 0xd4, 0xb3, 0x9f, 0xfb, 0x7a, 0x29, 0xd0,
				0xb6, 0x78, 0x51, 0xdd, 0xe2, 0x43, 0x36, 0x83, 0xbe, 0x6d, 0x46, 0x62, 0x3a, 0x79,
				0x60, 0xd2, 0x79, 0x9e, 0x62, 0x6f, 0xc9, 0x6f, 0xf3, 0x67, 0x8d, 0x4f, 0xa2, 0xde,
				0x96, 0x0b, 0x2c, 0x39, 0xd1, 0x99, 0x74, 0x7d, 0x3f, 0x47, 0xf0, 0x15, 0x08, 0xfb,
				0xbe, 0x24, 0x82, 0x5c, 0x4d, 0x11, 0xb5, 0x43
			]
		);
	}

	#[test]
	fn test_challenge() {
		// Again some random values that were used in a reference implementation
		// so that we can be sure the build_challenge method works as expected
		let public = Point::from_scalar(&Scalar::from_hex(
			"626FC96FF3678D4FA2DE960B2C39D199747D3F47F01508FBBE24825C4D11B543",
		));
		let commitment = Point::from_scalar(&Scalar::from_hex(
			"EB3F18E13AEFBF7AC9347F38B6E5D5576848B4E7927F6233222BA9286BB24F31",
		));
		assert_eq!(
			BtcCryptoScheme::build_challenge(
				public,
				commitment,
				&BtcCryptoScheme::signing_payload_for_test()
			),
			Scalar::from_hex("1FCA6ED81348426626DA247A3B0810F61EA46C592442F81FC9DFFDB43ABBE439")
		);
	}
}
