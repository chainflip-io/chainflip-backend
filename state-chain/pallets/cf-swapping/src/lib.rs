#![cfg_attr(not(feature = "std"), no_std)]
use cf_primitives::{Asset, AssetAmount, ForeignChain, ForeignChainAddress};
use cf_traits::{liquidity::SwappingApi, IngressApi};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
pub use pallet::*;
use sp_std::vec::Vec;

use sp_std::collections::btree_map::BTreeMap;

use sp_std::vec;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy)]
pub struct Swap<AccountId> {
	pub from: Asset,
	pub to: Asset,
	pub amount: AssetAmount,
	pub egress_address: ForeignChainAddress,
	pub relayer_id: AccountId,
	pub relayer_commission_bps: u16,
}

#[frame_support::pallet]
pub mod pallet {

	use cf_chains::{eth::assets, Ethereum};
	use cf_primitives::{Asset, AssetAmount, EthereumAddress, IntentId};
	use cf_traits::{AccountRoleRegistry, Chainflip, EgressApi, SwapIntentHandler};

	use super::*;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: Chainflip {
		/// Standard Event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// For registering and verifying the account role.
		type AccountRoleRegistry: AccountRoleRegistry<Self>;
		/// An interface to the ingress api implementation.
		type Ingress: IngressApi<AccountId = <Self as frame_system::Config>::AccountId, Ethereum>;
		/// An interface to the egress api implementation.
		type Egress: EgressApi<Ethereum>;
		/// An interface to the AMM api implementation.
		type SwappingApi: SwappingApi;
		/// The Weight information.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(PhantomData<T>);

	/// Scheduled Swaps
	#[pallet::storage]
	pub(super) type SwapQueue<T: Config> = StorageValue<_, Vec<Swap<T::AccountId>>, ValueQuery>;

	/// Earned Fees by Relayers
	#[pallet::storage]
	pub(super) type EarnedRelayerFees<T: Config> =
		StorageDoubleMap<_, Identity, T::AccountId, Twox64Concat, Asset, AssetAmount>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An new swap intent has been registered.
		NewSwapIntent { intent_id: IntentId, ingress_address: ForeignChainAddress },
	}
	#[pallet::error]
	pub enum Error<T> {
		InvalidAsset,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Do swapping with remaining weight in this block
		fn on_idle(_block_number: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			let swaps = SwapQueue::<T>::get();
			let base_weight =
				T::DbWeight::get().reads(1 as Weight) + T::DbWeight::get().writes(1 as Weight);

			let mut available_weight = remaining_weight - base_weight;
			let mut used_weight = base_weight;
			let mut swap_groups = Self::group_swaps(swaps);

			for (asset_pair, swaps) in swap_groups.clone() {
				let swap_group_weight = T::WeightInfo::execute_group_of_swaps(swaps.len() as u32);
				if available_weight < swap_group_weight {
					break
				}
				Self::execute_group_of_swaps(swaps.clone(), asset_pair.0, asset_pair.1);
				swap_groups.remove(&(asset_pair.0, asset_pair.1));
				available_weight -= swap_group_weight;
				used_weight += swap_group_weight;
			}

			let mut remaining_swaps: Vec<Swap<<T as frame_system::Config>::AccountId>> = vec![];

			for (_, swaps) in swap_groups {
				remaining_swaps.append(&mut swaps.clone());
			}

			SwapQueue::<T>::put(remaining_swaps);
			used_weight
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a new swap intent.
		///
		/// ## Events
		///
		/// - [NewSwapIntent](Event::NewSwapIntent)
		#[pallet::weight(T::WeightInfo::register_swap_intent())]
		pub fn register_swap_intent(
			origin: OriginFor<T>,
			ingress_asset: Asset,
			egress_asset: Asset,
			egress_address: ForeignChainAddress,
			relayer_commission_bps: u16,
		) -> DispatchResultWithPostInfo {
			let relayer = T::AccountRoleRegistry::ensure_relayer(origin)?;

			let (intent_id, ingress_address) = match ingress_asset.into() {
				ForeignChain::Ethereum => T::Ingress::register_swap_intent(
					ingress_asset.try_into().unwrap(),
					egress_asset,
					egress_address,
					relayer_commission_bps,
					relayer,
				),
				_ => todo!(),
			}?;
			Self::deposit_event(Event::<T>::NewSwapIntent { intent_id, ingress_address });

			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		fn calc_prop_swap(from: AssetAmount, to: AssetAmount, amount: AssetAmount) -> AssetAmount {
			if to > from {
				from.saturating_mul(amount).saturating_div(to)
			} else {
				to.saturating_mul(amount).saturating_div(from)
			}
		}

		fn calc_fee(amount: AssetAmount, bps: u16) -> AssetAmount {
			// TODO: figure out how to deal with integer math properly if bps is below 100
			amount.saturating_div(100) * bps.saturating_div(100) as u128
		}

		fn calc_netto_swap_amount(swaps: Vec<Swap<T::AccountId>>) -> AssetAmount {
			let mut total_fee = 0;
			let mut total_swap_amount = 0;
			for swap in swaps.into_iter() {
				let fee = Self::calc_fee(swap.amount, swap.relayer_commission_bps);
				total_fee += fee;
				total_swap_amount += swap.amount;
			}
			total_swap_amount.saturating_sub(total_fee)
		}

		fn store_relayer_fees(swaps: Vec<Swap<T::AccountId>>) {
			for swap in swaps.into_iter() {
				let fee = Self::calc_fee(swap.amount, swap.relayer_commission_bps);
				EarnedRelayerFees::<T>::mutate(&swap.relayer_id, swap.from, |maybe_fees| {
					if let Some(fees) = maybe_fees {
						*maybe_fees = Some(fees.saturating_add(fee))
					} else {
						*maybe_fees = Some(fee)
					}
				});
			}
		}

		pub fn execute_group_of_swaps(swaps: Vec<Swap<T::AccountId>>, from: Asset, to: Asset) {
			let total_funds_to_swap = Self::calc_netto_swap_amount(swaps.clone());
			Self::store_relayer_fees(swaps.clone());
			let (swap_output, (_, _)) = T::SwappingApi::swap(from, to, total_funds_to_swap, 1);
			for swap in swaps {
				let swap_amount =
					Self::calc_prop_swap(total_funds_to_swap, swap_output, swap.amount);
				T::Egress::schedule_egress(
					assets::eth::Asset::try_from(swap.to).expect("Only eth assets supported"),
					swap_amount,
					EthereumAddress::try_from(swap.egress_address)
						.expect("On eth assets supported")
						.into(),
				);
			}
		}

		fn group_swaps(
			swaps: Vec<Swap<T::AccountId>>,
		) -> BTreeMap<(Asset, Asset), Vec<Swap<<T as frame_system::Config>::AccountId>>> {
			let mut grouped_swaps: BTreeMap<
				(Asset, Asset),
				Vec<Swap<<T as frame_system::Config>::AccountId>>,
			> = BTreeMap::new();
			for swap in swaps {
				grouped_swaps
					.entry((swap.from, swap.to))
					.and_modify(|swaps| swaps.push(swap.clone()))
					.or_insert_with(|| vec![swap]);
			}
			grouped_swaps
		}
	}

	impl<T: Config> SwapIntentHandler for Pallet<T> {
		type AccountId = T::AccountId;
		/// Callback function to kick of the swapping process after a successful ingress.
		fn schedule_swap(
			from: Asset,
			to: Asset,
			amount: AssetAmount,
			egress_address: ForeignChainAddress,
			relayer_id: Self::AccountId,
			relayer_commission_bps: u16,
		) {
			SwapQueue::<T>::append(Swap {
				from,
				to,
				amount,
				egress_address,
				relayer_id,
				relayer_commission_bps,
			});
		}
	}
}
