use cf_chains::{eth::api::EthereumReplayProtection, ChainAbi, Ethereum, ReplayProtectionProvider};

pub struct MockEthReplayProtectionProvider;

pub const FAKE_KEYMAN_ADDR: [u8; 20] = [0xcf; 20];
pub const CHAIN_ID: u64 = 31337;
pub const COUNTER: u64 = 42;

impl ReplayProtectionProvider<Ethereum> for MockEthReplayProtectionProvider {
	fn replay_protection() -> <Ethereum as ChainAbi>::ReplayProtection {
		EthereumReplayProtection {
			key_manager_address: FAKE_KEYMAN_ADDR,
			chain_id: CHAIN_ID,
			nonce: COUNTER,
		}
	}
}
