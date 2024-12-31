use crate::{AssetBalances, Swapping};
use cf_primitives::EpochIndex;
use cf_traits::EpochTransitionHandler;

use crate::{ArbitrumVault, BitcoinVault, EthereumVault, PolkadotVault, SolanaVault, Witnesser};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	fn on_expired_epoch(expired: EpochIndex) {
		<Witnesser as EpochTransitionHandler>::on_expired_epoch(expired);
		<EthereumVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<PolkadotVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<BitcoinVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<ArbitrumVault as EpochTransitionHandler>::on_expired_epoch(expired);
		<SolanaVault as EpochTransitionHandler>::on_expired_epoch(expired);
	}
	fn on_new_epoch(_new: EpochIndex) {
		AssetBalances::trigger_reconciliation();
		Swapping::trigger_commission_distribution();
	}
}
