use ring::signature::{self, EcdsaKeyPair};

use crate::common::Staker;

/// Get fake ecdsa keypiar used for signing unstake txs
fn get_fake_ecdsa_key() -> EcdsaKeyPair {
    let algo = &signature::ECDSA_P256_SHA256_FIXED_SIGNING;

    let bytes: Vec<u8> = vec![
        48, 129, 135, 2, 1, 0, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72, 206,
        61, 3, 1, 7, 4, 109, 48, 107, 2, 1, 1, 4, 32, 161, 231, 12, 64, 10, 98, 188, 142, 95, 151,
        41, 75, 22, 45, 167, 228, 199, 84, 182, 50, 7, 167, 152, 143, 58, 184, 72, 26, 229, 154,
        192, 79, 161, 68, 3, 66, 0, 4, 51, 130, 154, 162, 204, 205, 164, 133, 238, 33, 84, 33, 189,
        108, 42, 243, 230, 225, 112, 46, 50, 2, 121, 10, 244, 42, 115, 50, 195, 252, 6, 236, 8,
        190, 175, 239, 11, 80, 78, 210, 13, 81, 118, 246, 50, 61, 163, 164, 211, 76, 87, 97, 168,
        36, 135, 8, 125, 147, 235, 214, 115, 202, 114, 147,
    ];

    EcdsaKeyPair::from_pkcs8(algo, &bytes).unwrap()
}

/// Get a fake staker capable of signing transaction
pub fn get_fake_staker() -> Staker {
    Staker {
        keys: get_fake_ecdsa_key(),
    }
}

pub fn get_random_ecdsa_key() -> EcdsaKeyPair {
    let rng = ring::rand::SystemRandom::new();

    let algo = &signature::ECDSA_P256_SHA256_FIXED_SIGNING;

    let pkcs8 = EcdsaKeyPair::generate_pkcs8(algo, &rng).expect("could not generate random key");

    EcdsaKeyPair::from_pkcs8(algo, &pkcs8.as_ref()).unwrap()
}

/// Create a staker represented by a valid but arbitrary keypair
pub fn get_random_staker() -> Staker {
    Staker {
        keys: get_random_ecdsa_key(),
    }
}
