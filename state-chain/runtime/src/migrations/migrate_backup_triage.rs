use crate::Runtime;
use cf_traits::{Chainflip, ChainflipAccountStore};
use frame_support::{
	dispatch::Weight, generate_storage_alias, traits::OnRuntimeUpgrade, weights::RuntimeDbWeight,
};
use pallet_cf_validator::{BackupValidatorTriage, RuntimeBackupTriage};
use sp_std::prelude::*;

generate_storage_alias!(
	Auction, HighestPassiveNodeBid => Value<u128>
);
generate_storage_alias!(
	Auction, LowestBackupValidatorBid => Value<u128>
);
generate_storage_alias!(
	Auction, BackupGroupSize => Value<u32>
);
generate_storage_alias!(
	Auction, RemainingBidders => Value<Vec<(<Runtime as Chainflip>::ValidatorId, u128)>>
);

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		HighestPassiveNodeBid::kill();
		LowestBackupValidatorBid::kill();
		let backup_group_size_target = BackupGroupSize::take().unwrap_or_default();
		if let Some(remaining_bidders) = RemainingBidders::take() {
			let triage = RuntimeBackupTriage::<Runtime>::new::<ChainflipAccountStore<Runtime>>(
				remaining_bidders.into_iter().map(Into::into).collect(),
				backup_group_size_target as usize,
			);
			BackupValidatorTriage::<Runtime>::put(triage);
		}
		RuntimeDbWeight::default().reads_writes(2, 3)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		use frame_support::ensure;

		ensure!(RemainingBidders::get().is_some(), "Expected RemainingBidders to be removed.");

		Ok(())
	}
}
