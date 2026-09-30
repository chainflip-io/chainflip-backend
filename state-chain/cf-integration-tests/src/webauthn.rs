//! A governance member whose only key is a WebAuthn credential approves a proposal, via both
//! signing paths: the `CfSignature::WebAuthn` variant (signed v4 extrinsic) and the
//! `CheckWebAuthn` transaction extension (general v5 extrinsic).
use super::*;
use cf_webauthn::{
	signature::{challenge, CfSignature, WebAuthnSignature},
	test_authenticator::TestAuthenticator,
	Assertion, PublicKey,
};
use codec::Encode;
use frame_support::pallet_prelude::{InvalidTransaction, TransactionValidityError};
use pallet_cf_governance::{ExecutionMode, ProposalIdCounter};
use sp_runtime::{generic::Era, traits::TransactionExtension, ApplyExtrinsicResult};
use state_chain_runtime::{
	CheckWebAuthn, Executive, Governance, SignedPayload, TxExtension, UncheckedExtrinsic,
};

const RP_ID: &[u8] = b"localhost";

/// Anything that can answer a WebAuthn `get()` ceremony.
trait Authenticator {
	fn public_key(&self) -> PublicKey;
	fn sign(&self, challenge: &[u8]) -> Assertion;
}

impl Authenticator for TestAuthenticator {
	fn public_key(&self) -> PublicKey {
		TestAuthenticator::public_key(self)
	}
	fn sign(&self, challenge: &[u8]) -> Assertion {
		TestAuthenticator::sign(self, challenge)
	}
}

/// Every extension after `CheckWebAuthn`.
type RestOfPipeline = (
	frame_system::AuthorizeCall<Runtime>,
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
	frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
	frame_system::WeightReclaim<Runtime>,
);

fn rest_of_pipeline(account_id: &AccountId) -> RestOfPipeline {
	(
		frame_system::AuthorizeCall::new(),
		frame_system::CheckNonZeroSender::new(),
		frame_system::CheckSpecVersion::new(),
		frame_system::CheckTxVersion::new(),
		frame_system::CheckGenesis::new(),
		frame_system::CheckEra::from(Era::Immortal),
		frame_system::CheckNonce::from(System::account_nonce(account_id)),
		frame_system::CheckWeight::new(),
		pallet_transaction_payment::ChargeTransactionPayment::from(0),
		frame_metadata_hash_extension::CheckMetadataHash::new(false),
		frame_system::WeightReclaim::new(),
	)
}

fn pipeline(webauthn: CheckWebAuthn<Runtime>, rest: RestOfPipeline) -> TxExtension {
	let (a, b, c, d, e, f, g, h, i, j, k) = rest;
	(webauthn, a, b, c, d, e, f, g, h, i, j, k)
}

/// A governance member with no native key, and a pending proposal for them to approve.
fn setup(authenticator: &impl Authenticator) -> (AccountId, u32) {
	let public_key = authenticator.public_key();
	let account_id = cf_webauthn::account_id(&public_key);
	pallet_cf_webauthn::RelyingPartyId::<Runtime>::put(
		frame_support::BoundedVec::try_from(RP_ID.to_vec()).unwrap(),
	);
	// Registration (attestation) is covered by the pallet tests against a real YubiKey recording.
	pallet_cf_webauthn::Credentials::<Runtime>::insert(
		&account_id,
		pallet_cf_webauthn::Credential {
			public_key,
			credential_id: Default::default(),
			aaguid: [0; 16],
		},
	);
	System::inc_sufficients(&account_id);
	pallet_cf_governance::Members::<Runtime>::mutate(|council| {
		council.members.insert(account_id.clone());
		council.threshold = 2;
	});

	assert_ok!(Governance::propose_governance_extrinsic(
		RuntimeOrigin::signed(ERIN.into()),
		Box::new(frame_system::Call::remark { remark: vec![] }.into()),
		ExecutionMode::Manual,
	));
	(account_id, ProposalIdCounter::<Runtime>::get())
}

fn approve(id: u32) -> RuntimeCall {
	pallet_cf_governance::Call::approve { approved_id: id }.into()
}

fn assert_approved(id: u32) {
	assert!(System::events().iter().any(|record| matches!(
		record.event,
		state_chain_runtime::RuntimeEvent::Governance(pallet_cf_governance::Event::Approved(approved))
			if approved == id
	)));
}

/// Path 1: a signed v4 extrinsic whose signature is `CfSignature::WebAuthn`.
fn signed_with_webauthn_signature(
	authenticator: &impl Authenticator,
	account_id: &AccountId,
	call: RuntimeCall,
) -> UncheckedExtrinsic {
	let ext = pipeline(CheckWebAuthn::disabled(), rest_of_pipeline(account_id));
	// `using_encoded` hands over exactly what `Verify::verify` later receives: the payload, or
	// its blake2 hash if longer than 256 bytes.
	let signature =
		SignedPayload::new(call.clone(), ext.clone()).unwrap().using_encoded(|payload| {
			CfSignature::WebAuthn(WebAuthnSignature {
				public_key: authenticator.public_key(),
				assertion: authenticator.sign(&challenge(payload)),
			})
		});
	UncheckedExtrinsic::new_signed(call, account_id.clone().into(), signature, ext)
}

/// Path 2: a general v5 extrinsic authorised by `CheckWebAuthn`.
fn authorised_by_webauthn_extension(
	authenticator: &impl Authenticator,
	account_id: &AccountId,
	call: RuntimeCall,
) -> UncheckedExtrinsic {
	const EXTENSION_VERSION: u8 = 0;
	let rest = rest_of_pipeline(account_id);
	let message = (EXTENSION_VERSION, &call, &rest, rest.implicit().unwrap())
		.using_encoded(sp_core::blake2_256);
	UncheckedExtrinsic::new_transaction(
		call,
		pipeline(CheckWebAuthn::signed(account_id.clone(), authenticator.sign(&message)), rest),
	)
}

fn apply(uxt: UncheckedExtrinsic) -> ApplyExtrinsicResult {
	Executive::apply_extrinsic(uxt)
}

#[test]
fn webauthn_signature_variant_approves_proposal() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let (account_id, id) = setup(&authenticator);

		assert_eq!(
			apply(signed_with_webauthn_signature(&authenticator, &account_id, approve(id))),
			Ok(Ok(()))
		);
		assert_approved(id);
		assert_eq!(System::account_nonce(&account_id), 1);
	});
}

#[test]
fn webauthn_extension_approves_proposal() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let (account_id, id) = setup(&authenticator);

		assert_eq!(
			apply(authorised_by_webauthn_extension(&authenticator, &account_id, approve(id))),
			Ok(Ok(()))
		);
		assert_approved(id);
		assert_eq!(System::account_nonce(&account_id), 1);
	});
}

#[test]
fn extension_transactions_cannot_be_replayed() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let (account_id, id) = setup(&authenticator);
		let uxt = authorised_by_webauthn_extension(&authenticator, &account_id, approve(id));

		assert_eq!(apply(uxt.clone()), Ok(Ok(())));
		assert_eq!(apply(uxt), Err(TransactionValidityError::Invalid(InvalidTransaction::Stale)));
	});
}

#[test]
fn extension_rejects_unregistered_credential() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let registered = TestAuthenticator::new(1, RP_ID);
		let (_, id) = setup(&registered);
		let unregistered = TestAuthenticator::new(2, RP_ID);
		let account_id = cf_webauthn::account_id(&unregistered.public_key());
		System::inc_sufficients(&account_id);

		assert_eq!(
			apply(authorised_by_webauthn_extension(&unregistered, &account_id, approve(id))),
			Err(TransactionValidityError::Invalid(InvalidTransaction::BadSigner))
		);
	});
}

/// `Verify::verify` has no storage access, so the signature variant can't consult the registry:
/// any P-256 WebAuthn key whose account exists can sign, attested or not.
#[test]
fn signature_variant_does_not_consult_registry() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let registered = TestAuthenticator::new(1, RP_ID);
		setup(&registered);
		let unregistered = TestAuthenticator::new(2, RP_ID);
		let account_id = cf_webauthn::account_id(&unregistered.public_key());
		System::inc_sufficients(&account_id);

		<Flip as cf_traits::Funding>::credit_funds(&account_id, super::genesis::GENESIS_BALANCE);

		assert_eq!(
			apply(signed_with_webauthn_signature(
				&unregistered,
				&account_id,
				frame_system::Call::remark { remark: vec![] }.into()
			)),
			Ok(Ok(()))
		);
		assert_eq!(System::account_nonce(&account_id), 1);
	});
}

/// Governance calls are fee-waived, so check that `ChargeTransactionPayment` charges the account
/// that `CheckWebAuthn` authorised.
#[test]
fn extension_transactions_pay_fees() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let (account_id, _) = setup(&authenticator);
		<Flip as cf_traits::Funding>::credit_funds(&account_id, super::genesis::GENESIS_BALANCE);
		let before = Flip::total_balance_of(&account_id);

		assert_eq!(
			apply(authorised_by_webauthn_extension(
				&authenticator,
				&account_id,
				frame_system::Call::remark { remark: vec![] }.into()
			)),
			Ok(Ok(()))
		);
		assert!(Flip::total_balance_of(&account_id) < before);
	});
}

#[test]
fn signature_variant_needs_existing_account() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let authenticator = TestAuthenticator::new(3, RP_ID);
		let account_id = cf_webauthn::account_id(&authenticator.public_key());
		assert!(matches!(
			apply(signed_with_webauthn_signature(&authenticator, &account_id, approve(1))),
			Err(TransactionValidityError::Invalid(_))
		));
	});
}

#[test]
fn signature_variant_rejects_wrong_key() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let (account_id, id) = setup(&authenticator);
		let other = TestAuthenticator::new(2, RP_ID);
		assert_eq!(
			apply(signed_with_webauthn_signature(&other, &account_id, approve(id))),
			Err(TransactionValidityError::Invalid(InvalidTransaction::BadProof))
		);
	});
}

/// Assertions recorded from a real YubiKey with `state-chain/webauthn/poc-page`.
///
/// The signed payloads commit to the genesis hash and spec version, so these recordings go
/// stale whenever either changes; re-record with the challenges the failing test prints.
mod yubikey {
	use super::*;
	use serde_json::Value;

	const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../webauthn/fixtures");

	fn fixture(name: &str) -> Value {
		let path = format!("{FIXTURES}/{name}.json");
		serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path}")))
			.unwrap()
	}

	struct Recorded {
		public_key: PublicKey,
		assertions: Vec<Value>,
	}

	impl Recorded {
		fn load() -> Self {
			let registration = fixture("yubikey_registration");
			let credential = cf_webauthn::attestation::parse_unverified(
				&hex::decode(registration["attestation_object"].as_str().unwrap()).unwrap(),
			)
			.unwrap();
			Self {
				public_key: credential.public_key,
				assertions: ["yubikey_assertion_signature_variant", "yubikey_assertion_extension"]
					.into_iter()
					.map(fixture)
					.collect(),
			}
		}
	}

	impl Authenticator for Recorded {
		fn public_key(&self) -> PublicKey {
			self.public_key
		}

		fn sign(&self, challenge: &[u8]) -> Assertion {
			let challenge = hex::encode(challenge);
			let recorded = self
				.assertions
				.iter()
				.find(|a| a["challenge"] == challenge.as_str())
				.unwrap_or_else(|| panic!("no recorded assertion for challenge {challenge}"));
			let bytes = |field: &str| hex::decode(recorded[field].as_str().unwrap()).unwrap();
			Assertion {
				authenticator_data: bytes("authenticator_data").try_into().unwrap(),
				client_data_json: recorded["client_data_json"]
					.as_str()
					.unwrap()
					.as_bytes()
					.to_vec()
					.try_into()
					.unwrap(),
				signature: bytes("signature").try_into().unwrap(),
			}
		}
	}

	/// Prints the challenges to sign for the tests below. The payloads don't depend on the key.
	#[test]
	#[ignore]
	fn print_challenges() {
		struct Print;
		impl Authenticator for Print {
			fn public_key(&self) -> PublicKey {
				TestAuthenticator::new(0, RP_ID).public_key()
			}
			fn sign(&self, challenge: &[u8]) -> Assertion {
				println!("challenge: {}", hex::encode(challenge));
				TestAuthenticator::new(0, RP_ID).sign(challenge)
			}
		}
		for build in [signed_with_webauthn_signature, authorised_by_webauthn_extension] {
			super::super::genesis::with_test_defaults().build().execute_with(|| {
				let (account_id, id) = setup(&Print);
				build(&Print, &account_id, approve(id));
			});
		}
	}

	#[test]
	#[ignore]
	fn yubikey_approves_via_signature_variant() {
		super::super::genesis::with_test_defaults().build().execute_with(|| {
			let yubikey = Recorded::load();
			let (account_id, id) = setup(&yubikey);
			assert_eq!(
				apply(signed_with_webauthn_signature(&yubikey, &account_id, approve(id))),
				Ok(Ok(()))
			);
			assert_approved(id);
		});
	}

	#[test]
	#[ignore]
	fn yubikey_approves_via_extension() {
		super::super::genesis::with_test_defaults().build().execute_with(|| {
			let yubikey = Recorded::load();
			let (account_id, id) = setup(&yubikey);
			assert_eq!(
				apply(authorised_by_webauthn_extension(&yubikey, &account_id, approve(id))),
				Ok(Ok(()))
			);
			assert_approved(id);
		});
	}
}
