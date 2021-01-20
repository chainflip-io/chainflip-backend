use chainflip::{
    local_store::{ISideChain, MemorySideChain},
    utils::test_utils,
    vault::witness::fake_witness::{Block, CoinTx, FakeWitness},
};
use chainflip_common::types::{coin::Coin, Timestamp, UUIDv4};
use std::sync::{Arc, Mutex};
use test_utils::data::TestData;

#[test]
fn test_witness_tx_is_made() {
    // - add a quote onto the side chain
    // - add a corresponding coin tx onto the main chain
    // - test that there is witness shortly after

    // Tests use a simpler logger
    test_utils::logging::init();

    let timeout = std::time::Duration::from_millis(1000);

    let s_chain = MemorySideChain::new();
    let s_chain = Arc::new(Mutex::new(s_chain));

    let (loki_block_sender, loki_block_receiver) = crossbeam_channel::unbounded();

    let witness = FakeWitness::new(loki_block_receiver, s_chain.clone());
    witness.start();

    let quote_tx = TestData::swap_quote(Coin::ETH, Coin::LOKI);

    s_chain
        .lock()
        .unwrap()
        .add_block(vec![quote_tx.clone().into()])
        .expect("Could not add TX");

    // TODO: wait until witness acknowledged the quote (there must be
    //  a better way to do it than simply waiting)

    std::thread::sleep(std::time::Duration::from_millis(100));

    let coin_tx = CoinTx {
        id: UUIDv4::new(),
        timestamp: Timestamp::now(),
        deposit_address: quote_tx.input_address.clone().to_string(),
        return_address: quote_tx.return_address.clone().map(|t| t.to_string()),
    };

    let block = Block { txs: vec![coin_tx] };

    loki_block_sender.send(block).unwrap();

    let now = std::time::Instant::now();

    let res = loop {
        std::thread::sleep(std::time::Duration::from_millis(10));

        let witness_txs = s_chain.lock().unwrap().get_witness_txs();

        if witness_txs
            .iter()
            .find(|tx| tx.quote == quote_tx.id)
            .is_some()
        {
            break true;
        } else if now.elapsed() > timeout {
            break false;
        }
    };

    assert!(res);
}
