#[macro_use]
extern crate log;

use blockswap::side_chain::{ISideChain, PeristentSideChain, SideChainTx};
use blockswap::vault::blockchain_connection::LokiConnection;
use blockswap::vault::witness::Witness;
use std::sync::{Arc, Mutex};

use blockswap::vault::api::APIServer;

use blockswap::logging;

/// Entry point for the Quoter binary. We should try to keep it as small as posible
/// and implement most of the core logic as part of the library (src/lib.rs). This way
/// of organising code works better with integration tests.
/// Ideally we would just parse commad line arguments here and call into the library.
fn main() {
    logging::init("vault");

    info!("Starting a Blockswap Vault node");

    let s_chain = PeristentSideChain::open("blocks.db");
    let s_chain = Arc::new(Mutex::new(s_chain));

    let loki_connection = LokiConnection::new();
    let loki_block_receiver = loki_connection.start();

    let _witness = Witness::new(loki_block_receiver, s_chain.clone());

    // This code is temporary, for now just used to test the implementation
    let tx = blockswap::utils::test_utils::create_fake_quote_tx();
    s_chain
        .lock()
        .unwrap()
        .add_tx(SideChainTx::QuoteTx(tx))
        .expect("Could not add a Quote TX");

    APIServer::serve();
}
