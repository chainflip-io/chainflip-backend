//! Common Witnesser functionality

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use cf_chains::{address::ScriptPubkeyBytes, btc::BitcoinScriptBounded};
use cf_primitives::EpochIndex;

pub mod block_head_stream_from;
pub mod block_witnesser;
pub mod checkpointing;
pub mod epoch_process_runner;
pub mod http_safe_stream;

use anyhow::Result;

use multisig::ChainTag;

pub type ChainBlockNumber<Chain> = <Chain as cf_chains::Chain>::ChainBlockNumber;

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct EpochStart<Chain: cf_chains::Chain> {
	pub epoch_index: EpochIndex,
	pub block_number: ChainBlockNumber<Chain>,
	pub current: bool,
	pub participant: bool,
	pub data: Chain::EpochStartData,
}

pub trait HasBlockNumber {
	type BlockNumber: PartialOrd + Into<u64>;

	fn block_number(&self) -> Self::BlockNumber;
}

impl HasBlockNumber for u64 {
	type BlockNumber = Self;

	fn block_number(&self) -> Self::BlockNumber {
		*self
	}
}

// TODO: implement this directly on cf_chains::Chain?
pub trait HasChainTag {
	const CHAIN_TAG: ChainTag;
}

impl HasChainTag for cf_chains::Ethereum {
	const CHAIN_TAG: ChainTag = ChainTag::Ethereum;
}

impl HasChainTag for cf_chains::Bitcoin {
	const CHAIN_TAG: ChainTag = ChainTag::Bitcoin;
}

impl HasChainTag for cf_chains::Polkadot {
	const CHAIN_TAG: ChainTag = ChainTag::Polkadot;
}

/// General trait for getting the latest/height block number for a particular chain
#[async_trait]
pub trait LatestBlockNumber {
	type BlockNumber;

	async fn latest_block_number(&self) -> Result<Self::BlockNumber>;
}

#[derive(Debug)]
pub enum MonitorCommand<MonitorData> {
	Add(MonitorData),
	Remove(MonitorData),
}

/// This stores items we are interested in. New items
/// come through a channel which can be polled by calling
/// [ItemMonitor::sync_items].
pub struct ItemMonitor<A, K, V> {
	items: BTreeMap<K, V>,
	item_receiver: tokio::sync::mpsc::UnboundedReceiver<MonitorCommand<A>>,
}

// Some addresses act as key value pairs.
pub trait ItemKeyValue {
	type Key;
	type Value;

	fn key_value(&self) -> (Self::Key, Self::Value);
}

impl<A: std::fmt::Debug + ItemKeyValue<Key = K, Value = V>, K: std::cmp::Ord + Clone, V: Clone>
	ItemMonitor<A, K, V>
{
	pub fn new(
		items: BTreeSet<A>,
	) -> (tokio::sync::mpsc::UnboundedSender<MonitorCommand<A>>, Self) {
		let items = items.into_iter().map(|a| a.key_value()).collect();
		let (item_sender, item_receiver) = tokio::sync::mpsc::unbounded_channel();
		(item_sender, Self { items, item_receiver })
	}

	/// Check if we are interested in the address. [ItemMonitor::sync_items]
	/// should be called first to ensure we check against recently added items.
	/// (We keep it as a separate function to make it possible to check multiple
	/// items in a tight loop without having to fetch items on every item)
	pub fn get(&self, address: &K) -> Option<V> {
		self.items.get(address).cloned()
	}
	pub fn contains(&self, address: &K) -> bool {
		self.items.contains_key(address)
	}

	/// Ensure the list of interesting items is up to date
	pub fn sync_items(&mut self) {
		while let Ok(address) = self.item_receiver.try_recv() {
			match address {
				MonitorCommand::Add(address) => {
					let (k, v) = address.key_value();
					if self.items.insert(k, v).is_some() {
						tracing::debug!("Starting to monitor: {:?}", address);
						tracing::warn!("Address {:?} already being monitored", address);
					}
				},
				MonitorCommand::Remove(address) =>
					if self.items.remove(&address.key_value().0).is_none() {
						tracing::warn!("Address {:?} already not being monitored", address);
					},
			}
		}
	}
}

impl ItemKeyValue for sp_core::H160 {
	type Key = Self;
	type Value = ();

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(*self, ())
	}
}

impl ItemKeyValue for BitcoinScriptBounded {
	type Key = ScriptPubkeyBytes;
	type Value = Self;

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(self.data.clone().into(), self.clone())
	}
}

impl ItemKeyValue for sp_runtime::AccountId32 {
	type Key = Self;
	type Value = ();

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(self.clone(), ())
	}
}

impl ItemKeyValue for [u8; 32] {
	type Key = Self;
	type Value = ();

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(*self, ())
	}
}
