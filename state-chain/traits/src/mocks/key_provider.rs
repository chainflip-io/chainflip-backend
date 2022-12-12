use std::marker::PhantomData;

use crate::EpochKey;

#[derive(Default)]
pub struct MockKeyProvider<Chain: cf_chains::Chain>(PhantomData<Chain>);

impl<C: cf_chains::ChainCrypto> crate::KeyProvider<C> for MockKeyProvider<C> {
	fn current_epoch_key() -> EpochKey<C::AggKey> {
		EpochKey::default()
	}
}
