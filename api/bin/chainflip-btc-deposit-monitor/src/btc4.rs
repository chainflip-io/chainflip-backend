use std::{collections::BTreeSet, time::Duration};

use bitcoin::{Transaction, Txid};
use cf_chains::btc::BitcoinNetwork;
use chainflip_api::settings::HttpBasicAuthEndpoint;
use chainflip_engine::btc::rpc::{BtcRpcApi, BtcRpcClient};
use futures::{future::join_all, stream, FutureExt, Stream, TryFutureExt};
use tokio::time::sleep;
use crate::{elliptic::EllipticClient, monitor_provider::{monitor2, Addresses, AnalysisResult, Transactions, TransactionsUpdate}, targets::{get_blocks, get_targets}};
use async_stream::stream;


pub async fn get_mempool(endpoint: HttpBasicAuthEndpoint) -> impl Stream<Item=TransactionsUpdate> {
    let rpc_client = BtcRpcClient::new(endpoint, Some(BitcoinNetwork::Mainnet)).unwrap().await;

    stream::unfold((MempoolState::new(), rpc_client), |(mut mempool, rpc_client)| async move {
        println!("mempool sleep");
        sleep(Duration::from_secs(4)).await;
        println!("mempool sleep end");
        
        let transactions = mempool.get_new_transactions(&rpc_client).await;
        Some((transactions, (mempool, rpc_client)))
    })
}


/// Naive access to the mempool
// async fn poll_mempool(client: &BtcRpcClient) -> Vec<Transaction> {
//     println!("getting mempool");
//     let tx_ids = client.get_raw_mempool().await.unwrap();
//     println!("Got: {}", tx_ids.len());
//     let mut result = Vec::new();
//     for tx_id in tx_ids.chunks_exact(1000) {
//         match client.get_raw_transactions(tx_id.to_vec()).await {
//             Ok(mut e) => {
//                 result.append(&mut e);
//             },
//             Err(e) => {
//                 // println!("second rpc call error: {e}");
//             }
//         }
//     }
//     println!("Got txs: {}", result.len());
//     result
// }

async fn get_bunched_raw_transactions(client: &BtcRpcClient, tx_ids: Vec<Txid>) -> (Vec<Transaction>, BTreeSet<Txid>) {
    let mut failed_ids = BTreeSet::from_iter(tx_ids.iter().map(Clone::clone));
    let mut result = Vec::new();
    let mut num_calls = 0;
    let mut num_errs = 0;
    let chunk_size = tx_ids.len() / 100;
    println!("mempool (getting tx data): requesting for {} txs, chunksize: {chunk_size}", tx_ids.len());

    // let results = join_all(tx_ids.chunks(chunk_size).map(|tx_ids| client.get_raw_transactions(tx_ids.into()).map(move |res| (tx_ids, res)))).await;

    let results = match client.get_raw_transactions(tx_ids.clone()).await {
        Ok(txs) => txs,
        Err(e) => {
            println!("Error while getting transactions from mempool: {e}");
            Vec::new()
        },
    };

    for tx in results {
        num_calls += 1;
        match tx {
            Ok(tx) => {
                failed_ids.remove(&tx.txid());
                result.push(tx);
            },
            Err(e) => {
                num_errs += 1;
            }
        }
    }

    // for tx_id in tx_ids.chunks(chunk_size) {
    //     num_calls += 1;
    //     match client.get_raw_transactions(tx_id.to_vec()).await {
    //         Ok(mut e) => {
    //             result.append(&mut e);
    //         },
    //         Err(e) => {
    //             num_errs += 1;

    //             // remember failed ids, because these are no longer in the mempool,
    //             // probably
    //             for id in tx_id {
    //                 failed_ids.insert(id.clone());
    //             }
    //         }
    //     }
    // }
    println!("mempool (getting tx data): got transaction data for {} of {} transactions. {num_errs} of {num_calls} calls failed", result.len(), tx_ids.len());
    (result, failed_ids)
}

/// Access mempool by only requesting info for new items
struct MempoolState {
    tx_ids: BTreeSet<Txid>
}
impl MempoolState {

    pub fn new() -> Self {
        MempoolState {
            tx_ids: BTreeSet::new(),
        }
    }

    pub async fn get_new_transactions(&mut self, client: &BtcRpcClient) -> TransactionsUpdate {
        let new_tx_ids : BTreeSet<Txid> = 
            BTreeSet::from_iter(client.get_raw_mempool().await.unwrap().into_iter());


        let removed_tx_ids : Vec<_> = self.tx_ids.difference(&new_tx_ids).map(Clone::clone).collect();
        let added_tx_ids = new_tx_ids.difference(&self.tx_ids);

        println!("mempool: currently contains {} txs. Of these, {} txs are new, and {} have been removed since the last update.", new_tx_ids.len(), added_tx_ids.clone().count(), removed_tx_ids.len());

        let (tx, failed_tx_ids) = get_bunched_raw_transactions(client, added_tx_ids.map(|a| a.clone()).collect()).await;
        let actually_new_tx_ids = new_tx_ids.difference(&failed_tx_ids);

        // set state to actually successfully processed ids
        self.tx_ids = actually_new_tx_ids.map(|a| a.clone()).collect();

        println!("mempool: successfully processed {} new txs, active pool size: {}.", tx.len(), new_tx_ids.len());

        // get_bunched_raw_transactions(client, new_tx_ids).await

        TransactionsUpdate {
            added: tx,
            removed: removed_tx_ids,
        }
    }
}


pub async fn call_monitor(endpoint: HttpBasicAuthEndpoint, addresses: Addresses) {

    monitor2(get_targets(addresses).await, get_mempool(endpoint).await, get_blocks().await, EllipticClient::new()).await;
}

