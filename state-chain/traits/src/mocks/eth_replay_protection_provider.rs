use cf_chains::{eth::api::EthereumReplayProtection, ChainAbi, Ethereum, ReplayProtectionProvider};

pub struct MockEthReplayProtectionProvider;

pub const COUNTER: u64 = 42;

impl ReplayProtectionProvider<Ethereum> for MockEthReplayProtectionProvider {
	fn replay_protection() -> <Ethereum as ChainAbi>::ReplayProtection {
		EthereumReplayProtection { nonce: COUNTER }
	}
}
