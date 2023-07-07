use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

use cf_primitives::AuthorityCount;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use tracing::warn;
use zeroize::Zeroize;

use crate::{
	client::{
		common::{KeygenFailureReason, ParticipantStatus, ResharingContext},
		KeygenResult, KeygenResultInfo, PartyIdxMapping, ThresholdParameters,
	},
	crypto::{ECPoint, ECScalar, KeyShare, Rng},
	CryptoScheme,
};

use super::keygen_data::HashComm1;

/// Evaluate polynomial f(x) = c0 + c1 * x + c2 * x^2 + ... (expressed as
/// an iterator over its coefficients [c0, c1, c2, ...]) at x = index
fn evaluate_polynomial<'a, T, I, Scalar: ECScalar>(coefficients: I, index: AuthorityCount) -> T
where
	T: 'a + Clone,
	T: std::ops::Mul<Scalar, Output = T>,
	T: std::ops::Add<T, Output = T>,
	I: DoubleEndedIterator<Item = &'a T>,
{
	coefficients
		.rev()
		.cloned()
		.reduce(|acc, coefficient| acc * Scalar::from(index) + coefficient)
		.unwrap()
}

#[test]
fn test_simple_polynomial() {
	use crate::crypto::eth::Scalar;

	// f(x) = 4 + 5x + 2x^2
	let secret = Scalar::from(4);
	let coefficients = [Scalar::from(5), Scalar::from(2)];

	// f(3) = 4 + 15 + 18 = 37
	let value: Scalar =
		evaluate_polynomial::<_, _, Scalar>([secret].iter().chain(coefficients.iter()), 3);
	assert_eq!(value, Scalar::from(37));
}

/// Evaluation of a sharing polynomial for a given party index
/// as per Shamir Secret Sharing scheme
#[derive(
	Debug, Default, Clone, Deserialize, Serialize, Zeroize, PartialOrd, Ord, PartialEq, Eq,
)]
pub struct ShamirShare<P: ECPoint> {
	/// the result of polynomial evaluation
	pub value: P::Scalar,
}

#[cfg(test)]
impl<P: ECPoint> ShamirShare<P> {
	pub fn create_random(rng: &mut Rng) -> Self {
		ShamirShare { value: P::Scalar::random(rng) }
	}
}

/// Test-only helper function used to sanity check our sharing polynomial
#[cfg(test)]
fn reconstruct_secret<P: ECPoint>(shares: &BTreeMap<AuthorityCount, ShamirShare<P>>) -> P::Scalar {
	use crate::client::signing;

	let all_idxs: BTreeSet<AuthorityCount> = shares.keys().cloned().collect();

	shares.iter().fold(P::Scalar::zero(), |acc, (index, ShamirShare { value })| {
		acc + signing::get_lagrange_coeff::<P>(*index, &all_idxs) * value
	})
}

/// Context used in hashing to prevent replay attacks
pub struct HashContext(pub [u8; 32]);

/// Generate challenge against which a ZKP of our secret will be generated
fn generate_dkg_challenge<P: ECPoint>(
	index: AuthorityCount,
	context: &HashContext,
	public: P,
	commitment: P,
) -> P::Scalar {
	use blake2::{Blake2b, Digest};

	let mut hasher = Blake2b::<typenum::U32>::new();

	hasher.update(public.as_bytes());
	hasher.update(commitment.as_bytes());

	hasher.update(index.to_be_bytes());
	hasher.update(context.0);

	let result = hasher.finalize();

	let x: [u8; 32] = result.as_slice().try_into().expect("Invalid hash size");

	P::Scalar::from_bytes_mod_order(&x)
}

/// Generate ZKP (zero-knowledge proof) of `secret`
fn generate_zkp_of_secret<Point: ECPoint>(
	rng: &mut Rng,
	secret: Point::Scalar,
	context: &HashContext,
	index: AuthorityCount,
) -> ZKPSignature<Point> {
	let nonce = Point::Scalar::random(rng);
	let nonce_commitment = Point::from_scalar(&nonce);

	let secret_commitment = Point::from_scalar(&secret);

	let challenge = generate_dkg_challenge(index, context, secret_commitment, nonce_commitment);

	let z = nonce + secret * challenge;

	ZKPSignature { r: nonce_commitment, z }
}

#[derive(Default)]
pub struct OutgoingShares<P: ECPoint>(pub BTreeMap<AuthorityCount, ShamirShare<P>>);

pub struct IncomingShares<P: ECPoint>(pub BTreeMap<AuthorityCount, ShamirShare<P>>);

#[derive(PartialOrd, Eq, Ord, PartialEq, Copy, Clone, Debug)]
pub struct IndexPair {
	/// Party index in the current ceremony
	pub current_index: AuthorityCount,
	/// Party index in consequent signing ceremonies, which
	/// is used to evaluate sharing polynomial at
	pub future_index: AuthorityCount,
}

pub struct SharingParameters {
	// The indexes of the parties that will receive new shares
	pub indexes_to_share_at: BTreeSet<IndexPair>,
	// Parameters for the new aggregate key (after resharing)
	pub key_params: ThresholdParameters,
}

impl SharingParameters {
	pub fn for_key_handover<C: CryptoScheme>(
		// Parameters for the new key
		key_params: ThresholdParameters,
		context: &ResharingContext<C>,
		// The mapping is for the current ceremony
		// (the union between old and new nodes)
		current_party_mapping: &Arc<PartyIdxMapping>,
	) -> Self {
		let indexes_to_share_at = context
			.receiving_participants
			.iter()
			.map(|idx| {
				let id = current_party_mapping.get_id(*idx);
				let future_index = context
					.future_index_mapping
					.get_idx(id)
					.expect("receiving party must have a future index");
				IndexPair { current_index: *idx, future_index }
			})
			.collect();

		Self { indexes_to_share_at, key_params }
	}

	pub fn for_keygen(key_params: ThresholdParameters) -> Self {
		// In regular keygen, all parties receive shares; current indices
		// match future indices
		let indexes_to_share_at = (1..=key_params.share_count)
			.map(|idx| IndexPair { current_index: idx, future_index: idx })
			.collect();
		Self { indexes_to_share_at, key_params }
	}
}

/// Generate a secret and derive shares and commitments from it.
/// (The secret will never be needed again, so it is not exposed
/// to the caller.)
pub fn generate_shares_and_commitment<P: ECPoint>(
	rng: &mut Rng,
	context: &HashContext,
	index: AuthorityCount,
	sharing_parameters: &SharingParameters,
	existing_secret: Option<&P::Scalar>,
) -> (OutgoingShares<P>, DKGUnverifiedCommitment<P>) {
	let (secret, commitments, shares) =
		generate_secret_and_shares(rng, sharing_parameters, existing_secret);

	// Zero-knowledge proof of `secret`
	let zkp = generate_zkp_of_secret(rng, secret, context, index);

	// Secret will be zeroized on drop here

	(OutgoingShares(shares), DKGUnverifiedCommitment { commitments, zkp })
}

// NOTE: shares should be sent after participants have exchanged commitments
fn generate_secret_and_shares<P: ECPoint>(
	rng: &mut Rng,
	sharing_parameters: &SharingParameters,
	existing_secret: Option<&P::Scalar>,
) -> (P::Scalar, CoefficientCommitments<P>, BTreeMap<AuthorityCount, ShamirShare<P>>) {
	// Our secret contribution to the aggregate key
	let secret = existing_secret.cloned().unwrap_or_else(|| P::Scalar::random(rng));

	// Coefficients for the sharing polynomial used to share `secret` via the Shamir Secret Sharing
	// scheme (Figure 1: Round 1, Step 1)
	let coefficients: Vec<_> = (0..sharing_parameters.key_params.threshold)
		.map(|_| P::Scalar::random(rng))
		.collect();

	// (Figure 1: Round 1, Step 3)
	let commitments: Vec<_> =
		[secret.clone()].iter().chain(&coefficients).map(P::from_scalar).collect();

	// TODO: don't bother creating shares if you are not a sharing party

	// Generate shares
	// (Figure 1: Round 2, Step 1)
	// NOTE: we only generate shares for the parties that need to receive them
	// (mostly relevant for Key Handover where some parties don't receive shares)
	let shares = sharing_parameters
		.indexes_to_share_at
		.iter()
		.map(|IndexPair { current_index, future_index }| {
			let share = ShamirShare {
				// NOTE: we want to evaluate at party's future index,
				// not the index in the current ceremony
				// TODO: Make this work on references
				value: evaluate_polynomial::<_, _, P::Scalar>(
					[secret.clone()].iter().chain(coefficients.iter()),
					*future_index,
				),
			};

			(*current_index, share)
		})
		.collect();

	// Coefficients are zeroized on drop here
	(secret, CoefficientCommitments(commitments), shares)
}

fn is_valid_zkp<P: ECPoint>(
	challenge: P::Scalar,
	zkp: &ZKPSignature<P>,
	comm: &CoefficientCommitments<P>,
) -> bool {
	zkp.r + comm.0[0] * challenge == P::from_scalar(&zkp.z)
}

// (Figure 1: Round 2, Step 2)
pub fn verify_share<P: ECPoint>(
	share: &ShamirShare<P>,
	com: &DKGCommitment<P>,
	index: AuthorityCount,
) -> bool {
	P::from_scalar(&share.value) ==
		evaluate_polynomial::<_, _, P::Scalar>(com.commitments.0.iter(), index)
}

/// Commitments to the sharing polynomial coefficient
#[derive(Debug, Clone, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
struct CoefficientCommitments<P>(Vec<P>);

/// Zero-knowledge proof of us knowing the secret
/// (in a form of a Schnorr signature)
#[derive(Debug, Clone, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
struct ZKPSignature<P: ECPoint> {
	#[serde(bound = "")]
	r: P,
	z: P::Scalar,
}

/// Commitments along with the corresponding ZKP
/// which should be sent to other parties at the
/// beginning of the ceremony
#[derive(Clone, Debug, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
pub struct DKGUnverifiedCommitment<P: ECPoint> {
	#[serde(bound = "")]
	commitments: CoefficientCommitments<P>,
	#[serde(bound = "")]
	zkp: ZKPSignature<P>,
}

pub use serialisation::MAX_COEFF_COMM_3_SIZE;

mod serialisation {

	use state_chain_runtime::constants::common::MAX_AUTHORITIES;

	use crate::crypto::{MAX_POINT_SIZE, MAX_SCALAR_SIZE};

	const MAX_ZKP_SIZE: usize = MAX_POINT_SIZE + MAX_SCALAR_SIZE;

	const MAX_COEFFICIENTS: usize =
		utilities::threshold_from_share_count(MAX_AUTHORITIES) as usize + 1;

	// NOTE: 8 bytes for the length of the vector
	pub const MAX_COEFF_COMM_3_SIZE: usize = MAX_ZKP_SIZE + MAX_POINT_SIZE * MAX_COEFFICIENTS + 8;

	#[cfg(test)]
	mod tests {
		use crate::{client::helpers::test_all_crypto_schemes, ChainTag, CryptoScheme};

		#[test]
		fn check_comm3_max_size() {
			test_all_crypto_schemes!(check_comm3_size_for_scheme());
		}

		fn check_comm3_size_for_scheme<C: CryptoScheme>() {
			// Generate Comm3 data for MAX_AUTHORITIES and check
			// that its size is no greater than MAX_COEFF_COMM_3_SIZE
			// for a given scheme:

			use super::{super::*, *};
			use crate::client::{common::DelayDeserialization, keygen::CoeffComm3};

			use rand::SeedableRng;

			let mut rng = rand::rngs::StdRng::from_seed([0u8; 32]);

			let params = ThresholdParameters::from_share_count(MAX_AUTHORITIES);

			let context = HashContext([0; 32]);

			let (secret, shares_commitments, _shares) =
				generate_secret_and_shares::<<C as CryptoScheme>::Point>(
					&mut rng,
					&SharingParameters::for_keygen(params),
					None,
				);
			// Zero-knowledge proof of `secret`
			let zkp = generate_zkp_of_secret(&mut rng, secret, &context, 1 /* own index */);
			let zkp_bytes = bincode::serialize(&zkp).unwrap();

			let dkg_commitment = DKGUnverifiedCommitment { commitments: shares_commitments, zkp };

			let comm3: CoeffComm3<<C as CryptoScheme>::Point> =
				DelayDeserialization::new(&dkg_commitment);

			if matches!(<C as CryptoScheme>::CHAIN_TAG, ChainTag::Ethereum) {
				// The constants are defined as to exactly match Ethereum/secp256k1,
				// which we demonstrate here:
				assert!(comm3.payload.len() == MAX_COEFF_COMM_3_SIZE);
				assert!(zkp_bytes.len() == MAX_ZKP_SIZE);
			} else {
				// Other chains might use a more compact serialization of primitives
				assert!(zkp_bytes.len() <= MAX_ZKP_SIZE);
				assert!(comm3.payload.len() <= MAX_COEFF_COMM_3_SIZE);
			}
		}
	}
}

/// Commitments that have already been checked against the ZKP
#[derive(Debug)]
pub struct DKGCommitment<P: ECPoint> {
	commitments: CoefficientCommitments<P>,
}

fn is_valid_hash_commitment<P: ECPoint>(
	public_coefficients: &DKGUnverifiedCommitment<P>,
	hash_commitment: &H256,
) -> bool {
	hash_commitment == &generate_hash_commitment(public_coefficients)
}

// (Figure 1: Round 1, Step 5)
pub fn validate_commitments<C: CryptoScheme>(
	public_coefficients: BTreeMap<AuthorityCount, DKGUnverifiedCommitment<C::Point>>,
	hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
	resharing_context: Option<&ResharingContext<C>>,
	context: &HashContext,
	validator_mapping: Arc<PartyIdxMapping>,
) -> Result<
	BTreeMap<AuthorityCount, DKGCommitment<C::Point>>,
	(BTreeSet<AuthorityCount>, KeygenFailureReason),
> {
	let invalid_idxs: BTreeSet<_> = public_coefficients
		.iter()
		.filter_map(|(idx, c)| {
			if let Some(context) = resharing_context {
				let expected_public_keys = match &context.party_status {
					ParticipantStatus::Sharing { public_key_shares, .. } => public_key_shares,
					ParticipantStatus::NonSharing => panic!("invalid state for the stage"),
					ParticipantStatus::NonSharingReceivedKeys(public_key_shares) =>
						public_key_shares,
				};

				// In a key handover ceremony, we check for each sharing party
				// that the commitment to their first coefficient corresponds
				// to the party's original pubkey share (scaled by lagrange coefficient).
				if context.sharing_participants.contains(idx) {
					let id = validator_mapping.get_id(*idx);
					let expected_pubkey = expected_public_keys
						.get(id)
						.expect("must have keys for all sharing parties");

					if expected_pubkey != &c.commitments.0[0] {
						warn!(from_id = id.to_string(), "Invalid first commitment");
						return Some(*idx)
					}
				}
			}

			let challenge = generate_dkg_challenge(*idx, context, c.commitments.0[0], c.zkp.r);

			let hash_commitment = hash_commitments
				.get(idx)
				.expect("message must be present due to ceremony runner invariants");

			if !is_valid_zkp(challenge, &c.zkp, &c.commitments) {
				warn!(
					from_id = validator_mapping.get_id(*idx).to_string(),
					"Invalid ZKP commitment"
				);
				Some(*idx)
			} else if !is_valid_hash_commitment(c, &hash_commitment.0) {
				warn!(
					from_id = validator_mapping.get_id(*idx).to_string(),
					"Invalid hash commitment"
				);
				Some(*idx)
			} else {
				None
			}
		})
		.collect();

	if invalid_idxs.is_empty() {
		Ok(public_coefficients
			.into_iter()
			.map(|(idx, c)| (idx, DKGCommitment { commitments: c.commitments }))
			.collect())
	} else {
		Err((invalid_idxs, KeygenFailureReason::InvalidCommitment))
	}
}

pub struct ValidAggregateKey<P: ECPoint>(pub P);

/// Derive aggregate pubkey from party commitments. The resulting
/// key might be incompatible according to [C::is_pubkey_compatible].
pub fn derive_aggregate_pubkey<C: CryptoScheme>(
	commitments: &BTreeMap<AuthorityCount, DKGCommitment<C::Point>>,
) -> ValidAggregateKey<C::Point> {
	let pubkey: C::Point = commitments.iter().map(|(_idx, c)| c.commitments.0[0]).sum();

	if check_high_degree_commitments(commitments) {
		// Sanity check (the chance of this failing is infinitesimal due to the
		// hash commitment stage at the beginning of the ceremony)
		panic!("high degree coefficient is zero");
	}

	ValidAggregateKey(pubkey)
}

pub fn derive_local_pubkeys_for_parties<P: ECPoint>(
	sharing_params: &SharingParameters,
	commitments: &BTreeMap<AuthorityCount, DKGCommitment<P>>,
) -> BTreeMap<AuthorityCount, P> {
	// Recall that each party i's secret key share `s` is the sum
	// of secret shares they receive from all other parties, which
	// are in turn calculated by evaluating each party's sharing
	// polynomial `f(x)` at `x = i`. We can derive `G * s` (unlike
	// `s` itself), because we know `G * f(x)` from coefficient
	// commitments.
	// I.e. y_i = G * f_1(i) + G * f_2(i) + ... G * f_n(i), where
	// G * f_j(i) = G * s_j + G * c_j_1(i) + G * c_j_2(i) + ... + c_j_{t-1}(i)

	use rayon::prelude::*;

	// TODO: As a sanity check, assert that commitments are only from sharing parties

	sharing_params
		.indexes_to_share_at
		.par_iter()
		.map(|IndexPair { current_index, future_index }| {
			(
				*current_index,
				commitments
					.values()
					.map(|party_commitments| {
						evaluate_polynomial::<_, _, P::Scalar>(
							(0..=sharing_params.key_params.threshold)
								.map(|k| &party_commitments.commitments.0[k as usize]),
							*future_index,
						)
					})
					.sum(),
			)
		})
		.collect()
}

pub fn generate_hash_commitment<P: ECPoint>(
	coefficient_commitments: &DKGUnverifiedCommitment<P>,
) -> H256 {
	use blake2::{Blake2b, Digest};

	let mut hasher = Blake2b::<typenum::U32>::new();

	for comm in &coefficient_commitments.commitments.0 {
		hasher.update(bincode::serialize(&comm).expect("serialization can't fail"));
	}

	H256::from(hasher.finalize().as_ref())
}

/// We don't want the coefficient commitments to add up to the "point at infinity" as this
/// corresponds to the sum of the actual coefficient being zero, which would reduce the degree of
/// the sharing polynomial (in Shamir Secret Sharing) and thus would reduce the effective threshold
/// of the aggregate key
fn check_high_degree_commitments<P: ECPoint>(
	commitments: &BTreeMap<AuthorityCount, DKGCommitment<P>>,
) -> bool {
	let high_degree_sum: P =
		commitments.values().map(|c| c.commitments.0.last().copied().unwrap()).sum();

	high_degree_sum.is_point_at_infinity()
}

pub fn compute_secret_key_share<P: ECPoint>(secret_shares: IncomingShares<P>) -> P::Scalar {
	// Note: the shares in secret_shares will be zeroized on drop here
	secret_shares.0.values().map(|share| share.value.clone()).sum()
}

#[cfg(test)]
impl<P: ECPoint> DKGUnverifiedCommitment<P> {
	/// Change the lowest degree coefficient so that it fails ZKP check
	pub fn corrupt_primary_coefficient(&mut self, rng: &mut Rng) {
		self.commitments.0[0] = P::from_scalar(&P::Scalar::random(rng));
	}

	/// Change a higher degree coefficient, so that it fails hash commitment check
	pub fn corrupt_secondary_coefficient(&mut self, rng: &mut Rng) {
		self.commitments.0[1] = P::from_scalar(&P::Scalar::random(rng));
	}
}

#[cfg(test)]
mod tests {

	use utilities::assert_ok;

	use crate::eth::EthSigning;

	use super::*;

	#[test]
	fn basic_sharing() {
		use crate::crypto::eth::Point;

		let params = ThresholdParameters { share_count: 7, threshold: 5 };

		use rand::SeedableRng;
		let mut rng = Rng::from_seed([0; 32]);

		let (secret, _commitments, shares) = generate_secret_and_shares::<Point>(
			&mut rng,
			&SharingParameters::for_keygen(params),
			None,
		);

		assert_eq!(secret, reconstruct_secret(&shares));
	}

	#[test]
	fn keygen_sequential() {
		use crate::crypto::eth::{Point, Scalar};
		use state_chain_runtime::AccountId;

		let params = ThresholdParameters { share_count: 4, threshold: 2 };

		let context = HashContext([0; 32]);

		use rand::SeedableRng;
		let mut rng = Rng::from_seed([0; 32]);

		let (commitments, hash_commitments, outgoing_shares): (
			BTreeMap<_, _>,
			BTreeMap<_, _>,
			BTreeMap<_, _>,
		) = itertools::multiunzip((1..=params.share_count).map(|idx| {
			let (secret, shares_commitments, shares) =
				generate_secret_and_shares(&mut rng, &SharingParameters::for_keygen(params), None);
			// Zero-knowledge proof of `secret`
			let zkp = generate_zkp_of_secret(&mut rng, secret, &context, idx);

			let dkg_commitment = DKGUnverifiedCommitment { commitments: shares_commitments, zkp };

			let hash_commitment = generate_hash_commitment(&dkg_commitment);

			((idx, dkg_commitment), (idx, HashComm1(hash_commitment)), (idx, shares))
		}));

		let coeff_commitments = assert_ok!(validate_commitments::<EthSigning>(
			commitments,
			hash_commitments,
			None,
			&context,
			Arc::new(PartyIdxMapping::from_participants(BTreeSet::from_iter(
				(1..=params.share_count as u8).map(|i| AccountId::new([i; 32]))
			))),
		));

		// Now it is okay to distribute the shares
		let _agg_pubkey: Point = coeff_commitments.values().map(|c| c.commitments.0[0]).sum();

		let mut secret_shares = vec![];

		for receiver_idx in 1..=params.share_count {
			let received_shares: Vec<_> = outgoing_shares
				.iter()
				.map(|(idx, shares)| {
					let share = shares[&receiver_idx].clone();
					assert!(verify_share(&share, &coeff_commitments[idx], receiver_idx));
					share
				})
				.collect();

			// (Round 2, Step 3)
			let secret_share: Scalar =
				received_shares.iter().map(|share| share.value.clone()).sum();

			secret_shares.push(secret_share);
		}
	}
}

pub mod genesis {

	use std::collections::HashMap;

	use super::*;
	use crate::{client::PartyIdxMapping, eth::EthSigning};
	use state_chain_runtime::AccountId;

	/// Generate keys for all participants in a centralised manner.
	/// (Useful for testing and genesis keygen)
	fn generate_key_data_detail<C: CryptoScheme>(
		participants: BTreeSet<AccountId>,
		initial_key_must_be_incompatible: bool,
		rng: &mut Rng,
	) -> (C::PublicKey, HashMap<AccountId, KeygenResultInfo<C>>) {
		let params = ThresholdParameters::from_share_count(participants.len() as AuthorityCount);

		let (commitments, outgoing_secret_shares, agg_pubkey) = loop {
			let (commitments, outgoing_secret_shares): (BTreeMap<_, _>, BTreeMap<_, _>) = (1..=
				params.share_count)
				.map(|idx| {
					let (_secret, commitments, shares) = generate_secret_and_shares::<C::Point>(
						rng,
						&SharingParameters::for_keygen(params),
						None,
					);
					((idx, DKGCommitment { commitments }), (idx, shares))
				})
				.unzip();

			let agg_pubkey = derive_aggregate_pubkey::<C>(&commitments);

			if !initial_key_must_be_incompatible || !C::is_pubkey_compatible(&agg_pubkey.0) {
				break (commitments, outgoing_secret_shares, agg_pubkey)
			}
		};

		let validator_mapping = PartyIdxMapping::from_participants(participants);

		// Local pubkeys for parties are the same for all parties,
		// so we derive them only once:
		let local_pubkeys: BTreeMap<_, _> = derive_local_pubkeys_for_parties::<C::Point>(
			&SharingParameters::for_keygen(params),
			&commitments,
		)
		.into_iter()
		.map(|(idx, pk)| (validator_mapping.get_id(idx).clone(), pk))
		.collect();

		let keygen_result_infos: HashMap<_, _> = (1..=params.share_count)
			.map(|idx| {
				// Collect shares destined for `idx`
				let incoming_shares: BTreeMap<_, _> = outgoing_secret_shares
					.iter()
					.map(|(sender_idx, shares)| (*sender_idx, shares[&idx].clone()))
					.collect();

				(
					validator_mapping.get_id(idx).clone(),
					KeygenResultInfo {
						key: Arc::new(KeygenResult::new_compatible(
							KeyShare {
								y: agg_pubkey.0,
								x_i: compute_secret_key_share(IncomingShares(incoming_shares)),
							},
							local_pubkeys.clone(),
						)),
						validator_mapping: Arc::new(validator_mapping.clone()),
						params,
					},
				)
			})
			.collect();

		let agg_key: C::PublicKey =
			keygen_result_infos.values().next().unwrap().key.get_agg_public_key();
		(agg_key, keygen_result_infos)
	}

	pub fn generate_key_data<C: CryptoScheme>(
		signers: BTreeSet<AccountId>,
		rng: &mut Rng,
	) -> (C::PublicKey, HashMap<AccountId, KeygenResultInfo<C>>) {
		generate_key_data_detail(signers, false, rng)
	}

	pub fn generate_key_data_with_initial_incompatibility(
		signers: BTreeSet<AccountId>,
		rng: &mut Rng,
	) -> (<EthSigning as CryptoScheme>::PublicKey, HashMap<AccountId, KeygenResultInfo<EthSigning>>)
	{
		generate_key_data_detail(signers, true, rng)
	}
}

/// Generates key data using a default seed and returns the KeygenResultInfo for the
/// first signer.
#[cfg(feature = "test")]
pub fn get_key_data_for_test<C: CryptoScheme>(
	signers: BTreeSet<cf_primitives::AccountId>,
) -> KeygenResultInfo<C> {
	super::generate_key_data::<C>(
		signers.clone(),
		&mut <Rng as rand::SeedableRng>::from_seed([8; 32]),
	)
	.1
	.get(signers.iter().next().unwrap())
	.expect("should get keygen for an account")
	.to_owned()
}
