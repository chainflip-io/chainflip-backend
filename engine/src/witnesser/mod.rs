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
						tracing::debug!("Starting to monitor: {:?}", address);
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

impl AddressKeyValue for BitcoinScriptBounded {
	type Key = ScriptPubkeyBytes;
	type Value = Self;

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(self.data.clone().into(), self.clone())
	}
}

impl AddressKeyValue for sp_runtime::AccountId32 {
	type Key = Self;
	type Value = ();

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(self.clone(), ())
	}
}

impl AddressKeyValue for [u8; 32] {
	type Key = Self;
	type Value = ();

	fn key_value(&self) -> (Self::Key, Self::Value) {
		(*self, ())
	}
}

// #[test]
// fn hex_from_bytes() {
// 	// 0x1c1fbffe58444c1b8780982706932ffeac19e348d3a771fd0c02ae4e5e6a1ce4
// 	let bytes: Vec<u8> = vec![
// 		28, 31, 191, 254, 88, 68, 76, 27, 135, 128, 152, 39, 6, 147, 47, 254, 172, 25, 227, 72,
// 		211, 167, 113, 253, 12, 2, 174, 78, 94, 106, 28, 228,
// 	];

// 	let hex_str = hex::encode(bytes);

// 	println!("Hex: {:?}", hex_str);

// 	let encoded_tx = vec![
// 		2u8, 0, 0, 0, 0, 1, 1, 144, 60, 174, 18, 173, 89, 89, 63, 19, 169, 223, 157, 69, 238, 170,
// 		27, 233, 189, 128, 59, 65, 16, 228, 95, 116, 87, 122, 47, 66, 104, 131, 4, 0, 0, 0, 0, 0,
// 		253, 255, 255, 255, 2, 22, 132, 75, 0, 0, 0, 0, 0, 25, 118, 169, 20, 42, 164, 164, 77, 70,
// 		150, 15, 156, 233, 241, 224, 31, 150, 96, 9, 26, 232, 70, 154, 140, 136, 172, 2, 66, 79,
// 		59, 0, 0, 0, 0, 34, 81, 32, 215, 190, 7, 74, 1, 37, 171, 102, 138, 179, 158, 227, 200, 34,
// 		51, 121, 149, 86, 230, 234, 217, 250, 212, 127, 33, 39, 147, 119, 92, 214, 159, 247, 3, 64,
// 		139, 188, 175, 77, 185, 235, 252, 246, 28, 131, 194, 155, 171, 198, 212, 87, 50, 76, 62,
// 		216, 38, 22, 147, 57, 194, 44, 111, 160, 17, 161, 200, 60, 10, 211, 72, 212, 56, 129, 225,
// 		71, 249, 246, 98, 232, 165, 255, 70, 37, 7, 71, 92, 129, 129, 148, 44, 75, 222, 180, 246,
// 		34, 147, 87, 231, 187, 36, 81, 117, 32, 24, 251, 230, 61, 24, 119, 151, 133, 31, 119, 26,
// 		159, 129, 164, 98, 241, 11, 106, 230, 153, 13, 228, 135, 213, 187, 168, 75, 180, 186, 175,
// 		151, 228, 172, 33, 193, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238,
// 		238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238,
// 		238, 0, 0, 0, 0,
// 	];

// 	// let double_hash = sha2_256(&sha2_256(&encoded_tx));
// 	// println!("Here's the double hash: {:?}", double_hash);
// 	// }, transaction_out_id: [28, 31, 191, 254, 88, 68, 76, 27, 135, 128, 152, 39, 6, 147, 47,
// 	// 254, 172, 25, 227, 72, 211, 167, 113, 253, 12, 2, 174, 78, 94, 106, 28, 228];
// }

// 0x1c1fbffe58444c1b8780982706932ffeac19e348d3a771fd0c02ae4e5e6a1ce4

// "Handling event RuntimeEvent::BitcoinBroadcaster(Event::TransactionBroadcastRequest {
// broadcast_attempt_id: BroadcastAttemptId { broadcast_id: 1, attempt_count: 0 }, nominee:
// 36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911 (cFK7GTah...),
// transaction_payload: BitcoinTransactionData { encoded_transaction: [2, 0, 0, 0, 0, 1, 1, 144, 60,
// 174, 18, 173, 89, 89, 63, 19, 169, 223, 157, 69, 238, 170, 27, 233, 189, 128, 59, 65, 16, 228,
// 95, 116, 87, 122, 47, 66, 104, 131, 4, 0, 0, 0, 0, 0, 253, 255, 255, 255, 2, 22, 132, 75, 0, 0,
// 0, 0, 0, 25, 118, 169, 20, 42, 164, 164, 77, 70, 150, 15, 156, 233, 241, 224, 31, 150, 96, 9, 26,
// 232, 70, 154, 140, 136, 172, 2, 66, 79, 59, 0, 0, 0, 0, 34, 81, 32, 215, 190, 7, 74, 1, 37, 171,
// 102, 138, 179, 158, 227, 200, 34, 51, 121, 149, 86, 230, 234, 217, 250, 212, 127, 33, 39, 147,
// 119, 92, 214, 159, 247, 3, 64, 139, 188, 175, 77, 185, 235, 252, 246, 28, 131, 194, 155, 171,
// 198, 212, 87, 50, 76, 62, 216, 38, 22, 147, 57, 194, 44, 111, 160, 17, 161, 200, 60, 10, 211, 72,
// 212, 56, 129, 225, 71, 249, 246, 98, 232, 165, 255, 70, 37, 7, 71, 92, 129, 129, 148, 44, 75,
// 222, 180, 246, 34, 147, 87, 231, 187, 36, 81, 117, 32, 24, 251, 230, 61, 24, 119, 151, 133, 31,
// 119, 26, 159, 129, 164, 98, 241, 11, 106, 230, 153, 13, 228, 135, 213, 187, 168, 75, 180, 186,
// 175, 151, 228, 172, 33, 193, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238,
// 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 238, 0,
// 0, 0, 0] }, transaction_out_id: [28, 31, 191, 254, 88, 68, 76, 27, 135, 128, 152, 39, 6, 147, 47,
// 254, 172, 25, 227, 72, 211, 167, 113, 253, 12, 2, 174, 78, 94, 106, 28, 228]
// })"},"target":"chainflip_engine::state_chain_observer::sc_observer","span":{"name":"SCObserver"},
// "spans":[{"name":"SCObserver"}]}

// "Tx not monitored with hash
// [13, 242, 42, 135, 91, 235, 107, 96, 238, 111, 230, 17, 160, 213, 251, 25, 40, 245, 136, 143,
// 151, 213, 181, 212, 148, 244, 28, 64, 255, 63, 217,
// 106]."},"target":"chainflip_engine::btc::witnesser"}

// "Tx not monitored with hash
// [10, 39, 29, 236, 151, 39, 119, 88, 105, 138, 128, 102, 125, 23, 239, 188, 189, 40, 199, 184, 38,
// 79, 228, 2, 240, 187, 89, 169, 136, 141, 133, 63]."},"target":"chainflip_engine::btc::witnesser"}
