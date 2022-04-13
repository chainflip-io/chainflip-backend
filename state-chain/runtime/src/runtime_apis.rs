use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	/// Definition for all runtime API interfaces.
	pub trait CustomRuntimeApi {
		/// Returns true if the current phase is the auction phase.
		fn is_auction_phase() -> bool;
	}
);
