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
    // TODO: Remove the checkout kyle/stakeManagerWitnessTests once it's merged to the eth repo
    // run_cmd!(
    //     pwd;
    //     cd ./tests/eth-contracts;
    //     git checkout kyle/stakeManagerWitnessTests;
    //     git pull;
    //     poetry run brownie test tests/external/test_executeClaim.py;
    // )
    // .unwrap();

    println!("Running brownie test completed. Events can now be read");

    let first_item = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next());
    let first_item = first_item
        .await
        .expect("Check the CFE is running")
        .unwrap()
        .unwrap();
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

    let second_item = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next());
    let second_item = second_item
        .await
        .expect("Check the CFE is running")
        .unwrap()
        .unwrap();
    match second_item {
        StakingEvent::ClaimRegistered(node_id, amount, address, _start_time, _end_time) => {
            println!("ClaimRegistered({}, {})", node_id, amount);
            assert_eq!(node_id, U256::from_dec_str("12345").unwrap());
            assert_eq!(
                amount,
                U256::from_dec_str("40000000000000000000000").unwrap()
            );
            assert_eq!(
                address,
                web3::types::H160::from_str("0xf588b889948a4a902590425770057e202b34b5bd").unwrap()
            );
            // These are not determinstic, would be good to use a test with deterministic amounts
            // assert_eq!(start_time, U256::from_dec_str("1621570067").unwrap());
            // assert_eq!(end_time, U256::from_dec_str("1621570072").unwrap());
        }
        _ => panic!("Staking event that isn't ClaimRegistered"),
    }
}
