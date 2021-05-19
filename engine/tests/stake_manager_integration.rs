//! This tests integration with the StakeManager contract
//! In order for these tests to work, setup must be completed first

use std::str::FromStr;

use chainflip_engine::{
    eth::stake_manager::stake_manager::StakingEvent,
    mq::{nats_client::NatsMQClient, pin_message_stream, IMQClient, Options, Subject},
};

use tokio_stream::StreamExt;

use cmd_lib::*;
use web3::types::U256;

pub async fn setup_mq() -> Box<NatsMQClient> {
    let mq_options = Options {
        url: "localhost:4222".to_string(),
    };
    NatsMQClient::connect(mq_options).await.unwrap()
}

#[tokio::test]
pub async fn test_execute_claim_integration() {
    let mq_c = setup_mq().await;

    let stream = mq_c
        .subscribe::<StakingEvent>(Subject::StakeManager)
        .await
        .unwrap();

    let mut stream = pin_message_stream(stream);

    // this will block, and only when the test is finished, will we read from the mq
    run_cmd!(
        pwd;
        cd ./tests/eth-contracts;
        poetry run brownie test tests/unit/stakeManagerWitness/test_executeClaim.py;
    )
    .unwrap();

    println!("Running command complete");
    let first_item = stream.next().await.unwrap().unwrap();
    println!("Event is: {:#?}", first_item);
    match first_item {
        StakingEvent::Staked(node_id, amount) => {
            println!("Staked({}, {})", node_id, amount);
            assert_eq!(node_id, U256::from_dec_str("12345").unwrap());
            assert_eq!(
                amount,
                U256::from_dec_str("40000000000000000000000").unwrap()
            );
        }
        _ => panic!("Staking event that isn't Staked"),
    };

    let second_item = stream.next().await.unwrap().unwrap();
    println!("Second event is: {:#?}", second_item);
    match second_item {
        StakingEvent::ClaimRegistered(node_id, amount, address, start_time, end_time) => {
            println!("ClaimRegistered({}, {})", node_id, amount);
            assert_eq!(node_id, U256::from_dec_str("1").unwrap());
            assert_eq!(amount, U256::from_dec_str("1").unwrap());
            assert_eq!(
                address,
                web3::types::H160::from_str("0x9dbe382b57bcdc2aabc874130e120a3e7de09bda").unwrap()
            );
            // These are not determinstic, would be good to use a test with deterministic amounts
            // assert_eq!(start_time, U256::from_dec_str("1621570067").unwrap());
            // assert_eq!(end_time, U256::from_dec_str("1621570072").unwrap());
        }
        _ => panic!("Staking event that isn't ClaimExecuted"),
    }

    // TODO: Add timeouts on some of these futures, should be quick quick
}
