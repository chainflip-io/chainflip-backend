// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! Certificate generation for QUIC transport.
//!
//! Generates self-signed TLS certificates from Ed25519 keys. The certificate
//! embeds the Ed25519 public key in the Common Name for peer verification.

use ed25519_dalek::SigningKey;
use rcgen::{CertificateParams, DnType, KeyPair};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// TLS certificate and private key generated from an Ed25519 signing key.
pub struct CertificateIdentity {
	/// DER-encoded X.509 certificate
	pub cert_der: CertificateDer<'static>,
	/// DER-encoded PKCS#8 private key
	pub key_der: PrivateKeyDer<'static>,
	/// The Ed25519 public key bytes (for quick access)
	pub ed25519_pubkey: [u8; 32],
}

impl CertificateIdentity {
	/// Generate a self-signed TLS certificate from an Ed25519 signing key.
	///
	/// The certificate:
	/// - Uses Ed25519 for TLS 1.3 signatures
	/// - Embeds the public key in the CN for easy extraction
	/// - Is valid for 1 year (regenerated on each node startup)
	pub fn from_ed25519(signing_key: &SigningKey) -> anyhow::Result<Self> {
		let pubkey_bytes = signing_key.verifying_key().to_bytes();
		let pubkey_hex = hex::encode(pubkey_bytes);

		// Create the key pair from the Ed25519 secret key
		// rcgen expects PKCS#8 DER format
		let secret_bytes = signing_key.to_bytes();
		let keypair_der = ed25519_to_pkcs8_der(&secret_bytes);
		let key_pair = KeyPair::try_from(keypair_der.as_slice())
			.map_err(|e| anyhow::anyhow!("Failed to create KeyPair: {e}"))?;

		// Configure certificate parameters
		let mut params = CertificateParams::default();

		// Set the Common Name to the hex-encoded public key
		// This allows easy extraction during certificate verification
		params.distinguished_name.push(DnType::CommonName, &pubkey_hex);
		params.distinguished_name.push(DnType::OrganizationName, "Chainflip");

		// Generate the self-signed certificate
		let cert = params
			.self_signed(&key_pair)
			.map_err(|e| anyhow::anyhow!("Failed to generate certificate: {e}"))?;

		let cert_der = CertificateDer::from(cert.der().to_vec());
		let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(keypair_der));

		Ok(CertificateIdentity { cert_der, key_der, ed25519_pubkey: pubkey_bytes })
	}

	/// Extract the Ed25519 public key from a DER-encoded certificate.
	///
	/// Parses the Common Name which contains the hex-encoded public key.
	pub fn extract_pubkey_from_cert(cert_der: &[u8]) -> anyhow::Result<[u8; 32]> {
		use x509_parser::prelude::*;

		let (_, cert) = X509Certificate::from_der(cert_der)
			.map_err(|e| anyhow::anyhow!("Failed to parse certificate: {e}"))?;

		// Parse CN from subject
		for rdn in cert.subject().iter() {
			for attr in rdn.iter() {
				if attr.attr_type() == &x509_parser::oid_registry::OID_X509_COMMON_NAME {
					if let Ok(cn) = attr.as_str() {
						let pubkey_bytes = hex::decode(cn)
							.map_err(|e| anyhow::anyhow!("Invalid hex in CN: {e}"))?;
						if pubkey_bytes.len() == 32 {
							let mut pubkey = [0u8; 32];
							pubkey.copy_from_slice(&pubkey_bytes);
							return Ok(pubkey);
						}
					}
				}
			}
		}

		anyhow::bail!("Could not extract Ed25519 public key from certificate")
	}
}

/// Convert an Ed25519 secret key to PKCS#8 DER format.
///
/// The PKCS#8 structure for Ed25519 is:
/// ```text
/// PrivateKeyInfo ::= SEQUENCE {
///   version INTEGER (0),
///   privateKeyAlgorithm AlgorithmIdentifier,
///   privateKey OCTET STRING (containing CurvePrivateKey)
/// }
/// ```
fn ed25519_to_pkcs8_der(secret_key: &[u8; 32]) -> Vec<u8> {
	// Ed25519 algorithm OID: 1.3.101.112
	const ED25519_OID: &[u8] = &[0x06, 0x03, 0x2b, 0x65, 0x70];

	// PKCS#8 structure:
	// SEQUENCE {
	//   INTEGER 0 (version)
	//   SEQUENCE { OID 1.3.101.112 } (algorithm)
	//   OCTET STRING { OCTET STRING { secret_key } } (private key)
	// }

	let mut der = Vec::with_capacity(48);

	// Outer SEQUENCE tag + length (will be filled later)
	der.push(0x30);
	der.push(0x00); // placeholder for length

	// Version INTEGER 0
	der.extend_from_slice(&[0x02, 0x01, 0x00]);

	// Algorithm SEQUENCE containing just the OID
	der.push(0x30); // SEQUENCE
	der.push(ED25519_OID.len() as u8);
	der.extend_from_slice(ED25519_OID);

	// Private key: OCTET STRING containing OCTET STRING containing the key
	// Outer OCTET STRING
	der.push(0x04);
	der.push(34); // length of inner OCTET STRING (2 + 32)

	// Inner OCTET STRING with the actual key
	der.push(0x04);
	der.push(32);
	der.extend_from_slice(secret_key);

	// Fill in the outer SEQUENCE length
	der[1] = (der.len() - 2) as u8;

	der
}

#[cfg(test)]
mod tests {
	use super::*;
	use rand::rngs::OsRng;

	#[test]
	fn certificate_generation_roundtrip() {
		let signing_key = SigningKey::generate(&mut OsRng);
		let identity = CertificateIdentity::from_ed25519(&signing_key).unwrap();

		// Verify the pubkey matches
		assert_eq!(identity.ed25519_pubkey, signing_key.verifying_key().to_bytes());

		// Verify we can extract the pubkey from the certificate
		let extracted = CertificateIdentity::extract_pubkey_from_cert(&identity.cert_der).unwrap();
		assert_eq!(extracted, identity.ed25519_pubkey);
	}

	#[test]
	fn pkcs8_der_format() {
		let signing_key = SigningKey::generate(&mut OsRng);
		let der = ed25519_to_pkcs8_der(signing_key.as_bytes());

		// Verify the DER can be parsed back into a key pair
		let key_pair = KeyPair::try_from(der.as_slice());
		assert!(key_pair.is_ok(), "Generated PKCS#8 DER should be valid");
	}
}
