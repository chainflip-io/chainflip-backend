use crate::NonceProvider;

use super::{MockPallet, MockPalletStorage};

pub struct MockNonceProvider {}

const NONCE: &[u8] = b"NONCE";

type AccountId = u64;
type Nonce = u32;

impl NonceProvider<AccountId, Nonce> for MockNonceProvider {
	fn get_nonce(_account: &AccountId) -> u32 {
		// Here we just make sure we provide a new value each time this funciton is called which
		// is good enough for current tests
		Self::mutate_value(NONCE, |value: &mut Option<u32>| {
			let nonce = value.get_or_insert_default();
			let current = *nonce;
			*nonce += 1;
			current
		})
	}
}

impl MockPallet for MockNonceProvider {
	const PREFIX: &'static [u8] = b"MockNonceProvider";
}
