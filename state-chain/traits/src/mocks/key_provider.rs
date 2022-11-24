use std::marker::PhantomData;

use crate::KeyState;

#[derive(Default)]
pub struct MockKeyProvider<Chain: cf_chains::Chain>(PhantomData<Chain>);

impl<C: cf_chains::ChainCrypto> crate::KeyProvider<C> for MockKeyProvider<C> {
	fn current_key_epoch_index() -> KeyState<C::AggKey> {
		KeyState::default()
	}
}
