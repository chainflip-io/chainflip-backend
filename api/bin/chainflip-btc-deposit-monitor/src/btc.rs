

// struct BtcMonitor {
//     btc_client
// }

use cf_chains::{btc::BitcoinNetwork, Chain};
use chainflip_api::settings::{HttpBasicAuthEndpoint, NodeContainer};
use chainflip_engine::{btc::retry_rpc::{BtcRetryRpcApi, BtcRetryRpcClient }, witness::{btc::source::BtcSource, common::{chain_source::{extension::ChainSourceExt, shared::SharedSource}, chunked_chain_source::chunked_by_vault::{builder::ChunkedByVaultBuilder, deposit_addresses::Addresses, monitored_items::MonitoredSCItems, ChunkByVault}, epoch_source::VaultSource}}};
use utilities::task_scope::Scope;



// fn <Inner: ChunkedByVault> 





pub async fn start_monitor(
    scope: &Scope<'_, anyhow::Error>,
    nodes: NodeContainer<HttpBasicAuthEndpoint>,
) {

    let btc_client: BtcRetryRpcClient = BtcRetryRpcClient::new(scope, nodes, BitcoinNetwork::Mainnet).await.unwrap();

	let btc_source = BtcSource::new(btc_client.clone()).strictly_monotonic().shared(scope);

	let block_source = btc_source
		.then({
			let btc_client = btc_client.clone();
			move |header| {
				let btc_client = btc_client.clone();
				async move {
					let block = btc_client.block(header.hash).await;
					(header.data, block.txdata)
				}
			}
		});
		// .shared(scope);


    let chunked_vault_builder: ChunkedByVaultBuilder<ChunkByVault<_, _, _>>
        = todo!();

    type MyInner = ChunkByVault<SharedSource<_>,_,_>;

    fn with_addresses_builder() -> ChunkedByVaultBuilder<
		MonitoredSCItems<
			MyInner,
			Addresses<MyInner>,
			impl Fn(<MyInner::Chain as Chain>::ChainBlockNumber, &Addresses<MyInner>) -> Addresses<MyInner>
				+ Send
				+ Sync
				+ Clone
				+ 'static,
                >
                > {
                    todo!()
                }

    let res = with_addresses_builder()
        .btc_deposits(|a1, a2| {
            todo!()
        })
        .spawn(scope);
    // .then(move |epoch, header| {
	// 		async move {
	// 			// TODO: Make addresses a Map of some kind?
	// 			let (((), txs), addresses) = header.data;

	// 			let script_addresses = script_addresses(addresses);

	// 			let deposit_witnesses = deposit_witnesses(&txs, &script_addresses);

	// 			// Submit all deposit witnesses for the block.
	// 			if !deposit_witnesses.is_empty() {
	// 				process_call(
	// 					pallet_cf_ingress_egress::Call::<_, BitcoinInstance>::process_deposits {
	// 						deposit_witnesses,
	// 						block_height: header.index,
	// 					}
	// 					.into(),
	// 					epoch.index,
	// 				)
	// 				.await;
	// 			}
	// 			txs
	// 		}
    // });


    // let vaults: VaultSource<cf_chains::Bitcoin, _, _> = todo!();

	// Pre-witnessing stream.
	// block_source
	// 	.clone()
	// 	.chunk_by_vault(vaults.clone(), scope);
    // chunked_vault_builder
	// 	.deposit_addresses(scope, unfinalised_state_chain_stream, state_chain_client.clone())
		// .await
		// .btc_deposits(prewitness_call)
		// .logging("pre-witnessing")
		// .spawn(scope);
}

