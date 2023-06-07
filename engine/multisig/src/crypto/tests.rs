use std::collections::BTreeMap;

use crate::{
	client::KeygenResult,
	crypto::{generate_single_party_signature, ECPoint, ECScalar, KeyShare},
	test_all_crypto_schemes, CryptoScheme, Rng,
};
use cf_primitives::AccountId;
use rand::SeedableRng;

/// This test covers the specifics of signature generation
/// for a given scheme
fn test_signing_for_scheme<C: CryptoScheme>() {
	let mut rng = Rng::from_seed([0; 32]);

	// Running this multiple times will ensure that we produce various keys with potential
	// incompatibilities that should get fixed by the code This applies mostly to Ethereum and
	// Bitcoin keys, where not every random private key is valid.
	for _ in 0..10 {
		let secret_key = <C::Point as ECPoint>::Scalar::random(&mut rng);
		let public_key = <C::Point as ECPoint>::from_scalar(&secret_key);

		let my_key_share = KeyShare { x_i: secret_key, y: public_key };
		let my_keygen_result: KeygenResult<C> = KeygenResult::new_compatible(
			my_key_share,
			BTreeMap::from_iter(vec![(AccountId::new([0; 32]), public_key)]),
		);
		let secret_key = my_keygen_result.key_share.x_i.clone();

		let agg_key = my_keygen_result.get_agg_public_key();

		let payload = C::signing_payload_for_test();

		let signature = generate_single_party_signature::<C>(&secret_key, &payload, &mut rng);

		// Verification is typically delegated to third-party libraries whose
		// behaviour we are attempting to replicate with FROST.
		assert!(C::verify_signature(&signature, &agg_key, &payload).is_ok());
	}
}

#[test]
fn test_signing_for_all_schemes() {
	test_all_crypto_schemes!(test_signing_for_scheme());
}
