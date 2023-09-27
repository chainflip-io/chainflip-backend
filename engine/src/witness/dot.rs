mod dot_chain_tracking;
mod dot_deposits;
mod dot_source;

use cf_chains::dot::{
	PolkadotAccountId, PolkadotBalance, PolkadotExtrinsicIndex, PolkadotUncheckedExtrinsic,
};
use cf_primitives::{EpochIndex, PolkadotBlockNumber, TxId};
use futures_core::Future;
use state_chain_runtime::PolkadotInstance;
use subxt::{
	config::PolkadotConfig,
	events::{EventDetails, Phase, StaticEvent},
	utils::AccountId32,
};

use tracing::error;

use std::{collections::BTreeSet, sync::Arc};

use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	dot::retry_rpc::{DotRetryRpcApi, DotRetryRpcClient},
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

#[derive(Debug, Clone)]
pub enum EventWrapper {
	ProxyAdded { delegator: AccountId32, delegatee: AccountId32 },
	Transfer { to: AccountId32, from: AccountId32, amount: PolkadotBalance },
	TransactionFeePaid { actual_fee: PolkadotBalance, tip: PolkadotBalance },
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
			(ProxyAdded::PALLET, ProxyAdded::EVENT) => {
				let ProxyAdded { delegator, delegatee, .. } =
					event_details.as_event::<ProxyAdded>().unwrap().unwrap();
				Some(EventWrapper::ProxyAdded { delegator, delegatee })
			},
			(Transfer::PALLET, Transfer::EVENT) => {
				let Transfer { to, amount, from } =
					event_details.as_event::<Transfer>().unwrap().unwrap();
				Some(EventWrapper::Transfer { to, amount, from })
			},
			(TransactionFeePaid::PALLET, TransactionFeePaid::EVENT) => {
				let TransactionFeePaid { actual_fee, tip, .. } =
					event_details.as_event::<TransactionFeePaid>().unwrap().unwrap();
				Some(EventWrapper::TransactionFeePaid { actual_fee, tip })
			},
			_ => None,
		}
		.map(|event| (event_details.phase(), event)),
		Err(err) => {
			error!("Error while parsing event: {:?}", err);
			None
		},
	}
}

pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	dot_client: DotRetryRpcClient,
	process_call: ProcessCall,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi + Clone,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	DotUnfinalisedSource::new(dot_client.clone())
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.shared(scope)
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
		.strictly_monotonic()
		.logging("finalised block produced")
		.then(|header| async move {
			header.data.iter().filter_map(filter_map_events).collect::<Vec<_>>()
		})
		.shared(scope)
		.chunk_by_vault(epoch_source.vaults().await)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		// Deposit witnessing
		.dot_deposits(process_call.clone())
		// Proxy added witnessing
		.then({
			let process_call = process_call.clone();
			move |epoch, header| {
				let process_call = process_call.clone();
				async move {
					let (events, mut broadcast_indices) = header.data;

					let (vault_key_rotated_calls, mut proxy_added_broadcasts) = proxy_addeds(header.index, &events, &epoch.info.1);
					broadcast_indices.append(&mut proxy_added_broadcasts);

					for call in vault_key_rotated_calls {
						process_call(call, epoch.index).await;
					}

					(events, broadcast_indices)
				}
			}}
		)
		// Broadcast success
		.egress_items(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.then({
			let process_call = process_call.clone();
			let dot_client = dot_client.clone();
			move |epoch, header| {
				let process_call = process_call.clone();
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
									process_call(
										pallet_cf_broadcast::Call::<
											_,
											PolkadotInstance,
										>::transaction_succeeded {
											tx_out_id: signature,
											signer_id: epoch.info.1,
											tx_fee,
										}
										.into(),
										epoch.index,
									).await;
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

fn transaction_fee_paids(
	indices: &BTreeSet<PolkadotExtrinsicIndex>,
	events: &Vec<(Phase, EventWrapper)>,
) -> BTreeSet<(PolkadotExtrinsicIndex, PolkadotBalance)> {
	let mut indices_with_fees = BTreeSet::new();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(extrinsic_index) = phase {
			if indices.contains(extrinsic_index) {
				if let EventWrapper::TransactionFeePaid { actual_fee, .. } = wrapped_event {
					indices_with_fees.insert((*extrinsic_index, *actual_fee));
				}
			}
		}
	}
	indices_with_fees
}

fn proxy_addeds(
	block_number: PolkadotBlockNumber,
	events: &Vec<(Phase, EventWrapper)>,
	our_vault: &PolkadotAccountId,
) -> (Vec<state_chain_runtime::RuntimeCall>, BTreeSet<PolkadotExtrinsicIndex>) {
	let mut vault_key_rotated_calls = vec![];
	let mut extrinsic_indices = BTreeSet::new();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(extrinsic_index) = *phase {
			if let EventWrapper::ProxyAdded { delegator, delegatee } = wrapped_event {
				if &PolkadotAccountId::from_aliased(delegator.0) != our_vault {
					continue
				}

				tracing::info!("Witnessing ProxyAdded. new delegatee: {delegatee:?} at block number {block_number} and extrinsic_index; {extrinsic_index}");

				vault_key_rotated_calls.push(
					pallet_cf_vaults::Call::<_, PolkadotInstance>::vault_key_rotated {
						block_number,
						tx_id: TxId { block_number, extrinsic_index },
					}
					.into(),
				);

				extrinsic_indices.insert(extrinsic_index);
			}
		}
	}
	(vault_key_rotated_calls, extrinsic_indices)
}

#[cfg(test)]
pub mod test {
	use super::*;

	pub fn phase_and_events(
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
		EventWrapper::ProxyAdded {
			delegator: delegator.aliased_ref().to_owned().into(),
			delegatee: delegatee.aliased_ref().to_owned().into(),
		}
	}

	fn mock_tx_fee_paid(actual_fee: PolkadotBalance) -> EventWrapper {
		EventWrapper::TransactionFeePaid { actual_fee, tip: Default::default() }
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
