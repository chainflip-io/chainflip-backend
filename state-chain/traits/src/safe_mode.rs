use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::RuntimeDebug;
use scale_info::TypeInfo;

pub trait SafeMode {
	const VERSION_ID: VersionId;
	const CODE_RED: Self;
	const CODE_GREEN: Self;
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct VersionId(pub u64);

impl VersionId {
	pub const fn as_bytes(&self) -> [u8; 8] {
		self.0.to_be_bytes()
	}

	pub const fn from_input(input: &[u8]) -> Self {
		Self(xxhash_rust::const_xxh3::xxh3_64(input))
	}
}

/// Trait for setting the value of current runtime Safe Mode.
pub trait SetSafeMode<SafeModeType: SafeMode> {
	fn set_safe_mode(mode: SafeModeType);
	fn set_code_red() {
		Self::set_safe_mode(SafeModeType::CODE_RED);
	}
	fn set_code_green() {
		Self::set_safe_mode(SafeModeType::CODE_GREEN);
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
			use $crate::{SafeMode, SetSafeMode, VersionId};
			use codec::{Encode, Decode, MaxEncodedLen};
			use frame_support::{
				storage::StorageValue,
				traits::Get,
				pallet_prelude::RuntimeDebug,
			};
			use scale_info::TypeInfo;

			#[derive(Encode, TypeInfo, MaxEncodedLen, Clone, PartialEq, Eq, RuntimeDebug)]
			pub struct $runtime_safe_mode {
				pub __version_id: VersionId,
				$( pub $name: $pallet_safe_mode ),*
			}

			impl Decode for $runtime_safe_mode {
				fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
					let version = VersionId::decode(input)?;
					if version != Self::VERSION_ID {
						return Err(codec::Error::from("Invalid version id for runtime safemode."));
					}
					Ok(Self {
						__version_id: version,
						$( $name: <$pallet_safe_mode as Decode>::decode(input)? ),*
					})
				}
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
				const VERSION_ID: VersionId = VersionId::from_input([
					$( <$pallet_safe_mode as SafeMode>::VERSION_ID.as_bytes() ),*
				].flatten());// doesn't work - try using const_concat crate.
				const CODE_RED: Self = Self {
					__version_id: Self::VERSION_ID,
					$( $name: <$pallet_safe_mode as SafeMode>::CODE_RED ),*
				};
				const CODE_GREEN: Self = Self {
					__version_id: Self::VERSION_ID,
					$( $name: <$pallet_safe_mode as SafeMode>::CODE_GREEN ),*
				};
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
                <Self as $crate::SafeMode>::CODE_GREEN
            }
        }

        impl $crate::SafeMode for $pallet_safe_mode {
            const VERSION_ID: $crate::VersionId = $crate::VersionId::from_input(
                stringify!($pallet_safe_mode:$( $flag ),+).as_bytes()
            );
            const CODE_RED: Self = Self {
                $(
                    $flag: false,
                )+
            };
            const CODE_GREEN: Self = Self {
                $(
                    $flag: true,
                )+
            };
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
                <Self as $crate::SafeMode>::CODE_GREEN
            }
        }

        impl<$generic> $crate::SafeMode for $pallet_safe_mode<$generic> {
            const CODE_RED: Self = Self {
                $(
                    $flag: false,
                )+
                _phantom: ::core::marker::PhantomData,
            };
            const CODE_GREEN: Self = Self {
                $(
                    $flag: true,
                )+
                _phantom: ::core::marker::PhantomData,
            };
        }
    };
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
		const VERSION_ID: VersionId = VersionId::from_input(b"ExampleSafeModeA");
		const CODE_RED: Self = Self { safe: false };
		const CODE_GREEN: Self = Self { safe: true };
	}

	impl SafeMode for ExampleSafeModeB {
		const VERSION_ID: VersionId = VersionId::from_input(b"ExampleSafeModeB");
		const CODE_RED: Self = Self::NotSafe;
		const CODE_GREEN: Self = Self::Safe;
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
			assert!(matches!(
				<TestRuntimeSafeMode as Get<TestRuntimeSafeMode>>::get(),
				TestRuntimeSafeMode {
					example_a: ExampleSafeModeA::CODE_GREEN,
					example_b: ExampleSafeModeB::CODE_GREEN,
					pallet: SafeMode::CODE_GREEN,
					pallet_2: SafeMode::CODE_GREEN,
					..
				}
			));
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

			// Activate Code Red for all
			<TestRuntimeSafeMode as SetSafeMode<TestRuntimeSafeMode>>::set_code_red();

			assert!(matches!(
				<TestRuntimeSafeMode as Get<TestRuntimeSafeMode>>::get(),
				TestRuntimeSafeMode {
					example_a: ExampleSafeModeA::CODE_RED,
					example_b: ExampleSafeModeB::CODE_RED,
					pallet: SafeMode::CODE_RED,
					pallet_2: SafeMode::CODE_RED,
					..
				}
			));
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
			TestRuntimeSafeMode::set_safe_mode(TestRuntimeSafeMode {
				example_a: ExampleSafeModeA::CODE_RED,
				example_b: ExampleSafeModeB::CODE_RED,
				pallet: TestPalletSafeMode { flag_1: true, flag_2: false },
				pallet_2: TestPalletSafeMode2 { flag_1: false, flag_2: true },
				..Default::default()
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
					ExampleSafeModeA::CODE_GREEN,
			);
		});
	}

	// Imagine we make an update to a pallet that changes the semantics of safe mode but has
	// compatible encoding with the old safe mode.
	// Note here we use example_c with the same type as
	// previous example_b. This would normally decode fine, but the version check should prevent
	// this.
	#[storage_alias]
	pub type SafeModeStorageV2 = StorageValue<Mock, TestRuntimeSafeModeV2, ValueQuery>;

	impl_runtime_safe_mode! {
		TestRuntimeSafeModeV2,
		SafeModeStorageV2,
		example_a: ExampleSafeModeA,
		example_c: ExampleSafeModeB,
		pallet: TestPalletSafeMode,
		pallet_2: TestPalletSafeMode2,
	}

	#[test]
	fn safe_mode_incompatible_update() {
		sp_io::TestExternalities::default().execute_with(|| {
			assert!(
				TestRuntimeSafeMode::CODE_RED.__version_id !=
					TestRuntimeSafeModeV2::CODE_RED.__version_id
			);
			assert!(TestRuntimeSafeModeV2::decode(
				&mut &TestRuntimeSafeMode::CODE_GREEN.encode()[..]
			)
			.is_err());
		});
	}
}
