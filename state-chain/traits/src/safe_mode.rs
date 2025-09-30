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

use codec::{Decode, Encode};
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
			use codec::{Encode, Decode};
			use frame_support::{
				storage::StorageValue,
				traits::Get,
				pallet_prelude::RuntimeDebug,
			};
			use scale_info::TypeInfo;

			#[derive(serde::Serialize, serde::Deserialize, Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
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
        #[derive(serde::Serialize, serde::Deserialize, codec::Encode, codec::Decode, codec::MaxEncodedLen, scale_info::TypeInfo, Copy, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug)]
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
        #[derive(serde::Serialize, serde::Deserialize, codec::Encode, codec::Decode, codec::MaxEncodedLen, scale_info::TypeInfo, Copy, Clone, PartialEq, Eq, frame_support::pallet_prelude::RuntimeDebug)]
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

/// A wrapper around a BTreeSet to make setting safe mode for all items easier.
#[derive(Deserialize, Encode, Decode, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug, Default)]
pub enum SafeModeSet<T: Ord> {
	#[default]
	Green,
	Red,
	Amber(BTreeSet<T>),
}

impl<T: Ord> SafeModeSet<T> {
	pub fn enabled(&self, t: &T) -> bool {
		match self {
			SafeModeSet::Red => false,
			SafeModeSet::Green => true,
			SafeModeSet::Amber(set) => set.contains(t),
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

/// Custom serialization for SafeModeSet so that it serializes like a normal BTreeSet without the
/// enum wrapper.
impl<T: Ord + Serialize + strum::IntoEnumIterator> Serialize for SafeModeSet<T> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::Red => BTreeSet::<T>::new().serialize(serializer),
			Self::Green => T::iter().collect::<BTreeSet<_>>().serialize(serializer),
			Self::Amber(set) => set.serialize(serializer),
		}
	}
}

#[cfg(test)]
pub(crate) mod test {
	use super::*;
	use codec::{Decode, Encode, MaxEncodedLen};
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
}
