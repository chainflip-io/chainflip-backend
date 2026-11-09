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

use crate::Config;
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

mod old {
	use crate::{Config, Pallet};
	use frame_support::storage_alias;
	use frame_system::pallet_prelude::BlockNumberFor;

	#[storage_alias]
	pub type ThresholdSignatureResponseTimeout<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, BlockNumberFor<T>>;

	#[storage_alias]
	pub type KeygenResponseTimeout<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, BlockNumberFor<T>>;
}

pub struct Migration<T, I>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
	fn on_runtime_upgrade() -> Weight {
		old::ThresholdSignatureResponseTimeout::<T, I>::kill();
		old::KeygenResponseTimeout::<T, I>::kill();
		T::DbWeight::get().writes(2)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			!old::ThresholdSignatureResponseTimeout::<T, I>::exists(),
			"ThresholdSignatureResponseTimeout storage should have been removed"
		);
		frame_support::ensure!(
			!old::KeygenResponseTimeout::<T, I>::exists(),
			"KeygenResponseTimeout storage should have been removed"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};
	use frame_support::instances::Instance1;

	#[test]
	fn removes_response_timeout_storage() {
		new_test_ext().execute_with(|| {
			old::ThresholdSignatureResponseTimeout::<Test, Instance1>::put(25);
			old::KeygenResponseTimeout::<Test, Instance1>::put(90);

			Migration::<Test, Instance1>::on_runtime_upgrade();

			assert!(!old::ThresholdSignatureResponseTimeout::<Test, Instance1>::exists());
			assert!(!old::KeygenResponseTimeout::<Test, Instance1>::exists());
		});
	}
}
