//! This tests integration with the StakeManager contract
//! In order for these tests to work, setup must be completed first

use anyhow::Result;
use chainflip_engine::{
    eth::stake_manager::stake_manager::StakingEvent,
    mq::{nats_client::NatsMQClient, pin_message_stream, IMQClient, Options, Subject},
};

use sp_core::crypto::UncheckedInto;
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
        poetry run brownie test tests/unit/stakeManager/test_executeClaim.py;
    )
    .unwrap();

    let first_item = stream.next().await.unwrap().unwrap();
    match first_item {
        StakingEvent::ClaimExecuted(node_id, amount) => {
            println!("ClaimExecuted({}, {})", node_id, amount);
            assert_eq!(node_id, U256::from_dec_str("12345").unwrap());
            assert_eq!(
                amount,
                U256::from_dec_str("40000000000000000000000").unwrap()
            );
        }
        _ => println!("Staking event that isn't claim_executed"),
    };
}
