use cf_chains::{Chain, ChainCrypto};
use cf_primitives::ThresholdSignatureRequestId;

use super::{MockPallet, MockPalletStorage};
use crate::{EpochKey, KeyState};
use std::marker::PhantomData;

#[derive(Default)]
pub struct MockKeyProvider<C: Chain>(PhantomData<C>);

impl<C: Chain> MockPallet for MockKeyProvider<C> {
	const PREFIX: &'static [u8] = b"MockKeyProvider::";
}

const EPOCH_KEY: &[u8] = b"EPOCH_KEY";

impl<C: ChainCrypto> MockKeyProvider<C> {
	pub fn add_key(key: C::AggKey) {
		Self::put_value(
			EPOCH_KEY,
			EpochKey { key, epoch_index: Default::default(), key_state: KeyState::Active },
		);
	}

	pub fn lock_key(request_id: ThresholdSignatureRequestId) {
		Self::mutate_value(EPOCH_KEY, |maybe_key| {
			let mut key: EpochKey<C::AggKey> = maybe_key.unwrap_or_default();
			key.lock_for_request(request_id);
			let _ = maybe_key.insert(key);
		});
	}
}

impl<C: ChainCrypto> crate::KeyProvider<C> for MockKeyProvider<C> {
	fn current_epoch_key() -> EpochKey<C::AggKey> {
		Self::get_value(EPOCH_KEY).unwrap_or_default()
	}
}
