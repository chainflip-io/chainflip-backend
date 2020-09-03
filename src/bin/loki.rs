#[macro_use]
extern crate log;

use blockswap::*;

use std::str::FromStr;

use vault::blockchain_connection::loki_rpc;
// use std::io::Error;

use crate::side_chain::ISideChain;

use crate::common::{coins::CoinAmount, LokiAmount, LokiPaymentId, LokiWalletAddress};

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

    let res = loki_rpc::transfer(PORT, &amount, &other_address, None)
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
    use side_chain::MemorySideChain;
    use std::sync::{Arc, Mutex};

    use vault::{
        blockchain_connection::{LokiConnection, LokiConnectionConfig},
        witness::LokiWitness,
    };

    let mut s_chain = MemorySideChain::new();

    let int_address = loki_rpc::make_integrated_address(PORT, None)
        .await
        .expect("loki rpc");

    info!("Integrated address: {:?}", int_address);

    let mut tx = crate::utils::test_utils::create_fake_quote_tx();
    tx.input_address_id = int_address.payment_id.clone();

    // Send some money to integrated address
    {
        let res = loki_rpc::get_balance(PORT).await.expect("Req is Err");
        info!("Balance before: {}", res);

        let amount = LokiAmount::from_atomic(50_000_000);
        let address = LokiWalletAddress::from_str(&int_address.integrated_address)
            .expect("Incorrect wallet address");
        let res = loki_rpc::transfer(PORT, &amount, &address, None).await;

        info!("Transfer response: {:?}", &res);

        let res = loki_rpc::get_balance(PORT).await.expect("Req is Err");
        info!("Balance after: {}", res);
    }

    let tx = crate::side_chain::SideChainTx::from(tx);

    s_chain
        .add_block(vec![tx.clone()])
        .expect("Error adding a transaction to the database");

    let s_chain = Arc::new(Mutex::new(s_chain));

    let config = LokiConnectionConfig {
        rpc_wallet_port: PORT,
    };

    let loki_connection = LokiConnection::new(config);
    let loki_block_receiver = loki_connection.start();

    let witness = LokiWitness::new(loki_block_receiver, s_chain.clone());
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
