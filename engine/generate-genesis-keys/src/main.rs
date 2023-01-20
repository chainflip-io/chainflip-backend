use chainflip_engine::{
	logging::utils::new_discard_logger,
	multisig::{
		client::keygen::generate_key_data, eth::EthSigning, polkadot::PolkadotSigning,
		CryptoScheme, PersistentKeyDB, Rng,
	},
};
use chainflip_node::chain_spec::use_chainflip_account_id_encoding;
use rand_legacy::FromEntropy;
use state_chain_runtime::AccountId;
use std::{
	collections::{BTreeSet, HashMap},
	env, io,
	path::Path,
};

use serde::{Deserialize, Serialize};

const ENV_VAR_INPUT_FILE: &str = "GENESIS_NODE_IDS";

const DB_EXTENSION: &str = "db";

type Record = (String, AccountId);

#[derive(Serialize, Deserialize)]
struct AggKeys {
	eth_agg_key: String,
	dot_agg_key: String,
}

fn load_node_ids_from_csv<R>(mut reader: csv::Reader<R>) -> HashMap<AccountId, String>
where
	R: io::Read,
{
	// Note: The csv reader will ignore the first row by default. Make sure the first row is only
	// used for headers.

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
		csv::Reader::from_path(env::var(ENV_VAR_INPUT_FILE).unwrap_or_else(|_| {
			panic!("No genesis node id csv file defined with {ENV_VAR_INPUT_FILE}")
		}))
		.expect("Should read from csv file"),
	);

	let agg_keys = AggKeys {
		eth_agg_key: generate_and_save_keys::<EthSigning>(&node_id_to_name_map),
		dot_agg_key: generate_and_save_keys::<PolkadotSigning>(&node_id_to_name_map),
	};

	// output to stdout - CI can read the json from stdout
	println!("{}", serde_json::to_string_pretty(&agg_keys).expect("Should prettify json"));
}

fn generate_and_save_keys<Crypto: CryptoScheme>(
	node_id_to_name_map: &HashMap<AccountId, String>,
) -> String {
	let (key_id, key_shares) = generate_key_data::<Crypto>(
		BTreeSet::from_iter(node_id_to_name_map.keys().cloned()),
		&mut Rng::from_entropy(),
	);

	// Create a db for each key share, giving the db the name of the node it is for.
	for (node_id, key_share) in key_shares {
		PersistentKeyDB::new_and_migrate_to_latest(
			&Path::new(
				node_id_to_name_map
					.get(&node_id)
					.unwrap_or_else(|| panic!("Should have name for node_id: {node_id}")),
			)
			.with_extension(DB_EXTENSION),
			// The genesis hash is unknown at this time, it will be written when the node runs for
			// the first time.
			None,
			&new_discard_logger(),
		)
		.expect("Should create database at latest version")
		.update_key::<Crypto>(&key_id, &key_share);
	}

	key_id.to_string()
}

#[cfg(test)]
#[test]
fn should_generate_and_save_all_keys() {
	let tempdir = tempfile::TempDir::new().unwrap();
	let db_path = tempdir.path().to_owned().join("test");

	// Using the db_path as the node name, so the db is created within the temp directory
	let node_id_to_name_map =
		HashMap::from_iter(vec![(AccountId::new([0; 32]), db_path.to_string_lossy().to_string())]);

	generate_and_save_keys::<EthSigning>(&node_id_to_name_map);
	generate_and_save_keys::<PolkadotSigning>(&node_id_to_name_map);

	// Open the db and check the keys
	let db = PersistentKeyDB::new_and_migrate_to_latest(
		&db_path.with_extension(DB_EXTENSION),
		None,
		&new_discard_logger(),
	)
	.unwrap();

	assert_eq!(db.load_keys::<EthSigning>().len(), 1);
	assert_eq!(db.load_keys::<PolkadotSigning>().len(), 1);
}
