//! This tests integration with the StakeManager contract
//! In order for these tests to work, nats and ganache with the preloaded db
//! in `./eth-db` must be loaded in

use std::str::FromStr;

use chainflip_engine::{
    eth::{self, stake_manager::stake_manager::StakingEvent},
    mq::{nats_client::NatsMQClient, pin_message_stream, IMQClient, Subject},
    settings::{self, Settings},
};

use config::{Config, ConfigError, File};
use tokio_stream::StreamExt;

use web3::types::U256;

pub async fn setup_mq(mq_settings: settings::MessageQueue) -> Box<NatsMQClient> {
    NatsMQClient::connect(mq_settings).await.unwrap()
}

// Creating the settings to be used for tests
pub fn test_settings() -> Result<Settings, ConfigError> {
    let mut s = Config::new();

    // Start off by merging in the "default" configuration file
    s.merge(File::with_name("config/testing.toml"))?;

    // You can deserialize (and thus freeze) the entire configuration as
    s.try_into()
}

#[tokio::test]
pub async fn test_all_stake_manager_events() {
    let settings = test_settings().unwrap();
    let mq_c = setup_mq(settings.clone().message_queue).await;

    // subscribe before pushing events to the queue
    let stream = mq_c
        .subscribe::<StakingEvent>(Subject::StakeManager)
        .await
        .unwrap();

    println!("Subscribing to eth events");
    // this future contains an infinite loop, so we must end it's life
    let sm_future = eth::stake_manager::start_stake_manager_witness(settings);

    // We just want the future to end, it should already have done it's job in 1 second
    let _ = tokio::time::timeout(std::time::Duration::from_secs(4), sm_future).await;

    println!("What's the next event?");
    let mut stream = pin_message_stream(stream);
    match stream.next().await.unwrap().unwrap() {
        StakingEvent::Staked(node_id, amount) => {
            assert_eq!(node_id, U256::from_dec_str("12345").unwrap());
            assert_eq!(
                amount,
                U256::from_dec_str("40000000000000000000000").unwrap()
            );
        }
        _ => panic!("Was expected Staked event"),
    };

    match stream.next().await.unwrap().unwrap() {
        StakingEvent::ClaimRegistered(node_id, amount, address, start_time, end_time) => {
            assert_eq!(node_id, U256::from_dec_str("12345").unwrap());
            assert_eq!(
                amount,
                U256::from_dec_str("13333333333333334032384").unwrap()
            );
            assert_eq!(
                address,
                web3::types::H160::from_str("0x4726b1555bf7ab73553be4eb3cfe15376d0db188").unwrap()
            );
            assert_eq!(start_time, U256::from_dec_str("1621727544").unwrap());
            assert_eq!(end_time, U256::from_dec_str("1621900344").unwrap());
        }
        _ => panic!("Was expecting ClaimRegistered event"),
    }

    match stream.next().await.unwrap().unwrap() {
        StakingEvent::ClaimExecuted(node_id, amount) => {
            assert_eq!(node_id, U256::from_dec_str("12345").unwrap());
            assert_eq!(
                amount,
                U256::from_dec_str("13333333333333334032384").unwrap()
            );
        }
        _ => panic!("Was expecting ClaimExecuted event"),
    }

    match stream.next().await.unwrap().unwrap() {
        StakingEvent::MinStakeChanged(before, after) => {
            assert_eq!(
                before,
                U256::from_dec_str("40000000000000000000000").unwrap()
            );
            assert_eq!(
                after,
                U256::from_dec_str("13333333333333334032384").unwrap()
            );
        }
        _ => panic!("Was expecting MinStakeChanged event"),
    }

    match stream.next().await.unwrap().unwrap() {
        StakingEvent::EmissionChanged(before, after) => {
            assert_eq!(before, U256::from_dec_str("5607877281367557723").unwrap());
            assert_eq!(after, U256::from_dec_str("1869292427122519296").unwrap());
        }
        _ => panic!("Was expecting MinStakeChanged event"),
    }
}
