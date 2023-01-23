use crate::multisig::{
	crypto::{generate_single_party_signature, ECPoint, ECScalar},
	CryptoScheme, KeyId, Rng,
};
use rand_legacy::SeedableRng;

/// This test covers the specifics of signature generation
/// for a given scheme
fn test_signing_for_scheme<C: CryptoScheme>() {
	let mut rng = Rng::from_seed([0; 32]);

	let secret_key = <C::Point as ECPoint>::Scalar::random(&mut rng);

	let public_key = <C::Point as ECPoint>::from_scalar(&secret_key).as_bytes();

	let payload = C::signing_payload_for_test();

	let signature = generate_single_party_signature::<C>(&secret_key, &payload, &mut rng);

	// Verification is typically delegated to third-party libraries whose
	// behaviour we are attempting to replicate with FROST.
	assert!(C::verify_signature(&signature, &KeyId(public_key.to_vec()), &payload).is_ok());
}

#[test]
fn test_eth_signing() {
	test_signing_for_scheme::<super::eth::EthSigning>();
}

#[test]
fn test_polkadot_signing() {
	test_signing_for_scheme::<super::polkadot::PolkadotSigning>();
}
#[test]
fn test_sui_signing() {
	test_signing_for_scheme::<super::ed25519::Ed25519Signing>();
}
