#![feature(async_closure)]

use std::{collections::BTreeMap, time::Duration};
use tokio::fs;

use bitvec::vec::BitVec;
use chainflip_engine::{
	state_chain_observer::client::{
		base_rpc_api::{BaseRpcClient, RawRpcApi},
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		BlockInfo, StateChainClient,
	},
	witness::dot::polkadot::storage,
};
use codec::{Decode, Encode};
use custom_rpc::CustomApiClient;
// use elections::traces;
// use pallet_cf_elections::electoral_system::{BitmapComponentOf, ElectionData};
use cf_chains::sol::{SolAddress, SolAmount, SolAsset};
use cf_utilities::task_scope;
use chainflip_engine::state_chain_observer::client::storage_api::StorageApi;
use futures::{stream, StreamExt, TryStreamExt};
use futures_util::FutureExt;
use pallet_cf_elections::{
	electoral_systems::composite::tuple_6_impls::*, UniqueMonotonicIdentifier,
};
use serde::{Deserialize, Serialize};
use state_chain_runtime::{Runtime, SolanaInstance};
use std::env;
use tokio::time::sleep;
use anyhow::anyhow;

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	println!("Hello, world!");

	watch_stuck_solana_ingress().await;
}

// #[derive(Clone, Debug, Serialize, Deserialize)]
type NotifiedChannels = Vec<NotifiedChannel>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct NotifiedChannel {
	account: SolAddress,
	ingressed: SolAmount,
	witnessed: SolAmount,
	asset: SolAsset,
}

const FILENAME: &'static str = "/data/notified_channels.txt";

async fn watch_stuck_solana_ingress() {
	let health_url = env::var("HEALTH_URL").expect("HEALTH_URL required");

	tokio::spawn(async move {
		// Process each socket concurrently.
		loop {
			let body = reqwest::get(&health_url).await.unwrap().text().await.unwrap();

			sleep(Duration::from_secs(30)).await
		}
	});

	task_scope::task_scope(|scope| async move { 

		let rpc_url = env::var("CF_RPC_NODE").expect("CF_RPC_NODE required");
		let discord_url = env::var("DISCORD_URL").expect("DISCORD_URL required");

		let (finalized_stream, _, client) = StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();

		let notified_channels : Vec<NotifiedChannel> = match
        	fs::read_to_string(FILENAME)
			.await
			.map_err(|err| anyhow!("could not read from file {err}"))
			.and_then(|content| 
				serde_json::from_str(&content)
				.map_err(|err| anyhow!("could not deserialize: {err}"))
			) {
				Ok(x) => x,
				Err(e) => {
					println!("{e}");
					Vec::new()
				}
			};


		finalized_stream.fold((client, notified_channels), async |(client, mut notified_channels), block| {

			// let block_hash = client.latest_finalized_block().hash;
			let block_hash = block.hash;

			let all_properties : BTreeMap<_,_> = client
				.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage");

			let delta_properties : Vec<_> =
				all_properties.iter().map(|(_, value)| match value {
					pallet_cf_elections::electoral_systems::composite::tuple_6_impls::CompositeElectionProperties::C(props) => Some(props),
					_ => None
				})
				.collect();

			let all_state_map : BTreeMap<_,_> = client
				.storage_map::<pallet_cf_elections::ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage");

			let delta_state : BTreeMap<_,_> =
				all_state_map.iter().filter_map(|(key, value)| match (key,value) {
					(CompositeElectoralUnsynchronisedStateMapKey::C(key), CompositeElectoralUnsynchronisedStateMapValue::C(val))
					=> Some((key,val)),
					_ => None
				})
				.collect();

			let block_height_state = client
				.storage_value::<pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, SolanaInstance>>(block_hash)
				.await
				.expect("could not get storage")
				.map(|(value, ..)| value)
				.expect("could not get block height");

			for delta_prop_one in delta_properties {
				for delta_prop in delta_prop_one {

					for (account, (channel_details, total_consensus)) in delta_prop {
						let total_ingressed = delta_state.get(&(account.clone(), channel_details.asset)).map(|i| i.amount).unwrap_or(0);

						let min_amount = match channel_details.asset {
								SolAsset::Sol => 10_000_000,
								SolAsset::SolUsdc => 2_000_000,
							};

						if total_consensus.block_number < block_height_state && total_consensus.amount >= total_ingressed + min_amount {

							println!("account: {account:?}");
							println!("asset: {:?}", channel_details.asset);
							println!("ingressed value: {total_ingressed}, witnessed: {}", total_consensus.amount);

							let channel = NotifiedChannel {
								account: account.clone(),
								asset: channel_details.asset,
								ingressed: total_ingressed,
								witnessed: total_consensus.amount,
							};

							if !notified_channels.contains(&channel) {
								send_discord(&discord_url, &channel).await;
								notified_channels.push(channel);
								println!("new channel, notifying!")
							} else {
								println!("old channel, not notifying!")
							}
						}
					}
				}
			}

			// write to file
			let contents = serde_json::to_string_pretty(&notified_channels).unwrap();
			fs::write(FILENAME, contents).await.unwrap_or_else(|err| println!("could not write file! {err}"));

			println!("processed block!");

			(client, notified_channels)

		}).await;

		Ok(())

	 }.boxed()).await.unwrap()
}

#[derive(Serialize, Deserialize)]
struct DiscordMessage {
	embeds: Vec<DiscordEmbed>,
}

#[derive(Serialize, Deserialize)]
struct DiscordEmbed {
	title: String,
	description: String,
	color: u64,
}

async fn send_discord(url: &str, channel: &NotifiedChannel) {
	let description = format!("```\naccount: {}\nasset: {:?}\ningressed value: {}, witnessed: {}```", channel.account, channel.asset, channel.ingressed, channel.witnessed);
	
	let message = DiscordMessage {
		embeds: vec![DiscordEmbed {
			title: "🚨 Stuck Chainflip Swap Detected! 🚨".into(),
			description,
			color: 16724783,
		}],
	};

	match reqwest::Client::new().post(url).json(&message).send().await {
		Ok(_) => (),
		Err(err) => println!("failed to post to discord: {err}"),
	}
}
