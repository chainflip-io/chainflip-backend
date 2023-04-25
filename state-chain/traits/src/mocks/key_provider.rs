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
			EpochKey { key, epoch_index: Default::default(), key_state: KeyState::Unlocked },
		);
	}

	pub fn lock_key(request_id: ThresholdSignatureRequestId) {
		Self::mutate_value::<EpochKey<C::AggKey>, _, _>(EPOCH_KEY, |maybe_key| {
			if let Some(key) = maybe_key.as_mut() {
				key.lock_for_request(request_id);
			}
		});
	}
}

impl<C: ChainCrypto> crate::KeyProvider<C> for MockKeyProvider<C> {
	fn current_epoch_key() -> Option<EpochKey<C::AggKey>> {
		Self::get_value(EPOCH_KEY)
	}
}
