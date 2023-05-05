#![cfg_attr(not(feature = "std"), no_std)]
use cf_chains::{
	address::{AddressConverter, ForeignChainAddress},
	CcmDepositMetadata,
};
use cf_primitives::{Asset, AssetAmount, ChannelId, ForeignChain};
use cf_traits::{liquidity::SwappingApi, CcmHandler, DepositApi, SystemStateInfo};
use frame_support::{
	pallet_prelude::*,
	sp_runtime::{
		traits::{BlockNumberProvider, Saturating},
		Permill,
	},
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_arithmetic::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
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
}

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
	pub(super) type SwapQueue<T: Config> = StorageValue<_, Vec<Swap>, ValueQuery>;

	/// SwapId Counter
	#[pallet::storage]
	pub type SwapIdCounter<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Earned Fees by Relayers
	#[pallet::storage]
	pub(super) type EarnedRelayerFees<T: Config> =
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
	pub(super) type PendingCcms<T: Config> = StorageMap<_, Twox64Concat, u64, CcmSwap>;

	/// For a given block number, stores the list of swap channels that expire at that block.
	#[pallet::storage]
	pub(super) type SwapChannelExpiries<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::BlockNumber,
		Vec<(ChannelId, ForeignChain, ForeignChainAddress)>,
		ValueQuery,
	>;
	/// Tracks the outputs of Ccm swaps.
	#[pallet::storage]
	pub(super) type CcmOutputs<T: Config> = StorageMap<_, Twox64Concat, u64, CcmSwapOutput>;

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
		SwapDepositReceived {
			swap_id: u64,
			deposit_address: EncodedAddress,
			deposit_amount: AssetAmount,
		},
		SwapScheduledByWitnesser {
			swap_id: u64,
			deposit_amount: AssetAmount,
			destination_address: EncodedAddress,
		},
		/// A swap has been executed.
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
		/// A relayer fee withdrawal has been requested.
		WithdrawalRequested {
			amount: AssetAmount,
			address: EncodedAddress,
			egress_id: EgressId,
		},
		BatchSwapFailed {
			asset_pair: (Asset, Asset),
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
		/// Do swapping with remaining weight in this block
		fn on_idle(_block_number: BlockNumberFor<T>, available_weight: Weight) -> Weight {
			let swaps = SwapQueue::<T>::take();
			let mut used_weight = T::DbWeight::get().reads(1);

			let swap_groups = Self::group_swaps_by_asset_pair(swaps);
			let mut unexecuted = vec![];

			for (asset_pair, swaps) in swap_groups {
				let swap_group_weight = T::WeightInfo::execute_group_of_swaps(swaps.len() as u32);
				if used_weight.saturating_add(swap_group_weight).all_gt(available_weight) {
					// Add un-excecuted swaps back to storage
					unexecuted.extend(swaps)
				} else {
					// Execute the swaps and add the weights.
					used_weight.saturating_accrue(swap_group_weight);
					if Self::execute_group_of_swaps(&swaps[..], asset_pair.0, asset_pair.1).is_err()
					{
						// If the swaps failed to execute, add them back into the queue.
						Self::deposit_event(Event::<T>::BatchSwapFailed { asset_pair });
						unexecuted.extend(swaps)
					}
				}
			}

			if !unexecuted.is_empty() {
				SwapQueue::<T>::put(unexecuted);
				used_weight.saturating_accrue(T::DbWeight::get().writes(1));
			}
			used_weight
		}

		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			let expired = SwapChannelExpiries::<T>::take(n);
			for (channel_id, chain, address) in expired.clone() {
				T::DepositHandler::expire_channel(chain, channel_id, address.clone());
				Self::deposit_event(Event::<T>::SwapDepositAddressExpired {
					deposit_address: address,
				});
			}
			T::WeightInfo::on_initialize(expired.len() as u32)
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
			relayer_commission_bps: BasisPoints,
			message_metadata: Option<CcmDepositMetadata>,
		) -> DispatchResult {
			let relayer = T::AccountRoleRegistry::ensure_relayer(origin)?;

			if message_metadata.is_some() {
				// Currently only Ethereum supports CCM.
				ensure!(
					ForeignChain::Ethereum == destination_asset.into(),
					Error::<T>::CcmUnsupportedForTargetChain
				);
			}
			let destination_address_internal =
				T::AddressConverter::try_from_encoded_address(destination_address.clone())
					.map_err(|_| Error::<T>::InvalidDestinationAddress)?;
			ensure!(
				ForeignChain::from(destination_address_internal.clone()) ==
					ForeignChain::from(destination_asset),
				Error::<T>::IncompatibleAssetAndAddress
			);

			let (channel_id, deposit_address) = T::DepositHandler::request_swap_deposit_address(
				source_asset,
				destination_asset,
				destination_address_internal,
				relayer_commission_bps,
				relayer,
				message_metadata,
			)?;

			let expiry_block = frame_system::Pallet::<T>::current_block_number()
				.saturating_add(SwapTTL::<T>::get());
			SwapChannelExpiries::<T>::append(
				expiry_block,
				(channel_id, ForeignChain::from(source_asset), deposit_address.clone()),
			);

			Self::deposit_event(Event::<T>::SwapDepositAddressReady {
				deposit_address: T::AddressConverter::try_to_encoded_address(deposit_address)?,
				destination_address,
				expiry_block,
			});

			Ok(())
		}

		/// Relayers can withdraw their collected fees.
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
			let account_id = T::AccountRoleRegistry::ensure_relayer(origin)?;

			let destination_address_internal =
				T::AddressConverter::try_from_encoded_address(destination_address.clone())
					.map_err(|_| Error::<T>::InvalidDestinationAddress)?;

			ensure!(
				ForeignChain::from(destination_address_internal.clone()) ==
					ForeignChain::from(asset),
				Error::<T>::InvalidEgressAddress
			);

			let amount = EarnedRelayerFees::<T>::take(account_id, asset);
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
		/// - [SwapScheduled](Event::SwapDepositReceived)
		#[pallet::weight(T::WeightInfo::schedule_swap_by_witnesser())]
		pub fn schedule_swap_by_witnesser(
			origin: OriginFor<T>,
			from: Asset,
			to: Asset,
			deposit_amount: AssetAmount,
			destination_address: EncodedAddress,
		) -> DispatchResult {
			T::EnsureWitnessed::ensure_origin(origin)?;

			let destination_address_internal =
				T::AddressConverter::try_from_encoded_address(destination_address.clone())
					.map_err(|_| Error::<T>::InvalidDestinationAddress)?;

			let swap_id = Self::schedule_swap(
				from,
				to,
				deposit_amount,
				SwapType::Swap(destination_address_internal),
			);

			Self::deposit_event(Event::<T>::SwapScheduledByWitnesser {
				swap_id,
				deposit_amount,
				destination_address,
			});

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

			let destination_address_internal = T::AddressConverter::try_from_encoded_address(
				destination_address,
			)
			.map_err(|_| {
				DispatchError::Other("Invalid destination address, cannot decode the address")
			})?;

			ensure!(
				ForeignChain::from(destination_asset) ==
					ForeignChain::from(destination_address_internal.clone()),
				Error::<T>::IncompatibleAssetAndAddress
			);

			Self::on_ccm_deposit(
				source_asset,
				deposit_amount,
				destination_asset,
				destination_address_internal,
				message_metadata,
			)
		}

		/// Register the account as a Relayer.
		///
		/// Account roles are immutable once registered.
		#[pallet::weight(T::WeightInfo::register_as_relayer())]
		pub fn register_as_relayer(who: OriginFor<T>) -> DispatchResult {
			let account_id = ensure_signed(who)?;

			T::AccountRoleRegistry::register_as_relayer(&account_id)?;

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
			let _ok = T::EnsureGovernance::ensure_origin(origin)?;
			SwapTTL::<T>::set(ttl);

			Self::deposit_event(Event::<T>::SwapTtlSet { ttl });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn execute_group_of_swaps(swaps: &[Swap], from: Asset, to: Asset) -> DispatchResult {
			debug_assert!(
				!swaps.is_empty(),
				"The implementation of grouped_swaps ensures that the swap groups are non-empty."
			);

			let bundle_input: AssetAmount = swaps
				.iter()
				.map(|swap| {
					debug_assert_eq!((swap.from, swap.to), (from, to));
					swap.amount
				})
				.sum();

			debug_assert!(bundle_input > 0, "Swap input of zero is invalid.");

			let bundle_output = T::SwappingApi::swap(from, to, bundle_input)?.output;
			for swap in swaps {
				Self::deposit_event(Event::<T>::SwapExecuted { swap_id: swap.swap_id });
				let swap_output = multiply_by_rational_with_rounding(
					swap.amount,
					bundle_output,
					bundle_input,
					Rounding::Down,
				)
				.expect("bundle_input >= swap_amount ∴ result can't overflow");

				if swap_output > 0 {
					match &swap.swap_type {
						SwapType::Swap(destination_address) => {
							let egress_id = T::EgressHandler::schedule_egress(
								swap.to,
								swap_output,
								destination_address.clone(),
								None,
							);

							Self::deposit_event(Event::<T>::SwapEgressScheduled {
								swap_id: swap.swap_id,
								egress_id,
								asset: to,
								amount: swap_output,
							});
						},
						SwapType::CcmPrincipal(ccm_id) => {
							CcmOutputs::<T>::mutate_exists(ccm_id, |maybe_ccm_output| {
								let ccm_output = maybe_ccm_output
									.as_mut()
									.expect("CCM that scheduled Swaps must exist in storage");
								ccm_output.principal = Some(swap_output);
								if let Some((principal, gas)) = ccm_output.completed_result() {
									Self::schedule_ccm_egress(
										*ccm_id,
										PendingCcms::<T>::take(ccm_id)
											.expect("Ccm can only be completed once."),
										(principal, gas),
									);
									*maybe_ccm_output = None;
								}
							});
						},
						SwapType::CcmGas(ccm_id) => {
							CcmOutputs::<T>::mutate_exists(ccm_id, |maybe_ccm_output| {
								let ccm_output = maybe_ccm_output
									.as_mut()
									.expect("CCM that scheduled Swaps must exist in storage");
								ccm_output.gas = Some(swap_output);
								if let Some((principal, gas)) = ccm_output.completed_result() {
									Self::schedule_ccm_egress(
										*ccm_id,
										PendingCcms::<T>::take(ccm_id)
											.expect("Ccm can only be completed once."),
										(principal, gas),
									);

									*maybe_ccm_output = None;
								}
							});
						},
					};
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

		fn group_swaps_by_asset_pair(swaps: Vec<Swap>) -> BTreeMap<(Asset, Asset), Vec<Swap>> {
			let mut grouped_swaps = BTreeMap::new();
			for swap in swaps {
				grouped_swaps.entry((swap.from, swap.to)).or_insert(vec![]).push(swap)
			}
			grouped_swaps
		}

		fn schedule_swap(from: Asset, to: Asset, amount: AssetAmount, swap_type: SwapType) -> u64 {
			let swap_id = SwapIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			SwapQueue::<T>::append(Swap { swap_id, from, to, amount, swap_type });

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
	}

	impl<T: Config> SwapDepositHandler for Pallet<T> {
		type AccountId = T::AccountId;

		/// Callback function to kick off the swapping process after a successful deposit.
		fn on_swap_deposit(
			deposit_address: ForeignChainAddress,
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			destination_address: ForeignChainAddress,
			relayer_id: Self::AccountId,
			relayer_commission_bps: BasisPoints,
		) {
			let fee = Permill::from_parts(relayer_commission_bps as u32 * BASIS_POINTS_PER_MILLION) *
				amount;

			EarnedRelayerFees::<T>::mutate(&relayer_id, from, |earned_fees| {
				earned_fees.saturating_accrue(fee)
			});

			let swap_id = Self::schedule_swap(
				from,
				to,
				amount.saturating_sub(fee),
				SwapType::Swap(destination_address),
			);

			Self::deposit_event(Event::<T>::SwapDepositReceived {
				swap_id,
				deposit_address: T::AddressConverter::try_to_encoded_address(deposit_address)
					.expect("The deposit address is generated internally and is always valid."),
				deposit_amount: amount,
			});
		}
	}

	impl<T: Config> CcmHandler for Pallet<T> {
		fn on_ccm_deposit(
			source_asset: Asset,
			deposit_amount: AssetAmount,
			destination_asset: Asset,
			destination_address: ForeignChainAddress,
			message_metadata: CcmDepositMetadata,
		) -> DispatchResult {
			// Caller should ensure that assets and addresses are compatible.
			debug_assert!(
				ForeignChain::from(destination_address.clone()) ==
					ForeignChain::from(destination_asset)
			);

			// Currently only Ethereum supports CCM.
			ensure!(
				ForeignChain::Ethereum == destination_asset.into(),
				Error::<T>::CcmUnsupportedForTargetChain
			);

			// Deposited amount should be enough to pay for gas.
			ensure!(
				deposit_amount > message_metadata.gas_budget,
				Error::<T>::CcmInsufficientDepositAmount
			);

			let ccm_id = CcmIdCounter::<T>::mutate(|id| {
				id.saturating_accrue(1);
				*id
			});

			let mut swap_output = CcmSwapOutput::default();

			let principal_swap_amount = deposit_amount.saturating_sub(message_metadata.gas_budget);
			let principal_swap_id = if source_asset == destination_asset {
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

			Ok(())
		}
	}
}
