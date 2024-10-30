#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::{
	pallet_prelude::GetStorageVersion,
	traits::{OnRuntimeUpgrade, PalletInfoAccess, StorageVersion, UncheckedOnRuntimeUpgrade},
};
use sp_std::marker::PhantomData;

mod helper_functions;
pub use helper_functions::*;

pub mod migration_template;

pub mod genesis_hashes {
	use frame_support::sp_runtime::traits::Zero;
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_core::H256;

	pub const BERGHAIN: [u8; 32] =
		hex_literal::hex!("8b8c140b0af9db70686583e3f6bf2a59052bfe9584b97d20c45068281e976eb9");
	pub const PERSEVERANCE: [u8; 32] =
		hex_literal::hex!("7a5d4db858ada1d20ed6ded4933c33313fc9673e5fffab560d0ca714782f2080");
	/// NOTE: IF YOU USE THIS CONSTANT, MAKE SURE IT IS STILL VALID: SISYPHOS IS RELAUNCHED
	/// FROM TIME TO TIME.
	pub const SISYPHOS: [u8; 32] =
		hex_literal::hex!("7db0684f891ad10fa919c801f9a9f030c0f6831aafa105b1a68e47803f91f2b6");

	pub fn genesis_hash<T: frame_system::Config<Hash = H256>>() -> [u8; 32] {
		frame_system::BlockHash::<T>::get(BlockNumberFor::<T>::zero()).to_fixed_bytes()
	}
}

/// A placeholder migration that does nothing. Useful too allow us to keep the boilerplate in the
/// runtime consistent.
pub struct PlaceholderMigration<
	const AT: u16,
	P: PalletInfoAccess + GetStorageVersion<InCodeStorageVersion = StorageVersion>,
>(PhantomData<P>);

impl<const AT: u16, P> OnRuntimeUpgrade for PlaceholderMigration<AT, P>
where
	P: PalletInfoAccess + GetStorageVersion<InCodeStorageVersion = StorageVersion>,
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		if <P as GetStorageVersion>::on_chain_storage_version() == AT {
			log::info!(
				"👌 {}: Placeholder migration at pallet storage version {:?}. Nothing to do.",
				P::name(),
				AT,
			);
		} else {
			log::warn!(
				"🚨 {}: Placeholder migration at pallet storage version {:?} but storage version is {:?}.",
				P::name(),
				AT,
				<P as GetStorageVersion>::on_chain_storage_version(),
			);
		}
		Default::default()
	}
}

pub struct NoopRuntimeUpgrade;

impl UncheckedOnRuntimeUpgrade for NoopRuntimeUpgrade {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Default::default()
	}
}
