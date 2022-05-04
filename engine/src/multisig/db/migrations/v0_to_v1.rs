use std::{collections::HashMap, path::Path, sync::Arc};

use rocksdb::{WriteBatch, DB};

use anyhow::Result;

use crate::multisig::{
    client::{KeygenResultInfo, PartyIdxMapping, ThresholdParameters},
    db::persistent::{
        add_schema_version_to_batch_write, get_data_column_handle, update_key, KEYGEN_DATA_PREFIX,
        LEGACY_DATA_COLUMN_NAME, PREFIX_SIZE,
    },
    KeyId,
};

mod old_types {
    use std::{collections::HashMap, sync::Arc};

    use frame_support::Deserialize;
    use state_chain_runtime::AccountId;

    use crate::multisig::client::KeygenResult;

    #[derive(Deserialize, Debug)]
    pub struct ThresholdParameters {
        pub share_count: usize,
        pub threshold: usize,
    }

    #[derive(Deserialize, Debug)]
    pub struct PartyIdxMapping {
        pub id_to_idx: HashMap<AccountId, usize>,
        pub account_ids: Vec<AccountId>,
    }

    #[derive(Deserialize, Debug)]
    pub struct KeygenResultInfo {
        pub key: Arc<KeygenResult>,
        pub validator_map: Arc<PartyIdxMapping>,
        pub params: ThresholdParameters,
    }
}

// We require the keys to be loaded using the kvdb library if the database was initially created with it

// Load the keys using kvdb for a special migration (not included in `migrate_db_to_latest`).
// NB: If the database is on this version, then it necessarily also has the old key version data
// This is necessary to load the keys using the kvdb library
// We then insert using the rocks db library
// We can't do this all with rocks db because the compression algo used by default by rust_rocksdb collides with system libs, so we use an alternate algo (lz4)
pub fn load_keys_using_kvdb_to_latest_key_type(
    path: &Path,
    logger: &slog::Logger,
) -> Result<HashMap<KeyId, KeygenResultInfo>> {
    slog::info!(logger, "Loading keys using kvdb");

    let config = kvdb_rocksdb::DatabaseConfig::default();
    let old_db = kvdb_rocksdb::Database::open(&config, path)
        .map_err(|e| anyhow::Error::msg(format!("could not open kvdb database: {}", e)))?;

    // Load the keys from column 0 (aka "col0")
    let old_keys: HashMap<KeyId, old_types::KeygenResultInfo> = old_db
        .iter(0)
        .map(|(key_id, key_info)| {
            let key_id: KeyId = KeyId(key_id.into());
            match bincode::deserialize::<old_types::KeygenResultInfo>(&*key_info) {
                Ok(keygen_result_info) => {
                    slog::debug!(
                        logger,
                        "Loaded key_info (key_id: {}) from kvdb database",
                        key_id
                    );
                    Ok((key_id, keygen_result_info))
                }
                Err(err) => Err(anyhow::Error::msg(format!(
                    "Could not deserialize key_info (key_id: {}) from kvdb database: {}",
                    key_id, err,
                ))),
            }
        })
        .collect::<Result<HashMap<_, _>>>()?;

    Ok(old_to_new_keygen_result_info(old_keys))
}

fn old_to_new_keygen_result_info(
    old_keys: HashMap<KeyId, old_types::KeygenResultInfo>,
) -> HashMap<KeyId, KeygenResultInfo> {
    old_keys
        .into_iter()
        .map(|(key_id, old_keygen_result_info)| {
            (
                key_id,
                KeygenResultInfo {
                    key: old_keygen_result_info.key,
                    validator_map: Arc::new(PartyIdxMapping::from_unsorted_signers(
                        &old_keygen_result_info.validator_map.account_ids,
                    )),
                    params: ThresholdParameters {
                        share_count: old_keygen_result_info
                            .params
                            .share_count
                            .try_into()
                            .expect("Should fit into u16"),
                        threshold: old_keygen_result_info
                            .params
                            .threshold
                            .try_into()
                            .expect("Should fit into u16"),
                    },
                },
            )
        })
        .collect()
}

// Just adding schema version to the metadata column and delete col0 if it exists
pub fn migration_0_to_1(db: &mut DB) -> Result<(), anyhow::Error> {
    // Update version data
    let mut batch = WriteBatch::default();
    add_schema_version_to_batch_write(db, 1, &mut batch);

    // Write the batch
    db.write(batch).map_err(|e| {
        anyhow::Error::msg(format!("Failed to write to db during migration: {}", e))
    })?;

    // Delete the old column family
    let old_cf_name = LEGACY_DATA_COLUMN_NAME;
    if db.cf_handle(LEGACY_DATA_COLUMN_NAME).is_some() {
        db.drop_cf(old_cf_name)
            .unwrap_or_else(|_| panic!("Should drop old column family {}", old_cf_name));
    }

    // Read in old key types and add the new
    let old_keys: HashMap<KeyId, old_types::KeygenResultInfo> = db
        .prefix_iterator_cf(get_data_column_handle(&db), KEYGEN_DATA_PREFIX)
        .map(|(key_id, key_info)| {
            // Strip the prefix off the key_id
            let key_id: KeyId = KeyId(key_id[PREFIX_SIZE..].into());

            // deserialize the `KeygenResultInfo`
            match bincode::deserialize::<old_types::KeygenResultInfo>(&*key_info) {
                Ok(keygen_result_info) => {
                    println!(
                        "Successfully deceoding old keygen result info: {:?}",
                        keygen_result_info
                    );
                    (key_id, keygen_result_info)
                }
                Err(_) => {
                    panic!("We should not get an error on the db");
                }
            }
        })
        .collect();

    // only write if all the keys were successfully deserialized
    old_to_new_keygen_result_info(old_keys)
        .into_iter()
        .for_each(|(key_id, keygen_result_info)| {
            let keygen_result_info_bin = bincode::serialize(&keygen_result_info).unwrap();
            update_key(db, &key_id, keygen_result_info_bin).expect("Should update key in database");
        });

    Ok(())
}
