//! WebAuthn (FIDO2) verification for the Chainflip runtime.
//!
//! Only ES256 (ECDSA P-256 + SHA-256) credentials are supported. `clientDataJSON` is checked with
//! the WebAuthn *limited verification algorithm* (a byte-prefix match) so that no JSON parser is
//! needed in the runtime.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod attestation;
pub mod signature;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_authenticator;

use alloc::vec::Vec;
use base64ct::{Base64UrlUnpadded, Encoding};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use scale_info::TypeInfo;
use sha2::{Digest, Sha256};
use sp_core::{ConstU32, RuntimeDebug};
use sp_runtime::{AccountId32, BoundedVec};

/// Compressed SEC1 P-256 public key.
pub type PublicKey = [u8; 33];

pub const MAX_AUTHENTICATOR_DATA_LEN: u32 = 256;
pub const MAX_CLIENT_DATA_JSON_LEN: u32 = 512;
pub const MAX_CREDENTIAL_ID_LEN: u32 = 1023;

/// Domain separation so a P-256 key can never map to the same account as a secp256k1 key with
/// identical SEC1 bytes.
const ACCOUNT_ID_DOMAIN: &[u8] = b"chainflip/webauthn-p256";

const FLAG_USER_PRESENT: u8 = 0x01;
const FLAG_USER_VERIFIED: u8 = 0x04;
const FLAG_BACKUP_ELIGIBLE: u8 = 0x08;
const FLAG_BACKUP_STATE: u8 = 0x10;
const FLAG_ATTESTED_CREDENTIAL_DATA: u8 = 0x40;

#[derive(Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub enum Error {
	MalformedAuthenticatorData,
	MalformedClientData,
	ChallengeMismatch,
	RpIdMismatch,
	UserNotPresent,
	UserNotVerified,
	BackupEligible,
	InvalidPublicKey,
	InvalidSignature,
	MalformedAttestation,
	UnsupportedAttestationFormat,
	UnsupportedAlgorithm,
	MalformedCertificate,
	AaguidMismatch,
	UntrustedIssuer,
	InvalidCertificateSignature,
}

/// What a relying party requires of a ceremony, beyond a valid signature over the challenge.
#[derive(Clone, Copy, Default, RuntimeDebug)]
pub struct Policy {
	/// `sha256(rp_id)`. When `None` the RP id is not checked; the authenticator already scopes
	/// credentials to the RP id they were created for.
	pub rp_id_hash: Option<[u8; 32]>,
	pub require_user_verification: bool,
	/// Reject credentials that may be synced off the authenticator (e.g. platform passkeys).
	pub reject_backup_eligible: bool,
}

/// The parts of a `navigator.credentials.get()` response needed to verify it.
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
pub struct Assertion {
	pub authenticator_data: BoundedVec<u8, ConstU32<MAX_AUTHENTICATOR_DATA_LEN>>,
	pub client_data_json: BoundedVec<u8, ConstU32<MAX_CLIENT_DATA_JSON_LEN>>,
	/// Fixed-width `r || s`. Browsers return DER, which clients convert.
	pub signature: [u8; 64],
}

pub fn account_id(public_key: &PublicKey) -> AccountId32 {
	sp_io::hashing::blake2_256(&[ACCOUNT_ID_DOMAIN, public_key.as_slice()].concat()).into()
}

pub fn rp_id_hash(rp_id: &[u8]) -> [u8; 32] {
	Sha256::digest(rp_id).into()
}

/// Parsed fixed-size prefix of `authenticatorData`.
#[derive(Clone, Copy, RuntimeDebug)]
pub(crate) struct AuthenticatorData {
	pub rp_id_hash: [u8; 32],
	pub flags: u8,
}

impl AuthenticatorData {
	/// rpIdHash (32) || flags (1) || signCount (4)
	const MIN_LEN: usize = 37;

	pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
		if bytes.len() < Self::MIN_LEN {
			return Err(Error::MalformedAuthenticatorData)
		}
		let (rp_id_hash, rest) = bytes.split_at(32);
		Ok(Self {
			rp_id_hash: rp_id_hash.try_into().map_err(|_| Error::MalformedAuthenticatorData)?,
			flags: *rest.first().ok_or(Error::MalformedAuthenticatorData)?,
		})
	}

	pub fn check(&self, policy: &Policy) -> Result<(), Error> {
		if policy.rp_id_hash.is_some_and(|expected| expected != self.rp_id_hash) {
			return Err(Error::RpIdMismatch)
		}
		if self.flags & FLAG_USER_PRESENT == 0 {
			return Err(Error::UserNotPresent)
		}
		if policy.require_user_verification && self.flags & FLAG_USER_VERIFIED == 0 {
			return Err(Error::UserNotVerified)
		}
		if policy.reject_backup_eligible &&
			self.flags & (FLAG_BACKUP_ELIGIBLE | FLAG_BACKUP_STATE) != 0
		{
			return Err(Error::BackupEligible)
		}
		Ok(())
	}

	pub fn has_attested_credential_data(&self) -> bool {
		self.flags & FLAG_ATTESTED_CREDENTIAL_DATA != 0
	}
}

#[derive(Clone, Copy)]
pub(crate) enum CeremonyType {
	Create,
	Get,
}

/// WebAuthn L3 §5.8.1.1 limited verification: the serialisation of `CollectedClientData` is
/// specified to begin with `type` then `challenge`, so a prefix match is sufficient.
pub(crate) fn check_client_data(
	client_data_json: &[u8],
	ceremony: CeremonyType,
	challenge: &[u8],
) -> Result<(), Error> {
	let type_prefix: &[u8] = match ceremony {
		CeremonyType::Create => br#"{"type":"webauthn.create","challenge":""#,
		CeremonyType::Get => br#"{"type":"webauthn.get","challenge":""#,
	};
	let rest = client_data_json.strip_prefix(type_prefix).ok_or(Error::MalformedClientData)?;
	let encoded_challenge = Base64UrlUnpadded::encode_string(challenge);
	rest.strip_prefix(encoded_challenge.as_bytes())
		.and_then(|rest| rest.strip_prefix(b"\""))
		.ok_or(Error::ChallengeMismatch)?;
	Ok(())
}

/// The message a WebAuthn signature covers: `authenticatorData || sha256(clientDataJSON)`.
pub(crate) fn signed_message(authenticator_data: &[u8], client_data_json: &[u8]) -> Vec<u8> {
	[authenticator_data, Sha256::digest(client_data_json).as_slice()].concat()
}

/// Verifies that `assertion` is a valid WebAuthn signature over `challenge` by `public_key`.
pub fn verify_assertion(
	public_key: &PublicKey,
	challenge: &[u8],
	assertion: &Assertion,
	policy: &Policy,
) -> Result<(), Error> {
	AuthenticatorData::parse(&assertion.authenticator_data)?.check(policy)?;
	check_client_data(&assertion.client_data_json, CeremonyType::Get, challenge)?;
	let key = VerifyingKey::from_sec1_bytes(public_key).map_err(|_| Error::InvalidPublicKey)?;
	let signature =
		Signature::from_slice(&assertion.signature).map_err(|_| Error::InvalidSignature)?;
	key.verify(
		&signed_message(&assertion.authenticator_data, &assertion.client_data_json),
		&signature,
	)
	.map_err(|_| Error::InvalidSignature)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_authenticator::TestAuthenticator;

	const RP_ID: &[u8] = b"localhost";

	fn policy() -> Policy {
		Policy { rp_id_hash: Some(rp_id_hash(RP_ID)), ..Default::default() }
	}

	#[test]
	fn software_assertion_round_trip() {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let assertion = authenticator.sign(b"some challenge");
		assert_eq!(
			verify_assertion(&authenticator.public_key(), b"some challenge", &assertion, &policy()),
			Ok(())
		);
	}

	#[test]
	fn rejects_wrong_challenge() {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let assertion = authenticator.sign(b"some challenge");
		assert_eq!(
			verify_assertion(&authenticator.public_key(), b"other", &assertion, &policy()),
			Err(Error::ChallengeMismatch)
		);
	}

	#[test]
	fn rejects_challenge_prefix() {
		// base64url of a prefix of the challenge is a prefix of the encoded challenge.
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let assertion = authenticator.sign(b"some challenge!");
		assert_eq!(
			verify_assertion(&authenticator.public_key(), b"some chal", &assertion, &policy()),
			Err(Error::ChallengeMismatch)
		);
	}

	#[test]
	fn rejects_wrong_key() {
		let assertion = TestAuthenticator::new(1, RP_ID).sign(b"c");
		assert_eq!(
			verify_assertion(
				&TestAuthenticator::new(2, RP_ID).public_key(),
				b"c",
				&assertion,
				&policy()
			),
			Err(Error::InvalidSignature)
		);
	}

	#[test]
	fn rejects_wrong_rp_id() {
		let authenticator = TestAuthenticator::new(1, b"evil.example");
		let assertion = authenticator.sign(b"c");
		assert_eq!(
			verify_assertion(&authenticator.public_key(), b"c", &assertion, &policy()),
			Err(Error::RpIdMismatch)
		);
	}

	#[test]
	fn rejects_create_ceremony_client_data() {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let mut assertion = authenticator.sign(b"c");
		assertion.client_data_json =
			authenticator.client_data_json("webauthn.create", b"c").try_into().unwrap();
		assert_eq!(
			verify_assertion(&authenticator.public_key(), b"c", &assertion, &policy()),
			Err(Error::MalformedClientData)
		);
	}

	#[test]
	fn enforces_flags_policy() {
		let authenticator = TestAuthenticator::new(1, RP_ID).with_flags(FLAG_USER_PRESENT);
		let assertion = authenticator.sign(b"c");
		assert_eq!(
			verify_assertion(
				&authenticator.public_key(),
				b"c",
				&assertion,
				&Policy { require_user_verification: true, ..policy() }
			),
			Err(Error::UserNotVerified)
		);

		let synced = TestAuthenticator::new(1, RP_ID)
			.with_flags(FLAG_USER_PRESENT | FLAG_BACKUP_ELIGIBLE | FLAG_BACKUP_STATE);
		assert_eq!(
			verify_assertion(
				&synced.public_key(),
				b"c",
				&synced.sign(b"c"),
				&Policy { reject_backup_eligible: true, ..policy() }
			),
			Err(Error::BackupEligible)
		);
	}
}

#[cfg(test)]
mod timing {
	use super::*;
	use crate::test_authenticator::TestAuthenticator;

	/// Native lower bound only; the runtime runs this as WASM with no host function.
	/// `cargo test --release -p cf-webauthn native_verify_timing -- --ignored --nocapture`
	#[test]
	#[ignore]
	fn native_verify_timing() {
		const N: u32 = 1000;
		let authenticator = TestAuthenticator::new(1, b"localhost");
		let public_key = authenticator.public_key();
		let assertion = authenticator.sign(&[7; 32]);
		let start = std::time::Instant::now();
		for _ in 0..N {
			verify_assertion(&public_key, &[7; 32], &assertion, &Policy::default()).unwrap();
		}
		println!("verify_assertion: {:?} per call", start.elapsed() / N);
	}
}
