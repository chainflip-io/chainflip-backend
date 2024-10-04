use bitcoin::params::Params;
use bitcoin::Transaction;
use cf_chains::Bitcoin;
use zmq::Message;
// use chainflip_engine::{btc::rpc::{BtcRpcClient, VerboseTransaction}, witness::common::epoch_source::Vault};
use core::str;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
// use bitcoin_zmq::ZMQListener;
use futures::prelude::*;
use bitcoincore_zmq::subscribe_receiver;
use bitcoincore_zmq::subscribe_async;
use futures::executor::block_on;
use futures_util::StreamExt;
use bitcoincore_zmq::Message::Tx;

use crate::elliptic::EllipticClient;


// bitcoin_endpoint = "tcp://*:8888"

struct MempoolMonitor {
    addresses: Arc<Mutex<Vec<bitcoin::Address>>>,
    params: Params,
    elliptic_client: EllipticClient,
}

pub async fn monitor_mempool(bitcoin_endpoint: String) {

    let mut stream = subscribe_async(&["tcp://127.0.0.1:28332"]).unwrap();
    
    let monitor = MempoolMonitor {
        addresses: Arc::new(Mutex::new(Vec::new())),
        params: todo!(),
        elliptic_client: todo!(),
    };

    while let Some(msg) = stream.next().await {
        match msg {
            Ok(msg) => {
                println!("Received message: {msg}");
                match msg {
                    Tx(tx, id) => {
                        println!("got tx: {tx:?}");
                        monitor.handle_new_transaction(&tx);
                    },
                    _ => ()
                }
            },
            Err(err) => println!("Error receiving message: {err}"),
        }
    }
}


impl MempoolMonitor {
    pub async fn handle_new_transaction(&self, tx: &Transaction) {
        let addresses = (*self.addresses).lock().unwrap();

        // check if we care about some of the outputs
        let outs = tx.output.iter().filter_map(|out| {
            let address = bitcoin::Address::from_script(&out.script_pubkey, self.params.clone()).unwrap();
            if let Some(a) = addresses.iter().find(|a| **a == address) {
                // check elliptic score
                let res = self.elliptic_client.welltyped_single_analysis(tx.compute_txid(), a, "test_customer_1".to_string()).await;

                // submit extrinsic to block channel if it's a bad transaction
                Some(a)
            } else {
                None
            }
        });

    }
}



