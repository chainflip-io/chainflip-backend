#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extended_key_value_attributes)] // NOTE: This is stable as of rustc v1.54.0
#![doc = include_str!("../README.md")]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{ChainId, Ethereum};
	use cf_traits::{SigningContext, Witnesser};
	use frame_support::{
		dispatch::DispatchResultWithPostInfo, instances::Instance0, pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use pallet_cf_broadcast::{Call as BroadcastCall, Config as BroadcastConfig};
	use pallet_cf_staking::{
		Call as StakingCall, Config as StakingConfig, EthTransactionHash, EthereumAddress,
		FlipBalance,
	};
	use pallet_cf_threshold_signature::{Call as SigningCall, Config as SigningConfig};
	use pallet_cf_vaults::{Call as VaultsCall, CeremonyId, Config as VaultsConfig};
	use sp_std::prelude::*;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ StakingConfig
		+ VaultsConfig
		+ SigningConfig<Instance0, TargetChain = Ethereum>
		+ BroadcastConfig<Instance0, TargetChain = Ethereum>
	{
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: IsType<<Self as frame_system::Config>::Call>
			+ From<StakingCall<Self>>
			+ From<VaultsCall<Self>>
			+ From<SigningCall<Self, Instance0>>
			+ From<BroadcastCall<Self, Instance0>>;

		/// An implementation of the witnesser, allows us to define our witness_* helper extrinsics.
		type Witnesser: Witnesser<Call = <Self as Config>::Call, AccountId = AccountId<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		//*** Signing pallet witness calls ***//

		/// Witness the success of a threshold signing ceremony.
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
		#[pallet::weight(10_000)]
		pub fn witness_eth_signature_success(
			origin: OriginFor<T>,
			id: pallet_cf_threshold_signature::CeremonyId,
			signature: <<T as pallet_cf_threshold_signature::Config<Instance0>>::SigningContext as SigningContext<T>>::Signature,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = SigningCall::<T, Instance0>::signature_success(id, signature);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}

		/// Witness the failure of a threshold signing ceremony.
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
		#[pallet::weight(10_000)]
		pub fn witness_eth_signature_failed(
			origin: OriginFor<T>,
			id: pallet_cf_threshold_signature::CeremonyId,
			offenders: Vec<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = SigningCall::<T, Instance0>::signature_failed(id, offenders);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}

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
		#[pallet::weight(10_000)]
		pub fn witness_eth_transmission_success(
			origin: OriginFor<T>,
			id: pallet_cf_broadcast::BroadcastAttemptId,
			tx_hash: pallet_cf_broadcast::TransactionHashFor<T, Instance0>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = BroadcastCall::<T, Instance0>::transmission_success(id, tx_hash);
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
		#[pallet::weight(10_000)]
		pub fn witness_eth_transmission_failure(
			origin: OriginFor<T>,
			id: pallet_cf_broadcast::BroadcastAttemptId,
			failure: pallet_cf_broadcast::TransmissionFailure,
			tx_hash: pallet_cf_broadcast::TransactionHashFor<T, Instance0>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = BroadcastCall::<T, Instance0>::transmission_failure(id, failure, tx_hash);
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
		#[pallet::weight(10_000)]
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
		#[pallet::weight(10_000)]
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
		#[pallet::weight(10_000)]
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
		#[pallet::weight(10_000)]
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
