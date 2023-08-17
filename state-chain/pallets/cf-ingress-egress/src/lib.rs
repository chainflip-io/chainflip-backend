#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extract_if)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;
pub use weights::WeightInfo;

use cf_chains::{
	address::{AddressConverter, AddressDerivationApi, IntoForeignChainAddress},
	AllBatch, AllBatchError, CcmChannelMetadata, CcmDepositMetadata, Chain, ChainAbi,
	ChannelLifecycleHooks, DepositChannel, ExecutexSwapAndCall, FetchAssetParams,
	ForeignChainAddress, SwapOrigin, TransferAssetParams,
};
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, ChannelId, EgressCounter, EgressId, ForeignChain,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	liquidity::LpBalanceApi, Broadcaster, CcmHandler, Chainflip, DepositApi, DepositHandler,
	EgressApi, GetBlockHeight, SwapDepositHandler,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{DispatchError, TransactionOutcome},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::{vec, vec::Vec};

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum FetchOrTransfer<C: Chain> {
	Fetch {
		asset: C::ChainAsset,
		deposit_address: C::ChainAccount,
		deposit_fetch_id: Option<C::DepositFetchId>,
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
	pub source_chain: ForeignChain,
	pub source_address: Option<ForeignChainAddress>,
	// Where funds might be returned to if the message fails.
	pub cf_parameters: Vec<u8>,
}

impl<C: Chain> CrossChainMessage<C> {
	fn asset(&self) -> C::ChainAsset {
		self.asset
	}
}

#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub struct VaultTransfer<C: Chain> {
	asset: C::ChainAsset,
	amount: C::ChainAmount,
	destination_address: C::ChainAccount,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::ExecutexSwapAndCall;
	use cf_primitives::BroadcastId;
	use core::marker::PhantomData;
	use frame_support::{
		storage::with_transaction,
		traits::{EnsureOrigin, IsType},
	};
	use sp_std::vec::Vec;

	pub(crate) type TargetChainAsset<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAsset;
	pub(crate) type TargetChainAccount<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainAccount;
	pub(crate) type TargetChainAmount<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAmount;
	pub(crate) type TargetChainBlockNumber<T, I> =
		<<T as Config<I>>::TargetChain as Chain>::ChainBlockNumber;

	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct DepositWitness<C: Chain> {
		pub deposit_address: C::ChainAccount,
		pub asset: C::ChainAsset,
		pub amount: C::ChainAmount,
		pub deposit_details: C::DepositDetails,
	}

	#[derive(
		CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: <T::TargetChain as Chain>::ChainBlockNumber,
		/// The block number at which the deposit channel will be closed, expressed as a
		/// Chainflip-native block number.
		// TODO: We should consider changing this to also be an external block number and expire
		// based on external block numbers. See PRO-689.
		pub expires_at: BlockNumberFor<T>,
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
			channel_metadata: CcmChannelMetadata,
		},
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
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

		/// Benchmark weights
		type WeightInfo: WeightInfo;
	}

	/// Lookup table for addresses to correpsponding deposit channels.
	#[pallet::storage]
	pub type DepositChannelLookup<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;

	/// Stores the channel action against the address
	#[pallet::storage]
	pub type ChannelActions<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		ChannelAction<T::AccountId>,
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
		StorageMap<_, Twox64Concat, ChannelId, DepositChannel<T::TargetChain>>;

	/// Defines the minimum amount of Deposit allowed for each asset.
	#[pallet::storage]
	pub type MinimumDeposit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, TargetChainAmount<T, I>, ValueQuery>;

	/// Stores any failed transfers by the Vault contract.
	/// Without dealing with the underlying reason for the failure, retrying is unlike to succeed.
	/// Therefore these calls are stored here, until we can react to the reason for failure and
	/// respond appropriately.
	#[pallet::storage]
	pub type FailedVaultTransfers<T: Config<I>, I: 'static = ()> =
		StorageValue<_, Vec<VaultTransfer<T::TargetChain>>, ValueQuery>;

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
			deposit_details: <T::TargetChain as Chain>::DepositDetails,
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
			deposit_details: <T::TargetChain as Chain>::DepositDetails,
		},
		VaultTransferFailed {
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			destination_address: TargetChainAccount<T, I>,
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
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::finalise_ingress(addresses.len() as u32))]
		pub fn finalise_ingress(
			origin: OriginFor<T>,
			addresses: Vec<TargetChainAccount<T, I>>,
		) -> DispatchResult {
			T::EnsureWitnessedAtCurrentEpoch::ensure_origin(origin)?;
			for deposit_address in addresses {
				if let Some(mut deposit_details) =
					DepositChannelLookup::<T, I>::get(&deposit_address)
				{
					if deposit_details.deposit_channel.on_fetch_completed() {
						DepositChannelLookup::<T, I>::insert(&deposit_address, deposit_details);
					}
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
		#[pallet::call_index(1)]
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
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::process_single_deposit().saturating_mul(deposit_witnesses.len() as u64))]
		pub fn process_deposits(
			origin: OriginFor<T>,
			deposit_witnesses: Vec<DepositWitness<T::TargetChain>>,
			block_height: <T::TargetChain as Chain>::ChainBlockNumber,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			for DepositWitness { deposit_address, asset, amount, deposit_details } in
				deposit_witnesses
			{
				Self::process_single_deposit(
					deposit_address,
					asset,
					amount,
					deposit_details,
					block_height,
				)?;
			}
			Ok(())
		}

		/// Sets the minimum deposit amount allowed for an asset.
		/// Requires governance
		///
		/// ## Events
		///
		/// - [on_success](Event::MinimumDepositSet)
		#[pallet::call_index(3)]
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

		/// Stores information on failed Vault transfer.
		/// Requires Witness origin.
		///
		/// ## Events
		///
		/// - [on_success](Event::VaultTransferFailed)
		#[pallet::weight(T::WeightInfo::vault_transfer_failed())]
		#[pallet::call_index(4)]
		pub fn vault_transfer_failed(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			destination_address: TargetChainAccount<T, I>,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			FailedVaultTransfers::<T, I>::append(VaultTransfer {
				asset,
				amount,
				destination_address: destination_address.clone(),
			});

			Self::deposit_event(Event::<T, I>::VaultTransferFailed {
				asset,
				amount,
				destination_address,
			});
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
					.extract_if(|request| {
						!DisabledEgressAssets::<T, I>::contains_key(request.asset()) &&
							match request {
								FetchOrTransfer::Fetch {
									deposit_address,
									deposit_fetch_id,
									..
								} =>
									if let Some(mut details) =
										DepositChannelLookup::<T, I>::get(&*deposit_address)
									{
										if details.deposit_channel.can_fetch() {
											deposit_fetch_id
												.replace(details.deposit_channel.fetch_id());
											if details.deposit_channel.on_fetch_scheduled() {
												DepositChannelLookup::<T, I>::insert(
													deposit_address,
													details,
												);
											}
											true
										} else {
											false
										}
									} else {
										log::error!("Deposit address {:?} not found in DepositChannelLookup", deposit_address);
										false
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
		let mut transfer_params = vec![];
		let mut egress_ids = vec![];
		let mut addresses = vec![];

		for request in batch_to_send {
			match request {
				FetchOrTransfer::<T::TargetChain>::Fetch {
					asset,
					deposit_address,
					deposit_fetch_id,
				} => {
					fetch_params.push(FetchAssetParams {
						deposit_fetch_id: deposit_fetch_id.expect("Checked in extract_if"),
						asset,
					});
					addresses.push(deposit_address.clone());
				},
				FetchOrTransfer::<T::TargetChain>::Transfer {
					asset,
					amount,
					destination_address,
					egress_id,
				} => {
					egress_ids.push(egress_id);
					transfer_params.push(TransferAssetParams {
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
			transfer_params,
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
				ccms.extract_if(|ccm| !DisabledEgressAssets::<T, I>::contains_key(ccm.asset()))
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
				ccm.source_chain,
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
		deposit_details: <T::TargetChain as Chain>::DepositDetails,
		block_height: <T::TargetChain as Chain>::ChainBlockNumber,
	) -> DispatchResult {
		let deposit_channel_details = DepositChannelLookup::<T, I>::get(&deposit_address)
			.ok_or(Error::<T, I>::InvalidDepositAddress)?;

		ensure!(
			deposit_channel_details.deposit_channel.asset == asset,
			Error::<T, I>::AssetMismatch
		);

		if amount < MinimumDeposit::<T, I>::get(asset) {
			// If the amount is below the minimum allowed, the deposit is ignored.
			Self::deposit_event(Event::<T, I>::DepositIgnored {
				deposit_address,
				asset,
				amount,
				deposit_details,
			});
			return Ok(())
		}

		ScheduledEgressFetchOrTransfer::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Fetch {
			asset,
			deposit_address: deposit_address.clone(),
			deposit_fetch_id: None,
		});

		let channel_id = deposit_channel_details.deposit_channel.channel_id;
		Self::deposit_event(Event::<T, I>::DepositFetchesScheduled { channel_id, asset });

		// NB: Don't take here. We should continue witnessing this address
		// even after an deposit to it has occurred.
		// https://github.com/chainflip-io/chainflip-eth-contracts/pull/226
		match ChannelActions::<T, I>::get(&deposit_address)
			.ok_or(Error::<T, I>::InvalidDepositAddress)?
		{
			ChannelAction::LiquidityProvision { lp_account, .. } =>
				T::LpBalance::try_credit_account(&lp_account, asset.into(), amount.into())?,
			ChannelAction::Swap {
				destination_address,
				destination_asset,
				broker_id,
				broker_commission_bps,
				..
			} => T::SwapDepositHandler::schedule_swap_from_channel(
				<T::TargetChain as cf_chains::Chain>::ChainAccount::into_foreign_chain_address(deposit_address.clone()),
				block_height.into(),
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
				channel_metadata,
				..
			} => T::CcmHandler::on_ccm_deposit(
				asset.into(),
				amount.into(),
				destination_asset,
				destination_address,
				CcmDepositMetadata {
					source_chain: asset.into(),
					source_address: None,
					channel_metadata,
				},
				SwapOrigin::DepositChannel {
					deposit_address: T::AddressConverter::to_encoded_address(
						<T::TargetChain as cf_chains::Chain>::ChainAccount::into_foreign_chain_address(deposit_address.clone()),
					),
					channel_id,
					deposit_block_height: block_height.into(),
				},
			),
		};

		T::DepositHandler::on_deposit_made(
			deposit_details.clone(),
			amount,
			deposit_address.clone(),
			asset,
		);

		Self::deposit_event(Event::DepositReceived {
			deposit_address,
			asset,
			amount,
			deposit_details,
		});
		Ok(())
	}

	/// Opens a channel for the given asset and registers it with the given action.
	/// Emits the `StartWitnessing` event so CFEs can start watching for deposits to the address.
	///
	/// May re-use an existing deposit address, depending on chain configuration.
	fn open_channel(
		source_asset: TargetChainAsset<T, I>,
		channel_action: ChannelAction<T::AccountId>,
		expires_at: BlockNumberFor<T>,
	) -> Result<(ChannelId, TargetChainAccount<T, I>), DispatchError> {
		let (deposit_channel, channel_id) = if let Some((channel_id, mut deposit_channel)) =
			DepositChannelPool::<T, I>::drain().next()
		{
			deposit_channel.asset = source_asset;
			(deposit_channel, channel_id)
		} else {
			let next_channel_id =
				ChannelIdCounter::<T, I>::try_mutate::<_, Error<T, I>, _>(|id| {
					*id = id.checked_add(1).ok_or(Error::<T, I>::ChannelIdsExhausted)?;
					Ok(*id)
				})?;
			(
				DepositChannel::generate_new::<T::AddressDerivation>(
					next_channel_id,
					source_asset,
				)?,
				next_channel_id,
			)
		};

		let deposit_address = deposit_channel.address.clone();

		ChannelActions::<T, I>::insert(&deposit_address, channel_action);
		T::DepositHandler::on_channel_opened(deposit_address.clone(), channel_id)?;

		let opened_at = T::ChainTracking::get_block_height();

		Self::deposit_event(Event::StartWitnessing {
			deposit_address: deposit_address.clone(),
			source_asset,
			opened_at,
		});

		DepositChannelLookup::<T, I>::insert(
			&deposit_address,
			DepositChannelDetails { deposit_channel, opened_at, expires_at },
		);

		Ok((channel_id, deposit_address))
	}

	fn close_channel(address: TargetChainAccount<T, I>) {
		ChannelActions::<T, I>::remove(&address);
		if let Some(deposit_channel_details) = DepositChannelLookup::<T, I>::get(&address) {
			Self::deposit_event(Event::<T, I>::StopWitnessing {
				deposit_address: address,
				source_asset: deposit_channel_details.deposit_channel.asset,
			});
			if let Some(channel) = deposit_channel_details.deposit_channel.maybe_recycle() {
				DepositChannelPool::<T, I>::insert(channel.channel_id, channel);
			}
		} else {
			log_or_panic!("Tried to close an unknown channel.");
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
			Some(CcmDepositMetadata { source_chain, source_address, channel_metadata }) =>
				ScheduledEgressCcm::<T, I>::append(CrossChainMessage {
					egress_id,
					asset,
					amount,
					destination_address: destination_address.clone(),
					message: channel_metadata.message,
					cf_parameters: channel_metadata.cf_parameters,
					source_chain,
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
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	// This should be callable by the LP pallet.
	fn request_liquidity_deposit_address(
		lp_account: T::AccountId,
		source_asset: TargetChainAsset<T, I>,
		expiry_block: BlockNumberFor<T>,
	) -> Result<(ChannelId, ForeignChainAddress), DispatchError> {
		let (channel_id, deposit_address) = Self::open_channel(
			source_asset,
			ChannelAction::LiquidityProvision { lp_account },
			expiry_block,
		)?;

		Ok((
			channel_id,
			<T::TargetChain as cf_chains::Chain>::ChainAccount::into_foreign_chain_address(
				deposit_address,
			),
		))
	}

	// This should only be callable by the broker.
	fn request_swap_deposit_address(
		source_asset: TargetChainAsset<T, I>,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		broker_commission_bps: BasisPoints,
		broker_id: T::AccountId,
		channel_metadata: Option<CcmChannelMetadata>,
		expiry_block: BlockNumberFor<T>,
	) -> Result<(ChannelId, ForeignChainAddress), DispatchError> {
		let (channel_id, deposit_address) = Self::open_channel(
			source_asset,
			match channel_metadata {
				Some(msg) => ChannelAction::CcmTransfer {
					destination_asset,
					destination_address,
					channel_metadata: msg,
				},
				None => ChannelAction::Swap {
					destination_asset,
					destination_address,
					broker_commission_bps,
					broker_id,
				},
			},
			expiry_block,
		)?;

		Ok((
			channel_id,
			<T::TargetChain as cf_chains::Chain>::ChainAccount::into_foreign_chain_address(
				deposit_address,
			),
		))
	}

	// Note: we expect that the mapping from any instantiable pallet to the instance of this pallet
	// is matching to the right chain. Because of that we can ignore the chain parameter.
	fn expire_channel(address: TargetChainAccount<T, I>) {
		Self::close_channel(address);
	}
}
