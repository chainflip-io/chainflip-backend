use cf_chains::{
	dot::{
		PolkadotAccountId, PolkadotBalance, PolkadotExtrinsicIndex, PolkadotProxyType,
		PolkadotUncheckedExtrinsic,
	},
	Polkadot,
};
use cf_primitives::{chains::assets, PolkadotBlockNumber, TxId};
use codec::{Decode, Encode};
use frame_support::scale_info::TypeInfo;
use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
use state_chain_runtime::PolkadotInstance;
use subxt::events::{EventDetails, Phase, StaticEvent};

use tracing::error;

use std::{collections::BTreeSet, sync::Arc};

use utilities::task_scope::Scope;

use crate::{
	dot::{
		http_rpc::DotHttpRpcClient,
		retry_rpc::{DotRetryRpcApi, DotRetryRpcClient},
		rpc::DotSubClient,
	},
	settings::{self},
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
	witness::chain_source::{dot_source::DotUnfinalisedSource, extension::ChainSourceExt},
};

use anyhow::Result;

use super::{
	chain_source::dot_source::DotFinalisedSource, common::STATE_CHAIN_CONNECTION,
	epoch_source::EpochSourceBuilder,
};

/// This event represents a rotation of the agg key. We have handed over control of the vault
/// to the new aggregrate at this event.
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct ProxyAdded {
	pub delegator: PolkadotAccountId,
	pub delegatee: PolkadotAccountId,
	pub proxy_type: PolkadotProxyType,
	pub delay: PolkadotBlockNumber,
}

impl StaticEvent for ProxyAdded {
	const PALLET: &'static str = "Proxy";
	const EVENT: &'static str = "ProxyAdded";
}

/// This event must match the Transfer event definition of the Polkadot chain.
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct Transfer {
	pub from: PolkadotAccountId,
	pub to: PolkadotAccountId,
	pub amount: PolkadotBalance,
}

impl StaticEvent for Transfer {
	const PALLET: &'static str = "Balances";
	const EVENT: &'static str = "Transfer";
}

/// This event must match the TransactionFeePaid event definition of the Polkadot chain.
#[derive(Debug, Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct TransactionFeePaid {
	pub who: PolkadotAccountId,
	// includes the tip
	pub actual_fee: PolkadotBalance,
	pub tip: PolkadotBalance,
}

impl StaticEvent for TransactionFeePaid {
	const PALLET: &'static str = "TransactionPayment";
	const EVENT: &'static str = "TransactionFeePaid";
}

#[derive(Clone, Debug)]
pub enum EventWrapper {
	ProxyAdded(ProxyAdded),
	Transfer(Transfer),
	TransactionFeePaid(TransactionFeePaid),
}

fn filter_map_events(
	res_event_details: Result<EventDetails, subxt::Error>,
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
	settings: &settings::Dot,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi + Clone,
{
	let dot_client = DotRetryRpcClient::new(
		scope,
		DotHttpRpcClient::new(&settings.http_node_endpoint).await?,
		DotSubClient::new(&settings.ws_node_endpoint),
	);

	let dot_chain_tracking = DotUnfinalisedSource::new(dot_client.clone())
		.shared(scope)
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.chunk_by_time(epoch_source.clone())
		.chain_tracking(state_chain_client.clone(), dot_client.clone())
		.run();

	scope.spawn(async move {
		dot_chain_tracking.await;
		Ok(())
	});

	let dot_ingress_witnessing = DotFinalisedSource::new(dot_client.clone())
		.shared(scope)
		.then(|header| async move {
			header.data.iter().filter_map(filter_map_events).collect::<Vec<_>>()
		})
		.chunk_by_vault(epoch_source.vaults().await)
		.ingress_addresses(scope, state_chain_stream.clone(), state_chain_client.clone())
		.await
		// Deposit witnessing
		.then({
			let state_chain_client = state_chain_client.clone();
			move |epoch, header| {
				let state_chain_client = state_chain_client.clone();
				async move {
					let (events, addresses_and_details) = header.data;

					let addresses = address_and_details_to_addresses(addresses_and_details);

					// TODO: Pass this through with the epoch
					let our_vault = state_chain_client
						.storage_value::<pallet_cf_environment::PolkadotVaultAccountId<state_chain_runtime::Runtime>>(
							epoch.block_hash,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION)
						.expect("If we got here, then we have a vault");

					let (deposit_witnesses, broadcast_indices) =
						deposit_witnesses(header.index, addresses, &events, &our_vault);

					if !deposit_witnesses.is_empty() {
						state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_ingress_egress::Call::<_, PolkadotInstance>::process_deposits {
									deposit_witnesses,
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

					// TODO: Pass this through with the epoch
					let our_vault = state_chain_client
						.storage_value::<pallet_cf_environment::PolkadotVaultAccountId<state_chain_runtime::Runtime>>(
							epoch.block_hash,
						)
						.await
						.expect(STATE_CHAIN_CONNECTION)
						.expect("If we got here, then we have a vault");

					let (vault_key_rotated_calls, mut proxy_added_broadcasts) = proxy_addeds(header.index, &events, &our_vault);
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

			// maybe don't need this clone
			let dot_client = dot_client.clone();
			move |epoch, header| {
				let state_chain_client = state_chain_client.clone();
				let dot_client = dot_client.clone();
				async move {
					let ((events, broadcast_indices), monitored_egress_ids) = header.data;

					// TODO: Pass this through with the epoch
					let our_vault = state_chain_client
					.storage_value::<pallet_cf_environment::PolkadotVaultAccountId<state_chain_runtime::Runtime>>(
						epoch.block_hash,
					)
					.await
					.expect(STATE_CHAIN_CONNECTION)
					.expect("If we got here, then we have a vault");

					let extrinsics = dot_client
						.extrinsics(header.hash)
						.await;

					for (extrinsic_index, tx_fee) in transaction_fee_paids(&broadcast_indices, &events) {
						let xt = extrinsics.get(extrinsic_index as usize).expect("We know this exists since we got this index from the event, from the block we are querying.");
						let xt_encoded = xt.0.encode();
						let mut xt_bytes = xt_encoded.as_slice();
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
															signer_id: our_vault,
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
		.run();

	scope.spawn(async move {
		dot_ingress_witnessing.await;
		Ok(())
	});

	Ok(())
}

fn address_and_details_to_addresses(
	address_and_details: Vec<DepositChannelDetails<Polkadot>>,
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
				if monitored_addresses.contains(to) {
					deposit_witnesses.push(DepositWitness {
						deposit_address: *to,
						asset: assets::dot::Asset::Dot,
						amount: *amount,
						tx_id: TxId { block_number, extrinsic_index: *extrinsic_index },
					});
				}

				if from == our_vault || to == our_vault {
					tracing::info!("Transfer from or to our_vault at block: {block_number}, extrinsic index: {extrinsic_index}");
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
			match wrapped_event {
				EventWrapper::ProxyAdded(ProxyAdded { delegator, delegatee, .. }) => {
					if delegator != our_vault {
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
				},
				_ => {},
			}
		}
	}
	(vault_key_rotated_calls, extrinsic_indices)
}

#[cfg(test)]
mod test {
	use cf_chains::dot::{PolkadotBalance, PolkadotExtrinsicIndex, PolkadotProxyType};

	use super::*;

	fn mock_transfer(
		from: &PolkadotAccountId,
		to: &PolkadotAccountId,
		amount: PolkadotBalance,
	) -> EventWrapper {
		EventWrapper::Transfer(Transfer { from: *from, to: *to, amount })
	}

	fn phase_and_events(
		events: &[(PolkadotExtrinsicIndex, EventWrapper)],
	) -> Vec<(Phase, EventWrapper)> {
		events
			.iter()
			.map(|(xt_index, event)| (Phase::ApplyExtrinsic(*xt_index), event.clone()))
			.collect()
	}

	fn mock_proxy_added(
		delegator: &PolkadotAccountId,
		delegatee: &PolkadotAccountId,
	) -> EventWrapper {
		EventWrapper::ProxyAdded(ProxyAdded {
			delegator: *delegator,
			delegatee: *delegatee,
			proxy_type: PolkadotProxyType::Any,
			delay: 0,
		})
	}

	fn mock_tx_fee_paid(actual_fee: PolkadotBalance) -> EventWrapper {
		EventWrapper::TransactionFeePaid(TransactionFeePaid {
			actual_fee,
			who: PolkadotAccountId::from_aliased([0xab; 32]),
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

		let block_event_details = phase_and_events(&[
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
		]);

		let (deposit_witnesses, broadcast_indices) = deposit_witnesses(
			32,
			vec![transfer_1_deposit_address, transfer_2_deposit_address],
			&block_event_details,
			&our_vault,
		);

		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses.get(0).unwrap().amount, TRANSFER_1_AMOUNT);
		assert_eq!(deposit_witnesses.get(1).unwrap().amount, TRANSFER_2_AMOUNT);

		// Check the egress and ingress fetch
		assert_eq!(broadcast_indices.len(), 2);
		assert!(broadcast_indices.contains(&TRANSFER_FROM_OUR_VAULT_INDEX));
		assert!(broadcast_indices.contains(&TRANFER_TO_OUR_VAULT_INDEX));
	}

	#[test]
	fn proxy_added_event_for_our_vault_witnessed() {
		let our_vault = PolkadotAccountId::from_aliased([0; 32]);
		let other_acct = PolkadotAccountId::from_aliased([1; 32]);
		let our_proxy_added_index = 1u32;
		let fee_paid = 10000;
		let block_event_details = phase_and_events(&[
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
