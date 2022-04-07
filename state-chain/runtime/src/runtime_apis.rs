use sp_api::decl_runtime_apis;

decl_runtime_apis!(
	pub trait MeaningOfLiveRuntimeApi {
		fn ask() -> u32;
	}
);
