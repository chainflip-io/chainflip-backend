// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![feature(iterator_try_collect)]

use anyhow::Context;
use cf_utilities::{dynamic_events::EventDecoder, scale_json::ext::JsonValue};
use env_logger::Env;
use std::str::FromStr;
use subxt::{utils::H256, OnlineClientAtBlock, SubstrateConfig};

const HELP: &str = r#"
Usage: scale-json-event-logger <network> <latest|follow|0xhash> [path...]

network [required]: The Chainflip Network to connect to.
	Use 'l' or 'local' for ws://localhost:9944.
	Use 'b' or 'm' or 'berghain' or 'mainnet' for wss://mainnet-archive.chainflip.io.
	Use 'p' or 'persa' or 'perseverance' for wss://archive.perseverance.chainflip.io.
	Use 's' or 'sisy' or 'sisyphos' for wss://archive.sisyphos.chainflip.io.
	Any other string will be interpreted as a url.

latest|follow|0xhash [required]: Fetch events for the latest block, subscribe to new blocks and print events, or fetch events for a specific block hash.

path [optional]: JSON pointer paths to filter the decoded events.
	A path looks like: /event/LiquidityPools
	If any path matches the decoded event, the full event is printed.
	If none are provided, all decoded events are printed.

	DISCLAIMER: This is not production-grade code. In particular, events in a runtime upgrade block
	are decoded with the metadata of the new runtime, which might not match their encoding.
"#;

/// This valid across all Substrate chains using FRAME, ie. any chain that stores its events in
/// System::Events.
const EVENTS_STORAGE_KEY: [u8; 32] =
	hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7");

enum HashOption {
	/// Fetch events for the latest block.
	Latest,
	/// Subscribe to blocks and print events for each.
	Follow,
	/// Fetch events for a specific block hash.
	Hash(H256),
}

impl FromStr for HashOption {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"latest" => Ok(HashOption::Latest),
			"follow" => Ok(HashOption::Follow),
			_ => Ok(HashOption::Hash(H256(
				hex::decode(s.trim_start_matches("0x"))?
					.try_into()
					.map_err(|_| anyhow::anyhow!("Invalid option or hash: {s}."))?,
			))),
		}
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

	let args = std::env::args().skip(1).take(2).collect::<Vec<_>>();

	let (url, hash_opt) = if args.len() != 2 {
		eprintln!("{}", HELP);
		return Ok(());
	} else {
		(
			match args[0].as_str() {
				"l" | "local" | "localhost" => "ws://localhost:9944",
				"b" | "m" | "berghain" | "mainnet" => "wss://mainnet-archive.chainflip.io",
				"p" | "persa" | "perseverance" => "wss://archive.perseverance.chainflip.io",
				"s" | "sisy" | "sisyphos" => "wss://archive.sisyphos.chainflip.io",
				other => other,
			},
			args[1]
				.parse::<HashOption>()
				.context("Expected one of: latest|follow|0x<hash> ")?,
		)
	};

	// Any remaining arguments are assumed to be path filters.
	let filter = PathFilter(std::env::args().skip(3).collect::<Vec<_>>());

	let client = subxt::OnlineClient::<SubstrateConfig>::from_insecure_url(url).await.unwrap();

	match hash_opt {
		HashOption::Latest => {
			decode_events_at(&client.at_current_block().await?, &filter).await?;
		},
		HashOption::Hash(hash) => {
			decode_events_at(&client.at_block(hash).await?, &filter).await?;
		},
		HashOption::Follow => {
			let mut blocks = client.stream_blocks().await?;
			while let Some(block) = blocks.next().await {
				decode_events_at(&block?.at().await?, &filter).await?;
			}
		},
	};

	Ok(())
}

async fn decode_events_at(
	at_block: &OnlineClientAtBlock<SubstrateConfig>,
	filter: &PathFilter,
) -> anyhow::Result<()> {
	let events_data = at_block.storage().fetch_raw(EVENTS_STORAGE_KEY.to_vec()).await?;

	let metadata = at_block.metadata_ref();
	let event_decoder =
		EventDecoder::new(metadata.types().clone(), metadata.outer_enums().error_enum_ty());

	for event in event_decoder
		.decode_events(events_data)?
		.into_iter()
		.filter(|json_event| filter.include(json_event))
	{
		println!("{}", event.into_inner());
	}

	Ok(())
}

#[derive(Debug)]
struct PathFilter(Vec<String>);

impl PathFilter {
	fn include(&self, event: &JsonValue) -> bool {
		self.0.is_empty() || self.0.iter().any(|path| event.pointer(path).is_some())
	}
}
