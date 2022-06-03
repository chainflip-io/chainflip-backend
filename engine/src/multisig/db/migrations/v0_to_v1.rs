use std::{collections::HashMap, sync::Arc};

use rocksdb::{WriteBatch, DB};

use anyhow::{Context, Result};

use crate::{
    logging::utils::new_discard_logger,
    multisig::{
        client::{KeygenResultInfo, PartyIdxMapping, ThresholdParameters},
        crypto::ECPoint,
        db::persistent::{
            add_schema_version_to_batch_write, get_data_column_handle, KEYGEN_DATA_PREFIX,
            PREFIX_SIZE,
        },
        KeyDB, KeyId, PersistentKeyDB,
    },
};

mod old_types {
    use std::{collections::HashMap, sync::Arc};

    use frame_support::Deserialize;
    use state_chain_runtime::AccountId;

    use crate::multisig::{client::KeygenResult, crypto::ECPoint};

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
    pub struct KeygenResultInfo<P: ECPoint> {
        #[serde(bound = "")]
        pub key: Arc<KeygenResult<P>>,
        pub validator_map: Arc<PartyIdxMapping>,
        pub params: ThresholdParameters,
    }
}

fn old_to_new_keygen_result_info<P: ECPoint>(
    old_keygen_result_info: old_types::KeygenResultInfo<P>,
) -> KeygenResultInfo<P> {
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

// Adding schema version to the metadata column and updating the KeygenResultInfo
pub fn migration_0_to_1<P: ECPoint>(db: DB) -> Result<PersistentKeyDB<P>, anyhow::Error> {
    // Update version data
    let mut batch = WriteBatch::default();
    add_schema_version_to_batch_write(&db, 1, &mut batch);

    // Write the batch
    db.write(batch)
        .context("Failed to write to db during migration")?;

    // Read in old key types and add the new
    let old_keys = db
        .prefix_iterator_cf(get_data_column_handle(&db), KEYGEN_DATA_PREFIX)
        .map(|(key_id, key_info)| {
            // Strip the prefix off the key_id
            let key_id: KeyId = KeyId(key_id[PREFIX_SIZE..].into());

            bincode::deserialize::<old_types::KeygenResultInfo<P>>(&*key_info)
                .map(|keygen_result_info| {
                    println!(
                        "Successfully decoded old keygen result info: {:?}",
                        keygen_result_info
                    );
                    (key_id, keygen_result_info)
                })
                .map_err(|e| anyhow::anyhow!(e))
        })
        .collect::<Result<HashMap<KeyId, old_types::KeygenResultInfo<P>>>>()?;

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
