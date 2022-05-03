use rocksdb::{WriteBatch, DB};

use crate::multisig::db::persistent::{add_schema_version_to_batch_write, LEGACY_DATA_COLUMN_NAME};

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

    Ok(())
}
