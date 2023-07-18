use std::{collections::HashMap, sync::Arc};

use cf_chains::btc::{ScriptPubkey, UtxoId};
use cf_primitives::chains::assets::btc;
use pallet_cf_ingress_egress::DepositWitness;
use secp256k1::hashes::Hash;
use state_chain_runtime::BitcoinInstance;
use utilities::task_scope::Scope;

use crate::{
	btc::{
		retry_rpc::{BtcRetryRpcApi, BtcRetryRpcClient},
		rpc::BtcRpcClient,
	},
	settings::{self},
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};

use super::{
	chain_source::{btc_source::BtcSource, extension::ChainSourceExt},
	epoch_source::EpochSource,
};

use anyhow::Result;

const SAFETY_MARGIN: usize = 6;

pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Btc,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSource<'_, '_, StateChainClient, (), ()>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi + 'static + Send + Sync,
{
	let btc_client = BtcRetryRpcClient::new(scope, BtcRpcClient::new(settings)?);

	let btc_source = BtcSource::new(btc_client.clone()).shared(scope);

	let btc_chain_tracking_witnesser = btc_source
		.clone()
		.chunk_by_time(epoch_source.clone())
		.await
		.chain_tracking(state_chain_client.clone(), btc_client.clone())
		.run();

	scope.spawn(async move {
		btc_chain_tracking_witnesser.await;
		Ok(())
	});

	let btc_safe_source = btc_source.lag_safety(SAFETY_MARGIN);

	let vaults = epoch_source.vaults().await;

	let btc_client = btc_client.clone();
	let btc_ingress_witnesser = btc_safe_source
		.then(move |header| {
			let btc_client = btc_client.clone();
			async move {
				let block = btc_client.block(header.hash).await;
				(header.data, block.txdata)
			}
		})
		.chunk_by_vault(vaults)
		.await
		.ingress_addresses(scope, state_chain_stream, state_chain_client.clone())
		.await
		.then(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			async move {
				// TODO: Make addresses a Map of some kind?
				let ((_prev_data, txs), addresses) = header.data;
				let script_addresses: HashMap<Vec<u8>, ScriptPubkey> =
					addresses.into_iter().map(|(address, _)| (address.bytes(), address)).collect();

				let mut deposit_witnesses = Vec::new();
				for tx in txs {
					let tx_hash = tx.txid().as_raw_hash().to_byte_array();
					for (vout, tx_out) in (0..).zip(tx.output) {
						if tx_out.value > 0 {
							let tx_script_pubkey_bytes = tx_out.script_pubkey.to_bytes();
							if let Some(bitcoin_script) =
								script_addresses.get(&tx_script_pubkey_bytes)
							{
								deposit_witnesses.push(DepositWitness {
									deposit_address: bitcoin_script.clone(),
									asset: btc::Asset::Btc,
									amount: tx_out.value,
									tx_id: UtxoId { tx_id: tx_hash, vout },
								});
							}
						}
					}
				}

				// Submit all deposit witnesses for the block.
				if !deposit_witnesses.is_empty() {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_ingress_egress::Call::<_, BitcoinInstance>::process_deposits {
									deposit_witnesses,
								}
								.into(),
							),
							epoch_index: epoch.index,
						})
						.await;
				}
			}
		})
		.run();

	scope.spawn(async move {
		btc_ingress_witnesser.await;
		Ok(())
	});

	Ok(())
}
