use std::{collections::HashMap, convert::TryInto};

use pallet_cf_vaults::CeremonyId;
use serde::{Deserialize, Serialize};

use crate::signing::crypto::{
    BigInt, BigIntConverter, ECPoint, ECScalar, Point, Scalar, ScalarExt,
};

use super::client_inner::ThresholdParameters;

/// Ceremony peers are interested in evaluations of our secret polynomial
/// at their index `signer_idx`
fn evaluate_polynomial(secret: &Scalar, coefficients: &[Scalar], signer_idx: usize) -> Scalar {
    let index = Scalar::from_usize(signer_idx);

    [*secret]
        .iter()
        .cloned()
        .chain(coefficients.iter().cloned())
        .rev()
        .reduce(|acc, coefficient| acc * index + coefficient)
        .unwrap()
}

#[test]
fn test_simple_polynomial() {
    // f(x) = 4 + 5x + 2x^2
    let secret = Scalar::from_usize(4);
    let coefficients = [Scalar::from_usize(5), Scalar::from_usize(2)];

    // f(3) = 4 + 15 + 18 = 37
    let value = evaluate_polynomial(&secret, &coefficients, 3);
    assert_eq!(value, Scalar::from_usize(37));
}

/// Evaluation of a sharing polynomial for a given party index
/// as per Shamir Secret Sharing scheme
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShamirShare {
    /// index at which sharing polynomial was evaluated
    index: usize,
    /// the result of polynomial evaluation
    pub value: Scalar,
}

/// Test-only helper function used to sanity check our sharing polynomial
#[cfg(test)]
fn reconstruct_secret(shares: &HashMap<usize, ShamirShare>) -> Scalar {
    let all_idxs: Vec<usize> = shares.keys().into_iter().cloned().collect();

    shares.iter().fold(
        Scalar::zero(),
        |acc, (index, ShamirShare { index: _, value })| {
            acc + super::frost::get_lagrange_coeff(*index, &all_idxs).unwrap() * value
        },
    )
}

/// Generate challenge agains which a ZKP of our secret will be generated
fn generate_dkg_challenge(index: usize, context: &str, public: Point, commitment: Point) -> Scalar {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(public.get_element().to_string());
    hasher.update(commitment.get_element().to_string());

    hasher.update(index.to_be_bytes());
    hasher.update(context);

    let result = hasher.finalize();

    let x: [u8; 32] = result.as_slice().try_into().expect("Invalid hash size");

    ECScalar::from(&BigInt::from_bytes(&x))
}

/// Generate ZKP (zero-knowledge proof) of `secret`
fn generate_zkp_of_secret(secret: Scalar, context: &str, index: usize) -> ZKPSignature {
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

/// Generate a secret and derive shares and commitments from it.
/// (The secret will never be needed again, so it is not exposed
/// to the caller.)
pub fn generate_shares_and_commitment(
    context: &str,
    index: usize,
    params: ThresholdParameters,
) -> (HashMap<usize, ShamirShare>, DKGUnverifiedCommitment) {
    let (secret, commitments, shares) =
        generate_secret_and_shares(params.share_count, params.threshold);

    // Zero-knowledge proof of `secret`
    let zkp = generate_zkp_of_secret(secret, &context, index);

    // TODO: zeroize secret here

    (shares, DKGUnverifiedCommitment { commitments, zkp })
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
                    index,
                    value: evaluate_polynomial(&secret, &coefficients, index),
                },
            )
        })
        .collect();

    // TODO: we should probably zerozie coefficients here, and remove the secret?
    (secret, CoefficientCommitments(commitments), shares)
}

fn is_valid_zkp(challenge: Scalar, zkp: &ZKPSignature, comm: &CoefficientCommitments) -> bool {
    zkp.r + comm.0[0] * challenge == Point::generator() * zkp.z
}

// (Figure 1: Round 2, Step 2)
pub fn verify_share(share: &ShamirShare, com: &DKGCommitment) -> bool {
    let index = Scalar::from_usize(share.index);

    let res = com
        .commitments
        .0
        .iter()
        .cloned()
        .rev()
        .reduce(|acc, coefficient| acc * index + coefficient)
        .expect("can't be empty");

    Point::generator() * share.value == res
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
    commitments: Vec<DKGUnverifiedCommitment>,
    context: &str,
) -> Result<Vec<DKGCommitment>, Vec<usize>> {
    let mut invalid_idxs = vec![];

    for (idx, c) in commitments.iter().enumerate() {
        let challenge = generate_dkg_challenge(idx + 1, context, c.commitments.0[0], c.zkp.r);

        if !is_valid_zkp(challenge, &c.zkp, &c.commitments) {
            invalid_idxs.push(idx + 1);
        }
    }

    if !invalid_idxs.is_empty() {
        return Err(invalid_idxs);
    }

    Ok(commitments
        .into_iter()
        .map(|c| DKGCommitment {
            commitments: c.commitments,
        })
        .collect())
}

/// Unique context used for generating a ZKP
pub fn generate_keygen_context(ceremony_id: CeremonyId) -> String {
    // TODO: use a deterministic random string here for more security
    // (hash ceremony_id + the list of signers?)
    ceremony_id.to_string()
}

/// Derive aggregate pubkey from party commitments
pub fn derive_aggregate_pubkey(commitments: &[DKGCommitment]) -> Point {
    commitments
        .iter()
        .map(|c| c.commitments.0[0])
        .reduce(|acc, x| acc + x)
        .unwrap()
}

/// Derive each party's "local" pubkey
pub fn derive_local_pubkeys_for_parties(
    ThresholdParameters {
        share_count: n,
        threshold: t,
    }: ThresholdParameters,
    commitments: &[DKGCommitment],
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
            let idx_scalar = Scalar::from_usize(idx);

            (1..=n)
                .map(|j| {
                    (0..=t)
                        .map(|k| commitments[j - 1].commitments.0[k])
                        .rev()
                        .reduce(|acc, x| acc * idx_scalar + x)
                        .unwrap()
                })
                .reduce(|acc, x| acc + x)
                .unwrap()
        })
        .collect()
}

#[cfg(test)]
mod tests {

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

        let ceremony_id = 2;

        let context = generate_keygen_context(ceremony_id);

        let (commitments, outgoing_shares): (Vec<_>, Vec<_>) = (1..=n)
            .map(|index| {
                let (secret, shares_commitments, shares) = generate_secret_and_shares(n, t);
                // Zero-knowledge proof of `secret`
                let zkp = generate_zkp_of_secret(secret, &context, index);

                let dkg_commitment = DKGUnverifiedCommitment {
                    commitments: shares_commitments,
                    zkp,
                };

                (dkg_commitment, shares)
            })
            .unzip();

        let res = validate_commitments(commitments, &context);

        assert!(res.is_ok());

        let coeff_commitments = res.unwrap();

        // Now it is okay to distribute the shares

        let _agg_pubkey = coeff_commitments
            .iter()
            .map(|c| c.commitments.0[0])
            .reduce(|acc, x| acc + x)
            .unwrap();

        let mut secret_shares = vec![];

        for receiver_idx in 1..=n {
            let received_shares: Vec<_> = outgoing_shares
                .iter()
                .map(|shares| shares[&receiver_idx].clone())
                .collect();

            for (idx, share) in received_shares.iter().enumerate() {
                let res = verify_share(share, &coeff_commitments[idx]);
                assert!(res);
            }

            // (Roound 2, Step 3)
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
