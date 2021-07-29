//! ============ Helper state chain methods ==============

use std::{convert::TryInto, fs};

use super::runtime::StateChainRuntime;

use sp_core::Pair;
use substrate_subxt::PairSigner;

/// Converts a private key seed in a file to a PairSigner that can be used to submit extrinsics
pub fn get_signer_from_privkey_file(
    file_name: &str,
) -> PairSigner<StateChainRuntime, sp_core::sr25519::Pair> {
    let seed = fs::read_to_string(file_name).expect("Can't read in signing key");

    // remove the quotes that are in the file, as if entered from polkadot js
    let seed = seed.replace("\"", "");

    let bytes = hex::decode(&seed).unwrap();
    let bytes: [u8; 32] = bytes.try_into().unwrap();

    let pair = sp_core::sr25519::Pair::from_seed(&bytes);
    let signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair> = PairSigner::new(pair);

    return signer;
}

/// Converts a seed phrase in a file to a PairSigner that can be used to submit extrinsics
#[allow(dead_code)]
pub fn get_signer_from_seed_file(
    file_name: &str,
) -> PairSigner<StateChainRuntime, sp_core::sr25519::Pair> {
    let seed_phrase = fs::read_to_string(file_name).expect("Can't read in signing key");

    // remove the quotes that are in the file, as if entered from polkadot js
    let seed_phrase = seed_phrase.replace("\"", "");

    let pair = sp_core::sr25519::Pair::from_phrase(&seed_phrase, None)
        .unwrap()
        .0;
    let signer: PairSigner<StateChainRuntime, sp_core::sr25519::Pair> = PairSigner::new(pair);

    return signer;
}
