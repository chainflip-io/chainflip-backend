use super::tests::KeygenContext;

#[cfg(test)]
mod tests {
    use super::*;

    use csv;
    use serde_json;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::prelude::Write;
    use std::{env, io};

    use crate::multisig::client::ensure_unsorted;
    use crate::multisig::KeygenOptions;
    use crate::testing::assert_ok;
    use state_chain_runtime::AccountId;

    const ENV_VAR_OUTPUT_FILE: &str = "KEYSHARES_JSON_OUTPUT";
    const ENV_VAR_INPUT_FILE: &str = "GENESIS_NODE_IDS";

    // If no `ENV_VAR_INPUT_FILE` is defined, then these default names and ids are used to run the genesis unit test
    const DEFAULT_CSV_CONTENT: &str = "node_name, node_id
DOC,5F9ofCBWLMFXotypXtbUnbLXkM1Z9As47GRhDDFJDsxKgpBu
BASHFUL,5DJVVEYPDFZjj9JtJRE2vGvpeSnzBAUA74VXPSpkGKhJSHbN
DOPEY,5Ge1xF1U3EUgKiGYjLCWmgcDHXQnfGNEujwXYTjShF6GcmYZ";

    type Record = (String, AccountId);

    fn load_node_ids_from_csv<R>(mut reader: csv::Reader<R>) -> HashMap<String, AccountId>
    where
        R: io::Read,
    {
        // Note: The csv reader will ignore the first row by default. Make sure the first row is only used for headers.
        let nodes = reader
                .records()
                .filter_map(|result| match result {
                    Ok(result) => Some(result),
                    Err(e) => {
                        panic!("Error reading csv record: {}", e);
                    }
                })
                .filter_map(|record| {
                    match record.deserialize::<Record>(None) {
                        Ok(record) => Some(record),
                        Err(e) => {
                            panic!("Error reading CSV: Bad format. Could not deserialise record into (String, AccountId). Make sure it does not have spaces after/before the commas. Error: {}", e);
                        }
                    }
                })
                .collect::<Vec<(String, AccountId)>>();

        // Check for duplicate names when filling the HashMap
        let mut mode_id_map = HashMap::new();
        nodes.iter().for_each(|(name, id)| {
            assert!(
                mode_id_map.insert(name.clone(), id.clone()).is_none(),
                "Duplicate node name {} in csv",
                &name
            );
        });

        mode_id_map
    }

    // Generate the keys for genesis
    // Run test to ensure it doesn't panic
    #[tokio::test]
    pub async fn genesis_keys() {
        // Load the node id from a csv file if the env var exists
        let node_name_to_id_map = match env::var(ENV_VAR_INPUT_FILE) {
            Ok(input_file_path) => {
                println!("Loading node ids from {}", input_file_path);
                load_node_ids_from_csv(
                    csv::Reader::from_path(&input_file_path).expect("Should read from csv file"),
                )
            }
            Err(_) => {
                println!(
                    "No genesis node id csv file defined with {}, using default values",
                    ENV_VAR_INPUT_FILE
                );
                load_node_ids_from_csv(csv::Reader::from_reader(DEFAULT_CSV_CONTENT.as_bytes()))
            }
        };

        // Check for duplicate ids
        for (_, node_id) in node_name_to_id_map.clone() {
            let duplicates: HashMap<&String, &AccountId> = node_name_to_id_map
                .iter()
                .filter(|(_, id)| *id == &node_id)
                .collect();
            assert!(
                duplicates.len() == 1,
                "Found a duplicate node id {:?}",
                duplicates
            );
        }

        assert!(
            node_name_to_id_map.len() > 1,
            "Not enough nodes to run genesis"
        );

        // Run keygen
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
                let valid_keygen_states = keygen_context.generate_with_ceremony_id(count).await;

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

            ensure_unsorted(
                account_ids
                    .choose_multiple(
                        &mut StdRng::seed_from_u64(0),
                        utilities::success_threshold_from_share_count(account_ids.len() as u32)
                            as usize,
                    )
                    .cloned()
                    .collect(),
                0,
            )
        };

        assert_ok!(
            keygen_context
                .sign_with_ids(&active_ids)
                .await
                .sign_finished
                .outcome
                .result
        );

        // Print the output
        let pub_key = hex::encode(
            valid_keygen_states
                .key_ready_data()
                .expect("successful keygen")
                .pubkey
                .serialize(),
        );
        println!("Pubkey is (66 chars, 33 bytes): {:?}", pub_key);

        let secret_keys = &valid_keygen_states
            .key_ready_data()
            .expect("successful keygen")
            .sec_keys;

        let mut output: HashMap<String, String> = node_name_to_id_map
            .iter()
            .map(|(node_name, account_id)| {
                let secret = hex::encode(
                    bincode::serialize(&secret_keys[&account_id].clone())
                        .expect(&format!("Could not serialize secret for {}", node_name)),
                );
                println!("{}'s secret: {:?}", &node_name, &secret);
                (node_name.to_string(), secret.clone())
            })
            .collect();

        // Output the secret shares and the Pubkey to a file if the env var exists
        output.insert("AGG_KEY".to_string(), pub_key);
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
