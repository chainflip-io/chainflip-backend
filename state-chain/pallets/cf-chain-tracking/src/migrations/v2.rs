use crate::*;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

#[cfg(feature = "try-runtime")]
use sp_std::prelude::Vec;

impl<T, I> OnRuntimeUpgrade for Migration<T, I>
where
	T: Config<I>,
	I: 'static,
	ChainState<T::TargetChain>: old::FromV1,
{
	fn on_runtime_upgrade() -> Weight {
		// Runtime-check: only migrate the Bitcoin TrackedData
		if T::TargetChain::NAME == cf_chains::Bitcoin::NAME {
			// Compile-time: `impl v1::FromV1 for ChainState<Chain>`
			// should be defined for every `Chain` we use this migration with.
			CurrentChainState::<T, I>::translate(|old| old.map(old::FromV1::from_v1))
				.expect("failed to decode v1-storage");
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}

mod old {
	use crate::*;

	pub trait FromV1 {
		type OldType: Decode;
		fn from_v1(old: Self::OldType) -> Self;
	}

	#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub enum Never {}

	macro_rules! impl_unreachable_from_v1_for_chain {
		($chain: ty) => {
			impl FromV1 for crate::ChainState<$chain> {
				type OldType = Never;
				fn from_v1(_: Self::OldType) -> Self {
					unreachable!(
						"We are not supposed to have an instance of {}",
						core::any::type_name::<Self::OldType>()
					)
				}
			}
		};
	}
	impl_unreachable_from_v1_for_chain!(cf_chains::Ethereum);
	impl_unreachable_from_v1_for_chain!(cf_chains::Polkadot);

	pub mod btc {
		use cf_chains::btc::{BitcoinFeeInfo, BitcoinTrackedData};

		use super::FromV1;
		use crate::*;

		// The following type-aliases and constants are defined here intentionally (as opposed to
		// being imported). The types and values should correspond to the types and values that were
		// in effect right before this migration.
		pub type BtcBlockNumber = u64;
		pub type BtcAmount = u64;

		const BYTES_PER_KILOBYTE: BtcAmount = 1024;
		const INPUT_UTXO_SIZE_IN_BYTES: BtcAmount = 178;
		const OUTPUT_UTXO_SIZE_IN_BYTES: BtcAmount = 34;
		const MINIMUM_BTC_TX_SIZE_IN_BYTES: BtcAmount = 12;

		#[derive(
			Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
		)]
		pub struct FeeInfo {
			pub fee_per_input_utxo: BtcAmount,
			pub fee_per_output_utxo: BtcAmount,
			pub min_fee_required_per_tx: BtcAmount,
		}

		#[derive(
			Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo,
		)]
		pub struct TrackedData {
			pub block_height: BtcBlockNumber,
			pub tracked_data: FeeInfo,
		}

		impl FromV1 for ChainState<cf_chains::Bitcoin> {
			type OldType = TrackedData;

			fn from_v1(TrackedData { block_height, tracked_data }: TrackedData) -> Self {
				log::info!("upgrading {} @{:?}", core::any::type_name::<Self>(), block_height);

				fn undo(derived_fee: u64, size: u64) -> BtcAmount {
					// the old version of this entry used to contain three values,
					// all being a function of `sats_per_kilobyte`: `sats_per_kilobyte` *
					// `SOME_SIZE_CONSTANT` / 1K .
					//
					// Here we are reversing that calculation, thus restoring the value of
					// `sats_per_kilobyte`. The sought value below is — `quot`.
					//
					// If the `rem` appears to be non-zero, this would indicate
					// that the saturating-multiplication was hit in the original calculation.

					let a = derived_fee.saturating_mul(BYTES_PER_KILOBYTE);
					let quot = a / size;
					let rem = a % size;

					if !(rem == 0 || a == BtcAmount::MAX) {
						log::warn!(
							"Fee estimation may be inaccurate. Invoked as `undo(derived_fee: {:?}, size: {:?})`", 
							derived_fee, size);
					}

					quot
				}

				let via_in = undo(tracked_data.fee_per_input_utxo, INPUT_UTXO_SIZE_IN_BYTES);
				let via_out = undo(tracked_data.fee_per_output_utxo, OUTPUT_UTXO_SIZE_IN_BYTES);
				let via_min =
					undo(tracked_data.min_fee_required_per_tx, MINIMUM_BTC_TX_SIZE_IN_BYTES);

				if !(via_out == via_in && via_out == via_min) {
					log::warn!(
						"Fee estimate may be inaccurate! [via_out: {:?}; via_in: {:?}; via_min: {:?}]", 
						via_out, via_in, via_min);
				}
				let sats_per_kilobyte = via_out.max(via_in).max(via_min);

				ChainState {
					block_height,
					tracked_data: BitcoinTrackedData {
						btc_fee_info: BitcoinFeeInfo::new(sats_per_kilobyte),
					},
				}
			}
		}
	}
}
