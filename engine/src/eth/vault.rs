use web3::{ethabi, types::H160};

pub struct Vault {
	pub deployed_address: H160,
	contract: ethabi::Contract,
}

pub enum VaultEvent {
	CommunityGuardDisabled {
		community_guard_disabled: bool,
	},
	Suspended {
		suspended: bool,
	},
	UpdatedKeyManager {
		key_manager: ethabi::Address,
	},
	SwapNative {
		destination_chain: u32,
		destination_address: ethabi::Address,
		destination_token: u16,
		amount: u128,
		sender: ethabi::Address,
	},
	SwapToken {
		destination_chain: u32,
		destination_address: ethabi::Address,
		destination_token: u16,
		source_token: ethabi::Address,
		amount: u128,
		sender: ethabi::Address,
	},
	TransferNativeFailed {
		recipient: ethabi::Address,
		amount: u128,
	},
	TransferTokenFailed {
		recipient: ethabi::Address,
		amount: u128,
		token: ethabi::Address,
		reason: web3::types::Bytes,
	},
	XCallNative {
		destination_chain: u32,
		destination_address: ethabi::Address,
		destination_token: u16,
		amount: u128,
		sender: ethabi::Address,
		message: web3::types::Bytes,
		destination_native_budget: u128,
		refund_address: ethabi::Address,
	},
	XCallToken {
		destination_chain: u32,
		destination_address: ethabi::Address,
		destination_token: u16,
		source_token: ethabi::Address,
		amount: u128,
		sender: ethabi::Address,
		message: web3::types::Bytes,
		destination_native_budget: u128,
		refund_address: ethabi::Address,
	},
	AddGasNative {
		swap_id: [u8; 32],
		amount: u128,
	},
	AddGasToken {
		swap_id: [u8; 32],
		amount: u128,
		token: ethabi::Address,
	},
}
