#[macro_use]
extern crate log;

use blockswap::{
    common::{
        coins::{Coin, GenericCoinAmount},
        store, LokiAmount,
    },
    logging,
    side_chain::{ISideChain, PeristentSideChain},
    utils::test_utils::{btc::TestBitcoinClient, create_fake_stake_quote, create_fake_witness},
    vault::{
        api::APIServer,
        blockchain_connection::{LokiConnection, LokiConnectionConfig, Web3Client},
        config::VAULT_CONFIG,
        processor::{LokiSender, OutputCoinProcessor, SideChainProcessor},
        transactions::{MemoryTransactionsProvider, TransactionProvider},
        witness::LokiWitness,
    },
};
use parking_lot::RwLock;

use std::sync::{Arc, Mutex};

/// Currently only used for "testing"
fn add_fake_transactions<S>(s_chain: &Arc<Mutex<S>>)
where
    S: ISideChain,
{
    let quote = create_fake_stake_quote(
        LokiAmount::from_decimal_string("500"),
        GenericCoinAmount::from_decimal_string(Coin::ETH, "1.0"),
    );

    let witness = create_fake_witness(&quote, quote.loki_amount, Coin::LOKI);

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

    // Create the vault config and ensure it's valid
    let vault_config = &VAULT_CONFIG;

    info!("Starting a _ Vault node");

    let s_chain = PeristentSideChain::open("blocks.db");
    let s_chain = Arc::new(Mutex::new(s_chain));

    let mut provider = MemoryTransactionsProvider::new(s_chain.clone());
    provider.sync();

    let provider = Arc::new(RwLock::new(provider));

    let config = LokiConnectionConfig {
        rpc_wallet_port: vault_config.loki.rpc.port,
    };

    let loki_connection = LokiConnection::new(config);
    let loki_block_receiver = loki_connection.start();

    let _witness = LokiWitness::new(loki_block_receiver, s_chain.clone());

    let tx_provider = MemoryTransactionsProvider::new(s_chain.clone());

    // Opening another connection to the same database
    let db_connection = rusqlite::Connection::open("blocks.db").expect("Could not open database");
    let kvs = store::PersistentKVS::new(db_connection);

    let eth_client =
        Web3Client::url(&vault_config.eth.provider_url).expect("Failed to create web3 client");

    // TODO: use production client instead
    let btc = TestBitcoinClient::new();

    let loki = LokiSender::new(vault_config.loki.rpc.clone());

    let coin_processor = OutputCoinProcessor::new(loki, eth_client, btc);

    let processor = SideChainProcessor::new(tx_provider, kvs, coin_processor);

    processor.start(None);

    // This code is temporary, for now just used to test the implementation
    add_fake_transactions(&s_chain);

    // can be used to shutdown the server
    let (_tx, rx) = tokio::sync::oneshot::channel();

    APIServer::serve(s_chain, provider, rx);
}
