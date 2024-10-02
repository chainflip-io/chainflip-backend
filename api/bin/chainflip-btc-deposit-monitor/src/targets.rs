
use std::time::Duration;

use bitcoin::Transaction;
use cf_chains::btc::BitcoinNetwork;
use chainflip_api::settings::HttpBasicAuthEndpoint;
use chainflip_engine::btc::rpc::{BtcRpcApi, BtcRpcClient, VerboseBlock};
use futures::{stream, Stream};
use tokio::time::sleep;
use crate::monitor_provider::{Addresses, Transactions, monitor2};
use async_stream::stream;


pub async fn get_targets(default_targets: Addresses) -> impl Stream<Item=Addresses> {
    stream! {
        loop {
            yield default_targets.clone();
            
            println!("targets sleep");
            sleep(Duration::from_secs(10)).await;
            println!("targets sleep end");
        }
    }
}


pub async fn get_blocks() -> impl Stream<Item=Option<VerboseBlock>> {
    stream! {
        loop {
            if false {
                yield None
            }
            
            println!("block: sleeping");
            sleep(Duration::from_secs(10)).await;
        }
    }
}