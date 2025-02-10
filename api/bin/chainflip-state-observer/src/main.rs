
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
use elections::make_traces;
use pallet_cf_elections::electoral_system::{BitmapComponentOf, ElectionData};
use pallet_cf_elections::{ElectionDataFor, UniqueMonotonicIdentifier};
use state_chain_runtime::{Runtime, SolanaInstance};
use cf_utilities::task_scope;
use futures_util::FutureExt;
use chainflip_engine::state_chain_observer::client::storage_api::StorageApi;
use futures::{stream, StreamExt, TryStreamExt};
use pallet_cf_elections::{
	electoral_systems::composite::tuple_6_impls::*,
};
use trace::{diff, Trace};
use std::env;


#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	println!("Hello, world!");

	new_watch().await;

	/*
	task_scope::task_scope(|scope| async move { 

		// StateChainClient: ElectoralApi<Instance> + SignedExtrinsicApi + ChainApi,
		let (_, _, client) = StateChainClient::connect_without_account(scope, "ws://localhost:9944").await.unwrap();


		let block_hash = client.latest_finalized_block().hash;

		let bitmaps : BTreeMap<UniqueMonotonicIdentifier,
			_
			> = client
			.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
			.await
			.expect("could not get storage")
		;

		let bitmaps = bitmaps.into_iter()
			.map(|(k,v)| (k, v.bitmaps))
			.collect();

		let result : ElectionDataFor<Runtime, SolanaInstance> = ElectionData {
			bitmaps,
			_phantom: Default::default()
		};

		let traces = traces(result);

		println!("got election data: {traces:?}");

		Ok(())

	 }.boxed()).await.unwrap()

	 */

}

async fn new_watch() {

	task_scope::task_scope(|scope| async move { 

		let rpc_url = env::var("CF_RPC_NODE").expect("CF_RPC_NODE required");

		let (finalized_stream, _, client) = StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();

		let traces = BTreeMap::new();

		finalized_stream.fold((client, traces), async |(client, traces), block| {

			// let block_hash = client.latest_finalized_block().hash;
			let block_hash = block.hash;


			let bitmaps : BTreeMap<UniqueMonotonicIdentifier,
				_
				> = client
				.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage")
			;

			let bitmaps = bitmaps.into_iter()
				.map(|(k,v)| (k, v.bitmaps))
				.collect();

			let result : ElectionDataFor<Runtime, SolanaInstance> = ElectionData {
				bitmaps,
				_phantom: Default::default()
			};

			let new_traces = make_traces(result);
			// println!("got election data: {traces:?}");

			let δ = diff(traces, new_traces);
			let traces =
				δ.into_iter().filter_map(|(k,d)| match d {
						trace::NodeDiff::Left(x) => {println!("closing trace {k:?}"); None},
						trace::NodeDiff::Right(y) => {println!("open trace {k:?}"); Some((k, ()))},
						trace::NodeDiff::Both(x, _) => Some((k, x)),
					}).collect();

			// let all_properties : BTreeMap<_,_> = client
			// 	.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
			// 	.await
			// 	.expect("could not get storage");

			// let delta_properties : Vec<_> =
			// 	all_properties.iter().map(|(_, value)| match value {
			// 		pallet_cf_elections::electoral_systems::composite::tuple_6_impls::CompositeElectionProperties::C(props) => Some(props),
			// 		_ => None
			// 	})
			// 	.collect();

			// let all_state_map : BTreeMap<_,_> = client
			// 	.storage_map::<pallet_cf_elections::ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
			// 	.await
			// 	.expect("could not get storage");

			// let delta_state : BTreeMap<_,_> =
			// 	all_state_map.iter().filter_map(|(key, value)| match (key,value) {
			// 		(CompositeElectoralUnsynchronisedStateMapKey::C(key), CompositeElectoralUnsynchronisedStateMapValue::C(val))
			// 		=> Some((key,val)),
			// 		_ => None
			// 	})
			// 	.collect();

			// let block_height_state = client
			// 	.storage_value::<pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, SolanaInstance>>(block_hash)
			// 	.await
			// 	.expect("could not get storage")
			// 	.map(|(value, ..)| value)
			// 	.expect("could not get block height");

			(client, traces)

		}).await;

		Ok(())

	 }.boxed()).await.unwrap()
}



// async fn watch_stuck_solana_ingress() {

// 	task_scope::task_scope(|scope| async move { 

// 		// StateChainClient: ElectoralApi<Instance> + SignedExtrinsicApi + ChainApi,
// 		let (_, _, client) = StateChainClient::connect_without_account(scope, "ws://localhost:9944").await.unwrap();

// 		let block_hash = client.latest_finalized_block().hash;

// 		let all_properties : BTreeMap<_,_> = client
// 			.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
// 			.await
// 			.expect("could not get storage");

// 		let delta_properties : Vec<_> =
// 			all_properties.iter().map(|(_, value)| match value {
// 				pallet_cf_elections::electoral_systems::composite::tuple_6_impls::CompositeElectionProperties::B(props) => Some(props),
// 				_ => None
// 			})
// 			.collect();

// 		let block_height_properties : Vec<_> =
// 			all_properties.iter().filter_map(|(_, value)| match value {
// 				pallet_cf_elections::electoral_systems::composite::tuple_6_impls::CompositeElectionProperties::A(props) => Some(props),
// 				_ => None
// 			})
// 			.collect();

// 		for delta_prop in delta_properties {
// 			println!("delta: {delta_prop:?}");
// 		}






// 		/*
// 		let bitmaps : BTreeMap<UniqueMonotonicIdentifier, _ > = client
// 			.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
// 			.await
// 			.expect("could not get storage")
// 		;

// 		let bitmaps = bitmaps.into_iter()
// 			.map(|(k,v)| (k, v.bitmaps))
// 			.collect();

// 		let result : ElectionDataFor<Runtime, SolanaInstance> = ElectionData {
// 			bitmaps,
// 			_phantom: Default::default()
// 		};

// 		let traces = traces(result);

// 		println!("got election data: {traces:?}");

// 		*/

// 		Ok(())

// 	 }.boxed()).await.unwrap()
// }



