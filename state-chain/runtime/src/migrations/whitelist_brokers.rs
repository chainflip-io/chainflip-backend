use crate::Runtime;
use cf_chains::instances::EthereumInstance;
use cf_runtime_upgrade_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::vec;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("📺 Adding Screening Brokers");
		for broker_id in match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => vec![],
			genesis_hashes::PERSEVERANCE => vec![hex_literal::hex!(
				"061f99c033eb3ae6a64f323d037ceafcd3b537528a6f31927c5fd56e4625e532"
			)],
			genesis_hashes::SISYPHOS => vec![],
			_ => vec![],
		} {
			pallet_cf_ingress_egress::WhitelistedBrokers::<Runtime, EthereumInstance>::insert(
				crate::AccountId::new(broker_id),
				(),
			);
		}
		Weight::zero()
	}
}
