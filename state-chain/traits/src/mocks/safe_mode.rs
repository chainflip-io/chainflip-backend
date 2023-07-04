#[macro_export]
macro_rules! impl_mock_runtime_safe_mode {
	( $( $name:ident: $pallet_safe_mode:ty ),* $(,)? ) => {
		#[frame_support::storage_alias]
		pub type MockSafeModeStorage = StorageValue<
			Mock,
			MockRuntimeSafeMode,
			frame_support::pallet_prelude::ValueQuery
		>;

		$crate::impl_runtime_safe_mode! {
			MockRuntimeSafeMode,
			MockSafeModeStorage,
			$(
				$name: $pallet_safe_mode,
			)*
		}
	};
}

#[cfg(test)]
mod test {
	use crate::{
		safe_mode::test::{ExampleSafeModeA, ExampleSafeModeB},
		SafeMode, SetSafeMode,
	};

	impl_mock_runtime_safe_mode!(a: ExampleSafeModeA, b: ExampleSafeModeB);

	#[test]
	fn test_mock_safe_mode() {
		use frame_support::traits::Get;
		sp_io::TestExternalities::default().execute_with(|| {
			assert!(
				<MockRuntimeSafeMode as Get<MockRuntimeSafeMode>>::get() ==
					MockRuntimeSafeMode {
						a: ExampleSafeModeA::CODE_GREEN,
						b: ExampleSafeModeB::CODE_GREEN,
					}
			);
			assert!(
				<MockRuntimeSafeMode as Get<ExampleSafeModeA>>::get() ==
					ExampleSafeModeA::CODE_GREEN
			);
			assert!(
				<MockRuntimeSafeMode as Get<ExampleSafeModeB>>::get() ==
					ExampleSafeModeB::CODE_GREEN
			);

			MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode::CODE_RED);

			assert!(
				<MockRuntimeSafeMode as Get<MockRuntimeSafeMode>>::get() ==
					MockRuntimeSafeMode {
						a: ExampleSafeModeA::CODE_RED,
						b: ExampleSafeModeB::CODE_RED,
					}
			);
			assert_eq!(
				<MockRuntimeSafeMode as Get<ExampleSafeModeA>>::get(),
				ExampleSafeModeA::CODE_RED
			);
			assert_eq!(
				<MockRuntimeSafeMode as Get<ExampleSafeModeB>>::get(),
				ExampleSafeModeB::CODE_RED
			);
		});
	}
}
