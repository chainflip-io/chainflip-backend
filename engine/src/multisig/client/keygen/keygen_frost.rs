use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryInto,
};

use pallet_cf_validator::AuthorityCount;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use zeroize::Zeroize;

use crate::multisig::{
    client::ThresholdParameters,
    crypto::{Point, Rng, Scalar},
};

use super::keygen_data::HashComm1;

/// Evaluate polynomial f(x) = c0 + c1 * x + c2 * x^2 + ... (expressed as
/// an iterator over its coefficients [c0, c1, c2, ...]) at x = index
fn evaluate_polynomial<'a, T>(
    coefficients: impl DoubleEndedIterator<Item = &'a T>,
    index: AuthorityCount,
) -> T
where
    T: 'a + Clone,
    T: std::ops::Mul<Scalar, Output = T>,
    T: std::ops::Add<T, Output = T>,
{
    coefficients
        .rev()
        .cloned()
        .reduce(|acc, coefficient| acc * Scalar::from_usize(index as usize) + coefficient)
        .unwrap()
}

#[test]
fn test_simple_polynomial() {
    // f(x) = 4 + 5x + 2x^2
    let secret = Scalar::from_usize(4);
    let coefficients = [Scalar::from_usize(5), Scalar::from_usize(2)];

    // f(3) = 4 + 15 + 18 = 37
    let value = evaluate_polynomial([secret].iter().chain(coefficients.iter()), 3);
    assert_eq!(value, Scalar::from_usize(37));
}

/// Evaluation of a sharing polynomial for a given party index
/// as per Shamir Secret Sharing scheme
#[derive(Debug, Clone, Deserialize, Serialize, Zeroize)]
#[zeroize(drop)]
pub struct ShamirShare {
    /// the result of polynomial evaluation
    pub value: Scalar,
}

#[cfg(test)]
impl ShamirShare {
    pub fn create_random(rng: &mut Rng) -> Self {
        ShamirShare {
            value: Scalar::random(rng),
        }
    }
}

/// Test-only helper function used to sanity check our sharing polynomial
#[cfg(test)]
fn reconstruct_secret(shares: &BTreeMap<AuthorityCount, ShamirShare>) -> Scalar {
    use crate::multisig::client::signing::frost;

    let all_idxs: BTreeSet<AuthorityCount> = shares.keys().into_iter().cloned().collect();

    shares
        .iter()
        .fold(Scalar::zero(), |acc, (index, ShamirShare { value })| {
            acc + frost::get_lagrange_coeff(*index, &all_idxs).unwrap() * value
        })
}

/// Context used in hashing to prevent replay attacks
#[derive(Clone)]
pub struct HashContext(pub [u8; 32]);

/// Generate challenge against which a ZKP of our secret will be generated
fn generate_dkg_challenge(
    index: AuthorityCount,
    context: &HashContext,
    public: Point,
    commitment: Point,
) -> Scalar {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    hasher.update(public.as_bytes());
    hasher.update(commitment.as_bytes());

    hasher.update(index.to_be_bytes());
    hasher.update(context.0);

    let result = hasher.finalize();

    let x: [u8; 32] = result.as_slice().try_into().expect("Invalid hash size");

    Scalar::from_bytes(&x)
}

/// Generate ZKP (zero-knowledge proof) of `secret`
fn generate_zkp_of_secret(
    rng: &mut Rng,
    secret: Scalar,
    context: &HashContext,
    index: AuthorityCount,
) -> ZKPSignature {
    let nonce = Scalar::random(rng);
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
pub struct OutgoingShares(pub BTreeMap<AuthorityCount, ShamirShare>);

pub struct IncomingShares(pub BTreeMap<AuthorityCount, ShamirShare>);

/// Generate a secret and derive shares and commitments from it.
/// (The secret will never be needed again, so it is not exposed
/// to the caller.)
pub fn generate_shares_and_commitment(
    rng: &mut Rng,
    context: &HashContext,
    index: AuthorityCount,
    params: ThresholdParameters,
) -> (OutgoingShares, DKGUnverifiedCommitment) {
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
fn generate_secret_and_shares(
    rng: &mut Rng,
    n: AuthorityCount,
    t: AuthorityCount,
) -> (
    Scalar,
    CoefficientCommitments,
    BTreeMap<AuthorityCount, ShamirShare>,
) {
    // Our secret contribution to the aggregate key
    let secret = Scalar::random(rng);

    // Coefficients for the sharing polynomial used to share `secret` via the Shamir Secret Sharing scheme
    // (Figure 1: Round 1, Step 1)
    let coefficients: Vec<_> = (0..t).into_iter().map(|_| Scalar::random(rng)).collect();

    // (Figure 1: Round 1, Step 3)
    let commitments: Vec<_> = [secret.clone()]
        .iter()
        .chain(&coefficients)
        .map(Point::from_scalar)
        .collect();

    // Generate shares
    // (Figure 1: Round 2, Step 1)
    let shares = (1..=n)
        .map(|index| {
            (
                index,
                ShamirShare {
                    // TODO: Make this work on references
                    value: evaluate_polynomial(
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

fn is_valid_zkp(challenge: Scalar, zkp: &ZKPSignature, comm: &CoefficientCommitments) -> bool {
    zkp.r + comm.0[0] * challenge == Point::from_scalar(&zkp.z)
}

// (Figure 1: Round 2, Step 2)
pub fn verify_share(share: &ShamirShare, com: &DKGCommitment, index: AuthorityCount) -> bool {
    Point::from_scalar(&share.value) == evaluate_polynomial(com.commitments.0.iter(), index)
}

/// Commitments to the sharing polynomial coefficient
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoefficientCommitments(Vec<Point>);

/// Zero-knowledge proof of us knowing the secret
/// (in a form of a Schnorr signature)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZKPSignature {
    r: Point,
    z: Scalar,
}

/// Commitments along with the corresponding ZKP
/// which should be sent to other parties at the
/// beginning of the ceremony
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DKGUnverifiedCommitment {
    commitments: CoefficientCommitments,
    zkp: ZKPSignature,
}

/// Commitments that have already been checked against the ZKP
#[derive(Debug)]
pub struct DKGCommitment {
    commitments: CoefficientCommitments,
}

fn is_valid_hash_commitment(
    public_coefficients: &DKGUnverifiedCommitment,
    hash_commitment: &H256,
) -> bool {
    hash_commitment == &generate_hash_commitment(public_coefficients)
}

// (Figure 1: Round 1, Step 5)
pub fn validate_commitments(
    public_coefficients: BTreeMap<AuthorityCount, DKGUnverifiedCommitment>,
    hash_commitments: BTreeMap<AuthorityCount, HashComm1>,
    context: &HashContext,
) -> Result<BTreeMap<AuthorityCount, DKGCommitment>, BTreeSet<AuthorityCount>> {
    let invalid_idxs: BTreeSet<_> = public_coefficients
        .iter()
        .filter_map(|(idx, c)| {
            let challenge = generate_dkg_challenge(*idx, context, c.commitments.0[0], c.zkp.r);

            let hash_commitment = hash_commitments
                .get(idx)
                .expect("message must be present due to ceremony runner invariants");

            let invalid_zkp = !is_valid_zkp(challenge, &c.zkp, &c.commitments);
            let invalid_hash_commitment = !is_valid_hash_commitment(c, &hash_commitment.0);

            if invalid_zkp || invalid_hash_commitment {
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
        Err(invalid_idxs)
    }
}

/// Derive aggregate pubkey from party commitments
pub fn derive_aggregate_pubkey(commitments: &BTreeMap<AuthorityCount, DKGCommitment>) -> Point {
    commitments.iter().map(|(_idx, c)| c.commitments.0[0]).sum()
}

/// Derive each party's "local" pubkey
pub fn derive_local_pubkeys_for_parties(
    ThresholdParameters {
        share_count: n,
        threshold: t,
    }: ThresholdParameters,
    commitments: &BTreeMap<AuthorityCount, DKGCommitment>,
) -> Vec<Point> {
    // Recall that each party i's secret key share `s` is the sum
    // of secret shares they receive from all other parties, which
    // are in turn calculated by evaluating each party's sharing
    // polynomial `f(x)` at `x = i`. We can derive `G * s` (unlike
    // `s` itself), because we know `G * f(x)` from coefficient
    // commitments.
    // I.e. y_i = G * f_1(i) + G * f_2(i) + ... G * f_n(i), where
    // G * f_j(i) = G * s_j + G * c_j_1(i) + G * c_j_2(i) + ... + c_j_{t-1}(i)

    (1..=n)
        .map(|idx| {
            (1..=n)
                .map(|j| {
                    evaluate_polynomial(
                        (0..=t).map(|k| &commitments[&j].commitments.0[k as usize]),
                        idx,
                    )
                })
                .sum()
        })
        .collect()
}

pub fn generate_hash_commitment(coefficient_commitments: &DKGUnverifiedCommitment) -> H256 {
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
pub fn check_high_degree_commitments(
    commitments: &BTreeMap<AuthorityCount, DKGCommitment>,
) -> bool {
    let high_degree_sum: Point = commitments
        .values()
        .map(|c| c.commitments.0.last().copied().unwrap())
        .sum();

    high_degree_sum.is_point_at_infinity()
}

#[cfg(test)]
impl DKGUnverifiedCommitment {
    /// Change the lowest degree coefficient so that it fails ZKP check
    pub fn corrupt_primary_coefficient(&mut self, rng: &mut Rng) {
        self.commitments.0[0] = Point::from_scalar(&Scalar::random(rng));
    }

    /// Change a higher degree coefficient, so that it fails hash commitment check
    pub fn corrupt_secondary_coefficient(&mut self, rng: &mut Rng) {
        self.commitments.0[1] = Point::from_scalar(&Scalar::random(rng));
    }
}

#[cfg(test)]
mod tests {

    use crate::testing::assert_ok;

    use super::*;

    #[test]
    fn basic_sharing() {
        let n = 7;
        let threshold = 5;

        use rand_legacy::SeedableRng;
        let mut rng = Rng::from_seed([0; 32]);

        let (secret, _commitments, shares) = generate_secret_and_shares(&mut rng, n, threshold);

        assert_eq!(secret, reconstruct_secret(&shares));
    }

    #[test]
    fn keygen_sequential() {
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
            &context
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
