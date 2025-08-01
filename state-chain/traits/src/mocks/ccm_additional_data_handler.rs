use super::MockPallet;
use crate::{mocks::MockPalletStorage, CcmAdditionalDataHandler};
use cf_chains::ccm_checker::DecodedCcmAdditionalData;
use sp_std::vec::Vec;

pub struct MockCcmAdditionalDataHandler;

impl MockPallet for MockCcmAdditionalDataHandler {
	const PREFIX: &'static [u8] = b"MockCcmAdditionalDataHandler";
}

const CCM_ADDITIONAL_DATA_HANDLER: &[u8] = b"CCM_ADDITIONAL_DATA_HANDLER";

impl MockCcmAdditionalDataHandler {
	pub fn get_data_handled() -> Vec<DecodedCcmAdditionalData> {
		Self::get_value(CCM_ADDITIONAL_DATA_HANDLER).unwrap_or_default()
	}
}

impl CcmAdditionalDataHandler for MockCcmAdditionalDataHandler {
	fn handle_ccm_additional_data(new: DecodedCcmAdditionalData) {
		Self::mutate_value::<Vec<DecodedCcmAdditionalData>, _, _>(
			CCM_ADDITIONAL_DATA_HANDLER,
			|maybe_ccm_data| {
				let ccm_data = maybe_ccm_data.get_or_insert_default();
				ccm_data.push(new);
			},
		);
	}
}
