//! Common Witnesser functionality

use std::collections::BTreeSet;

use async_trait::async_trait;
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
pub enum AddressMonitorCommand<Address> {
	Add(Address),
	Remove(Address),
}

/// This stores addresses we are interested in. New addresses
/// come through a channel which can be polled by calling
/// [AddressMonitor::sync_addresses].
pub struct AddressMonitor<A> {
	addresses: BTreeSet<A>,
	address_receiver: tokio::sync::mpsc::UnboundedReceiver<AddressMonitorCommand<A>>,
}

impl<A: std::cmp::Ord + std::fmt::Debug + Clone> AddressMonitor<A> {
	pub fn new(
		addresses: BTreeSet<A>,
		address_receiver: tokio::sync::mpsc::UnboundedReceiver<AddressMonitorCommand<A>>,
	) -> Self {
		Self { addresses, address_receiver }
	}

	/// Check if we are interested in the address. [AddressMonitor::sync_addresses]
	/// should be called first to ensure we check against recently added addresses.
	/// (We keep it as a separate function to make it possible to check multiple
	/// addresses in a tight loop without having to fetch addresses on every item)
	pub fn contains(&self, address: &A) -> bool {
		self.addresses.contains(address)
	}

	/// Ensure the list of interesting addresses is up to date
	pub fn sync_addresses(&mut self) {
		while let Ok(address) = self.address_receiver.try_recv() {
			match address {
				AddressMonitorCommand::Add(address) =>
					if !self.addresses.insert(address.clone()) {
						tracing::warn!("Address {:?} already being monitored", address);
					},
				AddressMonitorCommand::Remove(address) =>
					if !self.addresses.remove(&address) {
						tracing::warn!("Address {:?} already not being monitored", address);
					},
			}
		}
	}
}
