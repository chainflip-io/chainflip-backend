use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	pub trait MeaningOfLiveRuntimeApi {
		fn ask() -> u32;
	}
	pub trait ValidatorRuntimeApi {
		fn is_auction_phase() -> bool;
	}
);
