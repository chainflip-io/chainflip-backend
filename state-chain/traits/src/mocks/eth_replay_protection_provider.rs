use std::marker::PhantomData;

use cf_chains::{eth::api::EthereumReplayProtection, ChainAbi};

/// A mock that just returns some constants for the EthereumReplayProtection.
pub struct MockEthReplayProtectionProvider<T>(PhantomData<T>);

impl<T: ChainAbi<ReplayProtection = EthereumReplayProtection>> crate::ReplayProtectionProvider<T>
	for MockEthReplayProtectionProvider<T>
{
	fn replay_protection() -> <T as ChainAbi>::ReplayProtection {
		EthereumReplayProtection { key_manager_address: [0xcf; 20], chain_id: 31337, nonce: 42 }
	}
}

impl<T: ChainAbi<ApiCallExtraData = ()>> crate::ApiCallDataProvider<T>
	for MockEthReplayProtectionProvider<T>
{
	fn chain_extra_data() -> <T as ChainAbi>::ApiCallExtraData {}
}
