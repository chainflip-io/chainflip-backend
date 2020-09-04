use blockswap::{
    common::{
        coins::{Coin, CoinAmount, GenericCoinAmount, PoolCoin},
        LokiAmount,
    },
    side_chain::{ISideChain, MemorySideChain},
    utils::test_utils::{create_fake_stake_quote, create_fake_witness, store::MemoryKVS},
    vault::{
        processor::SideChainProcessor,
        transactions::{MemoryTransactionsProvider, TransactionProvider},
    },
};

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[test]
fn witnessed_staked_changes_pool_liquidity() {
    let s_chain = MemorySideChain::new();
    let s_chain = Arc::new(Mutex::new(s_chain));

    let tx_provider = MemoryTransactionsProvider::new(s_chain.clone());

    let kvs = MemoryKVS::new();

    let processor = SideChainProcessor::new(tx_provider, kvs);

    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal(1.0);
    let coin_amount = GenericCoinAmount::from_decimal(coin_type, 2.0);

    let stake_tx = create_fake_stake_quote(loki_amount.clone(), coin_amount.clone());
    let wtx_loki = create_fake_witness(&stake_tx, loki_amount.clone().into(), Coin::LOKI);
    let wtx_eth = create_fake_witness(&stake_tx, coin_amount.clone(), coin_type);

    {
        // Add blocks with those transactions

        let mut s_chain = s_chain.lock().unwrap();

        s_chain
            .add_block(vec![stake_tx.into()])
            .expect("Could not add a Quote TX");

        s_chain
            .add_block(vec![wtx_loki.into(), wtx_eth.into()])
            .expect("Could not add a Quote TX");
    }

    // We start the processor this late to make sure if fetches all
    // in the first iteration its "event loop"
    processor.start();

    let mut tx_provider = MemoryTransactionsProvider::new(s_chain.clone());

    // spin until the transaction is added by the processor
    let now = std::time::Instant::now();
    loop {
        let block_idx = tx_provider.sync();

        if block_idx >= 3 {
            break;
        }

        if now.elapsed() > Duration::from_millis(100) {
            panic!("Timed out waiting for a pool change transaction");
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    let liquidity = tx_provider
        .get_liquidity(PoolCoin::from(coin_type).unwrap())
        .unwrap();

    // Check that a pool with the right amount was created
    assert_eq!(liquidity.loki_depth, loki_amount.to_atomic());
    assert_eq!(liquidity.depth, coin_amount.to_atomic());
}
