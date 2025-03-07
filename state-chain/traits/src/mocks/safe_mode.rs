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
