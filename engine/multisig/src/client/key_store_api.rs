use cf_primitives::KeyId;

use crate::crypto::CryptoScheme;

use super::KeygenResultInfo;

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait KeyStoreAPI<C: CryptoScheme>: Send + Sync {
	/// Get the key for the given key id
	fn get_key(&self, key_id: &KeyId) -> Option<KeygenResultInfo<C>>;

	/// Save or update the key data and write it to persistent memory
	fn set_key(&mut self, key_id: KeyId, key: KeygenResultInfo<C>);
}
