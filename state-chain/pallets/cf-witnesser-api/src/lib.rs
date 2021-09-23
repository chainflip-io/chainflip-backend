#![cfg_attr(not(feature = "std"), no_std)]

//! Witness Api Pallet
//!
//! A collection of convenience extrinsics that delegate to other pallets via witness consensus.

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::Ethereum;
	use cf_traits::{SigningContext, Witnesser};
	use frame_support::{
		dispatch::DispatchResultWithPostInfo, instances::Instance0, pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use pallet_cf_signing::{Call as SigningCall, Config as SigningConfig};
	use pallet_cf_staking::{
		Call as StakingCall, Config as StakingConfig, EthTransactionHash, EthereumAddress,
		FlipBalance,
	};
	use pallet_cf_vaults::rotation::{CeremonyId, KeygenResponse, VaultRotationResponse};
	use pallet_cf_vaults::{
		rotation::SchnorrSigTruncPubkey, Call as VaultsCall, Config as VaultsConfig,
		ThresholdSignatureResponse,
	};
	use sp_std::prelude::*;

	type AccountId<T> = <T as frame_system::Config>::AccountId;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ StakingConfig
		+ VaultsConfig
		+ SigningConfig<Instance0, TargetChain = Ethereum>
	{
		/// Standard Call type. We need this so we can use it as a constraint in `Witnesser`.
		type Call: IsType<<Self as frame_system::Config>::Call>
			+ From<StakingCall<Self>>
			+ From<VaultsCall<Self>>
			+ From<SigningCall<Self, Instance0>>;

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
		#[pallet::weight(10_000)]
		pub fn witness_eth_signature_success(
			origin: OriginFor<T>,
			id: pallet_cf_signing::CeremonyId,
			signature: <<T as pallet_cf_signing::Config<Instance0>>::SigningContext as SigningContext<T>>::Signature,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = SigningCall::<T, Instance0>::signature_success(id, signature);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}

		/// Witness the failure of a threshold signing ceremony.
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		#[pallet::weight(10_000)]
		pub fn witness_signature_failed(
			origin: OriginFor<T>,
			id: pallet_cf_signing::CeremonyId,
			offenders: Vec<T::ValidatorId>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = SigningCall::<T, Instance0>::signature_failed(id, offenders);
			T::Witnesser::witness(who, call.into())?;
			Ok(().into())
		}

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

		//*** Vaults pallet witness calls ***//

		/// Witness that a key generation response from 2/3 of our old validators
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		#[pallet::weight(10_000)]
		pub fn witness_keygen_response(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			response: KeygenResponse<T::ValidatorId, T::PublicKey>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = VaultsCall::keygen_response(ceremony_id, response);
			T::Witnesser::witness(who, call.into())
		}

		/// Witness that a vault rotation response from 2/3 of our old validators
		///
		/// This is a convenience extrinsic that simply delegates to the configured witnesser.
		#[pallet::weight(10_000)]
		pub fn witness_vault_rotation_response(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			response: VaultRotationResponse<T::TransactionHash>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = VaultsCall::vault_rotation_response(ceremony_id, response);
			T::Witnesser::witness(who, call.into())
		}

		#[pallet::weight(10_000)]
		pub fn witness_threshold_signature_response(
			origin: OriginFor<T>,
			ceremony_id: CeremonyId,
			response: ThresholdSignatureResponse<T::ValidatorId, SchnorrSigTruncPubkey>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let call = VaultsCall::threshold_signature_response(ceremony_id, response);
			T::Witnesser::witness(who, call.into())
		}
	}
}
