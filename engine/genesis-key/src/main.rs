//! A script that can generate keyshares at genesis

use std::convert::TryInto;

use chainflip_engine::multisig::KeygenContext;
use chainflip_engine::p2p::AccountId;

use chainflip_engine::multisig::ensure_unsorted;

#[tokio::main]
async fn main() {
    println!("Generating keys...");

    let bashful =
        hex::decode("36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911").unwrap();
    let bashful: [u8; 32] = bashful.try_into().unwrap();
    let bashful = AccountId(bashful);
    println!("bashful: {:?}", bashful);

    let doc =
        hex::decode("8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04").unwrap();
    let doc: [u8; 32] = doc.try_into().unwrap();
    let doc = AccountId(doc);
    println!("doc: {:?}", doc);

    let dopey =
        hex::decode("ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e").unwrap();
    let dopey: [u8; 32] = dopey.try_into().unwrap();
    let dopey = AccountId(dopey);
    println!("dopey: {:?}", dopey);

    let account_ids = ensure_unsorted(vec![doc.clone(), dopey.clone(), bashful.clone()], 0);
    let mut keygen_context = KeygenContext::new_with_account_ids(account_ids.clone());
    let result = keygen_context.generate().await;

    // Check that we can use the above keys
    let active_ids: Vec<_> = {
        use rand::prelude::*;

        let mut rng = StdRng::seed_from_u64(0);
        let active_count = utilities::threshold_from_share_count(account_ids.len() as u32) + 1;

        ensure_unsorted(
            account_ids
                .choose_multiple(&mut rng, active_count as usize)
                .cloned()
                .collect(),
            0,
        )
    };

    let signing_result = keygen_context.sign_with_ids(&active_ids).await;

    assert!(
        signing_result.outcome.result.is_ok(),
        "Signing ceremony failed"
    );

    println!(
        "Pubkey is (66 chars, 33 bytes): {:?}",
        hex::encode(result.key_ready_data().pubkey.serialize())
    );

    let secret_keys = &result.key_ready_data().sec_keys;

    // pretty print the output :)
    let bashful_secret = secret_keys[&bashful].clone();
    let bashful_secret =
        bincode::serialize(&bashful_secret).expect("Could not serialize bashful_secret");
    let bashful_secret = hex::encode(bashful_secret);
    println!("Bashfuls secret: {:?}", bashful_secret);

    let doc_secret = secret_keys[&doc].clone();
    let doc_secret = bincode::serialize(&doc_secret).expect("Could not serialize doc_secret");
    let doc_secret = hex::encode(doc_secret);
    println!("Doc secret_idx {:?}", doc_secret);

    let dopey_secret = secret_keys[&dopey].clone();
    let dopey_secret = bincode::serialize(&dopey_secret).expect("Could not serialize dopey_secret");
    let dopey_secret = hex::encode(dopey_secret);
    println!("Dopey secret idx {:?}", dopey_secret);
}
