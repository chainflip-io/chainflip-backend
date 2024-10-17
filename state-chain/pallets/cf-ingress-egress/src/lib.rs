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

use cf_chains::{
	address::{
		AddressConverter, AddressDerivationApi, AddressDerivationError, IntoForeignChainAddress,
	},
	assets::any::GetChainAssetMap,
	ccm_checker::CcmValidityCheck,
	AllBatch, AllBatchError, CcmCfParameters, CcmChannelMetadata, CcmDepositMetadata,
	CcmFailReason, CcmMessage, Chain, ChannelLifecycleHooks, ChannelRefundParameters,
	ConsolidateCall, DepositChannel, ExecutexSwapAndCall, FetchAssetParams, ForeignChainAddress,
	SwapOrigin, TransferAssetParams,
};
use cf_primitives::{
	Asset, AssetAmount, BasisPoints, Beneficiaries, BoostPoolTier, BroadcastId, ChannelId,
	DcaParameters, EgressCounter, EgressId, EpochIndex, ForeignChain, PrewitnessedDepositId,
	SwapRequestId, ThresholdSignatureRequestId, TransactionHash,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AccountRoleRegistry, AdjustedFeeEstimationApi, AssetConverter,
	AssetWithholding, BalanceApi, BoostApi, Broadcaster, Chainflip, DepositApi, EgressApi,
	EpochInfo, FeePayment, FetchesTransfersLimitProvider, GetBlockHeight, IngressEgressFeeApi,
	IngressSink, IngressSource, NetworkEnvironmentProvider, OnDeposit, PoolApi,
	ScheduledEgressDetails, SwapLimitsProvider, SwapRequestHandler, SwapRequestType,
};
use frame_support::{
	pallet_prelude::{OptionQuery, *},
	sp_runtime::{traits::Zero, DispatchError, Permill, Saturating},
	transactional,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use scale_info::{
	build::{Fields, Variants},
	Path, Type,
};
use sp_runtime::traits::UniqueSaturatedInto;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	marker::PhantomData,
	vec,
	vec::Vec,
};
pub use weights::WeightInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum BoostStatus<ChainAmount> {
	// If a (pre-witnessed) deposit on a channel has been boosted, we record
	// its id, amount, and the pools that participated in boosting it.
	Boosted {
		prewitnessed_deposit_id: PrewitnessedDepositId,
		pools: Vec<BoostPoolTier>,
		amount: ChainAmount,
	},
	NotBoosted,
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

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(15);

impl_pallet_safe_mode! {
	PalletSafeMode<I>;
	boost_deposits_enabled,
	add_boost_funds_enabled,
	stop_boosting_enabled,
	deposits_enabled,
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
	CloneNoBound, RuntimeDebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, MaxEncodedLen,
)]
pub enum PalletConfigUpdate<T: Config<I>, I: 'static> {
	/// Set the fixed fee that is burned when opening a channel, denominated in Flipperinos.
	ChannelOpeningFee { fee: T::Amount },
	/// Set the minimum deposit allowed for a particular asset.
	SetMinimumDeposit { asset: TargetChainAsset<T, I>, minimum_deposit: TargetChainAmount<T, I> },
}

macro_rules! append_chain_to_name {
	($name:ident) => {
		match T::TargetChain::NAME {
			"Ethereum" => concat!(stringify!($name), "Ethereum"),
			"Polkadot" => concat!(stringify!($name), "Polkadot"),
			"Bitcoin" => concat!(stringify!($name), "Bitcoin"),
			"Arbitrum" => concat!(stringify!($name), "Arbitrum"),
			"Solana" => concat!(stringify!($name), "Solana"),
			_ => concat!(stringify!($name), "Other"),
		}
	};
}

impl<T, I> TypeInfo for PalletConfigUpdate<T, I>
where
	T: Config<I>,
	I: 'static,
{
	type Identity = Self;
	fn type_info() -> Type {
		Type::builder()
			.path(Path::new(append_chain_to_name!(PalletConfigUpdate), module_path!()))
			.variant(
				Variants::new()
					.variant("ChannelOpeningFee", |v| {
						v.index(0)
							.fields(Fields::named().field(|f| f.ty::<T::Amount>().name("fee")))
					})
					.variant(append_chain_to_name!(SetMinimumDeposit), |v| {
						v.index(1).fields(
							Fields::named()
								.field(|f| f.ty::<TargetChainAsset<T, I>>().name("asset"))
								.field(|f| {
									f.ty::<TargetChainAmount<T, I>>().name("minimum_deposit")
								}),
						)
					}),
			)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use cf_chains::{
		address::EncodedAddress, CcmDepositMetadataEncoded, ExecutexSwapAndCall, TransferFallback,
	};
	use cf_primitives::{BroadcastId, EpochIndex};
	use cf_traits::{OnDeposit, SwapLimitsProvider};
	use core::marker::PhantomData;
	use frame_support::traits::{ConstU128, EnsureOrigin, IsType};
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
		pub boost_status: BoostStatus<TargetChainAmount<T, I>>,
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
			refund_params: Option<ChannelRefundParameters>,
			dca_params: Option<DcaParameters>,
		},
		LiquidityProvision {
			lp_account: AccountId,
		},
		CcmTransfer {
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			broker_fees: Beneficiaries<AccountId>,
			channel_metadata: CcmChannelMetadata,
			refund_params: Option<ChannelRefundParameters>,
			dca_params: Option<DcaParameters>,
		},
	}

	/// Contains identifying information about the particular actions that have occurred for a
	/// particular deposit.
	#[derive(Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum DepositAction<AccountId> {
		Swap { swap_request_id: SwapRequestId },
		LiquidityProvision { lp_account: AccountId },
		CcmTransfer { swap_request_id: SwapRequestId },
		NoAction,
		BoostersCredited { prewitnessed_deposit_id: PrewitnessedDepositId },
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

		/// Sets if the pallet should automatically manage the closing of channels.
		const MANAGE_CHANNEL_LIFETIME: bool;

		/// A hook to tell witnesses to start witnessing an opened channel.
		type IngressSource: IngressSource<Chain = Self::TargetChain>;

		/// Marks which chain this pallet is interacting with.
		type TargetChain: Chain + Get<ForeignChain>;

		/// Generates deposit addresses.
		type AddressDerivation: AddressDerivationApi<Self::TargetChain>;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		type Balance: BalanceApi<AccountId = Self::AccountId>;

		type PoolApi: PoolApi<AccountId = <Self as frame_system::Config>::AccountId>;

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

		type SwapRequestHandler: SwapRequestHandler<AccountId = Self::AccountId>;

		type AssetWithholding: AssetWithholding;

		type FetchesTransfersLimitProvider: FetchesTransfersLimitProvider;

		/// Safe Mode access.
		type SafeMode: Get<PalletSafeMode<I>>;

		type SwapLimitsProvider: SwapLimitsProvider;

		/// For checking if the CCM message passed in is valid.
		type CcmValidityChecker: CcmValidityCheck;
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
	pub type ScheduledEgressFetchOrTransfer<T: Config<I>, I: 'static = ()> =
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
	pub type DepositChannelRecycleBlocks<T: Config<I>, I: 'static = ()> =
		StorageValue<_, ChannelRecycleQueue<T, I>, ValueQuery>;

	// Determines the number of block confirmations is required for a block on
	// an external chain before CFE can submit any witness extrinsics for it.
	#[pallet::storage]
	#[pallet::getter(fn witness_safety_margin)]
	pub type WitnessSafetyMargin<T: Config<I>, I: 'static = ()> =
		StorageValue<_, TargetChainBlockNumber<T, I>, OptionQuery>;

	/// The fixed fee charged for opening a channel, in Flipperinos.
	#[pallet::storage]
	#[pallet::getter(fn channel_opening_fee)]
	pub type ChannelOpeningFee<T: Config<I>, I: 'static = ()> =
		StorageValue<_, T::Amount, ValueQuery>;

	/// Stores the latest prewitnessed deposit id used.
	#[pallet::storage]
	pub type PrewitnessedDepositIdCounter<T: Config<I>, I: 'static = ()> =
		StorageValue<_, PrewitnessedDepositId, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		DepositFinalised {
			deposit_address: TargetChainAccount<T, I>,
			asset: TargetChainAsset<T, I>,
			amount: TargetChainAmount<T, I>,
			block_height: TargetChainBlockNumber<T, I>,
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
			error: cf_chains::ExecutexSwapAndCallError,
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
			block_height: TargetChainBlockNumber<T, I>,
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
		BoostedDepositLost {
			prewitnessed_deposit_id: PrewitnessedDepositId,
			amount: TargetChainAmount<T, I>,
		},
		CcmFailed {
			reason: CcmFailReason,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadataEncoded,
			origin: SwapOrigin,
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
		/// Solana address derivation error.
		SolanaAddressDerivationError,
		/// Solana's Environment variables cannot be loaded via the SolanaEnvironment.
		MissingSolanaApiEnvironment,
		/// You cannot add 0 to a boost pool.
		AddBoostAmountMustBeNonZero,
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
		/// CCM parameters from a contract swap failed validity check.
		InvalidCcm,
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		/// Recycle addresses if we can
		fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let mut used_weight = Weight::zero();

			// Approximate weight calculation: r/w DepositChannelLookup + w DepositChannelPool
			let recycle_weight_per_address =
				frame_support::weights::constants::RocksDbWeight::get().reads_writes(1, 2);

			let maximum_addresses_to_recycle = remaining_weight
				.ref_time()
				.checked_div(recycle_weight_per_address.ref_time())
				.unwrap_or_default()
				.saturated_into::<usize>();

			// In some instances, like Solana, the channel lifetime is managed by the electoral
			// system.
			if T::MANAGE_CHANNEL_LIFETIME {
				let addresses_to_recycle =
					DepositChannelRecycleBlocks::<T, I>::mutate(|recycle_queue| {
						if recycle_queue.is_empty() {
							vec![]
						} else {
							Self::take_recyclable_addresses(
								recycle_queue,
								maximum_addresses_to_recycle,
								T::ChainTracking::get_block_height(),
							)
						}
					});

				// Add weight for the DepositChannelRecycleBlocks read/write plus the
				// DepositChannelLookup read/writes in the for loop below
				used_weight = used_weight.saturating_add(
					frame_support::weights::constants::RocksDbWeight::get().reads_writes(
						(addresses_to_recycle.len() + 1) as u64,
						(addresses_to_recycle.len() + 1) as u64,
					),
				);

				for address in addresses_to_recycle {
					Self::recycle_channel(&mut used_weight, address);
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
						T::Broadcaster::expire_broadcast(call.broadcast_id);
						Self::deposit_event(Event::<T, I>::FailedForeignChainCallExpired {
							broadcast_id: call.broadcast_id,
						});
					},
					// Previous epoch, signature is invalid. Re-sign but don't broadcast.
					1 => match T::Broadcaster::re_sign_broadcast(call.broadcast_id, false, false) {
						Ok(threshold_signature_id) => {
							Self::deposit_event(Event::<T, I>::FailedForeignChainCallResigned {
								broadcast_id: call.broadcast_id,
								threshold_signature_id,
							});
							FailedForeignChainCalls::<T, I>::append(current_epoch, call);
						},
						Err(e) => {
							// This can happen if a broadcast is still pending
							// since the previous epoch.
							// TODO: make sure this can't happen.
							log::warn!(
								"Failed CCM call for broadcast {} not re-signed: {:?}",
								call.broadcast_id,
								e
							);
						},
					},
					// Current epoch, shouldn't be possible.
					_ => {
						log_or_panic!(
							"Logic error: Found call for current epoch in prevoius epoch's failed calls: broadcast_id: {}.",
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
				Err(err) => {
					log_or_panic!(
						"Failed to construct TransferFallback call. Asset: {:?}, amount: {:?}, Destination: {:?}, Error: {:?}",
						asset, amount, destination_address, err
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
			ensure!(amount > Zero::zero(), Error::<T, I>::AddBoostAmountMustBeNonZero);

			// `try_debit_account` does not account for any unswept open positions, so we sweep to
			// ensure we have the funds in our free balance before attempting to debit the account.
			T::PoolApi::sweep(&booster_id)?;

			T::Balance::try_debit_account(&booster_id, asset.into(), amount.into())?;

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

			T::Balance::try_credit_account(&booster, asset.into(), unlocked_amount.into())?;

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

		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::contract_swap_request())]
		pub fn contract_swap_request(
			origin: OriginFor<T>,
			from: Asset,
			to: Asset,
			deposit_amount: AssetAmount,
			destination_address: EncodedAddress,
			tx_hash: TransactionHash,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let destination_address_internal =
				match T::AddressConverter::decode_and_validate_address_for_asset(
					destination_address.clone(),
					to,
				) {
					Ok(address) => address,
					Err(err) => {
						log::warn!("Failed to process contract swap due to invalid destination address. Tx hash: {tx_hash:?}. Error: {err:?}");
						return Ok(());
					},
				};

			T::SwapRequestHandler::init_swap_request(
				from,
				deposit_amount,
				to,
				SwapRequestType::Regular { output_address: destination_address_internal.clone() },
				Default::default(),
				// NOTE: FoK not yet supported for swaps from the contract
				None,
				// NOTE: DCA not yet supported for swaps from the contract
				None,
				SwapOrigin::Vault { tx_hash },
			);

			Ok(())
		}

		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::contract_ccm_swap_request())]
		pub fn contract_ccm_swap_request(
			origin: OriginFor<T>,
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
			tx_hash: TransactionHash,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let swap_origin = SwapOrigin::Vault { tx_hash };

			let ccm_failed = |reason| {
				log::warn!("Failed to process CCM. Tx hash: {:?}, Reason: {:?}", tx_hash, reason);

				Self::deposit_event(Event::<T, I>::CcmFailed {
					reason,
					destination_address: destination_address.clone(),
					deposit_metadata: deposit_metadata.clone().to_encoded::<T::AddressConverter>(),
					origin: swap_origin.clone(),
				});
			};

			if T::CcmValidityChecker::check_and_decode(
				&deposit_metadata.channel_metadata,
				destination_asset,
			)
			.is_err()
			{
				ccm_failed(CcmFailReason::InvalidMetadata);
				return Ok(());
			};

			let destination_address_internal =
				match T::AddressConverter::decode_and_validate_address_for_asset(
					destination_address.clone(),
					destination_asset,
				) {
					Ok(address) => address,
					Err(_) => {
						ccm_failed(CcmFailReason::InvalidDestinationAddress);
						return Ok(());
					},
				};

			let ccm_swap_metadata = match deposit_metadata.clone().into_swap_metadata(
				deposit_amount,
				source_asset,
				destination_asset,
			) {
				Ok(metadata) => metadata,
				Err(reason) => {
					ccm_failed(reason);
					return Ok(())
				},
			};

			T::SwapRequestHandler::init_swap_request(
				source_asset,
				deposit_amount,
				destination_asset,
				SwapRequestType::Ccm {
					ccm_swap_metadata,
					output_address: destination_address_internal.clone(),
				},
				Default::default(),
				// NOTE: FoK not yet supported for swaps from the contract
				None,
				// NOTE: DCA not yet supported for swaps from the contract
				None,
				swap_origin,
			);

			Ok(())
		}
	}
}

impl<T: Config<I>, I: 'static> IngressSink for Pallet<T, I> {
	type Account = <T::TargetChain as Chain>::ChainAccount;
	type Asset = <T::TargetChain as Chain>::ChainAsset;
	type Amount = <T::TargetChain as Chain>::ChainAmount;
	type BlockNumber = <T::TargetChain as Chain>::ChainBlockNumber;
	type DepositDetails = <T::TargetChain as Chain>::DepositDetails;

	fn on_ingress(
		channel: Self::Account,
		asset: Self::Asset,
		amount: Self::Amount,
		block_number: Self::BlockNumber,
		details: Self::DepositDetails,
	) {
		Self::process_single_deposit(channel.clone(), asset, amount, details.clone(), block_number)
			.unwrap_or_else(|e| {
				Self::deposit_event(Event::<T, I>::DepositWitnessRejected {
					reason: e,
					deposit_witness: DepositWitness {
						deposit_address: channel,
						asset,
						amount,
						deposit_details: details,
					},
				});
			});
	}

	fn on_ingress_reverted(_channel: Self::Account, _asset: Self::Asset, _amount: Self::Amount) {}

	fn on_channel_closed(channel: Self::Account) {
		Self::recycle_channel(&mut Weight::zero(), channel);
	}
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	fn recycle_channel(used_weight: &mut Weight, address: <T::TargetChain as Chain>::ChainAccount) {
		if let Some(DepositChannelDetails { deposit_channel, boost_status, .. }) =
			DepositChannelLookup::<T, I>::take(address)
		{
			if let Some(state) = deposit_channel.state.maybe_recycle() {
				DepositChannelPool::<T, I>::insert(
					deposit_channel.channel_id,
					DepositChannel { state, ..deposit_channel },
				);
				*used_weight = used_weight.saturating_add(
					frame_support::weights::constants::RocksDbWeight::get().reads_writes(0, 1),
				);
			}

			if let BoostStatus::Boosted { prewitnessed_deposit_id, pools, amount } = boost_status {
				for pool_tier in pools {
					BoostPools::<T, I>::mutate(deposit_channel.asset, pool_tier, |pool| {
						if let Some(pool) = pool {
							let affected_boosters_count =
								pool.process_deposit_as_lost(prewitnessed_deposit_id);
							used_weight.saturating_accrue(T::WeightInfo::process_deposit_as_lost(
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
				Self::deposit_event(Event::<T, I>::BoostedDepositLost {
					prewitnessed_deposit_id,
					amount,
				})
			}
		}
	}

	fn take_recyclable_addresses(
		channel_recycle_blocks: &mut ChannelRecycleQueue<T, I>,
		maximum_addresses_to_take: usize,
		current_block_height: TargetChainBlockNumber<T, I>,
	) -> Vec<TargetChainAccount<T, I>> {
		channel_recycle_blocks.sort_by_key(|(block, _)| *block);
		let partition_point = sp_std::cmp::min(
			channel_recycle_blocks.partition_point(|(block, _)| *block <= current_block_height),
			maximum_addresses_to_take,
		);
		channel_recycle_blocks
			.drain(..partition_point)
			.map(|(_, address)| address)
			.collect()
	}

	fn should_fetch_or_transfer(
		maybe_no_of_fetch_or_transfers_remaining: &mut Option<usize>,
	) -> bool {
		maybe_no_of_fetch_or_transfers_remaining
			.as_mut()
			.map(|no_of_fetch_or_transfers_remaining| {
				if *no_of_fetch_or_transfers_remaining != 0 {
					*no_of_fetch_or_transfers_remaining -= 1;
					true
				} else {
					false
				}
			})
			.unwrap_or(true)
	}

	/// Take all scheduled egress requests and send them out in an `AllBatch` call.
	///
	/// Note: Egress transactions with Blacklisted assets are not sent, and kept in storage.
	#[transactional]
	fn do_egress_scheduled_fetch_transfer() -> Result<(), AllBatchError> {
		let batch_to_send: Vec<_> =
			ScheduledEgressFetchOrTransfer::<T, I>::mutate(|requests: &mut Vec<_>| {
				let mut maybe_no_of_transfers_remaining =
					T::FetchesTransfersLimitProvider::maybe_transfers_limit();
				let mut maybe_no_of_fetches_remaining =
					T::FetchesTransfersLimitProvider::maybe_fetches_limit();
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
									Self::should_fetch_or_transfer(
										&mut maybe_no_of_fetches_remaining,
									) && DepositChannelLookup::<T, I>::mutate(
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
								FetchOrTransfer::Transfer { .. } => Self::should_fetch_or_transfer(
									&mut maybe_no_of_transfers_remaining,
								),
							}
					})
					.collect()
			});

		if batch_to_send.is_empty() {
			return Ok(())
		}

		let mut fetch_params = vec![];
		let mut transfer_params = vec![];
		let mut addresses = vec![];

		for request in batch_to_send {
			match request {
				FetchOrTransfer::<T::TargetChain>::Fetch {
					asset,
					deposit_address,
					deposit_fetch_id,
					..
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
					transfer_params.push((
						TransferAssetParams { asset, amount, to: destination_address },
						egress_id,
					));
				},
			}
		}

		// Construct and send the transaction.
		match <T::ChainApiCall as AllBatch<T::TargetChain>>::new_unsigned(
			fetch_params,
			transfer_params,
		) {
			Ok(egress_transactions) => {
				egress_transactions.into_iter().for_each(|(egress_transaction, egress_ids)| {
					let broadcast_id = T::Broadcaster::threshold_sign_and_broadcast_with_callback(
						egress_transaction,
						Some(Call::finalise_ingress { addresses: addresses.clone() }.into()),
						|_| None,
					);
					Self::deposit_event(Event::<T, I>::BatchBroadcastRequested {
						broadcast_id,
						egress_ids,
					});
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
		let mut maybe_no_of_transfers_remaining =
			T::FetchesTransfersLimitProvider::maybe_ccm_limit();

		let ccms_to_send: Vec<CrossChainMessage<T::TargetChain>> =
			ScheduledEgressCcm::<T, I>::mutate(|ccms: &mut Vec<_>| {
				// Filter out disabled assets, and take up to batch_size requests to be sent.
				ccms.extract_if(|ccm| {
					!DisabledEgressAssets::<T, I>::contains_key(ccm.asset()) &&
						Self::should_fetch_or_transfer(&mut maybe_no_of_transfers_remaining)
				})
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
				ccm.cf_parameters.to_vec(),
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

			// Only boost on non-zero fee and if the channel isn't already boosted:
			if T::SafeMode::get().boost_deposits_enabled &&
				boost_fee > 0 &&
				!matches!(boost_status, BoostStatus::Boosted { .. })
			{
				match Self::try_boosting(asset, amount, boost_fee, prewitnessed_deposit_id) {
					Ok(BoostOutput { used_pools, total_fee: boost_fee_amount }) => {
						DepositChannelLookup::<T, I>::mutate(&deposit_address, |details| {
							if let Some(details) = details {
								details.boost_status = BoostStatus::Boosted {
									prewitnessed_deposit_id,
									pools: used_pools.keys().cloned().collect(),
									amount,
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
							block_height,
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
		let swap_origin = SwapOrigin::DepositChannel {
			deposit_address: T::AddressConverter::to_encoded_address(
				<T::TargetChain as Chain>::ChainAccount::into_foreign_chain_address(
					deposit_address.clone(),
				),
			),
			channel_id,
			deposit_block_height: block_height.into(),
		};

		let action = match action {
			ChannelAction::LiquidityProvision { lp_account, .. } => {
				T::Balance::try_credit_account(
					&lp_account,
					asset.into(),
					amount_after_fees.into(),
				)?;
				DepositAction::LiquidityProvision { lp_account }
			},
			ChannelAction::Swap {
				destination_address,
				destination_asset,
				broker_fees,
				refund_params,
				dca_params,
			} => {
				let swap_request_id = T::SwapRequestHandler::init_swap_request(
					asset.into(),
					amount_after_fees.into(),
					destination_asset,
					SwapRequestType::Regular { output_address: destination_address },
					broker_fees,
					refund_params,
					dca_params,
					swap_origin,
				);
				DepositAction::Swap { swap_request_id }
			},
			ChannelAction::CcmTransfer {
				destination_asset,
				destination_address,
				broker_fees,
				channel_metadata,
				refund_params,
				dca_params,
			} => {
				let deposit_metadata = CcmDepositMetadata {
					channel_metadata,
					source_chain: asset.into(),
					source_address: None,
				};
				match deposit_metadata.clone().into_swap_metadata(
					amount_after_fees.into(),
					asset.into(),
					destination_asset,
				) {
					Ok(ccm_swap_metadata) => {
						let swap_request_id = T::SwapRequestHandler::init_swap_request(
							asset.into(),
							amount_after_fees.into(),
							destination_asset,
							SwapRequestType::Ccm {
								ccm_swap_metadata,
								output_address: destination_address,
							},
							broker_fees,
							refund_params,
							dca_params,
							swap_origin,
						);
						DepositAction::CcmTransfer { swap_request_id }
					},
					Err(reason) => {
						Self::deposit_event(Event::<T, I>::CcmFailed {
							reason,
							destination_address: T::AddressConverter::to_encoded_address(
								destination_address,
							),
							deposit_metadata: deposit_metadata
								.clone()
								.to_encoded::<T::AddressConverter>(),
							origin: swap_origin.clone(),
						});
						DepositAction::NoAction
					},
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

		// We received a deposit on a channel. If channel has been boosted earlier
		// (i.e. awaiting finalisation), *and* the boosted amount matches the amount
		// in this deposit, finalise the boost by crediting boost pools with the deposit.
		// Process as non-boosted deposit otherwise:
		let maybe_boost_to_process = match deposit_channel_details.boost_status {
			BoostStatus::Boosted { prewitnessed_deposit_id, pools, amount }
				if amount == deposit_amount =>
				Some((prewitnessed_deposit_id, pools)),
			_ => None,
		};

		if let Some((prewitnessed_deposit_id, used_pools)) = maybe_boost_to_process {
			// Note that ingress fee is not payed here, as it has already been payed at the time
			// of boosting
			for boost_tier in used_pools {
				BoostPools::<T, I>::mutate(asset, boost_tier, |maybe_pool| {
					if let Some(pool) = maybe_pool {
						for (booster_id, finalised_withdrawn_amount) in
							pool.process_deposit_as_finalised(prewitnessed_deposit_id)
						{
							if let Err(err) = T::Balance::try_credit_account(
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
				block_height,
				deposit_details,
				ingress_fee: 0u32.into(),
				action: DepositAction::BoostersCredited { prewitnessed_deposit_id },
				channel_id,
			});
		} else {
			let AmountAndFeesWithheld { amount_after_fees, fees_withheld } =
				Self::withhold_ingress_or_egress_fee(
					IngressOrEgress::Ingress,
					deposit_channel_details.deposit_channel.asset,
					deposit_amount,
				);

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
					block_height,
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
		// Goals:
		// 1. When chain tracking reaches a particular block number, we want to be able to process
		//   that block immediately on the CFE.
		// 2. The CFE's need to have a consistent view of what we want to witness (channels) in
		//    order to
		//   come to consensus.

		// We open deposit channels for the block after the current chain tracking block, so that
		// the set of channels open at the *current chain tracking* block does not change after
		// chain tracking reaches that block. This achieves the second goal. We achieve the first
		// goal by using this in conjunction with waiting until the chain tracking reaches a
		// particular block before we process it on the CFE.

		// This relates directly to the code in
		// `engine/src/witness/common/chunked_chain_source/chunked_by_vault/deposit_addresses.rs`
		// and `engine/src/witness/common/chunked_chain_source/chunked_by_vault/monitored_items.rs`
		let current_height =
			T::ChainTracking::get_block_height() + <T::TargetChain as Chain>::WITNESS_PERIOD;
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
						AddressDerivationError::SolanaDerivationError { .. } =>
							Error::<T, I>::SolanaAddressDerivationError,
						AddressDerivationError::MissingSolanaApiEnvironment =>
							Error::<T, I>::MissingSolanaApiEnvironment,
					})?,
				next_channel_id,
			)
		};

		let deposit_address = deposit_channel.address.clone();

		let (current_height, expiry_height, recycle_height) =
			Self::expiry_and_recycle_block_height();

		if T::MANAGE_CHANNEL_LIFETIME {
			DepositChannelRecycleBlocks::<T, I>::append((recycle_height, deposit_address.clone()));
		}

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
		<T::IngressSource as IngressSource>::open_channel(
			deposit_address.clone(),
			source_asset,
			expiry_height,
		)?;

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
	#[allow(clippy::redundant_pattern_matching)]
	pub fn withhold_ingress_or_egress_fee(
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
				T::SwapRequestHandler::init_swap_request(
					asset.into(),
					transaction_fee.into(),
					<T::TargetChain as Chain>::GAS_ASSET.into(),
					SwapRequestType::IngressEgressFee,
					Default::default(),
					None, /* no refund params */
					None, /* no DCA */
					SwapOrigin::Internal,
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
		EgressIdCounter::<T, I>::try_mutate(|id_counter| {
			*id_counter = id_counter.saturating_add(1);
			let egress_id = (<T as Config<I>>::TargetChain::get(), *id_counter);

			match maybe_ccm_with_gas_budget {
				Some((
					CcmDepositMetadata {
						channel_metadata: CcmChannelMetadata { message, cf_parameters, .. },
						source_chain,
						source_address,
						..
					},
					gas_budget,
				)) => {
					ScheduledEgressCcm::<T, I>::append(CrossChainMessage {
						egress_id,
						asset,
						amount,
						destination_address: destination_address.clone(),
						message,
						cf_parameters,
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
		})
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
		refund_params: Option<ChannelRefundParameters>,
		dca_params: Option<DcaParameters>,
	) -> Result<
		(ChannelId, ForeignChainAddress, <T::TargetChain as Chain>::ChainBlockNumber, Self::Amount),
		DispatchError,
	> {
		if let Some(params) = &refund_params {
			T::SwapLimitsProvider::validate_refund_params(params.retry_duration)?;
		}
		if let Some(params) = &dca_params {
			T::SwapLimitsProvider::validate_dca_params(params)?;
		}

		let (channel_id, deposit_address, expiry_height, channel_opening_fee) = Self::open_channel(
			&broker_id,
			source_asset,
			match channel_metadata {
				Some(channel_metadata) => ChannelAction::CcmTransfer {
					destination_asset,
					destination_address,
					broker_fees,
					channel_metadata,
					refund_params,
					dca_params,
				},
				None => ChannelAction::Swap {
					destination_asset,
					destination_address,
					broker_fees,
					refund_params,
					dca_params,
				},
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
			T::AssetWithholding::withhold_assets(
				<T::TargetChain as Chain>::GAS_ASSET.into(),
				fee.into(),
			);
		}
	}
}

impl<T: Config<I>, I: 'static> BoostApi for Pallet<T, I> {
	type AccountId = T::AccountId;
	type AssetMap = <<T as Config<I>>::TargetChain as Chain>::ChainAssetMap<AssetAmount>;
	fn boost_pool_account_balances(who: &Self::AccountId) -> Self::AssetMap {
		Self::AssetMap::from_fn(|chain_asset| {
			BoostPools::<T, I>::iter_prefix(chain_asset).fold(0, |acc, (_tier, pool)| {
				let active: AssetAmount = pool
					.get_amounts()
					.into_iter()
					.filter(|(id, _amount)| id == who)
					.map(|(_id, amount)| amount.into())
					.sum();

				let pending: AssetAmount = pool
					.get_pending_boosts()
					.into_values()
					.map(|owed| {
						owed.get(who).map_or(0u32.into(), |owed_amount| owed_amount.total.into())
					})
					.sum();

				acc + active + pending
			})
		})
	}
}
