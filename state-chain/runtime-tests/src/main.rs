use std::str::FromStr;

use anyhow::anyhow;
use frame_remote_externalities::{Mode, OfflineConfig, OnlineConfig, SnapshotConfig, Transport};
use tracing_subscriber::filter::LevelFilter;

mod tests;

type StateChainBlock = state_chain_runtime::Block;

#[derive(Debug, Clone)]
pub enum Network {
	Local,
	Sisyphos,
	Perseverance,
	Berghain,
	Custom(String),
}

impl FromStr for Network {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"local" => Ok(Self::Local),
			"sisyphos" | "s" => Ok(Self::Sisyphos),
			"perseverance" | "p" => Ok(Self::Perseverance),
			"berghain" | "mainnet" | "b" | "m" => Ok(Self::Berghain),
			s if !s.starts_with("0x") => Ok(Self::Custom(s.to_string())),
			_ => Err(()),
		}
	}
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args: Vec<String> = std::env::args().collect();

	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(
			tracing_subscriber::EnvFilter::builder()
				.with_default_directive(LevelFilter::INFO.into())
				.from_env()?,
		)
		.try_init()
		.expect("setting default subscriber failed");

	let network = Network::from_str(&args[1]).ok().map(|network| match network {
		Network::Local => "http://localhost:9944".to_string(),
		Network::Sisyphos => "https://archive.sisyphos.chainflip.io:443".to_string(),
		Network::Perseverance => "https://archive.perseverance.chainflip.io:443".to_string(),
		Network::Berghain => "https://mainnet-archive.chainflip.io:443".to_string(),
		Network::Custom(url) => url,
	});

	let hashes = args
		.into_iter()
		.skip(if network.is_some() { 2 } else { 1 })
		.map(|s| {
			if s == "latest" {
				Ok(None)
			} else {
				s.parse()
					.or_else(|_| {
						// Assume snapshot file relative to the current path.
						// TODO:
						// If a snapshot file is specified at some other path, try to use it.
						s.trim_start_matches("snapshots/").trim_end_matches(".snap").parse()
					})
					.map(Some)
			}
		})
		.collect::<Result<Vec<_>, _>>()?;

	if hashes.is_empty() {
		anyhow::bail!(
			"No hashes provided. Either provide a list of snapshots, hashes or 'latest'."
		);
	}

	let modes: Vec<_> = match (network, hashes) {
		(None, hashes) => hashes
			.into_iter()
			.map(|hash| {
				Mode::<StateChainBlock>::Offline(OfflineConfig {
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
					snapshot_file_for_hash(Some(remote_externalities.block_hash)).path,
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
