#![cfg_attr(not(feature = "std"), no_std)]

use cf_amm::common::Side;
use cf_chains::{
	address::{AddressConverter, ForeignChainAddress},
	CcmChannelMetadata, CcmDepositMetadata, ChannelRefundParameters, SwapOrigin,
	SwapRefundParameters,
};
use cf_primitives::{
	AccountRole, Affiliates, Asset, AssetAmount, Beneficiaries, Beneficiary, ChannelId,
	ForeignChain, SwapId, SwapLeg, TransactionHash, BASIS_POINTS_PER_MILLION, STABLE_ASSET,
	SWAP_RETRY_DELAY_BLOCKS,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	impl_pallet_safe_mode, liquidity::SwappingApi, CcmHandler, DepositApi, IngressEgressFeeApi,
	NetworkFeeTaken, SwapQueueApi, SwapType,
};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{Get, Saturating},
		DispatchError, Permill,
	},
	transactional,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_arithmetic::{helpers_128bit::multiply_by_rational_with_rounding, traits::Zero, Rounding};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};
#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

mod benchmarking;

pub mod migrations;
pub mod weights;
pub use weights::WeightInfo;

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(3);

pub const SWAP_DELAY_BLOCKS: u32 = 2;

struct SwapCtx {
	swap: Swap,
	stable_amount: Option<AssetAmount>,
	final_output: Option<AssetAmount>,
}

impl SwapCtx {
	fn new(swap: Swap) -> Self {
		Self {
			stable_amount: if swap.from == STABLE_ASSET { Some(swap.input_amount) } else { None },
			final_output: if swap.from == swap.to { Some(swap.input_amount) } else { None },
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
		from: Asset,
		to: Asset,
		input_amount: AssetAmount,
		refund_params: Option<SwapRefundParameters>,
		swap_type: SwapType,
	) -> Self {
		Self { swap_id, from, to, input_amount, swap_type, refund_params }
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CcmSwapLeg {
	Principal,
	Gas,
}

/// Struct denoting swap status of a cross-chain message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub(crate) struct CcmSwapOutput {
	principal: Option<AssetAmount>,
	gas: Option<AssetAmount>,
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

impl CcmSwapOutput {
	/// Returns Some of tuple (principal, gas) after swap is completed.
	/// else return None
	pub fn completed_result(self) -> Option<(AssetAmount, AssetAmount)> {
		if self.principal.is_some() && self.gas.is_some() {
			Some((self.principal.unwrap(), self.gas.unwrap()))
		} else {
			None
		}
	}
}

// Cross chain message, including information at different stages.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub(crate) struct CcmSwap {
	source_asset: Asset,
	deposit_amount: AssetAmount,
	destination_asset: Asset,
	destination_address: ForeignChainAddress,
	deposit_metadata: CcmDepositMetadata,
	principal_swap_id: Option<SwapId>,
	gas_swap_id: Option<SwapId>,
}

pub struct CcmSwapAmounts {
	pub principal_swap_amount: AssetAmount,
	pub gas_budget: AssetAmount,
	// if the gas asset is different to the input asset, it will require a swap
	pub other_gas_asset: Option<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum CcmFailReason {
	UnsupportedForTargetChain,
	InsufficientDepositAmount,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum PalletConfigUpdate {
	/// Set the maximum amount allowed to be put into a swap. Excess amounts are confiscated.
	MaximumSwapAmount { asset: Asset, amount: Option<AssetAmount> },
}

impl_pallet_safe_mode! {
	PalletSafeMode; swaps_enabled, withdrawals_enabled, broker_registration_enabled,
}

#[frame_support::pallet]
pub mod pallet {
	use cf_chains::{address::EncodedAddress, AnyChain, Chain};
	use cf_primitives::{Asset, AssetAmount, BasisPoints, EgressId, SwapId};
	use cf_traits::{
		AccountRoleRegistry, CcmSwapIds, Chainflip, EgressApi, ScheduledEgressDetails,
		SwapDepositHandler,
	};
	use frame_system::WeightInfo as SystemWeightInfo;

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
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Scheduled Swaps
	#[pallet::storage]
	#[pallet::getter(fn swap_queue)]
	pub type SwapQueue<T: Config> =
		StorageMap<_, Twox64Concat, BlockNumberFor<T>, Vec<Swap>, ValueQuery>;

	/// The first block for which swaps haven't yet been processed
	#[pallet::storage]
	pub(crate) type FirstUnprocessedBlock<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	/// SwapId Counter
	#[pallet::storage]
	pub type SwapIdCounter<T: Config> = StorageValue<_, SwapId, ValueQuery>;

	/// Earned Fees by Brokers
	#[pallet::storage]
	#[pallet::getter(fn earned_broker_fees)]
	pub(crate) type EarnedBrokerFees<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	/// Cross chain messages Counter
	#[pallet::storage]
	pub type CcmIdCounter<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Storage for storing CCMs pending assets to be swapped.
	#[pallet::storage]
	pub(crate) type PendingCcms<T: Config> = StorageMap<_, Twox64Concat, u64, CcmSwap>;

	/// Tracks the outputs of Ccm swaps.
	#[pallet::storage]
	pub(crate) type CcmOutputs<T: Config> = StorageMap<_, Twox64Concat, u64, CcmSwapOutput>;

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

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
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
		},
		/// A swap is scheduled for the first time
		SwapScheduled {
			swap_id: SwapId,
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			origin: SwapOrigin,
			swap_type: SwapType,
			#[deprecated(note = "Use broker_fee instead")]
			broker_commission: Option<AssetAmount>,
			broker_fee: Option<AssetAmount>,
			execute_at: BlockNumberFor<T>,
		},
		/// A swap is re-scheduled for a future block after failure
		SwapRescheduled {
			swap_id: SwapId,
			execute_at: BlockNumberFor<T>,
		},
		/// A swap has been executed.
		SwapExecuted {
			swap_id: SwapId,
			source_asset: Asset,
			#[deprecated(note = "Use swap_input instead")]
			deposit_amount: AssetAmount,
			swap_input: AssetAmount,
			destination_asset: Asset,
			#[deprecated(note = "Use swap_output instead")]
			egress_amount: AssetAmount,
			swap_output: AssetAmount,
			intermediate_amount: Option<AssetAmount>,
			swap_type: SwapType,
		},
		/// A swap egress has been scheduled.
		SwapEgressScheduled {
			swap_id: SwapId,
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
			fee: AssetAmount,
		},
		RefundEgressScheduled {
			swap_id: SwapId,
			egress_id: EgressId,
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
		CcmEgressScheduled {
			ccm_id: u64,
			egress_id: EgressId,
		},
		CcmDepositReceived {
			ccm_id: u64,
			principal_swap_id: Option<SwapId>,
			gas_swap_id: Option<SwapId>,
			deposit_amount: AssetAmount,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
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
			swap_id: SwapId,
			source_asset: Asset,
			destination_asset: Asset,
			total_amount: AssetAmount,
			confiscated_amount: AssetAmount,
		},
		SwapEgressIgnored {
			swap_id: SwapId,
			asset: Asset,
			amount: AssetAmount,
			reason: DispatchError,
		},
		RefundEgressIgnored {
			swap_id: SwapId,
			asset: Asset,
			amount: AssetAmount,
			reason: DispatchError,
		},
		NetworkFeeTaken {
			swap_id: SwapId,
			fee_amount: AssetAmount,
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
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Execute all swaps in the SwapQueue
		fn on_finalize(current_block: BlockNumberFor<T>) {
			if !T::SafeMode::get().swaps_enabled {
				return
			}

			// TODO: delete `FirstUnprocessedBlock` (but after this is merged to make migration
			// easier?)
			let mut block_to_process = FirstUnprocessedBlock::<T>::get();

			// NOTE: we iterate manually because BlockNumberFor<T> does not implement Step:
			while block_to_process <= current_block {
				match Self::process_swaps_for_block(current_block, block_to_process) {
					Ok(()) => {
						block_to_process += 1u32.into();
						FirstUnprocessedBlock::<T>::set(block_to_process);
					},
					Err(()) => break,
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
			refund_parameters: Option<ChannelRefundParameters>,
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
				refund_parameters,
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
			let swap_origin = SwapOrigin::Vault { tx_hash };

			let (swap_id, execute_at) = Self::schedule_swap(
				from,
				to,
				deposit_amount,
				// NOTE: FoK not yet supported for swaps from the contract
				None,
				SwapType::Swap(destination_address_internal.clone()),
			);

			Self::deposit_event(Event::<T>::SwapScheduled {
				swap_id,
				source_asset: from,
				deposit_amount,
				destination_asset: to,
				destination_address,
				origin: swap_origin,
				swap_type: SwapType::Swap(destination_address_internal),
				broker_commission: None,
				broker_fee: None,
				execute_at,
			});

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

			let _ = Self::on_ccm_deposit(
				source_asset,
				deposit_amount,
				destination_asset,
				destination_address_internal,
				deposit_metadata,
				SwapOrigin::Vault { tx_hash },
				// NOTE: FoK not yet supported for swaps from the contract
				None,
			);

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
			updates: BoundedVec<PalletConfigUpdate, ConstU32<10>>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			for update in updates {
				match update {
					PalletConfigUpdate::MaximumSwapAmount { asset, amount } => {
						MaximumSwapAmount::<T>::set(asset, amount);
						Self::deposit_event(Event::<T>::MaximumSwapAmountSet { asset, amount });
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
					refund_parameters,
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
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		#[allow(clippy::result_unit_err)]
		pub fn get_scheduled_swap_legs(
			swaps: Vec<Swap>,
			base_asset: Asset,
		) -> Result<Vec<SwapLegInfo>, ()> {
			let mut swaps: Vec<_> = swaps.into_iter().map(SwapCtx::new).collect();

			Self::swap_into_stable_taking_network_fee(&mut swaps)
				.map_err(|_| log::error!("Failed to simulate swaps"))?;

			Ok(swaps
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

						Some(SwapLegInfo {
							swap_id: swap.swap_id(),
							base_asset,
							// All swaps to `base_asset` have to go through the stable asset:
							quote_asset: STABLE_ASSET,
							side: Side::Buy,
							// Safe to unwrap as we have swapped everything into the stable asset at
							// this point
							amount: swap.stable_amount.unwrap(),
							source_asset,
							source_amount,
						})
					} else {
						None
					}
				})
				.collect())
		}

		fn swap_into_stable_taking_network_fee(
			swaps: &mut [SwapCtx],
		) -> Result<(), BatchExecutionError> {
			Self::do_group_and_swap(swaps, SwapLeg::ToStable)?;

			// Take NetworkFee for all swaps
			for swap in swaps.iter_mut() {
				if swap.swap_type() == &SwapType::NetworkFee {
					// Don't take network fee for network fee swaps
					continue;
				}

				debug_assert!(
					swap.stable_amount.is_some(),
					"All swaps should have Stable amount set here"
				);
				let stable_amount = swap.stable_amount.get_or_insert_with(Default::default);

				let NetworkFeeTaken { remaining_amount, network_fee } =
					T::SwappingApi::take_network_fee(*stable_amount);

				*stable_amount = remaining_amount;

				// Copy so we don't hold a mutable reference:
				let stable_amount = *stable_amount;

				Self::deposit_event(Event::<T>::NetworkFeeTaken {
					fee_amount: network_fee,
					swap_id: swap.swap_id(),
				});

				if swap.output_asset() == STABLE_ASSET {
					swap.final_output = Some(stable_amount);
				}
			}

			Ok(())
		}

		#[transactional]
		fn execute_batch(swaps: Vec<Swap>) -> Result<Vec<SwapCtx>, BatchExecutionError> {
			let mut swaps: Vec<_> = swaps.into_iter().map(SwapCtx::new).collect();

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

		fn process_swap_outcomes(swaps: &[SwapCtx]) {
			for swap in swaps {
				if let Some(swap_output) = swap.final_output {
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

					Self::deposit_event(Event::<T>::SwapExecuted {
						swap_id: swap.swap_id(),
						source_asset: swap.input_asset(),
						destination_asset: swap.output_asset(),
						deposit_amount: swap_input,
						swap_input,
						egress_amount: swap_output,
						swap_output,
						intermediate_amount: swap.intermediate_amount(),
						swap_type: swap.swap_type().clone(),
					});
					// Handle swap completion logic.
					match &swap.swap_type() {
						SwapType::Swap(destination_address) =>
							match T::EgressHandler::schedule_egress(
								swap.output_asset(),
								swap_output,
								destination_address.clone(),
								None,
							) {
								Ok(ScheduledEgressDetails {
									egress_id,
									egress_amount,
									fee_withheld,
								}) => {
									Self::deposit_event(Event::<T>::SwapEgressScheduled {
										swap_id: swap.swap_id(),
										egress_id,
										asset: swap.output_asset(),
										amount: egress_amount,
										fee: fee_withheld,
									});
								},
								Err(err) => {
									Self::deposit_event(Event::<T>::SwapEgressIgnored {
										swap_id: swap.swap_id(),
										asset: swap.output_asset(),
										amount: swap_output,
										reason: err.into(),
									});
								},
							},
						SwapType::CcmPrincipal(ccm_id) => {
							Self::handle_ccm_swap_result(
								*ccm_id,
								swap_output,
								CcmSwapLeg::Principal,
							);
						},
						SwapType::CcmGas(ccm_id) => {
							Self::handle_ccm_swap_result(*ccm_id, swap_output, CcmSwapLeg::Gas);
						},
						SwapType::NetworkFee =>
							if swap.output_asset() == Asset::Flip {
								FlipToBurn::<T>::mutate(|total| {
									total.saturating_accrue(swap_output);
								});
							} else {
								log_or_panic!(
									"NetworkFee burning should not be in asset: {:?}",
									swap.output_asset()
								);
							},
						SwapType::IngressEgressFee => {
							if swap.output_asset() ==
								ForeignChain::from(swap.output_asset()).gas_asset()
							{
								T::IngressEgressFeeHandler::accrue_withheld_fee(
									swap.output_asset(),
									swap_output,
								);
							} else {
								log_or_panic!(
									"IngressEgressFee swap should not be to non-gas asset: {:?}",
									swap.output_asset()
								);
							}
						},
					};
				} else {
					debug_assert!(false, "Swap is not completed yet!");
				}
			}
		}

		fn process_swaps_for_block(
			current_block: BlockNumberFor<T>,
			block_to_process: BlockNumberFor<T>,
		) -> Result<(), ()> {
			let mut swaps_to_execute = SwapQueue::<T>::take(block_to_process);

			loop {
				if swaps_to_execute.is_empty() {
					return Ok(())
				}

				match Self::execute_batch(swaps_to_execute.clone()) {
					Ok(successful_swaps) => {
						Self::process_swap_outcomes(&successful_swaps);
						// Nothing to do here, all swaps are processed for block
						return Ok(())
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
									log::error!(
										"Failed to execute swap batch at block {:?}: {:?}",
										block_to_process,
										error
									);
									(vec![], swaps_to_execute)
								},
							}
						};

						let retry_block = current_block + SWAP_RETRY_DELAY_BLOCKS.into();

						for swap in failed_swaps {
							dbg!(&swap.refund_params, retry_block);

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
													swap_id: swap.swap_id,
													egress_id,
													amount: egress_amount,
													egress_fee: fee_withheld,
												},
											);
										},
										Err(err) => {
											Self::deposit_event(Event::<T>::RefundEgressIgnored {
												swap_id: swap.swap_id,
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
							return Err(())
						}
					},
				}
			}
		}

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

			// if the gas asset is different.
			let output_gas_asset = ForeignChain::from(destination_asset).gas_asset();
			let other_gas_asset = if source_asset == output_gas_asset || gas_budget.is_zero() {
				None
			} else {
				Some(output_gas_asset)
			};

			Ok(CcmSwapAmounts { principal_swap_amount, gas_budget, other_gas_asset })
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
			swaps: &mut [SwapCtx],
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
			swaps: Vec<&mut SwapCtx>,
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

				if swap_output == 0 && matches!(swap.swap_type(), SwapType::Swap(_)) {
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

		fn handle_ccm_swap_result(ccm_id: u64, swap_output: AssetAmount, swap_leg: CcmSwapLeg) {
			CcmOutputs::<T>::mutate_exists(ccm_id, |maybe_ccm_output| {
				let ccm_output = maybe_ccm_output
					.as_mut()
					.expect("CCM that scheduled Swaps must exist in storage");
				match swap_leg {
					CcmSwapLeg::Principal => ccm_output.principal = Some(swap_output),
					CcmSwapLeg::Gas => ccm_output.gas = Some(swap_output),
				}
				if let Some((principal, gas)) = ccm_output.completed_result() {
					Self::schedule_ccm_egress(
						ccm_id,
						PendingCcms::<T>::take(ccm_id).expect("Ccm can only be completed once."),
						(principal, gas),
					);
					*maybe_ccm_output = None;
				}
			});
		}

		/// Schedule the egress of a completed Cross chain message.
		fn schedule_ccm_egress(
			ccm_id: u64,
			ccm_swap: CcmSwap,
			(ccm_output_principal, ccm_output_gas): (AssetAmount, AssetAmount),
		) {
			// Schedule the given ccm to be egressed and deposit a event.
			if let Ok(ScheduledEgressDetails { egress_id, egress_amount, fee_withheld }) =
				T::EgressHandler::schedule_egress(
					ccm_swap.destination_asset,
					ccm_output_principal,
					ccm_swap.destination_address.clone(),
					Some((ccm_swap.deposit_metadata, ccm_output_gas)),
				) {
				if let Some(swap_id) = ccm_swap.principal_swap_id {
					Self::deposit_event(Event::<T>::SwapEgressScheduled {
						swap_id,
						egress_id,
						asset: ccm_swap.destination_asset,
						amount: egress_amount,
						fee: fee_withheld,
					});
				}
				Self::deposit_event(Event::<T>::CcmEgressScheduled { ccm_id, egress_id });
			} else {
				log_or_panic!("CCM egress scheduling should never fail.");
			}
		}
	}

	impl<T: Config> SwapDepositHandler for Pallet<T> {
		type AccountId = T::AccountId;

		/// Callback function to kick off the swapping process after a successful deposit.
		fn schedule_swap_from_channel(
			deposit_address: ForeignChainAddress,
			deposit_block_height: u64,
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			destination_address: ForeignChainAddress,
			broker_commission: Beneficiaries<Self::AccountId>,
			refund_params: Option<ChannelRefundParameters>,
			channel_id: ChannelId,
		) -> SwapId {
			// Permill maxes out at 100% so this is safe.
			let fee: u128 = Permill::from_parts(
				broker_commission.iter().fold(0, |acc, entry| acc + entry.bps) as u32 *
					BASIS_POINTS_PER_MILLION,
			) * amount;

			assert!(fee <= amount, "Broker fee cannot be more than the amount");

			let net_amount = amount.saturating_sub(fee);

			let encoded_destination_address =
				T::AddressConverter::to_encoded_address(destination_address.clone());
			let swap_origin = SwapOrigin::DepositChannel {
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				channel_id,
				deposit_block_height,
			};

			// Now that we know input amount, we can calculate the minimum output amount:
			let refund_params = refund_params.map(|params| SwapRefundParameters {
				refund_block: {
					use sp_arithmetic::traits::UniqueSaturatedInto;
					// In practice block number always fits in u32:
					let current_block: u32 =
						frame_system::Pallet::<T>::block_number().unique_saturated_into();
					current_block.saturating_add(params.retry_duration)
				},
				refund_address: params.refund_address,
				min_output: u128::try_from(cf_amm::common::output_amount_ceil(
					net_amount.into(),
					params.price_limit,
				))
				.unwrap_or(u128::MAX),
			});

			let (swap_id, execute_at) = Self::schedule_swap(
				from,
				to,
				net_amount,
				refund_params,
				SwapType::Swap(destination_address.clone()),
			);

			for Beneficiary { account, bps } in broker_commission {
				EarnedBrokerFees::<T>::mutate(&account, from, |earned_fees| {
					earned_fees.saturating_accrue(
						Permill::from_parts(bps as u32 * BASIS_POINTS_PER_MILLION) * amount,
					)
				});
			}

			Self::deposit_event(Event::<T>::SwapScheduled {
				swap_id,
				source_asset: from,
				deposit_amount: amount,
				destination_asset: to,
				destination_address: encoded_destination_address,
				origin: swap_origin,
				swap_type: SwapType::Swap(destination_address),
				broker_commission: Some(fee),
				broker_fee: Some(fee),
				execute_at,
			});

			swap_id
		}
	}

	impl<T: Config> CcmHandler for Pallet<T> {
		fn on_ccm_deposit(
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			deposit_metadata: CcmDepositMetadata,
			origin: SwapOrigin,
			// TODO: CCM should use refund params
			_refund_params: Option<ChannelRefundParameters>,
		) -> Result<CcmSwapIds, ()> {
			let encoded_destination_address =
				T::AddressConverter::to_encoded_address(destination_address.clone());
			// Caller should ensure that assets and addresses are compatible.
			debug_assert!(destination_address.chain() == ForeignChain::from(destination_asset));

			let CcmSwapAmounts { principal_swap_amount, gas_budget, other_gas_asset } =
				match Self::principal_and_gas_amounts(
					deposit_amount,
					&deposit_metadata.channel_metadata,
					source_asset,
					destination_asset,
				) {
					Ok(amounts) => amounts,
					Err(reason) => {
						// Confiscate the deposit and emit an event.
						CollectedRejectedFunds::<T>::mutate(source_asset, |fund| {
							*fund = fund.saturating_add(deposit_amount)
						});

						Self::deposit_event(Event::<T>::CcmFailed {
							reason,
							destination_address: encoded_destination_address,
							deposit_metadata,
							origin: origin.clone(),
						});
						return Err(())
					},
				};

			let ccm_id = CcmIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			let mut swap_output = CcmSwapOutput::default();

			let principal_swap_id =
				if source_asset == destination_asset || principal_swap_amount.is_zero() {
					swap_output.principal = Some(principal_swap_amount);
					None
				} else {
					let (swap_id, execute_at) = Self::schedule_swap(
						source_asset,
						destination_asset,
						principal_swap_amount,
						None,
						SwapType::CcmPrincipal(ccm_id),
					);
					Self::deposit_event(Event::<T>::SwapScheduled {
						swap_id,
						source_asset,
						deposit_amount: principal_swap_amount,
						destination_asset,
						destination_address: encoded_destination_address.clone(),
						origin: origin.clone(),
						swap_type: SwapType::CcmPrincipal(ccm_id),
						broker_commission: None,
						broker_fee: None,
						execute_at,
					});
					Some(swap_id)
				};

			let gas_swap_id = if let Some(other_gas_asset) = other_gas_asset {
				let (swap_id, execute_at) = Self::schedule_swap(
					source_asset,
					other_gas_asset,
					gas_budget,
					None,
					SwapType::CcmGas(ccm_id),
				);
				Self::deposit_event(Event::<T>::SwapScheduled {
					swap_id,
					source_asset,
					deposit_amount: gas_budget,
					destination_asset: other_gas_asset,
					destination_address: encoded_destination_address.clone(),
					origin,
					swap_type: SwapType::CcmGas(ccm_id),
					broker_commission: None,
					broker_fee: None,
					execute_at,
				});
				Some(swap_id)
			} else {
				swap_output.gas = Some(gas_budget);
				None
			};

			Self::deposit_event(Event::<T>::CcmDepositReceived {
				ccm_id,
				principal_swap_id,
				gas_swap_id,
				deposit_amount,
				destination_address: encoded_destination_address,
				deposit_metadata: deposit_metadata.clone(),
			});

			// If no swap is required, egress the CCM.
			let ccm_swap = CcmSwap {
				source_asset,
				deposit_amount,
				destination_asset,
				destination_address,
				deposit_metadata,
				principal_swap_id,
				gas_swap_id,
			};
			if let Some((principal, gas)) = swap_output.completed_result() {
				Self::schedule_ccm_egress(ccm_id, ccm_swap, (principal, gas));
			} else {
				PendingCcms::<T>::insert(ccm_id, ccm_swap);
				CcmOutputs::<T>::insert(ccm_id, swap_output);
			}

			Ok(CcmSwapIds { principal_swap_id, gas_swap_id })
		}
	}
}

impl<T: Config> SwapQueueApi for Pallet<T> {
	type BlockNumber = BlockNumberFor<T>;

	fn schedule_swap(
		from: Asset,
		to: Asset,
		amount: AssetAmount,
		refund_params: Option<SwapRefundParameters>,
		swap_type: SwapType,
	) -> (u64, Self::BlockNumber) {
		let swap_id = SwapIdCounter::<T>::mutate(|id| {
			id.saturating_accrue(1);
			*id
		});

		// Do not limit the maximum swap amount for network fee swaps.
		let swap_amount = if swap_type == SwapType::NetworkFee {
			amount
		} else {
			let (swap_amount, confiscated_amount) = match MaximumSwapAmount::<T>::get(from) {
				Some(max) => (sp_std::cmp::min(amount, max), amount.saturating_sub(max)),
				None => (amount, Zero::zero()),
			};
			if !confiscated_amount.is_zero() {
				CollectedRejectedFunds::<T>::mutate(from, |fund| {
					*fund = fund.saturating_add(confiscated_amount)
				});
				Self::deposit_event(Event::<T>::SwapAmountConfiscated {
					swap_id,
					source_asset: from,
					destination_asset: to,
					total_amount: amount,
					confiscated_amount,
				});
			}
			swap_amount
		};

		let execute_at = frame_system::Pallet::<T>::block_number() + SWAP_DELAY_BLOCKS.into();

		SwapQueue::<T>::append(
			execute_at,
			Swap::new(swap_id, from, to, swap_amount, refund_params, swap_type),
		);

		(swap_id, execute_at)
	}
}

impl<T: Config> cf_traits::FlipBurnInfo for Pallet<T> {
	fn take_flip_to_burn() -> AssetAmount {
		FlipToBurn::<T>::take()
	}
}
