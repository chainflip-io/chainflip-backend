use std::{collections::HashMap, sync::Arc};

use rocksdb::{WriteBatch, DB};

use crate::multisig::{
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

    use crate::multisig::crypto::{KeyShare, Point};

    #[derive(Deserialize, Debug)]
    pub struct ThresholdParameters {
        pub threshold: usize,
        pub share_count: usize,
    }

    #[derive(Deserialize, Debug)]
    pub struct KeygenResult {
        pub key_share: KeyShare,
        pub party_public_keys: Vec<Point>,
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

mod new_types {
    use std::{collections::HashMap, sync::Arc};

    use state_chain_runtime::AccountId;

    use crate::multisig::crypto::{KeyShare, Point};
    use serde::Serialize;

    #[derive(Serialize, Debug)]
    pub struct ThresholdParameters {
        pub threshold: u16,
        pub share_count: u16,
    }

    #[derive(Serialize, Debug)]
    pub struct KeygenResult {
        pub key_share: KeyShare,
        pub party_public_keys: Vec<Point>,
    }

    #[derive(Serialize, Debug)]
    pub struct PartyIdxMapping {
        pub id_to_idx: HashMap<AccountId, u16>,
        pub account_ids: Vec<AccountId>,
    }

    #[derive(Serialize, Debug)]
    pub struct KeygenResultInfo {
        pub key: Arc<KeygenResult>,
        pub validator_map: Arc<PartyIdxMapping>,
        pub params: ThresholdParameters,
    }
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
    let items: HashMap<KeyId, old_types::KeygenResultInfo> = db
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
    items
        .into_iter()
        .for_each(|(key_id, old_keygen_result_info)| {
            // convert to new type:
            // let old_key_share = mem::take(&mut old_keygen_result_info.key.key_share);
            // let old_party_public_keys = mem::take(&mut old_keygen_result_info.key.key_share);
            let old_key =
                std::sync::Arc::<old_types::KeygenResult>::try_unwrap(old_keygen_result_info.key)
                    .unwrap();

            let new_keygen_result_info = new_types::KeygenResultInfo {
                key: Arc::new(new_types::KeygenResult {
                    key_share: old_key.key_share,
                    party_public_keys: old_key.party_public_keys,
                }),
                validator_map: Arc::new(new_types::PartyIdxMapping {
                    id_to_idx: old_keygen_result_info
                        .validator_map
                        .id_to_idx
                        .clone()
                        .into_iter()
                        .map(|(key, value)| (key, value.try_into().expect("should fit into u16")))
                        .collect(),
                    account_ids: old_keygen_result_info.validator_map.account_ids.clone(),
                }),
                params: new_types::ThresholdParameters {
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
            };
            let new_keygen_result_info_bytes = bincode::serialize(&new_keygen_result_info).unwrap();

            update_key(db, &key_id, new_keygen_result_info_bytes).expect("Failed to update key");
        });

    Ok(())
}
