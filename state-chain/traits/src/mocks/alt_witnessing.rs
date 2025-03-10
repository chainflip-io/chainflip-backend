use cf_primitives::DcaParameters;

use crate::InitiateSolanaAltWitnessing;

pub struct MockAltWitnessing;
impl InitiateSolanaAltWitnessing for MockAltWitnessing {
	fn initiate_alt_witnessing(
		_ccm_channel_metadata: cf_chains::CcmChannelMetadata,
		_swap_request_id: cf_primitives::SwapRequestId,
	) {
	}
}
