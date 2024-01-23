use cf_chains::ChainCrypto;

use super::{MockPallet, MockPalletStorage};
use crate::EpochKey;
use std::marker::PhantomData;

#[derive(Default)]
pub struct MockKeyProvider<C: ChainCrypto>(PhantomData<C>);

impl<C: ChainCrypto> MockPallet for MockKeyProvider<C> {
	const PREFIX: &'static [u8] = b"MockKeyProvider::";
}

const EPOCH_KEY: &[u8] = b"EPOCH_KEY";

impl<C: ChainCrypto> MockKeyProvider<C> {
	pub fn add_key(key: C::AggKey) {
		Self::put_value(EPOCH_KEY, EpochKey { key, epoch_index: Default::default() });
	}
}

impl<C: ChainCrypto> crate::KeyProvider<C> for MockKeyProvider<C> {
	fn active_epoch_key() -> Option<EpochKey<C::AggKey>> {
		Self::get_value(EPOCH_KEY)
	}
}
