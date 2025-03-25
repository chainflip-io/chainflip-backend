use cf_chains::ccm_checker::DecodedCcmAdditionalData;

use crate::InitiateSolanaAltWitnessing;

pub struct MockAltWitnessing;
impl InitiateSolanaAltWitnessing for MockAltWitnessing {
	fn initiate_alt_witnessing(
		_ccm_channel_metadata: DecodedCcmAdditionalData,
		_swap_request_id: cf_primitives::SwapRequestId,
	) {
	}
}
