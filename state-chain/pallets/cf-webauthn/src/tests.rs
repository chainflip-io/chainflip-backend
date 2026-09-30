use crate::{mock::*, CheckWebAuthn, Credential, Credentials};
use cf_webauthn::test_authenticator::TestAuthenticator;
use codec::Encode;
use frame_support::{dispatch::GetDispatchInfo, pallet_prelude::TransactionSource};
use sp_io::hashing::blake2_256;
use sp_runtime::{
	traits::DispatchTransaction,
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	AccountId32,
};

const EXTENSION_VERSION: u8 = 0;

fn register(authenticator: &TestAuthenticator) -> AccountId32 {
	let public_key = authenticator.public_key();
	let account_id = cf_webauthn::account_id(&public_key);
	Credentials::<Test>::insert(
		&account_id,
		Credential { public_key, credential_id: Default::default(), aaguid: [0; 16] },
	);
	account_id
}

fn remark() -> RuntimeCall {
	RuntimeCall::System(frame_system::Call::remark { remark: b"approve 42".to_vec() })
}

/// The message signed by the first extension of a single-extension pipeline.
fn challenge(call: &RuntimeCall) -> [u8; 32] {
	(EXTENSION_VERSION, call).using_encoded(blake2_256)
}

fn validate(
	ext: CheckWebAuthn<Test>,
	call: &RuntimeCall,
) -> Result<Option<AccountId32>, TransactionValidityError> {
	ext.validate_only(
		RuntimeOrigin::none(),
		call,
		&call.get_dispatch_info(),
		0,
		TransactionSource::External,
		EXTENSION_VERSION,
	)
	.map(|(_, _, origin)| frame_system::ensure_signed(origin).ok())
}

#[test]
fn registered_credential_authorises_call() {
	new_test_ext().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let account_id = register(&authenticator);
		let call = remark();
		assert_eq!(
			validate(
				CheckWebAuthn::signed(account_id.clone(), authenticator.sign(&challenge(&call))),
				&call,
			),
			Ok(Some(account_id))
		);
	});
}

#[test]
fn unregistered_signer_is_rejected_before_verification() {
	new_test_ext().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let call = remark();
		assert_eq!(
			validate(
				CheckWebAuthn::signed(
					cf_webauthn::account_id(&authenticator.public_key()),
					authenticator.sign(&challenge(&call)),
				),
				&call,
			),
			Err(InvalidTransaction::BadSigner.into())
		);
	});
}

#[test]
fn signature_over_different_call_is_rejected() {
	new_test_ext().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let account_id = register(&authenticator);
		let signed_for = RuntimeCall::System(frame_system::Call::remark { remark: b"x".to_vec() });
		assert_eq!(
			validate(
				CheckWebAuthn::signed(account_id, authenticator.sign(&challenge(&signed_for))),
				&remark(),
			),
			Err(InvalidTransaction::BadProof.into())
		);
	});
}

#[test]
fn credential_for_other_rp_is_rejected() {
	new_test_ext().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, b"evil.example");
		let account_id = register(&authenticator);
		let call = remark();
		assert_eq!(
			validate(
				CheckWebAuthn::signed(account_id, authenticator.sign(&challenge(&call))),
				&call
			),
			Err(InvalidTransaction::BadProof.into())
		);
	});
}

#[test]
fn disabled_extension_authorises_nothing() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			validate(CheckWebAuthn::disabled(), &remark()),
			Err(InvalidTransaction::UnknownOrigin.into())
		);
	});
}

/// Registration against an attestation recorded from a real YubiKey with
/// `state-chain/webauthn/poc-page`, using [`yubikey::SPONSOR`]'s registration challenge.
mod yubikey {
	use super::*;
	use crate::{Error, Event, TrustAnchors};
	use frame_support::{assert_noop, assert_ok, traits::ConstU32, BoundedVec};

	pub const SPONSOR: AccountId32 = AccountId32::new([1; 32]);
	const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../webauthn/fixtures");
	/// Yubico's FIDO issuing CAs: not the shared attestation root, which also issues PIV/OpenPGP.
	const FIDO_ISSUING_CAS: [&str; 5] = [
		"yubico-fido-attestation-a-1",
		"yubico-fido-attestation-b-1",
		"yubico-fido-attestation-b2-1",
		"yubico-fido-ca-1",
		"yubico-fido-ca-2",
	];

	fn trust_yubico() {
		TrustAnchors::<Test>::put(BoundedVec::truncate_from(
			FIDO_ISSUING_CAS
				.iter()
				.map(|name| {
					BoundedVec::truncate_from(
						std::fs::read(format!("{FIXTURES}/yubico/{name}.der")).unwrap(),
					)
				})
				.collect(),
		));
	}

	fn recording() -> (
		BoundedVec<u8, ConstU32<{ crate::MAX_ATTESTATION_OBJECT_LEN }>>,
		BoundedVec<u8, ConstU32<{ cf_webauthn::MAX_CLIENT_DATA_JSON_LEN }>>,
	) {
		let json: serde_json::Value = serde_json::from_str(
			&std::fs::read_to_string(format!("{FIXTURES}/yubikey_registration.json")).unwrap(),
		)
		.unwrap();
		(
			hex::decode(json["attestation_object"].as_str().unwrap())
				.unwrap()
				.try_into()
				.unwrap(),
			json["client_data_json"]
				.as_str()
				.unwrap()
				.as_bytes()
				.to_vec()
				.try_into()
				.unwrap(),
		)
	}

	#[test]
	#[ignore]
	fn print_registration_challenge() {
		new_test_ext().execute_with(|| {
			println!(
				"challenge: {}",
				hex::encode(crate::Pallet::<Test>::registration_challenge(&SPONSOR))
			);
		});
	}

	#[test]
	fn registers_attested_yubikey() {
		new_test_ext().execute_with(|| {
			trust_yubico();
			let (attestation_object, client_data_json) = recording();
			assert_ok!(WebAuthn::register_credential(
				RuntimeOrigin::signed(SPONSOR),
				attestation_object.clone(),
				client_data_json
			));
			let credential =
				cf_webauthn::attestation::parse_unverified(&attestation_object).unwrap();
			let account_id = cf_webauthn::account_id(&credential.public_key);
			assert_eq!(
				Credentials::<Test>::get(&account_id).unwrap().public_key,
				credential.public_key
			);
			assert_eq!(frame_system::Pallet::<Test>::sufficients(&account_id), 1);
			frame_system::Pallet::<Test>::assert_last_event(
				Event::CredentialRegistered {
					account_id,
					aaguid: credential.aaguid,
					sponsor: SPONSOR,
				}
				.into(),
			);
		});
	}

	#[test]
	fn rejects_duplicate_registration() {
		new_test_ext().execute_with(|| {
			trust_yubico();
			let (attestation_object, client_data_json) = recording();
			assert_ok!(WebAuthn::register_credential(
				RuntimeOrigin::signed(SPONSOR),
				attestation_object.clone(),
				client_data_json.clone()
			));
			assert_noop!(
				WebAuthn::register_credential(
					RuntimeOrigin::signed(SPONSOR),
					attestation_object,
					client_data_json
				),
				Error::<Test>::AlreadyRegistered
			);
		});
	}

	#[test]
	fn rejects_untrusted_issuer() {
		new_test_ext().execute_with(|| {
			let (attestation_object, client_data_json) = recording();
			assert_noop!(
				WebAuthn::register_credential(
					RuntimeOrigin::signed(SPONSOR),
					attestation_object,
					client_data_json
				),
				Error::<Test>::InvalidAttestation
			);
		});
	}

	/// The attestation is bound to the sponsor, so it can't be replayed by someone else.
	#[test]
	fn rejects_other_sponsor() {
		new_test_ext().execute_with(|| {
			trust_yubico();
			let (attestation_object, client_data_json) = recording();
			assert_noop!(
				WebAuthn::register_credential(
					RuntimeOrigin::signed(AccountId32::new([2; 32])),
					attestation_object,
					client_data_json
				),
				Error::<Test>::InvalidAttestation
			);
		});
	}
}
