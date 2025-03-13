use crate::SolanaAltWitnessingHandler;

pub struct MockAltWitnessing;
impl SolanaAltWitnessingHandler for MockAltWitnessing {
	fn initiate_alt_witnessing(
		_ccm_channel_metadata: cf_chains::CcmChannelMetadata,
		_swap_request_id: cf_primitives::SwapRequestId,
	) {
	}
}
