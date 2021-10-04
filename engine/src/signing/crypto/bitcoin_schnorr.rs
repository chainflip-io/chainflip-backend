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
use super::error::{InvalidKey, InvalidSS, InvalidSig};

use super::super::super::eth::utils;
// TODO: add tests for this module?

use curv::arithmetic::traits::*;

use curv::elliptic::curves::traits::*;

use curv::cryptographic_primitives::commitments::hash_commitment::HashCommitment;
use curv::cryptographic_primitives::commitments::traits::Commitment;
use curv::cryptographic_primitives::secret_sharing::feldman_vss::VerifiableSS;
use curv::BigInt;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

use sp_core::Hasher;
use sp_runtime::traits::Keccak256;

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
pub struct SharedKeys {
    pub y: GE,
    pub x_i: FE,
}

impl Keys {
    pub fn phase1_create(index: usize) -> Keys {
        let u: FE = ECScalar::new_random();
        let y = &ECPoint::generator() * &u;

        Keys {
            u_i: u,
            y_i: y,
            party_index: index.clone(),
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
        blind_vec: &Vec<BigInt>,
        y_vec: &Vec<GE>,
        bc1_vec: &Vec<KeyGenBroadcastMessage1>,
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
            0 => Ok((vss_scheme, secret_shares, self.party_index.clone())),
            _ => Err(InvalidKey(invalid_decom_indexes)),
        }
    }

    pub fn phase2_verify_vss_construct_keypair(
        &self,
        params: &Parameters,
        y_vec: &Vec<GE>,
        secret_shares_vec: &Vec<FE>,
        vss_scheme_vec: &Vec<VerifiableSS<GE>>,
        index: &usize,
    ) -> Result<SharedKeys, InvalidSS> {
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

        match invalid_idxs.len() {
            0 => {
                let mut y_vec_iter = y_vec.iter();
                let y0 = y_vec_iter.next().unwrap();
                let y = y_vec_iter.fold(y0.clone(), |acc, x| acc + x);
                let x_i = secret_shares_vec.iter().fold(FE::zero(), |acc, x| acc + x);
                Ok(SharedKeys { y, x_i })
            }
            _ => Err(InvalidSS(invalid_idxs)),
        }
    }

    // remove secret shares from x_i for parties that are not participating in signing
    pub fn update_shared_key(
        shared_key: &SharedKeys,
        parties_in: &[usize],
        secret_shares_vec: &Vec<FE>,
    ) -> SharedKeys {
        let mut new_xi: FE = FE::zero();
        for i in 0..secret_shares_vec.len() {
            if parties_in.iter().find(|&&x| x == i).is_some() {
                new_xi = new_xi + &secret_shares_vec[i]
            }
        }
        SharedKeys {
            y: shared_key.y.clone(),
            x_i: new_xi,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalSig {
    gamma_i: FE,
    e: FE,
}

impl LocalSig {
    pub fn compute(
        message: &[u8],
        local_ephemeral_key: &SharedKeys,
        local_private_key: &SharedKeys,
    ) -> LocalSig {
        let beta_i = local_ephemeral_key.x_i.clone();
        let alpha_i = local_private_key.x_i.clone();

        let e = LocalSig::build_challenge(
            local_private_key.y.get_element(),
            local_ephemeral_key.y.get_element(),
            message,
        );

        let rhs = e.clone() * alpha_i;
        let gamma_i = beta_i.sub(&rhs.get_element());

        LocalSig { gamma_i, e }
    }

    /// Assembles and hashes the challenge in the correct order for the KeyManager Contract
    pub fn build_challenge(
        nonce_key: secp256k1::PublicKey,
        pub_key: secp256k1::PublicKey,
        message: &[u8],
    ) -> FE {
        let eth_addr = utils::pubkey_to_eth_addr(pub_key);

        let (pubkey_x, pubkey_y_parity) = LocalSig::destructure_pubkey(nonce_key);

        // Assemble the challenge in correct order according to this contract:
        // https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/abstract/SchnorrSECP256K1.sol
        let e_bytes = [
            pubkey_x.to_vec(),
            [pubkey_y_parity].to_vec(),
            message.to_vec(),
            eth_addr.to_vec(),
        ]
        .concat();

        let e_bn = BigInt::from_bytes(Keccak256::hash(&e_bytes).as_bytes());
        let e: FE = ECScalar::from(&e_bn);
        e
    }

    fn destructure_pubkey(pubkey: secp256k1::PublicKey) -> ([u8; 32], u8) {
        let pubkey_bytes: [u8; 33] = pubkey.serialize();
        let pubkey_y_parity_byte = pubkey_bytes[0];
        let pubkey_y_parity = if pubkey_y_parity_byte == 2 { 0u8 } else { 1u8 };
        let pubkey_x: [u8; 32] = pubkey_bytes[1..].try_into().expect("Is valid pubkey");
        return (pubkey_x, pubkey_y_parity);
    }

    #[cfg(test)]
    pub fn get_gamma(&self) -> FE {
        self.gamma_i
    }

    // section 4.2 step 3
    #[allow(unused_doc_comments)]
    pub fn verify_local_sigs(
        gamma_vec: &Vec<LocalSig>,
        parties_index_vec: &[usize],
        vss_private_keys: &Vec<VerifiableSS<GE>>,
        vss_ephemeral_keys: &Vec<VerifiableSS<GE>>,
    ) -> Result<VerifiableSS<GE>, InvalidSS> {
        //parties_index_vec is a vector with indices of the parties that are participating and provided gamma_i for this step
        // test that enough parties are in this round
        assert!(parties_index_vec.len() > vss_private_keys[0].parameters.threshold);

        // Vec of joint commitments:
        // n' = num of signers, n - num of parties in keygen
        // [com0_eph_0,... ,com0_eph_n', e*com0_kg_0, ..., e*com0_kg_n ;
        // ...  ;
        // comt_eph_0,... ,comt_eph_n', e*comt_kg_0, ..., e*comt_kg_n ]
        let comm_vec = (0..vss_private_keys[0].parameters.threshold + 1)
            .map(|i| {
                let eph_comm_i_vec = (0..vss_ephemeral_keys.len())
                    .map(|j| vss_ephemeral_keys[j].commitments[i].clone())
                    .collect::<Vec<GE>>();

                let eph_comm_i_sum = eph_comm_i_vec
                    .iter()
                    .copied()
                    .reduce(|acc, x| acc + x)
                    .expect("Iter should not be empty");

                let key_gen_comm_i_vec = (0..vss_private_keys.len())
                    .map(|j| vss_private_keys[j].commitments[i].clone() * &gamma_vec[i].e)
                    .collect::<Vec<GE>>();

                let key_gen_comm_i_sum = key_gen_comm_i_vec
                    .iter()
                    .copied()
                    .reduce(|acc, x| acc + x)
                    .expect("Iter should not be empty");

                eph_comm_i_sum.sub_point(&key_gen_comm_i_sum.get_element())
            })
            .collect::<Vec<GE>>();

        let vss_sum = VerifiableSS {
            parameters: vss_ephemeral_keys[0].parameters.clone(),
            commitments: comm_vec,
        };

        let g: GE = GE::generator();
        let correct_ss_verify = (0..parties_index_vec.len())
            .map(|i| {
                let gamma_i_g = &g * &gamma_vec[i].gamma_i;
                vss_sum
                    .validate_share_public(&gamma_i_g, parties_index_vec[i] + 1)
                    .is_ok()
            })
            .collect::<Vec<bool>>();

        let invalid_idxs = (0..parties_index_vec.len())
            .into_iter()
            .filter_map(|i| {
                if correct_ss_verify[i] {
                    None
                } else {
                    Some(i + 1)
                }
            })
            .collect_vec();

        match correct_ss_verify.iter().all(|x| x.clone() == true) {
            true => Ok(vss_sum),
            false => Err(InvalidSS(invalid_idxs)),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// This is `s` in other literature
    pub sigma: FE,
    /// This is `r` in other literature
    pub v: GE,
}

impl Signature {
    pub fn generate(
        vss_sum_local_sigs: &VerifiableSS<GE>,
        local_sig_vec: &Vec<LocalSig>,
        parties_index_vec: &[usize],
        v: GE,
    ) -> Signature {
        let gamma_vec = (0..parties_index_vec.len())
            .map(|i| local_sig_vec[i].gamma_i.clone())
            .collect::<Vec<FE>>();
        let reconstruct_limit = vss_sum_local_sigs.parameters.threshold.clone() + 1;
        let sigma = vss_sum_local_sigs.reconstruct(
            &parties_index_vec[0..reconstruct_limit.clone()],
            &gamma_vec[0..reconstruct_limit.clone()],
        );
        Signature { sigma, v }
    }

    pub fn verify(&self, message: &[u8], pubkey_y: &GE) -> Result<(), InvalidSig> {
        let e = LocalSig::build_challenge(pubkey_y.get_element(), self.v.get_element(), message);

        let g: GE = GE::generator();
        let sigma_g = g * &self.sigma;
        let e_y = pubkey_y * &e;
        let v_minus_e_y = self.v.sub_point(&e_y.get_element());

        if v_minus_e_y == sigma_g {
            Ok(())
        } else {
            Err(InvalidSig)
        }
    }
}

#[cfg(test)]
mod test_schnorr {
    use super::LocalSig;
    use super::SharedKeys;
    use super::Signature;
    use anyhow::Result;
    use curv::elliptic::curves::secp256_k1::{Secp256k1Point, Secp256k1Scalar};
    use curv::elliptic::curves::traits::{ECPoint, ECScalar};
    use std::str::FromStr;

    // This test data has been signed and validated on the KeyManager.sol contract
    const SECRET_KEY_HEX: &str = "fbcb47bc85b881e0dfb31c872d4e06848f80530ccbd18fc016a27c4a744d0eba";
    const NONCE_KEY_HEX: &str = "d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285";
    const MESSAGE_HASH_HEX: &str =
        "2bdc19071c7994f088103dbf8d5476d6deb6d55ee005a2f510dc7640055cc84e";
    const SIGMA_HEX: &str = "beb37e87509e15cd88b19fa224441c56acc0e143cb25b9fd1e57fdafed215538";

    #[test]
    fn test_signature() {
        // using the known SECRET_KEY_HEX, build the local_private_key
        let sk_1_scalar = scalar_from_secretkey_hex(SECRET_KEY_HEX).unwrap();
        let local_private_key = SharedKeys {
            y: Secp256k1Point::generator() * &sk_1_scalar,
            x_i: sk_1_scalar,
        };

        // create the local_ephemeral_key from the known NONCE_KEY_HEX
        let k_scalar = scalar_from_secretkey_hex(NONCE_KEY_HEX).unwrap();
        let local_ephemeral_key = SharedKeys {
            y: Secp256k1Point::generator() * &k_scalar,
            x_i: k_scalar,
        };

        // sign the message
        let message_hash = hex::decode(MESSAGE_HASH_HEX).unwrap();
        let local_sig = LocalSig::compute(&message_hash, &local_ephemeral_key, &local_private_key);
        let sigma: [u8; 32] = local_sig.get_gamma().get_element().as_ref().clone();

        // by using the same key, nonce and message, we should get the same signature (sigma)
        assert_eq!(hex::encode(&sigma), SIGMA_HEX);

        // turn the sigma into a proper signature and run it though the verify function.
        let sigma_key = secp256k1::SecretKey::from_slice(&sigma).unwrap();
        let sig = Signature {
            sigma: scalar_from_secretkey(sigma_key),
            v: local_ephemeral_key.y,
        };

        let res = sig.verify(&message_hash, &local_private_key.y);
        assert!(res.is_ok());
    }

    fn scalar_from_secretkey(secret_key: secp256k1::SecretKey) -> Secp256k1Scalar {
        let mut scalar: Secp256k1Scalar = Secp256k1Scalar::new_random();
        scalar.set_element(secret_key);
        scalar
    }

    fn scalar_from_secretkey_hex(secret_key_hex: &str) -> Result<Secp256k1Scalar> {
        let sk = secp256k1::SecretKey::from_str(secret_key_hex)?;
        let scalar = scalar_from_secretkey(sk);
        Ok(scalar)
    }
}
