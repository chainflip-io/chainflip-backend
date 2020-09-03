use blockswap::{
    common::{
        coins::{Coin, PoolCoin},
        LokiAmount, LokiPaymentId,
    },
    side_chain::{ISideChain, MemorySideChain},
    transactions::{StakeQuoteTx, WitnessTx},
    utils::test_utils::store::MemoryKVS,
    vault::{
        processor::SideChainProcessor,
        transactions::{MemoryTransactionsProvider, TransactionProvider},
    },
};

use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use uuid::Uuid;

fn create_stake_loki_tx(loki_amount: LokiAmount) -> StakeQuoteTx {
    StakeQuoteTx {
        id: Uuid::new_v4(),
        input_loki_address_id: LokiPaymentId::from_str("60900e5603bf96e3").unwrap(),
        loki_amount,
    }
}

fn create_witness_tx(quote: &StakeQuoteTx) -> WitnessTx {
    WitnessTx {
        id: Uuid::new_v4(),
        quote_id: quote.id,
        transaction_id: "".to_owned(),
        transaction_block_number: 0,
        transaction_index: 0,
        amount: quote.loki_amount.to_atomic(),
        coin_type: Coin::LOKI,
        sender: None,
    }
}

#[test]
fn witnessed_staked_changes_pool_liquidity() {

    let s_chain = MemorySideChain::new();
    let s_chain = Arc::new(Mutex::new(s_chain));

    let tx_provider = MemoryTransactionsProvider::new(s_chain.clone());

    let kvs = MemoryKVS::new();

    let processor = SideChainProcessor::new(tx_provider, kvs);

    // add_fake_transactions(&s_chain);

    let loki_amount = LokiAmount::from_decimal(1.0);

    let stake_tx = create_stake_loki_tx(loki_amount.clone());
    let witness_tx = create_witness_tx(&stake_tx);

    {
        // Add blocks with those transactions

        let mut s_chain = s_chain.lock().unwrap();

        s_chain
            .add_block(vec![stake_tx.into()])
            .expect("Could not add a Quote TX");

        s_chain
            .add_block(vec![witness_tx.into()])
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

    let loki_liquidity = tx_provider
        .get_liquidity(PoolCoin::from(Coin::ETH).unwrap())
        .unwrap();

    // Check that a pool with the right amount was created
    assert_eq!(loki_liquidity.loki_depth, loki_amount.to_atomic());
}
