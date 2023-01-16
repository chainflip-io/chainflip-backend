//! The types and operations as discussed in <https://eprint.iacr.org/2020/852.pdf>.
//! Comments in this file reference sections from this document.
//! Note that unlike the protocol described in the document, we don't have a
//! centralised signature aggregator and don't have a preprocessing stage.
use std::collections::{BTreeMap, BTreeSet};

use cf_primitives::AuthorityCount;

use zeroize::Zeroize;

use crate::multisig::crypto::{CryptoScheme, ECPoint, ECScalar, KeyShare, Rng};

use sha2::{Digest, Sha256};

use super::{signing_data::SigningCommitment, LocalSig3};

/// A pair of secret single-use nonces (and their
/// corresponding public commitments). Correspond to (d,e)
/// generated during the preprocessing stage in Section 5.3 (page 13)
#[derive(Debug, Zeroize)]
pub struct SecretNoncePair<P: ECPoint> {
	pub d: P::Scalar,
	pub d_pub: P,
	pub e: P::Scalar,
	pub e_pub: P,
}

impl<P: ECPoint> SecretNoncePair<P> {
	/// Generate a random pair of nonces (in a Box,
	/// to avoid them being copied on move)
	pub fn sample_random(rng: &mut Rng) -> Box<Self> {
		let d = P::Scalar::random(rng);
		let e = P::Scalar::random(rng);

		let d_pub = P::from_scalar(&d);
		let e_pub = P::from_scalar(&e);

		Box::new(SecretNoncePair { d, d_pub, e, e_pub })
	}
}

/// Combine individual commitments into group (schnorr) commitment.
/// See "Signing Protocol" in Section 5.2 (page 14).
fn gen_group_commitment<P: ECPoint>(
	signing_commitments: &BTreeMap<AuthorityCount, SigningCommitment<P>>,
	bindings: &BTreeMap<AuthorityCount, P::Scalar>,
) -> P {
	signing_commitments
		.iter()
		.map(|(idx, comm)| {
			let rho_i = bindings[idx].clone();
			comm.d + comm.e * rho_i
		})
		.sum()
}

/// Generate a lagrange coefficient for party `signer_index`
/// according to Section 4 (page 9)
pub fn get_lagrange_coeff<P: ECPoint>(
	signer_index: AuthorityCount,
	all_signer_indices: &BTreeSet<AuthorityCount>,
) -> anyhow::Result<P::Scalar> {
	use anyhow::Context;

	let mut num = P::Scalar::from(1);
	let mut den = P::Scalar::from(1);

	for j in all_signer_indices {
		if *j == signer_index {
			continue
		}

		let j = P::Scalar::from(*j);
		let signer_index = P::Scalar::from(signer_index);
		num = num * j.clone();
		den = den * (j - signer_index);
	}

	let lagrange_coeff = num *
		den.invert()
			.context("Can't invert a zero scalar. Processing duplicate shares?")?;

	Ok(lagrange_coeff)
}

/// Generate a "binding value" for party `index`. See "Signing Protocol" in Section 5.2 (page 14)
fn gen_rho_i<P: ECPoint>(
	index: AuthorityCount,
	msg: &[u8],
	signing_commitments: &BTreeMap<AuthorityCount, SigningCommitment<P>>,
	all_idxs: &BTreeSet<AuthorityCount>,
) -> P::Scalar {
	let mut hasher = Sha256::new();
	hasher.update(b"I");
	hasher.update(index.to_be_bytes());
	hasher.update(msg);

	// This needs to be processed in order!

	for idx in all_idxs {
		let com = &signing_commitments[idx];
		hasher.update(idx.to_be_bytes());
		hasher.update(com.d.as_bytes());
		hasher.update(com.e.as_bytes());
	}

	let result = hasher.finalize();

	let x: [u8; 32] = result.as_slice().try_into().expect("Invalid hash size");

	P::Scalar::from_bytes_mod_order(&x)
}

type SigningResponse<P> = LocalSig3<P>;

/// Generate binding values for each party given their previously broadcast commitments
fn generate_bindings<C: CryptoScheme>(
	payload: &C::SigningPayload,
	commitments: &BTreeMap<AuthorityCount, SigningCommitment<C::Point>>,
	all_idxs: &BTreeSet<AuthorityCount>,
) -> BTreeMap<AuthorityCount, <C::Point as ECPoint>::Scalar> {
	all_idxs
		.iter()
		.map(|idx| (*idx, gen_rho_i(*idx, payload.as_ref(), commitments, all_idxs)))
		.collect()
}

/// Generate local signature/response (shard). See step 5 in Figure 3 (page 15).
pub fn generate_local_sig<C: CryptoScheme>(
	payload: &C::SigningPayload,
	key: &KeyShare<C::Point>,
	nonces: &SecretNoncePair<C::Point>,
	commitments: &BTreeMap<AuthorityCount, SigningCommitment<C::Point>>,
	own_idx: AuthorityCount,
	all_idxs: &BTreeSet<AuthorityCount>,
) -> SigningResponse<C::Point> {
	let bindings = generate_bindings::<C>(payload, commitments, all_idxs);

	// This is `R` in a Schnorr signature
	let group_commitment = gen_group_commitment(commitments, &bindings);

	let SecretNoncePair { d, e, .. } = nonces;

	let lambda_i = get_lagrange_coeff::<C::Point>(own_idx, all_idxs).expect("lagrange coeff");

	let rho_i = bindings[&own_idx].clone();

	let nonce_share = rho_i * e + d;

	let key_share = lambda_i * &key.x_i;

	let response =
		generate_schnorr_response::<C>(&key_share, key.y, group_commitment, nonce_share, payload);

	SigningResponse { response }
}

pub fn generate_schnorr_response<C: CryptoScheme>(
	private_key: &<C::Point as ECPoint>::Scalar,
	pubkey: C::Point,
	nonce_commitment: C::Point,
	nonce: <C::Point as ECPoint>::Scalar,
	payload: &C::SigningPayload,
) -> <C::Point as ECPoint>::Scalar {
	let challenge = C::build_challenge(pubkey, nonce_commitment, payload);

	C::build_response(nonce, private_key, challenge)
}

/// Combine local signatures received from all parties into the final
/// (aggregate) signature given that no party misbehaved. Otherwise
/// return the misbehaving parties.
pub fn aggregate_signature<C: CryptoScheme>(
	payload: &C::SigningPayload,
	signer_idxs: &BTreeSet<AuthorityCount>,
	agg_pubkey: C::Point,
	pubkeys: &BTreeMap<AuthorityCount, C::Point>,
	commitments: &BTreeMap<AuthorityCount, SigningCommitment<C::Point>>,
	responses: &BTreeMap<AuthorityCount, SigningResponse<C::Point>>,
) -> Result<C::Signature, BTreeSet<AuthorityCount>> {
	let bindings = generate_bindings::<C>(payload, commitments, signer_idxs);

	let group_commitment = gen_group_commitment(commitments, &bindings);

	let challenge = C::build_challenge(agg_pubkey, group_commitment, payload);

	let invalid_idxs: BTreeSet<AuthorityCount> = signer_idxs
		.iter()
		.copied()
		.filter(|signer_idx| {
			let rho_i = bindings[signer_idx].clone();
			let lambda_i = get_lagrange_coeff::<C::Point>(*signer_idx, signer_idxs).unwrap();

			let commitment = &commitments[signer_idx];
			let commitment_i = commitment.d + (commitment.e * rho_i);

			let y_i = pubkeys[signer_idx];

			let response = &responses[signer_idx];

			!C::is_party_response_valid(
				&y_i,
				&lambda_i,
				&commitment_i,
				&challenge,
				&response.response,
			)
		})
		.collect();

	if invalid_idxs.is_empty() {
		// Response shares/shards are additive, so we simply need to
		// add them together (see step 7.c in Figure 3, page 15).
		let z: <C::Point as ECPoint>::Scalar =
			responses.iter().map(|(_idx, sig)| sig.response.clone()).sum();

		Ok(C::build_signature(z, group_commitment))
	} else {
		Err(invalid_idxs)
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	use crate::multisig::{
		crypto::eth::{EthSigning, Point, Scalar},
		eth::SigningPayload,
	};

	const SECRET_KEY: &str = "fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba";
	const NONCE_KEY: &str = "d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285";
	const MESSAGE_HASH: &str = "2bdc19071c7994f088103dbf8d5476d6deb6d55ee005a2f510dc7640055cc84e";

	// Through integration tests with the KeyManager contract we know this
	// to be deemed valid by the contract for the data above
	const EXPECTED_SIGMA: &str = "beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538";

	#[test]
	fn signature_is_contract_compatible() {
		// Given the signing key, nonce and message hash, check that
		// sigma (signature response) is correct and matches the expected
		// (by the KeyManager contract) value
		let payload = SigningPayload(hex::decode(MESSAGE_HASH).unwrap().try_into().unwrap());

		let nonce = Scalar::from_hex(NONCE_KEY);
		let commitment = Point::from_scalar(&nonce);

		let private_key = Scalar::from_hex(SECRET_KEY);
		let public_key = Point::from_scalar(&private_key);

		let response = generate_schnorr_response::<EthSigning>(
			&private_key,
			public_key,
			commitment,
			nonce,
			&payload,
		);

		assert_eq!(hex::encode(response.as_bytes()), EXPECTED_SIGMA);

		// Build the challenge again to match how it is done on the receiving side
		let challenge = EthSigning::build_challenge(public_key, commitment, &payload);

		// A lambda that has no effect on the computation (as a way to adapt multi-party
		// signing to work for a single party)
		let dummy_lambda = Scalar::from(1);

		assert!(EthSigning::is_party_response_valid(
			&public_key,
			&dummy_lambda,
			&commitment,
			&challenge,
			&response,
		));
	}

	#[test]
	fn bindings_are_backwards_compatible() {
		use rand_legacy::SeedableRng;
		// The seed must not change or the test will break.
		let mut rng = Rng::from_seed([0; 32]);

		let payload = SigningPayload(hex::decode(MESSAGE_HASH).unwrap().try_into().unwrap());
		let idxs = BTreeSet::from_iter(vec![1u32, 2, 3]);
		let commitments: BTreeMap<AuthorityCount, SigningCommitment<Point>> = idxs
			.iter()
			.map(|id| {
				(*id, SigningCommitment { d: Point::random(&mut rng), e: Point::random(&mut rng) })
			})
			.collect();

		let bindings = generate_bindings::<EthSigning>(&payload, &commitments, &idxs);

		// Compare the generated bindings with existing bindings to confirm that the hashing in
		// `gen_rho_i` has not changed.
		assert_eq!(
			hex::encode(bindings.get(&1u32).unwrap().as_bytes()),
			"d21e9745014dedea06fc653b93845b17c20737ef9fe1bac189c70ffb2794250a"
		);
		assert_eq!(
			hex::encode(bindings.get(&2u32).unwrap().as_bytes()),
			"87c25a1056df0e55a359468f76822a7244232e8a339700d24293d7ea3547aad9"
		);
		assert_eq!(
			hex::encode(bindings.get(&3u32).unwrap().as_bytes()),
			"d74a3892851b2f4114fb58cd0a7813dec65a7b5c1bfe6c512091e627a92f512d"
		);
	}
}
