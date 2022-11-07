use std::marker::PhantomData;

use cf_primitives::EpochIndex;

/// A Mock that just returns KeyId::default().
///
/// Note: the `current_key()` method is unimplemented. If required, implement a custom mock for this
/// trait.
#[derive(Default)]
pub struct MockKeyProvider<Chain: cf_chains::Chain, KeyId: std::default::Default>(
	PhantomData<(Chain, KeyId)>,
);

impl<C: cf_chains::ChainCrypto, K: std::default::Default> crate::KeyProvider<C>
	for MockKeyProvider<C, K>
{
	type KeyId = K;

	fn current_key_id() -> Self::KeyId {
		Default::default()
	}

	fn current_key() -> C::AggKey {
		unimplemented!("Implement a custom mock if `current_key()` is required.")
	}

	fn vault_keyholders_epoch() -> EpochIndex {
		unimplemented!("Implement a custom mock if `vault_keyholders_epoch` is required.")
	}
}
