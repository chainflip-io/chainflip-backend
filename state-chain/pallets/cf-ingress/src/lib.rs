#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

// This should be instatiable to the INCOMING chain.
// This way intents and intent ids align per chain, which makes sense given they act as an index to
// the respective address generation function.

use cf_chains::Chain;
use cf_primitives::{Asset, AssetAmount, ForeignChainAddress, IntentId};
use cf_traits::{
	liquidity::LpProvisioningApi, AddressDerivationApi, IngressApi, IngressFetchApi,
	SwapIntentHandler,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{app_crypto::sp_core, DispatchError},
};
use sp_std::vec;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use core::marker::PhantomData;

	use super::*;
	use cf_traits::SwapIntentHandler;
	use frame_support::{
		pallet_prelude::{DispatchResultWithPostInfo, OptionQuery, ValueQuery},
		traits::{EnsureOrigin, IsType},
	};
	use sp_core::H256;
	use sp_std::vec::Vec;

	use frame_system::pallet_prelude::OriginFor;

	pub(crate) type TargetChainAsset<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAsset;
	pub(crate) type TargetChainAccount<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainAccount;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct IngressWitness<C: Chain> {
		pub ingress_address: C::ChainAccount,
		pub asset: C::ChainAsset,
		pub amount: u128,
		pub tx_hash: H256,
	}

	/// Details used to determine the ingress of funds.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct IngressDetails<C: Chain> {
		pub intent_id: IntentId,
		pub ingress_asset: C::ChainAsset,
	}

	/// Contains information relevant to the action to commence once ingress succeeds.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum IntentAction<AccountId> {
		Swap {
			egress_asset: Asset,
			egress_address: ForeignChainAddress,
			relayer_id: AccountId,
			relayer_commission_bps: u16,
		},
		LiquidityProvision {
			lp_account: AccountId,
		},
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::storage]
	pub type IntentIngressDetails<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		IngressDetails<T::TargetChain>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type IntentActions<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		IntentAction<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;

	/// Stores the latest intent id used to generate an address.
	#[pallet::storage]
	pub type IntentIdCounter<T: Config<I>, I: 'static = ()> = StorageValue<_, IntentId, ValueQuery>;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: cf_traits::Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self, I>> + IsType<<Self as frame_system::Config>::Event>;

		/// Marks which chain this pallet is interacting with.
		type TargetChain: Chain;

		/// Generates ingress addresses.
		type AddressDerivation: AddressDerivationApi;

		/// Pallet responsible for managing Liquidity Providers.
		type LpAccountHandler: LpProvisioningApi<AccountId = Self::AccountId, Amount = AssetAmount>;

		/// For scheduling fetch requests.
		type IngressFetchApi: IngressFetchApi<Self::TargetChain>;

		/// For scheduling swaps.
		type SwapIntentHandler: SwapIntentHandler<AccountId = Self::AccountId>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// We only want to witness for one asset on a particular chain
		StartWitnessing {
			ingress_address: TargetChainAccount<T, I>,
			ingress_asset: TargetChainAsset<T, I>,
		},

		IngressCompleted {
			ingress_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amount: u128,
			tx_hash: H256,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		InvalidIntent,
		IngressMismatchWithIntent,
		IntentIdsExhausted,
		UnsupportedAsset,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I>
	where
		<<T as Config<I>>::TargetChain as Chain>::ChainAsset: Into<cf_primitives::Asset>,
		<<T as Config<I>>::TargetChain as Chain>::ChainAccount:
			From<cf_primitives::ForeignChainAddress>,
	{
		#[pallet::weight(T::WeightInfo::do_single_ingress().saturating_mul(ingress_witnesses.
		len() as u64))]
		pub fn do_ingress(
			origin: OriginFor<T>,
			ingress_witnesses: Vec<IngressWitness<T::TargetChain>>,
		) -> DispatchResultWithPostInfo {
			T::EnsureWitnessed::ensure_origin(origin)?;

			for IngressWitness { ingress_address, asset, amount, tx_hash } in ingress_witnesses {
				Self::do_single_ingress(ingress_address, asset, amount, tx_hash)?;
			}
			Ok(().into())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I>
where
	<<T as Config<I>>::TargetChain as Chain>::ChainAsset: Into<cf_primitives::Asset>,
	<<T as Config<I>>::TargetChain as Chain>::ChainAccount:
		From<cf_primitives::ForeignChainAddress>,
{
	fn generate_new_address(
		ingress_asset: TargetChainAsset<T, I>,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		let next_intent_id = IntentIdCounter::<T, I>::get()
			.checked_add(1)
			.ok_or(Error::<T, I>::IntentIdsExhausted)?;
		let ingress_address =
			T::AddressDerivation::generate_address(ingress_asset.into(), next_intent_id)?;
		IntentIdCounter::<T, I>::put(next_intent_id);
		Ok((next_intent_id, ingress_address))
	}

	fn do_single_ingress(
		ingress_address: TargetChainAccount<T, I>,
		asset: TargetChainAsset<T, I>,
		amount: u128,
		tx_hash: sp_core::H256,
	) -> DispatchResult {
		let ingress = IntentIngressDetails::<T, I>::get(&ingress_address)
			.ok_or(Error::<T, I>::InvalidIntent)?;
		ensure!(ingress.ingress_asset == asset, Error::<T, I>::IngressMismatchWithIntent);

		// Ingress is called by witnessers, so asset/chain combination should always be valid.
		T::IngressFetchApi::schedule_ingress_fetch(vec![(asset, ingress.intent_id)]);

		// NB: Don't take here. We should continue witnessing this address
		// even after an ingress to it has occurred.
		// https://github.com/chainflip-io/chainflip-eth-contracts/pull/226
		match IntentActions::<T, I>::get(&ingress_address).ok_or(Error::<T, I>::InvalidIntent)? {
			IntentAction::LiquidityProvision { lp_account, .. } =>
				T::LpAccountHandler::provision_account(&lp_account, asset.into(), amount)?,
			IntentAction::Swap {
				egress_address,
				egress_asset,
				relayer_id,
				relayer_commission_bps,
			} => T::SwapIntentHandler::schedule_swap(
				asset.into(),
				egress_asset,
				amount,
				egress_address,
				relayer_id,
				relayer_commission_bps,
			),
		};

		Self::deposit_event(Event::IngressCompleted { ingress_address, asset, amount, tx_hash });
		Ok(())
	}
}

impl<T: Config<I>, I: 'static> IngressApi<T::TargetChain> for Pallet<T, I>
where
	<<T as Config<I>>::TargetChain as Chain>::ChainAsset: Into<cf_primitives::Asset>,
	<<T as Config<I>>::TargetChain as Chain>::ChainAccount:
		From<cf_primitives::ForeignChainAddress>,
{
	type AccountId = <T as frame_system::Config>::AccountId;

	// This should be callable by the LP pallet.
	fn register_liquidity_ingress_intent(
		lp_account: Self::AccountId,
		ingress_asset: TargetChainAsset<T, I>,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		let (intent_id, ingress_address) = Self::generate_new_address(ingress_asset)?;

		let chain_address = ingress_address.into();
		IntentIngressDetails::<T, I>::insert(
			&chain_address,
			IngressDetails { intent_id, ingress_asset },
		);
		IntentActions::<T, I>::insert(
			&chain_address,
			IntentAction::LiquidityProvision { lp_account },
		);

		Self::deposit_event(Event::StartWitnessing {
			ingress_address: chain_address,
			ingress_asset,
		});

		Ok((intent_id, ingress_address))
	}

	// This should only be callable by the relayer.
	fn register_swap_intent(
		ingress_asset: TargetChainAsset<T, I>,
		egress_asset: Asset,
		egress_address: ForeignChainAddress,
		relayer_commission_bps: u16,
		relayer_id: T::AccountId,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		let (intent_id, ingress_address) = Self::generate_new_address(ingress_asset)?;

		let chain_address = ingress_address.into();
		IntentIngressDetails::<T, I>::insert(
			&chain_address,
			IngressDetails { intent_id, ingress_asset },
		);
		IntentActions::<T, I>::insert(
			&chain_address,
			IntentAction::Swap { egress_address, egress_asset, relayer_commission_bps, relayer_id },
		);

		Self::deposit_event(Event::StartWitnessing {
			ingress_address: chain_address,
			ingress_asset,
		});

		Ok((intent_id, ingress_address))
	}
}
