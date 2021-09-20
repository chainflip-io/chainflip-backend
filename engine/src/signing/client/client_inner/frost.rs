use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt::Display,
};

use pallet_cf_vaults::CeremonyId;
use serde::{Deserialize, Serialize};

use crate::signing::crypto::{build_challenge, KeyShare};

use super::{client_inner::MultisigMessage, SchnorrSignature};

use crate::signing::crypto::{
    BigInt, BigIntConverter, ECPoint, ECScalar, FE as Scalar, GE as Point,
};

use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub struct SigningCommitment {
    pub index: usize,
    pub d: Point,
    pub e: Point,
}

pub type Comm1 = SigningCommitment;

// TODO: Not sure if it is a good idea to to make
// the secret values clonable
#[derive(Clone)]
pub struct SecretNoncePair {
    pub d: Scalar,
    pub d_pub: Point,
    pub e: Scalar,
    pub e_pub: Point,
}

impl SecretNoncePair {
    pub fn sample_random() -> Self {
        let d = Scalar::new_random();
        let e = Scalar::new_random();

        let d_pub = &ECPoint::generator() * &d;
        let e_pub = &ECPoint::generator() * &e;

        SecretNoncePair { d, d_pub, e, e_pub }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BroadcastVerificationMessage<T: Clone> {
    // Data is expected to be ordered by signer_idx
    pub data: Vec<T>,
}

pub type VerifyComm2 = BroadcastVerificationMessage<Comm1>;
pub type VerifyLocalSig4 = BroadcastVerificationMessage<LocalSig3>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalSig3 {
    pub response: Scalar,
}

macro_rules! derive_display {
    ($name: ty) => {
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, stringify!($name))
            }
        }
    };
}

macro_rules! derive_from_enum {
    ($variant: ty, $variant_path: path, $enum: ty) => {
        impl From<$variant> for $enum {
            fn from(x: $variant) -> Self {
                $variant_path(x)
            }
        }
    };
}

macro_rules! derive_try_from_variant {
    ($variant: ty, $variant_path: path, $enum: ty) => {
        impl TryFrom<$enum> for $variant {
            type Error = &'static str;

            fn try_from(data: $enum) -> Result<Self, Self::Error> {
                if let $variant_path(x) = data {
                    Ok(x)
                } else {
                    Err(stringify!($enum))
                }
            }
        }
    };
}

macro_rules! derive_impls_for_enum_variants {
    ($variant: ty, $variant_path: path, $enum: ty) => {
        derive_from_enum!($variant, $variant_path, $enum);
        derive_try_from_variant!($variant, $variant_path, $enum);
        derive_display!($variant);
    };
}

macro_rules! derive_impls_for_signing_data {
    ($variant: ty, $variant_path: path) => {
        derive_impls_for_enum_variants!($variant, $variant_path, SigningData);
    };
}

/// Data exchanged between parties during various stages
/// of the FROST signing protocol
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SigningData {
    CommStage1(Comm1),
    BroadcastVerificationStage2(VerifyComm2),
    LocalSigStage3(LocalSig3),
    VerifyLocalSigsStage4(VerifyLocalSig4),
}

derive_impls_for_signing_data!(Comm1, SigningData::CommStage1);
derive_impls_for_signing_data!(VerifyComm2, SigningData::BroadcastVerificationStage2);
derive_impls_for_signing_data!(LocalSig3, SigningData::LocalSigStage3);
derive_impls_for_signing_data!(VerifyLocalSig4, SigningData::VerifyLocalSigsStage4);

impl Display for SigningData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = match self {
            SigningData::CommStage1(x) => x.to_string(),
            SigningData::BroadcastVerificationStage2(x) => x.to_string(),
            SigningData::LocalSigStage3(x) => x.to_string(),
            SigningData::VerifyLocalSigsStage4(x) => x.to_string(),
        };
        write!(f, "SigningData({})", inner)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SigningDataWrapped {
    pub data: SigningData,
    pub ceremony_id: CeremonyId,
}

impl SigningDataWrapped {
    pub fn new<S>(data: S, ceremony_id: CeremonyId) -> Self
    where
        S: Into<SigningData>,
    {
        SigningDataWrapped {
            data: data.into(),
            ceremony_id,
        }
    }
}

impl From<SigningDataWrapped> for MultisigMessage {
    fn from(wrapped: SigningDataWrapped) -> Self {
        MultisigMessage::SigningMessage(wrapped)
    }
}

fn gen_group_commitment(
    signing_commitments: &[SigningCommitment],
    bindings: &HashMap<usize, Scalar>,
) -> Point {
    signing_commitments
        .iter()
        .map(|comm| {
            let rho_i = bindings[&comm.index];
            comm.d + comm.e * rho_i
        })
        .reduce(|a, b| a + b)
        .expect("non empty list")
}

// TODO: link to the reference
fn get_lagrange_coeff(
    signer_index: usize,
    all_signer_indices: &[usize],
) -> Result<Scalar, &'static str> {
    let mut num: Scalar = ECScalar::from(&BigInt::from(1));
    let mut den: Scalar = ECScalar::from(&BigInt::from(1));

    for j in all_signer_indices {
        if *j == signer_index {
            continue;
        }
        let j: Scalar = ECScalar::from(&BigInt::from(*j as u32));
        let signer_index: Scalar = ECScalar::from(&BigInt::from(signer_index as u32));
        num = num * j;
        den = den * (j.sub(&signer_index.get_element()));
    }

    if den == Scalar::zero() {
        return Err("Duplicate shares provided");
    }

    let lagrange_coeff = num * den.invert();

    Ok(lagrange_coeff)
}

// TODO: link to the reference
fn gen_rho_i(index: usize, msg: &[u8], signing_commitments: &[SigningCommitment]) -> Scalar {
    let mut hasher = Sha256::new();
    hasher.update(b"I");
    hasher.update(index.to_be_bytes());
    hasher.update(msg);

    for com in signing_commitments {
        hasher.update(com.index.to_be_bytes());
        hasher.update(com.d.get_element().serialize());
        hasher.update(com.e.get_element().serialize());
    }

    let result = hasher.finalize();

    let x: [u8; 32] = result.as_slice().try_into().expect("Invalid hash size");

    let x_bi = BigInt::from_bytes(&x);

    ECScalar::from(&x_bi)
}

type SigningResponse = LocalSig3;

fn generate_bindings(msg: &[u8], commitments: &[SigningCommitment]) -> HashMap<usize, Scalar> {
    let mut bindings: HashMap<usize, Scalar> = HashMap::with_capacity(commitments.len());

    for comm in commitments {
        let rho_i = gen_rho_i(comm.index, msg, commitments);
        bindings.insert(comm.index, rho_i);
    }

    bindings
}

// TODO: link to the reference
pub fn generate_local_sig(
    msg: &[u8],
    key: &KeyShare,
    nonces: &SecretNoncePair,
    commitments: &[SigningCommitment],
    own_idx: usize,
    all_idxs: &[usize],
) -> SigningResponse {
    let bindings = generate_bindings(&msg, commitments);

    // This is `R` in a Schnorr signature
    let group_commitment = gen_group_commitment(&commitments, &bindings);

    let challenge = build_challenge(group_commitment.get_element(), key.y.get_element(), msg);

    let SecretNoncePair { d, e, .. } = nonces;

    let lambda_i = get_lagrange_coeff(own_idx, all_idxs).expect("lagrange coeff");

    let key = key.x_i;

    let rho_i = bindings[&own_idx];

    let lhs = *d + (*e * rho_i);

    let response = lhs.sub(&(lambda_i * key * challenge).get_element());

    SigningResponse { response }
}

fn is_party_resonse_valid(
    y_i: &Point,
    lambda_i: &Scalar,
    commitment: &Point,
    challenge: &Scalar,
    response: &SigningResponse,
) -> bool {
    // MAXIM: the reponse naming is a bit unfortunate here
    (Point::generator() * response.response)
        == (commitment.sub_point(&(y_i * challenge * lambda_i).get_element()))
}

// TODO: check that this is the signature format the we want
pub struct Signature {
    r: Point,
    z: Scalar,
}

impl From<Signature> for SchnorrSignature {
    fn from(sig: Signature) -> Self {
        let s: [u8; 32] = sig.z.get_element().as_ref().clone();
        let r = sig.r.get_element();
        SchnorrSignature { s, r }
    }
}

/// Combine local signatures received from all parties into the final
/// (aggregate) signature given that no party misbehavied. Otherwise
/// return the misbehaving parties.
pub fn aggregate_signature(
    msg: &[u8],
    signer_idxs: &[usize],
    agg_pubkey: Point,
    pubkeys: &[Point],
    commitments: &[SigningCommitment],
    responses: &[SigningResponse],
) -> Result<Signature, Vec<usize>> {
    let bindings = generate_bindings(&msg, commitments);

    let group_commitment = gen_group_commitment(commitments, &bindings);

    let challenge = build_challenge(
        group_commitment.get_element(),
        agg_pubkey.get_element(),
        msg,
    );

    let mut invalid_idxs = vec![];

    for signer_idx in signer_idxs {
        let array_index = signer_idx - 1;

        let rho_i = bindings[&signer_idx];
        let lambda_i = get_lagrange_coeff(*signer_idx, signer_idxs).unwrap();

        let commitment = &commitments[array_index];
        let commitment_i = commitment.d + (commitment.e * rho_i);

        let y_i = pubkeys[array_index];

        let response = &responses[array_index];

        if !is_party_resonse_valid(&y_i, &lambda_i, &commitment_i, &challenge, &response) {
            invalid_idxs.push(*signer_idx);
            println!("A local signature is NOT valid!!!");
        }
    }

    if invalid_idxs.is_empty() {
        let z = responses
            .iter()
            .fold(Scalar::zero(), |acc, x| acc + x.response);

        Ok(Signature {
            z,
            r: group_commitment,
        })
    } else {
        Err(invalid_idxs)
    }
}
