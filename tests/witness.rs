use chainflip::{
    local_store::{ILocalStore, MemoryLocalStore},
    utils::test_utils,
    vault::witness::fake_witness::{Block, CoinTx, FakeWitness},
};
use chainflip_common::types::{coin::Coin, unique_id::GetUniqueId, Timestamp};
use std::sync::{Arc, Mutex};
use test_utils::data::TestData;

#[test]
fn test_witness_event_is_made() {
    // - add a quote to local store
    // - add a corresponding coin tx onto the main chain
    // - test that there is witness shortly after

    // Tests use a simpler logger
    test_utils::logging::init();

    let timeout = std::time::Duration::from_millis(1000);

    let local_store = MemoryLocalStore::new();
    let local_store = Arc::new(Mutex::new(local_store));

    let (loki_block_sender, loki_block_receiver) = crossbeam_channel::unbounded();

    let witness = FakeWitness::new(loki_block_receiver, local_store.clone());
    witness.start();

    let quote_tx = TestData::swap_quote(Coin::ETH, Coin::LOKI);

    local_store
        .lock()
        .unwrap()
        .add_events(vec![quote_tx.clone().into()])
        .expect("Could not add event");

    // TODO: wait until witness acknowledged the quote (there must be
    //  a better way to do it than simply waiting)

    std::thread::sleep(std::time::Duration::from_millis(100));

    let coin_tx = CoinTx {
        timestamp: Timestamp::now(),
        deposit_address: quote_tx.input_address.clone().to_string(),
        return_address: quote_tx.return_address.clone().map(|t| t.to_string()),
    };

    let block = Block { txs: vec![coin_tx] };

    loki_block_sender.send(block).unwrap();

    let now = std::time::Instant::now();

    let res = loop {
        std::thread::sleep(std::time::Duration::from_millis(10));

        let witnesses = local_store.lock().unwrap().get_witness_evts();

        if witnesses
            .iter()
            .find(|w| w.quote == quote_tx.unique_id())
            .is_some()
        {
            break true;
        } else if now.elapsed() > timeout {
            break false;
        }
    };

    assert!(res);
}
