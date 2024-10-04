use std::{collections::BTreeMap, ops::AsyncFn, time::Duration};

use anyhow::Result;
use async_stream::stream;
use bitcoin::TxIn;
use chainflip_api::primitives::TxId;
use chainflip_engine::btc::rpc::{VerboseBlock, VerboseTransaction, VerboseTxIn};
use futures::{stream::StreamExt, Stream};
use merge_streams::MergeStreams;
use sha2::digest::Update;
use tokio::{sync::watch, time::sleep};

use crate::elliptic::EllipticClient;

// pub struct AnalysisResult {
//     risk_score: Option<f64>
// }

pub enum AnalysisResult {
	Complete(Option<f64>),
	Incomplete,
}

pub type Addresses = Vec<bitcoin::Address>;
pub trait TargetsProvider {
	// fn should_be_monitored(&self, address: &bitcoin::Address) -> bool;
	fn get_addresses(&self) -> impl Stream<Item = Addresses>;
}

#[derive(Clone, Debug)]
struct AnalysisItem {
	tx_id: bitcoin::Txid,
	target_address: bitcoin::Address,
}

pub trait AnalysisProvider {
	// async fn analyze(&self, txid: &bitcoin::Txid, target_address: &bitcoin::Address) ->
	// Result<AnalysisResult>;
	async fn analyze(&self, tx: &AnalysisItem) -> AnalysisResult;
}

pub type Transactions = Vec<bitcoin::Transaction>;
pub trait MempoolProvider {
	fn transactions() -> impl Stream<Item = Transactions>;
}

/// NOTE: added and removed should be disjoint
pub struct TransactionsUpdate {
	pub added: Transactions,
	pub removed: Vec<bitcoin::Txid>,
}

enum Event {
	NewAddresses(Addresses),
	TransactionsUpdate(TransactionsUpdate),
	NewBlock(VerboseBlock),
	Tick(()),
}

#[derive(Debug, Clone)]
enum Location {
	Mempool,
	Chain,
}

#[derive(Debug, Clone, PartialEq)]
enum ItemState {
	Unprocessed,
	Removed,
	Processed { risk_score: Option<f64> },
}

#[derive(Debug, Clone)]
struct ItemInfo {
	location: Location,
	attempts: u64,
	state: ItemState,
}

async fn analyze_item<A: AnalysisProvider>(analyzer: &A, item: &AnalysisItem, info: &mut ItemInfo) {
	match &info.state {
		ItemState::Unprocessed => {
			info.attempts += 1;
			info.state = match analyzer.analyze(item).await {
				AnalysisResult::Complete(risk_score) => ItemState::Processed { risk_score },
				AnalysisResult::Incomplete => ItemState::Unprocessed,
			};
		},
		_ => (),
	}
}

// async fn move_item_to_chain<A: AnalysisProvider>(
// 	tx_ids: Vec<bitcoin::Txid>,
// ) -> impl AsyncFn(&A, &AnalysisItem, ItemState) -> ItemState {
//     |analyzer, item, state| async move {
//         ItemState::Processed { risk_score: None }
//     }
// }

struct State<A: AnalysisProvider> {
	addresses: Addresses,
	items: BTreeMap<bitcoin::Txid, (AnalysisItem, ItemInfo)>,

	// unprocessed_mempool_items: Vec<AnalysisItem>,
	// removed_mempool_items: Vec<AnalysisItem>,
	// unprocessed_chain_items: Vec<AnalysisItem>,
	analyzer: A,
}

struct UpdateItemInfoResult {
	failed_txids: Vec<bitcoin::Txid>,
}

impl<A: AnalysisProvider> State<A> {
	async fn analyze_all_items(&mut self) {
		println!("monitor: trying to analyze items");
		for (item, info) in self.items.values_mut() {
			analyze_item(&self.analyzer, item, info).await;
		}
	}

	fn add_items(&mut self, items: Vec<AnalysisItem>, info: ItemInfo) {
		for item in items {
			self.items.insert(item.tx_id, (item, info.clone()));
		}
	}

	fn update_item_info<F: Fn(&mut ItemInfo)>(
		&mut self,
		tx_ids: Vec<bitcoin::Txid>,
		update: F,
	) -> UpdateItemInfoResult {
		let mut failed_txids = Vec::new();
		for tx_id in tx_ids {
			match self.items.get_mut(&tx_id) {
				Some((item, info)) => {
					update(info);
				},
				None => failed_txids.push(tx_id),
			}
		}
		UpdateItemInfoResult { failed_txids }
	}

	fn print_stats(&self) {
		let mut unprocessed_mem = 0;
		let mut unprocessed_chain = 0;
		let mut removed = 0;
		let mut processed = 0;
		for (item, info) in self.items.values() {
			match info.state {
				ItemState::Unprocessed => match info.location {
					Location::Mempool => unprocessed_mem += 1,
					Location::Chain => unprocessed_chain += 1,
				},
				ItemState::Removed => removed += 1,
				ItemState::Processed { risk_score } => processed += 1,
			}
		}
		println!(
			"monitor: state: mem={}, chain={}, removed={}, processed={}",
			unprocessed_mem, unprocessed_chain, removed, processed,
		);
	}
}

struct ProcessTransactionsResult {
	unprocessed_transactions: Vec<AnalysisItem>,
	processed_transactions: Vec<(AnalysisItem, Option<f64>)>,
}

fn make_analysis_items(transaction: bitcoin::Transaction) -> Vec<AnalysisItem> {
	let mut results = Vec::new();

	println!(
		"monitor: got new transaction (id = {}) with {} outputs",
		transaction.txid(),
		transaction.output.len()
	);

	for output in transaction.output.clone() {
		match bitcoin::Address::from_script(&output.script_pubkey, bitcoin::Network::Bitcoin) {
			Ok(address) =>
				results.push(AnalysisItem { tx_id: transaction.txid(), target_address: address }),
			Err(e) => {
				println!("monitor: could not derive address from script_pubkey: {e}")
			},
		}
	}

	results
}

fn make_verbose_analysis_items(transaction: VerboseTransaction) -> Vec<AnalysisItem> {
	let mut results = Vec::new();

	for output in transaction.vout.clone() {
		match bitcoin::Address::from_script(&output.script_pubkey, bitcoin::Network::Bitcoin) {
			Ok(address) =>
				results.push(AnalysisItem { tx_id: transaction.txid, target_address: address }),
			Err(e) => {
				println!("monitor: could not derive address from script_pubkey: {e}")
			},
		}
	}

	results
}

async fn process_transactions<A>(
	transactions: Vec<AnalysisItem>,
	analyze: &A,
) -> ProcessTransactionsResult
where
	A: AnalysisProvider,
{
	let mut unprocessed_transactions = Vec::new();
	let mut processed_transactions = Vec::new();

	for transaction in transactions {
		match analyze.analyze(&transaction).await {
			AnalysisResult::Complete(result) => {
				processed_transactions.push((transaction, result));
			},
			AnalysisResult::Incomplete => {
				unprocessed_transactions.push(transaction);
			},
		}
	}

	ProcessTransactionsResult { unprocessed_transactions, processed_transactions }
}

fn filter_relevant_transactions(txs: &mut Transactions, addresses: &Addresses) {
	txs.retain(|tx| {
		tx.output
			.iter()
			.find(|out| {
				match bitcoin::Address::from_script(&out.script_pubkey, bitcoin::Network::Bitcoin) {
					Ok(a) => addresses.contains(&a),
					Err(_) => false,
				}
			})
			.is_some()
	});
}

fn filter_relevant_verbose_transactions(txs: &mut Vec<VerboseTransaction>, addresses: &Addresses) {
	txs.retain(|tx| {
		tx.vout
			.iter()
			.find(|out| {
				match bitcoin::Address::from_script(&out.script_pubkey, bitcoin::Network::Bitcoin) {
					Ok(a) => addresses.contains(&a),
					Err(_) => false,
				}
			})
			.is_some()
	});
}

/// non performant implementation, we iterate twice
fn drain_if<A, F>(xs: &mut Vec<A>, f: F) -> Vec<A>
where
	A: Clone,
	F: Fn(&A) -> bool,
{
	let mut drained = xs.clone();
	drained.retain(|x| !(f(x)));

	xs.retain(f);

	drained
}

pub fn get_tick_stream(interval: Duration) -> impl Stream<Item = ()> {
	stream! {
		loop {
			yield ();

			sleep(interval).await;
		}
	}
}

pub async fn monitor2<T, M, B, A>(targets: T, mempool_transactions: M, blocks: B, analyze: A)
where
	T: Stream<Item = Addresses>,
	M: Stream<Item = TransactionsUpdate>,
	B: Stream<Item = VerboseBlock>,
	A: AnalysisProvider + Clone,
{
	let mut s = (
		targets.map(Event::NewAddresses),
		mempool_transactions.map(Event::TransactionsUpdate),
		blocks.map(Event::NewBlock),
		get_tick_stream(Duration::from_secs(10)).map(Event::Tick),
	)
		.merge();

	let initial = State { addresses: Vec::new(), items: BTreeMap::new(), analyzer: analyze };

	s.fold(initial, |mut state, event| async move {
		state.print_stats();
		match event {
			Event::NewAddresses(new_addresses) => {
				println!("got new addresses: {new_addresses:?}");
				state.addresses = new_addresses;
				state
			},
			Event::TransactionsUpdate(update) => {
				//-------------------------
				// process removed transactions
				//
				// this means move all removed transactions into the
				// stale `removed_mempool_transactions` state
				state.update_item_info(update.removed, |info| info.state = ItemState::Removed);

				//-------------------------
				// process added transactions
				let mut txs = update.added;
				println!("monitor: got {} new transactions", txs.len());
				filter_relevant_transactions(&mut txs, &state.addresses);

				println!("monitor: {} txs are relevant", txs.len());
				let items: Vec<_> = txs.into_iter().flat_map(make_analysis_items).collect();
				state.add_items(items, ItemInfo { location: Location::Mempool, attempts: 0, state: ItemState::Unprocessed });

				state
			},
			Event::NewBlock(verbose_block) => {
				println!("monitor: got a new block: {}", verbose_block.header.hash);
				// check which interesting transactions have been
				// included in the block, resubmit request for analysis
				let mut txs = verbose_block.txdata.clone();
				filter_relevant_verbose_transactions(&mut txs, &state.addresses);
				println!(
					"monitor: block contains {} txs, relevant are {}",
					verbose_block.txdata.len(),
					txs.len()
				);
				let result = state.update_item_info(txs.iter().map(|tx| tx.txid).collect(), |info| {
					info.location = Location::Chain;
					if info.state == ItemState::Removed {
						info.state = ItemState::Unprocessed;
						println!("found one item which was removed, but now found in a block. Resetting state to unprocessed");
					}
				});
				for id in result.failed_txids.clone() {
					println!("Found relevant txid {id} in block, but no such item was registered previously.")
				}
				txs.retain(|tx| result.failed_txids.contains(&tx.txid));
				println!("Adding {} previously untracked transactions", txs.len());

				let items: Vec<_> = txs.into_iter().flat_map(make_verbose_analysis_items).collect();
				state.add_items(items, ItemInfo { location: Location::Chain, attempts: 0, state: ItemState::Unprocessed });

				state
			},
			Event::Tick(()) => {
				println!("monitor: tick");
				state.analyze_all_items().await;
				state
			},
		}
	})
	.await;
}

impl AnalysisProvider for EllipticClient {
	async fn analyze(&self, tx: &AnalysisItem) -> AnalysisResult {
		let result = self
			.welltyped_single_analysis(
				tx.tx_id,
				tx.target_address.clone(),
				"test_customer_1".into(),
			)
			.await;
		match result {
			Ok(val) => AnalysisResult::Complete(Some(val.risk_score)),
			Err(e) => {
				println!("elliptic: analysis error: {e}");
				AnalysisResult::Incomplete
			},
		}
	}
}
