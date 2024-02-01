use crate::{CcmHandler, CcmSwapIds};
use cf_chains::{address::ForeignChainAddress, CcmDepositMetadata, SwapOrigin};

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
	pub source_asset: Asset,
	pub deposit_amount: AssetAmount,
	pub destination_asset: Asset,
	pub destination_address: ForeignChainAddress,
	pub deposit_metadata: CcmDepositMetadata,
	pub origin: SwapOrigin,
}

impl MockCcmHandler {
	pub fn get_ccm_requests() -> Vec<CcmRequest> {
		<Self as MockPalletStorage>::get_value(CCM_HANDLER_PREFIX).unwrap_or_default()
	}
}

impl CcmHandler for MockCcmHandler {
	fn on_ccm_deposit(
		source_asset: Asset,
		deposit_amount: AssetAmount,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		deposit_metadata: CcmDepositMetadata,
		origin: SwapOrigin,
	) -> Result<CcmSwapIds, ()> {
		<Self as MockPalletStorage>::mutate_value(CCM_HANDLER_PREFIX, |ccm_requests| {
			if ccm_requests.is_none() {
				*ccm_requests = Some(vec![]);
			}
			ccm_requests.as_mut().map(|v| {
				v.push(CcmRequest {
					source_asset,
					deposit_amount,
					destination_asset,
					destination_address,
					deposit_metadata,
					origin,
				});
			})
		});

		// TODO: Return real ids
		Ok(CcmSwapIds { principal_swap_id: Some(1), gas_swap_id: Some(2) })
	}
}
