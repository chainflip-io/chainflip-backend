use ring::{
    rand,
    signature::{self, EcdsaKeyPair, VerificationAlgorithm},
};

use crate::common::*;

use super::UnstakeRequestTx;

fn serialize_for_signing(tx: &UnstakeRequestTx) -> Vec<u8> {
    format!(
        "{}|{}|{}|{}",
        tx.staker_id, tx.loki_address, tx.other_address, tx.timestamp.0
    )
    .as_bytes()
    .into()
}

/// Implementation details
mod detail {

    use super::*;

    pub(super) fn sign(data: &[u8], keys: &EcdsaKeyPair) -> Result<Vec<u8>, ()> {
        let rng = rand::SystemRandom::new();

        let sig = keys.sign(&rng, &data).map_err(|_| ())?;

        Ok(sig.as_ref().into())
    }

    pub(super) fn verify(signed_data: &[u8], signature: &[u8], pubkey: &[u8]) -> Result<(), ()> {
        signature::ECDSA_P256_SHA256_FIXED
            .verify(pubkey.into(), signed_data.into(), signature.into())
            .map_err(|_| ())
    }
}

/// Sign `tx` with `keys` (for testing)
pub fn sign_unstake(tx: &UnstakeRequestTx, keys: &EcdsaKeyPair) -> Result<Vec<u8>, ()> {
    let data = serialize_for_signing(tx);

    detail::sign(&data, keys)
}

/// Verify signature in `tx`
pub fn verify_unstake(tx: &UnstakeRequestTx) -> Result<(), ()> {
    let pubkey = hex::decode(&tx.staker_id.inner()).map_err(|_| ())?;

    let signed_data = serialize_for_signing(tx);

    let signature = hex::decode(&tx.signature).map_err(|_| ())?;

    detail::verify(&signed_data, &signature, &pubkey)
}

fn get_random_ecdsa_key() -> EcdsaKeyPair {
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

#[cfg(test)]
mod tests {

    use ring::signature::KeyPair;

    use crate::{common::Staker, utils::test_utils::fake_txs::create_unstake_for_staker};

    use super::*;

    fn get_fake_ecdsa_key() -> EcdsaKeyPair {
        let algo = &signature::ECDSA_P256_SHA256_FIXED_SIGNING;

        let bytes: Vec<u8> = vec![
            48, 129, 135, 2, 1, 0, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72,
            206, 61, 3, 1, 7, 4, 109, 48, 107, 2, 1, 1, 4, 32, 161, 231, 12, 64, 10, 98, 188, 142,
            95, 151, 41, 75, 22, 45, 167, 228, 199, 84, 182, 50, 7, 167, 152, 143, 58, 184, 72, 26,
            229, 154, 192, 79, 161, 68, 3, 66, 0, 4, 51, 130, 154, 162, 204, 205, 164, 133, 238,
            33, 84, 33, 189, 108, 42, 243, 230, 225, 112, 46, 50, 2, 121, 10, 244, 42, 115, 50,
            195, 252, 6, 236, 8, 190, 175, 239, 11, 80, 78, 210, 13, 81, 118, 246, 50, 61, 163,
            164, 211, 76, 87, 97, 168, 36, 135, 8, 125, 147, 235, 214, 115, 202, 114, 147,
        ];

        EcdsaKeyPair::from_pkcs8(algo, &bytes).unwrap()
    }

    fn get_fake_staker() -> Staker {
        Staker {
            keys: get_fake_ecdsa_key(),
        }
    }

    #[test]
    fn basic_signing() {
        let keys = get_random_ecdsa_key();

        let data = [1, 2, 3];

        let sig = detail::sign(&data, &keys).unwrap();

        assert!(detail::verify(&data, &sig, keys.public_key().as_ref()).is_ok());
    }

    #[test]
    fn unstake_is_serialized_as_expected() {
        let staker = get_fake_staker();

        let mut tx = create_unstake_for_staker(PoolCoin::ETH, &staker);

        tx.timestamp = Timestamp(1603777110013u128);

        let expected = "0433829aa2cccda485ee215421bd6c2af3e6e1702e3202790af42a7332c3fc06ec08beafef0b504ed20d5176f6323da3a4d34c5761a82487087d93ebd673ca7293|T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY|0x70e7db0678460c5e53f1ffc9221d1c692111dcc5|1603777110013".as_bytes();

        assert_eq!(serialize_for_signing(&tx), expected);
    }

    #[test]
    fn signature_verifies() {
        let staker = get_fake_staker();

        let fake_unstake = create_unstake_for_staker(PoolCoin::ETH, &staker);

        verify_unstake(&fake_unstake).expect("Signature should be valid for unstake tx");
    }
}
