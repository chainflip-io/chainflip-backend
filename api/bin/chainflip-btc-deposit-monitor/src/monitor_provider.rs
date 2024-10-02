
use std::ops::AsyncFn;

use anyhow::Result;
use chainflip_engine::btc::rpc::VerboseBlock;
use futures::Stream;
use tokio::sync::watch;
use futures::stream::StreamExt;
use merge_streams::MergeStreams;

use crate::elliptic::EllipticClient;

// pub struct AnalysisResult {
//     risk_score: Option<f64>
// }

pub enum AnalysisResult {
    Complete(Option<f64>),
    Incomplete
}

pub type Addresses = Vec<bitcoin::Address>;
pub trait TargetsProvider {
    // fn should_be_monitored(&self, address: &bitcoin::Address) -> bool;
    fn get_addresses(&self) -> impl Stream<Item=Addresses>;
}

pub trait AnalysisProvider {
    // async fn analyze(&self, txid: &bitcoin::Txid, target_address: &bitcoin::Address) -> Result<AnalysisResult>;
    async fn analyze(&self, tx: &bitcoin::Transaction) -> AnalysisResult;
}


pub type Transactions = Vec<bitcoin::Transaction>;
pub trait MempoolProvider {
    fn transactions() -> impl Stream<Item=Transactions>;
}


/// NOTE: added and removed should be disjoint
pub struct TransactionsUpdate {
    pub added: Transactions,
    pub removed: Vec<bitcoin::Txid>
}

enum Event {
    NewAddresses(Addresses),
    TransactionsUpdate(TransactionsUpdate),
    NewBlock(Option<VerboseBlock>)
}

struct State<A: AnalysisProvider> {
    addresses: Addresses,
    unprocessed_mempool_transactions: Transactions,
    removed_mempool_transactions: Transactions,
    unprocessed_chain_transactions: Transactions,
    analyzer: A,
}


struct ProcessTransactionsResult {
    unprocessed_transactions: Transactions,
    processed_transactions: Vec<(bitcoin::Transaction, Option<f64>)>
}

async fn process_transactions<A>(transactions: Transactions, analyze: &A) -> ProcessTransactionsResult
    where A : AnalysisProvider 
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
            }
        }
    }

    ProcessTransactionsResult {
        unprocessed_transactions,
        processed_transactions,
    }
}

async fn filter_relevant_transactions(txs: &mut Transactions, addresses: &Addresses) {
    txs.retain(|tx| tx.output.iter().find(|out| {
        match bitcoin::Address::from_script(&out.script_pubkey, bitcoin::Network::Bitcoin) {
            Ok(a) => addresses.contains(&a),
            Err(_) => false,
        }
    }).is_some());
}

/// non performant implementation, we iterate twice
fn drain_if<A, F>(xs: &mut Vec<A>, f: F) -> Vec<A> 
where 
    A: Clone,
    F: Fn(&A) -> bool
{
    let mut drained = xs.clone();
    drained.retain(|x| !(f(x)));

    xs.retain(f);

    drained
}


pub async fn monitor2<T,M,B,A>(targets: T, mempool_transactions: M, blocks: B, analyze: A)
  where
    T: Stream<Item=Addresses>,
    M: Stream<Item=TransactionsUpdate>,
    B: Stream<Item=Option<VerboseBlock>>,
    A: AnalysisProvider + Clone
{
    let mut s = (
        targets.map(Event::NewAddresses), 
        mempool_transactions.map(Event::TransactionsUpdate),
        blocks.map(Event::NewBlock)
    ).merge();

    let initial = State {
        addresses: Vec::new(),
        unprocessed_mempool_transactions: Vec::new(),
        removed_mempool_transactions: Vec::new(),
        unprocessed_chain_transactions: Vec::new(),
        analyzer: analyze
    };

    s.fold(initial, |mut state, event| async move {
        println!("monitor: state: mem={}, removed={}, chain={}",
            state.unprocessed_mempool_transactions.len(),
            state.removed_mempool_transactions.len(),
            state.unprocessed_chain_transactions.len(),
        );
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
                let mut removed = drain_if(&mut state.unprocessed_mempool_transactions, |tx| update.removed.contains(&tx.txid()));
                println!("monitor: removed {} transactions from unprocessed", removed.len());
                state.removed_mempool_transactions.append(&mut removed);

                //-------------------------
                // process added transactions
                let mut txs = update.added;
                println!("monitor: got {} new transactions", txs.len());
                filter_relevant_transactions(&mut txs, &state.addresses);

                println!("monitor: {} are relevant", txs.len());
                let mut result = process_transactions(txs, &state.analyzer).await;

                println!("monitor: successfully analyzed {} transactions", result.processed_transactions.len());
                state.unprocessed_mempool_transactions.append(&mut result.unprocessed_transactions);

                state
            }
            Event::NewBlock(verbose_block) => {
                println!("monitor: got a new block");
                // check which interesting transactions have been
                // included in the block, resubmit request for analysis
                state
            },
        }
    }).await;

}



impl AnalysisProvider for EllipticClient {
    async fn analyze(&self, tx: &bitcoin::Transaction) -> AnalysisResult {
        match bitcoin::Address::from_script(&tx.output[0].script_pubkey, bitcoin::Network::Bitcoin) {
            Ok(address) => {
                let result = self.welltyped_single_analysis(tx.txid(), address, "test_customer_1".into()).await;
                match result {
                    Ok(val) => AnalysisResult::Complete(Some(val.risk_score)),
                    Err(e) => {
                        println!("elliptic: analysis error: {e}");
                        AnalysisResult::Incomplete
                    },
                }
            },
            Err(e) => {
                println!("elliptic: could not get address: {e}");
                AnalysisResult::Incomplete
            },
        }
    }
}

