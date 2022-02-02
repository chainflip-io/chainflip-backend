//! TODO: This is temp migration code. Remove this after validators on Souncheck have upgraded their CFEs
use std::collections::HashMap;

use std::ffi::OsString;
use std::sync::Arc;

use crate::multisig::client::{KeygenResultInfo, ThresholdParameters};
use crate::multisig::{client::utils::PartyIdxMapping, KeyId};
use crate::multisig::{KeyDB, PersistentKeyDB};

use crate::multisig::client::signing::KeygenResult;
use fs_extra::dir::CopyOptions;
use kvdb_rocksdb::Database;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
struct OldAccountId([u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OldPartyIdxMapping {
    id_to_idx: HashMap<OldAccountId, usize>,
    // TODO: create SortedVec and use it here:
    // Sorted Account Ids
    account_ids: Vec<OldAccountId>,
}

impl From<OldPartyIdxMapping> for PartyIdxMapping {
    fn from(old_party_idx_mapping: OldPartyIdxMapping) -> Self {
        let id_to_idx = old_party_idx_mapping
            .id_to_idx
            .into_iter()
            .map(|(old_account_id, id)| (AccountId::new(old_account_id.0), id))
            .collect();

        let account_ids = old_party_idx_mapping
            .account_ids
            .into_iter()
            .map(|old_id| AccountId::new(old_id.0))
            .collect();

        PartyIdxMapping {
            id_to_idx,
            account_ids,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OldKeygenResultInfo {
    pub key: Arc<KeygenResult>,
    pub validator_map: Arc<OldPartyIdxMapping>,
    pub params: ThresholdParameters,
}

impl From<OldKeygenResultInfo> for KeygenResultInfo {
    fn from(old_keygen_result_info: OldKeygenResultInfo) -> Self {
        let old_party_idx_mapping = (*old_keygen_result_info.validator_map).clone();
        let validator_map: PartyIdxMapping = old_party_idx_mapping.into();
        let validator_map = Arc::new(validator_map);
        Self {
            key: old_keygen_result_info.key,
            validator_map,
            params: old_keygen_result_info.params,
        }
    }
}

use anyhow::Result;
use state_chain_runtime::AccountId;

use super::persistent::DB_COL_KEYGEN_RESULT_INFO;

fn deserialise_db_keygen_result_info<T: DeserializeOwned>(
    db: &Database,
) -> Result<HashMap<KeyId, T>> {
    db.iter(DB_COL_KEYGEN_RESULT_INFO)
        .map(|(key_id, key_info)| {
            let keygen_info = bincode::deserialize::<T>(&*key_info).map_err(anyhow::Error::msg);
            match keygen_info {
                Ok(info) => Ok((KeyId(key_id.into()), info)),
                Err(e) => Err(anyhow::Error::msg(e)),
            }
        })
        .collect::<Result<HashMap<KeyId, T>>>()
}

impl PersistentKeyDB {
    pub fn migrate_db_to_new_account_id(&mut self) {
        let logger = &self.logger.clone();
        let db_path = &self.path;

        let can_serialise_new =
            deserialise_db_keygen_result_info::<KeygenResultInfo>(&self.db).is_ok();

        // We don't need to do the migration, since we can already deserialise the old types
        if can_serialise_new {
            slog::info!(
                logger,
                "We can deserialise the new account id format. Skipping migration."
            );
            return;
        }

        assert!(
            db_path.exists(),
            "DB does not exist at: {}",
            db_path.display()
        );
        let db_name = db_path
            .file_name()
            .expect("db should have a name")
            .to_os_string();
        // prefix the backup
        let mut backup_name = OsString::from("act_id_bkp_");
        backup_name.push(db_name);

        let backup_path = db_path
            .parent()
            .expect("Should have parent")
            .join(backup_name);

        slog::info!(
            logger,
            "Backuping up from: {:?} to {:?}",
            db_path.as_os_str(),
            backup_path.as_os_str()
        );

        // create the backup
        let mut copy_options = CopyOptions::new();
        copy_options.copy_inside = true;
        fs_extra::dir::copy(db_path, backup_path, &copy_options)
            .expect("Backing up database failed");

        slog::info!(
            logger,
            "Key database backup taken successfully. Proceeding to migrate to new key serialisation"
        );

        let old_keys = deserialise_db_keygen_result_info::<OldKeygenResultInfo>(&self.db)
            .expect("Should be able to load old keys");

        let new_keys: HashMap<KeyId, KeygenResultInfo> = old_keys
            .into_iter()
            .map(|(key_id, old_keygen_result_info)| (key_id, old_keygen_result_info.into()))
            .collect();

        for (key_id, keygen_result_info) in new_keys {
            self.update_key(&key_id, &keygen_result_info);
        }

        slog::info!(logger, "Key database migration completed successfully.");
    }
}
