use crate::{chainflip::Offence, Runtime};
use cf_traits::Chainflip;
use codec::{Decode, Encode};
#[cfg(feature = "try-runtime")]
use frame_support::traits::OnRuntimeUpgradeHelpersExt;
use frame_support::{
	dispatch::GetStorageVersion, traits::OnRuntimeUpgrade, weights::RuntimeDbWeight,
	Blake2_128Concat,
};

pub struct Migration;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
struct Liveness {
	pub last_heartbeat: <Runtime as frame_system::Config>::BlockNumber,
	pub banned_until: <Runtime as frame_system::Config>::BlockNumber,
}

frame_support::generate_storage_alias!(
	Online, Nodes => Map<(Blake2_128Concat, <Runtime as Chainflip>::ValidatorId), Liveness>
);

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if <pallet_cf_reputation::Pallet<Runtime> as GetStorageVersion>::on_chain_storage_version() >=
			2
		{
			log::warn!("Obsolete storage upgrade triggered in runtime. Remove this!");
			return 0
		}
		let mut count = 0;
		for (id, Liveness { last_heartbeat, banned_until }) in Nodes::iter().drain() {
			pallet_cf_online::LastHeartbeat::<Runtime>::insert(&id, last_heartbeat);
			pallet_cf_reputation::Suspensions::<Runtime>::mutate(
				Offence::ParticipateSigningFailed,
				|s| s.push_back((banned_until, id)),
			);
			count += 1;
		}
		pallet_cf_reputation::Suspensions::<Runtime>::mutate(
			Offence::ParticipateSigningFailed,
			|s| s.make_contiguous().sort_unstable_by_key(|(block, _)| *block),
		);
		count += 1;
		RuntimeDbWeight::default().reads_writes(count, count * 2)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		let node_count = Nodes::iter().count() as u32;
		Self::set_temp_storage(node_count, "NODE_COUNT");
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		use frame_support::ensure;

		let heartbeat_count = pallet_cf_online::LastHeartbeat::<Runtime>::iter().count() as u32;
		let suspension_count =
			pallet_cf_reputation::Suspensions::<Runtime>::get(Offence::ParticipateSigningFailed)
				.len() as u32;
		let node_count = Self::get_temp_storage::<u32>("NODE_COUNT")
			.expect("shold have been stored pre-upgrade");

		ensure!(heartbeat_count == node_count, "heartbeat count does not match node count");
		ensure!(suspension_count == node_count, "suspension count does not match node count");
		Ok(())
	}
}
