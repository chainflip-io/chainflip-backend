pub trait SafeMode {
	const CODE_RED: Self;
	const CODE_GREEN: Self;
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
			use $crate::SafeMode;
			use codec::{Encode, Decode, MaxEncodedLen};
			use frame_support::{
				storage::StorageValue,
				traits::Get,
				RuntimeDebug
			};
			use scale_info::TypeInfo;

			#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, RuntimeDebug)]
			pub struct $runtime_safe_mode {
				$( pub $name: $pallet_safe_mode ),*
			}

			impl Get<Self> for $runtime_safe_mode {
				fn get() -> Self {
					<$root_storage as StorageValue<_>>::get()
				}
			}

			impl Default for $runtime_safe_mode {
				fn default() -> Self {
					<Self as SafeMode>::CODE_GREEN
				}
			}

			impl SafeMode for $runtime_safe_mode {
				const CODE_RED: Self = Self {
					$( $name: <$pallet_safe_mode as SafeMode>::CODE_RED ),*
				};
				const CODE_GREEN: Self = Self {
					$( $name: <$pallet_safe_mode as SafeMode>::CODE_GREEN ),*
				};
			}

			$(
				impl Get<$pallet_safe_mode> for $runtime_safe_mode {
					fn get() -> $pallet_safe_mode {
						<Self as Get<Self>>::get().$name
					}
				}
			)*
		}
	};
}

#[macro_export]
macro_rules! impl_pallet_safe_mode {
	(
		$pallet_safe_mode:ident, $($flag:ident),+
	) => {
		pub use __pallet_inner::$pallet_safe_mode;
		mod __pallet_inner {
			use $crate::SafeMode;
			use codec::{Encode, Decode, MaxEncodedLen};
			use scale_info::TypeInfo;
			use frame_support::RuntimeDebug;

			#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Copy, Clone, PartialEq, Eq, RuntimeDebug)]
			pub struct $pallet_safe_mode { $(pub $flag: bool,)* }

			impl Default for $pallet_safe_mode {
				fn default() -> Self {
					<Self as SafeMode>::CODE_GREEN
				}
			}

			impl SafeMode for $pallet_safe_mode {
				const CODE_RED: Self = Self { $($flag: false,)* };
				const CODE_GREEN: Self = Self { $($flag: true,)* };
			}
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

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub struct ExampleSafeModeA {
		safe: bool,
	}

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug)]
	pub enum ExampleSafeModeB {
		Safe,
		NotSafe,
	}

	impl SafeMode for ExampleSafeModeA {
		const CODE_RED: Self = Self { safe: false };
		const CODE_GREEN: Self = Self { safe: true };
	}

	impl SafeMode for ExampleSafeModeB {
		const CODE_RED: Self = Self::NotSafe;
		const CODE_GREEN: Self = Self::Safe;
	}

	impl_pallet_safe_mode!(TestPalletSafeMode, flag_1, flag_2);

	impl_runtime_safe_mode! {
		TestRuntimeSafeMode,
		SafeModeStorage,
		example_a: ExampleSafeModeA,
		example_b: ExampleSafeModeB,
		pallet: TestPalletSafeMode,
	}

	#[test]
	fn test_safe_mode() {
		use frame_support::traits::Get;
		sp_io::TestExternalities::default().execute_with(|| {
			// Default to code green
			assert!(
				<TestRuntimeSafeMode as Get<TestRuntimeSafeMode>>::get() ==
					TestRuntimeSafeMode {
						example_a: ExampleSafeModeA::CODE_GREEN,
						example_b: ExampleSafeModeB::CODE_GREEN,
						pallet: SafeMode::CODE_GREEN,
					}
			);
			assert!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeA>>::get() ==
					ExampleSafeModeA::CODE_GREEN
			);
			assert!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeB>>::get() ==
					ExampleSafeModeB::CODE_GREEN
			);
			assert!(
				<TestRuntimeSafeMode as Get<TestPalletSafeMode>>::get() ==
					TestPalletSafeMode::CODE_GREEN
			);

			// Code Red
			SafeModeStorage::put(TestRuntimeSafeMode::CODE_RED);

			assert!(
				<TestRuntimeSafeMode as Get<TestRuntimeSafeMode>>::get() ==
					TestRuntimeSafeMode {
						example_a: ExampleSafeModeA::CODE_RED,
						example_b: ExampleSafeModeB::CODE_RED,
						pallet: SafeMode::CODE_RED,
					}
			);
			assert_eq!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeA>>::get(),
				ExampleSafeModeA::CODE_RED
			);
			assert_eq!(
				<TestRuntimeSafeMode as Get<ExampleSafeModeB>>::get(),
				ExampleSafeModeB::CODE_RED
			);
			assert!(
				<TestRuntimeSafeMode as Get<TestPalletSafeMode>>::get() ==
					TestPalletSafeMode::CODE_RED
			);

			// Code Amber
			SafeModeStorage::put(TestRuntimeSafeMode {
				example_a: ExampleSafeModeA::CODE_RED,
				example_b: ExampleSafeModeB::CODE_RED,
				pallet: TestPalletSafeMode { flag_1: true, flag_2: false },
			});
			assert!(
				<TestRuntimeSafeMode as Get<TestPalletSafeMode>>::get() ==
					TestPalletSafeMode { flag_1: true, flag_2: false },
			);
		});
	}
}
