use crate::{Chainflip, IngressApi};
use cf_chains::{
	address::ForeignChainAddress, eth::assets::any, CcmIngressMetadata, Chain, ForeignChain,
};
use cf_primitives::{BasisPoints, ChannelId};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::marker::PhantomData;

use super::{MockPallet, MockPalletStorage};

pub struct MockIngressHandler<C, T>(PhantomData<(C, T)>);

impl<C, T> MockPallet for MockIngressHandler<C, T> {
	const PREFIX: &'static [u8] = b"MockIngressHandler";
}

enum SwapOrLp {
	Swap,
	Lp,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct SwapIntent<C: Chain, T: Chainflip> {
	pub deposit_address: ForeignChainAddress,
	pub source_asset: <C as Chain>::ChainAsset,
	pub destination_asset: any::Asset,
	pub egress_address: ForeignChainAddress,
	pub relayer_commission_bps: BasisPoints,
	pub relayer_id: <T as frame_system::Config>::AccountId,
	pub message_metadata: Option<CcmIngressMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LpIntent<C: Chain, T: Chainflip> {
	pub deposit_address: ForeignChainAddress,
	pub source_asset: <C as Chain>::ChainAsset,
	pub lp_account: <T as frame_system::Config>::AccountId,
}

impl<C: Chain, T: Chainflip> MockIngressHandler<C, T> {
	fn get_new_intent(
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
				ForeignChain::Ethereum => ForeignChainAddress::Eth([channel_id as u8; 20]),
				ForeignChain::Polkadot => ForeignChainAddress::Dot([channel_id as u8; 32]),
				ForeignChain::Bitcoin => todo!("Bitcoin address"),
			},
		)
	}

	pub fn get_liquidity_intents() -> Vec<LpIntent<C, T>> {
		<Self as MockPalletStorage>::get_value(b"LP_INGRESS_INTENTS").unwrap_or_default()
	}

	pub fn get_swap_intents() -> Vec<SwapIntent<C, T>> {
		<Self as MockPalletStorage>::get_value(b"SWAP_INGRESS_INTENTS").unwrap_or_default()
	}
}

impl<C: Chain, T: Chainflip> IngressApi<C> for MockIngressHandler<C, T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	fn request_liquidity_deposit_address(
		lp_account: Self::AccountId,
		source_asset: <C as cf_chains::Chain>::ChainAsset,
	) -> Result<(cf_primitives::ChannelId, ForeignChainAddress), sp_runtime::DispatchError> {
		let (channel_id, deposit_address) = Self::get_new_intent(SwapOrLp::Lp, source_asset);
		<Self as MockPalletStorage>::mutate_value(b"LP_INGRESS_INTENTS", |lp_intents| {
			if lp_intents.is_none() {
				*lp_intents = Some(vec![]);
			}
			if let Some(inner) = lp_intents.as_mut() {
				inner.push(LpIntent::<C, T> {
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
		egress_address: ForeignChainAddress,
		relayer_commission_bps: BasisPoints,
		relayer_id: Self::AccountId,
		message_metadata: Option<CcmIngressMetadata>,
	) -> Result<(cf_primitives::ChannelId, ForeignChainAddress), sp_runtime::DispatchError> {
		let (channel_id, deposit_address) = Self::get_new_intent(SwapOrLp::Swap, source_asset);
		<Self as MockPalletStorage>::mutate_value(b"SWAP_INGRESS_INTENTS", |swap_intents| {
			if swap_intents.is_none() {
				*swap_intents = Some(vec![]);
			}
			if let Some(inner) = swap_intents.as_mut() {
				inner.push(SwapIntent::<C, T> {
					deposit_address: deposit_address.clone(),
					source_asset,
					destination_asset,
					egress_address,
					relayer_commission_bps,
					relayer_id,
					message_metadata,
				});
			};
		});
		Ok((channel_id, deposit_address))
	}

	fn expire_intent(
		_chain: ForeignChain,
		_channel_id: ChannelId,
		address: <C as cf_chains::Chain>::ChainAccount,
	) {
		<Self as MockPalletStorage>::mutate_value(
			b"SWAP_INGRESS_INTENTS",
			|storage: &mut Option<Vec<SwapIntent<C, T>>>| {
				if let Some(inner) = storage.as_mut() {
					inner.retain(|x| x.deposit_address != address.clone().into());
				}
			},
		);
		<Self as MockPalletStorage>::mutate_value(
			b"LP_INGRESS_INTENTS",
			|storage: &mut Option<Vec<LpIntent<C, T>>>| {
				if let Some(inner) = storage.as_mut() {
					inner.retain(|x| x.deposit_address != address.clone().into());
				}
			},
		);
	}
}
