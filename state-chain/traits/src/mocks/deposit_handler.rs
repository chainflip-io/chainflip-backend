use crate::{Chainflip, DepositApi};
use cf_chains::{
	address::ForeignChainAddress, dot::PolkadotAccountId, CcmChannelMetadata, Chain, ForeignChain,
};
use cf_primitives::{chains::assets::any, BasisPoints, ChannelId};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

pub struct MockDepositHandler<C, T>(PhantomData<(C, T)>);

impl<C, T> MockPallet for MockDepositHandler<C, T> {
	const PREFIX: &'static [u8] = b"MockDepositHandler";
}

enum SwapOrLp {
	Swap,
	Lp,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct SwapChannel<C: Chain, T: Chainflip> {
	pub deposit_address: ForeignChainAddress,
	pub source_asset: <C as Chain>::ChainAsset,
	pub destination_asset: any::Asset,
	pub destination_address: ForeignChainAddress,
	pub broker_commission_bps: BasisPoints,
	pub broker_id: <T as frame_system::Config>::AccountId,
	pub channel_metadata: Option<CcmChannelMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LpChannel<C: Chain, T: Chainflip> {
	pub deposit_address: ForeignChainAddress,
	pub source_asset: <C as Chain>::ChainAsset,
	pub lp_account: <T as frame_system::Config>::AccountId,
}

impl<C: Chain, T: Chainflip> MockDepositHandler<C, T> {
	fn get_new_deposit_address(
		swap_or_lp: SwapOrLp,
		asset: <C as Chain>::ChainAsset,
	) -> (ChannelId, ForeignChainAddress) {
		let channel_id = <Self as MockPalletStorage>::mutate_value(
			match swap_or_lp {
				SwapOrLp::Swap => b"SWAP_INTENT_ID",
				SwapOrLp::Lp => b"LP_INTENT_ID",
			},
			|storage| {
				let channel_id: ChannelId = storage.unwrap_or_default();
				let _ = storage.insert(channel_id + 1);
				channel_id
			},
		);
		(
			channel_id,
			match asset.into() {
				ForeignChain::Ethereum => ForeignChainAddress::Eth([channel_id as u8; 20].into()),
				ForeignChain::Polkadot => ForeignChainAddress::Dot(
					PolkadotAccountId::from_aliased([channel_id as u8; 32]),
				),
				ForeignChain::Bitcoin => todo!("Bitcoin address"),
			},
		)
	}

	pub fn get_liquidity_channels() -> Vec<LpChannel<C, T>> {
		<Self as MockPalletStorage>::get_value(b"LP_INGRESS_CHANNELS").unwrap_or_default()
	}

	pub fn get_swap_channels() -> Vec<SwapChannel<C, T>> {
		<Self as MockPalletStorage>::get_value(b"SWAP_INGRESS_CHANNELS").unwrap_or_default()
	}
}

impl<C: Chain, T: Chainflip> DepositApi<C> for MockDepositHandler<C, T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	fn request_liquidity_deposit_address(
		lp_account: Self::AccountId,
		source_asset: <C as cf_chains::Chain>::ChainAsset,
	) -> Result<(cf_primitives::ChannelId, ForeignChainAddress), sp_runtime::DispatchError> {
		let (channel_id, deposit_address) =
			Self::get_new_deposit_address(SwapOrLp::Lp, source_asset);
		<Self as MockPalletStorage>::mutate_value(b"LP_INGRESS_CHANNELS", |lp_channels| {
			if lp_channels.is_none() {
				*lp_channels = Some(vec![]);
			}
			if let Some(inner) = lp_channels.as_mut() {
				inner.push(LpChannel::<C, T> {
					deposit_address: deposit_address.clone(),
					source_asset,
					lp_account,
				});
			}
		});
		Ok((channel_id, deposit_address))
	}

	fn request_swap_deposit_address(
		source_asset: <C as Chain>::ChainAsset,
		destination_asset: cf_primitives::Asset,
		destination_address: ForeignChainAddress,
		broker_commission_bps: BasisPoints,
		broker_id: Self::AccountId,
		channel_metadata: Option<CcmChannelMetadata>,
	) -> Result<(cf_primitives::ChannelId, ForeignChainAddress), sp_runtime::DispatchError> {
		let (channel_id, deposit_address) =
			Self::get_new_deposit_address(SwapOrLp::Swap, source_asset);
		<Self as MockPalletStorage>::mutate_value(b"SWAP_INGRESS_CHANNELS", |swap_channels| {
			if swap_channels.is_none() {
				*swap_channels = Some(vec![]);
			}
			if let Some(inner) = swap_channels.as_mut() {
				inner.push(SwapChannel::<C, T> {
					deposit_address: deposit_address.clone(),
					source_asset,
					destination_asset,
					destination_address,
					broker_commission_bps,
					broker_id,
					channel_metadata,
				});
			};
		});
		Ok((channel_id, deposit_address))
	}

	fn expire_channel(address: <C as cf_chains::Chain>::ChainAccount) {
		<Self as MockPalletStorage>::mutate_value(
			b"SWAP_INGRESS_CHANNELS",
			|storage: &mut Option<Vec<SwapChannel<C, T>>>| {
				if let Some(inner) = storage.as_mut() {
					inner.retain(|x| x.deposit_address != address.clone().into());
				}
			},
		);
		<Self as MockPalletStorage>::mutate_value(
			b"LP_INGRESS_CHANNELS",
			|storage: &mut Option<Vec<LpChannel<C, T>>>| {
				if let Some(inner) = storage.as_mut() {
					inner.retain(|x| x.deposit_address != address.clone().into());
				}
			},
		);
	}
}
