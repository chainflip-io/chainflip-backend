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
		traits::{BlockNumberProvider, Get, Saturating},
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

pub mod weights;
pub use weights::WeightInfo;

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

/// This impl is never used. This is purely used to satisfy trait requirment
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

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum CcmFailReason {
	UnsupportedForTargetChain,
	InsufficientDepositAmount,
	PrincipalSwapAmountTooLow,
}

impl_pallet_safe_mode! {
	PalletSafeMode; swaps_enabled, withdrawals_enabled, deposits_enabled, broker_registration_enabled,
}

#[frame_support::pallet]
pub mod pallet {

	use cf_chains::{address::EncodedAddress, AnyChain};
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
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
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

	/// Storage for storing gas budget for each CCM.
	#[pallet::storage]
	pub type CcmGasBudget<T: Config> = StorageMap<_, Twox64Concat, u64, (Asset, AssetAmount)>;

	/// Stores the swap TTL in blocks.
	#[pallet::storage]
	pub type SwapTTL<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	/// Storage for storing CCMs pending assets to be swapped.
	#[pallet::storage]
	pub(crate) type PendingCcms<T: Config> = StorageMap<_, Twox64Concat, u64, CcmSwap>;

	/// For a given block number, stores the list of swap channels that expire at that block.
	#[pallet::storage]
	pub type SwapChannelExpiries<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::BlockNumber,
		Vec<(ChannelId, ForeignChainAddress)>,
		ValueQuery,
	>;
	/// Tracks the outputs of Ccm swaps.
	#[pallet::storage]
	pub(crate) type CcmOutputs<T: Config> = StorageMap<_, Twox64Concat, u64, CcmSwapOutput>;

	/// Minimum swap amount for each asset.
	#[pallet::storage]
	#[pallet::getter(fn minimum_swap_amount)]
	pub type MinimumSwapAmount<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	/// Fund accrued from rejected swap and CCM calls.
	#[pallet::storage]
	pub type CollectedRejectedFunds<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An new swap deposit channel has been opened.
		SwapDepositAddressReady {
			deposit_address: EncodedAddress,
			destination_address: EncodedAddress,
			expiry_block: T::BlockNumber,
			source_asset: Asset,
			destination_asset: Asset,
			channel_id: ChannelId,
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
		SwapDepositAddressExpired {
			deposit_address: EncodedAddress,
			channel_id: ChannelId,
		},
		SwapTtlSet {
			ttl: T::BlockNumber,
		},
		CcmDepositReceived {
			ccm_id: u64,
			principal_swap_id: Option<u64>,
			gas_swap_id: Option<u64>,
			deposit_amount: AssetAmount,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
		},
		MinimumSwapAmountSet {
			asset: Asset,
			amount: AssetAmount,
		},
		SwapAmountTooLow {
			asset: Asset,
			amount: AssetAmount,
			destination_address: EncodedAddress,
		},
		CcmFailed {
			reason: CcmFailReason,
			destination_address: EncodedAddress,
			deposit_metadata: CcmDepositMetadata,
		},
	}
	#[pallet::error]
	pub enum Error<T> {
		/// The provided asset and withdrawal address are incompatible.
		IncompatibleAssetAndAddress,
		/// The Asset cannot be egressed to the destination chain.
		InvalidEgressAddress,
		/// The withdrawal is not possible because not enough funds are available.
		NoFundsAvailable,
		/// The target chain does not support CCM.
		CcmUnsupportedForTargetChain,
		/// The deposited amount is insufficient to pay for the gas budget.
		CcmInsufficientDepositAmount,
		/// The provided address could not be decoded.
		InvalidDestinationAddress,
		/// The swap amount is below the minimum required.
		SwapAmountTooLow,
		/// Withdrawals are disabled due to Safe Mode.
		WithdrawalsDisabled,
		/// Swap deposits are disabled due to Safe Mode.
		DepositsDisabled,
		/// Broker registration is disabled due to Safe Mode.
		BrokerRegistrationDisabled,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub swap_ttl: T::BlockNumber,
		pub minimum_swap_amounts: Vec<(Asset, AssetAmount)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			SwapTTL::<T>::put(self.swap_ttl);
			for (asset, min) in &self.minimum_swap_amounts {
				MinimumSwapAmount::<T>::insert(asset, min);
			}
		}
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			// 1200 = 2 hours (6 sec per block)
			Self { swap_ttl: T::BlockNumber::from(1_200u32), minimum_swap_amounts: vec![] }
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Clean up expired deposit channels
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let expired = SwapChannelExpiries::<T>::take(n);
			let expired_count = expired.len();
			for (channel_id, address) in expired {
				T::DepositHandler::expire_channel(address.clone());
				Self::deposit_event(Event::<T>::SwapDepositAddressExpired {
					deposit_address: T::AddressConverter::to_encoded_address(address),
					channel_id,
				});
			}
			T::WeightInfo::on_initialize(expired_count as u32)
		}

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

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, destination_asset)?;

			if channel_metadata.is_some() {
				// Currently only Ethereum supports CCM.
				ensure!(
					ForeignChain::Ethereum == destination_asset.into(),
					Error::<T>::CcmUnsupportedForTargetChain
				);
			}

			let (channel_id, deposit_address) = T::DepositHandler::request_swap_deposit_address(
				source_asset,
				destination_asset,
				destination_address_internal,
				broker_commission_bps,
				broker,
				channel_metadata,
			)?;

			let expiry_block = frame_system::Pallet::<T>::current_block_number()
				.saturating_add(SwapTTL::<T>::get());
			SwapChannelExpiries::<T>::append(expiry_block, (channel_id, deposit_address.clone()));

			Self::deposit_event(Event::<T>::SwapDepositAddressReady {
				deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
				destination_address,
				expiry_block,
				source_asset,
				destination_asset,
				channel_id,
			});

			Ok(())
		}

		/// Brokers can withdraw their collected fees.
		///
		/// ## Events
		///
		/// - [WithdrawalRequested](Event::WithdrawalRequested)
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

			if let Some(swap_id) = Self::schedule_swap_from_channel_received(
				from,
				to,
				deposit_amount,
				destination_address_internal.clone(),
			) {
				Self::deposit_event(Event::<T>::SwapScheduled {
					swap_id,
					source_asset: from,
					deposit_amount,
					destination_asset: to,
					destination_address,
					origin: SwapOrigin::Vault { tx_hash },
					swap_type: SwapType::Swap(destination_address_internal),
				});
			}
			Ok(())
		}

		/// Process the deposit of a CCM swap.
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

		/// Sets the lifetime of swap channels.
		///
		/// Requires Governance.
		///
		/// ## Events
		///
		/// - [On update](Event::SwapTtlSet)
		#[pallet::weight(T::WeightInfo::set_swap_ttl())]
		pub fn set_swap_ttl(origin: OriginFor<T>, ttl: T::BlockNumber) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			SwapTTL::<T>::set(ttl);

			Self::deposit_event(Event::<T>::SwapTtlSet { ttl });
			Ok(())
		}

		/// Sets the Minimum swap amount allowed for an asset.
		///
		/// Requires Governance.
		///
		/// ## Events
		///
		/// - [On update](Event::MinimumSwapAmountSet)
		#[pallet::weight(T::WeightInfo::set_minimum_swap_amount())]
		pub fn set_minimum_swap_amount(
			origin: OriginFor<T>,
			asset: Asset,
			amount: AssetAmount,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			MinimumSwapAmount::<T>::insert(asset, amount);

			Self::deposit_event(Event::<T>::MinimumSwapAmountSet { asset, amount });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
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
			let bundle_output = T::SwappingApi::swap_single_leg(direction, asset, bundle_input)
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

		fn schedule_swap(from: Asset, to: Asset, amount: AssetAmount, swap_type: SwapType) -> u64 {
			let swap_id = SwapIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			SwapQueue::<T>::append(Swap::new(swap_id, from, to, amount, swap_type));

			swap_id
		}

		/// Schedule the egress of a completed Cross chain message.
		fn schedule_ccm_egress(
			ccm_id: u64,
			ccm_swap: CcmSwap,
			(ccm_output_principal, ccm_output_gas): (AssetAmount, AssetAmount),
		) {
			let gas_asset = ForeignChain::from(ccm_swap.destination_asset).gas_asset();
			// If gas is non-zero, insert gas budget into storage.
			if !ccm_output_gas.is_zero() {
				CcmGasBudget::<T>::insert(ccm_id, (gas_asset, ccm_output_gas));
			}

			// Schedule the given ccm to be egressed and deposit a event.
			let egress_id = T::EgressHandler::schedule_egress(
				ccm_swap.destination_asset,
				ccm_output_principal,
				ccm_swap.destination_address.clone(),
				Some(ccm_swap.deposit_metadata),
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
					asset: gas_asset,
					amount: ccm_output_gas,
				});
			}
			Self::deposit_event(Event::<T>::CcmEgressScheduled { ccm_id, egress_id });
		}

		// Schedule and returns the swap id if the swap is valid.
		fn schedule_swap_from_channel_received(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			destination_address: ForeignChainAddress,
		) -> Option<u64> {
			if amount < MinimumSwapAmount::<T>::get(from) {
				// If the swap amount is less than the minimum required,
				// confiscate the fund and emit an event
				CollectedRejectedFunds::<T>::mutate(from, |fund| {
					*fund = fund.saturating_add(amount)
				});
				Self::deposit_event(Event::<T>::SwapAmountTooLow {
					asset: from,
					amount,
					destination_address: T::AddressConverter::to_encoded_address(
						destination_address,
					),
				});
				None
			} else {
				// Otherwise schedule the swap.
				Some(Self::schedule_swap(from, to, amount, SwapType::Swap(destination_address)))
			}
		}
	}

	impl<T: Config> SwapDepositHandler for Pallet<T> {
		type AccountId = T::AccountId;

		/// Callback function to kick off the swapping process after a successful deposit.
		fn schedule_swap_from_channel(
			deposit_address: ForeignChainAddress,
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			destination_address: ForeignChainAddress,
			broker_id: Self::AccountId,
			broker_commission_bps: BasisPoints,
			channel_id: ChannelId,
		) {
			let fee = Permill::from_parts(broker_commission_bps as u32 * BASIS_POINTS_PER_MILLION) *
				amount;

			EarnedBrokerFees::<T>::mutate(&broker_id, from, |earned_fees| {
				earned_fees.saturating_accrue(fee)
			});

			let encoded_destination_address =
				T::AddressConverter::to_encoded_address(destination_address.clone());

			if let Some(swap_id) = Self::schedule_swap_from_channel_received(
				from,
				to,
				amount,
				destination_address.clone(),
			) {
				Self::deposit_event(Event::<T>::SwapScheduled {
					swap_id,
					source_asset: from,
					deposit_amount: amount,
					destination_asset: to,
					destination_address: encoded_destination_address,
					origin: SwapOrigin::DepositChannel {
						deposit_address: T::AddressConverter::to_encoded_address(deposit_address),
						channel_id,
					},
					swap_type: SwapType::Swap(destination_address),
				});
			}
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

			let principal_swap_amount =
				deposit_amount.saturating_sub(deposit_metadata.channel_metadata.gas_budget);

			// Checks the validity of CCM.
			let error = if ForeignChain::Ethereum != destination_asset.into() {
				Some(CcmFailReason::UnsupportedForTargetChain)
			} else if deposit_amount < deposit_metadata.channel_metadata.gas_budget {
				Some(CcmFailReason::InsufficientDepositAmount)
			} else if source_asset != destination_asset &&
				!principal_swap_amount.is_zero() &&
				principal_swap_amount < MinimumSwapAmount::<T>::get(source_asset)
			{
				// If the CCM's principal requires a swap and is non-zero,
				// then the principal swap amount must be above minimum swap amount required.
				Some(CcmFailReason::PrincipalSwapAmountTooLow)
			} else {
				None
			};

			if let Some(reason) = error {
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
			}

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
					let swap_id = Self::schedule_swap(
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
					});
					Some(swap_id)
				};

			let output_gas_asset = ForeignChain::from(destination_asset).gas_asset();
			let gas_swap_id = if source_asset == output_gas_asset ||
				deposit_metadata.channel_metadata.gas_budget.is_zero()
			{
				// Deposit can be used as gas directly
				swap_output.gas = Some(deposit_metadata.channel_metadata.gas_budget);
				None
			} else {
				let swap_id = Self::schedule_swap(
					source_asset,
					output_gas_asset,
					deposit_metadata.channel_metadata.gas_budget,
					SwapType::CcmGas(ccm_id),
				);
				Self::deposit_event(Event::<T>::SwapScheduled {
					swap_id,
					source_asset,
					deposit_amount: deposit_metadata.channel_metadata.gas_budget,
					destination_asset: output_gas_asset,
					destination_address: encoded_destination_address.clone(),
					origin,
					swap_type: SwapType::CcmGas(ccm_id),
				});
				Some(swap_id)
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
