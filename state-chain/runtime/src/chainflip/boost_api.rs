use crate::{
	ArbitrumIngressEgress, AssethubIngressEgress, BitcoinIngressEgress, EthereumIngressEgress,
	PolkadotIngressEgress, SolanaIngressEgress,
};
use cf_primitives::AssetAmount;
use cf_traits::BoostApi;
use sp_core::crypto::AccountId32;

pub struct IngressEgressBoostApi;

impl BoostApi for IngressEgressBoostApi {
	type AccountId = AccountId32;
	type AssetMap = cf_chains::assets::any::AssetMap<AssetAmount>;

	fn boost_pool_account_balances(who: &Self::AccountId) -> Self::AssetMap {
		Self::AssetMap {
			eth: EthereumIngressEgress::boost_pool_account_balances(who),
			dot: PolkadotIngressEgress::boost_pool_account_balances(who),
			btc: BitcoinIngressEgress::boost_pool_account_balances(who),
			arb: ArbitrumIngressEgress::boost_pool_account_balances(who),
			sol: SolanaIngressEgress::boost_pool_account_balances(who),
			hub: AssethubIngressEgress::boost_pool_account_balances(who),
		}
	}
}
