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
#![feature(extract_if)]

use cf_amm::common::Side;
use cf_chains::{
	address::{AddressConverter, AddressError, ForeignChainAddress},
	eth::Address as EthereumAddress,
	evm::Signature as EthereumSignature,
	sol::{SolAddress, SolSignature},
	AccountOrAddress, CcmDepositMetadataChecked, ChannelRefundParametersCheckedInternal,
	ChannelRefundParametersUncheckedEncoded, SwapOrigin,
};
use cf_primitives::{
	AffiliateShortId, Affiliates, Asset, AssetAmount, BasisPoints, Beneficiaries, Beneficiary,
	BlockNumber, ChannelId, DcaParameters, ForeignChain, PriceLimits, SwapId, SwapLeg,
	SwapRequestId, BASIS_POINTS_PER_MILLION, FLIPPERINOS_PER_FLIP, MAX_BASIS_POINTS,
	SECONDS_PER_BLOCK, STABLE_ASSET, SWAP_DELAY_BLOCKS,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, AffiliateRegistry, AssetConverter, BalanceApi, Bonding,
	ChannelIdAllocator, DepositApi, FundingInfo, IngressEgressFeeApi, SwapOutputAction,
	SwapParameterValidation, SwapRequestHandler, SwapRequestType, SwapRequestTypeEncoded, SwapType,
	SwappingApi,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{Get, Saturating},
		AccountId32, DispatchError, Permill, TransactionOutcome,
	},
	storage::with_transaction_unchecked,
	traits::{Defensive, HandleLifetime},
	transactional, CloneNoBound, Hashable,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use serde::{Deserialize, Serialize};
use sp_arithmetic::{
	helpers_128bit::multiply_by_rational_with_rounding,
	traits::{UniqueSaturatedInto, Zero},
	Rounding,
};
use sp_runtime::traits::TrailingZeroInput;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec,
	vec::Vec,
};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;

pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(13);

pub(crate) const DEFAULT_SWAP_RETRY_DELAY_BLOCKS: u32 = 5;
const DEFAULT_MAX_SWAP_RETRY_DURATION_BLOCKS: u32 = 3600 / SECONDS_PER_BLOCK as u32; // 1 hour
const DEFAULT_MAX_SWAP_REQUEST_DURATION_BLOCKS: u32 = 86_400 / SECONDS_PER_BLOCK as u32; // 24 hours

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

enum EgressType {
	Regular,
	Refund { refund_fee: AssetAmount },
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Copy, Clone)]
pub struct AffiliateDetails {
	pub short_id: AffiliateShortId,
	pub withdrawal_address: EthereumAddress,
}

/// Refund parameter used within the swapping pallet.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct SwapRefundParameters {
	pub refund_block: cf_primitives::BlockNumber,
	pub price_limits: PriceLimits,
}

#[derive(CloneNoBound, DebugNoBound)]
pub struct SwapState<T: Config> {
	pub swap: Swap<T>,
	pub network_fee_taken: Option<AssetAmount>,
	pub broker_fee_taken: Option<AssetAmount>,
	pub stable_amount: Option<AssetAmount>,
	pub final_output: Option<AssetAmount>,
}

impl<T: Config> SwapState<T> {
	fn new(swap: Swap<T>) -> Self {
		Self {
			stable_amount: if swap.from == STABLE_ASSET { Some(swap.input_amount) } else { None },
			final_output: if swap.from == swap.to { Some(swap.input_amount) } else { None },
			network_fee_taken: None,
			broker_fee_taken: None,
			swap,
		}
	}

	pub fn swap_request_id(&self) -> SwapRequestId {
		self.swap.swap_request_id
	}

	fn swap_id(&self) -> SwapId {
		self.swap.swap_id
	}

	fn input_asset(&self) -> Asset {
		self.swap.from
	}

	fn output_asset(&self) -> Asset {
		self.swap.to
	}

	fn input_amount(&self) -> AssetAmount {
		self.swap.input_amount
	}

	fn refund_params(&self) -> &Option<SwapRefundParameters> {
		&self.swap.refund_params
	}

	fn update_swap_result(&mut self, direction: SwapLeg, output: AssetAmount) {
		match direction {
			SwapLeg::ToStable => {
				self.stable_amount = Some(output);
				if self.output_asset() == STABLE_ASSET {
					self.final_output = Some(output);
				}
			},
			SwapLeg::FromStable => self.final_output = Some(output),
		}
	}

	fn swap_amount(&self, direction: SwapLeg) -> Option<AssetAmount> {
		match direction {
			SwapLeg::ToStable => Some(self.input_amount()),
			SwapLeg::FromStable => self.stable_amount,
		}
	}

	fn swap_asset(&self, direction: SwapLeg) -> Option<Asset> {
		match (direction, self.input_asset(), self.output_asset()) {
			(SwapLeg::ToStable, STABLE_ASSET, _) => None,
			(SwapLeg::ToStable, from, _) => Some(from),
			(SwapLeg::FromStable, _, STABLE_ASSET) => None,
			(SwapLeg::FromStable, _, to) => Some(to),
		}
	}

	fn intermediate_amount(&self) -> Option<AssetAmount> {
		if self.input_asset() == STABLE_ASSET || self.output_asset() == STABLE_ASSET {
			None
		} else {
			self.stable_amount
		}
	}
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub enum FeeType<T: Config> {
	NetworkFee(NetworkFeeTracker),
	BrokerFee(Beneficiaries<T::AccountId>),
}

#[derive(
	Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, Default, Serialize, Deserialize,
)]
pub struct FeeRateAndMinimum {
	pub rate: sp_runtime::Permill,
	pub minimum: AssetAmount,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct NetworkFeeTracker {
	network_fee: FeeRateAndMinimum,
	// Total amount of stable asset that has been processed so far (before fees)
	accumulated_stable_amount: AssetAmount,
	accumulated_fee: AssetAmount,
}

impl NetworkFeeTracker {
	pub const fn new(network_fee: FeeRateAndMinimum) -> Self {
		Self { network_fee, accumulated_stable_amount: 0, accumulated_fee: 0 }
	}

	pub fn new_without_minimum(network_fee: FeeRateAndMinimum) -> Self {
		Self {
			network_fee: FeeRateAndMinimum { rate: network_fee.rate, minimum: 0 },
			accumulated_stable_amount: 0,
			accumulated_fee: 0,
		}
	}

	pub fn take_fee(&mut self, stable_amount: AssetAmount) -> FeeTaken {
		if stable_amount.is_zero() {
			return FeeTaken { remaining_amount: 0, fee: 0 };
		}
		let calculated_fee = core::cmp::max(
			self.network_fee.rate * (self.accumulated_stable_amount.saturating_add(stable_amount)),
			self.network_fee.minimum,
		);
		let fee_taken =
			core::cmp::min(calculated_fee.saturating_sub(self.accumulated_fee), stable_amount);

		self.accumulated_fee.saturating_accrue(fee_taken);
		self.accumulated_stable_amount.saturating_accrue(stable_amount);

		FeeTaken { remaining_amount: stable_amount.saturating_sub(fee_taken), fee: fee_taken }
	}
}

#[derive(Clone, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Swap<T: Config> {
	swap_id: SwapId,
	swap_request_id: SwapRequestId,
	pub from: Asset,
	pub to: Asset,
	input_amount: AssetAmount,
	fees: Vec<FeeType<T>>,
	refund_params: Option<SwapRefundParameters>,
	execute_at: BlockNumberFor<T>,
}

pub struct DefaultBrokerBond<T>(PhantomData<T>);
impl<T: Config> Get<T::Amount> for DefaultBrokerBond<T> {
	fn get() -> T::Amount {
		T::Amount::from(FLIPPERINOS_PER_FLIP * 100)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
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
		fees: impl IntoIterator<Item = FeeType<T>>,
		execute_at: BlockNumberFor<T>,
	) -> Self {
		Self {
			swap_id,
			swap_request_id,
			from,
			to,
			input_amount,
			fees: fees.into_iter().collect(),
			refund_params,
			execute_at,
		}
	}
}

pub enum BatchExecutionError<T: Config> {
	SwapLegFailed {
		asset: Asset,
		direction: SwapLeg,
		amount: AssetAmount,
		failed_swap_group: Vec<SwapState<T>>,
	},
	PriceViolation {
		violating_swaps: Vec<Swap<T>>,
		non_violating_swaps: Vec<Swap<T>>,
	},
	DispatchError {
		error: DispatchError,
	},
}

#[derive(DebugNoBound)]
struct BatchExecutionOutcomes<T: Config> {
	successful_swaps: Vec<SwapState<T>>,
	failed_swaps: Vec<Swap<T>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct ReplayProtection {
	nonce: u32, //TODO: Is this correct?
	expiry_block: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum UserSignatureData {
	Solana { signature: SolSignature, signer: SolAddress },
	Ethereum { signature: EthereumSignature, signer: EthereumAddress },
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Debug, PartialOrd, Ord)]
pub enum LendingApi {
	Borrow {},
}

#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Debug, PartialOrd, Ord)]
pub enum UserActionsApi {
	Lending(LendingApi),
	// reserved for future Apis for example Swap(SwapApi)...
	// This allows us to update the API without breaking the encoding.
}

/// This impl is never used. This is purely used to satisfy trait requirement
impl<T: Config> From<DispatchError> for BatchExecutionError<T> {
	fn from(error: DispatchError) -> Self {
		Self::DispatchError { error }
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct DcaState {
	scheduled_chunks: BTreeSet<SwapId>,
	remaining_input_amount: AssetAmount,
	remaining_chunks: u32,
	chunk_interval: u32,
	accumulated_output_amount: AssetAmount,
}

impl DcaState {
	fn new(input_amount: AssetAmount, params: Option<DcaParameters>) -> DcaState {
		DcaState {
			remaining_input_amount: input_amount,
			remaining_chunks: params.as_ref().map(|p| p.number_of_chunks).unwrap_or(1),
			// Chunk interval won't be used for non-DCA swaps but seems nicer to
			// set a reasonable default than unwrap Option when it is needed:
			chunk_interval: params.as_ref().map(|p| p.chunk_interval).unwrap_or(SWAP_DELAY_BLOCKS),
			accumulated_output_amount: 0,
			scheduled_chunks: BTreeSet::new(),
		}
	}

	/// Calculate the amount of the next chunk to be scheduled.
	fn calculate_next_chunk(&self) -> Option<AssetAmount> {
		if self.remaining_chunks > 0 {
			let chunk_input_amount = self
				.remaining_input_amount
				.checked_div(self.remaining_chunks as u128)
				.unwrap_or(0);

			Some(chunk_input_amount)
		} else {
			None
		}
	}

	/// Called directly after a chunk has been scheduled. Records the new swap in the DCA state.
	fn record_scheduled_chunk(
		&mut self,
		scheduled_chunk_swap_id: SwapId,
		scheduled_chunk_amount: AssetAmount,
	) {
		// Add the new chunk to the scheduled swaps.
		self.scheduled_chunks.insert(scheduled_chunk_swap_id);

		// Update the remaining values
		self.remaining_chunks.saturating_reduce(1);
		self.remaining_input_amount.saturating_reduce(scheduled_chunk_amount);
	}

	/// Remove the completed chunk from the DCA state and accumulate the output amount.
	fn record_chunk_completion(
		&mut self,
		completed_chunk_swap_id: SwapId,
		completed_chunk_output_amount: AssetAmount,
	) {
		if self.scheduled_chunks.remove(&completed_chunk_swap_id) {
			self.accumulated_output_amount += completed_chunk_output_amount;
		} else {
			log_or_panic!(
					"Invariant violation: the completed swap id {completed_chunk_swap_id} does not match a scheduled chunk."
				);
		}
	}
}

#[allow(clippy::large_enum_variant)]
#[derive(CloneNoBound, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
enum SwapRequestState<T: Config> {
	UserSwap {
		refund_params: Option<ChannelRefundParametersCheckedInternal<T::AccountId>>,
		output_action: SwapOutputAction<T::AccountId>,
		dca_state: DcaState,
	},
	NetworkFee,
	IngressEgressFee,
}

#[derive(CloneNoBound, DebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(T))]
struct SwapRequest<T: Config> {
	id: SwapRequestId,
	input_asset: Asset,
	output_asset: Asset,
	state: SwapRequestState<T>,
}

#[derive(Clone, RuntimeDebugNoBound, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
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
}

impl_pallet_safe_mode! {
	PalletSafeMode; swaps_enabled, withdrawals_enabled, broker_registration_enabled,
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
	use core::cmp::max;

	use cf_amm::math::{output_amount_ceil, output_amount_floor};
	use cf_chains::{
		address::EncodedAddress, AnyChain, CcmChannelMetadataChecked, CcmChannelMetadataUnchecked,
		Chain, ChannelRefundParametersCheckedInternal,
	};
	use cf_primitives::{
		AffiliateShortId, Asset, AssetAmount, BasisPoints, BlockNumber, DcaParameters, EgressId,
		Price, PriceLimits, SwapId, SwapOutput, SwapRequestId,
	};
	use cf_traits::{
		AccountRoleRegistry, Chainflip, EgressApi, PoolPriceProvider, PriceFeedApi,
		ScheduledEgressDetails,
	};
	use frame_system::WeightInfo as SystemWeightInfo;
	use sp_runtime::SaturatedConversion;

	use super::*;
	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Standard Event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

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

		type PoolPriceApi: PoolPriceProvider;

		type PriceFeedApi: PriceFeedApi;

		type ChannelIdAllocator: ChannelIdAllocator;

		type Bonder: Bonding<
			AccountId = <Self as frame_system::Config>::AccountId,
			Amount = <Self as Chainflip>::Amount,
		>;
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
	pub type FlipToBurn<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

	/// Interval at which we buy FLIP in order to burn it.
	#[pallet::storage]
	pub type FlipBuyInterval<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// Network fees, in USDC terms, that have been collected and are ready to be converted to FLIP.
	#[pallet::storage]
	pub type CollectedNetworkFee<T: Config> = StorageValue<_, AssetAmount, ValueQuery>;

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

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	#[allow(clippy::large_enum_variant)]
	pub enum Event<T: Config> {
		/// New swap has been requested
		SwapRequested {
			swap_request_id: SwapRequestId,
			input_asset: Asset,
			input_amount: AssetAmount, // includes broker fee
			output_asset: Asset,
			origin: SwapOrigin<T::AccountId>,
			request_type: SwapRequestTypeEncoded<T::AccountId>,
			broker_fees: Beneficiaries<T::AccountId>,
			refund_parameters: Option<ChannelRefundParametersCheckedInternal<T::AccountId>>,
			dca_parameters: Option<DcaParameters>,
		},
		SwapRequestCompleted {
			swap_request_id: SwapRequestId,
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
		},
		/// A swap has been executed.
		SwapExecuted {
			swap_request_id: SwapRequestId,
			swap_id: SwapId,
			input_asset: Asset,
			output_asset: Asset,
			// this amount excludes all fees (e.g. network fee, broker fee, etc.)
			input_amount: AssetAmount,
			network_fee: AssetAmount,
			broker_fee: AssetAmount,
			intermediate_amount: Option<AssetAmount>,
			output_amount: AssetAmount,
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
			refund_fee: AssetAmount,
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
			refund_fee: AssetAmount,
		},
		PalletConfigUpdated {
			update: PalletConfigUpdate<T>,
		},
		VaultSwapMinimumBrokerFeeSet {
			broker_id: T::AccountId,
			minimum_fee_bps: BasisPoints,
		},
		SwapCanceled {
			swap_id: SwapId,
		},
		UserSignedTransactionSubmitted {
			broker_id: T::AccountId,
			signer_account_id: AccountId32,
			payload: Vec<u8>,
			replay_protection: ReplayProtection,
			user_signature_data: UserSignatureData,
			valid: bool,
			expired: bool,
			signed_payload: Vec<u8>,
			decoded_action: Option<UserActionsApi>,
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
				weight_used.saturating_accrue(T::DbWeight::get().reads(1));
				if (current_block % interval).is_zero() &&
					!CollectedNetworkFee::<T>::get().is_zero()
				{
					weight_used.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
					CollectedNetworkFee::<T>::mutate(|collected_fee| {
						Self::init_network_fee_swap_request(Asset::Usdc, *collected_fee);

						collected_fee.set_zero();
					});
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

				swaps_to_execute.into_values().collect::<Vec<_>>()
			});

			let retry_delay = max(SwapRetryDelay::<T>::get(), 1u32.into());

			if !T::SafeMode::get().swaps_enabled {
				// Since we won't be executing swaps at this block, we need to reschedule them:
				for swap in swaps_to_execute {
					Self::reschedule_swap(swap, retry_delay);
				}

				return
			}

			let BatchExecutionOutcomes { successful_swaps, failed_swaps } =
				Self::execute_batch(swaps_to_execute.clone());

			for swap in successful_swaps {
				Self::process_swap_outcome(swap);
			}

			for swap in failed_swaps {
				match swap.refund_params {
					Some(ref params)
						if BlockNumberFor::<T>::from(params.refund_block) <
							current_block + retry_delay =>
					{
						// Reached refund block, process refund:
						Self::refund_failed_swap(swap);
					},
					_ => {
						// Either refund parameters not set, or refund block not
						// reached:
						Self::reschedule_swap(swap, retry_delay);
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

			ensure!(
				!BrokerPrivateBtcChannels::<T>::contains_key(&account_id),
				Error::<T>::PrivateChannelExistsForBroker
			);

			ensure!(
				T::BalanceApi::free_balances(&account_id).iter().all(|(_, amount)| *amount == 0),
				Error::<T>::EarnedFeesNotWithdrawn,
			);

			// Check the affiliate's balance before we allow deregistration
			for affiliate_account_id in AffiliateAccountDetails::<T>::iter_key_prefix(&account_id) {
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
				})
			}

			// Clear the affiliate account details and affiliate id mapping.
			// With this the broker has no longer access to the affiliate's account.
			let _ = AffiliateAccountDetails::<T>::clear_prefix(&account_id, u32::MAX, None);
			let _ = AffiliateIdMapping::<T>::clear_prefix(&account_id, u32::MAX, None);

			T::AccountRoleRegistry::deregister_as_broker(&account_id)?;

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

			let next_id: u8 = AffiliateIdMapping::<T>::iter_prefix_values(&broker_id)
				.count()
				.try_into()
				.map_err(|_| Error::<T>::AffiliateShortIdOutOfBounds)?;

			let short_id = AffiliateShortId::from(next_id);

			ensure!(
				!AffiliateIdMapping::<T>::contains_key(&broker_id, short_id),
				Error::<T>::AffiliateAlreadyRegistered
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

		#[pallet::call_index(18)]
		#[pallet::weight(T::WeightInfo::submit_user_signed_payload())]
		pub fn submit_user_signed_payload(
			origin: OriginFor<T>,
			payload: Vec<u8>,
			replay_protection: ReplayProtection,
			user_signature_data: UserSignatureData,
		) -> DispatchResult {
			use cf_chains::{
				evm::{EvmCrypto, ToAccountId32},
				sol::SolanaCrypto,
				ChainCrypto,
			};

			let broker_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			// TODO: Get Statechain's genesis hash from runtime api instead.
			let genesis_hash = cf_runtime_utilities::genesis_hashes::PERSEVERANCE;

			let signed_payload =
				[payload.clone(), genesis_hash.to_vec(), replay_protection.encode()].concat();

			let (valid, signer_account_id, decoded_action) = match user_signature_data {
				UserSignatureData::Solana { signature, signer } => (
					SolanaCrypto::verify_signature(&signer, &signed_payload, &signature),
					AccountId32::new(signer.into()),
					UserActionsApi::decode(&mut &payload[..]).ok(),
				),
				// Add prefix here from eth personal_sign. TBD if this is how we want to approach
				// it.
				UserSignatureData::Ethereum { signature, signer } => {
					let prefix = scale_info::prelude::format!(
						"\x19Ethereum Signed Message:\n{}",
						signed_payload.len()
					);
					let prefix_bytes = prefix.as_bytes();
					let prefixed_signed_payload = [prefix_bytes, &signed_payload].concat();
					(
						EvmCrypto::verify_signature(&signer, &prefixed_signed_payload, &signature),
						signer.into_account_id_32(),
						UserActionsApi::decode(&mut &payload[..]).ok(),
					)
				},
			};

			// TODO: Add check of replay protection mechanism (esp. nonce)
			// Check expiry
			let expired =
				frame_system::Pallet::<T>::block_number() >= replay_protection.expiry_block.into();

			// TODO: Decode the payload and execute the intended action on behalf
			// of the user, similar to the delegation Sc Api.

			Self::deposit_event(Event::<T>::UserSignedTransactionSubmitted {
				broker_id,
				signer_account_id,
				payload,
				replay_protection,
				user_signature_data,
				valid,
				expired,
				signed_payload,
				decoded_action,
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		#[allow(clippy::result_unit_err)]
		pub fn get_scheduled_swap_legs(base_asset: Asset) -> Vec<(SwapLegInfo, BlockNumberFor<T>)> {
			let mut swaps: Vec<_> = ScheduledSwaps::<T>::get()
				.values()
				.filter(|swap| swap.from == base_asset || swap.to == base_asset)
				.cloned()
				.map(SwapState::new)
				.collect();

			// Can ignore the result here because we use pool price fallback below
			let _res = Self::swap_into_stable_taking_fees(&mut swaps);

			swaps
				.into_iter()
				.filter_map(|state| {
					let swap_request = SwapRequests::<T>::get(state.swap_request_id())
						.expect("Swap request should exist");
					let dca_state = match swap_request.state {
						SwapRequestState::UserSwap { dca_state, .. } => Some(dca_state),
						_ => None,
					};
					let remaining_chunks =
						dca_state.as_ref().map(|dca| dca.remaining_chunks).unwrap_or(0);
					let chunk_interval =
						dca_state.map(|dca| dca.chunk_interval).unwrap_or(SWAP_DELAY_BLOCKS);

					if state.input_asset() == base_asset {
						Some((
							SwapLegInfo {
								swap_id: state.swap_id(),
								swap_request_id: state.swap_request_id(),
								base_asset,
								// All swaps from `base_asset` have to go through the stable asset:
								quote_asset: STABLE_ASSET,
								side: Side::Sell,
								amount: state.input_amount(),
								source_asset: None,
								source_amount: None,
								remaining_chunks,
								chunk_interval,
							},
							state.swap.execute_at,
						))
					} else if state.output_asset() == base_asset {
						// In case the swap is "simulated", the amount is just an estimate,
						// so we additionally include `source_asset` and `source_amount`:
						let (source_asset, source_amount) = if state.input_asset() != STABLE_ASSET {
							(Some(state.input_asset()), Some(state.input_amount()))
						} else {
							(None, None)
						};

						let amount = state.stable_amount.or_else(|| {
							// If the swap into stable asset failed, fallback to estimating the
							// amount via pool price.

							// Should be able to successfully retrieve the price since the pool
							// should exist as we wouldn't need to estimate if input asset
							// was already STABLE_ASSET):
							let sell_price =
								T::PoolPriceApi::pool_price(state.input_asset(), STABLE_ASSET)
									.ok()
									.map(|price| price.sell)?;

							Some(
								output_amount_ceil(
									cf_amm::math::Amount::from(state.input_amount()),
									sell_price,
								)
								.saturated_into(),
							)
						})?;

						Some((
							SwapLegInfo {
								swap_id: state.swap_id(),
								swap_request_id: state.swap_request_id(),
								base_asset,
								// All swaps to `base_asset` have to go through the stable asset:
								quote_asset: STABLE_ASSET,
								side: Side::Buy,
								amount,
								source_asset,
								source_amount,
								remaining_chunks,
								chunk_interval,
							},
							state.swap.execute_at,
						))
					} else {
						None
					}
				})
				.collect()
		}

		fn trigger_withdrawal(
			account_id: &T::AccountId,
			asset: Asset,
			destination_address: ForeignChainAddress,
		) -> DispatchResult {
			let earned_fees = T::BalanceApi::get_balance(account_id, asset);
			ensure!(earned_fees != 0, Error::<T>::NoFundsAvailable);
			T::BalanceApi::try_debit_account(account_id, asset, earned_fees)?;

			let ScheduledEgressDetails { egress_id, egress_amount, fee_withheld } =
				T::EgressHandler::schedule_egress(
					asset,
					earned_fees,
					destination_address.clone(),
					None,
				)
				.map_err(Into::into)?;

			Self::deposit_event(Event::<T>::WithdrawalRequested {
				account_id: account_id.clone(),
				egress_amount,
				egress_asset: asset,
				egress_fee: fee_withheld,
				destination_address: T::AddressConverter::to_encoded_address(destination_address),
				egress_id,
			});

			Ok(())
		}

		fn take_broker_fees(
			stable_amount: AssetAmount,
			broker_fees: &Beneficiaries<T::AccountId>,
		) -> FeeTaken {
			// Sanity check: it should already not be possible to open a channel with broker fees
			// this high, but if the total broker fee would exceed 100% we charge no broker fee
			// instead (for simplicity):
			let total_fee_bps =
				broker_fees.iter().fold(0u16, |fee_accumulator, Beneficiary { bps, .. }| {
					fee_accumulator.saturating_add(*bps)
				});

			if total_fee_bps > MAX_BASIS_POINTS {
				FeeTaken { remaining_amount: stable_amount, fee: 0 }
			} else {
				let total_fee = broker_fees
					.iter()
					.filter(|Beneficiary { account: _, bps }| *bps > 0)
					.fold(0u128, |fee_accumulator, Beneficiary { account, bps }| {
						let fee = Permill::from_parts(*bps as u32 * BASIS_POINTS_PER_MILLION) *
							stable_amount;

						T::BalanceApi::credit_account(account, STABLE_ASSET, fee);

						fee_accumulator.saturating_add(fee)
					});

				assert!(total_fee <= stable_amount, "Broker fee cannot be more than the amount");

				FeeTaken {
					remaining_amount: stable_amount.saturating_sub(total_fee),
					fee: total_fee,
				}
			}
		}

		fn swap_into_stable_taking_fees(
			swaps: &mut [SwapState<T>],
		) -> Result<(), BatchExecutionError<T>> {
			Self::do_group_and_swap(swaps, SwapLeg::ToStable)?;

			// Take fees as required:
			let mut total_network_fee_taken = 0_u128;
			for swap in swaps.iter_mut() {
				debug_assert!(
					swap.stable_amount.is_some(),
					"All swaps should have Stable amount set here"
				);

				for fee_type in swap.swap.fees.iter_mut() {
					let remaining_amount = match fee_type {
						FeeType::NetworkFee(fee_tracker) => {
							let FeeTaken { remaining_amount, fee } =
								fee_tracker.take_fee(swap.stable_amount.unwrap_or_default());
							swap.network_fee_taken = Some(fee);
							total_network_fee_taken.saturating_accrue(fee);
							remaining_amount
						},
						FeeType::BrokerFee(beneficiaries) => {
							let FeeTaken { remaining_amount, fee } = Self::take_broker_fees(
								swap.stable_amount.unwrap_or_default(),
								beneficiaries,
							);
							swap.broker_fee_taken = Some(fee);
							remaining_amount
						},
					};
					swap.stable_amount = Some(remaining_amount);
				}

				if swap.output_asset() == STABLE_ASSET {
					swap.final_output = swap.stable_amount;
				}
			}

			if !total_network_fee_taken.is_zero() {
				CollectedNetworkFee::<T>::mutate(|total| {
					total.saturating_accrue(total_network_fee_taken);
				});
			}

			Ok(())
		}

		#[transactional]
		pub fn try_execute_without_violations(
			swaps: Vec<Swap<T>>,
		) -> Result<Vec<SwapState<T>>, BatchExecutionError<T>> {
			let mut swaps: Vec<_> = swaps.into_iter().map(SwapState::new).collect();
			Self::swap_into_stable_taking_fees(&mut swaps)?;

			// Swap from Stable asset, and complete the swap logic.
			Self::do_group_and_swap(&mut swaps, SwapLeg::FromStable)?;

			// Successfully executed without hitting price impact limit.
			// Now checking for FoK violations:
			let (non_violating, violating): (Vec<_>, Vec<_>) =
				swaps.into_iter().partition(|swap| {
					swap.refund_params().as_ref().is_none_or(|params| {
						// Live price protection, aka oracle price protection
						let min_price_from_oracle =
							params.price_limits.max_oracle_price_slippage.map(|slippage_bps| {
								match (
									T::PriceFeedApi::get_price(swap.input_asset()),
									T::PriceFeedApi::get_price(swap.output_asset()),
								) {
									(Some(oracle1), Some(oracle2))
										if oracle1.stale || oracle2.stale =>
									{
										// If a oracle price is stale use a max value to force a
										// price violation so the swap will be rescheduled.
										Price::MAX
									},
									(None, _) | (_, None) => {
										// Ignore the oracle price check if not supported/available
										// for one of the assets.
										Price::zero()
									},
									(Some(oracle1), Some(oracle2)) => {
										let relative_price = cf_amm::math::relative_price(
											oracle1.price,
											oracle2.price,
										);
										// Reduce the relative price by slippage_bps:
										cf_amm::math::mul_div_floor(
											relative_price,
											(MAX_BASIS_POINTS - slippage_bps).into(),
											MAX_BASIS_POINTS,
										)
									},
								}
							});

						// Use the larger of the two prices
						let min_price = core::cmp::max(
							min_price_from_oracle.unwrap_or_default(),
							params.price_limits.min_price,
						);

						let min_output =
							output_amount_floor(swap.swap.input_amount.into(), min_price)
								.unique_saturated_into();

						swap.final_output.unwrap() >= min_output
					})
				});

			if violating.is_empty() {
				Ok(non_violating)
			} else {
				Err(BatchExecutionError::PriceViolation {
					violating_swaps: violating.into_iter().map(|ctx| ctx.swap).collect(),
					non_violating_swaps: non_violating.into_iter().map(|ctx| ctx.swap).collect(),
				})
			}
		}

		/// Attempts to find (and execute) a batch of swaps that wouldn't result in hitting the
		/// price impact limit, starting with the given batch, and taking swaps out of the batch if
		/// needed.
		fn execute_batch(mut swaps_to_execute: Vec<Swap<T>>) -> BatchExecutionOutcomes<T> {
			let mut failed_swaps = vec![];

			loop {
				if swaps_to_execute.is_empty() {
					return BatchExecutionOutcomes { successful_swaps: vec![], failed_swaps };
				}

				match Self::try_execute_without_violations(swaps_to_execute.clone()) {
					Ok(successful_swaps) =>
						return BatchExecutionOutcomes { successful_swaps, failed_swaps },
					Err(BatchExecutionError::SwapLegFailed {
						asset,
						direction,
						amount,
						failed_swap_group,
					}) => {
						Self::deposit_event(Event::<T>::BatchSwapFailed {
							asset,
							direction,
							amount,
						});

						// Find the largest swap from the failing pool/direction and remove it
						// so we can try the remaining swaps again. We should always be able to
						// find a swap to remove, but if we can't for some reason, abort.
						if let Some(removed_swap) = utilities::split_off_highest_impact_swap(
							&mut swaps_to_execute,
							&failed_swap_group,
							direction,
						) {
							failed_swaps.push(removed_swap);
						} else {
							break;
						}
					},
					Err(BatchExecutionError::PriceViolation {
						violating_swaps,
						non_violating_swaps,
					}) => {
						failed_swaps.extend(violating_swaps);
						swaps_to_execute = non_violating_swaps;
					},
					Err(BatchExecutionError::DispatchError { error }) => {
						// This should only happen when the transaction nested too deep,
						// which should not happen in practice (max nesting is 255):
						log_or_panic!("Failed to execute swap batch: {error:?}");
						break;
					},
				}
			}

			// If we are here, consider all swaps as failed:
			failed_swaps.extend(swaps_to_execute);
			BatchExecutionOutcomes { successful_swaps: vec![], failed_swaps }
		}

		fn refund_failed_swap(swap: Swap<T>) {
			let swap_request_id = swap.swap_request_id;

			let Some(mut request) = SwapRequests::<T>::take(swap_request_id) else {
				log_or_panic!("Swap request {swap_request_id} not found");
				return;
			};

			match &mut request.state {
				SwapRequestState::UserSwap { output_action, refund_params, dca_state } => {
					let Some(refund_params) = &refund_params else {
						log_or_panic!("Trying to refund swap request {swap_request_id}, but missing refund parameters");
						return;
					};

					// Cancel any other scheduled swaps for this swap request and add the amounts
					// back to the input remaining.
					let canceled_swaps_amount = dca_state
						.scheduled_chunks
						.iter()
						.filter(|swap_id| *swap_id != &swap.swap_id)
						.fold(0, |acc: u128, swap_id| {
							acc.saturating_add(Self::cancel_swap(*swap_id))
						});

					let total_input_remaining = swap.input_amount +
						dca_state.remaining_input_amount +
						canceled_swaps_amount;
					let FeeTaken { remaining_amount: amount_to_refund, fee: refund_fee } =
						match Self::take_refund_fee(
							total_input_remaining,
							request.input_asset,
							matches!(output_action, SwapOutputAction::CreditOnChain { .. }),
						) {
							Ok(fee_taken) => fee_taken,
							Err(e) => {
								log_or_panic!(
								"Failed to calculate refund fee for swap request {swap_request_id}: {e:?}"
							);
								FeeTaken { remaining_amount: total_input_remaining, fee: 0 }
							},
						};

					if amount_to_refund > 0 {
						match &refund_params.refund_address {
							AccountOrAddress::ExternalAddress(address) => {
								Self::egress_for_swap(
									request.id,
									amount_to_refund,
									request.input_asset,
									address.clone(),
									refund_params.refund_ccm_metadata.clone(),
									EgressType::Refund { refund_fee },
								);
							},
							AccountOrAddress::InternalAccount(account_id) => {
								Self::deposit_event(Event::<T>::RefundedOnChain {
									swap_request_id,
									account_id: account_id.clone(),
									asset: request.input_asset,
									amount: amount_to_refund,
									refund_fee,
								});

								T::BalanceApi::credit_account(
									account_id,
									request.input_asset,
									amount_to_refund,
								);
							},
						}
					} else {
						Self::deposit_event(Event::<T>::RefundEgressIgnored {
							swap_request_id,
							asset: request.input_asset,
							amount: amount_to_refund,
							reason: DispatchError::from(Error::<T>::NoRefundAmountRemaining),
						});
					}

					// In case of DCA we may have partially swapped and now have some output
					// asset to egress to the output address:
					if dca_state.accumulated_output_amount > 0 {
						match output_action {
							SwapOutputAction::Egress { ccm_deposit_metadata, output_address } => {
								Self::egress_for_swap(
									swap_request_id,
									dca_state.accumulated_output_amount,
									request.output_asset,
									output_address.clone(),
									ccm_deposit_metadata.clone(),
									EgressType::Regular,
								);
							},
							SwapOutputAction::CreditOnChain { account_id } => {
								Self::deposit_event(Event::<T>::CreditedOnChain {
									swap_request_id,
									account_id: account_id.clone(),
									asset: request.output_asset,
									amount: dca_state.accumulated_output_amount,
								});

								T::BalanceApi::credit_account(
									account_id,
									request.output_asset,
									dca_state.accumulated_output_amount,
								);
							},
						}
					}
				},
				non_refundable_request => {
					log_or_panic!(
						"Refund for swap request is not supported: {non_refundable_request:?}"
					);
				},
			};
			Self::deposit_event(Event::<T>::SwapRequestCompleted { swap_request_id: request.id });
		}

		// Removes the swap from the scheduled swaps and returns the input amount of the canceled
		// swap.
		fn cancel_swap(swap_id: SwapId) -> AssetAmount {
			ScheduledSwaps::<T>::mutate(|swaps| {
				let amount = swaps.remove(&swap_id).map(|swap| {
					Self::deposit_event(Event::<T>::SwapCanceled { swap_id: swap.swap_id });
					swap.input_amount
				});
				if amount.is_none() {
					log_or_panic!("Attempted to cancel swap {swap_id}, but it was not found in ScheduledSwaps");
				}
				amount.unwrap_or_default()
			})
		}

		fn process_swap_outcome(swap: SwapState<T>) {
			let swap_request_id = swap.swap_request_id();

			let Some(mut request) = SwapRequests::<T>::take(swap_request_id) else {
				log_or_panic!("Swap request {swap_request_id} not found");
				return;
			};

			let Some(output_amount) = swap.final_output else {
				log_or_panic!("Swap {} is not completed yet!", swap.swap_id());
				return;
			};

			Self::deposit_event(Event::<T>::SwapExecuted {
				swap_request_id,
				swap_id: swap.swap_id(),
				// To be consistent with `swap_output` and `intermediate_amount` (which do
				// not include the network fee), we report input amount without the network fee
				// for swaps from STABLE_ASSET:
				input_amount: if swap.input_asset() == STABLE_ASSET {
					swap.stable_amount.unwrap_or_else(|| {
						log_or_panic!("stable amount must be set for swaps from STABLE_ASSET");
						swap.input_amount()
					})
				} else {
					swap.input_amount()
				},
				input_asset: swap.input_asset(),
				network_fee: swap.network_fee_taken.unwrap_or_default(),
				broker_fee: swap.broker_fee_taken.unwrap_or_default(),
				output_asset: swap.output_asset(),
				output_amount,
				intermediate_amount: swap.intermediate_amount(),
			});

			let request_completed = match &mut request.state {
				SwapRequestState::UserSwap { output_action, dca_state, refund_params, .. } =>
					if let Some(chunk_input_amount) = dca_state.calculate_next_chunk() {
						let swap_id = Self::schedule_swap(
							request.input_asset,
							request.output_asset,
							chunk_input_amount,
							refund_params.as_ref(),
							SwapType::Swap,
							swap.swap.fees.clone(),
							request.id,
							// Schedule the next chunk to be after any currently scheduled chunks
							(dca_state.scheduled_chunks.len() as u32)
								.saturating_mul(dca_state.chunk_interval)
								.into(),
						);

						dca_state.record_scheduled_chunk(swap_id, chunk_input_amount);
						dca_state.record_chunk_completion(swap.swap_id(), output_amount);

						false
					} else {
						debug_assert!(dca_state.remaining_input_amount == 0);

						dca_state.record_chunk_completion(swap.swap_id(), output_amount);

						if dca_state.scheduled_chunks.is_empty() {
							match output_action {
								SwapOutputAction::Egress {
									ccm_deposit_metadata,
									output_address,
								} => {
									Self::egress_for_swap(
										swap_request_id,
										dca_state.accumulated_output_amount,
										swap.output_asset(),
										output_address.clone(),
										ccm_deposit_metadata.clone(),
										EgressType::Regular,
									);
								},
								SwapOutputAction::CreditOnChain { account_id } => {
									Self::deposit_event(Event::<T>::CreditedOnChain {
										swap_request_id,
										account_id: account_id.clone(),
										asset: request.output_asset,
										amount: dca_state.accumulated_output_amount,
									});

									T::BalanceApi::credit_account(
										account_id,
										request.output_asset,
										dca_state.accumulated_output_amount,
									);
								},
							}
							true
						} else {
							false
						}
					},
				SwapRequestState::NetworkFee => {
					if swap.output_asset() == Asset::Flip {
						FlipToBurn::<T>::mutate(|total| {
							total.saturating_accrue(output_amount);
						});
					} else {
						log_or_panic!(
							"NetworkFee burning should not be in asset: {:?}",
							swap.output_asset()
						);
					}
					true
				},
				SwapRequestState::IngressEgressFee => {
					if swap.output_asset() == ForeignChain::from(swap.output_asset()).gas_asset() {
						T::IngressEgressFeeHandler::accrue_withheld_fee(
							swap.output_asset(),
							output_amount,
						);
					} else {
						log_or_panic!(
							"IngressEgressFee swap should not be to non-gas asset: {:?}",
							swap.output_asset()
						);
					}

					true
				},
			};

			if request_completed {
				Self::deposit_event(Event::<T>::SwapRequestCompleted { swap_request_id });
			} else {
				SwapRequests::<T>::insert(swap_request_id, request);
			}
		}

		// Helper function that splits swaps of a given direction, group them by asset
		// and do the swaps of a given direction. Processed and unprocessed swaps are
		// returned.
		fn do_group_and_swap(
			swaps: &mut [SwapState<T>],
			direction: SwapLeg,
		) -> Result<(), BatchExecutionError<T>> {
			let swap_groups =
				swaps.iter_mut().fold(BTreeMap::new(), |mut groups: BTreeMap<_, Vec<_>>, swap| {
					if let Some(asset) = swap.swap_asset(direction) {
						groups.entry(asset).or_default().push(swap);
					}
					groups
				});

			for (asset, mut swaps) in swap_groups {
				Self::execute_group_of_swaps(&mut swaps, asset, direction).map_err(|amount| {
					BatchExecutionError::SwapLegFailed {
						asset,
						direction,
						amount,
						failed_swap_group: swaps.into_iter().map(|swap| swap.clone()).collect(),
					}
				})?;
			}
			Ok(())
		}

		/// Bundle the given swaps and do a single swap of a given direction. Updates the given
		/// swaps in-place. If batch swap failed, return the input amount.
		fn execute_group_of_swaps(
			swaps: &mut [&mut SwapState<T>],
			asset: Asset,
			direction: SwapLeg,
		) -> Result<(), AssetAmount> {
			// Stable -> stable swap should never be called.
			debug_assert_ne!(asset, STABLE_ASSET);
			debug_assert!(
				!swaps.is_empty(),
				"The implementation of grouped_swaps ensures that the swap groups are non-empty."
			);

			let bundle_input: AssetAmount =
				swaps.iter().map(|swap| swap.swap_amount(direction).unwrap_or_default()).sum();

			// Process the swap leg as a bundle. No network fee is taken here.
			let bundle_output = T::SwappingApi::swap_single_leg(
				match direction {
					SwapLeg::FromStable => STABLE_ASSET,
					SwapLeg::ToStable => asset,
				},
				match direction {
					SwapLeg::FromStable => asset,
					SwapLeg::ToStable => STABLE_ASSET,
				},
				bundle_input,
			)
			.map_err(|_| bundle_input)?;

			for swap in swaps.iter_mut() {
				let swap_output = if bundle_input > 0 {
					multiply_by_rational_with_rounding(
						swap.swap_amount(direction).unwrap_or_default(),
						bundle_output,
						bundle_input,
						Rounding::Down,
					)
					.expect(
						"bundle_input >= swap_amount && bundle_input != 0 ∴ result can't overflow",
					)
				} else {
					0
				};

				swap.update_swap_result(direction, swap_output);

				if swap_output == 0 {
					// This is unlikely but theoretically possible if, for example, the initial swap
					// input is so small compared to the total bundle size that it rounds down to
					// zero when we do the division.
					log::warn!(
						"Swap {:?} in bundle {{ input: {bundle_input}, output: {bundle_output} }}
						resulted in swap output of zero.",
						swap.swap
					);
				}
			}

			Ok(())
		}

		fn schedule_swap(
			input_asset: Asset,
			output_asset: Asset,
			input_amount: AssetAmount,
			refund_params: Option<&ChannelRefundParametersCheckedInternal<T::AccountId>>,
			swap_type: SwapType,
			fees: Vec<FeeType<T>>,
			swap_request_id: SwapRequestId,
			delay_blocks: BlockNumberFor<T>,
		) -> SwapId {
			let swap_id = SwapIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			let execute_at = frame_system::Pallet::<T>::block_number() + delay_blocks;

			let refund_params = refund_params.map(|params| {
				use sp_runtime::traits::UniqueSaturatedInto;

				let execute_at: cf_primitives::BlockNumber = execute_at.unique_saturated_into();

				SwapRefundParameters {
					refund_block: execute_at.saturating_add(params.retry_duration),
					price_limits: PriceLimits {
						min_price: params.min_price,
						max_oracle_price_slippage: params.max_oracle_price_slippage,
					},
				}
			});

			ScheduledSwaps::<T>::mutate(|swaps| {
				swaps.insert(
					swap_id,
					Swap::new(
						swap_id,
						swap_request_id,
						input_asset,
						output_asset,
						input_amount,
						refund_params,
						fees,
						execute_at,
					),
				)
			});

			Self::deposit_event(Event::<T>::SwapScheduled {
				swap_request_id,
				swap_id,
				input_amount,
				swap_type,
				execute_at,
			});

			swap_id
		}

		fn reschedule_swap(mut swap: Swap<T>, retry_delay: BlockNumberFor<T>) {
			SwapRequests::<T>::mutate(swap.swap_request_id, |request| {
				if let Some(request) = request {
					if let SwapRequestState::UserSwap { dca_state, .. } = &mut request.state {
						ScheduledSwaps::<T>::mutate(|swaps| {
							// Reschedule the main swap that was taken from the storage.
							let execute_at = swap.execute_at.saturating_add(retry_delay);
							let main_swap_id = swap.swap_id;
							swap.execute_at = execute_at;
							swaps.insert(main_swap_id, swap);
							Self::deposit_event(Event::<T>::SwapRescheduled {
								swap_id: main_swap_id,
								execute_at,
							});
							for swap_id in dca_state.scheduled_chunks.iter().copied() {
								if swap_id != main_swap_id {
									// All other scheduled swaps for this request need to also be
									// rescheduled.
									if let Some(s) = swaps.get_mut(&swap_id) {
										s.execute_at.saturating_accrue(retry_delay);
										Self::deposit_event(Event::<T>::SwapRescheduled {
											swap_id,
											execute_at: s.execute_at,
										});
									} else {
										log_or_panic!(
											"Swap {swap_id} not found in ScheduledSwaps for rescheduling",
										);
									}
								}
							}
						})
					}
				} else {
					log_or_panic!(
						"Swap request {} not found for rescheduling",
						swap.swap_request_id
					);
				}
			});
		}

		#[transactional]
		/// Must be called within a rollback. Used to simulate a swap for calculating gas amounts.
		/// Note: Network fees are taken into account, but not collected.
		pub fn swap_with_network_fee_for_gas(
			from: Asset,
			to: Asset,
			input_amount: AssetAmount,
		) -> Result<SwapOutput, DispatchError> {
			let mut network_fee_tracker =
				NetworkFeeTracker::new_without_minimum(NetworkFee::<T>::get());
			Ok(match (from, to) {
				(_, STABLE_ASSET) => {
					let FeeTaken { remaining_amount: output, fee } = network_fee_tracker
						.take_fee(T::SwappingApi::swap_single_leg(from, to, input_amount)?);

					SwapOutput { intermediary: None, output, network_fee: fee }
				},
				(STABLE_ASSET, _) => {
					let FeeTaken { remaining_amount: input_amount, fee } =
						network_fee_tracker.take_fee(input_amount);

					SwapOutput {
						intermediary: None,
						output: T::SwappingApi::swap_single_leg(from, to, input_amount)?,
						network_fee: fee,
					}
				},
				_ => {
					let FeeTaken { remaining_amount: intermediary, fee } = network_fee_tracker
						.take_fee(T::SwappingApi::swap_single_leg(
							from,
							STABLE_ASSET,
							input_amount,
						)?);

					SwapOutput {
						intermediary: Some(intermediary),
						output: T::SwappingApi::swap_single_leg(STABLE_ASSET, to, intermediary)?,
						network_fee: fee,
					}
				},
			})
		}

		fn egress_for_swap(
			swap_request_id: SwapRequestId,
			amount: AssetAmount,
			asset: Asset,
			address: ForeignChainAddress,
			maybe_ccm_metadata: Option<CcmDepositMetadataChecked<ForeignChainAddress>>,
			egress_type: EgressType,
		) {
			let is_ccm = maybe_ccm_metadata.is_some();
			match T::EgressHandler::schedule_egress(asset, amount, address, maybe_ccm_metadata) {
				Ok(ScheduledEgressDetails { egress_id, egress_amount, fee_withheld }) =>
					match egress_type {
						EgressType::Regular =>
							Self::deposit_event(Event::<T>::SwapEgressScheduled {
								swap_request_id,
								egress_id,
								asset,
								amount: egress_amount,
								egress_fee: (fee_withheld, asset),
							}),
						EgressType::Refund { refund_fee } =>
							Self::deposit_event(Event::<T>::RefundEgressScheduled {
								swap_request_id,
								egress_id,
								asset,
								amount: egress_amount,
								egress_fee: (fee_withheld, asset),
								refund_fee,
							}),
					},
				Err(err) => match egress_type {
					EgressType::Regular => {
						if is_ccm {
							log_or_panic!("CCM egress scheduling should never fail.");
						}
						Self::deposit_event(Event::<T>::SwapEgressIgnored {
							swap_request_id,
							asset,
							amount,
							reason: err.into(),
						});
					},
					EgressType::Refund { .. } =>
						Self::deposit_event(Event::<T>::RefundEgressIgnored {
							swap_request_id,
							asset,
							amount,
							reason: err.into(),
						}),
				},
			}
		}

		pub(super) fn take_refund_fee(
			total_input_amount: AssetAmount,
			input_asset: Asset,
			is_internal_swap: bool,
		) -> Result<FeeTaken, DispatchError> {
			// We use the network fee minimum as the refund fee
			let refund_fee_usdc = if is_internal_swap {
				InternalSwapNetworkFee::<T>::get().minimum
			} else {
				NetworkFee::<T>::get().minimum
			};
			if refund_fee_usdc.is_zero() || total_input_amount.is_zero() {
				return Ok(FeeTaken { remaining_amount: total_input_amount, fee: 0 });
			}

			let required_refund_fee_as_input_asset = Self::calculate_input_for_desired_output(
				input_asset,
				STABLE_ASSET,
				refund_fee_usdc,
				false, /* Without network fee */
			)
			.ok_or(DispatchError::Other("Invalid fee estimation"))?;

			let refund_fee =
				sp_std::cmp::min(required_refund_fee_as_input_asset, total_input_amount);
			let remaining_amount = total_input_amount.saturating_sub(refund_fee);

			if !refund_fee.is_zero() {
				Self::init_network_fee_swap_request(input_asset, refund_fee);
			}

			Ok(FeeTaken { remaining_amount, fee: refund_fee })
		}

		pub fn assemble_and_validate_broker_fees(
			broker_id: T::AccountId,
			broker_commission: BasisPoints,
			affiliate_fees: Affiliates<T::AccountId>,
		) -> Result<Beneficiaries<T::AccountId>, DispatchError> {
			let beneficiaries = [Beneficiary { account: broker_id, bps: broker_commission }]
				.into_iter()
				.chain(affiliate_fees.iter().cloned())
				.collect::<Vec<_>>()
				.try_into()
				.expect(
					"We are pushing affiliates + 1 which is exactly the maximum Beneficiaries size",
				);
			Pallet::<T>::validate_broker_fees(&beneficiaries)?;
			Ok(beneficiaries)
		}

		pub fn get_network_fee_for_swap(
			input_asset: Asset,
			output_asset: Asset,
			is_internal_swap: bool,
		) -> FeeRateAndMinimum {
			let (input_asset_fee, output_asset_fee, minimum) = if is_internal_swap {
				let default_fee = InternalSwapNetworkFee::<T>::get();
				(
					InternalSwapNetworkFeeForAsset::<T>::get(input_asset)
						.unwrap_or(default_fee.rate),
					InternalSwapNetworkFeeForAsset::<T>::get(output_asset)
						.unwrap_or(default_fee.rate),
					default_fee.minimum,
				)
			} else {
				let default_fee = NetworkFee::<T>::get();
				(
					NetworkFeeForAsset::<T>::get(input_asset).unwrap_or(default_fee.rate),
					NetworkFeeForAsset::<T>::get(output_asset).unwrap_or(default_fee.rate),
					default_fee.minimum,
				)
			};
			FeeRateAndMinimum { rate: input_asset_fee.max(output_asset_fee), minimum }
		}
	}

	impl<T: Config> SwapRequestHandler for Pallet<T> {
		type AccountId = T::AccountId;

		fn init_swap_request(
			input_asset: Asset,
			input_amount: AssetAmount,
			output_asset: Asset,
			request_type: SwapRequestType<Self::AccountId>,
			broker_fees: Beneficiaries<Self::AccountId>,
			refund_params: Option<ChannelRefundParametersCheckedInternal<Self::AccountId>>,
			dca_params: Option<DcaParameters>,
			origin: SwapOrigin<Self::AccountId>,
		) -> SwapRequestId {
			let request_id = SwapRequestIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			// Do not limit the maximum swap amount for network fee swaps.
			let net_amount = if matches!(
				request_type,
				SwapRequestType::NetworkFee | SwapRequestType::IngressEgressFee
			) {
				input_amount
			} else {
				let (swap_amount, confiscated_amount) =
					match MaximumSwapAmount::<T>::get(input_asset) {
						Some(max) =>
							(sp_std::cmp::min(input_amount, max), input_amount.saturating_sub(max)),
						None => (input_amount, Zero::zero()),
					};
				if !confiscated_amount.is_zero() {
					CollectedRejectedFunds::<T>::mutate(input_asset, |fund| {
						*fund = fund.saturating_add(confiscated_amount)
					});
					Self::deposit_event(Event::<T>::SwapAmountConfiscated {
						swap_request_id: request_id,
						asset: input_asset,
						total_amount: input_amount,
						confiscated_amount,
					});
				}
				swap_amount
			};

			// Restrict the number of chunks based on the minimum chunk size.
			let dca_params = dca_params.map(|mut dca_params| {
				let minimum_chunk_size = MinimumChunkSize::<T>::get(input_asset);
				if minimum_chunk_size > 0 {
					dca_params.number_of_chunks = core::cmp::min(
						max((input_amount / minimum_chunk_size) as u32, 1),
						dca_params.number_of_chunks,
					);
				}
				dca_params
			});

			Self::deposit_event(Event::<T>::SwapRequested {
				swap_request_id: request_id,
				input_asset,
				input_amount,
				output_asset,
				request_type: request_type.clone().into_encoded::<T::AddressConverter>(),
				origin: origin.clone(),
				broker_fees: broker_fees.clone(),
				refund_parameters: refund_params.clone(),
				dca_parameters: dca_params.clone(),
			});

			match request_type {
				SwapRequestType::NetworkFee => {
					Self::schedule_swap(
						input_asset,
						output_asset,
						net_amount,
						// No refund parameters for network fee swaps
						None,
						SwapType::NetworkFee,
						// No fees for network fee swaps
						Default::default(),
						request_id,
						SWAP_DELAY_BLOCKS.into(),
					);

					SwapRequests::<T>::insert(
						request_id,
						SwapRequest {
							id: request_id,
							input_asset,
							output_asset,
							state: SwapRequestState::NetworkFee,
						},
					);
				},
				SwapRequestType::IngressEgressFee => {
					// No minimum network fee for ingress/egress fee swaps
					let fees = vec![FeeType::NetworkFee(NetworkFeeTracker::new_without_minimum(
						Pallet::<T>::get_network_fee_for_swap(input_asset, output_asset, false),
					))];

					Self::schedule_swap(
						input_asset,
						output_asset,
						net_amount,
						// No refund parameters for ingress/egress fee swaps
						None,
						SwapType::IngressEgressFee,
						fees,
						request_id,
						SWAP_DELAY_BLOCKS.into(),
					);

					SwapRequests::<T>::insert(
						request_id,
						SwapRequest {
							id: request_id,
							input_asset,
							output_asset,
							state: SwapRequestState::IngressEgressFee,
						},
					);
				},
				SwapRequestType::Regular { output_action } => {
					let mut dca_state = DcaState::new(net_amount, dca_params.clone());
					let chunk_input_amount = dca_state.calculate_next_chunk().unwrap_or_default();

					// Choose correct network fee for the swap
					let mut fees = vec![FeeType::NetworkFee(NetworkFeeTracker::new(
						Pallet::<T>::get_network_fee_for_swap(
							input_asset,
							output_asset,
							matches!(output_action, SwapOutputAction::CreditOnChain { .. }),
						),
					))];

					// Add broker fees if any
					if !broker_fees.is_empty() {
						fees.push(FeeType::BrokerFee(broker_fees));
					}

					let swap_id = Self::schedule_swap(
						input_asset,
						output_asset,
						chunk_input_amount,
						refund_params.as_ref(),
						SwapType::Swap,
						fees.clone(),
						request_id,
						SWAP_DELAY_BLOCKS.into(),
					);

					dca_state.record_scheduled_chunk(swap_id, chunk_input_amount);

					if let Some(DcaParameters { chunk_interval, .. }) = dca_params {
						// This assumes that the swap delay is 2, so we will only even schedule max
						// of 2 chunks at a time.
						if chunk_interval == 1 {
							// Also schedule a second swap so we can have an chunk interval that is
							// smaller than the swap delay.
							let chunk_input_amount =
								dca_state.calculate_next_chunk().unwrap_or_default();
							if chunk_input_amount > 0 {
								let swap_id = Self::schedule_swap(
									input_asset,
									output_asset,
									chunk_input_amount,
									refund_params.as_ref(),
									SwapType::Swap,
									fees,
									request_id,
									SWAP_DELAY_BLOCKS.saturating_add(chunk_interval).into(),
								);
								dca_state.record_scheduled_chunk(swap_id, chunk_input_amount);
							}
						}
					}

					SwapRequests::<T>::insert(
						request_id,
						SwapRequest {
							id: request_id,
							input_asset,
							output_asset,
							state: SwapRequestState::UserSwap {
								output_action,
								refund_params,
								dca_state,
							},
						},
					);
				},
			};

			request_id
		}
	}

	impl<T: Config> AssetConverter for Pallet<T> {
		fn calculate_input_for_gas_output<C: Chain>(
			input_asset: C::ChainAsset,
			required_gas: C::ChainAmount,
		) -> C::ChainAmount {
			let input_asset_generic: Asset = input_asset.into();
			Self::calculate_input_for_desired_output(
				input_asset_generic,
				C::GAS_ASSET.into(),
				required_gas.into(),
				true,
			)
			.and_then(|amount| C::ChainAmount::try_from(amount).ok())
			.unwrap_or_else(|| {
				log::warn!("Unable to calculate input amount required for gas of {required_gas:?} for input asset ${input_asset:?}. Estimating the input amount based on a reference price.");
				C::input_asset_amount_using_reference_gas_asset_price(input_asset,required_gas)
			})
		}

		fn calculate_input_for_desired_output(
			input_asset: Asset,
			output_asset: Asset,
			desired_output_amount: AssetAmount,
			with_network_fee: bool,
		) -> Option<AssetAmount> {
			use frame_support::sp_runtime::helpers_128bit::multiply_by_rational_with_rounding;

			if desired_output_amount.is_zero() {
				return Some(Zero::zero())
			}

			if input_asset == output_asset {
				return Some(desired_output_amount)
			}

			let estimation_input = utilities::fee_estimation_basis(input_asset);

			let estimation_output = with_transaction_unchecked(|| {
				TransactionOutcome::Rollback(if with_network_fee {
					Self::swap_with_network_fee_for_gas(input_asset, output_asset, estimation_input)
						.map(|swap| swap.output)
				} else {
					T::SwappingApi::swap_single_leg(input_asset, output_asset, estimation_input)
				})
			})
			.ok()?;

			if estimation_output == 0 {
				None
			} else {
				let input_amount_to_convert = multiply_by_rational_with_rounding(
					desired_output_amount,
					estimation_input,
					estimation_output,
					sp_arithmetic::Rounding::Down,
				)
				.defensive_proof(
					"Unexpected overflow occurred during asset conversion. Please report this to Chainflip Labs."
				)?;
				if input_amount_to_convert.is_zero() {
					None
				} else {
					Some(input_amount_to_convert.unique_saturated_into())
				}
			}
		}
	}
}

impl<T: Config> cf_traits::FlipBurnInfo for Pallet<T> {
	fn take_flip_to_burn() -> AssetAmount {
		FlipToBurn::<T>::take()
	}
}

impl<T: Config> SwapParameterValidation for Pallet<T> {
	type AccountId = T::AccountId;

	fn get_swap_limits() -> cf_traits::SwapLimits {
		cf_traits::SwapLimits {
			max_swap_retry_duration_blocks: MaxSwapRetryDurationBlocks::<T>::get(),
			max_swap_request_duration_blocks: MaxSwapRequestDurationBlocks::<T>::get(),
		}
	}

	fn validate_refund_params(retry_duration: BlockNumber) -> Result<(), DispatchError> {
		let max_swap_retry_duration_blocks = MaxSwapRetryDurationBlocks::<T>::get();
		if retry_duration > max_swap_retry_duration_blocks {
			return Err(DispatchError::from(Error::<T>::RetryDurationTooHigh));
		}
		Ok(())
	}

	fn validate_dca_params(params: &cf_primitives::DcaParameters) -> Result<(), DispatchError> {
		let max_swap_request_duration_blocks = MaxSwapRequestDurationBlocks::<T>::get();

		if params.number_of_chunks != 1 {
			if params.number_of_chunks == 0 {
				return Err(DispatchError::from(Error::<T>::ZeroNumberOfChunksNotAllowed));
			}
			if params.chunk_interval == 0 {
				return Err(DispatchError::from(Error::<T>::ChunkIntervalTooLow));
			}
			if let Some(total_swap_request_duration) =
				params.number_of_chunks.saturating_sub(1).checked_mul(params.chunk_interval)
			{
				if total_swap_request_duration > max_swap_request_duration_blocks {
					return Err(DispatchError::from(Error::<T>::SwapRequestDurationTooLong));
				}
			} else {
				return Err(DispatchError::from(Error::<T>::InvalidDcaParameters));
			}
		}
		Ok(())
	}

	fn validate_broker_fees(
		broker_fees: &Beneficiaries<Self::AccountId>,
	) -> Result<(), DispatchError> {
		let total_bps = broker_fees
			.iter()
			.fold(0, |total, Beneficiary { bps, .. }| total.saturating_add(*bps));

		ensure!(total_bps <= 1000, Error::<T>::BrokerCommissionBpsTooHigh);

		Ok(())
	}

	fn get_minimum_vault_swap_fee_for_broker(broker_id: &Self::AccountId) -> BasisPoints {
		VaultSwapMinimumBrokerFee::<T>::get(broker_id)
	}
}

impl<T: Config> AffiliateRegistry for Pallet<T> {
	type AccountId = T::AccountId;

	fn get_account_id(
		broker_id: &Self::AccountId,
		affiliate_short_id: AffiliateShortId,
	) -> Option<Self::AccountId> {
		AffiliateIdMapping::<T>::get(broker_id, affiliate_short_id)
	}

	/// This function iterates over a storage map. Only for use in rpc methods.
	fn get_short_id(
		broker_id: &Self::AccountId,
		affiliate_id: &Self::AccountId,
	) -> Option<AffiliateShortId> {
		AffiliateAccountDetails::<T>::get(broker_id, affiliate_id).map(|details| details.short_id)
	}

	fn reverse_mapping(broker_id: &Self::AccountId) -> BTreeMap<Self::AccountId, AffiliateShortId> {
		AffiliateIdMapping::<T>::iter_prefix(broker_id)
			.map(|(short_id, account_id)| (account_id, short_id))
			.collect()
	}
}

pub(crate) mod utilities {
	use super::*;

	/// The amount of a non-gas asset to be used for transaction fee estimation.
	///
	/// This should be of a similar order of magnitude to expected fees to get an accurate result.
	///
	/// The value should be large enough to allow a good estimation of the fee, but small enough
	/// to not exhaust the pool liquidity.
	pub(crate) fn fee_estimation_basis(asset: Asset) -> u128 {
		use cf_primitives::FLIPPERINOS_PER_FLIP;

		const ETH_DECIMALS: u32 = 18;
		const DOT_DECIMALS: u32 = 10;
		const BTC_DECIMALS: u32 = 8;
		const SOL_DECIMALS: u32 = 9;

		/// ~20 Dollars.
		const FLIP_ESTIMATION_CAP: u128 = 10 * FLIPPERINOS_PER_FLIP;
		const USD_ESTIMATION_CAP: u128 = 20_000_000;
		const ETH_ESTIMATION_CAP: u128 = 8 * 10u128.pow(ETH_DECIMALS - 3);
		const DOT_ESTIMATION_CAP: u128 = 4 * 10u128.pow(DOT_DECIMALS);
		const BTC_ESTIMATION_CAP: u128 = 2 * 10u128.pow(BTC_DECIMALS - 4);
		const SOL_ESTIMATION_CAP: u128 = 14 * 10u128.pow(SOL_DECIMALS - 2);

		match asset {
			Asset::Flip => FLIP_ESTIMATION_CAP,
			Asset::Usdc => USD_ESTIMATION_CAP,
			Asset::Usdt => USD_ESTIMATION_CAP,
			Asset::ArbUsdc => USD_ESTIMATION_CAP,
			Asset::SolUsdc => USD_ESTIMATION_CAP,
			Asset::Eth => ETH_ESTIMATION_CAP,
			Asset::Dot => DOT_ESTIMATION_CAP,
			Asset::ArbEth => ETH_ESTIMATION_CAP,
			Asset::Btc => BTC_ESTIMATION_CAP,
			Asset::Sol => SOL_ESTIMATION_CAP,
			Asset::HubDot => DOT_ESTIMATION_CAP,
			Asset::HubUsdc => USD_ESTIMATION_CAP,
			Asset::HubUsdt => USD_ESTIMATION_CAP,
		}
	}

	pub(super) fn split_off_highest_impact_swap<T: Config>(
		swaps: &mut Vec<Swap<T>>,
		failed_swap_group: &[SwapState<T>],
		direction: SwapLeg,
	) -> Option<Swap<T>> {
		// Check invariants:
		if failed_swap_group.is_empty() {
			log_or_panic!(
				"Invariant violation: there should be at least one swap in a failed group"
			)
		}
		for failed_swap in failed_swap_group {
			if !swaps.iter().any(|swap| swap.swap_id == failed_swap.swap_id()) {
				log_or_panic!(
					"Invariant violation: failed group must be a subset of all executed swaps"
				)
			}
		}
		// Find a swap id that we want to remove (in theory there should always be
		// one from the failing asset/direction, but if we don't for some reason, the fallback is to
		// remove nothing, which would abort the entire batch):
		let maybe_swap_id_to_remove = failed_swap_group
			.iter()
			// If the direction is TO_STABLE, swap amount is in the input amount of
			// *the same* asset (swaps from different assets are executed separately).
			// If the direction is FROM_STABLE, swap amount is the amount in USDC.
			// Either way, the amounts are in the same asset, so we can compare them directly:
			.max_by_key(|swap| swap.swap_amount(direction).unwrap_or_default())
			.map(|swap| swap.swap_id());

		maybe_swap_id_to_remove.and_then(|swap_id_to_remove| {
			swaps.extract_if(.., |swap| swap.swap_id == swap_id_to_remove).next()
		})
	}
}
