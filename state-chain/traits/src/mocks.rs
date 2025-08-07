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

#![cfg(feature = "std")]

use codec::{Decode, Encode, EncodeLike};
use frame_support::{storage, StorageHasher, Twox64Concat};

pub mod account_role_registry;
pub mod address_converter;
pub mod affiliate_registry;
pub mod api_call;
pub mod asset_converter;
pub mod asset_withholding;
pub mod balance_api;
pub mod block_height_provider;
pub mod bonding;
pub mod broadcaster;
pub mod ccm_additional_data_handler;
pub mod ceremony_id_provider;
pub mod cfe_interface_mock;
pub mod chain_tracking;
pub mod deposit_handler;
pub mod deregistration_check;
pub mod egress_handler;
pub mod ensure_origin_mock;
pub mod epoch_info;
pub mod eth_environment_provider;
pub mod fee_payment;
pub mod fetches_transfers_limit_provider;
pub mod flip_burn_info;
pub mod funding_info;
pub mod ingress_egress_fee_handler;
pub mod key_provider;
pub mod key_rotator;
pub mod lending_pools;
pub mod liability_tracker;
pub mod offence_reporting;
pub mod on_account_funded;
pub mod pool_api;
pub mod pool_price_api;
pub mod price_feed_api;
pub mod qualify_node;
pub mod reputation_resetter;
pub mod safe_mode;
pub mod signer_nomination;
pub mod swap_parameter_validation;
pub mod swap_request_api;
pub mod threshold_signer;
pub mod time_source;
pub mod tracked_data_provider;
pub mod waived_fees_mock;

#[macro_export]
macro_rules! impl_mock_chainflip {
	($runtime:ty) => {
		use $crate::{
			impl_mock_epoch_info,
			mocks::{
				account_role_registry::MockAccountRoleRegistry,
				ensure_origin_mock::FailOnNoneOrigin, funding_info::MockFundingInfo,
			},
			Chainflip,
		};

		impl_mock_epoch_info!(
			<$runtime as frame_system::Config>::AccountId,
			u128,
			cf_primitives::EpochIndex,
			cf_primitives::AuthorityCount,
		);

		impl Chainflip for $runtime {
			type Amount = u128;
			type ValidatorId = <Self as frame_system::Config>::AccountId;
			type RuntimeCall = RuntimeCall;
			type EnsureWitnessed = FailOnNoneOrigin<Self>;
			type EnsurePrewitnessed = FailOnNoneOrigin<Self>;
			type EnsureWitnessedAtCurrentEpoch = FailOnNoneOrigin<Self>;
			type EnsureGovernance =
				frame_system::EnsureRoot<<Self as frame_system::Config>::AccountId>;
			type EpochInfo = MockEpochInfo;
			type AccountRoleRegistry = MockAccountRoleRegistry;
			type FundingInfo = MockFundingInfo<Self>;
		}
	};
}

trait MockPallet {
	const PREFIX: &'static [u8];
}

trait MockPalletStorage {
	fn put_storage<K: Encode, V: Encode>(store: &[u8], k: K, v: V);
	fn get_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V>;
	fn take_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V>;
	fn put_value<V: Encode>(store: &[u8], v: V) {
		Self::put_storage(store, (), v);
	}
	fn get_value<V: Decode + Sized>(store: &[u8]) -> Option<V> {
		Self::get_storage(store, ())
	}
	fn take_value<V: Decode + Sized>(store: &[u8]) -> Option<V> {
		Self::take_storage(store, ())
	}
	fn mutate_storage<
		K: Encode,
		E: EncodeLike<K>,
		V: Encode + Decode + Sized,
		R,
		F: FnOnce(&mut Option<V>) -> R,
	>(
		store: &[u8],
		k: &E,
		f: F,
	) -> R {
		let mut storage = Self::get_storage(store, k);
		let result = f(&mut storage);
		if let Some(v) = storage {
			Self::put_storage(store, k, v);
		}
		result
	}
	fn mutate_value<V: Encode + Decode + Sized, R, F: FnOnce(&mut Option<V>) -> R>(
		store: &[u8],
		f: F,
	) -> R {
		let mut storage = Self::get_value(store);
		let result = f(&mut storage);
		if let Some(v) = storage {
			Self::put_value(store, v);
		}
		result
	}
}

fn storage_key<K: Encode>(prefix: &[u8], store: &[u8], k: K) -> Vec<u8> {
	[prefix, store, &k.encode()].concat()
}

impl<T: MockPallet> MockPalletStorage for T {
	fn put_storage<K: Encode, V: Encode>(store: &[u8], k: K, v: V) {
		storage::hashed::put(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, k),
			&v,
		)
	}

	fn get_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V> {
		storage::hashed::get(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, k),
		)
	}

	fn take_storage<K: Encode, V: Decode + Sized>(store: &[u8], k: K) -> Option<V> {
		storage::hashed::take(
			&<Twox64Concat as StorageHasher>::hash,
			&storage_key(Self::PREFIX, store, k),
		)
	}
}
