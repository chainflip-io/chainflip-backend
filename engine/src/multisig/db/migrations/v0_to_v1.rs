use std::{collections::HashMap, path::Path, sync::Arc};

use rocksdb::{WriteBatch, DB};

use anyhow::{Context, Result};

use crate::{
    logging::utils::new_discard_logger,
    multisig::{
        client::{KeygenResultInfo, PartyIdxMapping, ThresholdParameters},
        db::persistent::{
            add_schema_version_to_batch_write, get_data_column_handle, KEYGEN_DATA_PREFIX,
            LEGACY_DATA_COLUMN_NAME, PREFIX_SIZE,
        },
        KeyDB, KeyId, PersistentKeyDB,
    },
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

    Ok(old_keys
        .into_iter()
        .map(|(key, old_keygen_result_info)| {
            (key, old_to_new_keygen_result_info(old_keygen_result_info))
        })
        .collect())
}

fn old_to_new_keygen_result_info(
    old_keygen_result_info: old_types::KeygenResultInfo,
) -> KeygenResultInfo {
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
    }
}

// Just adding schema version to the metadata column and delete col0 if it exists
pub fn migration_0_to_1(mut db: DB) -> Result<PersistentKeyDB, anyhow::Error> {
    // Update version data
    let mut batch = WriteBatch::default();
    add_schema_version_to_batch_write(&db, 1, &mut batch);

    // Write the batch
    db.write(batch)
        .context("Failed to write to db during migration")?;

    // Delete the old column family
    let old_cf_name = LEGACY_DATA_COLUMN_NAME;
    if db.cf_handle(LEGACY_DATA_COLUMN_NAME).is_some() {
        db.drop_cf(old_cf_name)
            .context("Error dropping old column family")?;
    }

    // Read in old key types and add the new
    let old_keys = db
        .prefix_iterator_cf(get_data_column_handle(&db), KEYGEN_DATA_PREFIX)
        .map(|(key_id, key_info)| {
            // Strip the prefix off the key_id
            let key_id: KeyId = KeyId(key_id[PREFIX_SIZE..].into());

            bincode::deserialize::<old_types::KeygenResultInfo>(&*key_info)
                .map(|keygen_result_info| {
                    println!(
                        "Successfully deceoding old keygen result info: {:?}",
                        keygen_result_info
                    );
                    (key_id, keygen_result_info)
                })
                .map_err(|e| anyhow::anyhow!(e))
        })
        .collect::<Result<HashMap<KeyId, old_types::KeygenResultInfo>>>()?;

    let mut p_kdb = PersistentKeyDB::new_from_db(db, &new_discard_logger());

    // only write if all the keys were successfully deserialized
    old_keys
        .into_iter()
        .map(|(key_id, old_keygen_result_info)| {
            (
                key_id,
                old_to_new_keygen_result_info(old_keygen_result_info),
            )
        })
        .for_each(|(key_id, keygen_result_info)| {
            p_kdb.update_key(&key_id, &keygen_result_info);
        });

    Ok(p_kdb)
}
