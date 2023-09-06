// Migration for the Settings.toml of the engine for versions 0.9.1 to 0.9.2

// Anything to do with the RPC nodes of each of the chains needs to be migrated to be support a
// backup rpc. In the TOML it looks like:
// Before:
// [eth]
// # Ethereum private key file path. Default is the docker secrets path. This file should contain a
// hex-encoded private key. private_key_file = "./localnet/init/keys/eth_private_key_file"
// ws_node_endpoint = "ws://localhost:8546"
// http_node_endpoint = "http://localhost:8545"
//
// After:
//
// [eth]
// # Ethereum private key file path. Default is the docker secrets path. This file should contain a
// hex-encoded private key. private_key_file = "./localnet/init/keys/eth_private_key_file"

// [eth.node]
// ws_node_endpoint = "ws://localhost:8546"
// http_node_endpoint = "http://localhost:8545"

use std::{fs, path::PathBuf};

use toml::Table;

use anyhow::{bail, Context, Result};

const PRIVATE_KEY_FILE: &str = "private_key_file";
const WS_NODE_ENDPOINT: &str = "ws_node_endpoint";
const HTTP_NODE_ENDPOINT: &str = "http_node_endpoint";

pub fn migrate_settings0_9_1_to_0_9_2(config_root: String) -> Result<()> {
	let settings_file = PathBuf::from(config_root).join("config/Settings.toml");

	if !settings_file.is_file() {
		bail!("Unable to migrate. Please check that the Settings.toml file exists at {settings_file:?}");
	}

	let old_settings_table = std::fs::read_to_string(&settings_file)
		.context("Unable to read Settings.toml for migration")?
		.parse::<Table>()?;

	let mut migrate = false;
	// These have the same "node" configs
	for chain in ["eth", "dot"] {
		let chain_should_migrate = if let Some(chain_settings) = old_settings_table.get(chain) {
			chain_settings.get(WS_NODE_ENDPOINT).is_some() ||
				chain_settings.get(HTTP_NODE_ENDPOINT).is_some()
		} else {
			false
		};
		migrate = migrate || chain_should_migrate;
	}
	migrate = migrate ||
		if let Some(btc_settings) = old_settings_table.get("btc") {
			btc_settings.get(HTTP_NODE_ENDPOINT).is_some() ||
				btc_settings.get("rpc_user").is_some() ||
				btc_settings.get("rpc_password").is_some()
		} else {
			false
		};

	if !migrate {
		tracing::info!("No settings migration required. Already up to date.");
		return Ok(())
	}

	let mut new_settings_table = old_settings_table.clone();

	// Need to do ETH differently to the other chains since it has a `private_key_file` field
	// that stays flat.
	if let Some(old_eth_map) = old_settings_table.get("eth").cloned() {
		let mut eth_map = Table::new();

		if let Some(private_key_file) = old_eth_map.get(PRIVATE_KEY_FILE) {
			// same as before
			eth_map.insert(PRIVATE_KEY_FILE.to_string(), private_key_file.clone());
		}
		// nested by "node"
		let mut node_map_has_value = false;
		let mut node_map = Table::new();
		if let Some(ws_node_endpoint) = old_eth_map.get(WS_NODE_ENDPOINT) {
			node_map.insert(WS_NODE_ENDPOINT.to_string(), ws_node_endpoint.clone());
			node_map_has_value = true;
		}
		if let Some(http_node_endpoint) = old_eth_map.get(HTTP_NODE_ENDPOINT) {
			node_map.insert(HTTP_NODE_ENDPOINT.to_string(), http_node_endpoint.clone());
			node_map_has_value = true;
		}

		if node_map_has_value {
			eth_map.insert("node".to_string(), toml::Value::Table(node_map));
		}

		new_settings_table.insert("eth".to_string(), toml::Value::Table(eth_map));
	}

	// btc and dot are identical. Take what's under them, and nest it by "node".
	for chain in ["btc", "dot"] {
		if let Some(old_chain_map) = old_settings_table.get(chain).cloned() {
			let mut new_node_map = Table::new();
			new_node_map.insert("node".to_string(), old_chain_map.clone());

			new_settings_table.insert(chain.to_string(), toml::Value::Table(new_node_map));
		}
	}

	fs::write(
		settings_file,
		toml::to_string(&new_settings_table).context("Unable to new Settings to TOML")?,
	)
	.context("Unable to write Settings.toml for migration")?;

	Ok(())
}
