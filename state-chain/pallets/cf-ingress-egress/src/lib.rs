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
use sp_runtime::traits::BlockNumberProvider;

use cf_chains::{address::ForeignChainAddress, CcmIngressMetadata, IngressIdConstructor};

use cf_chains::{
	AllBatch, Chain, ChainAbi, ChainCrypto, ExecutexSwapAndCall, FetchAssetParams,
	TransferAssetParams,
};
use cf_primitives::{Asset, AssetAmount, IntentId};
use cf_traits::{
	liquidity::LpProvisioningApi, AddressDerivationApi, Broadcaster, CcmHandler, Chainflip,
	EgressApi, IngressApi, IngressHandler, SwapIntentHandler,
};
use frame_support::{pallet_prelude::*, sp_runtime::DispatchError};
pub use pallet::*;
use sp_runtime::{Saturating, TransactionOutcome};
pub use sp_std::{vec, vec::Vec};

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum FetchOrTransfer<C: Chain> {
	Fetch {
		intent_id: IntentId,
		asset: C::ChainAsset,
	},
	Transfer {
		egress_id: EgressId,
		asset: C::ChainAsset,
		egress_address: C::ChainAccount,
		amount: AssetAmount,
	},
}

impl<C: Chain> FetchOrTransfer<C> {
	fn asset(&self) -> &C::ChainAsset {
		match self {
			FetchOrTransfer::Fetch { asset, .. } => asset,
			FetchOrTransfer::Transfer { asset, .. } => asset,
		}
	}
}

/// Cross-chain messaging requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub(crate) struct CrossChainMessage<C: Chain> {
	pub egress_id: EgressId,
	pub asset: C::ChainAsset,
	pub amount: AssetAmount,
	pub egress_address: C::ChainAccount,
	pub message: Vec<u8>,
	pub refund_address: ForeignChainAddress,
}

impl<C: Chain> CrossChainMessage<C> {
	fn asset(&self) -> C::ChainAsset {
		self.asset
	}
}

#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum DeploymentStatus {
	Deployed,   // an address that has already been deployed
	Undeployed, // an address that has not been deployed yet
}

impl Default for DeploymentStatus {
	fn default() -> Self {
		Self::Undeployed
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::ExecutexSwapAndCall;
	use cf_primitives::BroadcastId;
	use core::marker::PhantomData;
	use sp_std::vec::Vec;

	use frame_support::{
		pallet_prelude::{OptionQuery, ValueQuery},
		storage::with_transaction,
		traits::{EnsureOrigin, IsType},
	};
	use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};

	pub(crate) type TargetChainAsset<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAsset;
	pub(crate) type TargetChainAccount<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainAccount;

	pub(crate) type IngressFetchIdOf<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::IngressFetchId;

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
		CcmTransfer {
			egress_asset: Asset,
			egress_address: ForeignChainAddress,
			message_metadata: CcmIngressMetadata,
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

		/// The pallet dispatches calls, so it depends on the runtime's aggregated Call type.
		type RuntimeCall: From<Call<Self, I>> + IsType<<Self as frame_system::Config>::RuntimeCall>;

		/// Marks which chain this pallet is interacting with.
		type TargetChain: ChainAbi + Get<ForeignChain>;

		/// Generates ingress addresses.
		type AddressDerivation: AddressDerivationApi<Self::TargetChain>;

		/// Pallet responsible for managing Liquidity Providers.
		type LpProvisioning: LpProvisioningApi<AccountId = Self::AccountId>;

		/// For scheduling swaps.
		type SwapIntentHandler: SwapIntentHandler<AccountId = Self::AccountId>;

		/// Handler for Cross Chain Messages.
		type CcmHandler: CcmHandler;

		/// The type of the chain-native transaction.
		type ChainApiCall: AllBatch<Self::TargetChain> + ExecutexSwapAndCall<Self::TargetChain>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<
			Self::TargetChain,
			ApiCall = Self::ChainApiCall,
			Callback = <Self as Config<I>>::RuntimeCall,
		>;

		/// Governance origin to manage allowed assets
		type EnsureGovernance: EnsureOrigin<Self::RuntimeOrigin>;

		/// Time to life for an intent in blocks.
		#[pallet::constant]
		type IntentTTL: Get<Self::BlockNumber>;

		/// Ingress Handler for performing action items on ingress needed elsewhere
		type IngressHandler: IngressHandler<Self::TargetChain>;

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
	pub(crate) type ScheduledEgressFetchOrTransfer<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<FetchOrTransfer<T::TargetChain>>, ValueQuery>;

	/// Scheduled cross chain messages for the Ethereum chain.
	#[pallet::storage]
	pub(crate) type ScheduledEgressCcm<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<CrossChainMessage<T::TargetChain>>, ValueQuery>;

	/// Stores the list of assets that are not allowed to be egressed.
	#[pallet::storage]
	pub(crate) type DisabledEgressAssets<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, ()>;

	/// Stores a pool of addresses that is available for use together with the intent id.
	#[pallet::storage]
	pub(crate) type AddressPool<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, IntentId, TargetChainAccount<T, I>>;

	/// Stores the status of an address.
	#[pallet::storage]
	pub(crate) type AddressStatus<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, TargetChainAccount<T, I>, DeploymentStatus, ValueQuery>;

	/// Stores a block for when an intent will expire against the intent infos.
	#[pallet::storage]
	pub(crate) type IntentExpiries<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, T::BlockNumber, Vec<(IntentId, TargetChainAccount<T, I>)>>;

	/// Map of intent id to the ingress id.
	#[pallet::storage]
	pub(crate) type FetchParamDetails<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, IntentId, (IngressFetchIdOf<T, I>, TargetChainAccount<T, I>)>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		StartWitnessing {
			ingress_address: TargetChainAccount<T, I>,
			ingress_asset: TargetChainAsset<T, I>,
		},
		StopWitnessing {
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
		CcmBroadcastRequested {
			broadcast_id: BroadcastId,
			egress_id: EgressId,
		},
		CcmEgressInvalid {
			egress_id: EgressId,
			error: DispatchError,
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
			if remaining_weight.all_lte(T::WeightInfo::egress_assets(1u32)) ||
				(ScheduledEgressFetchOrTransfer::<T, I>::decode_len() == Some(0) &&
					ScheduledEgressCcm::<T, I>::decode_len() == Some(0))
			{
				return T::WeightInfo::on_idle_with_nothing_to_send()
			}

			// Send fetch/transfer requests as a batch
			let mut weights_left = remaining_weight;
			let single_request_cost = T::WeightInfo::egress_assets(1u32)
				.saturating_sub(T::WeightInfo::egress_assets(0u32));
			let request_count = weights_left
				.saturating_sub(T::WeightInfo::egress_assets(0u32))
				.ref_time()
				.saturating_div(single_request_cost.ref_time()) as u32;

			weights_left = weights_left.saturating_sub(
				with_transaction(|| Self::do_egress_scheduled_fetch_transfer(Some(request_count)))
					.unwrap_or_else(|_| T::WeightInfo::egress_assets(0)),
			);

			// Send as many Cross chain messages as the weights allow.
			let single_ccm_cost =
				T::WeightInfo::egress_ccm(1u32).saturating_sub(T::WeightInfo::egress_ccm(0u32));
			let ccm_count = weights_left
				.saturating_sub(T::WeightInfo::egress_ccm(0u32))
				.ref_time()
				.saturating_div(single_ccm_cost.ref_time()) as u32;
			weights_left =
				weights_left.saturating_sub(Self::do_egress_scheduled_ccm(Some(ccm_count)));

			remaining_weight.saturating_sub(weights_left)
		}

		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let mut total_weight: Weight = Weight::zero();
			if let Some(expired) = IntentExpiries::<T, I>::take(n) {
				for (intent_id, address) in expired.clone() {
					IntentActions::<T, I>::remove(&address);
					if AddressStatus::<T, I>::get(&address) == DeploymentStatus::Deployed {
						AddressPool::<T, I>::insert(intent_id, address.clone());
					}
					if let Some(intent_ingress_details) =
						IntentIngressDetails::<T, I>::take(&address)
					{
						Self::deposit_event(Event::<T, I>::StopWitnessing {
							ingress_address: address.clone(),
							ingress_asset: intent_ingress_details.ingress_asset,
						});
					}

					total_weight = total_weight
						.saturating_add(T::WeightInfo::on_initialize(expired.len() as u32));
				}
			}
			total_weight.saturating_add(T::WeightInfo::on_initialize_has_no_expired())
		}

		fn integrity_test() {
			// Ensures the weights are benchmarked correctly.
			assert!(T::WeightInfo::egress_assets(1).all_gte(T::WeightInfo::egress_assets(0)));
			assert!(T::WeightInfo::do_single_ingress().all_gte(Weight::zero()));
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Callback for when a signature is accepted by the chain.
		#[pallet::weight(T::WeightInfo::finalise_ingress(addresses.len() as u32))]
		pub fn finalise_ingress(
			origin: OriginFor<T>,
			addresses: Vec<(IntentId, TargetChainAccount<T, I>)>,
		) -> DispatchResult {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;
			for (intent_id, address) in addresses {
				IntentActions::<T, I>::remove(&address);
				AddressPool::<T, I>::insert(intent_id, address.clone());
				AddressStatus::<T, I>::insert(address.clone(), DeploymentStatus::Deployed);
				if let Some(intent_ingress_details) = IntentIngressDetails::<T, I>::take(&address) {
					Self::deposit_event(Event::<T, I>::StopWitnessing {
						ingress_address: address.clone(),
						ingress_asset: intent_ingress_details.ingress_asset,
					});
				}
			}
			Ok(())
		}
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
		pub fn egress_scheduled_fetch_transfer(
			origin: OriginFor<T>,
			maybe_size: Option<u32>,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			with_transaction(|| Self::do_egress_scheduled_fetch_transfer(maybe_size))?;
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

		/// Send up to `maybe_size` number of scheduled Cross chain messages out.
		/// If None is set for `maybe_size`, send all scheduled CCMs.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_sucessful_ccm](Event::CcmBroadcastRequested)
		/// - [on_failed_ccm](Event::CcmEgressInvalid)
		#[pallet::weight(0)]
		pub fn egress_scheduled_ccms(
			origin: OriginFor<T>,
			maybe_size: Option<u32>,
		) -> DispatchResult {
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			let _ = Self::do_egress_scheduled_ccm(maybe_size);
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Take up to `maybe_size` number of scheduled requests for the Ethereum chain and send them
	/// out in an `AllBatch` call. If `maybe_size` is `None`, send all scheduled transactions.
	///
	/// Returns the actual amount of weights used.
	///
	/// Egress transactions with Blacklisted assets are not sent, and kept in storage.
	#[allow(clippy::type_complexity)]
	fn do_egress_scheduled_fetch_transfer(
		maybe_size: Option<u32>,
	) -> TransactionOutcome<Result<Weight, DispatchError>> {
		let batch_to_send: Vec<_> =
			ScheduledEgressFetchOrTransfer::<T, I>::mutate(|requests: &mut Vec<_>| {
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
		let mut addresses = vec![];

		for request in batch_to_send {
			match request {
				FetchOrTransfer::<T::TargetChain>::Fetch { intent_id, asset } => {
					let (ingress_id, ingress_address) = FetchParamDetails::<T, I>::take(intent_id)
						.expect("to have fetch param details available");
					fetch_params.push(FetchAssetParams { ingress_fetch_id: ingress_id, asset });
					addresses.push((intent_id, ingress_address.clone()));
				},
				FetchOrTransfer::<T::TargetChain>::Transfer {
					asset,
					amount,
					egress_address,
					egress_id,
				} => {
					egress_ids.push(egress_id);
					egress_params.push(TransferAssetParams { asset, amount, to: egress_address });
				},
			}
		}
		let fetch_batch_size = fetch_params.len() as u32;
		let egress_batch_size = egress_params.len() as u32;

		// Construct and send the transaction.
		#[allow(clippy::unit_arg)]
		match <T::ChainApiCall as AllBatch<T::TargetChain>>::new_unsigned(
			fetch_params,
			egress_params,
		) {
			Ok(egress_transaction) => {
				let (broadcast_id, _) = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
					egress_transaction,
					Call::finalise_ingress { addresses }.into(),
				);
				Self::deposit_event(Event::<T, I>::BatchBroadcastRequested {
					broadcast_id,
					egress_ids,
				});
				TransactionOutcome::Commit(Ok(T::WeightInfo::egress_assets(
					fetch_batch_size.saturating_add(egress_batch_size),
				)))
			},
			Err(_) => TransactionOutcome::Rollback(Err(DispatchError::Other(
				"AllBatch ApiCall creation failed, rolled back storage",
			))),
		}
	}

	/// Send as many as `maybe_size` numer of scheduled Cross Chain Messages out to the target
	/// chain. If `maybe_size` is None, then send all scheduled Cross Chain Messages.
	///
	/// Returns the actual weight used to send the transactions.
	///
	/// Blacklisted assets are not sent and will remain in storage.
	#[allow(clippy::type_complexity)]
	fn do_egress_scheduled_ccm(maybe_size: Option<u32>) -> Weight {
		let ccm_to_send: Vec<_> = ScheduledEgressCcm::<T, I>::mutate(|ccms: &mut Vec<_>| {
			// Take up to batch_size requests to be sent
			let mut target_size = maybe_size.unwrap_or(ccms.len() as u32);

			// Filter out disabled assets
			ccms.drain_filter(|ccm| {
				if target_size > 0 && !DisabledEgressAssets::<T, I>::contains_key(ccm.asset()) {
					target_size.saturating_reduce(1);
					true
				} else {
					false
				}
			})
			.collect()
		});

		if ccm_to_send.is_empty() {
			return T::WeightInfo::egress_ccm(0u32)
		}

		let ccms_sent = ccm_to_send.len() as u32;
		for ccm in ccm_to_send {
			match <T::ChainApiCall as ExecutexSwapAndCall<T::TargetChain>>::new_unsigned(
				ccm.egress_id,
				TransferAssetParams {
					asset: ccm.asset,
					amount: ccm.amount,
					to: ccm.egress_address,
				},
				ccm.refund_address,
				ccm.message,
			) {
				Ok(api_call) => {
					let (broadcast_id, _) = T::Broadcaster::threshold_sign_and_broadcast(api_call);
					Self::deposit_event(Event::<T, I>::CcmBroadcastRequested {
						broadcast_id,
						egress_id: ccm.egress_id,
					});
				},
				Err(error) => Self::deposit_event(Event::<T, I>::CcmEgressInvalid {
					egress_id: ccm.egress_id,
					error,
				}),
			};
		}

		T::WeightInfo::egress_ccm(ccms_sent)
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
		ScheduledEgressFetchOrTransfer::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Fetch {
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
			IntentAction::CcmTransfer { egress_asset, egress_address, message_metadata } =>
				T::CcmHandler::on_ccm_ingress(
					asset.into(),
					amount,
					egress_asset,
					egress_address,
					message_metadata,
				)?,
		};

		T::IngressHandler::handle_ingress(
			tx_id.clone(),
			amount.into(),
			ingress_address.clone(),
			asset,
		);

		Self::deposit_event(Event::IngressCompleted { ingress_address, asset, amount, tx_id });
		Ok(())
	}

	/// Create a new intent address for the given asset and registers it with the given action.
	fn register_ingress_intent(
		ingress_asset: TargetChainAsset<T, I>,
		intent_action: IntentAction<T::AccountId>,
	) -> Result<(IntentId, TargetChainAccount<T, I>), DispatchError> {
		let intent_ttl = frame_system::Pallet::<T>::current_block_number() + T::IntentTTL::get();
		// We have an address available, so we can just use it.

		let (address, intent_id) = if let Some((intent_id, address)) =
			AddressPool::<T, I>::drain().next()
		{
			FetchParamDetails::<T, I>::insert(
				intent_id,
				(IngressFetchIdOf::<T, I>::deployed(intent_id, address.clone()), address.clone()),
			);
			IntentExpiries::<T, I>::append(intent_ttl, (intent_id, address.clone()));
			(address, intent_id)
		} else {
			let next_intent_id = IntentIdCounter::<T, I>::get()
				.checked_add(1)
				.ok_or(Error::<T, I>::IntentIdsExhausted)?;
			let new_address: TargetChainAccount<T, I> =
				T::AddressDerivation::generate_address(ingress_asset, next_intent_id)?;
			AddressStatus::<T, I>::insert(new_address.clone(), DeploymentStatus::Undeployed);
			FetchParamDetails::<T, I>::insert(
				next_intent_id,
				(
					IngressFetchIdOf::<T, I>::undeployed(next_intent_id, new_address.clone()),
					new_address.clone(),
				),
			);
			IntentExpiries::<T, I>::append(intent_ttl, (next_intent_id, new_address.clone()));
			IntentIdCounter::<T, I>::put(next_intent_id);
			(new_address, next_intent_id)
		};

		IntentIngressDetails::<T, I>::insert(&address, IngressDetails { intent_id, ingress_asset });
		IntentActions::<T, I>::insert(&address, intent_action);
		Ok((intent_id, address))
	}
}

impl<T: Config<I>, I: 'static> EgressApi<T::TargetChain> for Pallet<T, I> {
	fn schedule_egress(
		asset: TargetChainAsset<T, I>,
		amount: AssetAmount,
		egress_address: TargetChainAccount<T, I>,
		maybe_message: Option<CcmIngressMetadata>,
	) -> EgressId {
		let egress_counter = EgressIdCounter::<T, I>::get().saturating_add(1);
		let egress_id = (<T as Config<I>>::TargetChain::get(), egress_counter);
		match maybe_message {
			Some(message) => ScheduledEgressCcm::<T, I>::append(CrossChainMessage {
				egress_id,
				asset,
				amount,
				egress_address: egress_address.clone(),
				message: message.message,
				refund_address: message.refund_address,
			}),
			None => ScheduledEgressFetchOrTransfer::<T, I>::append(FetchOrTransfer::<
				T::TargetChain,
			>::Transfer {
				asset,
				egress_address: egress_address.clone(),
				amount,
				egress_id,
			}),
		}

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
		let (intent_id, ingress_address) = Self::register_ingress_intent(
			ingress_asset,
			IntentAction::LiquidityProvision { lp_account },
		)?;

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
		message_metadata: Option<CcmIngressMetadata>,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		let (intent_id, ingress_address) = Self::register_ingress_intent(
			ingress_asset,
			match message_metadata {
				Some(msg) => IntentAction::CcmTransfer {
					egress_asset,
					egress_address,
					message_metadata: msg,
				},
				None => IntentAction::Swap {
					egress_asset,
					egress_address,
					relayer_commission_bps,
					relayer_id,
				},
			},
		)?;

		Self::deposit_event(Event::StartWitnessing {
			ingress_address: ingress_address.clone(),
			ingress_asset,
		});

		Ok((intent_id, ingress_address.into()))
	}
}
