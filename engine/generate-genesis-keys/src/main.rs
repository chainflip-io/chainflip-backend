use chainflip_engine::multisig::{
    client::keygen::generate_key_data_until_compatible,
    db::persistent::{DATA_COLUMN, DB_SCHEMA_VERSION, DB_SCHEMA_VERSION_KEY, METADATA_COLUMN},
    eth, KeyDB, PersistentKeyDB,
};
use rocksdb::{Options, DB};
use state_chain_runtime::AccountId;
use std::{
    collections::{BTreeSet, HashMap},
    env, io,
};

const COLUMN_FAMILIES: &[&str] = &[DATA_COLUMN, METADATA_COLUMN];

const ENV_VAR_INPUT_FILE: &str = "GENESIS_NODE_IDS";

const DB_NAME_SUFFIX: &str = ".db";

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

    let num_nodes = node_id_to_name_map.len();

    assert!(
        num_nodes > 1,
        "Must have more than one node to run genesis key share generation"
    );

    println!("Creating genesis databases for {} nodes...", num_nodes);

    let account_ids = node_id_to_name_map.keys().cloned().collect::<Vec<_>>();

    let (eth_key_id, key_shares) =
        generate_key_data_until_compatible::<eth::Point>(&account_ids, 20);

    let mut opts = Options::default();
    opts.create_missing_column_families(true);
    opts.create_if_missing(true);

    // Open a db for each key share:=
    for (node_id, key_share) in key_shares {
        let node_name = node_id_to_name_map
            .get(&node_id)
            .unwrap_or_else(|| panic!("Should have name for node_id: {}", node_id));
        let db_path = format!("{}{}", node_name, DB_NAME_SUFFIX);
        let db = DB::open_cf(&opts, &db_path, COLUMN_FAMILIES).expect("Should open db file");

        // Write the schema version
        db.put_cf(
            db.cf_handle(METADATA_COLUMN).unwrap_or_else(|| {
                panic!("Should get column family handle for {}", METADATA_COLUMN)
            }),
            DB_SCHEMA_VERSION_KEY,
            DB_SCHEMA_VERSION.to_be_bytes(),
        )
        .expect("Should write DB_SCHEMA_VERSION");

        let mut p_kdb = PersistentKeyDB::new_from_db(
            db,
            &chainflip_engine::logging::utils::new_discard_logger(),
        );

        // Write the key share to the db
        p_kdb.update_key(&eth_key_id, &key_share);
    }

    println!("Done!");
}
