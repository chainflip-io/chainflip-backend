//! This tests integration with the StakeManager contract
//! In order for these tests to work, nats and ganache with the preloaded db
//! in `./eth-db` must be loaded in
use std::{str::FromStr, time::Duration};

use chainflip_engine::{
    eth::{self, stake_manager::stake_manager::StakeManagerEvent},
    mq::{nats_client::NatsMQClient, IMQClient, Subject},
    settings::Settings,
};

use sp_runtime::AccountId32;
use tokio_stream::StreamExt;

use web3::types::U256;

use slog::{o, Drain};

#[tokio::test]
pub async fn test_all_stake_manager_events() {

    let drain = slog_json::Json::new(std::io::stdout())
        .add_default_keys()
        .build()
        .fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    let root_logger = slog::Logger::root(drain, o!());

    let settings = Settings::from_file("config/Testing.toml").unwrap();
    let mq_c = NatsMQClient::new(&settings.message_queue).await.unwrap();

    // subscribe before pushing events to the queue
    let mut sm_event_stream = mq_c
        .subscribe::<StakeManagerEvent>(Subject::StakeManager)
        .await
        .unwrap();

    // The Stake Manager Witness will run forever unless we stop it after a short time
    // in which it should have already done it's job.
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        eth::stake_manager::start_stake_manager_witness(&settings, mq_c, &root_logger),
    )
    .await;
    slog::info!(&root_logger, "Subscribed");

    // Grab the events from the stream and put them into a vec
    let mut sm_events: Vec<StakeManagerEvent> = Vec::new();
    loop {
        // All events should already be built up in the event stream, so no need to wait.
        match tokio::time::timeout(Duration::from_millis(1), sm_event_stream.next()).await {
            Ok(Some(Ok(e))) =>{
                sm_events.push(e);
            }
            Ok(_) => {
                panic!("Error in event stream")
            }
            Err(_) => {
                // Timeout, all events in the stream have been pulled.
                break;
            }
        }
    }

    if sm_events.len()==0{
        panic!("Event stream was empty. Have you ran the setup script to deploy/run the contracts?")
    }

    // The following event details correspond to the events in chainflip-eth-contracts/scripts/deploy_and.py
    sm_events.iter().find(|event| 
        match event {
            StakeManagerEvent::Staked {
                account_id,
                amount,
                return_addr,
                ..
            } => {
                assert_eq!(
                    account_id,
                    &AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(amount, &40000000000000000000000);
                assert_eq!(
                    return_addr,
                    &web3::types::H160::from_str("0x0000000000000000000000000000000000000001")
                        .unwrap()
                );
                true
            }
            _ => {false}
        }
    ).expect("Didn't find the Staked event");

    sm_events.iter().find(|event| 
        match event {
            StakeManagerEvent::ClaimRegistered {
                account_id,
                amount,
                staker,
                ..
            } => {
                assert_eq!(
                    account_id,
                    &AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(
                    amount,
                    &U256::from_dec_str("13333333333333334032384").unwrap()
                );
                assert_eq!(
                    staker,
                    &web3::types::H160::from_str("0x33a4622b82d4c04a53e170c638b944ce27cffce3")
                        .unwrap()
                );
                true
            }
            _ => {false}
        }
    ).expect("Didn't find the ClaimRegistered event");

    sm_events.iter().find(|event| 
        match event {
            StakeManagerEvent::ClaimExecuted {
                account_id, amount, ..
            } => {
                assert_eq!(
                    account_id,
                    &AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU")
                        .unwrap()
                );
                assert_eq!(amount, &13333333333333334032384);
                true
            }
            _ => {false}
        }
    ).expect("Didn't find the ClaimExecuted event");

    sm_events.iter().find(|event| 
        match event {
            StakeManagerEvent::MinStakeChanged {
                old_min_stake,
                new_min_stake,
                ..
            } => {
                assert_eq!(
                    old_min_stake,
                    &U256::from_dec_str("40000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_min_stake,
                    &U256::from_dec_str("13333333333333334032384").unwrap()
                );
                true
            }
            _ => {false}
        }
    ).expect("Didn't find the MinStakeChanged event");

    sm_events.iter().find(|event| 
        match event {
            StakeManagerEvent::FlipSupplyUpdated {
                old_supply,
                new_supply,
                ..
            } => {
                assert_eq!(
                    old_supply,
                    &U256::from_dec_str("90000000000000000000000000").unwrap()
                );
                assert_eq!(
                    new_supply,
                    &U256::from_dec_str("100000000000000000000000000").unwrap()
                );
                true
            }
            _ => {false}
        }
    ).expect("Didn't find the FlipSupplyUpdated event");
}
