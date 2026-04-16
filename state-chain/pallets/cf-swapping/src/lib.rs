// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(not(feature = "std"), no_std)]

use cf_amm::{
	common::{AssetPair, Side},
	math::{Price, PriceLimits},
};
use cf_chains::{
	address::{AddressConverter, AddressError, ForeignChainAddress},
	evm::Address as EthereumAddress,
	AccountOrAddress, CcmDepositMetadataChecked, ChannelRefundParametersUncheckedEncoded,
	SwapOrigin,
};
use cf_primitives::{
	basis_points::SignedBasisPoints, AffiliateShortId, Affiliates, Asset, AssetAmount, BasisPoints,
	Beneficiaries, Beneficiary, BlockNumber, ChannelId, ForeignChain, SwapId, SwapLeg,
	SwapRequestId, FLIPPERINOS_PER_FLIP, ONE_AS_BASIS_POINTS, SECONDS_PER_BLOCK, STABLE_ASSET,
	SWAP_DELAY_BLOCKS,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AffiliateRegistry, AssetConverter, BalanceApi, Bonding,
	ChainflipNetworkInfo, ChannelIdAllocator, DepositApi, DeregistrationCheck, ExpiryBehaviour,
	FundingInfo, FundingSource, GetMinimumFunding, IngressEgressFeeApi, PriceFeedApi,
	PriceLimitsAndExpiry, SwapOutputAction, SwapParameterValidation, SwapRequestHandler,
	SwapRequestType, SwapRequestTypeEncoded, SwapType, SwappingApi,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{ConstU16, Get, Saturating},
		DispatchError, Permill, TransactionOutcome,
	},
	storage::with_transaction_unchecked,
	traits::HandleLifetime,
	transactional, Hashable,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use pallet_cf_environment::submit_runtime_call::{
	is_valid_signature, SignatureData, TransactionMetadata,
};
use serde::{Deserialize, Serialize};
use sp_arithmetic::{
	helpers_128bit::multiply_by_rational_with_rounding,
	traits::{UniqueSaturatedInto, Zero},
	Rounding,
};
use sp_runtime::traits::TrailingZeroInput;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	fmt::Debug,
	vec,
	vec::Vec,
};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;
mod dca;
mod execution;
mod fees;
mod impls;
mod swap_state;
pub mod utilities;

pub mod migrations;
pub mod weights;
pub use dca::DcaState;
pub use fees::{BrokerFeesTracker, FeeRateAndMinimum, NetworkFeeTracker};
pub use impls::BrokerDeregistrationCheck;
pub(crate) use swap_state::{GroupSwapState, SwapGroupPair};
pub use weights::WeightInfo;

use crate::swap_state::SuccessfulSwap;

pub(crate) type FailedSwapState<T> = swap_state::SwapState<T, swap_state::StageFailed>;
pub(crate) type AssetAndAmount = cf_primitives::AssetAndAmount<AssetAmount>;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(18);

pub(crate) const DEFAULT_SWAP_RETRY_DELAY_BLOCKS: u32 = 5;
const DEFAULT_MAX_SWAP_RETRY_DURATION_BLOCKS: u32 = 3600 / SECONDS_PER_BLOCK as u32; // 1 hour
const DEFAULT_MAX_SWAP_REQUEST_DURATION_BLOCKS: u32 = 86_400 / SECONDS_PER_BLOCK as u32; // 24 hours

/// Default oracle price slippage protection when no specific value is configured.
///
/// Note this only applies to assets where we have oracle prices. If we don't have an oracle
/// price we can't apply LPP at all and we treat the limit as zero because the slippage will
/// always default to zero for those assets.
pub const FALLBACK_DEFAULT_LPP_LIMIT_BPS: BasisPoints = 100;

pub struct DefaultSwapRetryDelay<T> {
	_phantom: PhantomData<T>,
}
impl<T: Config> Get<BlockNumberFor<T>> for DefaultSwapRetryDelay<T> {
	fn get() -> BlockNumberFor<T> {
		BlockNumberFor::<T>::from(DEFAULT_SWAP_RETRY_DELAY_BLOCKS)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeTaken {
	pub remaining_amount: AssetAmount,
	pub fee: AssetAmount,
}

pub(crate) enum EgressType {
	Regular,
	Refund,
}

#[derive(
	Encode, Decode, DecodeWithMemTracking, TypeInfo, Serialize, Deserialize, Copy, Clone, Debug,
)]
pub struct AffiliateDetails {
	pub short_id: AffiliateShortId,
	pub withdrawal_address: EthereumAddress,
}

/// Refund parameter used within the swapping pallet.
#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct SwapRefundParameters {
	pub refund_block: cf_primitives::BlockNumber,
	pub price_limits: PriceLimits,
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Swap<T: Config> {
	swap_id: SwapId,
	swap_request_id: SwapRequestId,
	pub from: Asset,
	pub to: Asset,
	input_amount: AssetAmount,
	refund_params: Option<SwapRefundParameters>,
	execute_at: BlockNumberFor<T>,
}

pub struct DefaultBrokerBond<T>(PhantomData<T>);
impl<T: Config> Get<T::Amount> for DefaultBrokerBond<T> {
	fn get() -> T::Amount {
		T::Amount::from(FLIPPERINOS_PER_FLIP * 100)
	}
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct SwapLegInfo {
	pub swap_id: SwapId,
	pub swap_request_id: SwapRequestId,
	pub base_asset: Asset,
	pub quote_asset: Asset,
	pub side: Side,
	pub amount: AssetAmount,
	pub source_asset: Option<Asset>,
	pub source_amount: Option<AssetAmount>,
	pub remaining_chunks: u32,
	pub chunk_interval: u32,
}

impl<T: Config> Swap<T> {
	pub fn new(
		swap_id: SwapId,
		swap_request_id: SwapRequestId,
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
		refund_params: Option<SwapRefundParameters>,
		execute_at: BlockNumberFor<T>,
	) -> Self {
		Self { swap_id, swap_request_id, from, to, input_amount, refund_params, execute_at }
	}

	/// Remove the refund params from the swap. Used for simulating swaps without price protection.
	fn without_price_protection(self) -> Self {
		Self { refund_params: None, ..self }
	}
}

#[derive(Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq)]
pub enum SwapFailureReason {
	/// Batch swap failed due to price impact limit
	PriceImpactLimit,
	/// The minimum price limit was exceeded
	MinPriceViolation,
	/// The oracle price slippage limit was exceeded
	OraclePriceSlippageExceeded,
	/// Unable to use oracle slippage parameter because the oracle price is stale
	OraclePriceStale,
	/// An earlier chunk for the same swap request was aborted or rescheduled
	PredecessorSwapFailure,
	/// Swapping is disabled due to safe mode
	SafeModeActive,
	/// Aborted by the originator of the swap
	AbortedFromOrigin,
	/// Some unexpected state has been reached.
	LogicError,
}

pub enum BatchExecutionError<T: Config> {
	SwapLegFailed {
		from_asset: Asset,
		to_asset: Asset,
		amount: AssetAmount,
		failed_swap_group: Vec<FailedSwapState<T>>,
	},
	PriceViolation {
		violating_swaps: Vec<(Swap<T>, SwapFailureReason)>,
		non_violating_swaps: BTreeMap<SwapId, Swap<T>>,
	},
	DispatchError {
		error: DispatchError,
	},
}

#[derive(DebugNoBound)]
pub(crate) struct BatchExecutionOutcomes<T: Config> {
	pub(crate) successful_swaps: Vec<SuccessfulSwap>,
	pub(crate) failed_swaps: Vec<(Swap<T>, SwapFailureReason)>,
}

/// This impl is never used. This is purely used to satisfy the transactional trait requirement
impl<T: Config> From<DispatchError> for BatchExecutionError<T> {
	fn from(error: DispatchError) -> Self {
		Self::DispatchError { error }
	}
}

#[derive(Clone, Copy, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, PartialEq, Eq)]
pub enum SwapRequestCompletionReason {
	/// Aborted explicitly without waiting for timeout (e.g. used in liquidation swaps).
	Aborted,
	/// Auto-aborted due to reaching timeout (usually due to failing to meet min price limit).
	Expired,
	/// Fully swapped.
	Executed,
}

#[expect(clippy::large_enum_variant)]
#[derive(DebugNoBound, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub(crate) enum SwapRequestState<T: Config> {
	UserSwap {
		price_limits_and_expiry: Option<PriceLimitsAndExpiry<T::AccountId>>,
		output_action: SwapOutputAction<T::AccountId>,
		dca_state: DcaState,
		network_fee_tracker: NetworkFeeTracker,
		broker_fees_tracker: BrokerFeesTracker<T::AccountId>,
	},
	NetworkFee,
	IngressEgressFee,
	BrokerFee {
		account_id: T::AccountId,
	},
}

#[derive(DebugNoBound, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub(crate) struct SwapRequest<T: Config> {
	pub(crate) id: SwapRequestId,
	pub(crate) input_asset: Asset,
	pub(crate) output_asset: Asset,
	pub(crate) state: SwapRequestState<T>,
}

#[derive(
	Clone,
	RuntimeDebugNoBound,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(T, I))]
pub enum PalletConfigUpdate<T: Config> {
	/// Set the maximum amount allowed to be put into a swap. Excess amounts are confiscated.
	MaximumSwapAmount { asset: Asset, amount: Option<AssetAmount> },
	/// Set the delay in blocks before retrying a previously failed swap.
	SwapRetryDelay { delay: BlockNumberFor<T> },
	/// Set the interval at which we buy FLIP in order to burn it.
	FlipBuyInterval { interval: BlockNumberFor<T> },
	/// Set the max allowed value for the number of blocks to keep retrying a swap before it is
	/// refunded
	SetMaxSwapRetryDuration { blocks: BlockNumber },
	/// Set the max allowed total duration of a DCA swap request.
	SetMaxSwapRequestDuration { blocks: BlockNumber },
	/// Set the minimum chunk size for DCA swaps. The number of chunks of a DCA swap will be
	/// reduced to meet this requirement.
	SetMinimumChunkSize { asset: Asset, size: AssetAmount },
	/// Set the broker bond. This is the amount of FLIP that must be bonded to open a private
	/// broker channel. The funds are getting freed when the channel is closed.
	SetBrokerBond { bond: T::Amount },
	/// Set the network fee rate and minimum in USDC
	SetNetworkFee { rate: Option<Permill>, minimum: Option<AssetAmount> },
	/// Set the network fee rate and minimum in USDC that will be used just for internal swaps
	/// (credit on-chain swaps)
	SetInternalSwapNetworkFee { rate: Option<Permill>, minimum: Option<AssetAmount> },
	/// Set a custom network fee for a specific asset. Set to None to remove the custom network fee
	/// rate for that asset and fallback to the standard network fee.
	SetNetworkFeeForAsset { asset: Asset, rate: Option<Permill> },
	/// Set a custom network fee for internal swaps for a specific asset. Set to None to remove the
	/// custom network fee rate for that asset and fallback to the standard internal network fee.
	SetInternalSwapNetworkFeeForAsset { asset: Asset, rate: Option<Permill> },
	/// If no oracle protection is set by the user, a default will be
	/// applied. The default will be the sum of both pools' values. Only
	/// used for regular swaps (not fee swaps). Set to `None` to reset
	/// to the permissive default (100bps per leg).
	SetDefaultOraclePriceSlippageProtectionForAsset {
		base_asset: Asset,
		quote_asset: Asset,
		bps: Option<BasisPoints>,
	},
}

impl_pallet_safe_mode! {
	PalletSafeMode; swaps_enabled, withdrawals_enabled, broker_registration_enabled, deposit_enabled
}

fn address_error_to_pallet_error<T>(error: AddressError) -> Error<T>
where
	T: Config,
{
	match error {
		AddressError::InvalidAddress => Error::<T>::InvalidDestinationAddress,
		AddressError::InvalidAddressForChain => Error::<T>::IncompatibleAssetAndAddress,
	}
}

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{
		address::EncodedAddress, AnyChain, CcmChannelMetadataChecked, CcmChannelMetadataUnchecked,
		Chain,
	};
	use cf_primitives::{
		AffiliateShortId, Asset, AssetAmount, BasisPoints, BlockNumber, DcaParameters, EgressId,
		SwapId, SwapRequestId,
	};
	use cf_traits::{
		lending::LendingSystemApi, AccountRoleRegistry, AdditionalDepositAction, Chainflip,
		EgressApi, FundAccount, PoolPriceProvider, PriceFeedApi,
	};
	use core::cmp::max;
	use frame_system::WeightInfo as SystemWeightInfo;

	use super::*;
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// API for handling asset deposits.
		type DepositHandler: DepositApi<
			AnyChain,
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = <Self as Chainflip>::Amount,
		>;

		/// API for handling asset egress.
		type EgressHandler: EgressApi<AnyChain>;

		/// An interface to the AMM api implementation.
		type SwappingApi: SwappingApi;

		/// A converter to convert address to and from human readable to internal address
		/// representation.
		type AddressConverter: AddressConverter;

		/// Safe mode access.
		type SafeMode: Get<PalletSafeMode>;

		/// The Weight information.
		type WeightInfo: WeightInfo;

		#[cfg(feature = "runtime-benchmarks")]
		type FeePayment: cf_traits::FeePayment<
			Amount = <Self as Chainflip>::Amount,
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		type IngressEgressFeeHandler: IngressEgressFeeApi<AnyChain>;

		/// The balance API for interacting with the asset-balance pallet.
		type BalanceApi: BalanceApi<AccountId = <Self as frame_system::Config>::AccountId>;

		type LendingSystemApi: LendingSystemApi<
			AccountId = <Self as frame_system::Config>::AccountId,
		>;

		type FundAccount: FundAccount<
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = <Self as Chainflip>::Amount,
		>;

		type PoolPriceApi: PoolPriceProvider;

		type PriceFeedApi: PriceFeedApi;

		type ChannelIdAllocator: ChannelIdAllocator;

		type Bonder: Bonding<
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = <Self as Chainflip>::Amount,
		>;

		type MinimumFunding: GetMinimumFunding;

		type RuntimeCall: Member + Parameter + From<frame_system::Call<Self>> + From<Call<Self>>;

		/// For getting the Chainflip network.
		type ChainflipNetwork: ChainflipNetworkInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub(super) type SwapRequests<T: Config> =
		StorageMap<_, Twox64Concat, SwapRequestId, SwapRequest<T>>;

	/// AKA swap queue.
	/// Storing as a StorageValue because we expect to read all swaps most of the time.
	#[pallet::storage]
	pub type ScheduledSwaps<T: Config> = StorageValue<_, BTreeMap<SwapId, Swap<T>>, ValueQuery>;

	/// SwapId Counter
	#[pallet::storage]
	pub type SwapIdCounter<T: Config> = StorageValue<_, SwapId, ValueQuery>;

	#[pallet::storage]
	pub type SwapRequestIdCounter<T: Config> = StorageValue<_, SwapRequestId, ValueQuery>;

	/// Fund accrued from rejected swap and CCM calls.
	#[pallet::storage]
	pub type CollectedRejectedFunds<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	/// Maximum amount allowed to be put into a swap. Excess amounts are confiscated.
	#[pallet::storage]
	#[pallet::getter(fn maximum_swap_amount)]
	pub type MaximumSwapAmount<T: Config> = StorageMap<_, Twox64Concat, Asset, AssetAmount>;

	/// FLIP ready to be burned.
	#[pallet::storage]
	pub type FlipToBurn<T: Config> = StorageValue<_, i128, ValueQuery>;

	/// FLIP ready to be sent to gateway.
	#[pallet::storage]
	pub type FlipToBeSentToGateway<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	/// Interval at which we buy FLIP in order to burn it.
	#[pallet::storage]
	pub type FlipBuyInterval<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Network fees collected from the input asset of swaps.
	#[pallet::storage]
	pub type CollectedNetworkFee<T: Config> =
		StorageValue<_, BTreeMap<Asset, AssetAmount>, ValueQuery>;

	/// The delay in blocks before retrying a previously failed swap.
	#[pallet::storage]
	pub type SwapRetryDelay<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery, DefaultSwapRetryDelay<T>>;

	/// Max allowed value for the number of blocks to keep retrying a swap before it is refunded
	#[pallet::storage]
	pub type MaxSwapRetryDurationBlocks<T> =
		StorageValue<_, BlockNumber, ValueQuery, ConstU32<DEFAULT_MAX_SWAP_RETRY_DURATION_BLOCKS>>;

	/// Max allowed total duration of a DCA swap request.
	#[pallet::storage]
	pub type MaxSwapRequestDurationBlocks<T> = StorageValue<
		_,
		BlockNumber,
		ValueQuery,
		ConstU32<DEFAULT_MAX_SWAP_REQUEST_DURATION_BLOCKS>,
	>;

	/// The minimum chunk size for DCA swaps. The number of chunks of a DCA swap will be reduced
	/// so that the chunk size is greater than or equal to this value. Setting to zero will disable
	/// the check for that asset.
	#[pallet::storage]
	#[pallet::getter(fn minimum_chunk_size)]
	pub type MinimumChunkSize<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::storage]
	pub type BrokerPrivateBtcChannels<T: Config> =
		StorageMap<_, Identity, T::AccountId, ChannelId, OptionQuery>;

	/// Associates for a given broker an affiliate broker account with short id (u8) so that
	/// it can be used in place of the full account id in order to save space (e.g. in UTXO encoding
	/// for BTC)
	#[pallet::storage]
	pub type AffiliateIdMapping<T: Config> = StorageDoubleMap<
		_,
		Identity,
		T::AccountId,
		Twox64Concat,
		AffiliateShortId,
		T::AccountId,
		OptionQuery,
	>;

	/// Stores the details of an affiliate account against the account id of a broker and the
	/// derived affiliate id.
	#[pallet::storage]
	pub type AffiliateAccountDetails<T: Config> = StorageDoubleMap<
		_,
		Identity,
		T::AccountId,
		Identity,
		T::AccountId,
		AffiliateDetails,
		OptionQuery,
	>;

	/// The bond for a broker to open a private channel.
	#[pallet::storage]
	pub type BrokerBond<T: Config> = StorageValue<_, T::Amount, ValueQuery, DefaultBrokerBond<T>>;

	/// Network fee rate and minimum in USDC, charged per swap request. Used for regular swaps and
	/// fee swaps, it excludes internal swaps (credit on-chain swaps).
	#[pallet::storage]
	pub type NetworkFee<T: Config> = StorageValue<_, FeeRateAndMinimum, ValueQuery>;

	/// Alternate network fee rate and minimum in USDC, just for internal swaps (credit on-chain
	/// swaps).
	#[pallet::storage]
	pub type InternalSwapNetworkFee<T: Config> = StorageValue<_, FeeRateAndMinimum, ValueQuery>;

	/// A custom network fee for a specific asset. A swap will use the highest fee rate (custom or
	/// standard) between the input and output asset.
	#[pallet::storage]
	pub type NetworkFeeForAsset<T: Config> =
		StorageMap<_, Twox64Concat, Asset, Permill, OptionQuery>;

	/// A custom network fee for internal swaps for a specific asset.
	/// A swap will use the highest fee rate (custom or standard) between the input and output
	/// asset.
	#[pallet::storage]
	pub type InternalSwapNetworkFeeForAsset<T: Config> =
		StorageMap<_, Twox64Concat, Asset, Permill, OptionQuery>;

	/// Set by the broker, this is the minimum broker commission that the broker will accept for a
	/// vault swap.
	#[pallet::storage]
	pub type VaultSwapMinimumBrokerFee<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, BasisPoints, ValueQuery>;

	/// Map of bound addresses for accounts.
	#[pallet::storage]
	#[pallet::getter(fn bound_broker_withdrawal_address)]
	pub type BoundBrokerWithdrawalAddress<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, EthereumAddress, OptionQuery>;

	#[pallet::storage]
	pub type DefaultOraclePriceSlippageProtection<T: Config> = StorageMap<
		_,
		Twox64Concat,
		AssetPair,
		BasisPoints,
		ValueQuery,
		ConstU16<FALLBACK_DEFAULT_LPP_LIMIT_BPS>,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	#[expect(clippy::large_enum_variant)]
	pub enum Event<T: Config> {
		/// New swap has been requested
		SwapRequested {
			swap_request_id: SwapRequestId,
			input_asset: Asset,
			input_amount: AssetAmount, // Before network fee
			output_asset: Asset,
			origin: SwapOrigin<T::AccountId>,
			request_type: SwapRequestTypeEncoded<T::AccountId>,
			broker_fees: Beneficiaries<T::AccountId>,
			price_limits_and_expiry: Option<PriceLimitsAndExpiry<T::AccountId>>,
			dca_parameters: Option<DcaParameters>,
		},
		SwapRequestCompleted {
			swap_request_id: SwapRequestId,
			reason: SwapRequestCompletionReason,
			broker_fee_swaps: BTreeMap<T::AccountId, SwapRequestId>,
		},
		/// An new swap deposit channel has been opened.
		SwapDepositAddressReady {
			deposit_address: EncodedAddress,
			destination_address: EncodedAddress,
			source_asset: Asset,
			destination_asset: Asset,
			channel_id: ChannelId,
			broker_id: T::AccountId,
			broker_commission_rate: BasisPoints,
			channel_metadata: Option<CcmChannelMetadataChecked>,
			source_chain_expiry_block: <AnyChain as Chain>::ChainBlockNumber,
			boost_fee: BasisPoints,
			channel_opening_fee: T::Amount,
			affiliate_fees: Affiliates<T::AccountId>,
			refund_parameters: ChannelRefundParametersUncheckedEncoded,
			dca_parameters: Option<DcaParameters>,
		},
		/// A swap is scheduled for the first time
		SwapScheduled {
			swap_request_id: SwapRequestId,
			swap_id: SwapId,
			input_amount: AssetAmount,
			swap_type: SwapType,
			execute_at: BlockNumberFor<T>,
		},
		/// A swap is re-scheduled for a future block after failure
		SwapRescheduled {
			swap_id: SwapId,
			execute_at: BlockNumberFor<T>,
			reason: SwapFailureReason,
		},
		/// A swap has been executed.
		SwapExecuted {
			swap_request_id: SwapRequestId,
			swap_id: SwapId,
			// Input amount after deducting network fee
			input: AssetAndAmount,
			// Output amount after deducting broker fee
			output: AssetAndAmount,
			network_fee: AssetAndAmount,
			broker_fee: AssetAndAmount,
			intermediate: Option<AssetAndAmount>,
			// Total difference between final swap output price and oracle price (including fees).
			// Negative means worse price than oracle.
			oracle_delta: Option<SignedBasisPoints>,
			// Cumulative price delta across both swap legs (excluding fees).
			// Negative values indicate worse execution price compared to oracle.
			// `Some` if oracle price available for at least one leg; `None` if unavailable for
			// both.
			oracle_delta_ex_fees: Option<SignedBasisPoints>,
		},
		/// A swap egress has been scheduled.
		SwapEgressScheduled {
			swap_request_id: SwapRequestId,
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			egress_fee: (AssetAmount, Asset),
		},
		RefundEgressScheduled {
			swap_request_id: SwapRequestId,
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			egress_fee: (AssetAmount, Asset),
		},
		/// A broker fee withdrawal has been requested.
		WithdrawalRequested {
			account_id: T::AccountId,
			egress_id: EgressId,
			egress_asset: Asset,
			egress_amount: AssetAmount,
			egress_fee: AssetAmount,
			destination_address: EncodedAddress,
		},
		/// Most likely cause of this error is that there are insufficient
		/// liquidity in the Pool. Also this could happen if the result overflowed u128::MAX
		BatchSwapFailed {
			asset: Asset,
			direction: SwapLeg,
			amount: AssetAmount,
		},
		SwapAmountConfiscated {
			swap_request_id: SwapRequestId,
			asset: Asset,
			total_amount: AssetAmount,
			confiscated_amount: AssetAmount,
		},
		SwapEgressIgnored {
			swap_request_id: SwapRequestId,
			asset: Asset,
			amount: AssetAmount,
			reason: DispatchError,
		},
		RefundEgressIgnored {
			swap_request_id: SwapRequestId,
			asset: Asset,
			amount: AssetAmount,
			reason: DispatchError,
		},
		PrivateBrokerChannelOpened {
			broker_id: T::AccountId,
			channel_id: ChannelId,
		},
		PrivateBrokerChannelClosed {
			broker_id: T::AccountId,
			channel_id: ChannelId,
		},
		AffiliateRegistration {
			broker_id: T::AccountId,
			short_id: AffiliateShortId,
			withdrawal_address: EthereumAddress,
			affiliate_id: T::AccountId,
		},
		// Account credited as a result of an on-chain swap
		CreditedOnChain {
			swap_request_id: SwapRequestId,
			account_id: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
		},
		// Account received a refund as a result of an on-chain swap
		RefundedOnChain {
			swap_request_id: SwapRequestId,
			account_id: T::AccountId,
			asset: Asset,
			amount: AssetAmount,
		},
		PalletConfigUpdated {
			update: PalletConfigUpdate<T>,
		},
		VaultSwapMinimumBrokerFeeSet {
			broker_id: T::AccountId,
			minimum_fee_bps: BasisPoints,
		},
		SwapAborted {
			swap_id: SwapId,
			reason: SwapFailureReason,
		},
		AccountCreationDepositAddressReady {
			channel_id: ChannelId,
			asset: Asset,
			deposit_address: EncodedAddress,
			requested_by: T::AccountId,
			// account the funds will be credited to upon deposit
			requested_for: T::AccountId,
			deposit_chain_expiry_block: <AnyChain as Chain>::ChainBlockNumber,
			boost_fee: BasisPoints,
			channel_opening_fee: T::Amount,
			refund_address: EncodedAddress,
		},
		/// A broker has been bound to an address.
		BoundBrokerWithdrawalAddress {
			broker: T::AccountId,
			address: EthereumAddress,
		},
		/// An affiliate has been deregistered.
		AffiliateDeregistration {
			broker_id: T::AccountId,
			short_id: AffiliateShortId,
			affiliate_account_id: T::AccountId,
		},
		/// List of network fee swaps that were started by the periodic task.
		NetworkFeeSwapsInitiated {
			swap_request_ids: Vec<SwapRequestId>,
		},
	}
	#[pallet::error]
	pub enum Error<T> {
		/// The provided asset and withdrawal address are incompatible.
		IncompatibleAssetAndAddress,
		/// The Asset cannot be egressed because the destination address is not invalid.
		InvalidEgressAddress,
		/// The withdrawal is not possible because not enough funds are available.
		NoFundsAvailable,
		/// The target chain does not support CCM.
		CcmUnsupportedForTargetChain,
		/// The provided address could not be decoded.
		InvalidDestinationAddress,
		/// Withdrawals are disabled due to Safe Mode.
		WithdrawalsDisabled,
		/// Broker registration is disabled due to Safe Mode.
		BrokerRegistrationDisabled,
		/// Broker commission bps is limited to 1000 points.
		BrokerCommissionBpsTooHigh,
		/// Brokers should withdraw their earned fees before deregistering.
		EarnedFeesNotWithdrawn,
		/// Failed to open deposit channel because the CCM message is invalid.
		InvalidCcm,
		/// Setting the buy interval to zero is not allowed.
		ZeroBuyIntervalNotAllowed,
		/// Setting the swap retry delay to zero is not allowed.
		ZeroSwapRetryDelayNotAllowed,
		/// Setting the max swap request duration to less than the swap delay is not allowed.
		MaxSwapRequestDurationTooShort,
		/// Swap Retry duration is set above the max allowed.
		RetryDurationTooHigh,
		/// The number of DCA chunks must be greater than 0.
		ZeroNumberOfChunksNotAllowed,
		/// The chunk interval must be greater than the swap delay (2).
		ChunkIntervalTooLow,
		/// The total duration of a DCA swap request must be less then the max allowed.
		SwapRequestDurationTooLong,
		/// Invalid DCA parameters.
		InvalidDcaParameters,
		/// The provided Refund address cannot be decoded into ForeignChainAddress.
		InvalidRefundAddress,
		/// The given boost fee is too large to fit in a u8.
		BoostFeeTooHigh,
		/// The broker fee is too large to fit in a u8.
		BrokerFeeTooHigh,
		/// Broker cannot deregister or open a new private channel because one already exists.
		PrivateChannelExistsForBroker,
		/// The Broker does not have an open private channel.
		NoPrivateChannelExistsForBroker,
		/// The affiliate fee is too large to fit in a u8.
		AffiliateFeeTooHigh,
		/// The affiliate id is not registered for the broker.
		AffiliateNotRegisteredForBroker,
		/// The Bonder does not have enough Funds to cover the bond.
		InsufficientFunds,
		/// The affiliate is already registered.
		AffiliateAlreadyRegistered,
		/// The affiliate account id could not be derived.
		AffiliateAccountIdDerivationFailed,
		/// The affiliate short id is out of bounds. That means the broker has registered more than
		/// 255 affiliates.
		AffiliateShortIdOutOfBounds,
		/// The affiliate has not withdrawn their earned fees. This is a pre-requisite for
		/// deregistration of a broker.
		AffiliateEarnedFeesNotWithdrawn,
		/// Refund egress was not performed because no amount remained after deducting the refund
		/// fee.
		NoRefundAmountRemaining,
		/// CCM is not supported for the refund chain.
		CcmUnsupportedForRefundChain,
		/// Oracle price not available for one or more of the assets.
		OraclePriceNotAvailable,
		/// The provided Signature Data is invalid
		InvalidUserSignatureData,
		/// The provided Transaction Metadata is invalid
		InvalidTransactionMetadata,
		/// Failed to encode data
		CannotEncodeData,
		/// Liquidity deposit is disabled due to Safe Mode.
		LiquidityDepositDisabled,
		/// Account already exists, cannot open an account creation deposit channel.
		AccountAlreadyExists,
		/// The broker is already bound to a withdrawal address.
		BrokerAlreadyBound,
		/// The broker tried to withdraw to an address which is not the address the broker is bound
		/// to.
		BrokerBoundWithdrawalAddressRestrictionViolated,
		/// A zero default slippage protection will result in most swaps failing. Set to `None` to
		/// reset to the permissive default (100bps).
		ZeroDefaultSlippageNotAllowed,
		/// The specified pool does not exist.
		PoolDoesNotExist,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub flip_buy_interval: BlockNumberFor<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			FlipBuyInterval::<T>::set(self.flip_buy_interval);
		}
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { flip_buy_interval: BlockNumberFor::<T>::zero() }
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(current_block: BlockNumberFor<T>) -> Weight {
			let mut weight_used: Weight = T::DbWeight::get().reads(1);
			let interval = FlipBuyInterval::<T>::get();
			if interval.is_zero() {
				log::debug!("Flip buy interval is zero, skipping.")
			} else {
				if (current_block % interval).is_zero() {
					weight_used.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
					let swap_request_ids: Vec<_> = CollectedNetworkFee::<T>::take()
						.into_iter()
						.filter_map(|(asset, amount)| {
							if amount > 0 {
								weight_used.saturating_accrue(
									T::WeightInfo::init_network_fee_swap_request(),
								);
								Some(Self::init_network_fee_swap_request(asset, amount))
							} else {
								None
							}
						})
						.collect();
					if !swap_request_ids.is_empty() {
						Self::deposit_event(Event::NetworkFeeSwapsInitiated { swap_request_ids });
					}
				}
			}
			weight_used
		}

		/// Execute swaps in the ScheduledSwaps
		fn on_finalize(current_block: BlockNumberFor<T>) {
			// Take all swaps that are scheduled to be executed at this block.
			let swaps_to_execute = ScheduledSwaps::<T>::mutate(|swaps| {
				let (swaps_to_execute, remaining_swap_ids) =
					core::mem::take(swaps).into_iter().partition::<BTreeMap<_, _>, _>(
						|(_, swap)| swap.execute_at <= current_block,
					);

				*swaps = remaining_swap_ids;

				swaps_to_execute
			});

			let retry_delay = max(SwapRetryDelay::<T>::get(), 1u32.into());

			if !T::SafeMode::get().swaps_enabled {
				// Since we won't be executing swaps at this block, we need to reschedule them:
				for (_, swap) in swaps_to_execute {
					Self::reschedule_swap(swap, retry_delay, SwapFailureReason::SafeModeActive);
				}

				return
			}

			let BatchExecutionOutcomes { successful_swaps, failed_swaps } =
				Self::execute_batch(swaps_to_execute.clone());

			for swap in successful_swaps {
				Self::process_swap_outcome(swap);
			}

			for (swap, reason) in failed_swaps {
				match swap.refund_params {
					Some(ref params)
						if BlockNumberFor::<T>::from(params.refund_block) <
							current_block + retry_delay =>
					{
						// Reached refund block, process refund:
						Self::refund_failed_swap(swap, reason);
					},
					_ => {
						// Either refund parameters not set, or refund block not
						// reached:
						Self::reschedule_swap(swap, retry_delay, reason);
					},
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Request a swap deposit address.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::request_swap_deposit_address())]
		pub fn request_swap_deposit_address(
			origin: OriginFor<T>,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
			boost_fee: BasisPoints,
			refund_parameters: ChannelRefundParametersUncheckedEncoded,
		) -> DispatchResult {
			Self::request_swap_deposit_address_with_affiliates(
				origin,
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				channel_metadata,
				boost_fee,
				// No affiliate fees on this version of the extrinsic
				Default::default(),
				// No ccm refund or oracle price parameters on the original extrinsic, but it will
				// decode fine because they are optional
				refund_parameters,
				None,
			)
		}

		/// Brokers can withdraw their collected fees.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::withdraw())]
		pub fn withdraw(
			origin: OriginFor<T>,
			asset: Asset,
			destination_address: EncodedAddress,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().withdrawals_enabled, Error::<T>::WithdrawalsDisabled);

			let account_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			let destination_address_internal =
				T::AddressConverter::decode_and_validate_address_for_asset(
					destination_address.clone(),
					asset,
				)
				.map_err(address_error_to_pallet_error::<T>)?;

			if ForeignChain::from(asset) == ForeignChain::Ethereum {
				if let Some(bound_address) = BoundBrokerWithdrawalAddress::<T>::get(&account_id) {
					ensure!(
						destination_address_internal == ForeignChainAddress::Eth(bound_address),
						Error::<T>::BrokerBoundWithdrawalAddressRestrictionViolated
					);
				}
			}

			Self::trigger_withdrawal(&account_id, asset, destination_address_internal)?;

			Ok(())
		}

		/// Register the account as a Broker.
		///
		/// Account roles are immutable once registered.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::register_as_broker())]
		pub fn register_as_broker(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

			ensure!(
				T::SafeMode::get().broker_registration_enabled,
				Error::<T>::BrokerRegistrationDisabled,
			);

			T::AccountRoleRegistry::register_as_broker(&account_id)?;

			Ok(())
		}

		/// Apply a list of configuration updates to the pallet.
		///
		/// Requires Governance.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as frame_system::Config>::SystemWeightInfo::set_storage(updates.len() as u32))]
		pub fn update_pallet_config(
			origin: OriginFor<T>,
			updates: BoundedVec<PalletConfigUpdate<T>, ConstU32<100>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::MaximumSwapAmount { asset, amount } => {
						MaximumSwapAmount::<T>::set(asset, amount);
					},
					PalletConfigUpdate::SwapRetryDelay { delay } => {
						ensure!(
							delay != BlockNumberFor::<T>::zero(),
							Error::<T>::ZeroSwapRetryDelayNotAllowed
						);
						SwapRetryDelay::<T>::set(delay);
					},
					PalletConfigUpdate::FlipBuyInterval { interval } => {
						ensure!(
							interval != BlockNumberFor::<T>::zero(),
							Error::<T>::ZeroBuyIntervalNotAllowed
						);
						FlipBuyInterval::<T>::set(interval);
					},
					PalletConfigUpdate::SetMaxSwapRetryDuration { blocks } => {
						MaxSwapRetryDurationBlocks::<T>::set(blocks);
					},
					PalletConfigUpdate::SetMaxSwapRequestDuration { blocks } => {
						ensure!(
							blocks >= SWAP_DELAY_BLOCKS,
							Error::<T>::MaxSwapRequestDurationTooShort
						);
						MaxSwapRequestDurationBlocks::<T>::set(blocks);
					},
					PalletConfigUpdate::SetMinimumChunkSize { asset, size: amount } => {
						MinimumChunkSize::<T>::set(asset, amount);
					},
					PalletConfigUpdate::SetBrokerBond { bond } => {
						BrokerBond::<T>::set(bond);
					},
					PalletConfigUpdate::SetNetworkFee { rate, minimum } => match (rate, minimum) {
						(Some(rate), Some(minimum)) => {
							NetworkFee::<T>::set(FeeRateAndMinimum { rate, minimum });
						},
						(Some(rate), None) => {
							NetworkFee::<T>::mutate(|fee| fee.rate = rate);
						},
						(None, Some(minimum)) => {
							NetworkFee::<T>::mutate(|fee| fee.minimum = minimum);
						},
						(None, None) => {
							// No change, do nothing
						},
					},
					PalletConfigUpdate::SetInternalSwapNetworkFee { rate, minimum } => {
						match (rate, minimum) {
							(Some(rate), Some(minimum)) => {
								InternalSwapNetworkFee::<T>::set(FeeRateAndMinimum {
									rate,
									minimum,
								});
							},
							(Some(rate), None) => {
								InternalSwapNetworkFee::<T>::mutate(|fee| fee.rate = rate);
							},
							(None, Some(minimum)) => {
								InternalSwapNetworkFee::<T>::mutate(|fee| fee.minimum = minimum);
							},
							(None, None) => {
								// No change, do nothing
							},
						}
					},
					PalletConfigUpdate::SetNetworkFeeForAsset { asset, rate } => {
						if let Some(rate) = rate {
							NetworkFeeForAsset::<T>::insert(asset, rate);
						} else {
							NetworkFeeForAsset::<T>::remove(asset);
						}
					},
					PalletConfigUpdate::SetInternalSwapNetworkFeeForAsset { asset, rate } =>
						if let Some(rate) = rate {
							InternalSwapNetworkFeeForAsset::<T>::insert(asset, rate);
						} else {
							InternalSwapNetworkFeeForAsset::<T>::remove(asset);
						},
					PalletConfigUpdate::SetDefaultOraclePriceSlippageProtectionForAsset {
						base_asset,
						quote_asset,
						bps,
					} => {
						let pool = AssetPair::new(base_asset, quote_asset)
							.ok_or(Error::<T>::PoolDoesNotExist)?;
						if let Some(bps) = bps {
							ensure!(bps != 0, Error::<T>::ZeroDefaultSlippageNotAllowed);
							DefaultOraclePriceSlippageProtection::<T>::insert(pool, bps);
						} else {
							DefaultOraclePriceSlippageProtection::<T>::remove(pool);
						}
					},
				}
				Self::deposit_event(Event::<T>::PalletConfigUpdated { update });
			}

			Ok(())
		}

		/// Register the account as a Broker.
		///
		/// Account roles are immutable once registered.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::deregister_as_broker())]
		pub fn deregister_as_broker(who: OriginFor<T>) -> DispatchResult {
			let account_id = T::AccountRoleRegistry::ensure_broker(who)?;

			T::AccountRoleRegistry::deregister_as_broker(&account_id)?;

			for affiliate_account_id in AffiliateAccountDetails::<T>::iter_key_prefix(&account_id) {
				frame_system::Provider::<T>::killed(&affiliate_account_id).unwrap_or_else(|e| {
					// This shouldn't happen, and not much we can do if it does except fix it on a
					// subsequent release. Consequences are minor.
					log::error!(
						"Unexpected reference count error while reaping the affiliate {:?}: {:?}.",
						affiliate_account_id,
						e
					);
				})
			}

			// Clear the affiliate account details and affiliate id mapping.
			// With this the broker has no longer access to the affiliate's account.
			let _ = AffiliateAccountDetails::<T>::clear_prefix(&account_id, u32::MAX, None);
			let _ = AffiliateIdMapping::<T>::clear_prefix(&account_id, u32::MAX, None);

			Ok(())
		}

		/// Request a swap deposit address.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::request_swap_deposit_address_with_affiliates())]
		pub fn request_swap_deposit_address_with_affiliates(
			origin: OriginFor<T>,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<T::AccountId>,
			refund_parameters: ChannelRefundParametersUncheckedEncoded,
			dca_parameters: Option<DcaParameters>,
		) -> DispatchResult {
			let broker = T::AccountRoleRegistry::ensure_broker(origin)?;

			let beneficiaries = Pallet::<T>::assemble_and_validate_broker_fees(
				broker.clone(),
				broker_commission,
				affiliate_fees.clone(),
			)?;

			let destination_address_internal =
				T::AddressConverter::decode_and_validate_address_for_asset(
					destination_address.clone(),
					destination_asset,
				)
				.map_err(address_error_to_pallet_error::<T>)?;

			// Convert the refund parameter from `EncodedAddress` into `ForeignChainAddress` type.
			let refund_params_internal = refund_parameters.clone().try_map_address(|addr| {
				T::AddressConverter::try_from_encoded_address(addr)
					.map_err(|_| Error::<T>::InvalidRefundAddress)
			})?;

			let channel_metadata = channel_metadata
				.map(|ccm| {
					let destination_chain: ForeignChain = destination_asset.into();
					ensure!(
						destination_chain.ccm_support(),
						Error::<T>::CcmUnsupportedForTargetChain
					);

					ccm.to_checked(destination_asset, destination_address_internal.clone()).map_err(
						|e| {
							log::warn!(
							"Failed to open channel due to invalid CCM. Broker: {:?}, Error: {:?}",
							broker,
							e
						);
							Error::<T>::InvalidCcm
						},
					)
				})
				.transpose()?;

			let (channel_id, deposit_address, expiry_height, channel_opening_fee) =
				T::DepositHandler::request_swap_deposit_address(
					source_asset,
					destination_asset,
					destination_address_internal,
					beneficiaries.clone(),
					broker.clone(),
					channel_metadata.clone(),
					boost_fee,
					refund_params_internal,
					dca_parameters.clone(),
				)?;

			Self::deposit_event(Event::<T>::SwapDepositAddressReady {
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				destination_address,
				source_asset,
				destination_asset,
				channel_id,
				broker_id: broker,
				broker_commission_rate: broker_commission,
				channel_metadata,
				source_chain_expiry_block: expiry_height,
				boost_fee,
				channel_opening_fee,
				affiliate_fees,
				refund_parameters,
				dca_parameters,
			});

			Ok(())
		}

		/// Opens a private broker channel for Bitcoin vault swaps.
		///
		/// This requires the broker to have sufficient funds to cover the bond.
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::open_private_btc_channel())]
		pub fn open_private_btc_channel(origin: OriginFor<T>) -> DispatchResult {
			let broker_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			ensure!(
				!BrokerPrivateBtcChannels::<T>::contains_key(&broker_id),
				Error::<T>::PrivateChannelExistsForBroker
			);

			ensure!(
				T::FundingInfo::total_balance_of(&broker_id) >= BrokerBond::<T>::get(),
				Error::<T>::InsufficientFunds
			);

			let channel_id = T::ChannelIdAllocator::allocate_private_channel_id()?;

			BrokerPrivateBtcChannels::<T>::insert(broker_id.clone(), channel_id);

			T::Bonder::update_bond(&broker_id, BrokerBond::<T>::get());

			Self::deposit_event(Event::<T>::PrivateBrokerChannelOpened { broker_id, channel_id });

			Ok(())
		}

		/// Closes the currently open private broker channel.
		///
		/// Closing the channel will unlock the bonded funds.
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::close_private_btc_channel())]
		pub fn close_private_btc_channel(origin: OriginFor<T>) -> DispatchResult {
			let broker_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			let Some(channel_id) = BrokerPrivateBtcChannels::<T>::take(&broker_id) else {
				return Err(Error::<T>::NoPrivateChannelExistsForBroker.into())
			};

			T::Bonder::update_bond(&broker_id, 0u128.into());

			Self::deposit_event(Event::<T>::PrivateBrokerChannelClosed { broker_id, channel_id });

			Ok(())
		}

		/// Registers an affiliate for a broker.
		///
		/// The broker must provide an Ethereum address to which any earned affiliate fees
		/// can be withdrawn. The broker can trigger a withdrawal request to the affiliate's
		/// withdrawal address.
		///
		/// Affiliates have a unique account id that can only be accessed through the affiliate's
		/// broker. The affiliate account id is derived from the broker account id using a short id
		/// that is unique to that combination of broker and affiliate.
		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::register_affiliate())]
		pub fn register_affiliate(
			origin: OriginFor<T>,
			withdrawal_address: EthereumAddress,
		) -> DispatchResult {
			let broker_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			let short_id = AffiliateShortId(
				(0..=u8::MAX)
					.find(|short_id| {
						!AffiliateIdMapping::<T>::contains_key(
							&broker_id,
							AffiliateShortId::from(*short_id),
						)
					})
					.ok_or(Error::<T>::AffiliateShortIdOutOfBounds)?,
			);

			let affiliate_id = Decode::decode(&mut TrailingZeroInput::new(
				(*b"chainflip/affiliate", broker_id.clone(), short_id).blake2_256().as_ref(),
			))
			.map_err(|_| Error::<T>::AffiliateAccountIdDerivationFailed)?;

			AffiliateIdMapping::<T>::insert(&broker_id, short_id, &affiliate_id);
			if !frame_system::Pallet::<T>::account_exists(&affiliate_id) {
				// Creates an account
				let _ = frame_system::Provider::<T>::created(&affiliate_id);
			}

			AffiliateAccountDetails::<T>::insert(
				&broker_id,
				&affiliate_id,
				AffiliateDetails { short_id, withdrawal_address },
			);

			Self::deposit_event(Event::<T>::AffiliateRegistration {
				broker_id,
				short_id,
				withdrawal_address,
				affiliate_id,
			});

			Ok(())
		}

		#[pallet::call_index(15)]
		#[pallet::weight(T::WeightInfo::deregister_affiliate())]
		pub fn deregister_affiliate(
			origin: OriginFor<T>,
			affiliate_account_id: T::AccountId,
		) -> DispatchResult {
			let broker_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			let AffiliateDetails { short_id, .. } =
				AffiliateAccountDetails::<T>::get(&broker_id, &affiliate_account_id)
					.ok_or(Error::<T>::AffiliateNotRegisteredForBroker)?;

			ensure!(
				T::BalanceApi::get_balance(&affiliate_account_id, Asset::Usdc).is_zero(),
				Error::<T>::AffiliateEarnedFeesNotWithdrawn
			);

			frame_system::Provider::<T>::killed(&affiliate_account_id).unwrap_or_else(|e| {
				// This shouldn't happen, and not much we can do if it does except fix it on a
				// subsequent release. Consequences are minor.
				log::error!(
					"Unexpected reference count error while reaping the affiliate {:?}: {:?}.",
					affiliate_account_id,
					e
				);
			});

			AffiliateAccountDetails::<T>::remove(&broker_id, &affiliate_account_id);
			AffiliateIdMapping::<T>::remove(&broker_id, short_id);

			Self::deposit_event(Event::<T>::AffiliateDeregistration {
				broker_id,
				short_id,
				affiliate_account_id,
			});

			Ok(())
		}

		/// Triggers a withdrawal to the registered withdrawal address of the affiliate.
		///
		/// Note: This extrinsic is secured by the broker that has registered the affiliate account.
		#[pallet::call_index(16)]
		#[pallet::weight(T::WeightInfo::affiliate_withdrawal_request())]
		pub fn affiliate_withdrawal_request(
			origin: OriginFor<T>,
			affiliate_account_id: T::AccountId,
		) -> DispatchResult {
			let broker_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			let details = AffiliateAccountDetails::<T>::get(&broker_id, &affiliate_account_id)
				.ok_or(Error::<T>::AffiliateNotRegisteredForBroker)?;

			Self::trigger_withdrawal(
				&affiliate_account_id,
				Asset::Usdc,
				ForeignChainAddress::Eth(details.withdrawal_address),
			)?;
			Ok(())
		}

		/// Sets the brokers personal minimum fee for vault swaps.
		/// This minimum is used to stop encoding vault swaps with a lower broker fee.
		/// If a swap is witnessed with a lower fee, it will be changed to the minimum.
		#[pallet::call_index(17)]
		#[pallet::weight(T::WeightInfo::set_vault_swap_minimum_broker_fee())]
		pub fn set_vault_swap_minimum_broker_fee(
			origin: OriginFor<T>,
			minimum_fee_bps: BasisPoints,
		) -> DispatchResult {
			let broker_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			Pallet::<T>::validate_broker_fees(
				&vec![Beneficiary { account: broker_id.clone(), bps: minimum_fee_bps }]
					.try_into()
					.expect("Single broker will fit"),
			)?;

			VaultSwapMinimumBrokerFee::<T>::insert(broker_id.clone(), minimum_fee_bps);
			Self::deposit_event(Event::<T>::VaultSwapMinimumBrokerFeeSet {
				broker_id,
				minimum_fee_bps,
			});

			Ok(())
		}

		/// Open a channel that allows a user to create an account by depositing liquidity.
		///
		/// The deposit will be partially swapped into FLIP which is used to credit the new account.
		#[pallet::call_index(18)]
		#[pallet::weight(T::WeightInfo::request_account_creation_deposit_address())]
		pub fn request_account_creation_deposit_address(
			origin: OriginFor<T>,
			signature_data: SignatureData,
			transaction_metadata: TransactionMetadata,
			asset: Asset,
			boost_fee: BasisPoints,
			refund_address: EncodedAddress,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().deposit_enabled, Error::<T>::LiquidityDepositDisabled);

			let requested_by = T::AccountRoleRegistry::ensure_broker(origin)?;

			let Ok(signer_account) =
				signature_data.signer_account::<<T as frame_system::Config>::AccountId>()
			else {
				return Err(DispatchError::from(Error::<T>::InvalidUserSignatureData));
			};

			ensure!(
				!frame_system::Pallet::<T>::account_exists(&signer_account),
				DispatchError::from(Error::<T>::AccountAlreadyExists)
			);
			let refund_address_internal =
				T::AddressConverter::decode_and_validate_address_for_asset(
					refund_address.clone(),
					asset,
				)
				.map_err(|_| Error::<T>::IncompatibleAssetAndAddress)?;

			// Manual metadata validation because the `validate_metadata` function has
			// mempool-specific logic
			let tx_nonce: <T as frame_system::Config>::Nonce = transaction_metadata.nonce.into();
			ensure!(
				tx_nonce == 0u32.into(),
				DispatchError::from(Error::<T>::InvalidTransactionMetadata)
			);
			ensure!(
				BlockNumberFor::<T>::from(transaction_metadata.expiry_block) >
					frame_system::Pallet::<T>::block_number(),
				DispatchError::from(Error::<T>::InvalidTransactionMetadata)
			);

			// Simple runtime call for signature verification. Signing over the refund address
			// so they can't be tampered with.
			let remark_data = refund_address.clone().encode();
			let runtime_call: <T as Config>::RuntimeCall =
				frame_system::Call::<T>::remark { remark: remark_data }.into();

			match is_valid_signature(
				runtime_call,
				&T::ChainflipNetwork::chainflip_network(),
				&transaction_metadata,
				&signature_data,
				<T as frame_system::Config>::Version::get().spec_version,
			) {
				Ok(is_valid) => ensure!(is_valid, Error::<T>::InvalidUserSignatureData),
				Err(_) => return Err(Error::<T>::CannotEncodeData.into()),
			}

			let (channel_id, deposit_address, expiry_block, channel_opening_fee) =
				T::DepositHandler::request_liquidity_deposit_address(
					requested_by.clone(),
					signer_account.clone(),
					asset,
					boost_fee,
					refund_address_internal.clone(),
					Some(AdditionalDepositAction::FundFlip {
						flip_amount_to_credit: T::MinimumFunding::get_min_funding_amount(),
					}),
				)?;

			Self::deposit_event(Event::AccountCreationDepositAddressReady {
				channel_id,
				asset,
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				requested_by,
				requested_for: signer_account,
				deposit_chain_expiry_block: expiry_block,
				boost_fee,
				channel_opening_fee,
				refund_address: T::AddressConverter::to_encoded_address(refund_address_internal),
			});

			Ok(())
		}

		/// Binds a broker account to a redeem address. This is used to allow a broker to redeem
		/// their funds only to a specific address.
		#[pallet::call_index(19)]
		#[pallet::weight(T::WeightInfo::bind_broker_fee_withdrawal_address())]
		pub fn bind_broker_fee_withdrawal_address(
			origin: OriginFor<T>,
			address: EthereumAddress,
		) -> DispatchResult {
			let broker = T::AccountRoleRegistry::ensure_broker(origin)?;
			ensure!(
				!BoundBrokerWithdrawalAddress::<T>::contains_key(&broker),
				Error::<T>::BrokerAlreadyBound
			);
			BoundBrokerWithdrawalAddress::<T>::insert(&broker, address);
			Self::deposit_event(Event::BoundBrokerWithdrawalAddress { broker, address });
			Ok(())
		}
	}
}
