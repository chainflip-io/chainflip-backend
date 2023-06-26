use std::collections::BTreeMap;

use super::ethers_rpc::*;
use ethers::prelude::*;
use itertools::Itertools;
use sp_core::U256;

#[allow(unused)]
fn eth_ingresses_at_block(
	addresses: Vec<H160>,
	previous_block_balances: Vec<U256>,
	address_states: Vec<AddressState>,
	native_events: Vec<FetchedNativeFilter>,
) -> Vec<(H160, U256)> {
	assert!(
		addresses.len() == address_states.len() &&
			address_states.len() == previous_block_balances.len()
	);

	let mut ingresses_for_block = vec![];
	let fetched_native_totals: BTreeMap<_, _> = native_events
		.into_iter()
		.into_group_map_by(|f| f.sender)
		.into_iter()
		.map(|(sender, events)| {
			(sender, events.into_iter().fold(U256::from(0), |acc, f| acc + f.amount))
		})
		.collect();

	for (i, (address, address_state)) in addresses.iter().zip(address_states).enumerate() {
		if address_state.has_contract {
			if let Some(amount) = fetched_native_totals.get(address) {
				let amount = *amount;
				if amount > U256::from(0) {
					ingresses_for_block.push((*address, amount));
				}
			}
		} else {
			let balance_prev_block = previous_block_balances.get(i).expect(
				"The contract will return the items in the same order as the input addresses",
			);

			let balance_diff = address_state.balance.saturating_sub(*balance_prev_block);

			if balance_diff > U256::from(0) {
				ingresses_for_block.push((*address, balance_diff));
			}
		}
	}
	ingresses_for_block
}

#[cfg(test)]
mod tests {
	use crate::settings::Settings;

	use super::*;
	use ethers::{abi::ethereum_types::BloomInput, prelude::U256};
	use utilities::assert_panics;

	#[test]
	fn block_empty_lists() {
		let addresses = vec![];
		let address_states = vec![];
		let previous_block_balances = vec![];
		let native_events = vec![];

		let ingresses = eth_ingresses_at_block(
			addresses,
			previous_block_balances,
			address_states,
			native_events,
		);

		assert!(ingresses.is_empty());
	}

	#[test]
	fn panics_on_unmatching_input_lengths() {
		assert_panics!(eth_ingresses_at_block(
			vec![],
			vec![],
			vec![AddressState { balance: U256::from(0), has_contract: false }],
			vec![],
		));

		assert_panics!(eth_ingresses_at_block(
			vec![],
			vec![U256::from(2)],
			vec![AddressState { balance: U256::from(0), has_contract: false }],
			vec![],
		));

		assert_panics!(eth_ingresses_at_block(vec![], vec![U256::from(2)], vec![], vec![],));

		assert_panics!(eth_ingresses_at_block(
			vec![H160::random()],
			vec![U256::from(2)],
			vec![],
			vec![],
		));

		assert_panics!(eth_ingresses_at_block(vec![H160::random()], vec![], vec![], vec![],));
	}

	#[test]
	fn test_eth_ingresses_at_block_no_contract() {
		let addresses = vec![H160::random()];
		let previous_block_balances = vec![U256::from(100)];
		let address_states = vec![AddressState { balance: U256::from(200), has_contract: false }];

		// some random event should not be ignored
		let native_events =
			vec![FetchedNativeFilter { sender: H160::random(), amount: U256::from(300) }];

		let ingresses = eth_ingresses_at_block(
			addresses,
			previous_block_balances,
			address_states,
			native_events,
		);

		assert_eq!(ingresses.len(), 1);
		assert_eq!(ingresses[0].1, U256::from(100)); // Balance increase should be 100
	}

	#[test]
	fn test_eth_ingresses_at_block_with_contract() {
		let addresses = vec![H160::random(), H160::random()];

		let before_contract_deployed = U256::from(200);
		let previous_block_balances = vec![before_contract_deployed, U256::from(0)];
		let address_states = vec![
			AddressState { balance: U256::from(0), has_contract: true },
			AddressState { balance: U256::from(0), has_contract: true },
		];

		// There were two events were emitted in the same Ethereum block
		let native_events = vec![
			FetchedNativeFilter { sender: addresses[0], amount: before_contract_deployed },
			FetchedNativeFilter { sender: addresses[0], amount: U256::from(123) },
			FetchedNativeFilter { sender: addresses[1], amount: U256::from(212) },
			// Not in our list of monitored addresses, so we don't witness it.
			FetchedNativeFilter { sender: H160::random(), amount: U256::from(420) },
		];

		let ingresses = eth_ingresses_at_block(
			addresses,
			previous_block_balances,
			address_states,
			native_events,
		);

		assert_eq!(ingresses.len(), 2);
		// We ignore the previous block balances since it is captured by the first event.
		assert_eq!(ingresses[0].1, U256::from(323)); // Balance increase should be 323, sum of the events
		assert_eq!(ingresses[1].1, U256::from(212));
	}

	#[ignore = "requries connection to a node"]
	#[tokio::test]
	async fn test_get_ingress_contract() {
		let vault_address = "B7A5bd0345EF1Cc5E66bf61BdeC17D2461fBd968".parse().unwrap();
		let address_checker_address =
			"e7f1725E7734CE288F8367e1Bb143E90bb3F0512".parse::<Address>().unwrap();

		let settings = Settings::new_test().unwrap();
		let client = EthersRpcClient::new(&settings.eth, vault_address, address_checker_address)
			.await
			.unwrap();

		let addresses = vec![
			"41aD2bc63A2059f9b623533d87fe99887D794847".parse().unwrap(),
			"c2774b2f1972f50ac6113e81721cc7214388434d".parse().unwrap(),
		];

		let block_number_observered = 138;

		let mut vault_bloom = Bloom::default();
		vault_bloom.accrue(BloomInput::Raw(&vault_address.0));

		let block = client.block(block_number_observered.into()).await.unwrap();
		let logs_bloom = block.logs_bloom.unwrap();

		let prev_block_hash: H256 =
			"332c938832eb7537f962a3d95de7e4064be0bb4d95d5a7caaa19daedbed3cca6"
				.parse()
				.unwrap();

		let block_hash: H256 = "332c938832eb7537f962a3d95de7e4064be0bb4d95d5a7caaa19daedbed3cca6"
			.parse()
			.unwrap();

		let previous_block_balances =
			client.balances(prev_block_hash, addresses.clone()).await.unwrap();

		let address_states = client.address_states(block_hash, addresses.clone()).await.unwrap();

		let fetched_native_events = if logs_bloom.contains_bloom(&vault_bloom) {
			client.fetched_native_events(block_hash).await.unwrap()
		} else {
			vec![]
		};

		let increases = eth_ingresses_at_block(
			addresses,
			previous_block_balances,
			address_states,
			fetched_native_events,
		);

		for (address, increase) in increases {
			println!("{}: {}", address, increase);
		}
	}
}
