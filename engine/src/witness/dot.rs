mod dot_chain_tracking;
mod dot_deposits;
mod dot_source;

use cf_chains::dot::{
	PolkadotAccountId, PolkadotBalance, PolkadotExtrinsicIndex, PolkadotHash, PolkadotSignature,
	PolkadotUncheckedExtrinsic,
};
use cf_primitives::{EpochIndex, PolkadotBlockNumber};
use futures_core::Future;
use state_chain_runtime::PolkadotInstance;
use subxt::{
	backend::legacy::rpc_methods::Bytes,
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
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
		STATE_CHAIN_CONNECTION,
	},
	witness::common::chain_source::extension::ChainSourceExt,
};
use anyhow::Result;
pub use dot_source::{DotFinalisedSource, DotUnfinalisedSource};

use super::common::{
	chain_source::Header,
	epoch_source::{EpochSourceBuilder, Vault},
};

// To generate the metadata file, use the subxt-cli tool (`cargo install subxt-cli`):
// subxt metadata --format=json --pallets Proxy,Balances,TransactionPayment,System --url
// wss://polkadot-rpc.dwellir.com:443 > metadata.polkadot.json.scale
#[subxt::subxt(runtime_metadata_path = "metadata.polkadot.scale")]
pub mod polkadot {}

#[derive(Debug, Clone)]
pub enum EventWrapper {
	ProxyAdded { delegator: AccountId32, delegatee: AccountId32 },
	Transfer { to: AccountId32, from: AccountId32, amount: PolkadotBalance },
	TransactionFeePaid { actual_fee: PolkadotBalance, tip: PolkadotBalance },
	ExtrinsicSuccess,
}

use polkadot::{
	balances::events::Transfer, proxy::events::ProxyAdded, system::events::ExtrinsicSuccess,
	transaction_payment::events::TransactionFeePaid,
};

pub fn filter_map_events(
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
			(ExtrinsicSuccess::PALLET, ExtrinsicSuccess::EVENT) => {
				let ExtrinsicSuccess { .. } =
					event_details.as_event::<ExtrinsicSuccess>().unwrap().unwrap();
				Some(EventWrapper::ExtrinsicSuccess)
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

pub async fn proxy_added_witnessing(
	epoch: Vault<cf_chains::Polkadot, PolkadotAccountId, ()>,
	header: Header<PolkadotBlockNumber, PolkadotHash, Vec<(Phase, EventWrapper)>>,
) -> (Vec<(Phase, EventWrapper)>, BTreeSet<u32>) {
	let events = header.data;
	let proxy_added_broadcasts = proxy_addeds(header.index, &events, &epoch.info.1);

	(events, proxy_added_broadcasts)
}

#[allow(clippy::type_complexity)]
pub async fn process_egress<ProcessCall, ProcessingFut>(
	epoch: Vault<cf_chains::Polkadot, PolkadotAccountId, ()>,
	header: Header<
		PolkadotBlockNumber,
		PolkadotHash,
		(
			(Vec<(Phase, EventWrapper)>, BTreeSet<u32>),
			Vec<(PolkadotSignature, PolkadotBlockNumber)>,
		),
	>,
	process_call: ProcessCall,
	dot_client: DotRetryRpcClient,
) where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let ((events, mut extrinsic_indices), monitored_egress_data) = header.data;

	let monitored_egress_ids = monitored_egress_data
		.into_iter()
		.map(|(signature, _)| signature)
		.collect::<BTreeSet<_>>();

	// To guarantee witnessing egress, we are interested in all extrinsics that were successful
	extrinsic_indices.extend(extrinsic_success_indices(&events));

	let extrinsics: Vec<Bytes> = dot_client.extrinsics(header.hash).await;

	for (extrinsic_index, tx_fee) in transaction_fee_paids(&extrinsic_indices, &events) {
		let xt = extrinsics.get(extrinsic_index as usize).expect(
			"We know this exists since we got
	this index from the event, from the block we are querying.",
		);
		let mut xt_bytes = xt.0.as_slice();

		match PolkadotUncheckedExtrinsic::decode(&mut xt_bytes) {
			Ok(unchecked) =>
				if let Some(signature) = unchecked.signature() {
					if monitored_egress_ids.contains(&signature) {
						tracing::info!(
							"Witnessing transaction_succeeded. signature: {signature:?}"
						);
						process_call(
							pallet_cf_broadcast::Call::<_, PolkadotInstance>::transaction_succeeded {
								tx_out_id: signature,
								signer_id: epoch.info.1,
								tx_fee,
								tx_metadata: (),
							}
							.into(),
							epoch.index,
						)
						.await;
					}
				},
			Err(error) => {
				// We expect this to occur when attempting to decode
				// a transaction that was not sent by us.
				// We can safely ignore it, but we log it in case.
				tracing::debug!("Failed to decode UncheckedExtrinsic {error}");
			},
		}
	}
}

pub async fn start<StateChainClient, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	dot_client: DotRetryRpcClient,
	process_call: ProcessCall,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: impl StreamApi<FINALIZED> + Clone,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let unfinalised_source = DotUnfinalisedSource::new(dot_client.clone())
		.strictly_monotonic()
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.shared(scope);

	unfinalised_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
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

	let vaults = epoch_source.vaults().await;

	// Full witnessing
	DotFinalisedSource::new(dot_client.clone())
		.strictly_monotonic()
		.logging("finalised block produced")
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.chunk_by_vault(vaults, scope)
		.deposit_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		// Deposit witnessing
		.dot_deposits(process_call.clone())
		// Proxy added witnessing
		.then(proxy_added_witnessing)
		// Broadcast success
		.egress_items(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		.then({
			let process_call = process_call.clone();
			let dot_client = dot_client.clone();
			move |epoch, header| {
				process_egress(epoch, header, process_call.clone(), dot_client.clone())
			}
		})
		.continuous("Polkadot".to_string(), db)
		.logging("witnessing")
		.spawn(scope);

	Ok(())
}

fn transaction_fee_paids(
	indices: &BTreeSet<PolkadotExtrinsicIndex>,
	events: &[(Phase, EventWrapper)],
) -> BTreeSet<(PolkadotExtrinsicIndex, PolkadotBalance)> {
	events
		.iter()
		.filter_map(|(phase, wrapped_event)| match (phase, wrapped_event) {
			(
				Phase::ApplyExtrinsic(extrinsic_index),
				EventWrapper::TransactionFeePaid { actual_fee, .. },
			) if indices.contains(extrinsic_index) => Some((*extrinsic_index, *actual_fee)),
			_ => None,
		})
		.collect()
}

fn extrinsic_success_indices(events: &[(Phase, EventWrapper)]) -> BTreeSet<PolkadotExtrinsicIndex> {
	events
		.iter()
		.filter_map(|(phase, wrapped_event)| match (phase, wrapped_event) {
			(Phase::ApplyExtrinsic(extrinsic_index), EventWrapper::ExtrinsicSuccess) =>
				Some(*extrinsic_index),
			_ => None,
		})
		.collect()
}

fn proxy_addeds(
	block_number: PolkadotBlockNumber,
	events: &Vec<(Phase, EventWrapper)>,
	our_vault: &PolkadotAccountId,
) -> BTreeSet<PolkadotExtrinsicIndex> {
	let mut extrinsic_indices = BTreeSet::new();
	for (phase, wrapped_event) in events {
		if let Phase::ApplyExtrinsic(extrinsic_index) = *phase {
			if let EventWrapper::ProxyAdded { delegator, delegatee } = wrapped_event {
				if &PolkadotAccountId::from_aliased(delegator.0) != our_vault {
					continue
				}

				tracing::info!("Witnessing ProxyAdded. new delegatee: {delegatee:?} at block number {block_number} and extrinsic_index; {extrinsic_index}");

				extrinsic_indices.insert(extrinsic_index);
			}
		}
	}
	extrinsic_indices
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

		let extrinsic_indices = proxy_addeds(20, &block_event_details, &our_vault);

		assert_eq!(extrinsic_indices.len(), 1);
		assert!(extrinsic_indices.contains(&our_proxy_added_index));
	}

	#[tokio::test]
	async fn test_extrinsic_success_filtering() {
		let events = phase_and_events(vec![
			(1u32, EventWrapper::ExtrinsicSuccess),
			(2u32, mock_tx_fee_paid(20000)),
			(2u32, EventWrapper::ExtrinsicSuccess),
			(3u32, mock_tx_fee_paid(20000)),
		]);

		assert_eq!(extrinsic_success_indices(&events), BTreeSet::from([1, 2]));
	}
}
