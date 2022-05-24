use chainflip_engine::{
    logging::utils::new_discard_logger,
    multisig::{
        client::keygen::generate_key_data_until_compatible, eth, KeyDB, PersistentKeyDB, Rng,
    },
};
use rand_legacy::FromEntropy;
use state_chain_runtime::AccountId;
use std::{
    collections::{BTreeSet, HashMap},
    env, io,
    path::Path,
};

const ENV_VAR_INPUT_FILE: &str = "GENESIS_NODE_IDS";

const DB_EXTENSION: &str = "db";

type Record = (String, AccountId);

fn load_node_ids_from_csv<R>(mut reader: csv::Reader<R>) -> HashMap<AccountId, String>
where
    R: io::Read,
{
    // Note: The csv reader will ignore the first row by default. Make sure the first row is only used for headers.

    // Used to check for duplicate names and ids in the CSV. If there are duplicates,
    // we want to panic and have the problem in the CSV resolved rather than potentially
    // generating unexpected results.
    let mut node_names: BTreeSet<String> = BTreeSet::new();
    let mut node_ids: BTreeSet<AccountId> = BTreeSet::new();
    reader
            .records()
            .map(|result_record| {
                let (name, id) = result_record.expect("Error reading csv record").deserialize::<Record>(None).expect("Error reading CSV: Bad format. Could not deserialise record into (String, AccountId). Make sure it does not have spaces after/before the commas.");
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
                (id, name)
            })
            .collect()
}

fn main() {
    println!("Starting...");
    let input_file_path = env::var(ENV_VAR_INPUT_FILE).unwrap_or_else(|_| {
        panic!(
            "No genesis node id csv file defined with {}",
            ENV_VAR_INPUT_FILE
        )
    });

    let node_id_to_name_map = load_node_ids_from_csv(
        csv::Reader::from_path(&input_file_path).expect("Should read from csv file"),
    );

    println!(
        "Creating genesis databases for {} nodes...",
        node_id_to_name_map.len()
    );

    let rng = Rng::from_entropy();

    let (eth_key_id, key_shares) = generate_key_data_until_compatible::<eth::Point>(
        &node_id_to_name_map.keys().cloned().collect::<Vec<_>>(),
        20,
        rng,
    );

    // Open a db for each key share:=
    for (node_id, key_share) in key_shares {
        PersistentKeyDB::new_and_migrate_to_latest(
            Path::new(
                &Path::new(
                    node_id_to_name_map
                        .get(&node_id)
                        .unwrap_or_else(|| panic!("Should have name for node_id: {}", node_id)),
                )
                .with_extension(DB_EXTENSION),
            ),
            &new_discard_logger(),
        )
        .expect("Should create database at latest version")
        .update_key(&eth_key_id, &key_share);
    }

    println!("Done!");
}
