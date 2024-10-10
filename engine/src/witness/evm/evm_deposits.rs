use crate::{
	evm::retry_rpc::address_checker::*,
	witness::common::{RuntimeCallHasChain, RuntimeHasChain},
};
use anyhow::ensure;
use cf_chains::{instances::ChainInstanceFor, Chain};
use cf_primitives::EpochIndex;
use ethers::types::Bloom;
use futures_core::Future;
use sp_core::H256;

use crate::witness::{
	common::chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses,
	evm::vault::VaultEvents,
};

use std::collections::BTreeMap;

use cf_chains::evm::DepositDetails;
use ethers::prelude::*;
use itertools::Itertools;
use sp_core::U256;

use crate::evm::rpc::address_checker::*;

use super::{contract_common::events_at_block, vault::FetchedNativeFilter};
use crate::witness::common::chain_source::Header;

use super::super::common::chunked_chain_source::chunked_by_vault::{
	builder::ChunkedByVaultBuilder, ChunkedByVault,
};
use crate::evm::retry_rpc::EvmRetryRpcApi;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	/// We track Ethereum deposits by checking the balance via our own deployed AddressChecker
	/// contract. This is to ensure we can detect deposits from:
	/// - A standard ETH transfer
	/// - A transfer made via a contract, which would not be detected by checking the `to` field in
	///   standard transfers since the `to` field would not be set
	/// We do *not* officially support ETH deposited using Ethereum/Solidity's self-destruct.
	/// See [below](`eth_ingresses_at_block`) for more details.
	pub async fn ethereum_deposits<ProcessCall, ProcessingFut, EvmRetryRpcClient>(
		self,
		process_call: ProcessCall,
		eth_rpc: EvmRetryRpcClient,
		native_asset: <Inner::Chain as cf_chains::Chain>::ChainAsset,
		address_checker_address: H160,
		vault_address: H160,
	) -> ChunkedByVaultBuilder<impl ChunkedByVault>
	where
		Inner::Chain: cf_chains::Chain<
			ChainAmount = u128,
			DepositDetails = DepositDetails,
			ChainAccount = H160,
		>,
		Inner: ChunkedByVault<Index = u64, Hash = H256, Data = (Bloom, Addresses<Inner>)>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
		EvmRetryRpcClient: EvmRetryRpcApi + AddressCheckerRetryRpcApi + Send + Sync + Clone,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.then(move |epoch, header| {
			assert!(<Inner::Chain as Chain>::is_block_witness_root(header.index));

			let eth_rpc = eth_rpc.clone();
			let process_call = process_call.clone();
			async move {
				let (bloom, deposit_channels) = header.data;

				// Genesis block cannot contain any transactions
				if let Some(parent_hash) = header.parent_hash {
					if !deposit_channels.is_empty() {
						let addresses = deposit_channels
							.into_iter()
							.filter(|deposit_channel| {
								deposit_channel.deposit_channel.asset == native_asset
							})
							.map(|deposit_channel| deposit_channel.deposit_channel.address)
							.collect::<Vec<_>>();

						let ingresses = eth_ingresses_at_block(
							address_states(
								&eth_rpc,
								address_checker_address,
								parent_hash,
								header.hash,
								addresses,
							)
							.await?,
							events_at_block::<Inner::Chain, VaultEvents, _>(
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
								VaultEvents::FetchedNativeFilter(inner_event) =>
									Some((inner_event, event.tx_hash)),
								_ => None,
							})
							.collect(),
						)?;

						if !ingresses.is_empty() {
							process_call(
								pallet_cf_ingress_egress::Call::<
									_,
									ChainInstanceFor<Inner::Chain>,
								>::process_deposits {
									deposit_witnesses: ingresses
										.into_iter()
										.map(|(to_addr, value, tx_hashes)| {
											pallet_cf_ingress_egress::DepositWitness {
												deposit_address: to_addr,
												asset: native_asset,
												amount:
													value
													.try_into()
													.expect("Ingress witness transfer value should fit u128"),
												deposit_details: DepositDetails {
													tx_hashes,
												}
											}
										})
										.collect(),
									block_height: header.index,
								}
								.into(),
								epoch.index,
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

async fn address_states<EvmRetryRpcClient>(
	eth_rpc: &EvmRetryRpcClient,
	address_checker_address: H160,
	parent_hash: H256,
	hash: H256,
	addresses: Vec<H160>,
) -> Result<impl Iterator<Item = (H160, (AddressState, AddressState))>, anyhow::Error>
where
	EvmRetryRpcClient: AddressCheckerRetryRpcApi + Send + Sync + Clone,
{
	let previous_address_states = eth_rpc
		.address_states(parent_hash, address_checker_address, addresses.clone())
		.await;

	let address_states =
		eth_rpc.address_states(hash, address_checker_address, addresses.clone()).await;

	ensure!(
		addresses.len() == previous_address_states.len() &&
			previous_address_states.len() == address_states.len()
	);

	Ok(addresses
		.into_iter()
		.zip(previous_address_states.into_iter().zip(address_states)))
}

/// To ensure we don't double witness deposits, we use the following pseudo-code, implemented by
/// `eth_ingresses_at_block`.
///
/// if !address.hasContract:
///    swap = address.balanceAtCurrentBlock - address.balanceAtPreviousBlock
///  else:
///    swap = (sum of amounts in the FetchedNative events for the particular sender) -
/// address.balanceAtPreviousBlock
///
/// We then do this on *every* block. This ensures we don't miss anything. See the tests below.
/// The `FetchedNative` events are emitted by the Vault contract when native asset funds are fetched
/// by the Deposit contract upon deployment or after it.
/// Note that when we have a contract deployed already we substrate the balance at the previous
/// block, since we will have already witnessed the deposits at the time the deposit was made.
/// We also return the transactions hashes of the transactions that caused the ingress, after the
/// contract is deployed. These transaction hashes are not important for witnessing, but are used
/// for tracing the end to end flow of the funds.
fn eth_ingresses_at_block<Addresses: IntoIterator<Item = (H160, (AddressState, AddressState))>>(
	addresses: Addresses,
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

	addresses
		.into_iter()
		.map(|(address, (previous_address_state, address_state))| {
			let (ingress_amount, tx_hashes) = if !address_state.has_contract {
				ensure!(!previous_address_state.has_contract);
				ensure!(fetched_native_totals.get(&address).is_none());

				(address_state.balance.saturating_sub(previous_address_state.balance), None)
			} else {
				let fetched_native_total =
					fetched_native_totals.get(&address).cloned().unwrap_or_default();

				if !previous_address_state.has_contract {
					(fetched_native_total.0.saturating_sub(previous_address_state.balance), None)
				} else {
					(fetched_native_total.0, Some(fetched_native_total.1))
				}
			};
			Ok((address, ingress_amount, tx_hashes))
		})
		.filter_ok(|(_address, ingress_amount, _tx_hashes)| !ingress_amount.is_zero())
		.collect::<Result<Vec<_>, _>>()
}

#[cfg(test)]
mod tests {
	use crate::{
		evm::{
			retry_rpc::{EvmRetryRpcApi, EvmRetryRpcClient},
			rpc::EvmRpcClient,
		},
		settings::Settings,
		witness::common::chain_source::Header,
	};

	use super::{super::contract_common::events_at_block, *};
	use cf_chains::{Chain, Ethereum};
	use ethers::prelude::U256;
	use futures_util::FutureExt;
	use utilities::task_scope;

	use super::super::vault::VaultEvents;

	#[test]
	fn block_empty_lists() {
		let addresses = [];
		let native_events = Default::default();

		let ingresses = eth_ingresses_at_block(addresses, native_events).unwrap();

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
		)];

		// some random event should not be ignored
		let native_events = vec![(
			FetchedNativeFilter { sender: H160::random(), amount: U256::from(300) },
			H256::random(),
		)];

		let ingresses = eth_ingresses_at_block(addresses, native_events).unwrap();

		assert!(ingresses.eq(&[(address, U256::from(100), None)]));
	}

	#[test]
	fn test_eth_ingresses_at_block_when_contract_deployed() {
		let before_contract_deployed = U256::from(200);

		let addresses = vec![
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
			// Not in our list of monitored addresses, so we don't witness it.
			(
				FetchedNativeFilter { sender: H160::random(), amount: U256::from(420) },
				H256::random(),
			),
		];

		let ingresses = eth_ingresses_at_block(addresses.clone(), native_events).unwrap();

		// For both addresses, in the previous block there was no contract, therefore we expect that
		// any FetchedNative events are from the deployment of the contract, and therefore not a
		// tx_hash we care about.
		assert!(ingresses.eq(&[
			(addresses[0].0, U256::from(123), None),
			(addresses[1].0, U256::from(212), None)
		]));
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
			// Not in our list of monitored addresses, so we don't witness it.
			(FetchedNativeFilter { sender: H160::random(), amount: U256::from(420) }, tx_hashes[3]),
		];

		let ingresses = eth_ingresses_at_block(addresses.clone(), native_events).unwrap();

		assert!(ingresses.eq(&[
			// NB: Here the amounts are the sum of the FetchedNative events, since the contract was
			// already deployed last block.
			(addresses[0].0, U256::from(323), Some(vec![tx_hashes[0], tx_hashes[1]])),
			(addresses[1].0, U256::from(212), Some(vec![tx_hashes[2]]))
		]));
	}

	#[ignore = "requires connection to a node"]
	#[tokio::test]
	async fn test_get_ingress_contract() {
		task_scope::task_scope(|scope| {
			async {
				let vault_address: H160 =
					"B7A5bd0345EF1Cc5E66bf61BdeC17D2461fBd968".parse().unwrap();
				let address_checker_address =
					"e7f1725E7734CE288F8367e1Bb143E90bb3F0512".parse::<Address>().unwrap();

				let settings = Settings::new_test().unwrap();
				let client = EvmRetryRpcClient::<EvmRpcClient>::new(
					scope,
					settings.eth.nodes,
					U256::from(1337u64),
					"eth_rpc",
					"eth_subscribe",
					"Ethereum",
					Ethereum::WITNESS_PERIOD,
				)
				.unwrap();

				let addresses = vec![
					"41aD2bc63A2059f9b623533d87fe99887D794847".parse().unwrap(),
					"c2774b2f1972f50ac6113e81721cc7214388434d".parse().unwrap(),
				];

				let block_number = 138;
				let block = client.block(block_number.into()).await;

				let address_states = address_states(
					&client,
					address_checker_address,
					block.parent_hash,
					block.hash.unwrap(),
					addresses.clone(),
				)
				.await
				.unwrap();

				let fetched_native_events = events_at_block::<cf_chains::Ethereum, VaultEvents, _>(
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
					VaultEvents::FetchedNativeFilter(inner_event) =>
						Some((inner_event, event.tx_hash)),
					_ => None,
				})
				.collect();

				let increases =
					eth_ingresses_at_block(address_states, fetched_native_events).unwrap();

				for (address, increase, tx_hashes) in increases {
					println!("{}: {}. Txs: {:?}", address, increase, tx_hashes);
				}

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
