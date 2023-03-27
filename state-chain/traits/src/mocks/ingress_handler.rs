use crate::{Chainflip, IngressApi};
use cf_chains::{address::ForeignChainAddress, eth::assets::any, Chain, ForeignChain};
use cf_primitives::IntentId;
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
	ingress_address: ForeignChainAddress,
	ingress_asset: <C as Chain>::ChainAsset,
	egress_asset: any::Asset,
	egress_address: ForeignChainAddress,
	relayer_commission_bps: u16,
	relayer_id: <T as frame_system::Config>::AccountId,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct LpIntent<C: Chain, T: Chainflip> {
	ingress_address: ForeignChainAddress,
	ingress_asset: <C as Chain>::ChainAsset,
	lp_account: <T as frame_system::Config>::AccountId,
}

impl<C: Chain, T: Chainflip> MockIngressHandler<C, T> {
	fn get_new_intent(
		swap_or_lp: SwapOrLp,
		asset: <C as Chain>::ChainAsset,
	) -> (IntentId, ForeignChainAddress) {
		let intent_id = <Self as MockPalletStorage>::mutate_value(
			match swap_or_lp {
				SwapOrLp::Swap => b"SWAP_INTENT_ID",
				SwapOrLp::Lp => b"LP_INTENT_ID",
			},
			|storage| {
				let intent_id: IntentId = storage.unwrap_or_default();
				let _ = storage.insert(intent_id + 1);
				intent_id
			},
		);
		(
			intent_id,
			match asset.into() {
				ForeignChain::Ethereum => ForeignChainAddress::Eth([intent_id as u8; 20]),
				ForeignChain::Polkadot => ForeignChainAddress::Dot([intent_id as u8; 32]),
				ForeignChain::Bitcoin => todo!("Bitcoin address"),
			},
		)
	}

	pub fn get_liquidity_intents(
	) -> Vec<(ForeignChainAddress, <C as Chain>::ChainAsset, <T as frame_system::Config>::AccountId)>
	{
		<Self as MockPalletStorage>::get_value(b"LP_INGRESS_INTENTS").unwrap_or_default()
	}

	pub fn get_swap_intents() -> Vec<SwapIntent<C, T>> {
		<Self as MockPalletStorage>::get_value(b"SWAP_INGRESS_INTENTS").unwrap_or_default()
	}
}

impl<C: Chain, T: Chainflip> IngressApi<C> for MockIngressHandler<C, T> {
	type AccountId = <T as frame_system::Config>::AccountId;

	fn register_liquidity_ingress_intent(
		lp_account: Self::AccountId,
		ingress_asset: <C as cf_chains::Chain>::ChainAsset,
	) -> Result<(cf_primitives::IntentId, ForeignChainAddress), sp_runtime::DispatchError> {
		let (intent_id, ingress_address) = Self::get_new_intent(SwapOrLp::Lp, ingress_asset);
		<Self as MockPalletStorage>::mutate_value(b"LP_INGRESS_INTENTS", |storage| {
			storage.as_mut().unwrap_or(&mut vec![]).push(LpIntent::<C, T> {
				ingress_address: ingress_address.clone(),
				ingress_asset,
				lp_account,
			});
		});
		Ok((intent_id, ingress_address))
	}

	fn register_swap_intent(
		ingress_asset: <C as Chain>::ChainAsset,
		egress_asset: cf_primitives::Asset,
		egress_address: ForeignChainAddress,
		relayer_commission_bps: u16,
		relayer_id: Self::AccountId,
	) -> Result<(cf_primitives::IntentId, ForeignChainAddress), sp_runtime::DispatchError> {
		let (intent_id, ingress_address) = Self::get_new_intent(SwapOrLp::Swap, ingress_asset);
		<Self as MockPalletStorage>::mutate_value(b"SWAP_INGRESS_INTENTS", |storage| {
			storage.as_mut().unwrap_or(&mut vec![]).push(SwapIntent::<C, T> {
				ingress_address: ingress_address.clone(),
				ingress_asset,
				egress_asset,
				egress_address,
				relayer_commission_bps,
				relayer_id,
			})
		});
		Ok((intent_id, ingress_address))
	}
}
