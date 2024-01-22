#![cfg_attr(not(feature = "std"), no_std)]
use cf_chains::{
	address::{AddressConverter, ForeignChainAddress},
	CcmChannelMetadata, CcmDepositMetadata, SwapOrigin,
};
use cf_primitives::{
	Asset, AssetAmount, ChannelId, ForeignChain, SwapLeg, TransactionHash, STABLE_ASSET,
};
use cf_traits::{impl_pallet_safe_mode, liquidity::SwappingApi, CcmHandler, DepositApi};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{Get, Saturating},
		DispatchError, Permill,
	},
	storage::with_storage_layer,
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

pub const PALLET_VERSION: StorageVersion = StorageVersion::new(1);

const BASIS_POINTS_PER_MILLION: u32 = 100;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SwapType {
	Swap(ForeignChainAddress),
	CcmPrincipal(u64),
	CcmGas(u64),
}
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct Swap {
	pub swap_id: u64,
	pub from: Asset,
	pub to: Asset,
	pub amount: AssetAmount,
	pub swap_type: SwapType,
	pub stable_amount: Option<AssetAmount>,
	pub final_output: Option<AssetAmount>,
	pub fee_taken: bool,
}

impl Swap {
	fn new(swap_id: u64, from: Asset, to: Asset, amount: AssetAmount, swap_type: SwapType) -> Self {
		Self {
			swap_id,
			from,
			to,
			amount,
			swap_type,
			stable_amount: if from == STABLE_ASSET { Some(amount) } else { None },
			final_output: if from == to { Some(amount) } else { None },
			fee_taken: false,
		}
	}

	fn swap_asset(&self, direction: SwapLeg) -> Option<Asset> {
		match (direction, self.from, self.to) {
			(SwapLeg::ToStable, STABLE_ASSET, _) => None,
			(SwapLeg::ToStable, from, _) => Some(from),
			(SwapLeg::FromStable, _, STABLE_ASSET) => None,
			(SwapLeg::FromStable, _, to) => Some(to),
		}
	}

	fn swap_amount(&self, direction: SwapLeg) -> Option<AssetAmount> {
		match direction {
			SwapLeg::ToStable => Some(self.amount),
			SwapLeg::FromStable => self.stable_amount,
		}
	}

	fn update_swap_result(&mut self, direction: SwapLeg, output: AssetAmount) {
		match direction {
			SwapLeg::ToStable => {
				self.stable_amount = Some(output);
				if self.to == STABLE_ASSET {
					self.final_output = Some(output);
				}
			},
			SwapLeg::FromStable => self.final_output = Some(output),
		}
	}

	fn intermediate_amount(&self) -> Option<AssetAmount> {
		if self.from == STABLE_ASSET || self.to == STABLE_ASSET {
			None
		} else {
			self.stable_amount
		}
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
	principal_swap_id: Option<u64>,
	gas_swap_id: Option<u64>,
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

impl_pallet_safe_mode! {
	PalletSafeMode; swaps_enabled, withdrawals_enabled, deposits_enabled, broker_registration_enabled,
}

#[frame_support::pallet]
pub mod pallet {

	use cf_chains::{address::EncodedAddress, AnyChain, Chain};
	use cf_primitives::{Asset, AssetAmount, BasisPoints, EgressId};
	use cf_traits::{AccountRoleRegistry, Chainflip, EgressApi, SwapDepositHandler};

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
	}

	#[pallet::pallet]
	#[pallet::storage_version(PALLET_VERSION)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(PhantomData<T>);

	/// Scheduled Swaps
	#[pallet::storage]
	pub(crate) type SwapQueue<T: Config> = StorageValue<_, Vec<Swap>, ValueQuery>;

	/// SwapId Counter
	#[pallet::storage]
	pub type SwapIdCounter<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Earned Fees by Brokers
	#[pallet::storage]
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
		},
		/// A swap deposit has been received.
		SwapScheduled {
			swap_id: u64,
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			origin: SwapOrigin,
			swap_type: SwapType,
			broker_commission: Option<AssetAmount>,
		},
		/// A swap has been executed.
		SwapExecuted {
			swap_id: u64,
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			egress_amount: AssetAmount,
			intermediate_amount: Option<AssetAmount>,
		},
		/// A swap egress has been scheduled.
		SwapEgressScheduled {
			swap_id: u64,
			egress_id: EgressId,
			asset: Asset,
			amount: AssetAmount,
		},
		/// A broker fee withdrawal has been requested.
		WithdrawalRequested {
			egress_id: EgressId,
			egress_amount: AssetAmount,
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
			principal_swap_id: Option<u64>,
			gas_swap_id: Option<u64>,
			deposit_amount: AssetAmount,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
		},
		CcmFailed {
			reason: CcmFailReason,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
		},
		MaximumSwapAmountSet {
			asset: Asset,
			amount: Option<AssetAmount>,
		},
		SwapAmountConfiscated {
			swap_id: u64,
			source_asset: Asset,
			destination_asset: Asset,
			total_amount: AssetAmount,
			confiscated_amount: AssetAmount,
		},
		/// The swap has been executed, but has led to a zero egress amount.
		EgressAmountZero {
			swap_id: u64,
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
		/// Swap deposits are disabled due to Safe Mode.
		DepositsDisabled,
		/// Broker registration is disabled due to Safe Mode.
		BrokerRegistrationDisabled,
		/// Broker commission bps is limited to 1000 points.
		BrokerCommissionBpsTooHigh,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Execute all swaps in the SwapQueue
		fn on_finalize(_n: BlockNumberFor<T>) {
			if !T::SafeMode::get().swaps_enabled {
				return
			}
			// Wrap the entire swapping section as a transaction, any failed swap will rollback all
			// storage changes.
			if let Err(failed_swap) = with_storage_layer(|| -> Result<(), BatchExecutionError> {
				let mut swaps = SwapQueue::<T>::take();

				// Swap into Stable asset first.
				Self::do_group_and_swap(&mut swaps, SwapLeg::ToStable)?;

				// Take NetworkFee for all swaps
				for swap in swaps.iter_mut() {
					debug_assert!(
						swap.stable_amount.is_some(),
						"All swaps should have Stable amount set here"
					);
					let stable_amount = swap.stable_amount.get_or_insert_with(Default::default);
					*stable_amount = T::SwappingApi::take_network_fee(*stable_amount);
				}

				// Swap from Stable asset, and complete the swap logic.
				Self::do_group_and_swap(&mut swaps, SwapLeg::FromStable)?;

				for swap in swaps {
					if let Some(egress_amount) = swap.final_output {
						Self::deposit_event(Event::<T>::SwapExecuted {
							swap_id: swap.swap_id,
							source_asset: swap.from,
							destination_asset: swap.to,
							deposit_amount: swap.amount,
							egress_amount,
							intermediate_amount: swap.intermediate_amount(),
						});
						// Handle swap completion logic.
						match &swap.swap_type {
							SwapType::Swap(destination_address) =>
								if !egress_amount.is_zero() {
									let egress_id = T::EgressHandler::schedule_egress(
										swap.to,
										egress_amount,
										destination_address.clone(),
										None,
									);

									Self::deposit_event(Event::<T>::SwapEgressScheduled {
										swap_id: swap.swap_id,
										egress_id,
										asset: swap.to,
										amount: egress_amount,
									});
								} else {
									Self::deposit_event(Event::<T>::EgressAmountZero {
										swap_id: swap.swap_id,
									})
								},
							SwapType::CcmPrincipal(ccm_id) => {
								Self::handle_ccm_swap_result(
									*ccm_id,
									egress_amount,
									CcmSwapLeg::Principal,
								);
							},
							SwapType::CcmGas(ccm_id) => {
								Self::handle_ccm_swap_result(
									*ccm_id,
									egress_amount,
									CcmSwapLeg::Gas,
								);
							},
						};
					} else {
						debug_assert!(false, "Swap is not completed yet!");
					}
				}
				Ok(())
			}) {
				match failed_swap {
					BatchExecutionError::SwapLegFailed { asset, direction, amount } =>
						Self::deposit_event(Event::<T>::BatchSwapFailed {
							asset,
							direction,
							amount,
						}),
					BatchExecutionError::DispatchError { error } => {
						log::error!("Failed to execute swap batch: {:?}", error);
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
			broker_commission_bps: BasisPoints,
			channel_metadata: Option<CcmChannelMetadata>,
		) -> DispatchResult {
			ensure!(T::SafeMode::get().deposits_enabled, Error::<T>::DepositsDisabled);
			let broker = T::AccountRoleRegistry::ensure_broker(origin)?;
			ensure!(broker_commission_bps <= 1000, Error::<T>::BrokerCommissionBpsTooHigh);

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, destination_asset)?;

			if channel_metadata.is_some() {
				// Currently only Ethereum supports CCM.
				ensure!(
					ForeignChain::Ethereum == destination_asset.into(),
					Error::<T>::CcmUnsupportedForTargetChain
				);
			}

			let (channel_id, deposit_address, expiry_height) =
				T::DepositHandler::request_swap_deposit_address(
					source_asset,
					destination_asset,
					destination_address_internal,
					broker_commission_bps,
					broker,
					channel_metadata.clone(),
				)?;

			Self::deposit_event(Event::<T>::SwapDepositAddressReady {
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				destination_address,
				source_asset,
				destination_asset,
				channel_id,
				broker_commission_rate: broker_commission_bps,
				channel_metadata,
				source_chain_expiry_block: expiry_height,
			});

			Ok(())
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

			let egress_amount = EarnedBrokerFees::<T>::take(account_id, asset);
			ensure!(egress_amount != 0, Error::<T>::NoFundsAvailable);

			Self::deposit_event(Event::<T>::WithdrawalRequested {
				egress_amount,
				destination_address,
				egress_id: T::EgressHandler::schedule_egress(
					asset,
					egress_amount,
					destination_address_internal,
					None,
				),
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

			let swap_id = Self::schedule_swap_internal(
				from,
				to,
				deposit_amount,
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

			Self::on_ccm_deposit(
				source_asset,
				deposit_amount,
				destination_asset,
				destination_address_internal,
				deposit_metadata,
				SwapOrigin::Vault { tx_hash },
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

		/// Sets the Maximum amount allowed in a single swap for an asset.
		///
		/// Requires Governance.
		///
		/// ## Events
		///
		/// - [On update](Event::MaximumSwapAmountSet)
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::set_maximum_swap_amount())]
		pub fn set_maximum_swap_amount(
			origin: OriginFor<T>,
			asset: Asset,
			amount: Option<AssetAmount>,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;

			match amount {
				Some(max) => MaximumSwapAmount::<T>::insert(asset, max),
				None => MaximumSwapAmount::<T>::remove(asset),
			};

			Self::deposit_event(Event::<T>::MaximumSwapAmountSet { asset, amount });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn principal_and_gas_amounts(
			deposit_amount: AssetAmount,
			channel_metadata: &CcmChannelMetadata,
			source_asset: Asset,
			destination_asset: Asset,
		) -> Result<CcmSwapAmounts, CcmFailReason> {
			let gas_budget = channel_metadata.gas_budget;
			let principal_swap_amount = deposit_amount.saturating_sub(gas_budget);

			if ForeignChain::Ethereum != destination_asset.into() {
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
			swaps: &mut Vec<Swap>,
			direction: SwapLeg,
		) -> Result<(), BatchExecutionError> {
			let swap_groups = Self::split_and_group_swaps(swaps, direction);

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
			swaps: Vec<&mut Swap>,
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

			debug_assert!(bundle_input > 0, "Swap input of zero is invalid.");

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

				if swap_output == 0 && matches!(swap.swap_type, SwapType::Swap(_)) {
					// This is unlikely but theoretically possible if, for example, the initial swap
					// input is so small compared to the total bundle size that it rounds down to
					// zero when we do the division.
					log::warn!(
						"Swap {:?} in bundle {{ input: {bundle_input}, output: {bundle_output} }} resulted in swap output of zero.",
						swap
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

		/// Split all swaps of a given direction, and group them by asset into a BTreeMap and return
		/// the rest
		fn split_and_group_swaps(
			swaps: &mut Vec<Swap>,
			direction: SwapLeg,
		) -> BTreeMap<Asset, Vec<&mut Swap>> {
			let mut grouped_swaps = BTreeMap::new();

			for swap in swaps {
				if let Some(asset) = swap.swap_asset(direction) {
					grouped_swaps.entry(asset).or_insert(vec![]).push(swap);
				}
			}

			grouped_swaps
		}

		/// Schedule the egress of a completed Cross chain message.
		fn schedule_ccm_egress(
			ccm_id: u64,
			ccm_swap: CcmSwap,
			(ccm_output_principal, ccm_output_gas): (AssetAmount, AssetAmount),
		) {
			// Schedule the given ccm to be egressed and deposit a event.
			let egress_id = T::EgressHandler::schedule_egress(
				ccm_swap.destination_asset,
				ccm_output_principal,
				ccm_swap.destination_address.clone(),
				Some((ccm_swap.deposit_metadata, ccm_output_gas)),
			);

			if let Some(swap_id) = ccm_swap.principal_swap_id {
				Self::deposit_event(Event::<T>::SwapEgressScheduled {
					swap_id,
					egress_id,
					asset: ccm_swap.destination_asset,
					amount: ccm_output_principal,
				});
			}
			if let Some(swap_id) = ccm_swap.gas_swap_id {
				Self::deposit_event(Event::<T>::SwapEgressScheduled {
					swap_id,
					egress_id,
					asset: ForeignChain::from(ccm_swap.destination_asset).gas_asset(),
					amount: ccm_output_gas,
				});
			}
			Self::deposit_event(Event::<T>::CcmEgressScheduled { ccm_id, egress_id });
		}

		/// Schedule the swap, assuming all checks already passed.
		fn schedule_swap_internal(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			swap_type: SwapType,
		) -> u64 {
			let swap_id = SwapIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});
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

			SwapQueue::<T>::append(Swap::new(swap_id, from, to, swap_amount, swap_type));

			swap_id
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
			broker_id: Self::AccountId,
			broker_commission_bps: BasisPoints,
			channel_id: ChannelId,
		) {
			// Permill maxes out at 100% so this is safe.
			let fee = Permill::from_parts(broker_commission_bps as u32 * BASIS_POINTS_PER_MILLION) *
				amount;
			assert!(fee <= amount, "Broker fee cannot be more than the amount");

			let net_amount = amount.saturating_sub(fee);

			let encoded_destination_address =
				T::AddressConverter::to_encoded_address(destination_address.clone());
			let swap_origin = SwapOrigin::DepositChannel {
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				channel_id,
				deposit_block_height,
			};

			let swap_id = Self::schedule_swap_internal(
				from,
				to,
				net_amount,
				SwapType::Swap(destination_address.clone()),
			);
			EarnedBrokerFees::<T>::mutate(&broker_id, from, |earned_fees| {
				earned_fees.saturating_accrue(fee)
			});
			Self::deposit_event(Event::<T>::SwapScheduled {
				swap_id,
				source_asset: from,
				deposit_amount: amount,
				destination_asset: to,
				destination_address: encoded_destination_address,
				origin: swap_origin,
				swap_type: SwapType::Swap(destination_address),
				broker_commission: Some(fee),
			});
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
		) {
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
						});
						return
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
					let swap_id = Self::schedule_swap_internal(
						source_asset,
						destination_asset,
						principal_swap_amount,
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
					});
					Some(swap_id)
				};

			let gas_swap_id = if let Some(other_gas_asset) = other_gas_asset {
				let swap_id = Self::schedule_swap_internal(
					source_asset,
					other_gas_asset,
					gas_budget,
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
		}
	}
}
