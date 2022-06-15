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
		/// Returns the Auction params in the form [min_set_size, max_set_size]
		fn cf_auction_parameters() -> (u32, u32);
		fn cf_min_stake() -> u64;
		fn cf_current_epoch() -> u32;
		fn cf_current_epoch_started_at() -> u32;
		fn cf_authority_emission_per_block() -> u64;
		fn cf_backup_emission_per_block() -> u64;
		/// Returns the flip supply in the form [total_issuance, offchain_funds]
		fn cf_flip_supply() -> (u64, u64);
	}
);
