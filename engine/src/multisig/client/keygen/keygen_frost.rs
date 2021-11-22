use std::{collections::HashMap, convert::TryInto};

use serde::{Deserialize, Serialize};

use crate::multisig::{
    client::ThresholdParameters,
    crypto::{BigInt, BigIntConverter, ECPoint, ECScalar, Point, Scalar, ScalarExt},
};

/// Evaluate polynomial f(x) = c0 + c1 * x + c2 * x^2 + ... (expressed as
/// an iterator over its coefficients [c0, c1, c2, ...]) at x = index
fn evaluate_polynomial<'a, T>(
    coefficients: impl DoubleEndedIterator<Item = &'a T>,
    index: usize,
) -> T
where
    T: 'a + Clone,
    T: std::ops::Mul<Scalar, Output = T>,
    T: std::ops::Add<Output = T>,
{
    let index = Scalar::from_usize(index);

    coefficients
        .rev()
        .cloned()
        .reduce(|acc, coefficient| acc * index + coefficient)
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

use zeroize::Zeroize;

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
    pub fn create_random() -> Self {
        ShamirShare {
            value: Scalar::new_random(),
        }
    }
}

/// Test-only helper function used to sanity check our sharing polynomial
#[cfg(test)]
fn reconstruct_secret(shares: &HashMap<usize, ShamirShare>) -> Scalar {
    use crate::multisig::client::signing::frost;
    use std::collections::BTreeSet;

    let all_idxs: BTreeSet<usize> = shares.keys().into_iter().cloned().collect();

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
    index: usize,
    context: &HashContext,
    public: Point,
    commitment: Point,
) -> Scalar {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(public.get_element().to_string());
    hasher.update(commitment.get_element().to_string());

    hasher.update(index.to_be_bytes());
    hasher.update(context.0);

    let result = hasher.finalize();

    let x: [u8; 32] = result.as_slice().try_into().expect("Invalid hash size");

    ECScalar::from(&BigInt::from_bytes(&x))
}

/// Generate ZKP (zero-knowledge proof) of `secret`
fn generate_zkp_of_secret(secret: Scalar, context: &HashContext, index: usize) -> ZKPSignature {
    let nonce = Scalar::new_random();
    let nonce_commitment = Point::generator() * nonce;

    let secret_commitment = Point::generator() * secret;

    let challenge = generate_dkg_challenge(index, context, secret_commitment, nonce_commitment);

    let z = nonce + secret * challenge;

    ZKPSignature {
        r: nonce_commitment,
        z,
    }
}

#[derive(Clone, Default)]
pub struct OutgoingShares(pub HashMap<usize, ShamirShare>);

#[derive(Clone)]
pub struct IncomingShares(pub HashMap<usize, ShamirShare>);

/// Generate a secret and derive shares and commitments from it.
/// (The secret will never be needed again, so it is not exposed
/// to the caller.)
pub fn generate_shares_and_commitment(
    context: &HashContext,
    index: usize,
    params: ThresholdParameters,
) -> (OutgoingShares, DKGUnverifiedCommitment) {
    let (secret, commitments, shares) =
        generate_secret_and_shares(params.share_count, params.threshold);

    // Zero-knowledge proof of `secret`
    let zkp = generate_zkp_of_secret(secret, context, index);

    // TODO: zeroize secret here

    (
        OutgoingShares(shares),
        DKGUnverifiedCommitment { commitments, zkp },
    )
}

// NOTE: shares should be sent after participants have exchanged commitments
fn generate_secret_and_shares(
    n: usize,
    t: usize,
) -> (Scalar, CoefficientCommitments, HashMap<usize, ShamirShare>) {
    // Our secret contribution to the aggregate key
    let secret = Scalar::new_random();

    // Coefficients for the sharing polynomial used to share `secret` via the Shamir Secret Sharing scheme
    // (Figure 1: Round 1, Step 1)
    let coefficients: Vec<_> = (0..t).into_iter().map(|_| Scalar::new_random()).collect();

    // (Figure 1: Round 1, Step 3)
    let commitments: Vec<_> = [secret]
        .iter()
        .chain(&coefficients)
        .map(|scalar| Point::generator() * scalar)
        .collect();

    // Generate shares
    // (Figure 1: Round 2, Step 1)
    let shares = (1..=n)
        .map(|index| {
            (
                index,
                ShamirShare {
                    value: evaluate_polynomial([secret].iter().chain(coefficients.iter()), index),
                },
            )
        })
        .collect();

    // TODO: we should probably zeroize coefficients here, and remove the secret?
    (secret, CoefficientCommitments(commitments), shares)
}

fn is_valid_zkp(challenge: Scalar, zkp: &ZKPSignature, comm: &CoefficientCommitments) -> bool {
    zkp.r + comm.0[0] * challenge == Point::generator() * zkp.z
}

// (Figure 1: Round 2, Step 2)
pub fn verify_share(share: &ShamirShare, com: &DKGCommitment, index: usize) -> bool {
    Point::generator() * share.value == evaluate_polynomial(com.commitments.0.iter(), index)
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
#[derive(Debug, Clone)]
pub struct DKGCommitment {
    commitments: CoefficientCommitments,
}

// (Figure 1: Round 1, Step 5)
pub fn validate_commitments(
    commitments: HashMap<usize, DKGUnverifiedCommitment>,
    context: &HashContext,
) -> Result<HashMap<usize, DKGCommitment>, Vec<usize>> {
    let invalid_idxs: Vec<_> = commitments
        .iter()
        .filter_map(|(idx, c)| {
            let challenge = generate_dkg_challenge(*idx, context, c.commitments.0[0], c.zkp.r);

            if !is_valid_zkp(challenge, &c.zkp, &c.commitments) {
                Some(*idx)
            } else {
                None
            }
        })
        .collect();

    if invalid_idxs.is_empty() {
        Ok(commitments
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
pub fn derive_aggregate_pubkey(commitments: &HashMap<usize, DKGCommitment>) -> Point {
    commitments
        .iter()
        .map(|(_idx, c)| c.commitments.0[0])
        .reduce(|acc, x| acc + x)
        .unwrap()
}

/// Derive each party's "local" pubkey
pub fn derive_local_pubkeys_for_parties(
    ThresholdParameters {
        share_count: n,
        threshold: t,
    }: ThresholdParameters,
    commitments: &HashMap<usize, DKGCommitment>,
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
                    evaluate_polynomial((0..=t).map(|k| &commitments[&j].commitments.0[k]), idx)
                })
                .reduce(|acc, x| acc + x)
                .unwrap()
        })
        .collect()
}

#[cfg(test)]
mod tests {

    use crate::testing::assert_ok;

    use super::*;

    #[test]
    fn basic_sharing() {
        let n = 7;
        let threshold = 5;

        let (secret, _commitments, shares) = generate_secret_and_shares(n, threshold);

        assert_eq!(secret, reconstruct_secret(&shares));
    }

    #[test]
    fn keygen_sequential() {
        let n = 4;
        let t = 2;

        let context = HashContext([0; 32]);

        let (commitments, outgoing_shares): (HashMap<_, _>, HashMap<_, _>) = (1..=n)
            .map(|idx| {
                let (secret, shares_commitments, shares) = generate_secret_and_shares(n, t);
                // Zero-knowledge proof of `secret`
                let zkp = generate_zkp_of_secret(secret, &context, idx);

                let dkg_commitment = DKGUnverifiedCommitment {
                    commitments: shares_commitments,
                    zkp,
                };

                ((idx, dkg_commitment), (idx, shares))
            })
            .unzip();

        let coeff_commitments = assert_ok!(validate_commitments(commitments, &context));

        // Now it is okay to distribute the shares

        let _agg_pubkey = coeff_commitments
            .iter()
            .map(|(_idx, c)| c.commitments.0[0])
            .reduce(|acc, x| acc + x)
            .unwrap();

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
            let secret_share = received_shares
                .iter()
                .map(|share| share.value)
                .reduce(|acc, share| acc + share)
                .unwrap();

            // TODO: delete all received_shares

            secret_shares.push(secret_share);
        }
    }
}
