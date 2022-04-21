#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{ChainCrypto, Ethereum};
	use cf_traits::{NetworkStateInfo, Witnesser};
	use frame_support::{
		dispatch::DispatchResultWithPostInfo, instances::Instance1, pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use pallet_cf_broadcast::{Call as BroadcastCall, Config as BroadcastConfig};
	use pallet_cf_staking::{
		Call as StakingCall, Config as StakingConfig, EthTransactionHash, EthereumAddress,
		FlipBalance,
	};
	use pallet_cf_threshold_signature::{Call as SigningCall, Config as SigningConfig};
	use pallet_cf_vaults::{Call as VaultsCall, Config as VaultsConfig};
	use pallet_cf_witnesser::WeightInfo;
	use sp_std::prelude::*;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ StakingConfig
		+ VaultsConfig<Instance1, Chain = Ethereum>
		+ SigningConfig<Instance1, TargetChain = Ethereum>
		+ BroadcastConfig<Instance1, TargetChain = Ethereum>
	{
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: IsType<<Self as frame_system::Config>::Call>
			+ From<StakingCall<Self>>
			+ From<VaultsCall<Self, Instance1>>
			+ From<SigningCall<Self, Instance1>>
			+ From<BroadcastCall<Self, Instance1>>;

		/// An implementation of the witnesser, allows us to define our witness_* helper extrinsics.
		type Witnesser: Witnesser<Call = <Self as Config>::Call, AccountId = AccountId<Self>>;

		/// Benchmark stuff
		type WeightInfoWitnesser: pallet_cf_witnesser::WeightInfo;

		/// Handles access to the network state.
		type NetworkStateAccess: NetworkStateInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		//*** Broadcast pallet witness calls ***//

		/// Witness the successful completion of an outgoing broadcast.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(BroadcastCall::<T, Instance1>::transmission_success(*broadcast_attempt_id, *tx_hash)
		.get_dispatch_info()
		.weight))]
		pub fn witness_eth_transmission_success(
			origin: OriginFor<T>,
			broadcast_attempt_id: pallet_cf_broadcast::BroadcastAttemptId,
			tx_hash: pallet_cf_broadcast::TransactionHashFor<T, Instance1>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Check if the network is paused
			T::NetworkStateAccess::ensure_paused()?;
			let call =
				BroadcastCall::<T, Instance1>::transmission_success(broadcast_attempt_id, tx_hash);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}

		/// Witness the failure of an outgoing broadcast.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(BroadcastCall::<T, Instance1>::transmission_failure(*broadcast_attempt_id, failure.clone(), *tx_hash)
		.get_dispatch_info()
		.weight))]
		pub fn witness_eth_transmission_failure(
			origin: OriginFor<T>,
			broadcast_attempt_id: pallet_cf_broadcast::BroadcastAttemptId,
			failure: pallet_cf_broadcast::TransmissionFailure,
			tx_hash: pallet_cf_broadcast::TransactionHashFor<T, Instance1>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Check if the network is paused
			T::NetworkStateAccess::ensure_paused()?;
			let call = BroadcastCall::<T, Instance1>::transmission_failure(
				broadcast_attempt_id,
				failure,
				tx_hash,
			);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}

		//*** Staking pallet witness calls ***//

		/// Witness that a `Staked` event was emitted by the `StakeManager` smart contract.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(StakingCall::<T>::staked(staker_account_id.clone(), *amount, *withdrawal_address, *tx_hash)
		.get_dispatch_info()
		.weight))]
		pub fn witness_staked(
			origin: OriginFor<T>,
			staker_account_id: AccountId<T>,
			amount: FlipBalance<T>,
			withdrawal_address: EthereumAddress,
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Check if the network is paused
			T::NetworkStateAccess::ensure_paused()?;
			let call = StakingCall::staked(staker_account_id, amount, withdrawal_address, tx_hash);
			T::Witnesser::witness(who, call.into())
		}

		/// Witness that a `Claimed` event was emitted by the `StakeManager` smart contract.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(StakingCall::<T>::claimed(account_id.clone(), *claimed_amount, *tx_hash)
		.get_dispatch_info()
		.weight))]
		pub fn witness_claimed(
			origin: OriginFor<T>,
			account_id: AccountId<T>,
			claimed_amount: FlipBalance<T>,
			tx_hash: EthTransactionHash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Check if the network is paused
			T::NetworkStateAccess::ensure_paused()?;
			let call = StakingCall::claimed(account_id, claimed_amount, tx_hash);
			T::Witnesser::witness(who, call.into())
		}

		/// Witness an on-chain vault key rotation
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		#[pallet::weight(
			T::WeightInfoWitnesser::witness().saturating_add(
				VaultsCall::<T, Instance1>::vault_key_rotated(*new_public_key, *block_number, *tx_hash)
					.get_dispatch_info()
					.weight
		))]
		pub fn witness_eth_aggkey_rotation(
			origin: OriginFor<T>,
			new_public_key: <Ethereum as ChainCrypto>::AggKey,
			block_number: u64,
			tx_hash: <Ethereum as ChainCrypto>::TransactionHash,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			// Check if the network is paused
			T::NetworkStateAccess::ensure_paused()?;
			let call = VaultsCall::<T, Instance1>::vault_key_rotated(
				new_public_key,
				block_number,
				tx_hash,
			);
			T::Witnesser::witness(who, call.into())
		}
	}
}
