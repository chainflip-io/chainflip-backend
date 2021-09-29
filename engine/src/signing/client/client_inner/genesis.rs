use std::{convert::TryInto, env};

use super::{tests::KeygenContext, KeygenResultInfo};
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

    // // pretty print the output :)
    // let bashful_secret =
    //     secret_keys[account_id_to_idx_mapping.get_idx(&bashful).unwrap() - 1].clone();
    // let bashful_secret =
    //     bincode::serialize(&bashful_secret).expect("Could not serialize bashful_secret");
    // let bashful_secret = hex::encode(bashful_secret);
    // println!("Bashfuls secret: {:?}", bashful_secret);

    // let doc_secret = secret_keys[account_id_to_idx_mapping.get_idx(&doc).unwrap() - 1].clone();
    // let doc_secret = bincode::serialize(&doc_secret).expect("Could not serialize doc_secret");
    // let doc_secret = hex::encode(doc_secret);
    // println!("Doc secret_idx {:?}", doc_secret);

    let dopey_secret = secret_keys[account_id_to_idx_mapping.get_idx(&dopey).unwrap() - 1].clone();
    let dopey_secret_bin =
        bincode::serialize(&dopey_secret).expect("Could not serialize dopey_secret");
    let dopey_secret_hex = hex::encode(dopey_secret_bin.clone());

    // what we do before script is run
    env::set_var("TEST_SECRET_SHARE", dopey_secret_hex);

    // what the script is doing, before inserting into the db
    let dopey_secret_hex_from_env = env::var("TEST_SECRET_SHARE").unwrap();
    println!(
        "Dopey len: {}, secret: {}",
        dopey_secret_hex_from_env.len(),
        dopey_secret_hex_from_env
    );

    // Here we would be exporting them into environment variables

    let dopey_secret_bin_again = hex::decode(dopey_secret_hex_from_env).unwrap();
    println!("Dopey bin: {:?}", dopey_secret_bin_again);
    println!("Len of binary dopey: {}", dopey_secret_bin_again.len());
    assert_eq!(dopey_secret_bin, dopey_secret_bin_again);
    let keygen_info: KeygenResultInfo = bincode::deserialize(&dopey_secret_bin).unwrap();
    println!("here's the decoded keygen_info: {:?}", keygen_info);
    // println!("Length of the decoded secret: {}", decoded.len());
    // println!("Here's decoded: {:?}", decoded);
    // let key_info: KeygenResultInfo = bincode::deserialize(decoded.as_ref()).unwrap();
    // println!("key info is: {:?}", key_info);

    // MAIN SCRIPT
    // let current_path = env::current_dir().expect("Could not get current path");
    // println!("Current path is: {}", current_path.display());
    // let agg_pubkey_hex = env::var("AGG_PUBKEY").expect("AGG_PUBKEY environment variable not set");
    // let agg_pubkey_bytes = hex::decode(agg_pubkey_hex).unwrap();

    // let secret_share_hex = env::var("SIGNING_SECRET_SHARE")
    //     .expect("SIGNING_SECRET_SHARE environment variable not set");
    // // secret should be inserted as binary - we pass in a hex(binary(info)). we decode the hex to give binary(info)
    // let secret_share_bytes = hex::decode(secret_share_hex).expect("Secret is not valid hex");

    // println!("Len secret_shared_bytes: {}", secret_share_bytes.len());
    // // let signing_db_path =
    // //     env::var("SIGNING_DB_PATH").expect("SIGNING_DB_PATH environment variable not set");
    // // let config = DatabaseConfig::default();
    // // let db = Database::open(&config, &signing_db_path).expect("could not open database");
    // // let mut tx = db.transaction();
    // // tx.put_vec(0, &agg_pubkey_bytes, secret_share_bytes);

    // // db.write(tx)
    // //     .expect("Could not write shared key to database");
    // println!("Secret successfully added to database.")
}
