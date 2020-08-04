use std::sync::{Arc, Mutex};
use blockswap::vault::{SideChain};
use blockswap::vault::witness::Witness;
use blockswap::vault::blockchain_connection::LokiConnection;

/// Entry point for the Quoter binary. We should try to keep it as small as posible
/// and implement most of the core logic as part of the library (src/lib.rs). This way
/// of organising code works better with integration tests.
/// Ideally we would just parse commad line arguments here and call into the library.
fn main() {

    let s_chain = SideChain::new();
    let s_chain = Arc::new(Mutex::new(s_chain));

    let loki_connection = LokiConnection::new();
    let loki_block_receiver = loki_connection.start();

    let witness = Witness::new(loki_block_receiver, s_chain.clone());

    // TODO: processor should run in this thread
    loop {
        // let other thread do the work
        std::thread::sleep(std::time::Duration::from_secs(1));
    }


}