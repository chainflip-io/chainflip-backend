#![cfg_attr(not(feature = "std"), no_std)]
use cf_chains::{
	address::{AddressConverter, ForeignChainAddress},
	CcmDepositMetadata,
};
use cf_primitives::{Asset, AssetAmount, ChannelId, ForeignChain, SwapLeg, STABLE_ASSET};
use cf_traits::{liquidity::SwappingApi, CcmHandler, DepositApi, SystemStateInfo};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{BlockNumberProvider, Saturating},
		DispatchError, Permill,
	},
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CcmSwapLeg {
	Principal,
	Gas,
}

pub type TransactionHash = [u8; 32];

/// Struct denoting swap status of a cross-chain message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub(crate) struct CcmSwapOutput {
	principal: Option<AssetAmount>,
	gas: Option<AssetAmount>,
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
	message_metadata: CcmDepositMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum CcmFailReason {
	UnsupportedForTargetChain,
	InsufficientDepositAmount,
	PrincipalSwapAmountTooLow,
	GasBudgetBelowMinimum,
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub enum SwapOrigin {
	DepositChannel { deposit_address: ForeignChainAddress, channel_id: ChannelId },
	Vault { tx_hash: TransactionHash },
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
	pub type MinimumSwapAmount<T: Config> =
		StorageMap<_, Twox64Concat, Asset, AssetAmount, ValueQuery>;

	/// Minimum gas budget allowed for Cross chain messages.
	#[pallet::storage]
	pub type MinimumCcmGasBudget<T: Config> =
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
		},
		/// A swap deposit has been received.
		SwapScheduled {
			swap_id: u64,
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			origin: SwapOrigin,
		},
		/// A swap has been fully completed.
		SwapExecuted {
			swap_id: u64,
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
			amount: AssetAmount,
			address: EncodedAddress,
			egress_id: EgressId,
		},
		BatchSwapFailed {
			asset: Asset,
			direction: SwapLeg,
		},
		CcmEgressScheduled {
			ccm_id: u64,
			egress_id: EgressId,
		},
		SwapDepositAddressExpired {
			deposit_address: ForeignChainAddress,
		},
		SwapTtlSet {
			ttl: T::BlockNumber,
		},
		CcmDepositReceived {
			ccm_id: u64,
			principal_swap_id: Option<u64>,
			gas_swap_id: Option<u64>,
			deposit_amount: AssetAmount,
			destination_address: ForeignChainAddress,
		},
		MinimumSwapAmountSet {
			asset: Asset,
			amount: AssetAmount,
		},
		MinimumCcmGasBudgetSet {
			asset: Asset,
			amount: AssetAmount,
		},
		SwapAmountTooLow {
			asset: Asset,
			amount: AssetAmount,
			destination_address: ForeignChainAddress,
		},
		CcmFailed {
			reason: CcmFailReason,
			destination_address: ForeignChainAddress,
			message_metadata: CcmDepositMetadata,
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
		/// The CCM's gas budget is below the minimum allowed.
		CcmGasBudgetBelowMinimum,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub swap_ttl: T::BlockNumber,
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			SwapTTL::<T>::put(self.swap_ttl);
		}
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { swap_ttl: T::BlockNumber::from(1200u32) }
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Clean up expired deposit channels
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let expired = SwapChannelExpiries::<T>::take(n);
			let expired_count = expired.len();
			for (channel_id, address) in expired {
				T::DepositHandler::expire_channel(channel_id, address.clone());
				Self::deposit_event(Event::<T>::SwapDepositAddressExpired {
					deposit_address: address,
				});
			}
			T::WeightInfo::on_initialize(expired_count as u32)
		}

		/// Execute all swaps in the SwapQueue
		fn on_finalize(_n: BlockNumberFor<T>) {
			let swaps = SwapQueue::<T>::take();
			let mut unprocessed_swaps = vec![];
			// Helper function that splits swaps of a given direction, group them by asset and do
			// the swaps of a given direction. Processed and unprocessed swaps are returned.
			let mut do_group_and_swap = |swaps: Vec<Swap>, direction: SwapLeg| {
				let (swap_groups, mut remaining) = Self::split_and_group_swaps(swaps, direction);

				for (asset, mut swaps) in swap_groups {
					match Self::execute_group_of_swaps(&mut swaps, asset, direction) {
						Ok(()) => remaining.extend(swaps),
						Err(_) => {
							// If the swaps failed to execute, add them back into the queue.
							Self::deposit_event(Event::<T>::BatchSwapFailed { asset, direction });
							unprocessed_swaps.extend(swaps);
						},
					};
				}
				remaining
			};

			// Swap into Stable asset first.
			let mut remaining_swaps = do_group_and_swap(swaps, SwapLeg::ToStable);

			// Take NetworkFee for all swaps
			for swap in remaining_swaps.iter_mut() {
				debug_assert!(
					swap.stable_amount.is_some(),
					"All swaps should have Stable amount set here"
				);
				let stable_amount =
					T::SwappingApi::take_network_fee(swap.stable_amount.unwrap_or_default());
				swap.stable_amount = Some(stable_amount);
			}

			// Swap from Stable asset, and complete the swap logic.
			for swap in do_group_and_swap(remaining_swaps, SwapLeg::FromStable) {
				if let Some(output_amount) = swap.final_output {
					Self::deposit_event(Event::<T>::SwapExecuted { swap_id: swap.swap_id });
					// Handle swap completion logic.
					match &swap.swap_type {
						SwapType::Swap(destination_address) => {
							let egress_id = T::EgressHandler::schedule_egress(
								swap.to,
								output_amount,
								destination_address.clone(),
								None,
							);

							Self::deposit_event(Event::<T>::SwapEgressScheduled {
								swap_id: swap.swap_id,
								egress_id,
								asset: swap.to,
								amount: output_amount,
							});
						},
						SwapType::CcmPrincipal(ccm_id) => {
							Self::handle_ccm_swap_result(
								*ccm_id,
								output_amount,
								CcmSwapLeg::Principal,
							);
						},
						SwapType::CcmGas(ccm_id) => {
							Self::handle_ccm_swap_result(*ccm_id, output_amount, CcmSwapLeg::Gas);
						},
					};
				} else {
					debug_assert!(false, "Swap is not completed yet!");
				}
			}

			// Reinsert unprocessed swaps back into the map.
			if !unprocessed_swaps.is_empty() {
				SwapQueue::<T>::put(unprocessed_swaps);
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
			message_metadata: Option<CcmDepositMetadata>,
		) -> DispatchResult {
			let broker = T::AccountRoleRegistry::ensure_broker(origin)?;

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, destination_asset)?;

			if let Some(CcmDepositMetadata { gas_budget, .. }) = message_metadata {
				// Currently only Ethereum supports CCM.
				ensure!(
					ForeignChain::Ethereum == destination_asset.into(),
					Error::<T>::CcmUnsupportedForTargetChain
				);

				// Ensures the gas budget is above minimum allowed
				ensure!(
					gas_budget >= MinimumCcmGasBudget::<T>::get(source_asset),
					Error::<T>::CcmGasBudgetBelowMinimum,
				)
			}

			let (channel_id, deposit_address) = T::DepositHandler::request_swap_deposit_address(
				source_asset,
				destination_asset,
				destination_address_internal,
				broker_commission_bps,
				broker,
				message_metadata,
			)?;

			let expiry_block = frame_system::Pallet::<T>::current_block_number()
				.saturating_add(SwapTTL::<T>::get());
			SwapChannelExpiries::<T>::append(expiry_block, (channel_id, deposit_address.clone()));

			Self::deposit_event(Event::<T>::SwapDepositAddressReady {
				deposit_address: T::AddressConverter::try_to_encoded_address(deposit_address)?,
				destination_address,
				expiry_block,
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
			T::SystemState::ensure_no_maintenance()?;
			let account_id = T::AccountRoleRegistry::ensure_broker(origin)?;

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, asset)?;

			let amount = EarnedBrokerFees::<T>::take(account_id, asset);
			ensure!(amount != 0, Error::<T>::NoFundsAvailable);

			Self::deposit_event(Event::<T>::WithdrawalRequested {
				amount,
				address: destination_address,
				egress_id: T::EgressHandler::schedule_egress(
					asset,
					amount,
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
				destination_address_internal,
			) {
				Self::deposit_event(Event::<T>::SwapScheduled {
					swap_id,
					source_asset: from,
					deposit_amount,
					destination_asset: to,
					destination_address,
					origin: SwapOrigin::Vault { tx_hash },
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
			message_metadata: CcmDepositMetadata,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let destination_address_internal =
				Self::validate_destination_address(&destination_address, destination_asset)?;

			Self::on_ccm_deposit(
				source_asset,
				deposit_amount,
				destination_asset,
				destination_address_internal,
				message_metadata,
			);

			Ok(())
		}

		/// Register the account as a Broker.
		///
		/// Account roles are immutable once registered.
		#[pallet::weight(T::WeightInfo::register_as_broker())]
		pub fn register_as_broker(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

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

		/// Sets the Minimum gas budget for CCMs.
		///
		/// Requires Governance.
		///
		/// ## Events
		///
		/// - [On update](Event::MinimumCcmGasBudgetSet)
		#[pallet::weight(T::WeightInfo::set_minimum_ccm_gas_budget())]
		pub fn set_minimum_ccm_gas_budget(
			origin: OriginFor<T>,
			asset: Asset,
			amount: AssetAmount,
		) -> DispatchResult {
			T::EnsureGovernance::ensure_origin(origin)?;
			MinimumCcmGasBudget::<T>::insert(asset, amount);

			Self::deposit_event(Event::<T>::MinimumCcmGasBudgetSet { asset, amount });
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

		/// Bundle the given swaps and do a single swap of a given direction.
		/// Return the remaining swaps as a vec.
		pub fn execute_group_of_swaps(
			swaps: &mut Vec<Swap>,
			asset: Asset,
			direction: SwapLeg,
		) -> DispatchResult {
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
			let bundle_output = T::SwappingApi::swap_single_leg(direction, asset, bundle_input)?;

			for swap in swaps {
				let swap_output = multiply_by_rational_with_rounding(
					swap.swap_amount(direction).unwrap_or_default(),
					bundle_output,
					bundle_input,
					Rounding::Down,
				)
				.expect("bundle_input >= swap_amount ∴ result can't overflow");

				if swap_output > 0 {
					swap.update_swap_result(direction, swap_output);
				} else {
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
			swaps: Vec<Swap>,
			direction: SwapLeg,
		) -> (BTreeMap<Asset, Vec<Swap>>, Vec<Swap>) {
			let mut grouped_swaps = BTreeMap::new();
			let mut remaining = vec![];

			for swap in swaps {
				if let Some(asset) = swap.swap_asset(direction) {
					grouped_swaps.entry(asset).or_insert(vec![]).push(swap);
				} else {
					remaining.push(swap);
				}
			}
			(grouped_swaps, remaining)
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
			// Insert gas budget into storage.
			CcmGasBudget::<T>::insert(
				ccm_id,
				(ForeignChain::from(ccm_swap.destination_asset).gas_asset(), ccm_output_gas),
			);

			// Schedule the given ccm to be egressed and deposit a event.
			let egress_id = T::EgressHandler::schedule_egress(
				ccm_swap.destination_asset,
				ccm_output_principal,
				ccm_swap.destination_address.clone(),
				Some(ccm_swap.message_metadata),
			);
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
					destination_address,
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
				T::AddressConverter::try_to_encoded_address(destination_address.clone())
					.expect("what to expect here?");

			if let Some(swap_id) =
				Self::schedule_swap_from_channel_received(from, to, amount, destination_address)
			{
				Self::deposit_event(Event::<T>::SwapScheduled {
					swap_id,
					source_asset: from,
					deposit_amount: amount,
					destination_asset: to,
					destination_address: encoded_destination_address,
					origin: SwapOrigin::DepositChannel { deposit_address, channel_id },
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
			message_metadata: CcmDepositMetadata,
		) {
			// Caller should ensure that assets and addresses are compatible.
			debug_assert!(destination_address.chain() == ForeignChain::from(destination_asset));

			let principal_swap_amount = deposit_amount.saturating_sub(message_metadata.gas_budget);

			// Checks the validity of CCM.
			let error = if ForeignChain::Ethereum != destination_asset.into() {
				Some(CcmFailReason::UnsupportedForTargetChain)
			} else if deposit_amount < message_metadata.gas_budget {
				Some(CcmFailReason::InsufficientDepositAmount)
			} else if source_asset != destination_asset &&
				!principal_swap_amount.is_zero() &&
				principal_swap_amount < MinimumSwapAmount::<T>::get(source_asset)
			{
				// If the CCM's principal requires a swap and is non-zero,
				// then the principal swap amount must be above minimum swap amount required.
				Some(CcmFailReason::PrincipalSwapAmountTooLow)
			} else if message_metadata.gas_budget < MinimumCcmGasBudget::<T>::get(source_asset) {
				// The gas budget must be above the minimum allowed
				Some(CcmFailReason::GasBudgetBelowMinimum)
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
					destination_address,
					message_metadata,
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
					Some(Self::schedule_swap(
						source_asset,
						destination_asset,
						principal_swap_amount,
						SwapType::CcmPrincipal(ccm_id),
					))
				};

			let output_gas_asset = ForeignChain::from(destination_asset).gas_asset();
			let gas_swap_id = if source_asset == output_gas_asset {
				// Deposit can be used as gas directly
				swap_output.gas = Some(message_metadata.gas_budget);
				None
			} else {
				Some(Self::schedule_swap(
					source_asset,
					output_gas_asset,
					message_metadata.gas_budget,
					SwapType::CcmGas(ccm_id),
				))
			};

			Self::deposit_event(Event::<T>::CcmDepositReceived {
				ccm_id,
				principal_swap_id,
				gas_swap_id,
				deposit_amount,
				destination_address: destination_address.clone(),
			});

			// If no swap is required, egress the CCM.
			let ccm_swap = CcmSwap {
				source_asset,
				deposit_amount,
				destination_asset,
				destination_address,
				message_metadata,
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
