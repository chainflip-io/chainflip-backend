
pub mod elections;
pub mod trace;

use std::collections::BTreeMap;

use bitvec::vec::BitVec;
use chainflip_engine::state_chain_observer::client::chain_api::ChainApi;
use chainflip_engine::witness::dot::polkadot::storage;
use codec::{Decode, Encode};
use chainflip_engine::state_chain_observer::client::{
	base_rpc_api::RawRpcApi, extrinsic_api::signed::SignedExtrinsicApi, BlockInfo,
	StateChainClient,
};
use chainflip_engine::state_chain_observer::client::base_rpc_api::BaseRpcClient;
use custom_rpc::CustomApiClient;
use elections::traces;
use pallet_cf_elections::electoral_system::{BitmapComponentOf, ElectionData};
use pallet_cf_elections::{ElectionDataFor, UniqueMonotonicIdentifier};
use state_chain_runtime::{Runtime, SolanaInstance};
use cf_utilities::task_scope;
use futures_util::FutureExt;
use chainflip_engine::state_chain_observer::client::storage_api::StorageApi;

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	println!("Hello, world!");


	task_scope::task_scope(|scope| async move { 

		// StateChainClient: ElectoralApi<Instance> + SignedExtrinsicApi + ChainApi,
		let (_, _, client) = StateChainClient::connect_without_account(scope, "ws://localhost:9944").await.unwrap();


		let block_hash = client.latest_finalized_block().hash;

			let bitmaps : BTreeMap<UniqueMonotonicIdentifier,
				// Vec<(BitmapComponentOf<_>, BitVec<u8, bitvec::order::Lsb0>)>,
				_
			 > = client
				.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage")
			//  = pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>::iter()
			;

			let bitmaps = bitmaps.into_iter()
				.map(|(k,v)| (k, v.bitmaps))
				.collect();

			let result : ElectionDataFor<Runtime, SolanaInstance> = ElectionData {
				bitmaps,
				_phantom: Default::default()
			};


		// let response : Vec<u8> = client
		// 		.base_rpc_client
		// 		.raw_rpc_client
		// 		.cf_solana_election_data(None).await
		// 		.expect("Could not get data (wrong format)");

		// let election_data = <ElectionDataFor<Runtime, SolanaInstance> as Decode>::decode(&mut &response[..]).expect("could not decode");

		let traces = traces(result);


		println!("got election data: {traces:?}");

		Ok(())

	 }.boxed()).await.unwrap()

}

