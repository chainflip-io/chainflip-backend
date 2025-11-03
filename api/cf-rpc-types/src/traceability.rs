use cf_chains::{address::AddressString, ForeignChain};
use cf_primitives::Asset;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ops::Add};

pub struct AssetAndAddress {
	asset: Asset,
	address: String,
}

pub struct ChainflipDepositAddresses {
	bitcoin_deposit_addresses: Vec<AssetAndAddress>,
	solana_deposit_addresses: Vec<AssetAndAddress>,
	ethereum_deposit_addresses: Vec<AssetAndAddress>,
	arbitrum_deposit_addresses: Vec<AssetAndAddress>,
}

pub type ControlledDepositAddresses = HashMap<ForeignChain, Vec<AddressString>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AddressAndExplanation {
	pub name: String,
	pub address: AddressString,
	pub explanation: String,
	pub expected_expiry: Option<String>,
}

pub struct ChainflipControlledAddresses {
	bitcoin_vaults: Vec<AddressAndExplanation>,
	solana_vaults: Vec<AddressAndExplanation>,
	ethereum_vaults: Vec<AddressAndExplanation>,
	arbitrum_vault: Vec<AddressAndExplanation>,
}

pub type ControlledVaultAddresses = HashMap<ForeignChain, Vec<AddressAndExplanation>>;
