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

use crate::{Config, RefundFeeMultiple};
use cf_chains::ForeignChain;
use frame_support::{
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use frame_support::{ensure, pallet_prelude::DispatchError};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// The refund-fee multiple to apply to Tron, overriding the default of 100.
const TRON_REFUND_FEE_MULTIPLE: u32 = 15;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		RefundFeeMultiple::<T>::insert(ForeignChain::Tron, TRON_REFUND_FEE_MULTIPLE);
		T::DbWeight::get().writes(1)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		ensure!(
			RefundFeeMultiple::<T>::get(ForeignChain::Tron) == TRON_REFUND_FEE_MULTIPLE,
			"Tron refund fee multiple should be set after migration"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;

	const REFUND_FEE_MULTIPLE_DEFAULT: u32 = 100;

	#[test]
	fn sets_only_tron_refund_fee_multiple() {
		new_test_ext().execute_with(|| {
			assert_eq!(
				RefundFeeMultiple::<Test>::get(ForeignChain::Tron),
				REFUND_FEE_MULTIPLE_DEFAULT
			);
			assert_eq!(
				RefundFeeMultiple::<Test>::get(ForeignChain::Ethereum),
				REFUND_FEE_MULTIPLE_DEFAULT
			);

			Migration::<Test>::on_runtime_upgrade();

			assert_eq!(
				RefundFeeMultiple::<Test>::get(ForeignChain::Tron),
				TRON_REFUND_FEE_MULTIPLE
			);
			assert_eq!(
				RefundFeeMultiple::<Test>::get(ForeignChain::Ethereum),
				REFUND_FEE_MULTIPLE_DEFAULT
			);
		});
	}
}
