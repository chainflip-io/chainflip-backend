//! This tests integration with the StakeManager contract
//! In order for these tests to work, setup must be completed first

use anyhow::Result;
use chainflip_engine::mq::{nats_client::NatsMQClient, IMQClient, Options};

use cmd_lib::*;

use chainflip_common::*;

use chainflip_engine::*;

#[tokio::test]
pub async fn test_execute_claim_integration() {
    println!("Integration test StakeManager contract");
    let mq_options = Options {
        url: "localhost:4422".to_string(),
    };
    let mq_c = *NatsMQClient::connect(mq_options).await.unwrap();

    // let stream = mq_c.subscribe(Subject::StakeManager).await.unwrap();

    // println!("Subscribed to stream successfully");
}

// pub fn my_test {
//     let coin = chainflip_common::
// }

// Subscribe to execute claim events
// async fn setup_client() -> Box<NatsMQClient> {

//     let options = Options {
//         url: "http://localhost:4222".to_string(),
//     };

//     NatsMQClient::connect(options).await.unwrap()
// }

// #[test]
// fn setup() -> anyhow::Result<()> {
//     println!("Pull the latest chainflip-eth-contracts repo");

//     cmd_lib::set_debug(true);

//     run_cmd!(
//         cd ./tests/eth-contracts;
//         poetry run brownie test "tests/unit/stakeManager/test_executeClaim.py";
//     )?;

//     Ok(())
// }

// Only test particular tests from the brownie test suite
