#![cfg_attr(not(feature = "std"), no_std)]
#![feature(extract_if)]
#![feature(map_try_insert)]
#![doc = include_str!("../README.md")]
#![doc = include_str!("../../cf-doc-head.md")]

mod benchmarking;

pub mod migrations;
#[cfg(test)]
mod mock_btc;
#[cfg(test)]
mod mock_eth;
#[cfg(test)]
mod tests;
pub mod weights;

mod boost_pool;

use boost_pool::BoostPool;
pub use boost_pool::OwedAmount;

use frame_support::{pallet_prelude::OptionQuery, transactional};

use cf_chains::{
	address::{
		AddressConverter, AddressDerivationApi, AddressDerivationError, IntoForeignChainAddress,
	},
	AllBatch, AllBatchError, CcmCfParameters, CcmChannelMetadata, CcmDepositMetadata, CcmMessage,
	Chain, ChannelLifecycleHooks, ConsolidateCall, DepositChannel, ExecutexSwapAndCall,
	FetchAssetParams, ForeignChainAddress, SwapOrigin, TransferAssetParams,
};
use cf_primitives::{
	Asset, BasisPoints, Beneficiaries, BoostPoolTier, BroadcastId, ChannelId, EgressCounter,
	EgressId, EpochIndex, ForeignChain, PrewitnessedDepositId, SwapId, ThresholdSignatureRequestId,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	liquidity::{LpBalanceApi, LpDepositHandler},
	AccountRoleRegistry, AdjustedFeeEstimationApi, AssetConverter, Broadcaster, CcmHandler,
	CcmSwapIds, Chainflip, DepositApi, EgressApi, EpochInfo, FeePayment, GetBlockHeight,
	IngressEgressFeeApi, NetworkEnvironmentProvider, OnDeposit, SafeMode, ScheduledEgressDetails,
	SwapDepositHandler, SwapQueueApi, SwapType,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{traits::Zero, DispatchError, Permill, Saturating},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	vec,
	vec::Vec,
};
pub use weights::WeightInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum BoostStatus {
	Boosted { prewitnessed_deposit_id: PrewitnessedDepositId, pools: Vec<BoostPoolTier> },
	NotBoosted,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct PrewitnessedDeposit<C: Chain> {
	pub asset: C::ChainAsset,
	pub amount: C::ChainAmount,
	pub deposit_address: C::ChainAccount,
	pub block_height: C::ChainBlockNumber,
	pub deposit_details: C::DepositDetails,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BoostPoolId<C: Chain> {
	asset: C::ChainAsset,
	tier: BoostPoolTier,
}

pub struct BoostOutput<C: Chain> {
	used_pools: BTreeMap<BoostPoolTier, C::ChainAmount>,
	total_fee: C::ChainAmount,
}

/// Enum wrapper for fetch and egress requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum FetchOrTransfer<C: Chain> {
	Fetch {
		asset: C::ChainAsset,
		deposit_address: C::ChainAccount,
		deposit_fetch_id: Option<C::DepositFetchId>,
		amount: C::ChainAmount,
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

#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo)]
pub enum DepositIgnoredReason {
	BelowMinimumDeposit,

	/// The deposit was ignored because the amount provided was not high enough to pay for the fees
	/// required to process the requisite transactions.
	NotEnoughToPayFees,
}

/// Cross-chain messaging requests.
#[derive(RuntimeDebug, Eq, PartialEq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub(crate) struct CrossChainMessage<C: Chain> {
	pub egress_id: EgressId,
	pub asset: C::ChainAsset,
	pub amount: C::ChainAmount,
	pub destination_address: C::ChainAccount,
	pub message: CcmMessage,
	// The sender of the deposit transaction.
	pub source_chain: ForeignChain,
	pub source_address: Option<ForeignChainAddress>,
	// Where funds might be returned to if the message fails.
	pub cf_parameters: CcmCfParameters,
	pub gas_budget: C::ChainAmount,
}

impl<C: Chain> CrossChainMessage<C> {
	fn asset(&self) -> C::ChainAsset {
		self.asset
	}
}

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(8);

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
#[scale_info(skip_type_params(I))]
pub struct PalletSafeMode<I: 'static> {
	pub boost_deposits_enabled: bool,
	pub add_boost_funds_enabled: bool,
	pub stop_boosting_enabled: bool,
	pub deposits_enabled: bool,
	#[doc(hidden)]
	#[codec(skip)]
	_phantom: PhantomData<I>,
}

impl<I: 'static> SafeMode for PalletSafeMode<I> {
	const CODE_RED: Self = PalletSafeMode {
		boost_deposits_enabled: false,
		add_boost_funds_enabled: false,
		stop_boosting_enabled: false,
		deposits_enabled: false,
		_phantom: PhantomData,
	};
	const CODE_GREEN: Self = PalletSafeMode {
		boost_deposits_enabled: true,
		add_boost_funds_enabled: true,
		stop_boosting_enabled: true,
		deposits_enabled: true,
		_phantom: PhantomData,
	};
}

/// Calls to the external chains that has failed to be broadcast/accepted by the target chain.
/// User can use information stored here to query for relevant information to broadcast
/// the call themselves.
#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct FailedForeignChainCall {
	/// Broadcast ID used in the broadcast pallet. Use it to query broadcast information,
	/// such as the threshold signature, the API call etc.
	pub broadcast_id: BroadcastId,
	/// The epoch the call originally failed in. Calls are cleaned from storage 2 epochs.
	pub original_epoch: EpochIndex,
}

#[derive(
	CloneNoBound,
	RuntimeDebugNoBound,
	PartialEqNoBound,
	EqNoBound,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(T, I))]
pub enum PalletConfigUpdate<T: Config<I>, I: 'static = ()> {
	/// Set the fixed fee that is burned when opening a channel, denominated in Flipperinos.
	ChannelOpeningFee { fee: T::Amount },
	/// Set the minimum deposit allowed for a particular asset.
	SetMinimumDeposit { asset: TargetChainAsset<T, I>, minimum_deposit: TargetChainAmount<T, I> },
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::{ExecutexSwapAndCall, TransferFallback};
	use cf_primitives::{BroadcastId, EpochIndex};
	use cf_traits::{LpDepositHandler, OnDeposit, SwapQueueApi};
	use core::marker::PhantomData;
	use frame_support::{
		traits::{ConstU128, EnsureOrigin, IsType},
		DefaultNoBound,
	};
	use frame_system::WeightInfo as SystemWeightInfo;
	use sp_runtime::SaturatedConversion;
	use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

	pub(crate) type ChannelRecycleQueue<T, I> =
		Vec<(TargetChainBlockNumber<T, I>, TargetChainAccount<T, I>)>;

	pub type TargetChainAsset<T, I> = <<T as Config<I>>::TargetChain as Chain>::ChainAsset;
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

	#[derive(CloneNoBound, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositChannelDetails<T: Config<I>, I: 'static> {
		pub deposit_channel: DepositChannel<T::TargetChain>,
		/// The block number at which the deposit channel was opened, expressed as a block number
		/// on the external Chain.
		pub opened_at: TargetChainBlockNumber<T, I>,
		/// The last block on the target chain that the witnessing will witness it in. If funds are
		/// sent after this block, they will not be witnessed.
		pub expires_at: TargetChainBlockNumber<T, I>,

		/// The action to be taken when the DepositChannel is deposited to.
		pub action: ChannelAction<T::AccountId>,
		/// The boost fee
		pub boost_fee: BasisPoints,
		/// Boost status, indicating whether there is pending boost on the channel
		pub boost_status: BoostStatus,
	}

	pub enum IngressOrEgress {
		Ingress,
		Egress,
	}

	pub struct AmountAndFeesWithheld<T: Config<I>, I: 'static> {
		pub amount_after_fees: TargetChainAmount<T, I>,
		pub fees_withheld: TargetChainAmount<T, I>,
	}

	/// Determines the action to take when a deposit is made to a channel.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum ChannelAction<AccountId> {
		Swap {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
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

	/// Contains identifying information about the particular actions that have occurred for a
	/// particular deposit.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum DepositAction<AccountId> {
		Swap { swap_id: SwapId },
		LiquidityProvision { lp_account: AccountId },
		CcmTransfer { principal_swap_id: Option<SwapId>, gas_swap_id: Option<SwapId> },
		NoAction,
		BoostersCredited { prewitnessed_deposit_id: PrewitnessedDepositId },
	}

	/// Tracks funds that are owned by the vault and available for egress.
	#[derive(
		CloneNoBound,
		DefaultNoBound,
		RuntimeDebug,
		PartialEq,
		Eq,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub struct DepositTracker<T: Config<I>, I: 'static> {
		pub unfetched: TargetChainAmount<T, I>,
		pub fetched: TargetChainAmount<T, I>,
	}

	// TODO: make this chain-specific. Something like:
	// Replace Amount with an type representing a single deposit (ie. a single UTXO).
	// Register transfer would store the change UTXO.
	impl<T: Config<I>, I: 'static> DepositTracker<T, I> {
		pub fn total(&self) -> TargetChainAmount<T, I> {
			self.unfetched.saturating_add(self.fetched)
		}

		pub fn register_deposit(&mut self, amount: TargetChainAmount<T, I>) {
			self.unfetched.saturating_accrue(amount);
		}

		pub fn register_transfer(&mut self, amount: TargetChainAmount<T, I>) {
			if amount > self.fetched {
				log::error!("Transfer amount is greater than available funds");
			}
			self.fetched.saturating_reduce(amount);
		}

		pub fn mark_as_fetched(&mut self, amount: TargetChainAmount<T, I>) {
			debug_assert!(
				self.unfetched >= amount,
				"Accounting error: not enough unfetched funds."
			);
			self.unfetched.saturating_reduce(amount);
			self.fetched.saturating_accrue(amount);
		}
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config<I>, I: 'static = ()> {
		pub deposit_channel_lifetime: TargetChainBlockNumber<T, I>,
		pub witness_safety_margin: Option<TargetChainBlockNumber<T, I>>,
		pub dust_limits: Vec<(TargetChainAsset<T, I>, TargetChainAmount<T, I>)>,
	}

	impl<T: Config<I>, I: 'static> Default for GenesisConfig<T, I> {
		fn default() -> Self {
			Self {
				deposit_channel_lifetime: Default::default(),
				witness_safety_margin: None,
				dust_limits: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config<I>, I: 'static> BuildGenesisConfig for GenesisConfig<T, I> {
		fn build(&self) {
			DepositChannelLifetime::<T, I>::put(self.deposit_channel_lifetime);
			WitnessSafetyMargin::<T, I>::set(self.witness_safety_margin);

			for (asset, dust_limit) in self.dust_limits.clone() {
				EgressDustLimit::<T, I>::set(asset, dust_limit.unique_saturated_into());
			}
		}
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
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
		type TargetChain: Chain + Get<ForeignChain>;

		/// Generates deposit addresses.
		type AddressDerivation: AddressDerivationApi<Self::TargetChain>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		/// Pallet responsible for managing Liquidity Providers.
		type LpBalance: LpBalanceApi<AccountId = Self::AccountId>
			+ LpDepositHandler<AccountId = Self::AccountId>;

		/// For scheduling swaps.
		type SwapDepositHandler: SwapDepositHandler<AccountId = Self::AccountId>;

		/// Handler for Cross Chain Messages.
		type CcmHandler: CcmHandler;

		/// The type of the chain-native transaction.
		type ChainApiCall: AllBatch<Self::TargetChain>
			+ ExecutexSwapAndCall<Self::TargetChain>
			+ TransferFallback<Self::TargetChain>
			+ ConsolidateCall<Self::TargetChain>;

		/// Get the latest chain state of the target chain.
		type ChainTracking: GetBlockHeight<Self::TargetChain>
			+ AdjustedFeeEstimationApi<Self::TargetChain>;

		/// A broadcaster instance.
		type Broadcaster: Broadcaster<
			Self::TargetChain,
			ApiCall = Self::ChainApiCall,
			Callback = <Self as Config<I>>::RuntimeCall,
		>;

		/// Provides callbacks for deposit lifecycle events.
		type DepositHandler: OnDeposit<Self::TargetChain>;

		type NetworkEnvironment: NetworkEnvironmentProvider;

		/// Allows assets to be converted through the AMM.
		type AssetConverter: AssetConverter;

		/// For paying the channel opening fee.
		type FeePayment: FeePayment<Amount = Self::Amount, AccountId = Self::AccountId>;

		/// Benchmark weights
		type WeightInfo: WeightInfo;

		type SwapQueueApi: SwapQueueApi;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode<I>>;
	}

	/// Lookup table for addresses to corresponding deposit channels.
	#[pallet::storage]
	#[pallet::unbounded]
	pub type DepositChannelLookup<T: Config<I>, I: 'static = ()> = StorageMap<
		_,
		Twox64Concat,
		TargetChainAccount<T, I>,
		DepositChannelDetails<T, I>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type BoostPools<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		TargetChainAsset<T, I>,
		Twox64Concat,
		BoostPoolTier,
		BoostPool<T::AccountId, T::TargetChain>,
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

	/// Defines the minimum amount aka. dust limit for a single egress i.e. *not* of a batch, but
	/// the outputs of each individual egress within that batch. If not set, defaults to 1.
	///
	/// This is required for bitcoin, for example, where any amount below 600 satoshis is considered
	/// dust and will be rejected by miners.
	#[pallet::storage]
	#[pallet::getter(fn egress_dust_limit)]
	pub type EgressDustLimit<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, u128, ValueQuery, ConstU128<1>>;

	#[pallet::storage]
	pub type DepositChannelLifetime<T: Config<I>, I: 'static = ()> =
		StorageValue<_, TargetChainBlockNumber<T, I>, ValueQuery>;

	/// Stores information about Calls to external chains that have failed to be broadcasted.
	/// These calls are signed and stored on-chain so that the user can broadcast the call
	/// themselves. These messages will be re-threshold-signed once during the next epoch, and
	/// removed from storage in the epoch after that.
	/// Hashmap: last_signed_epoch -> Vec<FailedForeignChainCall>
	#[pallet::storage]
	#[pallet::getter(fn failed_foreign_chain_calls)]
	pub type FailedForeignChainCalls<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, EpochIndex, Vec<FailedForeignChainCall>, ValueQuery>;

	#[pallet::storage]
	pub type DepositBalances<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, DepositTracker<T, I>, ValueQuery>;

	#[pallet::storage]
	pub type DepositChannelRecycleBlocks<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChannelRecycleQueue<T, I>, ValueQuery>;

	// Determines the number of block confirmations is required for a block on
	// an external chain before CFE can submit any witness extrinsics for it.
	#[pallet::storage]
	#[pallet::getter(fn witness_safety_margin)]
	pub type WitnessSafetyMargin<T: Config<I>, I: 'static = ()> =
		StorageValue<_, TargetChainBlockNumber<T, I>, OptionQuery>;

	/// Tracks fees withheld from ingresses and egresses.
	#[pallet::storage]
	pub type WithheldTransactionFees<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, TargetChainAsset<T, I>, TargetChainAmount<T, I>, ValueQuery>;

	/// The fixed fee charged for opening a channel, in Flipperinos.
	#[pallet::storage]
	#[pallet::getter(fn channel_opening_fee)]
	pub type ChannelOpeningFee<T: Config<I>, I: 'static = ()> =
		StorageValue<_, T::Amount, ValueQuery>;

	/// Stores the latest prewitnessed deposit id used.
	#[pallet::storage]
	pub type PrewitnessedDepositIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, PrewitnessedDepositId, ValueQuery>;

	/// Stores all deposits that have been prewitnessed but not yet finalised.
	#[pallet::storage]
	pub type PrewitnessedDeposits<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		ChannelId,
		Twox64Concat,
		PrewitnessedDepositId,
		PrewitnessedDeposit<T::TargetChain>,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		DepositFinalised {
			deposit_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			deposit_details: <T::TargetChain as Chain>::DepositDetails,
			// Ingress fee in the deposit asset. i.e. *NOT* the gas asset, if the deposit asset is
			// a non-gas asset.
			ingress_fee: TargetChainAmount<T, I>,
			action: DepositAction<T::AccountId>,
			channel_id: ChannelId,
		},
		AssetEgressStatusChanged {
			asset: TargetChainAsset<T, I>,
			disabled: bool,
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
		/// The deposits was rejected because the amount was below the minimum allowed.
		DepositIgnored {
			deposit_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			deposit_details: <T::TargetChain as Chain>::DepositDetails,
			reason: DepositIgnoredReason,
		},
		TransferFallbackRequested {
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			destination_address: TargetChainAccount<T, I>,
			broadcast_id: BroadcastId,
		},
		/// The deposit witness was rejected.
		DepositWitnessRejected {
			reason: DispatchError,
			deposit_witness: DepositWitness<T::TargetChain>,
		},
		/// A CCM has failed to broadcast.
		CcmBroadcastFailed {
			broadcast_id: BroadcastId,
		},
		/// A failed CCM call has been re-threshold-signed for the current epoch.
		FailedForeignChainCallResigned {
			broadcast_id: BroadcastId,
			threshold_signature_id: ThresholdSignatureRequestId,
		},
		/// A failed CCM has been in the system storage for more than 1 epoch.
		/// It's broadcast data has been cleaned from storage.
		FailedForeignChainCallExpired {
			broadcast_id: BroadcastId,
		},
		UtxoConsolidation {
			broadcast_id: BroadcastId,
		},
		FailedToBuildAllBatchCall {
			error: AllBatchError,
		},
		ChannelOpeningFeePaid {
			fee: T::Amount,
		},
		ChannelOpeningFeeSet {
			fee: T::Amount,
		},
		DepositBoosted {
			deposit_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amounts: BTreeMap<BoostPoolTier, TargetChainAmount<T, I>>,
			deposit_details: <T::TargetChain as Chain>::DepositDetails,
			prewitnessed_deposit_id: PrewitnessedDepositId,
			channel_id: ChannelId,
			// Ingress fee in the deposit asset. i.e. *NOT* the gas asset, if the deposit asset is
			// a non-gas asset. The ingress fee is taken *after* the boost fee.
			ingress_fee: TargetChainAmount<T, I>,
			// Total fee the user paid for their deposit to be boosted.
			boost_fee: TargetChainAmount<T, I>,
			action: DepositAction<T::AccountId>,
		},
		BoostFundsAdded {
			booster_id: T::AccountId,
			boost_pool: BoostPoolId<T::TargetChain>,
			amount: TargetChainAmount<T, I>,
		},
		StoppedBoosting {
			booster_id: T::AccountId,
			boost_pool: BoostPoolId<T::TargetChain>,
			// When we stop boosting, the amount in the pool that isn't currently pending
			// finalisation can be returned immediately.
			unlocked_amount: TargetChainAmount<T, I>,
			// The ids of the boosts that are pending finalisation, such that the funds can then be
			// returned to the user's free balance when the finalisation occurs.
			pending_boosts: BTreeSet<PrewitnessedDepositId>,
		},
		InsufficientBoostLiquidity {
			prewitnessed_deposit_id: PrewitnessedDepositId,
			asset: TargetChainAsset<T, I>,
			amount_attempted: TargetChainAmount<T, I>,
			channel_id: ChannelId,
		},
		BoostPoolCreated {
			boost_pool: BoostPoolId<T::TargetChain>,
		},
	}

	#[derive(CloneNoBound, PartialEqNoBound, EqNoBound)]
	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// The deposit address is not valid. It may have expired or may never have been issued.
		InvalidDepositAddress,
		/// A deposit was made using the wrong asset.
		AssetMismatch,
		/// Channel ID has reached maximum
		ChannelIdsExhausted,
		/// Polkadot's Vault Account does not exist in storage.
		MissingPolkadotVault,
		/// Bitcoin's Vault key does not exist for the current epoch.
		MissingBitcoinVault,
		/// Channel ID is too large for Bitcoin address derivation
		BitcoinChannelIdTooLarge,
		/// The amount is below the minimum egress amount.
		BelowEgressDustLimit,
		/// Adding boost funds is disabled due to safe mode.
		AddBoostFundsDisabled,
		/// Retrieving boost funds disabled due to safe mode.
		StopBoostingDisabled,
		/// Cannot create a boost pool if it already exists.
		BoostPoolAlreadyExists,
		/// Cannot create a boost pool of 0 bps
		InvalidBoostPoolTier,
		/// Disabled due to safe mode for the chain
		DepositChannelCreationDisabled,
		/// The specified boost pool does not exist.
		BoostPoolDoesNotExist,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Recycle addresses if we can
		fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut used_weight = Weight::zero();

			// Approximate weight calculation: r/w DepositChannelLookup + r PrewitnessedDeposits +
			// w DepositChannelPool + 1 clear_prewitnessed_deposits
			let recycle_weight_per_address =
				frame_support::weights::constants::RocksDbWeight::get()
					.reads_writes(2, 2)
					.saturating_add(T::WeightInfo::clear_prewitnessed_deposits(1));

			let maximum_addresses_to_recycle = remaining_weight
				.ref_time()
				.checked_div(recycle_weight_per_address.ref_time())
				.unwrap_or_default()
				.saturated_into::<usize>();

			let addresses_to_recycle =
				DepositChannelRecycleBlocks::<T, I>::mutate(|recycle_queue| {
					Self::take_recyclable_addresses(
						recycle_queue,
						maximum_addresses_to_recycle,
						T::ChainTracking::get_block_height(),
					)
				});

			// Add weight for the DepositChannelRecycleBlocks read/write plus the
			// DepositChannelLookup read/writes in the for loop below
			used_weight = used_weight.saturating_add(
				frame_support::weights::constants::RocksDbWeight::get().reads_writes(
					(addresses_to_recycle.len() + 1) as u64,
					(addresses_to_recycle.len() + 1) as u64,
				),
			);

			for address in addresses_to_recycle.iter() {
				if let Some(DepositChannelDetails { deposit_channel, boost_status, .. }) =
					DepositChannelLookup::<T, I>::take(address)
				{
					if let Some(state) = deposit_channel.state.maybe_recycle() {
						DepositChannelPool::<T, I>::insert(
							deposit_channel.channel_id,
							DepositChannel { state, ..deposit_channel },
						);
						used_weight = used_weight.saturating_add(
							frame_support::weights::constants::RocksDbWeight::get()
								.reads_writes(0, 1),
						);
					}
					let removed_deposits =
						Self::clear_prewitnessed_deposits(deposit_channel.channel_id);

					if let BoostStatus::Boosted { prewitnessed_deposit_id, pools } = boost_status {
						for pool_tier in pools {
							BoostPools::<T, I>::mutate(deposit_channel.asset, pool_tier, |pool| {
								if let Some(pool) = pool {
									let affected_boosters_count =
										pool.on_lost_deposit(prewitnessed_deposit_id);
									used_weight.saturating_accrue(T::WeightInfo::on_lost_deposit(
										affected_boosters_count as u32,
									));
								} else {
									log_or_panic!(
										"Pool must exist: ({pool_tier:?}, {:?})",
										deposit_channel.asset
									);
								}
							});
						}
					}

					used_weight = used_weight.saturating_add(
						T::WeightInfo::clear_prewitnessed_deposits(removed_deposits),
					);
				}
			}
			used_weight
		}

		/// Take all scheduled Egress and send them out
		fn on_finalize(_n: BlockNumberFor<T>) {
			// Send all fetch/transfer requests as a batch. Revert storage if failed.
			if let Err(error) = Self::do_egress_scheduled_fetch_transfer() {
				Self::deposit_event(Event::<T, I>::FailedToBuildAllBatchCall { error });
			}

			if let Ok(egress_transaction) =
				<T::ChainApiCall as ConsolidateCall<T::TargetChain>>::consolidate_utxos()
			{
				let (broadcast_id, _) =
					T::Broadcaster::threshold_sign_and_broadcast(egress_transaction);
				Self::deposit_event(Event::<T, I>::UtxoConsolidation { broadcast_id });
			};

			// Egress all scheduled Cross chain messages
			Self::do_egress_scheduled_ccm();

			// Process failed external chain calls: re-sign or cull storage.
			// Take 1 call per block to avoid weight spike.
			let current_epoch = T::EpochInfo::epoch_index();
			if let Some(call) = FailedForeignChainCalls::<T, I>::mutate_exists(
				current_epoch.saturating_sub(1),
				|calls| {
					let next_call = calls.as_mut().and_then(Vec::pop);
					if calls.as_ref().map(Vec::len).unwrap_or_default() == 0 {
						// Ensures we remove the storage if there are no more calls.
						*calls = None;
					}
					next_call
				},
			) {
				match current_epoch.saturating_sub(call.original_epoch) {
					// The call is stale, clean up storage.
					n if n >= 2 => {
						T::Broadcaster::clean_up_broadcast_storage(call.broadcast_id);
						Self::deposit_event(Event::<T, I>::FailedForeignChainCallExpired {
							broadcast_id: call.broadcast_id,
						});
					},
					// Previous epoch, signature is invalid. Re-sign and store.
					1 => {
						if let Some(threshold_signature_id) =
							T::Broadcaster::threshold_resign(call.broadcast_id)
						{
							Self::deposit_event(Event::<T, I>::FailedForeignChainCallResigned {
								broadcast_id: call.broadcast_id,
								threshold_signature_id,
							});
							FailedForeignChainCalls::<T, I>::append(current_epoch, call);
						} else {
							// We are here if the Call needs to be resigned, yet no API call data is
							// available to use. In this situation, there's nothing else that can be
							// done.
							log::error!("Foreign Chain Call message cannot be re-signed: Call data unavailable. Broadcast Id: {:?}", call.broadcast_id);
						}
					},
					// Current epoch, shouldn't be possible.
					_ => {
						log_or_panic!(
							"Unexpected Call for the current epoch. Broadcast Id: {:?}",
							call.broadcast_id,
						);
					},
				}
			}
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
				DepositChannelLookup::<T, I>::mutate(deposit_address, |deposit_channel_details| {
					deposit_channel_details
						.as_mut()
						.map(|details| details.deposit_channel.state.on_fetch_completed());
				});
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
		/// Requires `EnsurePrewitnessed` or `EnsureWitnessed` origin.
		///
		/// We calculate weight assuming the most expensive code path is taken, i.e. the deposit
		/// had been boosted and is now being finalised
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::boost_finalised().saturating_mul(deposit_witnesses.len() as u64))]
		pub fn process_deposits(
			origin: OriginFor<T>,
			deposit_witnesses: Vec<DepositWitness<T::TargetChain>>,
			block_height: TargetChainBlockNumber<T, I>,
		) -> DispatchResult {
			if T::EnsurePrewitnessed::ensure_origin(origin.clone()).is_ok() {
				Self::add_prewitnessed_deposits(deposit_witnesses, block_height)?;
			} else {
				T::EnsureWitnessed::ensure_origin(origin)?;
				Self::process_deposit_witnesses(deposit_witnesses, block_height)?;
			}
			Ok(())
		}

		/// Stores information on failed Vault transfer.
		/// Requires Witness origin.
		///
		/// ## Events
		///
		/// - [on_success](Event::TransferFallbackRequested)
		#[pallet::weight(T::WeightInfo::vault_transfer_failed())]
		#[pallet::call_index(4)]
		pub fn vault_transfer_failed(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			destination_address: TargetChainAccount<T, I>,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let current_epoch = T::EpochInfo::epoch_index();
			match <T::ChainApiCall as TransferFallback<T::TargetChain>>::new_unsigned(
				TransferAssetParams { asset, amount, to: destination_address.clone() },
			) {
				Ok(api_call) => {
					let (broadcast_id, _) = T::Broadcaster::threshold_sign(api_call);
					FailedForeignChainCalls::<T, I>::append(
						current_epoch,
						FailedForeignChainCall { broadcast_id, original_epoch: current_epoch },
					);
					Self::deposit_event(Event::<T, I>::TransferFallbackRequested {
						asset,
						amount,
						destination_address,
						broadcast_id,
					});
				},
				// The only way this can fail is if the target chain is unsupported, which should
				// never happen.
				Err(_) => {
					log_or_panic!(
						"Failed to construct TransferFallback call. Asset: {:?}, amount: {:?}, Destination: {:?}",
						asset, amount, destination_address
					);
				},
			};
			Ok(())
		}

		/// Callback for when CCMs failed to be broadcasted. We will resign the call
		/// so the user can broadcast the CCM themselves.
		/// Requires Root origin.
		///
		/// ## Events
		///
		/// - [on_success](Event::CcmBroadcastFailed)
		#[pallet::weight(T::WeightInfo::ccm_broadcast_failed())]
		#[pallet::call_index(5)]
		pub fn ccm_broadcast_failed(
			origin: OriginFor<T>,
			broadcast_id: BroadcastId,
		) -> DispatchResult {
			ensure_root(origin)?;

			let current_epoch = T::EpochInfo::epoch_index();

			// Stores the broadcast ID, so the user can use it to query for
			// information such as Threshold Signature etc.
			FailedForeignChainCalls::<T, I>::append(
				current_epoch,
				FailedForeignChainCall { broadcast_id, original_epoch: current_epoch },
			);

			Self::deposit_event(Event::<T, I>::CcmBroadcastFailed { broadcast_id });
			Ok(())
		}

		/// Apply a list of configuration updates to the pallet.
		///
		/// Requires Governance.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as frame_system::Config>::SystemWeightInfo::set_storage(updates.len() as u32))]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			updates: BoundedVec<PalletConfigUpdate<T, I>, ConstU32<10>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::<T, I>::ChannelOpeningFee { fee } => {
						let fee = fee.unique_saturated_into();
						ChannelOpeningFee::<T, I>::set(fee);
						Self::deposit_event(Event::<T, I>::ChannelOpeningFeeSet { fee });
					},
					PalletConfigUpdate::<T, I>::SetMinimumDeposit { asset, minimum_deposit } => {
						MinimumDeposit::<T, I>::insert(asset, minimum_deposit);
						Self::deposit_event(Event::<T, I>::MinimumDepositSet {
							asset,
							minimum_deposit,
						});
					},
				}
			}

			Ok(())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::add_boost_funds())]
		pub fn add_boost_funds(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			pool_tier: BoostPoolTier,
		) -> DispatchResult {
			let booster_id = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			ensure!(
				T::SafeMode::get().add_boost_funds_enabled,
				Error::<T, I>::AddBoostFundsDisabled
			);

			T::LpBalance::try_debit_account(&booster_id, asset.into(), amount.into())?;

			BoostPools::<T, I>::mutate(asset, pool_tier, |pool| {
				let pool = pool.as_mut().ok_or(Error::<T, I>::BoostPoolDoesNotExist)?;
				pool.add_funds(booster_id.clone(), amount);

				Ok::<(), DispatchError>(())
			})?;

			Self::deposit_event(Event::<T, I>::BoostFundsAdded {
				booster_id,
				boost_pool: BoostPoolId { asset, tier: pool_tier },
				amount,
			});

			Ok(())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::stop_boosting())]
		pub fn stop_boosting(
			origin: OriginFor<T>,
			asset: TargetChainAsset<T, I>,
			pool_tier: BoostPoolTier,
		) -> DispatchResult {
			let booster = T::AccountRoleRegistry::ensure_liquidity_provider(origin)?;
			ensure!(T::SafeMode::get().stop_boosting_enabled, Error::<T, I>::StopBoostingDisabled);

			let (unlocked_amount, pending_boosts) =
				BoostPools::<T, I>::mutate(asset, pool_tier, |pool| {
					let pool = pool.as_mut().ok_or(Error::<T, I>::BoostPoolDoesNotExist)?;
					pool.stop_boosting(booster.clone())
				})?;

			T::LpBalance::try_credit_account(&booster, asset.into(), unlocked_amount.into())?;

			Self::deposit_event(Event::StoppedBoosting {
				booster_id: booster,
				boost_pool: BoostPoolId { asset, tier: pool_tier },
				unlocked_amount,
				pending_boosts,
			});

			Ok(())
		}

		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::create_boost_pools() * new_pools.len() as u64)]
		pub fn create_boost_pools(
			origin: OriginFor<T>,
			new_pools: Vec<BoostPoolId<T::TargetChain>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			new_pools.into_iter().try_for_each(|pool_id| {
				ensure!(pool_id.tier != 0, Error::<T, I>::InvalidBoostPoolTier);
				BoostPools::<T, I>::try_mutate_exists(pool_id.asset, pool_id.tier, |pool| {
					ensure!(pool.is_none(), Error::<T, I>::BoostPoolAlreadyExists);
					*pool = Some(BoostPool::new(pool_id.tier));

					Self::deposit_event(Event::<T, I>::BoostPoolCreated { boost_pool: pool_id });

					Ok::<(), Error<T, I>>(())
				})
			})?;
			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn take_recyclable_addresses(
		channel_recycle_blocks: &mut ChannelRecycleQueue<T, I>,
		maximum_addresses_to_take: usize,
		current_block_height: TargetChainBlockNumber<T, I>,
	) -> Vec<TargetChainAccount<T, I>> {
		let partition_point = sp_std::cmp::min(
			channel_recycle_blocks.partition_point(|(block, _)| *block <= current_block_height),
			maximum_addresses_to_take,
		);
		channel_recycle_blocks
			.drain(..partition_point)
			.map(|(_, address)| address)
			.collect()
	}

	// Clears all prewitnessed deposits for a given channel, returning the number of items removed.
	fn clear_prewitnessed_deposits(channel_id: ChannelId) -> u32 {
		let item_count = PrewitnessedDeposits::<T, I>::iter_prefix(channel_id).count() as u32;
		// TODO: find out why clear_prefix returns 0 and ignores the given limit.
		let _removed = PrewitnessedDeposits::<T, I>::clear_prefix(channel_id, item_count, None);
		item_count
	}

	/// Take all scheduled egress requests and send them out in an `AllBatch` call.
	///
	/// Note: Egress transactions with Blacklisted assets are not sent, and kept in storage.
	#[transactional]
	fn do_egress_scheduled_fetch_transfer() -> Result<(), AllBatchError> {
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
								} => DepositChannelLookup::<T, I>::mutate(
									deposit_address,
									|details| {
										details
											.as_mut()
											.map(|details| {
												let can_fetch =
													details.deposit_channel.state.can_fetch();

												if can_fetch {
													deposit_fetch_id.replace(
														details.deposit_channel.fetch_id(),
													);
													details
														.deposit_channel
														.state
														.on_fetch_scheduled();
												}
												can_fetch
											})
											.unwrap_or(false)
									},
								),
								FetchOrTransfer::Transfer { .. } => true,
							}
					})
					.collect()
			});

		if batch_to_send.is_empty() {
			return Ok(())
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
					amount,
				} => {
					fetch_params.push(FetchAssetParams {
						deposit_fetch_id: deposit_fetch_id.expect("Checked in extract_if"),
						asset,
					});
					addresses.push(deposit_address.clone());
					DepositBalances::<T, I>::mutate(asset, |tracker| {
						tracker.mark_as_fetched(amount);
					});
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
				let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
					egress_transaction,
					Some(Call::finalise_ingress { addresses }.into()),
					|_| None,
				);
				Self::deposit_event(Event::<T, I>::BatchBroadcastRequested {
					broadcast_id,
					egress_ids,
				});
				Ok(())
			},
			Err(AllBatchError::NotRequired) => Ok(()),
			Err(other) => Err(other),
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
				TransferAssetParams {
					asset: ccm.asset,
					amount: ccm.amount,
					to: ccm.destination_address,
				},
				ccm.source_chain,
				ccm.source_address,
				ccm.gas_budget,
				ccm.message.to_vec(),
			) {
				Ok(api_call) => {
					let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
						api_call,
						None,
						|broadcast_id| Some(Call::ccm_broadcast_failed { broadcast_id }.into()),
					);
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

	fn process_deposit_witnesses(
		deposit_witnesses: Vec<DepositWitness<T::TargetChain>>,
		block_height: TargetChainBlockNumber<T, I>,
	) -> DispatchResult {
		for ref deposit_witness @ DepositWitness {
			ref deposit_address,
			asset,
			amount,
			ref deposit_details,
		} in deposit_witnesses
		{
			Self::process_single_deposit(
				deposit_address.clone(),
				asset,
				amount,
				deposit_details.clone(),
				block_height,
			)
			.unwrap_or_else(|e| {
				Self::deposit_event(Event::<T, I>::DepositWitnessRejected {
					reason: e,
					deposit_witness: deposit_witness.clone(),
				});
			})
		}
		Ok(())
	}

	/// Returns a list of contributions from the used pools and the total boost fee.
	#[transactional]
	fn try_boosting(
		asset: TargetChainAsset<T, I>,
		required_amount: TargetChainAmount<T, I>,
		max_boost_fee_bps: BasisPoints,
		prewitnessed_deposit_id: PrewitnessedDepositId,
	) -> Result<BoostOutput<T::TargetChain>, DispatchError> {
		let mut remaining_amount = required_amount;

		let mut total_fee_amount: TargetChainAmount<T, I> = 0u32.into();

		let mut used_pools = BTreeMap::new();

		let sorted_boost_tiers = BoostPools::<T, I>::iter_prefix(asset)
			.map(|(tier, _)| tier)
			.collect::<BTreeSet<_>>();

		debug_assert!(
			sorted_boost_tiers
				.iter()
				.zip(sorted_boost_tiers.iter().skip(1))
				.all(|(a, b)| a < b),
			"Boost tiers should be in ascending order"
		);

		for boost_tier in sorted_boost_tiers {
			if boost_tier > max_boost_fee_bps {
				break
			}

			// For each fee tier, get the amount that the pool is boosting and the boost fee
			let (boosted_amount, fee) = BoostPools::<T, I>::mutate(asset, boost_tier, |pool| {
				let pool = match pool {
					Some(pool) if pool.get_available_amount() == Zero::zero() => {
						return Ok::<_, DispatchError>((0u32.into(), 0u32.into()));
					},
					None => {
						// Pool not existing for some reason is equivalent to not having funds:
						return Ok::<_, DispatchError>((0u32.into(), 0u32.into()));
					},
					Some(pool) => pool,
				};

				pool.provide_funds_for_boosting(prewitnessed_deposit_id, remaining_amount)
					.map_err(Into::into)
			})?;

			if !boosted_amount.is_zero() {
				used_pools.insert(boost_tier, boosted_amount);
			}

			remaining_amount.saturating_reduce(boosted_amount);
			total_fee_amount.saturating_accrue(fee);

			if remaining_amount == 0u32.into() {
				return Ok(BoostOutput { used_pools, total_fee: total_fee_amount });
			}
		}

		Err("Insufficient boost funds".into())
	}

	fn add_prewitnessed_deposits(
		deposit_witnesses: Vec<DepositWitness<T::TargetChain>>,
		block_height: TargetChainBlockNumber<T, I>,
	) -> DispatchResult {
		for DepositWitness { deposit_address, asset, amount, deposit_details } in deposit_witnesses
		{
			if amount < MinimumDeposit::<T, I>::get(asset) {
				// We do not process/record pre-witnessed deposits for amounts smaller
				// than MinimumDeposit to match how this is done on finalisation
				continue;
			}

			let prewitnessed_deposit_id =
				PrewitnessedDepositIdCounter::<T, I>::mutate(|id| -> u64 {
					*id = id.saturating_add(1);
					*id
				});

			let DepositChannelDetails { deposit_channel, action, boost_fee, boost_status, .. } =
				DepositChannelLookup::<T, I>::get(&deposit_address)
					.ok_or(Error::<T, I>::InvalidDepositAddress)?;

			let channel_id = deposit_channel.channel_id;

			PrewitnessedDeposits::<T, I>::insert(
				channel_id,
				prewitnessed_deposit_id,
				PrewitnessedDeposit {
					asset,
					amount,
					deposit_address: deposit_address.clone(),
					block_height,
					deposit_details: deposit_details.clone(),
				},
			);

			// Only boost on non-zero fee and if the channel isn't already boosted:
			if T::SafeMode::get().boost_deposits_enabled &&
				boost_fee > 0 && !matches!(boost_status, BoostStatus::Boosted { .. })
			{
				match Self::try_boosting(asset, amount, boost_fee, prewitnessed_deposit_id) {
					Ok(BoostOutput { used_pools, total_fee: boost_fee_amount }) => {
						DepositChannelLookup::<T, I>::mutate(&deposit_address, |details| {
							if let Some(details) = details {
								details.boost_status = BoostStatus::Boosted {
									prewitnessed_deposit_id,
									pools: used_pools.keys().cloned().collect(),
								};
							}
						});

						let amount_after_boost_fee = amount.saturating_sub(boost_fee_amount);

						// Note that ingress fee is deducted at the time of boosting rather than the
						// time the deposit is finalised (which allows us to perform the channel
						// action immediately):
						let AmountAndFeesWithheld { amount_after_fees, fees_withheld: ingress_fee } =
							Self::withhold_ingress_or_egress_fee(
								IngressOrEgress::Ingress,
								asset,
								amount_after_boost_fee,
							);

						let action = Self::perform_channel_action(
							action,
							deposit_channel,
							amount_after_fees,
							block_height,
						)?;

						Self::deposit_event(Event::DepositBoosted {
							deposit_address: deposit_address.clone(),
							asset,
							amounts: used_pools,
							prewitnessed_deposit_id,
							channel_id,
							deposit_details: deposit_details.clone(),
							ingress_fee,
							boost_fee: boost_fee_amount,
							action,
						});
					},
					Err(err) => {
						Self::deposit_event(Event::InsufficientBoostLiquidity {
							prewitnessed_deposit_id,
							asset,
							amount_attempted: amount,
							channel_id,
						});
						log::debug!(
							"Deposit (id: {prewitnessed_deposit_id}) of {amount:?} {asset:?} and boost fee {boost_fee} could not be boosted: {err:?}"
						);
					},
				}
			}
		}
		Ok(())
	}

	fn perform_channel_action(
		action: ChannelAction<T::AccountId>,
		DepositChannel { asset, address: deposit_address, channel_id, .. }: DepositChannel<
			T::TargetChain,
		>,
		amount_after_fees: TargetChainAmount<T, I>,
		block_height: TargetChainBlockNumber<T, I>,
	) -> Result<DepositAction<T::AccountId>, DispatchError> {
		let action = match action {
			ChannelAction::LiquidityProvision { lp_account, .. } => {
				T::LpBalance::add_deposit(&lp_account, asset.into(), amount_after_fees.into())?;

				DepositAction::LiquidityProvision { lp_account }
			},
			ChannelAction::Swap { destination_address, destination_asset, broker_fees, .. } =>
				DepositAction::Swap {
					swap_id: T::SwapDepositHandler::schedule_swap_from_channel(
						<<T::TargetChain as Chain>::ChainAccount as IntoForeignChainAddress<
							T::TargetChain,
						>>::into_foreign_chain_address(deposit_address.clone()),
						block_height.into(),
						asset.into(),
						destination_asset,
						amount_after_fees.into(),
						destination_address,
						broker_fees,
						channel_id,
					),
				},
			ChannelAction::CcmTransfer {
				destination_asset,
				destination_address,
				channel_metadata,
				..
			} => {
				if let Ok(CcmSwapIds { principal_swap_id, gas_swap_id }) =
					T::CcmHandler::on_ccm_deposit(
						asset.into(),
						amount_after_fees.into(),
						destination_asset,
						destination_address,
						CcmDepositMetadata {
							source_chain: asset.into(),
							source_address: None,
							channel_metadata,
						},
						SwapOrigin::DepositChannel {
							deposit_address: T::AddressConverter::to_encoded_address(
								<T::TargetChain as Chain>::ChainAccount::into_foreign_chain_address(
									deposit_address.clone(),
								),
							),
							channel_id,
							deposit_block_height: block_height.into(),
						},
					) {
					DepositAction::CcmTransfer { principal_swap_id, gas_swap_id }
				} else {
					DepositAction::NoAction
				}
			},
		};

		Ok(action)
	}

	/// Completes a single deposit request.
	#[transactional]
	fn process_single_deposit(
		deposit_address: TargetChainAccount<T, I>,
		asset: TargetChainAsset<T, I>,
		deposit_amount: TargetChainAmount<T, I>,
		deposit_details: <T::TargetChain as Chain>::DepositDetails,
		block_height: TargetChainBlockNumber<T, I>,
	) -> DispatchResult {
		let deposit_channel_details = DepositChannelLookup::<T, I>::get(&deposit_address)
			.ok_or(Error::<T, I>::InvalidDepositAddress)?;

		let channel_id = deposit_channel_details.deposit_channel.channel_id;

		if DepositChannelPool::<T, I>::get(channel_id).is_some() {
			log_or_panic!(
				"Deposit channel {} should not be in the recycled address pool if it's active",
				channel_id
			);
			#[cfg(not(debug_assertions))]
			return Err(Error::<T, I>::InvalidDepositAddress.into())
		}

		ensure!(
			deposit_channel_details.deposit_channel.asset == asset,
			Error::<T, I>::AssetMismatch
		);

		// TODO: only apply this check if the deposit hasn't been boosted
		// already (in case MinimumDeposit increases after some small deposit
		// is boosted)?

		if deposit_amount < MinimumDeposit::<T, I>::get(asset) {
			// If the deposit amount is below the minimum allowed, the deposit is ignored.
			// TODO: track these funds somewhere, for example add them to the withheld fees.
			Self::deposit_event(Event::<T, I>::DepositIgnored {
				deposit_address,
				asset,
				amount: deposit_amount,
				deposit_details,
				reason: DepositIgnoredReason::BelowMinimumDeposit,
			});
			return Ok(())
		}

		ScheduledEgressFetchOrTransfer::<T, I>::append(FetchOrTransfer::<T::TargetChain>::Fetch {
			asset,
			deposit_address: deposit_address.clone(),
			deposit_fetch_id: None,
			amount: deposit_amount,
		});
		Self::deposit_event(Event::<T, I>::DepositFetchesScheduled { channel_id, asset });

		// Add the deposit to the balance.
		T::DepositHandler::on_deposit_made(
			deposit_details.clone(),
			deposit_amount,
			&deposit_channel_details.deposit_channel,
		);

		let maybe_boost_to_process =
			if let BoostStatus::Boosted { prewitnessed_deposit_id, pools } =
				deposit_channel_details.boost_status
			{
				// We are expecting a boost, but check if the amount is matching
				match PrewitnessedDeposits::<T, I>::get(channel_id, prewitnessed_deposit_id) {
					Some(boosted_deposit) if boosted_deposit.amount == deposit_amount => {
						// Deposit matches boosted deposit, process as boosted
						Some((prewitnessed_deposit_id, pools))
					},
					Some(_) => {
						// Boosted deposit is found but the amounts didn't match, the deposit
						// should be processed as not boosted.
						None
					},
					None => {
						log_or_panic!("Could not find deposit by prewitness deposit id: {prewitnessed_deposit_id}");
						// This is unexpected since we always add a prewitnessed deposit at the
						// same time as boosting it! Because we won't be able to confirm if the
						// amount is correct, we fallback to processing the deposit as not boosted.
						None
					},
				}
			} else {
				// The channel is not even boosted, so we process the deposit as not boosted
				None
			};

		if let Some((prewitnessed_deposit_id, used_pools)) = maybe_boost_to_process {
			PrewitnessedDeposits::<T, I>::remove(channel_id, prewitnessed_deposit_id);

			// Note that ingress fee is not payed here, as it has already been payed at the time
			// of boosting
			DepositBalances::<T, I>::mutate(asset, |deposits| {
				deposits.register_deposit(deposit_amount)
			});

			for boost_tier in used_pools {
				BoostPools::<T, I>::mutate(asset, boost_tier, |maybe_pool| {
					if let Some(pool) = maybe_pool {
						for (booster_id, finalised_withdrawn_amount) in
							pool.on_finalised_deposit(prewitnessed_deposit_id)
						{
							if let Err(err) = T::LpBalance::try_credit_account(
								&booster_id,
								asset.into(),
								finalised_withdrawn_amount.into(),
							) {
								log_or_panic!(
									"Failed to credit booster account {:?} after unlock of {finalised_withdrawn_amount:?} {asset:?}: {:?}",
									booster_id, err
								);
							}
						}
					}
				});
			}

			// This allows the channel to be boosted again:
			DepositChannelLookup::<T, I>::mutate(&deposit_address, |details| {
				if let Some(details) = details {
					details.boost_status = BoostStatus::NotBoosted;
				}
			});

			Self::deposit_event(Event::DepositFinalised {
				deposit_address,
				asset,
				amount: deposit_amount,
				deposit_details,
				ingress_fee: 0u32.into(),
				action: DepositAction::BoostersCredited { prewitnessed_deposit_id },
				channel_id,
			});
		} else {
			// If the deposit isn't boosted, we don't care which prewitness deposit we remove
			// (in case there are multiple on the channel), as long is the amounts match:
			if let Some((prewitnessed_deposit_id, _)) =
				PrewitnessedDeposits::<T, I>::iter_prefix(channel_id)
					.find(|(_id, deposit)| deposit.amount == deposit_amount)
			{
				PrewitnessedDeposits::<T, I>::remove(channel_id, prewitnessed_deposit_id);
			}

			let AmountAndFeesWithheld { amount_after_fees, fees_withheld } =
				Self::withhold_ingress_or_egress_fee(
					IngressOrEgress::Ingress,
					deposit_channel_details.deposit_channel.asset,
					deposit_amount,
				);

			DepositBalances::<T, I>::mutate(asset, |deposits| {
				deposits.register_deposit(amount_after_fees)
			});

			if amount_after_fees.is_zero() {
				Self::deposit_event(Event::<T, I>::DepositIgnored {
					deposit_address,
					asset,
					amount: deposit_amount,
					deposit_details,
					reason: DepositIgnoredReason::NotEnoughToPayFees,
				});
			} else {
				let deposit_action = Self::perform_channel_action(
					deposit_channel_details.action,
					deposit_channel_details.deposit_channel,
					amount_after_fees,
					block_height,
				)?;

				Self::deposit_event(Event::DepositFinalised {
					deposit_address,
					asset,
					amount: deposit_amount,
					deposit_details,
					ingress_fee: fees_withheld,
					action: deposit_action,
					channel_id,
				});
			}
		}

		Ok(())
	}

	fn expiry_and_recycle_block_height(
	) -> (TargetChainBlockNumber<T, I>, TargetChainBlockNumber<T, I>, TargetChainBlockNumber<T, I>)
	{
		let current_height = T::ChainTracking::get_block_height();
		debug_assert!(<T::TargetChain as Chain>::is_block_witness_root(current_height));

		let lifetime = DepositChannelLifetime::<T, I>::get();

		let expiry_height = <T::TargetChain as Chain>::saturating_block_witness_next(
			current_height.saturating_add(lifetime),
		);
		let recycle_height = <T::TargetChain as Chain>::saturating_block_witness_next(
			expiry_height.saturating_add(lifetime),
		);

		debug_assert!(current_height < expiry_height);
		debug_assert!(expiry_height < recycle_height);

		(current_height, expiry_height, recycle_height)
	}

	/// Opens a channel for the given asset and registers it with the given action.
	///
	/// May re-use an existing deposit address, depending on chain configuration.
	///
	/// The requester must have enough FLIP available to pay the channel opening fee.
	#[allow(clippy::type_complexity)]
	fn open_channel(
		requester: &T::AccountId,
		source_asset: TargetChainAsset<T, I>,
		action: ChannelAction<T::AccountId>,
		boost_fee: BasisPoints,
	) -> Result<
		(ChannelId, TargetChainAccount<T, I>, TargetChainBlockNumber<T, I>, T::Amount),
		DispatchError,
	> {
		ensure!(T::SafeMode::get().deposits_enabled, Error::<T, I>::DepositChannelCreationDisabled);
		let channel_opening_fee = ChannelOpeningFee::<T, I>::get();
		T::FeePayment::try_burn_fee(requester, channel_opening_fee)?;
		Self::deposit_event(Event::<T, I>::ChannelOpeningFeePaid { fee: channel_opening_fee });

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
				DepositChannel::generate_new::<T::AddressDerivation>(next_channel_id, source_asset)
					.map_err(|e| match e {
						AddressDerivationError::MissingPolkadotVault =>
							Error::<T, I>::MissingPolkadotVault,
						AddressDerivationError::MissingBitcoinVault =>
							Error::<T, I>::MissingBitcoinVault,
						AddressDerivationError::BitcoinChannelIdTooLarge =>
							Error::<T, I>::BitcoinChannelIdTooLarge,
					})?,
				next_channel_id,
			)
		};

		let deposit_address = deposit_channel.address.clone();

		let (current_height, expiry_height, recycle_height) =
			Self::expiry_and_recycle_block_height();

		DepositChannelRecycleBlocks::<T, I>::append((recycle_height, deposit_address.clone()));

		DepositChannelLookup::<T, I>::insert(
			&deposit_address,
			DepositChannelDetails {
				deposit_channel,
				opened_at: current_height,
				expires_at: expiry_height,
				action,
				boost_fee,
				boost_status: BoostStatus::NotBoosted,
			},
		);

		Ok((channel_id, deposit_address, expiry_height, channel_opening_fee))
	}

	pub fn get_failed_call(broadcast_id: BroadcastId) -> Option<FailedForeignChainCall> {
		let epoch = T::EpochInfo::epoch_index();
		FailedForeignChainCalls::<T, I>::get(epoch)
			.iter()
			.find(|ccm| ccm.broadcast_id == broadcast_id)
			.cloned()
	}

	/// Withholds the fee for a given amount.
	///
	/// Returns the remaining amount after the fee has been withheld, and the fee itself, both
	/// measured in units of the input asset. A swap may be scheduled to convert the fee into the
	/// gas asset.
	fn withhold_ingress_or_egress_fee(
		ingress_or_egress: IngressOrEgress,
		asset: TargetChainAsset<T, I>,
		available_amount: TargetChainAmount<T, I>,
	) -> AmountAndFeesWithheld<T, I> {
		let fee_estimate = match ingress_or_egress {
			IngressOrEgress::Ingress => T::ChainTracking::estimate_ingress_fee(asset),
			IngressOrEgress::Egress => T::ChainTracking::estimate_egress_fee(asset),
		};

		let fees_withheld = if asset == <T::TargetChain as Chain>::GAS_ASSET {
			// No need to schedule a swap for gas, it's already in the gas asset.
			Self::accrue_withheld_fee(asset, sp_std::cmp::min(fee_estimate, available_amount));
			fee_estimate
		} else {
			let transaction_fee = sp_std::cmp::min(T::AssetConverter::calculate_input_for_gas_output::<T::TargetChain>(
				asset,
				fee_estimate,
			)
			.unwrap_or_else(|| {
				log::warn!("Unable to convert input to gas for input of {available_amount:?} ${asset:?}. Ignoring ingress egress fees.");
				<T::TargetChain as Chain>::ChainAmount::zero()
			}), available_amount);

			if !transaction_fee.is_zero() {
				T::SwapQueueApi::schedule_swap(
					asset.into(),
					<T::TargetChain as Chain>::GAS_ASSET.into(),
					transaction_fee.into(),
					SwapType::IngressEgressFee,
				);
			}

			transaction_fee
		};

		AmountAndFeesWithheld::<T, I> {
			amount_after_fees: available_amount.saturating_sub(fees_withheld),
			fees_withheld,
		}
	}
}

impl<T: Config<I>, I: 'static> EgressApi<T::TargetChain> for Pallet<T, I> {
	type EgressError = Error<T, I>;

	fn schedule_egress(
		asset: TargetChainAsset<T, I>,
		amount: TargetChainAmount<T, I>,
		destination_address: TargetChainAccount<T, I>,
		maybe_ccm_with_gas_budget: Option<(CcmDepositMetadata, TargetChainAmount<T, I>)>,
	) -> Result<ScheduledEgressDetails<T::TargetChain>, Error<T, I>> {
		let result = EgressIdCounter::<T, I>::try_mutate(|id_counter| {
			*id_counter = id_counter.saturating_add(1);
			let egress_id = (<T as Config<I>>::TargetChain::get(), *id_counter);

			match maybe_ccm_with_gas_budget {
				Some((
					CcmDepositMetadata { source_chain, source_address, channel_metadata },
					gas_budget,
				)) => {
					ScheduledEgressCcm::<T, I>::append(CrossChainMessage {
						egress_id,
						asset,
						amount,
						destination_address: destination_address.clone(),
						message: channel_metadata.message,
						cf_parameters: channel_metadata.cf_parameters,
						source_chain,
						source_address,
						gas_budget,
					});

					// The ccm gas budget is already in terms of the swap asset.
					Ok(ScheduledEgressDetails::new(*id_counter, amount, gas_budget))
				},
				None => {
					let AmountAndFeesWithheld { amount_after_fees, fees_withheld } =
						Self::withhold_ingress_or_egress_fee(
							IngressOrEgress::Egress,
							asset,
							amount,
						);

					if amount_after_fees >=
						EgressDustLimit::<T, I>::get(asset).unique_saturated_into() ||
						// We always want to benchmark the success case.
						cfg!(all(feature = "runtime-benchmarks", not(test)))
					{
						let egress_details = ScheduledEgressDetails::new(
							*id_counter,
							amount_after_fees,
							fees_withheld,
						);

						ScheduledEgressFetchOrTransfer::<T, I>::append({
							FetchOrTransfer::<T::TargetChain>::Transfer {
								asset,
								destination_address: destination_address.clone(),
								amount: amount_after_fees,
								egress_id: egress_details.egress_id,
							}
						});

						Ok(egress_details)
					} else {
						// TODO: Consider tracking the ignored egresses somewhere.
						// For example, store the egress and try it again later when fees have
						// dropped?
						Err(Error::<T, I>::BelowEgressDustLimit)
					}
				},
			}
		});

		if let Ok(ScheduledEgressDetails { egress_amount, .. }) = result {
			// Only the egress_amount will be transferred. The fee was converted to the native
			// asset and will be consumed in terms of the native asset.
			DepositBalances::<T, I>::mutate(asset, |tracker| {
				tracker.register_transfer(egress_amount);
			});
		};

		result
	}
}

impl<T: Config<I>, I: 'static> DepositApi<T::TargetChain> for Pallet<T, I> {
	type AccountId = T::AccountId;
	type Amount = T::Amount;

	// This should be callable by the LP pallet.
	fn request_liquidity_deposit_address(
		lp_account: T::AccountId,
		source_asset: TargetChainAsset<T, I>,
		boost_fee: BasisPoints,
	) -> Result<
		(ChannelId, ForeignChainAddress, <T::TargetChain as Chain>::ChainBlockNumber, Self::Amount),
		DispatchError,
	> {
		let (channel_id, deposit_address, expiry_block, channel_opening_fee) = Self::open_channel(
			&lp_account,
			source_asset,
			ChannelAction::LiquidityProvision { lp_account: lp_account.clone() },
			boost_fee,
		)?;

		Ok((
			channel_id,
			<T::TargetChain as Chain>::ChainAccount::into_foreign_chain_address(deposit_address),
			expiry_block,
			channel_opening_fee,
		))
	}

	// This should only be callable by the broker.
	fn request_swap_deposit_address(
		source_asset: TargetChainAsset<T, I>,
		destination_asset: Asset,
		destination_address: ForeignChainAddress,
		broker_fees: Beneficiaries<Self::AccountId>,
		broker_id: T::AccountId,
		channel_metadata: Option<CcmChannelMetadata>,
		boost_fee: BasisPoints,
	) -> Result<
		(ChannelId, ForeignChainAddress, <T::TargetChain as Chain>::ChainBlockNumber, Self::Amount),
		DispatchError,
	> {
		let (channel_id, deposit_address, expiry_height, channel_opening_fee) = Self::open_channel(
			&broker_id,
			source_asset,
			match channel_metadata {
				Some(msg) => ChannelAction::CcmTransfer {
					destination_asset,
					destination_address,
					channel_metadata: msg,
				},
				None => ChannelAction::Swap { destination_asset, destination_address, broker_fees },
			},
			boost_fee,
		)?;

		Ok((
			channel_id,
			<T::TargetChain as Chain>::ChainAccount::into_foreign_chain_address(deposit_address),
			expiry_height,
			channel_opening_fee,
		))
	}
}

impl<T: Config<I>, I: 'static> IngressEgressFeeApi<T::TargetChain> for Pallet<T, I> {
	fn accrue_withheld_fee(
		_asset: <T::TargetChain as Chain>::ChainAsset,
		fee: TargetChainAmount<T, I>,
	) {
		if !fee.is_zero() {
			WithheldTransactionFees::<T, I>::mutate(<T::TargetChain as Chain>::GAS_ASSET, |fees| {
				fees.saturating_accrue(fee);
			});
			// Since we credit the fees to the withheld fees, we need to take these from somewhere,
			// ie. we effectively have transferred them from the vault.
			DepositBalances::<T, I>::mutate(<T::TargetChain as Chain>::GAS_ASSET, |tracker| {
				tracker.register_transfer(fee);
			});
		}
	}
}
