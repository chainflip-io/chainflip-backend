//! Verification of `packed` FIDO attestation, as produced by `navigator.credentials.create()` with
//! `attestation: "direct"` on a security key.
//!
//! A credential that passes this check was generated inside an authenticator whose attestation
//! certificate chains to one of the given trust anchors, so later signatures by it imply that
//! hardware. Batch attestation identifies the model, not the individual device.
use crate::{
	check_client_data, signed_message, AuthenticatorData, CeremonyType, Error, Policy, PublicKey,
};
use alloc::vec::Vec;
use ciborium::Value;
use p256::ecdsa::{signature::Verifier, Signature as EcdsaSignature, VerifyingKey};
use rsa::{pkcs1v15, pkcs8::DecodePublicKey, RsaPublicKey};
use sha2::Sha256;
use sp_core::RuntimeDebug;
use x509_cert::{
	der::{asn1::OctetString, oid::ObjectIdentifier, Decode, Encode},
	Certificate,
};

/// COSE algorithm identifier for ES256.
const COSE_ALG_ES256: i128 = -7;
/// FIDO extension carrying the authenticator model's AAGUID.
const OID_FIDO_GEN_CE_AAGUID: ObjectIdentifier =
	ObjectIdentifier::new_unwrap("1.3.6.1.4.1.45724.1.1.4");
const OID_SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
const OID_ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");

#[derive(Clone, PartialEq, Eq, RuntimeDebug)]
pub struct RegisteredCredential {
	pub public_key: PublicKey,
	pub credential_id: Vec<u8>,
	pub aaguid: [u8; 16],
	pub flags: u8,
}

/// Verifies a registration ceremony.
///
/// `trust_anchors` are DER certificates of the CAs allowed to issue attestation certificates.
/// These must be the FIDO *issuing* CAs rather than a vendor root shared with other products.
pub fn verify_registration(
	attestation_object: &[u8],
	client_data_json: &[u8],
	challenge: &[u8],
	policy: &Policy,
	trust_anchors: &[&[u8]],
) -> Result<RegisteredCredential, Error> {
	check_client_data(client_data_json, CeremonyType::Create, challenge)?;

	let attestation_object: Value =
		ciborium::de::from_reader(attestation_object).map_err(|_| Error::MalformedAttestation)?;
	let attestation_object = attestation_object.as_map().ok_or(Error::MalformedAttestation)?;
	let text_field = |key: &str| {
		attestation_object
			.iter()
			.find(|(k, _)| k.as_text() == Some(key))
			.map(|(_, v)| v)
			.ok_or(Error::MalformedAttestation)
	};

	if text_field("fmt")?.as_text() != Some("packed") {
		return Err(Error::UnsupportedAttestationFormat)
	}
	let authenticator_data =
		text_field("authData")?.as_bytes().ok_or(Error::MalformedAttestation)?;
	let statement = text_field("attStmt")?.as_map().ok_or(Error::MalformedAttestation)?;
	let statement_field = |key: &str| {
		statement
			.iter()
			.find(|(k, _)| k.as_text() == Some(key))
			.map(|(_, v)| v)
			.ok_or(Error::MalformedAttestation)
	};
	if integer(statement_field("alg")?) != Some(COSE_ALG_ES256) {
		return Err(Error::UnsupportedAlgorithm)
	}
	let signature = statement_field("sig")?.as_bytes().ok_or(Error::MalformedAttestation)?;
	// Self attestation (no x5c) proves nothing about the hardware, so it is rejected.
	let leaf = statement_field("x5c")?
		.as_array()
		.and_then(|chain| chain.first())
		.and_then(Value::as_bytes)
		.ok_or(Error::MalformedAttestation)?;

	let parsed_data = AuthenticatorData::parse(authenticator_data)?;
	parsed_data.check(policy)?;
	if !parsed_data.has_attested_credential_data() {
		return Err(Error::MalformedAuthenticatorData)
	}
	let (aaguid, credential_id, public_key) = parse_attested_credential_data(authenticator_data)?;

	let leaf = Certificate::from_der(leaf).map_err(|_| Error::MalformedCertificate)?;
	check_leaf_aaguid(&leaf, &aaguid)?;
	let leaf_key = VerifyingKey::from_sec1_bytes(
		leaf.tbs_certificate.subject_public_key_info.subject_public_key.raw_bytes(),
	)
	.map_err(|_| Error::MalformedCertificate)?;
	let signature = EcdsaSignature::from_der(signature).map_err(|_| Error::InvalidSignature)?;
	leaf_key
		.verify(&signed_message(authenticator_data, client_data_json), &signature)
		.map_err(|_| Error::InvalidSignature)?;

	verify_issued_by_anchor(&leaf, trust_anchors)?;

	Ok(RegisteredCredential { public_key, credential_id, aaguid, flags: parsed_data.flags })
}

/// Extracts the credential from an attestation object without verifying anything.
pub fn parse_unverified(attestation_object: &[u8]) -> Result<RegisteredCredential, Error> {
	let attestation_object: Value =
		ciborium::de::from_reader(attestation_object).map_err(|_| Error::MalformedAttestation)?;
	let authenticator_data = attestation_object
		.as_map()
		.and_then(|map| map.iter().find(|(k, _)| k.as_text() == Some("authData")))
		.and_then(|(_, v)| v.as_bytes())
		.ok_or(Error::MalformedAttestation)?;
	let flags = AuthenticatorData::parse(authenticator_data)?.flags;
	let (aaguid, credential_id, public_key) = parse_attested_credential_data(authenticator_data)?;
	Ok(RegisteredCredential { public_key, credential_id, aaguid, flags })
}

fn integer(value: &Value) -> Option<i128> {
	value.as_integer().map(i128::from)
}

/// Attested credential data follows the 37-byte fixed prefix:
/// aaguid (16) || credentialIdLength (2, BE) || credentialId || credentialPublicKey (COSE)
fn parse_attested_credential_data(
	authenticator_data: &[u8],
) -> Result<([u8; 16], Vec<u8>, PublicKey), Error> {
	let malformed = || Error::MalformedAuthenticatorData;
	let rest = authenticator_data.get(AuthenticatorData::MIN_LEN..).ok_or_else(malformed)?;
	let aaguid: [u8; 16] =
		rest.get(..16).ok_or_else(malformed)?.try_into().map_err(|_| malformed())?;
	let id_len = u16::from_be_bytes(
		rest.get(16..18).ok_or_else(malformed)?.try_into().map_err(|_| malformed())?,
	) as usize;
	let credential_id = rest.get(18..18 + id_len).ok_or_else(malformed)?.to_vec();
	let mut cose_key_bytes = rest.get(18 + id_len..).ok_or_else(malformed)?;
	// Extensions may follow the key, so decode a single item rather than the whole remainder.
	let cose_key: Value =
		ciborium::de::from_reader(&mut cose_key_bytes).map_err(|_| malformed())?;
	Ok((aaguid, credential_id, cose_es256_to_sec1(&cose_key)?))
}

/// COSE_Key (RFC 9053) EC2 P-256 → compressed SEC1.
fn cose_es256_to_sec1(cose_key: &Value) -> Result<PublicKey, Error> {
	let map = cose_key.as_map().ok_or(Error::InvalidPublicKey)?;
	let field = |label: i128| {
		map.iter()
			.find(|(k, _)| integer(k) == Some(label))
			.map(|(_, v)| v)
			.ok_or(Error::InvalidPublicKey)
	};
	// kty = EC2, alg = ES256, crv = P-256
	if integer(field(1)?) != Some(2) || integer(field(3)?) != Some(COSE_ALG_ES256) {
		return Err(Error::UnsupportedAlgorithm)
	}
	if integer(field(-1)?) != Some(1) {
		return Err(Error::UnsupportedAlgorithm)
	}
	let x = field(-2)?.as_bytes().ok_or(Error::InvalidPublicKey)?;
	let y = field(-3)?.as_bytes().ok_or(Error::InvalidPublicKey)?;
	let uncompressed = [&[0x04u8][..], x, y].concat();
	// Parsing validates that the point is on the curve.
	VerifyingKey::from_sec1_bytes(&uncompressed)
		.map_err(|_| Error::InvalidPublicKey)?
		.to_encoded_point(true)
		.as_bytes()
		.try_into()
		.map_err(|_| Error::InvalidPublicKey)
}

/// If the leaf carries the FIDO AAGUID extension it must match the authenticator data
/// (WebAuthn §8.2.1).
fn check_leaf_aaguid(leaf: &Certificate, aaguid: &[u8; 16]) -> Result<(), Error> {
	let Some(extension) = leaf
		.tbs_certificate
		.extensions
		.iter()
		.flatten()
		.find(|ext| ext.extn_id == OID_FIDO_GEN_CE_AAGUID)
	else {
		return Ok(())
	};
	let inner = OctetString::from_der(extension.extn_value.as_bytes())
		.map_err(|_| Error::MalformedCertificate)?;
	if inner.as_bytes() != aaguid {
		return Err(Error::AaguidMismatch)
	}
	Ok(())
}

fn verify_issued_by_anchor(leaf: &Certificate, trust_anchors: &[&[u8]]) -> Result<(), Error> {
	let issuer = leaf.tbs_certificate.issuer.to_der().map_err(|_| Error::MalformedCertificate)?;
	let anchor = trust_anchors
		.iter()
		.filter_map(|der| Certificate::from_der(der).ok())
		.find(|anchor| anchor.tbs_certificate.subject.to_der().ok().as_ref() == Some(&issuer))
		.ok_or(Error::UntrustedIssuer)?;

	let tbs = leaf.tbs_certificate.to_der().map_err(|_| Error::MalformedCertificate)?;
	let signature = leaf.signature.raw_bytes();
	let spki = &anchor.tbs_certificate.subject_public_key_info;
	let valid = match leaf.signature_algorithm.oid {
		OID_SHA256_WITH_RSA => {
			let spki_der = spki.to_der().map_err(|_| Error::MalformedCertificate)?;
			let key = RsaPublicKey::from_public_key_der(&spki_der)
				.map_err(|_| Error::MalformedCertificate)?;
			pkcs1v15::Signature::try_from(signature).is_ok_and(|sig| {
				pkcs1v15::VerifyingKey::<Sha256>::new(key).verify(&tbs, &sig).is_ok()
			})
		},
		OID_ECDSA_WITH_SHA256 => {
			let key = VerifyingKey::from_sec1_bytes(spki.subject_public_key.raw_bytes())
				.map_err(|_| Error::MalformedCertificate)?;
			EcdsaSignature::from_der(signature).is_ok_and(|sig| key.verify(&tbs, &sig).is_ok())
		},
		_ => return Err(Error::UnsupportedAlgorithm),
	};
	valid.then_some(()).ok_or(Error::InvalidCertificateSignature)
}
