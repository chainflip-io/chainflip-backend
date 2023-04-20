use crate::CcmHandler;
use cf_chains::{address::ForeignChainAddress, CcmIngressMetadata};
use frame_support::dispatch::DispatchResult;

use cf_primitives::{Asset, AssetAmount};
use codec::{Decode, Encode};
use scale_info::TypeInfo;

use super::{MockPallet, MockPalletStorage};

pub struct MockCcmHandler;
pub const CCM_HANDLER_PREFIX: &[u8] = b"MockCcmHandler";
impl MockPallet for MockCcmHandler {
	const PREFIX: &'static [u8] = CCM_HANDLER_PREFIX;
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct CcmRequest {
	pub ingress_asset: Asset,
	pub ingress_amount: AssetAmount,
	pub egress_asset: Asset,
	pub egress_address: ForeignChainAddress,
	pub message_metadata: CcmIngressMetadata,
}

impl MockCcmHandler {
	pub fn get_ccm_requests() -> Vec<CcmRequest> {
		<Self as MockPalletStorage>::get_value(CCM_HANDLER_PREFIX).unwrap_or_default()
	}
}

impl CcmHandler for MockCcmHandler {
	fn on_ccm_ingress(
		ingress_asset: Asset,
		ingress_amount: AssetAmount,
		egress_asset: Asset,
		egress_address: ForeignChainAddress,
		message_metadata: CcmIngressMetadata,
	) -> DispatchResult {
		<Self as MockPalletStorage>::mutate_value(CCM_HANDLER_PREFIX, |ccm_requests| {
			if ccm_requests.is_none() {
				*ccm_requests = Some(vec![]);
			}
			ccm_requests.as_mut().map(|v| {
				v.push(CcmRequest {
					ingress_asset,
					ingress_amount,
					egress_asset,
					egress_address,
					message_metadata,
				});
			})
		});
		Ok(())
	}
}
