use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use cf_traits::AuthorityCount;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use zeroize::Zeroize;

use anyhow::anyhow;

use crate::multisig::{
    client::{common::KeygenFailureReason, KeygenResult, KeygenResultInfo, ThresholdParameters},
    crypto::{ECPoint, ECScalar, KeyShare, Rng},
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
    use crate::multisig::crypto::eth::Scalar;

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
#[derive(Debug, Default, Clone, Deserialize, Serialize, Zeroize)]
pub struct ShamirShare<P: ECPoint> {
    /// the result of polynomial evaluation
    pub value: P::Scalar,
}

#[cfg(test)]
impl<P: ECPoint> ShamirShare<P> {
    pub fn create_random(rng: &mut Rng) -> Self {
        ShamirShare {
            value: P::Scalar::random(rng),
        }
    }
}

/// Test-only helper function used to sanity check our sharing polynomial
#[cfg(test)]
fn reconstruct_secret<P: ECPoint>(shares: &BTreeMap<AuthorityCount, ShamirShare<P>>) -> P::Scalar {
    use crate::multisig::client::signing::frost;

    let all_idxs: BTreeSet<AuthorityCount> = shares.keys().into_iter().cloned().collect();

    shares
        .iter()
        .fold(P::Scalar::zero(), |acc, (index, ShamirShare { value })| {
            acc + frost::get_lagrange_coeff::<P>(*index, &all_idxs).unwrap() * value
        })
}

/// Context used in hashing to prevent replay attacks
#[derive(Clone)]
pub struct HashContext(pub [u8; 32]);

/// Generate challenge against which a ZKP of our secret will be generated
fn generate_dkg_challenge<P: ECPoint>(
    index: AuthorityCount,
    context: &HashContext,
    public: P,
    commitment: P,
) -> P::Scalar {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

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

    ZKPSignature {
        r: nonce_commitment,
        z,
    }
}

#[derive(Default)]
pub struct OutgoingShares<P: ECPoint>(pub BTreeMap<AuthorityCount, ShamirShare<P>>);

pub struct IncomingShares<P: ECPoint>(pub BTreeMap<AuthorityCount, ShamirShare<P>>);

/// Generate a secret and derive shares and commitments from it.
/// (The secret will never be needed again, so it is not exposed
/// to the caller.)
pub fn generate_shares_and_commitment<P: ECPoint>(
    rng: &mut Rng,
    context: &HashContext,
    index: AuthorityCount,
    params: ThresholdParameters,
) -> (OutgoingShares<P>, DKGUnverifiedCommitment<P>) {
    let (secret, commitments, shares) =
        generate_secret_and_shares(rng, params.share_count, params.threshold);

    // Zero-knowledge proof of `secret`
    let zkp = generate_zkp_of_secret(rng, secret, context, index);

    // Secret will be zeroized on drop here

    (
        OutgoingShares(shares),
        DKGUnverifiedCommitment { commitments, zkp },
    )
}

// NOTE: shares should be sent after participants have exchanged commitments
fn generate_secret_and_shares<P: ECPoint>(
    rng: &mut Rng,
    n: AuthorityCount,
    t: AuthorityCount,
) -> (
    P::Scalar,
    CoefficientCommitments<P>,
    BTreeMap<AuthorityCount, ShamirShare<P>>,
) {
    // Our secret contribution to the aggregate key
    let secret = P::Scalar::random(rng);

    // Coefficients for the sharing polynomial used to share `secret` via the Shamir Secret Sharing scheme
    // (Figure 1: Round 1, Step 1)
    let coefficients: Vec<_> = (0..t).into_iter().map(|_| P::Scalar::random(rng)).collect();

    // (Figure 1: Round 1, Step 3)
    let commitments: Vec<_> = [secret.clone()]
        .iter()
        .chain(&coefficients)
        .map(P::from_scalar)
        .collect();

    // Generate shares
    // (Figure 1: Round 2, Step 1)
    let shares = (1..=n)
        .map(|index| {
            (
                index,
                ShamirShare {
                    // TODO: Make this work on references
                    value: evaluate_polynomial::<_, _, P::Scalar>(
                        [secret.clone()].iter().chain(coefficients.iter()),
                        index,
                    ),
                },
            )
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
    P::from_scalar(&share.value)
        == evaluate_polynomial::<_, _, P::Scalar>(com.commitments.0.iter(), index)
}

/// Commitments to the sharing polynomial coefficient
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoefficientCommitments<P>(Vec<P>);

/// Zero-knowledge proof of us knowing the secret
/// (in a form of a Schnorr signature)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZKPSignature<P: ECPoint> {
    #[serde(bound = "")]
    r: P,
    z: P::Scalar,
}

/// Commitments along with the corresponding ZKP
/// which should be sent to other parties at the
/// beginning of the ceremony
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DKGUnverifiedCommitment<P: ECPoint> {
    #[serde(bound = "")]
    commitments: CoefficientCommitments<P>,
    #[serde(bound = "")]
    zkp: ZKPSignature<P>,
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
pub fn validate_commitments<P: ECPoint>(
    public_coefficients: BTreeMap<AuthorityCount, DKGUnverifiedCommitment<P>>,
    hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
    context: &HashContext,
    logger: &slog::Logger,
) -> Result<
    BTreeMap<AuthorityCount, DKGCommitment<P>>,
    (BTreeSet<AuthorityCount>, KeygenFailureReason),
> {
    let invalid_idxs: BTreeSet<_> = public_coefficients
        .iter()
        .filter_map(|(idx, c)| {
            let challenge = generate_dkg_challenge(*idx, context, c.commitments.0[0], c.zkp.r);

            let hash_commitment = hash_commitments
                .get(idx)
                .expect("message must be present due to ceremony runner invariants");

            if !is_valid_zkp(challenge, &c.zkp, &c.commitments) {
                slog::warn!(logger, "Invalid ZKP commitment from party: {}", idx);
                Some(*idx)
            } else if !is_valid_hash_commitment(c, &hash_commitment.0) {
                slog::warn!(logger, "Invalid hash commitment for party: {}", idx);
                Some(*idx)
            } else {
                None
            }
        })
        .collect();

    if invalid_idxs.is_empty() {
        Ok(public_coefficients
            .into_iter()
            .map(|(idx, c)| {
                (
                    idx,
                    DKGCommitment {
                        commitments: c.commitments,
                    },
                )
            })
            .collect())
    } else {
        Err((invalid_idxs, KeygenFailureReason::InvalidCommitment))
    }
}

pub struct ValidAggregateKey<P: ECPoint>(pub P);

/// Derive aggregate pubkey from party commitments
/// Note that setting `allow_high_pubkey` to true lets us skip compatibility check
/// in tests as otherwise they would be non-deterministic
pub fn derive_aggregate_pubkey<P: ECPoint>(
    commitments: &BTreeMap<AuthorityCount, DKGCommitment<P>>,
    allow_high_pubkey: bool,
) -> anyhow::Result<ValidAggregateKey<P>> {
    let pubkey: P = commitments.iter().map(|(_idx, c)| c.commitments.0[0]).sum();

    if !allow_high_pubkey && !pubkey.is_compatible() {
        Err(anyhow!("pubkey is not compatible"))
    } else if check_high_degree_commitments(commitments) {
        // Sanity check (the chance of this failing is infinitesimal due to the
        // hash commitment stage at the beginning of the ceremony)
        Err(anyhow!("high degree coefficient is zero"))
    } else {
        Ok(ValidAggregateKey(pubkey))
    }
}

/// Derive each party's "local" pubkey
pub fn derive_local_pubkeys_for_parties<P: ECPoint>(
    ThresholdParameters {
        share_count: n,
        threshold: t,
    }: ThresholdParameters,
    commitments: &BTreeMap<AuthorityCount, DKGCommitment<P>>,
) -> Vec<P> {
    // Recall that each party i's secret key share `s` is the sum
    // of secret shares they receive from all other parties, which
    // are in turn calculated by evaluating each party's sharing
    // polynomial `f(x)` at `x = i`. We can derive `G * s` (unlike
    // `s` itself), because we know `G * f(x)` from coefficient
    // commitments.
    // I.e. y_i = G * f_1(i) + G * f_2(i) + ... G * f_n(i), where
    // G * f_j(i) = G * s_j + G * c_j_1(i) + G * c_j_2(i) + ... + c_j_{t-1}(i)

    use rayon::prelude::*;

    (1..=n)
        .into_par_iter()
        .map(|idx| {
            (1..=n)
                .map(|j| {
                    evaluate_polynomial::<_, _, P::Scalar>(
                        (0..=t).map(|k| &commitments[&j].commitments.0[k as usize]),
                        idx,
                    )
                })
                .sum()
        })
        .collect()
}

pub fn generate_hash_commitment<P: ECPoint>(
    coefficient_commitments: &DKGUnverifiedCommitment<P>,
) -> H256 {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    for comm in &coefficient_commitments.commitments.0 {
        hasher.update(bincode::serialize(&comm).expect("serialization can't fail"));
    }

    H256::from(hasher.finalize().as_ref())
}

/// We don't want the coefficient commitments to add up to the "point at infinity" as this corresponds
/// to the sum of the actual coefficient being zero, which would reduce the degree of the sharing polynomial
/// (in Shamir Secret Sharing) and thus would reduce the effective threshold of the aggregate key
pub fn check_high_degree_commitments<P: ECPoint>(
    commitments: &BTreeMap<AuthorityCount, DKGCommitment<P>>,
) -> bool {
    let high_degree_sum: P = commitments
        .values()
        .map(|c| c.commitments.0.last().copied().unwrap())
        .sum();

    high_degree_sum.is_point_at_infinity()
}

pub fn compute_secret_key_share<P: ECPoint>(secret_shares: IncomingShares<P>) -> P::Scalar {
    // Note: the shares in secret_shares will be zeroized on drop here
    secret_shares
        .0
        .values()
        .map(|share| share.value.clone())
        .sum()
}

impl<P: ECPoint> DKGUnverifiedCommitment<P> {
    /// Get the number of commitments
    pub fn get_commitments_len(&self) -> usize {
        self.commitments.0.len()
    }
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

    use crate::logging::test_utils::new_test_logger;

    use super::*;

    #[test]
    fn basic_sharing() {
        use crate::multisig::crypto::eth::Point;

        let n = 7;
        let threshold = 5;

        use rand_legacy::SeedableRng;
        let mut rng = Rng::from_seed([0; 32]);

        let (secret, _commitments, shares) =
            generate_secret_and_shares::<Point>(&mut rng, n, threshold);

        assert_eq!(secret, reconstruct_secret(&shares));
    }

    #[test]
    fn keygen_sequential() {
        use crate::multisig::crypto::eth::{Point, Scalar};

        let n = 4;
        let t = 2;

        let context = HashContext([0; 32]);

        use rand_legacy::SeedableRng;
        let mut rng = Rng::from_seed([0; 32]);

        let (commitments, hash_commitments, outgoing_shares): (
            BTreeMap<_, _>,
            BTreeMap<_, _>,
            BTreeMap<_, _>,
        ) = itertools::multiunzip((1..=n).map(|idx| {
            let (secret, shares_commitments, shares) = generate_secret_and_shares(&mut rng, n, t);
            // Zero-knowledge proof of `secret`
            let zkp = generate_zkp_of_secret(&mut rng, secret, &context, idx);

            let dkg_commitment = DKGUnverifiedCommitment {
                commitments: shares_commitments,
                zkp,
            };

            let hash_commitment = generate_hash_commitment(&dkg_commitment);

            (
                (idx, dkg_commitment),
                (idx, HashComm1(hash_commitment)),
                (idx, shares),
            )
        }));

        let coeff_commitments = assert_ok!(validate_commitments(
            commitments,
            hash_commitments,
            &context,
            &new_test_logger()
        ));

        // Now it is okay to distribute the shares

        let _agg_pubkey: Point = coeff_commitments
            .iter()
            .map(|(_idx, c)| c.commitments.0[0])
            .sum();

        let mut secret_shares = vec![];

        for receiver_idx in 1..=n {
            let received_shares: Vec<_> = outgoing_shares
                .iter()
                .map(|(idx, shares)| {
                    let share = shares[&receiver_idx].clone();
                    assert!(verify_share(&share, &coeff_commitments[idx], receiver_idx));
                    share
                })
                .collect();

            // (Round 2, Step 3)
            let secret_share: Scalar = received_shares
                .iter()
                .map(|share| share.value.clone())
                .sum();

            secret_shares.push(secret_share);
        }
    }
}

pub mod genesis {

    use std::collections::HashMap;

    use super::*;
    use crate::multisig::client::PartyIdxMapping;
    use crate::multisig::KeyId;
    use state_chain_runtime::AccountId;

    /// Attempts to generate key data until we generate a key that is contract
    /// compatible. It will try `max_attempts` before failing.
    pub fn generate_key_data_until_compatible<P: ECPoint>(
        account_ids: BTreeSet<AccountId>,
        max_attempts: usize,
        mut rng: Rng,
    ) -> (KeyId, HashMap<AccountId, KeygenResultInfo<P>>) {
        let mut attempt_counter = 0;

        loop {
            attempt_counter += 1;
            match generate_key_data::<P>(account_ids.clone(), &mut rng, false) {
                Ok(result) => break result,
                Err(_) => {
                    // limit iteration so we don't loop forever
                    if attempt_counter >= max_attempts {
                        panic!("too many keygen attempts");
                    }
                }
            }
        }
    }

    /// Generates key data using the DEFAULT_KEYGEN_SEED and returns the KeygenResultInfo for signers[0].
    #[cfg(test)]
    pub fn get_key_data_for_test<P: ECPoint>(signers: BTreeSet<AccountId>) -> KeygenResultInfo<P> {
        use crate::multisig::client::tests::DEFAULT_KEYGEN_SEED;
        use rand_legacy::SeedableRng;

        generate_key_data(
            signers.clone(),
            &mut Rng::from_seed(DEFAULT_KEYGEN_SEED),
            true,
        )
        .expect("Should not be able to fail generating key data")
        .1
        .get(signers.iter().next().unwrap())
        .expect("should get keygen for an account")
        .to_owned()
    }

    pub fn generate_key_data<P: ECPoint>(
        signers: BTreeSet<AccountId>,
        rng: &mut Rng,
        allow_high_pubkey: bool,
    ) -> anyhow::Result<(KeyId, HashMap<AccountId, KeygenResultInfo<P>>)> {
        let params = ThresholdParameters::from_share_count(signers.len() as AuthorityCount);
        let n = params.share_count;
        let t = params.threshold;

        let (commitments, outgoing_secret_shares): (BTreeMap<_, _>, BTreeMap<_, _>) = (1..=n)
            .map(|idx| {
                let (_secret, commitments, shares) = generate_secret_and_shares::<P>(rng, n, t);
                ((idx, DKGCommitment { commitments }), (idx, shares))
            })
            .unzip();

        let agg_pubkey = derive_aggregate_pubkey(&commitments, allow_high_pubkey)?;

        let validator_map = PartyIdxMapping::from_participants(signers);

        let keygen_result_infos: HashMap<_, _> = (1..=n)
            .map(|idx| {
                // Collect shares destined for `idx`
                let incoming_shares: BTreeMap<_, _> = outgoing_secret_shares
                    .iter()
                    .map(|(sender_idx, shares)| (*sender_idx, shares[&idx].clone()))
                    .collect();

                (
                    validator_map.get_id(idx).unwrap().clone(),
                    KeygenResultInfo {
                        key: Arc::new(KeygenResult {
                            key_share: KeyShare {
                                y: agg_pubkey.0,
                                x_i: compute_secret_key_share(IncomingShares(incoming_shares)),
                            },
                            party_public_keys: derive_local_pubkeys_for_parties(
                                params,
                                &commitments,
                            ),
                        }),
                        validator_map: Arc::new(validator_map.clone()),
                        params,
                    },
                )
            })
            .collect();

        Ok((KeyId(agg_pubkey.0.as_bytes().to_vec()), keygen_result_infos))
    }
}
