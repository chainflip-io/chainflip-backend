#![cfg_attr(not(feature = "std"), no_std)]
#![feature(drain_filter)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;
use cf_traits::DepositChannel;
pub use weights::WeightInfo;

use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, ForeignChainAddress},
	AllBatch, AllBatchError, CcmDepositMetadata, Chain, ChainAbi, ChainCrypto, ExecutexSwapAndCall,
	FetchAssetParams, SwapOrigin, TransferAssetParams,
};
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, ChannelId, EgressCounter, EgressId, ForeignChain,
};
use cf_traits::{
	liquidity::LpBalanceApi, Broadcaster, CcmHandler, Chainflip, DepositApi, DepositHandler,
	EgressApi, GetBlockHeight, SwapDepositHandler,
};
use frame_support::{pallet_prelude::*, sp_runtime::DispatchError};
pub use pallet::*;
use sp_runtime::TransactionOutcome;
use sp_std::{vec, vec::Vec};

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum FetchOrTransfer<C: Chain> {
	Fetch {
		channel_id: ChannelId,
		asset: C::ChainAsset,
		deposit_address: C::ChainAccount,
	},
	Transfer {
		egress_id: EgressId,
		asset: C::ChainAsset,
		destination_address: C::ChainAccount,
		amount: C::ChainAmount,
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
	pub amount: C::ChainAmount,
	pub destination_address: C::ChainAccount,
	pub message: Vec<u8>,
	// The sender of the deposit transaction.
	pub source_address: ForeignChainAddress,
	// Where funds might be returned to if the message fails.
	pub cf_parameters: Vec<u8>,
}

impl<C: Chain> CrossChainMessage<C> {
	fn asset(&self) -> C::ChainAsset {
		self.asset
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
	pub(crate) type TargetChainAmount<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAmount;
	pub(crate) type TargetChainBlockNumber<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

	pub(crate) type DepositFetchIdOf<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::DepositFetchId;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct DepositWitness<C: Chain + ChainCrypto> {
		pub deposit_address: C::ChainAccount,
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub tx_id: <C as ChainCrypto>::TransactionInId,
	}

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct DepositChannelDetails<C: Chain, Channel: DepositChannel<C>> {
		pub deposit_channel: Channel,
		pub opened_at: C::ChainBlockNumber,
	}

	/// Determines the action to take when a deposit is made to a channel.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_id: AccountId,
			broker_commission_bps: BasisPoints,
		},
		LiquidityProvision {
			lp_account: AccountId,
		},
		CcmTransfer {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			message_metadata: CcmDepositMetadata,
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

		/// Generates deposit addresses.
		type AddressDerivation: AddressDerivationApi<Self::TargetChain>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		/// Pallet responsible for managing Liquidity Providers.
		type LpBalance: LpBalanceApi<AccountId = Self::AccountId>;

		/// For scheduling swaps.
		type SwapDepositHandler: SwapDepositHandler<AccountId = Self::AccountId>;

		/// Handler for Cross Chain Messages.
		type CcmHandler: CcmHandler;

		/// The type of the chain-native transaction.
		type ChainApiCall: AllBatch<Self::TargetChain> + ExecutexSwapAndCall<Self::TargetChain>;

		/// Get the latest block height of the target chain via Chain Tracking.
		type ChainTracking: GetBlockHeight<Self::TargetChain>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<
			Self::TargetChain,
			ApiCall = Self::ChainApiCall,
			Callback = <Self as Config<I>>::RuntimeCall,
		>;

		/// Provides callbacks for deposit lifecycle events.
		type DepositHandler: DepositHandler<Self::TargetChain>;

		/// Manages the chain specific deposit channel.
		type DepositChannel: Member
			+ Parameter
			+ DepositChannel<
				Self::TargetChain,
				Address = <<Self as Config<I>>::TargetChain as Chain>::ChainAccount,
				DepositFetchId = <<Self as Config<I>>::TargetChain as Chain>::DepositFetchId,
			> + Unpin;

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	/// Lookup table for addresses to correpsponding deposit channels.
	#[pallet::storage]
	pub type DepositChannelLookup<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T::TargetChain, T::DepositChannel>,
		OptionQuery,
	>;

	/// Stores the channel action against the address
	#[pallet::storage]
	pub type ChannelActions<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		ChannelAction<<T as frame_system::Config>::AccountId>,
		OptionQuery,
	>;

	/// Stores the latest channel id used to generate an address.
	#[pallet::storage]
	pub type ChannelIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChannelId, ValueQuery>;

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
	pub type DisabledEgressAssets<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, ()>;

	/// Stores address ready for use.
	#[pallet::storage]
	pub(crate) type DepositChannelPool<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, ChannelId, T::DepositChannel>;

	/// Defines the minimum amount of Deposit allowed for each asset.
	#[pallet::storage]
	pub type MinimumDeposit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, TargetChainAmount<T, I>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		StartWitnessing {
			deposit_address: TargetChainAccount<T, I>,
			source_asset: TargetChainAsset<T, I>,
			opened_at: TargetChainBlockNumber<T, I>,
		},
		StopWitnessing {
			deposit_address: TargetChainAccount<T, I>,
			source_asset: TargetChainAsset<T, I>,
		},
		DepositReceived {
			deposit_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			tx_id: <T::TargetChain as ChainCrypto>::TransactionInId,
		},
		AssetEgressStatusChanged {
			asset: TargetChainAsset<T, I>,
			disabled: bool,
		},
		EgressScheduled {
			id: EgressId,
			asset: TargetChainAsset<T, I>,
			amount: AssetAmount,
			destination_address: TargetChainAccount<T, I>,
		},
		CcmBroadcastRequested {
			broadcast_id: BroadcastId,
			egress_id: EgressId,
		},
		CcmEgressInvalid {
			egress_id: EgressId,
			error: DispatchError,
		},
		DepositFetchesScheduled {
			channel_id: ChannelId,
			asset: TargetChainAsset<T, I>,
		},
		BatchBroadcastRequested {
			broadcast_id: BroadcastId,
			egress_ids: Vec<EgressId>,
		},
		MinimumDepositSet {
			asset: TargetChainAsset<T, I>,
			minimum_deposit: TargetChainAmount<T, I>,
		},
		///The deposits is rejected because the amount is below the minimum allowed.
		DepositIgnored {
			deposit_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			tx_id: <T::TargetChain as ChainCrypto>::TransactionInId,
		},
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The deposit address is not valid. It may have expired or may never have been issued.
		InvalidDepositAddress,
		/// A deposit was made using the wrong asset.
		AssetMismatch,
		/// Channel ID has reached maximum
		ChannelIdsExhausted,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Take all scheduled Egress and send them out
		fn on_finalize(_n: BlockNumberFor<T>) {
			// Send all fetch/transfer requests as a batch. Revert storage if failed.
			if let Err(e) = with_transaction(|| Self::do_egress_scheduled_fetch_transfer()) {
				log::error!("Ingress-Egress failed to send BatchAll. Error: {e:?}");
			}

			// Egress all scheduled Cross chain messages
			Self::do_egress_scheduled_ccm();
		}
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Callback for when a signature is accepted by the chain.
		#[pallet::weight(T::WeightInfo::finalise_ingress(addresses.len() as u32))]
		pub fn finalise_ingress(
			origin: OriginFor<T>,
			addresses: Vec<(DepositFetchIdOf<T, I>, TargetChainAccount<T, I>)>,
		) -> DispatchResult {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;
			for (_, deposit_address) in addresses {
				if let Some(deposit_details) =
					DepositChannelLookup::<T, I>::get(deposit_address.clone())
				{
					DepositChannelLookup::<T, I>::insert(
						deposit_address,
						DepositChannelDetails {
							opened_at: deposit_details.opened_at,
							deposit_channel: deposit_details.deposit_channel.finalize(),
						},
					);
				} else {
					log::error!(
						"Deposit address {:?} not found in DepositChannelLookup",
						deposit_address
					);
				}
			}
			Ok(())
		}

		/// Sets if an asset is not allowed to be sent out of the chain via Egress.
		/// Requires Governance
		///
		/// ## Events
		///
		/// - [On update](Event::AssetEgressStatusChanged)
		#[pallet::weight(T::WeightInfo::disable_asset_egress())]
		pub fn enable_or_disable_egress(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			set_disabled: bool,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			let is_currently_disabled = DisabledEgressAssets::<T, I>::contains_key(asset);

			let do_disable = !is_currently_disabled && set_disabled;
			let do_enable = is_currently_disabled && !set_disabled;

			if do_disable {
				DisabledEgressAssets::<T, I>::insert(asset, ());
			} else if do_enable {
				DisabledEgressAssets::<T, I>::remove(asset);
			}

			if do_disable || do_enable {
				Self::deposit_event(Event::<T, I>::AssetEgressStatusChanged {
					asset,
					disabled: set_disabled,
				});
			}

			Ok(())
		}

		/// Called when funds have been deposited into the given address.
		///
		/// Requires `EnsureWitnessed` origin.
		#[pallet::weight(T::WeightInfo::process_single_deposit().saturating_mul(deposit_witnesses.len() as u64))]
		pub fn process_deposits(
			origin: OriginFor<T>,
			deposit_witnesses: Vec<DepositWitness<T::TargetChain>>,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			for DepositWitness { deposit_address, asset, amount, tx_id } in deposit_witnesses {
				Self::process_single_deposit(deposit_address, asset, amount, tx_id)?;
			}
			Ok(())
		}

		/// Sets the minimum deposit amount allowed for an asset.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_sucess](Event::MinimumDepositSet)
		#[pallet::weight(T::WeightInfo::set_minimum_deposit())]
		pub fn set_minimum_deposit(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			minimum_deposit: TargetChainAmount<T, I>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			MinimumDeposit::<T, I>::insert(asset, minimum_deposit);

			Self::deposit_event(Event::<T, I>::MinimumDepositSet { asset, minimum_deposit });
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// Take all scheduled egress requests and send them out in an `AllBatch` call.
	///
	/// Note: Egress transactions with Blacklisted assets are not sent, and kept in storage.
	fn do_egress_scheduled_fetch_transfer() -> TransactionOutcome<DispatchResult> {
		let batch_to_send: Vec<_> =
			ScheduledEgressFetchOrTransfer::<T, I>::mutate(|requests: &mut Vec<_>| {
				// Filter out disabled assets and requests that are not ready to be egressed.
				requests
					.drain_filter(|request| {
						!DisabledEgressAssets::<T, I>::contains_key(request.asset()) &&
							match request {
								FetchOrTransfer::Fetch {
									channel_id: _,
									asset: _,
									deposit_address,
								} => {
									if let Some(details) =
										DepositChannelLookup::<T, I>::get(deposit_address.clone())
									{
										let (updated_deposit_channel, skip) =
											details.clone().deposit_channel.skip_broadcast();
										DepositChannelLookup::<T, I>::insert(
											updated_deposit_channel.get_address(),
											DepositChannelDetails {
												opened_at: details.opened_at,
												deposit_channel: updated_deposit_channel,
											},
										);
										skip
									} else {
										log::error!("Deposit address {:?} not found in DepositChannelLookup", deposit_address);
										false
									}
								},
								FetchOrTransfer::Transfer { .. } => true,
							}
					})
					.collect()
			});

		// Returns Ok(()) if there's nothing to send.
		if batch_to_send.is_empty() {
			return TransactionOutcome::Commit(Ok(()))
		}

		let mut fetch_params = vec![];
		let mut egress_params = vec![];
		let mut egress_ids = vec![];
		let mut addresses = vec![];

		for request in batch_to_send {
			match request {
				FetchOrTransfer::<T::TargetChain>::Fetch {
					channel_id: _,
					asset,
					deposit_address,
				} => {
					if let Some(details) =
						DepositChannelLookup::<T, I>::get(deposit_address.clone())
					{
						fetch_params.push(FetchAssetParams {
							deposit_fetch_id: details.deposit_channel.get_deposit_fetch_id(),
							asset,
						});
						addresses.push((
							details.deposit_channel.get_deposit_fetch_id(),
							deposit_address.clone(),
						));
					} else {
						log::error!(
							"Deposit address {:?} not found in DepositChannelLookup",
							deposit_address
						);
					}
				},
				FetchOrTransfer::<T::TargetChain>::Transfer {
					asset,
					amount,
					destination_address,
					egress_id,
				} => {
					egress_ids.push(egress_id);
					egress_params.push(TransferAssetParams {
						asset,
						amount,
						to: destination_address,
					});
				},
			}
		}

		// Construct and send the transaction.
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
				TransactionOutcome::Commit(Ok(()))
			},
			Err(AllBatchError::NotRequired) => TransactionOutcome::Commit(Ok(())),
			Err(AllBatchError::Other) => TransactionOutcome::Rollback(Err(DispatchError::Other(
				"AllBatch ApiCall creation failed, rolled back storage",
			))),
		}
	}

	/// Send all scheduled Cross Chain Messages out to the target chain.
	///
	/// Blacklisted assets are not sent and will remain in storage.
	fn do_egress_scheduled_ccm() {
		let ccms_to_send: Vec<CrossChainMessage<T::TargetChain>> =
			ScheduledEgressCcm::<T, I>::mutate(|ccms: &mut Vec<_>| {
				// Filter out disabled assets, and take up to batch_size requests to be sent.
				ccms.drain_filter(|ccm| !DisabledEgressAssets::<T, I>::contains_key(ccm.asset()))
					.collect()
			});
		for ccm in ccms_to_send {
			match <T::ChainApiCall as ExecutexSwapAndCall<T::TargetChain>>::new_unsigned(
				ccm.egress_id,
				TransferAssetParams {
					asset: ccm.asset,
					amount: ccm.amount,
					to: ccm.destination_address,
				},
				ccm.source_address,
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
	}

	/// Completes a single deposit request.
	fn process_single_deposit(
		deposit_address: TargetChainAccount<T, I>,
		asset: TargetChainAsset<T, I>,
		amount: TargetChainAmount<T, I>,
		tx_id: <T::TargetChain as ChainCrypto>::TransactionInId,
	) -> DispatchResult {
		let deposit_channel_details = DepositChannelLookup::<T, I>::get(&deposit_address)
			.ok_or(Error::<T, I>::InvalidDepositAddress)?;

		let source_asset = deposit_channel_details.deposit_channel.get_asset();
		let channel_id = deposit_channel_details.deposit_channel.get_channel_id();

		ensure!(source_asset == asset, Error::<T, I>::AssetMismatch);

		if amount < MinimumDeposit::<T, I>::get(asset) {
			// If the amount is below the minimum allowed, the deposit is ignored.
			Self::deposit_event(Event::<T, I>::DepositIgnored {
				deposit_address,
				asset,
				amount,
				tx_id,
			});
			return Ok(())
		}

		ScheduledEgressFetchOrTransfer::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Fetch {
			channel_id,
			asset,
			deposit_address: deposit_address.clone(),
		});

		Self::deposit_event(Event::<T, I>::DepositFetchesScheduled { channel_id, asset });

		// NB: Don't take here. We should continue witnessing this address
		// even after an deposit to it has occurred.
		// https://github.com/chainflip-io/chainflip-eth-contracts/pull/226
		match ChannelActions::<T, I>::get(&deposit_address)
			.ok_or(Error::<T, I>::InvalidDepositAddress)?
		{
			ChannelAction::LiquidityProvision { lp_account } =>
				T::LpBalance::try_credit_account(&lp_account, asset.into(), amount.into())?,
			ChannelAction::Swap {
				destination_address,
				destination_asset,
				broker_id,
				broker_commission_bps,
			} => T::SwapDepositHandler::schedule_swap_from_channel(
				deposit_address.clone().into(),
				asset.into(),
				destination_asset,
				amount.into(),
				destination_address,
				broker_id,
				broker_commission_bps,
				channel_id,
			),
			ChannelAction::CcmTransfer {
				destination_asset,
				destination_address,
				message_metadata,
			} => T::CcmHandler::on_ccm_deposit(
				asset.into(),
				amount.into(),
				destination_asset,
				destination_address,
				message_metadata,
				SwapOrigin::DepositChannel {
					deposit_address: T::AddressConverter::to_encoded_address(
						deposit_address.clone().into(),
					),
					channel_id,
				},
			),
		};

		T::DepositHandler::on_deposit_made(tx_id.clone(), amount, deposit_address.clone(), asset);

		Self::deposit_event(Event::DepositReceived { deposit_address, asset, amount, tx_id });
		Ok(())
	}

	/// Opens a channel for the given asset and registers it with the given action.
	/// Emits the `StartWitnessing` event so CFEs can start watching for deposits to the address.
	///
	/// May re-use an existing deposit address, depending on chain configuration.
	fn open_channel(
		source_asset: TargetChainAsset<T, I>,
		channel_action: ChannelAction<T::AccountId>,
	) -> Result<(ChannelId, TargetChainAccount<T, I>), DispatchError> {
		// We have an address available, so we can just use it.

		let (deposit_channel, channel_id) =
			if let Some((channel_id, address)) = DepositChannelPool::<T, I>::drain().next() {
				(address, channel_id)
			} else {
				let next_channel_id = ChannelIdCounter::<T, I>::get()
					.checked_add(1)
					.ok_or(Error::<T, I>::ChannelIdsExhausted)?;
				ChannelIdCounter::<T, I>::put(next_channel_id);
				(T::DepositChannel::new(next_channel_id, source_asset)?, next_channel_id)
			};

		let new_address = deposit_channel.get_address();

		ChannelActions::<T, I>::insert(&new_address, channel_action);
		T::DepositHandler::on_channel_opened(new_address.clone(), channel_id)?;

		let opened_at = T::ChainTracking::get_block_height();

		Self::deposit_event(Event::StartWitnessing {
			deposit_address: deposit_channel.get_address(),
			source_asset,
			opened_at,
		});

		DepositChannelLookup::<T, I>::insert(
			new_address.clone(),
			DepositChannelDetails { deposit_channel, opened_at },
		);

		Ok((channel_id, new_address))
	}

	fn close_channel(channel_id: ChannelId, address: TargetChainAccount<T, I>) {
		ChannelActions::<T, I>::remove(&address);
		if let Some(deposit_channel_details) = DepositChannelLookup::<T, I>::get(&address) {
			if deposit_channel_details.deposit_channel.maybe_recycle() {
				DepositChannelPool::<T, I>::insert(
					channel_id,
					deposit_channel_details.deposit_channel.clone(),
				);
			}
			Self::deposit_event(Event::<T, I>::StopWitnessing {
				deposit_address: address,
				source_asset: deposit_channel_details.deposit_channel.get_asset(),
			});
		} else {
			log::error!("This should not error since we create the DepositChannelLookup at the time of opening the channel")
		}
	}
}

impl<T: Config<I>, I: 'static> EgressApi<T::TargetChain> for Pallet<T, I> {
	fn schedule_egress(
		asset: TargetChainAsset<T, I>,
		amount: TargetChainAmount<T, I>,
		destination_address: TargetChainAccount<T, I>,
		maybe_message: Option<CcmDepositMetadata>,
	) -> EgressId {
		let egress_counter = EgressIdCounter::<T, I>::mutate(|id| {
			*id = id.saturating_add(1);
			*id
		});
		let egress_id = (<T as Config<I>>::TargetChain::get(), egress_counter);
		match maybe_message {
			Some(CcmDepositMetadata { message, cf_parameters, source_address, .. }) =>
				ScheduledEgressCcm::<T, I>::append(CrossChainMessage {
					egress_id,
					asset,
					amount,
					destination_address: destination_address.clone(),
					message,
					cf_parameters,
					source_address,
				}),
			None => ScheduledEgressFetchOrTransfer::<T, I>::append(FetchOrTransfer::<
				T::TargetChain,
			>::Transfer {
				asset,
				destination_address: destination_address.clone(),
				amount,
				egress_id,
			}),
		}

		Self::deposit_event(Event::<T, I>::EgressScheduled {
			id: egress_id,
			asset,
			amount: amount.into(),
			destination_address,
		});

		egress_id
	}
}

impl<T: Config<I>, I: 'static> DepositApi<T::TargetChain> for Pallet<T, I> {
	type AccountId = <T as frame_system::Config>::AccountId;
	// This should be callable by the LP pallet.
	fn request_liquidity_deposit_address(
		lp_account: T::AccountId,
		source_asset: TargetChainAsset<T, I>,
	) -> Result<(ChannelId, ForeignChainAddress), DispatchError> {
		let (channel_id, deposit_address) =
			Self::open_channel(source_asset, ChannelAction::LiquidityProvision { lp_account })?;

		Ok((channel_id, deposit_address.into()))
	}

	// This should only be callable by the broker.
	fn request_swap_deposit_address(
		source_asset: TargetChainAsset<T, I>,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		broker_commission_bps: BasisPoints,
		broker_id: T::AccountId,
		message_metadata: Option<CcmDepositMetadata>,
	) -> Result<(ChannelId, ForeignChainAddress), DispatchError> {
		let (channel_id, deposit_address) = Self::open_channel(
			source_asset,
			match message_metadata {
				Some(msg) => ChannelAction::CcmTransfer {
					destination_asset,
					destination_address,
					message_metadata: msg,
				},
				None => ChannelAction::Swap {
					destination_asset,
					destination_address,
					broker_commission_bps,
					broker_id,
				},
			},
		)?;

		Ok((channel_id, deposit_address.into()))
	}

	// Note: we expect that the mapping from any instantiable pallet to the instance of this pallet
	// is matching to the right chain. Because of that we can ignore the chain parameter.
	fn expire_channel(channel_id: ChannelId, address: TargetChainAccount<T, I>) {
		Self::close_channel(channel_id, address);
	}
}
