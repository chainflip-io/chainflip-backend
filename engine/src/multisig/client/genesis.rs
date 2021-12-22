use super::tests::KeygenContext;

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
    use csv;
    use serde_json;
    use std::collections::HashMap;
    use std::convert::TryInto;
    use std::env;
    use std::fs::File;
    use std::io::prelude::Write;

    use crate::multisig::KeygenOptions;
    use crate::{multisig::client::ensure_unsorted, p2p::AccountId};

    const ENV_VAR_OUTPUT_FILE: &str = "KEYSHARES_JSON_OUTPUT";
    const ENV_VAR_INPUT_FILE: &str = "GENESIS_NODE_IDS";

    const NODE_NAMES: &[&str] = &["bashful", "doc", "dopey"];
    const DEFAULT_NODE_IDS: &[&str] = &[
        "36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911",
        "8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04",
        "ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e",
    ];

    fn account_id_from_string(account_id_hex: &str) -> Result<AccountId, anyhow::Error> {
        let data = hex::decode(&account_id_hex.trim()).map_err(|e| {
            anyhow::Error::msg(format!("Invalid account id {:?}: {:?}", &account_id_hex, e))
        })?;

        let data: [u8; 32] = data.try_into().map_err(|e| {
            anyhow::Error::msg(format!("Invalid account id {:?}: {:?}", &account_id_hex, e))
        })?;

        Ok(AccountId(data))
    }

    // Generate the keys for genesis
    // Run test to ensure it doesn't panic
    #[tokio::test]
    pub async fn genesis_keys() {
        let mut node_name_to_id_map: HashMap<String, AccountId> = HashMap::new();

        // Load the node id from a csv file if the env var exists
        if let Ok(input_file_path) = env::var(ENV_VAR_INPUT_FILE) {
            if let Ok(mut rdr) = csv::Reader::from_path(&input_file_path) {
                for result in rdr.records() {
                    // Get the data from the csv
                    let mut records: Vec<Vec<String>> = vec![];
                    match result {
                        Ok(record) => {
                            println!("record found: {:?}", record);
                            let mut items: Vec<String> = vec![];
                            for item in record.iter() {
                                println!("item: {}", item);
                                items.push(item.to_string());
                            }
                            records.push(items);
                        }
                        Err(e) => {
                            println!("Error reading csv record: {}", e);
                        }
                    }
                    // Parse the node id's and fill in the map
                    for record in records {
                        if record.len() != 2 {
                            println!("Error reading csv: bad format");
                        } else {
                            for node_name in NODE_NAMES {
                                if &record[0].to_lowercase() == *node_name {
                                    println!(
                                        "Got id from csv for {} - {:?}",
                                        *node_name,
                                        account_id_from_string(&record[1])
                                    );
                                    node_name_to_id_map.insert(
                                        node_name.to_string(),
                                        account_id_from_string(&record[1]).expect(&format!(
                                            "Error loading node ids from {}",
                                            &input_file_path
                                        )),
                                    );
                                }
                            }
                        }
                    }
                }
            } else {
                println!("No genesis csv found at {}", &input_file_path);
            }
        } else {
            println!(
                "No genesis node id csv file defined with {}, using default values",
                ENV_VAR_INPUT_FILE
            );
            for i in 0..NODE_NAMES.len() {
                node_name_to_id_map.insert(
                    NODE_NAMES[i].to_string(),
                    account_id_from_string(DEFAULT_NODE_IDS[i]).unwrap(),
                );
            }
        }

        println!("Generating keys");

        let account_ids = ensure_unsorted(node_name_to_id_map.values().cloned().collect(), 0);
        let mut keygen_context =
            KeygenContext::new_with_account_ids(account_ids.clone(), KeygenOptions::default());

        let valid_keygen_states = {
            let mut count = 0;
            let value = loop {
                if count >= 20 {
                    panic!("20 runs and no key generated. There's a 0.5^20 chance of this happening. Well done.");
                }
                let valid_keygen_states = keygen_context.generate().await;

                if valid_keygen_states.key_ready_data().is_some() {
                    break valid_keygen_states;
                }
                count += 1;
            };
            value
        };

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
            hex::encode(
                valid_keygen_states
                    .key_ready_data()
                    .expect("successful_keygen")
                    .pubkey
                    .serialize()
            )
        );

        let secret_keys = &valid_keygen_states
            .key_ready_data()
            .expect("successful keygen")
            .sec_keys;

        // Print the output :)
        let mut output: HashMap<String, String> = HashMap::new();
        for i in 0..NODE_NAMES.len() {
            let secret = secret_keys[&node_name_to_id_map[NODE_NAMES[i]]].clone();
            let secret = bincode::serialize(&secret)
                .expect(&format!("Could not serialize secret for {}", NODE_NAMES[i]));
            let secret = hex::encode(secret);
            output.insert(NODE_NAMES[i].to_string(), secret.clone());
            println!("{}'s secret: {:?}", NODE_NAMES[i], secret);
        }

        // Output the secret shares to a file if the env var exists
        if let Ok(output_file_path) = env::var(ENV_VAR_OUTPUT_FILE) {
            println!("Outputting key shares to {}", output_file_path);
            let mut file = File::create(&output_file_path)
                .expect(&format!("Cant create file {}", output_file_path));

            let json_output = serde_json::to_string(&output).expect("Should make output into json");
            file.write_all(json_output.as_bytes())
                .expect(&format!("Failed to write to file {}", output_file_path));
        } else {
            println!("No output file defined with {}", ENV_VAR_OUTPUT_FILE);
        }
    }
}
