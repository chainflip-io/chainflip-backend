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

//! Custom TLS certificate verifier for allowlist-based peer authentication.
//!
//! Implements rustls verifier traits to check that connecting peers have
//! Ed25519 public keys that are on our allowlist (registered validators).

use std::{
	collections::HashMap,
	fmt::Debug,
	sync::{Arc, RwLock},
};

use rustls::{
	client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
	pki_types::{CertificateDer, ServerName, UnixTime},
	server::danger::{ClientCertVerified, ClientCertVerifier},
	CertificateError, DigitallySignedStruct, DistinguishedName, Error as TlsError, SignatureScheme,
};
use tracing::{trace, warn};

use crate::message::AccountId;
use cf_utilities::metrics::{P2P_ALLOWED_PUBKEYS, P2P_DECLINED_CONNECTIONS};

use super::cert::CertificateIdentity;

/// Wrapper for the allowed pubkeys map with metrics tracking.
struct AllowedPubkeysWrapper {
	metric: &'static P2P_ALLOWED_PUBKEYS,
	map: HashMap<[u8; 32], AccountId>,
}

impl AllowedPubkeysWrapper {
	fn new() -> Self {
		AllowedPubkeysWrapper { metric: &P2P_ALLOWED_PUBKEYS, map: Default::default() }
	}

	fn get(&self, pubkey: &[u8; 32]) -> Option<&AccountId> {
		self.map.get(pubkey)
	}

	fn insert(&mut self, pubkey: [u8; 32], account_id: AccountId) {
		// Enforce one key per account: if this account previously registered a
		// different key (e.g. a node-key rotation), revoke the old key so it can
		// no longer authenticate.
		self.map
			.retain(|existing_pubkey, existing_account| {
				*existing_account != account_id || *existing_pubkey == pubkey
			});
		self.map.insert(pubkey, account_id);
		self.metric.set(self.map.len());
	}

	fn remove(&mut self, pubkey: &[u8; 32]) -> Option<AccountId> {
		let result = self.map.remove(pubkey);
		self.metric.set(self.map.len());
		result
	}

	fn contains(&self, pubkey: &[u8; 32]) -> bool {
		self.map.contains_key(pubkey)
	}
}

/// Allowlist-based TLS certificate verifier.
///
/// Verifies that connecting peers have Ed25519 public keys that are
/// registered in our allowlist. This mirrors the ZAP authentication
/// approach used by ZMQ.
#[derive(Clone)]
pub struct AllowlistVerifier {
	allowed_pubkeys: Arc<RwLock<AllowedPubkeysWrapper>>,
}

impl Debug for AllowlistVerifier {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("AllowlistVerifier")
			.field("allowed_count", &self.allowed_pubkeys.read().unwrap().map.len())
			.finish()
	}
}

impl AllowlistVerifier {
	/// Create a new empty allowlist verifier.
	pub fn new() -> Self {
		AllowlistVerifier { allowed_pubkeys: Arc::new(RwLock::new(AllowedPubkeysWrapper::new())) }
	}

	/// Add a peer's Ed25519 public key to the allowlist.
	pub fn add_peer(&self, ed_pubkey: [u8; 32], account_id: AccountId) {
		trace!("Adding to allowlist: {} (pubkey: {})", account_id, hex::encode(ed_pubkey));
		self.allowed_pubkeys.write().unwrap().insert(ed_pubkey, account_id);
	}

	/// Remove a peer from the allowlist.
	pub fn remove_peer(&self, ed_pubkey: &[u8; 32]) {
		if let Some(account_id) = self.allowed_pubkeys.write().unwrap().remove(ed_pubkey) {
			trace!("Removed from allowlist: {} (pubkey: {})", account_id, hex::encode(ed_pubkey));
		}
	}

	/// Check if a public key is on the allowlist.
	pub fn is_allowed(&self, ed_pubkey: &[u8; 32]) -> bool {
		self.allowed_pubkeys.read().unwrap().contains(ed_pubkey)
	}

	/// Get the account ID for a public key, if it's on the allowlist.
	pub fn get_account_id(&self, ed_pubkey: &[u8; 32]) -> Option<AccountId> {
		self.allowed_pubkeys.read().unwrap().get(ed_pubkey).cloned()
	}

	/// Verify a certificate and return the Ed25519 public key if valid.
	fn verify_certificate(&self, cert: &CertificateDer<'_>) -> Result<[u8; 32], TlsError> {
		// Extract the Ed25519 public key from the certificate
		let pubkey = CertificateIdentity::extract_pubkey_from_cert(cert)
			.map_err(|e| TlsError::General(format!("Failed to extract pubkey: {e}")))?;

		// Check if the public key is on our allowlist
		if self.allowed_pubkeys.read().unwrap().contains(&pubkey) {
			trace!("Allowing connection for pubkey: {}", hex::encode(pubkey));
			Ok(pubkey)
		} else {
			warn!("Declining connection for unknown pubkey: {}", hex::encode(pubkey));
			P2P_DECLINED_CONNECTIONS.inc();
			Err(TlsError::InvalidCertificate(CertificateError::ApplicationVerificationFailure))
		}
	}
}

impl Default for AllowlistVerifier {
	fn default() -> Self {
		Self::new()
	}
}

// Implement ServerCertVerifier for client-side verification of servers
impl ServerCertVerifier for AllowlistVerifier {
	fn verify_server_cert(
		&self,
		end_entity: &CertificateDer<'_>,
		_intermediates: &[CertificateDer<'_>],
		_server_name: &ServerName<'_>,
		_ocsp_response: &[u8],
		_now: UnixTime,
	) -> Result<ServerCertVerified, TlsError> {
		self.verify_certificate(end_entity)?;
		Ok(ServerCertVerified::assertion())
	}

	fn verify_tls12_signature(
		&self,
		_message: &[u8],
		_cert: &CertificateDer<'_>,
		_dss: &DigitallySignedStruct,
	) -> Result<HandshakeSignatureValid, TlsError> {
		// We don't support TLS 1.2, only TLS 1.3 with Ed25519
		Err(TlsError::General("TLS 1.2 not supported".into()))
	}

	fn verify_tls13_signature(
		&self,
		message: &[u8],
		cert: &CertificateDer<'_>,
		dss: &DigitallySignedStruct,
	) -> Result<HandshakeSignatureValid, TlsError> {
		// Verify the signature using the certificate's public key
		verify_ed25519_signature(message, cert, dss)
	}

	fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
		vec![SignatureScheme::ED25519]
	}
}

// Implement ClientCertVerifier for server-side verification of clients
impl ClientCertVerifier for AllowlistVerifier {
	fn root_hint_subjects(&self) -> &[DistinguishedName] {
		// We don't use a CA, so no root hints
		&[]
	}

	fn verify_client_cert(
		&self,
		end_entity: &CertificateDer<'_>,
		_intermediates: &[CertificateDer<'_>],
		_now: UnixTime,
	) -> Result<ClientCertVerified, TlsError> {
		self.verify_certificate(end_entity)?;
		Ok(ClientCertVerified::assertion())
	}

	fn verify_tls12_signature(
		&self,
		_message: &[u8],
		_cert: &CertificateDer<'_>,
		_dss: &DigitallySignedStruct,
	) -> Result<HandshakeSignatureValid, TlsError> {
		// We don't support TLS 1.2
		Err(TlsError::General("TLS 1.2 not supported".into()))
	}

	fn verify_tls13_signature(
		&self,
		message: &[u8],
		cert: &CertificateDer<'_>,
		dss: &DigitallySignedStruct,
	) -> Result<HandshakeSignatureValid, TlsError> {
		verify_ed25519_signature(message, cert, dss)
	}

	fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
		vec![SignatureScheme::ED25519]
	}

	fn client_auth_mandatory(&self) -> bool {
		// Require client certificates for mutual TLS
		true
	}
}

/// Verify an Ed25519 signature from a TLS handshake.
fn verify_ed25519_signature(
	message: &[u8],
	cert: &CertificateDer<'_>,
	dss: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, TlsError> {
	if dss.scheme != SignatureScheme::ED25519 {
		return Err(TlsError::General(format!("Unsupported signature scheme: {:?}", dss.scheme)));
	}

	// Extract the public key from the certificate
	let pubkey_bytes = CertificateIdentity::extract_pubkey_from_cert(cert)
		.map_err(|e| TlsError::General(format!("Failed to extract pubkey: {e}")))?;

	// Create the verifying key
	let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_bytes)
		.map_err(|e| TlsError::General(format!("Invalid Ed25519 public key: {e}")))?;

	// Parse the signature
	let signature_bytes: [u8; 64] = dss
		.signature()
		.try_into()
		.map_err(|_| TlsError::General("Invalid Ed25519 signature length".into()))?;
	let signature = ed25519_dalek::Signature::from_bytes(&signature_bytes);

	// Verify the signature. `verify_strict` rejects non-canonical signatures and
	// small-order keys, which is the recommended choice for an authentication boundary.
	verifying_key
		.verify_strict(message, &signature)
		.map_err(|e| TlsError::General(format!("Signature verification failed: {e}")))?;

	Ok(HandshakeSignatureValid::assertion())
}

#[cfg(test)]
mod tests {
	use super::*;
	use ed25519_dalek::SigningKey;
	use rand::rngs::OsRng;

	#[test]
	fn allowlist_accepts_known_peer() {
		let verifier = AllowlistVerifier::new();
		let signing_key = SigningKey::generate(&mut OsRng);
		let pubkey = signing_key.verifying_key().to_bytes();
		let account_id = AccountId::new([1; 32]);

		verifier.add_peer(pubkey, account_id.clone());

		assert!(verifier.is_allowed(&pubkey));
		assert_eq!(verifier.get_account_id(&pubkey), Some(account_id));
	}

	#[test]
	fn allowlist_rejects_unknown_peer() {
		let verifier = AllowlistVerifier::new();
		let signing_key = SigningKey::generate(&mut OsRng);
		let pubkey = signing_key.verifying_key().to_bytes();

		assert!(!verifier.is_allowed(&pubkey));
		assert_eq!(verifier.get_account_id(&pubkey), None);
	}

	#[test]
	fn allowlist_remove_peer() {
		let verifier = AllowlistVerifier::new();
		let signing_key = SigningKey::generate(&mut OsRng);
		let pubkey = signing_key.verifying_key().to_bytes();
		let account_id = AccountId::new([1; 32]);

		verifier.add_peer(pubkey, account_id);
		assert!(verifier.is_allowed(&pubkey));

		verifier.remove_peer(&pubkey);
		assert!(!verifier.is_allowed(&pubkey));
	}

	#[test]
	fn rotating_account_key_revokes_old_key() {
		let verifier = AllowlistVerifier::new();
		let account_id = AccountId::new([7; 32]);
		let old_key = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
		let new_key = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();

		verifier.add_peer(old_key, account_id.clone());
		assert!(verifier.is_allowed(&old_key));

		// The same account re-registers with a rotated key.
		verifier.add_peer(new_key, account_id.clone());

		assert!(verifier.is_allowed(&new_key), "new key must be allowed");
		assert!(!verifier.is_allowed(&old_key), "old key must be revoked after rotation");
		assert_eq!(verifier.get_account_id(&new_key), Some(account_id));
	}

	#[test]
	fn re_registering_same_key_for_account_keeps_it() {
		let verifier = AllowlistVerifier::new();
		let account_id = AccountId::new([7; 32]);
		let key = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();

		verifier.add_peer(key, account_id.clone());
		verifier.add_peer(key, account_id);

		assert!(verifier.is_allowed(&key));
	}

	#[test]
	fn rotating_one_account_does_not_affect_others() {
		let verifier = AllowlistVerifier::new();
		let account_a = AccountId::new([1; 32]);
		let account_b = AccountId::new([2; 32]);
		let key_a = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
		let key_b = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
		let key_a2 = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();

		verifier.add_peer(key_a, account_a.clone());
		verifier.add_peer(key_b, account_b);
		verifier.add_peer(key_a2, account_a);

		assert!(verifier.is_allowed(&key_a2));
		assert!(!verifier.is_allowed(&key_a), "account A's old key must be revoked");
		assert!(verifier.is_allowed(&key_b), "account B's key must be unaffected");
	}
}
