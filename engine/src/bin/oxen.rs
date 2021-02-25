use chainflip::{
    common::*,
    local_store::{ILocalStore, MemoryLocalStore},
    logging, utils,
    vault::{
        blockchain_connection::{oxen_rpc, OxenConnection, OxenConnectionConfig},
        transactions::{MemoryTransactionsProvider, TransactionProvider},
        witness::OxenWitness,
    },
};
use chainflip_common::types::{addresses::OxenAddress, coin::Coin};
use parking_lot::RwLock;
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};
use utils::test_utils::data::TestData;

#[macro_use]
extern crate log;

const PORT: u16 = 6934;

#[allow(unused)]
async fn test_oxen_rpc() {
    let res = oxen_rpc::get_balance(PORT).await.expect("Req is Err");
    info!("Balance: {}", res);

    // integrated_address: "TGArxr3H99KcMxGDgLR9ejGmbY5iphPiG9YwDZyNiCM81dgM776a1h7FwFCZZxm7yPabRxQeyfLesBynTWP6DfJq5669EoibhUa2J8zgrtF2"
    // payment_id: "60900e5603bf96e3"

    let own_address = OxenAddress::from_str("T6UBx3DnXsocMxGDgLR9ejGmbY5iphPiG9YwDZyNiCM81dgM776a1h7FwFCZZxm7yPabRxQeyfLesBynTWP6DfJq1DAtb6QYn").unwrap();

    let other_address = OxenAddress::from_str("T6T6otxMejTKavFEQP66VufY9y8vr2Z6RMzoQ95BZx7KWy6zCngrfh39dUVtrF3crtLRFdXpmgjjH7658C74NoJ91imYo7zMk").unwrap();

    let int_address = OxenAddress::from_str("TGArxr3H99KcMxGDgLR9ejGmbY5iphPiG9YwDZyNiCM81dgM776a1h7FwFCZZxm7yPabRxQeyfLesBynTWP6DfJq5669EoibhUa2J8zgrtF2").unwrap();

    let amount = OxenAmount::from_atomic(100_000_000_000);

    // make_int_address().await;

    let res = oxen_rpc::transfer(PORT, &amount, &other_address)
        .await
        .expect("Could not transfer");

    info!("Transfer fee: {}", res.fee);

    // println!("Testing oxen integration");

    // let payment_id = OxenPaymentId::from_str("60900e5603bf96e3").unwrap();

    // let res = oxen_rpc::get_all_transfers().await;
    // let res = oxen_rpc::get_bulk_payments(vec![payment_id], 3000).await;
    dbg!(&res);
}

async fn test_oxen_witness() {
    let mut local_store = MemoryLocalStore::new();

    let int_address = oxen_rpc::make_integrated_address(PORT, None)
        .await
        .expect("oxen rpc");

    info!("Integrated address: {:?}", int_address);

    let mut tx = TestData::swap_quote(Coin::ETH, Coin::OXEN);
    tx.input_address_id = hex::decode(&int_address.payment_id).unwrap();

    // Send some money to integrated address
    {
        let res = oxen_rpc::get_balance(PORT).await.expect("Req is Err");
        info!("Balance before: {}", res);

        let amount = OxenAmount::from_atomic(50_000_000);
        let address = OxenAddress::from_str(&int_address.integrated_address)
            .expect("Incorrect wallet address");
        let res = oxen_rpc::transfer(PORT, &amount, &address).await;

        info!("Transfer response: {:?}", &res);

        let res = oxen_rpc::get_balance(PORT).await.expect("Req is Err");
        info!("Balance after: {}", res);
    }

    local_store
        .add_events(vec![tx.into()])
        .expect("Error adding a transaction to the database");

    let local_store = Arc::new(Mutex::new(local_store));

    let mut provider = MemoryTransactionsProvider::new(local_store.clone());
    provider.sync();

    let provider = Arc::new(RwLock::new(provider));

    let config = OxenConnectionConfig {
        rpc_wallet_port: PORT,
    };

    let oxen_connection = OxenConnection::new(config);
    let oxen_block_receiver = oxen_connection.start();

    let witness = OxenWitness::new(oxen_block_receiver, provider.clone());
    witness.start();

    // Block current thread
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[tokio::main]
async fn main() {
    logging::init("oxen-integration", Some(log::LevelFilter::Info));

    test_oxen_witness().await;
}
