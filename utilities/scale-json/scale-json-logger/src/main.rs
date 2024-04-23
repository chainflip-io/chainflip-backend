use anyhow::Context;
use env_logger::Env;
use scale_json::{
	ext::{DecodeAsType, JsonValue},
	ScaleDecodedToJson,
};
use std::str::FromStr;
use subxt::{
	ext::{
		codec::{Compact, Decode},
		futures::TryStreamExt,
		sp_core::H256,
	},
	SubstrateConfig,
};

const HELP: &'static str = r#"
Usage: scale-json-logger <url> <latest|follow|0xhash> [path...]

url [required]: The URL of the Substrate node to connect to.
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
"#;

/// This valid across all Substrate chains.
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

	// Any remaining arguments are assumed to be paths.
	let paths = std::env::args().skip(3).collect::<Vec<_>>();

	let client = subxt::OnlineClient::<SubstrateConfig>::from_url(url).await.unwrap();

	match hash_opt {
		HashOption::Latest => {
			let hash = client.blocks().at_latest().await?.hash();
			decode_events_at_hash(&client, hash, paths).await?;
		},
		HashOption::Hash(hash) => {
			decode_events_at_hash(&client, hash, paths).await?;
		},
		HashOption::Follow => {
			client
				.blocks()
				.subscribe_finalized()
				.await?
				.map_err(Into::into)
				.try_for_each(|header| decode_events_at_hash(&client, header.hash(), paths.clone()))
				.await?;
		},
	};

	Ok(())
}

async fn decode_events_at_hash(
	client: &subxt::OnlineClient<SubstrateConfig>,
	hash: H256,
	paths: Vec<String>,
) -> anyhow::Result<()> {
	let storage_api = client.storage().at(hash);
	let events_data = storage_api.fetch_raw(EVENTS_STORAGE_KEY).await?.expect("No events in block");
	let event_data_cursor = &mut &events_data[..];

	// Works as long as there is only one EventRecord<T, H> type in the metadata.
	let type_id = find_type_id(&client.metadata(), "frame_system::EventRecord").unwrap();

	// The data is Vec<EventRecord<T, H>>, so it's prefixed with a Compact<u32> length.
	let event_count = Compact::<u32>::decode(event_data_cursor)
		.expect("Failed to decode CompactLen")
		.0;

	let initial_len = event_data_cursor.len();
	(0..event_count)
		.map(|_| {
			ScaleDecodedToJson::decode_as_type(
				event_data_cursor,
				&type_id,
				client.metadata().types(),
			)
			.map(JsonValue::from)
			.inspect(|json| log::debug!("Decoded event: {}", json))
			.inspect_err(|e| {
				log::error!(
					"Failed to decode event at data index {}: {}",
					event_data_cursor.len() - initial_len,
					e
				)
			})
		})
		.filter(|res| {
			res.as_ref()
				.is_ok_and(|json_event| paths.iter().any(|path| json_event.pointer(path).is_some()))
		})
		.try_for_each(|json_event| {
			println!("{}", json_event?);
			Ok::<_, anyhow::Error>(())
		})?;

	log::debug!("Decoded {} events", event_count);
	log::debug!("Remaining bytes: {}", event_data_cursor.len());

	Ok(())
}

fn find_type_id(metadata: &subxt::Metadata, type_path: &str) -> Option<u32> {
	metadata
		.types()
		.types
		.iter()
		.find(|t| &t.ty.path.segments.join("::") == type_path)
		.map(|t| t.id)
}
