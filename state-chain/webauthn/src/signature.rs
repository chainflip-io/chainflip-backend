//! `MultiSignature` extended with a WebAuthn variant, for use as the runtime's extrinsic signature
//! type.
//!
//! The first three variants encode identically to `sp_runtime::MultiSignature`, so existing
//! signers are unaffected.
use crate::{account_id, verify_assertion, Assertion, Policy, PublicKey};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::{ecdsa, ed25519, sr25519, RuntimeDebug};
use sp_runtime::{
	traits::{IdentifyAccount, Lazy, Verify},
	AccountId32, MultiSignature, MultiSigner,
};

#[derive(
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebug,
)]
pub struct WebAuthnSignature {
	/// Required because P-256 signatures don't support recovery of the public key, and the
	/// account id is a hash of it.
	pub public_key: PublicKey,
	pub assertion: Assertion,
}

#[derive(
	Clone,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
	RuntimeDebug,
)]
pub enum CfSignature {
	Ed25519(ed25519::Signature),
	Sr25519(sr25519::Signature),
	Ecdsa(ecdsa::Signature),
	WebAuthn(WebAuthnSignature),
}

#[derive(
	Clone,
	PartialEq,
	Eq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	RuntimeDebug,
)]
pub enum CfSigner {
	Ed25519(ed25519::Public),
	Sr25519(sr25519::Public),
	Ecdsa(ecdsa::Public),
	WebAuthn(PublicKey),
}

/// The WebAuthn challenge for a signed extrinsic payload. Hashing keeps `clientDataJSON` short
/// and bounded regardless of payload size.
pub fn challenge(payload: &[u8]) -> [u8; 32] {
	sp_io::hashing::blake2_256(payload)
}

impl Verify for CfSignature {
	type Signer = CfSigner;

	fn verify<L: Lazy<[u8]>>(&self, mut msg: L, signer: &AccountId32) -> bool {
		let delegate = |sig: MultiSignature, msg: L| sig.verify(msg, signer);
		match self {
			Self::Ed25519(sig) => delegate(MultiSignature::Ed25519(*sig), msg),
			Self::Sr25519(sig) => delegate(MultiSignature::Sr25519(*sig), msg),
			Self::Ecdsa(sig) => delegate(MultiSignature::Ecdsa(*sig), msg),
			// Checked before the (pure-WASM, no host function) P-256 verification.
			Self::WebAuthn(sig) if account_id(&sig.public_key) != *signer => false,
			// No storage access here, so no RP id pinning, UV or backup-eligibility policy.
			Self::WebAuthn(WebAuthnSignature { public_key, assertion }) =>
				verify_assertion(public_key, &challenge(msg.get()), assertion, &Policy::default())
					.is_ok(),
		}
	}
}

impl IdentifyAccount for CfSigner {
	type AccountId = AccountId32;

	fn into_account(self) -> AccountId32 {
		match self {
			Self::Ed25519(public) => MultiSigner::Ed25519(public).into_account(),
			Self::Sr25519(public) => MultiSigner::Sr25519(public).into_account(),
			Self::Ecdsa(public) => MultiSigner::Ecdsa(public).into_account(),
			Self::WebAuthn(public_key) => account_id(&public_key),
		}
	}
}

impl From<ed25519::Public> for CfSigner {
	fn from(public: ed25519::Public) -> Self {
		Self::Ed25519(public)
	}
}

impl From<sr25519::Public> for CfSigner {
	fn from(public: sr25519::Public) -> Self {
		Self::Sr25519(public)
	}
}

impl From<ecdsa::Public> for CfSigner {
	fn from(public: ecdsa::Public) -> Self {
		Self::Ecdsa(public)
	}
}

impl From<MultiSignature> for CfSignature {
	fn from(signature: MultiSignature) -> Self {
		match signature {
			MultiSignature::Ed25519(sig) => Self::Ed25519(sig),
			MultiSignature::Sr25519(sig) => Self::Sr25519(sig),
			MultiSignature::Ecdsa(sig) => Self::Ecdsa(sig),
		}
	}
}

impl From<ed25519::Signature> for CfSignature {
	fn from(sig: ed25519::Signature) -> Self {
		Self::Ed25519(sig)
	}
}

impl From<sr25519::Signature> for CfSignature {
	fn from(sig: sr25519::Signature) -> Self {
		Self::Sr25519(sig)
	}
}

impl From<ecdsa::Signature> for CfSignature {
	fn from(sig: ecdsa::Signature) -> Self {
		Self::Ecdsa(sig)
	}
}

impl From<WebAuthnSignature> for CfSignature {
	fn from(sig: WebAuthnSignature) -> Self {
		Self::WebAuthn(sig)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_authenticator::TestAuthenticator;
	use sp_core::Pair;

	#[test]
	fn existing_variants_encode_like_multi_signature() {
		let sig = sp_core::sr25519::Pair::from_seed(&[1; 32]).sign(b"msg");
		assert_eq!(CfSignature::from(sig).encode(), MultiSignature::from(sig).encode());
		let sig = sp_core::ed25519::Pair::from_seed(&[1; 32]).sign(b"msg");
		assert_eq!(CfSignature::from(sig).encode(), MultiSignature::from(sig).encode());
	}

	#[test]
	fn verifies_webauthn_signature() {
		let authenticator = TestAuthenticator::new(7, b"localhost");
		let public_key = authenticator.public_key();
		let signature = CfSignature::from(WebAuthnSignature {
			public_key,
			assertion: authenticator.sign(&challenge(b"payload")),
		});
		assert!(signature.verify(&b"payload"[..], &account_id(&public_key)));
		assert!(!signature.verify(&b"other payload"[..], &account_id(&public_key)));
		assert!(!signature.verify(
			&b"payload"[..],
			&account_id(&TestAuthenticator::new(8, b"localhost").public_key())
		));
	}
}
