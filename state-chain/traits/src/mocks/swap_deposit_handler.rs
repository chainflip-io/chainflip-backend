use cf_chains::Chain;
use cf_primitives::SwapId;

use crate::{EgressApi, SwapDepositHandler};

/// Simple mock that applies 1:1 swap ratio to all pairs.
pub struct MockSwapDepositHandler<T>(sp_std::marker::PhantomData<T>);

impl<C: Chain, E: EgressApi<C>> SwapDepositHandler for MockSwapDepositHandler<(C, E)>
where
	cf_primitives::Asset: TryInto<C::ChainAsset>,
{
	type AccountId = u64;

	fn schedule_swap_from_channel(
		_deposit_address: cf_chains::ForeignChainAddress,
		_deposit_block_height: u64,
		_from: cf_primitives::Asset,
		to: cf_primitives::Asset,
		amount: cf_primitives::AssetAmount,
		destination_address: cf_chains::ForeignChainAddress,
		_broker_id: Self::AccountId,
		_broker_commission_bps: cf_primitives::BasisPoints,
		_channel_id: cf_primitives::ChannelId,
	) -> SwapId {
		let _ = E::schedule_egress(
			to.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
			amount.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
			destination_address.try_into().unwrap_or_else(|_| panic!("Unable to convert")),
			None,
		);
		1
	}
}
