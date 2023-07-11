use super::KeygenResultInfo;
use crate::crypto::{ChainSigning, KeyId};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait KeyStoreAPI<C: ChainSigning>: Send + Sync {
	/// Get the key for the given key id
	fn get_key(&self, key_id: &KeyId) -> Option<KeygenResultInfo<C>>;

	/// Save or update the key data and write it to persistent memory
	fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C>);
}
