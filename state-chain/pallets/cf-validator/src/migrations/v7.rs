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

use cf_primitives::EpochIndex;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

use crate::{Config, HistoricalActiveEpochs, HistoricalBonds};

#[cfg(feature = "try-runtime")]
use frame_support::pallet_prelude::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::prelude::*;

pub struct Migration<T>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let bonds = HistoricalBonds::<T>::iter().collect::<BTreeMap<EpochIndex, T::Amount>>();
		HistoricalActiveEpochs::<T>::translate(|_k, v: Vec<EpochIndex>| {
			Some(
				v.into_iter()
					.filter_map(|e| Some((e, *bonds.get(&e)?)))
					.collect::<Vec<(EpochIndex, T::Amount)>>(),
			)
		});
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use crate::CurrentAuthorities;
		use sp_std::collections::btree_set::BTreeSet;

		// There should be one entry per authority
		let accounts_in_historical_active_epochs = HistoricalActiveEpochs::<T>::iter()
			.map(|(account, _)| account)
			.collect::<BTreeSet<_>>();
		let authorities = CurrentAuthorities::<T>::get().into_iter().collect::<BTreeSet<_>>();
		assert_eq!(accounts_in_historical_active_epochs, authorities);
		Ok(())
	}
}
