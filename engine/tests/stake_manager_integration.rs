//! This tests integration with the StakeManager contract
//! In order for these tests to work, setup must be completed first

use anyhow::Result;
use async_nats::Options;

use cmd_lib::*;

use chainflip_common::*;

use chainflip_engine::*;

#[test]
pub fn my_test() {
    println!("Hello");
    let mq_c = chainflip_engine::mq::mq::NatsMQClient::new();
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
