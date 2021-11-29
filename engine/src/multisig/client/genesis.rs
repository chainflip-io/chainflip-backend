use std::convert::TryInto;

use super::tests::KeygenContext;
use crate::p2p::AccountId;

// Generate the keys for genesis
// Run test to ensure it doesn't panic
#[tokio::test]
pub async fn genesis_keys() {
    println!("Generating keys..");
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

    let grumpy =
        hex::decode("28b5f5f1654393975f58e78cf06b6f3ab509b3629b0a4b08aaa3dce6bf6af805").unwrap();
    let grumpy: [u8; 32] = grumpy.try_into().unwrap();
    let grumpy = AccountId(grumpy);
    println!("grumpy: {:?}", grumpy);

    let happy =
        hex::decode("7e6eb0b15c1767360fdad63d6ff78a97374355b00b4d3511a522b1a8688a661d").unwrap();
    let happy: [u8; 32] = happy.try_into().unwrap();
    let happy = AccountId(happy);
    println!("happy: {:?}", happy);

    // These are in lexicographical order
    let account_ids = vec![
        grumpy.clone(),
        bashful.clone(),
        happy.clone(),
        doc.clone(),
        dopey.clone(),
    ];
    let mut keygen_context = KeygenContext::new_with_account_ids(account_ids);
    let result = keygen_context.generate().await;

    println!(
        "Pubkey is (66 chars, 33 bytes): {:?}",
        hex::encode(result.key_ready_data().pubkey.serialize())
    );
    println!("NB: When deploying the contracts, remember to flip the first byte to 00 or 01, in place of 02 or 03.");
    let account_id_to_idx_mapping = result.key_ready_data().sec_keys[0].validator_map.clone();
    let secret_keys = &result.key_ready_data().sec_keys;

    // pretty print the output :)
    let bashful_secret =
        secret_keys[account_id_to_idx_mapping.get_idx(&bashful).unwrap() - 1].clone();
    let bashful_secret =
        bincode::serialize(&bashful_secret).expect("Could not serialize bashful_secret");
    let bashful_secret = hex::encode(bashful_secret);
    println!("Bashful secret: {:?}", bashful_secret);

    let doc_secret = secret_keys[account_id_to_idx_mapping.get_idx(&doc).unwrap() - 1].clone();
    let doc_secret = bincode::serialize(&doc_secret).expect("Could not serialize doc_secret");
    let doc_secret = hex::encode(doc_secret);
    println!("Doc secret: {:?}", doc_secret);

    let dopey_secret = secret_keys[account_id_to_idx_mapping.get_idx(&dopey).unwrap() - 1].clone();
    let dopey_secret = bincode::serialize(&dopey_secret).expect("Could not serialize dopey_secret");
    let dopey_secret = hex::encode(dopey_secret);
    println!("Dopey secret: {:?}", dopey_secret);

    let grumpy_secret =
        secret_keys[account_id_to_idx_mapping.get_idx(&grumpy).unwrap() - 1].clone();
    let grumpy_secret =
        bincode::serialize(&grumpy_secret).expect("Could not serialize grumpy_secret");
    let grumpy_secret = hex::encode(grumpy_secret);
    println!("Grumpy secret: {:?}", grumpy_secret);

    let happy_secret = secret_keys[account_id_to_idx_mapping.get_idx(&happy).unwrap() - 1].clone();
    let happy_secret = bincode::serialize(&happy_secret).expect("Could not serialize happy_secret");
    let happy_secret = hex::encode(happy_secret);
    println!("Happy secret: {:?}", happy_secret);
}
