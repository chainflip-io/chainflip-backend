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

use super::vault::FetchedNativeFilter;
use crate::evm::rpc::address_checker::*;
use anyhow::ensure;
use cf_chains::evm::H256;
use ethers::types::{H160, U256};

use itertools::Itertools;
use std::collections::{BTreeMap, HashMap};

/// Calculates native asset ingresses from a combination of address states and `FetchedNative`
/// events.
///
/// `address_states` is provided only for channels with undeployed contracts. For these addresses:
/// - If no contract deployed: ingress = balance diff between current and previous block
/// - If contract just deployed this block: ingress = fetched amount - previous balance
///
/// For addresses not in `address_states` (i.e., channels with already deployed contracts), we rely
/// solely on `FetchedNative` events emitted by the Vault contract when funds are fetched.
///
/// Transaction hashes are returned for deposits after contract deployment, used for tracing the
/// end-to-end flow of funds (not for witnessing).
pub fn eth_ingresses_at_block(
	undeployed_address_states: HashMap<H160, (AddressState, AddressState)>,
	native_events: Vec<(FetchedNativeFilter, H256)>,
) -> Result<Vec<(H160, U256, Option<Vec<H256>>)>, anyhow::Error> {
	let fetched_native_totals: BTreeMap<_, _> = native_events
		.into_iter()
		.into_group_map_by(|(event, _tx_hash)| event.sender)
		.into_iter()
		.map(|(sender, events)| {
			// collect the tx_hashes here too.
			(
				sender,
				events.into_iter().fold(
					(U256::from(0), Vec::new()),
					|(total_fetched, mut tx_hashes), (event, tx_hash)| {
						tx_hashes.push(tx_hash);
						(total_fetched.saturating_add(event.amount), tx_hashes)
					},
				),
			)
		})
		.collect();

	let mut results = Vec::new();

	// Process addresses from address_states (may or may not have events)
	for (address, (previous_address_state, address_state)) in &undeployed_address_states {
		let (ingress_amount, tx_hashes) = if !address_state.has_contract {
			ensure!(!previous_address_state.has_contract);
			ensure!(!fetched_native_totals.contains_key(address));

			(address_state.balance.saturating_sub(previous_address_state.balance), None)
		} else {
			let fetched_native_total =
				fetched_native_totals.get(address).cloned().unwrap_or_default();

			if !previous_address_state.has_contract {
				(fetched_native_total.0.saturating_sub(previous_address_state.balance), None)
			} else {
				(fetched_native_total.0, Some(fetched_native_total.1))
			}
		};

		if !ingress_amount.is_zero() {
			results.push((*address, ingress_amount, tx_hashes));
		}
	}

	// Process events for addresses NOT in address_states
	for (address, (amount, tx_hashes)) in fetched_native_totals {
		if !undeployed_address_states.contains_key(&address) && !amount.is_zero() {
			results.push((address, amount, Some(tx_hashes)));
		}
	}

	Ok(results)
}

#[cfg(test)]
mod tests {
	use super::*;
	use ethers::prelude::U256;

	#[test]
	fn block_empty_lists() {
		let native_events = Default::default();

		let ingresses = eth_ingresses_at_block(Default::default(), native_events).unwrap();

		assert!(ingresses.is_empty());
	}

	#[test]
	fn test_eth_ingresses_at_block_no_contract() {
		let address = H160::random();
		let addresses = [(
			address,
			(
				AddressState { balance: U256::from(100), has_contract: false },
				AddressState { balance: U256::from(200), has_contract: false },
			),
		)]
		.into();

		// some random event should not be ignored, with the new logic if we pass an event to
		// eth_ingresses_at_block it will always be witnessed, we filter events against the
		// provided deposit channel beofre calling eth_ingresses_at_block
		let native_events = vec![(
			FetchedNativeFilter { sender: H160::random(), amount: U256::from(300) },
			H256::random(),
		)];

		let ingresses = eth_ingresses_at_block(addresses, native_events.clone()).unwrap();

		assert!(ingresses.contains(&(address, U256::from(100), None)));
		assert!(ingresses.contains(&(
			native_events[0].0.sender,
			U256::from(300),
			Some(vec![native_events[0].1])
		)));
	}

	#[test]
	fn test_eth_ingresses_at_block_when_contract_deployed() {
		let before_contract_deployed = U256::from(200);

		let addresses = [
			(
				H160::random(),
				(
					AddressState { balance: U256::from(200), has_contract: false },
					AddressState { balance: U256::from(0), has_contract: true },
				),
			),
			(
				H160::random(),
				(
					AddressState { balance: U256::from(0), has_contract: false },
					AddressState { balance: U256::from(0), has_contract: true },
				),
			),
		];

		// There were two events were emitted in the same Ethereum block
		let native_events = vec![
			(
				FetchedNativeFilter { sender: addresses[0].0, amount: before_contract_deployed },
				H256::random(),
			),
			(
				FetchedNativeFilter { sender: addresses[0].0, amount: U256::from(123) },
				H256::random(),
			),
			(
				FetchedNativeFilter { sender: addresses[1].0, amount: U256::from(212) },
				H256::random(),
			),
		];

		let ingresses =
			eth_ingresses_at_block(addresses.clone().into_iter().collect(), native_events).unwrap();

		// For both addresses, in the previous block there was no contract, therefore we expect that
		// any FetchedNative events are from the deployment of the contract, and therefore not a
		// tx_hash we care about.
		assert!(
			ingresses.contains(&(addresses[1].0, U256::from(212), None)) &&
				ingresses.contains(&(addresses[0].0, U256::from(123), None))
		);
	}

	#[test]
	fn test_eth_ingresses_at_block_with_contract() {
		let addresses = vec![
			(
				H160::random(),
				(
					AddressState { balance: U256::from(200), has_contract: true },
					AddressState { balance: U256::from(0), has_contract: true },
				),
			),
			(
				H160::random(),
				(
					AddressState { balance: U256::from(0), has_contract: true },
					AddressState { balance: U256::from(0), has_contract: true },
				),
			),
		];

		let tx_hashes = (0..=3).map(|_| H256::random()).collect::<Vec<_>>();

		// There were two events were emitted in the same Ethereum block
		let native_events = vec![
			(FetchedNativeFilter { sender: addresses[0].0, amount: U256::from(200) }, tx_hashes[0]),
			(FetchedNativeFilter { sender: addresses[0].0, amount: U256::from(123) }, tx_hashes[1]),
			(FetchedNativeFilter { sender: addresses[1].0, amount: U256::from(212) }, tx_hashes[2]),
		];

		let ingresses =
			eth_ingresses_at_block(addresses.clone().into_iter().collect(), native_events).unwrap();

		assert!(
			ingresses.contains(&(
				addresses[0].0,
				U256::from(323),
				Some(vec![tx_hashes[0], tx_hashes[1]])
			)) && ingresses.contains(&(addresses[1].0, U256::from(212), Some(vec![tx_hashes[2]])))
		);
	}
}
