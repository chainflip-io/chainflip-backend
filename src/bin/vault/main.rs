#[macro_use]
extern crate log;

use blockswap::{
    common::{
        coins::{GenericCoinAmount, PoolCoin},
        store,
    },
    logging,
    side_chain::{ISideChain, PeristentSideChain},
    vault::{
        api::APIServer,
        blockchain_connection::{LokiConnection, LokiConnectionConfig},
        processor::SideChainProcessor,
        transactions::{MemoryTransactionsProvider, TransactionProvider},
        witness::LokiWitness,
    },
};
use std::sync::{Arc, Mutex};

use std::str::FromStr;

use uuid::Uuid;

/// Currently only used for "testing"
fn add_fake_transactions<S>(s_chain: &Arc<Mutex<S>>)
where
    S: ISideChain,
{
    use blockswap::{
        common::{coins::Coin, LokiAmount, LokiPaymentId},
        transactions::{StakeQuoteTx, WitnessTx},
    };

    let quote = StakeQuoteTx {
        id: Uuid::new_v4(),
        input_loki_address_id: LokiPaymentId::from_str("60900e5603bf96e3").unwrap(),
        loki_amount: LokiAmount::from_decimal(500.0),
        coin_type: PoolCoin::ETH,
        coin_amount: GenericCoinAmount::from_decimal(Coin::ETH, 1.0),
    };

    let witness = WitnessTx {
        id: Uuid::new_v4(),
        quote_id: quote.id,
        transaction_id: "".to_owned(),
        transaction_block_number: 0,
        transaction_index: 0,
        amount: quote.loki_amount.to_atomic(),
        coin_type: Coin::LOKI,
        sender: None,
    };

    let mut s_chain = s_chain.lock().unwrap();

    s_chain
        .add_block(vec![quote.into()])
        .expect("Could not add a Quote TX");

    s_chain
        .add_block(vec![witness.into()])
        .expect("Could not add a Quote TX");
}

/// Entry point for the Quoter binary. We should try to keep it as small as posible
/// and implement most of the core logic as part of the library (src/lib.rs). This way
/// of organising code works better with integration tests.
/// Ideally we would just parse commad line arguments here and call into the library.
fn main() {
    std::panic::set_hook(Box::new(|msg| {
        error!("Panicked with: {}", msg);
        std::process::exit(101); // Rust's panics use 101 by default
    }));

    logging::init("vault", None);

    info!("Starting a Blockswap Vault node");

    let s_chain = PeristentSideChain::open("blocks.db");
    let s_chain = Arc::new(Mutex::new(s_chain));

    let mut provider = MemoryTransactionsProvider::new(s_chain.clone());
    provider.sync();

    let provider = Arc::new(Mutex::new(provider));

    let config = LokiConnectionConfig {
        rpc_wallet_port: 6934,
    };

    let loki_connection = LokiConnection::new(config);
    let loki_block_receiver = loki_connection.start();

    let _witness = LokiWitness::new(loki_block_receiver, s_chain.clone());

    let tx_provider = MemoryTransactionsProvider::new(s_chain.clone());

    // Opening another connection to the same database
    let db_connection = rusqlite::Connection::open("blocks.db").expect("Could not open database");
    let kvs = store::PersistentKVS::new(db_connection);

    let processor = SideChainProcessor::new(tx_provider, kvs);

    processor.start();

    // This code is temporary, for now just used to test the implementation
    add_fake_transactions(&s_chain);

    // can be used to shutdown the server
    let (_tx, rx) = tokio::sync::oneshot::channel();

    APIServer::serve(s_chain, provider, rx);
}
