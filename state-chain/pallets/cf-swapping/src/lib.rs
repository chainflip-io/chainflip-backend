#![cfg_attr(not(feature = "std"), no_std)]

use cf_amm::common::Side;
use cf_chains::{
	address::{AddressConverter, ForeignChainAddress},
	CcmChannelMetadata, CcmDepositMetadata, ChannelRefundParameters, SwapOrigin,
	SwapRefundParameters,
};
use cf_primitives::{
	AccountRole, Affiliates, Asset, AssetAmount, Beneficiaries, Beneficiary, ChannelId,
	ForeignChain, SwapId, SwapLeg, SwapRequestId, TransactionHash, BASIS_POINTS_PER_MILLION,
	STABLE_ASSET,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, DepositApi, ExecutionCondition, IngressEgressFeeApi, NetworkFeeTaken,
	SwapRequestHandler, SwapRequestType, SwapRequestTypeEncoded, SwapType, SwappingApi,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{Get, Saturating},
		DispatchError, Permill, TransactionOutcome,
	},
	storage::with_transaction_unchecked,
	traits::Defensive,
	transactional,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_arithmetic::{
	helpers_128bit::multiply_by_rational_with_rounding,
	traits::{UniqueSaturatedInto, Zero},
	Rounding,
};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;

pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(6);

pub const SWAP_DELAY_BLOCKS: u32 = 2;

pub struct DefaultSwapRetryDelay<T> {
	_phantom: PhantomData<T>,
}
impl<T: Config> Get<BlockNumberFor<T>> for DefaultSwapRetryDelay<T> {
	fn get() -> BlockNumberFor<T> {
		BlockNumberFor::<T>::from(5u32)
	}
}

struct SwapState {
	swap: Swap,
	network_fee_taken: Option<AssetAmount>,
	stable_amount: Option<AssetAmount>,
	final_output: Option<AssetAmount>,
}

impl SwapState {
	fn new(swap: Swap) -> Self {
		Self {
			stable_amount: if swap.from == STABLE_ASSET { Some(swap.input_amount) } else { None },
			final_output: if swap.from == swap.to { Some(swap.input_amount) } else { None },
			network_fee_taken: None,
			swap,
		}
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

	fn refund_params(&self) -> Option<&SwapRefundParameters> {
		self.swap.refund_params.as_ref()
	}

	fn swap_type(&self) -> &SwapType {
		&self.swap.swap_type
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

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct Swap {
	swap_id: SwapId,
	swap_request_id: SwapRequestId,
	pub from: Asset,
	pub to: Asset,
	input_amount: AssetAmount,
	refund_params: Option<SwapRefundParameters>,
	swap_type: SwapType,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct SwapLegInfo {
	pub swap_id: SwapId,
	pub base_asset: Asset,
	pub quote_asset: Asset,
	pub side: Side,
	pub amount: AssetAmount,
	pub source_asset: Option<Asset>,
	pub source_amount: Option<AssetAmount>,
}

impl Swap {
	fn new(
		swap_id: SwapId,
		swap_request_id: SwapId,
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
		refund_params: Option<SwapRefundParameters>,
		swap_type: SwapType,
	) -> Self {
		Self { swap_id, swap_request_id, from, to, input_amount, swap_type, refund_params }
	}
}

pub mod ccm {

	use super::*;

	pub fn principal_and_gas_amounts(
		deposit_amount: AssetAmount,
		channel_metadata: &CcmChannelMetadata,
		source_asset: Asset,
		destination_asset: Asset,
	) -> Result<CcmSwapAmounts, CcmFailReason> {
		let gas_budget = channel_metadata.gas_budget;
		let principal_swap_amount = deposit_amount.saturating_sub(gas_budget);

		let destination_chain: ForeignChain = destination_asset.into();
		if !destination_chain.ccm_support() {
			return Err(CcmFailReason::UnsupportedForTargetChain)
		} else if deposit_amount < gas_budget {
			return Err(CcmFailReason::InsufficientDepositAmount)
		}

		// Return gas asset only if it is different from the input asset (and thus requires a swap)
		let output_gas_asset = destination_chain.gas_asset();

		Ok(CcmSwapAmounts {
			principal_swap_amount,
			gas_budget,
			other_gas_asset: if source_asset == output_gas_asset || gas_budget.is_zero() {
				None
			} else {
				Some(output_gas_asset)
			},
		})
	}
}

enum BatchExecutionError {
	SwapLegFailed { asset: Asset, direction: SwapLeg, amount: AssetAmount },
	PriceLimitHit { successful_swaps: Vec<Swap>, failed_swaps: Vec<Swap> },
	DispatchError { error: DispatchError },
}

/// This impl is never used. This is purely used to satisfy trait requirement
impl From<DispatchError> for BatchExecutionError {
	fn from(error: DispatchError) -> Self {
		Self::DispatchError { error }
	}
}

pub struct CcmSwapAmounts {
	pub principal_swap_amount: AssetAmount,
	pub gas_budget: AssetAmount,
	// if the gas asset is different to the input asset, it will require a swap
	pub other_gas_asset: Option<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
enum CcmSwapState {
	PrincipalSwapScheduled {
		principal_swap_id: SwapId,
		gas_budget: AssetAmount,
		other_gas_asset: Option<Asset>,
	},
	GasSwapScheduled {
		gas_swap_id: SwapId,
		principal_output_amount: AssetAmount,
	},
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
enum SwapRequestState {
	Ccm {
		state: CcmSwapState,
		ccm_deposit_metadata: CcmDepositMetadata,
		output_address: ForeignChainAddress,
	},
	Regular {
		output_address: ForeignChainAddress,
	},
	NetworkFee,
	IngressEgressFee,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]

struct SwapRequest {
	id: SwapRequestId,
	input_asset: Asset,
	output_asset: Asset,
	refund_params: Option<ChannelRefundParameters>,
	state: SwapRequestState,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum CcmFailReason {
	UnsupportedForTargetChain,
	InsufficientDepositAmount,
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
}

impl_pallet_safe_mode! {
	PalletSafeMode; swaps_enabled, withdrawals_enabled, broker_registration_enabled,
}

#[frame_support::pallet]
pub mod pallet {
	use core::cmp::max;

	use cf_amm::common::{output_amount_ceil, sqrt_price_to_price, SqrtPriceQ64F96};
	use cf_chains::{address::EncodedAddress, AnyChain, Chain};
	use cf_primitives::{
		Asset, AssetAmount, BasisPoints, EgressId, SwapId, SwapOutput, SwapRequestId,
	};
	use cf_traits::{AccountRoleRegistry, Chainflip, EgressApi, ScheduledEgressDetails};
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

		#[pallet::constant]
		type NetworkFee: Get<Permill>;
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::storage]
	pub(super) type SwapRequests<T: Config> =
		StorageMap<_, Twox64Concat, SwapRequestId, SwapRequest>;

	/// Scheduled Swaps
	#[pallet::storage]
	#[pallet::getter(fn swap_queue)]
	pub type SwapQueue<T: Config> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<Swap>, ValueQuery>;

	/// SwapId Counter
	#[pallet::storage]
	pub type SwapIdCounter<T: Config> = StorageValue<_, SwapId, ValueQuery>;

	#[pallet::storage]
	pub type SwapRequestIdCounter<T: Config> = StorageValue<_, SwapRequestId, ValueQuery>;

	/// Earned Fees by Brokers
	#[pallet::storage]
	#[pallet::getter(fn earned_broker_fees)]
	pub(crate) type EarnedBrokerFees<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, AssetAmount, ValueQuery>;

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

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// New swap has been requested
		SwapRequested {
			swap_request_id: SwapRequestId,
			input_asset: Asset,
			input_amount: AssetAmount, // includes broker fee
			output_asset: Asset,
			broker_fee: AssetAmount,
			origin: SwapOrigin,
			request_type: SwapRequestTypeEncoded,
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
			broker_commission_rate: BasisPoints,
			channel_metadata: Option<CcmChannelMetadata>,
			source_chain_expiry_block: <AnyChain as Chain>::ChainBlockNumber,
			boost_fee: BasisPoints,
			channel_opening_fee: T::Amount,
			affiliate_fees: Affiliates<T::AccountId>,
			refund_parameters: Option<ChannelRefundParameters>,
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
			intermediate_amount: Option<AssetAmount>,
			output_amount: AssetAmount,
		},
		/// A swap egress has been scheduled.
		SwapEgressScheduled {
			swap_request_id: SwapRequestId,
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			fee: AssetAmount,
		},
		RefundEgressScheduled {
			swap_request_id: SwapRequestId,
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			egress_fee: AssetAmount,
		},
		/// A broker fee withdrawal has been requested.
		WithdrawalRequested {
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
		CcmFailed {
			reason: CcmFailReason,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
			origin: SwapOrigin,
		},
		MaximumSwapAmountSet {
			asset: Asset,
			amount: Option<AssetAmount>,
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
		BuyIntervalSet {
			buy_interval: BlockNumberFor<T>,
		},
		SwapRetryDelaySet {
			swap_retry_delay: BlockNumberFor<T>,
		},
		// TODO: add SwapFailed?
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
		/// The deposited amount is insufficient to pay for the gas budget.
		CcmInsufficientDepositAmount,
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
		/// The provided list of broker contains an account which is not registered as Broker
		AffiliateAccountIsNotABroker,
		/// Setting the buy interval to zero is not allowed.
		ZeroBuyIntervalNotAllowed,
		/// Setting the swap retry delay to zero is not allowed.
		ZeroSwapRetryDelayNotAllowed,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub flip_buy_interval: BlockNumberFor<T>,
		pub swap_retry_delay: BlockNumberFor<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			FlipBuyInterval::<T>::set(self.flip_buy_interval);
			SwapRetryDelay::<T>::set(self.swap_retry_delay);
		}
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				flip_buy_interval: BlockNumberFor::<T>::zero(),
				swap_retry_delay: DefaultSwapRetryDelay::<T>::get(),
			}
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
					if let Err(e) = CollectedNetworkFee::<T>::try_mutate(|collected_fee| {
						if Self::init_swap_request(
							Asset::Usdc,
							*collected_fee,
							Asset::Flip,
							SwapRequestType::NetworkFee,
							Default::default(),
							None,
							SwapOrigin::Internal,
						)
						.is_err()
						{
							log_or_panic!("Network fee swap should never fail");
						}

						collected_fee.set_zero();
						Ok::<_, DispatchError>(())
					}) {
						log::warn!("Unable to swap Network Fee to Flip: {e:?}");
					}
				}
			}
			weight_used
		}

		/// Execute all swaps in the SwapQueue
		fn on_finalize(current_block: BlockNumberFor<T>) {
			let mut swaps_to_execute = SwapQueue::<T>::take(current_block);
			let retry_block = current_block + max(SwapRetryDelay::<T>::get(), 1u32.into());

			if !T::SafeMode::get().swaps_enabled {
				// Since we won't be executing swaps at this block, we need to reschedule them:
				for swap in swaps_to_execute {
					Self::deposit_event(Event::<T>::SwapRescheduled {
						swap_id: swap.swap_id,
						execute_at: retry_block,
					});

					SwapQueue::<T>::append(retry_block, swap);
				}

				return
			}

			loop {
				if swaps_to_execute.is_empty() {
					return
				}

				match Self::execute_batch(swaps_to_execute.clone()) {
					Ok(successful_swaps) => {
						Self::process_swap_outcomes(&successful_swaps);
						// Nothing to do here, all swaps are processed for block
						return
					},
					Err(err) => {
						// Depending on the error, split the swaps into "satisfactory"
						// (have a chance of succeeding if retried immediately), and "failed"
						// (unlikely to succeed now and should be retried later or refunded).
						let (satisfactory_swaps, failed_swaps) = {
							match err {
								BatchExecutionError::PriceLimitHit {
									successful_swaps,
									failed_swaps,
								} => (successful_swaps, failed_swaps),
								BatchExecutionError::SwapLegFailed { asset, direction, amount } => {
									Self::deposit_event(Event::<T>::BatchSwapFailed {
										asset,
										direction,
										amount,
									});
									(vec![], swaps_to_execute)
								},
								BatchExecutionError::DispatchError { error } => {
									// This should only happen when the transaction nested too deep,
									// which should not happen in practice (max nesting is 255):
									log_or_panic!(
										"Failed to execute swap batch at block {:?}: {:?}",
										current_block,
										error
									);
									(vec![], swaps_to_execute)
								},
							}
						};

						for swap in failed_swaps {
							match swap.refund_params {
								Some(params)
									if BlockNumberFor::<T>::from(params.refund_block) <
										retry_block =>
								{
									// Reached refund block, schedule refund:
									match T::EgressHandler::schedule_egress(
										swap.from,
										swap.input_amount,
										params.refund_address,
										None,
									) {
										Ok(ScheduledEgressDetails {
											egress_id,
											egress_amount,
											fee_withheld,
										}) => {
											Self::deposit_event(
												Event::<T>::RefundEgressScheduled {
													swap_request_id: swap.swap_request_id,
													egress_id,
													asset: swap.from,
													amount: egress_amount,
													egress_fee: fee_withheld,
												},
											);
										},
										Err(err) => {
											Self::deposit_event(Event::<T>::RefundEgressIgnored {
												swap_request_id: swap.swap_request_id,
												asset: swap.from,
												amount: swap.input_amount,
												reason: err.into(),
											});
										},
									}
								},
								_ => {
									// Either refund parameters not set, or refund block not
									// reached:

									Self::deposit_event(Event::<T>::SwapRescheduled {
										swap_id: swap.swap_id,
										execute_at: retry_block,
									});

									SwapQueue::<T>::append(retry_block, swap);
								},
							}
						}

						if !satisfactory_swaps.is_empty() {
							swaps_to_execute = satisfactory_swaps;
						} else {
							return
						}
					},
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Request a swap deposit address.
		///
		/// ## Events
		///
		/// - [SwapDepositAddressReady](Event::SwapDepositAddressReady)
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::request_swap_deposit_address())]
		pub fn request_swap_deposit_address(
			origin: OriginFor<T>,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			channel_metadata: Option<CcmChannelMetadata>,
			boost_fee: BasisPoints,
		) -> DispatchResult {
			Self::request_swap_deposit_address_with_affiliates(
				origin,
				source_asset,
				destination_asset,
				destination_address,
				broker_commission,
				channel_metadata,
				boost_fee,
				Default::default(),
				// This extrinsic is for backwards compatibility and does not support FoK
				None,
			)
		}

		/// Brokers can withdraw their collected fees.
		///
		/// ## Events
		///
		/// - [WithdrawalRequested](Event::WithdrawalRequested)
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
				Self::validate_destination_address(&destination_address, asset)?;

			let earned_fees = EarnedBrokerFees::<T>::take(account_id, asset);
			ensure!(earned_fees != 0, Error::<T>::NoFundsAvailable);

			let ScheduledEgressDetails { egress_id, egress_amount, fee_withheld } =
				T::EgressHandler::schedule_egress(
					asset,
					earned_fees,
					destination_address_internal,
					None,
				)
				.map_err(Into::into)?;

			Self::deposit_event(Event::<T>::WithdrawalRequested {
				egress_amount,
				egress_asset: asset,
				egress_fee: fee_withheld,
				destination_address,
				egress_id,
			});

			Ok(())
		}

		/// Allow Witnessers to submit a Swap request on the behalf of someone else.
		/// Requires Witnesser origin.
		///
		/// ## Events
		///
		/// - [SwapScheduled](Event::SwapScheduled)
		/// - [SwapAmountTooLow](Event::SwapAmountTooLow)
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::schedule_swap_from_contract())]
		pub fn schedule_swap_from_contract(
			origin: OriginFor<T>,
			from: Asset,
			to: Asset,
			deposit_amount: AssetAmount,
			destination_address: EncodedAddress,
			tx_hash: TransactionHash,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, to)?;

			if Self::init_swap_request(
				from,
				deposit_amount,
				to,
				SwapRequestType::Regular { output_address: destination_address_internal.clone() },
				Default::default(),
				// NOTE: FoK not yet supported for swaps from the contract
				None,
				SwapOrigin::Vault { tx_hash },
			)
			.is_err()
			{
				log_or_panic!("Regular swap request should never fail");
			}

			Ok(())
		}

		/// Process the deposit of a CCM swap.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::ccm_deposit())]
		pub fn ccm_deposit(
			origin: OriginFor<T>,
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
			tx_hash: TransactionHash,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, destination_asset)?;

			Self::init_swap_request(
				source_asset,
				deposit_amount,
				destination_asset,
				SwapRequestType::Ccm {
					ccm_deposit_metadata: deposit_metadata,
					output_address: destination_address_internal.clone(),
				},
				Default::default(),
				// NOTE: FoK not yet supported for swaps from the contract
				None,
				SwapOrigin::Vault { tx_hash },
			)?;

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
			updates: BoundedVec<PalletConfigUpdate<T>, ConstU32<10>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::MaximumSwapAmount { asset, amount } => {
						MaximumSwapAmount::<T>::set(asset, amount);
						Self::deposit_event(Event::<T>::MaximumSwapAmountSet { asset, amount });
					},
					PalletConfigUpdate::SwapRetryDelay { delay } => {
						ensure!(
							delay != BlockNumberFor::<T>::zero(),
							Error::<T>::ZeroSwapRetryDelayNotAllowed
						);
						SwapRetryDelay::<T>::set(delay);
						Self::deposit_event(Event::<T>::SwapRetryDelaySet {
							swap_retry_delay: delay,
						});
					},
					PalletConfigUpdate::FlipBuyInterval { interval } => {
						ensure!(
							interval != BlockNumberFor::<T>::zero(),
							Error::<T>::ZeroBuyIntervalNotAllowed
						);
						FlipBuyInterval::<T>::set(interval);
						Self::deposit_event(Event::<T>::BuyIntervalSet { buy_interval: interval });
					},
				}
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
				EarnedBrokerFees::<T>::iter_prefix(&account_id)
					.all(|(_asset, balance)| balance.is_zero()),
				Error::<T>::EarnedFeesNotWithdrawn,
			);
			let _ = EarnedBrokerFees::<T>::clear_prefix(&account_id, u32::MAX, None);

			T::AccountRoleRegistry::deregister_as_broker(&account_id)?;

			Ok(())
		}

		/// Request a swap deposit address.
		///
		/// ## Events
		///
		/// - [SwapDepositAddressReady](Event::SwapDepositAddressReady)
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::request_swap_deposit_address_with_affiliates())]
		pub fn request_swap_deposit_address_with_affiliates(
			origin: OriginFor<T>,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			channel_metadata: Option<CcmChannelMetadata>,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<T::AccountId>,
			refund_parameters: Option<ChannelRefundParameters>,
		) -> DispatchResult {
			let broker = T::AccountRoleRegistry::ensure_broker(origin)?;
			let (beneficiaries, total_bps) = {
				let mut beneficiaries = Beneficiaries::new();
				if broker_commission > 0 {
					beneficiaries
						.try_push(Beneficiary { account: broker.clone(), bps: broker_commission })
						.expect("First element, impossible to exceed the maximum size");
				}
				for affiliate in &affiliate_fees {
					ensure!(
						T::AccountRoleRegistry::has_account_role(
							&affiliate.account,
							AccountRole::Broker
						),
						Error::<T>::AffiliateAccountIsNotABroker
					);
					if affiliate.bps > 0 {
						beneficiaries
							.try_push(affiliate.clone())
							.expect("Cannot exceed MAX_BENEFICIARY size which is MAX_AFFILIATE + 1 (main broker)");
					}
				}
				let total_bps = beneficiaries
					.iter()
					.fold(0, |total, Beneficiary { bps, .. }| total.saturating_add(*bps));
				(beneficiaries, total_bps)
			};

			ensure!(total_bps <= 1000, Error::<T>::BrokerCommissionBpsTooHigh);

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, destination_asset)?;

			if channel_metadata.is_some() {
				let destination_chain: ForeignChain = destination_asset.into();
				ensure!(destination_chain.ccm_support(), Error::<T>::CcmUnsupportedForTargetChain);
			}

			let (channel_id, deposit_address, expiry_height, channel_opening_fee) =
				T::DepositHandler::request_swap_deposit_address(
					source_asset,
					destination_asset,
					destination_address_internal,
					beneficiaries.clone(),
					broker,
					channel_metadata.clone(),
					boost_fee,
					refund_parameters.clone(),
				)?;

			Self::deposit_event(Event::<T>::SwapDepositAddressReady {
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				destination_address,
				source_asset,
				destination_asset,
				channel_id,
				broker_commission_rate: broker_commission,
				channel_metadata,
				source_chain_expiry_block: expiry_height,
				boost_fee,
				channel_opening_fee,
				affiliate_fees,
				refund_parameters,
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		#[allow(clippy::result_unit_err)]
		pub fn get_scheduled_swap_legs(
			swaps: Vec<Swap>,
			base_asset: Asset,
			pool_sell_price: Option<SqrtPriceQ64F96>,
		) -> Vec<SwapLegInfo> {
			let mut swaps: Vec<_> = swaps.into_iter().map(SwapState::new).collect();

			// Can ignore the result here because we use pool price fallback below
			let _res = Self::swap_into_stable_taking_network_fee(&mut swaps);

			swaps
				.into_iter()
				.filter_map(|swap| {
					if swap.input_asset() == base_asset {
						Some(SwapLegInfo {
							swap_id: swap.swap_id(),
							base_asset,
							// All swaps from `base_asset` have to go through the stable asset:
							quote_asset: STABLE_ASSET,
							side: Side::Sell,
							amount: swap.input_amount(),
							source_asset: None,
							source_amount: None,
						})
					} else if swap.output_asset() == base_asset {
						// In case the swap is "simulated", the amount is just an estimate,
						// so we additionally include `source_asset` and `source_amount`:
						let (source_asset, source_amount) = if swap.input_asset() != STABLE_ASSET {
							(Some(swap.input_asset()), Some(swap.input_amount()))
						} else {
							(None, None)
						};

						let amount = swap.stable_amount.or_else(|| {
							// If the swap into stable asset failed, fallback to estimating the
							// amount via pool price.
							Some(
								output_amount_ceil(
									cf_amm::common::Amount::from(swap.input_amount()),
									sqrt_price_to_price(pool_sell_price?),
								)
								.saturated_into(),
							)
						})?;

						Some(SwapLegInfo {
							swap_id: swap.swap_id(),
							base_asset,
							// All swaps to `base_asset` have to go through the stable asset:
							quote_asset: STABLE_ASSET,
							side: Side::Buy,
							amount,
							source_asset,
							source_amount,
						})
					} else {
						None
					}
				})
				.collect()
		}

		fn swap_into_stable_taking_network_fee(
			swaps: &mut [SwapState],
		) -> Result<(), BatchExecutionError> {
			Self::do_group_and_swap(swaps, SwapLeg::ToStable)?;

			// Take NetworkFee for all swaps
			for swap in swaps.iter_mut() {
				debug_assert!(
					swap.stable_amount.is_some(),
					"All swaps should have Stable amount set here"
				);

				// Copy some data to avoid holding references:
				let swap_type = *swap.swap_type();
				let stable_amount = swap.stable_amount.get_or_insert_with(Default::default);

				let stable_amount_copy = *stable_amount;

				let NetworkFeeTaken { remaining_amount, network_fee } =
					if matches!(swap_type, SwapType::NetworkFee) {
						// Don't take network fee for network fee swaps
						NetworkFeeTaken { remaining_amount: stable_amount_copy, network_fee: 0 }
					} else {
						Self::take_network_fee(stable_amount_copy)
					};

				*stable_amount = remaining_amount;
				swap.network_fee_taken = Some(network_fee);

				if swap.output_asset() == STABLE_ASSET {
					swap.final_output = Some(remaining_amount);
				}
			}

			Ok(())
		}

		#[transactional]
		fn execute_batch(swaps: Vec<Swap>) -> Result<Vec<SwapState>, BatchExecutionError> {
			let mut swaps: Vec<_> = swaps.into_iter().map(SwapState::new).collect();

			// Swap into Stable asset first, then take network fees:
			Self::swap_into_stable_taking_network_fee(&mut swaps)?;

			// Swap from Stable asset, and complete the swap logic.
			Self::do_group_and_swap(&mut swaps, SwapLeg::FromStable)?;

			// Swaps executed without triggering price impact protection, but we still need to
			// check that none of the swaps violated their minimum output requirements:
			let (non_violating, violating): (Vec<_>, Vec<_>) =
				swaps.into_iter().partition(|swap| {
					let final_output = swap.final_output.unwrap();
					swap.refund_params()
						.as_ref()
						.map_or(true, |params| final_output >= params.min_output)
				});

			if violating.is_empty() {
				Ok(non_violating)
			} else {
				Err(BatchExecutionError::PriceLimitHit {
					successful_swaps: non_violating.into_iter().map(|ctx| ctx.swap).collect(),
					failed_swaps: violating.into_iter().map(|ctx| ctx.swap).collect(),
				})
			}
		}

		fn process_ccm_outcome(
			swap_request_id: SwapRequestId,
			output_asset: Asset,
			output_address: ForeignChainAddress,
			principal_amount: AssetAmount,
			gas_amount: AssetAmount,
			ccm_deposit_metadata: CcmDepositMetadata,
		) {
			Self::deposit_event(Event::<T>::SwapRequestCompleted { swap_request_id });
			// Schedule the given ccm to be egressed and deposit an event.
			if let Ok(ScheduledEgressDetails { egress_id, egress_amount, fee_withheld }) =
				T::EgressHandler::schedule_egress(
					output_asset,
					principal_amount,
					output_address,
					Some((ccm_deposit_metadata, gas_amount)),
				) {
				Self::deposit_event(Event::<T>::SwapEgressScheduled {
					swap_request_id,
					egress_id,
					asset: output_asset,
					amount: egress_amount,
					fee: fee_withheld,
				});
			} else {
				log_or_panic!("CCM egress scheduling should never fail.");
			}
		}

		fn schedule_ccm_gas_swap(
			request_id: SwapRequestId,
			input_asset: Asset,
			gas_asset: Asset,
			gas_budget: AssetAmount,
			principal_output_amount: AssetAmount,
		) -> CcmSwapState {
			let swap_id = Self::schedule_swap(
				input_asset,
				gas_asset,
				gas_budget,
				None, // FoK does not apply to gas swaps
				SwapType::CcmGas,
				request_id,
			);

			CcmSwapState::GasSwapScheduled { gas_swap_id: swap_id, principal_output_amount }
		}

		fn process_swap_outcomes(swaps: &[SwapState]) {
			for swap in swaps {
				// To be consistent with `swap_output` and `intermediate_amount` (which do
				// not include the network fee), we report input amount without the network fee
				// for swaps from STABLE_ASSET:
				let swap_input = if swap.input_asset() == STABLE_ASSET {
					swap.stable_amount.unwrap_or_else(|| {
						log_or_panic!("stable amount must be set for swaps from STABLE_ASSET");
						swap.input_amount()
					})
				} else {
					swap.input_amount()
				};

				let swap_request_id = swap.swap.swap_request_id;

				let Some(mut request) = SwapRequests::<T>::take(swap_request_id) else {
					log_or_panic!("swap request not found");
					continue;
				};

				let Some(output_amount) = swap.final_output else {
					log_or_panic!("Swap is not completed yet!");
					continue;
				};

				Self::deposit_event(Event::<T>::SwapExecuted {
					swap_request_id,
					swap_id: swap.swap_id(),
					input_amount: swap_input,
					input_asset: swap.input_asset(),
					network_fee: swap.network_fee_taken.unwrap_or_default(),
					output_asset: swap.output_asset(),
					output_amount,
					intermediate_amount: swap.intermediate_amount(),
				});

				match &mut request.state {
					SwapRequestState::Ccm { state, ccm_deposit_metadata, output_address } =>
						match state {
							CcmSwapState::PrincipalSwapScheduled {
								principal_swap_id,
								gas_budget,
								other_gas_asset,
							} => {
								debug_assert!(swap.swap_id() == *principal_swap_id);

								if let Some(other_gas_asset) = other_gas_asset {
									*state = Self::schedule_ccm_gas_swap(
										swap_request_id,
										request.input_asset,
										*other_gas_asset,
										*gas_budget,
										output_amount,
									);

									// We are not done with the request, so put it back
									SwapRequests::<T>::insert(swap_request_id, request);
								} else {
									Self::process_ccm_outcome(
										swap_request_id,
										request.output_asset,
										output_address.clone(),
										output_amount,
										*gas_budget,
										ccm_deposit_metadata.clone(),
									);
								}
							},
							CcmSwapState::GasSwapScheduled {
								gas_swap_id,
								principal_output_amount,
							} => {
								debug_assert!(swap.swap_id() == *gas_swap_id);

								Self::process_ccm_outcome(
									swap_request_id,
									request.output_asset,
									output_address.clone(),
									*principal_output_amount,
									output_amount,
									ccm_deposit_metadata.clone(),
								);
							},
						},
					SwapRequestState::Regular { output_address } => {
						Self::deposit_event(Event::<T>::SwapRequestCompleted { swap_request_id });
						match T::EgressHandler::schedule_egress(
							swap.output_asset(),
							output_amount,
							output_address.clone(),
							None, /* ccm metadata */
						) {
							Ok(ScheduledEgressDetails {
								egress_id,
								egress_amount,
								fee_withheld,
							}) => {
								Self::deposit_event(Event::<T>::SwapEgressScheduled {
									swap_request_id: swap.swap.swap_request_id,
									egress_id,
									asset: swap.output_asset(),
									amount: egress_amount,
									fee: fee_withheld,
								});
							},
							Err(err) => {
								Self::deposit_event(Event::<T>::SwapEgressIgnored {
									swap_request_id,
									asset: swap.output_asset(),
									amount: output_amount,
									reason: err.into(),
								});
							},
						};
					},
					SwapRequestState::NetworkFee => {
						Self::deposit_event(Event::<T>::SwapRequestCompleted { swap_request_id });
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
					},
					SwapRequestState::IngressEgressFee => {
						Self::deposit_event(Event::<T>::SwapRequestCompleted { swap_request_id });

						if swap.output_asset() ==
							ForeignChain::from(swap.output_asset()).gas_asset()
						{
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
					},
				}
			}
		}

		// The address and the asset being sent or withdrawn must be compatible.
		fn validate_destination_address(
			destination_address: &EncodedAddress,
			destination_asset: Asset,
		) -> Result<ForeignChainAddress, DispatchError> {
			let destination_address_internal =
				T::AddressConverter::try_from_encoded_address(destination_address.clone())
					.map_err(|_| Error::<T>::InvalidDestinationAddress)?;
			ensure!(
				destination_address_internal.chain() == ForeignChain::from(destination_asset),
				Error::<T>::IncompatibleAssetAndAddress
			);
			Ok(destination_address_internal)
		}

		// Helper function that splits swaps of a given direction, group them by asset
		// and do the swaps of a given direction. Processed and unprocessed swaps are
		// returned.
		fn do_group_and_swap(
			swaps: &mut [SwapState],
			direction: SwapLeg,
		) -> Result<(), BatchExecutionError> {
			let swap_groups =
				swaps.iter_mut().fold(BTreeMap::new(), |mut groups: BTreeMap<_, Vec<_>>, swap| {
					if let Some(asset) = swap.swap_asset(direction) {
						groups.entry(asset).or_default().push(swap);
					}
					groups
				});

			for (asset, swaps) in swap_groups {
				Self::execute_group_of_swaps(swaps, asset, direction).map_err(|amount| {
					BatchExecutionError::SwapLegFailed { asset, direction, amount }
				})?;
			}
			Ok(())
		}

		/// Bundle the given swaps and do a single swap of a given direction. Updates the given
		/// swaps in-place. If batch swap failed, return the input amount.
		fn execute_group_of_swaps(
			swaps: Vec<&mut SwapState>,
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

			for swap in swaps {
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

				if swap_output == 0 && matches!(swap.swap_type(), SwapType::Swap) {
					// This is unlikely but theoretically possible if, for example, the initial swap
					// input is so small compared to the total bundle size that it rounds down to
					// zero when we do the division.
					log::warn!(
						"Swap {:?} in bundle {{ input: {bundle_input}, output: {bundle_output} }} resulted in swap output of zero.",
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
			refund_params: Option<SwapRefundParameters>,
			swap_type: SwapType,
			swap_request_id: SwapRequestId,
		) -> SwapId {
			let swap_id = SwapIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			let execute_at = frame_system::Pallet::<T>::block_number() + SWAP_DELAY_BLOCKS.into();

			SwapQueue::<T>::append(
				execute_at,
				Swap::new(
					swap_id,
					swap_request_id,
					input_asset,
					output_asset,
					input_amount,
					refund_params,
					swap_type,
				),
			);

			Self::deposit_event(Event::<T>::SwapScheduled {
				swap_request_id,
				swap_id,
				input_amount,
				swap_type,
				execute_at,
			});

			swap_id
		}

		#[transactional]
		pub fn swap_with_network_fee(
			from: Asset,
			to: Asset,
			input_amount: AssetAmount,
		) -> Result<SwapOutput, DispatchError> {
			Ok(match (from, to) {
				(_, STABLE_ASSET) => {
					let NetworkFeeTaken { remaining_amount: output, network_fee } =
						Self::take_network_fee(T::SwappingApi::swap_single_leg(
							from,
							to,
							input_amount,
						)?);

					SwapOutput { intermediary: None, output, network_fee }
				},
				(STABLE_ASSET, _) => {
					let NetworkFeeTaken { remaining_amount: input_amount, network_fee } =
						Self::take_network_fee(input_amount);

					SwapOutput {
						intermediary: None,
						output: T::SwappingApi::swap_single_leg(from, to, input_amount)?,
						network_fee,
					}
				},
				_ => {
					let NetworkFeeTaken { remaining_amount: intermediary, network_fee } =
						Self::take_network_fee(T::SwappingApi::swap_single_leg(
							from,
							STABLE_ASSET,
							input_amount,
						)?);

					SwapOutput {
						intermediary: Some(intermediary),
						output: T::SwappingApi::swap_single_leg(STABLE_ASSET, to, intermediary)?,
						network_fee,
					}
				},
			})
		}

		pub fn take_network_fee(input: AssetAmount) -> NetworkFeeTaken {
			if input.is_zero() {
				return NetworkFeeTaken { remaining_amount: 0, network_fee: 0 };
			}
			let (remaining, fee) = utilities::calculate_network_fee(T::NetworkFee::get(), input);
			CollectedNetworkFee::<T>::mutate(|total| {
				total.saturating_accrue(fee);
			});
			NetworkFeeTaken { remaining_amount: remaining, network_fee: fee }
		}
	}

	impl<T: Config> SwapRequestHandler for Pallet<T> {
		type AccountId = T::AccountId;

		fn init_swap_request(
			input_asset: Asset,
			input_amount: AssetAmount,
			output_asset: Asset,
			request_type: SwapRequestType,
			broker_fees: Beneficiaries<Self::AccountId>,
			refund_params: Option<ChannelRefundParameters>,
			origin: SwapOrigin,
		) -> Result<SwapRequestId, DispatchError> {
			let (net_amount, broker_fee) = {
				// Subtract broker fee:

				// Permill maxes out at 100% so this is safe.
				let fee: u128 = Permill::from_parts(
					broker_fees.iter().fold(0, |acc, entry| acc + entry.bps) as u32 *
						BASIS_POINTS_PER_MILLION,
				) * input_amount;

				assert!(fee <= input_amount, "Broker fee cannot be more than the amount");

				for Beneficiary { account, bps } in broker_fees {
					EarnedBrokerFees::<T>::mutate(&account, input_asset, |earned_fees| {
						earned_fees.saturating_accrue(
							Permill::from_parts(bps as u32 * BASIS_POINTS_PER_MILLION) *
								input_amount,
						)
					});
				}

				(input_amount.saturating_sub(fee), fee)
			};

			let request_id = SwapRequestIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			// Do not limit the maximum swap amount for network fee swaps.
			let net_amount = if matches!(
				request_type,
				SwapRequestType::NetworkFee | SwapRequestType::IngressEgressFee
			) {
				net_amount
			} else {
				let (swap_amount, confiscated_amount) =
					match MaximumSwapAmount::<T>::get(input_asset) {
						Some(max) =>
							(sp_std::cmp::min(net_amount, max), net_amount.saturating_sub(max)),
						None => (net_amount, Zero::zero()),
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

			// Now that we know input amount, we can calculate the minimum output amount:
			let swap_refund_params = refund_params.clone().map(|params| SwapRefundParameters {
				refund_block: {
					use sp_arithmetic::traits::UniqueSaturatedInto;
					// In practice block number always fits in u32:
					let current_block: u32 =
						frame_system::Pallet::<T>::block_number().unique_saturated_into();
					current_block.saturating_add(params.retry_duration)
				},
				refund_address: params.refund_address,
				min_output: u128::try_from(cf_amm::common::output_amount_ceil(
					input_amount.into(),
					params.min_price,
				))
				.unwrap_or(u128::MAX),
			});

			let request_type_for_event = match request_type {
				SwapRequestType::NetworkFee => {
					let _swap_id = Self::schedule_swap(
						input_asset,
						output_asset,
						net_amount,
						None,
						SwapType::NetworkFee,
						request_id,
					);

					SwapRequests::<T>::insert(
						request_id,
						SwapRequest {
							id: request_id,
							input_asset,
							output_asset,
							refund_params: None,
							state: SwapRequestState::NetworkFee,
						},
					);

					SwapRequestTypeEncoded::NetworkFee
				},
				SwapRequestType::IngressEgressFee => {
					let _swap_id = Self::schedule_swap(
						input_asset,
						output_asset,
						net_amount,
						None,
						SwapType::IngressEgressFee,
						request_id,
					);

					SwapRequests::<T>::insert(
						request_id,
						SwapRequest {
							id: request_id,
							input_asset,
							output_asset,
							refund_params: None,
							state: SwapRequestState::IngressEgressFee,
						},
					);

					SwapRequestTypeEncoded::IngressEgressFee
				},
				SwapRequestType::Regular { output_address } => {
					Self::schedule_swap(
						input_asset,
						output_asset,
						net_amount,
						swap_refund_params.clone(),
						SwapType::Swap,
						request_id,
					);

					SwapRequests::<T>::insert(
						request_id,
						SwapRequest {
							id: request_id,
							input_asset,
							output_asset,
							refund_params: refund_params.clone(),
							state: SwapRequestState::Regular {
								output_address: output_address.clone(),
							},
						},
					);

					SwapRequestTypeEncoded::Regular {
						output_address: T::AddressConverter::to_encoded_address(output_address),
					}
				},
				SwapRequestType::Ccm { ccm_deposit_metadata, output_address } => {
					let encoded_destination_address =
						T::AddressConverter::to_encoded_address(output_address.clone());
					// Caller should ensure that assets and addresses are compatible.
					debug_assert!(output_address.chain() == ForeignChain::from(output_asset));

					let CcmSwapAmounts { principal_swap_amount, gas_budget, other_gas_asset } =
						match ccm::principal_and_gas_amounts(
							net_amount,
							&ccm_deposit_metadata.channel_metadata,
							input_asset,
							output_asset,
						) {
							Ok(amounts) => amounts,
							Err(reason) => {
								// Confiscate the deposit and emit an event.
								CollectedRejectedFunds::<T>::mutate(input_asset, |fund| {
									*fund = fund.saturating_add(net_amount)
								});

								Self::deposit_event(Event::<T>::CcmFailed {
									reason,
									destination_address: encoded_destination_address,
									deposit_metadata: ccm_deposit_metadata.clone(),
									origin: origin.clone(),
								});
								return Err(DispatchError::Other("Invalid CCM parameters"));
							},
						};

					// See if princial swap is needed, schedule it first if so:
					if input_asset != output_asset && !principal_swap_amount.is_zero() {
						let swap_id = Self::schedule_swap(
							input_asset,
							output_asset,
							principal_swap_amount,
							swap_refund_params.clone(),
							SwapType::CcmPrincipal,
							request_id,
						);

						SwapRequests::<T>::insert(
							request_id,
							SwapRequest {
								id: request_id,
								input_asset,
								output_asset,
								refund_params: refund_params.clone(),
								state: SwapRequestState::Ccm {
									state: CcmSwapState::PrincipalSwapScheduled {
										principal_swap_id: swap_id,
										gas_budget,
										other_gas_asset,
									},
									ccm_deposit_metadata: ccm_deposit_metadata.clone(),
									output_address: output_address.clone(),
								},
							},
						);
					// See if gas swap is needed, schedule it immediately if so
					// (since there is no principal swap in this case):
					} else if let Some(other_gas_asset) = other_gas_asset {
						let ccm_state = Self::schedule_ccm_gas_swap(
							request_id,
							input_asset,
							other_gas_asset,
							gas_budget,
							principal_swap_amount,
						);

						let swap_request = SwapRequest {
							id: request_id,
							input_asset,
							output_asset,
							refund_params: None,
							state: SwapRequestState::Ccm {
								state: ccm_state,
								ccm_deposit_metadata: ccm_deposit_metadata.clone(),
								output_address,
							},
						};

						SwapRequests::<T>::insert(request_id, swap_request);
					} else {
						// No swaps are needed, process the CCM outcome immediately:
						Self::process_ccm_outcome(
							request_id,
							output_asset,
							output_address,
							principal_swap_amount,
							gas_budget,
							ccm_deposit_metadata.clone(),
						);
					}

					SwapRequestTypeEncoded::Ccm {
						output_address: encoded_destination_address,
						ccm_deposit_metadata,
					}
				},
			};

			Self::deposit_event(Event::<T>::SwapRequested {
				swap_request_id: request_id,
				input_asset,
				input_amount,
				output_asset,
				broker_fee,
				request_type: request_type_for_event,
				origin: origin.clone(),
			});

			Ok(request_id)
		}
	}

	impl<T: Config> cf_traits::AssetConverter for Pallet<T> {
		fn calculate_input_for_gas_output<C: Chain>(
			input_asset: C::ChainAsset,
			required_gas: C::ChainAmount,
		) -> Option<C::ChainAmount> {
			use frame_support::sp_runtime::helpers_128bit::multiply_by_rational_with_rounding;

			if required_gas.is_zero() {
				return Some(Zero::zero())
			}

			let output_asset = C::GAS_ASSET.into();
			let input_asset = input_asset.into();
			if input_asset == output_asset {
				return Some(required_gas)
			}

			let estimation_input = utilities::fee_estimation_basis(input_asset).defensive_proof(
				"Fee estimation cap not available. Please report this to Chainflip Labs.",
			)?;

			let estimation_output = with_transaction_unchecked(|| {
				TransactionOutcome::Rollback(
					Self::swap_with_network_fee(input_asset, output_asset, estimation_input).ok(),
				)
			})?
			.output;

			if estimation_output == 0 {
				None
			} else {
				let input_amount_to_convert = multiply_by_rational_with_rounding(
					required_gas.into(),
					estimation_input,
					estimation_output,
					sp_arithmetic::Rounding::Down,
				)
				.defensive_proof(
					"Unexpected overflow occurred during asset conversion. Please report this to Chainflip Labs."
				)?;

				Some(input_amount_to_convert.unique_saturated_into())
			}
		}
	}
}

impl<T: Config> cf_traits::FlipBurnInfo for Pallet<T> {
	fn take_flip_to_burn() -> AssetAmount {
		FlipToBurn::<T>::take()
	}
}

pub struct NoPendingSwaps<T: Config>(PhantomData<T>);

impl<T: Config> ExecutionCondition for NoPendingSwaps<T> {
	fn is_satisfied() -> bool {
		SwapQueue::<T>::iter().all(|(_, swaps)| swaps.is_empty())
	}
}
pub mod utilities {
	use super::*;

	pub fn calculate_network_fee(
		fee_percentage: Permill,
		input: AssetAmount,
	) -> (AssetAmount, AssetAmount) {
		let fee = fee_percentage * input;
		(input - fee, fee)
	}

	/// The amount of a non-gas asset to be used for transaction fee estimation.
	///
	/// This should be of a similar order of magnitude to expected fees to get an accurate result.
	///
	/// The value should be large enough to allow a good estimation of the fee, but small enough
	/// to not exhaust the pool liquidity.
	pub fn fee_estimation_basis(asset: Asset) -> Option<u128> {
		use cf_primitives::FLIPPERINOS_PER_FLIP;
		/// 20 Dollars.
		const USD_ESTIMATION_CAP: u128 = 20_000_000;

		match asset {
			Asset::Flip => Some(10 * FLIPPERINOS_PER_FLIP),
			Asset::Usdc => Some(USD_ESTIMATION_CAP),
			Asset::Usdt => Some(USD_ESTIMATION_CAP),
			Asset::ArbUsdc => Some(USD_ESTIMATION_CAP),
			_ => None,
		}
	}
}
