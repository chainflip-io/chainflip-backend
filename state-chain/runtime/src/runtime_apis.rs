use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	/// Definition for all runtime API interfaces.
	pub trait CustomRuntimeApi {
		/// Returns true if the current phase is the auction phase.
		fn is_auction_phase() -> bool;
		fn eth_flip_token_address() -> [u8; 20];
		fn eth_stake_manager_address() -> [u8; 20];
		fn eth_key_manager_address() -> [u8; 20];
		fn eth_chain_id() -> u64;
	}
);
