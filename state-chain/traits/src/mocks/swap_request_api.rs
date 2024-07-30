use crate::{swapping::SwapRequestType, EgressApi, SwapRequestHandler};
use cf_chains::{Chain, ChannelRefundParameters, SwapOrigin};
use cf_primitives::{Asset, AssetAmount, Beneficiaries, DCAParameters, SwapRequestId};
use codec::{Decode, Encode};
use frame_support::sp_runtime::DispatchError;
use scale_info::TypeInfo;

use crate::mocks::MockPalletStorage;

use super::MockPallet;

/// Simple mock that applies 1:1 swap ratio to all pairs.
pub struct MockSwapRequestHandler<T>(sp_std::marker::PhantomData<T>);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct MockSwapRequest {
	pub input_asset: Asset,
	pub output_asset: Asset,
	pub input_amount: AssetAmount,
	pub swap_type: SwapRequestType,
}

impl<T> MockPallet for MockSwapRequestHandler<T> {
	const PREFIX: &'static [u8] = b"MockSwapRequestHandler";
}

const SWAP_REQUESTS: &[u8] = b"SWAP_REQUESTS";

impl<T> MockSwapRequestHandler<T> {
	pub fn get_swap_requests() -> Vec<MockSwapRequest> {
		Self::get_value(SWAP_REQUESTS).unwrap_or_default()
	}
}

impl<C: Chain, E: EgressApi<C>> SwapRequestHandler for MockSwapRequestHandler<(C, E)>
where
	Asset: TryInto<C::ChainAsset>,
{
	type AccountId = u64;

	fn init_swap_request(
		input_asset: Asset,
		input_amount: AssetAmount,
		output_asset: Asset,
		swap_type: SwapRequestType,
		_broker_fees: Beneficiaries<Self::AccountId>,
		_refund_params: Option<ChannelRefundParameters>,
		_dca_params: Option<DCAParameters>,
		_origin: SwapOrigin,
	) -> Result<SwapRequestId, DispatchError> {
		Self::mutate_value(SWAP_REQUESTS, |swaps: &mut Option<Vec<MockSwapRequest>>| {
			swaps.get_or_insert(vec![]).push(MockSwapRequest {
				input_asset,
				output_asset,
				input_amount,
				swap_type: swap_type.clone(),
			});
		});

		match swap_type {
			SwapRequestType::Regular { output_address } |
			SwapRequestType::Ccm { output_address, .. } => {
				let _ = E::schedule_egress(
					output_asset.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
					input_amount.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
					output_address.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
					None,
				);
			},
			_ => { /* do nothing */ },
		};

		Ok(1)
	}
}
