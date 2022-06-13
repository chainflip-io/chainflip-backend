use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	/// Definition for all runtime API interfaces.
	pub trait CustomRuntimeApi {
		/// Returns true if the current phase is the auction phase.
		fn cf_is_auction_phase() -> bool;
		fn cf_eth_flip_token_address() -> [u8; 20];
		fn cf_eth_stake_manager_address() -> [u8; 20];
		fn cf_eth_key_manager_address() -> [u8; 20];
		fn cf_eth_chain_id() -> u64;
		fn cf_epoch_index() -> u32;
		fn cf_current_epoch_started_at() -> u32;
	}
);
