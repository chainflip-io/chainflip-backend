use std::time::Duration;

use crate::monitor_provider::{monitor2, Addresses, Transactions};
use async_stream::stream;
use bitcoin::{block, Transaction};
use cf_chains::btc::BitcoinNetwork;
use chainflip_api::settings::HttpBasicAuthEndpoint;
use chainflip_engine::btc::rpc::{BtcRpcApi, BtcRpcClient, VerboseBlock};
use futures::{stream, Stream};
use tokio::time::sleep;

pub async fn get_targets(default_targets: Addresses) -> impl Stream<Item = Addresses> {
	stream! {
		loop {
			yield default_targets.clone();

			sleep(Duration::from_secs(10)).await;
		}
	}
}

pub async fn get_blocks(endpoint: HttpBasicAuthEndpoint) -> impl Stream<Item = VerboseBlock> {
	let rpc_client = BtcRpcClient::new(endpoint, Some(BitcoinNetwork::Mainnet)).unwrap().await;

	stream::unfold((None, rpc_client), |(blockhash, rpc_client)| async move {
		let mut current_blockhash = blockhash.clone();
		while current_blockhash == blockhash {
			println!("getblocks: current block is: {:?}", blockhash);
			sleep(Duration::from_secs(15)).await;

			current_blockhash = match rpc_client.best_block_hash().await {
				Ok(h) => Some(h),
				Err(e) => {
					println!("getblocks: Could not get best_block_hash: {e}");
					current_blockhash
				},
			};
		}

		println!("getblocks: found new block: {}", current_blockhash.unwrap());

		let block = rpc_client
			.block(current_blockhash.unwrap())
			.await
			.expect("getblocks: could not get blockinfo");

		Some((block, (current_blockhash, rpc_client)))
	})
}

// pub async fn get_blocks() -> impl Stream<Item=Option<VerboseBlock>> {
//     stream! {
//         loop {
//             if false {
//                 yield None
//             }

//             println!("block: sleeping");
//             sleep(Duration::from_secs(10)).await;
//         }
//     }
// }
