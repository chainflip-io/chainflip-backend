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

use frame_support::traits::UncheckedOnRuntimeUpgrade;
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData, vec::Vec};

use crate::{Config, ManagedValidators, OperatorChoice};

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let operators = ManagedValidators::<T>::iter_keys().collect::<BTreeSet<T::AccountId>>();

		for operator in operators {
			ManagedValidators::<T>::mutate(&operator, |validators| {
				for extracted in validators.extract_if(|v| !OperatorChoice::<T>::contains_key(v)) {
					crate::Pallet::<T>::deposit_event(
						crate::Event::<T>::ValidatorRemovedFromOperator {
							validator: extracted,
							operator: operator.clone(),
						},
					);
				}
			});
		}

		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(Default::default())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		frame_support::ensure!(
			ManagedValidators::<T>::iter().all(|(operator, validators)| {
				validators
					.iter()
					.all(|v| OperatorChoice::<T>::get(v).as_ref() == Some(&operator))
			}),
			"Found a dangling validator in ManagedValidators"
		);
		Ok(())
	}
}
