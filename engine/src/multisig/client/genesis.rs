#[cfg(test)]
mod tests {
    use csv;
    use serde_json;
    use std::collections::{BTreeSet, HashMap};
    use std::fs::File;
    use std::io::prelude::Write;
    use std::{env, io};

    use crate::multisig::client::ensure_unsorted;
    use crate::multisig::client::tests::{
        run_keygen_with_err_on_high_pubkey, standard_signing, SigningCeremonyRunner,
    };
    use crate::multisig::crypto::Rng;
    use crate::multisig::tests::fixtures::MESSAGE_HASH;
    use state_chain_runtime::AccountId;

    const ENV_VAR_OUTPUT_FILE: &str = "KEYSHARES_JSON_OUTPUT";
    const ENV_VAR_INPUT_FILE: &str = "GENESIS_NODE_IDS";

    // If no `ENV_VAR_INPUT_FILE` is defined, then these default names and ids are used to run the genesis unit test
    const DEFAULT_CSV_CONTENT: &str = "node_name, node_id
DOC,5HEezwP9EediVA3s7UqkWKhxqTBwUuYgx3jCcqKV2jB79Fpy
BASHFUL,5DJVVEYPDFZjj9JtJRE2vGvpeSnzBAUA74VXPSpkGKhJSHbN
DOPEY,5Ge1xF1U3EUgKiGYjLCWmgcDHXQnfGNEujwXYTjShF6GcmYZ";

    type Record = (String, AccountId);

    fn load_node_ids_from_csv<R>(mut reader: csv::Reader<R>) -> HashMap<String, AccountId>
    where
        R: io::Read,
    {
        // Note: The csv reader will ignore the first row by default. Make sure the first row is only used for headers.
        let mut node_names: BTreeSet<String> = BTreeSet::new();
        let mut node_ids: BTreeSet<AccountId> = BTreeSet::new();
        reader
                .records()
                .map(|result_record| {
                    let (name, id) = result_record.expect("Error reading csv record").deserialize::<Record>(None).expect("Error reading CSV: Bad format. Could not deserialise record into (String, AccountId). Make sure it does not have spaces after/before the commas.");
                    // Check for duplicate names and ids
                    assert!(
                        node_names.insert(name.clone()),
                        "Duplicate node name {} in csv",
                        &name
                    );
                    assert!(
                        node_ids.insert(id.clone()),
                        "Duplicate node id {} reused by {}",
                        &id,
                        &name
                    );
                    (name, id)
                })
                .collect::<HashMap<String, AccountId>>()
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

        assert!(
            node_name_to_id_map.len() > 1,
            "Not enough nodes to run genesis"
        );

        // Run keygen
        println!("Generating keys");

        let account_ids = ensure_unsorted(node_name_to_id_map.values().cloned().collect(), 0);

        let (key_id, key_data, _) = {
            let mut count = 0;
            loop {
                if count >= 20 {
                    panic!("20 runs and no key generated. There's a 0.5^20 chance of this happening. Well done.");
                } else {
                    if let Ok((key_id, key_data, nodes)) =
                        run_keygen_with_err_on_high_pubkey(account_ids.clone()).await
                    {
                        // Check Key Works
                        use rand_legacy::FromEntropy;
                        let (mut signing_ceremony, non_signing_nodes) =
                            SigningCeremonyRunner::new_with_threshold_subset_of_signers(
                                nodes,
                                1,
                                key_id.clone(),
                                key_data.clone(),
                                MESSAGE_HASH.clone(),
                                Rng::from_entropy(),
                            );
                        standard_signing(&mut signing_ceremony).await;

                        break (
                            key_id,
                            key_data,
                            signing_ceremony
                                .nodes
                                .into_iter()
                                .chain(non_signing_nodes)
                                .collect::<HashMap<_, _>>(),
                        );
                    }
                    count += 1;
                }
            }
        };

        // Print the output
        println!("Pubkey is (66 chars, 33 bytes): {}", key_id);

        let mut output: HashMap<String, String> = node_name_to_id_map
            .iter()
            .map(|(node_name, account_id)| {
                let secret = hex::encode(
                    bincode::serialize(&key_data[account_id])
                        .unwrap_or_else(|_| panic!("Could not serialize secret for {}", node_name)),
                );
                println!("{}'s secret: {:?}", &node_name, &secret);
                (node_name.to_string(), secret)
            })
            .collect();

        // Output the secret shares and the Pubkey to a file if the env var exists
        output.insert("AGG_KEY".to_string(), key_id.to_string());
        if let Ok(output_file_path) = env::var(ENV_VAR_OUTPUT_FILE) {
            println!("Outputting key shares to {}", output_file_path);
            let mut file = File::create(&output_file_path)
                .unwrap_or_else(|_| panic!("Cant create file {}", output_file_path));

            let json_output = serde_json::to_string(&output).expect("Should make output into json");
            file.write_all(json_output.as_bytes())
                .unwrap_or_else(|_| panic!("Failed to write to file {}", output_file_path));
        } else {
            println!("No output file defined with {}", ENV_VAR_OUTPUT_FILE);
        }
    }
}
