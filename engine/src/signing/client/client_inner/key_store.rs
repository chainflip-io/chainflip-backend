use std::collections::HashMap;

use crate::signing::{db::KeyDB, KeyId};

use super::common::KeygenResultInfo;

// Successfully generated multisig keys live here
#[derive(Clone)]
pub struct KeyStore<S>
where
    S: KeyDB,
{
    keys: HashMap<KeyId, KeygenResultInfo>,
    db: S,
}

impl<S> KeyStore<S>
where
    S: KeyDB,
{
    pub fn new(db: S) -> Self {
        let keys = db.load_keys();
        println!("Keys loaded in from db: {:?}", keys);

        KeyStore { keys, db }
    }

    #[cfg(test)]
    pub fn get_db(&self) -> &S {
        &self.db
    }

    pub fn get_key(&self, key_id: KeyId) -> Option<&KeygenResultInfo> {
        self.keys.get(&key_id)
    }

    // Save `key` under key `key_id` overwriting if exists
    pub fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo) {
        self.db.update_key(key_id.clone(), &key);
        self.keys.insert(key_id, key);
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    use crate::{
        logging::test_utils::create_test_logger, settings::test_utils::new_test_settings,
        signing::db::PersistentKeyDB,
    };

    #[test]
    #[ignore = "manual test"]
    fn startup_keystore() {
        let current_path = env::current_dir().unwrap();
        println!("The current directory is: {}", current_path.display());
        let settings = new_test_settings().unwrap();
        let path_to_db = settings.signing.db_file.as_path();
        if !path_to_db.exists() {
            panic!("Db path does not exist")
        } else {
            println!("Path does exist, carry on");
        }
        println!("here's the path to the db: {}", path_to_db.display());
        let logger = create_test_logger();
        let p_db = PersistentKeyDB::new(&path_to_db, &logger);
        let db = p_db.db;

        let mut tx = db.transaction();
        tx.put_vec(0, &[0; 2], vec![1; 3]);
        db.write(tx).unwrap();

        for item in db.iter(0) {
            println!("Here's an item");
        }
    }
}
