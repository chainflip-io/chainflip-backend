#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cf_webauthn::{
	signature::{challenge, CfSignature, WebAuthnSignature},
	test_authenticator::TestAuthenticator,
};
use codec::Decode;
use frame_benchmarking::v2::*;
use frame_support::{dispatch::GetDispatchInfo, pallet_prelude::TransactionSource};
use frame_system::RawOrigin;
use sp_runtime::traits::{
	AsTransactionAuthorizedOrigin, DispatchTransaction, Dispatchable, Verify,
};

const RP_ID: &[u8] = b"localhost";

/// Recorded from a real YubiKey. Its challenge is [`Pallet::registration_challenge`] for
/// `[1; 32]` on a chain whose genesis hash is `[69; 32]` (the `frame_system` test default).
const ATTESTATION_OBJECT: &[u8] =
	include_bytes!("../../../webauthn/fixtures/yubikey_attestation_object.bin");
const CLIENT_DATA_JSON: &[u8] =
	include_bytes!("../../../webauthn/fixtures/yubikey_client_data_create.json");
/// The issuing CA of the recorded attestation is last, so every anchor is parsed.
const TRUST_ANCHORS: [&[u8]; 5] = [
	include_bytes!("../../../webauthn/fixtures/yubico/yubico-fido-attestation-a-1.der"),
	include_bytes!("../../../webauthn/fixtures/yubico/yubico-fido-attestation-b-1.der"),
	include_bytes!("../../../webauthn/fixtures/yubico/yubico-fido-attestation-b2-1.der"),
	include_bytes!("../../../webauthn/fixtures/yubico/yubico-fido-ca-2.der"),
	include_bytes!("../../../webauthn/fixtures/yubico/yubico-fido-ca-1.der"),
];

fn configure_relying_party<T: Config>() {
	RelyingPartyId::<T>::put(BoundedVec::truncate_from(RP_ID.to_vec()));
	TrustAnchors::<T>::put(BoundedVec::truncate_from(
		TRUST_ANCHORS
			.iter()
			.map(|der| BoundedVec::truncate_from(der.to_vec()))
			.collect(),
	));
}

#[benchmarks(where
	T: Send + Sync,
	T::RuntimeCall: From<frame_system::Call<T>> + Dispatchable<Info = frame_support::dispatch::DispatchInfo>,
	<T::RuntimeCall as Dispatchable>::RuntimeOrigin: AsTransactionAuthorizedOrigin,
)]
mod benchmarks {
	use super::*;

	/// Host-function sr25519 verification, for comparison with the WebAuthn paths.
	#[benchmark]
	fn sr25519_verify_baseline() {
		let public = sp_io::crypto::sr25519_generate(0.into(), None);
		let message = [7u8; 32];
		let signature = sp_io::crypto::sr25519_sign(0.into(), &public, &message).unwrap();

		#[block]
		{
			assert!(sp_io::crypto::sr25519_verify(&signature, &message, &public));
		}
	}

	/// Path 1: `CfSignature::WebAuthn` verification over a maximum-size (256 byte) payload.
	#[benchmark]
	fn webauthn_signature_verify() {
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let payload = [7u8; 256];
		let signature = CfSignature::from(WebAuthnSignature {
			public_key: authenticator.public_key(),
			assertion: authenticator.sign(&challenge(&payload)),
		});
		let signer = cf_webauthn::account_id(&authenticator.public_key());

		#[block]
		{
			assert!(signature.verify(&payload[..], &signer));
		}
	}

	/// Path 2: `CheckWebAuthn` validation including the registry lookup.
	#[benchmark]
	fn check_webauthn() {
		configure_relying_party::<T>();
		let authenticator = TestAuthenticator::new(1, RP_ID);
		let public_key = authenticator.public_key();
		let account_id = cf_webauthn::account_id(&public_key);
		Credentials::<T>::insert(
			&account_id,
			Credential { public_key, credential_id: Default::default(), aaguid: [0; 16] },
		);
		let call: T::RuntimeCall = frame_system::Call::<T>::remark { remark: Vec::new() }.into();
		let message = (0u8, &call).using_encoded(sp_io::hashing::blake2_256);
		let extension = CheckWebAuthn::<T>::signed(account_id, authenticator.sign(&message));

		#[block]
		{
			assert!(extension
				.validate_only(
					RawOrigin::None.into(),
					&call,
					&call.get_dispatch_info(),
					0,
					TransactionSource::External,
					0,
				)
				.is_ok());
		}
	}

	#[benchmark]
	fn register_credential() {
		configure_relying_party::<T>();
		frame_system::BlockHash::<T>::insert(
			BlockNumberFor::<T>::zero(),
			T::Hash::decode(&mut &[69u8; 32][..]).unwrap(),
		);
		let sponsor = AccountId32::new([1; 32]);

		#[extrinsic_call]
		register_credential(
			RawOrigin::Signed(sponsor),
			BoundedVec::truncate_from(ATTESTATION_OBJECT.to_vec()),
			BoundedVec::truncate_from(CLIENT_DATA_JSON.to_vec()),
		);

		assert_eq!(Credentials::<T>::iter().count(), 1);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
