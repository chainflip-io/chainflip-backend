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

use super::{MockPallet, MockPalletStorage};
use crate::BrokerWithdrawalAddressRegistry;
use cf_chains::evm::Address as EthereumAddress;
use codec::{Decode, Encode};

pub struct MockBrokerWithdrawalAddressRegistry<AccountId>(core::marker::PhantomData<AccountId>);

impl<AccountId> MockPallet for MockBrokerWithdrawalAddressRegistry<AccountId> {
	const PREFIX: &'static [u8] = b"cf-mocks//BrokerWithdrawalAddressRegistry";
}

impl<AccountId: Encode + Decode + Clone> BrokerWithdrawalAddressRegistry
	for MockBrokerWithdrawalAddressRegistry<AccountId>
{
	type AccountId = AccountId;

	fn broker_withdrawal_address(owner: &Self::AccountId) -> Option<EthereumAddress> {
		Self::get_storage(b"Address", owner)
	}

	fn bind_broker_withdrawal_address(owner: &Self::AccountId, address: EthereumAddress) {
		Self::put_storage(b"Address", owner, address);
	}
}
