#[macro_use]
extern crate log;

use blockswap::{
    logging,
    side_chain::{ISideChain, PeristentSideChain},
    vault::{
        api::APIServer,
        blockchain_connection::{LokiConnection, LokiConnectionConfig},
        witness::LokiWitness,
    },
};
use std::sync::{Arc, Mutex};

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

    let config = LokiConnectionConfig {
        rpc_wallet_port: 6934,
    };

    let loki_connection = LokiConnection::new(config);
    let loki_block_receiver = loki_connection.start();

    let _witness = LokiWitness::new(loki_block_receiver, s_chain.clone());

    // This code is temporary, for now just used to test the implementation
    let tx = blockswap::utils::test_utils::create_fake_quote_tx();

    s_chain
        .lock()
        .unwrap()
        .add_block(vec![tx.into()])
        .expect("Could not add a Quote TX");

    // can be used to shutdown the server
    let (_tx, rx) = tokio::sync::oneshot::channel();

    APIServer::serve(s_chain, rx);
}
