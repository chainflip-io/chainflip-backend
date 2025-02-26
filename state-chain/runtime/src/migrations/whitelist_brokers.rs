use crate::Runtime;
use cf_chains::instances::EthereumInstance;
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use sp_std::vec;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		log::info!("📺 Adding Screening Brokers");
		for broker_id in match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => vec![
				// cFNwtr2mPhpUEB5AyJq38DqMKMkSdzaL9548hajN2DRTwh7Mq
				hex_literal::hex!(
					"e08affbdaac0211709784a45048c804828d0588d0ed2e507cd6f2d60782b7c49"
				),
				// cFLRQDfEdmnv6d2XfHJNRBQHi4fruPMReLSfvB8WWD2ENbqj7
				hex_literal::hex!(
					"70d0cd75a367987344a3896a18e1510e5429ca5e88357b6c2a2e306b3877380d"
				),
			],
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
