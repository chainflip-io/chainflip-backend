use crate::SolanaAltWitnessingHandler;

pub struct MockCcmAuxWitnessingHandler;
impl SolanaAltWitnessingHandler for MockCcmAuxWitnessingHandler {
	fn initiate_alt_witnessing(
		_ccm_channel_metadata: cf_chains::CcmChannelMetadata,
		_swap_request_id: cf_primitives::SwapRequestId,
	) {
	}
	fn should_expire(_created_at: u32, _current: cf_primitives::BlockNumber) -> bool {
		false
	}
}
