use chainflip_engine::multisig::{
    client::KeygenResultInfo,
    db::persistent::{DATA_COLUMN, METADATA_COLUMN},
    eth, KeyDB, KeyId, PersistentKeyDB,
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

    let secret_share_bytes = hex::decode(secret_share_hex).expect("Secret is not valid hex");

    let keygen_result_info =
        bincode::deserialize::<KeygenResultInfo<eth::Point>>(&*secret_share_bytes)
            .expect("Could not deserialize KeygenResultInfo");

    let signing_db_path =
        env::var("SIGNING_DB_PATH").expect("SIGNING_DB_PATH environment variable not set");

    // Open the db
    let mut opts = Options::default();
    opts.create_missing_column_families(true);
    opts.create_if_missing(true);
    let db = DB::open_cf(&opts, &signing_db_path, COLUMN_FAMILIES).expect("Should open db file");

    let mut p_kdb = PersistentKeyDB::new_from_db_and_set_schema_version_to_latest(
        db,
        &chainflip_engine::logging::utils::new_discard_logger(),
    )
    .unwrap();

    // Write the key share to the db
    p_kdb.update_key(&KeyId(agg_pubkey_bytes), &keygen_result_info);
}
