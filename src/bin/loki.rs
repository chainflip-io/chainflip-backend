use chainflip::{
    common::*,
    local_store::{ILocalStore, MemoryLocalStore},
    logging, utils,
    vault::{
        blockchain_connection::{loki_rpc, LokiConnection, LokiConnectionConfig},
        transactions::{MemoryTransactionsProvider, TransactionProvider},
        witness::LokiWitness,
    },
};
use chainflip_common::types::coin::Coin;
use parking_lot::RwLock;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use utils::test_utils::data::TestData;

#[macro_use]
extern crate log;

const PORT: u16 = 6934;

async fn make_int_address() {
    let my_int_address = loki_rpc::make_integrated_address(PORT, None).await.unwrap();

    dbg!(my_int_address);
}

#[allow(unused)]
async fn test_loki_rpc() {
    let res = loki_rpc::get_balance(PORT).await.expect("Req is Err");
    info!("Balance: {}", res);

    // integrated_address: "TGArxr3H99KcMxGDgLR9ejGmbY5iphPiG9YwDZyNiCM81dgM776a1h7FwFCZZxm7yPabRxQeyfLesBynTWP6DfJq5669EoibhUa2J8zgrtF2"
    // payment_id: "60900e5603bf96e3"

    let own_address = LokiWalletAddress::from_str("T6UBx3DnXsocMxGDgLR9ejGmbY5iphPiG9YwDZyNiCM81dgM776a1h7FwFCZZxm7yPabRxQeyfLesBynTWP6DfJq1DAtb6QYn").unwrap();

    let other_address = LokiWalletAddress::from_str("T6T6otxMejTKavFEQP66VufY9y8vr2Z6RMzoQ95BZx7KWy6zCngrfh39dUVtrF3crtLRFdXpmgjjH7658C74NoJ91imYo7zMk").unwrap();

    let int_address = LokiWalletAddress::from_str("TGArxr3H99KcMxGDgLR9ejGmbY5iphPiG9YwDZyNiCM81dgM776a1h7FwFCZZxm7yPabRxQeyfLesBynTWP6DfJq5669EoibhUa2J8zgrtF2").unwrap();

    let amount = LokiAmount::from_atomic(100_000_000_000);

    // make_int_address().await;

    let res = loki_rpc::transfer(PORT, &amount, &other_address)
        .await
        .expect("Could not transfer");

    info!("Transfer fee: {}", res.fee);

    // println!("Testing loki integration");

    // let payment_id = LokiPaymentId::from_str("60900e5603bf96e3").unwrap();

    // let res = loki_rpc::get_all_transfers().await;
    // let res = loki_rpc::get_bulk_payments(vec![payment_id], 3000).await;
    dbg!(&res);
}

async fn test_loki_witness() {
    let mut local_store = MemoryLocalStore::new();

    let int_address = loki_rpc::make_integrated_address(PORT, None)
        .await
        .expect("loki rpc");

    info!("Integrated address: {:?}", int_address);

    let mut tx = TestData::swap_quote(Coin::ETH, Coin::LOKI);
    tx.input_address_id = LokiPaymentId::from_str(&int_address.payment_id)
        .unwrap()
        .to_bytes()
        .to_vec();

    // Send some money to integrated address
    {
        let res = loki_rpc::get_balance(PORT).await.expect("Req is Err");
        info!("Balance before: {}", res);

        let amount = LokiAmount::from_atomic(50_000_000);
        let address = LokiWalletAddress::from_str(&int_address.integrated_address)
            .expect("Incorrect wallet address");
        let res = loki_rpc::transfer(PORT, &amount, &address).await;

        info!("Transfer response: {:?}", &res);

        let res = loki_rpc::get_balance(PORT).await.expect("Req is Err");
        info!("Balance after: {}", res);
    }

    local_store
        .add_events(vec![tx.into()])
        .expect("Error adding a transaction to the database");

    let local_store = Arc::new(Mutex::new(local_store));

    let mut provider = MemoryTransactionsProvider::new(local_store.clone());
    provider.sync();

    let provider = Arc::new(RwLock::new(provider));

    let config = LokiConnectionConfig {
        rpc_wallet_port: PORT,
    };

    let loki_connection = LokiConnection::new(config);
    let loki_block_receiver = loki_connection.start();

    let witness = LokiWitness::new(loki_block_receiver, provider.clone());
    witness.start();

    // Block current thread
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[tokio::main]
async fn main() {
    logging::init("loki-integration", Some(log::LevelFilter::Info));

    test_loki_witness().await;
}
