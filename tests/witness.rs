use std::sync::{Arc, Mutex};

use chainflip::{
    common::Timestamp,
    side_chain::{ISideChain, MemorySideChain},
    utils::test_utils,
    vault::witness::fake_witness::{Block, CoinTx, FakeWitness},
};
use uuid::Uuid;

#[test]
fn test_witness_tx_is_made() {
    // - add a quote tx onto the side chain
    // - add a corresponding coin tx onto the main chain
    // - test that there is witness transaction shortly after

    // Tests use a simpler logger
    test_utils::logging::init();

    let timeout = std::time::Duration::from_millis(1000);

    let s_chain = MemorySideChain::new();
    let s_chain = Arc::new(Mutex::new(s_chain));

    let (loki_block_sender, loki_block_receiver) = crossbeam_channel::unbounded();

    let witness = FakeWitness::new(loki_block_receiver, s_chain.clone());
    witness.start();

    let quote_tx = test_utils::create_fake_quote_tx_eth_loki();

    s_chain
        .lock()
        .unwrap()
        .add_block(vec![quote_tx.clone().into()])
        .expect("Could not add TX");

    // TODO: wait until witness acknowledged the quote (there must be
    //  a better way to do it than simply waiting)

    std::thread::sleep(std::time::Duration::from_millis(100));

    let coin_tx = CoinTx {
        id: Uuid::new_v4(),
        timestamp: Timestamp::now(),
        deposit_address: quote_tx.input_address.clone(),
        return_address: quote_tx.return_address.clone(),
    };

    let block = Block { txs: vec![coin_tx] };

    loki_block_sender.send(block).unwrap();

    let now = std::time::Instant::now();

    let res = loop {
        std::thread::sleep(std::time::Duration::from_millis(10));

        let witness_txs = s_chain.lock().unwrap().get_witness_txs();

        if witness_txs
            .iter()
            .find(|tx| tx.quote_id == quote_tx.id)
            .is_some()
        {
            break true;
        } else if now.elapsed() > timeout {
            break false;
        }
    };

    assert!(res);
}
