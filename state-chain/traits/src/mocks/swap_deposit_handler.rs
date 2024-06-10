use crate::{EgressApi, SwapDepositHandler};
use cf_chains::{Chain, ChannelRefundParameters, ForeignChainAddress};
use cf_primitives::{Asset, AssetAmount, Beneficiaries, ChannelId, SwapId};

/// Simple mock that applies 1:1 swap ratio to all pairs.
pub struct MockSwapDepositHandler<T>(sp_std::marker::PhantomData<T>);

impl<C: Chain, E: EgressApi<C>> SwapDepositHandler for MockSwapDepositHandler<(C, E)>
where
	Asset: TryInto<C::ChainAsset>,
{
	type AccountId = u64;

	fn schedule_swap_from_channel(
		_deposit_address: ForeignChainAddress,
		_deposit_block_height: u64,
		_from: Asset,
		to: Asset,
		amount: AssetAmount,
		destination_address: ForeignChainAddress,
		_broker_commission: Beneficiaries<Self::AccountId>,
		_refund_params: Option<ChannelRefundParameters>,
		_channel_id: ChannelId,
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
