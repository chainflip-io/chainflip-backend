//! Common Witnesser functionality

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use cf_chains::address::{BitcoinAddressData, ScriptPubkeyBytes};
use cf_primitives::EpochIndex;

pub mod block_head_stream_from;
pub mod checkpointing;
pub mod epoch_witnesser;
pub mod http_safe_stream;

use anyhow::Result;

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

pub trait BlockNumberable {
	type BlockNumber;

	fn block_number(&self) -> Self::BlockNumber;
}

impl BlockNumberable for u64 {
	type BlockNumber = Self;

	fn block_number(&self) -> Self::BlockNumber {
		*self
	}
}

/// General trait for getting the latest/height block number for a particular chain
#[async_trait]
pub trait LatestBlockNumber {
	type BlockNumber;

	async fn latest_block_number(&self) -> Result<Self::BlockNumber>;
}

#[derive(Debug)]
pub enum AddressMonitorCommand<AddressData> {
	Add(AddressData),
	Remove(AddressData),
}

/// This stores addresses we are interested in. New addresses
/// come through a channel which can be polled by calling
/// [AddressMonitor::sync_addresses].
pub struct AddressMonitor<A, K, V> {
	addresses: BTreeMap<K, V>,
	address_receiver: tokio::sync::mpsc::UnboundedReceiver<AddressMonitorCommand<A>>,
}

// Some addresses act as key value pairs.
pub trait AddressKeyValue {
	type Key;
	type Value;

	fn key_value(&self) -> (Self::Key, Self::Value);
}

impl<
		A: std::fmt::Debug + AddressKeyValue<Key = K, Value = V>,
		K: std::cmp::Ord + Clone,
		V: Clone,
	> AddressMonitor<A, K, V>
{
	pub fn new(
		addresses: BTreeSet<A>,
	) -> (tokio::sync::mpsc::UnboundedSender<AddressMonitorCommand<A>>, Self) {
		let addresses = addresses.into_iter().map(|a| a.key_value()).collect();
		let (address_sender, address_receiver) = tokio::sync::mpsc::unbounded_channel();
		(address_sender, Self { addresses, address_receiver })
	}

	/// Check if we are interested in the address. [AddressMonitor::sync_addresses]
	/// should be called first to ensure we check against recently added addresses.
	/// (We keep it as a separate function to make it possible to check multiple
	/// addresses in a tight loop without having to fetch addresses on every item)
	pub fn get(&self, address: &K) -> Option<V> {
		self.addresses.get(address).cloned()
	}
	pub fn contains(&self, address: &K) -> bool {
		self.addresses.contains_key(address)
	}

	/// Ensure the list of interesting addresses is up to date
	pub fn sync_addresses(&mut self) {
		while let Ok(address) = self.address_receiver.try_recv() {
			match address {
				AddressMonitorCommand::Add(address) => {
					let (k, v) = address.key_value();
					if self.addresses.insert(k, v).is_some() {
						tracing::warn!("Address {:?} already being monitored", address);
					}
				},
				AddressMonitorCommand::Remove(address) =>
					if self.addresses.remove(&address.key_value().0).is_none() {
						tracing::warn!("Address {:?} already not being monitored", address);
					},
			}
		}
	}
}

impl AddressKeyValue for sp_core::H160 {
	type Key = Self;
	type Value = ();

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(*self, ())
	}
}

impl AddressKeyValue for BitcoinAddressData {
	type Key = ScriptPubkeyBytes;
	type Value = Self;

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(self.to_scriptpubkey().unwrap().serialize(), self.clone())
	}
}

impl AddressKeyValue for sp_runtime::AccountId32 {
	type Key = Self;
	type Value = ();

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(self.clone(), ())
	}
}
