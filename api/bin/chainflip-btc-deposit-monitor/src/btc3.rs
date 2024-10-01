//! Mempool monitoring via jsonrpc

use std::{sync::{Arc, Mutex}, time::{Duration, SystemTime}};
use crate::elliptic::EllipticClient;
use bitcoin::Transaction;
use cf_chains::btc::BitcoinNetwork;
use chainflip_api::{primitives::state_chain_runtime::System, settings::HttpBasicAuthEndpoint};
use chainflip_engine::{btc::rpc::{BtcRpcApi, BtcRpcClient, VerboseTransaction}, witness::common::epoch_source::Vault};
use tokio::time::sleep;

struct MonitoringState {
    addresses: Arc<Mutex<Vec<bitcoin::Address>>>,

    // for testing
    last_accepted: SystemTime,
}

impl MonitoringState {
    pub async fn update_monitoring_state(&mut self) {

    }

    pub fn is_relevant_address(&mut self, address: &bitcoin::Address) -> bool {
        let now = SystemTime::now();
        if now.duration_since(self.last_accepted).unwrap() > Duration::from_secs(10) {
            self.last_accepted = now;
            true
        } else {
            false
        }
    }
}

struct MempoolMonitor {
    elliptic_client: EllipticClient,
    monitoring_state: MonitoringState,

    monitored_txs: Arc<Mutex<Vec<(bitcoin::Txid, Vec<bitcoin::Address>, Vec<bitcoin::Txid>)>>>,
}

// async fn get_input_addresses(client: &BtcRpcClient, tx: &Transaction) -> Vec<bitcoin::Address> {
//     for input in &tx.input {
//         match client.get_raw_transactions(vec![input.previous_output.txid]).await {
//             Ok(input_txs) => {
//                 if input_txs.len() != 1 {
//                     println!("Found wrong number of transactions with id")
//                 }
//                 for input_tx in input_txs {
//                     let address = bitcoin::Address::from_script(input_tx.output, network)
//                 }
//             },
//             Err(e) => println!("error getting input for {}", tx.txid())
//         }
//         // for input_tx in client.get_raw_transactions(vec![input]).await {
//         //     let address = 
//         // }
//     }
//     Vec::new()
// }

async fn poll_addresses_to_monitor() -> Vec<bitcoin::Address> {
    Vec::new()
}

async fn poll_mempool(client: &BtcRpcClient) -> Vec<Transaction> {
    println!("getting mempool");
    let tx_ids = client.get_raw_mempool().await.unwrap();
    println!("Got: {}", tx_ids.len());
    let mut result = Vec::new();
    for tx_id in tx_ids.chunks_exact(5) {
        match client.get_raw_transactions(tx_id.to_vec()).await {
            Ok(mut e) => {
                result.append(&mut e);
            },
            Err(e) => {
                // println!("second rpc call error: {e}");
            }
        }
    }
    println!("Got txs: {}", result.len());
    result
}

impl MempoolMonitor {
}

pub async fn start_monitor(endpoint: HttpBasicAuthEndpoint, ) {

    let addresses = Arc::new(Mutex::new(Vec::new()));
    let monitored_txs = Arc::new(Mutex::new(Vec::new()));

    let elliptic_client = EllipticClient::new();

    let mut monitor = MempoolMonitor {
        monitoring_state: MonitoringState {
            addresses,
            last_accepted: SystemTime::UNIX_EPOCH
        },
        elliptic_client,
        monitored_txs,
    };

    println!("trying to connect to rpc node");
    let rpc_client = BtcRpcClient::new(endpoint, Some(BitcoinNetwork::Mainnet)).unwrap().await;


    let moved_monitored_txs : Arc<_> = monitor.monitored_txs.clone();

    tokio::task::spawn(async move {

        loop {
            println!("side sleep");
            sleep(Duration::from_secs(10)).await;
            println!("side sleep end");

            println!("Calling Elliptic...");
            let txs = moved_monitored_txs.lock().unwrap().clone();
            for (tx, addresses, in_hashes) in &*txs {
                println!("Calling elliptic for a transaction {tx:?} with relevant target address: {addresses:?}");

                let score = monitor.elliptic_client.welltyped_single_analysis(*tx, addresses[0].clone(), "test_customer_1".into()).await;
                // let score = monitor.elliptic_client.welltyped_single_wallet(addresses[0].clone(), "test_customer_1".into()).await;

                match score {
                    Ok(x) => println!("elliptic score: {}", x.risk_score),
                    Err(error) => println!("error: {error}"),
                }
            }
        }

    });

    loop {
        println!("inside main loop");

        // poll addresses to monitor
        // let addresses = poll_addresses_to_monitor().await;
        monitor.monitoring_state.update_monitoring_state().await;
        let txs = poll_mempool(&rpc_client).await;

        // find all transactions we are interested in
        let relevant_txs = txs.iter().filter_map(|tx| {
            let outs : Vec<_> = tx.output.iter().filter_map(|out| {
                if let Ok(address) = bitcoin::Address::from_script(&out.script_pubkey, bitcoin::Network::Bitcoin) {
                    if monitor.monitoring_state.is_relevant_address(&address) {
                        Some(address)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }).collect();
            if outs.len() > 0 {
                Some((tx, outs))
            } else {
                None
            }
        });

        // add them to the monitored txs
        {
            println!("main: trying to get lock.");
            let mut txs = monitor.monitored_txs.lock().unwrap();
            println!("main: got lock.");
            let mut new_txs : Vec<_> = relevant_txs.map(|(tx, outs)| {
                // let input_addresses = tx.input;
                let in_hashes = tx.input.iter().map(|i| i.previous_output.txid).collect::<Vec<_>>();
                (tx.txid(), outs, in_hashes)
            })
            .filter(|(txid,_,_)| txs.iter().find(|(txid2,_,_)| txid == txid2).is_none())
            .collect();
            txs.append(&mut new_txs);
        }

        // for all interesting transactions, call elliptic to get a score
        // for (tx, outs) in relevant_txs {
        //     println!("Found a transaction {tx:?} with relevant target address: {outs:?}");
        //     let score = monitor.elliptic_client.welltyped_single_analysis(tx.wtxid(), outs[0].clone(), "test_customer_1".into()).await;
        //     match score {
        //         Ok(x) => println!("elliptic score: {}", x.risk_score),
        //         Err(error) => println!("error: {error}"),
        //     }
        // }

        println!("main sleep");
        sleep(Duration::from_secs(5)).await;
        println!("main sleep end");
    }


}







