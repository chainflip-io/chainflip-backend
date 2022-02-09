use chainflip_engine::multisig::{
    db::persistent::{
        update_key, DATA_COLUMN, DB_SCHEMA_VERSION, DB_SCHEMA_VERSION_KEY, METADATA_COLUMN,
    },
    KeyId,
};
use rocksdb::{Options, DB};
use std::env;

const COLUMN_FAMILIES: &[&str] = &[DATA_COLUMN, METADATA_COLUMN];

fn main() {
    let current_path = env::current_dir().expect("Could not get current path");
    println!("Current path is: {}", current_path.display());
    let agg_pubkey_hex = env::var("AGG_PUBKEY").expect("AGG_PUBKEY environment variable not set");
    let agg_pubkey_bytes = hex::decode(agg_pubkey_hex).unwrap();

    let secret_share_hex = env::var("SIGNING_SECRET_SHARE")
        .expect("SIGNING_SECRET_SHARE environment variable not set");

    // Secret should be inserted as binary
    let secret_share_bytes = hex::decode(secret_share_hex).expect("Secret is not valid hex");

    let signing_db_path =
        env::var("SIGNING_DB_PATH").expect("SIGNING_DB_PATH environment variable not set");

    // Open the db
    let mut opts = Options::default();
    opts.create_missing_column_families(true);
    opts.create_if_missing(true);
    let db = DB::open_cf(&opts, &signing_db_path, COLUMN_FAMILIES).expect("Should open db file");

    // Write the schema version
    db.put_cf(
        db.cf_handle(METADATA_COLUMN)
            .unwrap_or_else(|| panic!("Should get column family handle for {}", METADATA_COLUMN)),
        DB_SCHEMA_VERSION_KEY,
        DB_SCHEMA_VERSION.to_be_bytes(),
    )
    .expect("Should write DB_SCHEMA_VERSION");

    // Write the key share to the db
    update_key(&db, &KeyId(agg_pubkey_bytes), secret_share_bytes)
        .expect("Should write key share to db");
}
