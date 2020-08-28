use crate::common::store::KeyValueStore;
use std::collections::HashMap;

/// A memory key value store
pub struct MemoryKVS {
    store: HashMap<String, String>,
}

impl MemoryKVS {
    /// Create a new memory kvs
    pub fn new() -> Self {
        MemoryKVS {
            store: HashMap::new(),
        }
    }
}

impl KeyValueStore for MemoryKVS {
    fn get_data<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        match self.store.get(key) {
            Some(val) => val.parse().ok(),
            None => None,
        }
    }

    fn set_data<T: ToString>(&mut self, key: &str, value: Option<T>) -> Result<(), String> {
        match value {
            Some(value) => {
                self.store.insert(key.to_owned(), value.to_string());
                Ok(())
            }
            None => {
                self.store.remove(key);
                Ok(())
            }
        }
    }
}
