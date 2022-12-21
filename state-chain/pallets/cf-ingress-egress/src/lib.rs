#![cfg_attr(not(feature = "std"), no_std)]
#![feature(drain_filter)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;
pub use weights::WeightInfo;

use cf_primitives::{EgressCounter, EgressId, ForeignChain};

use cf_chains::{AllBatch, Chain, ChainAbi, ChainCrypto, FetchAssetParams, TransferAssetParams};
use cf_primitives::{Asset, AssetAmount, ForeignChainAddress, IntentId};
use cf_traits::{
	liquidity::LpProvisioningApi, AddressDerivationApi, Broadcaster, EgressApi, IngressApi,
	SwapIntentHandler,
};
use frame_support::{pallet_prelude::*, sp_runtime::DispatchError};
pub use pallet::*;
use sp_runtime::{traits::Saturating, TransactionOutcome};
pub use sp_std::{vec, vec::Vec};

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Copy, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum FetchOrTransfer<C: Chain> {
	Fetch { intent_id: IntentId, asset: C::ChainAsset },
	Transfer { egress_id: EgressId, asset: C::ChainAsset, to: C::ChainAccount, amount: AssetAmount },
}

impl<C: Chain> FetchOrTransfer<C> {
	fn asset(&self) -> &C::ChainAsset {
		match self {
			FetchOrTransfer::Fetch { asset, .. } => asset,
			FetchOrTransfer::Transfer { asset, .. } => asset,
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_primitives::BroadcastId;
	use core::marker::PhantomData;
	use sp_std::vec::Vec;

	use cf_traits::{Chainflip, SwapIntentHandler};

	use frame_support::{
		pallet_prelude::{OptionQuery, ValueQuery},
		storage::with_transaction,
		traits::{EnsureOrigin, IsType},
	};
	use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};

	pub(crate) type TargetChainAsset<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAsset;
	pub(crate) type TargetChainAccount<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainAccount;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct IngressWitness<C: Chain + ChainCrypto> {
		pub ingress_address: C::ChainAccount,
		pub asset: C::ChainAsset,
		pub amount: AssetAmount,
		pub tx_id: <C as ChainCrypto>::TransactionId,
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

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>: Chainflip {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Marks which chain this pallet is interacting with.
		type TargetChain: ChainAbi + Get<ForeignChain>;

		/// Generates ingress addresses.
		type AddressDerivation: AddressDerivationApi<Self::TargetChain>;

		/// Pallet responsible for managing Liquidity Providers.
		type LpProvisioning: LpProvisioningApi<AccountId = Self::AccountId>;

		/// For scheduling swaps.
		type SwapIntentHandler: SwapIntentHandler<AccountId = Self::AccountId>;

		/// The type of the chain-native transaction.
		type AllBatch: AllBatch<Self::TargetChain>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<Self::TargetChain, ApiCall = Self::AllBatch>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::Origin>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

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

	/// Stores the latest egress id used to generate an address.
	#[pallet::storage]
	pub type EgressIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, EgressCounter, ValueQuery>;

	/// Scheduled fetch and egress for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type ScheduledEgressRequests<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<FetchOrTransfer<T::TargetChain>>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub(crate) type DisabledEgressAssets<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, ()>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		StartWitnessing {
			ingress_address: TargetChainAccount<T, I>,
			ingress_asset: TargetChainAsset<T, I>,
		},

		IngressCompleted {
			ingress_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amount: AssetAmount,
			tx_id: <T::TargetChain as ChainCrypto>::TransactionId,
		},
		AssetEgressDisabled {
			asset: TargetChainAsset<T, I>,
			disabled: bool,
		},
		EgressScheduled {
			id: EgressId,
			asset: TargetChainAsset<T, I>,
			amount: AssetAmount,
			egress_address: TargetChainAccount<T, I>,
		},
		IngressFetchesScheduled {
			intent_id: IntentId,
			asset: TargetChainAsset<T, I>,
		},
		BatchBroadcastRequested {
			broadcast_id: BroadcastId,
			egress_ids: Vec<EgressId>,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		InvalidIntent,
		IngressMismatchWithIntent,
		IntentIdsExhausted,
		UnsupportedAsset,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Take a batch of scheduled Egress and send them out
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// Ensure we have enough weight to send an non-empty batch, and request queue isn't
			// empty.
			if remaining_weight <= T::WeightInfo::egress_assets(1u32) ||
				ScheduledEgressRequests::<T, I>::decode_len() == Some(0)
			{
				return T::WeightInfo::on_idle_with_nothing_to_send()
			}

			// Calculate the number of requests that the weight allows.
			let single_request_cost = T::WeightInfo::egress_assets(1u32)
				.saturating_sub(T::WeightInfo::egress_assets(0u32));
			let request_count = remaining_weight
				.saturating_sub(T::WeightInfo::egress_assets(0u32))
				.saturating_div(single_request_cost) as u32;

			with_transaction(|| Self::egress_scheduled_assets(Some(request_count)))
				.unwrap_or_else(|_| T::WeightInfo::egress_assets(0))
		}

		fn integrity_test() {
			// Ensures the weights are benchmarked correctly.
			assert!(T::WeightInfo::egress_assets(1) > T::WeightInfo::egress_assets(0));
			assert!(T::WeightInfo::do_single_ingress() > 0);
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Sets if an asset is not allowed to be sent out of the chain via Egress.
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::AssetEgressDisabled)
		#[pallet::weight(T::WeightInfo::disable_asset_egress())]
		pub fn disable_asset_egress(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			disabled: bool,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;

			if disabled {
				DisabledEgressAssets::<T, I>::insert(asset, ());
			} else {
				DisabledEgressAssets::<T, I>::remove(asset);
			}

			Self::deposit_event(Event::<T, I>::AssetEgressDisabled { asset, disabled });

			Ok(())
		}

		/// Send up to `maybe_size` number of scheduled transactions out for a specific chain.
		/// If None is set for `maybe_size`, send all scheduled transactions.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_success](Event::BatchBroadcastRequested)
		#[pallet::weight(0)]
		pub fn egress_scheduled_assets_for_chain(
			origin: OriginFor<T>,
			maybe_size: Option<u32>,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			with_transaction(|| Self::egress_scheduled_assets(maybe_size))?;
			Ok(())
		}

		/// Complete an ingress request. Called when funds have been deposited into the given
		/// address. Requires `EnsureWitnessed` origin.
		#[pallet::weight(T::WeightInfo::do_single_ingress().saturating_mul(ingress_witnesses.len() as u64))]
		pub fn do_ingress(
			origin: OriginFor<T>,
			ingress_witnesses: Vec<IngressWitness<T::TargetChain>>,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			for IngressWitness { ingress_address, asset, amount, tx_id } in ingress_witnesses {
				Self::do_single_ingress(ingress_address, asset, amount, tx_id)?;
			}
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Take up to `maybe_size` number of scheduled requests for the Ethereum chain and send them
	/// out in an `AllBatch` call. If `maybe_size` is `None`, send all scheduled transactions.
	///
	/// Returns the actual number of transactions sent.
	///
	/// Egress transactions with Blacklisted assets are not sent, and kept in storage.
	#[allow(clippy::type_complexity)]
	fn egress_scheduled_assets(
		maybe_size: Option<u32>,
	) -> TransactionOutcome<Result<u64, DispatchError>> {
		let batch_to_send: Vec<_> =
			ScheduledEgressRequests::<T, I>::mutate(|requests: &mut Vec<_>| {
				// Take up to batch_size requests to be sent
				let mut available_batch_size = maybe_size.unwrap_or(requests.len() as u32);

				// Filter out disabled assets
				requests
					.drain_filter(|request| {
						if available_batch_size > 0 &&
							!DisabledEgressAssets::<T, I>::contains_key(request.asset())
						{
							available_batch_size.saturating_reduce(1);
							true
						} else {
							false
						}
					})
					.collect()
			});

		if batch_to_send.is_empty() {
			return TransactionOutcome::Rollback(Err(DispatchError::Other(
				"Nothing to send, batch to send is empty, rolled back storage",
			)))
		}

		let mut fetch_params = vec![];
		let mut egress_params = vec![];
		let mut egress_ids = vec![];

		for request in batch_to_send {
			match request {
				FetchOrTransfer::<T::TargetChain>::Fetch { intent_id, asset } => {
					fetch_params.push(FetchAssetParams { intent_id, asset });
				},
				FetchOrTransfer::<T::TargetChain>::Transfer { asset, to, amount, egress_id } => {
					egress_ids.push(egress_id);
					egress_params.push(TransferAssetParams { asset, to, amount });
				},
			}
		}
		let fetch_batch_size = fetch_params.len() as u32;
		let egress_batch_size = egress_params.len() as u32;

		// Construct and send the transaction.
		#[allow(clippy::unit_arg)]
		match T::AllBatch::new_unsigned(fetch_params, egress_params) {
			Ok(egress_transaction) => {
				let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast(egress_transaction);
				Self::deposit_event(Event::<T, I>::BatchBroadcastRequested {
					broadcast_id,
					egress_ids,
				});
				TransactionOutcome::Commit(Ok(T::WeightInfo::egress_assets(
					fetch_batch_size.saturating_add(egress_batch_size),
				)))
			},
			Err(_) => TransactionOutcome::Rollback(Err(DispatchError::Other(
				"AllBatch Apicall creation failed, rolled back storage",
			))),
		}
	}

	/// Generate a new address for the user to deposit assets into.
	/// Generate an `intent_id` and a chain-specific address.
	fn generate_new_address(
		ingress_asset: TargetChainAsset<T, I>,
	) -> Result<(IntentId, TargetChainAccount<T, I>), DispatchError> {
		let next_intent_id = IntentIdCounter::<T, I>::get()
			.checked_add(1)
			.ok_or(Error::<T, I>::IntentIdsExhausted)?;
		let ingress_address =
			T::AddressDerivation::generate_address(ingress_asset, next_intent_id)?;
		IntentIdCounter::<T, I>::put(next_intent_id);
		Ok((next_intent_id, ingress_address))
	}

	/// Completes a single ingress request.
	fn do_single_ingress(
		ingress_address: TargetChainAccount<T, I>,
		asset: TargetChainAsset<T, I>,
		amount: AssetAmount,
		tx_id: <T::TargetChain as ChainCrypto>::TransactionId,
	) -> DispatchResult {
		let ingress = IntentIngressDetails::<T, I>::get(&ingress_address)
			.ok_or(Error::<T, I>::InvalidIntent)?;
		ensure!(ingress.ingress_asset == asset, Error::<T, I>::IngressMismatchWithIntent);

		// Ingress is called by witnessers, so asset/chain combination should always be valid.
		ScheduledEgressRequests::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Fetch {
			intent_id: ingress.intent_id,
			asset,
		});

		Self::deposit_event(Event::<T, I>::IngressFetchesScheduled {
			intent_id: ingress.intent_id,
			asset,
		});

		// NB: Don't take here. We should continue witnessing this address
		// even after an ingress to it has occurred.
		// https://github.com/chainflip-io/chainflip-eth-contracts/pull/226
		match IntentActions::<T, I>::get(&ingress_address).ok_or(Error::<T, I>::InvalidIntent)? {
			IntentAction::LiquidityProvision { lp_account } =>
				T::LpProvisioning::provision_account(&lp_account, asset.into(), amount)?,
			IntentAction::Swap {
				egress_address,
				egress_asset,
				relayer_id,
				relayer_commission_bps,
			} => T::SwapIntentHandler::on_swap_ingress(
				ingress_address.clone().into(),
				asset.into(),
				egress_asset,
				amount,
				egress_address,
				relayer_id,
				relayer_commission_bps,
			),
		};

		Self::deposit_event(Event::IngressCompleted { ingress_address, asset, amount, tx_id });
		Ok(())
	}
}

impl<T: Config<I>, I: 'static> EgressApi<T::TargetChain> for Pallet<T, I> {
	fn schedule_egress(
		asset: TargetChainAsset<T, I>,
		amount: AssetAmount,
		egress_address: TargetChainAccount<T, I>,
	) -> EgressId {
		let egress_counter = EgressIdCounter::<T, I>::get().saturating_add(1);
		let egress_id = (<T as Config<I>>::TargetChain::get(), egress_counter);
		ScheduledEgressRequests::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Transfer {
			asset,
			to: egress_address.clone(),
			amount,
			egress_id,
		});
		EgressIdCounter::<T, I>::put(egress_counter);
		Self::deposit_event(Event::<T, I>::EgressScheduled {
			id: egress_id,
			asset,
			amount,
			egress_address,
		});
		egress_id
	}
}

impl<T: Config<I>, I: 'static> IngressApi<T::TargetChain> for Pallet<T, I> {
	type AccountId = <T as frame_system::Config>::AccountId;
	// This should be callable by the LP pallet.
	fn register_liquidity_ingress_intent(
		lp_account: T::AccountId,
		ingress_asset: TargetChainAsset<T, I>,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		let (intent_id, ingress_address) = Self::generate_new_address(ingress_asset)?;

		// Generated address guarantees the right address type is returned.
		IntentIngressDetails::<T, I>::insert(
			&ingress_address,
			IngressDetails { intent_id, ingress_asset },
		);
		IntentActions::<T, I>::insert(
			&ingress_address,
			IntentAction::LiquidityProvision { lp_account },
		);

		Self::deposit_event(Event::StartWitnessing {
			ingress_address: ingress_address.clone(),
			ingress_asset,
		});

		Ok((intent_id, ingress_address.into()))
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

		// Generated address guarantees the right address type is returned.
		IntentIngressDetails::<T, I>::insert(
			&ingress_address,
			IngressDetails { intent_id, ingress_asset },
		);
		IntentActions::<T, I>::insert(
			&ingress_address,
			IntentAction::Swap { egress_address, egress_asset, relayer_commission_bps, relayer_id },
		);

		Self::deposit_event(Event::StartWitnessing {
			ingress_address: ingress_address.clone(),
			ingress_asset,
		});

		Ok((intent_id, ingress_address.into()))
	}
}
