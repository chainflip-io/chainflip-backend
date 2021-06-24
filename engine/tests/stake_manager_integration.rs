//! This tests integration with the StakeManager contract
//! In order for these tests to work, nats and ganache with the preloaded db
//! in `./eth-db` must be loaded in

use std::{str::FromStr, time::Duration};

use chainflip_engine::{
    eth::{self, stake_manager::stake_manager::StakeManagerEvent},
    mq::{
        nats_client::{NatsMQClient, NatsMQClientFactory},
        pin_message_stream, IMQClient, IMQClientFactory, Subject,
    },
    settings::{self, Settings},
};

use config::{Config, ConfigError, File};
use sp_runtime::AccountId32;
use tokio_stream::StreamExt;

use web3::types::U256;

pub async fn setup_mq(mq_settings: settings::MessageQueue) -> Box<NatsMQClient> {
    let factory = NatsMQClientFactory::new(&mq_settings);

    factory.create().await.unwrap()
}

// Creating the settings to be used for tests
pub fn test_settings() -> Result<Settings, ConfigError> {
    let mut s = Config::new();

    // Start off by merging in the "default" configuration file
    s.merge(File::with_name("config/Testing.toml"))?;

    // You can deserialize (and thus freeze) the entire configuration as
    s.try_into()
}

#[tokio::test]
pub async fn test_all_stake_manager_events() {
    let settings = test_settings().unwrap();
    let mq_c = setup_mq(settings.clone().message_queue).await;

    // subscribe before pushing events to the queue
    let stream = mq_c
        .subscribe::<StakeManagerEvent>(Subject::StakeManager)
        .await
        .unwrap();

    println!("Subscribing to eth events");
    // this future contains an infinite loop, so we must end it's life
    let sm_future = eth::stake_manager::start_stake_manager_witness(settings);
    println!("Subscribed");

    // We just want the future to end, it should already have done it's job in 1 second
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), sm_future).await;

    println!("What's the next event?");
    let mut stream = pin_message_stream(stream);
    let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Future timed out")
        .unwrap()
        .unwrap();
    match next {
        StakeManagerEvent::Staked {
            account_id,
            amount,
            tx_hash,
        } => {
            assert_eq!(
                account_id,
                AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU").unwrap()
            );
            assert_eq!(amount, 40000000000000000000000);
            // TODO: Check tx_hash
            // assert_eq!(tx_hash, "")
        }
        _ => panic!("Was expected Staked event"),
    };

    let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Future timed out")
        .unwrap()
        .unwrap();
    match next {
        StakeManagerEvent::ClaimRegistered {
            account_id,
            amount,
            staker,
            start_time,
            expiry_time,
            tx_hash,
        } => {
            assert_eq!(
                account_id,
                AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU").unwrap()
            );
            assert_eq!(
                amount,
                U256::from_dec_str("13333333333333334032384").unwrap()
            );
            assert_eq!(
                staker,
                web3::types::H160::from_str("0x4726b1555bf7ab73553be4eb3cfe15376d0db188").unwrap()
            );
            // these aren't determinstic, so exclude from the test
            // assert_eq!(start_time, U256::from_dec_str("1621727544").unwrap());
            // assert_eq!(end_time, U256::from_dec_str("1621900344").unwrap());
        }
        _ => panic!("Was expecting ClaimRegistered event"),
    }

    let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Future timed out")
        .unwrap()
        .unwrap();
    match next {
        StakeManagerEvent::ClaimExecuted {
            account_id,
            amount,
            tx_hash,
        } => {
            assert_eq!(
                account_id,
                AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU").unwrap()
            );
            assert_eq!(amount, 13333333333333334032384);
        }
        _ => panic!("Was expecting ClaimExecuted event"),
    }

    let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Future timed out")
        .unwrap()
        .unwrap();
    match next {
        StakeManagerEvent::MinStakeChanged {
            old_min_stake,
            new_min_stake,
            tx_hash,
        } => {
            assert_eq!(
                old_min_stake,
                U256::from_dec_str("40000000000000000000000").unwrap()
            );
            assert_eq!(
                new_min_stake,
                U256::from_dec_str("13333333333333334032384").unwrap()
            );
        }
        _ => panic!("Was expecting MinStakeChanged event"),
    }
    let next = tokio::time::timeout(Duration::from_secs(1), stream.next())
        .await
        .expect("Future timed out")
        .unwrap()
        .unwrap();

    match next {
        StakeManagerEvent::EmissionChanged {
            old_emission_per_block,
            new_emission_per_block,
            tx_hash,
        } => {
            assert_eq!(
                old_emission_per_block,
                U256::from_dec_str("5607877281367557723").unwrap()
            );
            assert_eq!(
                new_emission_per_block,
                U256::from_dec_str("1869292427122519296").unwrap()
            );
        }
        _ => panic!("Was expecting MinStakeChanged event"),
    }
}
