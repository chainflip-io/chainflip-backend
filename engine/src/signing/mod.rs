mod bitcoin_schnorr;
mod client;
mod error;
mod utils;

#[cfg(test)]
mod distributed_signing;

pub use client::MultisigClient;

use bitcoin_schnorr::{Keys, Parameters, SharedKeys};

use curv::elliptic::curves::secp256_k1::GE;
use curv::{
    cryptographic_primitives::secret_sharing::feldman_vss::VerifiableSS,
    elliptic::curves::secp256_k1::FE,
};

pub type MessageHash = Vec<u8>;

use crate::signing::bitcoin_schnorr::{LocalSig, Signature};

// TODO: move to examples directory

#[allow(dead_code)]
pub fn keygen_t_n_parties(
    t: usize,
    n: usize,
    parties: &[usize],
) -> (Vec<Keys>, Vec<SharedKeys>, GE, Vec<VerifiableSS<GE>>) {
    let params = Parameters {
        threshold: t,
        share_count: n,
    };
    // >>> Each party generates a key (privately)

    dbg!(parties);

    let party_keys_vec: Vec<Keys> = (0..n).map(|i| Keys::phase1_create(parties[i])).collect();

    let mut bc1_vec = Vec::new();
    let mut blind_vec = Vec::new();
    for i in 0..n.clone() {
        let (bc1, blind) = party_keys_vec[i].phase1_broadcast();
        bc1_vec.push(bc1);
        blind_vec.push(blind);
    }

    // Values bc1_i and y_i are broadcast
    // bc1_i are blinded commitments to y_i???

    let y_vec = (0..n.clone())
        .map(|i| party_keys_vec[i].y_i.clone())
        .collect::<Vec<GE>>();

    let mut y_vec_iter = y_vec.iter();
    let head = y_vec_iter.next().unwrap();
    let tail = y_vec_iter;
    let y_sum = tail.fold(head.clone(), |acc, x| acc + x);

    dbg!(&y_sum); // Public key?

    let mut vss_scheme_vec = Vec::new();
    let mut secret_shares_vec = Vec::new();
    let mut index_vec = Vec::new();
    for i in 0..n.clone() {
        println!("Player: [{}]", i);
        let (vss_scheme, secret_shares, index) = party_keys_vec[i]
            .phase1_verify_com_phase2_distribute(&params, &blind_vec, &y_vec, &bc1_vec, parties)
            .expect("invalid key");
        vss_scheme_vec.push(vss_scheme);
        secret_shares_vec.push(secret_shares);
        index_vec.push(index);
    }

    // For each player i, collect shares distributed by players j
    let party_shares = (0..n.clone())
        .map(|i| {
            (0..n.clone())
                .map(|j| {
                    let vec_j = &secret_shares_vec[j];
                    vec_j[i].clone()
                })
                .collect::<Vec<FE>>()
        })
        .collect::<Vec<Vec<FE>>>();

    println!("Party shares");
    for (i, player_shares) in party_shares.iter().enumerate() {
        println!("Player [{}]", i);
        for (j, share) in player_shares.iter().enumerate() {
            println!("  [{}]: {:?}", j, share);
        }
    }

    let mut shared_keys_vec = Vec::new();
    for i in 0..n.clone() {
        println!("Player: [{}]", i);
        let shared_keys = party_keys_vec[i]
            .phase2_verify_vss_construct_keypair(
                &params,
                &y_vec,
                &party_shares[i],
                &vss_scheme_vec,
                &index_vec[i],
            )
            .expect("invalid vss");
        shared_keys_vec.push(shared_keys);
    }

    // y_sum is the shared public key
    (party_keys_vec, shared_keys_vec, y_sum, vss_scheme_vec)

    // let (_eph_keys_vec, eph_shared_keys_vec, V, eph_vss_vec)
}

fn sign(
    message: &[u8],
    t: usize,
    eph_shared_keys_vec: &Vec<SharedKeys>,
    priv_shared_keys_vec: &Vec<SharedKeys>,
    parties_index_vec: &[usize],
    key_gen_vss_vec: &Vec<VerifiableSS<GE>>,
    eph_vss_vec: &Vec<VerifiableSS<GE>>,
    v: GE,
    y: GE,
) {
    // each party computes and share a local sig, we collected them here to a vector as each party should do AFTER receiving all local sigs
    let local_sig_vec = (0..t.clone())
        .map(|i| {
            LocalSig::compute(
                &message,
                &eph_shared_keys_vec[i],
                &priv_shared_keys_vec[parties_index_vec[i]],
            )
        })
        .collect::<Vec<LocalSig>>();

    let verify_local_sig = LocalSig::verify_local_sigs(
        &local_sig_vec,
        &parties_index_vec,
        &key_gen_vss_vec,
        &eph_vss_vec,
    );

    assert!(verify_local_sig.is_ok());
    let vss_sum_local_sigs = verify_local_sig.unwrap();

    // each party / dealer can generate the signature
    let signature = Signature::generate(&vss_sum_local_sigs, &local_sig_vec, &parties_index_vec, v);
    let verify_sig = signature.verify(&message, &y);
    assert!(verify_sig.is_ok());
}

#[cfg(test)]
mod test {

    use curv::{
        arithmetic::Samplable,
        cryptographic_primitives::{
            commitments::{hash_commitment::HashCommitment, traits::Commitment},
            secret_sharing::feldman_vss::{ShamirSecretSharing, VerifiableSS},
        },
        elliptic::curves::traits::{ECPoint, ECScalar},
        BigInt,
    };

    use super::*;

    /// This test is mostly covering third party code,
    /// but it might be useful to keep around as an illustration
    /// of how the multisig works
    #[test]
    #[allow(non_snake_case)]
    #[ignore]
    fn test_sequential_signing() {
        let t = 1;
        let n = 3;

        let params = Parameters {
            threshold: t,
            share_count: n,
        };

        let key_gen_parties_index_vec: [usize; 3] = [0, 1, 3];
        let key_gen_parties_points_vec = (0..key_gen_parties_index_vec.len())
            .map(|i| key_gen_parties_index_vec[i].clone() + 1)
            .collect::<Vec<usize>>();

        let parties = &key_gen_parties_points_vec;

        let (_priv_keys_vec, priv_shared_keys_vec, Y, key_gen_vss_vec) =
            keygen_t_n_parties(t, n, parties);

        // signing

        // Generate a random shared secret: (e, V, c_iG)
        let parties_index_vec: [usize; 2] = [0, 1];
        let parties_points_vec = (0..parties_index_vec.len())
            .map(|i| parties_index_vec[i].clone() + 1)
            .collect::<Vec<usize>>();
        let num_parties = parties_index_vec.len();
        let (_eph_keys_vec, eph_shared_keys_vec, V, eph_vss_vec) =
            keygen_t_n_parties(t.clone(), num_parties.clone(), &parties_points_vec);

        let message: [u8; 4] = [79, 77, 69, 82];

        // Note that "private" keys are not used here (unlike the private components of the shared key)
        sign(
            &message,
            t + 1,
            &eph_shared_keys_vec,
            &priv_shared_keys_vec,
            &parties_index_vec,
            &key_gen_vss_vec,
            &eph_vss_vec,
            V,
            Y,
        );
    }

    pub fn share_at_indices_my(
        t: usize,
        n: usize,
        secret: &FE,
        index_vec: &[usize],
    ) -> (VerifiableSS<GE>, Vec<FE>) {
        assert_eq!(n, index_vec.len());
        let poly = VerifiableSS::<GE>::sample_polynomial(t, secret);

        dbg!(&secret);
        dbg!(&poly);

        let secret_shares = VerifiableSS::<GE>::evaluate_polynomial(&poly, index_vec);

        #[allow(non_snake_case)]
        let G: GE = ECPoint::generator();
        let commitments = (0..poly.len())
            .map(|i| G.clone() * poly[i].clone())
            .collect::<Vec<GE>>();
        (
            VerifiableSS {
                parameters: ShamirSecretSharing {
                    threshold: t,
                    share_count: n,
                },
                commitments,
            },
            secret_shares,
        )
    }

    /// Just trying out some crypto. This test doesn't cover any of our own code,
    /// and should probably be deleted.
    #[test]
    #[ignore]
    fn basic_crypto() {
        const SECURITY: usize = 256;

        // Both parties generate key pairs
        let u_1: FE = ECScalar::new_random();
        let y_1 = &ECPoint::generator() * &u_1;

        let u_2: FE = ECScalar::new_random();
        let y_2 = &ECPoint::generator() * &u_2;

        // both parties commit to their public keys

        let blind_1 = BigInt::sample(SECURITY);
        let blind_2 = BigInt::sample(SECURITY);

        let com_1 = HashCommitment::create_commitment_with_user_defined_randomness(
            &y_1.bytes_compressed_to_big_int(),
            &blind_1,
        );

        let com_2 = HashCommitment::create_commitment_with_user_defined_randomness(
            &y_2.bytes_compressed_to_big_int(),
            &blind_2,
        );

        // Both need to share u_i via VSS

        // Player 1 wants to share u_1 with P2, so he splits his secret (u_1) into secret shares secret_shares_1[0] and secrete_shares_1[1]
        // They also create commitments to the coefficients vss_scheme_1 (so that anyone could verify anyones secret share if necessary)

        let (vss_scheme_1, secret_shares_1): (VerifiableSS<GE>, Vec<FE>) =
            share_at_indices_my(1, 2, &u_1, &[1, 2]);

        let (vss_scheme_2, secret_shares_2): (VerifiableSS<GE>, Vec<FE>) =
            VerifiableSS::share_at_indices(1, 2, &u_2, &[1, 2]);

        dbg!(&vss_scheme_1);
        dbg!(&secret_shares_1);

        // Sanity check our own share
        let ok = vss_scheme_1.validate_share(&secret_shares_1[0], 1).is_ok();

        dbg!(ok);

        // Player 1 send to Player 2: vss_scheme_1, secret_shares_1[2]

        let ok = vss_scheme_1.validate_share(&secret_shares_1[1], 2).is_ok();

        assert!(ok);

        // Player 2 verifies that his share is correct

        assert_eq!(vss_scheme_1.commitments[0], y_1);

        // Player 2 can now compute y_2, and his share of the key

        let y = y_1 + y_2;

        let r = u_1 + u_2;

        assert_eq!(&ECPoint::generator() * &r, y);

        // *** CREATE SIGNATURE ***

        // Need to generate another secret: (e|V) as per the Schnorr scheme
    }
}
