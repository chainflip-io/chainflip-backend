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

//! The boundary between alloy's Ethereum primitives, used by the EVM RPC clients and contract
//! bindings, and the sp-core primitives used everywhere else in the engine and on the State Chain.

use alloy_primitives::{Address, B256};
use sp_core::{H160, H256, U256};

pub trait IntoAlloy {
	type Output;
	fn into_alloy(self) -> Self::Output;
}

pub trait IntoSpCore {
	type Output;
	fn into_sp_core(self) -> Self::Output;
}

impl IntoAlloy for H160 {
	type Output = Address;
	fn into_alloy(self) -> Address {
		Address::from(self.0)
	}
}

impl IntoAlloy for H256 {
	type Output = B256;
	fn into_alloy(self) -> B256 {
		B256::from(self.0)
	}
}

impl IntoAlloy for U256 {
	type Output = alloy_primitives::U256;
	fn into_alloy(self) -> alloy_primitives::U256 {
		// Both store little-endian u64 limbs.
		alloy_primitives::U256::from_limbs(self.0)
	}
}

impl IntoSpCore for Address {
	type Output = H160;
	fn into_sp_core(self) -> H160 {
		H160(self.into_array())
	}
}

impl IntoSpCore for B256 {
	type Output = H256;
	fn into_sp_core(self) -> H256 {
		H256(self.0)
	}
}

impl IntoSpCore for alloy_primitives::U256 {
	type Output = U256;
	fn into_sp_core(self) -> U256 {
		U256(self.into_limbs())
	}
}

impl<T: IntoAlloy> IntoAlloy for Vec<T> {
	type Output = Vec<T::Output>;
	fn into_alloy(self) -> Self::Output {
		self.into_iter().map(IntoAlloy::into_alloy).collect()
	}
}

impl<T: IntoSpCore> IntoSpCore for Vec<T> {
	type Output = Vec<T::Output>;
	fn into_sp_core(self) -> Self::Output {
		self.into_iter().map(IntoSpCore::into_sp_core).collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn conversions_preserve_byte_order() {
		let bytes: [u8; 32] = core::array::from_fn(|i| i as u8 + 1);

		let u256 = U256::from_big_endian(&bytes);
		let alloy_u256 = u256.into_alloy();
		assert_eq!(alloy_u256, alloy_primitives::U256::from_be_bytes(bytes));
		assert_eq!(alloy_u256.into_sp_core(), u256);
		assert_eq!(U256::from(42u64).into_alloy(), alloy_primitives::U256::from(42u64));

		let h256 = H256(bytes);
		assert_eq!(h256.into_alloy(), B256::from(bytes));
		assert_eq!(h256.into_alloy().into_sp_core(), h256);

		let h160 = H160::from_slice(&bytes[..20]);
		assert_eq!(h160.into_alloy(), Address::from_slice(&bytes[..20]));
		assert_eq!(h160.into_alloy().into_sp_core(), h160);
	}
}
