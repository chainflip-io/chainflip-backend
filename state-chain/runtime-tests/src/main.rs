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
use std::str::FromStr;

use anyhow::anyhow;
use frame_remote_externalities::{Mode, OfflineConfig, OnlineConfig, SnapshotConfig, Transport};
use tracing_subscriber::filter::LevelFilter;

mod tests;

type StateChainBlock = state_chain_runtime::Block;

const HELP: &str = "\
Runs the state chain runtime tests against real chain state, either fetched from a network
or loaded from a local snapshot.

USAGE:
    cargo run -- [NETWORK] <TARGET>...
    cargo run -- help

    This crate is excluded from the workspace, so cargo must be run from within
    `state-chain/runtime-tests`.

NETWORK (optional):
    local                       http://localhost:9944
    sisyphos | s                https://archive.sisyphos.chainflip.io:443
    perseverance | p            https://archive.perseverance.chainflip.io:443
    berghain | mainnet | b | m  https://mainnet-archive.chainflip.io:443
    <url>                       any other archive node RPC endpoint, with an explicit
                                scheme (http://, https://, ws:// or wss://)

    The leading argument is taken as the network only if it matches one of the above.
    With no network, state is loaded from local snapshots and nothing is fetched.

TARGET (one or more):
    latest                      the network's latest finalised block (needs a NETWORK)
    0x<hash>                    a specific block hash
    snapshots/0x<hash>.snap     a previously downloaded snapshot

    Fetched state is cached under `snapshots/`, so a block that has been run before is
    re-used offline instead of being fetched again.

ENVIRONMENT:
    STORAGE_ANALYSIS_ONLY=1     run the storage analysis only, skipping the tests
    RUST_LOG=debug              log level (default: info)

EXAMPLES:
    cargo run -- berghain latest             # fetch and test mainnet's latest block
    cargo run -- b 0xabc123... 0xdef456...   # fetch and test two specific blocks
    cargo run -- 0xabc123...                 # re-run against an existing snapshot

A storage report is written to `storage-report-<hash>.md` in this directory for every
block that is processed.\
";

#[derive(Debug, Clone)]
pub enum Network {
	Local,
	Sisyphos,
	Perseverance,
	Berghain,
	Custom(String),
}

impl Network {
	fn url(self) -> String {
		match self {
			Self::Local => "http://localhost:9944".to_string(),
			Self::Sisyphos => "https://archive.sisyphos.chainflip.io:443".to_string(),
			Self::Perseverance => "https://archive.perseverance.chainflip.io:443".to_string(),
			Self::Berghain => "https://mainnet-archive.chainflip.io:443".to_string(),
			Self::Custom(url) => url,
		}
	}
}

impl FromStr for Network {
	/// `Err` means "not a network", not "malformed": the argument is then tried as a target.
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		const SCHEMES: [&str; 4] = ["http://", "https://", "ws://", "wss://"];

		match s {
			"local" => Ok(Self::Local),
			"sisyphos" | "s" => Ok(Self::Sisyphos),
			"perseverance" | "p" => Ok(Self::Perseverance),
			"berghain" | "mainnet" | "b" | "m" => Ok(Self::Berghain),
			// A custom endpoint must carry an explicit scheme. Accepting bare strings would
			// swallow any mistyped target as a URL, and fail much later at connection time.
			s if SCHEMES.iter().any(|scheme| s.starts_with(scheme)) =>
				Ok(Self::Custom(s.to_string())),
			_ => Err(()),
		}
	}
}

/// A block to run the tests against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
	/// The network's latest finalised block.
	Latest,
	/// A specific block, given either as a hash or as the snapshot it was cached in.
	Block(state_chain_runtime::Hash),
}

impl FromStr for Target {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s == "latest" {
			return Ok(Self::Latest)
		}

		// Snapshots are named after the block hash they hold.
		// TODO: If a snapshot file is specified at some other path, try to use it.
		s.trim_start_matches("snapshots/")
			.trim_end_matches(".snap")
			.parse()
			.map(Self::Block)
			.map_err(|_| {
				anyhow!(
					"Invalid argument `{s}`: expected a network name or url, `latest`, a block \
					 hash, or a snapshot. Run with `help` for details."
				)
			})
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args: Vec<String> = std::env::args().skip(1).collect();

	if args.iter().any(|arg| matches!(arg.as_str(), "help" | "--help" | "-h")) {
		println!("{HELP}");
		return Ok(())
	}

	let Some((first, rest)) = args.split_first() else {
		eprintln!("{HELP}");
		anyhow::bail!("No arguments provided.");
	};

	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(
			tracing_subscriber::EnvFilter::builder()
				.with_default_directive(LevelFilter::INFO.into())
				.from_env()?,
		)
		.try_init()
		.expect("setting default subscriber failed");

	// The leading argument is the network if it names one; otherwise every argument is a
	// target and the run is offline.
	let (network, target_args) = match first.parse::<Network>() {
		Ok(network) => (Some(network), rest),
		Err(()) => (None, args.as_slice()),
	};

	let targets = target_args
		.iter()
		.map(|arg| arg.parse::<Target>())
		.collect::<anyhow::Result<Vec<_>>>()?;

	if targets.is_empty() {
		eprintln!("{HELP}");
		anyhow::bail!("No targets provided. Provide one or more snapshots, hashes or `latest`.");
	}

	// Resolving `latest` requires querying the network, and the snapshot it would otherwise be
	// read from is renamed to its block hash as soon as it has been fetched.
	if network.is_none() && targets.contains(&Target::Latest) {
		anyhow::bail!("`latest` requires a network, e.g. `berghain latest`.");
	}

	let network = network.map(Network::url);

	let hashes = targets
		.into_iter()
		.map(|target| match target {
			Target::Latest => None,
			Target::Block(hash) => Some(hash),
		})
		.collect::<Vec<_>>();

	let modes: Vec<_> = match (network, hashes) {
		(None, hashes) => hashes
			.into_iter()
			.map(|hash| {
				Mode::<state_chain_runtime::Hash>::Offline(OfflineConfig {
					state_snapshot: snapshot_file_for_hash(hash),
				})
			})
			.collect(),
		(Some(network), hashes) => hashes
			.into_iter()
			.map(|hash| {
				Mode::OfflineOrElseOnline(
					OfflineConfig { state_snapshot: snapshot_file_for_hash(hash) },
					OnlineConfig {
						at: hash,
						state_snapshot: Some(snapshot_file_for_hash(hash)),
						transport: Transport::Uri(network.clone()),
						..Default::default()
					},
				)
			})
			.collect(),
	};

	for mode in modes {
		let snapshot_config = match &mode {
			Mode::Offline(OfflineConfig { ref state_snapshot }) => Some(state_snapshot),
			Mode::OfflineOrElseOnline(OfflineConfig { ref state_snapshot, .. }, _) =>
				Some(state_snapshot),
			_ => None,
		}
		.cloned();

		let remote_externalities = frame_remote_externalities::Builder::<StateChainBlock>::new()
			.mode(mode)
			.build()
			.await
			.map_err(|e| anyhow!(e))?;

		// If the snapshot was for "latest", rename it to the actual hash.
		if let Some(snapshot) = snapshot_config {
			if snapshot.path == snapshot_file_for_hash(None).path {
				std::fs::rename(
					snapshot.path,
					snapshot_file_for_hash(Some(remote_externalities.header.hash())).path,
				)?;
			}
		}

		tests::run_all(remote_externalities)?;
	}

	Ok(())
}

fn snapshot_file_for_hash(hash: Option<state_chain_runtime::Hash>) -> SnapshotConfig {
	if let Some(hash) = hash {
		format!("snapshots/{:?}.snap", hash)
	} else {
		"snapshots/latest.snap".to_string()
	}
	.into()
}
