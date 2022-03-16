use cf_traits::Issuance;
use frame_support::{
	generate_storage_alias, migration,
	traits::{Imbalance, OnRuntimeUpgrade},
	weights::RuntimeDbWeight,
	Twox64Concat,
};
use pallet_cf_flip::{FlipIssuance, ReserveId};

use crate::{Flip, FlipBalance, Runtime};

pub struct DeleteRewardsPallet;

const VALIDATOR_REWARDS: ReserveId = *b"VALR";

generate_storage_alias!(
	Rewards, RewardsEntitlement => Map<(ReserveId, Twox64Concat), FlipBalance>
);

impl OnRuntimeUpgrade for DeleteRewardsPallet {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// 1. Burn the reserves.
		// 2. Delete the reserve.
		// 3. Delete the storage items from the pallet.
		if let Some(amount) = RewardsEntitlement::take(VALIDATOR_REWARDS) {
			if let Ok(reserve) = Flip::try_withdraw_reserves(VALIDATOR_REWARDS, amount) {
				let _ = FlipIssuance::burn(amount).offset(reserve);
			}
			if Flip::reserve(VALIDATOR_REWARDS) > 0 {
				log::error!("runtime upgrade: rewards reserve was not burned");
			}
			pallet_cf_flip::Reserve::<Runtime>::remove(VALIDATOR_REWARDS);
		}
		migration::remove_storage_prefix(b"Rewards", b"RewardsEntitlement", &[]);
		migration::remove_storage_prefix(b"Rewards", b"ApportionedRewards", &[]);
		migration::remove_storage_prefix(b"Rewards", b"Beneficiaries", &[]);

		RuntimeDbWeight::default().reads_writes(3, 5)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		use frame_support::ensure;

		ensure!(
			RewardsEntitlement::contains_key(VALIDATOR_REWARDS),
			"Expected a validator rewards reserve."
		);
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		Ok(())
	}
}
