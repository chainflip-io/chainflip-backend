use blockswap::{
    common::{
        coins::{Coin, CoinAmount, GenericCoinAmount, PoolCoin},
        LokiAmount,
    },
    side_chain::{ISideChain, MemorySideChain},
    utils::test_utils::{create_fake_stake_quote, create_fake_witness, store::MemoryKVS},
    vault::{
        processor::{ProcessorEvent, SideChainProcessor},
        transactions::{MemoryTransactionsProvider, TransactionProvider},
    },
};

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use log::{error, info};

fn spin_until_block(receiver: &crossbeam_channel::Receiver<ProcessorEvent>, target_idx: u32) {
    // Long timeout just to make sure a failing test
    let timeout = Duration::from_secs(10);

    loop {
        match receiver.recv_timeout(timeout) {
            Ok(event) => {
                info!("--- received event: {:?}", event);
                let ProcessorEvent::BLOCK(idx) = event;
                if idx >= target_idx {
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                error!("Channel timeout on receive");
                break;
            }
            Err(_) => {
                panic!("Unexpected channel error");
            }
        }
    }
}

fn check_liquidity<T>(
    tx_provider: &mut T,
    coin_type: Coin,
    loki_amount: &LokiAmount,
    coin_amount: &GenericCoinAmount,
) where
    T: TransactionProvider,
{
    tx_provider.sync();

    let liquidity = tx_provider
        .get_liquidity(PoolCoin::from(coin_type).unwrap())
        .unwrap();

    // Check that a pool with the right amount was created
    assert_eq!(liquidity.loki_depth, loki_amount.to_atomic());
    assert_eq!(liquidity.depth, coin_amount.to_atomic());
}

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
            .add_block(vec![stake_tx.clone().into()])
            .expect("Could not add a Quote TX");

        s_chain
            .add_block(vec![wtx_loki.into(), wtx_eth.into()])
            .expect("Could not add a Quote TX");
    }

    // We start the processor this late to make sure if fetches all
    // in the first iteration its "event loop"

    // Create a channel to receive processor events through
    let (sender, receiver) = crossbeam_channel::unbounded::<ProcessorEvent>();

    processor.start(Some(sender));

    let mut tx_provider = MemoryTransactionsProvider::new(s_chain.clone());

    // spin until the transaction is added by the processor
    spin_until_block(&receiver, 2);

    check_liquidity(&mut tx_provider, coin_type, &loki_amount, &coin_amount);

    {
        // Adding the same quote again
        let mut s_chain = s_chain.lock().unwrap();

        s_chain
            .add_block(vec![stake_tx.clone().into()])
            .expect("Could not add a Quote TX");
    }

    spin_until_block(&receiver, 4);

    // Check that the balance has not changed
    check_liquidity(&mut tx_provider, coin_type, &loki_amount, &coin_amount);
}
