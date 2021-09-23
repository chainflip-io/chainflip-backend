use std::convert::TryInto;

use super::tests::KeygenContext;
use crate::p2p::AccountId;

// Generate the keys for genesis
#[tokio::test]
#[ignore = "Run manually to generate genesis key shares/shards"]
pub async fn genesis_keys() {
    println!("Generating keys");
    let bashful =
        hex::decode("36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911").unwrap();
    println!("bashful: {:?}", bashful);
    let bashful: [u8; 32] = bashful.try_into().unwrap();
    let bashful = AccountId(bashful);

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

    let account_ids = vec![bashful.clone(), doc.clone(), dopey.clone()];
    let mut keygen_context = KeygenContext::new_with_account_ids(account_ids);
    let result = keygen_context.generate().await;

    println!(
        "Pubkey is (66 chars, 33 bytes): {:?}",
        hex::encode(result.key_ready.pubkey.serialize())
    );
    let account_id_to_idx_mapping = result.key_ready.sec_keys[0].validator_map.clone();
    let secret_keys = result.key_ready.sec_keys;

    // pretty print the output :)
    let bashful_secret =
        secret_keys[account_id_to_idx_mapping.get_idx(&bashful).unwrap() - 1].clone();
    let bashful_secret =
        bincode::serialize(&bashful_secret).expect("Could not serialize bashful_secret");
    let bashful_secret = hex::encode(bashful_secret);
    println!("Bashfuls secret: {:?}", bashful_secret);

    let doc_secret = secret_keys[account_id_to_idx_mapping.get_idx(&doc).unwrap() - 1].clone();
    let doc_secret = bincode::serialize(&doc_secret).expect("Could not serialize doc_secret");
    let doc_secret = hex::encode(doc_secret);
    println!("Doc secret_idx {:?}", doc_secret);

    let dopey_secret = secret_keys[account_id_to_idx_mapping.get_idx(&dopey).unwrap() - 1].clone();
    let dopey_secret = bincode::serialize(&dopey_secret).expect("Could not serialize dopey_secret");
    let dopey_secret = hex::encode(dopey_secret);
    println!("Dopey secret idx {:?}", dopey_secret);
}
