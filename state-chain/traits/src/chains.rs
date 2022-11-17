use super::*;
use cf_chains::AnyChain;
use cf_primitives::{chains::assets::any, ForeignChain};

pub struct ForeignChainIngressEgressHandler<EthereumHandler, PolkadotHandler>(
	PhantomData<(EthereumHandler, PolkadotHandler)>,
);

impl<EthereumHandler, PolkadotHandler> EgressApi<AnyChain>
	for ForeignChainIngressEgressHandler<EthereumHandler, PolkadotHandler>
where
	EthereumHandler: EgressApi<Ethereum>,
	PolkadotHandler: EgressApi<Polkadot>,
{
	fn schedule_egress(
		asset: any::Asset,
		amount: AssetAmount,
		egress_address: <AnyChain as Chain>::ChainAccount,
	) {
		match asset.into() {
			ForeignChain::Ethereum => EthereumHandler::schedule_egress(
				asset.try_into().expect("Checked for asset compatibility"),
				amount,
				egress_address
					.try_into()
					.expect("Caller must ensure for account is of the compatible type."),
			),
			ForeignChain::Polkadot => PolkadotHandler::schedule_egress(
				asset.try_into().expect("Checked for asset compatibility"),
				amount,
				egress_address
					.try_into()
					.expect("Caller must ensure for account is of the compatible type."),
			),
		}
	}
}

impl<EthereumHandler, PolkadotHandler, NativeAccountId> IngressApi<AnyChain, NativeAccountId>
	for ForeignChainIngressEgressHandler<EthereumHandler, PolkadotHandler>
where
	EthereumHandler: IngressApi<Ethereum, NativeAccountId>,
	PolkadotHandler: IngressApi<Polkadot, NativeAccountId>,
{
	// This should be callable by the LP pallet.
	fn register_liquidity_ingress_intent(
		lp_account: NativeAccountId,
		ingress_asset: Asset,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		match ingress_asset.into() {
			ForeignChain::Ethereum => EthereumHandler::register_liquidity_ingress_intent(
				lp_account,
				ingress_asset.try_into().unwrap(),
			),
			ForeignChain::Polkadot => PolkadotHandler::register_liquidity_ingress_intent(
				lp_account,
				ingress_asset.try_into().unwrap(),
			),
		}
	}

	// This should only be callable by the relayer.
	fn register_swap_intent(
		ingress_asset: Asset,
		egress_asset: Asset,
		egress_address: ForeignChainAddress,
		relayer_commission_bps: u16,
		relayer_id: NativeAccountId,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		match ingress_asset.into() {
			ForeignChain::Ethereum => EthereumHandler::register_swap_intent(
				ingress_asset.try_into().unwrap(),
				egress_asset,
				egress_address,
				relayer_commission_bps,
				relayer_id,
			),
			ForeignChain::Polkadot => PolkadotHandler::register_swap_intent(
				ingress_asset.try_into().unwrap(),
				egress_asset,
				egress_address,
				relayer_commission_bps,
				relayer_id,
			),
		}
	}
}
