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

use cf_primitives::{Asset, ForeignChain};
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::sp_runtime::RuntimeDebug;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::collections::btree_set::BTreeSet;

pub trait SafeMode {
	fn code_red() -> Self;
	fn code_green() -> Self;
}

/// Trait for setting the value of current runtime Safe Mode.
pub trait SetSafeMode<SafeModeType: SafeMode> {
	fn set_safe_mode(mode: SafeModeType);
	fn set_code_red() {
		Self::set_safe_mode(SafeModeType::code_red());
	}
	fn set_code_green() {
		Self::set_safe_mode(SafeModeType::code_green());
	}
}

/// Implements the top-level RuntimeSafeMode struct.
///
/// The macros takes the following arguments:
/// - The name of the struct to be generated.
/// - The type of the storage item that will be used to store the struct.
/// - A list of the names and types of the constituent pallet-defined safe modes
///
/// For example:
///
/// ```ignore
/// impl_runtime_safe_mode! {
///     RuntimeSafeMode,
///     SafeModeStorage<Runtime>, // This must implement StorageValue<RuntimeSafeMode>
///     a: pallet_a::SafeMode,
///     b: pallet_b::SafeMode,
/// }
/// ```
#[macro_export]
macro_rules! impl_runtime_safe_mode {
	(
		$runtime_safe_mode:ident,
		$root_storage:ty,
		$( $name:ident: $pallet_safe_mode:ty ),* $(,)?
	) => {
		pub use __inner::$runtime_safe_mode;

		/// Hides imports.
		mod __inner {
			use super::*;
			use $crate::{SafeMode, SetSafeMode};
			use codec::{Encode, Decode, DecodeWithMemTracking};
			use frame_support::{
				storage::StorageValue,
				traits::Get,
				pallet_prelude::RuntimeDebug,
			};
			use scale_info::TypeInfo;

			#[derive(serde::Serialize, serde::Deserialize, Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
			pub struct $runtime_safe_mode {
				$( pub $name: $pallet_safe_mode ),*
			}

			impl Get<Self> for $runtime_safe_mode {
				fn get() -> Self {
					<$root_storage as StorageValue<_>>::get()
				}
			}

			impl Get<()> for $runtime_safe_mode {
				fn get() -> () {}
			}

			impl Default for $runtime_safe_mode {
				fn default() -> Self {
					<Self as SafeMode>::code_green()
				}
			}

			impl SafeMode for $runtime_safe_mode {
				fn code_red() -> Self {
					Self {
						$( $name: <$pallet_safe_mode as SafeMode>::code_red()),*
					}
				}

				fn code_green() -> Self {
					Self {
						$( $name: <$pallet_safe_mode as SafeMode>::code_green()),*
					}
				}
			}

			impl SetSafeMode<$runtime_safe_mode> for $runtime_safe_mode {
				fn set_safe_mode(mode: Self) {
					<$root_storage as StorageValue<_>>::put(mode);
				}
			}

			$(
				impl Get<$pallet_safe_mode> for $runtime_safe_mode {
					fn get() -> $pallet_safe_mode {
						<Self as Get<Self>>::get().$name
					}
				}

				impl SetSafeMode<$pallet_safe_mode> for $runtime_safe_mode {
					fn set_safe_mode(mode: $pallet_safe_mode) {
						<$root_storage as StorageValue<_>>::mutate(|current|{
							current.$name = mode;
						});
					}
				}
			)*
		}
	};
}

/// Implements a basic SafeMode struct for a pallet.
/// Creates a struct made up of a list of bools.
/// For pallets that requires more complex logic, SafeMode can be implemented
/// manually with custom logic.
///
/// Params:
/// - The name of the pallet's SafeMode struct.
/// - A list of the names of bool flags used to control functionalities.
///
/// Code red sets all bool flags to `false`
/// Code gree sets all bool flags to `true`
///
/// For example:
///
/// ```ignore
/// impl_pallet_safe_mode! {
///     PalletSafeMode;
///     function_1_enabled,
///     function_2_enabled,
///     function_3_enabled,
/// }
/// ```
#[macro_export]
macro_rules! impl_pallet_safe_mode {
	// Case for the non-generic version
	(
		$pallet_safe_mode:ident; $($flag:ident),+ $(,)?
	) => {
		#[derive(serde::Serialize, serde::Deserialize, codec::Encode, codec::Decode, codec::DecodeWithMemTracking, codec::MaxEncodedLen, scale_info::TypeInfo, Copy, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug)]
		pub struct $pallet_safe_mode {
			$(
				pub $flag: bool,
			)+
		}

		impl Default for $pallet_safe_mode {
			fn default() -> Self {
				<Self as $crate::SafeMode>::code_green()
			}
		}

		impl $crate::SafeMode for $pallet_safe_mode {
			fn code_red() -> Self {
				Self {
					$(
						$flag: false,
					)+
				}
			}
			fn code_green() -> Self {
				Self {
					$(
						$flag: true,
					)+
				}
			}
		}
	};

	// Case for the generic version
	(
		$pallet_safe_mode:ident<$generic:ident>; $($flag:ident),+ $(,)?
	) => {
		#[derive(serde::Serialize, serde::Deserialize, codec::Encode, codec::Decode, codec::DecodeWithMemTracking, codec::MaxEncodedLen, scale_info::TypeInfo, Copy, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug)]
		#[scale_info(skip_type_params($generic))]
		pub struct $pallet_safe_mode<$generic: 'static> {
			$(
				pub $flag: bool,
			)+
			#[doc(hidden)]
			#[codec(skip)]
			#[serde(skip_serializing)]
			_phantom: ::core::marker::PhantomData<$generic>,
		}

		impl<$generic> Default for $pallet_safe_mode<$generic> {
			fn default() -> Self {
				<Self as $crate::SafeMode>::code_green()
			}
		}

		impl<$generic> $crate::SafeMode for $pallet_safe_mode<$generic> {
			fn code_red() -> Self {
				Self {
					$(
						$flag: false,
					)+
					_phantom: ::core::marker::PhantomData,
				}
			}
			fn code_green() -> Self {
				Self {
					$(
						$flag: true,
					)+
					_phantom: ::core::marker::PhantomData,
				}
			}
		}
	};
}

/// A wrapper around a BTreeSet to make setting safe mode for all items easier. Amber contains a
/// list of disabled items.
///
/// The JSON form (e.g. in RPC output) is instead a plain list of the *enabled* items; see the
/// `Serialize` impl.
#[derive(
	Deserialize,
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	Default,
)]
pub enum SafeModeSet<T: Ord> {
	#[default]
	Green,
	Red,
	Amber(BTreeSet<T>),
}

impl<T: Ord> SafeModeSet<T> {
	pub fn enabled(&self, t: &T) -> bool {
		!self.disabled(t)
	}
	pub fn disabled(&self, t: &T) -> bool {
		match self {
			SafeModeSet::Red => true,
			SafeModeSet::Green => false,
			SafeModeSet::Amber(set) => set.contains(t),
		}
	}
	/// Disables `t`, leaving every other item as it was.
	pub fn disable(&mut self, t: T) {
		match self {
			SafeModeSet::Red => {},
			SafeModeSet::Green => *self = SafeModeSet::Amber(BTreeSet::from([t])),
			SafeModeSet::Amber(set) => {
				set.insert(t);
			},
		}
	}
}

impl<T: Ord> SafeMode for SafeModeSet<T> {
	fn code_red() -> Self {
		Self::Red
	}
	fn code_green() -> Self {
		Self::Green
	}
}

/// Serializes as the set of *enabled* items, without the enum wrapper, matching the `true` =
/// enabled convention of the bool flags it sits next to in RPC output.
impl<T: Ord + Serialize + strum::IntoEnumIterator> Serialize for SafeModeSet<T> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::Red => BTreeSet::<T>::new().serialize(serializer),
			Self::Green => T::iter().collect::<BTreeSet<_>>().serialize(serializer),
			Self::Amber(disabled) => T::iter()
				.filter(|t| !disabled.contains(t))
				.collect::<BTreeSet<_>>()
				.serialize(serializer),
		}
	}
}

/// Safe-mode changes that any single governance member can apply without a vote.
///
/// Rule for adding a preset: a member can apply any combination of presets on top of any state
/// governance has approved. So every combination must be safe on top of any approved state, and a
/// preset may only switch settings off.
///
/// Codec indices are fixed: never renumber or reuse them.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	RuntimeDebug,
)]
pub enum EmergencyPreset {
	/// Stops new deposit channels on this chain. Open channels and vault swaps keep working.
	#[codec(index = 0)]
	DisableDepositChannelCreation(ForeignChain),
	/// Stops new loans and additions to existing loans in this asset. Boost still borrows from
	/// the lending pool; see `DisableBoost`.
	#[codec(index = 1)]
	DisableNewLoans(Asset),
	/// Stops LP withdrawals, LP-to-LP transfers and transfers of FLIP to the on-chain balance.
	/// Broker withdrawals are covered by `DisableBrokerWithdrawals`.
	#[codec(index = 2)]
	DisableLpWithdrawals,
	/// Stops LP internal swaps.
	#[codec(index = 3)]
	DisableInternalSwaps,
	/// Stops boosting deposits on this chain. Deposits are credited at full confirmation instead.
	#[codec(index = 4)]
	DisableBoost(ForeignChain),
	/// Stops broker and affiliate fee withdrawals.
	#[codec(index = 5)]
	DisableBrokerWithdrawals,
	/// Stops new FLIP redemption requests. Redemptions already requested are unaffected.
	#[codec(index = 6)]
	DisableRedemptions,
	/// Pauses witnessing of deposits to channels and vault swaps on this chain. They are credited
	/// once witnessing is switched back on; deposits already prewitnessed still complete. Solana's
	/// witnessing doesn't honour this yet (PRO-3360).
	#[codec(index = 7)]
	DisableIngressWitnessing(ForeignChain),
}

impl EmergencyPreset {
	/// Every distinct preset. Only for benchmarks and tests.
	#[cfg(any(test, feature = "runtime-benchmarks", feature = "mocks"))]
	pub fn all() -> impl Iterator<Item = Self> {
		// Adding a variant fails to compile here until it is listed below.
		let _ = |preset: Self| match preset {
			Self::DisableDepositChannelCreation(_) |
			Self::DisableNewLoans(_) |
			Self::DisableLpWithdrawals |
			Self::DisableInternalSwaps |
			Self::DisableBoost(_) |
			Self::DisableBrokerWithdrawals |
			Self::DisableRedemptions |
			Self::DisableIngressWitnessing(_) => (),
		};
		ForeignChain::iter()
			.map(Self::DisableDepositChannelCreation)
			.chain(Asset::all().map(Self::DisableNewLoans))
			.chain(ForeignChain::iter().map(Self::DisableBoost))
			.chain(ForeignChain::iter().map(Self::DisableIngressWitnessing))
			.chain([
				Self::DisableLpWithdrawals,
				Self::DisableInternalSwaps,
				Self::DisableBrokerWithdrawals,
				Self::DisableRedemptions,
			])
	}
}

/// Enough for every chain, every asset and every global preset in a single call.
pub const MAX_EMERGENCY_PRESETS: u32 = 64;

pub trait EmergencySafeMode {
	/// Applies each preset to the runtime safe mode.
	fn apply(presets: &BTreeSet<EmergencyPreset>);
}

#[cfg(test)]
pub(crate) mod test {
	use super::*;
	use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
	use frame_support::{pallet_prelude::ValueQuery, storage_alias};
	use scale_info::TypeInfo;

	#[storage_alias]
	pub type SafeModeStorage = StorageValue<Mock, TestRuntimeSafeMode, ValueQuery>;

	// SafeMode struct can be defined manually
	#[derive(
		serde::Serialize,
		serde::Deserialize,
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Clone,
		PartialEq,
		Eq,
		Debug,
	)]
	pub struct ExampleSafeModeA {
		safe: bool,
	}

	#[derive(
		serde::Serialize,
		serde::Deserialize,
		Encode,
		Decode,
		DecodeWithMemTracking,
		MaxEncodedLen,
		TypeInfo,
		Clone,
		PartialEq,
		Eq,
		Debug,
	)]
	pub enum ExampleSafeModeB {
		Safe,
		NotSafe,
	}

	impl SafeMode for ExampleSafeModeA {
		fn code_red() -> Self {
			Self { safe: false }
		}
		fn code_green() -> Self {
			Self { safe: true }
		}
	}

	impl SafeMode for ExampleSafeModeB {
		fn code_red() -> Self {
			Self::NotSafe
		}
		fn code_green() -> Self {
			Self::Safe
		}
	}

	// Use this macro to define a basic safe mode struct with a list of bool flags.
	impl_pallet_safe_mode!(TestPalletSafeMode; flag_1, flag_2);

	// Multiple `impl_pallet_safe_mode` calls within the same scope requires a different mod name.
	impl_pallet_safe_mode!(TestPalletSafeMode2; flag_1, flag_2,);

	impl_runtime_safe_mode! {
		TestRuntimeSafeMode,
		SafeModeStorage,
		example_a: ExampleSafeModeA,
		example_b: ExampleSafeModeB,
		pallet: TestPalletSafeMode,
		pallet_2: TestPalletSafeMode2,
	}

	#[test]
	fn test_safe_mode() {
		use frame_support::traits::Get;
		sp_io::TestExternalities::default().execute_with(|| {
			// Default to code green
			assert!(
				<TestRuntimeSafeMode as Get<TestRuntimeSafeMode>>::get() ==
					TestRuntimeSafeMode {
						example_a: ExampleSafeModeA::code_green(),
						example_b: ExampleSafeModeB::code_green(),
						pallet: SafeMode::code_green(),
						pallet_2: SafeMode::code_green(),
					}
			);
			assert!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeA>>::get() ==
					ExampleSafeModeA::code_green()
			);
			assert!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeB>>::get() ==
					ExampleSafeModeB::code_green()
			);
			assert!(
				<TestRuntimeSafeMode as Get<TestPalletSafeMode>>::get() ==
					TestPalletSafeMode::code_green()
			);

			// Activate Code Red for all
			<TestRuntimeSafeMode as SetSafeMode<TestRuntimeSafeMode>>::set_code_red();

			assert!(
				<TestRuntimeSafeMode as Get<TestRuntimeSafeMode>>::get() ==
					TestRuntimeSafeMode {
						example_a: ExampleSafeModeA::code_red(),
						example_b: ExampleSafeModeB::code_red(),
						pallet: SafeMode::code_red(),
						pallet_2: SafeMode::code_red(),
					}
			);
			assert_eq!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeA>>::get(),
				ExampleSafeModeA::code_red()
			);
			assert_eq!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeB>>::get(),
				ExampleSafeModeB::code_red()
			);
			assert!(
				<TestRuntimeSafeMode as Get<TestPalletSafeMode>>::get() ==
					TestPalletSafeMode::code_red()
			);

			// Code Amber
			TestRuntimeSafeMode::set_safe_mode(TestRuntimeSafeMode {
				example_a: ExampleSafeModeA::code_red(),
				example_b: ExampleSafeModeB::code_red(),
				pallet: TestPalletSafeMode { flag_1: true, flag_2: false },
				pallet_2: TestPalletSafeMode2 { flag_1: false, flag_2: true },
			});
			assert!(
				<TestRuntimeSafeMode as Get<TestPalletSafeMode>>::get() ==
					TestPalletSafeMode { flag_1: true, flag_2: false },
			);
			assert!(
				<TestRuntimeSafeMode as Get<TestPalletSafeMode2>>::get() ==
					TestPalletSafeMode2 { flag_1: false, flag_2: true },
			);

			<TestRuntimeSafeMode as SetSafeMode<ExampleSafeModeA>>::set_code_green();
			assert!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeA>>::get() ==
					ExampleSafeModeA::code_green(),
			);
		});
	}

	#[test]
	fn safe_mode_set_serializes_enabled_items() {
		use cf_primitives::Asset;

		let to_json = |set: SafeModeSet<Asset>| serde_json::to_value(set).unwrap();
		let expected = |assets: BTreeSet<Asset>| serde_json::to_value(assets).unwrap();

		assert_eq!(to_json(SafeModeSet::Green), expected(Asset::all().collect()));
		assert_eq!(to_json(SafeModeSet::Red), expected(BTreeSet::new()));
		assert_eq!(
			to_json(SafeModeSet::Amber(BTreeSet::from([Asset::Btc]))),
			expected(Asset::all().filter(|a| *a != Asset::Btc).collect()),
		);
		assert_eq!(to_json(SafeModeSet::Amber(Asset::all().collect())), expected(BTreeSet::new()));
	}

	#[test]
	fn safe_mode_set_disable() {
		let disabled = |mut set: SafeModeSet<u8>, item| {
			set.disable(item);
			set
		};

		assert_eq!(disabled(SafeModeSet::Green, 1), SafeModeSet::Amber(BTreeSet::from([1])));
		assert_eq!(
			disabled(SafeModeSet::Amber(BTreeSet::from([2])), 1),
			SafeModeSet::Amber(BTreeSet::from([1, 2]))
		);
		assert_eq!(
			disabled(SafeModeSet::Amber(BTreeSet::from([1])), 1),
			SafeModeSet::Amber(BTreeSet::from([1]))
		);
		assert_eq!(disabled(SafeModeSet::Red, 1), SafeModeSet::Red);
	}

	#[test]
	fn every_emergency_preset_fits_in_one_call() {
		assert!(EmergencyPreset::all().count() <= MAX_EMERGENCY_PRESETS as usize);
	}
}
