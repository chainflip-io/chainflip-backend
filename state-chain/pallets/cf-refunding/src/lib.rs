#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../../cf-doc-head.md")]

use cf_chains::{address::AddressConverter, AnyChain, ForeignChainAddress};
use cf_primitives::{AccountRole, Asset, AssetAmount, BasisPoints, ForeignChain};
use cf_traits::{
	impl_pallet_safe_mode, liquidity::LpBalanceApi, AccountRoleRegistry, Chainflip, DepositApi,
	EgressApi, LpDepositHandler, PoolApi, ScheduledEgressDetails,
};

use sp_std::vec;

use cf_chains::assets::any::AssetMap;
use frame_support::{pallet_prelude::*, sp_runtime::DispatchResult};
use frame_system::pallet_prelude::*;
pub use pallet::*;

mod benchmarking;

// #[cfg(test)]
// mod mock;
// #[cfg(test)]
// mod tests;

// pub mod migrations;
// pub mod weights;
// pub use weights::WeightInfo;

use cf_chains::address::EncodedAddress;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(2);

impl_pallet_safe_mode!(PalletSafeMode; deposit_enabled, withdrawal_enabled);

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::Chain;
	use cf_primitives::{ChannelId, EgressId};

	use super::*;

	/// AccountOrAddress is a enum that can represent an internal account or an external address.
	/// This is used to represent the destination address for an egress during a withdrawal or an
	/// internal account to move funds internally.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, PartialOrd, Ord)]
	pub enum AccountOrAddress<AccountId> {
		Internal(AccountId),
		External(EncodedAddress),
	}

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Because we want to emit events when there is a config change during
		/// an runtime upgrade
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode>;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The user does not have enough funds.
		InsufficientBalance,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AnEvent { account_id: T::AccountId, asset: Asset, amount_debited: AssetAmount },
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(1)]
		#[pallet::weight(10_000)]
		pub fn do_something(origin: OriginFor<T>) -> DispatchResult {
			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {}
