#![cfg_attr(not(feature = "std"), no_std)]

//! Witness Api Pallet
//!
//! A collection of convenience extrinsics that delegate to other pallets via witness consensus.

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
	use cf_traits::Witnesser;
	use frame_support::{dispatch::DispatchResultWithPostInfo, pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use pallet_cf_staking::{
		Call as StakingCall, Config as StakingConfig, EthTransactionHash, EthereumAddress,
		FlipBalance,
	};

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[pallet::config]
	pub trait Config: frame_system::Config + StakingConfig {
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: IsType<<Self as frame_system::Config>::Call>
			+ From<StakingCall<Self>>;

		/// An implementation of the witnesser, allows us to define our witness_* helper extrinsics.
		type Witnesser: Witnesser<Call = <Self as Config>::Call, AccountId = AccountId<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		//*** Staking pallet witness calls ***//

		/// Witness that a `Staked` event was emitted by the `StakeManager` smart contract.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		#[pallet::weight(10_000)]
		pub fn witness_staked(
			origin: OriginFor<T>,
			staker_account_id: AccountId<T>,
			amount: FlipBalance<T>,
			withdrawal_address: Option<EthereumAddress>,
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = StakingCall::staked(staker_account_id, amount, withdrawal_address, tx_hash);
			T::Witnesser::witness(who, call.into())
		}

		/// Witness that a `Claimed` event was emitted by the `StakeManager` smart contract.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		#[pallet::weight(10_000)]
		pub fn witness_claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: FlipBalance<T>,
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = StakingCall::claimed(account_id, claimed_amount, tx_hash);
			T::Witnesser::witness(who, call.into())
		}
	}
}
