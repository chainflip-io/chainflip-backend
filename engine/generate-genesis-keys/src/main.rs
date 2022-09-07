use chainflip_engine::{
    logging::utils::new_discard_logger,
    multisig::{client::keygen::generate_key_data_until_compatible, eth, PersistentKeyDB, Rng},
};
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
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
    use_chainflip_account_id_encoding();

    let node_id_to_name_map = load_node_ids_from_csv(
        csv::Reader::from_path(&env::var(ENV_VAR_INPUT_FILE).unwrap_or_else(|_| {
            panic!(
                "No genesis node id csv file defined with {}",
                ENV_VAR_INPUT_FILE
            )
        }))
        .expect("Should read from csv file"),
    );

    let (eth_key_id, key_shares) = generate_key_data_until_compatible::<eth::Point>(
        BTreeSet::from_iter(node_id_to_name_map.keys().cloned()),
        20,
        Rng::from_entropy(),
    );

    // Create a db for each key share, giving the db the name of the node it is for.
    for (node_id, key_share) in key_shares {
        PersistentKeyDB::new_and_migrate_to_latest(
            &Path::new(
                node_id_to_name_map
                    .get(&node_id)
                    .unwrap_or_else(|| panic!("Should have name for node_id: {}", node_id)),
            )
            .with_extension(DB_EXTENSION),
            // The genesis hash is unknown at this time, it will be written when the node runs for the first time.
            None,
            &new_discard_logger(),
        )
        .expect("Should create database at latest version")
        .update_key::<eth::EthSigning>(&eth_key_id, &key_share);
    }

    // output to stdout - CI can read the json from stdout
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "eth_agg_key": eth_key_id.to_string() }))
            .expect("Should prettify_json")
    );
}
