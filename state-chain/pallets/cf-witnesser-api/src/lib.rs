#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{ChainId, Ethereum};
	use cf_traits::Witnesser;
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
	use pallet_cf_vaults::{Call as VaultsCall, CeremonyId, Config as VaultsConfig};
	use pallet_cf_witnesser::WeightInfo;
	use sp_std::prelude::*;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ StakingConfig
		+ VaultsConfig
		+ SigningConfig<Instance1, TargetChain = Ethereum>
		+ BroadcastConfig<Instance1, TargetChain = Ethereum>
	{
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: IsType<<Self as frame_system::Config>::Call>
			+ From<StakingCall<Self>>
			+ From<VaultsCall<Self>>
			+ From<SigningCall<Self, Instance1>>
			+ From<BroadcastCall<Self, Instance1>>;

		/// An implementation of the witnesser, allows us to define our witness_* helper extrinsics.
		type Witnesser: Witnesser<Call = <Self as Config>::Call, AccountId = AccountId<Self>>;

		/// Benchmark stuff
		type WeightInfoWitnesser: pallet_cf_witnesser::WeightInfo;
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
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(BroadcastCall::<T, Instance1>::transmission_success(*id, tx_hash.clone())
		.get_dispatch_info()
		.weight))]
		pub fn witness_eth_transmission_success(
			origin: OriginFor<T>,
			id: pallet_cf_broadcast::BroadcastAttemptId,
			tx_hash: pallet_cf_broadcast::TransactionHashFor<T, Instance1>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = BroadcastCall::<T, Instance1>::transmission_success(id, tx_hash);
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
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(BroadcastCall::<T, Instance1>::transmission_failure(*id, failure.clone(), tx_hash.clone())
		.get_dispatch_info()
		.weight))]
		pub fn witness_eth_transmission_failure(
			origin: OriginFor<T>,
			id: pallet_cf_broadcast::BroadcastAttemptId,
			failure: pallet_cf_broadcast::TransmissionFailure,
			tx_hash: pallet_cf_broadcast::TransactionHashFor<T, Instance1>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = BroadcastCall::<T, Instance1>::transmission_failure(id, failure, tx_hash);
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
			let call = StakingCall::claimed(account_id, claimed_amount, tx_hash);
			T::Witnesser::witness(who, call.into())
		}

		//*** Vaults pallet witness calls ***//

		/// Witness a successful key generation.
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
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(VaultsCall::<T>::keygen_success(*ceremony_id, *chain_id, new_public_key.clone())
		.get_dispatch_info()
		.weight))]
		pub fn witness_keygen_success(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			chain_id: ChainId,
			new_public_key: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = VaultsCall::keygen_success(ceremony_id, chain_id, new_public_key);
			T::Witnesser::witness(who, call.into())
		}

		/// Witness a keygen failure
		///
		/// ## Events
		///
		/// - None
		///
		/// ## Errors
		///
		/// - None
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(VaultsCall::<T>::keygen_failure(*ceremony_id, *chain_id, guilty_validators.clone())
		.get_dispatch_info()
		.weight))]
		pub fn witness_keygen_failure(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			chain_id: ChainId,
			guilty_validators: Vec<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = VaultsCall::keygen_failure(ceremony_id, chain_id, guilty_validators);
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
		#[pallet::weight(T::WeightInfoWitnesser::witness().saturating_add(VaultsCall::<T>::vault_key_rotated(*chain_id, new_public_key.clone(), *block_number, tx_hash.clone())
		.get_dispatch_info()
		.weight))]
		pub fn witness_vault_key_rotated(
			origin: OriginFor<T>,
			chain_id: ChainId,
			new_public_key: Vec<u8>,
			block_number: u64,
			tx_hash: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call =
				VaultsCall::vault_key_rotated(chain_id, new_public_key, block_number, tx_hash);
			T::Witnesser::witness(who, call.into())
		}
	}
}
