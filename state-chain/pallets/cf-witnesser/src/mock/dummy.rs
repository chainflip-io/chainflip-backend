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

#![cfg(test)]
#![cfg_attr(not(feature = "std"), no_std)]

/// Based on the substrate example template pallet
pub use pallet::*;

#[allow(dead_code)]
#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type EnsureWitnessed: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type Something<T> = StorageValue<_, u32>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Value Incremented [value]
		ValueIncrementedTo(u32),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Empty Storage
		NoneValue,
		/// Storage overflow while incrementing
		StorageOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// increments value, starting from 0
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn increment_value(origin: OriginFor<T>) -> DispatchResult {
			let _who = T::EnsureWitnessed::ensure_origin(origin)?;

			// Update storage.
			let new_val = match <Something<T>>::get() {
				// Set the value to 0 if the storage is currently empty.
				None => 0u32,
				// Increment the value read from storage; will error in the event of overflow.
				Some(old) => old.checked_add(1).ok_or(Error::<T>::StorageOverflow)?,
			};
			// Update the value in storage with the incremented result.
			<Something<T>>::put(new_val);
			// Emit an event.
			Self::deposit_event(Event::ValueIncrementedTo(new_val));

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn put_value(origin: OriginFor<T>, value: u32) -> DispatchResult {
			let _who = T::EnsureWitnessed::ensure_origin(origin)?;

			<Something<T>>::put(value);

			Ok(())
		}
	}
}
