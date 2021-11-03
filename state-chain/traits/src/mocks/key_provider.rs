use std::marker::PhantomData;
use crate::mocks::epoch_info::MockEpochInfo;

/// A Mock that just returns KeyId::default().
#[derive(Default)]
pub struct MockKeyProvider<Chain: cf_chains::Chain, KeyId: std::default::Default>(
	PhantomData<(Chain, KeyId)>,
);

impl<C: cf_chains::Chain, K: std::default::Default> crate::KeyProvider<C>
	for MockKeyProvider<C, K>
{
	type KeyId = K;
	type EpochInfo = MockEpochInfo;

	fn current_key() -> Self::KeyId {
		Default::default()
	}
}
