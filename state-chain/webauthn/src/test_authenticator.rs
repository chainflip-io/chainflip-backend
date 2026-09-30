//! A software authenticator producing browser-shaped assertions, for tests and benchmarks.
use crate::{signed_message, Assertion, PublicKey, FLAG_USER_PRESENT, FLAG_USER_VERIFIED};
use alloc::{format, vec::Vec};
use base64ct::{Base64UrlUnpadded, Encoding};
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use sha2::{Digest, Sha256};

pub struct TestAuthenticator {
	key: SigningKey,
	rp_id: Vec<u8>,
	flags: u8,
}

impl TestAuthenticator {
	pub fn new(seed: u64, rp_id: &[u8]) -> Self {
		let secret: [u8; 32] = Sha256::digest(seed.to_le_bytes()).into();
		Self {
			key: SigningKey::from_slice(&secret).expect("sha256 output is a valid scalar"),
			rp_id: rp_id.to_vec(),
			flags: FLAG_USER_PRESENT | FLAG_USER_VERIFIED,
		}
	}

	pub fn with_flags(self, flags: u8) -> Self {
		Self { flags, ..self }
	}

	pub fn public_key(&self) -> PublicKey {
		self.key
			.verifying_key()
			.to_encoded_point(true)
			.as_bytes()
			.try_into()
			.expect("compressed P-256 point is 33 bytes")
	}

	pub fn client_data_json(&self, ceremony: &str, challenge: &[u8]) -> Vec<u8> {
		format!(
			r#"{{"type":"{ceremony}","challenge":"{}","origin":"http://{}","crossOrigin":false}}"#,
			Base64UrlUnpadded::encode_string(challenge),
			core::str::from_utf8(&self.rp_id).unwrap_or_default(),
		)
		.into_bytes()
	}

	pub fn sign(&self, challenge: &[u8]) -> Assertion {
		let authenticator_data =
			[Sha256::digest(&self.rp_id).as_slice(), &[self.flags], &1u32.to_be_bytes()].concat();
		let client_data_json = self.client_data_json("webauthn.get", challenge);
		let signature: Signature =
			self.key.sign(&signed_message(&authenticator_data, &client_data_json));
		Assertion {
			authenticator_data: authenticator_data.try_into().expect("37 bytes"),
			client_data_json: client_data_json.try_into().expect("within bound"),
			signature: signature.to_bytes().into(),
		}
	}
}
