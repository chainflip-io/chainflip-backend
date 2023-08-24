mod dot_chain_tracking;
mod dot_source;

use cf_chains::{
	dot::{PolkadotAccountId, PolkadotBalance, PolkadotExtrinsicIndex, PolkadotUncheckedExtrinsic},
	Polkadot,
};
use cf_primitives::{chains::assets, PolkadotBlockNumber, TxId};
use futures_core::Future;
use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
use state_chain_runtime::PolkadotInstance;
use subxt::{
	config::PolkadotConfig,
	events::{EventDetails, Phase, StaticEvent},
};

use tracing::error;

use std::{collections::BTreeSet, sync::Arc};

use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	dot::{
		http_rpc::DotHttpRpcClient,
		retry_rpc::{DotRetryRpcApi, DotRetryRpcClient},
		rpc::DotSubClient,
	},
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
	witness::common::chain_source::extension::ChainSourceExt,
};
use anyhow::Result;
use dot_source::{DotFinalisedSource, DotUnfinalisedSource};

use super::common::{epoch_source::EpochSourceBuilder, STATE_CHAIN_CONNECTION};

// To generate the metadata file, use the subxt-cli tool (`cargo install subxt-cli`):
// subxt metadata --format=json --pallets Proxy,Balances,TransactionPayment --url
// wss://polkadot-rpc.dwellir.com:443 > metadata.polkadot.json.scale
#[subxt::subxt(runtime_metadata_path = "metadata.polkadot.scale")]
pub mod polkadot {}

#[derive(Debug)]
pub enum EventWrapper {
	ProxyAdded(ProxyAdded),
	Transfer(Transfer),
	TransactionFeePaid(TransactionFeePaid),
}

use polkadot::{
	balances::events::Transfer, proxy::events::ProxyAdded,
	transaction_payment::events::TransactionFeePaid,
};

fn filter_map_events(
	res_event_details: Result<EventDetails<PolkadotConfig>, subxt::Error>,
) -> Option<(Phase, EventWrapper)> {
	match res_event_details {
		Ok(event_details) => match (event_details.pallet_name(), event_details.variant_name()) {
			(ProxyAdded::PALLET, ProxyAdded::EVENT) => Some(EventWrapper::ProxyAdded(
				event_details.as_event::<ProxyAdded>().unwrap().unwrap(),
			)),
			(Transfer::PALLET, Transfer::EVENT) =>
				Some(EventWrapper::Transfer(event_details.as_event::<Transfer>().unwrap().unwrap())),
			(TransactionFeePaid::PALLET, TransactionFeePaid::EVENT) =>
				Some(EventWrapper::TransactionFeePaid(
					event_details.as_event::<TransactionFeePaid>().unwrap().unwrap(),
				)),
			_ => None,
		}
		.map(|event| (event_details.phase(), event)),
		Err(err) => {
			error!("Error while parsing event: {:?}", err);
			None
		},
	}
}

pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	dot_client: DotRetryRpcClient<
		impl Future<Output = DotHttpRpcClient> + Send,
		impl Future<Output = DotSubClient> + Send,
	>,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi + Clone,
{
	DotUnfinalisedSource::new(dot_client.clone())
		.shared(scope)
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), dot_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	let epoch_source = epoch_source
		.filter_map(
			|state_chain_client, _epoch_index, hash, _info| async move {
				state_chain_client
					.storage_value::<pallet_cf_environment::PolkadotVaultAccountId<state_chain_runtime::Runtime>>(
						hash,
					)
					.await
					.expect(STATE_CHAIN_CONNECTION)
			},
			|_state_chain_client, _epoch, _block_hash, historic_info| async move { historic_info },
		)
		.await;

	DotFinalisedSource::new(dot_client.clone())
		.shared(scope)
		.strictly_monotonic()
		.logging("finalised block produced")
		.then(|header| async move {
			header.data.iter().filter_map(filter_map_events).collect::<Vec<_>>()
		})
		.chunk_by_vault(epoch_source.vaults().await)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		// Deposit witnessing
		.then({
			let state_chain_client = state_chain_client.clone();
			move |epoch, header| {
				let state_chain_client = state_chain_client.clone();
				async move {
					let (events, addresses_and_details) = header.data;

					let addresses = address_and_details_to_addresses(addresses_and_details);

					let (deposit_witnesses, broadcast_indices) =
						deposit_witnesses(header.index, addresses, &events, &epoch.info.1);

					if !deposit_witnesses.is_empty() {
						state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_ingress_egress::Call::<_, PolkadotInstance>::process_deposits {
									deposit_witnesses,
									block_height: header.index,
								}
								.into(),
							),
							epoch_index: epoch.index,
						})
						.await;
					}

					(events, broadcast_indices)
				}
			}
		})
		// Proxy added witnessing
		.then({
			let state_chain_client = state_chain_client.clone();
			move |epoch, header| {
				let state_chain_client = state_chain_client.clone();
				async move {
					let (events, mut broadcast_indices) = header.data;

					let (vault_key_rotated_calls, mut proxy_added_broadcasts) = proxy_addeds(header.index, &events, &epoch.info.1);
					broadcast_indices.append(&mut proxy_added_broadcasts);

					for call in vault_key_rotated_calls {
						state_chain_client
							.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
								call,
								epoch_index: epoch.index,
							})
							.await;
					}

					(events, broadcast_indices)
				}
			}}
		)
		// Broadcast success
		.egress_items(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.then({
			let state_chain_client = state_chain_client.clone();
			let dot_client = dot_client.clone();
			move |epoch, header| {
				let state_chain_client = state_chain_client.clone();
				let dot_client = dot_client.clone();
				async move {
					let ((events, broadcast_indices), monitored_egress_ids) = header.data;

					let extrinsics = dot_client
						.extrinsics(header.hash)
						.await;

					for (extrinsic_index, tx_fee) in transaction_fee_paids(&broadcast_indices, &events) {
						let xt = extrinsics.get(extrinsic_index as usize).expect("We know this exists since we got this index from the event, from the block we are querying.");
						let mut xt_bytes = xt.0.as_slice();

						let unchecked = PolkadotUncheckedExtrinsic::decode(&mut xt_bytes);
						if let Ok(unchecked) = unchecked {
							if let Some(signature) = unchecked.signature() {
								if monitored_egress_ids.contains(&signature) {
									tracing::info!("Witnessing transaction_succeeded. signature: {signature:?}");
									state_chain_client
										.submit_signed_extrinsic(
											pallet_cf_witnesser::Call::witness_at_epoch {
												call:
													Box::new(
														pallet_cf_broadcast::Call::<
															_,
															PolkadotInstance,
														>::transaction_succeeded {
															tx_out_id: signature,
															signer_id: epoch.info.1,
															tx_fee,
														}
														.into(),
													),
												epoch_index: epoch.index,
											},
										)
										.await;
								}
							}
						} else {
							// We expect this to occur when attempting to decode
							// a transaction that was not sent by us.
							// We can safely ignore it, but we log it in case.
							tracing::debug!("Failed to decode UncheckedExtrinsic {unchecked:?}");
						}
					}
				}
				}
			}
		)
		.continuous("Polkadot".to_string(), db)
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}

fn address_and_details_to_addresses(
	address_and_details: Vec<DepositChannelDetails<state_chain_runtime::Runtime, PolkadotInstance>>,
) -> Vec<PolkadotAccountId> {
	address_and_details
		.into_iter()
		.map(|deposit_channel_details| {
			assert_eq!(deposit_channel_details.deposit_channel.asset, assets::dot::Asset::Dot);
			deposit_channel_details.deposit_channel.address
		})
		.collect()
}

// Return the deposit witnesses and the extrinsic indices of transfers we want
// to confirm the broadcast of.
fn deposit_witnesses(
	block_number: PolkadotBlockNumber,
	monitored_addresses: Vec<PolkadotAccountId>,
	events: &Vec<(Phase, EventWrapper)>,
	our_vault: &PolkadotAccountId,
) -> (Vec<DepositWitness<Polkadot>>, BTreeSet<PolkadotExtrinsicIndex>) {
	let mut deposit_witnesses = vec![];
	let mut extrinsic_indices = BTreeSet::new();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(extrinsic_index) = phase {
			if let EventWrapper::Transfer(Transfer { to, amount, from }) = wrapped_event {
				let deposit_address = PolkadotAccountId::from_aliased(to.0);
				if monitored_addresses.contains(&deposit_address) {
					deposit_witnesses.push(DepositWitness {
						deposit_address,
						asset: assets::dot::Asset::Dot,
						amount: *amount,
						deposit_details: (),
					});
				}
				// It's possible a transfer to one of the monitored addresses comes from our_vault,
				// so this cannot be an else if
				if &PolkadotAccountId::from_aliased(from.0) == our_vault ||
					&deposit_address == our_vault
				{
					tracing::info!(
						"Interesting transfer at block: {block_number}, extrinsic index: {extrinsic_index} from: {from:?} to: {to:?}", 
					);
					extrinsic_indices.insert(*extrinsic_index);
				}
			}
		}
	}
	(deposit_witnesses, extrinsic_indices)
}

fn transaction_fee_paids(
	indices: &BTreeSet<PolkadotExtrinsicIndex>,
	events: &Vec<(Phase, EventWrapper)>,
) -> BTreeSet<(PolkadotExtrinsicIndex, PolkadotBalance)> {
	let mut indices_with_fees = BTreeSet::new();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(extrinsic_index) = phase {
			if indices.contains(extrinsic_index) {
				if let EventWrapper::TransactionFeePaid(TransactionFeePaid { actual_fee, .. }) =
					wrapped_event
				{
					indices_with_fees.insert((*extrinsic_index, *actual_fee));
				}
			}
		}
	}
	indices_with_fees
}

#[allow(clippy::vec_box)]
fn proxy_addeds(
	block_number: PolkadotBlockNumber,
	events: &Vec<(Phase, EventWrapper)>,
	our_vault: &PolkadotAccountId,
) -> (Vec<Box<state_chain_runtime::RuntimeCall>>, BTreeSet<PolkadotExtrinsicIndex>) {
	let mut vault_key_rotated_calls = vec![];
	let mut extrinsic_indices = BTreeSet::new();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(extrinsic_index) = *phase {
			if let EventWrapper::ProxyAdded(ProxyAdded { delegator, delegatee, .. }) = wrapped_event
			{
				if &PolkadotAccountId::from_aliased(delegator.0) != our_vault {
					continue
				}

				tracing::info!("Witnessing ProxyAdded. new delegatee: {delegatee:?} at block number {block_number} and extrinsic_index; {extrinsic_index}");

				vault_key_rotated_calls.push(Box::new(
					pallet_cf_vaults::Call::<_, PolkadotInstance>::vault_key_rotated {
						block_number,
						tx_id: TxId { block_number, extrinsic_index },
					}
					.into(),
				));

				extrinsic_indices.insert(extrinsic_index);
			}
		}
	}
	(vault_key_rotated_calls, extrinsic_indices)
}

#[cfg(test)]
mod test {

	use super::{polkadot::runtime_types::polkadot_runtime::ProxyType as PolkadotProxyType, *};

	fn mock_transfer(
		from: &PolkadotAccountId,
		to: &PolkadotAccountId,
		amount: PolkadotBalance,
	) -> EventWrapper {
		EventWrapper::Transfer(Transfer {
			from: from.aliased_ref().to_owned().into(),
			to: to.aliased_ref().to_owned().into(),
			amount,
		})
	}

	fn phase_and_events(
		events: Vec<(PolkadotExtrinsicIndex, EventWrapper)>,
	) -> Vec<(Phase, EventWrapper)> {
		events
			.into_iter()
			.map(|(xt_index, event)| (Phase::ApplyExtrinsic(xt_index), event))
			.collect()
	}

	fn mock_proxy_added(
		delegator: &PolkadotAccountId,
		delegatee: &PolkadotAccountId,
	) -> EventWrapper {
		EventWrapper::ProxyAdded(ProxyAdded {
			delegator: delegator.aliased_ref().to_owned().into(),
			delegatee: delegatee.aliased_ref().to_owned().into(),
			proxy_type: PolkadotProxyType::Any,
			delay: 0,
		})
	}

	fn mock_tx_fee_paid(actual_fee: PolkadotBalance) -> EventWrapper {
		EventWrapper::TransactionFeePaid(TransactionFeePaid {
			actual_fee,
			who: [0xab; 32].into(),
			tip: Default::default(),
		})
	}

	#[test]
	fn witness_deposits_for_addresses_we_monitor() {
		let our_vault = PolkadotAccountId::from_aliased([0; 32]);

		// we want two monitors, one sent through at start, and one sent through channel
		const TRANSFER_1_INDEX: u32 = 1;
		let transfer_1_deposit_address = PolkadotAccountId::from_aliased([1; 32]);
		const TRANSFER_1_AMOUNT: PolkadotBalance = 10000;

		const TRANSFER_2_INDEX: u32 = 2;
		let transfer_2_deposit_address = PolkadotAccountId::from_aliased([2; 32]);
		const TRANSFER_2_AMOUNT: PolkadotBalance = 20000;

		const TRANSFER_FROM_OUR_VAULT_INDEX: u32 = 7;
		const TRANFER_TO_OUR_VAULT_INDEX: u32 = 8;

		const TRANSFER_TO_SELF_INDEX: u32 = 9;
		const TRANSFER_TO_SELF_AMOUNT: PolkadotBalance = 30000;

		let block_event_details = phase_and_events(vec![
			// we'll be witnessing this from the start
			(
				TRANSFER_1_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_1_deposit_address,
					TRANSFER_1_AMOUNT,
				),
			),
			// we'll receive this address from the channel
			(
				TRANSFER_2_INDEX,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&transfer_2_deposit_address,
					TRANSFER_2_AMOUNT,
				),
			),
			// this one is not for us
			(
				19,
				mock_transfer(
					&PolkadotAccountId::from_aliased([7; 32]),
					&PolkadotAccountId::from_aliased([9; 32]),
					93232,
				),
			),
			(
				TRANSFER_FROM_OUR_VAULT_INDEX,
				mock_transfer(&our_vault, &PolkadotAccountId::from_aliased([9; 32]), 93232),
			),
			(
				TRANFER_TO_OUR_VAULT_INDEX,
				mock_transfer(&PolkadotAccountId::from_aliased([9; 32]), &our_vault, 93232),
			),
			// Example: Someone generates a DOT -> ETH swap, getting the DOT address that we're now
			// monitoring for inputs. They now generate a BTC -> DOT swap, and set the destination
			// address of the DOT to the address they generated earlier.
			// Now our Polakdot vault is sending to an address we're monitoring for deposits.
			(
				TRANSFER_TO_SELF_INDEX,
				mock_transfer(&our_vault, &transfer_2_deposit_address, TRANSFER_TO_SELF_AMOUNT),
			),
		]);

		let (deposit_witnesses, broadcast_indices) = deposit_witnesses(
			32,
			vec![transfer_1_deposit_address, transfer_2_deposit_address],
			&block_event_details,
			&our_vault,
		);

		assert_eq!(deposit_witnesses.len(), 3);
		assert_eq!(deposit_witnesses.get(0).unwrap().amount, TRANSFER_1_AMOUNT);
		assert_eq!(deposit_witnesses.get(1).unwrap().amount, TRANSFER_2_AMOUNT);
		assert_eq!(deposit_witnesses.get(2).unwrap().amount, TRANSFER_TO_SELF_AMOUNT);

		// Check the egress and ingress fetch
		assert_eq!(broadcast_indices.len(), 3);
		assert!(broadcast_indices.contains(&TRANSFER_FROM_OUR_VAULT_INDEX));
		assert!(broadcast_indices.contains(&TRANFER_TO_OUR_VAULT_INDEX));
		assert!(broadcast_indices.contains(&TRANSFER_TO_SELF_INDEX));
	}

	#[test]
	fn proxy_added_event_for_our_vault_witnessed() {
		let our_vault = PolkadotAccountId::from_aliased([0; 32]);
		let other_acct = PolkadotAccountId::from_aliased([1; 32]);
		let our_proxy_added_index = 1u32;
		let fee_paid = 10000;
		let block_event_details = phase_and_events(vec![
			// we should witness this one
			(our_proxy_added_index, mock_proxy_added(&our_vault, &other_acct)),
			(our_proxy_added_index, mock_tx_fee_paid(fee_paid)),
			// we should not witness this one
			(3u32, mock_proxy_added(&other_acct, &our_vault)),
			(3u32, mock_tx_fee_paid(20000)),
		]);

		let (vault_key_rotated_calls, broadcast_indices) =
			proxy_addeds(20, &block_event_details, &our_vault);

		assert_eq!(vault_key_rotated_calls.len(), 1);
		assert_eq!(broadcast_indices.len(), 1);
		assert!(broadcast_indices.contains(&our_proxy_added_index));
	}
}
