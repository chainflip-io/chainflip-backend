/*
    Multisig Schnorr

    Copyright 2018 by Kzen Networks

    This file is part of Multisig Schnorr library
    (https://github.com/KZen-networks/multisig-schnorr)

    Multisig Schnorr is free software: you can redistribute
    it and/or modify it under the terms of the GNU General Public
    License as published by the Free Software Foundation, either
    version 3 of the License, or (at your option) any later version.

    @license GPL-3.0+ <https://github.com/KZen-networks/multisig-schnorr/blob/master/LICENSE>
*/
/// following the variant used in bip-schnorr: https://github.com/sipa/bips/blob/bip-schnorr/bip-schnorr.mediawiki
use super::error::{InvalidKey, InvalidSS};

use curv::arithmetic::traits::*;

use curv::elliptic::curves::traits::*;

use curv::cryptographic_primitives::commitments::hash_commitment::HashCommitment;
use curv::cryptographic_primitives::commitments::traits::Commitment;
use curv::cryptographic_primitives::secret_sharing::feldman_vss::VerifiableSS;
use curv::elliptic::curves::secp256_k1::Secp256k1Point;
use curv::BigInt;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

type GE = curv::elliptic::curves::secp256_k1::GE;
type FE = curv::elliptic::curves::secp256_k1::FE;

const SECURITY: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keys {
    pub u_i: FE,
    pub y_i: GE,
    pub party_index: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyGenBroadcastMessage1 {
    com: BigInt,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Parameters {
    pub threshold: usize,   //t
    pub share_count: usize, //n
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyShare {
    pub y: GE,
    pub x_i: FE,
}

impl Keys {
    pub fn phase1_create(index: usize) -> Keys {
        let u: FE = ECScalar::new_random();
        let unscaled_point: Secp256k1Point = ECPoint::generator();
        let y = unscaled_point * u;

        Keys {
            u_i: u,
            y_i: y,
            party_index: index,
        }
    }

    pub fn phase1_broadcast(&self) -> (KeyGenBroadcastMessage1, BigInt) {
        let blind_factor = BigInt::sample(SECURITY);
        let com = HashCommitment::create_commitment_with_user_defined_randomness(
            &self.y_i.bytes_compressed_to_big_int(),
            &blind_factor,
        );
        let bcm1 = KeyGenBroadcastMessage1 { com };
        (bcm1, blind_factor)
    }

    pub fn phase1_verify_com_phase2_distribute(
        &self,
        params: &Parameters,
        blind_vec: &[BigInt],
        y_vec: &[GE],
        bc1_vec: &[KeyGenBroadcastMessage1],
        parties: &[usize],
    ) -> Result<(VerifiableSS<GE>, Vec<FE>, usize), InvalidKey> {
        // test length:
        assert_eq!(blind_vec.len(), params.share_count);
        assert_eq!(bc1_vec.len(), params.share_count);
        assert_eq!(y_vec.len(), params.share_count);
        // test decommitments
        let invalid_decom_indexes = (0..bc1_vec.len())
            .into_iter()
            .filter_map(|i| {
                let valid = HashCommitment::create_commitment_with_user_defined_randomness(
                    &y_vec[i].bytes_compressed_to_big_int(),
                    &blind_vec[i],
                ) == bc1_vec[i].com;
                if valid {
                    None
                } else {
                    // signer indexes are their array indexes + 1
                    Some(i + 1)
                }
            })
            .collect_vec();

        let (vss_scheme, secret_shares) = VerifiableSS::share_at_indices(
            params.threshold,
            params.share_count,
            &self.u_i,
            &parties,
        );

        match invalid_decom_indexes.len() {
            0 => Ok((vss_scheme, secret_shares, self.party_index)),
            _ => Err(InvalidKey(invalid_decom_indexes)),
        }
    }

    pub fn phase2_verify_vss_construct_keypair(
        &self,
        params: &Parameters,
        y_vec: &[GE],
        secret_shares_vec: &[FE],
        vss_scheme_vec: &[VerifiableSS<GE>],
        index: &usize,
    ) -> Result<(KeyShare, Vec<GE>), InvalidSS> {
        assert_eq!(y_vec.len(), params.share_count);
        assert_eq!(secret_shares_vec.len(), params.share_count);
        assert_eq!(vss_scheme_vec.len(), params.share_count);

        let invalid_idxs = (0..y_vec.len())
            .into_iter()
            .filter_map(|i| {
                let valid = vss_scheme_vec[i]
                    .validate_share(&secret_shares_vec[i], *index)
                    .is_ok()
                    && vss_scheme_vec[i].commitments[0] == y_vec[i];
                if valid {
                    None
                } else {
                    Some(i + 1)
                }
            })
            .collect_vec();

        // TODO: Why is it safe to unwrap in these 3 places below. Use expect and explain why
        if invalid_idxs.is_empty() {
            let mut y_vec_iter = y_vec.iter();
            let y0 = y_vec_iter
                .next()
                .expect("Not called until we have a threshold greater than 0");
            let y = y_vec_iter.fold(*y0, |acc, x| acc + x);
            let x_i = secret_shares_vec.iter().fold(FE::zero(), |acc, x| acc + x);

            let n = params.share_count;
            let t = params.threshold;

            let pubkeys: Vec<_> = (1..=n)
                .map(|idx| {
                    let idx_scalar: FE = ECScalar::from(&BigInt::from(idx as u32));

                    (1..=n)
                        .map(|j| {
                            (0..=t)
                                .map(|k| vss_scheme_vec[j - 1].commitments[k])
                                .rev()
                                .reduce(|acc, x| acc * idx_scalar + x)
                                .expect("Length is equal to the threshold size we have")
                        })
                        .reduce(|acc, x| acc + x)
                        .expect("Length is equal to total shares we have")
                })
                .collect();

            // Sanity check: our pubkey is among generated pubkeys for all parties
            assert_eq!(pubkeys[index - 1], GE::generator() * x_i);

            Ok((KeyShare { y, x_i }, pubkeys))
        } else {
            Err(InvalidSS(invalid_idxs))
        }
    }
}
