use crate::{
	eth::retry_rpc::address_checker::*,
	state_chain_observer::client::{
		chain_api::ChainApi, extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
};
use cf_chains::Ethereum;
use cf_primitives::chains::assets::eth;
use ethers::types::Bloom;
use pallet_cf_ingress_egress::DepositChannelDetails;
use sp_core::H256;
use state_chain_runtime::EthereumInstance;
use std::sync::Arc;

use crate::witness::eth::vault::VaultEvents;

use std::collections::BTreeMap;

use ethers::prelude::*;
use itertools::Itertools;
use sp_core::U256;

use crate::eth::rpc::address_checker::*;

use super::{contract_common::events_at_block, vault::FetchedNativeFilter};
use crate::witness::common::chain_source::Header;

use super::super::common::{
	chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, ChunkedByVault},
	STATE_CHAIN_CONNECTION,
};
use crate::eth::retry_rpc::EthersRetryRpcApi;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn ethereum_deposits<StateChainClient, EthRetryRpcClient>(
		self,
		state_chain_client: Arc<StateChainClient>,
		eth_rpc: EthRetryRpcClient,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner: ChunkedByVault<
			Index = u64,
			Hash = H256,
			Data = (Bloom, Vec<DepositChannelDetails<Ethereum>>),
			Chain = Ethereum,
		>,
		StateChainClient: SignedExtrinsicApi + StorageApi + ChainApi + Send + Sync + 'static,
		EthRetryRpcClient: EthersRetryRpcApi + AddressCheckerRetryRpcApi + Send + Sync + Clone,
	{
		let address_checker_address = state_chain_client
			.storage_value::<pallet_cf_environment::EthereumAddressCheckerAddress<state_chain_runtime::Runtime>>(
				state_chain_client.latest_finalized_hash(),
			)
			.await
			.expect(STATE_CHAIN_CONNECTION);

		let vault_address = state_chain_client
			.storage_value::<pallet_cf_environment::EthereumVaultAddress<state_chain_runtime::Runtime>>(
				state_chain_client.latest_finalized_hash(),
			)
			.await
			.expect(STATE_CHAIN_CONNECTION);

		self.then(move |epoch, header| {
			let eth_rpc = eth_rpc.clone();
			let state_chain_client = state_chain_client.clone();
			async move {
				let (bloom, deposit_channels) = header.data;

				const NATIVE_ASSET: eth::Asset = eth::Asset::Eth;

				// Genesis block cannot contain any transactions
				if let Some(parent_hash) = header.parent_hash {
					if !deposit_channels.is_empty() {
						let addresses = deposit_channels
							.into_iter()
							.filter(|deposit_channel| {
								deposit_channel.deposit_channel.asset == NATIVE_ASSET
							})
							.map(|deposit_channel| deposit_channel.deposit_channel.address)
							.collect::<Vec<_>>();

						let previous_block_balances = eth_rpc
							.balances(parent_hash, address_checker_address, addresses.clone())
							.await;

						let address_states = eth_rpc
							.address_states(header.hash, address_checker_address, addresses.clone())
							.await;

						let ingresses = eth_ingresses_at_block(
							addresses,
							previous_block_balances,
							address_states,
							events_at_block::<VaultEvents, _>(
								Header {
									index: header.index,
									hash: header.hash,
									parent_hash: header.parent_hash,
									data: bloom,
								},
								vault_address,
								&eth_rpc,
							)
							.await?
							.into_iter()
							.filter_map(|event| match event.event_parameters {
								VaultEvents::FetchedNativeFilter(event) => Some(event),
								_ => None,
							})
							.collect(),
						);

						if !ingresses.is_empty() {
							state_chain_client
								.submit_signed_extrinsic(
									pallet_cf_witnesser::Call::witness_at_epoch {
										call:
											Box::new(
												pallet_cf_ingress_egress::Call::<
													_,
													EthereumInstance,
												>::process_deposits {
													deposit_witnesses: ingresses
														.into_iter()
														.map(|(to_addr, value)| {
															pallet_cf_ingress_egress::DepositWitness {
																deposit_address: to_addr,
																asset: NATIVE_ASSET,
																amount:
																	value
																	.try_into()
																	.expect("Ingress witness transfer value should fit u128"),
																deposit_details: (),
															}
														})
														.collect(),
													block_height: header.index,
												}
												.into(),
											),
										epoch_index: epoch.index,
									},
								)
								.await;
						}
					}
				}

				Ok::<_, anyhow::Error>(())
			}
		})
	}
}

pub fn eth_ingresses_at_block(
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
			(sender, events.into_iter().fold(U256::from(0), |acc, f| acc.saturating_add(f.amount)))
		})
		.collect();

	for ((address, address_state), previous_block_balance) in
		addresses.iter().zip(address_states).zip(previous_block_balances)
	{
		if address_state.has_contract {
			if let Some(amount) = fetched_native_totals.get(address) {
				let amount = *amount;
				if amount > U256::from(0) {
					ingresses_for_block.push((*address, amount));
				}
			}
		} else {
			assert!(fetched_native_totals.get(address).is_none());

			let balance_diff = address_state.balance.saturating_sub(previous_block_balance);

			if balance_diff > U256::from(0) {
				ingresses_for_block.push((*address, balance_diff));
			}
		}
	}
	ingresses_for_block
}

#[cfg(test)]
mod tests {
	use crate::{
		eth::{
			retry_rpc::{EthersRetryRpcApi, EthersRetryRpcClient},
			rpc::{EthRpcClient, ReconnectSubscriptionClient},
		},
		settings::Settings,
		witness::common::chain_source::Header,
	};

	use super::{super::contract_common::events_at_block, *};
	use ethers::prelude::U256;
	use futures_util::FutureExt;
	use utilities::{assert_panics, task_scope};

	use super::super::vault::VaultEvents;

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
			addresses.clone(),
			previous_block_balances,
			address_states,
			native_events,
		);

		assert!(ingresses.eq(&[(addresses[0], U256::from(100))]));
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
			addresses.clone(),
			previous_block_balances,
			address_states,
			native_events,
		);

		assert!(ingresses.eq(&[(addresses[0], U256::from(323)), (addresses[1], U256::from(212))]));
	}

	#[ignore = "requries connection to a node"]
	#[tokio::test]
	async fn test_get_ingress_contract() {
		task_scope::task_scope(|scope| {
			async {
				let vault_address: H160 =
					"B7A5bd0345EF1Cc5E66bf61BdeC17D2461fBd968".parse().unwrap();
				let address_checker_address =
					"e7f1725E7734CE288F8367e1Bb143E90bb3F0512".parse::<Address>().unwrap();

				let settings = Settings::new_test().unwrap();

				let client = EthersRetryRpcClient::new(
					scope,
					EthRpcClient::new(&settings.eth).await.unwrap(),
					ReconnectSubscriptionClient::new(
						settings.eth.ws_node_endpoint,
						web3::types::U256::from(1337),
					),
				);

				let addresses = vec![
					"41aD2bc63A2059f9b623533d87fe99887D794847".parse().unwrap(),
					"c2774b2f1972f50ac6113e81721cc7214388434d".parse().unwrap(),
				];

				let block_number = 138;
				let block = client.block(block_number.into()).await;

				let previous_block_balances = client
					.balances(block.parent_hash, address_checker_address, addresses.clone())
					.await;

				let address_states = client
					.address_states(block.hash.unwrap(), address_checker_address, addresses.clone())
					.await;

				let fetched_native_events = events_at_block::<VaultEvents, _>(
					Header {
						index: block_number,
						parent_hash: Some(block.parent_hash),
						hash: block.hash.unwrap(),
						data: block.logs_bloom.unwrap(),
					},
					vault_address,
					&client,
				)
				.await
				.unwrap()
				.into_iter()
				.filter_map(|event| match event.event_parameters {
					VaultEvents::FetchedNativeFilter(event) => Some(event),
					_ => None,
				})
				.collect();

				let increases = eth_ingresses_at_block(
					addresses,
					previous_block_balances,
					address_states,
					fetched_native_events,
				);

				for (address, increase) in increases {
					println!("{}: {}", address, increase);
				}

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
